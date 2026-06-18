use crate::colors;
use crate::widget::*;

pub struct Header {
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
}

impl Header {
    pub fn new() -> Self { Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, hovered: false } }
}

impl Element for Header {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { colors::HEADER_BG }
    fn as_ptr(&self) -> *mut (dyn Element + 'static) {
        self as *const Self as *mut Self as *mut (dyn Element + 'static)
    }
    fn as_ptr_mut(&mut self) -> *mut (dyn Element + 'static) {
        self as *mut Self as *mut (dyn Element + 'static)
    }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }
}
