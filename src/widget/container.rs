use crate::colors;
use crate::widget::*;
use crate::widget::display::make_widget_text_buffer;
use crate::widget::input::{BREADCRUMB_PADDING, SEGMENT_GAP};

#[derive(Clone)]
pub struct Container {
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub children: Vec<*mut (dyn Element + 'static)>,
}

impl Container {
    pub fn new() -> Self {
        Self { parent: None, children: Vec::new() }
    }
}

impl Element for Container {
    fn rect(&self) -> (f32, f32, f32, f32) { (0.0, 0.0, 0.0, 0.0) }
    fn set_rect(&mut self, _x: f32, _y: f32, _w: f32, _h: f32) {}
    fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }

    fn focus(&mut self) {
        focus::set_focused(self);
    }
    fn unfocus(&mut self) {}

    fn parent(&self) -> Option<*mut (dyn Element + 'static)> { self.parent }
    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>) { self.parent = parent; }
    fn children(&self) -> Vec<*mut (dyn Element + 'static)> { self.children.clone() }
    fn add_child(&mut self, child: *mut (dyn Element + 'static)) { self.children.push(child); }
    fn clear_children(&mut self) { self.children.clear(); }
}

impl Drop for Container {
    fn drop(&mut self) {
        focus::clear_if_matches(self);
    }
}


pub struct Header {
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
}

impl Header {
    pub fn new() -> Self { Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, hovered: false } }
}

impl Element for Header {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { colors::HEADER_BG }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }
}

pub struct ContentBg {
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
    show_network_grid: bool,
    grid_size_x: f32,
    grid_size_y: f32,
    grid_origin_x: f32,
    grid_origin_y: f32,
    skipped_row_h: f32,
    skipped_col_w: f32,
}

impl ContentBg {
    pub fn new() -> Self {
        Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, hovered: false, show_network_grid: false, grid_size_x: 150.0, grid_size_y: 75.0, grid_origin_x: 0.0, grid_origin_y: 0.0, skipped_row_h: 37.5, skipped_col_w: 37.5 }
    }
}

