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
    // device, the device before debug/instance; `_entry` (the loaded library)
    // must outlive everything.
    pub(crate) allocator: Option<Allocator>,
    pub(crate) command_pool: vk::CommandPool,
    pub(crate) queue: vk::Queue,
    #[allow(dead_code)] // RT engine / future consumers select by family
    pub(crate) queue_family: u32,
    pub(crate) device: ash::Device,
    pub(crate) physical_device: vk::PhysicalDevice,
    pub(crate) surface_loader: ash::khr::surface::Instance,
    pub(crate) debug: Option<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
    pub(crate) instance: ash::Instance,
    pub(crate) _entry: ash::Entry,
    pub(crate) min_uniform_align: vk::DeviceSize,
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

        let mut extension_names: Vec<*const i8> = Vec::new();
        if wayland.is_some() {
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

        // Instance-level loader; only usable when VK_KHR_surface was enabled.
        let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);

        let surface = wayland.map(|(display_ptr, surface_ptr)| {
            let wayland_loader = ash::khr::wayland_surface::Instance::new(&entry, &instance);
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
        // a surface exists. Prefer integrated (the toolkit's LowPower default).
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
                let rank = match props.device_type {
                    vk::PhysicalDeviceType::INTEGRATED_GPU => 0,
                    vk::PhysicalDeviceType::DISCRETE_GPU => 1,
                    vk::PhysicalDeviceType::VIRTUAL_GPU => 2,
                    _ => 3,
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
        let device_extensions: Vec<*const i8> = if wayland.is_some() {
            vec![ash::khr::swapchain::NAME.as_ptr()]
        } else {
            Vec::new()
        };
        let device = instance
            .create_device(
                physical_device,
                &vk::DeviceCreateInfo::default()
                    .queue_create_infos(&queue_infos)
                    .enabled_extension_names(&device_extensions),
                None,
            )
            .expect("Failed to create Vulkan device");
        let queue = device.get_device_queue(queue_family, 0);

        let allocator = Allocator::new(&AllocatorCreateDesc {
            instance: instance.clone(),
            device: device.clone(),
            physical_device,
            debug_settings: Default::default(),
            buffer_device_address: false,
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
                device,
                physical_device,
                surface_loader,
                debug,
                instance,
                _entry: entry,
                min_uniform_align,
            },
            surface,
        )
    }
}

impl Drop for VkCore {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            // The allocator must go before the device it allocates from.
            drop(self.allocator.take());
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
            if let Some((loader, messenger)) = self.debug.take() {
                loader.destroy_debug_utils_messenger(messenger, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}
