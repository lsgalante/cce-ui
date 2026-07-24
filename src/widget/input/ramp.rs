use crate::colors;
use crate::scene::layout::{Rect, Size};
use crate::scene::paint::{Cap, PaintCtx};
use crate::widget::model::{EventCtx, Input, Layout, Paint};
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
    pub r_slider: Adapted<Slider>,
    pub g_slider: Adapted<Slider>,
    pub b_slider: Adapted<Slider>,
    pub del_button: Adapted<Button>,
    
}

impl ColorRamp {
    pub fn new() -> Adapted<ColorRamp> {
        let keys = vec![
            ColorRampKey { pos: 0.0, color: [0.0, 0.0, 0.0] },
            ColorRampKey { pos: 1.0, color: [1.0, 1.0, 1.0] },
        ];
        
        let r_slider = Slider::new().with_label("Red");
        let g_slider = Slider::new().with_label("Green");
        let b_slider = Slider::new().with_label("Blue");
        let del_button = Button::new(0.0, 0.0, 70.0, 28.0).with_label("Delete Key");
        
        Adapted::new(ColorRamp {
            base: Widget::new(),
            keys,
            selected_key_idx: None,
            is_dragging_key: false,
            just_changed: false,
            r_slider,
            g_slider,
            b_slider,
            del_button,
        })
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
    pub val_slider: Adapted<Slider>,
    pub del_button: Adapted<Button>,
    pub preset_dropdown: Adapted<Dropdown>,
    pub line_type_dropdown: Adapted<Dropdown>,
    
}

impl Ramp {
    pub fn new() -> Adapted<Ramp> {
        let keys = vec![
            RampKey { pos: 0.0, value: 0.5 },
            RampKey { pos: 0.2, value: 1.0 },
            RampKey { pos: 0.8, value: 1.0 },
            RampKey { pos: 1.0, value: 0.5 },
        ];
        
        // Labeled like the dropdowns: the slider draws "Value" in its own
        // carve-out tab.
        let val_slider = Slider::new().with_label("Value");
        // A square x-icon button (cce-icons); label fallback if the icon set
        // is missing on this machine.
        let del_button = match crate::upload_icon("x", 32) {
            Some((id, w, h)) => {
                Button::new(0.0, 0.0, 22.0, 22.0).with_icon(id, w as f32, h as f32)
            }
            None => Button::new(0.0, 0.0, 64.0, 22.0).with_label("Delete"),
        };
        // Short names on purpose: the strip's columns are narrow, and these
        // render inside param rows too ("Bevel (Raised)" used to clip).
        // Labeled: the dropdowns draw their own detached labels, sitting on
        // the expanded top wall of their inset (the labeled-relief style).
        let preset_dropdown = Dropdown::new(
            vec![
                "Custom".to_string(),
                "Linear".to_string(),
                "Raised".to_string(),
                "Sunken".to_string(),
                "Peak".to_string(),
                "Valley".to_string(),
            ],
            2,
        ).with_open_upward(true).with_label("Preset");
        let line_type_dropdown = Dropdown::new(
            vec![
                "Linear".to_string(),
                "Bezier".to_string(),
            ],
            0,
        ).with_open_upward(true).with_label("Line");
        
        Adapted::new(Ramp {
            base: Widget::new(),
            keys,
            selected_key_idx: None,
            is_dragging_key: false,
            just_changed: false,
            val_slider,
            del_button,
            preset_dropdown,
            line_type_dropdown,
        })
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

    /// Whether segments blend with smoothstep (the Bezier line type) vs linearly.
    pub fn smooth(&self) -> bool {
        self.line_type_dropdown.selected == 1
    }

    /// This ramp's state as the DE's ramp spec string ([`format_ramp_spec`]).
    pub fn spec_string(&self) -> String {
        let keys: Vec<(f32, f32)> = self.keys.iter().map(|k| (k.pos, k.value)).collect();
        format_ramp_spec(&keys, self.smooth())
    }

    /// Apply a spec string ([`parse_ramp_spec`]); returns whether anything changed.
    /// Unparsable specs are ignored (keeps the current curve).
    pub fn set_spec(&mut self, spec: &str) -> bool {
        let Some((keys, smooth)) = parse_ramp_spec(spec) else {
            return false;
        };
        let new_keys: Vec<RampKey> =
            keys.into_iter().map(|(pos, value)| RampKey { pos, value }).collect();
        let new_line = if smooth { 1 } else { 0 };
        let changed = self.line_type_dropdown.selected != new_line
            || self.keys.len() != new_keys.len()
            || self
                .keys
                .iter()
                .zip(new_keys.iter())
                .any(|(a, b)| (a.pos - b.pos).abs() > 0.0005 || (a.value - b.value).abs() > 0.0005);
        if changed {
            self.keys = new_keys;
            self.line_type_dropdown.selected = new_line;
            self.selected_key_idx = None;
            self.preset_dropdown.selected = 0; // Custom
            self.arrange_fields();
        }
        changed
    }
}

/// Serialize ramp keys + line type as the DE's ramp spec string:
/// `"smooth;0.000:0.500,0.200:1.000,…"` (`"linear;…"` for straight segments) —
/// the format ramp-valued params travel in (`ParametersBg` "ramp" rows,
/// project files, `cce_ui::layout::set_bevel_profile_keys` consumers).
pub fn format_ramp_spec(keys: &[(f32, f32)], smooth: bool) -> String {
    let body: Vec<String> =
        keys.iter().map(|(p, v)| format!("{:.3}:{:.3}", p, v)).collect();
    format!("{};{}", if smooth { "smooth" } else { "linear" }, body.join(","))
}

/// Parse a ramp spec string ([`format_ramp_spec`]) into `(keys, smooth)`.
/// `None` for anything that doesn't yield at least two keys.
pub fn parse_ramp_spec(spec: &str) -> Option<(Vec<(f32, f32)>, bool)> {
    let (head, body) = spec.split_once(';')?;
    let smooth = head.trim() == "smooth";
    let mut keys = Vec::new();
    for part in body.split(',') {
        let (p, v) = part.split_once(':')?;
        keys.push((
            p.trim().parse::<f32>().ok()?.clamp(0.0, 1.0),
            v.trim().parse::<f32>().ok()?.clamp(0.0, 1.0),
        ));
    }
    if keys.len() < 2 {
        return None;
    }
    keys.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    Some((keys, smooth))
}



impl ColorRamp {
    fn arrange_fields(&mut self) {
        let (x, y, w, h) = (self.base.x, self.base.y, self.base.w, self.base.h);
        self.base.x = x;
        self.base.y = y;
        self.base.w = w;
        self.base.h = h;
        
        
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
}

impl Layout for ColorRamp {
    fn rect_assigned(&mut self, rect: Rect) {
        let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);
        self.base.x = x;
        self.base.y = y;
        self.base.w = w;
        self.base.h = h;
        
        
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

}

impl Paint for ColorRamp {
    fn color(&self) -> [f32; 4] {
        colors::ramp_background_color()
    }

