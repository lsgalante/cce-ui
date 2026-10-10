//! Uploading: the shared staging buffer (grown as an upload needs, released once idle), a one-shot
//! upload and a batched run of them, and recording the copy into the image and its mip chain.

use super::*;

impl ImageStage {
    /// The shared staging buffer, grown if this upload needs more room.
    pub(super) fn staging_for(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        bytes: usize,
    ) -> &mut AllocatedBuffer {
        let too_small = self
            .staging
            .as_ref()
            .is_none_or(|b| (b.size as usize) < bytes);
        if too_small {
            if let Some(mut old) = self.staging.take() {
                // No wait: every copy submitted from here is followed by
                // `queue_wait_idle` before its caller returns, so no GPU work
                // is referencing the old buffer by the time anything asks for
                // a bigger one.
                destroy_cpu_buffer(device, allocator, &mut old);
            }
            self.staging = Some(create_cpu_buffer(
                device,
                allocator,
                bytes as vk::DeviceSize,
                vk::BufferUsageFlags::TRANSFER_SRC,
                "image-staging",
            ));
        }
        self.staging.as_mut().expect("staging buffer")
    }

    #[allow(clippy::too_many_arguments)]
    /// Upload RGBA8 pixels into THIS table now, rather than queueing them for
    /// whichever renderer drains the shared queue next — for a renderer with
    /// images of its own (see [`Self::shared_uploads`]). The id comes from
    /// the process-wide counter, so it never collides with a queued one.
    pub(crate) fn upload_now(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        pixels: &[u8],
        width: u32,
        height: u32,
    ) -> u32 {
        let id = crate::draw::images::next_image_id();
        self.upload(device, allocator, queue, command_pool, id, pixels, width, height, PixelFormat::Rgba, false);
        id
    }

    pub(super) fn upload(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        id: u32,
        pixels: &[u8],
        width: u32,
        height: u32,
        format: PixelFormat,
        mips: bool,
    ) {
        let one = [RunItem { id, pixels, width, height, format, mips }];
        self.upload_run(device, allocator, queue, command_pool, &one);
    }

    /// Upload a run of images in one submission: every image's pixels
    /// staged side by side in the shared staging buffer, every copy (and mip
    /// chain) recorded into one command buffer, one submit, one wait. Until
    /// 2026-10-06 each image was its own submit followed by a queue wait:
    /// 200 thumbnails took ~70 ms of round trips in one frame.
    pub(super) fn upload_run(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        run: &[RunItem<'_>],
    ) {
        // Room in the registry, in order; what does not fit is dropped as before.
        let room = (MAX_IMAGES as usize).saturating_sub(self.images.len());
        for item in run.iter().skip(room) {
            log::error!("image registry full ({MAX_IMAGES}); dropping upload {}", item.id);
        }
        let run = &run[..run.len().min(room)];
        if run.is_empty() {
            return;
        }
        // Each image's pixels at its own offset, 16-byte aligned (a copy's
        // buffer offset must be a multiple of the texel size).
        let mut offsets = Vec::with_capacity(run.len());
        let mut total = 0usize;
        for item in run {
            offsets.push(total);
            total += item.pixels.len().next_multiple_of(16);
        }
        let staging_buffer = {
            let staging = self.staging_for(device, allocator, total);
            let mapped = staging.allocation.as_mut().unwrap().mapped_slice_mut().unwrap();
            for (item, &at) in run.iter().zip(&offsets) {
                mapped[at..at + item.pixels.len()].copy_from_slice(item.pixels);
            }
            staging.buffer
        };
        unsafe {
            let cmd = device
                .allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(command_pool)
                        .level(vk::CommandBufferLevel::PRIMARY)
                        .command_buffer_count(1),
                )
                .expect("Failed to allocate upload command buffer")[0];
            device
                .begin_command_buffer(
                    cmd,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .unwrap();
            for (item, &at) in run.iter().zip(&offsets) {
                self.record_upload(
                    device, allocator, cmd, staging_buffer, at as vk::DeviceSize, item.id, item.width,
                    item.height, item.format, item.mips,
                );
            }
            device.end_command_buffer(cmd).unwrap();
            let cmds = [cmd];
            device
                .queue_submit(queue, &[vk::SubmitInfo::default().command_buffers(&cmds)], vk::Fence::null())
                .expect("Image upload submit failed");
            device.queue_wait_idle(queue).expect("Image upload wait failed");
            device.free_command_buffers(command_pool, &cmds);
        }
    }

    /// Create one image and record its upload from `staging_buffer` at
    /// `offset` into `cmd` (`upload_run` submits it). The view and the
    /// descriptor set are made here too; nothing reads them before the run's
    /// wait returns.
    #[allow(clippy::too_many_arguments)]
    pub(super) unsafe fn record_upload(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        cmd: vk::CommandBuffer,
        staging_buffer: vk::Buffer,
        offset: vk::DeviceSize,
        id: u32,
        width: u32,
        height: u32,
        format: PixelFormat,
        mips: bool,
    ) {
        let mip_levels =
            if mips && self.mips_supported { mip_level_count(width, height) } else { 1 };
        // Each level is blitted from the one above it, so a mipmapped image
        // is a transfer's source as well as its destination.
        let usage = if mip_levels > 1 {
            vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_DST
                | vk::ImageUsageFlags::TRANSFER_SRC
        } else {
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST
        };
        unsafe {
            let image = device
                .create_image(
                    &vk::ImageCreateInfo::default()
                        .image_type(vk::ImageType::TYPE_2D)
                        // SRGB, not UNORM: uploaded pixels are sRGB-encoded
                        // (rasterized SVGs, decoded PNGs, Servo page readback),
                        // and the swapchain is an sRGB format, so the hardware
                        // encodes shader output on write. Sampling as UNORM
                        // fed those bytes through as if linear and encoded
                        // them a second time, lightening every midtone —
                        // a page's #101010 measured (71,71,71) on screen.
                        // Decoding on sample makes the round trip exact.
                        //
                        // Which channel comes first is the caller's business
                        // (`PixelFormat`): the sampler reads either order at
                        // no cost, so a BGRA source never needs a CPU swizzle.
                        .format(format.vk())
                        .extent(vk::Extent3D { width, height, depth: 1 })
                        .mip_levels(mip_levels)
                        .array_layers(1)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .tiling(vk::ImageTiling::OPTIMAL)
                        .usage(usage)
                        .initial_layout(vk::ImageLayout::UNDEFINED),
                    None,
                )
                .expect("Failed to create user image");
            let requirements = device.get_image_memory_requirements(image);
            let allocation = allocator
                .allocate(&AllocationCreateDesc {
                    name: "user-image",
                    requirements,
                    location: MemoryLocation::GpuOnly,
                    linear: false,
                    allocation_scheme: AllocationScheme::GpuAllocatorManaged,
                })
                .expect("Failed to allocate user image memory");
            device
                .bind_image_memory(image, allocation.memory(), allocation.offset())
                .expect("Failed to bind user image memory");

            let range = vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .level_count(mip_levels)
                .layer_count(1);
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
            device.cmd_copy_buffer_to_image(
                cmd,
                staging_buffer,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[vk::BufferImageCopy::default()
                    .buffer_offset(offset)
                    .buffer_row_length(width)
                    .buffer_image_height(height)
                    .image_subresource(
                        vk::ImageSubresourceLayers::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .layer_count(1),
                    )
                    .image_extent(vk::Extent3D { width, height, depth: 1 })],
            );
            record_levels(device, cmd, image, width, height, mip_levels);

            let view = device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(format.vk())
                        .subresource_range(range),
                    None,
                )
                .expect("Failed to create user image view");

            let set_layouts = [self.descriptor_set_layout];
            let descriptor_set = device
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
                        .dst_set(descriptor_set)
                        .dst_binding(0)
                        .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                        .image_info(&image_infos),
                    vk::WriteDescriptorSet::default()
                        .dst_set(descriptor_set)
                        .dst_binding(1)
                        .descriptor_type(vk::DescriptorType::SAMPLER)
                        .image_info(&sampler_infos),
                ],
                &[],
            );

