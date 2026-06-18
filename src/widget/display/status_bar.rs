use crate::colors;
use crate::widget::*;
use crate::widget::display::{TextLabel, make_widget_text_buffer};

pub struct StatusBar {
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
    pub text: String,
    pub text_buf: Option<glyphon::Buffer>,
    pub text_offset_x: Option<f32>,
    pub text_color: Option<[f32; 4]>,
    pub bg_color: Option<[f32; 4]>,
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            hovered: false,
            text: String::new(),
            text_buf: None,
            text_offset_x: None,
            text_color: None,
            bg_color: None,
        }
    }
    pub fn with_text(mut self, text: &str) -> Self {
        self.text = text.to_string();
        self
    }
    pub fn with_text_offset_x(mut self, offset: f32) -> Self {
        self.text_offset_x = Some(offset);
        self
    }
    pub fn with_text_color(mut self, color: [f32; 4]) -> Self {
        self.text_color = Some(color);
        self
    }
    pub fn with_bg_color(mut self, color: [f32; 4]) -> Self {
        self.bg_color = Some(color);
        self
    }
    pub fn set_text_offset_x(&mut self, offset: f32) {
        self.text_offset_x = Some(offset);
    }
    pub fn set_text_color(&mut self, color: [f32; 4]) {
        self.text_color = Some(color);
    }
    pub fn set_bg_color(&mut self, color: [f32; 4]) {
        self.bg_color = Some(color);
    }
}

impl Element for StatusBar {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn as_ptr(&self) -> *mut (dyn Element + 'static) {
        self as *const Self as *mut Self as *mut (dyn Element + 'static)
    }
    fn as_ptr_mut(&mut self) -> *mut (dyn Element + 'static) {
        self as *mut Self as *mut (dyn Element + 'static)
    }
    fn color(&self) -> [f32; 4] { self.bg_color.unwrap_or(colors::STATUS_BG) }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }
    fn set_text(&mut self, text: &str) {
        if self.text != text {
            self.text = text.to_string();
            self.text_buf = None;
        }
    }
    fn prepare_text(&mut self, fs: &mut glyphon::FontSystem) {
        if !self.text.is_empty() && self.text_buf.is_none() {
            self.text_buf = Some(make_widget_text_buffer(fs, &self.text, 12.0, "Outfit"));
        }
    }
    fn get_text_items(&self) -> Vec<(&glyphon::Buffer, f32, f32, glyphon::Color)> {
        if let Some(ref text_buf) = self.text_buf {
            let offset_x = self.text_offset_x.unwrap_or(12.0);
            let color = self.text_color.map(|c| glyphon::Color::rgb(
                (c[0] * 255.0) as u8,
                (c[1] * 255.0) as u8,
                (c[2] * 255.0) as u8,
            )).unwrap_or_else(|| glyphon::Color::rgb(0xaa, 0xaa, 0xbb));
            vec![(text_buf, self.x + offset_x, self.y + 4.0, color)]
        } else {
            Vec::new()
        }
    }
    fn text_labels(&self) -> Vec<TextLabel> {
        if !self.text.is_empty() {
            let offset_x = self.text_offset_x.unwrap_or(12.0);
            let color = self.text_color.map(|c| [
                (c[0] * 255.0) as u8,
                (c[1] * 255.0) as u8,
                (c[2] * 255.0) as u8,
            ]).unwrap_or([0xaa, 0xaa, 0xbb]);
            vec![TextLabel {
                text: self.text.clone(),
                x: self.x + offset_x,
                y: self.y + 4.0,
                font_size: 12.0,
                color,
            }]
        } else {
            Vec::new()
        }
    }
}
