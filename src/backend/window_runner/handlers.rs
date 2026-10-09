//! The SCTK handlers and their delegates: compositor, output, shm, registry, window, layer shell,
//! seat; the region and frame-callback dispatches.

use super::*;

impl<A: Application> CompositorHandler for EngineState<A> {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        scale_factor: i32,
    ) {
        // Don't send set_buffer_scale here: an in-flight present can commit an
        // old-scale-sized buffer right after it, which is a fatal invalid_size
        // protocol error (seen on resume, when outputs bounce 2→1→2). The scale
        // request is sent in `render`, paired with a matching-size present.
        if crate::scale::forced_scale().is_some() {
            // Forced mode: the compositor's opinion (scale 1 under cage) must
            // not clobber the override.
            return;
        }
        if self.inner.as_ref().is_some_and(|a| a.grid()) {
            // Grid surfaces stay at scale 1 — patch.scale is the sole
            // resolution authority (see the pin at surface creation).
            return;
        }
        // Resume bounce: when the surface sits on no LIVE output (the DRM
        // connector was destroyed and not yet re-created), the reported
        // factor is SCTK's no-outputs fallback, not information — hold the
        // last real scale. When the reborn output arrives, surface enter
        // recomputes and this handler runs again with a live output backing
        // it. Liveness matters (not just enter/leave counting): the leave
        // for a destroyed output may never be delivered.
        let on_live_output = self
            .entered_outputs
            .iter()
            .any(|o| self.output_state.info(o).is_some());
        if !on_live_output && (scale_factor as f64) < self.scale_factor {
            return;
        }
        self.scale_factor = scale_factor as f64;
        self.resize(self.logical_width, self.logical_height);
        self.redraw = true;
    }
    
    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {}
    
    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {}
    
    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        if !self.entered_outputs.contains(output) {
            self.entered_outputs.push(output.clone());
        }
        // Dead entries (destroyed outputs never send leave) are harmless —
        // the liveness check in scale_factor_changed skips them — but drop
        // them here so the list doesn't grow across suspend cycles.
        self.entered_outputs
            .retain(|o| self.output_state.info(o).is_some());
        self.redraw = true;
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        self.entered_outputs.retain(|o| o != output);
    }
}

impl<A: Application> OutputHandler for EngineState<A> {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    
    fn new_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {
        let scale = crate::wayland::detect_scale_factor(&self.output_state);
        crate::scale::set_scale_factor(scale as f32);
        crate::window_state::set_metric(crate::wayland::detect_metric(&self.output_state, scale));
    }
    fn update_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {
        let scale = crate::wayland::detect_scale_factor(&self.output_state);
        crate::scale::set_scale_factor(scale as f32);
        crate::window_state::set_metric(crate::wayland::detect_metric(&self.output_state, scale));
    }
    fn output_destroyed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {}
}

impl<A: Application> ShmHandler for EngineState<A> {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm_state
    }
}

impl<A: Application> ProvidesRegistryState for EngineState<A> {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    
    fn runtime_add_global(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _name: u32,
        _interface: &str,
        _version: u32,
    ) {}
    
    fn runtime_remove_global(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _name: u32,
        _interface: &str,
    ) {}
}

impl<A: Application> WindowHandler for EngineState<A> {
    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _window: &XdgWindow,
        configure: WindowConfigure,
        _serial: u32,
    ) {
        let is_fs = configure.is_fullscreen();
        let is_max = configure.is_maximized();
        crate::scale::set_fullscreen(is_fs);
        crate::scale::set_maximized(is_max);

        let (w, h) = configure.new_size;
        // Configure sizes are window-geometry sizes; with an overflow margin
        // the surface is a rim larger on the right and bottom.
        let m = self.inner.as_ref().unwrap().overflow_margin() as f32;
        if let (Some(w), Some(h)) = (w, h) {
            let width = w.get();
            let height = h.get();
            // Forced mode: the compositor's logical size is really physical
            // pixels (scale-1 output); divide to get the app's logical space.
            let f = crate::scale::forced_scale().unwrap_or(1.0);
            self.frame_logical = (width as f32 / f, height as f32 / f);
            self.applied_margin = m;
            self.resize(width as f32 / f + m, height as f32 / f + m);
        } else if self.inner.as_ref().unwrap().grid() && self.logical_width > 1.0 {
            // A grid app's size belongs to its PATCHES: the compositor's
            // "you choose" 0x0 must not bounce the surface back to the
            // settings size — that thrash recreated multi-hundred-MB
            // swapchains per bounce (6.3G peak in 10s). Keep the current
            // size; the next grid_patch is the only resizer.
        } else {
            let settings = self.inner.as_ref().unwrap().settings();
            self.frame_logical = (settings.width as f32, settings.height as f32);
            self.applied_margin = m;
            self.resize(settings.width as f32 + m, settings.height as f32 + m);
        }
        self.redraw = true;
        self.frame_callback_pending = false;
        self.first_configure_received = true;
        self.just_configured = true;
    }

    fn request_close(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _window: &XdgWindow) {
        self.exit = true;
    }
}

