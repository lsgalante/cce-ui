//! The passes beneath the UI: the offscreen 3D scene and the path tracer into the backdrop, and
//! the backdrop's copy into the swapchain image.

use super::*;

impl VkRenderer {
    /// The passes beneath the UI: the offscreen 3D scene and the path tracer, then — with a
    /// valid backdrop — its copy into the swapchain image, so the UI pass loads instead of
    /// clearing. Returns whether the frame has a backdrop.
    pub(super) unsafe fn record_backdrop(&mut self, cmd: vk::CommandBuffer, frame_index: usize, image_index: u32) -> bool {
        // Offscreen 3D pass (only when a scene was staged); leaves the
        // backdrop in TRANSFER_SRC.
        let mut scene_recorded =
            self.scene.record(&self.core.device, cmd, frame_index, &self.image);

        // Path-tracer pass (only when staged via `stage_rt`): one
        // accumulation dispatch, blitted into the backdrop's pane region —
        // it fills the same slot as the raster scene pass and leaves the
        // backdrop in TRANSFER_SRC likewise.
        if let Some(rt) = self.rt.as_mut() {
            let rt_recorded = rt.record(
                &self.core.device,
                cmd,
                frame_index,
                self.scene.backdrop_image,
                self.extent,
                scene_recorded,
            );
            if rt_recorded {
                self.scene.backdrop_valid = true;
                scene_recorded = true;
            }
        }

        // With a valid backdrop, replay it under the UI: copy it into the
        // swapchain image and open the UI pass with LOAD instead of CLEAR.
        let use_backdrop = self.scene.backdrop_valid;
        if use_backdrop {
            if !scene_recorded {
                // Reused backdrop is in SHADER_READ_ONLY from last frame.
                self.core.device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[vk::ImageMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::SHADER_READ)
                        .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                        .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .image(self.scene.backdrop_image)
                        .subresource_range(COLOR_RANGE)],
                );
            }
            let swapchain_image = self.swapchain_images[image_index as usize];
            self.core.device.cmd_pipeline_barrier(
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
                    .image(swapchain_image)
                    .subresource_range(COLOR_RANGE)],
            );
            let subresource = vk::ImageSubresourceLayers::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .layer_count(1);
            self.core.device.cmd_copy_image(
                cmd,
                self.scene.backdrop_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                swapchain_image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[vk::ImageCopy::default()
                    .src_subresource(subresource)
                    .dst_subresource(subresource)
                    .extent(vk::Extent3D {
                        width: self.extent.width,
                        height: self.extent.height,
                        depth: 1,
                    })],
            );
            // Backdrop back to sampleable for the UI pass's blur plates.
            self.core.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_READ)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ)
                    .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(self.scene.backdrop_image)
                    .subresource_range(COLOR_RANGE)],
            );
        }
        use_backdrop
    }
}
