//! The blur-behind snapshot: the frame so far copied into the snapshot image and its mip chain,
//! and keeping the backdrop and snapshot targets the size of the swapchain.

use super::*;

impl VkRenderer {

    /// Suspend the UI pass, copy the swapchain's frame-so-far into the blur
    /// snapshot image, and resume drawing — the mechanism behind blur-behind
    /// plates (`Batch2D::blur_behind`). Ending the pass leaves the swapchain in
    /// its PRESENT final layout; the copy walks it through TRANSFER_SRC and
    /// hands it back in TRANSFER_DST, which is exactly `render_pass_load`'s
    /// expected initial layout, so the resume reuses that pass (and the shared
    /// framebuffers). Dynamic viewport state dies with the pass and is restored;
    /// scissor/pipeline/descriptors are re-bound per draw by the batch loop.
    pub(super) fn snapshot_frame_so_far(&self, cmd: vk::CommandBuffer, image_index: usize) {
        let device = &self.core.device;
        let swapchain_image = self.swapchain_images[image_index];
        unsafe {
            device.cmd_end_render_pass(cmd);
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[
                    vk::ImageMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                        .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                        .old_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                        .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .image(swapchain_image)
                        .subresource_range(COLOR_RANGE),
                    // Covers the previous frame's blur reads of the snapshot,
                    // every level of it: the whole chain is rewritten.
                    vk::ImageMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::SHADER_READ)
                        .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                        .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .image(self.snapshot_image)
                        .subresource_range(color_levels(0, self.snapshot_levels)),
                ],
            );
            let subresource = vk::ImageSubresourceLayers::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .layer_count(1);
            device.cmd_copy_image(
                cmd,
                swapchain_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                self.snapshot_image,
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
            self.snapshot_mip_chain(cmd);
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER | vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[
                    vk::ImageMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::TRANSFER_READ)
                        .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                        .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .image(swapchain_image)
                        .subresource_range(COLOR_RANGE),
                ],
            );
            device.cmd_begin_render_pass(
                cmd,
                &vk::RenderPassBeginInfo::default()
                    .render_pass(self.render_pass_load)
                    .framebuffer(self.framebuffers[image_index])
                    .render_area(vk::Rect2D {
                        offset: vk::Offset2D { x: 0, y: 0 },
                        extent: self.extent,
                    }),
                vk::SubpassContents::INLINE,
            );
            device.cmd_set_viewport(cmd, 0, &[flipped_viewport(self.extent)]);
        }
    }

    /// Build the snapshot's mip chain from the level 0 just copied in, and
    /// leave every level SHADER_READ_ONLY. Expects every level in
    /// TRANSFER_DST, as `snapshot_frame_so_far` leaves them.
    ///
    /// The chain is what the frost kernel samples (shader2d's `resolve_blur`):
    /// its 7x7 taps stand a whole STRIDE apart — 5.5 physical px for the
    /// panel's default kernel — and a tap at level 0 reads only the texel or
    /// two it lands between. So anything behind a plate thinner than the
    /// stride (a hairline, a well's edge, a glyph) was not blurred but picked
    /// up whole by the taps that hit it and missed by the rest: seven faint
    /// copies a stride apart, which over a UI's rows read as horizontal
    /// bands. Read at the level whose texel is as wide as the stride, each tap
    /// is already the average of the cell around it, and the copies merge
    /// into one smooth smear. Each level is a linear-filtered blit of the
    /// one above it, a 2x2 box.
    pub(super) fn snapshot_mip_chain(&self, cmd: vk::CommandBuffer) {
        let device = &self.core.device;
        let barrier = |level: u32, src: vk::AccessFlags, dst: vk::AccessFlags, old: vk::ImageLayout, new: vk::ImageLayout| {
            vk::ImageMemoryBarrier::default()
                .src_access_mask(src)
                .dst_access_mask(dst)
                .old_layout(old)
                .new_layout(new)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(self.snapshot_image)
                .subresource_range(color_levels(level, 1))
        };
        let size = |level: u32| {
            [
                (self.extent.width >> level).max(1) as i32,
                (self.extent.height >> level).max(1) as i32,
            ]
        };
        unsafe {
            for level in 1..self.snapshot_levels {
                // The level above is written; read it for the blit.
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[barrier(
                        level - 1,
                        vk::AccessFlags::TRANSFER_WRITE,
                        vk::AccessFlags::TRANSFER_READ,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    )],
                );
                let ([sw, sh], [dw, dh]) = (size(level - 1), size(level));
                let layers = |mip: u32| {
                    vk::ImageSubresourceLayers::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .mip_level(mip)
                        .layer_count(1)
                };
                device.cmd_blit_image(
                    cmd,
                    self.snapshot_image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    self.snapshot_image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[vk::ImageBlit::default()
                        .src_subresource(layers(level - 1))
                        .src_offsets([vk::Offset3D::default(), vk::Offset3D { x: sw, y: sh, z: 1 }])
                        .dst_subresource(layers(level))
                        .dst_offsets([vk::Offset3D::default(), vk::Offset3D { x: dw, y: dh, z: 1 }])],
                    vk::Filter::LINEAR,
                );
            }
            // Every level sampleable: the ones blitted FROM are in
            // TRANSFER_SRC, the last (or the only) one still in TRANSFER_DST.
            let last = self.snapshot_levels - 1;
            let mut to_read: Vec<vk::ImageMemoryBarrier> = (0..last)
                .map(|l| {
                    barrier(
                        l,
                        vk::AccessFlags::TRANSFER_READ,
                        vk::AccessFlags::SHADER_READ,
                        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    )
                })
                .collect();
            to_read.push(barrier(
                last,
                vk::AccessFlags::TRANSFER_WRITE,
                vk::AccessFlags::SHADER_READ,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            ));
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &to_read,
            );
        }
    }

    /// Recreate backdrop + depth at the surface size (device must be idle),
    /// re-point the UI descriptor at the new view, and make the fresh image
    /// legal to sample.
    ///
    /// A rebuild at the SAME size (a swapchain reported suboptimal or out of
    /// date, a corner-radius change) keeps the backdrop, and must not clear
    /// it: `backdrop_valid` stays true across it, so a cleared image is
    /// replayed under the UI as the scene, and an app that stages only when
    /// its scene changes never repairs it. Seen as a designer viewport that
    /// stayed black until the pointer moved, on a discrete GPU presenting to
    /// a compositor on the integrated one — its swapchain is rebuilt at the
    /// same size a few frames in (`CCE_PRESENT_DEBUG` logs every rebuild).
    pub(super) fn sync_backdrop_targets(&mut self) {
        let extent = self.scene.target_extent(self.extent);
        let recreated = self.scene.resize(
            &self.core.device,
            self.core.allocator.as_mut().unwrap(),
            extent,
        );
        if recreated {
            clear_image_to_shader_read(
                &self.core.device,
                self.core.queue,
                self.core.command_pool,
                self.scene.backdrop_image,
            );
        }
        let image_infos = [vk::DescriptorImageInfo::default()
            .image_view(self.scene.backdrop_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        unsafe {
            self.core.device.update_descriptor_sets(
                &[vk::WriteDescriptorSet::default()
                    .dst_set(self.descriptor_set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(&image_infos)],
                &[],
            );
        }
        if self.snapshot_wanted {
            self.sync_snapshot_target();
        }
    }

    /// (Re)create the blur snapshot at the surface size and point its
    /// descriptor set at it. Only for a renderer that has drawn a blur plate
    /// needing one (`snapshot_wanted`): most windows frost only their root
    /// plate, which reads the zeroed backdrop instead (`first_frost_exempt`),
    /// and the grid draws no frost at all — and the snapshot is a whole
    /// surface, ~16 MiB for a 2560x1600 window and ~240 MiB for the grid's
    /// patch. The device must not be using the snapshot set (idle, or the set
    /// never bound because the snapshot never existed).
    pub(super) fn sync_snapshot_target(&mut self) {
        let levels = if self.blur_mips { snapshot_levels(self.extent) } else { 1 };
        self.snapshot_levels = levels;
        // Same format as the swapchain, so cmd_copy_image from it is legal.
        unsafe {
            let device = &self.core.device;
            if self.snapshot_view != vk::ImageView::null() {
                device.destroy_image_view(self.snapshot_view, None);
                device.destroy_image(self.snapshot_image, None);
                self.snapshot_view = vk::ImageView::null();
                self.snapshot_image = vk::Image::null();
            }
            if let Some(alloc) = self.snapshot_allocation.take() {
                let _ = self.core.allocator.as_mut().unwrap().free(alloc);
            }
            let device = &self.core.device;
            let snapshot_image = device
                .create_image(
                    &vk::ImageCreateInfo::default()
                        .image_type(vk::ImageType::TYPE_2D)
                        .format(self.surface_format.format)
                        .extent(vk::Extent3D {
                            width: self.extent.width,
                            height: self.extent.height,
                            depth: 1,
                        })
                        .mip_levels(levels)
                        .array_layers(1)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .tiling(vk::ImageTiling::OPTIMAL)
                        .usage(
                            vk::ImageUsageFlags::SAMPLED
                                | vk::ImageUsageFlags::TRANSFER_DST
                                | vk::ImageUsageFlags::TRANSFER_SRC,
                        )
                        .initial_layout(vk::ImageLayout::UNDEFINED),
                    None,
                )
                .expect("Failed to create snapshot image");
            let requirements = device.get_image_memory_requirements(snapshot_image);
            let allocation = self
                .core
                .allocator
                .as_mut()
                .unwrap()
                .allocate(&AllocationCreateDesc {
                    name: "blur-snapshot",
                    requirements,
                    location: MemoryLocation::GpuOnly,
                    linear: false,
                    allocation_scheme: AllocationScheme::GpuAllocatorManaged,
                })
                .expect("Failed to allocate snapshot memory");
            self.core
                .device
                .bind_image_memory(snapshot_image, allocation.memory(), allocation.offset())
                .expect("Failed to bind snapshot memory");
            let snapshot_view = self
                .core
                .device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(snapshot_image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(self.surface_format.format)
                        .subresource_range(color_levels(0, levels)),
                    None,
                )
                .expect("Failed to create snapshot view");
            self.snapshot_image = snapshot_image;
            self.snapshot_view = snapshot_view;
            self.snapshot_allocation = Some(allocation);
        }
        // A fresh snapshot must be legal to sample before its first copy.
        clear_image_levels_to_shader_read(
            &self.core.device,
            self.core.queue,
            self.core.command_pool,
            self.snapshot_image,
            levels,
        );
        let snapshot_infos = [vk::DescriptorImageInfo::default()
            .image_view(self.snapshot_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        unsafe {
            self.core.device.update_descriptor_sets(
                &[vk::WriteDescriptorSet::default()
                    .dst_set(self.descriptor_set_snapshot)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(&snapshot_infos)],
                &[],
            );
        }
    }
}
