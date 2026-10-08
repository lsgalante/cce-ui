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
//! **A page turn is a new popup at the old one's corner** (2026-10-02,
//! where until then a row's submenu was a second popup, the CHILD of this
//! one, flying out beside it). `context_menu::show_page` puts the page's
//! top-left where the menu's was and marks the menu `turned`; its generation
//! moves, and the popup that is up is REPOSITIONED (`xdg_popup.reposition`)
//! to the page's size, anchored at that corner, rather than replaced: a new
//! surface under a pointer that has not moved gets no pointer focus until it
//! moves, so a swipe that turned the page, and the swipe back, would land on
//! nothing. Its positioner slides the page on screen rather than flipping it
//! to open up from the corner — a page that jumped above the plate it
//! replaced would not read as that plate turned.
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
    /// The menu is still drawn in the window until this popup is placed —
    /// see `open_menu_popup`.
    handoff: bool,
    /// Placed after a hand-off and not drawn yet: the next frame draws it
    /// AHEAD of the window's (`take_menu_popup_lead`).
    lead: bool,
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
    crate::scale::forced_scale().unwrap_or(1.0)
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
    /// menu, close it for a hidden one. Runs
    /// once per loop, after input, so a host that shows the menu and then
    /// sets its slider rows (which widen it) has done both before the popup
    /// is sized.
    pub(crate) fn sync_menu_popup(&mut self) {
        let visible = context_menu::is_visible();
        // A page turn is animated: frames until it lands, and the popup is
        // sized to hold both plates meanwhile (`natural_geometry`), then to
        // the page's own once it has.
        if visible && context_menu::is_turning() {
            self.redraw = true;
        }
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
        // A page turned to in place of the menu MOVES the popup that is up
        // rather than replacing it: a new surface under a pointer that has
        // not moved gets no pointer focus until it does, so the rest of a
        // swipe — and the swipe back — would land on nothing.
        if context_menu::is_turned() && self.reposition_menu_popup(anchor, key) {
            return;
        }
        self.close_menu_popup();
        self.open_menu_popup(anchor, key);
    }

    /// Move and resize the popup that is up to show a turned page, through
    /// `xdg_popup.reposition` (version 3). `false` where it cannot be: no
    /// popup placed yet, an older protocol, no positioner.
    fn reposition_menu_popup(&mut self, anchor: (f32, f32), key: (u64, u32, u32)) -> bool {
        let Some(mp) = self.menu_popup.as_ref() else { return false };
        if mp.placed.is_none() || mp.popup.xdg_popup().version() < 3 {
            return false;
        }
        let Some(positioner) = self.menu_positioner(anchor, key) else { return false };
        // Configures as the popup is moved, so the token is not read back.
        let mp = self.menu_popup.as_mut().unwrap();
        mp.popup.reposition(&positioner, key.0 as u32);
        mp.key = key;
        self.redraw = true;
        true
    }

    /// The positioner the menu's popup is placed by, sized to `key`'s width
    /// and height and anchored at `anchor`.
    fn menu_positioner(&self, anchor: (f32, f32), key: (u64, u32, u32)) -> Option<XdgPositioner> {
        let positioner = match XdgPositioner::new(&self.xdg_shell_state) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("[menu_popup] no positioner ({e}); drawing the menu in the window");
                return None;
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
        // A page keeps the corner of the plate it turned from: no flip.
        let flip = if context_menu::is_turned() { ConstraintAdjustment::empty() } else { ConstraintAdjustment::FlipY };
        positioner.set_constraint_adjustment(
            flip | ConstraintAdjustment::SlideX | ConstraintAdjustment::SlideY | ConstraintAdjustment::ResizeY,
        );
        Some(positioner)
    }

    fn open_menu_popup(&mut self, anchor: (f32, f32), key: (u64, u32, u32)) {
        let Some(positioner) = self.menu_positioner(anchor, key) else { return };
        let Some(window) = self.window.as_ref() else { return };
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
        // A menu opened at the pointer is hosted from now: until the first
        // configure places it, it is drawn nowhere — a few milliseconds,
        // against a copy in the window that would blink out when the popup
        // appears somewhere else.
        //
        // A TURNED page is not: it takes the place of a plate that was just
        // on screen (the designer's dialog, which closed in the same
        // dispatch), and drawn nowhere it was a frame of nothing between the
        // two. Its place is known — the corner — so the window keeps drawing
        // it until the popup is placed, and the frame after the configure
        // commits the popup FIRST and then the window without it: for the
        // time the window's frame takes to draw, the two stand one over the
        // other at one place, where the other order left neither.
        let handoff = context_menu::is_turned();
        context_menu::set_hosted(!handoff);
        self.menu_popup = Some(MenuPopup { popup, key, placed: None, committed_scale: 0, handoff, lead: false });
    }

    /// Close the popup. The renderer lets go
    /// of the surface FIRST: dropping the popup destroys the `wl_surface`,
    /// and a swapchain must never outlive the surface it presents to.
    pub(crate) fn close_menu_popup(&mut self) {
        if let Some(renderer) = self.menu_renderer.as_mut() {
            if renderer.has_surface() {
                renderer.detach_surface();
            }
        }
        self.menu_popup = None;
        context_menu::set_hosted(false);
    }

    /// Whether the next frame draws the popup ahead of the window's own: once,
    /// for the first frame after a hand-off (see `open_menu_popup`).
    pub(crate) fn take_menu_popup_lead(&mut self) -> bool {
        self.menu_popup.as_mut().is_some_and(|p| std::mem::take(&mut p.lead))
    }

    /// The offset from the window of the menu popup, if `surface` is it.
    pub(crate) fn menu_popup_offset(&self, surface: &wl_surface::WlSurface) -> Option<(f32, f32)> {
        self.menu_popup.as_ref().and_then(|p| p.offset_for(surface))
    }

    /// Draw the menu into its popup. Called after the window's own frame,
    /// and after a configure; a no-op for a popup not yet placed.
    pub(crate) fn render_menu_popup(&mut self) {
        let (mp, renderer) = (self.menu_popup.as_mut(), self.menu_renderer.as_mut());
        let Some(mp) = mp else { return };
        let Some((_, _, w, h)) = mp.placed else { return };
        let Some(renderer) = renderer else { return };
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
        let (verts, dl_batches, dl_images, plate_features) = tessellate_display_list(&dl, w, h, scale);
        let bounds = TextBounds { left: 0, top: 0, right: pw as i32, bottom: ph as i32 };
        let spans = dl_text_spans(&items, scale, bounds, &[]);
        renderer.prepare_text(self.font_system.as_mut().unwrap(), &mut self.swash_cache, &spans);
        let batches = dl_batches_2d(&dl_batches, scale);
        // The menu's glyphs (its marks, its page and back chevrons) were
        // painted with the WINDOW renderer's ids; this renderer keeps images
        // of its own, so each is drawn by its copy here, uploaded once.
        let icon_ids = &mut self.menu_icon_ids;
        let images: Vec<crate::vk::ImageQuad> = dl_images
            .iter()
            .filter_map(|di| {
                let own = *icon_ids.entry(di.image).or_insert_with(|| {
                    let (name, px, tint) = crate::icon_source(di.image)?;
                    let (rgba, iw, ih) = crate::icon_pixels(&name, px, tint)?;
                    Some(renderer.upload_rgba_now(&rgba, iw, ih))
                });
                Some(crate::vk::ImageQuad {
                    image: own?,
                    rect: (di.rect.x * scale, di.rect.y * scale, di.rect.width * scale, di.rect.height * scale),
                    alpha: di.alpha,
                    z_before: di.at,
                    clip: di.clip.map(|c| {
                        ((c.x * scale).max(0.0) as u32, (c.y * scale).max(0.0) as u32, (c.width * scale) as u32, (c.height * scale) as u32)
                    }),
                })
            })
            .collect();

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
            images: &images,
            plate_features: &plate_features,
            clear_color: [0.0; 4],
            damage: None,
        });
    }

    fn is_menu_popup(&self, surface: &wl_surface::WlSurface) -> bool {
        self.menu_popup.as_ref().is_some_and(|p| p.popup.wl_surface() == surface)
    }
}