impl<A: Application> LayerShellHandler for EngineState<A> {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        // new_size is in logical pixels; 0 means "client decides", so fall back
        // to the app's requested size (mirrors the xdg WindowHandler above).
        let (w, h) = configure.new_size;
        if w > 0 && h > 0 {
            self.resize(w as f32, h as f32);
        } else {
            let settings = self.inner.as_ref().unwrap().settings();
            self.resize(settings.width as f32, settings.height as f32);
        }
        self.redraw = true;
        self.frame_callback_pending = false;
        self.first_configure_received = true;
        self.just_configured = true;
    }
}

impl<A: Application> SeatHandler for EngineState<A> {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    
    fn new_seat(&mut self, _conn: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        self.ensure_data_device(qh, &seat);
        self.seats.push(seat);
    }
    
    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        // Every seat arrives here, unlike `new_seat` — SCTK binds the seats
        // that already exist at startup without announcing them, so a device
        // created only there is never created at all on a normal launch.
        self.ensure_data_device(qh, &seat);
        if capability == Capability::Pointer && self.pointer.is_none() {
            let surface = self.compositor_state.create_surface::<Self>(qh);
            let themed_pointer = self.seat_state.get_pointer_with_theme(
                qh,
                &seat,
                self.shm_state.wl_shm(),
                surface,
                ThemeSpec::System,
            ).unwrap();
            if let Some(ref pg) = self.pointer_gestures {
                self.pinch_gesture = Some(pg.get_pinch_gesture(themed_pointer.pointer(), qh, ()));
            }
            self.pointer = Some(themed_pointer);
        }
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            let keyboard = self.seat_state.get_keyboard(qh, &seat, None).unwrap();
            self.keyboard = Some(keyboard);
            if let (Some(m), None) = (&self.text_input_manager, &self.text_input) {
                self.text_input = Some(m.get_text_input(&seat, qh, ()));
            }
        }
        if capability == Capability::Touch && self.touch.is_none() {
            self.touch = self.seat_state.get_touch(qh, &seat).ok();
        }
    }
    
    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer {
            self.pinch_gesture = None;
            self.pointer = None;
        }
        if capability == Capability::Keyboard {
            self.keyboard = None;
            self.text_input = None;
        }
        if capability == Capability::Touch {
            self.touch_lost();
        }
    }
    
    fn remove_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        self.seats.retain(|s| s != &seat);
    }
}

impl<A: Application> wayland_client::Dispatch<wl_registry::WlRegistry, GlobalList, Self> for EngineState<A> {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalList,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}
}

delegate_compositor!(@<A: Application> EngineState<A>);

delegate_xdg_shell!(@<A: Application> EngineState<A>);

delegate_xdg_window!(@<A: Application> EngineState<A>);

delegate_layer!(@<A: Application> EngineState<A>);

delegate_shm!(@<A: Application> EngineState<A>);

delegate_seat!(@<A: Application> EngineState<A>);

delegate_pointer!(@<A: Application> EngineState<A>);

delegate_keyboard!(@<A: Application> EngineState<A>);

delegate_registry!(@<A: Application> EngineState<A>);

delegate_output!(@<A: Application> EngineState<A>);

impl<A: Application> wayland_client::Dispatch<wl_region::WlRegion, ()> for EngineState<A> {
    fn event(
        _state: &mut Self,
        _proxy: &wl_region::WlRegion,
        _event: wl_region::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}
}

impl<A: Application> wayland_client::Dispatch<wl_callback::WlCallback, ()> for EngineState<A> {
    fn event(
        state: &mut Self,
        _proxy: &wl_callback::WlCallback,
        event: wl_callback::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event {
            state.frame_callback_pending = false;
            if crate::vk::present_debug() {
                let t = debug_clock_ms();
                let waited = state.frame_callback_armed_at.map(|a| a.elapsed().as_millis()).unwrap_or(0);
                eprintln!("[vk] t={} frame-done (waited {}ms)", t, waited);
            }
        }
    }
}