impl Element for ContentBg {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] {
        if self.show_network_grid {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            colors::CONTENT_BG
        }
    }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }
    fn hit_test(&self, _px: f32, _py: f32) -> bool { false }

    fn set_show_network_grid(&mut self, show: bool) { self.show_network_grid = show; }
    fn set_grid_sizes(&mut self, gx: f32, gy: f32) { self.grid_size_x = gx; self.grid_size_y = gy; }
    fn set_grid_origin(&mut self, ox: f32, oy: f32) { self.grid_origin_x = ox; self.grid_origin_y = oy; }
    fn set_skipped_sizes(&mut self, row_h: f32, col_w: f32) { self.skipped_row_h = row_h; self.skipped_col_w = col_w; }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.show_network_grid || self.grid_size_x <= 0.0 || self.grid_size_y <= 0.0 {
            return vec![];
        }
        let mut quads = Vec::new();
        let grid_color = [0.0, 0.0, 0.0, 0.0];
        let max_alpha = colors::CONTENT_BG[3]; // Peak opacity in the middle of gradient cells matches non-gradient cells
        let steps = 20; // Silky-smooth gradient transition

        let step_y = self.grid_size_y + self.skipped_row_h;
        let step_x = self.grid_size_x + self.skipped_col_w;

        if step_y >= 4.0 && step_x >= 4.0 {
            let ry_start = ((self.y - self.grid_origin_y) / step_y).floor() as i32 - 1;
            let ry_end = ((self.y + self.h - self.grid_origin_y) / step_y).ceil() as i32 + 1;
            let ry_start = ry_start.max(-100_000);
            let ry_end = ry_end.min(100_000);

            let cx_start = ((self.x - self.grid_origin_x) / step_x).floor() as i32 - 1;
            let cx_end = ((self.x + self.w - self.grid_origin_x) / step_x).ceil() as i32 + 1;
            let cx_start = cx_start.max(-100_000);
            let cx_end = cx_end.min(100_000);

            // Draw individual cell backgrounds to avoid stacking with gradients
            for ry in ry_start..=ry_end {
                let y1 = self.grid_origin_y + (ry as f32) * step_y;
                let draw_start_y = y1.max(self.y);
                let draw_end_y = (y1 + self.grid_size_y).min(self.y + self.h);
                if draw_start_y < draw_end_y {
                    for cx in cx_start..=cx_end {
                        let x1 = self.grid_origin_x + (cx as f32) * step_x;
                        let draw_start_x = x1.max(self.x);
                        let draw_end_x = (x1 + self.grid_size_x).min(self.x + self.w);
                        if draw_start_x < draw_end_x {
                            quads.push((draw_start_x, draw_start_y, draw_end_x - draw_start_x, draw_end_y - draw_start_y, colors::CONTENT_BG));
                        }
                    }
                }
            }
        }

        // Draw interstitial row gradients (horizontal bands fading to 0 alpha at left and right sides)
        if self.skipped_row_h > 0.0 {
            let step_y = self.grid_size_y + self.skipped_row_h;
            let step_x = self.grid_size_x + self.skipped_col_w;
            if step_y >= 4.0 && step_x >= 4.0 {
                let k_start = ((self.y - self.grid_origin_y) / step_y).floor() as i32 - 1;
                let k_end = ((self.y + self.h - self.grid_origin_y) / step_y).ceil() as i32 + 1;
                let k_start = k_start.max(-100_000);
                let k_end = k_end.min(100_000);

                let cx_start = ((self.x - self.grid_origin_x) / step_x).floor() as i32 - 1;
                let cx_end = ((self.x + self.w - self.grid_origin_x) / step_x).ceil() as i32 + 1;
                let cx_start = cx_start.max(-100_000);
                let cx_end = cx_end.min(100_000);

                for k in k_start..=k_end {
                    let y1 = self.grid_origin_y + (k as f32) * step_y;
                    let y2 = y1 + self.grid_size_y;
                    if y1 >= self.y + self.h {
                        continue;
                    }
                    let draw_start_y = y2.max(self.y);
                    let draw_end_y = (y2 + self.skipped_row_h).min(self.y + self.h);
                    if draw_start_y >= draw_end_y {
                        continue;
                    }

                    for cx in cx_start..=cx_end {
                        let x1 = self.grid_origin_x + (cx as f32) * step_x;
                        let x_mid = x1 + self.grid_size_x / 2.0;
                        let w_total = self.grid_size_x;
                        let sub_w = w_total / steps as f32;

                        for i in 0..steps {
                            let sx_start = x1 + i as f32 * sub_w;
                            let sx_end = sx_start + sub_w;
                            let draw_start_x = sx_start.max(self.x);
                            let draw_end_x = sx_end.min(self.x + self.w);
                            if draw_start_x < draw_end_x {
                                let sx_mid = (sx_start + sx_end) / 2.0;
                                let dist = (sx_mid - x_mid).abs();
                                let d = (dist / (w_total / 2.0)).min(1.0);
                                
                                // Fade the cell background color from max_alpha in the middle to transparent at the edges
                                let alpha = max_alpha * (1.0 - d);
                                if alpha > 0.001 {
                                    quads.push((draw_start_x, draw_start_y, draw_end_x - draw_start_x, draw_end_y - draw_start_y, [colors::CONTENT_BG[0], colors::CONTENT_BG[1], colors::CONTENT_BG[2], alpha]));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Draw interstitial column gradients (vertical bands fading to 0 alpha at top and bottom)
        if self.skipped_col_w > 0.0 {
            let step_y = self.grid_size_y + self.skipped_row_h;
            let step_x = self.grid_size_x + self.skipped_col_w;
            if step_y >= 4.0 && step_x >= 4.0 {
                let k_start = ((self.x - self.grid_origin_x) / step_x).floor() as i32 - 1;
                let k_end = ((self.x + self.w - self.grid_origin_x) / step_x).ceil() as i32 + 1;
                let k_start = k_start.max(-100_000);
                let k_end = k_end.min(100_000);

                let ry_start = ((self.y - self.grid_origin_y) / step_y).floor() as i32 - 1;
                let ry_end = ((self.y + self.h - self.grid_origin_y) / step_y).ceil() as i32 + 1;
                let ry_start = ry_start.max(-100_000);
                let ry_end = ry_end.min(100_000);

                for k in k_start..=k_end {
                    let x1 = self.grid_origin_x + (k as f32) * step_x;
                    let x2 = x1 + self.grid_size_x;
                    if x1 >= self.x + self.w {
                        continue;
                    }
                    let draw_start_x = x2.max(self.x);
                    let draw_end_x = (x2 + self.skipped_col_w).min(self.x + self.w);
                    if draw_start_x >= draw_end_x {
                        continue;
                    }

                    for ry in ry_start..=ry_end {
                        let y1 = self.grid_origin_y + (ry as f32) * step_y;
                        let y_mid = y1 + self.grid_size_y / 2.0;
                        let h_total = self.grid_size_y;
                        let sub_h = h_total / steps as f32;

                        for i in 0..steps {
                            let sy_start = y1 + i as f32 * sub_h;
                            let sy_end = sy_start + sub_h;
                            let draw_start_y = sy_start.max(self.y);
                            let draw_end_y = sy_end.min(self.y + self.h);
                            if draw_start_y < draw_end_y {
                                let sy_mid = (sy_start + sy_end) / 2.0;
                                let dist = (sy_mid - y_mid).abs();
                                let d = (dist / (h_total / 2.0)).min(1.0);
                                
                                // Fade the cell background color from max_alpha in the middle to transparent at the edges
                                let alpha = max_alpha * (1.0 - d);
                                if alpha > 0.001 {
                                    quads.push((draw_start_x, draw_start_y, draw_end_x - draw_start_x, draw_end_y - draw_start_y, [colors::CONTENT_BG[0], colors::CONTENT_BG[1], colors::CONTENT_BG[2], alpha]));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Draw the grid borders
        let step_y = self.grid_size_y + self.skipped_row_h;
        if step_y >= 4.0 {
            let k_start = ((self.y - self.grid_origin_y) / step_y).floor() as i32 - 1;
            let k_end = ((self.y + self.h - self.grid_origin_y) / step_y).ceil() as i32 + 1;
            let k_start = k_start.max(-100_000);
            let k_end = k_end.min(100_000);
            for k in k_start..=k_end {
                let y1 = self.grid_origin_y + (k as f32) * step_y;
                let y2 = y1 + self.grid_size_y;
                if y1 >= self.y + self.h {
                    continue;
                }
                if y1 >= self.y {
                    quads.push((self.x, y1, self.w, 1.0, grid_color));
                }
                if y2 >= self.y && y2 < self.y + self.h {
                    quads.push((self.x, y2, self.w, 1.0, grid_color));
                }
            }
        }

        let step_x = self.grid_size_x + self.skipped_col_w;
        if step_x >= 4.0 {
            let k_start = ((self.x - self.grid_origin_x) / step_x).floor() as i32 - 1;
            let k_end = ((self.x + self.w - self.grid_origin_x) / step_x).ceil() as i32 + 1;
            let k_start = k_start.max(-100_000);
            let k_end = k_end.min(100_000);
            for k in k_start..=k_end {
                let x1 = self.grid_origin_x + (k as f32) * step_x;
                let x2 = x1 + self.grid_size_x;
                if x1 >= self.x + self.w {
                    continue;
                }
                if x1 >= self.x {
                    quads.push((x1, self.y, 1.0, self.h, grid_color));
                }
                if x2 >= self.x && x2 < self.x + self.w {
                    quads.push((x2, self.y, 1.0, self.h, grid_color));
                }
            }
        }
        quads
    }
}

pub struct ViewportBg {
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
}

impl ViewportBg {
    pub fn new() -> Self { Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, hovered: false } }
}

impl Element for ViewportBg {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { colors::VIEWPORT_BG }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }
    fn hit_test(&self, _px: f32, _py: f32) -> bool { false }
}

pub struct ParametersBg {
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
    display_params: Vec<(String, String, String)>,
    dragging_param: Option<usize>,
    pub focused_param: Option<usize>,
    mouse_pos: Option<(f32, f32)>,
    sliders: Vec<Option<Slider>>,
    float3s: Vec<Option<Float3>>,
    spinboxes: Vec<Option<Spinbox>>,
    visible: bool,
}

impl ParametersBg {
    pub fn new() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            hovered: false,
            display_params: Vec::new(),
            dragging_param: None,
            focused_param: None,
            mouse_pos: None,
            sliders: Vec::new(),
            float3s: Vec::new(),
            spinboxes: Vec::new(),
            visible: true,
        }
    }

    pub fn get_param_rects(&self) -> Vec<(f32, f32, f32, f32)> {
        let mut rects = Vec::new();
        let mut cur_y = self.y + 30.0;
        for p in &self.display_params {
            let h = if p.2 == "code" {
                200.0
            } else if p.2 == "section" {
                24.0
            } else if p.2.starts_with("float3") {
                108.0
            } else if p.2 == "text" {
                24.0
            } else {
                20.0
            };
            rects.push((self.x + 8.0, cur_y, self.w - 16.0, h));
            cur_y += h + 8.0;
        }
        rects
    }

    fn update_slider_rects(&mut self) {
        let rects = self.get_param_rects();
        for (i, s_opt) in self.sliders.iter_mut().enumerate() {
            if let Some(s) = s_opt {
                let r = rects[i];
                let track_x = self.x + 100.0;
                let track_w = (self.w - 100.0 - 20.0).max(10.0);
                let track_y = r.1 + 4.0;
                let track_h = 12.0;
                s.set_rect(track_x, track_y, track_w, track_h);
            }
        }
        for (i, f_opt) in self.float3s.iter_mut().enumerate() {
            if let Some(f) = f_opt {
                let r = rects[i];
                f.set_rect(r.0, r.1, r.2, r.3);
            }
        }
        for (i, sb_opt) in self.spinboxes.iter_mut().enumerate() {
            if let Some(sb) = sb_opt {
                let r = rects[i];
                let box_x = self.x + 100.0;
                let box_w = (self.w - 100.0 - 16.0).max(10.0);
                sb.set_rect(box_x, r.1, box_w, r.3);
            }
        }
    }
}

fn parse_slider_range(ptype: &str) -> (f32, f32) {
    if ptype.starts_with("slider:") || ptype.starts_with("float3:") {
        let parts: Vec<&str> = ptype.split(':').collect();
        if parts.len() >= 3 {
            if let (Ok(min), Ok(max)) = (parts[1].parse::<f32>(), parts[2].parse::<f32>()) {
                return (min, max);
            }
        }
    }
    (0.0, 2.0)
}

fn parse_spinbox_range(ptype: &str) -> (i32, i32, i32) {
    if ptype.starts_with("spinbox:") {
        let parts: Vec<&str> = ptype.split(':').collect();
        if parts.len() >= 4 {
            if let (Ok(min), Ok(max), Ok(step)) = (parts[1].parse::<i32>(), parts[2].parse::<i32>(), parts[3].parse::<i32>()) {
                return (min, max, step);
            }
        } else if parts.len() == 3 {
            if let (Ok(min), Ok(max)) = (parts[1].parse::<i32>(), parts[2].parse::<i32>()) {
                return (min, max, 1);
            }
        }
    }
    (0, 10000, 1)
}

fn parse_float3_value(val_str: &str, min: f32, max: f32) -> [f32; 3] {
    let mut out = [0.5, 0.5, 0.5];
    let parts: Vec<&str> = val_str
        .split(|c| c == ':' || c == ',' || c == ' ')
        .filter(|s| !s.is_empty())
        .collect();
    for i in 0..3 {
        if i < parts.len() {
            if let Ok(v) = parts[i].parse::<f32>() {
                let range = max - min;
                if range != 0.0 {
                    out[i] = ((v - min) / range).clamp(0.0, 1.0);
                } else {
                    out[i] = 0.0;
                }
            }
        }
    }
    out
}

impl Element for ParametersBg {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.x = x; self.y = y; self.w = w; self.h = h;
        self.update_slider_rects();
    }
    fn color(&self) -> [f32; 4] {
        if !self.visible {
            return [0.0, 0.0, 0.0, 0.0];
        }
        colors::PARAM_BG
    }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }
    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }
    fn visible(&self) -> bool {
        self.visible
    }
    fn hit_test(&self, px: f32, py: f32) -> bool {
        if !self.visible {
            return false;
        }
        if crate::widget::popovers::is_coordinate_covered(self as *const Self as *const () as usize, px, py) {
            return false;
        }
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }

    fn set_display_params(&mut self, params: &[(String, String, String)]) {
        let mut layout_changed = self.display_params.len() != params.len();
        if !layout_changed {
            for (p_old, p_new) in self.display_params.iter().zip(params.iter()) {
                if p_old.0 != p_new.0 || p_old.2 != p_new.2 {
                    layout_changed = true;
                    break;
                }
            }
        }

        if layout_changed {
            self.display_params = params.to_vec();
            self.focused_param = None;
            self.sliders = self.display_params.iter().map(|p| {
                if p.2.starts_with("slider") {
                    let val = p.1.parse::<f32>().unwrap_or(0.0);
                    let (min, max) = parse_slider_range(&p.2);
                    let t = if max - min != 0.0 {
                        ((val - min) / (max - min)).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    Some(Slider::new().with_value(t).with_range(min, max).with_readout(true))
                } else {
                    None
                }
            }).collect();
            self.float3s = self.display_params.iter().map(|p| {
                if p.2.starts_with("float3") {
                    let (min, max) = parse_slider_range(&p.2);
                    let vals = parse_float3_value(&p.1, min, max);
                    Some(Float3::new().with_values(vals).with_range(min, max).with_label(&p.0))
                } else {
                    None
                }
            }).collect();
            self.spinboxes = self.display_params.iter().map(|p| {
                if p.2.starts_with("spinbox") {
                    let (min, max, step) = parse_spinbox_range(&p.2);
                    let val = p.1.parse::<i32>().unwrap_or(min);
                    Some(Spinbox::new(val, min, max, step))
                } else {
                    None
                }
            }).collect();
        } else {
            for (i, p_new) in params.iter().enumerate() {
                if Some(i) != self.focused_param && Some(i) != self.dragging_param {
                    self.display_params[i].1 = p_new.1.clone();
                    if let Some(ref mut s) = self.sliders[i] {
                        let val = p_new.1.parse::<f32>().unwrap_or(0.0);
                        let (min, max) = parse_slider_range(&p_new.2);
                        let t = if max - min != 0.0 {
                            ((val - min) / (max - min)).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        s.set_value(t);
                    } else if let Some(ref mut f) = self.float3s[i] {
                        let (min, max) = parse_slider_range(&p_new.2);
                        let vals = parse_float3_value(&p_new.1, min, max);
                        f.set_values(vals);
                    } else if let Some(ref mut sb) = self.spinboxes[i] {
                        if !sb.editing {
                            let (min, _max, _step) = parse_spinbox_range(&p_new.2);
                            let val = p_new.1.parse::<i32>().unwrap_or(min);
                            sb.value = val;
                        }
                    }
                }
            }
        }
        self.update_slider_rects();
    }

    fn unfocus(&mut self) {
        if let Some(idx) = self.focused_param {
            if idx < self.display_params.len() {
                let p = &mut self.display_params[idx];
                if p.2.starts_with("spinbox") {
                    if let Some(sb) = &mut self.spinboxes[idx] {
                        sb.unfocus();
                        p.1 = sb.value.to_string();
                    }
                } else if p.2.starts_with("slider") {
                    if let Some(s) = &mut self.sliders[idx] {
                        s.unfocus();
                        let (min, max) = parse_slider_range(&p.2);
                        let new_val = min + s.value * (max - min);
                        p.1 = format!("{:.2}", new_val);
                    }
                } else if p.2.starts_with("float3") {
                    if let Some(f) = &mut self.float3s[idx] {
                        f.unfocus();
                        let (min, max) = parse_slider_range(&p.2);
                        let val0 = min + f.values[0] * (max - min);
                        let val1 = min + f.values[1] * (max - min);
                        let val2 = min + f.values[2] * (max - min);
                        p.1 = format!("{:.2}:{:.2}:{:.2}", val0, val1, val2);
                    }
                }
            }
        }
        self.focused_param = None;
    }

    fn node_params(&self) -> Vec<(String, String, String)> {
        self.display_params.clone()
    }

    fn draggable(&self) -> bool {
        self.dragging_param.is_some() 
            || self.display_params.iter().any(|p| p.2.starts_with("slider") || p.2.starts_with("float3"))
    }

    fn is_dragging(&self) -> bool {
        self.dragging_param.is_some()
    }

    fn drag_begin(&mut self, px: f32, py: f32) {
        let rects = self.get_param_rects();
        for (i, p) in self.display_params.iter().enumerate() {
            if p.2.starts_with("slider") {
                let r = rects[i];
                let row_y = r.1;
                if py >= row_y - 2.0 && py <= row_y + 18.0 {
                    if let Some(s) = &mut self.sliders[i] {
                        s.drag_begin(px, py);
                        self.dragging_param = Some(i);
                        break;
                    }
                }
            } else if p.2.starts_with("float3") {
                let r = rects[i];
                if py >= r.1 && py <= r.1 + r.3 {
                    if let Some(f) = &mut self.float3s[i] {
                        if f.mouse_input(MouseButton::Left, ElementState::Pressed, px, py) {
                            self.dragging_param = Some(i);
                            break;
                        }
                    }
                }
            }
        }
    }

    fn drag_update(&mut self, px: f32, py: f32) -> bool {
        if let Some(i) = self.dragging_param {
            if let Some(s) = &mut self.sliders[i] {
                if s.drag_update(px, py) {
                    let (min, max) = parse_slider_range(&self.display_params[i].2);
                    let new_val = min + s.value * (max - min);
                    let old_val = &self.display_params[i].1;
                    let new_val_str = format!("{:.2}", new_val);
                    if *old_val != new_val_str {
                        self.display_params[i].1 = new_val_str;
                        return true;
                    }
                }
            } else if let Some(f) = &mut self.float3s[i] {
                if f.drag_update(px, py) {
                    let (min, max) = parse_slider_range(&self.display_params[i].2);
                    let val0 = min + f.values[0] * (max - min);
                    let val1 = min + f.values[1] * (max - min);
                    let val2 = min + f.values[2] * (max - min);
                    let new_val_str = format!("{:.2}:{:.2}:{:.2}", val0, val1, val2);
                    let old_val = &self.display_params[i].1;
                    if *old_val != new_val_str {
                        self.display_params[i].1 = new_val_str;
                        return true;
                    }
                }
            }
        }
        false
    }

    fn drag_end(&mut self) {
        if let Some(i) = self.dragging_param.take() {
            if let Some(s) = &mut self.sliders[i] {
                s.drag_end();
            } else if let Some(f) = &mut self.float3s[i] {
                f.drag_end();
            }
        }
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32) -> bool {
        self.mouse_pos = Some((px, py));
        true
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button == MouseButton::Left && state == ElementState::Pressed {
            let rects = self.get_param_rects();
            let mut clicked_any_focusable = false;
            for (i, p) in self.display_params.iter_mut().enumerate() {
                if p.2 == "code" {
                    let r = rects[i];
                    if px >= r.0 && px <= r.0 + r.2 && py >= r.1 + 18.0 && py <= r.1 + r.3 {
                        self.focused_param = Some(i);
                        clicked_any_focusable = true;
                        break;
                    }
                } else if p.2 == "text" {
                    let box_x = self.x + 100.0;
                    let box_w = (self.w - 100.0 - 16.0).max(10.0);
                    let r = rects[i];
                    if px >= box_x && px <= box_x + box_w && py >= r.1 && py <= r.1 + r.3 {
                        self.focused_param = Some(i);
                        clicked_any_focusable = true;
                        break;
                    }
                } else if p.2.starts_with("spinbox") {
                    if let Some(sb) = &mut self.spinboxes[i] {
                        if sb.mouse_input(button, state, px, py) {
                            p.1 = sb.value.to_string();
                            if sb.editing {
                                self.focused_param = Some(i);
                            } else {
                                self.unfocus();
                            }
                            return true;
                        }
                    }
                } else if p.2.starts_with("slider") {
                    let r = rects[i];
                    if py >= r.1 && py <= r.1 + r.3 {
                        if let Some(s) = &mut self.sliders[i] {
                            if s.mouse_input(button, state, px, py) {
                                if s.editing {
                                    self.focused_param = Some(i);
                                    clicked_any_focusable = true;
                                }
                                break;
                            }
                        }
                    }
                } else if p.2.starts_with("float3") {
                    let r = rects[i];
                    if py >= r.1 && py <= r.1 + r.3 {
                        if let Some(f) = &mut self.float3s[i] {
                            if f.mouse_input(button, state, px, py) {
                                if f.editing_idx.is_some() {
                                    self.focused_param = Some(i);
                                    clicked_any_focusable = true;
                                }
                                break;
                            }
                        }
                    }
                }
            }
            if !clicked_any_focusable {
                self.unfocus();
            }
            return true;
        }
        false
    }

    fn keyboard_input(&mut self, event: &KeyEvent) -> bool {
        if let Some(idx) = self.focused_param {
            if event.state == ElementState::Pressed {
                let p = &mut self.display_params[idx];
                if p.2 == "code" {
                    match &event.logical_key {
                        Key::Named(NamedKey::Backspace) => {
                            if !p.1.is_empty() {
                                p.1.pop();
                                return true;
                            }
                        }
                        Key::Named(NamedKey::Enter) => {
                            p.1.push('\n');
                            return true;
                        }
                        Key::Named(NamedKey::Escape) => {
                            self.focused_param = None;
                            return true;
                        }
                        Key::Character(s) => {
                            p.1.push_str(s);
                            return true;
                        }
                        _ => {}
                    }
                } else if p.2 == "text" {
                    match &event.logical_key {
                        Key::Named(NamedKey::Backspace) => {
                            if !p.1.is_empty() {
                                p.1.pop();
                                return true;
                            }
                        }
                        Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Escape) => {
                            self.focused_param = None;
                            return true;
                        }
                        Key::Character(s) => {
                            p.1.push_str(s);
                            return true;
                        }
                        _ => {}
                    }
                } else if p.2.starts_with("spinbox") {
                    if let Some(sb) = &mut self.spinboxes[idx] {
                        if sb.keyboard_input(event) {
                            if !sb.editing {
                                p.1 = sb.value.to_string();
                                self.focused_param = None;
                            } else {
                                p.1 = sb.edit_buffer.clone();
                            }
                            return true;
                        }
                    }
                } else if p.2.starts_with("slider") {
                    if let Some(s) = &mut self.sliders[idx] {
                        if s.keyboard_input(event) {
                            let (min, max) = parse_slider_range(&p.2);
                            let new_val = min + s.value * (max - min);
                            p.1 = format!("{:.2}", new_val);
                            if !s.editing {
                                self.focused_param = None;
                            }
                            return true;
                        }
                    }
                } else if p.2.starts_with("float3") {
                    if let Some(f) = &mut self.float3s[idx] {
                        if f.keyboard_input(event) {
                            let (min, max) = parse_slider_range(&p.2);
                            let val0 = min + f.values[0] * (max - min);
                            let val1 = min + f.values[1] * (max - min);
                            let val2 = min + f.values[2] * (max - min);
                            p.1 = format!("{:.2}:{:.2}:{:.2}", val0, val1, val2);
                            if f.editing_idx.is_none() {
                                self.focused_param = None;
                            }
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32) -> bool {
        let mut changed = false;
        let rects = self.get_param_rects();
        for (i, p) in self.display_params.iter_mut().enumerate() {
            if p.2.starts_with("slider") {
                let r = rects[i];
                let row_y = r.1;
                if py >= row_y - 2.0 && py <= row_y + 18.0 && px >= self.x && px <= self.x + self.w {
                    if let Some(s) = &mut self.sliders[i] {
                        let was_scroll = s.scroll_enabled;
                        s.set_scroll(true);
                        if s.mouse_wheel(delta, px, py) {
                            let (min, max) = parse_slider_range(&p.2);
                            let new_val = min + s.value * (max - min);
                            let old_val = &p.1;
                            let new_val_str = format!("{:.2}", new_val);
                            if *old_val != new_val_str {
                                p.1 = new_val_str;
                                changed = true;
                            }
                        }
                        s.set_scroll(was_scroll);
                    }
                }
            } else if p.2.starts_with("float3") {
                let r = rects[i];
                let row_y = r.1;
                if py >= row_y && py <= row_y + r.3 && px >= self.x && px <= self.x + self.w {
                    if let Some(f) = &mut self.float3s[i] {
                        let rects_inner = f.get_row_rects();
                        for j in 0..3 {
                            let r_inner = rects_inner[j];
                            if py >= r_inner.1 && py <= r_inner.1 + r_inner.3 {
                                let scroll_amount = match delta {
                                    MouseScrollDelta::LineDelta(_x, y) => *y,
                                    MouseScrollDelta::PixelDelta(pos) => (pos.y as f32) / 120.0,
                                };
                                let step = 0.02;
                                let new_val = (f.values[j] - scroll_amount * step).clamp(0.0, 1.0);
                                if (new_val - f.values[j]).abs() > 0.0001 {
                                    f.values[j] = new_val;
                                    if f.editing_idx == Some(j) {
                                        let scaled_val = f.mins[j] + f.values[j] * (f.maxs[j] - f.mins[j]);
                                        f.edit_buffer = format!("{:.2}", scaled_val);
                                    }
                                    let (min, max) = parse_slider_range(&p.2);
                                    let val0 = min + f.values[0] * (max - min);
                                    let val1 = min + f.values[1] * (max - min);
                                    let val2 = min + f.values[2] * (max - min);
                                    let new_val_str = format!("{:.2}:{:.2}:{:.2}", val0, val1, val2);
                                    if p.1 != new_val_str {
                                        p.1 = new_val_str;
                                        changed = true;
                                    }
                                }
                            }
                        }
                    }
                }
            } else if p.2.starts_with("spinbox") {
                let r = rects[i];
                let row_y = r.1;
                if py >= row_y && py <= row_y + r.3 && px >= self.x && px <= self.x + self.w {
                    if let Some(sb) = &mut self.spinboxes[i] {
                        let scroll_amount = match delta {
                            MouseScrollDelta::LineDelta(_x, y) => *y as i32,
                            MouseScrollDelta::PixelDelta(pos) => {
                                let dy = pos.y;
                                if dy > 0.0 { 1 } else if dy < 0.0 { -1 } else { 0 }
                            }
                        };
                        let new_val = (sb.value + scroll_amount * sb.step).clamp(sb.min, sb.max);
                        if sb.value != new_val {
                            sb.value = new_val;
                            p.1 = new_val.to_string();
                            changed = true;
                        }
                    }
                }
            }
        }
        changed
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.visible {
            return Vec::new();
        }
        let mut quads = Vec::new();
        let rects = self.get_param_rects();

        // Find sections and their ranges
        let mut sections = Vec::new();
        let mut current_section: Option<(usize, usize)> = None;
        let mut in_section = false;
        for (i, p) in self.display_params.iter().enumerate() {
            if p.2 == "section" {
                if let Some((start, end)) = current_section {
                    sections.push((start, end));
                }
                current_section = None;
                in_section = true;
            } else {
                if in_section {
                    if let Some((_, ref mut end)) = current_section {
                        *end = i;
                    } else {
                        current_section = Some((i, i));
                    }
                }
            }
        }
        if let Some((start, end)) = current_section {
            sections.push((start, end));
        }

        // Draw section border boxes
        for (start, end) in sections {
            if start <= end && start < rects.len() && end < rects.len() {
                let r_start = rects[start];
                let r_end = rects[end];
                let bx = self.x + 4.0;
                let bw = self.w - 8.0;
                let by = r_start.1 - 4.0;
                let bh = (r_end.1 + r_end.3 + 4.0) - by;
                
                let border_color = [0.18, 0.18, 0.27, 1.0];
                let border_t = 1.0;
                
                // Top border
                quads.push((bx, by, bw, border_t, border_color));
                // Bottom border
                quads.push((bx, by + bh - border_t, bw, border_t, border_color));
                // Left border
                quads.push((bx, by, border_t, bh, border_color));
                // Right border
                quads.push((bx + bw - border_t, by, border_t, bh, border_color));
            }
        }

        for (i, p) in self.display_params.iter().enumerate() {
            let r = rects[i];
            if p.2.starts_with("slider") {
                if let Some(s) = &self.sliders[i] {
                    let (sx, sy, sw, sh) = s.rect();
                    quads.push((sx, sy, sw, sh, s.color()));
                    quads.extend(s.extra_quads());
                }
            } else if p.2 == "section" {
                // Section header line is handled by the border box top border now
            } else if p.2.starts_with("float3") {
                if let Some(f) = &self.float3s[i] {
                    quads.extend(f.extra_quads());
                }
            } else if p.2 == "code" {
                quads.push((r.0, r.1 + 18.0, r.2, r.3 - 18.0, [0.08, 0.08, 0.10, 1.0]));
                let border_color = if self.focused_param == Some(i) {
                    [0.25, 0.45, 0.85, 1.0]
                } else {
                    [0.20, 0.20, 0.25, 1.0]
                };
                let (bx, by, bw, bh) = (r.0, r.1 + 18.0, r.2, r.3 - 18.0);
                quads.push((bx, by, bw, 1.0, border_color));
                quads.push((bx, by + bh - 1.0, bw, 1.0, border_color));
                quads.push((bx, by, 1.0, bh, border_color));
                quads.push((bx + bw - 1.0, by, 1.0, bh, border_color));
            } else if p.2 == "text" {
                let box_x = self.x + 100.0;
                let box_w = (self.w - 100.0 - 16.0).max(10.0);
                quads.push((box_x, r.1, box_w, r.3, [0.08, 0.08, 0.10, 1.0]));
                let border_color = if self.focused_param == Some(i) {
                    [0.25, 0.45, 0.85, 1.0]
                } else {
                    [0.20, 0.20, 0.25, 1.0]
                };
                let (bx, by, bw, bh) = (box_x, r.1, box_w, r.3);
                let border_t = 1.0;
                quads.push((bx, by, bw, border_t, border_color));
                quads.push((bx, by + bh - border_t, bw, border_t, border_color));
                quads.push((bx, by, border_t, bh, border_color));
                quads.push((bx + bw - border_t, by, border_t, bh, border_color));
            } else if p.2.starts_with("spinbox") {
                if let Some(sb) = &self.spinboxes[i] {
                    quads.push((sb.base.x, sb.base.y, sb.base.w, sb.base.h, sb.color()));
                    quads.extend(sb.extra_quads());
                }
            }
        }
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        if !self.visible {
            return Vec::new();
        }
        let rects = self.get_param_rects();
        let mut labels = Vec::new();
        for (i, (name, value, ptype)) in self.display_params.iter().enumerate() {
            let r = rects[i];
            if ptype.starts_with("slider") {
                labels.push(TextLabel {
                    text: name.clone(),
                    x: self.x + 8.0,
                    y: r.1,
                    font_size: 12.0,
                    color: [0xaa, 0xaa, 0xbb],
                });
                if let Some(s) = &self.sliders[i] {
                    labels.extend(s.text_labels());
                }
            } else if ptype.starts_with("float3") {
                if let Some(f) = &self.float3s[i] {
                    labels.extend(f.text_labels());
                }
            } else if ptype == "section" {
                labels.push(TextLabel {
                    text: name.clone(),
                    x: self.x + 12.0,
                    y: r.1 + 2.0,
                    font_size: 13.0,
                    color: [0xee, 0xee, 0xf0],
                });
            } else if ptype == "code" {
                labels.push(TextLabel {
                    text: format!("{}:\n{}", name, value),
                    x: self.x + 12.0,
                    y: r.1,
                    font_size: 12.0,
                    color: [0xaa, 0xaa, 0xbb],
                });
            } else if ptype.starts_with("spinbox") {
                labels.push(TextLabel {
                    text: name.clone(),
                    x: self.x + 8.0,
                    y: r.1 + (r.3 - 12.0) / 2.0 - 2.0,
                    font_size: 12.0,
                    color: [0xaa, 0xaa, 0xbb],
                });
                if let Some(sb) = &self.spinboxes[i] {
                    labels.extend(sb.text_labels());
                }
            } else if ptype == "text" {
                labels.push(TextLabel {
                    text: name.clone(),
                    x: self.x + 8.0,
                    y: r.1 + (r.3 - 12.0) / 2.0 - 2.0,
                    font_size: 12.0,
                    color: [0xaa, 0xaa, 0xbb],
                });
                let val_text = if self.focused_param == Some(i) {
                    format!("{}|", value)
                } else {
                    value.clone()
                };
                labels.push(TextLabel {
                    text: val_text,
                    x: self.x + 106.0,
                    y: r.1 + (r.3 - 12.0) / 2.0 - 2.0,
                    font_size: 12.0,
                    color: [0xee, 0xee, 0xf0],
                });
            } else {
                labels.push(TextLabel {
                    text: format!("{}: {}", name, value),
                    x: self.x + 8.0,
                    y: r.1,
                    font_size: 12.0,
                    color: [0xaa, 0xaa, 0xbb],
                });
            }
        }
        labels
    }
}

pub struct MenuBar {
    x: f32, y: f32, w: f32, h: f32,
    hovering: bool,
    pub title: String,
    pub menus: Vec<Box<Menu>>,
    pub menu_items: Vec<String>,
    pub vertical_items: Vec<String>,
    pub menu_dropdowns: Vec<Vec<String>>,
    pub menu_dropdown_checked: Vec<Vec<Option<bool>>>,
    pub hovered_menu: Option<usize>,
    pub open_menu: Option<usize>,
    pub hovered_dropdown: Option<usize>,
    pub clicked_dropdown: Option<(usize, usize)>,
    pub was_open: Option<usize>,
    pub vertical: bool,
    pub visible: bool,
    pub focused: bool,
    pub z_level: i32,
    pub center_items: bool,
    pub curved_circle: Option<(f32, f32, f32)>,
    pub title_pos: Option<(f32, f32)>,
    pub title_buf: Option<glyphon::Buffer>,
    pub curved_title_char_bufs: Vec<glyphon::Buffer>,
    pub network_opacity: f32,
}

impl MenuBar {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            x, y, w, h, hovering: false,
            title: String::new(),
            menus: Vec::new(),
            menu_items: Vec::new(),
            vertical_items: Vec::new(),
            menu_dropdowns: Vec::new(),
            menu_dropdown_checked: Vec::new(),
            hovered_menu: None,
            open_menu: None,
            hovered_dropdown: None,
            clicked_dropdown: None,
            was_open: None,
            vertical: false,
            visible: true,
            focused: false,
            z_level: 100,
            center_items: false,
            curved_circle: None,
            title_pos: None,
            title_buf: None,
            curved_title_char_bufs: Vec::new(),
            network_opacity: 1.0,
        }
    }

    pub fn with_center_items(mut self, center: bool) -> Self {
        self.center_items = center;
        self
    }

    pub fn with_title(mut self, title: &str) -> Self {
        self.title = title.to_string();
        self
    }

    pub fn with_item(mut self, label: &str, items: &[&str]) -> Self {
        self.menu_items.push(label.to_string());
        self.vertical_items.push(label.to_string());
        self.menu_dropdowns.push(items.iter().map(|s| s.to_string()).collect());
        self.menu_dropdown_checked.push(vec![None; items.len()]);

        let item_strs: Vec<String> = items.iter().map(|s| s.to_string()).collect();
        let mut menu = Menu::new(label, label, &item_strs);
        menu.vertical = self.vertical;
        self.menus.push(Box::new(menu));
        self
    }

    pub fn with_item_vh(mut self, horizontal_label: &str, vertical_label: &str, items: &[&str]) -> Self {
        self.menu_items.push(horizontal_label.to_string());
        self.vertical_items.push(vertical_label.to_string());
        self.menu_dropdowns.push(items.iter().map(|s| s.to_string()).collect());
        self.menu_dropdown_checked.push(vec![None; items.len()]);

        let item_strs: Vec<String> = items.iter().map(|s| s.to_string()).collect();
        let mut menu = Menu::new(horizontal_label, vertical_label, &item_strs);
        menu.vertical = self.vertical;
        self.menus.push(Box::new(menu));
        self
    }

    pub fn with_vertical(mut self, vertical: bool) -> Self {
        self.vertical = vertical;
        for menu in &mut self.menus {
            menu.vertical = vertical;
        }
        self
    }

    pub fn with_z_index(mut self, z: i32) -> Self {
        self.z_level = z;
        self
    }

    fn item_y_vertical(&self, idx: usize) -> f32 {
        let mut y = 8.0;
        if !self.title.is_empty() {
            y += 24.0;
        }
        y + idx as f32 * 24.0
    }

    fn item_h_vertical(&self) -> f32 {
        24.0
    }
}

impl Element for MenuBar {
    fn rect(&self) -> (f32, f32, f32, f32) {
        if !self.visible {
            return (0.0, 0.0, 0.0, 0.0);
        }
        if self.vertical {
            let total_h = if self.menus.is_empty() {
                self.h
            } else {
                let last_idx = self.menus.len() - 1;
                self.item_y_vertical(last_idx) + self.item_h_vertical()
            };
            (self.x, self.y, self.w, total_h)
        } else {
            (self.x, self.y, self.w, self.h)
        }
    }

    fn set_curved_circle(&mut self, circle: Option<(f32, f32, f32)>) {
        self.curved_circle = circle;
        if circle.is_none() {
            for menu in &mut self.menus {
                menu.curved_arc = None;
            }
        }
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.x = x;
        self.y = y;
        self.w = w;
        self.h = h;

        let parent_ptr = self as *mut MenuBar as *mut (dyn Element + 'static);

        if self.vertical {
            let mut cy = 8.0;
            if !self.title.is_empty() {
                cy += 24.0;
            }
            for menu in &mut self.menus {
                let ih = 24.0;
                menu.set_rect(x, y + cy, w, ih);
                menu.set_parent(Some(parent_ptr));
                cy += ih;
            }
        } else {
            if let Some((ccx, ccy, ccr)) = self.curved_circle {
                let r_mid = ccr - h / 2.0;
                let mut total_width = 8.0;
                if !self.title.is_empty() {
                    total_width += self.title.len() as f32 * 7.5 + 24.0;
                }
                for menu in &self.menus {
                    total_width += menu.active_title().len() as f32 * 7.5 + 16.0;
                }
                
                let total_angular_width = total_width / r_mid;
                let start_angle = 1.5 * std::f32::consts::PI - total_angular_width / 2.0;
                let mut current_angle = start_angle;
                
                if !self.title.is_empty() {
                    let title_w = self.title.len() as f32 * 7.5 + 24.0;
                    let dtheta_title = title_w / r_mid;
                    let theta_title = current_angle + dtheta_title / 2.0;
                    
                    let tx = ccx + r_mid * theta_title.cos() - title_w / 2.0 + 8.0;
                    let ty = ccy + r_mid * theta_title.sin() - h / 2.0;
                    self.title_pos = Some((tx, ty));
                    current_angle += dtheta_title;
                } else {
                    self.title_pos = None;
                }
                
                for menu in &mut self.menus {
                    let iw = menu.active_title().len() as f32 * 7.5 + 16.0;
                    let dtheta_menu = iw / r_mid;
                    let theta_menu = current_angle + dtheta_menu / 2.0;
                    
                    let mx = ccx + r_mid * theta_menu.cos() - iw / 2.0;
                    let my = ccy + r_mid * theta_menu.sin() - h / 2.0;
                    
                    menu.set_rect(mx, my, iw, h);
                    menu.set_parent(Some(parent_ptr));
                    menu.curved_arc = Some((ccx, ccy, ccr, h, current_angle, current_angle + dtheta_menu));
                    current_angle += dtheta_menu;
                }
            } else {
                self.title_pos = None;
                let mut cx = 8.0;
                if self.center_items {
                    let mut total_width = 8.0;
                    if !self.title.is_empty() {
                        total_width += self.title.len() as f32 * 7.5 + 24.0;
                    }
                    for menu in &self.menus {
                        total_width += menu.active_title().len() as f32 * 7.5 + 16.0;
                    }
                    if self.w > total_width {
                        cx = (self.w - total_width) / 2.0;
                    }
                }
                if !self.title.is_empty() {
                    cx += self.title.len() as f32 * 7.5 + 24.0;
                }
                for menu in &mut self.menus {
                    let iw = menu.active_title().len() as f32 * 7.5 + 16.0;
                    menu.set_rect(x + cx, y, iw, h);
                    menu.set_parent(Some(parent_ptr));
                    cx += iw;
                }
            }
        }
    }

    fn set_network_opacity(&mut self, opacity: f32) {
        self.network_opacity = opacity;
    }

    fn color(&self) -> [f32; 4] {
        if !self.visible {
            [0.0, 0.0, 0.0, 0.0]
        } else if self.focused {
            let mut c = colors::PANEL_MENU_FOCUSED;
            c[3] *= self.network_opacity;
            c
        } else {
            let mut c = colors::PANEL_MENU_BG;
            c[3] *= self.network_opacity;
            c
        }
    }

    fn set_hovered(&mut self, v: bool) {
        self.hovering = v;
    }

    fn hovered(&self) -> bool {
        self.hovering
    }

    fn hit_test(&self, px: f32, py: f32) -> bool {
        if !self.visible {
            return false;
        }
        if crate::widget::popovers::is_coordinate_covered(self as *const Self as *const () as usize, px, py) {
            return false;
        }
        if let Some((ccx, ccy, ccr)) = self.curved_circle {
            let dx = px - ccx;
            let dy = py - ccy;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist >= ccr - self.h && dist <= ccr {
                let angle = dy.atan2(dx);
                let mut norm_angle = angle;
                if norm_angle < 0.0 {
                    norm_angle += 2.0 * std::f32::consts::PI;
                }
                
                let r_mid = ccr - self.h / 2.0;
                let mut total_width = 8.0;
                if !self.title.is_empty() {
                    total_width += self.title.len() as f32 * 7.5 + 24.0;
                }
                for menu in &self.menus {
                    total_width += menu.active_title().len() as f32 * 7.5 + 16.0;
                }
                let total_angular_width = total_width / r_mid;
                let start_angle = 1.5 * std::f32::consts::PI - total_angular_width / 2.0;
                let end_angle = 1.5 * std::f32::consts::PI + total_angular_width / 2.0;
                
                if norm_angle >= start_angle && norm_angle <= end_angle {
                    return true;
                }
            }
            for menu in &self.menus {
                if menu.hit_test(px, py) {
                    return true;
                }
            }
            return false;
        }
        let (rx, ry, rw, rh) = self.rect();
        if px >= rx && px <= rx + rw && py >= ry && py <= ry + rh {
            return true;
        }
        for menu in &self.menus {
            if menu.hit_test(px, py) {
                return true;
            }
        }
        false
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32) -> bool {
        if !self.visible {
            return false;
        }
        let (rx, ry, rw, rh) = self.rect();
        self.set_rect(rx, ry, rw, rh);

        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/home/lsgalante/Dropbox/Clear/debug.txt") {
            use std::io::Write;
            let _ = writeln!(f, "MenuBar::on_cursor_moved px={}, py={} curved={:?} rect={:?}", px, py, self.curved_circle, (rx, ry, rw, rh));
        }

        let mut changed = false;
        self.hovered_menu = None;
        for (idx, menu) in self.menus.iter_mut().enumerate() {
            if menu.cursor_moved(px, py) {
                changed = true;
            }
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/home/lsgalante/Dropbox/Clear/debug.txt") {
                use std::io::Write;
                let _ = writeln!(f, "  Menu[{}] active_title={} curved={:?} hovered={} hit={}", idx, menu.active_title(), menu.curved_arc, menu.hovered(), menu.hit_test(px, py));
            }
            if menu.hovered() {
                self.hovered_menu = Some(idx);
            }
        }
        changed
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if !self.visible {
            return false;
        }
        let (rx, ry, rw, rh) = self.rect();
        self.set_rect(rx, ry, rw, rh);

        let mut changed = false;
        for menu in &mut self.menus {
            let res = menu.mouse_input(button, state, px, py);
            if res {
                changed = true;
            }
        }
        if !self.is_menu_open() {
            self.unfocus();
        }
        changed
    }

    fn focus(&mut self) {
        if self.is_menu_open() {
            self.focused = true;
            for menu in &mut self.menus {
                if menu.is_menu_open() {
                    menu.focus();
                    return;
                }
            }
        } else {
            self.focused = false;
            focus::clear_if_matches(self);
            return;
        }
        self.focused = true;
        focus::set_focused(self);
    }

    fn unfocus(&mut self) {
        self.focused = false;
        focus::clear_if_matches(self);
        for menu in &mut self.menus {
            menu.unfocus();
        }
    }

    fn focused(&self) -> bool {
        self.focused || self.is_menu_open()
    }

    fn set_selected(&mut self, selected: bool) {
        self.focused = selected;
        if !selected {
            for menu in &mut self.menus {
                menu.set_selected(false);
            }
        }
    }

    fn set_modifiers(&mut self, ctrl: bool, shift: bool, alt: bool) {
        for menu in &mut self.menus {
            menu.set_modifiers(ctrl, shift, alt);
        }
    }

    fn menu_names(&self) -> Vec<String> {
        self.menu_items.clone()
    }

    fn menu_click(&mut self) -> Option<(usize, usize)> {
        for (idx, menu) in self.menus.iter_mut().enumerate() {
            if let Some((_, item_idx)) = menu.menu_click() {
                return Some((idx, item_idx));
            }
        }
        None
    }

    fn set_item_checked(&mut self, menu_idx: usize, item_idx: usize, checked: bool) {
        if let Some(menu) = self.menu_dropdown_checked.get_mut(menu_idx) {
            if item_idx < menu.len() {
                menu[item_idx] = Some(checked);
            }
        }
        if let Some(menu) = self.menus.get_mut(menu_idx) {
            menu.set_item_checked(0, item_idx, checked);
        }
    }

    fn set_menu_items(&mut self, menu_idx: usize, items: &[String]) {
        if menu_idx < self.menu_dropdowns.len() {
            self.menu_dropdowns[menu_idx] = items.to_vec();
            self.menu_dropdown_checked[menu_idx] = vec![Some(false); items.len()];
        }
        if let Some(menu) = self.menus.get_mut(menu_idx) {
            menu.items = items.to_vec();
            menu.item_checked = vec![Some(false); items.len()];
            menu.item_bufs.clear();
        }
    }

    fn is_menu_bar(&self) -> bool {
        self.visible
    }

    fn get_menu_items_at(&self, px: f32, py: f32) -> Option<(usize, String, Vec<String>, f32, f32, f32, f32)> {
        if !self.visible {
            return None;
        }
        for (idx, menu) in self.menus.iter().enumerate() {
            if menu.hit_test(px, py) {
                let mut formatted_items = Vec::new();
                for (i, item) in menu.items.iter().enumerate() {
                    let checked = menu.item_checked.get(i).and_then(|&v| v);
                    let prefix = match checked {
                        Some(true) => "✓ ",
                        Some(false) => "  ",
                        None => "",
                    };
                    formatted_items.push(format!("{}{}", prefix, item));
                }
                return Some((idx, menu.active_title().to_string(), formatted_items, menu.base.x, menu.base.y, menu.base.w, menu.base.h));
            }
        }
        None
    }

    fn trigger_menu_click(&mut self, menu_idx: usize, item_idx: usize) {
        if let Some(menu) = self.menus.get_mut(menu_idx) {
            menu.clicked_item = Some(item_idx);
        }
    }

    fn is_menu_open(&self) -> bool {
        self.visible && self.menus.iter().any(|m| m.is_menu_open())
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.visible {
            return Vec::new();
        }
        let mut quads = Vec::new();
        for menu in &self.menus {
            quads.extend(menu.all_quads());
        }
        quads
    }

    fn extra_arcs(&self) -> Vec<(f32, f32, f32, f32, f32, f32, [f32; 4])> {
        if !self.visible {
            return Vec::new();
        }
        let mut arcs = Vec::new();
        for menu in &self.menus {
            arcs.extend(menu.extra_arcs());
        }
        arcs
    }

    fn prepare_text(&mut self, fs: &mut glyphon::FontSystem) {
        if !self.visible {
            return;
        }
        if !self.title.is_empty() {
            if let Some((_ccx, _ccy, _ccr)) = self.curved_circle {
                if self.curved_title_char_bufs.len() != self.title.chars().count() {
                    self.curved_title_char_bufs = self.title.chars()
                        .map(|c| make_widget_text_buffer(fs, &c.to_string(), 12.0, "Outfit"))
                        .collect();
                }
                self.title_buf = None;
            } else {
                if self.title_buf.is_none() {
                    self.title_buf = Some(make_widget_text_buffer(fs, &self.title, 12.0, "Outfit"));
                }
                self.curved_title_char_bufs.clear();
            }
        } else {
            self.title_buf = None;
            self.curved_title_char_bufs.clear();
        }
        for menu in &mut self.menus {
            menu.prepare_text(fs);
        }
    }

    fn get_text_items(&self) -> Vec<(&glyphon::Buffer, f32, f32, glyphon::Color)> {
        if !self.visible {
            return Vec::new();
        }
        let mut items = Vec::new();
        let color = glyphon::Color::rgb(0xaa, 0xaa, 0xbb);

        if let Some((ccx, ccy, ccr)) = self.curved_circle {
            let r_mid = ccr - self.h / 2.0;
            let mut total_width = 8.0;
            if !self.title.is_empty() {
                total_width += self.title.len() as f32 * 7.5 + 24.0;
            }
            for menu in &self.menus {
                total_width += menu.active_title().len() as f32 * 7.5 + 16.0;
            }
            let total_angular_width = total_width / r_mid;
            let start_angle = 1.5 * std::f32::consts::PI - total_angular_width / 2.0;
            let current_angle = start_angle;

            if !self.title.is_empty() {
                let title_w = self.title.len() as f32 * 7.5 + 24.0;
                let dtheta_title = title_w / r_mid;

                let char_widths: Vec<f32> = self.title.chars().map(|c| {
                    TextLabel::estimate_width(&c.to_string(), 12.0)
                }).collect();
                let total_chars_width: f32 = char_widths.iter().sum();
                let mid_angle = (current_angle + current_angle + dtheta_title) / 2.0;
                let angular_width = total_chars_width / r_mid;
                let text_start_angle = mid_angle - angular_width / 2.0;
                let mut cur_char_angle = text_start_angle;

                for (char_idx, c_buf) in self.curved_title_char_bufs.iter().enumerate() {
                    let cw = char_widths[char_idx];
                    let dtheta = cw / r_mid;
                    let char_center_angle = cur_char_angle + dtheta / 2.0;

                    let tx = ccx + r_mid * char_center_angle.cos() - cw / 2.0;
                    let ty = ccy + r_mid * char_center_angle.sin() - 12.0 / 2.0;

                    items.push((c_buf, tx, ty, color));
                    cur_char_angle += dtheta;
                }
            }
        } else {
            let mut start_x = 8.0;
            if self.center_items {
                let mut total_width = 8.0;
                if !self.title.is_empty() {
                    total_width += self.title.len() as f32 * 7.5 + 24.0;
                }
                for menu in &self.menus {
                    total_width += menu.active_title().len() as f32 * 7.5 + 16.0;
                }
                if self.w > total_width {
                    start_x = (self.w - total_width) / 2.0;
                }
            }
            if let Some(ref title_buf) = self.title_buf {
                items.push((title_buf, self.x + start_x, self.y + 7.0, color));
            }
        }

        for menu in &self.menus {
            items.extend(menu.get_text_items());
        }
        items
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        if !self.visible {
            return Vec::new();
        }
        let mut labels = Vec::new();
        if let Some((ccx, ccy, ccr)) = self.curved_circle {
            let r_mid = ccr - self.h / 2.0;
            let mut total_width = 8.0;
            if !self.title.is_empty() {
                total_width += self.title.len() as f32 * 7.5 + 24.0;
            }
            for menu in &self.menus {
                total_width += menu.active_title().len() as f32 * 7.5 + 16.0;
            }
            let total_angular_width = total_width / r_mid;
            let start_angle = 1.5 * std::f32::consts::PI - total_angular_width / 2.0;
            let current_angle = start_angle;

            if !self.title.is_empty() {
                let title_w = self.title.len() as f32 * 7.5 + 24.0;
                let dtheta_title = title_w / r_mid;
                labels.extend(TextLabel::curved_layout(
                    &self.title,
                    ccx, ccy, r_mid,
                    current_angle, current_angle + dtheta_title,
                    12.0,
                    [0xaa, 0xaa, 0xbb],
                ));
            }
        } else {
            let mut start_x = 8.0;
            if self.center_items {
                let mut total_width = 8.0;
                if !self.title.is_empty() {
                    total_width += self.title.len() as f32 * 7.5 + 24.0;
                }
                for menu in &self.menus {
                    total_width += menu.active_title().len() as f32 * 7.5 + 16.0;
                }
                if self.w > total_width {
                    start_x = (self.w - total_width) / 2.0;
                }
            }
            if !self.title.is_empty() {
                labels.push(TextLabel {
                    text: self.title.clone(),
                    x: self.x + start_x,
                    y: self.y + 7.0,
                    font_size: 12.0,
                    color: [0xaa, 0xaa, 0xbb],
                });
            }
        }
        for menu in &self.menus {
            labels.extend(menu.text_labels());
        }
        labels
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
        for menu in &mut self.menus {
            menu.set_visible(visible);
        }
    }

    fn visible(&self) -> bool {
        self.visible
    }

    fn children(&self) -> Vec<*mut (dyn Element + 'static)> {
        self.menus.iter().map(|m| {
            let ptr: *const dyn Element = &**m as &dyn Element;
            ptr as *mut (dyn Element + 'static)
        }).collect()
    }

    fn z_index(&self) -> i32 {
        self.z_level
    }

    fn set_center_items(&mut self, center: bool) {
        self.center_items = center;
    }

    fn menu_items_list(&self) -> Vec<Vec<String>> {
        self.menu_dropdowns.clone()
    }

    fn menu_checked_list(&self) -> Vec<Vec<Option<bool>>> {
        self.menu_dropdown_checked.clone()
    }
}

impl Drop for MenuBar {
    fn drop(&mut self) {
        focus::clear_if_matches(self);
    }
}

#[derive(Debug, Clone)]
pub struct Menu {
    pub base: Widget,
    pub title: String,
    pub vertical_title: String,
    pub items: Vec<String>,
    pub item_checked: Vec<Option<bool>>,
    pub open: bool,
    pub vertical: bool,
    hovered_item: Option<usize>,
    clicked_item: Option<usize>,
    was_open: Option<usize>,
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub children: Vec<*mut (dyn Element + 'static)>,
    pub curved_arc: Option<(f32, f32, f32, f32, f32, f32)>,
    pub title_buf: Option<glyphon::Buffer>,
    pub item_bufs: Vec<glyphon::Buffer>,
    pub check_buf: Option<glyphon::Buffer>,
    pub curved_char_bufs: Vec<glyphon::Buffer>,
}

impl Menu {
    pub fn new(title: &str, vertical_title: &str, items: &[String]) -> Self {
        Self {
            base: Widget::new(),
            title: title.to_string(),
            vertical_title: vertical_title.to_string(),
            items: items.to_vec(),
            item_checked: vec![None; items.len()],
            open: false,
            vertical: false,
            hovered_item: None,
            clicked_item: None,
            was_open: None,
            parent: None,
            children: Vec::new(),
            curved_arc: None,
            title_buf: None,
            item_bufs: Vec::new(),
            check_buf: None,
            curved_char_bufs: Vec::new(),
        }
    }

    pub fn active_title(&self) -> &str {
        if self.vertical {
            &self.vertical_title
        } else {
            &self.title
        }
    }

    fn dropdown_rect(&self) -> (f32, f32, f32, f32) {
        let dh = self.items.len() as f32 * DROPDOWN_ITEM_H;
        let mut max_len = 0;
        for item in &self.items {
            max_len = max_len.max(item.len());
        }
        let dw = (max_len as f32 * 7.5 + 40.0).max(120.0);
        let dx = if self.vertical {
            self.base.x + self.base.w
        } else {
            self.base.x
        };
        let dy = if self.vertical {
            self.base.y
        } else {
            self.base.y + self.base.h
        };
        (dx, dy, dw, dh)
    }
}
impl Element for Menu {
    crate::impl_widget_base!(Menu);

    fn label(&self) -> Option<String> {
        Some(self.active_title().to_string())
    }

    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn hit_test(&self, px: f32, py: f32) -> bool {
        if crate::widget::popovers::is_coordinate_covered(self as *const Self as *const () as usize, px, py) {
            return false;
        }
        if let Some((cx, cy, r, thickness, start_angle, end_angle)) = self.curved_arc {
            let dx = px - cx;
            let dy = py - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist >= r - thickness && dist <= r {
                let angle = dy.atan2(dx);
                let mut norm_angle = angle;
                if norm_angle < 0.0 {
                    norm_angle += 2.0 * std::f32::consts::PI;
                }
                if norm_angle >= start_angle && norm_angle <= end_angle {
                    return true;
                }
            }
            if self.open {
                let (dx, dy, dw, dh) = self.dropdown_rect();
                if dh > 0.0 && px >= dx && px < dx + dw && py >= dy && py < dy + dh {
                    return true;
                }
            }
            return false;
        }
        let (rx, ry, rw, rh) = self.rect();
        if px >= rx && px <= rx + rw && py >= ry && py <= ry + rh {
            return true;
        }
        if self.open {
            let (dx, dy, dw, dh) = self.dropdown_rect();
            if dh > 0.0 && px >= dx && px < dx + dw && py >= dy && py < dy + dh {
                return true;
            }
        }
        false
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32) -> bool {
        self.was_open = None;
        let was_hovering = self.base.hovered;
        self.base.hovered = self.hit_test(px, py);
        let old_item = self.hovered_item;
        self.hovered_item = None;

        if self.open {
            let (dx, dy, dw, dh) = self.dropdown_rect();
            if px >= dx && px < dx + dw && py >= dy && py < dy + dh {
                let di = ((py - dy) / DROPDOWN_ITEM_H) as usize;
                if di < self.items.len() {
                    self.hovered_item = Some(di);
                }
            }
        }

        was_hovering != self.base.hovered || old_item != self.hovered_item
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button != MouseButton::Left || state != ElementState::Pressed {
            return false;
        }
        if !self.hit_test(px, py) {
            return false;
        }

        // Check dropdown click if open
        if self.open {
            let (dx, dy, dw, dh) = self.dropdown_rect();
            if dh > 0.0 && px >= dx && px < dx + dw && py >= dy && py < dy + dh {
                let di = ((py - dy) / DROPDOWN_ITEM_H) as usize;
                if di < self.items.len() {
                    self.clicked_item = Some(di);
                    self.open = false;
                    return true;
                }
            }
        }

        // Since we passed hit_test and didn't click dropdown, it's a click on the header title
        if self.was_open == Some(0) || self.open {
            self.open = false;
            self.was_open = None;
        } else {
            self.open = true;
            self.was_open = None;
            focus::set_focused(self);
        }
        true
    }

    fn focus(&mut self) {
        self.open = true;
        self.base.focused = true;
        focus::set_focused(self);
    }

    fn unfocus(&mut self) {
        if self.open {
            self.was_open = Some(0);
        }
        self.open = false;
        self.base.focused = false;
        focus::clear_if_matches(self);
        self.hovered_item = None;
    }

    fn set_selected(&mut self, selected: bool) {
        self.base.focused = selected;
        if !selected {
            self.open = false;
            self.was_open = None;
            self.hovered_item = None;
            focus::clear_if_matches(self);
        }
    }

    fn menu_click(&mut self) -> Option<(usize, usize)> {
        self.clicked_item.take().map(|i| (0, i))
    }

    fn set_item_checked(&mut self, _menu_idx: usize, item_idx: usize, checked: bool) {
        if item_idx < self.item_checked.len() {
            self.item_checked[item_idx] = Some(checked);
            self.item_bufs.clear();
        }
    }

    fn is_menu_open(&self) -> bool {
        self.open
    }

    fn highlight_quad(&self) -> Option<(f32, f32, f32, f32, [f32; 4])> {
        if self.curved_arc.is_some() {
            None
        } else {
            let hc = self.highlight_color()?;
            Some((self.base.x, self.base.y, self.base.w, self.base.h, hc))
        }
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        if self.open {
            let (dx, dy, dw, dh) = self.dropdown_rect();
            if dh > 0.0 {
                quads.push((dx, dy, dw, dh, colors::PANEL_MENU_BG));
                if let Some(di) = self.hovered_item {
                    quads.push((dx, dy + di as f32 * DROPDOWN_ITEM_H, dw, DROPDOWN_ITEM_H, colors::PANEL_MENU_HOVER));
                }
            }
        }
        quads
    }

    fn extra_arcs(&self) -> Vec<(f32, f32, f32, f32, f32, f32, [f32; 4])> {
        let mut arcs = Vec::new();
        if let Some((cx, cy, r, thickness, start_angle, end_angle)) = self.curved_arc {
            if self.base.hovered && !self.open {
                arcs.push((cx, cy, r, thickness, start_angle, end_angle, colors::PANEL_MENU_HOVER));
            }
        }
        arcs
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        if let Some((cx, cy, r, thickness, start_angle, end_angle)) = self.curved_arc {
            let r_mid = r - thickness / 2.0;
            labels.extend(TextLabel::curved_layout(
                &self.active_title(),
                cx, cy, r_mid,
                start_angle, end_angle,
                12.0,
                [0xcc, 0xcc, 0xd4],
            ));
        } else {
            labels.push(TextLabel {
                text: self.active_title().to_string(),
                x: self.base.x + 8.0,
                y: self.base.y + 7.0,
                font_size: 12.0,
                color: [0xcc, 0xcc, 0xd4],
            });
        }
        if self.open {
            let (dx, dy, _, _) = self.dropdown_rect();
            for (i, item) in self.items.iter().enumerate() {
                let checked = self.item_checked.get(i).and_then(|&v| v);
                let prefix = match checked {
                    Some(true) => "\u{2713} ",
                    Some(false) => "  ",
                    None => "",
                };
                labels.push(TextLabel {
                    text: format!("{}{}", prefix, item),
                    x: dx + 8.0,
                    y: dy + i as f32 * DROPDOWN_ITEM_H + 5.0,
                    font_size: 12.0,
                    color: [0xcc, 0xcc, 0xd4],
                });
            }
        }
        labels
    }

    fn set_visible(&mut self, visible: bool) {
        self.base.hovered = false;
        if !visible {
            self.open = false;
            self.was_open = None;
            self.hovered_item = None;
            focus::clear_if_matches(self);
        }
    }

    fn parent(&self) -> Option<*mut (dyn Element + 'static)> {
        self.parent
    }

    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>) {
        self.parent = parent;
    }

    fn z_index(&self) -> i32 {
        100
    }

    fn menu_items(&self) -> Vec<String> {
        self.items.clone()
    }

    fn menu_item_checked(&self) -> Vec<Option<bool>> {
        self.item_checked.clone()
    }

    fn is_vertical(&self) -> bool {
        self.vertical
    }

    fn focused(&self) -> bool {
        self.base.focused
    }

    fn prepare_text(&mut self, fs: &mut glyphon::FontSystem) {
        let title_text = self.active_title();
        if let Some((_cx, _cy, _r, _thickness, _start_angle, _end_angle)) = self.curved_arc {
            if self.curved_char_bufs.len() != title_text.chars().count() {
                self.curved_char_bufs = title_text.chars()
                    .map(|c| make_widget_text_buffer(fs, &c.to_string(), 12.0, "Outfit"))
                    .collect();
            }
            self.title_buf = None;
        } else {
            if self.title_buf.is_none() {
                self.title_buf = Some(make_widget_text_buffer(fs, title_text, 12.0, "Outfit"));
            }
            self.curved_char_bufs.clear();
        }

        if self.open {
            if self.item_bufs.len() != self.items.len() {
                self.item_bufs = self.items.iter().enumerate().map(|(i, item)| {
                    let checked = self.item_checked.get(i).and_then(|&v| v);
                    let prefix = match checked {
                        Some(true) => "\u{2713} ",
                        Some(false) => "  ",
                        None => "",
                    };
                    let text = format!("{}{}", prefix, item);
                    make_widget_text_buffer(fs, &text, 12.0, "Outfit")
                }).collect();
            }
        } else {
            self.item_bufs.clear();
        }
    }

    fn get_text_items(&self) -> Vec<(&glyphon::Buffer, f32, f32, glyphon::Color)> {
        let mut items = Vec::new();
        let color = glyphon::Color::rgb(0xcc, 0xcc, 0xd4);

        if let Some((cx, cy, r, thickness, start_angle, end_angle)) = self.curved_arc {
            let r_mid = r - thickness / 2.0;
            let active_title = self.active_title();
            
            let char_widths: Vec<f32> = active_title.chars().map(|c| {
                TextLabel::estimate_width(&c.to_string(), 12.0)
            }).collect();
            let total_chars_width: f32 = char_widths.iter().sum();
            
            let angular_width = total_chars_width / r_mid;
            let text_start_angle = (start_angle + end_angle) / 2.0 - angular_width / 2.0;
            let mut cur_char_angle = text_start_angle;
            
            for (char_idx, c_buf) in self.curved_char_bufs.iter().enumerate() {
                if char_idx < char_widths.len() {
                    let cw = char_widths[char_idx];
                    let dtheta = cw / r_mid;
                    let char_center_angle = cur_char_angle + dtheta / 2.0;
                    
                    let tx = cx + r_mid * char_center_angle.cos() - cw / 2.0;
                    let ty = cy + r_mid * char_center_angle.sin() - 12.0 / 2.0;
                    
                    items.push((c_buf, tx, ty, color));
                    cur_char_angle += dtheta;
                }
            }
        } else {
            if let Some(ref title_buf) = self.title_buf {
                items.push((title_buf, self.base.x + 8.0, self.base.y + 7.0, color));
            }
        }

        if self.open {
            let (dx, dy, _, _) = self.dropdown_rect();
            for (i, item_buf) in self.item_bufs.iter().enumerate() {
                items.push((
                    item_buf,
                    dx + 8.0,
                    dy + i as f32 * DROPDOWN_ITEM_H + 5.0,
                    color,
                ));
            }
        }

        items
    }
}

unsafe impl Send for Menu {}
unsafe impl Sync for Menu {}

impl Drop for Menu {
    fn drop(&mut self) {
        focus::clear_if_matches(self);
    }
}



#[derive(Debug, Clone)]
pub struct Breadcrumb {
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
    path: Vec<String>,
    hovered_seg: Option<usize>,
    clicked_seg: Option<usize>,
    pub network_opacity: f32,
}

impl Breadcrumb {
    pub fn new() -> Self {
        Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, hovered: false,
               path: Vec::new(), hovered_seg: None, clicked_seg: None, network_opacity: 1.0 }
    }

    fn seg_at(&self, px: f32) -> Option<usize> {
        let mut cx = self.x + BREADCRUMB_PADDING;
        for (i, seg) in self.path.iter().enumerate() {
            let w = seg.len() as f32 * 7.5;
            if px >= cx && px < cx + w {
                return Some(i);
            }
            cx += w + SEGMENT_GAP;
        }
        None
    }
}

impl Element for Breadcrumb {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn set_network_opacity(&mut self, opacity: f32) { self.network_opacity = opacity; }
    fn color(&self) -> [f32; 4] { [0.10, 0.10, 0.14, self.network_opacity] }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }

    fn on_cursor_moved(&mut self, px: f32, py: f32) -> bool {
        let was = self.hovered;
        self.hovered = self.hit_test(px, py);
        let old = self.hovered_seg;
        self.hovered_seg = if self.hovered { self.seg_at(px) } else { None };
        was != self.hovered || old != self.hovered_seg
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, _py: f32) -> bool {
        if button != MouseButton::Left || state != ElementState::Pressed { return false; }
        if let Some(i) = self.seg_at(px) {
            if i < self.path.len() - 1 {
                self.clicked_seg = Some(i);
                return true;
            }
        }
        false
    }

    fn set_path(&mut self, segments: &[String]) {
        let mut s = Vec::with_capacity(segments.len().max(1));
        if segments.is_empty() || (segments.len() == 1 && segments[0].is_empty()) {
            s.push("/".to_string());
        } else {
            s.push("/".to_string());
            for name in segments {
                s.push(format!(" \u{203A} {}", name));
            }
        }
        self.path = s;
    }

    fn path_click(&mut self) -> Option<usize> { self.clicked_seg.take() }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        if let Some(i) = self.hovered_seg {
            let mut cx = self.x + BREADCRUMB_PADDING;
            for j in 0..i {
                let w = self.path[j].len() as f32 * 7.5;
                cx += w + SEGMENT_GAP;
            }
            let w = self.path[i].len() as f32 * 7.5;
            quads.push((cx, self.y, w, self.h, [1.0, 1.0, 1.0, 0.06]));
        }
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        let mut cx = self.x + BREADCRUMB_PADDING;
        for (i, seg) in self.path.iter().enumerate() {
            labels.push(TextLabel {
                text: seg.clone(),
                x: cx,
                y: self.y + 6.0,
                font_size: 12.0,
                color: if i == self.path.len() - 1 { [0xcc, 0xcc, 0xd4] } else { [0x88, 0x88, 0x99] },
            });
            cx += seg.len() as f32 * 7.5 + SEGMENT_GAP;
        }
        labels
    }
}


pub struct Spreadsheet {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    hovered: bool,
    visible: bool,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    scroll_y: f32,
    scroll_velocity: f32,
    dragging_scrollbar: bool,
    drag_offset_y: f32,
    scrollbar_hovered: bool,
    scrollbar_thumb_hovered: bool,
}

impl Spreadsheet {
    pub fn new() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            hovered: false,
            visible: false,
            headers: Vec::new(),
            rows: Vec::new(),
            scroll_y: 0.0,
            scroll_velocity: 0.0,
            dragging_scrollbar: false,
            drag_offset_y: 0.0,
            scrollbar_hovered: false,
            scrollbar_thumb_hovered: false,
        }
    }
}

impl Element for Spreadsheet {
    fn rect(&self) -> (f32, f32, f32, f32) {
        if !self.visible {
            (0.0, 0.0, 0.0, 0.0)
        } else {
            (self.x, self.y, self.w, self.h)
        }
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.x = x;
        self.y = y;
        self.w = w;
        self.h = h;
    }

    fn color(&self) -> [f32; 4] {
        if !self.visible {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            colors::PARAM_BG
        }
    }

    fn set_hovered(&mut self, v: bool) {
        self.hovered = v;
    }

    fn hovered(&self) -> bool {
        self.hovered
    }

    fn hit_test(&self, px: f32, py: f32) -> bool {
        if !self.visible {
            return false;
        }
        if crate::widget::popovers::is_coordinate_covered(self as *const Self as *const () as usize, px, py) {
            return false;
        }
        let (rx, ry, rw, rh) = self.rect();
        px >= rx && px <= rx + rw && py >= ry && py <= ry + rh
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn visible(&self) -> bool {
        self.visible
    }

    fn set_spreadsheet_data(&mut self, headers: Vec<String>, rows: Vec<Vec<String>>) {
        self.headers = headers;
        self.rows = rows;
        
        // Clamp scroll_y to new bounds
        let content_h = self.rows.len() as f32 * 24.0;
        let visible_h = (self.h - 24.0).max(0.0);
        let max_scroll_y = (content_h - visible_h).max(0.0);
        self.scroll_y = self.scroll_y.clamp(0.0, max_scroll_y);
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32) -> bool {
        let was_hovered = self.hovered;
        self.hovered = self.hit_test(px, py);

        let was_sb_hovered = self.scrollbar_hovered;
        let was_thumb_hovered = self.scrollbar_thumb_hovered;

        let content_h = self.rows.len() as f32 * 24.0;
        let visible_h = (self.h - 24.0).max(0.0);
        if visible_h > 0.0 && content_h > visible_h {
            let scrollbar_w = 6.0;
            let scrollbar_padding = 2.0;
            let scrollbar_x = self.x + self.w - scrollbar_w - scrollbar_padding;
            let track_y = self.y + 24.0;

            self.scrollbar_hovered = px >= scrollbar_x - 2.0 && px <= self.x + self.w
                && py >= track_y && py <= self.y + self.h;

            let thumb_h = ((visible_h / content_h) * visible_h).clamp(15.0_f32.min(visible_h), visible_h);
            let max_scroll_y = content_h - visible_h;
            let scroll_ratio = self.scroll_y / max_scroll_y;
            let track_scroll_range = visible_h - thumb_h;
            let thumb_y = track_y + scroll_ratio * track_scroll_range;

            self.scrollbar_thumb_hovered = px >= scrollbar_x - 2.0 && px <= self.x + self.w
                && py >= thumb_y && py <= thumb_y + thumb_h;
        } else {
            self.scrollbar_hovered = false;
            self.scrollbar_thumb_hovered = false;
        }

        was_hovered != self.hovered
            || was_sb_hovered != self.scrollbar_hovered
            || was_thumb_hovered != self.scrollbar_thumb_hovered
    }

    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32) -> bool {
        if !self.visible {
            return false;
        }
        if self.hit_test(px, py) {
            let content_h = self.rows.len() as f32 * 24.0;
            let visible_h = (self.h - 24.0).max(0.0);
            if visible_h > 0.0 && content_h > visible_h {
                let scroll_amount = match delta {
                    MouseScrollDelta::LineDelta(_x, y) => *y * 24.0,
                    MouseScrollDelta::PixelDelta(pos) => pos.y as f32,
                };
                self.scroll_velocity += scroll_amount * 12.0;
                return true;
            }
        }
        false
    }

    fn draggable(&self) -> bool {
        if !self.visible {
            return false;
        }
        let content_h = self.rows.len() as f32 * 24.0;
        let visible_h = (self.h - 24.0).max(0.0);
        visible_h > 0.0 && content_h > visible_h
    }

    fn is_dragging(&self) -> bool {
        self.dragging_scrollbar
    }

    fn drag_begin(&mut self, px: f32, py: f32) {
        self.scroll_velocity = 0.0;
        let content_h = self.rows.len() as f32 * 24.0;
        let visible_h = (self.h - 24.0).max(0.0);
        if visible_h > 0.0 && content_h > visible_h {
            let scrollbar_w = 6.0;
            let scrollbar_padding = 2.0;
            let scrollbar_x = self.x + self.w - scrollbar_w - scrollbar_padding;
            let track_y = self.y + 24.0;

            if px >= scrollbar_x - 4.0 && px <= self.x + self.w
                && py >= track_y && py <= self.y + self.h
            {
                self.dragging_scrollbar = true;

                let thumb_h = ((visible_h / content_h) * visible_h).clamp(15.0_f32.min(visible_h), visible_h);
                let max_scroll_y = content_h - visible_h;
                let scroll_ratio = self.scroll_y / max_scroll_y;
                let track_scroll_range = visible_h - thumb_h;
                let thumb_y = track_y + scroll_ratio * track_scroll_range;

                if py >= thumb_y && py <= thumb_y + thumb_h {
                    self.drag_offset_y = py - thumb_y;
                } else {
                    self.drag_offset_y = thumb_h / 2.0;
                    let new_thumb_y = py - self.drag_offset_y;
                    let scroll_ratio = if track_scroll_range > 0.0 {
                        ((new_thumb_y - track_y) / track_scroll_range).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    self.scroll_y = scroll_ratio * max_scroll_y;
                }
            }
        }
    }

    fn drag_update(&mut self, _px: f32, py: f32) -> bool {
        if self.dragging_scrollbar {
            self.scroll_velocity = 0.0;
            let content_h = self.rows.len() as f32 * 24.0;
            let visible_h = (self.h - 24.0).max(0.0);
            if visible_h > 0.0 && content_h > visible_h {
                let thumb_h = ((visible_h / content_h) * visible_h).clamp(15.0_f32.min(visible_h), visible_h);
                let max_scroll_y = content_h - visible_h;
                let track_y = self.y + 24.0;
                let track_scroll_range = visible_h - thumb_h;

                let new_thumb_y = py - self.drag_offset_y;
                let scroll_ratio = if track_scroll_range > 0.0 {
                    ((new_thumb_y - track_y) / track_scroll_range).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let old_scroll_y = self.scroll_y;
                self.scroll_y = scroll_ratio * max_scroll_y;

                return (self.scroll_y - old_scroll_y).abs() > 0.01;
            }
        }
        false
    }

    fn drag_end(&mut self) {
        self.dragging_scrollbar = false;
        self.scroll_velocity = 0.0;
    }

    fn tick(&mut self, dt: f32) -> bool {
        if self.scroll_velocity.abs() > 0.01 {
            let content_h = self.rows.len() as f32 * 24.0;
            let visible_h = (self.h - 24.0).max(0.0);
            let max_scroll_y = (content_h - visible_h).max(0.0);
            let old_scroll_y = self.scroll_y;

            self.scroll_y = (self.scroll_y + self.scroll_velocity * dt).clamp(0.0, max_scroll_y);

            // Decelerate with friction (exponential decay)
            let friction = 8.0;
            self.scroll_velocity *= (-friction * dt).exp();

            if self.scroll_y == 0.0 || self.scroll_y == max_scroll_y {
                self.scroll_velocity = 0.0;
            }

            if self.scroll_velocity.abs() < 5.0 {
                self.scroll_velocity = 0.0;
            }

            (self.scroll_y - old_scroll_y).abs() > 0.01
        } else {
            false
        }
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.visible {
            return Vec::new();
        }
        let mut quads = Vec::new();
        
        // Header bg
        quads.push((self.x, self.y, self.w, 24.0, [0.12, 0.12, 0.16, 0.4]));

        // Zebra rows
        let row_h = 24.0;
        let body_top = self.y + 24.0;
        let body_bottom = self.y + self.h;
        for i in 0..self.rows.len() {
            let ry = self.y + 24.0 + i as f32 * row_h - self.scroll_y;
            if ry + row_h <= body_top || ry >= body_bottom {
                continue;
            }
            let draw_y = ry.max(body_top);
            let draw_h = (ry + row_h).min(body_bottom) - draw_y;
            if draw_h > 0.0 {
                let row_color = if i % 2 == 0 {
                    [0.10, 0.10, 0.13, 0.15]
                } else {
                    [0.08, 0.08, 0.11, 0.05]
                };
                quads.push((self.x, draw_y, self.w, draw_h, row_color));

                // Horizontal row separator
                let sep_y = ry + row_h;
                if sep_y >= body_top && sep_y < body_bottom {
                    quads.push((self.x, sep_y, self.w, 1.0, [0.20, 0.20, 0.25, 0.15]));
                }
            }
        }

        // Header separator
        quads.push((self.x, self.y + 24.0, self.w, 1.0, [0.20, 0.20, 0.25, 0.25]));

        // Vertical separators
        let divider_h = self.h;
        if divider_h > 0.0 && !self.headers.is_empty() {
            let n_cols = self.headers.len();
            for i in 1..n_cols {
                let r = i as f32 / n_cols as f32;
                quads.push((self.x + self.w * r, self.y, 1.0, divider_h, [0.20, 0.20, 0.25, 0.15]));
            }
        }

        // Scrollbar track & thumb
        let content_h = self.rows.len() as f32 * row_h;
        let visible_h = (self.h - 24.0).max(0.0);
        if visible_h > 0.0 && content_h > visible_h {
            let scrollbar_w = 6.0;
            let scrollbar_padding = 2.0;
            let scrollbar_x = self.x + self.w - scrollbar_w - scrollbar_padding;
            let track_y = self.y + 24.0;
            let track_h = visible_h;

            // Track BG
            quads.push((scrollbar_x, track_y, scrollbar_w, track_h, [0.05, 0.05, 0.08, 0.15]));

            // Thumb
            let thumb_h = ((visible_h / content_h) * visible_h).clamp(15.0_f32.min(visible_h), visible_h);
            let max_scroll_y = content_h - visible_h;
            let scroll_ratio = self.scroll_y / max_scroll_y;
            let track_scroll_range = visible_h - thumb_h;
            let thumb_y = track_y + scroll_ratio * track_scroll_range;

            let thumb_color = if self.dragging_scrollbar {
                [0.40, 0.40, 0.48, 1.0]
            } else if self.scrollbar_thumb_hovered {
                [0.32, 0.32, 0.38, 1.0]
            } else if self.scrollbar_hovered {
                [0.24, 0.24, 0.30, 0.9]
            } else {
                [0.18, 0.18, 0.24, 0.7]
            };

            quads.push((scrollbar_x, thumb_y, scrollbar_w, thumb_h, thumb_color));
        }

        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        if !self.visible {
            return Vec::new();
        }
        let mut labels = Vec::new();
        if self.headers.is_empty() {
            return labels;
        }

        let n_cols = self.headers.len();
        for (i, header) in self.headers.iter().enumerate() {
            let cx = self.x + self.w * (i as f32 / n_cols as f32) + 8.0;
            labels.push(TextLabel {
                text: header.clone(),
                x: cx,
                y: self.y + 6.0,
                font_size: 12.0,
                color: [0xdd, 0xdd, 0xee],
            });
        }

        let row_h = 24.0;
        let body_top = self.y + 24.0;
        let body_bottom = self.y + self.h;
        for (i, row) in self.rows.iter().enumerate() {
            let ry = self.y + 24.0 + i as f32 * row_h - self.scroll_y;
            // Only show text if the row is fully inside the spreadsheet body
            if ry < body_top || ry + row_h > body_bottom {
                continue;
            }

            for (col_idx, val) in row.iter().enumerate().take(n_cols) {
                let cx = self.x + self.w * (col_idx as f32 / n_cols as f32) + 8.0;
                labels.push(TextLabel {
                    text: val.clone(),
                    x: cx,
                    y: ry + 6.0,
                    font_size: 12.0,
                    color: [0xbb, 0xbb, 0xcc],
                });
            }
        }
        labels
    }
}

#[derive(Debug, Clone)]
pub struct ScrollBox {
    x: f32, y: f32, w: f32, h: f32,
    pub scroll_y: f32,
    pub content_h: f32,
    pub viewport_y: f32,
    pub viewport_h: f32,
    hovered: bool,
    pub show_border: bool,
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub children: Vec<*mut (dyn Element + 'static)>,
}

impl ScrollBox {
    pub fn new() -> Self {
        Self {
            x: 0.0, y: 0.0, w: 0.0, h: 0.0,
            scroll_y: 0.0,
            content_h: 0.0,
            viewport_y: 0.0,
            viewport_h: 0.0,
            hovered: false,
            show_border: true,
            parent: None,
            children: Vec::new(),
        }
    }

    pub fn update_bounds(&mut self, content_h: f32, viewport_y: f32, viewport_h: f32) {
        self.content_h = content_h;
        self.viewport_y = viewport_y;
        self.viewport_h = viewport_h;
        let max_scroll = (content_h - viewport_h).max(0.0);
        self.scroll_y = self.scroll_y.clamp(0.0, max_scroll);
    }

    pub fn get_item_draw_y(&self, virtual_y: f32, item_h: f32) -> Option<f32> {
        let draw_y = self.viewport_y + virtual_y - self.scroll_y;
        if draw_y >= self.viewport_y - 1.0 && draw_y + item_h <= self.viewport_y + self.viewport_h + 1.0 {
            Some(draw_y)
        } else {
            None
        }
    }
}

impl Element for ScrollBox {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { [0.08, 0.08, 0.12, 0.3] }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }
    fn highlight_color(&self) -> Option<[f32; 4]> { None }

    fn focus(&mut self) {
        focus::set_focused(self);
    }
    fn unfocus(&mut self) {}

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button == MouseButton::Left && state == ElementState::Pressed {
            if self.hit_test(px, py) {
                self.focus();
                return true;
            }
        }
        false
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32) -> bool {
        let was = self.hovered;
        self.hovered = self.hit_test(px, py);
        was != self.hovered
    }

    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32) -> bool {
        if self.hit_test(px, py) {
            let scroll_speed = 24.0;
            let dy = match delta {
                MouseScrollDelta::LineDelta(_, y) => -y * scroll_speed,
                MouseScrollDelta::PixelDelta(pos) => -pos.y as f32,
            };
            let old_scroll = self.scroll_y;
            let max_scroll = (self.content_h - self.viewport_h).max(0.0);
            self.scroll_y = (self.scroll_y + dy).clamp(0.0, max_scroll);
            (self.scroll_y - old_scroll).abs() > 0.01
        } else {
            false
        }
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        
        // Background
        quads.push((self.x, self.y, self.w, self.h, [0.08, 0.08, 0.12, 0.3]));

        // Border lines
        let box_border_color = if focus::is_focused(self) {
            [0.30, 0.50, 0.32, 1.0] // Focused green
        } else if self.hovered {
            [0.25, 0.25, 0.35, 1.0] // Hovered
        } else {
            [0.18, 0.18, 0.24, 1.0] // Default
        };
        quads.push((self.x, self.y, self.w, 1.0, box_border_color)); // Top
        quads.push((self.x, self.y + self.h - 1.0, self.w, 1.0, box_border_color)); // Bottom
        quads.push((self.x, self.y, 1.0, self.h, box_border_color)); // Left
        quads.push((self.x + self.w - 1.0, self.y, 1.0, self.h, box_border_color)); // Right

        // Scrollbar
        if self.content_h > self.viewport_h {
            let sb_x = self.x + self.w - 8.0;
            let sb_w = 4.0;
            let sb_track_h = self.viewport_h - 8.0;
            let sb_track_y = self.viewport_y + 4.0;

            // Track
            quads.push((sb_x, sb_track_y, sb_w, sb_track_h, [0.15, 0.15, 0.20, 0.3]));

            // Thumb
            let visible_ratio = self.viewport_h / self.content_h;
            let thumb_h = (sb_track_h * visible_ratio).clamp(20.0, sb_track_h);
            let max_scroll = (self.content_h - self.viewport_h).max(0.0);
            let scroll_ratio = if max_scroll > 0.0 { self.scroll_y / max_scroll } else { 0.0 };
            let thumb_y = sb_track_y + scroll_ratio * (sb_track_h - thumb_h);

            quads.push((sb_x, thumb_y, sb_w, thumb_h, [0.60, 0.60, 0.65, 0.4]));
        }

        quads
    }

    fn keyboard_input(&mut self, event: &KeyEvent) -> bool {
        if !focus::is_focused(self) {
            return false;
        }
        if event.state != ElementState::Pressed {
            return false;
        }
        if event.ctrl {
            match &event.logical_key {
                Key::Character(c) if c == "n" || c == "N" => {
                    let old_scroll = self.scroll_y;
                    let max_scroll = (self.content_h - self.viewport_h).max(0.0);
                    self.scroll_y = (self.scroll_y + 24.0).clamp(0.0, max_scroll);
                    (self.scroll_y - old_scroll).abs() > 0.01
                }
                Key::Character(c) if c == "p" || c == "P" => {
                    let old_scroll = self.scroll_y;
                    let max_scroll = (self.content_h - self.viewport_h).max(0.0);
                    self.scroll_y = (self.scroll_y - 24.0).clamp(0.0, max_scroll);
                    (self.scroll_y - old_scroll).abs() > 0.01
                }
                _ => false,
            }
        } else {
            match &event.logical_key {
                Key::Named(NamedKey::ArrowDown) => {
                    let old_scroll = self.scroll_y;
                    let max_scroll = (self.content_h - self.viewport_h).max(0.0);
                    self.scroll_y = (self.scroll_y + 24.0).clamp(0.0, max_scroll);
                    (self.scroll_y - old_scroll).abs() > 0.01
                }
                Key::Named(NamedKey::ArrowUp) => {
                    let old_scroll = self.scroll_y;
                    let max_scroll = (self.content_h - self.viewport_h).max(0.0);
                    self.scroll_y = (self.scroll_y - 24.0).clamp(0.0, max_scroll);
                    (self.scroll_y - old_scroll).abs() > 0.01
                }
                _ => false,
            }
        }
    }

    fn parent(&self) -> Option<*mut (dyn Element + 'static)> { self.parent }
    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>) { self.parent = parent; }
    fn children(&self) -> Vec<*mut (dyn Element + 'static)> { self.children.clone() }
    fn add_child(&mut self, child: *mut (dyn Element + 'static)) { self.children.push(child); }
    fn clear_children(&mut self) { self.children.clear(); }
}

impl Drop for ScrollBox {
    fn drop(&mut self) {
        focus::clear_if_matches(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu_item_x(title: &str, menu_items: &[String], idx: usize) -> f32 {
        let mut x = 8.0;
        if !title.is_empty() {
            x += title.len() as f32 * 7.5 + 24.0;
        }
        for i in 0..idx {
            x += menu_items[i].len() as f32 * 7.5 + 16.0;
        }
        x
    }

    fn menu_item_w(menu_items: &[String], idx: usize) -> f32 {
        menu_items[idx].len() as f32 * 7.5 + 16.0
    }

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
    fn test_node_toggle_geometry_visibility() {
        let mut node = Node::new(100.0, 100.0, 200.0, 50.0, "Test Node");

        // 1. Initial state
        assert!(node.geom_visible());
        assert!(!node.take_geom_toggle());
        assert!(node.draggable());

        // Get the toggle rect
        let (tx, ty, tw, th) = node.toggle_rect();
        
        // 2. Hover toggle area
        // Move cursor inside toggle area
        let changed = node.cursor_moved(tx + tw / 2.0, ty + th / 2.0);
        assert!(changed);
        assert!(node.toggle_hovered);
        assert!(!node.draggable(), "Node should not be draggable when hovering over the toggle widget");

        // Move cursor outside toggle area but inside node
        let changed2 = node.cursor_moved(tx - 10.0, ty + th / 2.0);
        assert!(changed2);
        assert!(!node.toggle_hovered);
        assert!(node.draggable());

        // 3. Click toggle area
        // Move cursor back inside toggle area
        node.cursor_moved(tx + tw / 2.0, ty + th / 2.0);
        // Press Left button
        let input_changed = node.mouse_input(MouseButton::Left, ElementState::Pressed, tx + tw / 2.0, ty + th / 2.0);
        assert!(input_changed);
        assert!(!node.geom_visible(), "Geometry visibility should be toggled off");
        assert!(node.take_geom_toggle(), "take_geom_toggle should return true after toggle click");
        assert!(!node.take_geom_toggle(), "take_geom_toggle should clear state after being called once");

        // Click again to toggle back on
        let input_changed2 = node.mouse_input(MouseButton::Left, ElementState::Pressed, tx + tw / 2.0, ty + th / 2.0);
        assert!(input_changed2);
        assert!(node.geom_visible(), "Geometry visibility should be toggled back on");
        assert!(node.take_geom_toggle());
    }

    #[test]
    fn test_graph_interaction() {
        let mut graph = Graph::new();
        graph.set_rect(0.0, 0.0, 800.0, 600.0);
        graph.set_grid_sizes(100.0, 50.0);
        graph.set_skipped_sizes(10.0, 20.0);
        graph.set_grid_origin(0.0, 0.0);

        let nodes = vec![
            GraphNode {
                name: "Node A".to_string(),
                position: (0.0, 0.0),
                parameters: vec![],
                geom_visible: true,
            },
            GraphNode {
                name: "Node B".to_string(),
                position: (2.0, 1.0),
                parameters: vec![],
                geom_visible: true,
            },
        ];
        graph.set_nodes(&nodes);

        // 1. Initial State
        assert_eq!(graph.get_nodes().len(), 2);
        assert_eq!(graph.selected_node(), None);

        // 2. Select Node A
        // Node A screen rect: (0, 0, 100, 50)
        let clicked = graph.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 25.0);
        assert!(clicked);
        assert_eq!(graph.selected_node(), Some(0));
        assert!(graph.draggable());

        // 3. Drag Node A
        graph.drag_begin(50.0, 25.0);
        assert!(graph.is_dragging());

        // Enable snapping
        graph.set_grid_snap_enabled(true);
        graph.drag_update(170.0, 85.0); // drag offset from Node A center (50, 25): nx = 170 - 50 = 120, ny = 85 - 25 = 60
        assert_eq!(graph.drag_node_pos, Some((120.0, 60.0)));

        graph.drag_end();
        assert_eq!(graph.get_nodes()[0].position, (1.0, 1.0)); // Snapped grid position: (120/120, 60/60)

        // 4. Toggle geometry visibility of Node B
        // Node B screen rect: (2 * 120 = 240, 1 * 60 = 60, 100, 50)
        // Toggle button: tx = 240 + 100 - 30 = 310, ty = 60 + (50 - 18)/2 = 76, tw = 18, th = 18
        let clicked_toggle = graph.mouse_input(MouseButton::Left, ElementState::Pressed, 319.0, 85.0);
        assert!(clicked_toggle);
        assert_eq!(graph.take_node_geom_toggle(), Some((1, false)));
    }


    #[test]
    fn test_menubar_vertical_horizontal_labels() {
        // Create MenuBar with custom horizontal and vertical labels
        let mut menubar = MenuBar::new(0.0, 0.0, 120.0, 30.0)
            .with_title("App")
            .with_item_vh("FileH", "FileV", &["Open", "Save"])
            .with_item("Edit", &["Undo"]);

        // 1. Horizontal mode (default)
        assert!(!menubar.vertical);
        let labels_h = menubar.text_labels();
        // Title should be "App", item 0 should be "FileH", item 1 should be "Edit"
        assert_eq!(labels_h[0].text, "App");
        assert_eq!(labels_h[1].text, "FileH");
        assert_eq!(labels_h[2].text, "Edit");

        // Hover test in horizontal layout
        let ix = menu_item_x("App", &["FileH".to_string(), "Edit".to_string()], 0);
        let iw = menu_item_w(&["FileH".to_string(), "Edit".to_string()], 0);
        
        // Move cursor inside "FileH" bounds
        menubar.cursor_moved(ix + iw / 2.0, 15.0);
        assert_eq!(menubar.hovered_menu, Some(0));

        // 2. Vertical mode
        let mut menubar_v = menubar.with_vertical(true);
        assert!(menubar_v.vertical);
        let labels_v = menubar_v.text_labels();
        // Title should be "App", item 0 should be "FileV", item 1 should be "Edit"
        assert_eq!(labels_v[0].text, "App");
        assert_eq!(labels_v[1].text, "FileV");
        assert_eq!(labels_v[2].text, "Edit");

        // Hover test in vertical layout
        let iy = menubar_v.item_y_vertical(0);
        let ih = menubar_v.item_h_vertical();

        // Move cursor inside "FileV" bounds
        menubar_v.cursor_moved(20.0, iy + ih / 2.0);
        assert_eq!(menubar_v.hovered_menu, Some(0));
    }

    #[test]
    fn test_spreadsheet_dynamic() {
        let mut spreadsheet = Spreadsheet::new();
        spreadsheet.set_rect(10.0, 10.0, 100.0, 200.0);
        spreadsheet.set_visible(true);

        // Initially empty
        assert!(spreadsheet.headers.is_empty());
        assert!(spreadsheet.rows.is_empty());
        assert!(spreadsheet.text_labels().is_empty());

        // Set dynamic headers and rows
        let headers = vec!["ColA".to_string(), "ColB".to_string()];
        let rows = vec![
            vec!["Val1".to_string(), "Val2".to_string()],
            vec!["Val3".to_string(), "Val4".to_string()],
        ];
        spreadsheet.set_spreadsheet_data(headers, rows);

        assert_eq!(spreadsheet.headers.len(), 2);
        assert_eq!(spreadsheet.rows.len(), 2);

        // Verify labels generated
        let labels = spreadsheet.text_labels();
        // 2 headers + 4 cell values = 6 labels total
        assert_eq!(labels.len(), 6);
        assert_eq!(labels[0].text, "ColA");
        assert_eq!(labels[1].text, "ColB");
        assert_eq!(labels[2].text, "Val1");
        assert_eq!(labels[3].text, "Val2");
        assert_eq!(labels[4].text, "Val3");
        assert_eq!(labels[5].text, "Val4");

        // Verify positions are correct (col 0 starts at x = 10.0 + 8.0 = 18.0)
        assert_eq!(labels[0].x, 18.0);
        // Col 1 starts at x = 10.0 + 100.0 * 0.5 + 8.0 = 68.0
        assert_eq!(labels[1].x, 68.0);
        assert_eq!(labels[2].x, 18.0);
        assert_eq!(labels[3].x, 68.0);
    }

    #[test]
    fn test_spreadsheet_scrolling() {
        let mut spreadsheet = Spreadsheet::new();
        // Visible height is 100px. Header is 24px, so body is 76px.
        spreadsheet.set_rect(0.0, 0.0, 100.0, 100.0);
        spreadsheet.set_visible(true);

        let headers = vec!["ColA".to_string()];
        // Each row is 24px. With 10 rows, content_h = 240px.
        let mut rows = Vec::new();
        for i in 0..10 {
            rows.push(vec![format!("Row{}", i)]);
        }
        spreadsheet.set_spreadsheet_data(headers, rows);

        // Content height is 240px, visible height is 100px (body is 76px).
        // Since content height > visible body height, it should be draggable.
        assert!(spreadsheet.draggable());

        // Max scroll height = 240.0 - 76.0 = 164.0
        
        // Initial scroll position should be 0.0
        assert_eq!(spreadsheet.scroll_y, 0.0);

        // Scroll down via mouse wheel (positive delta scrolls content down, scroll_y increases via tick)
        let delta = MouseScrollDelta::LineDelta(0.0, 2.0);
        // Mouse over spreadsheet (50, 50)
        let changed = spreadsheet.mouse_wheel(&delta, 50.0, 50.0);
        assert!(changed);
        assert_eq!(spreadsheet.scroll_y, 0.0);
        assert!(spreadsheet.scroll_velocity > 0.0);

        // Tick to apply velocity
        let mut ticked_change = false;
        for _ in 0..100 {
            if spreadsheet.tick(0.016) {
                ticked_change = true;
            }
        }
        assert!(ticked_change);
        assert!(spreadsheet.scroll_y > 0.0);
        assert_eq!(spreadsheet.scroll_velocity, 0.0);

        // Scroll back to top
        let delta_up = MouseScrollDelta::LineDelta(0.0, -10.0);
        spreadsheet.mouse_wheel(&delta_up, 50.0, 50.0);
        assert!(spreadsheet.scroll_velocity < 0.0);

        // Tick back to top
        for _ in 0..100 {
            spreadsheet.tick(0.016);
        }
        assert_eq!(spreadsheet.scroll_y, 0.0);
        assert_eq!(spreadsheet.scroll_velocity, 0.0);

        // Drag test
        // Scrollbar width is 6px. Padding is 2px. Width is 100px.
        // Scrollbar track x is from 92px to 98px.
        // Let's drag. Click at (94, 50).
        spreadsheet.drag_begin(94.0, 50.0);
        assert!(spreadsheet.is_dragging());

        // Update drag to y = 80
        let changed_drag = spreadsheet.drag_update(94.0, 80.0);
        assert!(changed_drag);
        assert!(spreadsheet.scroll_y > 0.0);

        // End drag
        spreadsheet.drag_end();
        assert!(!spreadsheet.is_dragging());
    }

    #[test]
    fn test_spreadsheet_zero_height_no_panic() {
        let mut spreadsheet = Spreadsheet::new();
        // Visible height is set to 0.0
        spreadsheet.set_rect(0.0, 0.0, 100.0, 0.0);
        spreadsheet.set_visible(true);

        let headers = vec!["ColA".to_string()];
        let mut rows = Vec::new();
        for i in 0..10 {
            rows.push(vec![format!("Row{}", i)]);
        }
        // This should not panic
        spreadsheet.set_spreadsheet_data(headers, rows);

        // This should not panic
        spreadsheet.cursor_moved(50.0, 50.0);
        
        let delta = MouseScrollDelta::LineDelta(0.0, -2.0);
        // This should not panic
        spreadsheet.mouse_wheel(&delta, 50.0, 50.0);

        // This should not panic
        assert!(!spreadsheet.draggable());

        // This should not panic
        spreadsheet.drag_begin(94.0, 50.0);
        spreadsheet.drag_update(94.0, 80.0);
        spreadsheet.drag_end();

        // This should not panic and return empty quads for scrollbar
        let _quads = spreadsheet.extra_quads();
        // The header quad and divider (if any) are drawn, but scrollbar is not
        // Let's verify that the scrollbar was not drawn
        // (the last quad would be the scrollbar thumb with thumb_color if drawn,
        // but here scrollbar track & thumb shouldn't be added)
        assert_eq!(spreadsheet.scroll_y, 0.0);
        
        // Let's check text labels (should be empty because self.h is 0)
        let labels = spreadsheet.text_labels();
        // Headers labels are still generated since they don't depend on scroll/height,
        // but rows shouldn't be
        assert_eq!(labels.len(), 1); // Only header ColA
    }

    #[test]
    fn test_scroll_box_bounds_scrolling() {
        let mut sb = ScrollBox::new();
        sb.set_rect(10.0, 20.0, 100.0, 100.0);
        
        // 1. Initially scroll is 0
        assert_eq!(sb.scroll_y, 0.0);

        // 2. Update bounds: content_h = 150 (greater than viewport_h = 100)
        sb.update_bounds(150.0, 20.0, 100.0);
        assert_eq!(sb.scroll_y, 0.0);
        assert_eq!(sb.content_h, 150.0);
        assert_eq!(sb.viewport_h, 100.0);

        // 3. Scroll inside bounds
        let delta = MouseScrollDelta::LineDelta(0.0, -2.0); // scroll down by 2 lines (48px)
        let changed = sb.mouse_wheel(&delta, 50.0, 50.0);
        assert!(changed);
        assert_eq!(sb.scroll_y, 48.0);

        // 4. Clamps at max scroll: 150 - 100 = 50
        let delta_large = MouseScrollDelta::LineDelta(0.0, -10.0);
        sb.mouse_wheel(&delta_large, 50.0, 50.0);
        assert_eq!(sb.scroll_y, 50.0);

        // 5. Test item draw coordinates
        // Virtual item at virtual_y = 10, item_h = 24
        // Screen draw y = viewport_y + virtual_y - scroll_y = 20 + 10 - 50 = -20
        // -20 < viewport_y + 2.0 (22.0), so it should return None (not visible)
        assert!(sb.get_item_draw_y(10.0, 24.0).is_none());

        // Virtual item at virtual_y = 60, item_h = 24
        // Screen draw y = 20 + 60 - 50 = 30
        // 30 >= 22.0 and 30 + 24 <= 118.0, so it should return Some(30.0)
        assert_eq!(sb.get_item_draw_y(60.0, 24.0), Some(30.0));
    }

    #[test]
    fn test_dropdown_widget_interaction() {
        let options = vec!["Option A".to_string(), "Option B".to_string(), "Option C".to_string()];
        let mut dd = Dropdown::new(options, 0);
        dd.set_rect(10.0, 10.0, 100.0, 24.0);

        // 1. Initial State
        assert!(!dd.open);
        assert_eq!(dd.selected, 0);

        // 2. Click trigger area opens dropdown
        let input_changed = dd.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 20.0);
        assert!(input_changed);
        assert!(dd.open);

        // 3. Hovering options inside popover
        // Popover starts at y = 10 + 24 = 34. Options are of height 24 each.
        // Hover option B at y = 34 + 24 + 12 = 70.0
        let move_changed = dd.cursor_moved(50.0, 70.0);
        assert!(move_changed);
        assert_eq!(dd.hovered_item, Some(1));

        // 4. Click option B selects it and closes dropdown
        let select_changed = dd.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 70.0);
        assert!(select_changed);
        assert!(!dd.open);
        assert_eq!(dd.selected, 1);
        assert!(dd.take_change());
    }

    #[test]
    fn test_textbox_selection_highlight() {
        let mut tb = TextBox::new("Initial Text".to_string());
        tb.set_rect(10.0, 10.0, 200.0, 30.0);

        // 1. Initial state
        assert!(!tb.editing);
        assert!(!tb.all_selected);

        // 2. Click focuses and triggers highlighting
        let clicked = tb.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 20.0);
        assert!(clicked);
        assert!(tb.editing);
        assert!(tb.all_selected);
        assert_eq!(tb.edit_buffer, "Initial Text");

        // 3. Typing a key replaces all text
        let key_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Character("A".to_string()),
            text: Some("A".to_string()),
            repeat: false,
            ctrl: false,
            shift: false,
        };
        let handled = tb.keyboard_input(&key_ev);
        assert!(handled);
        assert!(!tb.all_selected);
        assert_eq!(tb.edit_buffer, "A");

        // 4. Pressing Enter commits change
        let enter_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::Enter),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
        };
        let handled_enter = tb.keyboard_input(&enter_ev);
        assert!(handled_enter);
        assert!(!tb.editing);
        assert_eq!(tb.text, "A");
        assert!(tb.take_change());
    }

    #[test]
    fn test_textbox_drag_and_modifier_selection() {
        let mut tb = TextBox::new("Hello World".to_string());
        tb.set_rect(10.0, 10.0, 200.0, 30.0);

        // 1. Initial click focuses and selects all
        let pressed = tb.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 20.0);
        assert!(pressed);
        let released = tb.mouse_input(MouseButton::Left, ElementState::Released, 50.0, 20.0);
        assert!(released);
        assert!(tb.editing);
        assert!(tb.all_selected);
        assert_eq!(tb.cursor_idx, 11);
        assert_eq!(tb.select_anchor, Some(0));

        // 2. Click inside placed caret at index 5 (x = 10 + 8 + 5 * 7.2 = 54)
        let pressed_inside = tb.mouse_input(MouseButton::Left, ElementState::Pressed, 54.0, 20.0);
        assert!(pressed_inside);
        assert_eq!(tb.cursor_idx, 5);
        assert_eq!(tb.select_anchor, Some(5));
        assert!(!tb.all_selected);

        // 3. Drag to index 11 (x = 10 + 8 + 11 * 7.2 = 97.2)
        tb.drag_begin(54.0, 20.0);
        let updated = tb.drag_update(97.2, 20.0);
        assert!(updated);
        assert_eq!(tb.cursor_idx, 11);
        assert_eq!(tb.select_anchor, Some(5));
        tb.drag_end();

        // 4. Keyboard ArrowLeft with Shift shrinks selection from 11 to 10
        let left_shift_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::ArrowLeft),
            text: None,
            repeat: false,
            ctrl: false,
            shift: true,
        };
        let handled = tb.keyboard_input(&left_shift_ev);
        assert!(handled);
        assert_eq!(tb.cursor_idx, 10);
        assert_eq!(tb.select_anchor, Some(5));

        // 5. Keyboard ArrowLeft without Shift collapses selection to start (index 5)
        let left_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::ArrowLeft),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
        };
        let handled = tb.keyboard_input(&left_ev);
        assert!(handled);
        assert_eq!(tb.cursor_idx, 5);
        assert_eq!(tb.select_anchor, None);

        // 6. Keyboard Shift+Up highlights to beginning (cursor 0, anchor 5)
        let up_shift_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::ArrowUp),
            text: None,
            repeat: false,
            ctrl: false,
            shift: true,
        };
        let handled = tb.keyboard_input(&up_shift_ev);
        assert!(handled);
        assert_eq!(tb.cursor_idx, 0);
        assert_eq!(tb.select_anchor, Some(5));

        // 7. Typing a key replaces selected range "Hello" with "Rust"
        let rust_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Character("Rust".to_string()),
            text: Some("Rust".to_string()),
            repeat: false,
            ctrl: false,
            shift: false,
        };
        let handled = tb.keyboard_input(&rust_ev);
        assert!(handled);
        assert_eq!(tb.edit_buffer, "Rust World");
        assert_eq!(tb.cursor_idx, 4);
        assert_eq!(tb.select_anchor, None);
    }

    #[test]
    fn test_paginator_rotated_tabs() {
        let pages = vec!["📁 Browse".to_string(), "🌐 Network".to_string()];
        let mut paginator = Paginator::new(56.0, pages)
            .with_tab_y_offset(100.0)
            .with_tabs_rotated(true);
        
        paginator.set_rect(0.0, 0.0, 1000.0, 600.0);

        // Verify target_y is updated correctly
        // selected_page = 0: tab_y_offset = 100.0
        assert_eq!(paginator.target_y, 100.0);

        paginator.set_selected_page(1);
        // selected_page = 1: tab_y_offset + 1 * (120.0 + 10.0) = 230.0
        assert_eq!(paginator.target_y, 230.0);

        // Verify hover coordinates
        // Tab 0 bx/by bounds:
        // tab_w = 32.0, tab_h = 120.0, spacing = 10.0, sidebar_w = 48.0
        // bx = self.x + (sidebar_w - tab_w) / 2 = 8.0
        // by = self.y + tab_y_offset + i * 130.0 = 100.0
        // bx range: [8.0, 40.0], by range: [100.0, 220.0]
        
        // Hover at (24.0, 150.0) should hit Tab 0
        let hover_tab0 = paginator.on_cursor_moved(24.0, 150.0);
        assert!(hover_tab0);
        assert_eq!(paginator.hovered_tab, Some(0));

        // Hover at (24.0, 280.0) should hit Tab 1 (by range: [230.0, 350.0])
        let hover_tab1 = paginator.on_cursor_moved(24.0, 280.0);
        assert!(hover_tab1);
        assert_eq!(paginator.hovered_tab, Some(1));

        // Clicking Tab 0 selects it
        let click_tab0 = paginator.mouse_input(MouseButton::Left, ElementState::Pressed, 24.0, 150.0);
        assert!(click_tab0);
        assert_eq!(paginator.pressed_tab, Some(0));

        let release_tab0 = paginator.mouse_input(MouseButton::Left, ElementState::Released, 24.0, 150.0);
        assert!(release_tab0);
        assert_eq!(paginator.selected_page, 0);

        // Check vertical text formatting
        let labels = paginator.text_labels();
        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0].text, "📁");
        assert_eq!(labels[1].text, "🌐");
        
        // Check that rotated text quads were generated
        assert!(!paginator.tab_text_quads[0].is_empty());
        assert!(!paginator.tab_text_quads[1].is_empty());
    }

    #[test]
    fn test_svg_text_rendering() {
        let svg_data = r##"<svg width="32" height="120" xmlns="http://www.w3.org/2000/svg">
  <text x="16" y="60" font-family="sans-serif" font-size="12" fill="#E6E6F2" text-anchor="middle" dominant-baseline="middle" transform="rotate(-90 16 60)">Audio</text>
</svg>"##.as_bytes();

        let opt = resvg::usvg::Options::default();
        let mut fontdb = resvg::usvg::fontdb::Database::new();
        fontdb.load_system_fonts();
        let tree = resvg::usvg::Tree::from_data(svg_data, &opt, &fontdb).unwrap();
        
        let mut pixmap = resvg::tiny_skia::Pixmap::new(32, 120).unwrap();
        resvg::render(&tree, resvg::tiny_skia::Transform::default(), &mut pixmap.as_mut());
        
        pixmap.save_png("/home/lsgalante/Dropbox/Clear/scratch/test_svg.png").unwrap();

        // Check that some pixels were drawn (are non-transparent)
        let pixels = pixmap.data();
        let mut non_transparent = 0;
        for i in (3..pixels.len()).step_by(4) {
            if pixels[i] > 0 {
                non_transparent += 1;
            }
        }
        assert!(non_transparent > 0, "Should have rendered some text pixels");
    }
}

