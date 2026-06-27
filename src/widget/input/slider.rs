use crate::colors;
use crate::widget::*;

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
    pub editor_state: TextEditorState,
    pub just_changed: bool,
}

impl Slider {
    pub fn new() -> Self {
        Self {
            base: Widget::new(),
            dragging: false,
            value: 0.5,
            drag_offset: 0.0,
            scroll_enabled: true,
            show_readout: false,
            editing: false,
            edit_buffer: String::new(),
            min: 0.0,
            max: 1.0,
            editor_state: TextEditorState::new(String::new()),
            just_changed: false,
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

    pub fn with_config(mut self, file: &str, key: &str) -> Self {
        self.base.config_file = Some(file.to_string());
        self.base.config_key = Some(key.to_string());
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

    pub fn range(&self) -> (f32, f32) {
        (self.min, self.max)
    }

    pub fn set_scaled_value(&mut self, val: f32) {
        let range = self.max - self.min;
        if range != 0.0 {
            self.value = ((val - self.min) / range).clamp(0.0, 1.0);
        } else {
            self.value = 0.0;
        }
    }
}

impl Element for Slider {
    crate::impl_widget_base!(Slider);

    fn get_value_string(&self) -> Option<String> {
        let scaled_val = self.min + self.value * (self.max - self.min);
        Some(format!("{:.2}", scaled_val))
    }

    fn set_value_string(&mut self, val: &str) -> bool {
        if let Ok(new_val) = val.trim().parse::<f32>() {
            let old_val = self.value;
            let range = self.max - self.min;
            if range != 0.0 {
                self.value = ((new_val - self.min) / range).clamp(0.0, 1.0);
            } else {
                self.value = 0.0;
            }
            if (self.value - old_val).abs() > 0.0001 {
                self.just_changed = true;
                if self.editing {
                    let scaled_val = self.min + self.value * (self.max - self.min);
                    self.edit_buffer = format!("{:.2}", scaled_val);
                }
                return true;
            }
        }
        false
    }

    fn take_change(&mut self) -> bool {
        let ret = self.just_changed;
        self.just_changed = false;
        ret
    }

    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn preferred_height(&self) -> Option<f32> {
        Some(crate::layout::slider_height())
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

    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if !self.scroll_enabled {
            return false;
        }
        let my_id = self.base.id();
        if !ctx.scroll_gesture_new {
            if ctx.scroll_initiate_widget_id != Some(my_id) {
                return false;
            }
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
            if ctx.scroll_gesture_new {
                ctx.scroll_initiate_widget_id = Some(my_id);
            }
            let scroll_amount = match delta {
                MouseScrollDelta::LineDelta(_x, y) => *y,
                MouseScrollDelta::PixelDelta(pos) => (pos.y as f32) / 120.0,
            };
            let step = 0.02;
            let new_val = (self.value - scroll_amount * step).clamp(0.0, 1.0);
            if (new_val - self.value).abs() > 0.0001 {
                self.value = new_val;
                self.just_changed = true;
                if self.editing {
                    let scaled_val = self.min + self.value * (self.max - self.min);
                    self.edit_buffer = format!("{:.2}", scaled_val);
                }
            }
            return true;
        }
        false
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if button == MouseButton::Right && state == ElementState::Pressed {
            if self.hit_test(px, py, ctx) {
                ctx.handle_right_click(self.as_ptr_mut(), px, py);
                return true;
            }
        }
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

    fn keyboard_input(&mut self, event: &KeyEvent, _ctx: &mut UiContext) -> bool {
        if !self.editing { return false; }
        if event.state != ElementState::Pressed { return false; }
        
        let mut state = TextEditorState {
            buffer: self.edit_buffer.clone(),
            cursor_idx: self.edit_buffer.chars().count(),
            select_anchor: None,
            all_selected: false,
        };
        
        let mut handled = false;
        match &event.logical_key {
            Key::Named(NamedKey::Backspace) => {
                state.delete_backwards();
                handled = true;
            }
            Key::Named(NamedKey::Enter) => {
                self.unfocus();
                handled = true;
            }
            Key::Named(NamedKey::Escape) => {
                self.editing = false;
                handled = true;
            }
            Key::Character(s) => {
                for ch in s.chars() {
                    if ch.is_ascii_digit() || ch == '.' || (ch == '-' && state.buffer.is_empty()) {
                        state.insert_text(&ch.to_string());
                    }
                }
                handled = true;
            }
            _ => {}
        }
        
        if self.editing {
            self.edit_buffer = state.buffer;
        }
        handled
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
        let _visual_h = self.base.h - top;
        
        if let Some(lbl) = self.control_label() {
            labels.push(lbl);
        }
        
        if self.show_readout {
            let readout_w = 60.0;
            let rx = self.base.x + self.base.w - readout_w;
            let ry = crate::layout::align_text_y(self.base.y, self.base.h, 12.0, top);
            
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

impl Drop for Slider {
    fn drop(&mut self) {
        clear_widget_references(self);
    }
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

    fn preferred_height(&self) -> Option<f32> {
        Some(crate::layout::slider_height())
    }

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

    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        let my_id = self.base.id();
        if !ctx.scroll_gesture_new {
            if ctx.scroll_initiate_widget_id != Some(my_id) {
                return false;
            }
        }
        let top = self.base.label_offset();
        let visual_h = self.base.h - top;
        let (sx, sy, sw, _) = self.rect();
        if px >= sx && px <= sx + sw && py >= sy + top && py <= sy + top + visual_h {
            if ctx.scroll_gesture_new {
                ctx.scroll_initiate_widget_id = Some(my_id);
            }
            let thumb_size = visual_h * 0.9;
            let range = sw - thumb_size;
            let thumb_low_x = sx + self.value_low * range;
            let thumb_high_x = sx + self.value_high * range;
            let center_low = thumb_low_x + thumb_size / 2.0;
            let center_high = thumb_high_x + thumb_size / 2.0;

            let dist_low = (px - center_low).abs();
            let dist_high = (px - center_high).abs();

            let scroll_amount = match delta {
                MouseScrollDelta::LineDelta(_x, y) => *y,
                MouseScrollDelta::PixelDelta(pos) => (pos.y as f32) / 120.0,
            };
            let step = 0.02;

            let adjust_low = if dist_low < dist_high {
                true
            } else if dist_high < dist_low {
                false
            } else {
                // dist_low == dist_high, e.g. when both thumbs are at the same value
                // If scroll decreases the value, adjust Low so it can move down.
                // Otherwise, adjust High so it can move up.
                scroll_amount > 0.0
            };

            if adjust_low {
                let new_val = (self.value_low - scroll_amount * step).clamp(0.0, self.value_high);
                if (new_val - self.value_low).abs() > 0.0001 {
                    self.value_low = new_val;
                }
            } else {
                let new_val = (self.value_high - scroll_amount * step).clamp(self.value_low, 1.0);
                if (new_val - self.value_high).abs() > 0.0001 {
                    self.value_high = new_val;
                }
            }
            return true;
        }
        false
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

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        if let Some(lbl) = self.control_label() {
            labels.push(lbl);
        }
        labels
    }
}

impl Control for Slider {}
impl Control for RangeSlider {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rangeslider_interaction() {
        let mut rs = RangeSlider::new();
        rs.set_rect(10.0, 10.0, 200.0, 20.0);

        // Low value: 0.2, High value: 0.8
        let (low, high) = rs.values();
        assert_eq!(low, 0.2);
        assert_eq!(high, 0.8);

        // Thumb size = h * 0.9 = 18.0
        // Range = w - thumb_size = 200.0 - 18.0 = 182.0
        // Thumb low center: x + 0.2 * 182.0 + 9.0 = 10.0 + 36.4 + 9.0 = 55.4
        // Thumb high center: x + 0.8 * 182.0 + 9.0 = 10.0 + 145.6 + 9.0 = 164.6

        // 1. Drag Low thumb from 0.2 to 0.45
        // Click at px = 55.4 (center of low thumb)
        rs.drag_begin(55.4, 20.0);
        assert_eq!(rs.active_thumb, Some(ActiveThumb::Low));

        // Drag to px = 100.9 (new low value = (100.9 - offset(9.0) - 10.0) / 182.0 = 81.9 / 182.0 = 0.45)
        let changed = rs.drag_update(100.9, 20.0);
        assert!(changed);
        assert!((rs.values().0 - 0.45).abs() < 0.01);
        assert_eq!(rs.values().1, 0.8); // High value unchanged

        rs.drag_end();
        assert_eq!(rs.active_thumb, None);

        // 2. Drag High thumb from 0.8 to 0.6
        // Click at px = 164.6 (center of high thumb)
        rs.drag_begin(164.6, 20.0);
        assert_eq!(rs.active_thumb, Some(ActiveThumb::High));

        // Drag to px = 128.2 (new high value = (128.2 - offset(9.0) - 10.0) / 182.0 = 109.2 / 182.0 = 0.6)
        let changed = rs.drag_update(128.2, 20.0);
        assert!(changed);
        assert!((rs.values().1 - 0.6).abs() < 0.01);

        rs.drag_end();
    }

    #[test]
    fn test_rangeslider_overlap() {
        let mut rs = RangeSlider::new().with_values(0.5, 0.5);
        rs.set_rect(10.0, 10.0, 200.0, 20.0);

        // Both low and high are 0.5. Thumb center = 10.0 + 0.5 * 182.0 + 9.0 = 110.0
        // Click to the left of center should select Low thumb
        rs.drag_begin(109.0, 20.0);
        assert_eq!(rs.active_thumb, Some(ActiveThumb::Low));
        rs.drag_end();

        // Click to the right of center should select High thumb
        rs.drag_begin(111.0, 20.0);
        assert_eq!(rs.active_thumb, Some(ActiveThumb::High));
        rs.drag_end();

        // Drag Low thumb past High value (0.5). It should be constrained to 0.5
        rs.drag_begin(110.0, 20.0); // selects low
        rs.drag_update(150.0, 20.0); // drag past high
        assert_eq!(rs.values().0, 0.5); // constrained
        rs.drag_end();
    }

    #[test]
    fn test_rangeslider_mouse_wheel() {
        let mut rs = RangeSlider::new().with_values(0.3, 0.7);
        rs.set_rect(10.0, 10.0, 200.0, 20.0);
        let mut dummy_ctx = crate::context::UiContext::new();
        dummy_ctx.scroll_gesture_new = true;

        // Thumb size = h * 0.9 = 18.0
        // Range = w - thumb_size = 182.0
        // Low thumb center: x + 0.3 * 182.0 + 9.0 = 10.0 + 54.6 + 9.0 = 73.6
        // High thumb center: x + 0.7 * 182.0 + 9.0 = 10.0 + 127.4 + 9.0 = 146.4

        // Scroll near low thumb (px = 75.0, py = 20.0)
        // Scroll UP: LineDelta(0.0, 1.0). (value_low - 1.0 * 0.02) = 0.28.
        let delta = MouseScrollDelta::LineDelta(0.0, 1.0);
        let handled = rs.mouse_wheel(&delta, 75.0, 20.0, &mut dummy_ctx);
        assert!(handled);
        assert!((rs.values().0 - 0.28).abs() < 0.001);
        assert_eq!(rs.values().1, 0.7); // high unchanged

        // Scroll near high thumb (px = 145.0, py = 20.0)
        // Scroll DOWN: LineDelta(0.0, -1.0). (value_high - (-1.0) * 0.02) = 0.72.
        let delta_down = MouseScrollDelta::LineDelta(0.0, -1.0);
        let handled = rs.mouse_wheel(&delta_down, 145.0, 20.0, &mut dummy_ctx);
        assert!(handled);
        assert!((rs.values().1 - 0.72).abs() < 0.001);
        assert!((rs.values().0 - 0.28).abs() < 0.001); // low unchanged

        // Scroll when both are at 0.5 (rs is updated to 0.5, 0.5)
        rs.set_values(0.5, 0.5);
        // Center: 110.0. Scroll at px = 110.0.
        // Scroll UP (decrease): LineDelta(0.0, 1.0).
        // Since it's a decrease (scroll_amount > 0), adjust_low should be true.
        // new_val for low = (0.5 - 0.02) = 0.48.
        let handled = rs.mouse_wheel(&delta, 110.0, 20.0, &mut dummy_ctx);
        assert!(handled);
        assert!((rs.values().0 - 0.48).abs() < 0.001);
        assert_eq!(rs.values().1, 0.5); // high unchanged

        // Reset both to 0.5
        rs.set_values(0.5, 0.5);
        // Scroll DOWN (increase): LineDelta(0.0, -1.0).
        // Since it's an increase (scroll_amount < 0), adjust_low should be false.
        // new_val for high = (0.5 - (-0.02)) = 0.52.
        let handled = rs.mouse_wheel(&delta_down, 110.0, 20.0, &mut dummy_ctx);
        assert!(handled);
        assert_eq!(rs.values().0, 0.5); // low unchanged
        assert!((rs.values().1 - 0.52).abs() < 0.001);
    }

    #[test]
    fn test_slider_scroll_initiation() {
        let mut slider1 = Slider::new();
        slider1.set_rect(10.0, 10.0, 200.0, 20.0);
        let id1 = slider1.base.id();

        let mut slider2 = Slider::new();
        slider2.set_rect(10.0, 40.0, 200.0, 20.0);
        let _id2 = slider2.base.id();

        let mut ctx = crate::context::UiContext::new();

        // 1. Initial scroll event on slider1
        // This is a new gesture (last_scroll_time is None)
        ctx.scroll_gesture_new = true;
        ctx.scroll_initiate_widget_id = None;
        let delta = MouseScrollDelta::LineDelta(0.0, 1.0);
        
        let handled = slider1.mouse_wheel(&delta, 50.0, 15.0, &mut ctx);
        assert!(handled);
        assert_eq!(ctx.scroll_initiate_widget_id, Some(id1));

        // 2. Subsequent scroll event in the same gesture (elapsed < 250ms), but the mouse moved over slider2
        ctx.scroll_gesture_new = false;
        // The mouse wheel event is now routed to slider2
        let handled2 = slider2.mouse_wheel(&delta, 50.0, 45.0, &mut ctx);
        // slider2 must reject the event because it wasn't the initiator
        assert!(!handled2);
        
        // 3. Subsequent scroll event routed to slider1 (the initiator)
        let handled1 = slider1.mouse_wheel(&delta, 50.0, 15.0, &mut ctx);
        assert!(handled1);
    }
}
