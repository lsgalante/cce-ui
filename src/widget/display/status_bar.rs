use crate::colors;
use crate::widget::*;
use crate::widget::display::{TextLabel, make_widget_text_buffer};
use crate::context::UiContext;

pub struct StatusBar {
    pub base: Widget,
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
    pub text: String,
    pub text_buf: Option<glyphon::Buffer>,
    pub text_offset_x: Option<f32>,
    pub text_color: Option<[f32; 4]>,
    pub bg_color: Option<[f32; 4]>,
    pub parent: Option<*mut (dyn Element + 'static)>,
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            base: Widget::new_rect(0.0, 0.0, 0.0, 0.0),
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
            parent: None,
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

    pub fn get_actual_text_color(&self) -> [f32; 4] {
        if let Some(p_ptr) = self.parent {
            if unsafe { (*p_ptr).is_backplate() } {
                return crate::colors::backplate_statusbar_text_color();
            }
        }
        self.text_color.unwrap_or([0.6666, 0.6666, 0.7333, 1.0])
    }

    pub fn is_blur_enabled(&self) -> bool {
        if let Some(p_ptr) = self.parent {
            if unsafe { (*p_ptr).is_backplate() } {
                return crate::colors::backplate_statusbar_blur();
            }
        }
        false
    }
}

impl Element for StatusBar {
    fn base(&self) -> Option<&Widget> { Some(&self.base) }
    fn base_mut(&mut self) -> Option<&mut Widget> { Some(&mut self.base) }
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.x = x; self.y = y; self.w = w; self.h = h;
        self.base.x = x; self.base.y = y; self.base.w = w; self.base.h = h;
    }
    fn as_ptr(&self) -> *mut (dyn Element + 'static) {
        self as *const Self as *mut Self as *mut (dyn Element + 'static)
    }
    fn as_ptr_mut(&mut self) -> *mut (dyn Element + 'static) {
        self as *mut Self as *mut (dyn Element + 'static)
    }
    fn color(&self) -> [f32; 4] {
        if let Some(p_ptr) = self.parent {
            if unsafe { (*p_ptr).is_backplate() } {
                return crate::colors::backplate_statusbar_color();
            }
        }
        self.bg_color.unwrap_or(colors::STATUS_BG)
    }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }
    fn blocks_backplate_drag(&self) -> bool { false }

    fn parent(&self, _ctx: &UiContext) -> Option<*mut (dyn Element + 'static)> {
        self.parent
    }

    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>, _ctx: &mut UiContext) {
        self.parent = parent;
    }

    fn rounded_corners(&self) -> (bool, bool, bool, bool) {
        if let Some(p_ptr) = self.parent {
            let is_bp = unsafe { (*p_ptr).is_backplate() };
            if is_bp {
                let (px, py, pw, ph) = unsafe { (*p_ptr).rect() };
                let (x, y, w, h) = self.rect();
                let is_at_top = (y - py).abs() < 0.1;
                let is_at_bottom = (y + h - (py + ph)).abs() < 0.1;
                
                if is_at_top && is_at_bottom {
                    let is_at_left = (x - px).abs() < 0.1;
                    let is_at_right = (x + w - (px + pw)).abs() < 0.1;
                    return (is_at_left, is_at_right, is_at_right, is_at_left);
                } else if is_at_top {
                    return (true, true, false, false);
                } else if is_at_bottom {
                    return (false, false, true, true);
                }
            }
        }
        (false, false, false, false)
    }

    fn corner_radius(&self) -> f32 {
        if let Some(p_ptr) = self.parent {
            unsafe { (*p_ptr).corner_radius() }
        } else {
            0.0
        }
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let (r1, r2, r3, r4) = self.rounded_corners();
        if !r1 && !r2 && !r3 && !r4 {
            vec![(self.x, self.y, self.w, self.h, self.color())]
        } else {
            Vec::new()
        }
    }
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
            let c = self.get_actual_text_color();
            let color = glyphon::Color::rgb(
                (c[0] * 255.0) as u8,
                (c[1] * 255.0) as u8,
                (c[2] * 255.0) as u8,
            );
            vec![(text_buf, self.x + offset_x, self.y + 4.0, color)]
        } else {
            Vec::new()
        }
    }
    fn text_labels(&self) -> Vec<TextLabel> {
        if !self.text.is_empty() {
            let offset_x = self.text_offset_x.unwrap_or(12.0);
            let c = self.get_actual_text_color();
            let color = [
                (c[0] * 255.0) as u8,
                (c[1] * 255.0) as u8,
                (c[2] * 255.0) as u8,
            ];
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