            self.images.insert(
                id,
                GpuImage {
                    image,
                    view,
                    allocation: Some(allocation),
                    descriptor_set,
                    width,
                    height,
                    format,
                    mip_levels,
                },
            );
        }
    }
}

/// Finish an image whose level 0 has just been copied into and whose every
/// level is in TRANSFER_DST: build each further level from the one above
/// it, and leave them all in the layout the shader reads.
///
/// A blit, filtered linearly, halves a level into the next: each texel of
/// the smaller is the mean of the four it covers. The formats are sRGB, so
/// the mean is taken of the light and not of its encoding — a level of a
/// black and white check is the grey that looks half as bright, where a mean
/// of the bytes is darker than that.
///
/// With one level there is nothing to build and this is the transition the
/// plain upload always ended on.
pub(super) unsafe fn record_levels(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    width: u32,
    height: u32,
    mip_levels: u32,
) {
    let level = |i: u32| {
        vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .base_mip_level(i)
            .level_count(1)
            .layer_count(1)
    };
    let barrier = |range, from_access, to_access, from_layout, to_layout, from_stage, to_stage| {
        device.cmd_pipeline_barrier(
            cmd,
            from_stage,
            to_stage,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[vk::ImageMemoryBarrier::default()
                .src_access_mask(from_access)
                .dst_access_mask(to_access)
                .old_layout(from_layout)
                .new_layout(to_layout)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(range)],
        );
    };
    let (mut w, mut h) = (width as i32, height as i32);
    for i in 1..mip_levels {
        let (next_w, next_h) = ((w / 2).max(1), (h / 2).max(1));
        barrier(
            level(i - 1),
            vk::AccessFlags::TRANSFER_WRITE,
            vk::AccessFlags::TRANSFER_READ,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::TRANSFER,
        );
        let layers = |i: u32| {
            vk::ImageSubresourceLayers::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .mip_level(i)
                .layer_count(1)
        };
        device.cmd_blit_image(
            cmd,
            image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[vk::ImageBlit::default()
                .src_subresource(layers(i - 1))
                .src_offsets([vk::Offset3D::default(), vk::Offset3D { x: w, y: h, z: 1 }])
                .dst_subresource(layers(i))
                .dst_offsets([
                    vk::Offset3D::default(),
                    vk::Offset3D { x: next_w, y: next_h, z: 1 },
                ])],
            vk::Filter::LINEAR,
        );
        barrier(
            level(i - 1),
            vk::AccessFlags::TRANSFER_READ,
            vk::AccessFlags::SHADER_READ,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
        );
        (w, h) = (next_w, next_h);
    }
    barrier(
        level(mip_levels - 1),
        vk::AccessFlags::TRANSFER_WRITE,
        vk::AccessFlags::SHADER_READ,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        vk::PipelineStageFlags::TRANSFER,
        vk::PipelineStageFlags::FRAGMENT_SHADER,
    );
}
