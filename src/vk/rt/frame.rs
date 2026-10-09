//! A frame of the tracer: its pane-sized targets, per-frame uniforms, and the recorded
//! accumulation dispatch, denoise and blit into the backdrop.

use super::*;

impl RtStage {

    pub(super) fn recreate_targets(&mut self, device: &ash::Device, allocator: &mut Allocator, w: u32, h: u32) {
        self.destroy_targets(device, allocator);
        self.output_size = (w, h);
        self.output_initialized = false;
        let gpu_buffer = |allocator: &mut Allocator,
                          bytes_per_px: vk::DeviceSize,
                          name: &'static str|
         -> AllocatedBuffer {
            let size = (w as vk::DeviceSize) * (h as vk::DeviceSize) * bytes_per_px;
            unsafe {
                let buffer = device
                    .create_buffer(
                        &vk::BufferCreateInfo::default()
                            .size(size)
                            .usage(vk::BufferUsageFlags::STORAGE_BUFFER)
                            .sharing_mode(vk::SharingMode::EXCLUSIVE),
                        None,
                    )
                    .expect("Failed to create RT target buffer");
                let requirements = device.get_buffer_memory_requirements(buffer);
                let allocation = allocator
                    .allocate(&AllocationCreateDesc {
                        name,
                        requirements,
                        location: MemoryLocation::GpuOnly,
                        linear: true,
                        allocation_scheme: AllocationScheme::GpuAllocatorManaged,
                    })
                    .expect("Failed to allocate RT target memory");
                device
                    .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
                    .expect("Failed to bind RT target memory");
                AllocatedBuffer { buffer, allocation: Some(allocation), size }
            }
        };
        self.accum = gpu_buffer(allocator, 16, "rt-accum");
        self.features = gpu_buffer(allocator, 32, "rt-features");
        if self.denoiser.is_some() {
            self.ping = gpu_buffer(allocator, 16, "rt-denoise-ping");
            self.pong = gpu_buffer(allocator, 16, "rt-denoise-pong");
        }
        unsafe {
            let image = device
                .create_image(
                    &vk::ImageCreateInfo::default()
                        .image_type(vk::ImageType::TYPE_2D)
                        .format(vk::Format::R8G8B8A8_UNORM)
                        .extent(vk::Extent3D { width: w, height: h, depth: 1 })
                        .mip_levels(1)
                        .array_layers(1)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .tiling(vk::ImageTiling::OPTIMAL)
                        .usage(vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_SRC)
                        .initial_layout(vk::ImageLayout::UNDEFINED),
                    None,
                )
                .expect("Failed to create RT output image");
            let requirements = device.get_image_memory_requirements(image);
            let allocation = allocator
                .allocate(&AllocationCreateDesc {
                    name: "rt-output",
                    requirements,
                    location: MemoryLocation::GpuOnly,
                    linear: false,
                    allocation_scheme: AllocationScheme::GpuAllocatorManaged,
                })
                .expect("Failed to allocate RT output memory");
            device
                .bind_image_memory(image, allocation.memory(), allocation.offset())
                .expect("Failed to bind RT output memory");
            let view = device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(vk::Format::R8G8B8A8_UNORM)
                        .subresource_range(
                            vk::ImageSubresourceRange::default()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .level_count(1)
                                .layer_count(1),
                        ),
                    None,
                )
                .expect("Failed to create RT output view");
            self.output_image = image;
            self.output_view = view;
            self.output_allocation = Some(allocation);

            for frame in &self.frames {
                let accum_infos = [vk::DescriptorBufferInfo::default()
                    .buffer(self.accum.buffer)
                    .range(vk::WHOLE_SIZE)];
                let feature_infos = [vk::DescriptorBufferInfo::default()
                    .buffer(self.features.buffer)
                    .range(vk::WHOLE_SIZE)];
                let image_infos = [vk::DescriptorImageInfo::default()
                    .image_view(view)
                    .image_layout(vk::ImageLayout::GENERAL)];
                device.update_descriptor_sets(
                    &[
                        vk::WriteDescriptorSet::default()
                            .dst_set(frame.descriptor_set)
                            .dst_binding(4)
                            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                            .buffer_info(&accum_infos),
                        vk::WriteDescriptorSet::default()
                            .dst_set(frame.descriptor_set)
                            .dst_binding(5)
                            .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                            .image_info(&image_infos),
                        vk::WriteDescriptorSet::default()
                            .dst_set(frame.descriptor_set)
                            .dst_binding(6)
                            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                            .buffer_info(&feature_infos),
                    ],
                    &[],
                );
            }
            if let Some(denoiser) = &self.denoiser {
                denoiser.write_target_descriptors(
                    device,
                    self.accum.buffer,
                    self.features.buffer,
                    self.ping.buffer,
                    self.pong.buffer,
                    view,
                );
            }
        }
    }

    pub(super) fn destroy_targets(&mut self, device: &ash::Device, allocator: &mut Allocator) {
        unsafe {
            if self.output_view != vk::ImageView::null() {
                device.destroy_image_view(self.output_view, None);
                device.destroy_image(self.output_image, None);
                self.output_view = vk::ImageView::null();
                self.output_image = vk::Image::null();
            }
        }
        if let Some(a) = self.output_allocation.take() {
            let _ = allocator.free(a);
        }
        destroy_cpu_buffer(device, allocator, &mut self.accum);
        destroy_cpu_buffer(device, allocator, &mut self.features);
        destroy_cpu_buffer(device, allocator, &mut self.ping);
        destroy_cpu_buffer(device, allocator, &mut self.pong);
        self.output_size = (0, 0);
    }

    /// After the frame fence: write this frame's params, and point this
    /// frame's image bindings at the scene's image as it is NOW.
    ///
    /// Each frame, because a shared image is the 2D pass's to replace: its
    /// upload may land after the scene was set, and freeing it destroys the
    /// view. This frame's set is past its fence, so it is safe to rewrite,
    /// and it is rewritten before every use — a view that is gone is never
    /// one a dispatch reads. `shared` looks an id up: its view and size.
    pub(crate) fn write_frame_uniforms(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        shared: &dyn Fn(u32) -> Option<(vk::ImageView, u32, u32)>,
    ) {
        if !self.staged || self.tri_count == 0 {
            return;
        }
        let Some(camera) = self.camera else { return };
        let bound = self.image.as_ref().and_then(|image| {
            let (view, w, h) = match (&image.owned, image.shared) {
                (Some(owned), _) => (owned.view, owned.size.0, owned.size.1),
                (None, Some(id)) => shared(id)?,
                (None, None) => return None,
            };
            Some((view, ParamImage { width: w, height: h, corners: image.corners, opacity: image.opacity }))
        });
        // No image, or one not there yet: opacity 0 lets every ray through
        // the quad, and the stand-in is never sampled for it.
        let (view, param_image) = match bound {
            Some((view, p)) => (view, Some(p)),
            None => (self.stand_in.view, None),
        };
        Self::write_image_descriptor(
            device,
            self.frames[frame_index].descriptor_set,
            view,
            self.image_sampler,
        );
        let params = rt_params(
            camera,
            self.output_size,
            self.sample_index,
            self.spp,
            param_image,
            self.background,
            &self.environment,
        );
        let frame = &mut self.frames[frame_index];
        frame.uniforms.allocation.as_mut().unwrap().mapped_slice_mut().unwrap()
            [..std::mem::size_of::<RtParams>()]
            .copy_from_slice(bytemuck::bytes_of(&params));
        if let Some(denoiser) = &mut self.denoiser {
            denoiser.write_frame_uniforms(
                frame_index,
                self.output_size.0,
                self.output_size.1,
                self.sample_index + self.spp,
            );
        }
    }

    /// Record one accumulation dispatch + the blit into the backdrop's pane
    /// region. Returns false when there is nothing to do (not staged, empty
    /// scene, or converged) — the backdrop then simply keeps its content.
    /// On true, the backdrop ends in TRANSFER_SRC (like `SceneStage::record`).
    ///
    /// `backdrop_in_transfer_src` says the raster scene pass already ran this
    /// frame (backdrop in TRANSFER_SRC); otherwise it is in SHADER_READ_ONLY.
    pub(crate) fn record(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame_index: usize,
        backdrop_image: vk::Image,
        backdrop_extent: vk::Extent2D,
        backdrop_in_transfer_src: bool,
    ) -> bool {
        if !self.staged || self.tri_count == 0 || self.output_image == vk::Image::null() {
            self.staged = false;
            return false;
        }
        self.staged = false;
        if self.sample_index >= MAX_SAMPLES {
            return false;
        }
        let (w, h) = self.output_size;
        let color_range = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .level_count(1)
            .layer_count(1);
        unsafe {
            // Output image to GENERAL for the compute write; order this
            // dispatch's accum access after the previous frame's. The src
            // stage always includes COMPUTE_SHADER — the accum buffer
            // barrier's access flags must be legal for it even on the first
            // dispatch, when the image side is still UNDEFINED.
            let (old_layout, src_access, src_stage) = if self.output_initialized {
                (
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    vk::AccessFlags::TRANSFER_READ,
                    vk::PipelineStageFlags::TRANSFER | vk::PipelineStageFlags::COMPUTE_SHADER,
                )
            } else {
                (
                    vk::ImageLayout::UNDEFINED,
                    vk::AccessFlags::empty(),
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                )
            };
            device.cmd_pipeline_barrier(
                cmd,
                src_stage,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[vk::BufferMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .buffer(self.accum.buffer)
                    .size(vk::WHOLE_SIZE)],
                &[vk::ImageMemoryBarrier::default()
                    .src_access_mask(src_access)
                    .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
                    .old_layout(old_layout)
                    .new_layout(vk::ImageLayout::GENERAL)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(self.output_image)
                    .subresource_range(color_range)],
            );
            self.output_initialized = true;

            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.pipeline_layout,
                0,
                &[self.frames[frame_index].descriptor_set],
                &[],
            );
            device.cmd_dispatch(cmd, w.div_ceil(WORKGROUP), h.div_ceil(WORKGROUP), 1);

            // À-trous denoise passes; the last one rewrites out_img (still
            // GENERAL), so the transfer barrier below covers either writer.
            if let Some(denoiser) = &self.denoiser {
                denoiser.record(device, cmd, frame_index, w, h);
            }

            // Output to TRANSFER_SRC, backdrop to TRANSFER_DST for the blit.
            let (bd_old, bd_access, bd_stage) = if backdrop_in_transfer_src {
                (
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    vk::AccessFlags::TRANSFER_READ,
                    vk::PipelineStageFlags::TRANSFER,
                )
            } else {
                (
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    vk::AccessFlags::SHADER_READ,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                )
            };
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER | bd_stage,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[
                    vk::ImageMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                        .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                        .old_layout(vk::ImageLayout::GENERAL)
                        .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .image(self.output_image)
                        .subresource_range(color_range),
                    vk::ImageMemoryBarrier::default()
                        .src_access_mask(bd_access)
                        .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                        .old_layout(bd_old)
                        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .image(backdrop_image)
                        .subresource_range(color_range),
                ],
            );

            if self.pane_moved {
                self.pane_moved = false;
                device.cmd_clear_color_image(
                    cmd,
                    backdrop_image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &vk::ClearColorValue { float32: [0.0; 4] },
                    &[color_range],
                );
            }

            // Blit (not copy): converts UNORM → the backdrop's sRGB format.
            let (px, py, _, _) = self.pane;
            let dst_x0 = px.min(backdrop_extent.width);
            let dst_y0 = py.min(backdrop_extent.height);
            let bw = w.min(backdrop_extent.width - dst_x0);
            let bh = h.min(backdrop_extent.height - dst_y0);
            if bw > 0 && bh > 0 {
                let layers = vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .layer_count(1);
                device.cmd_blit_image(
                    cmd,
                    self.output_image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    backdrop_image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[vk::ImageBlit::default()
                        .src_subresource(layers)
                        .src_offsets([
                            vk::Offset3D { x: 0, y: 0, z: 0 },
                            vk::Offset3D { x: bw as i32, y: bh as i32, z: 1 },
                        ])
                        .dst_subresource(layers)
                        .dst_offsets([
                            vk::Offset3D { x: dst_x0 as i32, y: dst_y0 as i32, z: 0 },
                            vk::Offset3D {
                                x: (dst_x0 + bw) as i32,
                                y: (dst_y0 + bh) as i32,
                                z: 1,
                            },
                        ])],
                    vk::Filter::NEAREST,
                );
            }

            // Backdrop to TRANSFER_SRC: the swapchain copy path expects it
            // exactly as SceneStage::record leaves it.
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                    .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(backdrop_image)
                    .subresource_range(color_range)],
            );
        }
        self.sample_index += self.spp;
        true
    }
}
