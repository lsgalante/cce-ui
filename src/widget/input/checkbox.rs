//! Narrow-trait `Checkbox` and `Toggle` (Phase 5e — first interactive widgets off `WidgetHost`).
//!
//! Both are inline-label widgets: they paint their own label (with hover/focus-dependent color)
//! inside their rect, so they track `hovered`/`focused` themselves from the `MouseEnter`/
//! `MouseLeave`/`FocusIn`/`FocusOut` events the adapter forwards — the migration shape for the
//! state that becomes `Animated<f32>` in RFC §3.6.
//!
//! Geometry parity: `paint` emits the same conditional geometry as the legacy `extra_quads` /
//! `extra_arcs` / `all_rounded_quads` overrides did, in the matching prim kinds, so the adapter's
//! per-prim reverse bridges reproduce the legacy getters byte-for-byte.

use crate::colors;
use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::{
    Adapted, ElementState, Event, EventCtx, Input, Justification, Layout, MouseButton, Paint,
};

fn parse_bool(val: &str) -> Option<bool> {
    match val.trim().to_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

pub struct Checkbox {
    checked: bool,
    just_clicked: bool,
    pub just_changed: bool,
    label: Option<String>,
    hovered: bool,
    focused: bool,
}

impl Checkbox {
    pub fn new() -> Adapted<Checkbox> {
        Adapted::new(Checkbox {
            checked: false,
            just_clicked: false,
            just_changed: false,
            label: None,
            hovered: false,
            focused: false,
        })
    }

    pub fn set_checked(&mut self, checked: bool) {
        self.checked = checked;
    }

    pub fn checked(&self) -> bool {
        self.checked
    }

    /// Hover state, also settable directly for immediate-mode hosts that do their own
    /// hit-testing instead of routing `MouseEnter`/`MouseLeave` (json_layout).
    pub fn hovered(&self) -> bool {
        self.hovered
    }

    pub fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    fn box_color(&self) -> [f32; 4] {
        if self.checked {
            colors::checkbox_checked()
        } else if self.hovered {
            colors::checkbox_hover()
        } else {
            colors::checkbox_bg()
        }
    }
}

impl Layout for Checkbox {
    fn inline_label(&self) -> bool {
        true
    }
}

impl Paint for Checkbox {
    fn color(&self) -> [f32; 4] {
        // Legacy mode split: wide (labeled row) has a transparent widget background; standalone
        // is the colored box itself. Style-property paths (demo `widget_vertices`) read this.
        // Width isn't known here, so mirror the legacy intent via the label: labeled checkboxes
        // are the wide rows.
        if self.label.is_some() {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            self.box_color()
        }
    }

    fn sync_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);
        if w > 30.0 {
            // Wide mode: label on the left, 18px box on the right — the legacy `extra_quads`
            // geometry verbatim.
            let box_size = 18.0f32;
            let box_x = x + w - box_size - 8.0;
            let box_y = y + (h - box_size) / 2.0;

            ctx.quad(Rect { x: box_x, y: box_y, width: box_size, height: box_size }, self.box_color());

            let border_color = if self.hovered {
                [0.35, 0.35, 0.40, 1.0]
            } else {
                [0.25, 0.25, 0.30, 1.0]
            };
            ctx.quad(Rect { x: box_x, y: box_y, width: box_size, height: 1.0 }, border_color);
            ctx.quad(Rect { x: box_x, y: box_y + box_size - 1.0, width: box_size, height: 1.0 }, border_color);
            ctx.quad(Rect { x: box_x, y: box_y, width: 1.0, height: box_size }, border_color);
            ctx.quad(Rect { x: box_x + box_size - 1.0, y: box_y, width: 1.0, height: box_size }, border_color);

            if self.checked {
                let pad = 5.0f32;
                ctx.quad(
                    Rect { x: box_x + pad, y: box_y + pad, width: box_size - 2.0 * pad, height: box_size - 2.0 * pad },
                    [1.0, 1.0, 1.0, 0.9],
                );
            }
        } else {
            // Standalone mode: the widget IS the box. The colored background was legacy
            // `color()`; emitting it here puts it on every render path (the legacy
            // `render_widget` path never drew it — same latent-invisibility class as StatusDot).
            ctx.quad(rect, self.box_color());
            if self.checked {
                let pad_x = w * 0.25;
                let pad_y = h * 0.25;
                ctx.quad(
                    Rect { x: x + pad_x, y: y + pad_y, width: w - 2.0 * pad_x, height: h - 2.0 * pad_y },
                    [1.0, 1.0, 1.0, 0.9],
                );
            }
        }

        if let Some(ref label) = self.label {
            let (_, font_size) = crate::layout::control_label_font_parsed();
            let ty = crate::layout::align_text_y(y, h, font_size, 0.0);
            ctx.text(
                label.clone(),
                x + 8.0,
                ty,
                font_size,
                colors::control_label_color_for_state(self.hovered, self.focused),
            );
        }
    }
}

