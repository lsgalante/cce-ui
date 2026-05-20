use winit::event::{ElementState, KeyEvent, MouseButton};
use winit::keyboard::{Key, NamedKey};

use crate::colors;

pub struct TextLabel {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
    pub color: [u8; 3],
}

pub trait Widget {
    fn rect(&self) -> (f32, f32, f32, f32);
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32);

    fn hit_test(&self, px: f32, py: f32) -> bool {
        let (x, y, w, h) = self.rect();
        px >= x && px <= x + w && py >= y && py <= y + h
    }

    fn cursor_moved(&mut self, _px: f32, _py: f32) -> bool { false }
    fn mouse_input(&mut self, _button: MouseButton, _state: ElementState, _px: f32, _py: f32) -> bool { false }

    fn color(&self) -> [f32; 4];

    fn is_dragging(&self) -> bool { false }
    fn drag_update(&mut self, _px: f32, _py: f32) -> bool { false }
    fn drag_begin(&mut self, _px: f32, _py: f32) {}
    fn drag_end(&mut self) {}
    fn take_click(&mut self) -> bool { false }
    fn draggable(&self) -> bool { false }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> { Vec::new() }
    fn text_labels(&self) -> Vec<TextLabel> { Vec::new() }
    fn value(&self) -> i32 { 0 }

    fn focus(&mut self) {}
    fn unfocus(&mut self) {}
    fn keyboard_input(&mut self, _event: &KeyEvent) -> bool { false }
}

pub struct Header {
    x: f32, y: f32, w: f32, h: f32,
}

impl Header {
    pub fn new() -> Self { Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0 } }
}

impl Widget for Header {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { colors::HEADER_BG }
}

pub struct ContentBg {
    x: f32, y: f32, w: f32, h: f32,
}

impl ContentBg {
    pub fn new() -> Self { Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0 } }
}

impl Widget for ContentBg {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { colors::CONTENT_BG }
}

pub enum ButtonKind {
    Primary,
    Reset,
}

pub struct Button {
    x: f32, y: f32, w: f32, h: f32,
    hovering: bool,
    pressed: bool,
    just_clicked: bool,
    kind: ButtonKind,
}

impl Button {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h, hovering: false, pressed: false, just_clicked: false, kind: ButtonKind::Primary }
    }

    pub fn new_reset(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h, hovering: false, pressed: false, just_clicked: false, kind: ButtonKind::Reset }
    }
}

impl Widget for Button {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] {
        match self.kind {
            ButtonKind::Primary => {
                if self.pressed { colors::BUTTON_PRESS }
                else if self.hovering { colors::BUTTON_HOVER }
                else { colors::BUTTON_IDLE }
            }
            ButtonKind::Reset => {
                if self.pressed { colors::RESET_BTN_PRESS }
                else if self.hovering { colors::RESET_BTN_HOVER }
                else { colors::RESET_BTN_IDLE }
            }
        }
    }

    fn cursor_moved(&mut self, px: f32, py: f32) -> bool {
        let was = self.hovering;
        self.hovering = self.hit_test(px, py);
        was != self.hovering
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button != MouseButton::Left { return false; }
        match state {
            ElementState::Pressed => {
                if self.hit_test(px, py) {
                    self.pressed = true;
                    return true;
                }
            }
            ElementState::Released => {
                if self.pressed && self.hit_test(px, py) {
                    self.just_clicked = true;
                }
                let was = self.pressed;
                self.pressed = false;
                return was;
            }
        }
        false
    }

    fn take_click(&mut self) -> bool {
        if self.just_clicked { self.just_clicked = false; true } else { false }
    }
}

pub enum PageButton {
    Active,
    Inactive,
}

pub struct Sidebar {
    x: f32, y: f32, w: f32, h: f32,
}

impl Sidebar {
    pub fn new(w: f32) -> Self { Self { x: 0.0, y: 0.0, w, h: 0.0 } }
}

impl Widget for Sidebar {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { colors::SIDEBAR_BG }
}

pub struct Panel {
    x: f32, y: f32, w: f32, h: f32,
    dragging: bool,
    drag_ox: f32, drag_oy: f32,
    drag_start_x: f32, drag_start_y: f32,
}

