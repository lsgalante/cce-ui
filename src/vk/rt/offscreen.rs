//! `RtOffscreen`: the tracer without a window — its own device, queue and target — for the GPU
//! tests and headless renders.

use super::*;

// --- Headless offscreen rendering (thumbnails, previews) ---

/// One-shot path-traced rendering with no window anywhere: a headless
/// [`crate::vk::VkCore`] + an [`RtStage`] whose "backdrop" is a private sRGB
/// target image, read back to CPU pixels. This is the seam consumers like
/// the cce-files thumbnailer sit on.
///
/// Not `Send`-safe by design intent (owns a device); create it on the worker
/// thread that renders.
pub struct RtOffscreen {
    pub(super) stage: RtStage,
    pub(super) target_image: vk::Image,
    pub(super) target_allocation: Option<Allocation>,
    pub(super) readback: AllocatedBuffer,
    pub(super) size: (u32, u32),
    pub(super) cmd: vk::CommandBuffer,
    pub(super) fence: vk::Fence,
    // Declared last: dropped after everything above is destroyed in Drop.
    pub(super) core: crate::vk::VkCore,
}

impl RtOffscreen {
    /// Samples per submit: keeps each dispatch well under GPU watchdog
    /// timeouts even at large sizes; a render loops submits to reach the
    /// requested sample count.
    const CHUNK_SPP: u32 = 8;

