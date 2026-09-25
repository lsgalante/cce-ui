//! The global context menu in its own `xdg_popup` surface.
//!
//! Every app paints [`context_menu`] into its own display list and routes the
//! pointer to it by window coordinates. Drawn in the window, a menu opened
//! near an edge is cut off at the window's edge — and the window is the only
//! room it has. Here the runner mirrors the open menu into a popup surface
//! parented to the window, which the compositor may place anywhere on the
//! output and, through the positioner's constraint adjustment, keeps ON the
//! output: it flips the menu to open upward, slides it in from an edge, or
//! cuts it short, in which case the rows scroll.
//!
//! Two rules make this safe, and they are the two things the previous popup
//! path (deleted in July 2026 as Phase 6x) got wrong:
//!
//! - **The compositor's placement is written back into the menu.** The popup
//!   lands where the configure says, not where the menu asked, and
//!   `context_menu::place` moves the menu's rect there. The rect every app
//!   hit-tests against is therefore the rect on screen — the anchor-mismatch
//!   bugs the old path had (a menu drawn in one place and clicked in another)
//!   cannot happen.
//! - **The popup takes its own input, translated into window coordinates.**
//!   The old popup had an empty input region and let clicks fall through to
//!   the window beneath, which only works where there IS window beneath. A
//!   pointer event on the popup is offset by the popup's position and handed
//!   to the app as if it had landed on the window, so apps need no change:
//!   their menu dispatch already works in window coordinates, which may now
//!   lie outside the window.
//!
//! While the popup is up the menu is `hosted`, which turns the apps' own
//! in-window paint calls into no-ops. The popup has no keyboard grab: the
//! keyboard stays with the window, whose Escape and press-outside handling
//! close the menu as before.
//!
//! Only xdg toplevels get a popup. A layer surface, or any app with
//! `CCE_UI_MENU_POPUP=0`, keeps the in-window menu, constrained to the window
//! by `context_menu::constrain_to` with the same flip / slide / shorten rules.

use smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_positioner::{
    Anchor, ConstraintAdjustment, Gravity,
};
use smithay_client_toolkit::shell::xdg::popup::{Popup, PopupConfigure, PopupHandler};
use smithay_client_toolkit::shell::xdg::{XdgPositioner, XdgSurface as _};
use wayland_client::{protocol::wl_surface, Connection, Proxy, QueueHandle};

use super::window_runner::{
    collect_dl_text, dl_batches_2d, dl_text_spans, tessellate_display_list, Application,
    EngineState, TextBounds,
};
use crate::vk::{Frame2D, VkRenderer};
use crate::widget::context_menu;

pub struct MenuPopup {
    popup: Popup,
    /// What the popup was opened for: the menu's generation and its natural
    /// size, rounded up. A re-show or a change of size opens a new popup.
    key: (u64, u32, u32),
    /// Where the compositor put it, in app-logical px relative to the
    /// window's geometry: `(x, y, w, h)`. `None` until the first configure,
    /// before which nothing may be attached to the surface.
    placed: Option<(f32, f32, f32, f32)>,
    /// The buffer scale last sent on the popup's surface.
    committed_scale: i32,
}

impl MenuPopup {
    /// The popup's offset from the window, if `surface` is its surface and it
    /// has been placed — what a pointer event on it is translated by.
    pub fn offset_for(&self, surface: &wl_surface::WlSurface) -> Option<(f32, f32)> {
        if self.popup.wl_surface() != surface {
            return None;
        }
        self.placed.map(|(x, y, _, _)| (x, y))
    }
}

fn popup_enabled() -> bool {
    std::env::var("CCE_UI_MENU_POPUP").map_or(true, |v| v != "0")
}

/// App-logical px to the compositor's surface coordinates: the same thing
/// except in forced-scale mode, where the compositor believes scale 1 and
/// the app's logical px are `forced` of its own.
fn forced() -> f32 {
    crate::scale::forced_scale().map(|f| f as f32).unwrap_or(1.0)
}

impl<A: Application> EngineState<A> {
    /// The window frame's logical size: the surface minus the overflow rim.
    fn frame_size(&self) -> (f32, f32) {
        (
            (self.logical_width - self.applied_margin).max(1.0),
            (self.logical_height - self.applied_margin).max(1.0),
        )
    }

    /// Bring the popup in line with the menu: open one for a newly shown
    /// menu, close it for a hidden one. Runs once per loop, after input, so
    /// a host that shows the menu and then sets its slider rows (which widen
    /// it) has done both before the popup is sized.
    pub(crate) fn sync_menu_popup(&mut self) {
        let visible = context_menu::is_visible();
        if !(visible && self.window.is_some() && popup_enabled()) {
            if self.menu_popup.is_some() {
                self.close_menu_popup();
            }
            context_menu::set_hosted(false);
            if visible {
                let (fw, fh) = self.frame_size();
                context_menu::constrain_to(0.0, 0.0, fw, fh);
            }
            return;
        }
        let (anchor, w, content_h) = context_menu::natural_geometry();
        let key = (context_menu::generation(), w.ceil() as u32, content_h.ceil() as u32);
        if self.menu_popup.as_ref().is_some_and(|p| p.key == key) {
            return;
        }
        self.close_menu_popup();
        self.open_menu_popup(anchor, key);
    }

