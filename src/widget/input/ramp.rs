use crate::colors;
use crate::widget::*;
use crate::widget::input::{Slider, Button};

// ==========================================
// 1. Color Ramp (renamed from Ramp)
// ==========================================

#[derive(Debug, Clone)]
pub struct ColorRampKey {
    pub pos: f32,
    pub color: [f32; 3],
}

pub struct ColorRamp {
    pub base: Widget,
    pub keys: Vec<ColorRampKey>,
    pub selected_key_idx: Option<usize>,
    pub is_dragging_key: bool,
    pub just_changed: bool,
    
    // Child controls for color editing & deletion
    pub r_slider: Slider,
    pub g_slider: Slider,
    pub b_slider: Slider,
    pub del_button: Button,
    
    pub parent: Option<*mut (dyn Element + 'static)>,
}

impl ColorRamp {
    pub fn new() -> Self {
        let keys = vec![
            ColorRampKey { pos: 0.0, color: [0.0, 0.0, 0.0] },
            ColorRampKey { pos: 1.0, color: [1.0, 1.0, 1.0] },
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
            just_changed: false,
            r_slider,
            g_slider,
            b_slider,
            del_button,
            parent: None,
        }
    }
    
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

impl Element for ColorRamp {
    crate::impl_widget_base!(ColorRamp);
    
    fn tick(&mut self, dt: f32, ctx: &mut UiContext) -> bool {
        let mut changed = self.just_changed;
        self.just_changed = false;
        
        if self.selected_key_idx.is_some() {
            if self.r_slider.tick(dt, ctx) {
                if let Some(idx) = self.selected_key_idx {
                    self.keys[idx].color[0] = self.r_slider.value();
                }
                changed = true;
            }
            if self.g_slider.tick(dt, ctx) {
                if let Some(idx) = self.selected_key_idx {
                    self.keys[idx].color[1] = self.g_slider.value();
                }
                changed = true;
            }
            if self.b_slider.tick(dt, ctx) {
                if let Some(idx) = self.selected_key_idx {
                    self.keys[idx].color[2] = self.b_slider.value();
                }
                changed = true;
            }
            if self.del_button.tick(dt, ctx) {
                changed = true;
            }
        }
        changed
    }
    
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
        let sy = y + th + 55.0;
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
        
        // Draw outer container border
        let bx = self.base.x;
        let by = self.base.y;
        let bw = self.base.w;
        let bh = self.base.h;
        let border_color = colors::ramp_border_color();
        quads.push((bx, by, bw, 1.0, border_color));                 // Top
        quads.push((bx, by + bh - 1.0, bw, 1.0, border_color));         // Bottom
        quads.push((bx, by, 1.0, bh, border_color));                 // Left
        quads.push((bx + bw - 1.0, by, 1.0, bh, border_color));         // Right
        
        // Draw track border
        quads.push((track_x - 1.0, self.base.y + 10.0 - 1.0, track_w + 2.0, th + 2.0, border_color));
        
        // Draw interpolated track slices (e.g. 100 slices)
        let slices = 100;
        let slice_w = track_w / slices as f32;
        for i in 0..slices {
            let t1 = i as f32 / slices as f32;
            let t2 = (i + 1) as f32 / slices as f32;
            let center_t = (t1 + t2) / 2.0;
            let col = self.get_interpolated_color(center_t);
            let sx = track_x + t1 * track_w;
            quads.push((sx, self.base.y + 10.0, slice_w, th, [col[0], col[1], col[2], 1.0]));
        }
        
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
        let py = self.base.y + 10.0 + th + 15.0;
        