    // Field children are ctx-linked for event propagation but painted here (gated on a
    // key being selected) — the walk must not also descend.
    fn paints_own_subtree(&self) -> bool {
        true
    }

    fn paint(&self, _rect: Rect, pc: &mut PaintCtx) {
        let quads: Vec<(f32, f32, f32, f32, [f32; 4])> = {
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
    
        };
        for (qx, qy, qw, qh, qc) in quads {
            pc.quad(Rect { x: qx, y: qy, width: qw, height: qh }, qc);
        }
        let circles: Vec<(f32, f32, f32, [f32; 4])> = {
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
    
        };
        for (cx, cy, r, c) in circles {
            pc.circle(cx, cy, r, c);
        }
        if self.selected_key_idx.is_some() {
            let dummy = UiContext::new();
            self.r_slider.paint_self(&dummy, pc);
            self.g_slider.paint_self(&dummy, pc);
            self.b_slider.paint_self(&dummy, pc);
            self.del_button.paint_self(&dummy, pc);
        }
    }
}

impl Input for ColorRamp {
    fn wants_tick(&self) -> bool {
        true
    }

    fn tick_ctx(&mut self, dt: f32, ectx: &mut EventCtx) -> bool {
        // (The per-tick field-widget re-parenting is gone, 6bd: it was a dummy-ctx
        // `set_parent` whose every effect was discarded — legacy behaved the same.)
        let Some(ui) = ectx.ui.as_deref_mut() else {
            return false;
        };
        let mut changed = self.just_changed;
        self.just_changed = false;
        
        if self.selected_key_idx.is_some() {
            if self.r_slider.tick(dt, ui) {
                if let Some(idx) = self.selected_key_idx {
                    self.keys[idx].color[0] = self.r_slider.inner().value();
                }
                changed = true;
            }
            if self.g_slider.tick(dt, ui) {
                if let Some(idx) = self.selected_key_idx {
                    self.keys[idx].color[1] = self.g_slider.inner().value();
                }
                changed = true;
            }
            if self.b_slider.tick(dt, ui) {
                if let Some(idx) = self.selected_key_idx {
                    self.keys[idx].color[2] = self.b_slider.inner().value();
                }
                changed = true;
            }
            if self.del_button.tick(dt, ui) {
                changed = true;
            }
        }
        changed
    
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button, state, x, y, .. } => {
                let (button, state, px, py_event) = (*button, *state, *x, *y);
                let Some(ui) = ectx.ui.as_deref_mut() else { return false; };
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
                    self.arrange_fields();
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
                self.arrange_fields();
                return true;
            }
            