// Generic text item layout wrapper
#[derive(Debug, Clone)]
pub struct ScrollingList {
    pub scroll_box: ScrollBox,
    pub item_height: f32,
    pub item_gap: f32,
}

impl ScrollingList {
    pub fn new(item_height: f32, item_gap: f32) -> Self {
        Self {
            scroll_box: ScrollBox::new(),
            item_height,
            item_gap,
        }
    }

    pub fn update_bounds(&mut self, count: usize, viewport_y: f32, viewport_h: f32) {
        let item_height_full = self.item_height + self.item_gap;
        let content_h = count as f32 * item_height_full;
        self.scroll_box.update_bounds(content_h, viewport_y, viewport_h);
    }

    pub fn get_item_draw_y(&self, idx: usize, offset: f32) -> Option<f32> {
        let item_height_full = self.item_height + self.item_gap;
        let virtual_y = idx as f32 * item_height_full + offset;
        self.scroll_box.get_item_draw_y(virtual_y, self.item_height)
    }

    pub fn scroll_y(&self) -> f32 {
        self.scroll_box.scroll_y
    }

    pub fn set_scroll_y(&mut self, val: f32) {
        self.scroll_box.scroll_y = val;
    }
}

impl Default for ScrollingList {
    fn default() -> Self {
        Self::new(24.0, 4.0)
    }
}

