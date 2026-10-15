//! Choosing a physical device: the `CCE_VK_DEVICE` preference and the rank it
//! gives each device, a device type's name, and the message printed when the
//! preference could not be met.

use super::*;

/// `CCE_VK_DEVICE`: "integrated" (the default), "discrete", or a device name
/// substring, lower-cased. An explicit request also lifts a session-wide ICD
/// pin (VK_DRIVER_FILES / VK_ICD_FILENAMES) for THIS process — the common
/// setup pins Vulkan to the iGPU to keep the dGPU asleep, which would
/// otherwise make "discrete" unsatisfiable. It has to run before the shared
/// instance is first made, which is why the dma-buf probe asks it too.
pub(super) fn device_preference() -> Option<String> {
    let pref = std::env::var("CCE_VK_DEVICE")
        .ok()
        .map(|v| v.to_lowercase())
        .filter(|v| !v.is_empty());
    if pref.is_some() {
        std::env::remove_var("VK_DRIVER_FILES");
        std::env::remove_var("VK_ICD_FILENAMES");
    }
    pref
}

/// How well a device meets the preference: 0 is a match, lower is better.
/// Integrated first unless the preference says otherwise; an unsatisfiable
/// preference falls back to that order rather than failing.
pub(super) unsafe fn device_rank(props: &vk::PhysicalDeviceProperties, pref: Option<&str>) -> i32 {
    let name = CStr::from_ptr(props.device_name.as_ptr()).to_string_lossy().to_lowercase();
    let type_rank = match props.device_type {
        vk::PhysicalDeviceType::INTEGRATED_GPU => 0,
        vk::PhysicalDeviceType::DISCRETE_GPU => 1,
        vk::PhysicalDeviceType::VIRTUAL_GPU => 2,
        _ => 3,
    };
    match pref {
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
    }
}

pub(super) fn device_type_name(t: vk::PhysicalDeviceType) -> &'static str {
    match t {
        vk::PhysicalDeviceType::INTEGRATED_GPU => "integrated",
        vk::PhysicalDeviceType::DISCRETE_GPU => "discrete",
        vk::PhysicalDeviceType::VIRTUAL_GPU => "virtual",
        vk::PhysicalDeviceType::CPU => "cpu",
        _ => "other",
    }
}

/// The warning for a `CCE_VK_DEVICE` preference the chosen device does not
/// meet, or `None` when there was no explicit preference or it was met (a
/// rank of 0 is a match, whatever the preference). `chosen` and `seen` are
/// device descriptions, `seen` every one the loader offered.
///
/// A vendor whose driver failed to LOAD is in no list at all, which is the
/// case worth naming: in a cce-shadow session the NVIDIA ICD will not load
/// without an X display (`DISPLAY` and `XAUTHORITY`), and the loader says why
/// only when asked (`VK_LOADER_DEBUG=error`).
pub(super) fn unmet_device_preference(pref: Option<&str>, chosen_rank: i32, chosen: &str, seen: &[String]) -> Option<String> {
    let pref = pref?;
    if chosen_rank == 0 {
        return None;
    }
    Some(format!(
        "cce-ui: CCE_VK_DEVICE={pref} is not met; using {chosen}. Devices the Vulkan loader offered: {}. \
         A driver that failed to load is not among them (VK_LOADER_DEBUG=error says why; in a \
         cce-shadow session the NVIDIA driver needs DISPLAY and XAUTHORITY).",
        seen.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An explicit preference that was not met says so; one that was met, or
    /// no preference at all, says nothing.
    #[test]
    fn an_unmet_device_preference_is_reported() {
        let seen = vec!["Intel(R) Iris(R) Xe Graphics (integrated)".to_string()];
        let msg = unmet_device_preference(Some("discrete"), 1, &seen[0], &seen).expect("unmet");
        assert!(msg.contains("CCE_VK_DEVICE=discrete is not met"), "{msg}");
        assert!(msg.contains("using Intel(R) Iris(R) Xe Graphics (integrated)"), "{msg}");
        assert!(msg.contains("DISPLAY and XAUTHORITY"), "{msg}");
        assert_eq!(unmet_device_preference(Some("discrete"), 0, &seen[0], &seen), None);
        assert_eq!(unmet_device_preference(None, 1, &seen[0], &seen), None);
    }
}
