//! A frame: queued image uploads and frees drained into the image table, the blur snapshot, the
//! render passes, and `draw_frame_2d`.

use super::*;

impl WebRenderer {
    /// Apply the image queue: uploads, in-place updates, frees.
    pub(super) fn process_images(&mut self) -> Result<(), JsValue> {
        for pending in take_pending() {
            match pending {
                Pending::Upload { id, pixels, width, height, format, mips: _ }
                | Pending::Update { id, pixels, width, height, format } => {
                    let same = self
                        .images
                        .get(&id)
                        .is_some_and(|img| img.width == width && img.height == height && img.format == format);
                    if !same {
                        if let Some(old) = self.images.remove(&id) {
                            old.texture.destroy();
                        }
                        let tex_format = match format {
                            PixelFormat::Rgba => GpuTextureFormat::Rgba8unormSrgb,
                            PixelFormat::Bgra => GpuTextureFormat::Bgra8unormSrgb,
                        };
                        let texture = texture(
                            &self.device,
                            tex_format,
                            width,
                            height,
                            texture_usage::TEXTURE_BINDING | texture_usage::COPY_DST,
                            "image",
                        )?;
                        let group = texture_group(&self.device, &self.glyph_layout, &whole_view(&texture)?, &self.image_sampler);
                        self.images.insert(id, WebImage { texture, group, width, height, format });
                    }
                    let img = &self.images[&id];
                    write_texture(&self.queue, &img.texture, &pixels, width, height)?;
                    retire_buffer(pixels);
                }
                Pending::UpdateRegions { id, pixels, width, height, format, regions } => {
                    // Only into the picture the regions were cut from; see
                    // `update_pixel_regions` for why a mismatch writes nothing.
                    if let Some(img) = self
                        .images
                        .get(&id)
                        .filter(|img| img.width == width && img.height == height && img.format == format)
                    {
                        let mut offset = 0usize;
                        for &region in &regions {
                            let len = (region.2 * region.3 * 4) as usize;
                            write_texture_region(&self.queue, &img.texture, &pixels[offset..offset + len], region)?;
                            offset += len;
                        }
                    }
                    retire_buffer(pixels);
                }
                Pending::Free { id } => {
                    if let Some(old) = self.images.remove(&id) {
                        old.texture.destroy();
                    }
                }
            }
        }
        Ok(())
    }

    /// The blur snapshot texture and its `@group(0)`, sized to the target.
    pub(super) fn snapshot_group(&mut self, w: u32, h: u32) -> Result<(GpuTexture, GpuBindGroup), JsValue> {
        if let Some((tex, group, sw, sh)) = &self.snapshot {
            if *sw == w && *sh == h {
                return Ok((tex.clone(), group.clone()));
            }
            tex.destroy();
        }
        let tex = texture(&self.device, self.view_format, w, h, texture_usage::TEXTURE_BINDING | texture_usage::COPY_DST, "blur-snapshot")?;
        let group = Self::group_0(&self.device, &self.layout_0, &whole_view(&tex)?, &self.backdrop_sampler, &self.window_info, &self.plate_features);
        self.snapshot = Some((tex.clone(), group.clone(), w, h));
        Ok((tex, group))
    }

    pub(super) fn begin_pass(encoder: &GpuCommandEncoder, view: &GpuTextureView, clear: Option<[f32; 4]>) -> Result<GpuRenderPassEncoder, JsValue> {
        let attachment = match clear {
            Some(c) => {
                let a = GpuRenderPassColorAttachment::new_with_gpu_texture_view(GpuLoadOp::Clear, GpuStoreOp::Store, view);
                a.set_clear_value(&[
                    js_sys::Number::from(c[0] as f64),
                    js_sys::Number::from(c[1] as f64),
                    js_sys::Number::from(c[2] as f64),
                    js_sys::Number::from(c[3] as f64),
                ]);
                a
            }
            None => GpuRenderPassColorAttachment::new_with_gpu_texture_view(GpuLoadOp::Load, GpuStoreOp::Store, view),
        };
        encoder.begin_render_pass(&GpuRenderPassDescriptor::new(&[js_sys::JsOption::wrap(attachment)]))
    }