    fn open_menu_popup(&mut self, anchor: (f32, f32), key: (u64, u32, u32)) {
        let Some(window) = self.window.as_ref() else { return };
        let positioner = match XdgPositioner::new(&self.xdg_shell_state) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("[menu_popup] no positioner ({e}); drawing the menu in the window");
                return;
            }
        };
        let f = forced();
        positioner.set_size(
            ((key.1 as f32) * f).round().max(1.0) as i32,
            ((key.2 as f32) * f).round().max(1.0) as i32,
        );
        // A 1x1 anchor at the point the menu was opened at, held inside the
        // window's geometry (the rect the positioner is relative to).
        let (fw, fh) = self.frame_size();
        let ax = (anchor.0.clamp(0.0, fw - 1.0) * f).round() as i32;
        let ay = (anchor.1.clamp(0.0, fh - 1.0) * f).round() as i32;
        positioner.set_anchor_rect(ax, ay, 1, 1);
        positioner.set_anchor(Anchor::TopLeft);
        positioner.set_gravity(Gravity::BottomRight);
        // Open down and right from the pointer. Short of room below, open UP
        // from it (flip); short either way, slide in from the edge; taller
        // than the output, cut it down (resize) — the menu scrolls.
        positioner.set_constraint_adjustment(
            ConstraintAdjustment::FlipY
                | ConstraintAdjustment::SlideX
                | ConstraintAdjustment::SlideY
                | ConstraintAdjustment::ResizeY,
        );
        let popup = match Popup::new(
            window.xdg_surface(),
            &positioner,
            &self.qh,
            &self.compositor_state,
            &self.xdg_shell_state,
        ) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("[menu_popup] cannot create popup ({e}); drawing the menu in the window");
                return;
            }
        };
        // Hosted from now: until the first configure places it, the menu is
        // drawn nowhere — a few milliseconds, against a copy in the window
        // that would blink out when the popup appears somewhere else.
        context_menu::set_hosted(true);
        self.menu_popup = Some(MenuPopup { popup, key, placed: None, committed_scale: 0 });
    }

    /// Close the popup. The renderer lets go of the surface FIRST: dropping
    /// the popup destroys the `wl_surface`, and a swapchain must never
    /// outlive the surface it presents to.
    pub(crate) fn close_menu_popup(&mut self) {
        if let Some(renderer) = self.menu_renderer.as_mut() {
            if renderer.has_surface() {
                renderer.detach_surface();
            }
        }
        self.menu_popup = None;
        context_menu::set_hosted(false);
    }

    /// Draw the menu into its popup. Called after the window's own frame,
    /// and after a configure; a no-op until the popup is placed.
    pub(crate) fn render_menu_popup(&mut self) {
        let Some(mp) = self.menu_popup.as_mut() else { return };
        let Some((_, _, w, h)) = mp.placed else { return };
        let Some(renderer) = self.menu_renderer.as_mut() else { return };
        if !renderer.has_surface() {
            return;
        }
        let scale = self.scale_factor as f32;
        let (s, pw, ph) = Self::buffer_geometry(self.scale_factor, w, h);

        let mut pc = crate::scene::paint::PaintCtx::new();
        context_menu::paint_hosted(&mut pc);
        let dl = pc.finish();

        let mut items = Vec::new();
        collect_dl_text(self.font_system.as_mut().unwrap(), &dl, &mut items);
        let (verts, dl_batches, _images, plate_features) = tessellate_display_list(&dl, w, h, scale);
        let bounds = TextBounds { left: 0, top: 0, right: pw as i32, bottom: ph as i32 };
        let spans = dl_text_spans(&items, scale, bounds, &[]);
        renderer.prepare_text(self.font_system.as_mut().unwrap(), &mut self.swash_cache, &spans);
        let batches = dl_batches_2d(&dl_batches, scale);

        let e = renderer.pending_extent();
        if e.width != pw || e.height != ph {
            renderer.resize(pw, ph);
        }
        if s != mp.committed_scale {
            mp.popup.wl_surface().set_buffer_scale(s);
            mp.committed_scale = s;
        }
        renderer.draw_frame_2d(Frame2D {
            verts: &verts,
            batches: &batches,
            overlay_verts: &[],
            images: &[],
            plate_features: &plate_features,
            clear_color: [0.0; 4],
        });
    }
}

impl<A: Application> PopupHandler for EngineState<A> {
    fn configure(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, popup: &Popup, config: PopupConfigure) {
        let Some(mp) = self.menu_popup.as_mut() else { return };
        if mp.popup.wl_surface() != popup.wl_surface() {
            return;
        }
        let f = forced();
        let (x, y) = (config.position.0 as f32 / f, config.position.1 as f32 / f);
        let (w, h) = (config.width.max(1) as f32 / f, config.height.max(1) as f32 / f);
        mp.placed = Some((x, y, w, h));
        // Where it landed IS where the menu is — see the module docs.
        context_menu::place(x, y, h);

        let (_, pw, ph) = Self::buffer_geometry(self.scale_factor, w, h);
        let surface_ptr = mp.popup.wl_surface().id().as_ptr() as *mut std::ffi::c_void;
        let display_ptr = self.display_ptr as *mut std::ffi::c_void;
        match self.menu_renderer.as_mut() {
            Some(r) if r.has_surface() => r.resize(pw, ph),
            Some(r) => unsafe { r.attach_surface(display_ptr, surface_ptr, pw, ph) },
            None => {
                let t = std::time::Instant::now();
                self.menu_renderer = Some(unsafe { VkRenderer::new(display_ptr, surface_ptr, pw, ph, 0.0) });
                log::debug!("[menu_popup] renderer created in {:?}", t.elapsed());
            }
        }
        self.redraw = true;
        self.render_menu_popup();
    }

    fn done(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, popup: &Popup) {
        // The compositor dismissed it (its parent went away, say). The menu
        // closes with it; an app that watches `is_visible` sees that.
        if self.menu_popup.as_ref().is_some_and(|mp| mp.popup.wl_surface() == popup.wl_surface()) {
            context_menu::hide();
            self.close_menu_popup();
            self.redraw = true;
        }
    }
}

smithay_client_toolkit::delegate_xdg_popup!(@<A: Application> EngineState<A>);