            if self.selected_key_idx.is_some() {
                if self.r_slider.mouse_input(button, state, px, py_event, ui) { return true; }
                if self.g_slider.mouse_input(button, state, px, py_event, ui) { return true; }
                if self.b_slider.mouse_input(button, state, px, py_event, ui) { return true; }
                if self.del_button.mouse_input(button, state, px, py_event, ui) {
                    if self.del_button.take_click() {
                        if let Some(idx) = self.selected_key_idx {
                            if self.keys.len() > 2 {
                                self.keys.remove(idx);
                                self.selected_key_idx = None;
                                self.just_changed = true;
                                self.arrange_fields();
                            }
                        }
                    }
                    return true;
                }
            }
        } else {
            self.is_dragging_key = false;
            if self.selected_key_idx.is_some() {
                self.r_slider.mouse_input(button, state, px, py_event, ui);
                self.g_slider.mouse_input(button, state, px, py_event, ui);
                self.b_slider.mouse_input(button, state, px, py_event, ui);
                if self.del_button.mouse_input(button, state, px, py_event, ui) {
                    if self.del_button.take_click() {
                        if let Some(idx) = self.selected_key_idx {
                            if self.keys.len() > 2 {
                                self.keys.remove(idx);
                                self.selected_key_idx = None;
                                self.just_changed = true;
                                self.arrange_fields();
                            }
                        }
                    }
                }
                return true;
            }
        }
        false
    
            }
            Event::PointerMove { x, y, .. } => {
                let (px, py_event) = (*x, *y);
                let Some(ui) = ectx.ui.as_deref_mut() else { return false; };
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
            if self.r_slider.cursor_moved(px, py_event, ui) {
                if let Some(idx) = self.selected_key_idx {
                    self.keys[idx].color[0] = self.r_slider.inner().value();
                    changed = true;
                }
            }
            if self.g_slider.cursor_moved(px, py_event, ui) {
                if let Some(idx) = self.selected_key_idx {
                    self.keys[idx].color[1] = self.g_slider.inner().value();
                    changed = true;
                }
            }
            if self.b_slider.cursor_moved(px, py_event, ui) {
                if let Some(idx) = self.selected_key_idx {
                    self.keys[idx].color[2] = self.b_slider.inner().value();
                    changed = true;
                }
            }
            if self.del_button.cursor_moved(px, py_event, ui) {
                changed = true;
            }
        }
        if changed {
            self.just_changed = true;
        }
        changed
    
            }
            Event::MouseWheel { delta, x, y, .. } => {
                // Wheel forwarding (6bd self-routing): with the field widgets no longer
                // tree-linked, the sliders' wheel rides this arm — and the key color syncs
                // immediately (the old descent path left it stale until the next hover flip).
                let (delta, px, py) = (delta.clone(), *x, *y);
                let Some(ui) = ectx.ui.as_deref_mut() else { return false; };
                if self.selected_key_idx.is_none() {
                    return false;
                }
                let mut changed = false;
                if self.r_slider.mouse_wheel(&delta, px, py, ui) {
                    if let Some(idx) = self.selected_key_idx {
                        self.keys[idx].color[0] = self.r_slider.inner().value();
                    }
                    changed = true;
                }
                if self.g_slider.mouse_wheel(&delta, px, py, ui) {
                    if let Some(idx) = self.selected_key_idx {
                        self.keys[idx].color[1] = self.g_slider.inner().value();
                    }
                    changed = true;
                }
                if self.b_slider.mouse_wheel(&delta, px, py, ui) {
                    if let Some(idx) = self.selected_key_idx {
                        self.keys[idx].color[2] = self.b_slider.inner().value();
                    }
                    changed = true;
                }
                if changed {
                    self.just_changed = true;
                }
                changed
            }
            Event::KeyInput(event) => {
                let Some(ui) = ectx.ui.as_deref_mut() else { return false; };
        if ui.is_focused(&self.r_slider) {
            return self.r_slider.keyboard_input(event, ui);
        }
        if ui.is_focused(&self.g_slider) {
            return self.g_slider.keyboard_input(event, ui);
        }
        if ui.is_focused(&self.b_slider) {
            return self.b_slider.keyboard_input(event, ui);
        }
        if ui.is_focused(&self.del_button) {
            return self.del_button.keyboard_input(event, ui);
        }
        false
    
            }
            _ => false,
        }
    }

    // Field-slider drags forward through the composite (6bd self-routing): the router
    // records THIS widget as the drag target once a press is handled here, so the hooks
    // hand DragUpdate to whichever slider armed itself — and sync the key color, which
    // the old descent path never did mid-drag.
    fn draggable(&self, _rect: Rect) -> bool {
        self.is_dragging_key
            || self.r_slider.is_dragging()
            || self.g_slider.is_dragging()
            || self.b_slider.is_dragging()
    }
    fn is_dragging(&self) -> bool {
        self.is_dragging_key
            || self.r_slider.is_dragging()
            || self.g_slider.is_dragging()
            || self.b_slider.is_dragging()
    }
    fn drag_update(&mut self, px: f32, py: f32, _rect: Rect) -> bool {
        let mut changed = false;
        if self.r_slider.is_dragging() && self.r_slider.drag_update(px, py) {
            if let Some(idx) = self.selected_key_idx {
                self.keys[idx].color[0] = self.r_slider.inner().value();
            }
            changed = true;
        }
        if self.g_slider.is_dragging() && self.g_slider.drag_update(px, py) {
            if let Some(idx) = self.selected_key_idx {
                self.keys[idx].color[1] = self.g_slider.inner().value();
            }
            changed = true;
        }
        if self.b_slider.is_dragging() && self.b_slider.drag_update(px, py) {
            if let Some(idx) = self.selected_key_idx {
                self.keys[idx].color[2] = self.b_slider.inner().value();
            }
            changed = true;
        }
        if changed {
            self.just_changed = true;
        }
        changed
    }
    fn drag_end(&mut self) {
        self.r_slider.drag_end();
        self.g_slider.drag_end();
        self.b_slider.drag_end();
        self.is_dragging_key = false;
    }
}

