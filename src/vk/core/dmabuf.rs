//! Which dma-bufs the device can import and sample: another process's GPU
//! memory — a web engine's frame — drawn straight from where it was
//! rendered, with no copy through the CPU (`image::dmabuf` does the
//! importing).
//!
//! A producer has to be told what to render into *before* it starts, and
//! that is usually before this process has a window, let alone a renderer
//! (WebKit settles a toplevel's buffer type when it first uses it, and a
//! browser loads its first page while it boots). So [`probe_dmabuf`] answers
//! without one: it picks the device the renderer will pick, by the same
//! `CCE_VK_DEVICE` ranking, and asks it.

use super::*;

use std::path::PathBuf;
use std::sync::OnceLock;

/// The device extensions an import takes, on top of Vulkan 1.2 (which
/// brings external memory, `VK_KHR_image_format_list` and
/// `vkGetPhysicalDeviceFormatProperties2`).
pub(super) const DMABUF_EXTENSIONS: [&CStr; 4] = [
    ash::khr::external_memory_fd::NAME,
    ash::ext::external_memory_dma_buf::NAME,
    ash::ext::image_drm_format_modifier::NAME,
    ash::ext::queue_family_foreign::NAME,
];

/// `XR24` and `AR24`: B,G,R,X/A in memory, what WebKit renders into.
const XRGB8888: u32 = u32::from_le_bytes(*b"XR24");
const ARGB8888: u32 = u32::from_le_bytes(*b"AR24");

/// What a producer may render into for this device to sample it.
#[derive(Clone, Debug, PartialEq)]
pub struct DmaBufSupport {
    /// The device's DRM nodes: render into buffers allocated here.
    pub render_node: PathBuf,
    pub primary_node: Option<PathBuf>,
    /// DRM fourcc, with every modifier the device can import and sample it
    /// in, in the driver's order. Single-plane layouts only: a modifier
    /// with an auxiliary plane (compression) is left out.
    pub formats: Vec<(u32, Vec<u64>)>,
}

/// The Vulkan format a fourcc is sampled as, sRGB like every other image
/// (`PixelFormat::vk`).
pub(crate) fn vk_format(fourcc: u32) -> Option<vk::Format> {
    match fourcc {
        XRGB8888 | ARGB8888 => Some(vk::Format::B8G8R8A8_SRGB),
        _ => None,
    }
}

/// Whether the fourcc's fourth channel is padding, to be read as opaque.
pub(crate) fn alpha_is_padding(fourcc: u32) -> bool {
    fourcc == XRGB8888
}

/// What the device the renderer will use can import, or `None` when it
/// cannot import dma-bufs at all (no extensions, an older Vulkan, or no DRM
/// node to say where to render). Asked once per process.
pub fn probe_dmabuf() -> Option<DmaBufSupport> {
    static PROBED: OnceLock<Option<DmaBufSupport>> = OnceLock::new();
    PROBED
        .get_or_init(|| unsafe {
            let pref = device_preference();
            let shared = shared_instance();
            let instance = &shared.instance;
            let mut best: Option<(vk::PhysicalDevice, i32)> = None;
            for pd in instance.enumerate_physical_devices().unwrap_or_default() {
                let graphics = instance
                    .get_physical_device_queue_family_properties(pd)
                    .iter()
                    .any(|f| f.queue_flags.contains(vk::QueueFlags::GRAPHICS));
                if !graphics {
                    continue;
                }
                let rank = device_rank(&instance.get_physical_device_properties(pd), pref.as_deref());
                if best.is_none_or(|(_, r)| rank < r) {
                    best = Some((pd, rank));
                }
            }
            let (pd, _) = best?;
            let support = dmabuf_support(instance, pd, shared.api_version);
            match &support {
                Some(s) => log::info!(
                    "[vk] dma-buf import: {} ({} formats)",
                    s.render_node.display(),
                    s.formats.len()
                ),
                None => log::info!("[vk] dma-buf import: not available on this device"),
            }
            support
        })
        .clone()
}

/// Whether `pd` has everything an import takes.
pub(super) unsafe fn has_dmabuf_extensions(instance: &ash::Instance, pd: vk::PhysicalDevice, api_version: u32) -> bool {
    let api = instance.get_physical_device_properties(pd).api_version.min(api_version);
    if api < vk::API_VERSION_1_2 {
        return false;
    }
    let exts = instance.enumerate_device_extension_properties(pd).unwrap_or_default();
    let has = |name: &CStr| exts.iter().any(|e| CStr::from_ptr(e.extension_name.as_ptr()) == name);
    DMABUF_EXTENSIONS.iter().all(|n| has(n)) && has(ash::ext::physical_device_drm::NAME)
}

