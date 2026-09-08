//! `Slider2D` — a two-axis pad control: one thumb dragged across a recessed
//! square well maps to an `(x, y)` pair in 0..1 × 0..1, y-up. Follows the
//! `Slider` narrow-trait shape: a detached label that does NOT inflate the
//! rect (the label eats into the assigned rect), host-driven drags through
//! the `Input` drag hooks, and `take_change`-based polling by composites.

use crate::scene::layout::{Rect, Size};
use crate::scene::paint::PaintCtx;
use crate::widget::{
    Adapted, ElementState, Event, EventCtx, Input, Layout, MouseButton, MouseScrollDelta, Paint,
};

#[derive(Debug, Clone)]
pub struct Slider2D {
    dragging: bool,
    pub(crate) value_x: f32,
    pub(crate) value_y: f32,
    pub just_changed: bool,
    label: Option<String>,
}

impl Slider2D {
    /// Thumb radius; also the pad's inner inset so the thumb center's range
    /// keeps the whole thumb inside the well.
    const THUMB_R: f32 = 6.0;

    pub fn new() -> Adapted<Slider2D> {
        Adapted::new(Slider2D {
            dragging: false,
            value_x: 0.5,
            value_y: 0.5,
            just_changed: false,
            label: None,
        })
    }

    pub fn value_x(&self) -> f32 {
        self.value_x
    }

    pub fn value_y(&self) -> f32 {
        self.value_y
    }

    pub fn set_values(&mut self, x: f32, y: f32) {
        self.value_x = x.clamp(0.0, 1.0);
        self.value_y = y.clamp(0.0, 1.0);
    }

    fn thumb_center(&self, rect: Rect) -> (f32, f32) {
        let inset = Self::THUMB_R + 2.0;
        (
            rect.x + inset + self.value_x * (rect.width - 2.0 * inset).max(1.0),
            rect.y + inset + (1.0 - self.value_y) * (rect.height - 2.0 * inset).max(1.0),
        )
    }

    /// The detached-label strip height above the content rect — the Slider /
    /// Dropdown replica of `Widget::label_offset` over the synced label.
    fn label_top(&self) -> f32 {
        if crate::layout::control_label_layout() == "side" {
            return 0.0;
        }
        if self.label.is_some() {
            let (_, font_size) = crate::layout::control_label_font_detached_parsed();
            font_size + crate::layout::control_label_margin()
        } else {
            0.0
        }
    }

    fn set_from_point(&mut self, px: f32, py: f32, rect: Rect) -> bool {
        let inset = Self::THUMB_R + 2.0;
        let w = (rect.width - 2.0 * inset).max(1.0);
        let h = (rect.height - 2.0 * inset).max(1.0);
        let nx = ((px - rect.x - inset) / w).clamp(0.0, 1.0);
        let ny = (1.0 - (py - rect.y - inset) / h).clamp(0.0, 1.0);
        if (nx - self.value_x).abs() > 0.0001 || (ny - self.value_y).abs() > 0.0001 {
            self.value_x = nx;
            self.value_y = ny;
            self.just_changed = true;
            true
        } else {
            false
        }
    }
}

impl Layout for Slider2D {
    fn inflates_label_rect(&self) -> bool {
        false // the Slider rule: the detached label eats into the assigned rect
    }


    /// A 64px pad, or wide enough for its label's carve-out tab (the tab is clipped
    /// to the pad and the label spilled past a 64px one).
    fn intrinsic_size(&self) -> Option<Size> {
        let label_w = crate::widget::input::slider::detached_label_width(&self.label);
        let tab_w = if label_w > 0.0 { label_w + 2.0 * crate::layout::DETACHED_LABEL_INSET + 16.0 } else { 0.0 };
        Some(Size::new(64.0_f32.max(tab_w), 64.0))
    }

    fn intrinsic_measure_width(&self) -> bool {
        true
    }
}