        for (idx, key) in self.keys.iter().enumerate() {
            let cx = track_x + key.pos * track_w;
            circles.push((cx, py, 7.0, [0.0, 0.0, 0.0, 0.8]));
            circles.push((cx, py, 6.0, [key.color[0], key.color[1], key.color[2], 1.0]));
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
        let py_peg = self.base.y + 10.0 + th + 15.0;
        
        if state == ElementState::Pressed {
            for (idx, key) in self.keys.iter().enumerate() {
                let cx = track_x + key.pos * track_w;
                let dx = px - cx;
                let dy = py_event - py_peg;
                if (dx*dx + dy*dy) <= 64.0 {
                    self.selected_key_idx = Some(idx);
                    self.is_dragging_key = true;
                    self.r_slider.set_value(key.color[0]);
                    self.g_slider.set_value(key.color[1]);
                    self.b_slider.set_value(key.color[2]);
                    self.set_rect(self.base.x, self.base.y, self.base.w, self.base.h);
                    return true;
                }
            }
            
            if px >= track_x && px <= track_x + track_w && py_event >= self.base.y + 10.0 && py_event <= self.base.y + 10.0 + th {
                let t = (px - track_x) / track_w;
                let col = self.get_interpolated_color(t);
                let new_key = ColorRampKey { pos: t, color: col };
                self.keys.push(new_key);
                self.sort_keys();
                self.just_changed = true;
                
                if let Some(new_idx) = self.keys.iter().position(|k| (k.pos - t).abs() < 0.0001) {
                    self.selected_key_idx = Some(new_idx);
                    self.r_slider.set_value(col[0]);
                    self.g_slider.set_value(col[1]);
                    self.b_slider.set_value(col[2]);
                }
                self.set_rect(self.base.x, self.base.y, self.base.w, self.base.h);
                return true;
            }
            
            if self.selected_key_idx.is_some() {
                if self.r_slider.mouse_input(button, state, px, py_event, ctx) { return true; }
                if self.g_slider.mouse_input(button, state, px, py_event, ctx) { return true; }
                if self.b_slider.mouse_input(button, state, px, py_event, ctx) { return true; }
                if self.del_button.mouse_input(button, state, px, py_event, ctx) {
                    if self.del_button.take_click() {
                        if let Some(idx) = self.selected_key_idx {
                            if self.keys.len() > 2 {
                                self.keys.remove(idx);
                                self.selected_key_idx = None;
                                self.just_changed = true;
                                self.set_rect(self.base.x, self.base.y, self.base.w, self.base.h);
                            }
                        }
                    }
                    return true;
                }
            }
        } else {
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
                                self.just_changed = true;
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
        let track_x = self.base.x + 10.0;
        let track_w = self.base.w - 20.0;
        
        if self.is_dragging_key {
            if let Some(idx) = self.selected_key_idx {
                let t = ((px - track_x) / track_w).clamp(0.0, 1.0);
                self.keys[idx].pos = t;
                self.sort_keys();
                changed = true;
            }
        }
        
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
        if changed {
            self.just_changed = true;
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

impl Drop for ColorRamp {
    fn drop(&mut self) {
        clear_widget_references(self);
    }
}


// ==========================================
// 2. Houdini-Style Float Ramp
// ==========================================

#[derive(Debug, Clone)]
pub struct RampKey {
    pub pos: f32,
    pub value: f32,
}

pub struct Ramp {
    pub base: Widget,
    pub keys: Vec<RampKey>,
    pub selected_key_idx: Option<usize>,
    pub is_dragging_key: bool,
    pub just_changed: bool,
    
    // Child controls for value editing & deletion
    pub val_slider: Slider,
    pub del_button: Button,
    pub preset_dropdown: Dropdown,
    pub line_type_dropdown: Dropdown,
    
    pub parent: Option<*mut (dyn Element + 'static)>,
}

impl Ramp {
    pub fn new() -> Self {
        let keys = vec![
            RampKey { pos: 0.0, value: 0.5 },
            RampKey { pos: 0.2, value: 1.0 },
            RampKey { pos: 0.8, value: 1.0 },
            RampKey { pos: 1.0, value: 0.5 },
        ];
        
        let val_slider = Slider::new().with_label("Value");
        let del_button = Button::new(0.0, 0.0, 70.0, 28.0).with_label("Delete Key");
        let preset_dropdown = Dropdown::new(
            vec![
                "Custom".to_string(),
                "Linear".to_string(),
                "Bevel (Raised)".to_string(),
                "Bevel (Sunken)".to_string(),
                "Peak".to_string(),
                "Valley".to_string(),
            ],
            2,
        ).with_label("Preset")
         .with_open_upward(false);
        let line_type_dropdown = Dropdown::new(
            vec![
                "Linear".to_string(),
                "Bezier".to_string(),
            ],
            0,
        ).with_label("Line Type")
         .with_open_upward(false);
        
        Self {
            base: Widget::new(),
            keys,
            selected_key_idx: None,
            is_dragging_key: false,
            just_changed: false,
            val_slider,
            del_button,
            preset_dropdown,
            line_type_dropdown,
            parent: None,
        }
    }
    
    pub fn apply_preset(&mut self, idx: usize) {
        match idx {
            1 => { // Linear
                self.keys = vec![
                    RampKey { pos: 0.0, value: 0.0 },
                    RampKey { pos: 1.0, value: 1.0 },
                ];
            }
            2 => { // Bevel (Raised)
                self.keys = vec![
                    RampKey { pos: 0.0, value: 0.5 },
                    RampKey { pos: 0.2, value: 1.0 },
                    RampKey { pos: 0.8, value: 1.0 },
                    RampKey { pos: 1.0, value: 0.5 },
                ];
            }
            3 => { // Bevel (Sunken)
                self.keys = vec![
                    RampKey { pos: 0.0, value: 0.5 },
                    RampKey { pos: 0.2, value: 0.0 },
                    RampKey { pos: 0.8, value: 0.0 },
                    RampKey { pos: 1.0, value: 0.5 },
                ];
            }
            4 => { // Peak
                self.keys = vec![
                    RampKey { pos: 0.0, value: 0.0 },
                    RampKey { pos: 0.5, value: 1.0 },
                    RampKey { pos: 1.0, value: 0.0 },
                ];
            }
            5 => { // Valley
                self.keys = vec![
                    RampKey { pos: 0.0, value: 1.0 },
                    RampKey { pos: 0.5, value: 0.0 },
                    RampKey { pos: 1.0, value: 1.0 },
                ];
            }
            _ => {}
        }
        self.selected_key_idx = None;
        self.just_changed = true;
    }
    
    pub fn get_interpolated_value(&self, t: f32) -> f32 {
        if self.keys.is_empty() {
            return 0.0;
        }
        if t <= self.keys[0].pos {
            return self.keys[0].value;
        }
        if t >= self.keys[self.keys.len() - 1].pos {
            return self.keys[self.keys.len() - 1].value;
        }
        
        for i in 0..self.keys.len() - 1 {
            let k1 = &self.keys[i];
            let k2 = &self.keys[i+1];
            if t >= k1.pos && t <= k2.pos {
                let range = k2.pos - k1.pos;
                if range.abs() < 0.0001 {
                    return k1.value;
                }
                let w = (t - k1.pos) / range;
                if self.line_type_dropdown.selected == 1 {
                    let w_smooth = w * w * (3.0 - 2.0 * w);
                    return k1.value * (1.0 - w_smooth) + k2.value * w_smooth;
                } else {
                    return k1.value * (1.0 - w) + k2.value * w;
                }
            }
        }
        self.keys[0].value
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
    
    fn hit_test(&self, px: f32, py: f32, ctx: &UiContext) -> bool {
        if ctx.is_coordinate_covered(self as *const Self as *const () as usize, px, py) {
            return false;
        }
        if let Some(pop_rect) = self.popover_rect() {
            if px >= pop_rect.0 && px <= pop_rect.0 + pop_rect.2 && py >= pop_rect.1 && py <= pop_rect.1 + pop_rect.3 {
                return true;
            }
        }
        
        let (x, y, w, h) = self.rect();
        if w <= 0.0 || h <= 0.0 {
            return false;
        }
        px >= x && px <= x + w && py >= y && py <= y + h
    }
    
    fn tick(&mut self, dt: f32, ctx: &mut UiContext) -> bool {
        let mut changed = self.just_changed;
        self.just_changed = false;
        
        if self.preset_dropdown.tick(dt, ctx) {
            let idx = self.preset_dropdown.selected;
            self.apply_preset(idx);
            changed = true;
        }
        
        if self.line_type_dropdown.tick(dt, ctx) {
            changed = true;
        }
        
        if self.selected_key_idx.is_some() {
            if self.val_slider.tick(dt, ctx) {
                if let Some(idx) = self.selected_key_idx {
                    self.keys[idx].value = self.val_slider.value();
                    self.preset_dropdown.selected = 0; // Custom
                }
                changed = true;
            }
            if self.del_button.tick(dt, ctx) {
                self.preset_dropdown.selected = 0; // Custom
                changed = true;
            }
        }
        changed
    }
    
    fn parent(&self, _ctx: &UiContext) -> Option<*mut (dyn Element + 'static)> { self.parent }
    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>, _ctx: &mut UiContext) {
        self.parent = parent;
    }
    
    fn children(&self, _ctx: &UiContext) -> Vec<*mut (dyn Element + 'static)> {
        let self_ptr = self as *const Self as *mut Self;
        unsafe {
            let mut list = vec![
                &mut (*self_ptr).preset_dropdown as *mut Dropdown as *mut (dyn Element + 'static),
                &mut (*self_ptr).line_type_dropdown as *mut Dropdown as *mut (dyn Element + 'static),
            ];
            if self.selected_key_idx.is_some() {
                list.push(&mut (*self_ptr).val_slider as *mut Slider as *mut (dyn Element + 'static));
                list.push(&mut (*self_ptr).del_button as *mut Button as *mut (dyn Element + 'static));
            }
            list
        }
    }
    
    fn color(&self) -> [f32; 4] {
        colors::ramp_background_color()
    }
    
    fn preferred_height(&self) -> Option<f32> {
        Some(150.0)
    }
    
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.base.x = x;
        self.base.y = y;
        self.base.w = w;
        self.base.h = h;
        
        let self_ptr = self.as_ptr_mut();
        let mut dummy = crate::context::UiContext::new();
        self.val_slider.set_parent(Some(self_ptr), &mut dummy);
        self.del_button.set_parent(Some(self_ptr), &mut dummy);
        self.preset_dropdown.set_parent(Some(self_ptr), &mut dummy);
        self.line_type_dropdown.set_parent(Some(self_ptr), &mut dummy);
        
        let gh = 80.0;
        let sy = y + gh + 25.0;
        
        let track_x = x + 10.0;
        let track_w = w - 20.0;
        
        if self.selected_key_idx.is_some() {
            let gap = 10.0;
            let del_w = 70.0;
            let available_for_inputs = track_w - del_w - 3.0 * gap;
            let col_w = (available_for_inputs / 3.0).max(40.0);
            
            self.preset_dropdown.set_rect(track_x, sy, col_w, 20.0);
            self.line_type_dropdown.set_rect(track_x + col_w + gap, sy, col_w, 20.0);
            self.val_slider.set_rect(track_x + 2.0 * (col_w + gap), sy, col_w, 20.0);
            self.del_button.set_rect(track_x + track_w - del_w, sy - 4.0, del_w, 28.0);
        } else {
            let gap = 10.0;
            let col_w = (track_w - gap) / 2.0;
            self.preset_dropdown.set_rect(track_x, sy, col_w, 20.0);
            self.line_type_dropdown.set_rect(track_x + col_w + gap, sy, col_w, 20.0);
            self.val_slider.set_rect(-1000.0, -1000.0, 0.0, 0.0);
            self.del_button.set_rect(-1000.0, -1000.0, 0.0, 0.0);
        }
    }
    
    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        let gh = 80.0;
        let track_x = self.base.x + 10.0;
        let track_w = self.base.w - 20.0;
        
        let border_color = colors::ramp_border_color();
        
        // Draw container background (enclosing graph and dropdowns)
        let ch = self.base.h - 20.0;
        quads.push((track_x, self.base.y + 10.0, track_w, ch, [0.15, 0.15, 0.18, 1.0]));
        // Draw container border
        quads.push((track_x - 1.0, self.base.y + 10.0 - 1.0, track_w + 2.0, ch + 2.0, border_color));
        
        // Draw grid lines
        for ratio in [0.25, 0.5, 0.75] {
            let gy = self.base.y + 10.0 + gh * (1.0 - ratio);
            quads.push((track_x, gy, track_w, 1.0, [0.25, 0.25, 0.28, 0.5]));
        }
        for ratio in [0.25, 0.5, 0.75] {
            let gx = track_x + track_w * ratio;
            quads.push((gx, self.base.y + 10.0, 1.0, gh, [0.25, 0.25, 0.28, 0.5]));
        }
        
        // Curve area fill and outline
        let slices = 600;
        let slice_w = track_w / slices as f32;
        for i in 0..slices {
            let t1 = i as f32 / slices as f32;
            let v1 = self.get_interpolated_value(t1);
            let sx1 = track_x + t1 * track_w;
            
            let slice_h = v1 * gh;
            let sy = self.base.y + 10.0 + gh - slice_h;
            quads.push((sx1, sy, slice_w, slice_h, [0.3, 0.45, 0.6, 0.25]));
            
            let outline_h = 2.0;
            let outline_y = self.base.y + 10.0 + gh - v1 * gh - 1.0;
            quads.push((sx1, outline_y, slice_w, outline_h, [0.5, 0.75, 1.0, 1.0]));
        }
        
        let ctx_dummy = crate::context::UiContext::new();
        quads.extend(self.preset_dropdown.all_quads(&ctx_dummy));
        quads.extend(self.line_type_dropdown.all_quads(&ctx_dummy));
        
        if self.selected_key_idx.is_some() {
            quads.extend(self.val_slider.all_quads(&ctx_dummy));
            quads.extend(self.del_button.all_quads(&ctx_dummy));
        }
        
        quads
    }
    
    fn extra_circles(&self) -> Vec<(f32, f32, f32, [f32; 4])> {
        let mut circles = Vec::new();
        let gh = 80.0;
        let track_x = self.base.x + 10.0;
        let track_w = self.base.w - 20.0;
        
        for (idx, key) in self.keys.iter().enumerate() {
            let cx = track_x + key.pos * track_w;
            let cy = self.base.y + 10.0 + gh - key.value * gh;
            
            circles.push((cx, cy, 7.0, [0.0, 0.0, 0.0, 0.8]));
            circles.push((cx, cy, 5.0, [0.5, 0.75, 1.0, 1.0]));
            if Some(idx) == self.selected_key_idx {
                circles.push((cx, cy, 9.0, [0.49, 1.0, 1.0, 0.5]));
            }
        }
        
        circles
    }
    
    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py_event: f32, ctx: &mut UiContext) -> bool {
        if button != MouseButton::Left { return false; }
        
        if self.preset_dropdown.mouse_input(button, state, px, py_event, ctx) {
            if self.preset_dropdown.take_change() {
                let idx = self.preset_dropdown.selected;
                self.apply_preset(idx);
            }
            return true;
        }
        
        if self.line_type_dropdown.mouse_input(button, state, px, py_event, ctx) {
            return true;
        }
        
        let gh = 80.0;
        let track_x = self.base.x + 10.0;
        let track_w = self.base.w - 20.0;
        
        if state == ElementState::Pressed {
            for (idx, key) in self.keys.iter().enumerate() {
                let cx = track_x + key.pos * track_w;
                let cy = self.base.y + 10.0 + gh - key.value * gh;
                let dx = px - cx;
                let dy = py_event - cy;
                if (dx*dx + dy*dy) <= 64.0 {
                    self.selected_key_idx = Some(idx);
                    self.is_dragging_key = true;
                    self.val_slider.set_value(key.value);
                    self.set_rect(self.base.x, self.base.y, self.base.w, self.base.h);
                    return true;
                }
            }
            
            if px >= track_x && px <= track_x + track_w && py_event >= self.base.y + 10.0 && py_event <= self.base.y + 10.0 + gh {
                let t = (px - track_x) / track_w;
                let val = 1.0 - (py_event - (self.base.y + 10.0)) / gh;
                let new_key = RampKey { pos: t, value: val };
                self.keys.push(new_key);
                self.sort_keys();
                self.preset_dropdown.selected = 0; // Custom
                self.just_changed = true;
                
                if let Some(new_idx) = self.keys.iter().position(|k| (k.pos - t).abs() < 0.0001) {
                    self.selected_key_idx = Some(new_idx);
                    self.val_slider.set_value(val);
                }
                self.set_rect(self.base.x, self.base.y, self.base.w, self.base.h);
                return true;
            }
            
            if self.selected_key_idx.is_some() {
                if self.val_slider.mouse_input(button, state, px, py_event, ctx) { return true; }
                if self.del_button.mouse_input(button, state, px, py_event, ctx) {
                    if self.del_button.take_click() {
                        if let Some(idx) = self.selected_key_idx {
                            if self.keys.len() > 2 {
                                self.keys.remove(idx);
                                self.selected_key_idx = None;
                                self.preset_dropdown.selected = 0; // Custom
                                self.just_changed = true;
                                self.set_rect(self.base.x, self.base.y, self.base.w, self.base.h);
                            }
                        }
                    }
                    return true;
                }
            }
        } else {
            self.is_dragging_key = false;
            if self.selected_key_idx.is_some() {
                self.val_slider.mouse_input(button, state, px, py_event, ctx);
                if self.del_button.mouse_input(button, state, px, py_event, ctx) {
                    if self.del_button.take_click() {
                        if let Some(idx) = self.selected_key_idx {
                            if self.keys.len() > 2 {
                                self.keys.remove(idx);
                                self.selected_key_idx = None;
                                self.preset_dropdown.selected = 0; // Custom
                                self.just_changed = true;
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
        if self.preset_dropdown.cursor_moved(px, py_event, ctx) {
            return true;
        }
        if self.line_type_dropdown.cursor_moved(px, py_event, ctx) {
            return true;
        }
        
        let mut changed = false;
        let gh = 80.0;
        let track_x = self.base.x + 10.0;
        let track_w = self.base.w - 20.0;
        
        if self.is_dragging_key {
            if let Some(idx) = self.selected_key_idx {
                let t = ((px - track_x) / track_w).clamp(0.0, 1.0);
                let val = (1.0 - (py_event - (self.base.y + 10.0)) / gh).clamp(0.0, 1.0);
                self.keys[idx].pos = t;
                self.keys[idx].value = val;
                self.val_slider.set_value(val);
                self.sort_keys();
                self.preset_dropdown.selected = 0; // Custom
                changed = true;
            }
        }
        
        if self.selected_key_idx.is_some() {
            if self.val_slider.cursor_moved(px, py_event, ctx) {
                if let Some(idx) = self.selected_key_idx {
                    self.keys[idx].value = self.val_slider.value();
                    self.preset_dropdown.selected = 0; // Custom
                    changed = true;
                }
            }
            if self.del_button.cursor_moved(px, py_event, ctx) {
                changed = true;
            }
        }
        if changed {
            self.just_changed = true;
        }
        changed
    }
    
    fn popover_rect(&self) -> Option<(f32, f32, f32, f32)> {
        self.preset_dropdown.popover_rect()
            .or_else(|| self.line_type_dropdown.popover_rect())
    }
    
    fn render_popover(&self, pc: &mut dyn crate::layout::RenderTarget) {
        self.preset_dropdown.render_popover(pc);
        self.line_type_dropdown.render_popover(pc);
    }
    
    fn text_labels_with_font_and_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        let mut labels = Vec::new();
        labels.extend(self.preset_dropdown.text_labels_with_font_and_bounds(ctx));
        labels.extend(self.line_type_dropdown.text_labels_with_font_and_bounds(ctx));
        if self.selected_key_idx.is_some() {
            labels.extend(self.val_slider.text_labels_with_font_and_bounds(ctx));
            labels.extend(self.del_button.text_labels_with_font_and_bounds(ctx));
        }
        labels
    }
    
    fn get_text_items(&self) -> Vec<(&glyphon::Buffer, f32, f32, glyphon::Color)> {
        let mut items = Vec::new();
        items.extend(self.preset_dropdown.get_text_items());
        items.extend(self.line_type_dropdown.get_text_items());
        if self.selected_key_idx.is_some() {
            items.extend(self.val_slider.get_text_items());
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
