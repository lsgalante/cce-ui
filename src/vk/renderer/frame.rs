//! Drawing a frame: `draw_frame_2d` (the order of its phases), immediate uploads, text
//! preparation, and uploading a frame's vertices, uniforms and parameter blocks.

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
            self.want_snapshot(&frame2d);
            let Some(image_index) = self.acquire(image_available) else {
                return false;
            };
            self.core.device.reset_fences(&[in_flight]).unwrap();
            self.upload_frame(frame_index, &frame2d);

            // Record.
            let cmd = self.frames[frame_index].cmd;
            self.core.device
                .begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())
                .unwrap();
            self.text.record_upload(&self.core.device, cmd, frame_index);
            let use_backdrop = self.record_backdrop(cmd, frame_index, image_index);
            let damage = self.frame_damage(&frame2d);
            let frost_exempt = first_frost_exempt(frame2d.batches, frame2d.images, frame2d.clear_color, use_backdrop);
            let partial = self.partial_region(&frame2d, damage, image_index, use_backdrop, frost_exempt);
            self.record_ui_pass(cmd, frame_index, image_index, &frame2d, use_backdrop, partial, frost_exempt);
            self.core.device.end_command_buffer(cmd).unwrap();

            self.submit(cmd, image_available, in_flight, image_index);
            self.age_images(image_index, damage);
            self.present(image_index, damage, partial);
            self.frame_index = (self.frame_index + 1) % FRAMES_IN_FLIGHT;
        }
        true
    }

    /// The first frame with a blur plate that will copy the frame so far allocates the
    /// snapshot it copies into. Decided the way [`Self::record_display_list`] decides, except
    /// that a scene ever staged counts as a backdrop (it may become valid while recording),
    /// which only ever errs toward allocating.
    unsafe fn want_snapshot(&mut self, frame2d: &Frame2D<'_>) {
        if !self.snapshot_wanted {
            let assume_backdrop = self.scene.wanted || self.scene.backdrop_valid;
            let exempt =
                first_frost_exempt(frame2d.batches, frame2d.images, frame2d.clear_color, assume_backdrop);
            if frame2d.batches.iter().enumerate().any(|(i, b)| b.blur_behind && Some(i) != exempt) {
                self.snapshot_wanted = true;
                self.sync_snapshot_target();
            }
        }
    }

    /// Everything this frame's slot carries to the GPU: the window-info block when the relief
    /// profiles changed, the plate carves, the display-list and overlay vertices, the text and
    /// image buffers, and the 3D and tracer uniforms.
    unsafe fn upload_frame(&mut self, frame_index: usize, frame2d: &Frame2D<'_>) {
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
    }
}