impl Input for Checkbox {
    fn on_event(&mut self, event: &Event, _ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, .. } => {
                // Already hit-gated by the adapter.
                self.checked = !self.checked;
                self.just_clicked = true;
                self.just_changed = true;
                true
            }
            Event::MouseEnter => {
                self.hovered = true;
                false
            }
            Event::MouseLeave => {
                self.hovered = false;
                false
            }
            Event::FocusIn => {
                self.focused = true;
                false
            }
            Event::FocusOut => {
                self.focused = false;
                false
            }
            _ => false,
        }
    }

    fn opens_context_menu(&self) -> bool {
        true
    }

    fn take_click(&mut self) -> bool {
        std::mem::take(&mut self.just_clicked)
    }

    fn take_change(&mut self) -> bool {
        std::mem::take(&mut self.just_changed)
    }

    fn value_string(&self) -> Option<String> {
        Some(self.checked.to_string())
    }

    fn set_value_string(&mut self, val: &str) -> bool {
        let Some(new_checked) = parse_bool(val) else { return false };
        if self.checked != new_checked {
            self.checked = new_checked;
            self.just_changed = true;
            true
        } else {
            false
        }
    }

    fn value(&self) -> i32 {
        if self.checked { 1 } else { 0 }
    }
}

#[derive(Debug, Clone)]
pub struct Toggle {
    toggled: bool,
    just_toggled: bool,
    label: Option<String>,
    hovered: bool,
    focused: bool,
    /// Where the label sits across the pill. Mirrors `Button::justify` — same enum, same
    /// 8px edge inset — so the two read as one control set wherever they share a column.
    justify: Justification,
    /// Raised style: the pill renders as a rocker — two flat half faces with
    /// beveled edges, the state half a raised plateau, the other recessed
    /// (see `rocker_reliefs` and the paint impl) — and the flat style's
    /// state gradient is dropped.
    raised: bool,
}

impl Toggle {
    pub fn new() -> Adapted<Toggle> {
        Adapted::new(Toggle {
            toggled: false,
            just_toggled: false,
            label: None,
            hovered: false,
            focused: false,
            justify: Justification::Center,
            raised: crate::layout::control_relief(),
        })
    }

    pub fn set_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    pub fn set_toggled(&mut self, v: bool) {
        self.toggled = v;
    }

    pub fn toggled(&self) -> bool {
        self.toggled
    }

    fn gradient_color(&self) -> [f32; 4] {
        if self.toggled {
            colors::toggle_on_color()
        } else {
            colors::toggle_off_color()
        }
    }

    /// The rocker's two halves over `rect` as FLAT relief steps: (half rect,
    /// per-corner radii, (top, right, bottom, left) walls, raised). The state
    /// half (top when on, bottom when off) is a plateau raised out of the
    /// surface (raised → `boss_edges`), the other falls away (`recess_edges`).
    /// Both faces stay flat — the lit read comes from `face_light`, not a
    /// tilt gradient. The hinge wall is open on both so the halves meet in a
    /// single step, not a double-shaded trough.
    pub fn rocker_reliefs(
        &self,
        rect: Rect,
    ) -> [(Rect, (f32, f32, f32, f32), (bool, bool, bool, bool), bool); 2] {
        let r = crate::layout::toggle_corner_radius();
        let half_h = rect.height / 2.0;
        let top = Rect { x: rect.x, y: rect.y, width: rect.width, height: half_h };
        let bottom =
            Rect { x: rect.x, y: rect.y + half_h, width: rect.width, height: rect.height - half_h };
        let top_half = (top, (r, r, 0.0, 0.0), (true, true, false, true));
        let bottom_half = (bottom, (0.0, 0.0, r, r), (false, true, true, true));
        let (state, other) = if self.toggled { (top_half, bottom_half) } else { (bottom_half, top_half) };
        [(state.0, state.1, state.2, true), (other.0, other.1, other.2, false)]
    }

