use crate::colors;
use crate::widget::*;

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
    pub editor_state: TextEditorState,
    pub just_changed: bool,
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
            editor_state: TextEditorState::new(String::new()),
            just_changed: false,
        }
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

    pub fn range(&self) -> (i32, i32) {
        (self.min, self.max)
    }
}

impl Element for Spinbox {
    crate::impl_widget_base!(Spinbox);

    fn get_value_string(&self) -> Option<String> {
        if self.decimals > 0 {
            let divisor = 10.0f32.powi(self.decimals as i32);
            Some(format!("{:.width$}", self.value as f32 / divisor, width = self.decimals as usize))
        } else {
            Some(self.value.to_string())
        }
    }

    fn set_value_string(&mut self, val: &str) -> bool {
        let val = val.trim();
        let old_val = self.value;
        if self.decimals > 0 {
            if let Ok(val_f) = val.parse::<f32>() {
                let divisor = 10.0f32.powi(self.decimals as i32);
                self.value = (val_f * divisor).round() as i32;
                self.value = self.value.clamp(self.min, self.max);
            }
        } else {
            if let Ok(val_i) = val.parse::<i32>() {
                self.value = val_i.clamp(self.min, self.max);
            }
        }
        if self.value != old_val {
            self.just_changed = true;
            if self.editing {
                self.edit_buffer = self.get_value_string().unwrap_or_default();
                self.cursor_idx = self.edit_buffer.chars().count();
            }
            return true;
        }
        false
    }

    fn take_change(&mut self) -> bool {
        let ret = self.just_changed;
        self.just_changed = false;
        ret
    }

    fn preferred_height(&self) -> Option<f32> {
        Some(crate::layout::spinbox_height())
    }

    fn rounded_corners(&self) -> (bool, bool, bool, bool) {
        let r = crate::layout::spinbox_corner_radius();
        if r > 0.0 {
            (true, true, true, true)
        } else {
            (false, false, false, false)
        }
    }

    fn corner_radius(&self) -> f32 {
        crate::layout::spinbox_corner_radius()
    }

    fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }
    fn value(&self) -> i32 { self.value }
    fn widget_font(&self) -> Option<String> { Some(crate::layout::spinbox_font()) }

    fn on_cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        let was = self.base.hovered;
        self.base.hovered = self.hit_test(px, py, ctx);
        if !self.base.hovered {
            let changed = self.hover_dec || self.hover_inc;
            self.hover_dec = false;
            self.hover_inc = false;
            return changed || was != self.base.hovered;
        }
        let hd = px >= self.base.x + self.base.w * 0.55 && px < self.base.x + self.base.w * 0.775;
        let hi = px >= self.base.x + self.base.w * 0.775;
        let changed = hd != self.hover_dec || hi != self.hover_inc;
        self.hover_dec = hd;
        self.hover_inc = hi;
        changed || was != self.base.hovered
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if button == MouseButton::Right && state == ElementState::Pressed {
            if self.hit_test(px, py, ctx) {
                ctx.handle_right_click(self.as_ptr_mut(), px, py);
                return true;
            }
        }
        if button != MouseButton::Left { return false; }
        if !self.hit_test(px, py, ctx) { return false; }
        match state {
            ElementState::Pressed => {
                let display_w = self.base.w * 0.55;
                let btn_w = self.base.w * 0.225;
                let split_dec = self.base.x + display_w;
                let split_inc = self.base.x + display_w + btn_w;
                if px >= split_dec && px < split_inc {
                    let old_val = self.value;
                    self.value = (self.value - self.step).max(self.min);
                    if self.value != old_val {
                        self.just_changed = true;
                    }
                    true
                } else if px >= split_inc {
                    let old_val = self.value;
                    self.value = (self.value + self.step).min(self.max);
                    if self.value != old_val {
                        self.just_changed = true;
                    }
                    true
                } else {
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
            let old_val = self.value;
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
            if self.value != old_val {
                self.just_changed = true;
            }
        }
    }

    fn keyboard_input(&mut self, event: &KeyEvent, _ctx: &mut UiContext) -> bool {
        if !self.editing { return false; }
        if event.state != ElementState::Pressed { return false; }
        
        let mut state = TextEditorState {
            buffer: self.edit_buffer.clone(),
            cursor_idx: self.cursor_idx,
            select_anchor: None,
            all_selected: false,
        };
        
        let mut handled = false;
        match &event.logical_key {
            Key::Named(NamedKey::Backspace) => {
                handled = state.delete_backwards();
            }
            Key::Named(NamedKey::Delete) => {
                handled = state.delete_forwards();
            }
            Key::Named(NamedKey::ArrowLeft) => {
                handled = state.move_cursor_left(false);
            }
            Key::Named(NamedKey::ArrowRight) => {
                handled = state.move_cursor_right(false);
            }
            Key::Named(NamedKey::Enter) => {
                let old_val = self.value;
                if self.decimals > 0 {
                    if let Ok(val_f) = state.buffer.parse::<f32>() {
                        let divisor = 10.0f32.powi(self.decimals as i32);
                        self.value = (val_f * divisor).round() as i32;
                        self.value = self.value.clamp(self.min, self.max);
                    }
                } else {
                    if let Ok(val) = state.buffer.parse::<i32>() {
                        self.value = val.clamp(self.min, self.max);
                    }
                }
                if self.value != old_val {
                    self.just_changed = true;
                }
                self.editing = false;
                handled = true;
            }
            Key::Named(NamedKey::Escape) => {
                self.editing = false;
                handled = true;
            }
            _ => {
                if let Some(text) = &event.text {
                    for ch in text.chars() {
                        match ch {
                            '-' if state.cursor_idx == 0 && !state.buffer.starts_with('-') => {
                                state.insert_text("-");
                                handled = true;
                            }
                            '.' if self.decimals > 0 && !state.buffer.contains('.') => {
                                state.insert_text(".");
                                handled = true;
                            }
                            '0'..='9' => {
                                state.insert_text(&ch.to_string());
                                handled = true;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        
        if self.editing {
            self.edit_buffer = state.buffer;
            self.cursor_idx = state.cursor_idx;
        }
        handled
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        let top = self.base.label_offset();
        let visual_h = self.base.h - top;
        let btn_w = self.base.w * 0.225;
        let display_w = self.base.w * 0.55;
        let split_dec = self.base.x + display_w;
        let split_inc = self.base.x + display_w + btn_w;
        let inc_col = if self.hover_inc { colors::SPINBOX_BUTTON_HOVER } else { colors::SPINBOX_BUTTON };
        let dec_col = if self.hover_dec { colors::SPINBOX_BUTTON_HOVER } else { colors::SPINBOX_BUTTON };
        
        let display_bg = if self.editing {
            [0.06, 0.10, 0.18, 1.0]
        } else {
            colors::SPINBOX_DISPLAY
        };
        
        quads.push((self.base.x, self.base.y + top, display_w, visual_h, display_bg));
        quads.push((split_dec, self.base.y + top, btn_w, visual_h, dec_col));
        quads.push((split_inc, self.base.y + top, btn_w, visual_h, inc_col));
        
        if self.editing {
            let border_color = [0.20, 0.50, 0.85, 1.0];
            quads.push((self.base.x, self.base.y + top, display_w, 1.0, border_color));
            quads.push((self.base.x, self.base.y + top + visual_h - 1.0, display_w, 1.0, border_color));
            quads.push((self.base.x, self.base.y + top, 1.0, visual_h, border_color));
            quads.push((self.base.x + display_w - 1.0, self.base.y + top, 1.0, visual_h, border_color));

            let char_width = 8.4;
            let cursor_x = self.base.x + 4.0 + (self.cursor_idx as f32 * char_width);
            let max_cursor_x = self.base.x + display_w - 4.0;
            let final_cursor_x = cursor_x.min(max_cursor_x);
            let cursor_y = self.base.y + top + (visual_h - 14.0) / 2.0;
            quads.push((final_cursor_x, cursor_y, 1.5, 14.0, [0.80, 0.80, 0.85, 1.0]));
        }
        
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        if let Some(lbl) = self.control_label() {
            labels.push(lbl);
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
        let _visual_h = self.base.h - top;
        
        labels.push(TextLabel {
            text: value_text,
            x: self.base.x + 4.0,
            y: crate::layout::align_text_y(self.base.y, self.base.h, 14.0, top),
            font_size: 14.0,
            color: [0xcc, 0xcc, 0xd4],
        });
        if let Some(ref unit) = self.unit {
            labels.push(TextLabel {
                text: unit.clone(),
                x: self.base.x + 4.0 + 36.0,
                y: crate::layout::align_text_y(self.base.y, self.base.h, 11.0, top),
                font_size: 11.0,
                color: [0x73, 0x73, 0x7a],
            });
        }

        labels.push(TextLabel {
            text: "-".to_string(),
            x: self.base.x + self.base.w * 0.6625 - 4.0,
            y: crate::layout::align_text_y(self.base.y, self.base.h, 12.0, top),
            font_size: 12.0,
            color: [0xcc, 0xcc, 0xd4],
        });
        labels.push(TextLabel {
            text: "+".to_string(),
            x: self.base.x + self.base.w * 0.8875 - 4.0,
            y: crate::layout::align_text_y(self.base.y, self.base.h, 12.0, top),
            font_size: 12.0,
            color: [0xcc, 0xcc, 0xd4],
        });
        labels
    }

    fn layout_ignore(&self) -> bool {
        true
    }
}

impl Drop for Spinbox {
    fn drop(&mut self) {
        clear_widget_references(self);
    }
}

impl Control for Spinbox {}

unsafe impl Send for Spinbox {}
unsafe impl Sync for Spinbox {}