impl Panel {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h, dragging: false, drag_ox: 0.0, drag_oy: 0.0, drag_start_x: 0.0, drag_start_y: 0.0 }
    }
}

impl Widget for Panel {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { if self.dragging { colors::PANEL_DRAG } else { colors::PANEL_IDLE } }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button != MouseButton::Left { return false; }
        match state {
            ElementState::Pressed => {
                if self.hit_test(px, py) {
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

    fn drag_update(&mut self, px: f32, py: f32) -> bool {
        let nx = px - self.drag_ox;
        let ny = py - self.drag_oy;
        if (nx - self.x).abs() > 0.01 || (ny - self.y).abs() > 0.01 {
            self.x = nx;
            self.y = ny;
            return true;
        }
        false
    }

    fn drag_begin(&mut self, px: f32, py: f32) {
        self.dragging = true;
        self.drag_ox = px - self.x;
        self.drag_oy = py - self.y;
        self.drag_start_x = self.x;
        self.drag_start_y = self.y;
    }

    fn drag_end(&mut self) { self.dragging = false; }
}

pub struct Checkbox {
    x: f32, y: f32, w: f32, h: f32,
    hovering: bool,
    checked: bool,
    just_clicked: bool,
}

impl Checkbox {
    pub fn new() -> Self {
        Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, hovering: false, checked: false, just_clicked: false }
    }
}

impl Widget for Checkbox {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { colors::CHECKBOX_BG }

    fn cursor_moved(&mut self, px: f32, py: f32) -> bool {
        let was = self.hovering;
        self.hovering = self.hit_test(px, py);
        was != self.hovering
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button != MouseButton::Left { return false; }
        match state {
            ElementState::Released => {
                if self.hit_test(px, py) {
                    self.checked = !self.checked;
                    self.just_clicked = true;
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    fn take_click(&mut self) -> bool {
        if self.just_clicked { self.just_clicked = false; true } else { false }
    }
}

pub struct Toggle {
    x: f32, y: f32, w: f32, h: f32,
    hovering: bool,
    toggled: bool,
    just_toggled: bool,
}

impl Toggle {
    pub fn new() -> Self {
        Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, hovering: false, toggled: false, just_toggled: false }
    }
}

impl Widget for Toggle {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { if self.toggled { colors::TOGGLE_ON } else { colors::TOGGLE_OFF } }

    fn cursor_moved(&mut self, px: f32, py: f32) -> bool {
        let was = self.hovering;
        self.hovering = self.hit_test(px, py);
        was != self.hovering
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button != MouseButton::Left { return false; }
        match state {
            ElementState::Released => {
                if self.hit_test(px, py) {
                    self.toggled = !self.toggled;
                    self.just_toggled = true;
                    return true;
                }
            }
            _ => {}
        }
        false
    }
}

pub struct Slider {
    x: f32, y: f32, w: f32, h: f32,
    dragging: bool,
    value: f32,
    drag_offset: f32,
}

impl Slider {
    pub fn new() -> Self {
        Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, dragging: false, value: 0.5, drag_offset: 0.0 }
    }
}

impl Widget for Slider {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { colors::SLIDER_TRACK }

    fn draggable(&self) -> bool { true }
    fn is_dragging(&self) -> bool { self.dragging }

    fn drag_update(&mut self, px: f32, _py: f32) -> bool {
        let (sx, _, sw, _) = self.rect();
        let thumb_size = self.h * 0.9;
        let range = sw - thumb_size;
        let raw = (px - self.drag_offset - sx) / range;
        let new_val = raw.clamp(0.0, 1.0);
        if (new_val - self.value).abs() > 0.001 {
            self.value = new_val;
            return true;
        }
        false
    }

    fn drag_begin(&mut self, px: f32, _py: f32) {
        self.dragging = true;
        let thumb_size = self.h * 0.9;
        let thumb_x = self.x + self.value * (self.w - thumb_size);
        self.drag_offset = px - thumb_x;
    }

    fn drag_end(&mut self) { self.dragging = false; }
}

pub struct ProgressBar {
    x: f32, y: f32, w: f32, h: f32,
    value: f32,
}

impl ProgressBar {
    pub fn new(value: f32) -> Self {
        Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, value }
    }
}

impl Widget for ProgressBar {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { colors::PROGRESS_BG }
}

pub struct StatusBar {
    x: f32, y: f32, w: f32, h: f32,
}

impl StatusBar {
    pub fn new() -> Self { Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0 } }
}

impl Widget for StatusBar {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { colors::STATUS_BG }
}

#[derive(Debug, Clone)]
pub struct Spinbox {
    x: f32, y: f32, w: f32, h: f32,
    pub value: i32,
    min: i32,
    max: i32,
    step: i32,
    editing: bool,
    edit_buffer: String,
    hover_dec: bool,
    hover_inc: bool,
}

impl Spinbox {
    pub fn new(value: i32, min: i32, max: i32, step: i32) -> Self {
        Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, value, min, max, step, editing: false, edit_buffer: String::new(), hover_dec: false, hover_inc: false }
    }
}

impl Widget for Spinbox {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { colors::SPINBOX_BG }
    fn value(&self) -> i32 { self.value }