impl Paint for Slider2D {
    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::control_label_font_detached())
    }

    fn sync_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        // The well: a dark floor read as an opening cut into the host's plate
        // (the ramp graph's look in miniature), recess rim drawn last so its
        // shading falls over the content at the edges.
        let radius = crate::layout::slider_corner_radius().max(4.0);
        ctx.rounded_rect(rect, radius, (true, true, true, true), [0.08, 0.08, 0.10, 1.0]);

        // Crosshair through the thumb — the pad's read of both axis values.
        let (cx, cy) = self.thumb_center(rect);
        let line = [0.25, 0.25, 0.28, 0.6];
        ctx.quad(Rect { x: rect.x + 2.0, y: cy - 0.5, width: rect.width - 4.0, height: 1.0 }, line);
        ctx.quad(Rect { x: cx - 0.5, y: rect.y + 2.0, width: 1.0, height: rect.height - 4.0 }, line);

        // Thumb: glassy fill in a thin white ring (the ramp peg look, small).
        let fill_a = if self.dragging { 0.9 } else { 0.35 };
        ctx.circle(cx, cy, Self::THUMB_R - 1.0, [0.5, 0.75, 1.0, fill_a]);
        ctx.arc(
            cx,
            cy,
            Self::THUMB_R + 1.0,
            2.0,
            0.0,
            std::f32::consts::TAU,
            [1.0, 1.0, 1.0, 0.9],
        );

        let depth = crate::layout::bevel_width().min(rect.height * 0.2);
        // The well's ring, with a labeled pad's label in a carve-out tab — the one
        // labeled-well composition the sliders share (`carve_labeled_well`).
        crate::widget::input::slider::carve_labeled_well(
            ctx,
            rect,
            self.label_top(),
            crate::widget::input::slider::detached_label_width(&self.label),
            radius,
            depth,
        );
    }
}

impl Input for Slider2D {
    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button: MouseButton::Left, state, x: px, y: py, .. } => {
                match state {
                    ElementState::Pressed => {
                        let r = ectx.rect;
                        if *px >= r.x && *px <= r.x + r.width && *py >= r.y && *py <= r.y + r.height {
                            self.dragging = true;
                            self.set_from_point(*px, *py, r);
                            return true;
                        }
                        false
                    }
                    ElementState::Released => std::mem::take(&mut self.dragging),
                }
            }
            Event::PointerMove { x: px, y: py, .. } => {
                if self.dragging {
                    self.set_from_point(*px, *py, ectx.rect)
                } else {
                    false
                }
            }
            Event::MouseWheel { delta, x: px, y: py, .. } => {
                // Vertical wheel nudges the y axis (the Slider gesture-gated
                // pattern), hovering anywhere over the pad.
                if let Some(ui) = ectx.ui.as_deref_mut() {
                    if !ui.scroll_gesture_new && ui.scroll_initiate_widget_id != Some(ectx.id) {
                        return false;
                    }
                    let r = ectx.rect;
                    if *px >= r.x && *px <= r.x + r.width && *py >= r.y && *py <= r.y + r.height {
                        if ui.scroll_gesture_new {
                            ui.scroll_initiate_widget_id = Some(ectx.id);
                        }
                        let amount = match delta {
                            MouseScrollDelta::LineDelta(_x, y) => *y,
                            MouseScrollDelta::PixelDelta(pos) => (pos.y as f32) / 120.0,
                        };
                        let ny = (self.value_y - amount * 0.02).clamp(0.0, 1.0);
                        if (ny - self.value_y).abs() > 0.0001 {
                            self.value_y = ny;
                            self.just_changed = true;
                        }
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }

    fn opens_context_menu(&self) -> bool {
        true
    }

    fn draggable(&self, _rect: Rect) -> bool {
        true
    }
    fn is_dragging(&self) -> bool {
        self.dragging
    }
    fn drag_begin(&mut self, px: f32, py: f32, rect: Rect) {
        self.dragging = true;
        self.set_from_point(px, py, rect);
    }
    fn drag_update(&mut self, px: f32, py: f32, rect: Rect) -> bool {
        self.set_from_point(px, py, rect)
    }
    fn drag_end(&mut self) {
        self.dragging = false;
    }

    fn take_change(&mut self) -> bool {
        std::mem::take(&mut self.just_changed)
    }

    fn value_string(&self) -> Option<String> {
        Some(format!("{:.3},{:.3}", self.value_x, self.value_y))
    }

    fn set_value_string(&mut self, val: &str) -> bool {
        let Some((xs, ys)) = val.split_once(',') else { return false };
        let (Ok(x), Ok(y)) = (xs.trim().parse::<f32>(), ys.trim().parse::<f32>()) else {
            return false;
        };
        let (x, y) = (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0));
        let changed = (x - self.value_x).abs() > 0.0005 || (y - self.value_y).abs() > 0.0005;
        self.value_x = x;
        self.value_y = y;
        if changed {
            self.just_changed = true;
        }
        changed
    }
}
