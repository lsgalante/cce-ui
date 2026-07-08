//! Narrow-trait `Checkbox` and `Toggle` (Phase 5e — first interactive widgets off `Element`).
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
use crate::widget::{Adapted, Control, ElementState, Event, EventCtx, Input, Layout, MouseButton, Paint};

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
}

impl Toggle {
    pub fn new() -> Adapted<Toggle> {
        Adapted::new(Toggle {
            toggled: false,
            just_toggled: false,
            label: None,
            hovered: false,
            focused: false,
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

    fn border_color(&self) -> [f32; 4] {
        if self.toggled {
            colors::toggle_on_color()
        } else {
            colors::toggle_off_color()
        }
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

    fn corner_style(&self) -> Option<(f32, (bool, bool, bool, bool))> {
        let r = crate::layout::toggle_corner_radius();
        if r > 0.0 {
            Some((r, (true, true, true, true)))
        } else {
            None
        }
    }

    fn solid_border(&self) -> Option<([f32; 4], f32)> {
        let border_w = crate::layout::toggle_border_width();
        if border_w > 0.0 {
            Some((self.border_color(), border_w))
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
        let border_w = crate::layout::toggle_border_width();
        let bg = colors::toggle_bg_color();
        let border_color = self.border_color();

        if radius > 0.0 {
            // Rounded mode: the legacy `all_rounded_quads` half-split verbatim — the border
            // hugs the "on" (top) or "off" (bottom) half.
            if self.toggled {
                ctx.rounded_rect(Rect { x, y: y + h / 2.0, width: w, height: h / 2.0 }, radius, (false, false, true, true), bg);
                if border_w > 0.0 {
                    ctx.rounded_rect(Rect { x, y, width: w, height: h / 2.0 }, radius, (true, true, false, false), border_color);
                    let inner_radius = (radius - border_w).max(0.0);
                    ctx.rounded_rect(
                        Rect { x: x + border_w, y: y + border_w, width: w - 2.0 * border_w, height: h / 2.0 - border_w },
                        inner_radius,
                        (true, true, false, false),
                        bg,
                    );
                }
                else {
                    ctx.rounded_rect(Rect { x, y, width: w, height: h / 2.0 }, radius, (true, true, false, false), bg);
                }
            } else {
                ctx.rounded_rect(Rect { x, y, width: w, height: h / 2.0 }, radius, (true, true, false, false), bg);
                if border_w > 0.0 {
                    ctx.rounded_rect(Rect { x, y: y + h / 2.0, width: w, height: h / 2.0 }, radius, (false, false, true, true), border_color);
                    let inner_radius = (radius - border_w).max(0.0);
                    ctx.rounded_rect(
                        Rect { x: x + border_w, y: y + h / 2.0, width: w - 2.0 * border_w, height: h / 2.0 - border_w },
                        inner_radius,
                        (false, false, true, true),
                        bg,
                    );
                } else {
                    ctx.rounded_rect(Rect { x, y: y + h / 2.0, width: w, height: h / 2.0 }, radius, (false, false, true, true), bg);
                }
            }

            // Corner arcs of the bordered half (legacy `extra_arcs`).
            if border_w > 0.0 && radius > 0.1 {
                use std::f32::consts::PI;
                if self.toggled {
                    ctx.arc(x + radius, y + radius, radius, border_w, PI, 1.5 * PI, border_color);
                    ctx.arc(x + w - radius, y + radius, radius, border_w, 1.5 * PI, 2.0 * PI, border_color);
                } else {
                    ctx.arc(x + radius, y + h - radius, radius, border_w, 0.5 * PI, PI, border_color);
                    ctx.arc(x + w - radius, y + h - radius, radius, border_w, 0.0, 0.5 * PI, border_color);
                }
            }
        } else {
            // Square mode: the legacy `extra_quads` geometry verbatim — bg quad plus border
            // edges on the "on" (top) or "off" (bottom) half.
            ctx.quad(rect, bg);
            if border_w > 0.0 {
                let t = border_w;
                let r = radius; // 0.0 here, kept for formula parity with the legacy code
                let edge_h = ((h / 2.0) - r).max(0.0);
                if self.toggled {
                    ctx.quad(Rect { x: x + r, y, width: w - 2.0 * r, height: t }, border_color);
                    if edge_h > 0.0 {
                        ctx.quad(Rect { x, y: y + r, width: t, height: edge_h }, border_color);
                        ctx.quad(Rect { x: x + w - t, y: y + r, width: t, height: edge_h }, border_color);
                    }
                } else {
                    ctx.quad(Rect { x: x + r, y: y + h - t, width: w - 2.0 * r, height: t }, border_color);
                    if edge_h > 0.0 {
                        ctx.quad(Rect { x, y: y + h / 2.0, width: t, height: edge_h }, border_color);
                        ctx.quad(Rect { x: x + w - t, y: y + h / 2.0, width: t, height: edge_h }, border_color);
                    }
                }
            }
        }

        if let Some(ref label) = self.label {
            let (font_fam, font_size) = crate::layout::control_label_font_parsed();
            let est_w = crate::widget::display::measure_text_width(label, &font_fam, font_size);
            ctx.text(
                label.clone(),
                x + (w - est_w) / 2.0,
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

// The `set_label` overrides route the trait entry point (e.g. `dyn Control` callers) to the
// synced inherent version — `Control`'s default writes only the base label, which would leave
// these self-painting labels stale.
impl Control for Adapted<Checkbox> {
    fn set_label(&mut self, label: &str) {
        Adapted::set_label(self, label);
    }
}
impl Control for Adapted<Toggle> {
    fn set_label(&mut self, label: &str) {
        Adapted::set_label(self, label);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::{Element, UiContext};

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
        Element::set_rect(&mut cb, 0.0, 0.0, 20.0, 20.0);

        assert!(ctx.propagate_event(&click_at(10.0, 10.0), ptr), "in-rect click consumed");
        assert!(cb.checked(), "click checked it");
        assert!(Element::take_click(&mut cb), "take_click reads once");
        assert!(!Element::take_click(&mut cb), "...then clears");
        assert!(Element::take_change(&mut cb));

        assert!(!ctx.propagate_event(&click_at(100.0, 100.0), ptr), "miss is not consumed");
        assert!(cb.checked(), "miss does not toggle");
    }

    #[test]
    fn checkbox_value_string_round_trip() {
        let mut cb = Checkbox::new();
        assert_eq!(Element::get_value_string(&cb), Some("false".to_string()));
        assert!(Element::set_value_string(&mut cb, "on"));
        assert!(cb.checked());
        assert_eq!(Element::value(&cb), 1);
        assert!(!Element::set_value_string(&mut cb, "on"), "unchanged value reports false");
        assert!(!Element::set_value_string(&mut cb, "junk"), "unparsable reports false");
        assert!(Element::take_change(&mut cb), "set_value_string marked the change");
    }

    /// Wide (labeled) mode reproduces the legacy `extra_quads` geometry through the bridge:
    /// box bg + 4 border edges (+ indicator when checked), and the label text with a
    /// hover-dependent color.
    #[test]
    fn checkbox_wide_mode_bridge_parity() {
        let ctx = UiContext::new();
        let mut cb = Checkbox::new().with_label("Enable");
        Element::set_rect(&mut cb, 0.0, 0.0, 200.0, 24.0);

        let quads = Element::extra_quads(&cb);
        // Unchecked: box bg + 4 border edges = 5 quads, at the legacy box position.
        assert_eq!(quads.len(), 5);
        let (box_x, box_y, box_size) = (200.0 - 18.0 - 8.0, (24.0 - 18.0) / 2.0, 18.0);
        assert_eq!(quads[0], (box_x, box_y, box_size, box_size, colors::checkbox_bg()));

        // Hover flips the box + border colors (tracked from MouseEnter, not base state).
        cb.inner_mut().hovered = true;
        let quads = Element::extra_quads(&cb);
        assert_eq!(quads[0].4, colors::checkbox_hover());

        // Label text comes through the prim-derived text bridge at the legacy position.
        let labels = Element::text_labels(&cb);
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].text, "Enable");
        assert_eq!(labels[0].x, 8.0);

        // Inline label => no set_rect inflation.
        assert_eq!(Element::rect(&cb), (0.0, 0.0, 200.0, 24.0));
        let _ = &ctx;
    }

    #[test]
    fn toggle_click_and_borders_switch_halves() {
        let mut ctx = UiContext::new();
        let mut t = Toggle::new();
        let (id, ptr) = (t.id(), t.as_ptr_mut());
        ctx.register_widget(id, ptr);
        Element::set_rect(&mut t, 0.0, 0.0, 60.0, 30.0);

        // Geometry parity is config-dependent (rounded vs square toggle); assert the invariant
        // that holds in both: the bordered half flips with the state.
        let before: Vec<_> = Element::all_rounded_quads(&t, &ctx);
        let before_quads = Element::extra_quads(&t);

        assert!(ctx.propagate_event(&click_at(30.0, 15.0), ptr), "toggle consumed the click");
        assert!(t.toggled());
        assert!(Element::take_click(&mut t));

        let after: Vec<_> = Element::all_rounded_quads(&t, &ctx);
        let after_quads = Element::extra_quads(&t);
        assert!(
            before != after || before_quads != after_quads,
            "toggling changes the emitted geometry (border switches halves)",
        );

        // preferred_height forwards the legacy toggle height.
        assert_eq!(Element::preferred_height(&t), Some(crate::layout::toggle_height()));
    }

    #[test]
    fn toggle_set_label_via_deref_reaches_paint() {
        let mut t = Toggle::new();
        Element::set_rect(&mut t, 0.0, 0.0, 60.0, 30.0);
        t.set_label("ON"); // the network.rs pattern: live label updates through Deref
        let labels = Element::text_labels(&t);
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].text, "ON");
    }
}
