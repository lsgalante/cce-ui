//! Narrow-trait pane splitter (Phase 5i leaf sweep). A self-moving widget: dragging repositions
//! the splitter itself via [`Input::drag_reposition`] (the adapter applies the new origin to the
//! base rect). Hover/drag drive the color, tracked from the forwarded events.

use crate::colors;
use crate::scene::layout::Rect;
use crate::widget::{Adapted, Element, ElementState, Event, EventCtx, Input, Layout, MouseButton, Paint};

pub struct Splitter {
    hovered: bool,
    dragging: bool,
    drag_ox: f32,
}

impl Splitter {
    pub fn new(w: f32) -> Adapted<Splitter> {
        let mut s = Adapted::new(Splitter { hovered: false, dragging: false, drag_ox: 0.0 });
        Element::set_rect(&mut s, 0.0, 0.0, w, 0.0);
        s
    }
}

impl Layout for Splitter {}

impl Paint for Splitter {
    fn color(&self) -> [f32; 4] {
        if self.dragging {
            colors::SPLITTER_DRAG
        } else if self.hovered {
            colors::SPLITTER_HOVER
        } else {
            colors::SPLITTER_IDLE
        }
    }
}

impl Input for Splitter {
    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, x, .. } => {
                self.dragging = true;
                self.drag_ox = x - ectx.rect.x;
                true
            }
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Released, .. } => {
                std::mem::take(&mut self.dragging)
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

    fn draggable(&self) -> bool {
        true
    }
    fn is_dragging(&self) -> bool {
        self.dragging
    }
    fn drag_begin(&mut self, px: f32, _py: f32, rect: Rect) {
        self.dragging = true;
        self.drag_ox = px - rect.x;
    }
    fn drag_reposition(&mut self, px: f32, _py: f32, rect: Rect) -> Option<(f32, f32)> {
        let new_x = px - self.drag_ox;
        if (new_x - rect.x).abs() > 0.5 {
            Some((new_x, rect.y))
        } else {
            None
        }
    }
    fn drag_end(&mut self) {
        self.dragging = false;
    }
}
