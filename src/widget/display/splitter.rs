use crate::colors;
use crate::widget::*;

pub struct Splitter {
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
    dragging: bool,
    drag_ox: f32,
}

impl Splitter {
    pub fn new(w: f32) -> Self {
        Self { x: 0.0, y: 0.0, w, h: 0.0, hovered: false, dragging: false, drag_ox: 0.0 }
    }
}

impl Element for Splitter {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] {
        if self.dragging { colors::SPLITTER_DRAG }
        else if self.hovered { colors::SPLITTER_HOVER }
        else { colors::SPLITTER_IDLE }
    }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }

    fn on_cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        let was = self.hovered;
        self.hovered = self.hit_test(px, py, ctx);
        was != self.hovered
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if button != MouseButton::Left { return false; }
        match state {
            ElementState::Pressed => {
                if self.hit_test(px, py, ctx) {
                    self.drag_begin(px, py);
                    return true;
                }
            }
            ElementState::Released => {
                if self.dragging { self.drag_end(); return true; }
            }
        }
        false
    }

    fn is_dragging(&self) -> bool { self.dragging }
    fn draggable(&self) -> bool { true }

    fn drag_update(&mut self, px: f32, _py: f32) -> bool {
        let new_x = px - self.drag_ox;
        if (new_x - self.x).abs() > 0.5 {
            self.x = new_x;
            return true;
        }
        false
    }

    fn drag_begin(&mut self, px: f32, _py: f32) {
        self.dragging = true;
        self.drag_ox = px - self.x;
    }

    fn drag_end(&mut self) { self.dragging = false; }
}
