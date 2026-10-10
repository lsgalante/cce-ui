//! `VkCore`: the device-level half of the Vulkan backend — instance (+
//! validation layers), physical device + graphics queue, gpu-allocator, and
//! the shared command pool.
//!
//! Two ways in: [`VkCore::new_for_surface`] picks a present-capable device
//! for a window — a Wayland surface, or on macOS a `CAMetalLayer` through
//! MoltenVK (what `VkRenderer` uses) — and [`VkCore::new_headless`]
//! builds the same core with no surface at all — for offscreen consumers
//! (thumbnail rendering, previews, the future RT engine) that render into
//! images instead of a swapchain.
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | `VkCore`: its constructors (for a surface, or headless), the device setup, `Drop` |
//! | `instance` | the one Vulkan instance a process shares, with its extensions and validation |
//! | `surface` | `SurfaceTarget` (what a surface is made from) and `SurfaceLost` |
//! | `device` | naming a device's type, and reporting a device preference that was not met |

mod device;
mod instance;
mod surface;

use device::*;
use instance::*;
pub use surface::*;

use std::ffi::{c_void, CStr, CString};

use ash::vk;
use gpu_allocator::vulkan::{Allocator, AllocatorCreateDesc};


/// Memory blocks start small and double as a client needs more, up to the
/// allocator's old fixed sizes. Those defaults (a 256 MiB device block and a
/// 64 MiB host one, reserved on the first allocation of each) held ~320 MiB
/// for every client, against ~60 MiB of allocations in a typical window — a
/// status-bar module included. On the iGPU that is system RAM. Until
/// gpu-allocator 0.28 the sizes passed here were ignored (0.27.0's
/// `Allocator::new` stored the defaults whatever the descriptor said).
/// An allocation larger than the current block size gets one of its own.
fn allocation_sizes() -> gpu_allocator::AllocationSizes {
    const MIB: u64 = 1024 * 1024;
    gpu_allocator::AllocationSizes::new(8 * MIB, 4 * MIB)
        .with_max_device_memblock_size(256 * MIB)
        .with_max_host_memblock_size(64 * MIB)
}

pub struct VkCore {
    // Field order is drop order: allocator and command pool go before the
    // device. The instance (and the loaded library) is process-shared and
    // never destroyed — see `shared_instance()`.
    pub(crate) allocator: Option<Allocator>,
    pub(crate) command_pool: vk::CommandPool,
    pub(crate) queue: vk::Queue,
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
    /// Widest rasterizable line (device lineWidthRange cap); 1.0 when the
    /// wideLines feature is absent or disabled.
    pub(crate) max_line_width: f32,
    /// The most anisotropy a sampler may ask for (device
    /// maxSamplerAnisotropy); 1.0 when the samplerAnisotropy feature is
    /// absent, which is a sampler that asks for none.
    pub(crate) max_anisotropy: f32,
    /// VK_KHR_incremental_present is enabled: a present may name the
    /// rectangles that changed, which the Wayland WSI forwards as the
    /// surface's buffer damage in place of "everything".
    pub(crate) incremental_present: bool,
}

impl VkCore {
    /// A core bound to a Wayland surface: the returned `vk::SurfaceKHR` is
    /// created from the raw pointers and the chosen device supports presenting
    /// to it. The caller owns the surface handle (destroy it before the core).
    ///
    /// # Safety
    /// `display_ptr` and `surface_ptr` must be live `wl_display` / `wl_surface`
    /// pointers that outlive the core and everything created from it.
    ///
    /// Fails with [`SurfaceLost`] when the surface cannot be created or no
    /// device can be asked whether it presents to it — a dead display
    /// connection, not a driver fault.
    pub unsafe fn new_for_wayland_surface(
        display_ptr: *mut c_void,
        surface_ptr: *mut c_void,
    ) -> Result<(Self, vk::SurfaceKHR), SurfaceLost> {
        Self::new_for_surface(SurfaceTarget::Wayland { display: display_ptr, surface: surface_ptr })
    }

    /// A core bound to a window's surface, whatever the window system: the
    /// general form of [`new_for_wayland_surface`](Self::new_for_wayland_surface).
    ///
    /// # Safety
    /// The target's pointers must be live and outlive the core and
    /// everything created from it.
    pub unsafe fn new_for_surface(target: SurfaceTarget) -> Result<(Self, vk::SurfaceKHR), SurfaceLost> {
        let (core, surface) = Self::new_inner(Some(target))?;
        Ok((core, surface.expect("surface requested but not created")))
    }

    /// A new `VkSurfaceKHR` on another Wayland surface, from this core's
    /// instance — for a renderer moving to a fresh `wl_surface` (a menu popup
    /// re-opened) without a new device. The caller owns the handle.
    ///
    /// # Safety
    /// `display_ptr` and `surface_ptr` must be live `wl_display` / `wl_surface`
    /// pointers that outlive the returned surface.
    pub unsafe fn create_wayland_surface(
        &self,
        display_ptr: *mut c_void,
        surface_ptr: *mut c_void,
    ) -> Result<vk::SurfaceKHR, SurfaceLost> {
        self.create_surface(SurfaceTarget::Wayland { display: display_ptr, surface: surface_ptr })
    }

    /// A new `VkSurfaceKHR` on another window, from this core's instance:
    /// the general form of [`create_wayland_surface`](Self::create_wayland_surface).
    ///
    /// # Safety
    /// The target's pointers must be live and outlive the returned surface.
    pub unsafe fn create_surface(&self, target: SurfaceTarget) -> Result<vk::SurfaceKHR, SurfaceLost> {
        let surface = target.create(&self.instance)?;
        // The device was chosen for the FIRST surface's present support; a
        // later surface on the same display is presentable from the same
        // family on every driver this runs on, but say so if not.
        if !self
            .surface_loader
            .get_physical_device_surface_support(self.physical_device, self.queue_family, surface)
            .unwrap_or(false)
        {
            log::warn!("[vk] queue family {} cannot present to the re-attached surface", self.queue_family);
        }
        Ok(surface)
    }

