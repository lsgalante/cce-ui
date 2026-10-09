//! A turn's render and present, the text input's sync, and the `Shell` impl the pacer drives.

use super::*;

impl<A: Application> EngineState<A> {

    pub fn render(&mut self) {
        // Grid patch: resize to the patch's buffer size, tell the app what
        // world region this frame covers, and ack — the commit this render
        // produces is the one the compositor latches at the new anchor.
        if let Some((serial, px, py, pw, ph, pscale)) = self.pending_grid_patch.take() {
            self.resize((pw * pscale) as f32, (ph * pscale) as f32);
            self.inner.as_mut().unwrap().grid_patch(px, py, pw, ph, pscale);
            if let Some(tl) = &self.cce_toplevel {
                tl.ack_grid_patch(serial);
            }
        }
        // Nothing to present on: a layer surface the app has hidden whose
        // renderer could not be rebuilt (`show_layer_surface`).
        if self.renderer.is_none() {
            return;
        }
        let logical_w = self.logical_width;
        let logical_h = self.logical_height;
        let scale_factor = self.scale_factor;

        if let Some(ref surface) = self.surface {
            if let Some(regions) = self.inner.as_ref().unwrap().input_regions() {
                // Only re-send when it actually changes. This runs per frame,
                // and a client whose region tracks its content (the desktop
                // grid's items follow every pan) would otherwise create and
                // destroy a wl_region on every frame of a camera flight.
                if self.applied_input_regions.as_deref() != Some(regions.as_slice()) {
                    let compositor = self.compositor_state.wl_compositor();
                    let wl_region = compositor.create_region(&self.qh, ());
                    for &(rx, ry, rw, rh) in &regions {
                        wl_region.add(rx, ry, rw, rh);
                    }
                    surface.set_input_region(Some(&wl_region));
                    wl_region.destroy();
                    self.applied_input_regions = Some(regions);
                }
            }
        }
        
        // The frame itself, built with no window system in it (`backend::frame`).
        let owed = self.damage_owed;
        let mut frame = build_frame(
            self.inner.as_mut().unwrap(),
            self.font_system.as_mut().unwrap(),
            LogicalSize::new(logical_w, logical_h),
            scale_factor,
            &mut self.damage_owed,
            &mut self.dl_text_items,
        );
        crate::backend::frame::derive_damage(&mut frame, &mut self.frame_record, &self.dl_text_items, owed);

        // Upload the frame's glyphs. An app without display-list text owns
        // the renderer's text state itself (it stages via stage_renderer
        // below); don't wipe it here. The renderer owns swapchain
        // rebuild/recovery.
        let renderer = self.renderer.as_mut().unwrap();
        if frame.dl_text {
            let spans = frame.text_spans(&self.dl_text_items);
            renderer.prepare_text(self.font_system.as_mut().unwrap(), &mut self.swash_cache, &spans);
        }

        // Commit the buffer scale together with a buffer it is legal for: the
        // present inside draw_frame_2d is the only commit on this surface, so
        // sending the request here orders it right before a matching-size
        // attach+commit.
        //
        // Present only the EXACT extent the current logical size and scale
        // call for. Divisibility is not enough: mid scale-transition (the
        // resume output bounce) the pending extent can belong to the other
        // scale, and an even-sized old-scale buffer divides cleanly by the
        // new scale — the commit is protocol-legal, so the compositor reads
        // it as a self-resize to half/double and reconfigures the window to
        // match (how the color editor came back from suspend at exactly half
        // size with the divisibility guard green). Odd sizes at least die
        // loudly (invalid_size). On mismatch, re-request the right extent
        // and skip — before the frame-callback request below, so the loop
        // isn't left waiting on a callback no commit will ever latch.
        if let Some(ref surface) = self.surface {
            let (s, epw, eph) =
                Self::buffer_geometry(self.scale_factor, self.logical_width, self.logical_height);
            let e = renderer.pending_extent();
            if e.width != epw || e.height != eph {
                renderer.resize(epw, eph);
                self.extent_gate_skips += 1;
                // ~5s of continuous skipping at the 16ms loop cadence: nothing
                // is presenting and nothing else will say so — this is the
                // only witness to a wedged pending extent.
                if self.extent_gate_skips.is_multiple_of(300) {
                    log::warn!(
                        "[window_runner] extent gate: pending {}x{} != expected {}x{} for {} consecutive renders; no frame is presenting",
                        e.width, e.height, epw, eph, self.extent_gate_skips,
                    );
                }
                self.redraw = true;
                return;
            }
            self.extent_gate_skips = 0;
            if s != self.committed_buffer_scale {
                surface.set_buffer_scale(s);
                self.committed_buffer_scale = s;
            }
        }

        if let Some(ref surface) = self.surface {
            let _callback = surface.frame(&self.qh, ());
            self.frame_callback_pending = true;
            self.frame_callback_armed_at = Some(std::time::Instant::now());
            if crate::vk::present_debug() {
                let t = debug_clock_ms();
                eprintln!("[vk] t={} armed frame callback", t);
            }
        }

        // Direct renderer staging (3D scenes, RT panes, app-shaped text).
        if self.inner.as_mut().unwrap().stage_renderer(
            renderer,
            LogicalSize::new(logical_w, logical_h),
            scale_factor,
        ) {
            self.redraw = true;
        }

        if !renderer.draw_frame_2d(frame.frame2d()) {
            // No present happened (swapchain out-of-date, or the created
            // swapchain didn't match the requested extent). The frame
            // callback requested above will never latch without a commit —
            // clear it or the demand-driven loop stalls waiting forever.
            self.frame_callback_pending = false;
            self.redraw = true;
        } else {
            self.damage_owed = false;
        }
        self.sync_text_input();
        // The tree after the frame that may have changed it; nothing is built while no
        // screen reader is connected.
        if let (Some(publisher), Some(app)) = (self.a11y.as_mut(), self.inner.as_mut()) {
            publisher.publish(app, crate::scale::scale_factor() as f64);
        }
    }

