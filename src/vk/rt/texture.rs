//! The tracer's own textures (an environment, a material's image) and images staged for upload.

use super::*;

/// A texture the stage made and owns: the 1x1 stand-in bound while the
/// scene has no image, and the headless tracer's image.
pub(super) struct OwnedTexture {
    pub(super) image: vk::Image,
    pub(super) view: vk::ImageView,
    pub(super) allocation: Option<Allocation>,
    pub(super) size: (u32, u32),
}

impl OwnedTexture {
    /// Upload sRGB RGBA8 pixels as a one-level texture, with a blocking
    /// one-time submit. One level: the headless tracer renders a still, and
    /// its samples average what a mip chain would have.
    pub(super) fn new(
        device: &ash::Device,
        allocator: &mut Allocator,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        pixels: &[u8],
        width: u32,
        height: u32,
    ) -> Self {
        assert_eq!(pixels.len(), (width * height * 4) as usize, "8888 size mismatch");
        let range = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .level_count(1)
            .layer_count(1);
        unsafe {
            let image = device
                .create_image(
                    &vk::ImageCreateInfo::default()
                        .image_type(vk::ImageType::TYPE_2D)
                        .format(vk::Format::R8G8B8A8_SRGB)
                        .extent(vk::Extent3D { width, height, depth: 1 })
                        .mip_levels(1)
                        .array_layers(1)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .tiling(vk::ImageTiling::OPTIMAL)
                        .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST)
                        .initial_layout(vk::ImageLayout::UNDEFINED),
                    None,
                )
                .expect("Failed to create RT texture");
            let requirements = device.get_image_memory_requirements(image);
            let allocation = allocator
                .allocate(&AllocationCreateDesc {
                    name: "rt-texture",
                    requirements,
                    location: MemoryLocation::GpuOnly,
                    linear: false,
                    allocation_scheme: AllocationScheme::GpuAllocatorManaged,
                })
                .expect("Failed to allocate RT texture memory");
            device
                .bind_image_memory(image, allocation.memory(), allocation.offset())
                .expect("Failed to bind RT texture memory");
            let mut staging = create_cpu_buffer(
                device,
                allocator,
                pixels.len() as vk::DeviceSize,
                vk::BufferUsageFlags::TRANSFER_SRC,
                "rt-texture-staging",
            );
            staging.allocation.as_mut().unwrap().mapped_slice_mut().unwrap()[..pixels.len()]
                .copy_from_slice(pixels);

            let cmd = device
                .allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(command_pool)
                        .level(vk::CommandBufferLevel::PRIMARY)
                        .command_buffer_count(1),
                )
                .expect("Failed to allocate RT texture command buffer")[0];
            device
                .begin_command_buffer(
                    cmd,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .unwrap();
            let barrier = |from_access, to_access, from_layout, to_layout, from_stage, to_stage| {
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
            barrier(
                vk::AccessFlags::empty(),
                vk::AccessFlags::TRANSFER_WRITE,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
            );
            device.cmd_copy_buffer_to_image(
                cmd,
                staging.buffer,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[vk::BufferImageCopy::default()
                    .buffer_row_length(width)
                    .buffer_image_height(height)
                    .image_subresource(
                        vk::ImageSubresourceLayers::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .layer_count(1),
                    )
                    .image_extent(vk::Extent3D { width, height, depth: 1 })],
            );
            barrier(
                vk::AccessFlags::TRANSFER_WRITE,
                vk::AccessFlags::SHADER_READ,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
            );
            device.end_command_buffer(cmd).unwrap();
            let cmds = [cmd];
            device
                .queue_submit(
                    queue,
                    &[vk::SubmitInfo::default().command_buffers(&cmds)],
                    vk::Fence::null(),
                )
                .expect("RT texture upload submit failed");
            device.queue_wait_idle(queue).expect("RT texture upload wait failed");
            device.free_command_buffers(command_pool, &cmds);
            destroy_cpu_buffer(device, allocator, &mut staging);

            let view = device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(vk::Format::R8G8B8A8_SRGB)
                        .subresource_range(range),
                    None,
                )
                .expect("Failed to create RT texture view");
            OwnedTexture { image, view, allocation: Some(allocation), size: (width, height) }
        }
    }

    /// Caller must have the device idle.
    pub(super) fn destroy(&mut self, device: &ash::Device, allocator: &mut Allocator) {
        unsafe {
            device.destroy_image_view(self.view, None);
            device.destroy_image(self.image, None);
        }
        if let Some(a) = self.allocation.take() {
            let _ = allocator.free(a);
        }
    }
}

/// The scene's image as the stage holds it.
pub(super) struct StagedImage {
    /// The 2D pass's image of this id, or None for one the stage owns.
    pub(super) shared: Option<u32>,
    pub(super) owned: Option<OwnedTexture>,
    pub(super) corners: [[f32; 3]; 4],
    pub(super) opacity: f32,
}
