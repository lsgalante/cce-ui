use crate::colors;
use crate::widget::*;

pub struct Canvas {
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
}

impl Canvas {
    pub fn new() -> Self { Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, hovered: false } }
}

impl Element for Canvas {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }
    fn hit_test(&self, _px: f32, _py: f32) -> bool { false }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Reset,
    ListRow,
    CopyIcon,
}

#[derive(Debug, Clone)]
pub struct Button {
    base: Widget,
    pressed: bool,
    just_clicked: bool,
    kind: ButtonKind,
    pub selected: bool,
}

impl Button {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            base: Widget::new_rect(x, y, w, h),
            pressed: false,
            just_clicked: false,
            kind: ButtonKind::Primary,
            selected: false,
        }
    }

    pub fn new_reset(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            base: Widget::new_rect(x, y, w, h),
            pressed: false,
            just_clicked: false,
            kind: ButtonKind::Reset,
            selected: false,
        }
    }

    pub fn new_list_row(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            base: Widget::new_rect(x, y, w, h),
            pressed: false,
            just_clicked: false,
            kind: ButtonKind::ListRow,
            selected: false,
        }
    }

    pub fn new_copy_icon(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            base: Widget::new_rect(x, y, w, h),
            pressed: false,
            just_clicked: false,
            kind: ButtonKind::CopyIcon,
            selected: false,
        }
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self
    }

    pub fn with_selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl Element for Button {
    crate::impl_widget_base!(Button);
    fn highlight_quad(&self) -> Option<(f32, f32, f32, f32, [f32; 4])> { None }

    fn color(&self) -> [f32; 4] {
        match self.kind {
            ButtonKind::Primary => {
                if self.pressed { colors::BUTTON_PRESS }
                else if self.base.hovered { colors::BUTTON_HOVER }
                else { colors::BUTTON_IDLE }
            }
            ButtonKind::Reset => {
                if self.pressed { colors::RESET_BTN_PRESS }
                else if self.base.hovered { colors::RESET_BTN_HOVER }
                else { colors::RESET_BTN_IDLE }
            }
            ButtonKind::ListRow => {
                if self.selected {
                    if self.pressed { [0.30, 0.52, 0.78, 0.6] }
                    else if self.base.hovered { [0.30, 0.52, 0.78, 0.5] }
                    else { [0.20, 0.40, 0.65, 0.4] }
                } else {
                    if self.pressed { [0.20, 0.20, 0.25, 0.25] }
                    else if self.base.hovered { [0.20, 0.20, 0.25, 0.15] }
                    else { [0.0, 0.0, 0.0, 0.0] }
                }
            }
            ButtonKind::CopyIcon => {
                if self.selected {
                    if self.pressed { [0.30, 0.52, 0.78, 0.5] }
                    else if self.base.hovered { [0.30, 0.52, 0.78, 0.5] }
                    else { [0.20, 0.40, 0.65, 0.2] }
                } else {
                    if self.pressed { [0.20, 0.20, 0.25, 0.25] }
                    else if self.base.hovered { [0.20, 0.20, 0.25, 0.25] }
                    else { [0.0, 0.0, 0.0, 0.0] }
                }
            }
        }
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

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        if let Some(ref label) = self.base.label {
            let font_size = 12.0;
            let est_w = if label == "📋" {
                12.0
            } else {
                TextLabel::estimate_width(label, font_size)
            };
            let color = match self.kind {
                ButtonKind::ListRow | ButtonKind::CopyIcon => {
                    if self.selected { [230, 230, 242] }
                    else { [178, 178, 191] }
                }
                _ => [0xcc, 0xcc, 0xd4]
            };
            labels.push(TextLabel {
                text: label.clone(),
                x: self.base.x + (self.base.w - est_w) / 2.0,
                y: self.base.y + (self.base.h - font_size) / 2.0 - 1.0,
                font_size,
                color,
            });
        }
        labels
    }
    fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        vec![(self.base.x, self.base.y, self.base.w, self.base.h, self.color())]
    }
}

pub enum PageButton {
    Active,
    Inactive,
}

pub struct Checkbox {
    base: Widget,
    checked: bool,
    just_clicked: bool,
}

impl Checkbox {
    pub fn new() -> Self {
        Self {
            base: Widget::new(),
            checked: false,
            just_clicked: false,
        }
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self
    }

    pub fn set_checked(&mut self, checked: bool) {
        self.checked = checked;
    }

    pub fn checked(&self) -> bool {
        self.checked
    }
}

impl Element for Checkbox {
    crate::impl_widget_base!(Checkbox);

    fn color(&self) -> [f32; 4] { if self.checked { colors::CHECKBOX_CHECKED } else { colors::CHECKBOX_BG } }

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

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        if self.checked {
            let (x, y, w, h) = self.rect();
            let pad_x = w * 0.25;
            let pad_y = h * 0.25;
            quads.push((x + pad_x, y + pad_y, w - 2.0 * pad_x, h - 2.0 * pad_y, [1.0, 1.0, 1.0, 0.9]));
        }
        quads
    }

    fn take_click(&mut self) -> bool {
        if self.just_clicked { self.just_clicked = false; true } else { false }
    }
    fn value(&self) -> i32 { if self.checked { 1 } else { 0 } }
}

#[derive(Debug, Clone)]
pub struct Toggle {
    base: Widget,
    toggled: bool,
    just_toggled: bool,
}

impl Toggle {
    pub fn new() -> Self {
        Self {
            base: Widget::new(),
            toggled: false,
            just_toggled: false,
        }
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self
    }

    pub fn set_label(&mut self, label: &str) {
        self.base.label = Some(label.to_string());
    }

    pub fn set_toggled(&mut self, v: bool) {
        self.toggled = v;
    }

    pub fn toggled(&self) -> bool {
        self.toggled
    }
}

impl Element for Toggle {
    crate::impl_widget_base!(Toggle);

    fn color(&self) -> [f32; 4] { if self.toggled { colors::TOGGLE_ON } else { colors::TOGGLE_OFF } }

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

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let top = self.base.label_offset();
        let visual_h = self.base.h - top;
        let bg = if self.toggled { colors::TOGGLE_ON } else { colors::TOGGLE_OFF };
        vec![(self.base.x, self.base.y + top, self.base.w, visual_h, bg)]
    }

    fn take_click(&mut self) -> bool {
        if self.just_toggled { self.just_toggled = false; true } else { false }
    }
}

#[derive(Debug, Clone)]
pub struct Slider {
    base: Widget,
    dragging: bool,
    pub(crate) value: f32,
    drag_offset: f32,
    pub(crate) scroll_enabled: bool,
    show_readout: bool,
    pub(crate) editing: bool,
    edit_buffer: String,
    min: f32,
    max: f32,
}

impl Slider {
    pub fn new() -> Self {
        Self {
            base: Widget::new(),
            dragging: false,
            value: 0.5,
            drag_offset: 0.0,
            scroll_enabled: false,
            show_readout: false,
            editing: false,
            edit_buffer: String::new(),
            min: 0.0,
            max: 1.0,
        }
    }

    pub fn with_range(mut self, min: f32, max: f32) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self
    }

    pub fn set_label(&mut self, label: &str) {
        self.base.label = Some(label.to_string());
    }

    pub fn set_range(&mut self, min: f32, max: f32) {
        self.min = min;
        self.max = max;
    }

    pub fn with_scroll(mut self, enabled: bool) -> Self {
        self.scroll_enabled = enabled;
        self
    }

    pub fn set_scroll(&mut self, enabled: bool) {
        self.scroll_enabled = enabled;
    }

    pub fn with_value(mut self, val: f32) -> Self {
        self.value = val.clamp(0.0, 1.0);
        self
    }

    pub fn set_value(&mut self, val: f32) {
        self.value = val.clamp(0.0, 1.0);
    }

    pub fn with_readout(mut self, enabled: bool) -> Self {
        self.show_readout = enabled;
        self
    }

    pub fn set_readout(&mut self, enabled: bool) {
        self.show_readout = enabled;
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    pub fn get_scaled_value(&self) -> f32 {
        self.min + self.value * (self.max - self.min)
    }
}

impl Element for Slider {
    crate::impl_widget_base!(Slider);

    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn draggable(&self) -> bool { true }
    fn is_dragging(&self) -> bool { self.dragging }

    fn drag_update(&mut self, px: f32, _py: f32) -> bool {
        let top = self.base.label_offset();
        let visual_h = self.base.h - top;
        let (track_x, track_w) = if self.show_readout {
            let readout_w = 60.0;
            let gap = 8.0;
            let tw = (self.base.w - readout_w - gap).max(10.0);
            (self.base.x, tw)
        } else {
            (self.base.x, self.base.w)
        };
        let thumb_size = visual_h * 0.9;
        let range = track_w - thumb_size;
        if range > 0.0 {
            let raw = (px - self.drag_offset - track_x) / range;
            let new_val = raw.clamp(0.0, 1.0);
            if (new_val - self.value).abs() > 0.001 {
                self.value = new_val;
                if self.editing {
                    let scaled_val = self.min + self.value * (self.max - self.min);
                    self.edit_buffer = format!("{:.2}", scaled_val);
                }
                return true;
            }
        }
        false
    }

    fn drag_begin(&mut self, px: f32, _py: f32) {
        self.dragging = true;
        let top = self.base.label_offset();
        let visual_h = self.base.h - top;
        let (track_x, track_w) = if self.show_readout {
            let readout_w = 60.0;
            let gap = 8.0;
            let tw = (self.base.w - readout_w - gap).max(10.0);
            (self.base.x, tw)
        } else {
            (self.base.x, self.base.w)
        };
        let thumb_size = visual_h * 0.9;
        let thumb_x = track_x + self.value * (track_w - thumb_size);
        self.drag_offset = px - thumb_x;
    }