impl Ramp {
    /// Vertical reserve under the curve area for the control strip (gap, label
    /// tabs, controls, bottom margin) — the graph gets the rest. Sized so the
    /// graph opening's rim shading stays clear of the label tabs' carves.
    const STRIP_RESERVE: f32 = 82.0;

    /// The curve area's height for a widget `h` tall.
    fn graph_h(h: f32) -> f32 {
        (h - Self::STRIP_RESERVE).max(30.0)
    }

    /// The detached-label strip height the labeled dropdowns carry
    /// (`Widget::label_offset`'s formula).
    fn label_strip() -> f32 {
        if crate::layout::control_label_layout() == "side" {
            return 0.0;
        }
        let (_, font_size) = crate::layout::control_label_font_detached_parsed();
        font_size + crate::layout::control_label_margin()
    }

    /// Lay out the control strip under the curve area. One rhythm: the label
    /// line sits 8px under the graph, the controls 4px under the labels, all
    /// columns one shared height on one shared baseline. The labeled dropdowns
    /// get rects that INCLUDE their label strip (the adapter carves it off the
    /// content); the unlabeled columns get the content band only. The preset
    /// column takes the wider share — its options are the strip's longest
    /// strings and used to clip.
    fn arrange_fields(&mut self) {
        let (x, y, w, h) = (self.base.x, self.base.y, self.base.w, self.base.h);
        let gh = Self::graph_h(h);
        let graph_bottom = y + 10.0 + gh;
        let ctrl_y = graph_bottom + 40.0;
        let ctrl_h = 22.0;
        let strip = Self::label_strip();
        let (dd_y, dd_h) = (ctrl_y - strip, ctrl_h + strip);
        let track_x = x + 10.0;
        let track_w = w - 20.0;
        let gap = 10.0;

        if self.selected_key_idx.is_some() {
            // Four columns: preset, line type, value, and the delete button —
            // a square x-icon tile (label fallback runs wider).
            let del_w: f32 = if self.del_button.inner().has_icon() { ctrl_h } else { 64.0 };
            let avail = (track_w - del_w - 3.0 * gap).max(120.0);
            let pre_w = (avail * 0.40).max(40.0);
            let line_w = (avail * 0.32).max(40.0);
            let val_w = (avail - pre_w - line_w).max(40.0);
            self.preset_dropdown.set_rect(track_x, dd_y, pre_w, dd_h);
            self.line_type_dropdown.set_rect(track_x + pre_w + gap, dd_y, line_w, dd_h);
            self.val_slider.set_rect(track_x + pre_w + line_w + 2.0 * gap, dd_y, val_w, dd_h);
            self.del_button.set_rect(track_x + track_w - del_w, ctrl_y, del_w, ctrl_h);
        } else {
            // Two columns, preset the wider share.
            let pre_w = ((track_w - gap) * 0.58).max(40.0);
            let line_w = (track_w - gap - pre_w).max(40.0);
            self.preset_dropdown.set_rect(track_x, dd_y, pre_w, dd_h);
            self.line_type_dropdown.set_rect(track_x + pre_w + gap, dd_y, line_w, dd_h);
            self.val_slider.set_rect(-1000.0, -1000.0, 0.0, 0.0);
            self.del_button.set_rect(-1000.0, -1000.0, 0.0, 0.0);
        }
    }
}