    /// Bring the text input in step with the frame just built: enabled at
    /// the editing widget's caret, disabled with nothing editing, reset for
    /// a composition a widget dropped (`backend::text_input`).
    pub(super) fn sync_text_input(&mut self) {
        use crate::backend::text_input::Send;
        use zwp_text_input_v3::{ContentHint, ContentPurpose};
        let reset = crate::ime::take_reset();
        let Some(ti) = self.text_input.clone() else { return };
        // Forced mode: the surface is the compositor's scale-1 space.
        let surface_scale = crate::scale::forced_scale().unwrap_or(1.0);
        let pressed = crate::ime::take_press();
        match self.text_input_state.plan(crate::ime::caret(), surface_scale, reset, pressed) {
            Send::Nothing => {}
            Send::Enable { rect: [x, y, w, h], reset } => {
                if reset {
                    ti.disable();
                    ti.commit();
                }
                ti.enable();
                ti.set_content_type(ContentHint::None, ContentPurpose::Normal);
                ti.set_cursor_rectangle(x, y, w, h);
                ti.commit();
            }
            Send::Move { rect: [x, y, w, h] } => {
                ti.set_cursor_rectangle(x, y, w, h);
                ti.commit();
            }
            Send::Disable => {
                ti.disable();
                ti.commit();
            }
        }
    }
}

impl<A: Application> Shell for EngineState<A> {
    type App = A;

    /// The driver and the app's turn, borrowed apart: every input dispatch
    /// is `let (driver, t) = self.turn(); driver.<event>(t, ..)`.
    fn turn(&mut self) -> (&mut Driver, Turn<'_, A>) {
        (
            &mut self.driver,
            Turn { app: self.inner.as_mut().unwrap(), redraw: &mut self.redraw, exit: &mut self.exit },
        )
    }

    fn app(&self) -> &A {
        self.inner.as_ref().unwrap()
    }

    fn redraw(&mut self) -> &mut bool {
        &mut self.redraw
    }

    fn exit_requested(&self) -> bool {
        self.exit
    }

