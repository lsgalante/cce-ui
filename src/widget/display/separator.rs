use crate::widget::*;

#[derive(Debug, Clone)]
pub struct Separator {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub color: [f32; 4],
}

impl Separator {
    pub fn new(x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> Self {
        Self { x, y, w, h, color }
    }
}

impl Element for Separator {
    fn rect(&self) -> (f32, f32, f32, f32) {
        (self.x, self.y, self.w, self.h)
    }
    fn as_ptr(&self) -> *mut (dyn Element + 'static) {
        self as *const Self as *mut Self as *mut (dyn Element + 'static)
    }
    fn as_ptr_mut(&mut self) -> *mut (dyn Element + 'static) {
        self as *mut Self as *mut (dyn Element + 'static)
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.x = x;
        self.y = y;
        self.w = w;
        self.h = h;
    }

    fn color(&self) -> [f32; 4] {
        self.color
    }
}
