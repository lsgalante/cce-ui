//! Drawing a frame: `draw_frame_2d` and its passes, immediate uploads, text preparation.

use super::*;

/// `CCE_PRESENT_DEBUG=1` traces every acquire/present to stderr — the
/// diagnostic for present-pipeline stalls (a present that logs `acquire...`
/// or `present img N...` with no matching completion line is blocked inside
/// the driver; see the off-viewport freeze notes on the present-mode choice
/// in `create_swapchain`).
pub(crate) fn present_debug() -> bool {
    static FLAG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FLAG.get_or_init(|| std::env::var_os("CCE_PRESENT_DEBUG").is_some())
}

impl VkRenderer {

    /// Whether presenting past an unacknowledged frame callback is safe.
    /// True under MAILBOX (the present replaces the queued buffer). Under
    /// FIFO the driver's present throttle waits on the previous present's
    /// frame event, so a forced present to a surface the compositor isn't
    /// rendering blocks forever — the caller must not force one.
    pub fn forced_present_safe(&self) -> bool {
        self.present_mode == vk::PresentModeKHR::MAILBOX
    }

    /// Whether this renderer takes the process-wide image upload queue
    /// ([`crate::vk::upload_rgba`] and kin) — true by default. A renderer
    /// that keeps images of its own, uploaded with
    /// [`upload_rgba_now`](Self::upload_rgba_now), turns it off so it never
    /// takes an upload the window's renderer was meant to draw.
    pub fn set_shared_uploads(&mut self, on: bool) {
        self.image.shared_uploads = on;
    }

    /// Upload RGBA8 pixels into this renderer's image table now, returning
    /// the id to draw them by in this renderer's frames.
    pub fn upload_rgba_now(&mut self, pixels: &[u8], width: u32, height: u32) -> u32 {
        self.image.upload_now(
            &self.core.device,
            self.core.allocator.as_mut().unwrap(),
            self.core.queue,
            self.core.command_pool,
            pixels,
            width,
            height,
        )
    }