    /// A rocker face's UNIFORM lighting overlay, evaluated under the SAME DE
    /// light the bevels answer to: `light_source_position` through the plate
    /// model (shader2d's `plate_shade` — ambient floor, diffuse off the
    /// normal, expressed relative to the flat face). The rocker reads as a
    /// bent plate: the state half tilts OUT toward the viewer, the other IN,
    /// so each half has ONE slightly tipped normal — the overlay stays
    /// uniform (the faces keep their flat-step read) but its sign and amount
    /// swing with the light azimuth exactly like the walls' shading, and the
    /// whole thing scales with `bevel_depth` through the same strength term.
    /// Mirrors `tessellate_display_list`'s `plate_light` construction and
    /// shader2d's `PLATE_AMBIENT` / `flat_shade` — keep the three in sync.
    pub fn face_light(&self, top_half: bool) -> [f32; 4] {
        const AMBIENT: f32 = 0.55; // shader2d PLATE_AMBIENT
        const FACE_TILT: f32 = 0.5; // the rocker plate's slope, as dh over dy
        let az = crate::layout::light_source_position();
        let el = std::f32::consts::FRAC_PI_4; // plate_light's elevation
        let (ly, lz) = (-az.sin() * el.cos(), el.sin());
        // The state half tilts out (outer edge toward the viewer), the other
        // in. A heightfield normal is (0, -dh/dy, 1): a top OUT half rises
        // toward -y, tipping its normal to +y; every other case follows.
        let out = top_half == self.toggled;
        let ny = if top_half == out { FACE_TILT } else { -FACE_TILT };
        // Faces pivot about the horizontal hinge, so only the light's y and
        // z components reach the dot product — azimuth enters through ly.
        let dot = ((ny * ly + lz) / (1.0 + ny * ny).sqrt()).max(0.0);
        let flat = AMBIENT + (1.0 - AMBIENT) * lz;
        let diff = AMBIENT + (1.0 - AMBIENT) * dot;
        let strength = crate::layout::bevel_depth() / 0.15;
        let v = (diff / flat - 1.0) * strength;
        if v >= 0.0 {
            [1.0, 1.0, 1.0, v.min(1.0)]
        } else {
            [0.0, 0.0, 0.0, (-v).min(1.0)]
        }
    }
}

impl Adapted<Toggle> {
    pub fn with_left_align(mut self, left_align: bool) -> Self {
        self.justify = if left_align { Justification::Left } else { Justification::Center };
        self
    }

    pub fn with_justify(mut self, justify: Justification) -> Self {
        self.justify = justify;
        self
    }

    /// Raised style: see the `raised` field.
    pub fn with_raised(mut self, raised: bool) -> Self {
        self.raised = raised;
        self
    }
}

impl Layout for Toggle {
    fn inline_label(&self) -> bool {
        true
    }

    fn intrinsic_size(&self) -> Option<crate::scene::layout::Size> {
        // Legacy `preferred_height`; width comes from the container.
        Some(crate::scene::layout::Size::new(0.0, crate::layout::toggle_height()))
    }
}

