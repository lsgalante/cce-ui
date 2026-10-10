//! Choosing a physical device: a device type's name, and the message printed when a
//! `CCE_VK_DEVICE` preference could not be met.

use super::*;

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
