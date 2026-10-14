//! A grid app's context menu, on an Overlay layer surface of its own.
//!
//! An ordinary window hosts the menu in an `xdg_popup` (`menu_popup`). A
//! grid app cannot: its surface is world-anchored, so the compositor scales
//! it — and every popup riding on it — with the desk camera (a menu opened
//! in overview would shrink with the desk), it is pinned at buffer scale 1,
//! and it never holds the keyboard. So its menu stands on the SCREEN instead:
//!
//! - **The menu's coordinates are output-logical px**, not window ones. A
//!   grid app shows it at a point the compositor gave it in those terms (the
//!   `desktop-menu` status topic) and hit-tests it there; the runner hands
//!   it pointer events on this surface at the surface's offset on the
//!   output, exactly as a popup's are offset into window coordinates.
//! - **The runner places it**, by `context_menu::constrain_to` against the
//!   output (the positioner's flip / slide / shorten, done here) and then the
//!   surface's margins: anchored top-left, exclusive zone -1, so the box the
//!   margins measure from is the whole output.
//! - **On the output the app names** (`context_menu::set_output`, a
//!   `wl_output` name the compositor sent with the press): the layer surface
//!   is created on it, the menu fitted to it and drawn at its scale. A menu
//!   naming no output, or one this client does not know, goes where the
//!   compositor puts a layer surface with none (its first output), fitted to
//!   the largest-scale output — right only with one display.
//! - **It is drawn at the output's scale**, whatever the grid's own is: the
//!   toolkit-wide factor is swapped for the frame (`draw_menu_to`).
//! - **It takes the keyboard** (exclusive), so Up/Down/Enter reach the app
//!   and Escape the driver; and it closes when the keyboard leaves it. That
//!   is the click-away: the compositor treats the `cce-menu` namespace as it
//!   treats cce-cloud's menus, and a press anywhere else drops focus from it.

use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerSurface, LayerSurfaceConfigure};
use smithay_client_toolkit::shell::WaylandSurface;
use wayland_client::{protocol::wl_output, protocol::wl_surface, Proxy};

use super::window_runner::{Application, EngineState};
use crate::vk::VkRenderer;
use crate::widget::context_menu;

/// The layer namespace the compositor knows a grid app's menu by.
pub const MENU_NAMESPACE: &str = "cce-menu";

pub struct MenuLayer {
    layer: LayerSurface,
    /// The menu's generation and the rect last asked for: `(x, y)` its
    /// margins on the output, `(w, h)` its size, all logical px.
    key: (u64, (i32, i32, u32, u32)),
    /// The output it was created on, as the app named it (`None`: the
    /// compositor's choice). A surface cannot change output, so a menu for
    /// another one gets a new surface.
    output: Option<String>,
    /// The size of the last configure; `None` before the first, until which
    /// nothing may be attached.
    placed: Option<(f32, f32)>,
    /// The output scale it is drawn at (its `preferred_buffer_scale`, else
    /// the largest output's).
    scale: f64,
    committed_scale: i32,
}

impl MenuLayer {
    pub fn surface(&self) -> &wl_surface::WlSurface {
        self.layer.wl_surface()
    }

    /// The surface's offset on the output, if `surface` is it and it is up.
    pub fn offset_for(&self, surface: &wl_surface::WlSurface) -> Option<(f32, f32)> {
        let (x, y, _, _) = self.key.1;
        (self.placed.is_some() && self.surface() == surface).then_some((x as f32, y as f32))
    }

    pub fn is_layer(&self, layer: &LayerSurface) -> bool {
        &self.layer == layer
    }
}

