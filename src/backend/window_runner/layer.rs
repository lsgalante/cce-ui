//! A layer surface while the app wants none (`Application::wants_surface`): the renderer
//! detached and the surface dropped, then a fresh surface with the same renderer.

use super::*;

impl<A: Application> EngineState<A> {
    /// Give `surface` its layer-shell role from `ls` at `width` x `height`
    /// and commit, which asks the compositor for the first configure. The
    /// session start and [`Self::show_layer_surface`] share this.
    pub(super) fn attach_layer_role(&mut self, surface: &wl_surface::WlSurface, ls: &LayerSettings, width: u32, height: u32) {
        let layer_shell = self
            .layer_shell_state
            .as_ref()
            .expect("compositor does not support wlr-layer-shell");
        let layer_surface = layer_shell.create_layer_surface(
            &self.qh,
            surface.clone(),
            ls.layer,
            Some(ls.namespace.clone()),
            None,
        );
        layer_surface.set_anchor(ls.anchor);
        layer_surface.set_exclusive_zone(ls.exclusive_zone);
        layer_surface.set_keyboard_interactivity(ls.keyboard_interactivity);
        let (t, r, b, l) = ls.margin;
        layer_surface.set_margin(t, r, b, l);
        layer_surface.set_size(width, height);
        layer_surface.commit();
        self.layer_surface = Some(layer_surface);
    }

    /// Follow [`Application::wants_surface`]: tear the layer surface down
    /// when the app has nothing to show, build it again when it does. Once a
    /// loop turn, before the present decision.
    pub(super) fn sync_surface_wanted(&mut self) {
        if !self.is_layer_app {
            return;
        }
        let want = self.inner.as_ref().unwrap().wants_surface();
        if !want && !self.layer_hidden {
            self.hide_layer_surface();
        } else if want && self.layer_hidden {
            self.show_layer_surface();
        }
    }

    /// Let the renderer go of the surface (its swapchain and `VkSurfaceKHR`;
    /// the device, pipelines, atlases and image table stay), then destroy the
    /// layer surface — SCTK destroys the role and then the `wl_surface`, which
    /// must not happen while a swapchain still presents to it.
    pub(super) fn hide_layer_surface(&mut self) {
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.detach_surface();
        }
        self.layer_surface = None;
        self.surface = None;
        self.layer_hidden = true;
        self.first_configure_received = false;
        self.frame_callback_pending = false;
        self.keepalive_pending = false;
        self.redraw = false;
        self.entered_outputs.clear();
        self.applied_input_regions = None;
        log::info!("[window_runner] nothing to show; layer surface unmapped");
    }

    /// A fresh `wl_surface` with the app's layer role, and the renderer moved
    /// onto it (`attach_surface`: one swapchain, where a new renderer costs a
    /// device and every pipeline). The first configure then makes it
    /// presentable, exactly as at session start. The renderer is the same one,
    /// so its image ids are still good and `renderer_init` is not called; only
    /// where there is none (an attach that failed before) is one made, and
    /// that one is announced. A surface nothing can draw to stays hidden.
    pub(super) fn show_layer_surface(&mut self) {
        let app = self.inner.as_ref().unwrap();
        let settings = app.settings();
        let Some(ls) = app.layer() else { return };
        let surface = self.compositor_state.create_surface(&self.qh);
        let buffer_scale = if crate::scale::forced_scale().is_some() { 1 } else { self.scale_factor as i32 };
        surface.set_buffer_scale(buffer_scale);
        self.committed_buffer_scale = buffer_scale;
        self.attach_layer_role(&surface, &ls, settings.width, settings.height);

        let s = self.scale_factor as f32;
        let (pw, ph) = ((settings.width as f32 * s) as u32, (settings.height as f32 * s) as u32);
        let display_ptr = self.display_ptr as *mut std::ffi::c_void;
        let surface_ptr = surface.id().as_ptr() as *mut std::ffi::c_void;
        self.surface = Some(surface);
        let made = match self.renderer.as_mut() {
            Some(renderer) => match unsafe { renderer.attach_surface(display_ptr, surface_ptr, pw, ph) } {
                Ok(()) => Ok(false),
                Err(lost) => Err(lost),
            },
            None => unsafe { VkRenderer::try_new(display_ptr, surface_ptr, pw, ph, 0.0) }.map(|renderer| {
                self.renderer = Some(renderer);
                true
            }),
        };
        let made = match made {
            Ok(made) => made,
            Err(lost) => {
                log::error!("[window_runner] cannot draw to the new surface, staying unmapped: {lost}");
                self.renderer = None;
                self.layer_surface = None;
                self.surface = None;
                return;
            }
        };
        self.logical_width = settings.width as f32;
        self.logical_height = settings.height as f32;
        self.layer_hidden = false;
        self.redraw = true;
        log::info!(
            "[window_runner] layer surface mapped again ({})",
            if made { "a new renderer" } else { "the renderer moved onto it" }
        );
        if made {
            self.inner.as_mut().unwrap().renderer_init(self.renderer.as_mut().unwrap());
        }
    }

    /// One warm-down step: a frame callback and a commit with no buffer, so
    /// the compositor keeps servicing this surface's callbacks at vsync
    /// (sparse commits were measured getting theirs 22-128 ms late) while
    /// nothing is drawn, uploaded or re-composited. wlroots schedules an
    /// output frame for a commit that asks for a callback, so it arrives
    /// without any damage.
    pub(super) fn keepalive_commit(&mut self) {
        let Some(ref surface) = self.surface else { return };
        let _callback = surface.frame(&self.qh, KeepAlive);
        surface.commit();
        self.keepalive_pending = true;
        self.keepalive_armed_at = Some(std::time::Instant::now());
        if crate::vk::present_debug() {
            eprintln!("[vk] t={} armed keepalive callback", debug_clock_ms());
        }
    }
}