impl Element for ScrollingList {
    fn rect(&self) -> (f32, f32, f32, f32) {
        self.scroll_box.rect()
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.scroll_box.set_rect(x, y, w, h);
    }

    fn color(&self) -> [f32; 4] {
        self.scroll_box.color()
    }

    fn set_hovered(&mut self, v: bool) {
        self.scroll_box.set_hovered(v);
    }

    fn hovered(&self) -> bool {
        self.scroll_box.hovered()
    }

    fn highlight_color(&self) -> Option<[f32; 4]> {
        self.scroll_box.highlight_color()
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32) -> bool {
        self.scroll_box.cursor_moved(px, py)
    }

    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32) -> bool {
        self.scroll_box.mouse_wheel(delta, px, py)
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        self.scroll_box.mouse_input(button, state, px, py)
    }

    fn focus(&mut self) {
        self.scroll_box.focus();
    }

    fn unfocus(&mut self) {
        self.scroll_box.unfocus();
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        self.scroll_box.extra_quads()
    }

    fn keyboard_input(&mut self, event: &KeyEvent) -> bool {
        self.scroll_box.keyboard_input(event)
    }

    fn parent(&self) -> Option<*mut (dyn Element + 'static)> { self.scroll_box.parent() }
    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>) { self.scroll_box.set_parent(parent); }
    fn children(&self) -> Vec<*mut (dyn Element + 'static)> { self.scroll_box.children() }
    fn add_child(&mut self, child: *mut (dyn Element + 'static)) { self.scroll_box.add_child(child); }
    fn clear_children(&mut self) { self.scroll_box.clear_children(); }

    fn update_bounds(&mut self, count: usize, viewport_y: f32, viewport_h: f32) {
        self.update_bounds(count, viewport_y, viewport_h);
    }

    fn get_item_draw_y(&self, idx: usize, offset: f32) -> Option<f32> {
        self.get_item_draw_y(idx, offset)
    }
}

unsafe impl Send for Container {}
unsafe impl Sync for Container {}
unsafe impl Send for ScrollBox {}
unsafe impl Sync for ScrollBox {}
unsafe impl Send for Spinbox {}
unsafe impl Sync for Spinbox {}
unsafe impl Send for ColorSelector {}
unsafe impl Sync for ColorSelector {}
unsafe impl Send for ScrollingList {}
unsafe impl Sync for ScrollingList {}

// ── Dropdown Widget ──

#[derive(Debug, Clone)]
pub struct Plate {
    pub base: Widget,
    pub dragging: bool,
    pub drag_ox: f32,
    pub drag_oy: f32,
    pub drag_start_x: f32,
    pub drag_start_y: f32,
    pub bounds: Option<(f32, f32, f32, f32)>,
    pub color: Option<[f32; 4]>,
    pub curved_circle: Option<(f32, f32, f32)>,
    pub network_opacity: f32,
    pub blur: bool,
    pub children: Vec<*mut (dyn Element + 'static)>,
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub visible: bool,
    pub column_layout: bool,
}

impl Plate {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            base: Widget::new_rect(x, y, w, h),
            dragging: false,
            drag_ox: 0.0,
            drag_oy: 0.0,
            drag_start_x: 0.0,
            drag_start_y: 0.0,
            bounds: None,
            color: None,
            curved_circle: None,
            network_opacity: 1.0,
            blur: true,
            children: Vec::new(),
            parent: None,
            visible: true,
            column_layout: false,
        }
    }

    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = Some(color);
        self
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self
    }

    pub fn with_blur(mut self, blur: bool) -> Self {
        self.blur = blur;
        self
    }

    pub fn set_bounds(&mut self, bx: f32, by: f32, bw: f32, bh: f32) {
        self.bounds = Some((bx, by, bw, bh));
    }
}