    /// Draw one frame into the canvas. The browser presents it when the task
    /// that called this returns.
    pub fn draw_frame_2d(&mut self, frame: Frame2D<'_>) -> Result<(), JsValue> {
        self.process_images()?;
        let target = self.context.get_current_texture()?;
        let (w, h) = (target.width(), target.height());
        // The scene's backdrop follows the canvas; a resized one holds no
        // scene until the next is drawn into it, as on Vulkan.
        let rt_staged = self.rt.as_ref().is_some_and(|rt| rt.staged());
        if self.scene.has_staged() || rt_staged || self.scene.target.is_some() {
            let had = self.scene.target.as_ref().map(|t| (t.width, t.height));
            self.scene.fit(&self.device, w, h)?;
            if had != Some((w, h)) {
                let t = self.scene.target.as_ref().expect("fit made one");
                self.scene_group_0 = Some(Self::group_0(
                    &self.device,
                    &self.layout_0,
                    &t.view,
                    &self.backdrop_sampler,
                    &self.window_info,
                    &self.plate_features,
                ));
            }
        }
        let view_desc = GpuTextureViewDescriptor::new();
        view_desc.set_format(self.view_format);
        let view = target.create_view_with_descriptor(&view_desc)?;

        // The frame's uniforms and vertices.
        let info = window_info_data(w, h, 0.0, relief_px_at(crate::scale::scale_factor()));
        self.queue.write_buffer_with_u32_and_u8_slice(&self.window_info, 0, bytemuck::cast_slice(&info))?;
        if !frame.plate_features.is_empty() {
            let n = frame.plate_features.len().min(MAX_PLATE_FEATURES);
            self.queue.write_buffer_with_u32_and_u8_slice(
                &self.plate_features,
                0,
                bytemuck::cast_slice(&frame.plate_features[..n]),
            )?;
        }
        let vert_bytes: &[u8] = bytemuck::cast_slice(frame.verts);
        let overlay_bytes: &[u8] = bytemuck::cast_slice(frame.overlay_verts);
        let mut all = Vec::with_capacity(vert_bytes.len() + overlay_bytes.len());
        all.extend_from_slice(vert_bytes);
        all.extend_from_slice(overlay_bytes);
        self.vertices.ensure(&self.device, all.len() as u32)?;
        self.vertices.write(&self.queue, &all)?;
        let vertex_count = frame.verts.len() as u32;
        let overlay_count = frame.overlay_verts.len() as u32;

        let default_batch = [Batch2D { scissor: None, clip_rrect: None, start: 0, end: vertex_count, plate: None, blur_behind: false }];
        let batches: &[Batch2D] = if frame.batches.is_empty() { &default_batch } else { frame.batches };
        // Corner-shape exponent for the rounded-rect clip SDF (see the Vulkan renderer).
        let clip_shape = crate::layout::corner_shape();
        // One block per batch, then the overlay's zero block.
        let block_floats = WEBGPU_BLOCK_STRIDE / 4;
        let mut blocks = vec![0.0f32; (batches.len() + 1) * block_floats];
        for (i, batch) in batches.iter().enumerate() {
            blocks[i * block_floats..i * block_floats + PUSH_CONSTANT_FLOATS]
                .copy_from_slice(&batch_push_constants(batch, clip_shape, 0));
        }
        let overlay_block = batches.len() as u32;
        if self.blocks.ensure(&self.device, (blocks.len() * 4) as u32)? {
            self.group_1 = Self::group_1(&self.device, &self.layout_1, &self.blocks.buffer);
        }
        self.blocks.write(&self.queue, bytemuck::cast_slice(&blocks))?;

        if self.atlas_uploaded != self.atlas.generation() {
            write_texture(&self.queue, &self.atlas_texture, self.atlas.pixels(), ATLAS_SIZE, ATLAS_SIZE)?;
            self.atlas_uploaded = self.atlas.generation();
        }
        let glyph_bytes: &[u8] = bytemuck::cast_slice(self.atlas.vertices());
        self.glyph_vertices.ensure(&self.device, glyph_bytes.len() as u32)?;
        self.glyph_vertices.write(&self.queue, glyph_bytes)?;
        let image_verts = image_quad_vertices(frame.images, w, h);
        let image_bytes: &[u8] = bytemuck::cast_slice(&image_verts);
        self.image_vertices.ensure(&self.device, image_bytes.len() as u32)?;
        self.image_vertices.write(&self.queue, image_bytes)?;

        // The blur snapshot, made (or resized) before recording when any
        // batch needs one.
        let snapshot = if batches.iter().any(|b| b.blur_behind) { Some(self.snapshot_group(w, h)?) } else { None };

        // Record. The 3D pass first, into the backdrop; with a scene shown,
        // the backdrop is copied into the canvas and the UI pass loads it
        // (rather than clearing) and samples it for its blur plates.
        let encoder = self.device.create_command_encoder();
        let images_for_scene = &self.images;
        self.scene.record(&self.device, &self.queue, &encoder, &|id| images_for_scene.get(&id).map(|i| i.group.clone()))?;
        // The traced pane, into the same backdrop after the raster scene.
        if let (Some(rt), Some(t)) = (self.rt.as_mut(), self.scene.target.as_ref()) {
            let image_view = |id: u32| {
                let img = images_for_scene.get(&id)?;
                Some((img.texture.create_view().ok()?, img.width, img.height))
            };
            if rt.record(&self.device, &self.queue, &encoder, &t.view, (t.width, t.height), &image_view)? {
                self.scene.backdrop_valid = true;
            }
        }
        let base_group_0 = match (&self.scene_group_0, &self.scene.target) {
            (Some(group), Some(t)) if self.scene.backdrop_valid => {
                encoder.copy_texture_to_texture_with_gpu_extent_3d_dict(
                    &GpuTexelCopyTextureInfo::new(&t.backdrop),
                    &GpuTexelCopyTextureInfo::new(&target),
                    &extent(w, h),
                )?;
                Some(group.clone())
            }
            _ => None,
        };
        let mut pass = Self::begin_pass(&encoder, &view, if base_group_0.is_some() { None } else { Some(frame.clear_color) })?;
        let base_group_0 = base_group_0.unwrap_or_else(|| self.group_0.clone());
        let clamp_scissor = |pass: &GpuRenderPassEncoder, (x, y, sw, sh): (u32, u32, u32, u32)| {
            let x = x.min(w);
            let y = y.min(h);
            pass.set_scissor_rect(x, y, sw.min(w - x), sh.min(h - y));
        };
        let images = frame.images;
        let mut order: Vec<usize> = (0..images.len()).collect();
        order.sort_by_key(|&k| images[k].z_before);
        let mut img_i = 0usize;
        let draw_image = |pass: &GpuRenderPassEncoder, k: usize| {
            let q = &images[k];
            let Some(img) = self.images.get(&q.image) else { return }; // not landed / freed: skipped, as on Vulkan
            clamp_scissor(pass, q.clip.unwrap_or((0, 0, w, h)));
            pass.set_pipeline(&self.glyph_pipeline);
            pass.set_bind_group(0, Some(&img.group));
            pass.set_vertex_buffer_with_u32(0, Some(&self.image_vertices.buffer), 0);
            pass.draw_with_instance_count_and_first_vertex(6, 1, (k * 6) as u32);
        };

        // The `@group(0)` vertex draws bind: the empty backdrop until the
        // first blur snapshot, the snapshot after. Consecutive blur plates
        // share one snapshot; only a non-blur draw invalidates it.
        let mut active_group_0 = base_group_0.clone();
        let mut snapshot_fresh = false;

        for (bi, batch) in batches.iter().enumerate() {
            // Images due at this batch's boundary draw first (beneath its
            // geometry, and inside a snapshot taken for it).
            while let Some(&k) = order.get(img_i) {
                if images[k].z_before > batch.start {
                    break;
                }
                img_i += 1;
                draw_image(&pass, k);
                snapshot_fresh = false;
            }
            if batch.blur_behind {
                if !snapshot_fresh {
                    let (snap, snap_group) = snapshot.as_ref().expect("made above for a blur batch");
                    pass.end();
                    encoder.copy_texture_to_texture_with_gpu_extent_3d_dict(
                        &GpuTexelCopyTextureInfo::new(&target),
                        &GpuTexelCopyTextureInfo::new(snap),
                        &extent(w, h),
                    )?;
                    pass = Self::begin_pass(&encoder, &view, None)?;
                    active_group_0 = snap_group.clone();
                    snapshot_fresh = true;
                }
            } else if batch.start < batch.end {
                snapshot_fresh = false;
            }
            // A degenerate scissor skips the geometry (images keep their own clips).
            let scissor = match batch.scissor {
                Some((bx, by, bw, bh)) => {
                    if bx >= w || by >= h || bw.min(w - bx) == 0 || bh.min(h - by) == 0 {
                        None
                    } else {
                        Some((bx, by, bw.min(w - bx), bh.min(h - by)))
                    }
                }
                None => Some((0, 0, w, h)),
            };
            let mut cursor = batch.start;
            while cursor < batch.end {
                let next_z = order.get(img_i).map(|&k| images[k].z_before).unwrap_or(u32::MAX);
                if next_z <= cursor {
                    let k = order[img_i];
                    img_i += 1;
                    draw_image(&pass, k);
                    continue;
                }
                let upto = next_z.min(batch.end);
                if let Some(s) = scissor {
                    pass.set_pipeline(&self.pipeline_2d);
                    pass.set_bind_group(0, Some(&active_group_0));
                    pass.set_bind_group_with_u32_slice_and_u32_and_dynamic_offsets_data_length(
                        1,
                        Some(&self.group_1),
                        &[(bi * WEBGPU_BLOCK_STRIDE) as u32],
                        0,
                        1,
                    )?;
                    pass.set_vertex_buffer_with_u32(0, Some(&self.vertices.buffer), 0);
                    clamp_scissor(&pass, s);
                    pass.draw_with_instance_count_and_first_vertex(upto - cursor, 1, cursor);
                }
                cursor = upto;
            }
        }
        // Images sorting after all geometry.
        while let Some(&k) = order.get(img_i) {
            img_i += 1;
            draw_image(&pass, k);
        }
        pass.set_scissor_rect(0, 0, w, h);

        // Text on top of the geometry.
        let glyph_count = self.atlas.vertices().len() as u32;
        if glyph_count > 0 {
            pass.set_pipeline(&self.glyph_pipeline);
            pass.set_bind_group(0, Some(&self.atlas_group));
            pass.set_vertex_buffer_with_u32(0, Some(&self.glyph_vertices.buffer), 0);
            pass.draw_with_instance_count_and_first_vertex(glyph_count, 1, 0);
        }
        // Overlays last, with the frame's backdrop and a zero parameter block.
        if overlay_count > 0 {
            pass.set_pipeline(&self.pipeline_2d);
            pass.set_bind_group(0, Some(&base_group_0));
            pass.set_bind_group_with_u32_slice_and_u32_and_dynamic_offsets_data_length(
                1,
                Some(&self.group_1),
                &[overlay_block * WEBGPU_BLOCK_STRIDE as u32],
                0,
                1,
            )?;
            pass.set_vertex_buffer_with_u32(0, Some(&self.vertices.buffer), 0);
            pass.draw_with_instance_count_and_first_vertex(overlay_count, 1, vertex_count);
        }
        pass.end();

        if std::mem::take(&mut self.capture_requested) {
            let row = (w * 4).div_ceil(256) * 256;
            let desc = GpuBufferDescriptor::new(row * h, buffer_usage::COPY_DST | buffer_usage::MAP_READ);
            desc.set_label("capture");
            let buffer = self.device.create_buffer(&desc)?;
            let dst = GpuTexelCopyBufferInfo::new(&buffer);
            dst.set_bytes_per_row(row);
            dst.set_rows_per_image(h);
            encoder.copy_texture_to_buffer_with_gpu_extent_3d_dict(&GpuTexelCopyTextureInfo::new(&target), &dst, &extent(w, h))?;
            self.capture = Some((buffer, w, h, row));
        }

        self.queue.submit(&[encoder.finish()]);
        Ok(())
    }
}
