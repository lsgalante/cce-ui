//! `VkCore`: the device-level half of the Vulkan backend — instance (+
//! validation layers), physical device + graphics queue, gpu-allocator, and
//! the shared command pool.
//!
//! Two ways in: [`VkCore::new_for_wayland_surface`] picks a present-capable
//! device for a window (what `VkRenderer` uses), and [`VkCore::new_headless`]
//! builds the same core with no surface at all — for offscreen consumers
//! (thumbnail rendering, previews, the future RT engine) that render into
//! images instead of a swapchain.

use std::ffi::{c_void, CStr, CString};

use ash::vk;
use gpu_allocator::vulkan::{Allocator, AllocatorCreateDesc};

const VALIDATION_LAYER: &CStr = c"VK_LAYER_KHRONOS_validation";

unsafe extern "system" fn debug_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _types: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _user_data: *mut c_void,
) -> vk::Bool32 {
    if data.is_null() {
        return vk::FALSE;
    }
    let message = unsafe {
        let p = (*data).p_message;
        if p.is_null() {
            return vk::FALSE;
        }
        CStr::from_ptr(p).to_string_lossy()
    };
    if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
        log::error!("[vulkan] {message}");
    } else if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::WARNING) {
        log::warn!("[vulkan] {message}");
    } else {
        log::debug!("[vulkan] {message}");
    }
    vk::FALSE
}

pub struct VkCore {
    // Field order is drop order: allocator and command pool go before the
    // device. The instance (and the loaded library) is process-shared and
    // never destroyed — see `shared_instance()`.
    pub(crate) allocator: Option<Allocator>,
    pub(crate) command_pool: vk::CommandPool,
    pub(crate) queue: vk::Queue,
    #[allow(dead_code)] // RT engine / future consumers select by family
    pub(crate) queue_family: u32,
    /// The VK_KHR_acceleration_structure device loader — present exactly when
    /// the ray-query stack (accel structs + ray_query + BDA) was enabled at
    /// device creation. Its presence IS the tier-2 capability signal.
    pub(crate) accel_loader: Option<ash::khr::acceleration_structure::Device>,
    pub(crate) device: ash::Device,
    pub(crate) physical_device: vk::PhysicalDevice,
    pub(crate) surface_loader: ash::khr::surface::Instance,
    pub(crate) instance: ash::Instance,
    pub(crate) min_uniform_align: vk::DeviceSize,
    /// minAccelerationStructureScratchOffsetAlignment; 1 when no ray-query stack.
    pub(crate) as_scratch_align: vk::DeviceSize,
}

/// The process-wide Vulkan entry + instance every [`VkCore`] hangs off.
///
/// Instance creation is the expensive part of bringing up a renderer (ICD
/// enumeration + driver init, ~70ms warm and much worse on a cold cache), and
/// popup-style consumers (the cce-cloud daemon) create a renderer per window —
/// so the instance is created once and intentionally lives for the process.
struct SharedInstance {
    entry: ash::Entry,
    instance: ash::Instance,
    // Held so the messenger stays alive; never destroyed.
    _debug: Option<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
    /// VK_KHR_surface + VK_KHR_wayland_surface were available and enabled.
    has_wayland_surface: bool,
    api_version: u32,
}

static SHARED_INSTANCE: std::sync::OnceLock<SharedInstance> = std::sync::OnceLock::new();