impl Element for Plate {
    crate::impl_widget_base!(Plate);
    fn is_plate(&self) -> bool { true }
    fn rounded_corners(&self) -> (bool, bool, bool, bool) { (true, true, true, true) }
    fn highlight_quad(&self) -> Option<(f32, f32, f32, f32, [f32; 4])> { None }

    fn set_modifiers(&mut self, ctrl: bool, shift: bool, alt: bool) {
        for &child_ptr in &self.children {
            unsafe {
                (*child_ptr).set_modifiers(ctrl, shift, alt);
            }
        }
    }

    fn visible(&self) -> bool {
        self.visible
    }

    fn color(&self) -> [f32; 4] {
        let mut c = if let Some(c) = self.color {
            c
        } else if self.dragging {
            colors::PANEL_DRAG
        } else {
            colors::PANEL_IDLE
        };
        c[3] *= self.network_opacity;
        if self.blur {
            c[3] = -c[3].abs();
        }
        c
    }

    fn set_network_opacity(&mut self, opacity: f32) {
        self.network_opacity = opacity;
    }

    fn set_drag_bounds(&mut self, bx: f32, by: f32, bw: f32, bh: f32) {
        self.bounds = Some((bx, by, bw, bh));
    }

    fn set_curved_circle(&mut self, circle: Option<(f32, f32, f32)>) {
        self.curved_circle = circle;
    }

