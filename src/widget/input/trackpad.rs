use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::model::{Adapted, EventCtx, Input, Layout, Paint};
use crate::widget::*;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Finger {
    pub slot: usize,
    pub x: f32,
    pub y: f32,
}

/// Touchpad visualization/input area (narrow-trait model, Phase 6as leaf sweep). The
/// content rect is cached on assignment (the ParametersBg pattern) because the finger
/// math runs from events and drags as well as paint; the control label is the adapter's
/// detached one above the pad, like every control's.
#[derive(Debug, Clone)]
pub struct Trackpad {
    rect: Rect,
    label: Option<String>,
    hovered: bool,
    pub fingers: Vec<Finger>,
    /// Recessed style: the touch area is a well carved into the plate below,
    /// a faint dark wash for its floor, instead of the framed dark pane.
    /// Defaults to `control_relief()`.
    recessed: Option<bool>,
}

impl Trackpad {
    /// The style in force: the per-widget override (`with_recessed`) when set, else
    /// the DE's `control_relief`, read live so a runtime switch
    /// (`layout::set_control_relief`) restyles every control at once.
    fn recessed(&self) -> bool {
        self.recessed.unwrap_or_else(crate::layout::control_relief)
    }

    pub fn new() -> Adapted<Trackpad> {
        Adapted::new(Trackpad {
            rect: Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 },
            label: None,
            hovered: false,
            fingers: Vec::new(),
            recessed: None,
        })
    }

    pub fn set_fingers(&mut self, fingers: Vec<Finger>) {
        self.fingers = fingers;
    }
}

impl Adapted<Trackpad> {
    /// Recessed style: see the `recessed` field.
    pub fn with_recessed(mut self, recessed: bool) -> Self {
        self.recessed = Some(recessed);
        self
    }
}

impl Trackpad {
    /// The touch area: the cached block rect less the detached-label strip above it.
    fn touch_area(&self) -> (f32, f32, f32, f32) {
        let top = crate::widget::input::slider::detached_strip(&self.label);
        (self.rect.x, self.rect.y + top, self.rect.width, self.rect.height - top)
    }

    fn finger_at(&self, px: f32, py: f32) -> Finger {
        let (x, y, w, h) = self.touch_area();
        Finger {
            slot: 0,
            x: ((px - x) / w).clamp(0.0, 1.0),
            y: ((py - y) / h).clamp(0.0, 1.0),
        }
    }
}

impl Layout for Trackpad {
    fn rect_assigned(&mut self, rect: Rect) {
        self.rect = rect;
    }
}

impl Paint for Trackpad {
    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::control_label_font())
    }

    fn color(&self) -> [f32; 4] {
        // The floor is the canvas well's (`paint`); no base fill of its own.
        [0.0, 0.0, 0.0, 0.0]
    }

    fn sync_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let _ = rect; // geometry reads the assignment cache (events/drags share it)
        let (x, y, w, visual_h) = self.touch_area();

        let area = Rect { x, y, width: w, height: visual_h };
        // 1+2. A canvas well in the plate — the floor tint and rim every draw-in
        // opening shares (`PaintCtx::canvas_well`), rounded like the text wells;
        // the fingers draw over the rim.
        let radius = crate::layout::textbox_corner_radius();
        ctx.canvas_well(area, radius, self.recessed(), false);

        // 3. Fingers
        for finger in &self.fingers {
            let rx = finger.x.clamp(0.0, 1.0);
            let ry = finger.y.clamp(0.0, 1.0);
            let fx = x + rx * w;
            let fy = y + ry * visual_h;
            let dot_size = 12.0;

            // Glow (outer light blue), then core (solid blue/purple)
            ctx.quad(
                Rect {
                    x: fx - (dot_size + 6.0) / 2.0,
                    y: fy - (dot_size + 6.0) / 2.0,
                    width: dot_size + 6.0,
                    height: dot_size + 6.0,
                },
                [0.35, 0.55, 0.95, 0.4],
            );
            ctx.quad(
                Rect { x: fx - dot_size / 2.0, y: fy - dot_size / 2.0, width: dot_size, height: dot_size },
                [0.45, 0.65, 1.0, 1.0],
            );
        }

        // 4. The "Touchpad Area" hint (the control label is the adapter's), in
        //    the label font like every other word a control draws.
        ctx.text(
            "Touchpad Area".to_string(),
            x + 12.0,
            y + visual_h - 22.0,
            11.0,
            [0x73, 0x73, 0x8c],
        );
    }
}

impl Input for Trackpad {
    fn on_event(&mut self, event: &Event, _ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button, state, x, y, .. } => {
                if *button != MouseButton::Left {
                    return false;
                }
                let (ax, ay, aw, ah) = self.touch_area();
                if *x >= ax && *x <= ax + aw && *y >= ay && *y <= ay + ah {
                    if *state == ElementState::Pressed {
                        self.fingers = vec![self.finger_at(*x, *y)];
                    } else {
                        self.fingers.clear();
                    }
                    return true;
                }
                false
            }
            Event::MouseEnter => {
                self.hovered = true;
                false
            }
            Event::MouseLeave => {
                self.hovered = false;
                false
            }
            _ => false,
        }
    }

    fn draggable(&self, _rect: Rect) -> bool {
        true
    }

    fn is_dragging(&self) -> bool {
        !self.fingers.is_empty()
    }

    fn drag_begin(&mut self, px: f32, py: f32, _rect: Rect) {
        let (_, _, w, h) = self.touch_area();
        if w > 0.0 && h > 0.0 {
            self.fingers = vec![self.finger_at(px, py)];
        }
    }

    fn drag_update(&mut self, px: f32, py: f32, _rect: Rect) -> bool {
        let (_, _, w, h) = self.touch_area();
        if w > 0.0 && h > 0.0 {
            self.fingers = vec![self.finger_at(px, py)];
            true
        } else {
            false
        }
    }

    fn drag_end(&mut self) {
        self.fingers.clear();
    }
}