impl Layout for Ramp {
    fn intrinsic_size(&self) -> Option<Size> {
        Some(Size::new(0.0, 150.0))
    }

    fn rect_assigned(&mut self, rect: Rect) {
        self.base.x = rect.x;
        self.base.y = rect.y;
        self.base.w = rect.width;
        self.base.h = rect.height;
        self.arrange_fields();
    }

    // register_embedded_children: gone entirely (6bd self-routing): the fields need no
    // eager registry presence — focus setters self-register on demand (6bc), the composite
    // itself covers the spatial grid, and an eagerly-registered child DROPDOWN's open
    // popover made `is_coordinate_covered` occlude the composite's own hit gate (the
    // exclusion is exact-id only), which is why preset-item clicks never landed.
}

impl Paint for Ramp {
    fn color(&self) -> [f32; 4] {
        [0.15, 0.15, 0.18, 1.0]
    }

    fn popover(&self, _rect: Rect) -> Option<(f32, f32, f32, f32)> {
        self.preset_dropdown.popover_rect()
            .or_else(|| self.line_type_dropdown.popover_rect())
    
    }

    fn draw_popover(&self, _rect: Rect, pc: &mut dyn crate::layout::RenderTarget) {
        self.preset_dropdown.render_popover(pc);
        self.line_type_dropdown.render_popover(pc);
    
    }

    // Field children are ctx-linked for event propagation but painted here — the walk
    // must not also descend (the legacy own-labels rule, now with the children too).
    fn paints_own_subtree(&self) -> bool {
        true
    }