/// What `pd` can import, asked of the driver.
pub(super) unsafe fn dmabuf_support(instance: &ash::Instance, pd: vk::PhysicalDevice, api_version: u32) -> Option<DmaBufSupport> {
    if !has_dmabuf_extensions(instance, pd, api_version) {
        return None;
    }
    let mut drm = vk::PhysicalDeviceDrmPropertiesEXT::default();
    let mut props2 = vk::PhysicalDeviceProperties2::default().push_next(&mut drm);
    instance.get_physical_device_properties2(pd, &mut props2);
    if drm.has_render == vk::FALSE {
        return None;
    }
    let render_node = dri_node(drm.render_major, drm.render_minor)?;
    let primary_node = (drm.has_primary == vk::TRUE)
        .then(|| dri_node(drm.primary_major, drm.primary_minor))
        .flatten();
    let mut formats = Vec::new();
    for fourcc in [XRGB8888, ARGB8888] {
        let Some(format) = vk_format(fourcc) else { continue };
        let modifiers = sampleable_modifiers(instance, pd, format);
        if !modifiers.is_empty() {
            formats.push((fourcc, modifiers));
        }
    }
    (!formats.is_empty()).then_some(DmaBufSupport { render_node, primary_node, formats })
}

/// Every single-plane modifier `format` can be imported from a dma-buf and
/// sampled (linearly filtered) in.
unsafe fn sampleable_modifiers(instance: &ash::Instance, pd: vk::PhysicalDevice, format: vk::Format) -> Vec<u64> {
    let mut list = vk::DrmFormatModifierPropertiesListEXT::default();
    let mut props = vk::FormatProperties2::default().push_next(&mut list);
    instance.get_physical_device_format_properties2(pd, format, &mut props);
    let mut mods = vec![vk::DrmFormatModifierPropertiesEXT::default(); list.drm_format_modifier_count as usize];
    let mut list = vk::DrmFormatModifierPropertiesListEXT::default().drm_format_modifier_properties(&mut mods);
    let mut props = vk::FormatProperties2::default().push_next(&mut list);
    instance.get_physical_device_format_properties2(pd, format, &mut props);
    let wanted = vk::FormatFeatureFlags::SAMPLED_IMAGE | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR;
    mods.iter()
        .filter(|m| m.drm_format_modifier_plane_count == 1 && m.drm_format_modifier_tiling_features.contains(wanted))
        .map(|m| m.drm_format_modifier)
        .filter(|&modifier| importable(instance, pd, format, modifier))
        .collect()
}

/// Whether an image of `format` in `modifier` can be made from imported
/// dma-buf memory.
unsafe fn importable(instance: &ash::Instance, pd: vk::PhysicalDevice, format: vk::Format, modifier: u64) -> bool {
    let mut drm_info = vk::PhysicalDeviceImageDrmFormatModifierInfoEXT::default()
        .drm_format_modifier(modifier)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let mut external = vk::PhysicalDeviceExternalImageFormatInfo::default()
        .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let info = vk::PhysicalDeviceImageFormatInfo2::default()
        .format(format)
        .ty(vk::ImageType::TYPE_2D)
        .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
        .usage(vk::ImageUsageFlags::SAMPLED)
        .push_next(&mut external)
        .push_next(&mut drm_info);
    let mut external_props = vk::ExternalImageFormatProperties::default();
    let mut out = vk::ImageFormatProperties2::default().push_next(&mut external_props);
    instance.get_physical_device_image_format_properties2(pd, &info, &mut out).is_ok()
        && external_props
            .external_memory_properties
            .external_memory_features
            .contains(vk::ExternalMemoryFeatureFlags::IMPORTABLE)
}

/// The `/dev/dri` node with this device number.
fn dri_node(major: i64, minor: i64) -> Option<PathBuf> {
    use std::os::unix::fs::MetadataExt;
    let (major, minor) = (major as u64, minor as u64);
    // glibc's makedev.
    let rdev = ((major & 0xffff_f000) << 32) | ((major & 0xfff) << 8) | ((minor & 0xffff_ff00) << 12) | (minor & 0xff);
    std::fs::read_dir("/dev/dri")
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| std::fs::metadata(p).is_ok_and(|m| m.rdev() == rdev))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webkits_formats_are_sampled_as_srgb_bgra() {
        assert_eq!(vk_format(XRGB8888), Some(vk::Format::B8G8R8A8_SRGB));
        assert_eq!(vk_format(ARGB8888), Some(vk::Format::B8G8R8A8_SRGB));
        assert_eq!(vk_format(u32::from_le_bytes(*b"NV12")), None);
        assert!(alpha_is_padding(XRGB8888) && !alpha_is_padding(ARGB8888));
    }
}