    fn cursor_moved(&mut self, px: f32, _py: f32) -> bool {
        if !self.hit_test(px, _py) {
            let changed = self.hover_dec || self.hover_inc;
            self.hover_dec = false;
            self.hover_inc = false;
            return changed;
        }
        let split = self.x + self.w * 0.55;
        let hd = px >= split && px < split + self.w * 0.225;
        let hi = px >= split + self.w * 0.225;
        let changed = hd != self.hover_dec || hi != self.hover_inc;
        self.hover_dec = hd;
        self.hover_inc = hi;
        changed
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button != MouseButton::Left { return false; }
        if !self.hit_test(px, py) { return false; }
        match state {
            ElementState::Pressed => {
                let split = self.x + self.w * 0.55;
                if px >= split && px < split + self.w * 0.225 {
                    self.value = (self.value + self.step).min(self.max);
                    true
                } else if px >= split + self.w * 0.225 {
                    self.value = (self.value - self.step).max(self.min);
                    true
                } else if px < split {
                    self.editing = true;
                    self.edit_buffer = self.value.to_string();
                    true
                } else {
                    false
                }
            }
            ElementState::Released => {
                false
            }
        }
    }

    fn focus(&mut self) {
        self.editing = true;
        self.edit_buffer = self.value.to_string();
    }

    fn unfocus(&mut self) {
        if self.editing {
            self.editing = false;
            if let Ok(val) = self.edit_buffer.parse::<i32>() {
                self.value = val.clamp(self.min, self.max);
            }
        }
    }

    fn keyboard_input(&mut self, event: &KeyEvent) -> bool {
        if !self.editing { return false; }
        if event.state != ElementState::Pressed { return false; }
        match &event.logical_key {
            Key::Named(NamedKey::Backspace) => {
                self.edit_buffer.pop();
                true
            }
            Key::Named(NamedKey::Enter) => {
                if let Ok(val) = self.edit_buffer.parse::<i32>() {
                    self.value = val.clamp(self.min, self.max);
                }
                self.editing = false;
                true
            }
            Key::Named(NamedKey::Escape) => {
                self.editing = false;
                true
            }
            _ => {
                if let Some(text) = &event.text {
                    if !event.repeat {
                        for ch in text.chars() {
                            match ch {
                                '-' if self.edit_buffer.is_empty() => self.edit_buffer.push('-'),
                                '0'..='9' => self.edit_buffer.push(ch),
                                _ => {}
                            }
                        }
                    }
                }
                true
            }
        }
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let split = self.x + self.w * 0.55;
        let btn_w = self.w * 0.225;
        let inc_col = if self.hover_inc { colors::SPINBOX_BUTTON_HOVER } else { colors::SPINBOX_BUTTON };
        let dec_col = if self.hover_dec { colors::SPINBOX_BUTTON_HOVER } else { colors::SPINBOX_BUTTON };
        vec![
            (self.x, self.y, self.w * 0.55, self.h, colors::SPINBOX_DISPLAY),
            (split, self.y, btn_w, self.h, inc_col),
            (split + btn_w, self.y, btn_w, self.h, dec_col),
        ]
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let value_text = if self.editing { self.edit_buffer.clone() } else { self.value.to_string() };
        vec![
            TextLabel {
                text: value_text,
                x: self.x + 4.0,
                y: self.y + 2.0,
                font_size: 14.0,
                color: [0xcc, 0xcc, 0xd4],
            },
            TextLabel {
                text: "+".to_string(),
                x: self.x + self.w * 0.6625 - 4.0,
                y: self.y + 2.0,
                font_size: 12.0,
                color: [0xcc, 0xcc, 0xd4],
            },
            TextLabel {
                text: "-".to_string(),
                x: self.x + self.w * 0.8875 - 4.0,
                y: self.y + 2.0,
                font_size: 12.0,
                color: [0xcc, 0xcc, 0xd4],
            },
        ]
    }
}

#[derive(Debug, Clone)]
pub struct ColorPicker {
    x: f32, y: f32, w: f32, h: f32,
    pub color: [u8; 3],
    just_clicked: bool,
    editing: bool,
    edit_buffer: String,
    label: Option<String>,
}

impl ColorPicker {
    pub fn new(color: [u8; 3]) -> Self {
        Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, color, just_clicked: false, editing: false, edit_buffer: String::new(), label: None }
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.label = Some(label.to_string());
        self
    }
}