impl Paint for Toggle {
    fn color(&self) -> [f32; 4] {
        colors::toggle_bg_color()
    }

    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        let r = crate::layout::toggle_corner_radius();
        if r > 0.0 {
            Some((r, (true, true, true, true)))
        } else {
            None
        }
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::control_label_font())
    }

    fn sync_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);
        let radius = crate::layout::toggle_corner_radius();
        let bg = colors::toggle_bg_color();

        if self.raised {
            // The rocker: two FLAT half faces (see `rocker_reliefs`) — the
            // state half a raised plateau, the other recessed, hinge wall
            // open so they meet in a single step. Each face carries its
            // uniform `face_light` overlay (lit plateau, shaded floor) under
            // the beveled edges, so faces reflect the same light the walls
            // do. This is the physical read of the flat style's state
            // gradient, which this style drops entirely.
            if bg[3] > 0.001 {
                if radius > 0.0 {
                    ctx.rounded_rect(rect, radius, (true, true, true, true), bg);
                } else {
                    ctx.quad(rect, bg);
                }
            }
            for (half, radii, _, _) in self.rocker_reliefs(rect) {
                let light = self.face_light(radii.0 > 0.0);
                if light[3] > 0.001 {
                    let corners = (radii.0 > 0.0, radii.1 > 0.0, radii.2 > 0.0, radii.3 > 0.0);
                    ctx.rounded_rect(half, radius, corners, light);
                }
            }
            let depth = crate::layout::bevel_width().min(h * 0.2);
            for (half, radii, walls, raised) in self.rocker_reliefs(rect) {
                if raised {
                    ctx.boss_edges(half, radii, depth, walls);
                } else {
                    ctx.recess_edges(half, radii, depth, walls);
                }
            }
        } else if radius > 0.0 {
            ctx.rounded_rect(rect, radius, (true, true, true, true), bg);
        } else {
            ctx.quad(rect, bg);
        }

        // The state half carries a vertical gradient of the state color: full opacity at
        // the widget's outer edge (top when on, bottom when off), fading to transparent at
        // the vertical middle. Banded quads (the content_bg gradient pattern) so it flows
        // through the plain-quad views; bands inside the corner radius inset to follow the
        // rounded corners. The band alphas follow a perceptual curve, not a straight ramp —
        // see `colors::perceptual_fade_alpha`. The bands composite onto the bg quad above,
        // so that is the backdrop the curve is solved against.
        let grad = if self.raised { [0.0; 4] } else { self.gradient_color() };
        let half = h / 2.0;
        if half > 0.0 && grad[3] > 0.0 {
            let steps = (half.ceil() as usize).clamp(4, 32);
            let band_h = half / steps as f32;
            for i in 0..steps {
                let t0 = i as f32 * band_h; // band start, as distance from the outer edge
                let t_mid = t0 + band_h / 2.0;
                let alpha = colors::perceptual_fade_alpha(
                    t_mid / half,
                    [grad[0], grad[1], grad[2]],
                    [bg[0], bg[1], bg[2]],
                    grad[3],
                );
                if alpha <= 0.003 {
                    continue;
                }
                let by = if self.toggled { y + t0 } else { y + h - t0 - band_h };
                let inset = if radius > 0.0 && t_mid < radius {
                    let dr = radius - t_mid;
                    radius - (radius * radius - dr * dr).max(0.0).sqrt()
                } else {
                    0.0
                };
                ctx.quad(
                    Rect { x: x + inset, y: by, width: w - 2.0 * inset, height: band_h },
                    [grad[0], grad[1], grad[2], alpha],
                );
            }
        }

        if let Some(ref label) = self.label {
            let (font_fam, font_size) = crate::layout::control_label_font_parsed();
            let est_w = crate::widget::display::measure_text_width(label, &font_fam, font_size);
            let tx = match self.justify {
                Justification::Left => x + 8.0,
                Justification::Right => x + w - est_w - 8.0,
                Justification::Center => x + (w - est_w) / 2.0,
            };
            ctx.text(
                label.clone(),
                tx,
                crate::layout::align_text_y(y, h, font_size, 0.0),
                font_size,
                colors::control_label_color_for_state(self.hovered, self.focused),
            );
        }
    }
}