impl<A: Application> EngineState<A> {
    /// The output the menu stands on: the one named `name` when this client
    /// knows it, else (and for no name) the one of the largest scale — the
    /// one `detect_scale_factor` picks. Its `wl_output` (only for a named
    /// output: an unnamed menu leaves the choice to the compositor), logical
    /// size and scale.
    fn menu_output(&self, name: Option<&str>) -> (Option<wl_output::WlOutput>, (f32, f32), i32) {
        let mut best: Option<((f32, f32), i32)> = None;
        for output in self.output_state.outputs() {
            let Some(info) = self.output_state.info(&output) else { continue };
            let Some((w, h)) = info.logical_size else { continue };
            let size = ((w.max(1) as f32, h.max(1) as f32), info.scale_factor.max(1));
            if name.is_some() && info.name.as_deref() == name {
                return (Some(output), size.0, size.1);
            }
            if best.is_none_or(|b| size.1 > b.1) {
                best = Some(size);
            }
        }
        if let Some(name) = name {
            log::warn!("[menu_layer] no output named {name}; leaving it to the compositor");
        }
        let (size, scale) = best.unwrap_or(((1920.0, 1080.0), 1));
        (None, size, scale)
    }

    /// Bring the layer in line with the menu, as `sync_menu_popup` does the
    /// popup — which hands over to this for a grid app.
    pub(crate) fn sync_menu_layer(&mut self) {
        let visible = context_menu::is_visible();
        if visible && context_menu::is_turning() {
            self.redraw = true;
        }
        if !visible || self.layer_shell_state.is_none() {
            if self.menu_layer.is_some() {
                self.close_menu_layer();
            }
            context_menu::set_hosted(false);
            return;
        }
        let name = context_menu::output();
        let (wl_output, (ow, oh), output_scale) = self.menu_output(name.as_deref());
        context_menu::constrain_to(0.0, 0.0, ow, oh);
        let (_, w, content_h) = context_menu::natural_geometry();
        let (x, y) = (context_menu::x(), context_menu::y());
        let h = content_h.min(oh - y).max(context_menu::h());
        let rect = (x.round() as i32, y.round() as i32, w.ceil().max(1.0) as u32, h.ceil().max(1.0) as u32);
        let key = (context_menu::generation(), rect);
        if self.menu_layer.as_ref().is_some_and(|l| l.key == key && l.output == name) {
            return;
        }
        // A menu for another output: that surface cannot follow it.
        if self.menu_layer.as_ref().is_some_and(|l| l.output != name) {
            self.close_menu_layer();
        }
        context_menu::set_hosted(true);
        if let Some(ml) = self.menu_layer.as_mut() {
            // Moved or resized (a page turn, rows refilled): the same surface,
            // so the pointer on it keeps its focus.
            ml.layer.set_margin(rect.1, 0, 0, rect.0);
            ml.layer.set_size(rect.2, rect.3);
            ml.layer.commit();
            ml.key = key;
            self.redraw = true;
            return;
        }
        let Some(layer_shell) = self.layer_shell_state.as_ref() else { return };
        let surface = self.compositor_state.create_surface(&self.qh);
        let layer =
            layer_shell.create_layer_surface(&self.qh, surface, Layer::Overlay, Some(MENU_NAMESPACE), wl_output.as_ref());
        layer.set_anchor(Anchor::TOP | Anchor::LEFT);
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        layer.set_margin(rect.1, 0, 0, rect.0);
        layer.set_size(rect.2, rect.3);
        layer.commit();
        let scale = crate::scale::forced_scale().map_or(output_scale as f64, |f| f as f64);
        self.menu_layer = Some(MenuLayer { layer, key, output: name, placed: None, scale, committed_scale: 0 });
    }

    /// Close the layer; the renderer lets go of the surface first, as for the
    /// popup (they share it — only one is ever up).
    pub(crate) fn close_menu_layer(&mut self) {
        if self.menu_layer.is_none() {
            return;
        }
        if let Some(renderer) = self.menu_renderer.as_mut() {
            if renderer.has_surface() {
                renderer.detach_surface();
            }
        }
        self.menu_layer = None;
        context_menu::set_hosted(false);
    }

    /// The offset on the output of the menu layer, if `surface` is it.
    pub(crate) fn menu_layer_offset(&self, surface: &wl_surface::WlSurface) -> Option<(f32, f32)> {
        self.menu_layer.as_ref().and_then(|l| l.offset_for(surface))
    }

