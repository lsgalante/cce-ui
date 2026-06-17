use crate::colors;
use crate::widget::*;
use crate::widget::display::TextLabel;

pub struct Float3 {
    base: Widget,
    pub values: [f32; 3],
    pub(crate) mins: [f32; 3],
    pub(crate) maxs: [f32; 3],
    labels: [String; 3],
    dragging_idx: Option<usize>,
    drag_offset: f32,
    pub editing_idx: Option<usize>,
    pub edit_buffer: String,
}

impl Float3 {
    pub fn new() -> Self {
        Self {
            base: Widget::new(),
            values: [0.5, 0.5, 0.5],
            mins: [0.0, 0.0, 0.0],
            maxs: [1.0, 1.0, 1.0],
            labels: ["X".to_string(), "Y".to_string(), "Z".to_string()],
            dragging_idx: None,
            drag_offset: 0.0,
            editing_idx: None,
            edit_buffer: String::new(),
        }
    }

    pub fn with_values(mut self, values: [f32; 3]) -> Self {
        self.values = values;
        self
    }

    pub fn with_range(mut self, min: f32, max: f32) -> Self {
        self.mins = [min, min, min];
        self.maxs = [max, max, max];
        self
    }

    pub fn set_values(&mut self, values: [f32; 3]) {
        self.values = values;
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self
    }

    pub fn get_row_rects(&self) -> Vec<(f32, f32, f32, f32)> {
        let mut rects = Vec::new();
        let by = self.base.y + 20.0;
        for i in 0..3 {
            rects.push((self.base.x + 8.0, by + 6.0 + i as f32 * 26.0, self.base.w - 16.0, 20.0));
        }
        rects
    }
}

