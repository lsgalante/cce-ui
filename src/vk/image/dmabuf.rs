//! Images that are another process's GPU memory: a dma-buf a web engine
//! rendered a frame into, drawn by id like any other image but never copied
//! — no readback, no staging, no upload. [`probe_dmabuf`] says what a
//! producer may render into; [`import_dmabuf`] makes an image of one buffer.
//!
//! **A producer renders into a few buffers in turn** (WebKit: two or
//! three), so an image is made once per *buffer* and kept: each new frame
//! in it is announced with [`dmabuf_acquire`], and the buffer goes back to
//! the producer with [`dmabuf_release`] once nothing still draws it. The
//! caller decides when that is from [`frames_submitted`] and
//! [`frames_finished`]: a buffer last shown while `frames_submitted()` was
//! `n` is free once `frames_finished() >= n`.
//!
//! **Ownership moves with each frame.** The memory belongs to the
//! producer's queue (`VK_QUEUE_FAMILY_FOREIGN_EXT`) while it renders, and
//! the acquire is the barrier that hands it to this queue — making its
//! writes visible and putting it in the layout the shader reads — while the
//! release hands it back. Both are recorded in a one-off submission ahead of
//! the frame, as uploads are, so neither touches the frame's own commands.
//! [`take_released`] lists the buffers whose release has run: until then the
//! producer must not have it.
//!
//! The producer's rendering must be finished when the acquire runs. WebKit's
//! are by the time it hands a buffer over (measured: every one of 373, under
//! load); a caller that cannot be sure waits on the dma-buf's fence (`poll`
//! for reading) first.

use super::*;

use std::os::fd::{AsRawFd, IntoRawFd, OwnedFd};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

pub use super::super::core::{probe_dmabuf, DmaBufSupport};
use super::super::core::{alpha_is_padding, vk_format};

/// One plane of a dma-buf: its memory, where the plane starts in it, and
/// its row pitch.
pub struct DmaBufPlane {
    pub fd: OwnedFd,
    pub offset: u32,
    pub stride: u32,
}

/// A buffer someone else rendered into, as the producer describes it.
pub struct DmaBuf {
    pub width: u32,
    pub height: u32,
    /// DRM fourcc (`XR24`, `AR24`).
    pub fourcc: u32,
    /// DRM format modifier: the memory layout (tiling).
    pub modifier: u64,
    pub planes: Vec<DmaBufPlane>,
}

enum Op {
    Import { id: u32, buf: DmaBuf },
    Acquire(u32),
    Release(u32),
}

static OPS: Mutex<Vec<Op>> = Mutex::new(Vec::new());
static RELEASED: Mutex<Vec<u32>> = Mutex::new(Vec::new());
static SUBMITTED: AtomicU64 = AtomicU64::new(0);
static FINISHED: AtomicU64 = AtomicU64::new(0);

/// Make an image of `buf`, drawn by the id returned, which is usable at
/// once: until the import has landed (the next frame) a draw of it is
/// skipped, as for an upload. Its first contents count as acquired. Freed
/// with [`free_image`], which also lets go of the memory.
pub fn import_dmabuf(buf: DmaBuf) -> u32 {
    let id = crate::draw::images::next_image_id();
    OPS.lock().unwrap().push(Op::Import { id, buf });
    id
}

/// The producer has rendered a new frame into the buffer behind `id` (one
/// that was released before): take it back for drawing.
pub fn dmabuf_acquire(id: u32) {
    OPS.lock().unwrap().push(Op::Acquire(id));
}

/// Hand the buffer behind `id` back to its producer. Only once no frame
/// still draws it (see the module comment); the producer may have it when
/// [`take_released`] lists it.
pub fn dmabuf_release(id: u32) {
    OPS.lock().unwrap().push(Op::Release(id));
}

/// Ids whose release has run since the last call: their buffers are the
/// producer's again.
pub fn take_released() -> Vec<u32> {
    std::mem::take(&mut *RELEASED.lock().unwrap())
}

/// Frames the window's renderer has submitted so far.
pub fn frames_submitted() -> u64 {
    SUBMITTED.load(Ordering::Acquire)
}

/// The newest frame known to have finished on the GPU, counted as
/// [`frames_submitted`] counts them.
pub fn frames_finished() -> u64 {
    FINISHED.load(Ordering::Acquire)
}

/// A frame was submitted; its serial, for [`frame_finished`].
pub(crate) fn frame_submitted() -> u64 {
    SUBMITTED.fetch_add(1, Ordering::AcqRel) + 1
}

/// The frame with this serial has finished.
pub(crate) fn frame_finished(serial: u64) {
    FINISHED.fetch_max(serial, Ordering::AcqRel);
}

/// What importing takes of the device: its loader for dma-buf fds, its
/// memory types, and the queue family ownership moves to.
pub(crate) struct DmaBufCtx {
    memory_fd: ash::khr::external_memory_fd::Device,
    memory: vk::PhysicalDeviceMemoryProperties,
    queue_family: u32,
}