    fn hit_test(&self, px: f32, py: f32) -> bool {
        if crate::widget::popovers::is_coordinate_covered(self as *const Self as *const () as usize, px, py) {
            return false;
        }
        if let Some((cx, cy, r)) = self.curved_circle {
            let dx = px - cx;
            let dy = py - cy;
            return dx * dx + dy * dy <= r * r;
        }
        
        let (x, y, w, h) = self.rect();
        if px < x || px >= x + w || py < y || py >= y + h {
            return false;
        }
        
        let r = 12.0f32.min(w * 0.5).min(h * 0.5);
        if r <= 0.1 {
            return true;
        }
        
        // Check corners
        if px < x + r && py < y + r {
            let dx = px - (x + r);
            let dy = py - (y + r);
            return dx * dx + dy * dy <= r * r;
        }
        if px >= x + w - r && py < y + r {
            let dx = px - (x + w - r);
            let dy = py - (y + r);
            return dx * dx + dy * dy <= r * r;
        }
        if px >= x + w - r && py >= y + h - r {
            let dx = px - (x + w - r);
            let dy = py - (y + h - r);
            return dx * dx + dy * dy <= r * r;
        }
        if px < x + r && py >= y + h - r {
            let dx = px - (x + r);
            let dy = py - (y + h - r);
            return dx * dx + dy * dy <= r * r;
        }
        
        true
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if let Some(b) = self.base_mut() {
            b.x = x;
            b.y = y;
            b.w = w;
            b.h = h;
        }

        if !self.visible {
            return;
        }

        let padding_x = 20.0;
        let padding_y = 20.0;
        let left_x = x + padding_x;
        let available_w = (w - 2.0 * padding_x).max(1.0);
        let start_y = y + padding_y;
        let available_h = (h - 2.0 * padding_y).max(1.0);

        let center_x = left_x + available_w / 2.0;
        let center_y = start_y + available_h / 2.0;
        let aspect_ratio = available_w / available_h;

        let mut active_widgets = Vec::new();
        for &w_ptr in &self.children {
            let w = unsafe { &*w_ptr };
            if !w.layout_ignore() {
                active_widgets.push(w_ptr);
            }
        }

        if self.column_layout {
            let mut current_y = start_y;
            let spacing = 12.0;
            for &w_ptr in &active_widgets {
                let w = unsafe { &mut *w_ptr };
                let (_, _, ww, wh) = w.rect();
                let use_w = if ww > 0.0 { ww.min(available_w) } else { available_w };
                let top = crate::widget::label_offset(w);
                let use_h = if wh > 0.0 { wh } else { 24.0 + top };

                w.set_rect(left_x, current_y, use_w, use_h);
                current_y += use_h + spacing;
            }
        } else {
            let mut total_diagonal = 0.0;
            let mut count = 0;
            for &w_ptr in &active_widgets {
                let w = unsafe { &*w_ptr };
                let (_, _, ww, wh) = w.rect();
                let use_w = if ww > 0.0 { ww.min(available_w) } else { available_w };
                let use_h = if wh > 0.0 { wh } else { 50.0 };
                total_diagonal += (use_w * use_w + use_h * use_h).sqrt();
                count += 1;
            }
            let avg_diagonal = if count > 0 { total_diagonal / count as f32 } else { 100.0 };
            let base_spacing = (avg_diagonal * 0.55).max(60.0);

            for (i, &w_ptr) in active_widgets.iter().enumerate() {
                let w = unsafe { &mut *w_ptr };
                let (_, _, ww, wh) = w.rect();
                let use_w = if ww > 0.0 { ww.min(available_w) } else { available_w };
                let use_h = if wh > 0.0 { wh } else { 50.0 };

                if i == 0 {
                    w.set_rect(center_x - use_w / 2.0, center_y - use_h / 2.0, use_w, use_h);
                } else {
                    let mut ring = 1;
                    let mut ring_start = 1;
                    let mut placed = false;
                    while !placed {
                        let ring_capacity = ring * 6;
                        if i < ring_start + ring_capacity {
                            let pos_in_ring = i - ring_start;
                            let angle = (pos_in_ring as f32) * (2.0 * std::f32::consts::PI / ring_capacity as f32);
                            let radius = (ring as f32) * base_spacing;

                            let x_offset = radius * angle.cos() * aspect_ratio;
                            let y_offset = radius * angle.sin();

                            w.set_rect(
                                center_x + x_offset - use_w / 2.0,
                                center_y + y_offset - use_h / 2.0,
                                use_w,
                                use_h,
                            );
                            placed = true;
                        } else {
                            ring_start += ring_capacity;
                            ring += 1;
                        }
                    }
                }
            }
        }
    }