    fn drag_end(&mut self) { self.dragging = false; }

    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32) -> bool {
        if !self.scroll_enabled {
            return false;
        }
        let top = self.base.label_offset();
        let visual_h = self.base.h - top;
        let (sx, sy, sw, _) = self.rect();
        let (track_x, track_w) = if self.show_readout {
            let readout_w = 60.0;
            let gap = 8.0;
            let tw = (sw - readout_w - gap).max(10.0);
            (sx, tw)
        } else {
            (sx, sw)
        };
        if px >= track_x && px <= track_x + track_w && py >= sy + top && py <= sy + top + visual_h {
            let scroll_amount = match delta {
                MouseScrollDelta::LineDelta(_x, y) => *y,
                MouseScrollDelta::PixelDelta(pos) => (pos.y as f32) / 120.0,
            };
            let step = 0.02;
            let new_val = (self.value - scroll_amount * step).clamp(0.0, 1.0);
            if (new_val - self.value).abs() > 0.0001 {
                self.value = new_val;
                if self.editing {
                    let scaled_val = self.min + self.value * (self.max - self.min);
                    self.edit_buffer = format!("{:.2}", scaled_val);
                }
                return true;
            }
        }
        false
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button != MouseButton::Left { return false; }
        
        let top = self.base.label_offset();
        let visual_h = self.base.h - top;
        
        if self.show_readout {
            let readout_w = 60.0;
            let rx = self.base.x + self.base.w - readout_w;
            
            if px >= rx && px <= rx + readout_w && py >= self.base.y + top && py <= self.base.y + top + visual_h {
                if state == ElementState::Pressed {
                    if !self.editing {
                        self.editing = true;
                        let scaled_val = self.min + self.value * (self.max - self.min);
                        self.edit_buffer = format!("{:.2}", scaled_val);
                        focus::set_focused(self);
                    }
                }
                return true;
            }
        }

        match state {
            ElementState::Pressed => {
                let (track_x, track_w) = if self.show_readout {
                    let readout_w = 60.0;
                    let gap = 8.0;
                    let tw = (self.base.w - readout_w - gap).max(10.0);
                    (self.base.x, tw)
                } else {
                    (self.base.x, self.base.w)
                };
                let thumb_size = visual_h * 0.9;
                let thumb_x = track_x + self.value * (track_w - thumb_size);
                
                if px >= track_x && px <= track_x + track_w && py >= self.base.y + top && py <= self.base.y + top + visual_h {
                    self.dragging = true;
                    self.drag_offset = px - thumb_x;
                    return true;
                }
                false
            }
            ElementState::Released => {
                if self.dragging {
                    self.dragging = false;
                    return true;
                }
                false
            }
        }
    }

    fn unfocus(&mut self) {
        if self.editing {
            self.editing = false;
            if let Ok(new_val) = self.edit_buffer.parse::<f32>() {
                let range = self.max - self.min;
                if range != 0.0 {
                    self.value = ((new_val - self.min) / range).clamp(0.0, 1.0);
                } else {
                    self.value = 0.0;
                }
            }
        }
    }

    fn keyboard_input(&mut self, event: &KeyEvent) -> bool {
        if !self.editing { return false; }
        if event.state != ElementState::Pressed { return false; }
        
        match &event.logical_key {
            Key::Named(NamedKey::Backspace) => {
                if !self.edit_buffer.is_empty() {
                    self.edit_buffer.pop();
                    return true;
                }
            }
            Key::Named(NamedKey::Enter) => {
                self.unfocus();
                return true;
            }
            Key::Named(NamedKey::Escape) => {
                self.editing = false;
                return true;
            }
            Key::Character(s) => {
                for ch in s.chars() {
                    if ch.is_ascii_digit() || ch == '.' || (ch == '-' && self.edit_buffer.is_empty()) {
                        self.edit_buffer.push(ch);
                    }
                }
                return true;
            }
            _ => {}
        }
        false
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        let top = self.base.label_offset();
        let visual_h = self.base.h - top;
        
        let (track_x, track_w) = if self.show_readout {
            let readout_w = 60.0;
            let gap = 8.0;
            let tw = (self.base.w - readout_w - gap).max(10.0);
            
            quads.push((self.base.x, self.base.y + top, tw, visual_h, colors::slider_track()));
            
            let rx = self.base.x + self.base.w - readout_w;
            let bg_color = if self.editing {
                [0.06, 0.10, 0.18, 1.0]
            } else {
                [0.10, 0.10, 0.13, 1.0]
            };
            quads.push((rx, self.base.y + top, readout_w, visual_h, bg_color));
            
            if self.base.focused || self.editing {
                let border_color = [0.20, 0.50, 0.85, 1.0];
                let border_t = 1.0;
                quads.push((rx, self.base.y + top, readout_w, border_t, border_color));
                quads.push((rx, self.base.y + top + visual_h - border_t, readout_w, border_t, border_color));
                quads.push((rx, self.base.y + top, border_t, visual_h, border_color));
                quads.push((rx + readout_w - border_t, self.base.y + top, border_t, visual_h, border_color));
            }

            (self.base.x, tw)
        } else {
            quads.push((self.base.x, self.base.y + top, self.base.w, visual_h, colors::slider_track()));
            (self.base.x, self.base.w)
        };

        let thumb_size = visual_h * 0.9;
        let thumb_x = track_x + self.value * (track_w - thumb_size);
        let thumb_y = self.base.y + top + (visual_h - thumb_size) / 2.0;
        let thumb_color = if self.dragging {
            colors::SLIDER_THUMB_DRAG
        } else {
            colors::SLIDER_THUMB
        };
        quads.push((thumb_x, thumb_y, thumb_size, thumb_size, thumb_color));
        
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        let top = self.base.label_offset();
        let visual_h = self.base.h - top;
        
        if let Some(ref label) = self.base.label {
            labels.push(TextLabel {
                text: label.clone(),
                x: self.base.x,
                y: self.base.y,
                font_size: 12.0,
                color: [0x83, 0x83, 0x8a],
            });
        }
        
        if self.show_readout {
            let readout_w = 60.0;
            let rx = self.base.x + self.base.w - readout_w;
            let ry = self.base.y + top + (visual_h - 12.0) / 2.0;
            
            let text = if self.editing {
                self.edit_buffer.clone()
            } else {
                let scaled_val = self.min + self.value * (self.max - self.min);
                format!("{:.2}", scaled_val)
            };
            
            labels.push(TextLabel {
                text,
                x: rx + 8.0,
                y: ry,
                font_size: 12.0,
                color: [0xee, 0xee, 0xf0],
            });
        }
        
        labels
    }

    fn value(&self) -> i32 { (self.value * 100.0) as i32 }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveThumb {
    Low,
    High,
}

pub struct RangeSlider {
    base: Widget,
    value_low: f32,
    value_high: f32,
    pub(crate) active_thumb: Option<ActiveThumb>,
    drag_offset: f32,
}

impl RangeSlider {
    pub fn new() -> Self {
        Self {
            base: Widget::new(),
            value_low: 0.2,
            value_high: 0.8,
            active_thumb: None,
            drag_offset: 0.0,
        }
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self
    }

    pub fn with_values(mut self, low: f32, high: f32) -> Self {
        self.value_low = low.clamp(0.0, 1.0);
        self.value_high = high.clamp(self.value_low, 1.0);
        self
    }

    pub fn set_values(&mut self, low: f32, high: f32) {
        self.value_low = low.clamp(0.0, 1.0);
        self.value_high = high.clamp(self.value_low, 1.0);
    }

    pub fn values(&self) -> (f32, f32) {
        (self.value_low, self.value_high)
    }
}

impl Element for RangeSlider {
    crate::impl_widget_base!(RangeSlider);

    fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }

    fn draggable(&self) -> bool { true }
    fn is_dragging(&self) -> bool { self.active_thumb.is_some() }

    fn drag_update(&mut self, px: f32, _py: f32) -> bool {
        let Some(active) = self.active_thumb else { return false; };
        let top = self.base.label_offset();
        let visual_h = self.base.h - top;
        let thumb_size = visual_h * 0.9;
        let range = self.base.w - thumb_size;
        if range <= 0.0 { return false; }
        
        let new_val = ((px - self.drag_offset - self.base.x) / range).clamp(0.0, 1.0);
        match active {
            ActiveThumb::Low => {
                let constrained = new_val.min(self.value_high);
                if (constrained - self.value_low).abs() > 0.001 {
                    self.value_low = constrained;
                    return true;
                }
            }
            ActiveThumb::High => {
                let constrained = new_val.max(self.value_low);
                if (constrained - self.value_high).abs() > 0.001 {
                    self.value_high = constrained;
                    return true;
                }
            }
        }
        false
    }

    fn drag_begin(&mut self, px: f32, _py: f32) {
        let top = self.base.label_offset();
        let visual_h = self.base.h - top;
        let thumb_size = visual_h * 0.9;
        let range = self.base.w - thumb_size;
        let thumb_low_x = self.base.x + self.value_low * range;
        let thumb_high_x = self.base.x + self.value_high * range;
        let center_low = thumb_low_x + thumb_size / 2.0;
        let center_high = thumb_high_x + thumb_size / 2.0;

        let active = if (self.value_low - self.value_high).abs() < 0.001 {
            if px < center_low {
                ActiveThumb::Low
            } else {
                ActiveThumb::High
            }
        } else {
            let dist_low = (px - center_low).abs();
            let dist_high = (px - center_high).abs();
            if dist_low < dist_high {
                ActiveThumb::Low
            } else {
                ActiveThumb::High
            }
        };

        self.active_thumb = Some(active);
        let active_x = match active {
            ActiveThumb::Low => thumb_low_x,
            ActiveThumb::High => thumb_high_x,
        };
        self.drag_offset = px - active_x;
    }

    fn drag_end(&mut self) {
        self.active_thumb = None;
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let top = self.base.label_offset();
        let visual_h = self.base.h - top;
        let thumb_size = visual_h * 0.9;
        let range = self.base.w - thumb_size;
        let thumb_low_x = self.base.x + self.value_low * range;
        let thumb_high_x = self.base.x + self.value_high * range;
        
        let thumb_y = self.base.y + top + (visual_h - thumb_size) / 2.0;
        
        // Highlighted track segment
        let highlight_x = thumb_low_x + thumb_size / 2.0;
        let highlight_w = thumb_high_x - thumb_low_x;
        let highlight_y = self.base.y + top + visual_h * 0.35;
        let highlight_h = visual_h * 0.3;
        
        let low_color = if self.active_thumb == Some(ActiveThumb::Low) {
            colors::SLIDER_THUMB_DRAG
        } else {
            colors::SLIDER_THUMB
        };

        let high_color = if self.active_thumb == Some(ActiveThumb::High) {
            colors::SLIDER_THUMB_DRAG
        } else {
            colors::SLIDER_THUMB
        };

        vec![
            (self.base.x, self.base.y + top, self.base.w, visual_h, colors::slider_track()),
            (highlight_x, highlight_y, highlight_w, highlight_h, colors::PROGRESS_FILL),
            (thumb_low_x, thumb_y, thumb_size, thumb_size, low_color),
            (thumb_high_x, thumb_y, thumb_size, thumb_size, high_color),
        ]
    }

    fn value(&self) -> i32 {
        ((self.value_low * 100.0) as i32) | (((self.value_high * 100.0) as i32) << 16)
    }
}

