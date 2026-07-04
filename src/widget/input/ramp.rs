use crate::colors;
use crate::widget::*;
use crate::widget::input::{Slider, Button};

#[derive(Debug, Clone)]
pub struct RampKey {
    pub pos: f32,
    pub color: [f32; 3],
}

pub struct Ramp {
    pub base: Widget,
    pub keys: Vec<RampKey>,
    pub selected_key_idx: Option<usize>,
    pub is_dragging_key: bool,
    
    // Child controls for color editing & deletion
    pub r_slider: Slider,
    pub g_slider: Slider,
    pub b_slider: Slider,
    pub del_button: Button,
    
    pub parent: Option<*mut (dyn Element + 'static)>,
}

impl Ramp {
    pub fn new() -> Self {
        let keys = vec![
            RampKey { pos: 0.0, color: [0.0, 0.0, 0.0] },
            RampKey { pos: 1.0, color: [1.0, 1.0, 1.0] },
        ];
        
        let r_slider = Slider::new().with_label("Red");
        let g_slider = Slider::new().with_label("Green");
        let b_slider = Slider::new().with_label("Blue");
        let del_button = Button::new(0.0, 0.0, 70.0, 28.0).with_label("Delete Key");
        
        Self {
            base: Widget::new(),
            keys,
            selected_key_idx: None,
            is_dragging_key: false,
            r_slider,
            g_slider,
            b_slider,
            del_button,
            parent: None,
        }
    }
    
    // Linear color interpolation helper
    pub fn get_interpolated_color(&self, t: f32) -> [f32; 3] {
        if self.keys.is_empty() {
            return [0.0, 0.0, 0.0];
        }
        if t <= self.keys[0].pos {
            return self.keys[0].color;
        }
        if t >= self.keys[self.keys.len() - 1].pos {
            return self.keys[self.keys.len() - 1].color;
        }
        
        for i in 0..self.keys.len() - 1 {
            let k1 = &self.keys[i];
            let k2 = &self.keys[i+1];
            if t >= k1.pos && t <= k2.pos {
                let range = k2.pos - k1.pos;
                if range.abs() < 0.0001 {
                    return k1.color;
                }
                let w = (t - k1.pos) / range;
                return [
                    k1.color[0] * (1.0 - w) + k2.color[0] * w,
                    k1.color[1] * (1.0 - w) + k2.color[1] * w,
                    k1.color[2] * (1.0 - w) + k2.color[2] * w,
                ];
            }
        }
        self.keys[0].color
    }
    
    fn sort_keys(&mut self) {
        let prev_selected_id = self.selected_key_idx.map(|idx| self.keys[idx].pos);
        self.keys.sort_by(|a, b| a.pos.partial_cmp(&b.pos).unwrap());
        if let Some(pos) = prev_selected_id {
            if let Some(new_idx) = self.keys.iter().position(|k| (k.pos - pos).abs() < 0.0001) {
                self.selected_key_idx = Some(new_idx);
            }
        }
    }
}

impl Element for Ramp {
    crate::impl_widget_base!(Ramp);
    
    fn parent(&self, _ctx: &UiContext) -> Option<*mut (dyn Element + 'static)> { self.parent }
    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>, _ctx: &mut UiContext) {
        self.parent = parent;
    }
    