    fn parent(&self) -> Option<*mut (dyn Element + 'static)> {
        self.parent
    }

    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>) {
        self.parent = parent;
    }

    fn children(&self) -> Vec<*mut (dyn Element + 'static)> {
        self.children.clone()
    }

    fn add_child(&mut self, child: *mut (dyn Element + 'static)) {
        self.children.push(child);
    }

    fn clear_children(&mut self) {
        self.children.clear();
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.visible {
            return Vec::new();
        }
        let mut quads = Vec::new();
        let (px, py, pw, ph) = self.rect();
        quads.push((px, py, pw, ph, self.color()));

        for &child_ptr in &self.children {
            let widget = unsafe { &*child_ptr };
            let c = widget.color();
            if c[3] > 0.0 {
                let (wx, wy, ww, wh) = widget.rect();
                quads.push((wx, wy, ww, wh, c));
            }
            quads.extend(widget.all_quads());
        }
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        if !self.visible {
            return Vec::new();
        }
        let mut labels = Vec::new();
        if let Some(ref label) = self.base.label {
            labels.push(TextLabel {
                text: label.clone(),
                x: self.base.x,
                y: self.base.y - 18.0,
                font_size: 12.0,
                color: [0x83, 0x83, 0x8a],
            });
        }
        for &child_ptr in &self.children {
            let widget = unsafe { &*child_ptr };
            labels.extend(widget.text_labels());
        }
        labels
    }

    fn text_labels_with_bounds(&self) -> Vec<(TextLabel, Option<[f32; 4]>)> {
        if !self.visible {
            return Vec::new();
        }
        let mut result = Vec::new();
        if let Some(ref label) = self.base.label {
            result.push((
                TextLabel {
                    text: label.clone(),
                    x: self.base.x,
                    y: self.base.y - 18.0,
                    font_size: 12.0,
                    color: [0x83, 0x83, 0x8a],
                },
                None,
            ));
        }
        for &child_ptr in &self.children {
            let widget = unsafe { &*child_ptr };
            result.extend(widget.text_labels_with_bounds());
        }
        result
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32) -> bool {
        if !self.visible {
            return false;
        }
        let mut changed = false;
        for &widget_ptr in &self.children {
            let widget = unsafe { &mut *widget_ptr };
            if widget.is_dragging() {
                if widget.drag_update(px, py) {
                    changed = true;
                }
            } else if widget.cursor_moved(px, py) {
                changed = true;
            }
        }
        changed
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if !self.visible {
            return false;
        }
        for &widget_ptr in self.children.iter().rev() {
            let widget = unsafe { &mut *widget_ptr };
            if widget.popover_rect().is_some() {
                if widget.mouse_input(button, state, px, py) {
                    return true;
                }
            }
        }
        for &widget_ptr in self.children.iter().rev() {
            let widget = unsafe { &mut *widget_ptr };
            if widget.mouse_input(button, state, px, py) {
                return true;
            }
            if state == ElementState::Pressed && !widget.hit_test(px, py) {
                widget.unfocus();
            }
        }

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

    fn keyboard_input(&mut self, event: &KeyEvent) -> bool {
        if !self.visible {
            return false;
        }
        for &widget_ptr in &self.children {
            let widget = unsafe { &mut *widget_ptr };
            if widget.keyboard_input(event) {
                return true;
            }
        }
        false
    }

    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32) -> bool {
        if !self.visible {
            return false;
        }
        for &widget_ptr in self.children.iter().rev() {
            let widget = unsafe { &mut *widget_ptr };
            if widget.mouse_wheel(delta, px, py) {
                return true;
            }
        }
        false
    }

    fn popover_rect(&self) -> Option<(f32, f32, f32, f32)> {
        if !self.visible {
            return None;
        }
        for &widget_ptr in self.children.iter().rev() {
            let widget = unsafe { &*widget_ptr };
            if let Some(r) = widget.popover_rect() {
                return Some(r);
            }
        }
        None
    }

    fn render_popover(&self, pc: &mut dyn crate::layout::RenderTarget) {
        if !self.visible {
            return;
        }
        for &widget_ptr in self.children.iter().rev() {
            let widget = unsafe { &*widget_ptr };
            widget.render_popover(pc);
        }
    }

    fn tick(&mut self, dt: f32) -> bool {
        if !self.visible {
            return false;
        }
        let mut changed = false;
        for &widget_ptr in &self.children {
            let widget = unsafe { &mut *widget_ptr };
            if widget.tick(dt) {
                changed = true;
            }
        }
        changed
    }

    fn drag_update(&mut self, px: f32, py: f32) -> bool {
        let nx = px - self.drag_ox;
        let ny = py - self.drag_oy;
        let (nx, ny) = if let Some((bx, by, bw, bh)) = self.bounds {
            (nx.clamp(bx, bx + bw - self.base.w), ny.clamp(by, by + bh - self.base.h))
        } else {
            (nx, ny)
        };
        if (nx - self.base.x).abs() > 0.01 || (ny - self.base.y).abs() > 0.01 {
            let dx = nx - self.base.x;
            let dy = ny - self.base.y;
            self.base.x = nx;
            self.base.y = ny;
            
            for &child_ptr in &self.children {
                unsafe {
                    let (cx, cy, cw, ch) = (*child_ptr).rect();
                    (*child_ptr).set_rect(cx + dx, cy + dy, cw, ch);
                }
            }
            return true;
        }
        false
    }

    fn drag_begin(&mut self, px: f32, py: f32) {
        self.dragging = true;
        self.drag_ox = px - self.base.x;
        self.drag_oy = py - self.base.y;
        self.drag_start_x = self.base.x;
        self.drag_start_y = self.base.y;
    }

    fn drag_end(&mut self) { self.dragging = false; }
}

unsafe impl Send for Plate {}
unsafe impl Sync for Plate {}

pub struct Paginator {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    hovered: bool,
    pub sidebar_menu: MenuBar,
    pub plates: Vec<Plate>,
    pub selected_page: usize,
    pub page_changed: bool,
    pub sidebar_label: Option<String>,
    pub tab_text_quads: Vec<Vec<(f32, f32, f32, f32, [f32; 4])>>,
    pub sidebar_scroll_y: f32,
    pub scale_factor: f32,
    pub sidebar_mode: bool,
    pub page_hidden: bool,
    pub sidebar_w: f32,
    pub pages: Vec<String>,
    pub tabs_at_top: bool,
    pub tab_y_offset: f32,
    pub tabs_rotated: bool,
    pub hovered_tab: Option<usize>,
    pub pressed_tab: Option<usize>,
    pub target_y: f32,
    pub target_x: f32,
    pub current_y: Option<f32>,
    pub current_x: Option<f32>,
    parent: Option<*mut (dyn Element + 'static)>,
}

impl Paginator {
    pub fn sidebar_w(&self) -> f32 {
        self.sidebar_w
    }

    pub fn new(sidebar_w: f32, pages: Vec<String>) -> Self {
        let num_pages = pages.len();
        
        let mut sidebar_menu = MenuBar::new(0.0, 0.0, sidebar_w, 0.0)
            .with_vertical(true);
        for page in &pages {
            sidebar_menu = sidebar_menu.with_item(page, &[]);
        }

        let mut plates = Vec::new();
        for _ in 0..num_pages {
            let mut plate = Plate::new(0.0, 0.0, 0.0, 0.0);
            plate.visible = false;
            plates.push(plate);
        }
        if num_pages > 0 {
            plates[0].visible = true;
            sidebar_menu.menus[0].set_selected(true);
        }

        let mut pag = Self {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            hovered: false,
            sidebar_menu,
            plates,
            selected_page: 0,
            page_changed: false,
            sidebar_label: None,
            tab_text_quads: Vec::new(),
            sidebar_scroll_y: 0.0,
            scale_factor: 1.0,
            sidebar_mode: true,
            page_hidden: false,
            sidebar_w,
            pages,
            tabs_at_top: false,
            tab_y_offset: 10.0,
            tabs_rotated: true,
            hovered_tab: None,
            pressed_tab: None,
            target_y: 10.0,
            target_x: 0.0,
            current_y: Some(10.0),
            current_x: Some(0.0),
            parent: None,
        };
        pag.update_target_pos();
        pag
    }

    pub fn tab_rect(&self, idx: usize) -> (f32, f32, f32, f32) {
        if idx >= self.pages.len() {
            return (0.0, 0.0, 0.0, 0.0);
        }
        if self.tabs_at_top {
            let tab_w = if self.pages.is_empty() { 0.0 } else { self.w / self.pages.len() as f32 };
            (self.x + idx as f32 * tab_w, self.y, tab_w, 40.0)
        } else if self.tabs_rotated {
            let (tab_w, tab_h) = self.vertical_tab_size();
            let spacing = 10.0;
            (
                self.x + (self.sidebar_w - tab_w) / 2.0,
                self.y + self.tab_y_offset + idx as f32 * (tab_h + spacing) - self.sidebar_scroll_y,
                tab_w,
                tab_h,
            )
        } else {
            let bw = self.sidebar_w - 10.0;
            (self.x + 5.0, self.y + self.tab_y_offset + idx as f32 * 50.0 - self.sidebar_scroll_y, bw, 40.0)
        }
    }

    pub fn tab_size(&self, idx: usize) -> (f32, f32) {
        let r = self.tab_rect(idx);
        (r.2, r.3)
    }

    pub fn with_column_layout(mut self, enabled: bool) -> Self {
        for plate in &mut self.plates {
            plate.column_layout = enabled;
        }
        self
    }

    pub fn with_sidebar_mode(mut self, enabled: bool) -> Self {
        self.sidebar_mode = enabled;
        self
    }

    pub fn with_sidebar_label(mut self, label: &str) -> Self {
        self.sidebar_label = Some(label.to_string());
        self
    }

    pub fn sidebar_label_height(&self) -> f32 {
        if self.sidebar_label.is_some() {
            24.0
        } else {
            0.0
        }
    }

    pub fn is_page_hidden(&self) -> bool {
        self.page_hidden
    }

    pub fn set_page_hidden(&mut self, hidden: bool) {
        self.page_hidden = hidden;
    }

    pub fn set_sidebar_mode(&mut self, enabled: bool) {
        self.sidebar_mode = enabled;
    }

    pub fn add_widget_to_page(&mut self, page_idx: usize, widget: *mut (dyn Element + 'static)) {
        if page_idx < self.plates.len() {
            self.plates[page_idx].add_child(widget);
            unsafe {
                (*widget).set_parent(Some(&mut self.plates[page_idx] as *mut _));
            }
        }
    }

    pub fn clear_page_widgets(&mut self, page_idx: usize) {
        if page_idx < self.plates.len() {
            self.plates[page_idx].clear_children();
        }
    }

    pub fn with_tabs_at_top(mut self, top: bool) -> Self {
        self.tabs_at_top = top;
        self.update_target_pos();
        self
    }

    pub fn with_tab_y_offset(mut self, offset: f32) -> Self {
        self.tab_y_offset = offset;
        if self.current_y == Some(10.0) {
            self.current_y = Some(offset);
        }
        self.update_target_pos();
        self
    }

    pub fn with_tabs_rotated(mut self, rotated: bool) -> Self {
        self.tabs_rotated = rotated;
        self.update_target_pos();
        self
    }

    pub fn selected_page(&self) -> usize {
        self.selected_page
    }

    pub fn set_selected_page(&mut self, page: usize) {
        if page < self.plates.len() {
            if self.selected_page != page {
                self.plates[self.selected_page].visible = false;
                self.selected_page = page;
                self.plates[self.selected_page].visible = true;
                self.update_target_pos();
                
                // Update MenuBar focus/selection
                for (i, menu) in self.sidebar_menu.menus.iter_mut().enumerate() {
                    menu.set_selected(i == page);
                }
            }
        }
    }

    pub fn set_pages(&mut self, pages: Vec<String>) {
        self.pages = pages.clone();
        let num_pages = pages.len();
        
        let mut sidebar_menu = MenuBar::new(0.0, 0.0, self.sidebar_w, 0.0)
            .with_vertical(true);
        for page in &pages {
            sidebar_menu = sidebar_menu.with_item(page, &[]);
        }
        self.sidebar_menu = sidebar_menu;

        let mut plates = Vec::new();
        for _ in 0..num_pages {
            let mut plate = Plate::new(0.0, 0.0, 0.0, 0.0);
            plate.visible = false;
            plates.push(plate);
        }
        self.plates = plates;
        if self.selected_page >= num_pages {
            self.selected_page = 0;
        }
        if !self.plates.is_empty() {
            self.plates[self.selected_page].visible = true;
            self.sidebar_menu.menus[self.selected_page].set_selected(true);
        }
        self.update_target_pos();
    }

    pub fn set_scale_factor(&mut self, scale: f32) {
        self.scale_factor = scale;
    }

    pub fn vertical_tab_size(&self) -> (f32, f32) {
        if self.tabs_rotated {
            ((self.sidebar_w - 16.0).clamp(24.0, 120.0), 120.0)
        } else {
            (self.sidebar_w - 10.0, 40.0)
        }
    }

    fn total_sidebar_height(&self) -> f32 {
        let (_, tab_h) = self.vertical_tab_size();
        let spacing = 10.0;
        let step = if self.tabs_rotated { tab_h + spacing } else { 50.0 };
        self.tab_y_offset + self.pages.len() as f32 * step - spacing
    }

    fn update_target_pos(&mut self) {
        if self.tabs_at_top {
            let tab_w = if self.pages.is_empty() { 0.0 } else { self.w / self.pages.len() as f32 };
            self.target_x = self.selected_page as f32 * tab_w;
            if self.current_x.is_none() {
                self.current_x = Some(self.target_x);
            }
        } else if self.tabs_rotated {
            let (_, tab_h) = self.vertical_tab_size();
            let spacing = 10.0;
            let target = self.tab_y_offset + self.selected_page as f32 * (tab_h + spacing);
            self.target_y = target;
            if self.current_y.is_none() {
                self.current_y = Some(target);
            }
        } else {
            let target = self.tab_y_offset + self.selected_page as f32 * 50.0;
            self.target_y = target;
            if self.current_y.is_none() {
                self.current_y = Some(target);
            }
        }
        self.generate_tab_quads();
    }

    fn generate_tab_quads(&mut self) {
        self.tab_text_quads.clear();
        let (tab_w, _) = self.vertical_tab_size();
        let active_color = colors::paginator_tab_label_color();
        let active_srgb = colors::to_srgb(active_color);
        let active_r = (active_srgb[0] * 255.0) as u8;
        let active_g = (active_srgb[1] * 255.0) as u8;
        let active_b = (active_srgb[2] * 255.0) as u8;
        let inactive_r = (active_r as f32 * 0.78) as u8;
        let inactive_g = (active_g as f32 * 0.78) as u8;
        let inactive_b = (active_b as f32 * 0.78) as u8;

        for (i, page_name) in self.pages.iter().enumerate() {
            let color = if self.selected_page == i {
                [active_r, active_g, active_b]
            } else {
                [inactive_r, inactive_g, inactive_b]
            };
            let hex_color = format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2]);

            let trimmed = page_name.trim();
            let has_icon = trimmed.find(' ').is_some();
            let label_text = if let Some(space_idx) = trimmed.find(' ') {
                trimmed.split_at(space_idx).1.trim()
            } else {
                trimmed
            };

            let w_px = tab_w as u32;
            let h_px = if has_icon { 80 } else { 120 };

            if w_px == 0 || h_px == 0 {
                self.tab_text_quads.push(Vec::new());
                continue;
            }

            let svg_data = format!(
                r##"<svg width="{}" height="{}" xmlns="http://www.w3.org/2000/svg">
  <text x="{}" y="{}" font-family="sans-serif" font-size="12" fill="{}" text-anchor="middle" dominant-baseline="middle" transform="rotate(-90 {} {})">{}</text>
</svg>"##,
                w_px, h_px,
                w_px as f32 / 2.0, h_px as f32 / 2.0,
                hex_color,
                w_px as f32 / 2.0, h_px as f32 / 2.0,
                label_text
            );

            let opt = resvg::usvg::Options::default();
            let fontdb = get_font_db();
            
            let mut page_quads = Vec::new();
            if let Ok(tree) = resvg::usvg::Tree::from_data(svg_data.as_bytes(), &opt, fontdb) {
                if let Some(mut pixmap) = resvg::tiny_skia::Pixmap::new(w_px, h_px) {
                    resvg::render(&tree, resvg::tiny_skia::Transform::default(), &mut pixmap.as_mut());
                    let pixels = pixmap.data();
                    for row in 0..h_px {
                        for col in 0..w_px {
                            let idx = ((row * w_px + col) * 4) as usize;
                            if idx + 3 < pixels.len() {
                                let a = pixels[idx + 3] as f32 / 255.0;
                                if a > 0.0 {
                                    let r = pixels[idx] as f32 / 255.0;
                                    let g = pixels[idx + 1] as f32 / 255.0;
                                    let b = pixels[idx + 2] as f32 / 255.0;
                                    page_quads.push((
                                        col as f32,
                                        row as f32,
                                        1.0,
                                        1.0,
                                        [r, g, b, a],
                                    ));
                                }
                            }
                        }
                    }
                }
            }
            self.tab_text_quads.push(page_quads);
        }
    }
}

