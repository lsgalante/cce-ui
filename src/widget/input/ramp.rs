use crate::colors;
use crate::scene::layout::{Rect, Size};
use crate::scene::paint::{Cap, PaintCtx};
use crate::widget::model::{EventCtx, Input, Layout, Paint};
use crate::widget::*;
use crate::widget::input::{Slider, Slider2D, Button};

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
    /// The key latched by the current hover-scroll gesture: a trackpad
    /// scroll starting over a key steers that key until the fingers lift
    /// (a >250ms pause reads as a new gesture and re-latches by hover).
    scroll_key_idx: Option<usize>,
    /// Context-menu toggle: hide the bottom control strip and let the graph
    /// claim its space.
    pub controls_collapsed: bool,
    /// Hover-scroll glide velocity (plot units/sec, applied-delta signs) and
    /// the last scroll-event instant: when the event stream stops, the tick
    /// keeps the latched key coasting with exponential decay.
    scroll_vel: (f32, f32),
    last_key_scroll: Option<std::time::Instant>,

    // Child controls for key editing & deletion. The key pad is a 2-axis
    // slider driving the selected key's position (x) and value (y).
    pub key_pad: Adapted<Slider2D>,
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
        
        // The key pad: a 2-axis slider driving the selected key's position
        // (x) and value (y), labeled like the dropdowns.
        let key_pad = Slider2D::new().with_label("Key");
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
            scroll_key_idx: None,
            controls_collapsed: false,
            scroll_vel: (0.0, 0.0),
            last_key_scroll: None,
            key_pad,
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
    /// The one spacing value the whole control strip uses — matching the
    /// visible gap between the graph opening and the window's top edge (the
    /// widget's 10px graph inset plus the host plate's padding).
    const STRIP_GAP: f32 = 18.0;

    /// The key pad's square well side.
    const PAD_SIDE: f32 = 64.0;

    /// Vertical reserve under the curve area — the strip stack at the
    /// uniform STRIP_GAP rhythm (labeled dropdown row, labeled pad row),
    /// closed by a bottom margin sized so the VISIBLE bottom gap (widget
    /// margin + host plate padding, ~8) lands on STRIP_GAP as well.
    fn strip_reserve() -> f32 {
        let strip = Self::label_strip();
        10.0 + Self::STRIP_GAP + strip + 22.0
            + Self::STRIP_GAP + strip + Self::PAD_SIDE
            + 10.0
    }

    /// Key peg ring stroke centerline radius (the 2px stroke spans ±1px).
    /// Paint and the grab hit-test share it: a press anywhere inside a ring
    /// lands on that key.
    const KEY_RING_R: f32 = 26.0;

    /// Inner margin between the graph opening's walls and the plotted 0..1
    /// domain, so the 0 and 1 gridlines (and their axis numbers) sit visibly
    /// inside the opening instead of on the walls.
    const PLOT_INSET: f32 = 22.0;

    /// The plot rect: where the ramp's 0..1 × 0..1 domain maps on screen —
    /// the graph opening inset by [`PLOT_INSET`](Self::PLOT_INSET). Every
    /// t/value ↔ pixel mapping (paint and input alike) goes through this.
    fn plot_rect(&self) -> Rect {
        let gh = self.graph_h();
        Rect {
            x: self.base.x + 10.0 + Self::PLOT_INSET,
            y: self.base.y + 10.0 + Self::PLOT_INSET,
            width: (self.base.w - 20.0 - 2.0 * Self::PLOT_INSET).max(1.0),
            height: (gh - 2.0 * Self::PLOT_INSET).max(1.0),
        }
    }

    /// Neighbor resistance (drag), in track units: the soft wall starts
    /// RESIST_ZONE before a neighbor's position, and pushing the cursor
    /// RESIST_BREAK past the neighbor breaks through.
    const RESIST_ZONE: f32 = 0.10;
    const RESIST_BREAK: f32 = 0.16;

    /// Where a drag whose cursor sits at `t_raw` actually puts key `idx`:
    /// 1:1 tracking until the cursor enters a neighbor's resistance zone,
    /// then the key compresses toward the neighbor with growing resistance
    /// (slope 1 at the zone edge, flattening at the wall), and once the
    /// cursor overshoots the neighbor by RESIST_BREAK the key pops through —
    /// the crossing completes and tracking is free again.
    fn resisted_pos(&self, idx: usize, t_raw: f32) -> f32 {
        let cur = self.keys[idx].pos;
        if t_raw > cur {
            if let Some(next) = self.keys.get(idx + 1) {
                return Self::soft_wall(t_raw, next.pos, 1.0);
            }
        } else if idx > 0 {
            return Self::soft_wall(t_raw, self.keys[idx - 1].pos, -1.0);
        }
        t_raw
    }

    /// Restore sort order after `keys[i]` changed position, by adjacent
    /// swaps, and return the key's new index. Exact identity tracking —
    /// `sort_keys`' float-pos re-match misidentifies the selection when the
    /// dragged key sits within ε of the key it is passing (leftward
    /// crossings flipped the selection onto the passed key).
    fn resettle_key(&mut self, mut i: usize) -> usize {
        while i + 1 < self.keys.len() && self.keys[i].pos > self.keys[i + 1].pos {
            self.keys.swap(i, i + 1);
            i += 1;
        }
        while i > 0 && self.keys[i].pos < self.keys[i - 1].pos {
            self.keys.swap(i, i - 1);
            i -= 1;
        }
        i
    }

    /// A key's rolled edge: the disc's own surface curving away at the
    /// perimeter — NOT a separate border. Each sub-arc blends radially from
    /// the surface color at the band's inner edge (continuing the flat top
    /// seamlessly), through a half-rolled tint, to the silhouette — which
    /// leans toward the light on the lit side and falls into shadow opposite,
    /// and runs denser than the top the way a glass edge reads. `r` is the
    /// outer-edge radius; `base`/`top_alpha` are the disc's surface color.
    #[allow(clippy::too_many_arguments)]
    fn rolled_rim_arc(
        pc: &mut PaintCtx,
        cx: f32,
        cy: f32,
        r: f32,
        thickness: f32,
        start: f32,
        end: f32,
        az: f32,
        base: [f32; 3],
        top_alpha: f32,
    ) {
        let sweep = end - start;
        let steps = ((sweep.abs() / 0.18).ceil() as usize).max(1);
        let tint = |sv: f32, k: f32| -> [f32; 3] {
            [
                (base[0] + k * sv).clamp(0.0, 1.0),
                (base[1] + k * sv).clamp(0.0, 1.0),
                (base[2] + k * sv).clamp(0.0, 1.0),
            ]
        };
        for i in 0..steps {
            let a0 = start + sweep * i as f32 / steps as f32;
            let a1 = start + sweep * (i + 1) as f32 / steps as f32;
            let sv = ((a0 + a1) / 2.0 + az).cos();
            let mid = tint(sv, 0.20);
            let edge = tint(sv, 0.38);
            let mid_a = (top_alpha + 0.78) / 2.0;
            pc.arc_shaded(
                cx,
                cy,
                r,
                thickness,
                a0,
                a1,
                [base[0], base[1], base[2], top_alpha],
                [mid[0], mid[1], mid[2], mid_a],
                [edge[0], edge[1], edge[2], 0.78],
            );
        }
    }

    /// Apply the key pad's two axes to the selected key: x is the key's
    /// track position (order restored by adjacent swaps), y its value.
    fn apply_pad_to_selected(&mut self) {
        let Some(idx) = self.selected_key_idx else { return };
        self.keys[idx].pos = self.key_pad.inner().value_x();
        self.keys[idx].value = self.key_pad.inner().value_y();
        let settled = self.resettle_key(idx);
        self.selected_key_idx = Some(settled);
        self.preset_dropdown.selected = 0; // Custom
        self.just_changed = true;
    }

    /// One soft wall at `wall`, approached along direction `s` (±1). Maps the
    /// cursor's depth into the zone onto the zone's width with an ease that
    /// reaches the wall exactly at breakthrough depth — continuous at the
    /// zone edge, asymptotically stiff at the wall, then a `RESIST_BREAK`
    /// pop as the mapping hands back to 1:1 tracking.
    fn soft_wall(t_raw: f32, wall: f32, s: f32) -> f32 {
        let entry = wall - s * Self::RESIST_ZONE;
        let depth = s * (t_raw - entry);
        let full = Self::RESIST_ZONE + Self::RESIST_BREAK;
        if depth <= 0.0 || depth >= full {
            return t_raw; // outside the zone, or broken through
        }
        let k = full / Self::RESIST_ZONE;
        let g = 1.0 - (1.0 - depth / full).powf(k);
        entry + s * Self::RESIST_ZONE * g
    }

    /// The curve area's height: the widget minus the control strip — or,
    /// with the controls collapsed (context-menu toggle), minus just the
    /// top/bottom insets, the graph claiming the strip's space.
    fn graph_h(&self) -> f32 {
        if self.controls_collapsed {
            (self.base.h - 20.0).max(30.0)
        } else {
            (self.base.h - Self::strip_reserve()).max(30.0)
        }
    }

    /// The detached-label strip height the labeled dropdowns carry
    /// (`Widget::label_offset`'s formula).
    pub fn label_strip() -> f32 {
        if crate::layout::control_label_layout() == "side" {
            return 0.0;
        }
        let (_, font_size) = crate::layout::control_label_font_detached_parsed();
        font_size + crate::layout::control_label_margin()
    }

    /// Lay out the control strip under the curve area. One rhythm: the label
    /// tabs sit STRIP_GAP under the graph and every other gap shares the
    /// same rhythm, all columns one shared height on one shared baseline. The labeled dropdowns
    /// get rects that INCLUDE their label strip (the adapter carves it off the
    /// content); the unlabeled columns get the content band only. The preset
    /// column takes the wider share — its options are the strip's longest
    /// strings and used to clip.
    fn arrange_fields(&mut self) {
        let (x, y, w, h) = (self.base.x, self.base.y, self.base.w, self.base.h);
        if self.controls_collapsed {
            self.preset_dropdown.set_rect(-1000.0, -1000.0, 0.0, 0.0);
            self.line_type_dropdown.set_rect(-1000.0, -1000.0, 0.0, 0.0);
            self.key_pad.set_rect(-1000.0, -1000.0, 0.0, 0.0);
            self.del_button.set_rect(-1000.0, -1000.0, 0.0, 0.0);
            let _ = (x, y, w, h);
            return;
        }
        let gh = self.graph_h();
        let graph_bottom = y + 10.0 + gh;
        let ctrl_h = 22.0;
        let strip = Self::label_strip();
        let gap = Self::STRIP_GAP;
        // One rhythm: every gap in the strip — graph to label tab, row to
        // row, columns, pad to button — is STRIP_GAP.
        let ctrl_y = graph_bottom + gap + strip;
        let (dd_y, dd_h) = (ctrl_y - strip, ctrl_h + strip);
        let track_x = x + 10.0;
        let track_w = w - 20.0;

        if self.selected_key_idx.is_some() {
            // Selected: the dropdowns keep their full-width row, and a second
            // row below carries the square key pad (pos × value) with the
            // delete button beside it, centered on the pad's well.
            let pad_side = Self::PAD_SIDE;
            let del_w: f32 = if self.del_button.inner().has_icon() { ctrl_h } else { 64.0 };
            let pre_w = ((track_w - gap) * 0.58).max(40.0);
            let line_w = (track_w - gap - pre_w).max(40.0);
            self.preset_dropdown.set_rect(track_x, dd_y, pre_w, dd_h);
            self.line_type_dropdown.set_rect(track_x + pre_w + gap, dd_y, line_w, dd_h);
            let row2_y = ctrl_y + ctrl_h + gap;
            self.key_pad.set_rect(track_x, row2_y, pad_side, pad_side + strip);
            self.del_button.set_rect(
                track_x + pad_side + gap,
                row2_y + strip + (pad_side - ctrl_h) / 2.0,
                del_w,
                ctrl_h,
            );
        } else {
            // Two columns, preset the wider share.
            let pre_w = ((track_w - gap) * 0.58).max(40.0);
            let line_w = (track_w - gap - pre_w).max(40.0);
            self.preset_dropdown.set_rect(track_x, dd_y, pre_w, dd_h);
            self.line_type_dropdown.set_rect(track_x + pre_w + gap, dd_y, line_w, dd_h);
            self.key_pad.set_rect(-1000.0, -1000.0, 0.0, 0.0);
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
            let gh = self.graph_h();
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
        let plot = self.plot_rect();

        // Grid lines over the plotted 0..1 domain — 0 and 1 included, sitting
        // inside the opening (the plot is inset from the walls).
        for ratio in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let gy = plot.y + plot.height * (1.0 - ratio);
            quads.push((plot.x, gy, plot.width, 1.0, [0.25, 0.25, 0.28, 0.5]));
            let gx = plot.x + plot.width * ratio;
            quads.push((gx, plot.y, 1.0, plot.height, [0.25, 0.25, 0.28, 0.5]));
        }

        // Curve area fill: translucent columns under the curve. The outline is
        // a real vector polyline below — these only tint the area. Columns
        // share exact edges (overlap double-blends a translucent fill into
        // visible banding; found the hard way).
        let slices = 200;
        for i in 0..slices {
            let t1 = i as f32 / slices as f32;
            let x0 = plot.x + t1 * plot.width;
            let x1 = plot.x + (i + 1) as f32 / slices as f32 * plot.width;
            let v1 = self.get_interpolated_value(t1);

            let slice_h = v1 * plot.height;
            let sy = plot.y + plot.height - slice_h;
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
        // floor (under the curve and keys, inside the opening). They sit in
        // the wall-side gutters the plot inset leaves free.
        let plot = self.plot_rect();
        let num_color = [0x84u8, 0x84, 0x92];
        for ratio in [0.0f32, 0.25, 0.5, 0.75, 1.0] {
            let gy = plot.y + plot.height * (1.0 - ratio);
            pc.text_with(
                format!("{ratio:.2}"),
                graph.x + 5.0,
                gy - 11.0,
                9.0,
                num_color,
                Some("monospace".to_string()),
                None,
            );
            let gx = plot.x + plot.width * ratio;
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
        // blending. Constant-value extensions reach the plot's 0/1 edges.
        let curve_color = [0.5, 0.75, 1.0, 1.0];
        let px_of = |t: f32, v: f32| {
            (plot.x + t * plot.width, plot.y + plot.height * (1.0 - v))
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
        // Key pegs: glassy translucent fills (solid when selected) in thin
        // white rings. Overlapping pegs render as foam cells: each pair's
        // shared wall is the chord through the two points where the ring
        // circles cross (equal radii, so it lies on the perpendicular
        // bisector of the centers); rings are cut at the wall, the wall is
        // stroked once, and each fill keeps to its own side.
        {
            let plot = self.plot_rect();
            let ring_r = Self::KEY_RING_R; // roll-band centerline
            // The disc surface: flat top out to the roll band's inner edge,
            // then the rolled perimeter out to ring_r + 2.5.
            let base = [0.5f32, 0.75, 1.0];
            let fill_r = ring_r - 3.0;
            let rim_t = 6.0f32;
            // Bevel light: the DE light azimuth the plate shading uses.
            let az = crate::layout::light_source_position();
            let tau = std::f32::consts::TAU;

            let centers: Vec<(f32, f32)> = self
                .keys
                .iter()
                .map(|k| (plot.x + k.pos * plot.width, plot.y + plot.height * (1.0 - k.value)))
                .collect();

            // Every intersecting pair: wall midpoint M + unit normal n toward
            // the neighbor per key, and the chord endpoints once per pair.
            let mut cuts: Vec<Vec<((f32, f32), (f32, f32))>> = vec![Vec::new(); centers.len()];
            let mut walls: Vec<((f32, f32), (f32, f32), (f32, f32))> = Vec::new();
            for i in 0..centers.len() {
                for j in (i + 1)..centers.len() {
                    let (dx, dy) = (centers[j].0 - centers[i].0, centers[j].1 - centers[i].1);
                    let d = (dx * dx + dy * dy).sqrt();
                    if d < 1e-3 || d >= 2.0 * ring_r {
                        continue;
                    }
                    let n = (dx / d, dy / d);
                    let m =
                        ((centers[i].0 + centers[j].0) / 2.0, (centers[i].1 + centers[j].1) / 2.0);
                    cuts[i].push((m, n));
                    cuts[j].push((m, (-n.0, -n.1)));
                    let h = (ring_r * ring_r - (d / 2.0) * (d / 2.0)).sqrt();
                    walls.push((
                        (m.0 - h * n.1, m.1 + h * n.0),
                        (m.0 + h * n.1, m.1 - h * n.0),
                        n,
                    ));
                }
            }

            // Fills. Uncut: one disc. Cut: the cell — vertical strips bounded
            // by the wall half-planes, the round edge from the circle clip.
            for (idx, &(cx, cy)) in centers.iter().enumerate() {
                let selected = Some(idx) == self.selected_key_idx;
                let fill = [base[0], base[1], base[2], if selected { 0.85 } else { 0.22 }];
                if cuts[idx].is_empty() {
                    pc.circle(cx, cy, fill_r, fill);
                    continue;
                }
                pc.push_clip_circle([cx, cy, fill_r]);
                let step = 1.5f32;
                let mut x = cx - fill_r;
                while x < cx + fill_r {
                    let mid = x + step / 2.0;
                    let (mut ylo, mut yhi) = (cy - fill_r, cy + fill_r);
                    let mut visible = true;
                    for &((mx, my), (nx, ny)) in &cuts[idx] {
                        // Keep (p − M)·n ≤ 0 — this key's side of the wall.
                        let c = nx * (mid - mx);
                        if ny.abs() < 1e-4 {
                            if c > 0.0 {
                                visible = false;
                                break;
                            }
                        } else {
                            let yb = my - c / ny;
                            if ny > 0.0 {
                                yhi = yhi.min(yb);
                            } else {
                                ylo = ylo.max(yb);
                            }
                        }
                    }
                    if visible && ylo < yhi {
                        pc.quad(Rect { x, y: ylo, width: step, height: yhi - ylo }, fill);
                    }
                    x += step;
                }
                pc.pop_clip_circle();
            }

            // Walls: the shared boundary as the surface rolling into the
            // seam and back out — surface-tinted slopes (lit side leans to
            // the light, far side into shadow) around a slightly lifted
            // crest, in the discs\' own color like the rims.
            let (lx, ly) = (az.cos(), -az.sin());
            let wall_tint = |sv: f32, k: f32| -> [f32; 3] {
                [
                    (base[0] + k * sv).clamp(0.0, 1.0),
                    (base[1] + k * sv).clamp(0.0, 1.0),
                    (base[2] + k * sv).clamp(0.0, 1.0),
                ]
            };
            for &((x1, y1), (x2, y2), (nx, ny)) in &walls {
                let facing = nx * lx + ny * ly;
                let cp = wall_tint(facing, 0.38);
                let cm = wall_tint(-facing, 0.38);
                let cc = wall_tint(facing, 0.12);
                pc.vector(
                    x1 + nx * 1.6, y1 + ny * 1.6, x2 + nx * 1.6, y2 + ny * 1.6,
                    1.6, [cp[0], cp[1], cp[2], 0.78], Cap::Round,
                );
                pc.vector(
                    x1 - nx * 1.6, y1 - ny * 1.6, x2 - nx * 1.6, y2 - ny * 1.6,
                    1.6, [cm[0], cm[1], cm[2], 0.78], Cap::Round,
                );
                pc.vector(x1, y1, x2, y2, 1.8, [cc[0], cc[1], cc[2], 0.85], Cap::Round);
            }

            // Rims: beveled circles minus the angular span facing each wall
            // (no drawn border — the shaded edge IS the ring).
            for (idx, &(cx, cy)) in centers.iter().enumerate() {
                let top_a = if Some(idx) == self.selected_key_idx { 0.85 } else { 0.22 };
                if cuts[idx].is_empty() {
                    Self::rolled_rim_arc(pc, cx, cy, ring_r + 2.5, rim_t, 0.0, tau, az, base, top_a);
                    continue;
                }
                // Excluded spans [θ−α, θ+α] toward each neighbor, normalized
                // into [0, τ) (wrapping spans split), then merged.
                let mut segs: Vec<(f32, f32)> = Vec::new();
                for &((mx, my), (nx, ny)) in &cuts[idx] {
                    let theta = ny.atan2(nx);
                    let half = (mx - cx) * nx + (my - cy) * ny;
                    let alpha = (half / ring_r).clamp(-1.0, 1.0).acos();
                    let (a, b) = ((theta - alpha).rem_euclid(tau), (theta + alpha).rem_euclid(tau));
                    if a <= b {
                        segs.push((a, b));
                    } else {
                        segs.push((a, tau));
                        segs.push((0.0, b));
                    }
                }
                segs.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap());
                let mut merged: Vec<(f32, f32)> = Vec::new();
                for s in segs {
                    match merged.last_mut() {
                        Some(last) if s.0 <= last.1 => last.1 = last.1.max(s.1),
                        _ => merged.push(s),
                    }
                }
                // Stroke the complement (the two pieces meeting at θ=0 join
                // seamlessly when no span covers 0).
                let mut prev = 0.0f32;
                for &(a, b) in &merged {
                    if a > prev + 1e-3 {
                        Self::rolled_rim_arc(pc, cx, cy, ring_r + 2.5, rim_t, prev, a, az, base, top_a);
                    }
                    prev = prev.max(b);
                }
                if prev < tau - 1e-3 {
                    Self::rolled_rim_arc(pc, cx, cy, ring_r + 2.5, rim_t, prev, tau, az, base, top_a);
                }
            }
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
        let (well, radii) = crate::layout::carve_inside(graph, radii, depth);
        pc.recess(well, radii, depth);
        if !self.controls_collapsed {
            let dummy = UiContext::new();
            self.preset_dropdown.paint_self(&dummy, pc);
            self.line_type_dropdown.paint_self(&dummy, pc);
            if self.selected_key_idx.is_some() {
                self.key_pad.paint_self(&dummy, pc);
                self.del_button.paint_self(&dummy, pc);
            }
        }
    }
}

impl Input for Ramp {
    fn wants_tick(&self) -> bool {
        true
    }

    /// The graph context menu's actions. Overriding loses the trait-default
    /// clipboard arms, so Copy/Paste (the spec string) are restated here.
    fn context_action(&mut self, action: ContextAction) -> bool {
        match action {
            ContextAction::ToggleRampControls => {
                self.controls_collapsed = !self.controls_collapsed;
                self.just_changed = true;
                self.arrange_fields();
                true
            }
            ContextAction::Copy => {
                crate::widget::clipboard::copy_to_clipboard(&self.spec_string());
                true
            }
            ContextAction::Paste => {
                if let Some(text) = crate::widget::clipboard::read_from_clipboard() {
                    let changed = self.set_spec(&text);
                    if changed {
                        self.just_changed = true;
                    }
                    changed
                } else {
                    false
                }
            }
            _ => false,
        }
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

        // Hover-scroll inertia: once the finger stream stops (>60ms without
        // an event), the latched key coasts on the estimated velocity with
        // exponential decay, still resettling and syncing like live scrolls.
        if let (Some(idx), Some(last)) = (self.scroll_key_idx, self.last_key_scroll) {
            if last.elapsed().as_secs_f32() > 0.06 && idx < self.keys.len() {
                let (vx, vy) = self.scroll_vel;
                if vx.abs() > 0.02 || vy.abs() > 0.02 {
                    self.keys[idx].pos = (self.keys[idx].pos + vx * dt).clamp(0.0, 1.0);
                    self.keys[idx].value = (self.keys[idx].value + vy * dt).clamp(0.0, 1.0);
                    let settled = self.resettle_key(idx);
                    self.scroll_key_idx = Some(settled);
                    self.selected_key_idx = Some(settled);
                    self.key_pad
                        .set_values(self.keys[settled].pos, self.keys[settled].value);
                    self.preset_dropdown.selected = 0; // Custom
                    let f = (-5.0 * dt).exp();
                    self.scroll_vel = (vx * f, vy * f);
                    changed = true;
                } else {
                    self.scroll_vel = (0.0, 0.0);
                    self.last_key_scroll = None;
                }
            }
        }

        if self.preset_dropdown.tick(dt, ui) {
            let idx = self.preset_dropdown.selected;
            self.apply_preset(idx);
            changed = true;
        }
        
        if self.line_type_dropdown.tick(dt, ui) {
            changed = true;
        }
        
        if self.selected_key_idx.is_some() {
            if self.key_pad.tick(dt, ui) {
                self.apply_pad_to_selected();
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
                // Right-press in the graph opening → the shared context menu
                // (the key-crossing toggle lives there). Before the ui borrow:
                // open_context_menu needs the whole EventCtx.
                if button == MouseButton::Right {
                    if state == ElementState::Pressed {
                        let gh = self.graph_h();
                        let gx = self.base.x + 10.0;
                        let gw = self.base.w - 20.0;
                        let gy = self.base.y + 10.0;
                        if px >= gx && px <= gx + gw && py_event >= gy && py_event <= gy + gh {
                            ectx.open_context_menu(px, py_event);
                            return true;
                        }
                    }
                    return false;
                }
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
        
        let gh = self.graph_h();
        let plot = self.plot_rect();

        if state == ElementState::Pressed {
            // Any press cancels a hover-scroll glide in progress.
            self.scroll_vel = (0.0, 0.0);
            self.scroll_key_idx = None;
            self.last_key_scroll = None;
            // Grab the NEAREST key whose ring contains the press — the rings
            // are the pegs' visual extent, and nearest-center also matches the
            // foam walls (perpendicular bisectors) where rings overlap.
            let hit_r = Self::KEY_RING_R + 2.5;
            let mut best: Option<(usize, f32)> = None;
            for (idx, key) in self.keys.iter().enumerate() {
                let cx = plot.x + key.pos * plot.width;
                let cy = plot.y + plot.height * (1.0 - key.value);
                let dx = px - cx;
                let dy = py_event - cy;
                let d2 = dx * dx + dy * dy;
                if d2 <= hit_r * hit_r && best.is_none_or(|(_, bd)| d2 < bd) {
                    best = Some((idx, d2));
                }
            }
            if let Some((idx, _)) = best {
                self.selected_key_idx = Some(idx);
                self.is_dragging_key = true;
                self.key_pad.set_values(self.keys[idx].pos, self.keys[idx].value);
                self.arrange_fields();
                return true;
            }

            // Creation accepts the whole opening (the inset gutters included);
            // the domain mapping clamps to the plot's 0..1.
            if px >= self.base.x + 10.0 && px <= self.base.x + self.base.w - 10.0 && py_event >= self.base.y + 10.0 && py_event <= self.base.y + 10.0 + gh {
                let t = ((px - plot.x) / plot.width).clamp(0.0, 1.0);
                let val = (1.0 - (py_event - plot.y) / plot.height).clamp(0.0, 1.0);
                let new_key = RampKey { pos: t, value: val };
                self.keys.push(new_key);
                let new_idx = self.resettle_key(self.keys.len() - 1);
                self.preset_dropdown.selected = 0; // Custom
                self.just_changed = true;
                self.selected_key_idx = Some(new_idx);
                self.key_pad.set_values(t, val);
                // Arm the drag: a fresh key follows the pointer until release,
                // so create-and-place is one gesture (the grab-branch behavior).
                self.is_dragging_key = true;
                self.arrange_fields();
                return true;
            }
            
            if self.selected_key_idx.is_some() {
                if self.key_pad.mouse_input(button, state, px, py_event, ui) {
                    self.apply_pad_to_selected();
                    return true;
                }
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
                self.key_pad.mouse_input(button, state, px, py_event, ui);
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
        let plot = self.plot_rect();

        if self.is_dragging_key {
            if let Some(idx) = self.selected_key_idx {
                let t_raw = ((px - plot.x) / plot.width).clamp(0.0, 1.0);
                let t = self.resisted_pos(idx, t_raw);
                let val = (1.0 - (py_event - plot.y) / plot.height).clamp(0.0, 1.0);
                self.keys[idx].pos = t;
                self.keys[idx].value = val;
                self.key_pad.set_values(t, val);
                let settled = self.resettle_key(idx);
                self.selected_key_idx = Some(settled);
                self.preset_dropdown.selected = 0; // Custom
                changed = true;
            }
        }
        
        if self.selected_key_idx.is_some() {
            if self.key_pad.cursor_moved(px, py_event, ui) {
                self.apply_pad_to_selected();
                changed = true;
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
                // Hover-scroll: a gesture STARTING over a key latches it and
                // steers it on both axes — following the fingers like a drag
                // — until the stream pauses (fingers lifted). Mid-gesture the
                // latch holds even if the key slides out from under the
                // cursor. Latching also selects the key, so the pad tracks.
                let plot = self.plot_rect();
                if ui.scroll_gesture_new {
                    let hit_r = Self::KEY_RING_R + 2.5;
                    let mut best: Option<(usize, f32)> = None;
                    for (idx, key) in self.keys.iter().enumerate() {
                        let cx = plot.x + key.pos * plot.width;
                        let cy = plot.y + plot.height * (1.0 - key.value);
                        let dx = px - cx;
                        let dy = py - cy;
                        let d2 = dx * dx + dy * dy;
                        if d2 <= hit_r * hit_r && best.is_none_or(|(_, bd)| d2 < bd) {
                            best = Some((idx, d2));
                        }
                    }
                    self.scroll_key_idx = best.map(|(i, _)| i);
                    self.scroll_vel = (0.0, 0.0);
                }
                if let Some(idx) = self.scroll_key_idx {
                    if idx < self.keys.len() {
                        ui.scroll_initiate_widget_id = Some(ectx.id);
                        // Damped well below 1:1 — hover-scroll is for fine
                        // adjustment; the drag paths cover coarse moves.
                        let (dx, dy) = match &delta {
                            MouseScrollDelta::LineDelta(x, y) => (*x * 0.005, *y * 0.005),
                            MouseScrollDelta::PixelDelta(pos) => (
                                0.2 * pos.x as f32 / plot.width,
                                0.2 * pos.y as f32 / plot.height,
                            ),
                        };
                        // Direct manipulation: the key moves WITH the scroll
                        // (runner deltas are content-motion negated, so both
                        // axes flip): scroll right → key right, down → down.
                        self.keys[idx].pos = (self.keys[idx].pos - dx).clamp(0.0, 1.0);
                        self.keys[idx].value = (self.keys[idx].value + dy).clamp(0.0, 1.0);
                        // Velocity estimate for the release glide: EMA of
                        // applied delta over inter-event time. A leisurely
                        // wheel produces negligible velocity (big gaps clamp
                        // to 0.1s); fast trackpad streams build real speed.
                        let now = std::time::Instant::now();
                        let dt_ev = self
                            .last_key_scroll
                            .map(|t| now.duration_since(t).as_secs_f32())
                            .unwrap_or(0.016)
                            .clamp(0.004, 0.1);
                        self.last_key_scroll = Some(now);
                        let (ivx, ivy) = (-dx / dt_ev, dy / dt_ev);
                        self.scroll_vel = (
                            self.scroll_vel.0 * 0.65 + ivx * 0.35,
                            self.scroll_vel.1 * 0.65 + ivy * 0.35,
                        );
                        let settled = self.resettle_key(idx);
                        self.scroll_key_idx = Some(settled);
                        self.selected_key_idx = Some(settled);
                        self.key_pad
                            .set_values(self.keys[settled].pos, self.keys[settled].value);
                        self.preset_dropdown.selected = 0; // Custom
                        self.just_changed = true;
                        self.arrange_fields();
                        return true;
                    }
                    self.scroll_key_idx = None;
                }
                if self.selected_key_idx.is_some() && self.key_pad.mouse_wheel(&delta, px, py, ui) {
                    self.apply_pad_to_selected();
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
                    list.push((*self_ptr).key_pad.as_ptr_mut());
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
        if ui.is_focused(&self.key_pad) {
            return self.key_pad.keyboard_input(event, ui);
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
        self.key_pad.unfocus();
        self.del_button.unfocus();
    
                false
            }
            _ => false,
        }
    }

    // Field-slider drags forward through the composite (6bd self-routing), with the key
    // value sync the old descent path never ran mid-drag.
    fn draggable(&self, _rect: Rect) -> bool {
        self.is_dragging_key || self.key_pad.is_dragging()
    }
    fn is_dragging(&self) -> bool {
        self.is_dragging_key || self.key_pad.is_dragging()
    }
    fn drag_update(&mut self, px: f32, py: f32, _rect: Rect) -> bool {
        if self.key_pad.is_dragging() && self.key_pad.drag_update(px, py) {
            self.apply_pad_to_selected();
            return true;
        }
        false
    }
    fn drag_end(&mut self) {
        self.key_pad.drag_end();
        self.is_dragging_key = false;
    }
}