impl Widget for ColorPicker {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }

    fn color(&self) -> [f32; 4] {
        [self.color[0] as f32 / 255.0, self.color[1] as f32 / 255.0, self.color[2] as f32 / 255.0, 1.0]
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button != MouseButton::Left { return false; }
        if state != ElementState::Pressed { return false; }
        if !self.hit_test(px, py) { return false; }
        if px >= self.x + self.w * 0.65 {
            self.just_clicked = true;
            return true;
        }
        self.focus();
        true
    }

    fn take_click(&mut self) -> bool {
        if self.just_clicked { self.just_clicked = false; true } else { false }
    }

    fn focus(&mut self) {
        self.editing = true;
        self.edit_buffer = format!("#{:02x}{:02x}{:02x}", self.color[0], self.color[1], self.color[2]);
    }

    fn unfocus(&mut self) {
        if self.editing {
            self.editing = false;
            if let Some(c) = parse_hex(&self.edit_buffer) {
                self.color = c;
            }
        }
    }

    fn keyboard_input(&mut self, event: &KeyEvent) -> bool {
        if !self.editing { return false; }
        if event.state != ElementState::Pressed { return false; }
        match &event.logical_key {
            Key::Named(NamedKey::Backspace) => {
                self.edit_buffer.pop();
                true
            }
            Key::Named(NamedKey::Enter) => {
                if let Some(c) = parse_hex(&self.edit_buffer) {
                    self.color = c;
                }
                self.editing = false;
                true
            }
            Key::Named(NamedKey::Escape) => {
                self.editing = false;
                true
            }
            _ => {
                if let Some(text) = &event.text {
                    if !event.repeat {
                        for ch in text.chars() {
                            match ch {
                                '#' if self.edit_buffer.is_empty() => self.edit_buffer.push('#'),
                                '0'..='9' | 'a'..='f' | 'A'..='F' => {
                                    if self.edit_buffer.len() < 7 { self.edit_buffer.push(ch.to_ascii_lowercase()); }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                true
            }
        }
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let pick_x = self.x + self.w * 0.65;
        let pick_w = self.w * 0.35;
        let (r, g, b, _) = (self.color[0] as f32 / 255.0, self.color[1] as f32 / 255.0, self.color[2] as f32 / 255.0, 1.0);
        vec![
            (pick_x, self.y, pick_w, self.h, [r, g, b, 1.0]),
        ]
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        if let Some(ref label) = self.label {
            labels.push(TextLabel {
                text: label.clone(),
                x: self.x + 4.0,
                y: self.y - 16.0,
                font_size: 12.0,
                color: [0x83, 0x83, 0x8a],
            });
        }
        let hex = if self.editing { self.edit_buffer.clone() } else { format!("#{:02x}{:02x}{:02x}", self.color[0], self.color[1], self.color[2]) };
        labels.push(TextLabel {
            text: hex,
            x: self.x + 4.0,
            y: self.y + 3.0,
            font_size: 12.0,
            color: [0xcc, 0xcc, 0xd4],
        });
        labels
    }
}

fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 { return None; }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some([r, g, b])
}