impl DmaBufCtx {
    pub(crate) fn new(core: &crate::vk::VkCore) -> Option<Self> {
        if !core.dmabuf {
            return None;
        }
        unsafe {
            Some(Self {
                memory_fd: ash::khr::external_memory_fd::Device::new(&core.instance, &core.device),
                memory: core.instance.get_physical_device_memory_properties(core.physical_device),
                queue_family: core.queue_family,
            })
        }
    }
}

/// Which way ownership is moving.
#[derive(Clone, Copy, PartialEq)]
enum Handover {
    Acquire,
    Release,
}

impl ImageStage {
    /// Run the imports, acquires and releases queued since the last frame:
    /// images made first, then every barrier in one submission and one wait.
    pub(crate) fn process_dmabuf(&mut self, device: &ash::Device, queue: vk::Queue, command_pool: vk::CommandPool) {
        let ops = std::mem::take(&mut *OPS.lock().unwrap());
        if ops.is_empty() {
            return;
        }
        let Some(queue_family) = self.dmabuf.as_ref().map(|c| c.queue_family) else {
            // No import here (an older device): nothing was ever imported, and
            // a release has nothing to hand back but the buffer itself.
            let mut released = RELEASED.lock().unwrap();
            for op in ops {
                if let Op::Release(id) = op {
                    released.push(id);
                }
            }
            return;
        };
        let mut barriers: Vec<(vk::Image, Handover)> = Vec::new();
        let mut released = Vec::new();
        for op in ops {
            match op {
                Op::Import { id, buf } => {
                    if let Some(image) = unsafe { self.import(device, id, buf) } {
                        barriers.push((image, Handover::Acquire));
                    }
                }
                Op::Acquire(id) => {
                    if let Some(gpu) = self.images.get(&id).filter(|g| g.imported.is_some()) {
                        barriers.push((gpu.image, Handover::Acquire));
                    }
                }
                Op::Release(id) => {
                    if let Some(gpu) = self.images.get(&id).filter(|g| g.imported.is_some()) {
                        barriers.push((gpu.image, Handover::Release));
                    }
                    released.push(id);
                }
            }
        }
        if !barriers.is_empty() {
            unsafe { record_handovers(device, queue, command_pool, queue_family, &barriers) };
        }
        RELEASED.lock().unwrap().extend(released);
    }

    /// Make the image for one buffer: memory imported from its fd, laid out
    /// as its modifier says, viewed in the format it is sampled as.
    unsafe fn import(&mut self, device: &ash::Device, id: u32, buf: DmaBuf) -> Option<vk::Image> {
        let ctx = self.dmabuf.as_ref()?;
        if self.images.len() >= MAX_IMAGES as usize {
            log::error!("image registry full ({MAX_IMAGES}); dropping dma-buf import {id}");
            return None;
        }
        let Some(format) = vk_format(buf.fourcc) else {
            log::warn!("dma-buf import {id}: fourcc {:#x} is not one this samples", buf.fourcc);
            return None;
        };
        // One plane: what `probe_dmabuf` offers. A modifier with an
        // auxiliary plane would need a disjoint image.
        let [plane] = buf.planes.as_slice() else {
            log::warn!("dma-buf import {id}: {} planes, only single-plane buffers are imported", buf.planes.len());
            return None;
        };
        let layouts = [vk::SubresourceLayout {
            offset: plane.offset as vk::DeviceSize,
            size: 0,
            row_pitch: plane.stride as vk::DeviceSize,
            array_pitch: 0,
            depth_pitch: 0,
        }];
        let mut explicit = vk::ImageDrmFormatModifierExplicitCreateInfoEXT::default()
            .drm_format_modifier(buf.modifier)
            .plane_layouts(&layouts);
        let mut external = vk::ExternalMemoryImageCreateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D { width: buf.width, height: buf.height, depth: 1 })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
            .usage(vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .push_next(&mut external)
            .push_next(&mut explicit);
        let image = match device.create_image(&info, None) {
            Ok(image) => image,
            Err(e) => {
                log::warn!("dma-buf import {id}: {e:?} making a {}x{} image, modifier {:#x}", buf.width, buf.height, buf.modifier);
                return None;
            }
        };
        // The memory: a dup of the fd, which Vulkan owns once the import
        // succeeds. The producer's own fd stays the producer's.
        let fd = match plane.fd.try_clone() {
            Ok(fd) => fd,
            Err(e) => {
                log::warn!("dma-buf import {id}: dup: {e}");
                device.destroy_image(image, None);
                return None;
            }
        };
        let mut fd_props = vk::MemoryFdPropertiesKHR::default();
        let fd_types = ctx
            .memory_fd
            .get_memory_fd_properties(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT, fd.as_raw_fd(), &mut fd_props)
            .map(|()| fd_props.memory_type_bits)
            .unwrap_or(0);
        let reqs = device.get_image_memory_requirements(image);
        let usable = fd_types & reqs.memory_type_bits;
        let Some(memory_type) = (0..ctx.memory.memory_type_count).find(|i| usable & (1 << i) != 0) else {
            log::warn!("dma-buf import {id}: no memory type fits (fd {fd_types:#x}, image {:#x})", reqs.memory_type_bits);
            device.destroy_image(image, None);
            return None;
        };
        let raw_fd = fd.into_raw_fd();
        let mut import = vk::ImportMemoryFdInfoKHR::default()
            .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
            .fd(raw_fd);
        let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(image);
        let alloc = vk::MemoryAllocateInfo::default()
            .allocation_size(reqs.size)
            .memory_type_index(memory_type)
            .push_next(&mut import)
            .push_next(&mut dedicated);
        let memory = match device.allocate_memory(&alloc, None) {
            Ok(m) => m,
            Err(e) => {
                // The fd is still ours when the import failed.
                drop(<OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(raw_fd));
                log::warn!("dma-buf import {id}: {e:?} importing the memory");
                device.destroy_image(image, None);
                return None;
            }
        };
        if let Err(e) = device.bind_image_memory(image, memory, 0) {
            log::warn!("dma-buf import {id}: {e:?} binding the memory");
            device.free_memory(memory, None);
            device.destroy_image(image, None);
            return None;
        }
        // XR24's fourth byte is padding: read it as opaque, or the page is
        // blended by whatever the producer left there.
        let a = if alpha_is_padding(buf.fourcc) { vk::ComponentSwizzle::ONE } else { vk::ComponentSwizzle::IDENTITY };
        let view = match device.create_image_view(
            &vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format)
                .components(vk::ComponentMapping { a, ..Default::default() })
                .subresource_range(color_range()),
            None,
        ) {
            Ok(v) => v,
            Err(e) => {
                log::warn!("dma-buf import {id}: {e:?} making the view");
                device.free_memory(memory, None);
                device.destroy_image(image, None);
                return None;
            }
        };
        let descriptor_set = self.descriptor_for(device, view);
        self.images.insert(
            id,
            GpuImage {
                image,
                view,
                allocation: None,
                imported: Some(memory),
                descriptor_set,
                width: buf.width,
                height: buf.height,
                format: PixelFormat::Bgra,
                mip_levels: 1,
            },
        );
        Some(image)
    }
}

