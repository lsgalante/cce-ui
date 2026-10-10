//! The one Vulkan instance a process shares — its loader entry, the extensions it enables (the
//! surface kinds, portability enumeration on macOS) and the validation layers — made on first use.

use super::*;

pub(super) const VALIDATION_LAYER: &CStr = c"VK_LAYER_KHRONOS_validation";

pub(super) unsafe extern "system" fn debug_callback(
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

/// The process-wide Vulkan entry + instance every [`VkCore`] hangs off.
///
/// Instance creation is the expensive part of bringing up a renderer (ICD
/// enumeration + driver init, ~70ms warm and much worse on a cold cache), and
/// popup-style consumers (the cce-cloud daemon) create a renderer per window —
/// so the instance is created once and intentionally lives for the process.
pub(super) struct SharedInstance {
    pub(super) entry: ash::Entry,
    pub(super) instance: ash::Instance,
    // Held so the messenger stays alive; never destroyed.
    pub(super) _debug: Option<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
    /// VK_KHR_surface + VK_KHR_wayland_surface were available and enabled.
    pub(super) has_wayland_surface: bool,
    /// VK_KHR_surface + VK_EXT_metal_surface were (MoltenVK, on macOS).
    pub(super) has_metal_surface: bool,
    pub(super) api_version: u32,
}

static SHARED_INSTANCE: std::sync::OnceLock<SharedInstance> = std::sync::OnceLock::new();

pub(super) fn shared_instance() -> &'static SharedInstance {
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
        let has_surface = has_inst_ext(ash::khr::surface::NAME);
        let has_wayland_surface = has_surface && has_inst_ext(ash::khr::wayland_surface::NAME);
        let has_metal_surface = has_surface && has_inst_ext(ash::ext::metal_surface::NAME);
        // MoltenVK is a PORTABILITY driver: since loader 1.3.216 the loader
        // lists one only to an instance that asks for portability
        // enumeration, and a device of one must enable
        // VK_KHR_portability_subset (below). macOS only, so the instance a
        // Linux driver sees is the one it always saw.
        let portability = cfg!(target_os = "macos") && has_inst_ext(ash::khr::portability_enumeration::NAME);

        let mut extension_names: Vec<*const i8> = Vec::new();
        if has_wayland_surface || has_metal_surface {
            extension_names.push(ash::khr::surface::NAME.as_ptr());
        }
        if has_wayland_surface {
            extension_names.push(ash::khr::wayland_surface::NAME.as_ptr());
        }
        if has_metal_surface {
            extension_names.push(ash::ext::metal_surface::NAME.as_ptr());
        }
        if portability {
            extension_names.push(ash::khr::portability_enumeration::NAME.as_ptr());
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
                    .flags(if portability {
                        vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR
                    } else {
                        vk::InstanceCreateFlags::empty()
                    })
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
            has_metal_surface,
            api_version,
        }
    })
}