impl Element for Paginator {
    fn rect(&self) -> (f32, f32, f32, f32) {
        (self.x, self.y, self.w, self.h)
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.x = x;
        self.y = y;
        self.w = w;
        self.h = h;
        self.update_target_pos();

        let self_ptr = self as *mut Paginator;
        self.sidebar_menu.set_parent(Some(self_ptr));
        for plate in &mut self.plates {
            plate.set_parent(Some(self_ptr));
        }

        let tabs_at_top = self.tabs_at_top;
        if tabs_at_top {
            self.sidebar_menu.set_rect(self.x, self.y, self.w, 40.0);
            for plate in &mut self.plates {
                plate.set_rect(self.x, self.y + 40.0, self.w, (self.h - 40.0).max(0.0));
            }
        } else {
            let sidebar_w = self.sidebar_w;
            self.sidebar_menu.set_rect(self.x, self.y, sidebar_w, self.h);
            for plate in &mut self.plates {
                plate.set_rect(self.x + sidebar_w, self.y, (self.w - sidebar_w).max(0.0), self.h);
            }
        }
    }

    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn set_hovered(&mut self, v: bool) {
        self.hovered = v;
    }

    fn hovered(&self) -> bool {
        self.hovered
    }

    fn highlight_quad(&self) -> Option<(f32, f32, f32, f32, [f32; 4])> {
        if let Some(i) = self.hovered_tab {
            if self.tabs_at_top {
                let tab_w = if self.pages.is_empty() { 0.0 } else { self.w / self.pages.len() as f32 };
                let bx = self.x + i as f32 * tab_w + 2.0;
                let by = self.y + 2.0;
                let bw = tab_w - 4.0;
                let bh = 40.0 - 4.0;
                let scroll_offset = hover_animation::get_scroll_offset();
                Some((bx, by + scroll_offset, bw, bh, colors::HIGHLIGHT_SECONDARY))
            } else {
                let (bx, by, bw, bh) = if self.tabs_rotated {
                    let (tab_w, tab_h) = self.vertical_tab_size();
                    let spacing = 10.0;
                    (
                        self.x + (self.sidebar_w - tab_w) / 2.0,
                        self.y + self.tab_y_offset + i as f32 * (tab_h + spacing) - self.sidebar_scroll_y,
                        tab_w,
                        tab_h,
                    )
                } else {
                    let bw = self.sidebar_w - 10.0;
                    (
                        self.x + 5.0,
                        self.y + self.tab_y_offset + i as f32 * 50.0 - self.sidebar_scroll_y,
                        bw,
                        40.0,
                    )
                };
                let scroll_offset = hover_animation::get_scroll_offset();
                let qy = by + scroll_offset;
                let qh = bh;

                let min_y = self.y;
                let max_y = self.y + self.h;
                let ry1 = qy.max(min_y);
                let ry2 = (qy + qh).min(max_y);
                let rh = ry2 - ry1;
                if rh > 0.0 {
                    Some((bx, ry1, bw, rh, colors::HIGHLIGHT_SECONDARY))
                } else {
                    None
                }
            }
        } else {
            None
        }
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        if self.tabs_rotated {
            quads.push((self.x, self.y, self.sidebar_w, self.h, colors::sidebar_bg_color()));
            
            let (tab_w, tab_h) = self.vertical_tab_size();
            let spacing = 10.0;
            let min_y = self.y;
            let max_y = self.y + self.h;

            // Draw primary highlight for the selected active tab
            if let Some(cy) = self.current_y {
                let bx = self.x + (self.sidebar_w - tab_w) / 2.0;
                let by = self.y + cy - self.sidebar_scroll_y;
                let ry1 = by.max(min_y);
                let ry2 = (by + tab_h).min(max_y);
                let rh = ry2 - ry1;
                if rh > 0.0 {
                    quads.push((bx, ry1, tab_w, rh, colors::highlight_primary_color()));
                }
            }

            for (i, page_name) in self.pages.iter().enumerate() {
                let bx = self.x + (self.sidebar_w - tab_w) / 2.0;
                let by = self.y + self.tab_y_offset + i as f32 * (tab_h + spacing) - self.sidebar_scroll_y;

                let trimmed = page_name.trim();
                let has_icon = trimmed.find(' ').is_some();
                let y_offset = if has_icon { 40.0 } else { 0.0 };

                if i < self.tab_text_quads.len() {
                    for &(qx, qy, qw, qh, qc) in &self.tab_text_quads[i] {
                        let absolute_x = bx + qx;
                        let absolute_y = by + y_offset + qy;
                        
                        let ry1 = absolute_y.max(min_y);
                        let ry2 = (absolute_y + qh).min(max_y);
                        let rh = ry2 - ry1;
                        if rh > 0.0 {
                            quads.push((absolute_x, ry1, qw, rh, qc));
                        }
                    }
                }
            }
        } else {
            quads.extend(self.sidebar_menu.extra_quads());
        }

        if self.selected_page < self.plates.len() {
            quads.extend(self.plates[self.selected_page].extra_quads());
        }
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        if self.tabs_rotated {
            let (tab_w, tab_h) = self.vertical_tab_size();
            let spacing = 10.0;
            let active_color = colors::paginator_tab_label_color();
            let active_srgb = colors::to_srgb(active_color);
            let active_r = (active_srgb[0] * 255.0) as u8;
            let active_g = (active_srgb[1] * 255.0) as u8;
            let active_b = (active_srgb[2] * 255.0) as u8;
            let inactive_r = (active_r as f32 * 0.78) as u8;
            let inactive_g = (active_g as f32 * 0.78) as u8;
            let inactive_b = (active_b as f32 * 0.78) as u8;

            for (i, page_name) in self.pages.iter().enumerate() {
                let color = if self.selected_page == i {
                    [active_r, active_g, active_b]
                } else {
                    [inactive_r, inactive_g, inactive_b]
                };
                let bx = self.x + (self.sidebar_w - tab_w) / 2.0;
                let by = self.y + self.tab_y_offset + i as f32 * (tab_h + spacing) - self.sidebar_scroll_y;
                let bw = tab_w;

                let trimmed = page_name.trim();
                let has_icon = trimmed.find(' ').is_some();
                if has_icon {
                    if let Some(space_idx) = trimmed.find(' ') {
                        let (icon, _) = trimmed.split_at(space_idx);
                        let icon = icon.trim();
                        if !icon.is_empty() {
                            let icon_font_size = 14.0;
                            let est_icon_w = TextLabel::estimate_width(icon, icon_font_size);
                            let icon_y = by + 12.0;
                            if icon_y >= self.y && icon_y + icon_font_size <= self.y + self.h {
                                labels.push(TextLabel {
                                    text: icon.to_string(),
                                    x: bx + (bw - est_icon_w) / 2.0,
                                    y: icon_y,
                                    font_size: icon_font_size,
                                    color,
                                });
                            }
                        }
                    }
                }
            }
        } else {
            labels.extend(self.sidebar_menu.text_labels());
        }

        if self.selected_page < self.plates.len() {
            labels.extend(self.plates[self.selected_page].text_labels());
        }
        labels
    }

    fn text_labels_with_bounds(&self) -> Vec<(TextLabel, Option<[f32; 4]>)> {
        let mut labels = Vec::new();
        for l in self.text_labels() {
            labels.push((l, None));
        }
        labels
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32) -> bool {
        let mut changed = false;
        let was_hovered_tab = self.hovered_tab;
        self.hovered_tab = None;
        for i in 0..self.pages.len() {
            let (bx, by, bw, bh) = if self.tabs_at_top {
                let tab_w = if self.pages.is_empty() { 0.0 } else { self.w / self.pages.len() as f32 };
                (self.x + i as f32 * tab_w, self.y, tab_w, 40.0)
            } else if self.tabs_rotated {
                let (tab_w, tab_h) = self.vertical_tab_size();
                let spacing = 10.0;
                (
                    self.x + (self.sidebar_w - tab_w) / 2.0,
                    self.y + self.tab_y_offset + i as f32 * (tab_h + spacing) - self.sidebar_scroll_y,
                    tab_w,
                    tab_h,
                )
            } else {
                let bw = self.sidebar_w - 10.0;
                (self.x + 5.0, self.y + self.tab_y_offset + i as f32 * 50.0 - self.sidebar_scroll_y, bw, 40.0)
            };
            if px >= bx && px <= bx + bw && py >= by && py <= by + bh && py >= self.y && py <= self.y + self.h {
                self.hovered_tab = Some(i);
                break;
            }
        }
        if was_hovered_tab != self.hovered_tab {
            changed = true;
        }

        if self.sidebar_menu.cursor_moved(px, py) {
            changed = true;
        }
        if self.selected_page < self.plates.len() {
            if self.plates[self.selected_page].cursor_moved(px, py) {
                changed = true;
            }
        }

        changed
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if self.selected_page < self.plates.len() {
            if self.plates[self.selected_page].popover_rect().is_some() {
                if self.plates[self.selected_page].mouse_input(button, state, px, py) {
                    return true;
                }
            }
        }

        let mut clicked_tab = false;
        if button == MouseButton::Left {
            match state {
                ElementState::Pressed => {
                    self.pressed_tab = None;
                    for i in 0..self.pages.len() {
                        let (bx, by, bw, bh) = if self.tabs_at_top {
                            let tab_w = if self.pages.is_empty() { 0.0 } else { self.w / self.pages.len() as f32 };
                            (self.x + i as f32 * tab_w, self.y, tab_w, 40.0)
                        } else if self.tabs_rotated {
                            let (tab_w, tab_h) = self.vertical_tab_size();
                            let spacing = 10.0;
                            (
                                self.x + (self.sidebar_w - tab_w) / 2.0,
                                self.y + self.tab_y_offset + i as f32 * (tab_h + spacing) - self.sidebar_scroll_y,
                                tab_w,
                                tab_h,
                            )
                        } else {
                            let bw = self.sidebar_w - 10.0;
                            (self.x + 5.0, self.y + self.tab_y_offset + i as f32 * 50.0 - self.sidebar_scroll_y, bw, 40.0)
                        };
                        if px >= bx && px <= bx + bw && py >= by && py <= by + bh && py >= self.y && py <= self.y + self.h {
                            self.pressed_tab = Some(i);
                            clicked_tab = true;
                            break;
                        }
                    }
                }
                ElementState::Released => {
                    if let Some(i) = self.pressed_tab.take() {
                        let (bx, by, bw, bh) = if self.tabs_at_top {
                            let tab_w = if self.pages.is_empty() { 0.0 } else { self.w / self.pages.len() as f32 };
                            (self.x + i as f32 * tab_w, self.y, tab_w, 40.0)
                        } else if self.tabs_rotated {
                            let (tab_w, tab_h) = self.vertical_tab_size();
                            let spacing = 10.0;
                            (
                                self.x + (self.sidebar_w - tab_w) / 2.0,
                                self.y + self.tab_y_offset + i as f32 * (tab_h + spacing) - self.sidebar_scroll_y,
                                tab_w,
                                tab_h,
                            )
                        } else {
                            let bw = self.sidebar_w - 10.0;
                            (self.x + 5.0, self.y + self.tab_y_offset + i as f32 * 50.0 - self.sidebar_scroll_y, bw, 40.0)
                        };
                        if px >= bx && px <= bx + bw && py >= by && py <= by + bh && py >= self.y && py <= self.y + self.h {
                            if self.selected_page != i {
                                self.set_selected_page(i);
                                self.page_changed = true;
                            }
                            clicked_tab = true;
                        }
                    }
                }
            }
        }

        if clicked_tab {
            return true;
        }

        if self.sidebar_menu.mouse_input(button, state, px, py) {
            return true;
        }

        if self.selected_page < self.plates.len() {
            if self.plates[self.selected_page].mouse_input(button, state, px, py) {
                return true;
            }
        }

        false
    }

    fn keyboard_input(&mut self, event: &KeyEvent) -> bool {
        if self.sidebar_menu.keyboard_input(event) {
            return true;
        }
        if self.selected_page < self.plates.len() {
            if self.plates[self.selected_page].keyboard_input(event) {
                return true;
            }
        }
        false
    }

    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32) -> bool {
        if !self.tabs_at_top {
            let bx = self.x;
            let by = self.y;
            let bw = self.sidebar_w;
            let bh = self.h;
            if px >= bx && px <= bx + bw && py >= by && py <= by + bh {
                let scroll_speed = 24.0;
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => -y * scroll_speed,
                    MouseScrollDelta::PixelDelta(pos) => -pos.y as f32,
                };
                let old_scroll = self.sidebar_scroll_y;
                let max_scroll = (self.total_sidebar_height() - self.h).max(0.0);
                self.sidebar_scroll_y = (self.sidebar_scroll_y + dy).clamp(0.0, max_scroll);
                if (self.sidebar_scroll_y - old_scroll).abs() > 0.01 {
                    self.update_target_pos();
                    return true;
                }
            }
        }

        if self.sidebar_menu.mouse_wheel(delta, px, py) {
            return true;
        }

        if self.selected_page < self.plates.len() {
            if self.plates[self.selected_page].mouse_wheel(delta, px, py) {
                return true;
            }
        }
        false
    }

    fn popover_rect(&self) -> Option<(f32, f32, f32, f32)> {
        if let Some(r) = self.sidebar_menu.popover_rect() {
            return Some(r);
        }
        if self.selected_page < self.plates.len() {
            if let Some(r) = self.plates[self.selected_page].popover_rect() {
                return Some(r);
            }
        }
        None
    }

    fn render_popover(&self, pc: &mut dyn crate::layout::RenderTarget) {
        self.sidebar_menu.render_popover(pc);
        if self.selected_page < self.plates.len() {
            self.plates[self.selected_page].render_popover(pc);
        }
    }

    fn take_click(&mut self) -> bool {
        if self.page_changed {
            self.page_changed = false;
            true
        } else {
            false
        }
    }

    fn value(&self) -> i32 {
        self.selected_page as i32
    }

    fn tick(&mut self, dt: f32) -> bool {
        let mut changed = false;
        if let Some(current) = self.current_y {
            let diff = self.target_y - current;
            if diff.abs() > 0.1 {
                let decay = 15.0;
                let next = current + diff * (1.0 - (-decay * dt).exp());
                self.current_y = Some(next);
                changed = true;
            } else {
                self.current_y = Some(self.target_y);
            }
        }
        if let Some(current) = self.current_x {
            let diff = self.target_x - current;
            if diff.abs() > 0.1 {
                let decay = 15.0;
                let next = current + diff * (1.0 - (-decay * dt).exp());
                self.current_x = Some(next);
                changed = true;
            } else {
                self.current_x = Some(self.target_x);
            }
        }

        let self_ptr = self as *mut Paginator;
        self.sidebar_menu.set_parent(Some(self_ptr));
        for plate in &mut self.plates {
            plate.set_parent(Some(self_ptr));
        }

        if self.sidebar_menu.tick(dt) {
            changed = true;
        }

        for plate in &mut self.plates {
            if plate.tick(dt) {
                changed = true;
            }
        }

        changed
    }

    fn parent(&self) -> Option<*mut (dyn Element + 'static)> {
        self.parent
    }

    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>) {
        self.parent = parent;
    }

    fn children(&self) -> Vec<*mut (dyn Element + 'static)> {
        let mut list = Vec::new();
        list.push(&self.sidebar_menu as *const dyn Element as *mut dyn Element);
        for plate in &self.plates {
            list.push(plate as *const dyn Element as *mut dyn Element);
        }
        list
    }

    fn add_child(&mut self, _child: *mut (dyn Element + 'static)) {}
    fn clear_children(&mut self) {}

    fn set_modifiers(&mut self, ctrl: bool, shift: bool, alt: bool) {
        self.sidebar_menu.set_modifiers(ctrl, shift, alt);
        if self.selected_page < self.plates.len() {
            self.plates[self.selected_page].set_modifiers(ctrl, shift, alt);
        }
    }
    fn menu_names(&self) -> Vec<String> {
        self.pages.clone()
    }
    fn selected_page(&self) -> usize {
        self.selected_page()
    }
    fn set_selected_page(&mut self, page: usize) {
        self.set_selected_page(page);
    }
    fn is_page_hidden(&self) -> bool {
        self.is_page_hidden()
    }
    fn set_page_hidden(&mut self, hidden: bool) {
        self.set_page_hidden(hidden);
    }
    fn set_pages(&mut self, pages: Vec<String>) {
        self.set_pages(pages);
    }
    fn sidebar_w(&self) -> f32 {
        self.sidebar_w()
    }
    fn set_sidebar_mode(&mut self, enabled: bool) {
        self.set_sidebar_mode(enabled);
    }
    fn set_sidebar_label(&mut self, label: Option<String>) {
        self.sidebar_label = label;
        self.update_target_pos();
    }
    fn add_widget_to_page(&mut self, page_idx: usize, widget: *mut (dyn Element + 'static)) {
        self.add_widget_to_page(page_idx, widget);
    }
    fn clear_page_widgets(&mut self, page_idx: usize) {
        self.clear_page_widgets(page_idx);
    }
    fn menu_items_list(&self) -> Vec<Vec<String>> {
        self.sidebar_menu.menu_items_list()
    }
    fn menu_checked_list(&self) -> Vec<Vec<Option<bool>>> {
        self.sidebar_menu.menu_checked_list()
    }
}

unsafe impl Send for Paginator {}
unsafe impl Sync for Paginator {}

