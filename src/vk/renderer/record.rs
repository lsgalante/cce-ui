//! Recording a frame's commands: the 3D backdrop, the UI pass, the display list's batches with
//! their blur snapshots and scissors, images, and the overlay — each clipped to the frame's
//! partial region.

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

    /// The UI pass: begun over the backdrop (load), over a partial region (load, then clear
    /// the region), or over a cleared image; the display list with its images and blur
    /// snapshots; then text and the overlay.
    #[allow(clippy::too_many_arguments)]
    pub(super) unsafe fn record_ui_pass(
        &self,
        cmd: vk::CommandBuffer,
        frame_index: usize,
        image_index: u32,
        frame2d: &Frame2D<'_>,
        use_backdrop: bool,
        partial: Option<vk::Rect2D>,
        frost_exempt: Option<usize>,
    ) {
        let clear_values = [vk::ClearValue {
            color: vk::ClearColorValue { float32: frame2d.clear_color },
        }];
        let full_scissor = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: self.extent,
        };
        let (ui_pass, ui_clear_values): (vk::RenderPass, &[vk::ClearValue]) = if use_backdrop {
            (self.render_pass_load, &[])
        } else if partial.is_some() {
            (self.render_pass_partial, &[])
        } else {
            (self.render_pass, &clear_values)
        };
        self.core.device.cmd_begin_render_pass(
            cmd,
            &vk::RenderPassBeginInfo::default()
                .render_pass(ui_pass)
                .framebuffer(self.framebuffers[image_index as usize])
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: self.extent,
                })
                .clear_values(ui_clear_values),
            vk::SubpassContents::INLINE,
        );
        self.core.device
            .cmd_set_viewport(cmd, 0, &[flipped_viewport(self.extent)]);
        self.core.device.cmd_set_scissor(cmd, 0, &[clip_to(partial, full_scissor)]);
        if let Some(region) = partial {
            // The partial pass loads instead of clearing; clear what it
            // is about to repaint.
            if region.extent.width > 0 && region.extent.height > 0 {
                self.core.device.cmd_clear_attachments(
                    cmd,
                    &[vk::ClearAttachment::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .color_attachment(0)
                        .clear_value(clear_values[0])],
                    &[vk::ClearRect::default().rect(region).layer_count(1)],
                );
            }
        }
        self.record_display_list(cmd, frame_index, image_index, frame2d, partial, frost_exempt);
        self.text.record_draw(&self.core.device, cmd, frame_index);
        self.record_overlay(cmd, frame_index);
        self.core.device.cmd_end_render_pass(cmd);
    }

    /// The display list's geometry interleaved with its images: each image quad draws before
    /// the vertex its `z_before` names, so it sits above earlier geometry and below later
    /// geometry. A blur-behind batch first snapshots the frame so far, and binds the snapshot
    /// for its draws.
    pub(super) unsafe fn record_display_list(
        &self,
        cmd: vk::CommandBuffer,
        frame_index: usize,
        image_index: u32,
        frame2d: &Frame2D<'_>,
        partial: Option<vk::Rect2D>,
        frost_exempt: Option<usize>,
    ) {
        let frame = &self.frames[frame_index];
        let full_scissor = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: self.extent,
        };
        let images = frame2d.images;
        let mut order: Vec<usize> = (0..images.len()).collect();
        order.sort_by_key(|&k| images[k].z_before);
        let mut img_i = 0usize;

        // Corner-shape exponent for the rounded-rect clip SDF, so clipped
        // edges cut along the same squircle family as the tessellated and
        // SDF-lit plate corners (a plate batch overwrites the slot with
        // its own — identical — per-plate value).
        let clip_shape = crate::layout::corner_shape();

        let default_batch = [Batch2D {
            scissor: None,
            clip_rrect: None,
            start: 0,
            end: frame.vertex_count,
            plate: None,
            blur_behind: false,
        }];
        let batches: &[Batch2D] =
            if frame2d.batches.is_empty() { &default_batch } else { frame2d.batches };

        // Which @group(0) the vertex draws bind: the scene-backdrop set
        // until the first blur-behind snapshot, the snapshot set after —
        // so blur plates sample the frame-so-far, and later blur plates
        // sample refreshed copies that include earlier ones.
        let mut active_set = self.descriptor_set;
        // CONSECUTIVE blur plates share one snapshot: only a non-blur
        // draw invalidates it. A run of blur plates (the designer's
        // node bodies) costs one copy, not one per plate — they don't
        // see each other, which only matters where they overlap.
        let mut snapshot_fresh = false;

        for (batch_i, batch) in batches.iter().enumerate() {
            // Images due at this batch's boundary draw first: they sit
            // beneath the batch's geometry, and a blur snapshot taken
            // for this batch must capture them (an image whose
            // `z_before` equals the batch start would otherwise slip
            // to after the snapshot and never be frosted).
            while let Some(&k) = order.get(img_i) {
                let q = &images[k];
                if q.z_before > batch.start {
                    break;
                }
                img_i += 1;
                self.record_image(cmd, frame_index, k, q, partial);
                snapshot_fresh = false;
            }
            if batch.blur_behind && Some(batch_i) == frost_exempt {
                // Nothing drawn yet: the zeroed backdrop the default
                // set binds IS the frame so far (`first_frost_exempt`).
            } else if batch.blur_behind {
                if !snapshot_fresh {
                    self.snapshot_frame_so_far(cmd, image_index as usize);
                    active_set = self.descriptor_set_snapshot;
                    snapshot_fresh = true;
                }
            } else if batch.start < batch.end {
                snapshot_fresh = false;
            }
            // A degenerate scissor skips the vertex draws (images still
            // process on their own clips).
            let batch_scissor = self.batch_scissor(batch.scissor);

            let mut cursor = batch.start;
            while cursor < batch.end {
                let next_z =
                    order.get(img_i).map(|&k| images[k].z_before).unwrap_or(u32::MAX);
                if next_z <= cursor {
                    let k = order[img_i];
                    img_i += 1;
                    self.record_image(cmd, frame_index, k, &images[k], partial);
                    continue;
                }
                let upto = next_z.min(batch.end);
                if let Some(scissor) = batch_scissor {
                    self.core.device
                        .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
                    self.core.device.cmd_bind_descriptor_sets(
                        cmd,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.pipeline_layout,
                        0,
                        &[active_set],
                        &[],
                    );
                    self.core.device
                        .cmd_bind_vertex_buffers(cmd, 0, &[frame.vertex.buffer], &[0]);
                    self.core.device.cmd_set_scissor(cmd, 0, &[clip_to(partial, scissor)]);
                    // Per-batch rounded-rect clip (fragments outside
                    // discard) + the SDF-lit plate block when this
                    // batch is a plate cover quad.
                    let pc = batch_push_constants(batch, clip_shape, frame_index * MAX_PLATE_FEATURES);
                    self.core.device.cmd_push_constants(
                        cmd,
                        self.pipeline_layout,
                        vk::ShaderStageFlags::FRAGMENT,
                        0,
                        bytemuck::cast_slice(&pc),
                    );
                    self.core.device.cmd_draw(cmd, upto - cursor, 1, cursor, 0);
                }
                cursor = upto;
            }
        }
        // Images sorting after all geometry.
        while let Some(&k) = order.get(img_i) {
            img_i += 1;
            self.record_image(cmd, frame_index, k, &images[k], partial);
        }
        // Restore for the text/overlay draws.
        self.core.device.cmd_set_scissor(cmd, 0, &[clip_to(partial, full_scissor)]);
    }

    /// One image quad, scissored to its clip within the partial region.
    pub(super) unsafe fn record_image(
        &self,
        cmd: vk::CommandBuffer,
        frame_index: usize,
        k: usize,
        q: &crate::draw::ImageQuad,
        partial: Option<vk::Rect2D>,
    ) {
        self.core.device.cmd_set_scissor(cmd, 0, &[clip_to(partial, self.image_scissor(q.clip))]);
        self.image.record_quad(&self.core.device, cmd, frame_index, k, q.image);
    }

    /// An image quad's scissor: its clip, cut to the surface, or the whole surface.
    pub(super) fn image_scissor(&self, clip: Option<(u32, u32, u32, u32)>) -> vk::Rect2D {
        match clip {
            Some((cx, cy, cw, ch)) => vk::Rect2D {
                offset: vk::Offset2D { x: cx as i32, y: cy as i32 },
                extent: vk::Extent2D {
                    width: cw.min(self.extent.width.saturating_sub(cx)),
                    height: ch.min(self.extent.height.saturating_sub(cy)),
                },
            },
            None => vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: self.extent,
            },
        }
    }

    /// A batch's scissor cut to the surface, or the whole surface; None when nothing of it
    /// is on the surface.
    pub(super) fn batch_scissor(&self, scissor: Option<(u32, u32, u32, u32)>) -> Option<vk::Rect2D> {
        match scissor {
            Some((bx, by, bw, bh)) => {
                if bx >= self.extent.width || by >= self.extent.height {
                    None
                } else {
                    let bw = bw.min(self.extent.width - bx);
                    let bh = bh.min(self.extent.height - by);
                    if bw == 0 || bh == 0 {
                        None
                    } else {
                        Some(vk::Rect2D {
                            offset: vk::Offset2D { x: bx as i32, y: by as i32 },
                            extent: vk::Extent2D { width: bw, height: bh },
                        })
                    }
                }
            }
            None => Some(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: self.extent,
            }),
        }
    }

    /// The overlay vertices, on top of everything, unclipped by any batch.
    pub(super) unsafe fn record_overlay(&self, cmd: vk::CommandBuffer, frame_index: usize) {
        let frame = &self.frames[frame_index];
        if frame.overlay_count == 0 {
            return;
        }
        // The text pass bound its own pipeline; rebind for the overlay.
        self.core.device
            .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
        self.core.device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            self.pipeline_layout,
            0,
            &[self.descriptor_set],
            &[],
        );
        self.core.device
            .cmd_bind_vertex_buffers(cmd, 0, &[frame.vertex.buffer], &[0]);
        // Push constants persist across binds — clear any batch's
        // rounded clip and plate mode.
        let pc = [0.0f32; PUSH_CONSTANT_FLOATS];
        self.core.device.cmd_push_constants(
            cmd,
            self.pipeline_layout,
            vk::ShaderStageFlags::FRAGMENT,
            0,
            bytemuck::cast_slice(&pc),
        );
        self.core.device
            .cmd_draw(cmd, frame.overlay_count, 1, frame.overlay_start, 0);
    }
}

/// A scissor cut to the frame's partial region, when it has one: every scissor of a frame
/// passes through this.
fn clip_to(partial: Option<vk::Rect2D>, r: vk::Rect2D) -> vk::Rect2D {
    match partial {
        Some(region) => rect_intersect(r, region),
        None => r,
    }
}
