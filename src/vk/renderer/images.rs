//! Image helpers: subresource ranges, the blur snapshot's mip levels, clears to shader-read,
//! the flipped viewport.

use super::*;

pub(super) const COLOR_RANGE: vk::ImageSubresourceRange = color_levels(0, 1);

/// `count` mip levels of a colour image from `base`.
pub(super) const fn color_levels(base: u32, count: u32) -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: base,
        level_count: count,
        base_array_layer: 0,
        layer_count: 1,
    }
}

/// How many mip levels the blur snapshot carries: enough that the coarsest
/// texel is as wide as the widest kernel stride a plate asks for (a radius of
/// 16 px at scale 4 is a 64 px stride, level 6), and no more — the levels
/// past that would be built every snapshot and never read. See
/// `snapshot_mip_chain`.
pub(crate) const SNAPSHOT_LEVELS_MAX: u32 = 7;

/// The levels a snapshot of `extent` can have, at most [`SNAPSHOT_LEVELS_MAX`]:
/// a level stops halving at one texel.
pub(crate) fn snapshot_levels(extent: vk::Extent2D) -> u32 {
    let side = extent.width.max(extent.height).max(1);
    (32 - side.leading_zeros()).min(SNAPSHOT_LEVELS_MAX)
}

/// One-time submit: clear a color image and leave it in SHADER_READ_ONLY, so a
/// freshly created backdrop is always legal to sample.
pub(crate) fn clear_image_to_shader_read(
    device: &ash::Device,
    queue: vk::Queue,
    command_pool: vk::CommandPool,
    image: vk::Image,
) {
    clear_image_levels_to_shader_read(device, queue, command_pool, image, 1);
}

/// [`clear_image_to_shader_read`] over the first `levels` mip levels.
pub(crate) fn clear_image_levels_to_shader_read(
    device: &ash::Device,
    queue: vk::Queue,
    command_pool: vk::CommandPool,
    image: vk::Image,
    levels: u32,
) {
    let range = color_levels(0, levels);
    unsafe {
        let cmd = device
            .allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(command_pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )
            .expect("Failed to allocate init command buffer")[0];
        device
            .begin_command_buffer(
                cmd,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )
            .unwrap();
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(range)],
        );
        device.cmd_clear_color_image(
            cmd,
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &vk::ClearColorValue { float32: [0.0, 0.0, 0.0, 0.0] },
            &[range],
        );
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(range)],
        );
        device.end_command_buffer(cmd).unwrap();
        let cmds = [cmd];
        let submit = vk::SubmitInfo::default().command_buffers(&cmds);
        device
            .queue_submit(queue, &[submit], vk::Fence::null())
            .expect("Init submit failed");
        device.queue_wait_idle(queue).expect("Init wait failed");
        device.free_command_buffers(command_pool, &cmds);
    }
}

/// The wgpu-convention viewport: Y flipped via negative height (Vulkan >= 1.1).
pub(crate) fn flipped_viewport(extent: vk::Extent2D) -> vk::Viewport {
    vk::Viewport {
        x: 0.0,
        y: extent.height as f32,
        width: extent.width as f32,
        height: -(extent.height as f32),
        min_depth: 0.0,
        max_depth: 1.0,
    }
}