fn shared_instance() -> &'static SharedInstance {
    SHARED_INSTANCE.get_or_init(|| unsafe {
        let t = std::time::Instant::now();
        let entry = ash::Entry::load().expect("Failed to load libvulkan");

        // Validation when available (debug builds or CCE_VK_VALIDATION=1).
        let want_validation =
            cfg!(debug_assertions) || std::env::var_os("CCE_VK_VALIDATION").is_some();
        let validation_available = want_validation
            && entry
                .enumerate_instance_layer_properties()
                .map(|layers| {
                    layers
                        .iter()
                        .any(|l| CStr::from_ptr(l.layer_name.as_ptr()) == VALIDATION_LAYER)
                })
                .unwrap_or(false);
        if want_validation && !validation_available {
            log::warn!(
                "Vulkan validation requested but VK_LAYER_KHRONOS_validation is not installed"
            );
        }

        let api_version = match entry.try_enumerate_instance_version().ok().flatten() {
            Some(v) if v >= vk::API_VERSION_1_2 => vk::API_VERSION_1_2,
            Some(v) => v,
            None => vk::API_VERSION_1_0,
        };
        let app_name = c"cce-ui";
        let app_info = vk::ApplicationInfo::default()
            .application_name(app_name)
            .engine_name(app_name)
            .api_version(api_version);

        // Surface extensions are enabled whenever the loader offers them, so
        // the one shared instance serves both windowed and headless cores.
        let ext_props = entry
            .enumerate_instance_extension_properties(None)
            .unwrap_or_default();
        let has_inst_ext = |name: &CStr| {
            ext_props
                .iter()
                .any(|e| CStr::from_ptr(e.extension_name.as_ptr()) == name)
        };
        let has_wayland_surface =
            has_inst_ext(ash::khr::surface::NAME) && has_inst_ext(ash::khr::wayland_surface::NAME);

        let mut extension_names: Vec<*const i8> = Vec::new();
        if has_wayland_surface {
            extension_names.push(ash::khr::surface::NAME.as_ptr());
            extension_names.push(ash::khr::wayland_surface::NAME.as_ptr());
        }
        if validation_available {
            extension_names.push(ash::ext::debug_utils::NAME.as_ptr());
        }
        let layer_names_owned: Vec<CString> = if validation_available {
            vec![VALIDATION_LAYER.to_owned()]
        } else {
            Vec::new()
        };
        let layer_names: Vec<*const i8> = layer_names_owned.iter().map(|l| l.as_ptr()).collect();

        let instance = entry
            .create_instance(
                &vk::InstanceCreateInfo::default()
                    .application_info(&app_info)
                    .enabled_extension_names(&extension_names)
                    .enabled_layer_names(&layer_names),
                None,
            )
            .expect("Failed to create Vulkan instance");

        let debug = if validation_available {
            let loader = ash::ext::debug_utils::Instance::new(&entry, &instance);
            let messenger = loader
                .create_debug_utils_messenger(
                    &vk::DebugUtilsMessengerCreateInfoEXT::default()
                        .message_severity(
                            vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                                | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING,
                        )
                        .message_type(
                            vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                                | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
                        )
                        .pfn_user_callback(Some(debug_callback)),
                    None,
                )
                .expect("Failed to create debug messenger");
            log::info!("Vulkan validation layers enabled");
            Some((loader, messenger))
        } else {
            None
        };

        log::debug!("[timing] shared Vulkan instance init: {:?}", t.elapsed());
        SharedInstance {
            entry,
            instance,
            _debug: debug,
            has_wayland_surface,
            api_version,
        }
    })
}

impl VkCore {
    /// A core bound to a Wayland surface: the returned `vk::SurfaceKHR` is
    /// created from the raw pointers and the chosen device supports presenting
    /// to it. The caller owns the surface handle (destroy it before the core).
    ///
    /// # Safety
    /// `display_ptr` and `surface_ptr` must be live `wl_display` / `wl_surface`
    /// pointers that outlive the core and everything created from it.
    pub unsafe fn new_for_wayland_surface(
        display_ptr: *mut c_void,
        surface_ptr: *mut c_void,
    ) -> (Self, vk::SurfaceKHR) {
        let (core, surface) = Self::new_inner(Some((display_ptr, surface_ptr)));
        (core, surface.expect("surface requested but not created"))
    }

    /// A windowless core: no surface extensions, any graphics-capable device.
    /// For offscreen rendering (thumbnails, previews) and compute.
    pub fn new_headless() -> Self {
        unsafe { Self::new_inner(None).0 }
    }