impl<A: Application> PopupHandler for EngineState<A> {
    fn configure(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, popup: &Popup, config: PopupConfigure) {
        if !self.is_menu_popup(popup.wl_surface()) {
            return;
        }
        let f = forced();
        let (x, y) = (config.position.0 as f32 / f, config.position.1 as f32 / f);
        let (w, h) = (config.width.max(1) as f32 / f, config.height.max(1) as f32 / f);
        let (scale_factor, display) = (self.scale_factor, self.display_ptr);
        let (mp, slot, icon_ids) = (self.menu_popup.as_mut(), &mut self.menu_renderer, &mut self.menu_icon_ids);
        let Some(mp) = mp else { return };
        mp.placed = Some((x, y, w, h));
        let handoff = std::mem::take(&mut mp.handoff);
        // Where it landed IS where the menu is — see the module docs.
        context_menu::place(x, y, h);
        if handoff {
            context_menu::set_hosted(true);
            mp.lead = true;
        }

        let (_, pw, ph) = Self::buffer_geometry(scale_factor, w, h);
        let surface_ptr = mp.popup.wl_surface().id().as_ptr() as *mut std::ffi::c_void;
        let display_ptr = display as *mut std::ffi::c_void;
        let attached = match slot.as_mut() {
            Some(r) if r.has_surface() => {
                r.resize(pw, ph);
                Ok(())
            }
            Some(r) => unsafe { r.attach_surface(display_ptr, surface_ptr, pw, ph) },
            None => {
                let t = std::time::Instant::now();
                let made = unsafe { VkRenderer::try_new(display_ptr, surface_ptr, pw, ph, 0.0) };
                log::debug!("[menu_popup] renderer created in {:?}", t.elapsed());
                // A new renderer holds none of the old one's copies.
                icon_ids.clear();
                made.map(|mut r| {
                    // Its images are its own (the menu's glyphs, copied in
                    // `render_menu_popup`); the shared queue is the window's.
                    r.set_shared_uploads(false);
                    *slot = Some(r)
                })
            }
        };
        // A lost surface is the connection dying under the menu; the window's
        // own event loop ends the session on it. Just drop the menu.
        if let Err(lost) = attached {
            log::warn!("[menu_popup] {lost}; closing the menu");
            context_menu::hide();
            self.close_menu_popup();
            self.redraw = true;
            return;
        }
        self.redraw = true;
        // A handed-off menu is drawn on the next frame, after the window's
        // own without it (see `open_menu_popup`); drawn here it would stand
        // over the window's copy until that frame.
        if !handoff {
            self.render_menu_popup();
        }
    }

    fn done(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, popup: &Popup) {
        // The compositor dismissed it (its parent went away, say). The menu
        // closes with it; an app that watches `is_visible` sees that.
        if self.is_menu_popup(popup.wl_surface()) {
            context_menu::hide();
            self.close_menu_popup();
            self.redraw = true;
        }
    }
}

smithay_client_toolkit::delegate_xdg_popup!(@<A: Application> EngineState<A>);