    /// A windowless core: no surface extensions, any graphics-capable device.
    /// For offscreen rendering (thumbnails, previews) and compute.
    pub fn new_headless() -> Self {
        // Only a surface can be lost, and there is none here.
        unsafe { Self::new_inner(None).expect("headless core cannot lose a surface").0 }
    }

    unsafe fn new_inner(
        window: Option<SurfaceTarget>,
    ) -> Result<(Self, Option<vk::SurfaceKHR>), SurfaceLost> {
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

        // Instance-level loader; only usable when VK_KHR_surface was enabled.
        let surface_loader = ash::khr::surface::Instance::new(entry, &instance);

        let surface = match window {
            Some(target) => Some(target.create(&instance)?),
            None => None,
        };

        // Physical device + queue family: graphics, plus present support when
        // a surface exists. Prefer integrated (the toolkit's LowPower default)
        // unless CCE_VK_DEVICE says otherwise; an unsatisfiable preference
        // falls back to the default order rather than failing.
        let mut candidates: Vec<(vk::PhysicalDevice, u32, i32)> = Vec::new();
        // A present-support query that FAILED, as opposed to answering no:
        // on a dead display connection every device fails it, and "no
        // suitable device" would then misreport a lost surface.
        let mut support_error: Option<vk::Result> = None;
        // Every device the loader offered, for the warning below when the
        // preference cannot be met — including those that cannot draw here.
        let mut seen: Vec<String> = Vec::new();
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
                        .unwrap_or_else(|e| {
                            support_error = Some(e);
                            false
                        }),
                    None => true,
                };
                (graphics && present).then_some(i as u32)
            });
            {
                let props = instance.get_physical_device_properties(pd);
                let name = CStr::from_ptr(props.device_name.as_ptr()).to_string_lossy();
                let usable = if family.is_some() { "" } else { ", cannot draw to this window" };
                seen.push(format!("{name} ({}{usable})", device_type_name(props.device_type)));
            }
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
        if let (true, Some(surface), Some(result)) = (candidates.is_empty(), surface, support_error) {
            surface_loader.destroy_surface(surface, None);
            return Err(SurfaceLost { call: "vkGetPhysicalDeviceSurfaceSupportKHR", result });
        }
        let (physical_device, queue_family, chosen_rank) = *candidates
            .first()
            .expect("No suitable Vulkan device found");
        {
            let props = instance.get_physical_device_properties(physical_device);
            let name = CStr::from_ptr(props.device_name.as_ptr()).to_string_lossy();
            log::info!("Vulkan device: {name}");
            // An explicit preference that was not met falls back SILENTLY
            // otherwise — and a fallback renders as the device asked for
            // would, so nothing on screen says it happened (2026-10-02: hours
            // of "discrete" shadow captures that were all on the Intel GPU).
            // stderr, not `log`: most clients init no logger.
            let chosen = format!("{name} ({})", device_type_name(props.device_type));
            if let Some(msg) = unmet_device_preference(device_pref.as_deref(), chosen_rank, &chosen, &seen) {
                static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
                if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                    eprintln!("{msg}");
                }
            }
        }
        let min_uniform_align = instance
            .get_physical_device_properties(physical_device)
            .limits
            .min_uniform_buffer_offset_alignment;

        let queue_priorities = [1.0f32];
        let queue_infos = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family)
            .queue_priorities(&queue_priorities)];
        let mut device_extensions: Vec<*const i8> = if window.is_some() {
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
        // wideLines for adjustable wire thickness (the scene stage's dynamic
        // line width); without it widths clamp to 1.0. (PolygonMode::LINE /
        // fillModeNonSolid is deliberately NOT used — wires are LINE_LIST
        // edge meshes; see the scene stage's wireframe pipeline comment.)
        let supported_features = instance.get_physical_device_features(physical_device);
        let wide_lines_supported = supported_features.wide_lines == vk::TRUE;
        let max_line_width = if wide_lines_supported {
            instance
                .get_physical_device_properties(physical_device)
                .limits
                .line_width_range[1]
        } else {
            1.0
        };
        // samplerAnisotropy for user images seen at a slant (a picture
        // standing in the 3D scene): without it a mipmapped image viewed
        // edge-on is blurred along BOTH axes to the level its short axis
        // asks for.
        let anisotropy_supported = supported_features.sampler_anisotropy == vk::TRUE;
        let max_anisotropy = if anisotropy_supported {
            instance
                .get_physical_device_properties(physical_device)
                .limits
                .max_sampler_anisotropy
                .max(1.0)
        } else {
            1.0
        };
        let enabled_features = vk::PhysicalDeviceFeatures::default()
            .wide_lines(wide_lines_supported)
            .sampler_anisotropy(anisotropy_supported);
        let mut device_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_infos)
            .enabled_features(&enabled_features);
        let incremental_present =
            window.is_some() && has_ext(ash::khr::incremental_present::NAME);
        // A portability driver's device (MoltenVK) MUST enable the subset
        // extension it offers; no Linux driver offers it.
        if has_ext(ash::khr::portability_subset::NAME) {
            device_extensions.push(ash::khr::portability_subset::NAME.as_ptr());
        }
        if incremental_present {
            device_extensions.push(ash::khr::incremental_present::NAME.as_ptr());
        }
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
            allocation_sizes: allocation_sizes(),
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

        Ok((
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
                max_line_width,
                max_anisotropy,
                incremental_present,
            },
            surface,
        ))
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