    pub(crate) fn is_menu_layer_surface(&self, surface: &wl_surface::WlSurface) -> bool {
        self.menu_layer.as_ref().is_some_and(|l| l.surface() == surface)
    }

    /// The compositor's scale for the menu layer's surface.
    pub(crate) fn menu_layer_scale_changed(&mut self, scale: i32) {
        if let Some(ml) = self.menu_layer.as_mut() {
            if crate::scale::forced_scale().is_none() && (ml.scale - scale as f64).abs() > f64::EPSILON {
                ml.scale = scale as f64;
                self.redraw = true;
                self.render_menu_layer();
            }
        }
    }

    /// The keyboard left the menu: a press elsewhere (the compositor drops
    /// focus from it) or another surface taking focus. The menu closes.
    pub(crate) fn menu_layer_lost_keyboard(&mut self) {
        context_menu::hide();
        self.close_menu_layer();
        self.redraw = true;
    }

    /// The layer's configure, if `layer` is it; `false` when it is not.
    pub(crate) fn configure_menu_layer(&mut self, layer: &LayerSurface, configure: &LayerSurfaceConfigure) -> bool {
        let display = self.display_ptr;
        let (ml, slot, icon_ids) = (self.menu_layer.as_mut(), &mut self.menu_renderer, &mut self.menu_icon_ids);
        let Some(ml) = ml.filter(|l| l.is_layer(layer)) else { return false };
        let (_, _, rw, rh) = ml.key.1;
        let (cw, ch) = configure.new_size;
        let (w, h) = if cw > 0 && ch > 0 { (cw as f32, ch as f32) } else { (rw as f32, rh as f32) };
        ml.placed = Some((w, h));
        let (_, pw, ph) = Self::buffer_geometry(ml.scale, w, h);
        let surface_ptr = ml.surface().id().as_ptr() as *mut std::ffi::c_void;
        let display_ptr = display as *mut std::ffi::c_void;
        let attached = match slot.as_mut() {
            Some(r) if r.has_surface() => {
                r.resize(pw, ph);
                Ok(())
            }
            Some(r) => unsafe { r.attach_surface(display_ptr, surface_ptr, pw, ph) },
            None => {
                let made = unsafe { VkRenderer::try_new(display_ptr, surface_ptr, pw, ph, 0.0) };
                icon_ids.clear();
                made.map(|mut r| {
                    r.set_shared_uploads(false);
                    *slot = Some(r)
                })
            }
        };
        if let Err(lost) = attached {
            log::warn!("[menu_layer] {lost}; closing the menu");
            context_menu::hide();
            self.close_menu_layer();
            self.redraw = true;
            return true;
        }
        self.redraw = true;
        self.render_menu_layer();
        true
    }

    /// The compositor closed the layer (its output went away, say): the menu
    /// closes with it. `false` when `layer` is not the menu's.
    pub(crate) fn menu_layer_closed(&mut self, layer: &LayerSurface) -> bool {
        if !self.menu_layer.as_ref().is_some_and(|l| l.is_layer(layer)) {
            return false;
        }
        context_menu::hide();
        self.close_menu_layer();
        self.redraw = true;
        true
    }

    /// Draw the menu into its layer; a no-op until the first configure.
    pub(crate) fn render_menu_layer(&mut self) {
        let Some(ml) = self.menu_layer.as_ref() else { return };
        let Some((w, h)) = ml.placed else { return };
        let (surface, committed, scale) = (ml.surface().clone(), ml.committed_scale, ml.scale);
        // The grid pins the toolkit-wide factor to its patch scale, and text
        // shapes at it: this surface is the output's.
        let pinned = crate::scale::scale_factor();
        crate::scale::set_scale_factor(scale as f32);
        let drawn = self.draw_menu_to(&surface, committed, w, h, scale);
        crate::scale::set_scale_factor(pinned);
        if let (Some(s), Some(ml)) = (drawn, self.menu_layer.as_mut()) {
            ml.committed_scale = s;
        }
    }
}