    unsafe fn new_inner(
        wayland: Option<(*mut c_void, *mut c_void)>,
    ) -> (Self, Option<vk::SurfaceKHR>) {
        // CCE_VK_DEVICE: "integrated" (the default), "discrete", or a device
        // name substring. An explicit request also lifts a session-wide ICD
        // pin (VK_DRIVER_FILES / VK_ICD_FILENAMES) for THIS process — the
        // common setup pins Vulkan to the iGPU to keep the dGPU asleep, which
        // would otherwise make "discrete" unsatisfiable.
        let device_pref = std::env::var("CCE_VK_DEVICE")
            .ok()
            .map(|v| v.to_lowercase())
            .filter(|v| !v.is_empty());
        if device_pref.is_some() {
            std::env::remove_var("VK_DRIVER_FILES");
            std::env::remove_var("VK_ICD_FILENAMES");
        }

        let shared = shared_instance();
        let entry = &shared.entry;
        let instance = shared.instance.clone();
        let api_version = shared.api_version;
        if wayland.is_some() && !shared.has_wayland_surface {
            panic!("Vulkan loader offers no VK_KHR_wayland_surface but a window was requested");
        }

        // Instance-level loader; only usable when VK_KHR_surface was enabled.
        let surface_loader = ash::khr::surface::Instance::new(entry, &instance);

        let surface = wayland.map(|(display_ptr, surface_ptr)| {
            let wayland_loader = ash::khr::wayland_surface::Instance::new(entry, &instance);
            wayland_loader
                .create_wayland_surface(
                    &vk::WaylandSurfaceCreateInfoKHR::default()
                        .display(display_ptr)
                        .surface(surface_ptr),
                    None,
                )
                .expect("Failed to create Wayland surface")
        });

        // Physical device + queue family: graphics, plus present support when
        // a surface exists. Prefer integrated (the toolkit's LowPower default)
        // unless CCE_VK_DEVICE says otherwise; an unsatisfiable preference
        // falls back to the default order rather than failing.
        let mut candidates: Vec<(vk::PhysicalDevice, u32, i32)> = Vec::new();
        for pd in instance
            .enumerate_physical_devices()
            .expect("No Vulkan physical devices")
        {
            let families = instance.get_physical_device_queue_family_properties(pd);
            let family = families.iter().enumerate().find_map(|(i, f)| {
                let graphics = f.queue_flags.contains(vk::QueueFlags::GRAPHICS);
                let present = match surface {
                    Some(surface) => surface_loader
                        .get_physical_device_surface_support(pd, i as u32, surface)
                        .unwrap_or(false),
                    None => true,
                };
                (graphics && present).then_some(i as u32)
            });
            if let Some(family) = family {
                let props = instance.get_physical_device_properties(pd);
                let name = CStr::from_ptr(props.device_name.as_ptr())
                    .to_string_lossy()
                    .to_lowercase();
                let type_rank = match props.device_type {
                    vk::PhysicalDeviceType::INTEGRATED_GPU => 0,
                    vk::PhysicalDeviceType::DISCRETE_GPU => 1,
                    vk::PhysicalDeviceType::VIRTUAL_GPU => 2,
                    _ => 3,
                };
                let rank = match device_pref.as_deref() {
                    Some("discrete") => match props.device_type {
                        vk::PhysicalDeviceType::DISCRETE_GPU => 0,
                        other => {
                            1 + match other {
                                vk::PhysicalDeviceType::INTEGRATED_GPU => 0,
                                vk::PhysicalDeviceType::VIRTUAL_GPU => 2,
                                _ => 3,
                            }
                        }
                    },
                    Some("integrated") | None => type_rank,
                    Some(substr) => {
                        if name.contains(substr) {
                            0
                        } else {
                            1 + type_rank
                        }
                    }
                };
                candidates.push((pd, family, rank));
            }
        }
        candidates.sort_by_key(|&(_, _, rank)| rank);
        let (physical_device, queue_family, _) = *candidates
            .first()
            .expect("No suitable Vulkan device found");
        {
            let props = instance.get_physical_device_properties(physical_device);
            let name = CStr::from_ptr(props.device_name.as_ptr()).to_string_lossy();
            log::info!("Vulkan device: {name}");
        }
        let min_uniform_align = instance
            .get_physical_device_properties(physical_device)
            .limits
            .min_uniform_buffer_offset_alignment;

        let queue_priorities = [1.0f32];
        let queue_infos = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family)
            .queue_priorities(&queue_priorities)];
        let mut device_extensions: Vec<*const i8> = if wayland.is_some() {
            vec![ash::khr::swapchain::NAME.as_ptr()]
        } else {
            Vec::new()
        };

        // The ray-query stack (the RT engine's tier 2): needs the three
        // extensions plus the BDA / accel-structure / ray-query features and
        // an API >= 1.2 device (SPIR-V 1.4 shaders). Enabled whenever the
        // device offers it; consumers check `accel_loader`.
        let ext_props = instance
            .enumerate_device_extension_properties(physical_device)
            .unwrap_or_default();
        let has_ext = |name: &CStr| {
            ext_props
                .iter()
                .any(|e| CStr::from_ptr(e.extension_name.as_ptr()) == name)
        };
        let device_api = instance
            .get_physical_device_properties(physical_device)
            .api_version
            .min(api_version);
        let mut ray_query = device_api >= vk::API_VERSION_1_2
            && has_ext(ash::khr::acceleration_structure::NAME)
            && has_ext(ash::khr::ray_query::NAME)
            && has_ext(ash::khr::deferred_host_operations::NAME);
        if ray_query {
            let mut bda = vk::PhysicalDeviceBufferDeviceAddressFeatures::default();
            let mut asf = vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default();
            let mut rqf = vk::PhysicalDeviceRayQueryFeaturesKHR::default();
            let mut features2 = vk::PhysicalDeviceFeatures2::default()
                .push_next(&mut bda)
                .push_next(&mut asf)
                .push_next(&mut rqf);
            instance.get_physical_device_features2(physical_device, &mut features2);
            ray_query = bda.buffer_device_address == vk::TRUE
                && asf.acceleration_structure == vk::TRUE
                && rqf.ray_query == vk::TRUE;
        }

        let mut bda_features =
            vk::PhysicalDeviceBufferDeviceAddressFeatures::default().buffer_device_address(true);
        let mut as_features = vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default()
            .acceleration_structure(true);
        let mut rq_features = vk::PhysicalDeviceRayQueryFeaturesKHR::default().ray_query(true);
        let mut device_info = vk::DeviceCreateInfo::default().queue_create_infos(&queue_infos);
        if ray_query {
            device_extensions.push(ash::khr::acceleration_structure::NAME.as_ptr());
            device_extensions.push(ash::khr::ray_query::NAME.as_ptr());
            device_extensions.push(ash::khr::deferred_host_operations::NAME.as_ptr());
            device_info = device_info
                .push_next(&mut bda_features)
                .push_next(&mut as_features)
                .push_next(&mut rq_features);
            log::info!("Vulkan ray-query stack enabled (RT tier 2 available)");
        }
        let t = std::time::Instant::now();
        let device = instance
            .create_device(
                physical_device,
                &device_info.enabled_extension_names(&device_extensions),
                None,
            )
            .expect("Failed to create Vulkan device");
        log::debug!("[timing] vk create_device: {:?}", t.elapsed());
        let queue = device.get_device_queue(queue_family, 0);
        let accel_loader =
            ray_query.then(|| ash::khr::acceleration_structure::Device::new(&instance, &device));
        let as_scratch_align = if ray_query {
            let mut as_props = vk::PhysicalDeviceAccelerationStructurePropertiesKHR::default();
            let mut props2 = vk::PhysicalDeviceProperties2::default().push_next(&mut as_props);
            instance.get_physical_device_properties2(physical_device, &mut props2);
            (as_props.min_acceleration_structure_scratch_offset_alignment as vk::DeviceSize).max(1)
        } else {
            1
        };

        let allocator = Allocator::new(&AllocatorCreateDesc {
            instance: instance.clone(),
            device: device.clone(),
            physical_device,
            debug_settings: Default::default(),
            buffer_device_address: ray_query,
            allocation_sizes: Default::default(),
        })
        .expect("Failed to create GPU allocator");

        let command_pool = device
            .create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                    .queue_family_index(queue_family),
                None,
            )
            .expect("Failed to create command pool");

        (
            VkCore {
                allocator: Some(allocator),
                command_pool,
                queue,
                queue_family,
                accel_loader,
                device,
                physical_device,
                surface_loader,
                instance,
                min_uniform_align,
                as_scratch_align,
            },
            surface,
        )
    }
}

impl Drop for VkCore {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            // The allocator must go before the device it allocates from. The
            // instance is process-shared and intentionally never destroyed.
            drop(self.allocator.take());
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
        }
    }
}
