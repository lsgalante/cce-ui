//! Narrow-trait movable panel (Phase 5i leaf sweep). Self-moving via
//! [`Input::drag_reposition`], with movement clamped to bounds pushed in through
//! [`Input::set_drag_bounds`] (or the inherent `set_bounds`).

use crate::colors;
use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::{Adapted, Element, ElementState, Event, EventCtx, Input, Layout, MouseButton, Paint};

pub struct Panel {
    dragging: bool,
    drag_ox: f32,
    drag_oy: f32,
    bounds: Option<(f32, f32, f32, f32)>,
}

impl Panel {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Adapted<Panel> {
        let mut p = Adapted::new(Panel { dragging: false, drag_ox: 0.0, drag_oy: 0.0, bounds: None });
        Element::set_rect(&mut p, x, y, w, h);
        p
    }

    pub fn set_bounds(&mut self, bx: f32, by: f32, bw: f32, bh: f32) {
        self.bounds = Some((bx, by, bw, bh));
    }
}

impl Layout for Panel {
    fn inline_label(&self) -> bool {
        true // legacy Panel never inflated for its label
    }
}

impl Paint for Panel {
    fn color(&self) -> [f32; 4] {
        if self.dragging { colors::PANEL_DRAG } else { colors::PANEL_IDLE }
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        ctx.quad(rect, self.color());
    }
}

impl Input for Panel {
    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, x, y, .. } => {
                self.dragging = true;
                self.drag_ox = x - ectx.rect.x;
                self.drag_oy = y - ectx.rect.y;
                true
            }
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Released, .. } => {
                std::mem::take(&mut self.dragging)
            }
            _ => false,
        }
    }

    fn draggable(&self) -> bool {
        true
    }
    fn is_dragging(&self) -> bool {
        self.dragging
    }
    fn drag_begin(&mut self, px: f32, py: f32, rect: Rect) {
        self.dragging = true;
        self.drag_ox = px - rect.x;
        self.drag_oy = py - rect.y;
    }
    fn drag_reposition(&mut self, px: f32, py: f32, rect: Rect) -> Option<(f32, f32)> {
        let nx = px - self.drag_ox;
        let ny = py - self.drag_oy;
        let (nx, ny) = if let Some((bx, by, bw, bh)) = self.bounds {
            (nx.clamp(bx, bx + bw - rect.width), ny.clamp(by, by + bh - rect.height))
        } else {
            (nx, ny)
        };
        if (nx - rect.x).abs() > 0.01 || (ny - rect.y).abs() > 0.01 {
            Some((nx, ny))
        } else {
            None
        }
    }
    fn drag_end(&mut self) {
        self.dragging = false;
    }
    fn set_drag_bounds(&mut self, bx: f32, by: f32, bw: f32, bh: f32) {
        self.bounds = Some((bx, by, bw, bh));
    }
}