    fn paint(&self, _rect: Rect, pc: &mut PaintCtx) {
        // No container box: the controls sit directly on the host's plate, and
        // the graph area reads as an OPENING cut through it — a dark floor
        // behind the plate, with the recess wall (drawn after the content, so
        // its shading falls across the graph's edges) as the cut's bevel.
        let graph = {
            let gh = Self::graph_h(self.base.h);
            Rect { x: self.base.x + 10.0, y: self.base.y + 10.0, width: self.base.w - 20.0, height: gh }
        };
        let graph_radius = 6.0f32;
        pc.rounded_rect(
            graph,
            graph_radius,
            (true, true, true, true),
            [0.08, 0.08, 0.10, 1.0],
        );

        let quads: Vec<(f32, f32, f32, f32, [f32; 4])> = {
        let mut quads = Vec::new();
        let gh = Self::graph_h(self.base.h);
        let track_x = self.base.x + 10.0;
        let track_w = self.base.w - 20.0;
        
        // Draw grid lines
        for ratio in [0.25, 0.5, 0.75] {
            let gy = self.base.y + 10.0 + gh * (1.0 - ratio);
            quads.push((track_x, gy, track_w, 1.0, [0.25, 0.25, 0.28, 0.5]));
        }
        for ratio in [0.25, 0.5, 0.75] {
            let gx = track_x + track_w * ratio;
            quads.push((gx, self.base.y + 10.0, 1.0, gh, [0.25, 0.25, 0.28, 0.5]));
        }
        
        // Curve area fill: translucent columns under the curve. The outline is
        // a real vector polyline below — these only tint the area. Columns
        // share exact edges (overlap double-blends a translucent fill into
        // visible banding; found the hard way).
        let slices = 200;
        for i in 0..slices {
            let t1 = i as f32 / slices as f32;
            let x0 = track_x + t1 * track_w;
            let x1 = track_x + (i + 1) as f32 / slices as f32 * track_w;
            let v1 = self.get_interpolated_value(t1);

            let slice_h = v1 * gh;
            let sy = self.base.y + 10.0 + gh - slice_h;
            // Faint on purpose: the graph reads as a dark opening behind the
            // plate — a strong fill floods the floor and flattens the depth.
            quads.push((x0, sy, x1 - x0, slice_h, [0.25, 0.40, 0.55, 0.10]));
        }

        quads

        };
        for (qx, qy, qw, qh, qc) in quads {
            pc.quad(Rect { x: qx, y: qy, width: qw, height: qh }, qc);
        }

        // Axis numbers on the gridlines — small, dim, part of the graph
        // floor (under the curve and keys, inside the opening).
        let num_color = [0x84u8, 0x84, 0x92];
        for ratio in [0.25f32, 0.5, 0.75] {
            let gy = graph.y + graph.height * (1.0 - ratio);
            pc.text_with(
                format!("{ratio:.2}"),
                graph.x + 5.0,
                gy - 11.0,
                9.0,
                num_color,
                Some("monospace".to_string()),
                None,
            );
            let gx = graph.x + graph.width * ratio;
            pc.text_with(
                format!("{ratio:.2}"),
                gx - 11.0,
                graph.y + graph.height - 13.0,
                9.0,
                num_color,
                Some("monospace".to_string()),
                None,
            );
        }

        // The curve itself: one anti-aliased round-capped polyline — exact
        // key-to-key segments in linear mode, dense samples under smoothstep
        // blending. Constant-value extensions reach the graph's side walls.
        let gh = Self::graph_h(self.base.h);
        let track_x = self.base.x + 10.0;
        let track_w = self.base.w - 20.0;
        let curve_color = [0.5, 0.75, 1.0, 1.0];
        let px_of = |t: f32, v: f32| {
            (track_x + t * track_w, self.base.y + 10.0 + gh - v * gh)
        };
        let mut pts: Vec<(f32, f32)> = Vec::new();
        if self.line_type_dropdown.selected == 1 {
            let n = 64;
            for i in 0..=n {
                let t = i as f32 / n as f32;
                pts.push(px_of(t, self.get_interpolated_value(t)));
            }
        } else {
            if let Some(first) = self.keys.first() {
                if first.pos > 0.0 {
                    pts.push(px_of(0.0, first.value));
                }
            }
            for k in &self.keys {
                pts.push(px_of(k.pos, k.value));
            }
            if let Some(last) = self.keys.last() {
                if last.pos < 1.0 {
                    pts.push(px_of(1.0, last.value));
                }
            }
        }
        for pair in pts.windows(2) {
            pc.vector(pair[0].0, pair[0].1, pair[1].0, pair[1].1, 2.0, curve_color, Cap::Round);
        }
        let circles: Vec<(f32, f32, f32, [f32; 4])> = {
        let mut circles = Vec::new();
        let gh = Self::graph_h(self.base.h);
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
    
        };
        for (cx, cy, r, c) in circles {
            pc.circle(cx, cy, r, c);
        }
        // The opening's cut edge: drawn after the graph content so the wall's
        // shading falls across the curve and keys where they pass behind the
        // plate's rim. Nested translucent border rings first — the contact
        // shadow the plate casts down into the opening — then the recess wall
        // itself as the cut's bevel.
        let radii = (graph_radius, graph_radius, graph_radius, graph_radius);
        for (t, a) in [(7.0, 0.08), (4.0, 0.10), (2.0, 0.14)] {
            pc.border(graph, radii, [0.0; 4], [0.0, 0.0, 0.0, a], t);
        }
        let depth = crate::layout::bevel_width().min(graph.height * 0.2);
        pc.recess(graph, radii, depth);
        let dummy = UiContext::new();
        self.preset_dropdown.paint_self(&dummy, pc);
        self.line_type_dropdown.paint_self(&dummy, pc);
        if self.selected_key_idx.is_some() {
            self.val_slider.paint_self(&dummy, pc);
            self.del_button.paint_self(&dummy, pc);
        }
    }
}

impl Input for Ramp {
    fn wants_tick(&self) -> bool {
        true
    }

    /// The curve as a ramp spec string ([`format_ramp_spec`]) — the value hosts
    /// poll and persist for ramp-valued params.
    fn value_string(&self) -> Option<String> {
        Some(self.spec_string())
    }