impl ImageStage {
    /// A descriptor set drawing `view` with the stage's sampler, as an
    /// upload's is.
    unsafe fn descriptor_for(&self, device: &ash::Device, view: vk::ImageView) -> vk::DescriptorSet {
        let set_layouts = [self.descriptor_set_layout];
        let set = device
            .allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(self.descriptor_pool)
                    .set_layouts(&set_layouts),
            )
            .expect("Failed to allocate image descriptor set")[0];
        let image_infos = [vk::DescriptorImageInfo::default()
            .image_view(view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let sampler_infos = [vk::DescriptorImageInfo::default().sampler(self.sampler)];
        device.update_descriptor_sets(
            &[
                vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(&image_infos),
                vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .image_info(&sampler_infos),
            ],
            &[],
        );
        set
    }
}

fn color_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    }
}

/// Move the images between the producer's queue and this one, in one
/// submission, and wait for it.
unsafe fn record_handovers(
    device: &ash::Device,
    queue: vk::Queue,
    command_pool: vk::CommandPool,
    queue_family: u32,
    handovers: &[(vk::Image, Handover)],
) {
    let barriers: Vec<vk::ImageMemoryBarrier> = handovers
        .iter()
        .map(|&(image, way)| {
            let b = vk::ImageMemoryBarrier::default().image(image).subresource_range(color_range());
            match way {
                // GENERAL, not UNDEFINED, on the way in: UNDEFINED lets the
                // driver throw the contents away, and the contents are the
                // point.
                Handover::Acquire => b
                    .src_queue_family_index(vk::QUEUE_FAMILY_FOREIGN_EXT)
                    .dst_queue_family_index(queue_family)
                    .old_layout(vk::ImageLayout::GENERAL)
                    .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .src_access_mask(vk::AccessFlags::empty())
                    .dst_access_mask(vk::AccessFlags::SHADER_READ),
                Handover::Release => b
                    .src_queue_family_index(queue_family)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_FOREIGN_EXT)
                    .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .new_layout(vk::ImageLayout::GENERAL)
                    .src_access_mask(vk::AccessFlags::SHADER_READ)
                    .dst_access_mask(vk::AccessFlags::empty()),
            }
        })
        .collect();
    let cmd = device
        .allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default()
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1),
        )
        .expect("Failed to allocate dma-buf command buffer")[0];
    device
        .begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT))
        .unwrap();
    device.cmd_pipeline_barrier(
        cmd,
        vk::PipelineStageFlags::TOP_OF_PIPE | vk::PipelineStageFlags::FRAGMENT_SHADER,
        vk::PipelineStageFlags::FRAGMENT_SHADER | vk::PipelineStageFlags::BOTTOM_OF_PIPE,
        vk::DependencyFlags::empty(),
        &[],
        &[],
        &barriers,
    );
    device.end_command_buffer(cmd).unwrap();
    let cmds = [cmd];
    device
        .queue_submit(queue, &[vk::SubmitInfo::default().command_buffers(&cmds)], vk::Fence::null())
        .expect("dma-buf handover submit failed");
    let _ = device.queue_wait_idle(queue);
    device.free_command_buffers(command_pool, &cmds);
}
