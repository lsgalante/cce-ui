//! Replacing an image's pixels in place — the streaming path: same id, same image, same descriptor —
//! whole or by regions.

use super::*;

impl ImageStage {
    /// Replace an existing image's contents in place.
    ///
    /// The whole streaming path. Against `upload` it skips creating an image,
    /// allocating its memory, allocating and writing a descriptor set, and —
    /// the expensive one — destroying last frame's image, which needs the
    /// device idle and so waits for every frame still in flight.
    ///
    /// The copy is still its own submission followed by `queue_wait_idle`,
    /// and that wait is doing real work: the image is one the *previous*
    /// frame may still be sampling, and two submissions on one queue are not
    /// ordered against each other by anything weaker. Lifting it means giving
    /// each streaming image a second buffer to alternate between and
    /// recording the copy into the frame's own command buffer, which is the
    /// next step rather than this one.
    pub(super) fn write_into(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        id: u32,
        pixels: &[u8],
    ) {
        let Some(&GpuImage { width, height, .. }) = self.images.get(&id) else {
            return;
        };
        self.write_regions(device, allocator, queue, command_pool, id, pixels, &[(0, 0, width, height)]);
    }

    /// [`Self::write_into`] for part of the image: each region's texels,
    /// packed one after another in `pixels`, copied into place in one
    /// submission. Everything outside the regions keeps what it held.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn write_regions(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        id: u32,
        pixels: &[u8],
        regions: &[Region],
    ) {
        let Some(&GpuImage { image, width, height, mip_levels, .. }) = self.images.get(&id) else {
            return;
        };
        let mut offset = 0u64;
        let copies: Vec<vk::BufferImageCopy> = regions
            .iter()
            .map(|&(x, y, w, h)| {
                let copy = vk::BufferImageCopy::default()
                    .buffer_offset(offset)
                    .buffer_row_length(w)
                    .buffer_image_height(h)
                    .image_subresource(
                        vk::ImageSubresourceLayers::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .layer_count(1),
                    )
                    .image_offset(vk::Offset3D { x: x as i32, y: y as i32, z: 0 })
                    .image_extent(vk::Extent3D { width: w, height: h, depth: 1 });
                offset += (w * h * 4) as u64;
                copy
            })
            .collect();
        unsafe {
            let staging_buffer = {
                let staging = self.staging_for(device, allocator, pixels.len());
                staging.allocation.as_mut().unwrap().mapped_slice_mut().unwrap()[..pixels.len()]
                    .copy_from_slice(pixels);
                staging.buffer
            };
            let range = vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .level_count(mip_levels)
                .layer_count(1);
            let cmd = device
                .allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(command_pool)
                        .level(vk::CommandBufferLevel::PRIMARY)
                        .command_buffer_count(1),
                )
                .expect("Failed to allocate update command buffer")[0];
            device
                .begin_command_buffer(
                    cmd,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .unwrap();
            // Unlike a fresh upload this image holds a picture already, and
            // it is in the layout the shader reads. Transitioning *from* that
            // layout (not UNDEFINED) keeps the contents, which a region
            // update depends on: everything outside its regions must survive.
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::SHADER_READ)
                    .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(image)
                    .subresource_range(range)],
            );
            device.cmd_copy_buffer_to_image(
                cmd,
                staging_buffer,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &copies,
            );
            record_levels(device, cmd, image, width, height, mip_levels);
            device.end_command_buffer(cmd).unwrap();
            let cmds = [cmd];
            device
                .queue_submit(
                    queue,
                    &[vk::SubmitInfo::default().command_buffers(&cmds)],
                    vk::Fence::null(),
                )
                .expect("Image update submit failed");
            device.queue_wait_idle(queue).expect("Image update wait failed");
            device.free_command_buffers(command_pool, &cmds);
        }
    }
}