    /// Stage text for the next `draw_frame`: shape-cache misses are rasterized
    /// into the glyph atlas and vertices are built against the current extent.
    /// Mirrors what was `glyphon::TextRenderer::prepare`.
    pub fn prepare_text(
        &mut self,
        font_system: &mut cosmic_text::FontSystem,
        swash_cache: &mut cosmic_text::SwashCache,
        spans: &[TextSpan<'_>],
    ) {
        // Against the extent this frame will actually be drawn at: `resize` is
        // lazy, so with a rebuild pending `self.extent` is still the previous
        // size and text would land in the wrong NDC (visibly mis-scaled and
        // offset while a window auto-sizes to its content).
        self.text.prepare(font_system, swash_cache, spans, self.pending_extent());
    }

    /// Render one frame of plain 2D geometry: a single unclipped batch, no
    /// overlay, transparent clear. See [`VkRenderer::draw_frame_2d`].
    pub fn draw_frame(&mut self, verts: &[Vertex]) -> bool {
        self.draw_frame_2d(Frame2D {
            verts,
            batches: &[],
            overlay_verts: &[],
            images: &[],
            plate_features: &[],
            clear_color: [0.0; 4],
            damage: None,
        })
    }

    /// Render one frame: the 2D geometry (optionally as scissored batches),
    /// then any text staged via `prepare_text`, then the overlay vertices on
    /// top. Returns false if the frame was skipped (swapchain rebuild); the
    /// caller just draws again next tick.
    pub fn draw_frame_2d(&mut self, frame2d: Frame2D<'_>) -> bool {
        if self.surface == vk::SurfaceKHR::null() || self.surface_lost {
            return false;
        }
        if self.swapchain_dirty {
            self.swapchain_dirty = false;
            if let Err(lost) = self.recreate_swapchain() {
                self.mark_surface_lost(lost);
                return false;
            }
            if self.swapchain_dirty {
                // The rebuild couldn't honor the requested extent (surface
                // caps disagree, e.g. mid suspend/resume) — presenting it
                // would commit a wrong-size buffer. Skip; the caller redraws.
                return false;
            }
        }

        unsafe {
            let frame_index = self.frame_index;
            let (in_flight, image_available) = {
                let f = &self.frames[frame_index];
                (f.in_flight, f.image_available)
            };
            self.core.device
                .wait_for_fences(&[in_flight], true, u64::MAX)
                .expect("Fence wait failed");
            // What this slot last carried has finished: the mesh buffers an
            // update replaced while those frames read them may be reused.
            self.scene.frame_waited(&self.core.device, self.core.allocator.as_mut().unwrap(), FRAMES_IN_FLIGHT as u64);

            // The first frame with a blur plate that will copy the frame so
            // far allocates the snapshot it copies into. Decided the way the
            // record below decides, except that a scene ever staged counts as
            // a backdrop (it may become valid while recording), which only
            // ever errs toward allocating.
            if !self.snapshot_wanted {
                let assume_backdrop = self.scene.wanted || self.scene.backdrop_valid;
                let exempt =
                    first_frost_exempt(frame2d.batches, frame2d.images, frame2d.clear_color, assume_backdrop);
                if frame2d.batches.iter().enumerate().any(|(i, b)| b.blur_behind && Some(i) != exempt) {
                    self.snapshot_wanted = true;
                    self.sync_snapshot_target();
                }
            }

            if present_debug() {
                eprintln!("[vk] frame {} acquire...", self.present_debug_count);
            }
            let image_index = match self.swapchain_loader.acquire_next_image(
                self.swapchain,
                u64::MAX,
                image_available,
                vk::Fence::null(),
            ) {
                Ok((index, suboptimal)) => {
                    if suboptimal {
                        self.swapchain_dirty = true;
                    }
                    index
                }
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    self.swapchain_dirty = true;
                    return false;
                }
                Err(result @ vk::Result::ERROR_SURFACE_LOST_KHR) => {
                    self.mark_surface_lost(SurfaceLost { call: "vkAcquireNextImageKHR", result });
                    return false;
                }
                Err(e) => {
                    log::error!("acquire_next_image failed: {e:?}");
                    return false;
                }
            };

            self.core.device.reset_fences(&[in_flight]).unwrap();

            // Re-upload the bevel-profile LUT when it changed (a live ramp
            // edit). The other in-flight frame may still read the old bytes —
            // both are valid profiles, so the one-frame mix is benign.
            if self.profile_gen != crate::layout::bevel_profile_generation()
                || self.roll_profile_gen != crate::layout::roll_profile_generation()
                || self.relief_uploaded != self.relief_px()
            {
                self.write_window_info();
            }

            // Upload this frame's plate carves into its slot of the feature
            // UBO (the slot's previous user has fenced, so no race).
            if !frame2d.plate_features.is_empty() {
                let n = frame2d.plate_features.len().min(MAX_PLATE_FEATURES);
                let base = frame_index * MAX_PLATE_FEATURES * PLATE_FEATURE_BYTES;
                if let Some(allocation) = self.plate_features.allocation.as_mut() {
                    let bytes: &[u8] = bytemuck::cast_slice(&frame2d.plate_features[..n]);
                    allocation.mapped_slice_mut().unwrap()[base..base + bytes.len()]
                        .copy_from_slice(bytes);
                }
            }

            // Upload display-list + overlay vertices into this frame's buffer
            // (its fence has signaled, so the GPU is done with it; growing swaps
            // in a fresh buffer). Overlay verts sit after the main range.
            let vert_bytes: &[u8] = bytemuck::cast_slice(frame2d.verts);
            let overlay_bytes: &[u8] = bytemuck::cast_slice(frame2d.overlay_verts);
            let needed = (vert_bytes.len() + overlay_bytes.len()) as vk::DeviceSize;
            if needed > self.frames[frame_index].vertex.size {
                let mut old =
                    std::mem::replace(&mut self.frames[frame_index].vertex, AllocatedBuffer::null());
                let allocator = self.core.allocator.as_mut().unwrap();
                destroy_cpu_buffer(&self.core.device, allocator, &mut old);
                self.frames[frame_index].vertex = create_cpu_buffer(
                    &self.core.device,
                    allocator,
                    needed.next_power_of_two(),
                    vk::BufferUsageFlags::VERTEX_BUFFER,
                    "vertices",
                );
            }
            if needed > 0 {
                let mapped = self.frames[frame_index]
                    .vertex
                    .allocation
                    .as_mut()
                    .unwrap()
                    .mapped_slice_mut()
                    .unwrap();
                mapped[..vert_bytes.len()].copy_from_slice(vert_bytes);
                mapped[vert_bytes.len()..vert_bytes.len() + overlay_bytes.len()]
                    .copy_from_slice(overlay_bytes);
            }
            self.frames[frame_index].vertex_count = frame2d.verts.len() as u32;
            self.frames[frame_index].overlay_start = frame2d.verts.len() as u32;
            self.frames[frame_index].overlay_count = frame2d.overlay_verts.len() as u32;
            self.text.write_frame_buffers(
                &self.core.device,
                self.core.allocator.as_mut().unwrap(),
                frame_index,
            );
            self.image.process_pending(
                &self.core.device,
                self.core.allocator.as_mut().unwrap(),
                self.core.queue,
                self.core.command_pool,
            );
            self.image.write_frame_buffer(
                &self.core.device,
                self.core.allocator.as_mut().unwrap(),
                frame_index,
                frame2d.images,
                self.extent,
            );
            let clip_radius = self.clip_corner_radius();
            self.scene.write_frame_uniforms(
                &self.core.device,
                self.core.allocator.as_mut().unwrap(),
                frame_index,
                clip_radius,
            );
            if let Some(rt) = self.rt.as_mut() {
                let images = &self.image;
                rt.write_frame_uniforms(&self.core.device, frame_index, &|id| {
                    images.view_and_size(id)
                });
            }

            // Record.
            let cmd = self.frames[frame_index].cmd;
            self.core.device
                .begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())
                .unwrap();
            self.text.record_upload(&self.core.device, cmd, frame_index);

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

            let frame = &self.frames[frame_index];
            let clear_values = [vk::ClearValue {
                color: vk::ClearColorValue { float32: frame2d.clear_color },
            }];
            let full_scissor = vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: self.extent,
            };
            // This frame's damage, and whether the acquired image can be
            // brought up to date by repainting part of it. Backdrop (3D
            // scene) frames copy whole images around and stay full; a
            // frosted plate grows the region instead (below).
            let damage = frame2d.damage.map(|(x, y, w, h)| {
                rect_intersect(
                    vk::Rect2D {
                        offset: vk::Offset2D { x: x as i32, y: y as i32 },
                        extent: vk::Extent2D { width: w, height: h },
                    },
                    full_scissor,
                )
            });
            let frost_exempt = first_frost_exempt(frame2d.batches, frame2d.images, frame2d.clear_color, use_backdrop);
            let mut partial: Option<vk::Rect2D> = match (damage, self.image_ages[image_index as usize]) {
                _ if use_backdrop => None,
                (Some(d), ImageAge::Current) => Some(d),
                (Some(d), ImageAge::Behind(missing)) => Some(rect_union(d, missing)),
                _ => None,
            };
            // A frosted plate's blur reads the snapshot of the frame so far,
            // and in a partial frame that snapshot is only right inside the
            // region — outside it the image holds the previous FINAL frame,
            // the plate and whatever covers it included. So a plate the
            // region touches is repainted whole, with everything its blur
            // can reach, and the grown region may touch the next plate.
            if let Some(mut region) = partial {
                let frosts: Vec<vk::Rect2D> = frame2d
                    .batches
                    .iter()
                    .enumerate()
                    .filter(|&(i, b)| b.blur_behind && Some(i) != frost_exempt && b.start < b.end)
                    .filter_map(|(_, b)| {
                        let verts = frame2d.verts.get(b.start as usize..b.end as usize)?;
                        verts_bounds(verts, self.extent)
                    })
                    .collect();
                loop {
                    let mut grew = false;
                    for &plate in &frosts {
                        let need = rect_intersect(rect_expand(plate, BLUR_REACH_PX), full_scissor);
                        if rect_overlaps(region, plate) && rect_intersect(region, need) != need {
                            region = rect_union(region, need);
                            grew = true;
                        }
                    }
                    if !grew {
                        break;
                    }
                }
                partial = Some(region);
            }
            // Every scissor of the frame passes through this.
            let clip = |r: vk::Rect2D| match partial {
                Some(region) => rect_intersect(r, region),
                None => r,
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
            self.core.device.cmd_set_scissor(cmd, 0, &[clip(full_scissor)]);
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
            // Display-list geometry interleaved with user images: each image
            // quad draws before the vertex its `z_before` names, so it sits
            // above earlier geometry and below later geometry.
            {
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
                        let img_scissor = match q.clip {
                            Some((cx, cy, cw, ch)) => vk::Rect2D {
                                offset: vk::Offset2D { x: cx as i32, y: cy as i32 },
                                extent: vk::Extent2D {
                                    width: cw.min(self.extent.width.saturating_sub(cx)),
                                    height: ch.min(self.extent.height.saturating_sub(cy)),
                                },
                            },
                            None => full_scissor,
                        };
                        self.core.device.cmd_set_scissor(cmd, 0, &[clip(img_scissor)]);
                        self.image.record_quad(&self.core.device, cmd, frame_index, k, q.image);
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
                    // Resolve the batch scissor; a degenerate one skips the
                    // vertex draws (images still process on their own clips).
                    let batch_scissor: Option<vk::Rect2D> = match batch.scissor {
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
                        None => Some(full_scissor),
                    };

                    let mut cursor = batch.start;
                    while cursor < batch.end {
                        let next_z =
                            order.get(img_i).map(|&k| images[k].z_before).unwrap_or(u32::MAX);
                        if next_z <= cursor {
                            let k = order[img_i];
                            img_i += 1;
                            let q = &images[k];
                            let img_scissor = match q.clip {
                                Some((cx, cy, cw, ch)) => vk::Rect2D {
                                    offset: vk::Offset2D { x: cx as i32, y: cy as i32 },
                                    extent: vk::Extent2D {
                                        width: cw.min(self.extent.width.saturating_sub(cx)),
                                        height: ch.min(self.extent.height.saturating_sub(cy)),
                                    },
                                },
                                None => full_scissor,
                            };
                            self.core.device.cmd_set_scissor(cmd, 0, &[clip(img_scissor)]);
                            self.image.record_quad(&self.core.device, cmd, frame_index, k, q.image);
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
                            self.core.device.cmd_set_scissor(cmd, 0, &[clip(scissor)]);
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
                    let q = &images[k];
                    let img_scissor = match q.clip {
                        Some((cx, cy, cw, ch)) => vk::Rect2D {
                            offset: vk::Offset2D { x: cx as i32, y: cy as i32 },
                            extent: vk::Extent2D {
                                width: cw.min(self.extent.width.saturating_sub(cx)),
                                height: ch.min(self.extent.height.saturating_sub(cy)),
                            },
                        },
                        None => full_scissor,
                    };
                    self.core.device.cmd_set_scissor(cmd, 0, &[clip(img_scissor)]);
                    self.image.record_quad(&self.core.device, cmd, frame_index, k, q.image);
                }
                // Restore for the text/overlay draws.
                self.core.device.cmd_set_scissor(cmd, 0, &[clip(full_scissor)]);
            }
            self.text.record_draw(&self.core.device, cmd, frame_index);
            if frame.overlay_count > 0 {
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
            self.core.device.cmd_end_render_pass(cmd);
            self.core.device.end_command_buffer(cmd).unwrap();

            // Submit + present. The acquire semaphore gates the swapchain image's
            // first use: the backdrop copy (TRANSFER) or the UI pass (COLOR).
            let wait_semaphores = [image_available];
            let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags::TRANSFER];
            let cmds = [cmd];
            let signal_semaphores = [self.render_finished[image_index as usize]];
            let submit = vk::SubmitInfo::default()
                .wait_semaphores(&wait_semaphores)
                .wait_dst_stage_mask(&wait_stages)
                .command_buffers(&cmds)
                .signal_semaphores(&signal_semaphores);
            self.core.device
                .queue_submit(self.core.queue, &[submit], in_flight)
                .expect("Queue submit failed");
            self.scene.submitted += 1;

            // The image now holds this frame; every other image fell behind
            // by this frame's damage.
            for (k, age) in self.image_ages.iter_mut().enumerate() {
                *age = if k == image_index as usize {
                    ImageAge::Current
                } else {
                    match (damage, *age) {
                        (Some(d), ImageAge::Current) => ImageAge::Behind(d),
                        (Some(d), ImageAge::Behind(m)) => ImageAge::Behind(rect_union(d, m)),
                        _ => ImageAge::Unknown,
                    }
                };
            }

            let swapchains = [self.swapchain];
            let image_indices = [image_index];
            // What changed since the previous present — the frame's damage,
            // however much of the image had to be repainted to get there.
            // Only for a frame that was itself partial: the first frames of
            // a swapchain replace a buffer of another size or none at all.
            let present_rects = [vk::RectLayerKHR::default()
                .offset(damage.unwrap_or(full_scissor).offset)
                .extent(damage.unwrap_or(full_scissor).extent)
                .layer(0)];
            let present_region = [vk::PresentRegionKHR::default().rectangles(&present_rects)];
            let mut present_regions = vk::PresentRegionsKHR::default().regions(&present_region);
            let mut present = vk::PresentInfoKHR::default()
                .wait_semaphores(&signal_semaphores)
                .swapchains(&swapchains)
                .image_indices(&image_indices);
            if self.core.incremental_present && partial.is_some() {
                present = present.push_next(&mut present_regions);
            }
            if present_debug() {
                eprintln!("[vk] frame {} present img {}...", self.present_debug_count, image_index);
            }
            match self.swapchain_loader.queue_present(self.core.queue, &present) {
                Ok(suboptimal) => {
                    if suboptimal {
                        self.swapchain_dirty = true;
                    }
                }
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    self.swapchain_dirty = true;
                }
                Err(result @ vk::Result::ERROR_SURFACE_LOST_KHR) => {
                    self.mark_surface_lost(SurfaceLost { call: "vkQueuePresentKHR", result });
                }
                Err(e) => log::error!("queue_present failed: {e:?}"),
            }

            if present_debug() {
                eprintln!("[vk] frame {} presented", self.present_debug_count);
                self.present_debug_count += 1;
            }
            self.frame_index = (self.frame_index + 1) % FRAMES_IN_FLIGHT;
        }
        true
    }
}