impl Element for Float3 {
    crate::impl_widget_base!(Float3);
    fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }
    fn rect(&self) -> (f32, f32, f32, f32) { (self.base.x, self.base.y, self.base.w, self.base.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.base.x = x; self.base.y = y; self.base.w = w; self.base.h = h;
    }

    fn draggable(&self) -> bool { self.dragging_idx.is_some() }
    fn is_dragging(&self) -> bool { self.dragging_idx.is_some() }
    
    fn drag_begin(&mut self, _px: f32, _py: f32) {}
    
    fn drag_update(&mut self, px: f32, _py: f32) -> bool {
        if let Some(i) = self.dragging_idx {
            let track_x = self.base.x + 100.0;
            let track_w = self.base.w - 188.0;
            let thumb_size = 12.0 * 0.9;
            let range = track_w - thumb_size;
            if range > 0.0 {
                let raw = (px - self.drag_offset - track_x) / range;
                let new_val = raw.clamp(0.0, 1.0);
                if (new_val - self.values[i]).abs() > 0.001 {
                    self.values[i] = new_val;
                    if self.editing_idx == Some(i) {
                        let scaled_val = self.mins[i] + self.values[i] * (self.maxs[i] - self.mins[i]);
                        self.edit_buffer = format!("{:.2}", scaled_val);
                    }
                    return true;
                }
            }
        }
        false
    }
    
    fn drag_end(&mut self) {
        self.dragging_idx = None;
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if button != MouseButton::Left { return false; }
        let rects = self.get_row_rects();
        
        for i in 0..3 {
            let r = rects[i];
            let rx = self.base.x + self.base.w - 68.0;
            let ry = r.1 + 4.0;
            let rh = 12.0;
            let readout_w = 60.0;
            
            if px >= rx && px <= rx + readout_w && py >= ry && py <= ry + rh {
                if state == ElementState::Pressed {
                    if self.editing_idx != Some(i) {
                        self.unfocus();
                        self.editing_idx = Some(i);
                        let scaled_val = self.mins[i] + self.values[i] * (self.maxs[i] - self.mins[i]);
                        self.edit_buffer = format!("{:.2}", scaled_val);
                        focus::set_focused(self);
                    }
                }
                return true;
            }
        }
        
        if state == ElementState::Pressed {
            for i in 0..3 {
                let r = rects[i];
                let track_x = self.base.x + 100.0;
                let track_w = self.base.w - 188.0;
                let track_y = r.1 + 4.0;
                let track_h = 12.0;
                let thumb_size = track_h * 0.9;
                let range = track_w - thumb_size;
                let thumb_x = track_x + self.values[i] * range;
                
                if px >= track_x && px <= track_x + track_w && py >= track_y && py <= track_y + track_h {
                    self.dragging_idx = Some(i);
                    self.drag_offset = px - thumb_x;
                    return true;
                }
            }
        } else if state == ElementState::Released {
            if self.dragging_idx.is_some() {
                self.dragging_idx = None;
                return true;
            }
        }
        false
    }

    fn keyboard_input(&mut self, event: &KeyEvent, ctx: &mut UiContext) -> bool {
        let _idx = match self.editing_idx {
            Some(i) => i,
            None => return false,
        };
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
                self.editing_idx = None;
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
        
        if self.editing_idx.is_some() {
            self.edit_buffer = state.buffer;
        }
        handled
    }

    fn unfocus(&mut self) {
        if let Some(i) = self.editing_idx.take() {
            if let Ok(new_val) = self.edit_buffer.parse::<f32>() {
                let range = self.maxs[i] - self.mins[i];
                if range != 0.0 {
                    self.values[i] = ((new_val - self.mins[i]) / range).clamp(0.0, 1.0);
                } else {
                    self.values[i] = 0.0;
                }
            }
        }
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        
        // Outline border box
        let bx = self.base.x + 4.0;
        let bw = self.base.w - 8.0;
        let by = self.base.y + 20.0;
        let bh = 84.0;
        let border_color = [0.18, 0.18, 0.27, 1.0];
        let border_t = 1.0;
        
        quads.push((bx, by, bw, border_t, border_color));
        quads.push((bx, by + bh - border_t, bw, border_t, border_color));
        quads.push((bx, by, border_t, bh, border_color));
        quads.push((bx + bw - border_t, by, border_t, bh, border_color));
        
        let rects = self.get_row_rects();
        for i in 0..3 {
            let r = rects[i];
            let track_x = self.base.x + 100.0;
            let track_w = self.base.w - 188.0;
            let track_y = r.1 + 4.0;
            let track_h = 12.0;
            
            quads.push((track_x, track_y, track_w, track_h, colors::slider_track()));
            
            let thumb_size = track_h * 0.9;
            let range = track_w - thumb_size;
            let thumb_x = track_x + self.values[i] * range;
            let thumb_color = if self.dragging_idx == Some(i) {
                colors::SLIDER_THUMB_DRAG
            } else {
                colors::SLIDER_THUMB
            };
            quads.push((thumb_x, track_y + (track_h - thumb_size)/2.0, thumb_size, thumb_size, thumb_color));
            
            let rx = self.base.x + self.base.w - 68.0;
            let bg_color = if self.editing_idx == Some(i) {
                [0.06, 0.10, 0.18, 1.0]
            } else {
                [0.10, 0.10, 0.13, 1.0]
            };
            quads.push((rx, track_y, 60.0, track_h, bg_color));
            
            if self.editing_idx == Some(i) {
                let border_color = [0.20, 0.50, 0.85, 1.0];
                quads.push((rx, track_y, 60.0, border_t, border_color));
                quads.push((rx, track_y + track_h - border_t, 60.0, border_t, border_color));
                quads.push((rx, track_y, border_t, track_h, border_color));
                quads.push((rx + 60.0 - border_t, track_y, border_t, track_h, border_color));
            }
        }
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        
        if let Some(ref l) = self.base.label {
            labels.push(TextLabel {
                text: l.clone(),
                x: self.base.x + 8.0,
                y: self.base.y + 2.0,
                font_size: 13.0,
                color: [0xee, 0xee, 0xf0],
            });
        }
        
        let rects = self.get_row_rects();
        for i in 0..3 {
            let r = rects[i];
            let track_y = r.1 + 4.0;
            let ry = track_y;
            
            labels.push(TextLabel {
                text: self.labels[i].clone(),
                x: self.base.x + 16.0,
                y: ry - 2.0,
                font_size: 12.0,
                color: [0xaa, 0xaa, 0xbb],
            });
            
            let rx = self.base.x + self.base.w - 68.0;
            let text = if self.editing_idx == Some(i) {
                self.edit_buffer.clone()
            } else {
                let scaled_val = self.mins[i] + self.values[i] * (self.maxs[i] - self.mins[i]);
                format!("{:.2}", scaled_val)
            };
            labels.push(TextLabel {
                text,
                x: rx + 8.0,
                y: ry - 2.0,
                font_size: 12.0,
                color: [0xee, 0xee, 0xf0],
            });
        }
        labels
    }
}

impl Drop for Float3 {
    fn drop(&mut self) {
        clear_widget_references(self);
    }
}