    fn set_value_string(&mut self, val: &str) -> bool {
        self.set_spec(val)
    }

    /// The open dropdown popover extends the hit area (the 5p Dropdown pattern).
    fn hit(&self, rect: Rect, x: f32, y: f32) -> bool {
        if let Some((px, py, pw, ph)) = {
        self.preset_dropdown.popover_rect()
            .or_else(|| self.line_type_dropdown.popover_rect())
    
        } {
            if x >= px && x <= px + pw && y >= py && y <= py + ph {
                return true;
            }
        }
        x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height
    }

    fn tick_ctx(&mut self, dt: f32, ectx: &mut EventCtx) -> bool {
        // (The per-tick field-widget re-parenting is gone, 6bd: it was a dummy-ctx
        // `set_parent` whose every effect was discarded — legacy behaved the same.)
        let Some(ui) = ectx.ui.as_deref_mut() else {
            return false;
        };
        let mut changed = self.just_changed;
        self.just_changed = false;
        
        if self.preset_dropdown.tick(dt, ui) {
            let idx = self.preset_dropdown.selected;
            self.apply_preset(idx);
            changed = true;
        }
        
        if self.line_type_dropdown.tick(dt, ui) {
            changed = true;
        }
        
        if self.selected_key_idx.is_some() {
            if self.val_slider.tick(dt, ui) {
                if let Some(idx) = self.selected_key_idx {
                    self.keys[idx].value = self.val_slider.inner().value();
                    self.preset_dropdown.selected = 0; // Custom
                }
                changed = true;
            }
            if self.del_button.tick(dt, ui) {
                self.preset_dropdown.selected = 0; // Custom
                changed = true;
            }
        }
        changed
    
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button, state, x, y, .. } => {
                let (button, state, px, py_event) = (*button, *state, *x, *y);
                let Some(ui) = ectx.ui.as_deref_mut() else { return false; };
        if button != MouseButton::Left { return false; }
        
        if self.preset_dropdown.mouse_input(button, state, px, py_event, ui) {
            if self.preset_dropdown.take_change() {
                let idx = self.preset_dropdown.selected;
                self.apply_preset(idx);
            }
            return true;
        }
        
        if self.line_type_dropdown.mouse_input(button, state, px, py_event, ui) {
            return true;
        }
        
        let gh = Self::graph_h(self.base.h);
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
                    self.arrange_fields();
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
                self.arrange_fields();
                return true;
            }
            