impl Input for Toggle {
    fn on_event(&mut self, event: &Event, _ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, .. } => {
                self.toggled = !self.toggled;
                self.just_toggled = true;
                true
            }
            Event::MouseEnter => {
                self.hovered = true;
                false
            }
            Event::MouseLeave => {
                self.hovered = false;
                false
            }
            Event::FocusIn => {
                self.focused = true;
                false
            }
            Event::FocusOut => {
                self.focused = false;
                false
            }
            _ => false,
        }
    }

    fn take_click(&mut self) -> bool {
        std::mem::take(&mut self.just_toggled)
    }

    fn take_change(&mut self) -> bool {
        std::mem::take(&mut self.just_toggled)
    }

    fn value_string(&self) -> Option<String> {
        Some(self.toggled.to_string())
    }

    fn set_value_string(&mut self, val: &str) -> bool {
        let Some(new_toggled) = parse_bool(val) else { return false };
        if self.toggled != new_toggled {
            self.toggled = new_toggled;
            self.just_toggled = true;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::{WidgetHost, UiContext};

    fn click_at(x: f32, y: f32) -> Event {
        Event::MouseButton {
            button: MouseButton::Left,
            state: ElementState::Pressed,
            x,
            y,
            local_x: x,
            local_y: y,
        }
    }

    #[test]
    fn checkbox_click_toggles_and_polls_like_legacy() {
        let mut ctx = UiContext::new();
        let mut cb = Checkbox::new();
        let (id, ptr) = (cb.id(), cb.as_ptr_mut());
        ctx.register_widget(id, ptr);
        WidgetHost::set_rect(&mut cb, 0.0, 0.0, 20.0, 20.0);

        assert!(ctx.propagate_event(&click_at(10.0, 10.0), id), "in-rect click consumed");
        assert!(cb.checked(), "click checked it");
        assert!(cb.take_click(), "take_click reads once");
        assert!(!cb.take_click(), "...then clears");
        assert!(cb.take_change());

        assert!(!ctx.propagate_event(&click_at(100.0, 100.0), id), "miss is not consumed");
        assert!(cb.checked(), "miss does not toggle");
    }

    #[test]
    fn checkbox_value_string_round_trip() {
        let mut cb = Checkbox::new();
        assert_eq!(cb.get_value_string(), Some("false".to_string()));
        assert!(cb.set_value_string("on"));
        assert!(cb.checked());
        assert_eq!(cb.value(), 1);
        assert!(!cb.set_value_string("on"), "unchanged value reports false");
        assert!(!cb.set_value_string("junk"), "unparsable reports false");
        assert!(cb.take_change(), "set_value_string marked the change");
    }

    /// Wide (labeled) mode reproduces the legacy `extra_quads` geometry through the bridge:
    /// box bg + 4 border edges (+ indicator when checked), and the label text with a
    /// hover-dependent color.
    #[test]
    fn checkbox_wide_mode_bridge_parity() {
        let ctx = UiContext::new();
        let mut cb = Checkbox::new().with_label("Enable");
        WidgetHost::set_rect(&mut cb, 0.0, 0.0, 200.0, 24.0);

        let quads = WidgetHost::extra_quads(&cb);
        // Unchecked: box bg + 4 border edges = 5 quads, at the legacy box position.
        assert_eq!(quads.len(), 5);
        let (box_x, box_y, box_size) = (200.0 - 18.0 - 8.0, (24.0 - 18.0) / 2.0, 18.0);
        assert_eq!(quads[0], (box_x, box_y, box_size, box_size, colors::checkbox_bg()));

        // Hover flips the box + border colors (tracked from MouseEnter, not base state).
        cb.inner_mut().hovered = true;
        let quads = WidgetHost::extra_quads(&cb);
        assert_eq!(quads[0].4, colors::checkbox_hover());

        // Label text comes through the prim-derived text bridge at the legacy position.
        let labels = cb.own_text_labels();
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].text, "Enable");
        assert_eq!(labels[0].x, 8.0);

        // Inline label => no set_rect inflation.
        assert_eq!(WidgetHost::rect(&cb), (0.0, 0.0, 200.0, 24.0));
        let _ = &ctx;
    }

    #[test]
    fn toggle_click_and_gradient_switches_halves() {
        let mut ctx = UiContext::new();
        let mut t = Toggle::new();
        let (id, ptr) = (t.id(), t.as_ptr_mut());
        ctx.register_widget(id, ptr);
        WidgetHost::set_rect(&mut t, 0.0, 0.0, 60.0, 30.0);

        // Geometry is config-dependent (rounded vs square, gradient vs rocker);
        // assert the invariant that holds in all styles: the state side flips —
        // the flat style's gradient half, the relief style's raised half.
        let painted = |t: &Adapted<Toggle>| {
            let mut pc = crate::scene::paint::PaintCtx::new();
            crate::widget::Paint::paint(
                t.inner(),
                Rect { x: 0.0, y: 0.0, width: 60.0, height: 30.0 },
                &mut pc,
            );
            pc.finish()
                .items
                .into_iter()
                .map(|i| format!("{:?}", i.prim))
                .collect::<Vec<_>>()
        };
        let before = painted(&t);

        assert!(ctx.propagate_event(&click_at(30.0, 15.0), id), "toggle consumed the click");
        assert!(t.toggled());
        assert!(t.take_click());

        assert!(
            painted(&t) != before,
            "toggling changes the emitted geometry (state side switches halves)",
        );

        // preferred_height forwards the legacy toggle height.
        assert_eq!(WidgetHost::preferred_height(&t), Some(crate::layout::toggle_height()));
    }

    #[test]
    fn toggle_set_label_via_deref_reaches_paint() {
        let mut t = Toggle::new();
        WidgetHost::set_rect(&mut t, 0.0, 0.0, 60.0, 30.0);
        t.set_label("ON"); // the network.rs pattern: live label updates through Deref
        let labels = t.own_text_labels();
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].text, "ON");
    }
}