    pub fn new() -> Self {
        let mut core = crate::vk::VkCore::new_headless();
        let device = core.device.clone();
        let accel_loader = core.accel_loader.clone();
        let as_scratch_align = core.as_scratch_align;
        let min_uniform_align = core.min_uniform_align;
        let (queue, command_pool) = (core.queue, core.command_pool);
        let allocator = core.allocator.as_mut().unwrap();
        let stage = RtStage::new(
            &device,
            allocator,
            1,
            accel_loader.as_ref(),
            as_scratch_align,
            min_uniform_align,
            queue,
            command_pool,
        );
        unsafe {
            let cmd = device
                .allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(core.command_pool)
                        .level(vk::CommandBufferLevel::PRIMARY)
                        .command_buffer_count(1),
                )
                .expect("Failed to allocate RT offscreen command buffer")[0];
            let fence = device
                .create_fence(&vk::FenceCreateInfo::default(), None)
                .expect("Failed to create RT offscreen fence");
            RtOffscreen {
                stage,
                target_image: vk::Image::null(),
                target_allocation: None,
                readback: AllocatedBuffer::null(),
                size: (0, 0),
                cmd,
                fence,
                core,
            }
        }
    }

    /// What a camera ray that meets nothing shows — see
    /// `VkRenderer::set_rt_background`.
    pub fn set_background(&mut self, background: Option<[f32; 3]>) {
        self.stage.set_background(background);
    }

    /// The sky and sun — see [`RtEnvironment`].
    pub fn set_environment(&mut self, environment: RtEnvironment) {
        self.stage.set_environment(environment);
    }

    /// Replace the scene (same schema as `VkRenderer::set_rt_scene`).
    pub fn set_scene(&mut self, triangles: &[RtTriangle], materials: &[RtMaterial]) {
        self.set_scene_with_image(triangles, materials, None);
    }

    /// [`set_scene`](Self::set_scene), with an image standing in the scene.
    pub fn set_scene_with_image(
        &mut self,
        triangles: &[RtTriangle],
        materials: &[RtMaterial],
        image: Option<RtImagePixels>,
    ) {
        let packed = pack_scene(
            triangles.to_vec(),
            materials,
            image.as_ref().map(|i| i.corners),
            self.stage.needs_bvh(),
        );
        unsafe {
            let _ = self.core.device.device_wait_idle();
        }
        let device = self.core.device.clone();
        let queue = self.core.queue;
        let command_pool = self.core.command_pool;
        self.stage.set_scene(
            &device,
            self.core.allocator.as_mut().unwrap(),
            queue,
            command_pool,
            &packed,
            image.map(|i| {
                (
                    RtImageSource::Pixels { pixels: i.pixels, width: i.width, height: i.height },
                    i.corners,
                    i.opacity,
                )
            }),
        );
    }

    /// Render `samples` paths per pixel and return tightly packed
    /// sRGB-encoded RGBA8 pixels (`width * height * 4` bytes). Blocks until
    /// the GPU finishes; meant for worker threads, not frame loops.
    pub fn render(
        &mut self,
        camera: RtCamera,
        width: u32,
        height: u32,
        samples: u32,
    ) -> Vec<u8> {
        let width = width.max(1);
        let height = height.max(1);
        let samples = samples.clamp(1, MAX_SAMPLES);
        let device = self.core.device.clone();
        self.ensure_target(width, height);

        // Fresh accumulation every render: thumbnails are one-shot.
        self.stage.sample_index = 0;
        let extent = vk::Extent2D { width, height };
        let mut done = 0u32;
        while done < samples {
            self.stage.spp = Self::CHUNK_SPP.min(samples - done);
            self.stage.stage(
                &device,
                self.core.allocator.as_mut().unwrap(),
                (0, 0, width, height),
                camera,
            );
            // stage() resets sample_index when the camera or size changed —
            // keep our resume point, not the reset, after the first chunk.
            self.stage.sample_index = done;
            self.stage.write_frame_uniforms(&device, 0, &|_| None);
            unsafe {
                device
                    .begin_command_buffer(self.cmd, &vk::CommandBufferBeginInfo::default())
                    .unwrap();
                let recorded =
                    self.stage.record(&device, self.cmd, 0, self.target_image, extent, true);
                device.end_command_buffer(self.cmd).unwrap();
                assert!(recorded, "RT offscreen: nothing recorded (empty scene?)");
                self.submit_and_wait();
            }
            done += Self::CHUNK_SPP.min(samples - done);
        }

        // Copy the sRGB target (left in TRANSFER_SRC by record) to the
        // readback buffer and map it.
        unsafe {
            device
                .begin_command_buffer(self.cmd, &vk::CommandBufferBeginInfo::default())
                .unwrap();
            device.cmd_copy_image_to_buffer(
                self.cmd,
                self.target_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                self.readback.buffer,
                &[vk::BufferImageCopy::default()
                    .image_subresource(
                        vk::ImageSubresourceLayers::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .layer_count(1),
                    )
                    .image_extent(vk::Extent3D { width, height, depth: 1 })],
            );
            device.cmd_pipeline_barrier(
                self.cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::HOST,
                vk::DependencyFlags::empty(),
                &[],
                &[vk::BufferMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::HOST_READ)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .buffer(self.readback.buffer)
                    .size(vk::WHOLE_SIZE)],
                &[],
            );
            device.end_command_buffer(self.cmd).unwrap();
            self.submit_and_wait();
        }
        let len = (width * height * 4) as usize;
        self.readback.allocation.as_ref().unwrap().mapped_slice().unwrap()[..len].to_vec()
    }

    pub(super) unsafe fn submit_and_wait(&mut self) {
        let device = &self.core.device;
        let cmds = [self.cmd];
        device
            .queue_submit(
                self.core.queue,
                &[vk::SubmitInfo::default().command_buffers(&cmds)],
                self.fence,
            )
            .expect("RT offscreen submit failed");
        device
            .wait_for_fences(&[self.fence], true, u64::MAX)
            .expect("RT offscreen fence wait failed");
        device.reset_fences(&[self.fence]).unwrap();
    }

    pub(super) fn ensure_target(&mut self, width: u32, height: u32) {
        if (width, height) == self.size {
            return;
        }
        let device = self.core.device.clone();
        unsafe {
            let _ = device.device_wait_idle();
        }
        self.destroy_target();
        let allocator = self.core.allocator.as_mut().unwrap();
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
                        .usage(
                            vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::TRANSFER_SRC,
                        )
                        .initial_layout(vk::ImageLayout::UNDEFINED),
                    None,
                )
                .expect("Failed to create RT offscreen target");
            let requirements = device.get_image_memory_requirements(image);
            let allocation = allocator
                .allocate(&AllocationCreateDesc {
                    name: "rt-offscreen-target",
                    requirements,
                    location: MemoryLocation::GpuOnly,
                    linear: false,
                    allocation_scheme: AllocationScheme::GpuAllocatorManaged,
                })
                .expect("Failed to allocate RT offscreen target memory");
            device
                .bind_image_memory(image, allocation.memory(), allocation.offset())
                .expect("Failed to bind RT offscreen target memory");
            self.target_image = image;
            self.target_allocation = Some(allocation);

            self.readback = create_cpu_buffer(
                &device,
                allocator,
                (width as vk::DeviceSize) * (height as vk::DeviceSize) * 4,
                vk::BufferUsageFlags::TRANSFER_DST,
                "rt-readback",
            );

            // RtStage::record expects the blit destination in TRANSFER_SRC
            // (the steady state SceneStage leaves the backdrop in).
            device
                .begin_command_buffer(self.cmd, &vk::CommandBufferBeginInfo::default())
                .unwrap();
            device.cmd_pipeline_barrier(
                self.cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::empty())
                    .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                    .old_layout(vk::ImageLayout::UNDEFINED)
                    .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(image)
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .level_count(1)
                            .layer_count(1),
                    )],
            );
            device.end_command_buffer(self.cmd).unwrap();
            self.submit_and_wait();
        }
        self.size = (width, height);
    }

    pub(super) fn destroy_target(&mut self) {
        unsafe {
            if self.target_image != vk::Image::null() {
                self.core.device.destroy_image(self.target_image, None);
                self.target_image = vk::Image::null();
            }
        }
        if let Some(a) = self.target_allocation.take() {
            let _ = self.core.allocator.as_mut().unwrap().free(a);
        }
        let device = self.core.device.clone();
        destroy_cpu_buffer(&device, self.core.allocator.as_mut().unwrap(), &mut self.readback);
        self.size = (0, 0);
    }
}

impl Default for RtOffscreen {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for RtOffscreen {
    fn drop(&mut self) {
        unsafe {
            let _ = self.core.device.device_wait_idle();
        }
        self.destroy_target();
        let device = self.core.device.clone();
        self.stage.destroy(&device, self.core.allocator.as_mut().unwrap());
        unsafe {
            self.core.device.destroy_fence(self.fence, None);
            // The command buffer dies with the pool in VkCore's Drop.
        }
    }
}