    fn take_just_configured(&mut self) -> bool {
        std::mem::replace(&mut self.just_configured, false)
    }

    fn request_size(&mut self, w: u32, h: u32) {
        // desired_size is a window-frame size; the surface adds the
        // right/bottom overflow rim (0 for margin-less apps).
        let m = self.inner.as_ref().unwrap().overflow_margin() as f32;
        let (sw, sh) = (w as f32 + m, h as f32 + m);
        if (self.logical_width - sw).abs() > 0.001 || (self.logical_height - sh).abs() > 0.001 {
            self.frame_logical = (w as f32, h as f32);
            self.applied_margin = m;
            self.resize(sw, sh);
            self.redraw = true;
        }
    }

    fn sync(&mut self) {
        self.sync_surface_wanted();
        // Overflow-margin drift (configure-sized apps): the rim can change at
        // runtime — a popover overhanging the window frame — so re-derive the
        // surface from the stored frame whenever the app's answer moves. While
        // the rim is live, re-publish geometry every loop: the input region
        // tracks the animating popover rects.
        let m_now = self.inner.as_ref().unwrap().overflow_margin() as f32;
        if (m_now - self.applied_margin).abs() > 0.001 && self.frame_logical.0 > 0.0 {
            self.applied_margin = m_now;
            let (fw, fh) = self.frame_logical;
            self.resize(fw + m_now, fh + m_now);
            self.redraw = true;
        }
        if self.applied_margin > 0.0 || self.overflow_was_active {
            self.publish_window_geometry();
            self.overflow_was_active = self.applied_margin > 0.0;
        }
        self.send_popover_region();
        self.sync_menu_popup();
    }

    fn set_title(&mut self, title: &str) {
        if let Some(ref window) = self.window {
            window.set_title(title);
            window.commit();
        }
    }

    fn frame_pending(&mut self) -> bool {
        // Frame-callback starvation fallback: the compositor only sends
        // frame-done for surfaces it actually renders, so a callback armed
        // while the window sat off-viewport (or the scene went static) may
        // never fire — and the vsync gate then freezes the app forever
        // with a perfectly live event loop (input processes, state changes,
        // nothing repaints). If a redraw has been waiting on a callback well
        // past any real vsync interval, stop waiting and draw.
        //
        // Gated on the renderer's present mode: forcing a present past a
        // dead callback is only safe under MAILBOX (the present replaces the
        // queued buffer). Under FIFO the driver's throttle waits on the
        // previous present's frame event, so the forced present itself
        // blocks forever inside the driver — the exact freeze this fallback
        // exists to prevent. There the gate stays closed: pixels may stale
        // until the next frame-done/configure, but the loop stays alive.
        if self.redraw
            && self.frame_callback_pending
            && self.renderer.as_ref().is_some_and(|r| r.forced_present_safe())
            && self.frame_callback_armed_at.is_none_or(|t| t.elapsed().as_millis() > 250)
        {
            self.frame_callback_pending = false;
            if crate::vk::present_debug() {
                let t = debug_clock_ms();
                eprintln!("[vk] t={} starvation fallback fired (callback never came)", t);
            }
        }
        self.frame_callback_pending
    }

    fn configured(&self) -> bool {
        self.first_configure_received
    }

    fn present(&mut self, fresh: bool) {
        if !fresh {
            // A warm-down step: the window alone, and nothing drawn — the
            // pixels have not changed. An occluded surface gets no callbacks;
            // past 250 ms stop waiting for one, as the starvation fallback does
            // for a real frame.
            let waiting = self.keepalive_pending
                && self.keepalive_armed_at.is_some_and(|t| t.elapsed().as_millis() < 250);
            if !waiting {
                self.keepalive_commit();
            }
            return;
        }
        // A menu handed over from the window commits first, so
        // there is no moment with neither (`take_menu_popup_lead`).
        let lead = self.take_menu_popup_lead();
        if lead {
            self.render_menu_popup();
        }
        self.render();
        if !lead {
            self.render_menu_popup();
        }
    }
}