            if self.selected_key_idx.is_some() {
                if self.val_slider.mouse_input(button, state, px, py_event, ui) { return true; }
                if self.del_button.mouse_input(button, state, px, py_event, ui) {
                    if self.del_button.take_click() {
                        if let Some(idx) = self.selected_key_idx {
                            if self.keys.len() > 2 {
                                self.keys.remove(idx);
                                self.selected_key_idx = None;
                                self.preset_dropdown.selected = 0; // Custom
                                self.just_changed = true;
                                self.arrange_fields();
                            }
                        }
                    }
                    return true;
                }
            }
        } else {
            self.is_dragging_key = false;
            if self.selected_key_idx.is_some() {
                self.val_slider.mouse_input(button, state, px, py_event, ui);
                if self.del_button.mouse_input(button, state, px, py_event, ui) {
                    if self.del_button.take_click() {
                        if let Some(idx) = self.selected_key_idx {
                            if self.keys.len() > 2 {
                                self.keys.remove(idx);
                                self.selected_key_idx = None;
                                self.preset_dropdown.selected = 0; // Custom
                                self.just_changed = true;
                                self.arrange_fields();
                            }
                        }
                    }
                }
                return true;
            }
        }
        false
    
            }
            Event::PointerMove { x, y, .. } => {
                let (px, py_event) = (*x, *y);
                let Some(ui) = ectx.ui.as_deref_mut() else { return false; };
        if self.preset_dropdown.cursor_moved(px, py_event, ui) {
            return true;
        }
        if self.line_type_dropdown.cursor_moved(px, py_event, ui) {
            return true;
        }
        
        let mut changed = false;
        let gh = Self::graph_h(self.base.h);
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
            if self.val_slider.cursor_moved(px, py_event, ui) {
                if let Some(idx) = self.selected_key_idx {
                    self.keys[idx].value = self.val_slider.inner().value();
                    self.preset_dropdown.selected = 0; // Custom
                    changed = true;
                }
            }
            if self.del_button.cursor_moved(px, py_event, ui) {
                changed = true;
            }
        }
        if changed {
            self.just_changed = true;
        }
        changed
    
            }
            Event::MouseWheel { delta, x, y, .. } => {
                // Wheel forwarding (6bd self-routing): dropdowns first (mirroring the press
                // order, incl. the preset drain), then the value slider with the key sync.
                let (delta, px, py) = (delta.clone(), *x, *y);
                let Some(ui) = ectx.ui.as_deref_mut() else { return false; };
                if self.preset_dropdown.mouse_wheel(&delta, px, py, ui) {
                    if self.preset_dropdown.take_change() {
                        let idx = self.preset_dropdown.selected;
                        self.apply_preset(idx);
                    }
                    return true;
                }
                if self.line_type_dropdown.mouse_wheel(&delta, px, py, ui) {
                    return true;
                }
                if self.selected_key_idx.is_some() && self.val_slider.mouse_wheel(&delta, px, py, ui) {
                    if let Some(idx) = self.selected_key_idx {
                        self.keys[idx].value = self.val_slider.inner().value();
                        self.preset_dropdown.selected = 0; // Custom
                    }
                    self.just_changed = true;
                    return true;
                }
                false
            }
            Event::KeyInput(event) => {
                let Some(ui) = ectx.ui.as_deref_mut() else { return false; };
        if event.state != ElementState::Pressed { return false; }
        
        if event.logical_key == Key::Named(NamedKey::Tab) {
            let is_shift = event.shift;
            let self_ptr = self as *mut Self;
            let mut children = unsafe {
                let mut list = vec![
                    (*self_ptr).preset_dropdown.as_ptr_mut(),
                    (*self_ptr).line_type_dropdown.as_ptr_mut(),
                ];
                if (*self_ptr).selected_key_idx.is_some() {
                    list.push((*self_ptr).val_slider.as_ptr_mut());
                    list.push((*self_ptr).del_button.as_ptr_mut());
                }
                list
            };
            
            let mut focused_idx = None;
            for (idx, child) in children.iter().enumerate() {
                if unsafe { ui.is_focused(&**child) } {
                    focused_idx = Some(idx);
                    break;
                }
            }
            
            if let Some(curr) = focused_idx {
                let next_idx = if is_shift {
                    if curr == 0 { children.len() - 1 } else { curr - 1 }
                } else {
                    (curr + 1) % children.len()
                };
                unsafe {
                    ui.set_focused(&mut *children[next_idx]);
                }
            } else {
                unsafe {
                    ui.set_focused(&mut *children[0]);
                }
            }
            return true;
        }
        
        if ui.is_focused(&self.preset_dropdown) {
            return self.preset_dropdown.keyboard_input(event, ui);
        }
        if ui.is_focused(&self.line_type_dropdown) {
            return self.line_type_dropdown.keyboard_input(event, ui);
        }
        if ui.is_focused(&self.val_slider) {
            return self.val_slider.keyboard_input(event, ui);
        }
        if ui.is_focused(&self.del_button) {
            return self.del_button.keyboard_input(event, ui);
        }
        false
    
            }
            Event::FocusIn => {
                if let Some(ui) = ectx.ui.as_deref_mut() {
                    ui.set_focused(&mut self.preset_dropdown);
                }
                false
            }
            Event::FocusOut => {
        self.base.focused = false;
        self.preset_dropdown.unfocus();
        self.line_type_dropdown.unfocus();
        self.val_slider.unfocus();
        self.del_button.unfocus();
    
                false
            }
            _ => false,
        }
    }

    // Field-slider drags forward through the composite (6bd self-routing), with the key
    // value sync the old descent path never ran mid-drag.
    fn draggable(&self, _rect: Rect) -> bool {
        self.is_dragging_key || self.val_slider.is_dragging()
    }
    fn is_dragging(&self) -> bool {
        self.is_dragging_key || self.val_slider.is_dragging()
    }
    fn drag_update(&mut self, px: f32, py: f32, _rect: Rect) -> bool {
        let mut changed = false;
        if self.val_slider.is_dragging() && self.val_slider.drag_update(px, py) {
            if let Some(idx) = self.selected_key_idx {
                self.keys[idx].value = self.val_slider.inner().value();
                self.preset_dropdown.selected = 0; // Custom
            }
            changed = true;
        }
        if changed {
            self.just_changed = true;
        }
        changed
    }
    fn drag_end(&mut self) {
        self.val_slider.drag_end();
        self.is_dragging_key = false;
    }
}