#[derive(Debug, Clone)]
pub struct Spinbox {
    pub(crate) base: Widget,
    pub value: i32,
    pub(crate) min: i32, pub(crate) max: i32, pub(crate) step: i32,
    pub editing: bool,
    pub edit_buffer: String,
    pub cursor_idx: usize,
    hover_dec: bool,
    hover_inc: bool,
    unit: Option<String>,
    pub decimals: u32,
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub children: Vec<*mut (dyn Element + 'static)>,
}

impl Spinbox {
    pub fn new(value: i32, min: i32, max: i32, step: i32) -> Self {
        Self {
            base: Widget::new(),
            value,
            min,
            max,
            step,
            editing: false,
            edit_buffer: String::new(),
            cursor_idx: 0,
            hover_dec: false,
            hover_inc: false,
            unit: None,
            decimals: 0,
            parent: None,
            children: Vec::new(),
        }
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self
    }

    pub fn set_label(&mut self, label: &str) {
        self.base.label = Some(label.to_string());
    }

    pub fn with_unit(mut self, unit: &str) -> Self {
        self.unit = Some(unit.to_string());
        self
    }

    pub fn set_unit(&mut self, unit: &str) {
        self.unit = Some(unit.to_string());
    }

    pub fn with_decimals(mut self, decimals: u32) -> Self {
        self.decimals = decimals;
        self
    }
}

impl Element for Spinbox {
    crate::impl_widget_base!(Spinbox);

    fn color(&self) -> [f32; 4] { colors::SPINBOX_BG }
    fn value(&self) -> i32 { self.value }
    fn widget_font(&self) -> Option<String> { Some("monospace".to_string()) }