    fn children(&self, _ctx: &UiContext) -> Vec<*mut (dyn Element + 'static)> {
        let self_ptr = self as *const Self as *mut Self;
        unsafe {
            vec![
                &mut (*self_ptr).r_slider as *mut Slider as *mut (dyn Element + 'static),
                &mut (*self_ptr).g_slider as *mut Slider as *mut (dyn Element + 'static),
                &mut (*self_ptr).b_slider as *mut Slider as *mut (dyn Element + 'static),
                &mut (*self_ptr).del_button as *mut Button as *mut (dyn Element + 'static),
            ]
        }
    }
    
    fn color(&self) -> [f32; 4] {
        colors::ramp_background_color()
    }
    
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.base.x = x;
        self.base.y = y;
        self.base.w = w;
        self.base.h = h;
        
        let self_ptr = self.as_ptr_mut();
        let mut dummy = crate::context::UiContext::new();
        self.r_slider.set_parent(Some(self_ptr), &mut dummy);
        self.g_slider.set_parent(Some(self_ptr), &mut dummy);
        self.b_slider.set_parent(Some(self_ptr), &mut dummy);
        self.del_button.set_parent(Some(self_ptr), &mut dummy);
        
        let th = crate::layout::ramp_height();
        let sy = y + th + 45.0;
        let slider_w = w - 100.0;
        
        if self.selected_key_idx.is_some() {
            self.r_slider.set_rect(x + 10.0, sy, slider_w, 20.0);
            self.g_slider.set_rect(x + 10.0, sy + 25.0, slider_w, 20.0);
            self.b_slider.set_rect(x + 10.0, sy + 50.0, slider_w, 20.0);
            self.del_button.set_rect(x + w - 80.0, sy + 20.0, 70.0, 28.0);
        } else {
            self.r_slider.set_rect(-1000.0, -1000.0, 0.0, 0.0);
            self.g_slider.set_rect(-1000.0, -1000.0, 0.0, 0.0);
            self.b_slider.set_rect(-1000.0, -1000.0, 0.0, 0.0);
            self.del_button.set_rect(-1000.0, -1000.0, 0.0, 0.0);
        }
    }
    
    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        let th = crate::layout::ramp_height();
        let track_x = self.base.x + 10.0;
        let track_w = self.base.w - 20.0;
        
        // Draw track border
        let border_color = colors::ramp_border_color();
        quads.push((track_x - 1.0, self.base.y - 1.0, track_w + 2.0, th + 2.0, border_color));
        
        // Draw interpolated track slices (e.g. 100 slices)
        let slices = 100;
        let slice_w = track_w / slices as f32;
        for i in 0..slices {
            let t1 = i as f32 / slices as f32;
            let t2 = (i + 1) as f32 / slices as f32;
            let center_t = (t1 + t2) / 2.0;
            let col = self.get_interpolated_color(center_t);
            let sx = track_x + t1 * track_w;
            quads.push((sx, self.base.y, slice_w, th, [col[0], col[1], col[2], 1.0]));
        }
        
        // Return child quads if selected
        if self.selected_key_idx.is_some() {
            let ctx_dummy = crate::context::UiContext::new();
            quads.extend(self.r_slider.all_quads(&ctx_dummy));
            quads.extend(self.g_slider.all_quads(&ctx_dummy));
            quads.extend(self.b_slider.all_quads(&ctx_dummy));
            quads.extend(self.del_button.all_quads(&ctx_dummy));
        }
        
        quads
    }
    
    fn extra_circles(&self) -> Vec<(f32, f32, f32, [f32; 4])> {
        let mut circles = Vec::new();
        let th = crate::layout::ramp_height();
        let track_x = self.base.x + 10.0;
        let track_w = self.base.w - 20.0;
        let py = self.base.y + th + 15.0;
        
        for (idx, key) in self.keys.iter().enumerate() {
            let cx = track_x + key.pos * track_w;
            
            // Draw peg outline / shadow
            circles.push((cx, py, 7.0, [0.0, 0.0, 0.0, 0.8]));
            
            // Draw peg color preview
            circles.push((cx, py, 6.0, [key.color[0], key.color[1], key.color[2], 1.0]));
            
            // Highlight selected peg
            if Some(idx) == self.selected_key_idx {
                circles.push((cx, py, 8.0, [0.49, 1.0, 1.0, 0.5]));
            }
        }
        
        circles
    }
    
    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py_event: f32, ctx: &mut UiContext) -> bool {
        if button != MouseButton::Left { return false; }
        
        let th = crate::layout::ramp_height();
        let track_x = self.base.x + 10.0;
        let track_w = self.base.w - 20.0;
        let py_peg = self.base.y + th + 15.0;
        
        if state == ElementState::Pressed {
            // 1. Check peg click selection/dragging
            for (idx, key) in self.keys.iter().enumerate() {
                let cx = track_x + key.pos * track_w;
                let dx = px - cx;
                let dy = py_event - py_peg;
                if (dx*dx + dy*dy) <= 64.0 { // hit circle radius 8.0
                    self.selected_key_idx = Some(idx);
                    self.is_dragging_key = true;
                    
                    // Update slider values to match key color
                    self.r_slider.set_value(key.color[0]);
                    self.g_slider.set_value(key.color[1]);
                    self.b_slider.set_value(key.color[2]);
                    
                    self.set_rect(self.base.x, self.base.y, self.base.w, self.base.h);
                    return true;
                }
            }
            
            // 2. Check track click to add key
            if px >= track_x && px <= track_x + track_w && py_event >= self.base.y && py_event <= self.base.y + th {
                let t = (px - track_x) / track_w;
                let col = self.get_interpolated_color(t);
                let new_key = RampKey { pos: t, color: col };
                self.keys.push(new_key);
                self.sort_keys();
                
                // Select newly added key
                if let Some(new_idx) = self.keys.iter().position(|k| (k.pos - t).abs() < 0.0001) {
                    self.selected_key_idx = Some(new_idx);
                    self.r_slider.set_value(col[0]);
                    self.g_slider.set_value(col[1]);
                    self.b_slider.set_value(col[2]);
                }
                
                self.set_rect(self.base.x, self.base.y, self.base.w, self.base.h);
                return true;
            }
            
            // 3. Delegate to slider / button click
            if self.selected_key_idx.is_some() {
                if self.r_slider.mouse_input(button, state, px, py_event, ctx) { return true; }
                if self.g_slider.mouse_input(button, state, px, py_event, ctx) { return true; }
                if self.b_slider.mouse_input(button, state, px, py_event, ctx) { return true; }
                if self.del_button.mouse_input(button, state, px, py_event, ctx) {
                    // Check if clicked
                    if self.del_button.take_click() {
                        if let Some(idx) = self.selected_key_idx {
                            if self.keys.len() > 2 {
                                self.keys.remove(idx);
                                self.selected_key_idx = None;
                                self.set_rect(self.base.x, self.base.y, self.base.w, self.base.h);
                            }
                        }
                    }
                    return true;
                }
            }
        } else {
            // Mouse released
            self.is_dragging_key = false;
            
            if self.selected_key_idx.is_some() {
                self.r_slider.mouse_input(button, state, px, py_event, ctx);
                self.g_slider.mouse_input(button, state, px, py_event, ctx);
                self.b_slider.mouse_input(button, state, px, py_event, ctx);
                if self.del_button.mouse_input(button, state, px, py_event, ctx) {
                    if self.del_button.take_click() {
                        if let Some(idx) = self.selected_key_idx {
                            if self.keys.len() > 2 {
                                self.keys.remove(idx);
                                self.selected_key_idx = None;
                                self.set_rect(self.base.x, self.base.y, self.base.w, self.base.h);
                            }
                        }
                    }
                }
                return true;
            }
        }
        
        false
    }
    
    fn cursor_moved(&mut self, px: f32, py_event: f32, ctx: &mut UiContext) -> bool {
        let mut changed = false;
        
        let _th = crate::layout::ramp_height();
        let track_x = self.base.x + 10.0;
        let track_w = self.base.w - 20.0;
        
        // 1. Update key dragging
        if self.is_dragging_key {
            if let Some(idx) = self.selected_key_idx {
                let t = ((px - track_x) / track_w).clamp(0.0, 1.0);
                self.keys[idx].pos = t;
                self.sort_keys();
                changed = true;
            }
        }
        
        // 2. Delegate to sliders / buttons
        if self.selected_key_idx.is_some() {
            if self.r_slider.cursor_moved(px, py_event, ctx) {
                if let Some(idx) = self.selected_key_idx {
                    self.keys[idx].color[0] = self.r_slider.value();
                    changed = true;
                }
            }
            if self.g_slider.cursor_moved(px, py_event, ctx) {
                if let Some(idx) = self.selected_key_idx {
                    self.keys[idx].color[1] = self.g_slider.value();
                    changed = true;
                }
            }
            if self.b_slider.cursor_moved(px, py_event, ctx) {
                if let Some(idx) = self.selected_key_idx {
                    self.keys[idx].color[2] = self.b_slider.value();
                    changed = true;
                }
            }
            if self.del_button.cursor_moved(px, py_event, ctx) {
                changed = true;
            }
        }
        
        changed
    }
    
    fn text_labels_with_font_and_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        let mut labels = Vec::new();
        if self.selected_key_idx.is_some() {
            labels.extend(self.r_slider.text_labels_with_font_and_bounds(ctx));
            labels.extend(self.g_slider.text_labels_with_font_and_bounds(ctx));
            labels.extend(self.b_slider.text_labels_with_font_and_bounds(ctx));
            labels.extend(self.del_button.text_labels_with_font_and_bounds(ctx));
        }
        labels
    }
    
    fn get_text_items(&self) -> Vec<(&glyphon::Buffer, f32, f32, glyphon::Color)> {
        let mut items = Vec::new();
        if self.selected_key_idx.is_some() {
            items.extend(self.r_slider.get_text_items());
            items.extend(self.g_slider.get_text_items());
            items.extend(self.b_slider.get_text_items());
            items.extend(self.del_button.get_text_items());
        }
        items
    }
}

impl Drop for Ramp {
    fn drop(&mut self) {
        clear_widget_references(self);
    }
}