    fn on_cursor_moved(&mut self, px: f32, py: f32) -> bool {
        let was = self.base.hovered;
        self.base.hovered = self.hit_test(px, py);
        if !self.base.hovered {
            let changed = self.hover_dec || self.hover_inc;
            self.hover_dec = false;
            self.hover_inc = false;
            return changed || was != self.base.hovered;
        }
        let split = self.base.x + self.base.w * 0.55;
        let hd = px >= split && px < split + self.base.w * 0.225;
        let hi = px >= split + self.base.w * 0.225;
        let changed = hd != self.hover_dec || hi != self.hover_inc;
        self.hover_dec = hd;
        self.hover_inc = hi;
        changed || was != self.base.hovered
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button != MouseButton::Left { return false; }
        if !self.hit_test(px, py) { return false; }
        match state {
            ElementState::Pressed => {
                let split = self.base.x + self.base.w * 0.55;
                if px >= split && px < split + self.base.w * 0.225 {
                    self.value = (self.value - self.step).max(self.min);
                    true
                } else if px >= split + self.base.w * 0.225 {
                    self.value = (self.value + self.step).min(self.max);
                    true
                } else if px < split {
                    self.editing = true;
                    if self.decimals > 0 {
                        let divisor = 10.0f32.powi(self.decimals as i32);
                        self.edit_buffer = format!("{:.width$}", self.value as f32 / divisor, width = self.decimals as usize);
                    } else {
                        self.edit_buffer = self.value.to_string();
                    }
                    let char_width = 8.4;
                    let click_idx = (((px - (self.base.x + 4.0)) / char_width).round() as isize)
                        .max(0)
                        .min(self.edit_buffer.chars().count() as isize) as usize;
                    self.cursor_idx = click_idx;
                    focus::set_focused(self);
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
        if self.decimals > 0 {
            let divisor = 10.0f32.powi(self.decimals as i32);
            self.edit_buffer = format!("{:.width$}", self.value as f32 / divisor, width = self.decimals as usize);
        } else {
            self.edit_buffer = self.value.to_string();
        }
        self.cursor_idx = self.edit_buffer.chars().count();
        focus::set_focused(self);
    }

    fn unfocus(&mut self) {
        if self.editing {
            self.editing = false;
            if self.decimals > 0 {
                if let Ok(val_f) = self.edit_buffer.parse::<f32>() {
                    let divisor = 10.0f32.powi(self.decimals as i32);
                    self.value = (val_f * divisor).round() as i32;
                    self.value = self.value.clamp(self.min, self.max);
                }
            } else {
                if let Ok(val) = self.edit_buffer.parse::<i32>() {
                    self.value = val.clamp(self.min, self.max);
                }
            }
        }
    }

    fn keyboard_input(&mut self, event: &KeyEvent) -> bool {
        if !self.editing { return false; }
        if event.state != ElementState::Pressed { return false; }
        match &event.logical_key {
            Key::Named(NamedKey::Backspace) => {
                if self.cursor_idx > 0 {
                    let mut chars: Vec<char> = self.edit_buffer.chars().collect();
                    chars.remove(self.cursor_idx - 1);
                    self.edit_buffer = chars.into_iter().collect();
                    self.cursor_idx -= 1;
                    return true;
                }
                false
            }
            Key::Named(NamedKey::Delete) => {
                if self.cursor_idx < self.edit_buffer.chars().count() {
                    let mut chars: Vec<char> = self.edit_buffer.chars().collect();
                    chars.remove(self.cursor_idx);
                    self.edit_buffer = chars.into_iter().collect();
                    return true;
                }
                false
            }
            Key::Named(NamedKey::ArrowLeft) => {
                if self.cursor_idx > 0 {
                    self.cursor_idx -= 1;
                    true
                } else {
                    false
                }
            }
            Key::Named(NamedKey::ArrowRight) => {
                if self.cursor_idx < self.edit_buffer.chars().count() {
                    self.cursor_idx += 1;
                    true
                } else {
                    false
                }
            }
            Key::Named(NamedKey::Enter) => {
                if self.decimals > 0 {
                    if let Ok(val_f) = self.edit_buffer.parse::<f32>() {
                        let divisor = 10.0f32.powi(self.decimals as i32);
                        self.value = (val_f * divisor).round() as i32;
                        self.value = self.value.clamp(self.min, self.max);
                    }
                } else {
                    if let Ok(val) = self.edit_buffer.parse::<i32>() {
                        self.value = val.clamp(self.min, self.max);
                    }
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
                    for ch in text.chars() {
                        match ch {
                            '-' if self.cursor_idx == 0 && !self.edit_buffer.starts_with('-') => {
                                self.edit_buffer.insert(0, '-');
                                self.cursor_idx += 1;
                            }
                            '.' if self.decimals > 0 && !self.edit_buffer.contains('.') => {
                                let mut chars: Vec<char> = self.edit_buffer.chars().collect();
                                chars.insert(self.cursor_idx, '.');
                                self.edit_buffer = chars.into_iter().collect();
                                self.cursor_idx += 1;
                            }
                            '0'..='9' => {
                                let mut chars: Vec<char> = self.edit_buffer.chars().collect();
                                chars.insert(self.cursor_idx, ch);
                                self.edit_buffer = chars.into_iter().collect();
                                self.cursor_idx += 1;
                            }
                            _ => {}
                        }
                    }
                }
                true
            }
        }
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        let top = self.base.label_offset();
        let visual_h = self.base.h - top;
        let split = self.base.x + self.base.w * 0.55;
        let btn_w = self.base.w * 0.225;
        let inc_col = if self.hover_inc { colors::SPINBOX_BUTTON_HOVER } else { colors::SPINBOX_BUTTON };
        let dec_col = if self.hover_dec { colors::SPINBOX_BUTTON_HOVER } else { colors::SPINBOX_BUTTON };
        
        let display_bg = if self.editing {
            [0.06, 0.10, 0.18, 1.0] // Focused dark-blue input field look
        } else {
            colors::SPINBOX_DISPLAY
        };
        
        quads.push((self.base.x, self.base.y + top, self.base.w * 0.55, visual_h, display_bg));
        quads.push((split, self.base.y + top, btn_w, visual_h, dec_col));
        quads.push((split + btn_w, self.base.y + top, btn_w, visual_h, inc_col));
        
        if self.editing {
            let border_color = [0.20, 0.50, 0.85, 1.0]; // Bright focused blue border
            // Top border
            quads.push((self.base.x, self.base.y + top, self.base.w * 0.55, 1.0, border_color));
            // Bottom border
            quads.push((self.base.x, self.base.y + top + visual_h - 1.0, self.base.w * 0.55, 1.0, border_color));
            // Left border
            quads.push((self.base.x, self.base.y + top, 1.0, visual_h, border_color));
            // Right border
            quads.push((self.base.x + self.base.w * 0.55 - 1.0, self.base.y + top, 1.0, visual_h, border_color));

            // Caret cursor
            let char_width = 8.4;
            let cursor_x = self.base.x + 4.0 + (self.cursor_idx as f32 * char_width);
            let max_cursor_x = split - 4.0;
            let final_cursor_x = cursor_x.min(max_cursor_x);
            let cursor_y = self.base.y + top + (visual_h - 14.0) / 2.0;
            quads.push((final_cursor_x, cursor_y, 1.5, 14.0, [0.80, 0.80, 0.85, 1.0]));
        }
        
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        if let Some(ref label) = self.base.label {
            labels.push(TextLabel {
                text: label.clone(),
                x: self.base.x + 4.0,
                y: self.base.y,
                font_size: 12.0,
                color: [0x83, 0x83, 0x8a],
            });
        }
        let value_text = if self.editing {
            self.edit_buffer.clone()
        } else if self.decimals > 0 {
            let divisor = 10.0f32.powi(self.decimals as i32);
            format!("{:.width$}", self.value as f32 / divisor, width = self.decimals as usize)
        } else {
            self.value.to_string()
        };
        
        let top = self.base.label_offset();
        let visual_h = self.base.h - top;
        
        labels.push(TextLabel {
            text: value_text,
            x: self.base.x + 4.0,
            y: self.base.y + top + (visual_h - 14.0) / 2.0 - 2.0,
            font_size: 14.0,
            color: [0xcc, 0xcc, 0xd4],
        });
        if let Some(ref unit) = self.unit {
            labels.push(TextLabel {
                text: unit.clone(),
                x: self.base.x + 4.0 + 36.0,
                y: self.base.y + top + (visual_h - 11.0) / 2.0 - 2.0,
                font_size: 11.0,
                color: [0x73, 0x73, 0x7a],
            });
        }

        labels.push(TextLabel {
            text: "-".to_string(),
            x: self.base.x + self.base.w * 0.6625 - 4.0,
            y: self.base.y + top + (visual_h - 12.0) / 2.0 - 2.0,
            font_size: 12.0,
            color: [0xcc, 0xcc, 0xd4],
        });
        labels.push(TextLabel {
            text: "+".to_string(),
            x: self.base.x + self.base.w * 0.8875 - 4.0,
            y: self.base.y + top + (visual_h - 12.0) / 2.0 - 2.0,
            font_size: 12.0,
            color: [0xcc, 0xcc, 0xd4],
        });
        labels
    }

    fn parent(&self) -> Option<*mut (dyn Element + 'static)> { self.parent }
    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>) { self.parent = parent; }
    fn children(&self) -> Vec<*mut (dyn Element + 'static)> { self.children.clone() }
    fn add_child(&mut self, child: *mut (dyn Element + 'static)) { self.children.push(child); }
    fn clear_children(&mut self) { self.children.clear(); }
}

impl Drop for Spinbox {
    fn drop(&mut self) {
        focus::clear_if_matches(self);
    }
}

#[derive(Debug)]
pub struct ColorSelector {
    pub(crate) base: Widget,
    pub color: [u8; 3],
    just_clicked: bool,
    pub editing: bool,
    edit_buffer: String,
    pub command: String,
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub children: Vec<*mut (dyn Element + 'static)>,
    child: std::sync::Arc<std::sync::Mutex<Option<std::process::Child>>>,
}

impl Clone for ColorSelector {
    fn clone(&self) -> Self {
        Self {
            base: self.base.clone(),
            color: self.color,
            just_clicked: self.just_clicked,
            editing: self.editing,
            edit_buffer: self.edit_buffer.clone(),
            command: self.command.clone(),
            parent: self.parent,
            children: self.children.clone(),
            child: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }
}

impl ColorSelector {
    pub fn new(color: [u8; 3]) -> Self {
        Self {
            base: Widget::new(),
            color,
            just_clicked: false,
            editing: false,
            edit_buffer: String::new(),
            command: "clear-color-interface".to_string(),
            parent: None,
            children: Vec::new(),
            child: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self
    }

    pub fn with_command(mut self, command: &str) -> Self {
        self.command = command.to_string();
        self
    }
}

impl Element for ColorSelector {
    crate::impl_widget_base!(ColorSelector);

    fn color_u8(&self) -> Option<[u8; 4]> {
        Some([self.color[0], self.color[1], self.color[2], 255])
    }



    fn color(&self) -> [f32; 4] {
        colors::to_linear([
            self.color[0] as f32 / 255.0,
            self.color[1] as f32 / 255.0,
            self.color[2] as f32 / 255.0,
            1.0,
        ])
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button != MouseButton::Left { return false; }
        if state != ElementState::Pressed { return false; }
        if !self.hit_test(px, py) { return false; }
        if px >= self.base.x + self.base.w * 0.65 {
            let hex = format!("#{:02x}{:02x}{:02x}", self.color[0], self.color[1], self.color[2]);
            let mut child_guard = self.child.lock().unwrap();
            if let Some(mut old_child) = child_guard.take() {
                let _ = old_child.kill();
            }
            if let Ok(child) = std::process::Command::new(&self.command)
                .arg(&hex)
                .stdout(std::process::Stdio::piped())
                .spawn()
            {
                *child_guard = Some(child);
            }
            return true;
        }
        self.focus();
        true
    }

    fn take_click(&mut self) -> bool {
        if self.just_clicked { self.just_clicked = false; true } else { false }
    }

    fn tick(&mut self, _dt: f32) -> bool {
        let mut child_opt = self.child.lock().unwrap();
        if let Some(ref mut child) = *child_opt {
            match child.try_wait() {
                Ok(Some(_status)) => {
                    let child = child_opt.take().unwrap();
                    if let Ok(output) = child.wait_with_output() {
                        let stdout_str = String::from_utf8_lossy(&output.stdout);
                        for line in stdout_str.lines().rev() {
                            if let Some(c) = parse_hex(line.trim()) {
                                self.color = c;
                                self.just_clicked = true;
                                return true;
                            }
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    eprintln!("Error checking color selector child process: {:?}", e);
                    *child_opt = None;
                }
            }
        }
        false
    }

    fn focus(&mut self) {
        self.editing = true;
        self.edit_buffer = format!("#{:02x}{:02x}{:02x}", self.color[0], self.color[1], self.color[2]);
        focus::set_focused(self);
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
                true
            }
        }
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        let top = self.base.label_offset();
        let visual_h = self.base.h - top;
        let pick_x = self.base.x + self.base.w * 0.65;
        let pick_w = self.base.w * 0.35;

        // Draw the text box part background and border
        let bg_color = if self.editing {
            [0.06, 0.10, 0.18, 1.0]
        } else {
            [0.08, 0.08, 0.12, 1.0]
        };
        let border_color = if self.editing {
            [0.20, 0.50, 0.85, 1.0]
        } else if self.base.hovered {
            [0.25, 0.25, 0.35, 1.0]
        } else {
            [0.18, 0.18, 0.24, 1.0]
        };

        // Background & border for hex text box part
        quads.push((self.base.x, self.base.y + top, self.base.w * 0.65, visual_h, border_color));
        quads.push((self.base.x + 1.0, self.base.y + top + 1.0, self.base.w * 0.65 - 2.0, visual_h - 2.0, bg_color));

        let linear_c = colors::to_linear([
            self.color[0] as f32 / 255.0,
            self.color[1] as f32 / 255.0,
            self.color[2] as f32 / 255.0,
            1.0,
        ]);
        let border_w = 1.0;
        let border_c = colors::color_borders_color();
        
        let r = border_c[0];
        let g = border_c[1];
        let b = border_c[2];
        let steps = 6;
        for i in (1..=steps).rev() {
            let offset = i as f32 * 0.75;
            let rx = pick_x - offset;
            let ry = self.base.y + top - offset;
            let rw = pick_w + 2.0 * offset;
            let rh = visual_h + 2.0 * offset;
            let alpha = 0.08 * (1.0 - (i as f32 / steps as f32).powf(1.5));
            if alpha > 0.001 {
                quads.push((rx, ry, rw, rh, [r, g, b, alpha]));
            }
        }

        quads.push((pick_x, self.base.y + top, pick_w, visual_h, border_c));
        quads.push((
            pick_x + border_w,
            self.base.y + top + border_w,
            pick_w - 2.0 * border_w,
            visual_h - 2.0 * border_w,
            linear_c,
        ));
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        if let Some(ref label) = self.base.label {
            labels.push(TextLabel {
                text: label.clone(),
                x: self.base.x + 4.0,
                y: self.base.y,
                font_size: 12.0,
                color: [0x83, 0x83, 0x8a],
            });
        }
        let hex = if self.editing { self.edit_buffer.clone() } else { format!("#{:02x}{:02x}{:02x}", self.color[0], self.color[1], self.color[2]) };
        let top = self.base.label_offset();
        let visual_h = self.base.h - top;
        labels.push(TextLabel {
            text: hex,
            x: self.base.x + 4.0,
            y: self.base.y + top + (visual_h - 12.0) / 2.0,
            font_size: 12.0,
            color: [0xcc, 0xcc, 0xd4],
        });
        labels
    }

    fn parent(&self) -> Option<*mut (dyn Element + 'static)> { self.parent }
    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>) { self.parent = parent; }
    fn children(&self) -> Vec<*mut (dyn Element + 'static)> { self.children.clone() }
    fn add_child(&mut self, child: *mut (dyn Element + 'static)) { self.children.push(child); }
    fn clear_children(&mut self) { self.children.clear(); }
}

impl Drop for ColorSelector {
    fn drop(&mut self) {
        focus::clear_if_matches(self);
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

pub(crate) const BREADCRUMB_PADDING: f32 = 8.0;
pub(crate) const SEGMENT_GAP: f32 = 4.0;

#[derive(Debug, Clone)]
pub struct Dropdown {
    base: Widget,
    pub options: Vec<String>,
    pub selected: usize,
    pub open: bool,
    pub(crate) hovered_item: Option<usize>,
    just_changed: bool,
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub children: Vec<*mut (dyn Element + 'static)>,
}

impl Dropdown {
    pub fn new(options: Vec<String>, selected: usize) -> Self {
        Self {
            base: Widget::new(),
            options,
            selected,
            open: false,
            hovered_item: None,
            just_changed: false,
            parent: None,
            children: Vec::new(),
        }
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self
    }

    pub fn set_label(&mut self, label: &str) {
        self.base.label = Some(label.to_string());
    }

    pub fn take_change(&mut self) -> bool {
        let changed = self.just_changed;
        self.just_changed = false;
        changed
    }

    pub fn render_popover(&self, pc: &mut dyn crate::layout::RenderTarget) {
        if !self.open { return; }
        
        let dy = self.base.y + self.base.h;
        let dh = self.options.len() as f32 * 24.0;
        
        // 1. Soft layered drop shadows
        pc.rect([0.02, 0.02, 0.05, 0.15], self.base.x + 1.0, dy + 1.0, self.base.w, dh);
        pc.rect([0.02, 0.02, 0.05, 0.08], self.base.x + 3.0, dy + 3.0, self.base.w, dh);
        pc.rect([0.02, 0.02, 0.05, 0.04], self.base.x + 5.0, dy + 5.0, self.base.w, dh);

        let theme = colors::active_theme();

        // 2. High-contrast premium outer border
        pc.rect(theme.surface_border, self.base.x, dy, self.base.w, dh);
        
        // 3. Frosted glass background (matching magic alpha 0.699 in shader)
        pc.rect(theme.surface_bg, self.base.x + 1.0, dy + 1.0, self.base.w - 2.0, dh - 2.0); // bg
        
        if let Some(h_idx) = self.hovered_item {
            let iy = dy + h_idx as f32 * 24.0;
            // 4. Vibrantly colored translucent selection highlight
            pc.rect(theme.primary_accent, self.base.x + 2.0, iy + 2.0, self.base.w - 4.0, 20.0);
        }
        
        for (idx, opt) in self.options.iter().enumerate() {
            let iy = dy + idx as f32 * 24.0 + (24.0 - 12.0) / 2.0;
            let text_color = if self.hovered_item == Some(idx) {
                [0xff, 0xff, 0xff]
            } else if self.selected == idx {
                [0x3a, 0x9a, 0xff]
            } else {
                [0xcc, 0xcc, 0xd4]
            };
            
            pc.text(
                opt,
                self.base.x + 8.0,
                iy,
                12.0,
                [
                    text_color[0] as f32 / 255.0,
                    text_color[1] as f32 / 255.0,
                    text_color[2] as f32 / 255.0,
                    1.0,
                ],
            );
        }
    }
}

impl Default for Dropdown {
    fn default() -> Self {
        Self::new(Vec::new(), 0)
    }
}

impl Element for Dropdown {
    crate::impl_widget_base!(Dropdown);

    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn hit_test(&self, px: f32, py: f32) -> bool {
        if crate::widget::popovers::is_coordinate_covered(self as *const Self as *const () as usize, px, py) {
            return false;
        }
        let (x, y, w, h) = self.rect();
        let hx = if self.base.row_w > 0.0 { self.base.row_x } else { x };
        let hw = if self.base.row_w > 0.0 { self.base.row_w } else { w };
        if self.open {
            let dy = y + h;
            let dh = self.options.len() as f32 * 24.0;
            let hit_trigger = px >= hx && px <= hx + hw && py >= y && py <= y + h;
            let hit_popover = px >= x && px <= x + w && py >= dy && py <= dy + dh;
            hit_trigger || hit_popover
        } else {
            px >= hx && px <= hx + hw && py >= y && py <= y + h
        }
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32) -> bool {
        let was_hovered = self.base.hovered;
        let was_hovered_item = self.hovered_item;
        
        self.base.hovered = self.hit_test(px, py);
        self.hovered_item = None;

        if self.open {
            let (x, y, w, h) = self.rect();
            let dy = y + h;
            let dh = self.options.len() as f32 * 24.0;
            if px >= x && px <= x + w && py >= dy && py <= dy + dh {
                let idx = ((py - dy) / 24.0) as usize;
                if idx < self.options.len() {
                    self.hovered_item = Some(idx);
                }
            }
        }

        self.base.hovered != was_hovered || self.hovered_item != was_hovered_item
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button != MouseButton::Left || state != ElementState::Pressed { return false; }

        let (x, y, w, h) = self.rect();
        let dy = y + h;
        let dh = self.options.len() as f32 * 24.0;

        let inside_trigger = px >= x && px <= x + w && py >= y && py <= y + h;
        let inside_popover = self.open && px >= x && px <= x + w && py >= dy && py <= dy + dh;

        if inside_popover {
            let idx = ((py - dy) / 24.0) as usize;
            if idx < self.options.len() {
                if self.selected != idx {
                    self.selected = idx;
                    self.just_changed = true;
                }
            }
            self.open = false;
            return true;
        }

        if inside_trigger {
            self.open = !self.open;
            if self.open {
                self.focus();
            } else {
                self.unfocus();
            }
            return true;
        }

        if self.open {
            self.open = false;
            return true;
        }

        false
    }

    fn focus(&mut self) {
        focus::set_focused(self);
    }

    fn unfocus(&mut self) {
        self.open = false;
    }

    fn keyboard_input(&mut self, event: &KeyEvent) -> bool {
        if event.state != ElementState::Pressed { return false; }
        if !self.open {
            if let Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) = event.logical_key {
                self.open = true;
                self.hovered_item = Some(self.selected);
                return true;
            }
            return false;
        }
        
        match event.logical_key {
            Key::Named(NamedKey::ArrowDown) => {
                let current = self.hovered_item.unwrap_or(self.selected);
                if current + 1 < self.options.len() {
                    self.hovered_item = Some(current + 1);
                } else {
                    self.hovered_item = Some(0);
                }
                true
            }
            Key::Named(NamedKey::ArrowUp) => {
                let current = self.hovered_item.unwrap_or(self.selected);
                if current > 0 {
                    self.hovered_item = Some(current - 1);
                } else {
                    self.hovered_item = Some(self.options.len() - 1);
                }
                true
            }
            Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                if let Some(idx) = self.hovered_item {
                    if self.selected != idx {
                        self.selected = idx;
                        self.just_changed = true;
                    }
                }
                self.open = false;
                true
            }
            Key::Named(NamedKey::Escape) => {
                self.open = false;
                true
            }
            _ => false
        }
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        let top = self.base.label_offset();
        let visual_h = self.base.h - top;

        let bg_color = [0.08, 0.08, 0.12, 1.0];
        let border_color = if self.open {
            [0.30, 0.50, 0.32, 1.0]
        } else if self.base.hovered {
            [0.25, 0.25, 0.35, 1.0]
        } else {
            [0.18, 0.18, 0.24, 1.0]
        };

        quads.push((self.base.x, self.base.y + top, self.base.w, visual_h, border_color));
        quads.push((self.base.x + 1.0, self.base.y + top + 1.0, self.base.w - 2.0, visual_h - 2.0, bg_color));

        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        let top = self.base.label_offset();
        let visual_h = self.base.h - top;
        
        if let Some(ref label) = self.base.label {
            labels.push(TextLabel {
                text: label.clone(),
                x: self.base.x + 4.0,
                y: self.base.y,
                font_size: 11.0,
                color: [0x83, 0x83, 0x8a],
            });
        }

        let selected_text = self.options.get(self.selected).cloned().unwrap_or_default();
        labels.push(TextLabel {
            text: selected_text,
            x: self.base.x + 8.0,
            y: self.base.y + top + (visual_h - 12.0) / 2.0,
            font_size: 12.0,
            color: [0xdd, 0xdd, 0xe2],
        });

        labels.push(TextLabel {
            text: "▼".to_string(),
            x: self.base.x + self.base.w - 18.0,
            y: self.base.y + top + (visual_h - 10.0) / 2.0,
            font_size: 10.0,
            color: [0x83, 0x83, 0x8a],
        });

        labels
    }

    fn parent(&self) -> Option<*mut (dyn Element + 'static)> { self.parent }
    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>) { self.parent = parent; }
    fn children(&self) -> Vec<*mut (dyn Element + 'static)> { self.children.clone() }
    fn add_child(&mut self, child: *mut (dyn Element + 'static)) { self.children.push(child); }
    fn clear_children(&mut self) { self.children.clear(); }
    fn value(&self) -> i32 { self.selected as i32 }
    fn take_click(&mut self) -> bool { self.take_change() }
    fn popover_rect(&self) -> Option<(f32, f32, f32, f32)> {
        if self.open {
            Some((self.base.x, self.base.y + self.base.h, self.base.w, self.options.len() as f32 * 24.0))
        } else {
            None
        }
    }
    fn render_popover(&self, pc: &mut dyn crate::layout::RenderTarget) {
        Dropdown::render_popover(self, pc);
    }
}

unsafe impl Send for Dropdown {}
unsafe impl Sync for Dropdown {}

// ── TextBox Widget ──

#[derive(Debug, Clone)]
pub struct TextBox {
    base: Widget,
    pub text: String,
    pub editing: bool,
    pub edit_buffer: String,
    pub(crate) just_changed: bool,
    pub disabled: bool,
    pub all_selected: bool,
    pub cursor_idx: usize,
    pub select_anchor: Option<usize>,
    pub dragging: bool,
    pub just_focused: bool,
    pub drag_start_idx: Option<usize>,
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub children: Vec<*mut (dyn Element + 'static)>,
    pub max_width: Option<f32>,
    pub width: Option<f32>,
    pub is_password: bool,
    pub multiline: bool,
    pub draw_bg_border: bool,
    pub text_color: Option<[u8; 3]>,
    pub font_size: f32,
    pub font_family: String,
}

impl TextBox {
    pub fn new(text: String) -> Self {
        Self {
            base: Widget::new(),
            text,
            editing: false,
            edit_buffer: String::new(),
            just_changed: false,
            disabled: false,
            all_selected: false,
            cursor_idx: 0,
            select_anchor: None,
            dragging: false,
            just_focused: false,
            drag_start_idx: None,
            parent: None,
            children: Vec::new(),
            max_width: Some(300.0),
            width: None,
            is_password: false,
            multiline: false,
            draw_bg_border: true,
            text_color: None,
            font_size: 12.0,
            font_family: "monospace".to_string(),
        }
    }

    pub fn with_multiline(mut self, multiline: bool) -> Self {
        self.multiline = multiline;
        self
    }

    pub fn with_draw_bg_border(mut self, draw: bool) -> Self {
        self.draw_bg_border = draw;
        self
    }

    pub fn with_text_color(mut self, color: Option<[u8; 3]>) -> Self {
        self.text_color = color;
        self
    }

    pub fn with_font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    pub fn with_font_family(mut self, family: String) -> Self {
        self.font_family = family;
        self
    }

    pub fn wrap_text(&self, max_chars_per_line: usize) -> (Vec<String>, Vec<(usize, usize)>) {
        let text_src = if self.editing { &self.edit_buffer } else { &self.text };
        let chars: Vec<char> = text_src.chars().collect();
        let mut lines = Vec::new();
        let mut current_line = Vec::new();
        let mut index_map = vec![(0, 0); chars.len() + 1];
        
        let max_chars = max_chars_per_line.max(1);
        
        let mut i = 0;
        while i < chars.len() {
            let ch = chars[i];
            
            if ch == '\n' {
                index_map[i] = (lines.len(), current_line.len());
                lines.push(current_line.iter().collect::<String>());
                current_line.clear();
                i += 1;
                continue;
            }
            
            current_line.push(ch);
            index_map[i] = (lines.len(), current_line.len() - 1);
            
            if current_line.len() > max_chars {
                let mut space_idx = None;
                for (s_idx, &c) in current_line.iter().enumerate().rev() {
                    if c.is_whitespace() {
                        space_idx = Some(s_idx);
                        break;
                    }
                }
                
                if let Some(s_idx) = space_idx {
                    let line_to_push: Vec<char> = current_line[0..s_idx + 1].to_vec();
                    let remaining: Vec<char> = current_line[s_idx + 1..].to_vec();
                    
                    let line_idx = lines.len();
                    lines.push(line_to_push.iter().collect::<String>());
                    
                    current_line = remaining;
                    let start_orig = i - current_line.len() + 1;
                    for c_idx in 0..current_line.len() {
                        index_map[start_orig + c_idx] = (line_idx + 1, c_idx);
                    }
                } else {
                    let line_to_push: Vec<char> = current_line[0..max_chars].to_vec();
                    let remaining: Vec<char> = current_line[max_chars..].to_vec();
                    
                    let line_idx = lines.len();
                    lines.push(line_to_push.iter().collect::<String>());
                    
                    current_line = remaining;
                    let start_orig = i - current_line.len() + 1;
                    for c_idx in 0..current_line.len() {
                        index_map[start_orig + c_idx] = (line_idx + 1, c_idx);
                    }
                }
            }
            i += 1;
        }
        
        index_map[chars.len()] = (lines.len(), current_line.len());
        lines.push(current_line.iter().collect::<String>());
        
        (lines, index_map)
    }

    pub fn map_2d_to_1d(&self, index_map: &[(usize, usize)], target_line: usize, target_col: usize, max_line_idx: usize) -> usize {
        let line = target_line.min(max_line_idx);
        let mut best_idx = 0;
        let mut best_dist = usize::MAX;
        
        for (i, &(l, c)) in index_map.iter().enumerate() {
            if l == line {
                let dist = (c as isize - target_col as isize).abs() as usize;
                if dist < best_dist {
                    best_dist = dist;
                    best_idx = i;
                }
            }
        }
        best_idx
    }

    pub fn with_password(mut self, is_password: bool) -> Self {
        self.is_password = is_password;
        self
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self
    }

    pub fn set_label(&mut self, label: &str) {
        self.base.label = Some(label.to_string());
    }

    pub fn take_change(&mut self) -> bool {
        let changed = self.just_changed;
        self.just_changed = false;
        changed
    }

    pub fn with_max_width(mut self, max_w: Option<f32>) -> Self {
        self.max_width = max_w;
        self
    }

    pub fn set_max_width(&mut self, max_w: Option<f32>) {
        self.max_width = max_w;
    }

    pub fn with_width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }

    pub fn set_width(&mut self, w: f32) {
        self.width = Some(w);
    }

    pub fn copy_selection(&self) {
        let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
        let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);
        if start != end {
            let chars: Vec<char> = self.edit_buffer.chars().collect();
            let selected_text: String = chars[start..end].iter().collect();
            clipboard::copy_to_clipboard(&selected_text);
        }
    }

    pub fn cut_selection(&mut self) -> bool {
        let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
        let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);
        if start != end {
            let chars: Vec<char> = self.edit_buffer.chars().collect();
            let selected_text: String = chars[start..end].iter().collect();
            clipboard::copy_to_clipboard(&selected_text);

            let mut new_buf = String::new();
            for i in 0..start {
                new_buf.push(chars[i]);
            }
            for i in end..chars.len() {
                new_buf.push(chars[i]);
            }
            self.edit_buffer = new_buf;
            self.cursor_idx = start;
            self.select_anchor = None;
            self.all_selected = false;
            return true;
        }
        false
    }

    pub fn paste_from_clipboard(&mut self) -> bool {
        if let Some(text) = clipboard::read_from_clipboard() {
            let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
            let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);
            let chars: Vec<char> = self.edit_buffer.chars().collect();
            let mut new_buf = String::new();
            for i in 0..start {
                new_buf.push(chars[i]);
            }
            let mut inserted_count = 0;
            for ch in text.chars() {
                if !ch.is_control() && ch != '\n' && ch != '\r' {
                    new_buf.push(ch);
                    inserted_count += 1;
                }
            }
            for i in end..chars.len() {
                new_buf.push(chars[i]);
            }
            self.edit_buffer = new_buf;
            self.cursor_idx = start + inserted_count;
            self.select_anchor = None;
            self.all_selected = false;
            true
        } else {
            false
        }
    }

    pub fn select_all(&mut self) {
        let len = self.edit_buffer.chars().count();
        self.select_anchor = Some(0);
        self.cursor_idx = len;
        self.all_selected = len > 0;
    }
}

impl Default for TextBox {
    fn default() -> Self {
        Self::new(String::new())
    }
}

impl Element for TextBox {
    crate::impl_widget_base!(TextBox);

    fn rect(&self) -> (f32, f32, f32, f32) { (self.base.x, self.base.y, self.base.w, self.base.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let final_w = if let Some(explicit_w) = self.width {
            explicit_w
        } else if let Some(max_w) = self.max_width {
            w.min(max_w)
        } else {
            w
        };
        if let Some(b) = self.base_mut() {
            b.x = x;
            b.y = y;
            b.w = final_w;
            b.h = h;
        }
    }
    fn set_row_rect(&mut self, x: f32, w: f32) {
        let final_w = if let Some(explicit_w) = self.width {
            explicit_w
        } else if let Some(max_w) = self.max_width {
            w.min(max_w)
        } else {
            w
        };
        if let Some(b) = self.base_mut() {
            b.row_x = x;
            b.row_w = final_w;
        }
    }

    fn color(&self) -> [f32; 4] {
        [0.10, 0.10, 0.16, 1.0]
    }



    fn on_cursor_moved(&mut self, px: f32, py: f32) -> bool {
        if self.disabled {
            let was = self.base.hovered;
            self.base.hovered = false;
            return was;
        }
        let mut changed = false;
        if self.dragging && self.editing {
            let char_width = self.font_size * 0.6;
            let drag_idx = if self.multiline {
                let line_height = self.font_size * 1.333;
                let max_chars = (((self.base.w - 16.0) / char_width).floor() as usize).max(1);
                let (lines, index_map) = self.wrap_text(max_chars);
                let click_line = (((py - (self.base.y + 8.0)) / line_height).floor() as isize).max(0) as usize;
                let click_col = (((px - (self.base.x + 8.0)) / char_width).round() as isize).max(0) as usize;
                self.map_2d_to_1d(&index_map, click_line, click_col, lines.len() - 1)
            } else {
                (((px - (self.base.x + 8.0)) / char_width).round() as isize)
                    .max(0)
                    .min(self.edit_buffer.chars().count() as isize) as usize
            };
            if self.cursor_idx != drag_idx {
                self.cursor_idx = drag_idx;
                self.just_focused = false;
                let len = self.edit_buffer.chars().count();
                let start = self.select_anchor.unwrap_or(0).min(self.cursor_idx);
                let end = self.select_anchor.unwrap_or(0).max(self.cursor_idx);
                self.all_selected = start == 0 && end == len && len > 0;
                changed = true;
            }
        }
        let was = self.base.hovered;
        self.base.hovered = self.hit_test(px, py);
        if was != self.base.hovered {
            changed = true;
        }
        changed
    }

    fn draggable(&self) -> bool { !self.disabled }
    fn is_dragging(&self) -> bool { self.dragging }
    fn widget_font(&self) -> Option<String> { Some(self.font_family.clone()) }

    fn drag_begin(&mut self, _px: f32, _py: f32) {
        if self.disabled || !self.editing { return; }
        self.dragging = true;
    }

    fn drag_update(&mut self, px: f32, py: f32) -> bool {
        if self.disabled || !self.editing { return false; }
        let char_width = self.font_size * 0.6;
        let drag_idx = if self.multiline {
            let line_height = self.font_size * 1.333;
            let max_chars = (((self.base.w - 16.0) / char_width).floor() as usize).max(1);
            let (lines, index_map) = self.wrap_text(max_chars);
            let click_line = (((py - (self.base.y + 8.0)) / line_height).floor() as isize).max(0) as usize;
            let click_col = (((px - (self.base.x + 8.0)) / char_width).round() as isize).max(0) as usize;
            self.map_2d_to_1d(&index_map, click_line, click_col, lines.len() - 1)
        } else {
            (((px - (self.base.x + 8.0)) / char_width).round() as isize)
                .max(0)
                .min(self.edit_buffer.chars().count() as isize) as usize
        };
        if self.cursor_idx != drag_idx {
            self.cursor_idx = drag_idx;
            self.just_focused = false;
            let len = self.edit_buffer.chars().count();
            let start = self.select_anchor.unwrap_or(0).min(self.cursor_idx);
            let end = self.select_anchor.unwrap_or(0).max(self.cursor_idx);
            self.all_selected = start == 0 && end == len && len > 0;
            return true;
        }
        false
    }

    fn drag_end(&mut self) {
        self.dragging = false;
    }

    fn value(&self) -> i32 { 0 }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if self.disabled { return false; }
        if button != MouseButton::Left { return false; }
        if !self.hit_test(px, py) { return false; }
        match state {
            ElementState::Pressed => {
                if !self.editing {
                    self.focus();
                } else {
                    let char_width = self.font_size * 0.6;
                    let idx = if self.multiline {
                        let line_height = self.font_size * 1.333;
                        let max_chars = (((self.base.w - 16.0) / char_width).floor() as usize).max(1);
                        let (lines, index_map) = self.wrap_text(max_chars);
                        let click_line = (((py - (self.base.y + 8.0)) / line_height).floor() as isize).max(0) as usize;
                        let click_col = (((px - (self.base.x + 8.0)) / char_width).round() as isize).max(0) as usize;
                        self.map_2d_to_1d(&index_map, click_line, click_col, lines.len() - 1)
                    } else {
                        (((px - (self.base.x + 8.0)) / char_width).round() as isize)
                            .max(0)
                            .min(self.edit_buffer.chars().count() as isize) as usize
                    };
                    self.cursor_idx = idx;
                    self.select_anchor = Some(idx);
                    self.all_selected = false;
                }
                true
            }
            ElementState::Released => {
                if self.dragging {
                    self.drag_end();
                }
                if self.select_anchor == Some(self.cursor_idx) {
                    self.select_anchor = None;
                }
                true
            }
        }
    }

    fn focus(&mut self) {
        if self.disabled { return; }
        self.editing = true;
        self.edit_buffer = self.text.clone();
        let len = self.edit_buffer.chars().count();
        self.cursor_idx = len;
        self.select_anchor = Some(0);
        self.all_selected = len > 0;
        self.just_focused = true;
        focus::set_focused(self);
    }

    fn unfocus(&mut self) {
        if self.editing {
            self.editing = false;
            if self.text != self.edit_buffer {
                self.text = self.edit_buffer.clone();
                self.just_changed = true;
            }
            self.select_anchor = None;
            self.all_selected = false;
        }
    }

    fn keyboard_input(&mut self, event: &KeyEvent) -> bool {
        if !self.editing || self.disabled { return false; }
        if event.state != ElementState::Pressed { return false; }
        
        let control = event.ctrl;
        
        match &event.logical_key {
            Key::Named(NamedKey::Backspace) => {
                if self.all_selected {
                    self.edit_buffer.clear();
                    self.cursor_idx = 0;
                    self.select_anchor = None;
                    self.all_selected = false;
                    return true;
                }
                let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
                let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);
                if start != end {
                    let chars: Vec<char> = self.edit_buffer.chars().collect();
                    let mut new_buf = String::new();
                    for i in 0..start {
                        new_buf.push(chars[i]);
                    }
                    for i in end..chars.len() {
                        new_buf.push(chars[i]);
                    }
                    self.edit_buffer = new_buf;
                    self.cursor_idx = start;
                    self.select_anchor = None;
                    return true;
                }
                if self.cursor_idx > 0 {
                    let mut chars: Vec<char> = self.edit_buffer.chars().collect();
                    chars.remove(self.cursor_idx - 1);
                    self.edit_buffer = chars.into_iter().collect();
                    self.cursor_idx -= 1;
                    self.select_anchor = None;
                    return true;
                }
                false
            }
            Key::Named(NamedKey::Delete) => {
                if self.all_selected {
                    self.edit_buffer.clear();
                    self.cursor_idx = 0;
                    self.select_anchor = None;
                    self.all_selected = false;
                    return true;
                }
                let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
                let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);
                if start != end {
                    let chars: Vec<char> = self.edit_buffer.chars().collect();
                    let mut new_buf = String::new();
                    for i in 0..start {
                        new_buf.push(chars[i]);
                    }
                    for i in end..chars.len() {
                        new_buf.push(chars[i]);
                    }
                    self.edit_buffer = new_buf;
                    self.cursor_idx = start;
                    self.select_anchor = None;
                    return true;
                }
                if self.cursor_idx < self.edit_buffer.chars().count() {
                    let mut chars: Vec<char> = self.edit_buffer.chars().collect();
                    chars.remove(self.cursor_idx);
                    self.edit_buffer = chars.into_iter().collect();
                    self.select_anchor = None;
                    return true;
                }
                false
            }
            Key::Named(NamedKey::ArrowLeft) => {
                let shift = event.shift;
                if shift {
                    if self.select_anchor.is_none() {
                        self.select_anchor = Some(self.cursor_idx);
                    }
                    if self.cursor_idx > 0 {
                        self.cursor_idx -= 1;
                        true
                    } else {
                        false
                    }
                } else {
                    if let Some(anchor) = self.select_anchor {
                        self.cursor_idx = anchor.min(self.cursor_idx);
                        self.select_anchor = None;
                        self.all_selected = false;
                        true
                    } else if self.cursor_idx > 0 {
                        self.cursor_idx -= 1;
                        true
                    } else {
                        false
                    }
                }
            }
            Key::Named(NamedKey::ArrowRight) => {
                let shift = event.shift;
                if shift {
                    if self.select_anchor.is_none() {
                        self.select_anchor = Some(self.cursor_idx);
                    }
                    if self.cursor_idx < self.edit_buffer.chars().count() {
                        self.cursor_idx += 1;
                        true
                    } else {
                        false
                    }
                } else {
                    if let Some(anchor) = self.select_anchor {
                        self.cursor_idx = anchor.max(self.cursor_idx);
                        self.select_anchor = None;
                        self.all_selected = false;
                        true
                    } else if self.cursor_idx < self.edit_buffer.chars().count() {
                        self.cursor_idx += 1;
                        true
                    } else {
                        false
                    }
                }
            }
            Key::Named(NamedKey::ArrowUp) => {
                let shift = event.shift;
                if shift {
                    if self.select_anchor.is_none() {
                        self.select_anchor = Some(self.cursor_idx);
                    }
                } else {
                    self.select_anchor = None;
                    self.all_selected = false;
                }
                if self.multiline {
                    let char_width = self.font_size * 0.6;
                    let max_chars = (((self.base.w - 16.0) / char_width).floor() as usize).max(1);
                    let (lines, index_map) = self.wrap_text(max_chars);
                    let (cursor_l, cursor_c) = index_map[self.cursor_idx.min(index_map.len() - 1)];
                    if cursor_l > 0 {
                        self.cursor_idx = self.map_2d_to_1d(&index_map, cursor_l - 1, cursor_c, lines.len() - 1);
                    } else {
                        self.cursor_idx = 0;
                    }
                } else {
                    self.cursor_idx = 0;
                }
                true
            }
            Key::Named(NamedKey::ArrowDown) => {
                let shift = event.shift;
                if shift {
                    if self.select_anchor.is_none() {
                        self.select_anchor = Some(self.cursor_idx);
                    }
                } else {
                    self.select_anchor = None;
                    self.all_selected = false;
                }
                if self.multiline {
                    let char_width = self.font_size * 0.6;
                    let max_chars = (((self.base.w - 16.0) / char_width).floor() as usize).max(1);
                    let (lines, index_map) = self.wrap_text(max_chars);
                    let (cursor_l, cursor_c) = index_map[self.cursor_idx.min(index_map.len() - 1)];
                    if cursor_l < lines.len() - 1 {
                        self.cursor_idx = self.map_2d_to_1d(&index_map, cursor_l + 1, cursor_c, lines.len() - 1);
                    } else {
                        self.cursor_idx = self.edit_buffer.chars().count();
                    }
                } else {
                    self.cursor_idx = self.edit_buffer.chars().count();
                }
                true
            }
            Key::Named(NamedKey::Home) => {
                let shift = event.shift;
                if shift {
                    if self.select_anchor.is_none() {
                        self.select_anchor = Some(self.cursor_idx);
                    }
                } else {
                    self.select_anchor = None;
                    self.all_selected = false;
                }
                self.cursor_idx = 0;
                true
            }
            Key::Named(NamedKey::End) => {
                let shift = event.shift;
                if shift {
                    if self.select_anchor.is_none() {
                        self.select_anchor = Some(self.cursor_idx);
                    }
                } else {
                    self.select_anchor = None;
                    self.all_selected = false;
                }
                self.cursor_idx = self.edit_buffer.chars().count();
                true
            }
            Key::Named(NamedKey::Enter) => {
                if self.multiline {
                    let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
                    let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);
                    let mut new_buf = String::new();
                    let chars: Vec<char> = self.edit_buffer.chars().collect();
                    for i in 0..start {
                        new_buf.push(chars[i]);
                    }
                    new_buf.push('\n');
                    for i in end..chars.len() {
                        new_buf.push(chars[i]);
                    }
                    self.edit_buffer = new_buf;
                    self.cursor_idx = start + 1;
                    self.select_anchor = None;
                    self.all_selected = false;
                } else {
                    self.unfocus();
                }
                true
            }
            Key::Named(NamedKey::Escape) => {
                self.editing = false;
                self.edit_buffer = self.text.clone();
                self.select_anchor = None;
                self.all_selected = false;
                true
            }
            Key::Character(ref ch_str) if control && (ch_str == "a" || ch_str == "A") => {
                self.cursor_idx = self.edit_buffer.chars().count();
                self.select_anchor = Some(0);
                self.all_selected = true;
                true
            }
            Key::Character(ref ch_str) if control && (ch_str == "c" || ch_str == "C") => {
                self.copy_selection();
                true
            }
            Key::Character(ref ch_str) if control && (ch_str == "x" || ch_str == "X") => {
                self.cut_selection();
                true
            }
            Key::Character(ref ch_str) if control && (ch_str == "v" || ch_str == "V") => {
                if let Some(pasted) = clipboard::read_from_clipboard() {
                    let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
                    let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);
                    let mut new_buf = String::new();
                    let chars: Vec<char> = self.edit_buffer.chars().collect();
                    for i in 0..start {
                        new_buf.push(chars[i]);
                    }
                    let inserted_count = pasted.chars().count();
                    new_buf.push_str(&pasted);
                    for i in end..chars.len() {
                        new_buf.push(chars[i]);
                    }
                    self.edit_buffer = new_buf;
                    self.cursor_idx = start + inserted_count;
                    self.select_anchor = None;
                    self.all_selected = false;
                } else {
                    self.select_anchor = None;
                    self.all_selected = false;
                }
                true
            }
            _ => {
                if let Some(text) = &event.text {
                    if !control {
                        let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
                        let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);
                        let mut new_buf = String::new();
                        let chars: Vec<char> = self.edit_buffer.chars().collect();
                        for i in 0..start {
                            new_buf.push(chars[i]);
                        }
                        let inserted_count = text.chars().count();
                        new_buf.push_str(text);
                        for i in end..chars.len() {
                            new_buf.push(chars[i]);
                        }
                        self.edit_buffer = new_buf;
                        self.cursor_idx = start + inserted_count;
                        self.select_anchor = None;
                        self.all_selected = false;
                        return true;
                    }
                }
                false
            }
        }
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        if self.disabled {
            if self.draw_bg_border {
                quads.push((self.base.x, self.base.y, self.base.w, self.base.h, [0.12, 0.12, 0.16, 1.0]));
                quads.push((self.base.x + 1.0, self.base.y + 1.0, self.base.w - 2.0, self.base.h - 2.0, [0.06, 0.06, 0.08, 1.0]));
            }
            return quads;
        }
        
        if self.draw_bg_border {
            let bg_color = if self.editing {
                [0.06, 0.10, 0.18, 1.0]
            } else {
                [0.08, 0.08, 0.12, 1.0]
            };
            let border_color = if self.editing {
                [0.20, 0.50, 0.85, 1.0]
            } else if self.base.hovered {
                [0.25, 0.25, 0.35, 1.0]
            } else {
                [0.18, 0.18, 0.24, 1.0]
            };
            quads.push((self.base.x, self.base.y, self.base.w, self.base.h, border_color));
            quads.push((self.base.x + 1.0, self.base.y + 1.0, self.base.w - 2.0, self.base.h - 2.0, bg_color));
        }

        if self.editing {
            let char_width = self.font_size * 0.6;
            let line_height = self.font_size * 1.333;
            
            let highlight_color = [0.20, 0.50, 0.85, 0.3];
            let cursor_color = if self.draw_bg_border {
                [0.80, 0.80, 0.85, 1.0]
            } else {
                [0.10, 0.10, 0.15, 1.0]
            };

            let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
            let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);

            if self.multiline {
                let max_chars = (((self.base.w - 16.0) / char_width).floor() as usize).max(1);
                let (_lines, index_map) = self.wrap_text(max_chars);

                if start != end {
                    let start_pos = index_map[start.min(index_map.len() - 1)];
                    let end_pos = index_map[end.min(index_map.len() - 1)];
                    
                    for line_idx in start_pos.0..=end_pos.0 {
                        let mut line_start_col = None;
                        let mut line_end_col = None;
                        for idx in start..end {
                            if idx < index_map.len() {
                                let (l, c) = index_map[idx];
                                if l == line_idx {
                                    if line_start_col.is_none() || c < line_start_col.unwrap() {
                                        line_start_col = Some(c);
                                    }
                                    if line_end_col.is_none() || c > line_end_col.unwrap() {
                                        line_end_col = Some(c);
                                    }
                                }
                            }
                        }
                        if let (Some(sc), Some(ec)) = (line_start_col, line_end_col) {
                            let highlight_x = self.base.x + 8.0 + (sc as f32 * char_width);
                            let highlight_w = (ec - sc + 1) as f32 * char_width;
                            let highlight_y = self.base.y + 8.0 + (line_idx as f32 * line_height);
                            quads.push((
                                highlight_x,
                                highlight_y,
                                highlight_w,
                                line_height,
                                highlight_color,
                            ));
                        }
                    }
                }
                
                let caret_h = self.font_size * 1.15;
                let (cursor_l, cursor_c) = index_map[self.cursor_idx.min(index_map.len() - 1)];
                let cursor_x = self.base.x + 8.0 + (cursor_c as f32 * char_width);
                let cursor_y = self.base.y + 8.0 + (cursor_l as f32 * line_height) + (line_height - caret_h) / 2.0;
                quads.push((cursor_x, cursor_y, 1.5, caret_h, cursor_color));
            } else {
                let caret_h = self.font_size * 1.15;
                let line_h = self.font_size * 1.333;
                if start != end {
                    let highlight_x = self.base.x + 8.0 + (start as f32 * char_width);
                    let max_x = self.base.x + self.base.w - 6.0;
                    let highlight_w = ((end - start) as f32 * char_width).min(max_x - highlight_x).max(0.0);
                    quads.push((
                        highlight_x,
                        self.base.y + (self.base.h - line_h) / 2.0,
                        highlight_w,
                        line_h,
                        highlight_color,
                    ));
                }

                let cursor_x = self.base.x + 8.0 + (self.cursor_idx as f32 * char_width);
                let max_cursor_x = self.base.x + self.base.w - 6.0;
                let final_cursor_x = cursor_x.min(max_cursor_x);
                let cursor_y = self.base.y + (self.base.h - caret_h) / 2.0;
                quads.push((final_cursor_x, cursor_y, 1.5, caret_h, cursor_color));
            }
        }

        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        if let Some(ref label) = self.base.label {
            labels.push(TextLabel {
                text: label.clone(),
                x: self.base.x + 4.0,
                y: self.base.y - 14.0,
                font_size: 11.0,
                color: [0x83, 0x83, 0x8a],
            });
        }
        let mut val_text = if self.editing {
            self.edit_buffer.clone()
        } else {
            self.text.clone()
        };
        if self.is_password {
            val_text = "•".repeat(val_text.chars().count());
        }

        let label_color = if let Some(custom_color) = self.text_color {
            custom_color
        } else if self.disabled {
            [0x53, 0x53, 0x5a]
        } else if self.all_selected {
            [0xff, 0xff, 0xff]
        } else if self.editing {
            [0xee, 0xee, 0xf5]
        } else {
            [0xcc, 0xcc, 0xd4]
        };

        if self.multiline {
            let char_width = self.font_size * 0.6;
            let line_height = self.font_size * 1.333;
            let max_chars = (((self.base.w - 16.0) / char_width).floor() as usize).max(1);
            let (lines, _) = self.wrap_text(max_chars);
            for (line_idx, line_text) in lines.iter().enumerate() {
                labels.push(TextLabel {
                    text: line_text.clone(),
                    x: self.base.x + 8.0,
                    y: self.base.y + 8.0 + (line_idx as f32 * line_height) + (line_height - self.font_size) / 2.0,
                    font_size: self.font_size,
                    color: label_color,
                });
            }
        } else {
            labels.push(TextLabel {
                text: val_text,
                x: self.base.x + 8.0,
                y: self.base.y + (self.base.h - self.font_size) / 2.0,
                font_size: self.font_size,
                color: label_color,
            });
        }
        labels
    }

    fn parent(&self) -> Option<*mut (dyn Element + 'static)> { self.parent }
    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>) { self.parent = parent; }
    fn children(&self) -> Vec<*mut (dyn Element + 'static)> { self.children.clone() }
    fn add_child(&mut self, child: *mut (dyn Element + 'static)) { self.children.push(child); }
    fn clear_children(&mut self) { self.children.clear(); }
}

impl Drop for TextBox {
    fn drop(&mut self) {
        focus::clear_if_matches(self);
    }
}

unsafe impl Send for TextBox {}
unsafe impl Sync for TextBox {}

use std::sync::OnceLock;
static FONT_DB: OnceLock<resvg::usvg::fontdb::Database> = OnceLock::new();

pub fn get_font_db() -> &'static resvg::usvg::fontdb::Database {
    FONT_DB.get_or_init(|| {
        let mut db = resvg::usvg::fontdb::Database::new();
        db.load_system_fonts();
        db
    })
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Finger {
    pub slot: usize,
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone)]
pub struct Trackpad {
    base: Widget,
    pub fingers: Vec<Finger>,
}

impl Trackpad {
    pub fn new() -> Self {
        Self {
            base: Widget::new(),
            fingers: Vec::new(),
        }
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self
    }

    pub fn set_fingers(&mut self, fingers: Vec<Finger>) {
        self.fingers = fingers;
    }

    pub fn label_offset(&self) -> f32 {
        if self.base.label.is_some() {
            16.0
        } else {
            0.0
        }
    }
}

impl Element for Trackpad {
    crate::impl_widget_base!(Trackpad);

    fn color(&self) -> [f32; 4] {
        [0.11, 0.11, 0.16, 0.85]
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let (x, y, w, h) = self.rect();
        let mut quads = Vec::new();

        // 1. Background
        quads.push((x, y, w, h, [0.11, 0.11, 0.16, 0.85]));

        // 2. Borders
        let border_color = [0.28, 0.28, 0.38, 1.0];
        quads.push((x, y, w, 1.0, border_color));             // Top
        quads.push((x, y + h - 1.0, w, 1.0, border_color));     // Bottom
        quads.push((x, y, 1.0, h, border_color));             // Left
        quads.push((x + w - 1.0, y, 1.0, h, border_color));     // Right

        // 3. Fingers
        for finger in &self.fingers {
            let rx = finger.x.clamp(0.0, 1.0);
            let ry = finger.y.clamp(0.0, 1.0);
            let fx = x + rx * w;
            let fy = y + ry * h;
            let dot_size = 12.0;

            // Render glow (outer light blue rectangle)
            quads.push((
                fx - (dot_size + 6.0) / 2.0,
                fy - (dot_size + 6.0) / 2.0,
                dot_size + 6.0,
                dot_size + 6.0,
                [0.35, 0.55, 0.95, 0.4],
            ));
            // Render core (solid blue/purple rectangle)
            quads.push((
                fx - dot_size / 2.0,
                fy - dot_size / 2.0,
                dot_size,
                dot_size,
                [0.45, 0.65, 1.0, 1.0],
            ));
        }

        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let (x, y, _w, h) = self.rect();
        let mut labels = Vec::new();

        // Render "Touchpad Area" label
        labels.push(TextLabel {
            text: "Touchpad Area".to_string(),
            x: x + 12.0,
            y: y + h - 22.0,
            font_size: 11.0,
            color: [0x73, 0x73, 0x8c],
        });

        // Optional widget-base label on top
        if let Some(ref label) = self.base.label {
            labels.push(TextLabel {
                text: label.clone(),
                x,
                y: y - 18.0,
                font_size: 12.0,
                color: [0x83, 0x83, 0x8a],
            });
        }

        labels
    }

    fn draggable(&self) -> bool { true }
    fn is_dragging(&self) -> bool { !self.fingers.is_empty() }

    fn drag_begin(&mut self, px: f32, py: f32) {
        let (x, y, w, h) = self.rect();
        if w > 0.0 && h > 0.0 {
            let rx = ((px - x) / w).clamp(0.0, 1.0);
            let ry = ((py - y) / h).clamp(0.0, 1.0);
            self.fingers = vec![Finger { slot: 0, x: rx, y: ry }];
        }
    }

    fn drag_update(&mut self, px: f32, py: f32) -> bool {
        let (x, y, w, h) = self.rect();
        if w > 0.0 && h > 0.0 {
            let rx = ((px - x) / w).clamp(0.0, 1.0);
            let ry = ((py - y) / h).clamp(0.0, 1.0);
            self.fingers = vec![Finger { slot: 0, x: rx, y: ry }];
            true
        } else {
            false
        }
    }

    fn drag_end(&mut self) {
        self.fingers.clear();
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button == MouseButton::Left {
            let (x, y, w, h) = self.rect();
            if px >= x && px <= x + w && py >= y && py <= y + h {
                if state == ElementState::Pressed {
                    let rx = ((px - x) / w).clamp(0.0, 1.0);
                    let ry = ((py - y) / h).clamp(0.0, 1.0);
                    self.fingers = vec![Finger { slot: 0, x: rx, y: ry }];
                    return true;
                } else {
                    self.fingers.clear();
                    return true;
                }
            }
        }
        false
    }
}

// ── FontSelector Widget ──

#[derive(Debug, Clone)]
pub struct FontSelector {
    base: Widget,
    pub font_family: String,
    just_changed: bool,
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub children: Vec<*mut (dyn Element + 'static)>,
    pressed: bool,
}

impl FontSelector {
    pub fn new(font_family: String) -> Self {
        Self {
            base: Widget::new(),
            font_family,
            just_changed: false,
            parent: None,
            children: Vec::new(),
            pressed: false,
        }
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self
    }

    pub fn take_change(&mut self) -> bool {
        let changed = self.just_changed;
        self.just_changed = false;
        changed
    }
}

impl Element for FontSelector {
    crate::impl_widget_base!(FontSelector);

    fn color(&self) -> [f32; 4] {
        [0.08, 0.08, 0.12, 1.0]
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
                    self.pressed = false;
                    let output = std::process::Command::new("/home/lsgalante/.local/bin/clear-typeface-interface")
                        .arg("--select")
                        .output();
                    if let Ok(out) = output {
                        if out.status.success() {
                            let stdout = String::from_utf8_lossy(&out.stdout);
                            let trimmed = stdout.trim().to_string();
                            if !trimmed.is_empty() && trimmed != self.font_family {
                                self.font_family = trimmed;
                                self.just_changed = true;
                            }
                        }
                    }
                    return true;
                }
                let was = self.pressed;
                self.pressed = false;
                return was;
            }
        }
        false
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        let bg_color = [0.08, 0.08, 0.12, 1.0];
        let border_color = if self.pressed {
            [0.30, 0.50, 0.32, 1.0]
        } else if self.base.hovered {
            [0.25, 0.25, 0.35, 1.0]
        } else {
            [0.18, 0.18, 0.24, 1.0]
        };

        quads.push((self.base.x, self.base.y, self.base.w, self.base.h, border_color));
        quads.push((self.base.x + 1.0, self.base.y + 1.0, self.base.w - 2.0, self.base.h - 2.0, bg_color));
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        if let Some(ref label) = self.base.label {
            labels.push(TextLabel {
                text: label.clone(),
                x: self.base.x + 4.0,
                y: self.base.y - 14.0,
                font_size: 11.0,
                color: [0x83, 0x83, 0x8a],
            });
        }

        labels.push(TextLabel {
            text: self.font_family.clone(),
            x: self.base.x + 8.0,
            y: self.base.y + (self.base.h - 12.0) / 2.0,
            font_size: 12.0,
            color: [0xdd, 0xdd, 0xe2],
        });

        labels.push(TextLabel {
            text: "🔤".to_string(),
            x: self.base.x + self.base.w - 20.0,
            y: self.base.y + (self.base.h - 11.0) / 2.0,
            font_size: 11.0,
            color: [0x83, 0x83, 0x8a],
        });

        labels
    }

    fn parent(&self) -> Option<*mut (dyn Element + 'static)> { self.parent }
    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>) { self.parent = parent; }
    fn children(&self) -> Vec<*mut (dyn Element + 'static)> { self.children.clone() }
    fn add_child(&mut self, child: *mut (dyn Element + 'static)) { self.children.push(child); }
    fn clear_children(&mut self) { self.children.clear(); }
}

impl Drop for FontSelector {
    fn drop(&mut self) {
        focus::clear_if_matches(self);
    }
}

unsafe impl Send for FontSelector {}
unsafe impl Sync for FontSelector {}


