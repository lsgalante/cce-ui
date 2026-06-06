use crate::colors;
use crate::widget::*;

#[derive(Debug, Clone)]
pub struct TextLabel {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
    pub color: [u8; 3],
}

impl TextLabel {
    pub fn estimate_width(text: &str, font_size: f32) -> f32 {
        let mut weight_sum = 0.0;
        for c in text.chars() {
            weight_sum += match c {
                'i' | 'l' | 't' | 'j' | 'I' | ' ' | '.' | ',' | '!' | ';' | ':' | '\'' | '1' | '-' | '(' | ')' | '[' | ']' => 0.26,
                'f' | 'r' | 's' | 'J' => 0.35,
                'w' | 'm' | 'M' | 'W' => 0.72,
                'A' | 'B' | 'C' | 'D' | 'E' | 'G' | 'H' | 'K' | 'N' | 'O' | 'P' | 'Q' | 'R' | 'S' | 'T' | 'U' | 'V' | 'X' | 'Y' | 'Z' => 0.65,
                _ => 0.52,
            };
        }
        weight_sum * font_size
    }

    pub fn curved_layout(
        text: &str,
        cx: f32,
        cy: f32,
        r: f32,
        start_angle: f32,
        end_angle: f32,
        font_size: f32,
        color: [u8; 3],
    ) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        let char_widths: Vec<f32> = text.chars().map(|c| {
            Self::estimate_width(&c.to_string(), font_size)
        }).collect();
        let total_width: f32 = char_widths.iter().sum();
        
        let mid_angle = (start_angle + end_angle) / 2.0;
        let angular_width = total_width / r;
        let text_start_angle = mid_angle - angular_width / 2.0;
        
        let mut current_angle = text_start_angle;
        for (i, c) in text.chars().enumerate() {
            let cw = char_widths[i];
            let dtheta = cw / r;
            let char_center_angle = current_angle + dtheta / 2.0;
            
            let x = cx + r * char_center_angle.cos() - cw / 2.0;
            let y = cy + r * char_center_angle.sin() - font_size / 2.0;
            
            labels.push(TextLabel {
                text: c.to_string(),
                x,
                y,
                font_size,
                color,
            });
            
            current_angle += dtheta;
        }
        labels
    }

    pub fn is_covered_by(&self, px: f32, py: f32, pw: f32, ph: f32) -> bool {
        let text_w = Self::estimate_width(&self.text, self.font_size);
        let x_overlap = self.x <= px + pw && (self.x + text_w) >= px;
        let y_overlap = self.y <= py + ph && (self.y + self.font_size) >= py;
        x_overlap && y_overlap
    }
}


#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct GraphNode {
    pub name: String,
    pub position: (f32, f32), // (column, row)
    pub parameters: Vec<(String, String, String)>, // (name, value, type)
    pub geom_visible: bool,
}

pub struct Sidebar {
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
}

impl Sidebar {
    pub fn new(w: f32) -> Self { Self { x: 0.0, y: 0.0, w, h: 0.0, hovered: false } }
}

impl Element for Sidebar {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { colors::sidebar_bg_color() }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }
}

pub struct Panel {
    base: Widget,
    dragging: bool,
    drag_ox: f32, drag_oy: f32,
    drag_start_x: f32, drag_start_y: f32,
    bounds: Option<(f32, f32, f32, f32)>,
}

impl Panel {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            base: Widget::new_rect(x, y, w, h),
            dragging: false,
            drag_ox: 0.0,
            drag_oy: 0.0,
            drag_start_x: 0.0,
            drag_start_y: 0.0,
            bounds: None,
        }
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self
    }

    pub fn set_bounds(&mut self, bx: f32, by: f32, bw: f32, bh: f32) {
        self.bounds = Some((bx, by, bw, bh));
    }
}

impl Element for Panel {
    crate::impl_widget_base!(Panel);
    fn highlight_quad(&self) -> Option<(f32, f32, f32, f32, [f32; 4])> { None }

    fn color(&self) -> [f32; 4] { if self.dragging { colors::PANEL_DRAG } else { colors::PANEL_IDLE } }

    fn set_drag_bounds(&mut self, bx: f32, by: f32, bw: f32, bh: f32) {
        self.bounds = Some((bx, by, bw, bh));
    }

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
        let (nx, ny) = if let Some((bx, by, bw, bh)) = self.bounds {
            (nx.clamp(bx, bx + bw - self.base.w), ny.clamp(by, by + bh - self.base.h))
        } else {
            (nx, ny)
        };
        if (nx - self.base.x).abs() > 0.01 || (ny - self.base.y).abs() > 0.01 {
            self.base.x = nx;
            self.base.y = ny;
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

pub struct Node {
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
    selected: bool,
    dragging: bool,
    drag_ox: f32, drag_oy: f32,
    bounds: Option<(f32, f32, f32, f32)>,
    grid_snap_x: f32,
    grid_snap_y: f32,
    grid_origin_x: f32,
    grid_origin_y: f32,
    name: String,
    pub parameters: Vec<(String, String, String)>,
    geom_visible: bool,
    geom_toggled: bool,
    pub(crate) toggle_hovered: bool,
}

impl Node {
    pub fn new(x: f32, y: f32, w: f32, h: f32, name: &str) -> Self {
        Self {
            x, y, w, h,
            hovered: false, selected: false, dragging: false, drag_ox: 0.0, drag_oy: 0.0,
            bounds: None, grid_snap_x: 0.0, grid_snap_y: 0.0,
            grid_origin_x: 0.0, grid_origin_y: 0.0,
            name: name.to_string(), parameters: Vec::new(),
            geom_visible: true, geom_toggled: false, toggle_hovered: false,
        }
    }

    pub fn with_params(mut self, params: &[(&str, &str)]) -> Self {
        self.parameters = params.iter().map(|(k, v)| (k.to_string(), v.to_string(), "string".to_string())).collect();
        self
    }

    pub fn with_grid_snap(mut self, gx: f32, gy: f32) -> Self {
        self.grid_snap_x = gx;
        self.grid_snap_y = gy;
        self
    }

    pub(crate) fn toggle_rect(&self) -> (f32, f32, f32, f32) {
        (self.x + self.w - 30.0, self.y + (self.h - 18.0) / 2.0, 18.0, 18.0)
    }
}

impl Element for Node {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] {
        if self.dragging { colors::node_drag_color() }
        else if self.selected { colors::node_selected_color() }
        else { colors::node_color() }
    }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }

    fn focus(&mut self) {
        self.selected = true;
        focus::set_focused(self);
    }
    fn unfocus(&mut self) { self.selected = false; }

    fn node_params(&self) -> Vec<(String, String, String)> { self.parameters.clone() }
    fn set_display_params(&mut self, params: &[(String, String, String)]) {
        self.parameters = params.to_vec();
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        vec![TextLabel {
            text: self.name.clone(),
            x: self.x + 8.0,
            y: self.y + (self.h - 12.0) / 2.0,
            font_size: 14.0,
            color: [0xcc, 0xcc, 0xd4],
        }]
    }

    fn set_drag_bounds(&mut self, bx: f32, by: f32, bw: f32, bh: f32) {
        self.bounds = Some((bx, by, bw, bh));
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32) -> bool {
        let was_hovered = self.hovered;
        self.hovered = self.hit_test(px, py);

        let was_toggle_hovered = self.toggle_hovered;
        let (tx, ty, tw, th) = self.toggle_rect();
        self.toggle_hovered = px >= tx && px < tx + tw && py >= ty && py < ty + th;

        was_hovered != self.hovered || was_toggle_hovered != self.toggle_hovered
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button != MouseButton::Left { return false; }
        match state {
            ElementState::Pressed => {
                if self.hit_test(px, py) {
                    let (tx, ty, tw, th) = self.toggle_rect();
                    if px >= tx && px < tx + tw && py >= ty && py < ty + th {
                        self.geom_visible = !self.geom_visible;
                        self.geom_toggled = true;
                        return true;
                    }
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
    fn draggable(&self) -> bool { !self.toggle_hovered }

    fn set_grid_snap(&mut self, gx: f32, gy: f32) { self.grid_snap_x = gx; self.grid_snap_y = gy; }
    fn set_grid_origin(&mut self, ox: f32, oy: f32) { self.grid_origin_x = ox; self.grid_origin_y = oy; }
    fn set_node_name(&mut self, name: &str) { self.name = name.to_string(); }

    fn drag_update(&mut self, px: f32, py: f32) -> bool {
        let nx = px - self.drag_ox;
        let ny = py - self.drag_oy;
        let (nx, ny) = if let Some((bx, by, bw, bh)) = self.bounds {
            (nx.clamp(bx, bx + bw - self.w), ny.clamp(by, by + bh - self.h))
        } else {
            (nx, ny)
        };
        let nx = if self.grid_snap_x > 0.0 {
            let relative = nx - self.grid_origin_x;
            let snapped = (relative / self.grid_snap_x).round() * self.grid_snap_x;
            snapped + self.grid_origin_x
        } else { nx };
        let ny = if self.grid_snap_y > 0.0 {
            let relative = ny - self.grid_origin_y;
            let snapped = (relative / self.grid_snap_y).round() * self.grid_snap_y;
            snapped + self.grid_origin_y
        } else { ny };
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
    }

    fn drag_end(&mut self) { self.dragging = false; }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let (tx, ty, tw, th) = self.toggle_rect();
        let bg_color = if self.toggle_hovered {
            colors::TOGGLE_HOVER
        } else {
            colors::TOGGLE_OFF
        };

        let mut quads = vec![
            (tx, ty, tw, th, bg_color)
        ];

        if self.geom_visible {
            let inset = 3.0;
            quads.push((tx + inset, ty + inset, tw - inset * 2.0, th - inset * 2.0, colors::TOGGLE_ON));
        }

        quads
    }

    fn set_geom_visible(&mut self, visible: bool) { self.geom_visible = visible; }
    fn geom_visible(&self) -> bool { self.geom_visible }
    fn take_geom_toggle(&mut self) -> bool { std::mem::take(&mut self.geom_toggled) }
}

impl Drop for Node {
    fn drop(&mut self) {
        focus::clear_if_matches(self);
    }
}


#[derive(Debug, Clone)]
pub struct Label {
    base: Widget,
    font_size: f32,
    color: [u8; 3],
}

impl Label {
    pub fn new(text: &str) -> Self {
        let mut base = Widget::new();
        base.label = Some(text.to_string());
        Self {
            base,
            font_size: 12.0,
            color: [0x83, 0x83, 0x8a],
        }
    }

    pub fn with_font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    pub fn with_color(mut self, color: [u8; 3]) -> Self {
        self.color = color;
        self
    }

    pub fn set_text(&mut self, text: &str) {
        self.base.label = Some(text.to_string());
    }

    pub fn set_color(&mut self, color: [u8; 3]) {
        self.color = color;
    }
}

impl Element for Label {
    crate::impl_widget_base!(Label);

    fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }

    fn text_labels(&self) -> Vec<TextLabel> {
        vec![TextLabel {
            text: self.base.label.clone().unwrap_or_default(),
            x: self.base.x,
            y: self.base.y + (self.base.h - self.font_size) / 2.0,
            font_size: self.font_size,
            color: self.color,
        }]
    }
}

#[derive(Clone)]
pub struct SectionHeader {
    base: Widget,
}

impl SectionHeader {
    pub fn new(title: &str) -> Self {
        let mut base = Widget::new();
        base.label = Some(title.to_string());
        Self { base }
    }
}

impl Element for SectionHeader {
    crate::impl_widget_base!(SectionHeader);
    fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }
    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        vec![(self.base.x + 8.0, self.base.y + 22.0, self.base.w - 16.0, 1.0, [0.18, 0.18, 0.27, 1.0])]
    }
    fn text_labels(&self) -> Vec<TextLabel> {
        vec![TextLabel {
            text: self.base.label.clone().unwrap_or_default(),
            x: self.base.x + 12.0,
            y: self.base.y,
            font_size: 14.0,
            color: [212, 212, 212],
        }]
    }
}


#[derive(Debug, Clone)]
pub struct Svg {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub quads: Vec<(f32, f32, f32, f32, [f32; 4])>,
}

impl Svg {
    pub fn new(svg_data: &[u8], x: f32, y: f32, w: f32, h: f32) -> Option<Self> {
        let opt = resvg::usvg::Options::default();
        let fontdb = resvg::usvg::fontdb::Database::new();
        let tree = resvg::usvg::Tree::from_data(svg_data, &opt, &fontdb).ok()?;
        
        let target_w = w as u32;
        let target_h = h as u32;
        if target_w == 0 || target_h == 0 {
            return None;
        }
        let mut pixmap = resvg::tiny_skia::Pixmap::new(target_w, target_h)?;
        
        let orig_w = tree.size().width();
        let orig_h = tree.size().height();
        let sx = target_w as f32 / orig_w;
        let sy = target_h as f32 / orig_h;
        let transform = resvg::tiny_skia::Transform::from_scale(sx, sy);
        
        resvg::render(&tree, transform, &mut pixmap.as_mut());
        
        let mut quads = Vec::new();
        let pixels = pixmap.data();
        for row in 0..target_h {
            for col in 0..target_w {
                let idx = ((row * target_w + col) * 4) as usize;
                if idx + 3 < pixels.len() {
                    let a = pixels[idx + 3] as f32 / 255.0;
                    if a > 0.0 {
                        let r = pixels[idx] as f32 / 255.0;
                        let g = pixels[idx + 1] as f32 / 255.0;
                        let b = pixels[idx + 2] as f32 / 255.0;
                        quads.push((
                            x + col as f32,
                            y + row as f32,
                            1.0,
                            1.0,
                            [r, g, b, a],
                        ));
                    }
                }
            }
        }
        
        Some(Self { x, y, w, h, quads })
    }

    pub fn from_file<P: AsRef<std::path::Path>>(path: P, x: f32, y: f32, w: f32, h: f32) -> Option<Self> {
        let data = std::fs::read(path).ok()?;
        Self::new(&data, x, y, w, h)
    }

    pub fn from_str(svg_str: &str, x: f32, y: f32, w: f32, h: f32) -> Option<Self> {
        Self::new(svg_str.as_bytes(), x, y, w, h)
    }
}

impl Element for Svg {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let dx = x - self.x;
        let dy = y - self.y;
        for quad in &mut self.quads {
            quad.0 += dx;
            quad.1 += dy;
        }
        self.x = x;
        self.y = y;
        self.w = w;
        self.h = h;
    }
    
    fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }
    
    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        self.quads.clone()
    }
}

#[derive(Debug, Clone)]
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

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
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

    fn keyboard_input(&mut self, event: &KeyEvent) -> bool {
        let _idx = match self.editing_idx {
            Some(i) => i,
            None => return false,
        };
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
                self.editing_idx = None;
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


pub struct ProgressBar {
    base: Widget,
    _value: f32,
}

impl ProgressBar {
    pub fn new(value: f32) -> Self {
        Self {
            base: Widget::new(),
            _value: value,
        }
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self
    }
}

impl Element for ProgressBar {
    crate::impl_widget_base!(ProgressBar);
    fn highlight_quad(&self) -> Option<(f32, f32, f32, f32, [f32; 4])> { None }

    fn color(&self) -> [f32; 4] { colors::PROGRESS_BG }
}

pub(crate) fn make_widget_text_buffer(fs: &mut glyphon::FontSystem, text: &str, size: f32, font_family: &str) -> glyphon::Buffer {
    let metrics = glyphon::Metrics::new(size, size * 1.4);
    let mut buf = glyphon::Buffer::new(fs, metrics);
    let attrs = glyphon::Attrs::new().family(glyphon::Family::Name(font_family));
    buf.set_text(fs, text, attrs, glyphon::Shaping::Advanced);
    buf.shape_until_scroll(fs, true);
    buf
}

pub struct StatusBar {
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
    pub text: String,
    pub text_buf: Option<glyphon::Buffer>,
    pub text_offset_x: Option<f32>,
    pub text_color: Option<[f32; 4]>,
    pub bg_color: Option<[f32; 4]>,
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            hovered: false,
            text: String::new(),
            text_buf: None,
            text_offset_x: None,
            text_color: None,
            bg_color: None,
        }
    }
    pub fn with_text(mut self, text: &str) -> Self {
        self.text = text.to_string();
        self
    }
    pub fn with_text_offset_x(mut self, offset: f32) -> Self {
        self.text_offset_x = Some(offset);
        self
    }
    pub fn with_text_color(mut self, color: [f32; 4]) -> Self {
        self.text_color = Some(color);
        self
    }
    pub fn with_bg_color(mut self, color: [f32; 4]) -> Self {
        self.bg_color = Some(color);
        self
    }
    pub fn set_text_offset_x(&mut self, offset: f32) {
        self.text_offset_x = Some(offset);
    }
    pub fn set_text_color(&mut self, color: [f32; 4]) {
        self.text_color = Some(color);
    }
    pub fn set_bg_color(&mut self, color: [f32; 4]) {
        self.bg_color = Some(color);
    }
}

impl Element for StatusBar {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { self.bg_color.unwrap_or(colors::STATUS_BG) }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }
    fn set_text(&mut self, text: &str) {
        if self.text != text {
            self.text = text.to_string();
            self.text_buf = None;
        }
    }
    fn prepare_text(&mut self, fs: &mut glyphon::FontSystem) {
        if !self.text.is_empty() && self.text_buf.is_none() {
            self.text_buf = Some(make_widget_text_buffer(fs, &self.text, 12.0, "Outfit"));
        }
    }
    fn get_text_items(&self) -> Vec<(&glyphon::Buffer, f32, f32, glyphon::Color)> {
        if let Some(ref text_buf) = self.text_buf {
            let offset_x = self.text_offset_x.unwrap_or(12.0);
            let color = self.text_color.map(|c| glyphon::Color::rgb(
                (c[0] * 255.0) as u8,
                (c[1] * 255.0) as u8,
                (c[2] * 255.0) as u8,
            )).unwrap_or_else(|| glyphon::Color::rgb(0xaa, 0xaa, 0xbb));
            vec![(text_buf, self.x + offset_x, self.y + 4.0, color)]
        } else {
            Vec::new()
        }
    }
    fn text_labels(&self) -> Vec<TextLabel> {
        if !self.text.is_empty() {
            let offset_x = self.text_offset_x.unwrap_or(12.0);
            let color = self.text_color.map(|c| [
                (c[0] * 255.0) as u8,
                (c[1] * 255.0) as u8,
                (c[2] * 255.0) as u8,
            ]).unwrap_or([0xaa, 0xaa, 0xbb]);
            vec![TextLabel {
                text: self.text.clone(),
                x: self.x + offset_x,
                y: self.y + 4.0,
                font_size: 12.0,
                color,
            }]
        } else {
            Vec::new()
        }
    }
}

pub struct Splitter {
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
    dragging: bool,
    drag_ox: f32,
}

impl Splitter {
    pub fn new(w: f32) -> Self {
        Self { x: 0.0, y: 0.0, w, h: 0.0, hovered: false, dragging: false, drag_ox: 0.0 }
    }
}

impl Element for Splitter {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] {
        if self.dragging { colors::SPLITTER_DRAG }
        else if self.hovered { colors::SPLITTER_HOVER }
        else { colors::SPLITTER_IDLE }
    }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }

    fn on_cursor_moved(&mut self, px: f32, py: f32) -> bool {
        let was = self.hovered;
        self.hovered = self.hit_test(px, py);
        was != self.hovered
    }

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

    fn drag_update(&mut self, px: f32, _py: f32) -> bool {
        let new_x = px - self.drag_ox;
        if (new_x - self.x).abs() > 0.5 {
            self.x = new_x;
            return true;
        }
        false
    }

    fn drag_begin(&mut self, px: f32, _py: f32) {
        self.dragging = true;
        self.drag_ox = px - self.x;
    }

    fn drag_end(&mut self) { self.dragging = false; }
}

#[derive(Debug, Clone)]
pub struct TextItem {
    pub buffer: glyphon::Buffer,
    pub x: f32,
    pub y: f32,
    pub color: glyphon::Color,
    pub bounds: Option<[f32; 4]>,
}

// Styled label builder with optional strikethrough
#[derive(Debug)]
pub struct StyledLabel {
    pub buffer: glyphon::Buffer,
    pub w: f32,
    pub color: [f32; 4],
    pub g_color: glyphon::Color,
    pub strikethrough: bool,
    pub strikethrough_color: Option<[f32; 4]>,
}

impl StyledLabel {
    pub fn new(fs: &mut glyphon::FontSystem, text: &str, size: f32, color: [f32; 4]) -> Self {
        Self::new_with_family(fs, text, size, color, "sans-serif")
    }

    pub fn new_with_family(fs: &mut glyphon::FontSystem, text: &str, size: f32, color: [f32; 4], family: &str) -> Self {
        let metrics = glyphon::Metrics::new(size, size * 1.4);
        let mut buffer = glyphon::Buffer::new(fs, metrics);
        let attrs = glyphon::Attrs::new().family(glyphon::Family::Name(family));
        buffer.set_text(fs, text, attrs, glyphon::Shaping::Advanced);
        buffer.shape_until_scroll(fs, true);
        let w = buffer.layout_runs().next().map(|r| r.line_w).unwrap_or(0.0);
        let g_color = glyphon::Color::rgb(
            (color[0] * 255.0) as u8,
            (color[1] * 255.0) as u8,
            (color[2] * 255.0) as u8,
        );
        Self {
            buffer,
            w,
            color,
            g_color,
            strikethrough: false,
            strikethrough_color: None,
        }
    }

    pub fn with_strikethrough(mut self, enabled: bool) -> Self {
        self.strikethrough = enabled;
        self
    }

    pub fn with_strikethrough_color(mut self, color: [f32; 4]) -> Self {
        self.strikethrough_color = Some(color);
        self
    }

    pub fn draw(self, text_items: &mut Vec<TextItem>, x: f32, y: f32) -> f32 {
        let w = self.w;
        text_items.push(TextItem {
            buffer: self.buffer,
            x,
            y,
            color: self.g_color,
            bounds: None,
        });
        w
    }

    pub fn strikethrough_rect(&self, x: f32, y: f32, scale: f32) -> Option<(f32, f32, f32, f32, [f32; 4])> {
        if self.strikethrough {
            let col = self.strikethrough_color.unwrap_or(self.color);
            let font_size = self.buffer.metrics().font_size;
            let line_y = self.buffer.layout_runs().next().map(|r| r.line_y).unwrap_or(font_size * 1.05);
            let offset_y = line_y - 0.28 * font_size;
            Some((
                x,
                y + offset_y * scale,
                self.w,
                1.0 * scale,
                col,
            ))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct Separator {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub color: [f32; 4],
}

impl Separator {
    pub fn new(x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> Self {
        Self { x, y, w, h, color }
    }
}

impl Element for Separator {
    fn rect(&self) -> (f32, f32, f32, f32) {
        (self.x, self.y, self.w, self.h)
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.x = x;
        self.y = y;
        self.w = w;
        self.h = h;
    }

    fn color(&self) -> [f32; 4] {
        self.color
    }
}

fn serialize_single_widget(w: &dyn Element, json: &mut String) {
    let (x, y, width, height) = w.rect();
    let label = w.label().or_else(|| w.base().and_then(|b| b.label.clone())).unwrap_or_default();
    let focused = w.base().map_or(false, |b| b.focused);
    let hovered = w.hovered();
    let value = w.value();
    let type_name = w.type_name();

    // Escape JSON label
    let escaped_label = label.replace('\\', "\\\\").replace('"', "\\\"");

    json.push_str(&format!(
        "{{\"type\":\"{}\",\"label\":\"{}\",\"rect\":[{},{},{},{}],\"focused\":{},\"hovered\":{},\"value\":{}",
        type_name, escaped_label, x, y, width, height, focused, hovered, value
    ));

    // Handle children
    let children = w.children();
    let menu_items = w.menu_items();

    if type_name == "Menu" && w.is_menu_open() && !menu_items.is_empty() {
        json.push_str(",\"children\":[");
        let mut max_len = 0;
        for item in &menu_items {
            max_len = max_len.max(item.len());
        }
        let dw = (max_len as f32 * 7.5 + 40.0).max(120.0);
        let vertical = w.is_vertical();
        let dx = if vertical { x + width } else { x };
        let dy = if vertical { y } else { y + height };

        let checked_states = w.menu_item_checked();
        for (i, item) in menu_items.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            let item_y = dy + i as f32 * DROPDOWN_ITEM_H;
            let checked = checked_states.get(i).copied().flatten().unwrap_or(false);
            let item_escaped = item.replace('\\', "\\\\").replace('"', "\\\"");
            json.push_str(&format!(
                "{{\"type\":\"MenuItem\",\"label\":\"{}\",\"rect\":[{},{},{},{}],\"focused\":false,\"hovered\":false,\"value\":{}}}",
                item_escaped, dx, item_y, dw, DROPDOWN_ITEM_H, if checked { 1 } else { 0 }
            ));
        }
        json.push_str("]}");
    } else if !children.is_empty() {
        json.push_str(",\"children\":[");
        for (i, child_ptr) in children.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            unsafe {
                serialize_single_widget(&**child_ptr, json);
            }
        }
        json.push_str("]}");
    } else {
        json.push('}');
    }
}

pub fn serialize_widgets(widgets: &[Box<dyn Element>]) -> String {
    let mut json = String::new();
    json.push('[');
    for (i, w) in widgets.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        serialize_single_widget(&**w, &mut json);
    }
    json.push(']');
    json
}

pub struct Graph {
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
    show_network_grid: bool,
    grid_size_x: f32,
    grid_size_y: f32,
    grid_origin_x: f32,
    grid_origin_y: f32,
    skipped_row_h: f32,
    skipped_col_w: f32,
    nodes: Vec<GraphNode>,
    selected_idx: Option<usize>,
    double_clicked_idx: Option<usize>,
    double_click_timer: Option<(std::time::Instant, usize)>,
    grid_snap_enabled: bool,
    node_geom_toggled: Option<(usize, bool)>,

    // For dragging a node
    dragging_idx: Option<usize>,
    drag_ox: f32,
    drag_oy: f32,
    pub(crate) drag_node_pos: Option<(f32, f32)>,

    // Hover tracking
    toggle_hovered_idx: Option<usize>,

    uniform_background: bool,
    network_opacity: f32,
    cell_color: [f32; 3],
    gap_color: [f32; 3],
}

impl Graph {
    pub fn new() -> Self {
        Self {
            x: 0.0, y: 0.0, w: 0.0, h: 0.0,
            hovered: false,
            show_network_grid: false,
            grid_size_x: 150.0,
            grid_size_y: 75.0,
            grid_origin_x: 0.0,
            grid_origin_y: 0.0,
            skipped_row_h: 37.5,
            skipped_col_w: 37.5,
            nodes: Vec::new(),
            selected_idx: None,
            double_clicked_idx: None,
            double_click_timer: None,
            grid_snap_enabled: false,
            node_geom_toggled: None,
            dragging_idx: None,
            drag_ox: 0.0,
            drag_oy: 0.0,
            drag_node_pos: None,
            toggle_hovered_idx: None,
            uniform_background: false,
            network_opacity: 0.95,
            cell_color: [0.13, 0.13, 0.16],
            gap_color: [0.07, 0.07, 0.09],
        }
    }

    pub fn node_rect(&self, idx: usize) -> Option<(f32, f32, f32, f32)> {
        let node = self.nodes.get(idx)?;
        let (nx, ny) = if self.dragging_idx == Some(idx) {
            self.drag_node_pos.unwrap_or((
                node.position.0 * (self.grid_size_x + self.skipped_col_w) + self.grid_origin_x,
                node.position.1 * (self.grid_size_y + self.skipped_row_h) + self.grid_origin_y,
            ))
        } else {
            (
                node.position.0 * (self.grid_size_x + self.skipped_col_w) + self.grid_origin_x,
                node.position.1 * (self.grid_size_y + self.skipped_row_h) + self.grid_origin_y,
            )
        };
        Some((nx, ny, self.grid_size_x, self.grid_size_y))
    }

    pub fn toggle_rect(&self, idx: usize) -> Option<(f32, f32, f32, f32)> {
        let (nx, ny, nw, nh) = self.node_rect(idx)?;
        Some((nx + nw - 30.0, ny + (nh - 18.0) / 2.0, 18.0, 18.0))
    }

    fn find_empty_cell(&self, start_x: f32, start_y: f32, skip_idx: Option<usize>) -> (f32, f32) {
        let x = start_x;
        let mut y = start_y;
        loop {
            let occupied = self.nodes.iter().enumerate().any(|(idx, node)| {
                if Some(idx) == skip_idx {
                    false
                } else {
                    (node.position.0 - x).abs() < 0.01 && (node.position.1 - y).abs() < 0.01
                }
            });
            if occupied {
                y += 1.0;
            } else {
                break;
            }
        }
        (x, y)
    }
}

impl Element for Graph {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn set_uniform_background(&mut self, uniform: bool) { self.uniform_background = uniform; }
    fn set_network_opacity(&mut self, opacity: f32) { self.network_opacity = opacity; }
    fn set_cell_color(&mut self, color: [f32; 3]) { self.cell_color = color; }
    fn set_gap_color(&mut self, color: [f32; 3]) { self.gap_color = color; }
    fn color(&self) -> [f32; 4] {
        if self.uniform_background {
            [0.10, 0.10, 0.13, self.network_opacity]
        } else {
            [0.0, 0.0, 0.0, 0.0]
        }
    }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }
    fn hit_test(&self, px: f32, py: f32) -> bool {
        if crate::widget::popovers::is_coordinate_covered(self as *const Self as *const () as usize, px, py) {
            return false;
        }
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }

    fn set_show_network_grid(&mut self, show: bool) { self.show_network_grid = show; }
    fn set_grid_sizes(&mut self, gx: f32, gy: f32) { self.grid_size_x = gx; self.grid_size_y = gy; }
    fn set_grid_origin(&mut self, ox: f32, oy: f32) { self.grid_origin_x = ox; self.grid_origin_y = oy; }
    fn set_skipped_sizes(&mut self, row_h: f32, col_w: f32) { self.skipped_row_h = row_h; self.skipped_col_w = col_w; }

    fn set_nodes(&mut self, nodes: &[GraphNode]) {
        self.nodes = nodes.to_vec();
        if let Some(sel) = self.selected_idx {
            if sel >= self.nodes.len() {
                self.selected_idx = None;
            }
        }
    }
    fn get_nodes(&self) -> Vec<GraphNode> { self.nodes.clone() }
    fn selected_node(&self) -> Option<usize> { self.selected_idx }
    fn set_selected_node(&mut self, idx: Option<usize>) { self.selected_idx = idx; }
    fn double_clicked_node(&self) -> Option<usize> { self.double_clicked_idx }
    fn clear_double_clicked_node(&mut self) { self.double_clicked_idx = None; }
    fn set_grid_snap_enabled(&mut self, enabled: bool) { self.grid_snap_enabled = enabled; }
    fn take_node_geom_toggle(&mut self) -> Option<(usize, bool)> { self.node_geom_toggled.take() }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        for (i, node) in self.nodes.iter().enumerate() {
            if let Some((nx, ny, _nw, nh)) = self.node_rect(i) {
                let lx = nx + 8.0;
                let ly = ny + (nh - 12.0) / 2.0;
                if lx >= self.x && lx < self.x + self.w && ly >= self.y && ly < self.y + self.h {
                    labels.push(TextLabel {
                        text: node.name.clone(),
                        x: lx,
                        y: ly,
                        font_size: 14.0,
                        color: [0xcc, 0xcc, 0xd4],
                    });
                }
            }
        }
        labels
    }

    fn focus(&mut self) {
        focus::set_focused(self);
    }

    fn is_dragging(&self) -> bool { self.dragging_idx.is_some() }
    fn draggable(&self) -> bool { self.dragging_idx.is_some() }

    fn drag_begin(&mut self, px: f32, py: f32) {
        if let Some(idx) = self.dragging_idx {
            if let Some((nx, ny, _, _)) = self.node_rect(idx) {
                self.drag_ox = px - nx;
                self.drag_oy = py - ny;
                self.drag_node_pos = Some((nx, ny));
            }
        }
    }

    fn drag_update(&mut self, px: f32, py: f32) -> bool {
        if self.dragging_idx.is_some() {
            let nx = px - self.drag_ox;
            let ny = py - self.drag_oy;

            let snap_x = if self.grid_snap_enabled { self.grid_size_x + self.skipped_col_w } else { 0.0 };
            let snap_y = if self.grid_snap_enabled { self.grid_size_y + self.skipped_row_h } else { 0.0 };

            let nx = if snap_x > 0.0 {
                let relative = nx - self.grid_origin_x;
                let snapped = (relative / snap_x).round() * snap_x;
                snapped + self.grid_origin_x
            } else { nx };

            let ny = if snap_y > 0.0 {
                let relative = ny - self.grid_origin_y;
                let snapped = (relative / snap_y).round() * snap_y;
                snapped + self.grid_origin_y
            } else { ny };

            self.drag_node_pos = Some((nx, ny));
            return true;
        }
        false
    }

    fn drag_end(&mut self) {
        if let Some((nx, ny)) = self.drag_node_pos.take() {
            let c = ((nx - self.grid_origin_x) / (self.grid_size_x + self.skipped_col_w)).round();
            let r = ((ny - self.grid_origin_y) / (self.grid_size_y + self.skipped_row_h)).round();
            if let Some(idx) = self.dragging_idx.take() {
                let (nx, ny) = self.find_empty_cell(c, r, Some(idx));
                self.nodes[idx].position = (nx, ny);
            }
        } else {
            self.dragging_idx = None;
        }
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32) -> bool {
        let was_toggle_hovered = self.toggle_hovered_idx;
        self.toggle_hovered_idx = None;
        for i in 0..self.nodes.len() {
            if let Some((tx, ty, tw, th)) = self.toggle_rect(i) {
                if px >= tx && px < tx + tw && py >= ty && py < ty + th {
                    self.toggle_hovered_idx = Some(i);
                    break;
                }
            }
        }
        was_toggle_hovered != self.toggle_hovered_idx
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button != MouseButton::Left { return false; }
        match state {
            ElementState::Pressed => {
                for i in (0..self.nodes.len()).rev() {
                    if let Some((tx, ty, tw, th)) = self.toggle_rect(i) {
                        if px >= tx && px < tx + tw && py >= ty && py < ty + th {
                            self.nodes[i].geom_visible = !self.nodes[i].geom_visible;
                            self.node_geom_toggled = Some((i, self.nodes[i].geom_visible));
                            return true;
                        }
                    }
                    if let Some((nx, ny, nw, nh)) = self.node_rect(i) {
                        if px >= nx && px < nx + nw && py >= ny && py < ny + nh {
                            let now = std::time::Instant::now();
                            if let Some((prev_time, prev_idx)) = self.double_click_timer {
                                if prev_idx == i && now.duration_since(prev_time) < std::time::Duration::from_millis(500) {
                                    self.double_clicked_idx = Some(i);
                                }
                            }
                            self.double_click_timer = Some((now, i));
                            self.selected_idx = Some(i);
                            self.dragging_idx = Some(i);
                            self.drag_ox = px - nx;
                            self.drag_oy = py - ny;
                            self.drag_node_pos = Some((nx, ny));
                            self.focus();
                            return true;
                        }
                    }
                }
                self.selected_idx = None;
                false
            }
            ElementState::Released => {
                if self.dragging_idx.is_some() {
                    self.drag_end();
                    return true;
                }
                false
            }
        }
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        let min_x = self.x;
        let min_y = self.y;
        let max_x = self.x + self.w;
        let max_y = self.y + self.h;
        let push_clipped = |qx: f32, qy: f32, qw: f32, qh: f32, qc: [f32; 4], q: &mut Vec<(f32, f32, f32, f32, [f32; 4])>| {
            let rx1 = qx.max(min_x);
            let ry1 = qy.max(min_y);
            let rx2 = (qx + qw).min(max_x);
            let ry2 = (qy + qh).min(max_y);
            let rw = rx2 - rx1;
            let rh = ry2 - ry1;
            if rw > 0.0 && rh > 0.0 {
                q.push((rx1, ry1, rw, rh, qc));
            }
        };

        // Draw connection wires
        let wire_color = [0.0, 0.75, 1.0, 0.7]; // Vibrant cyan glow
        let wire_thickness = 3.0;
        for i in 0..self.nodes.len() {
            let node = &self.nodes[i];
            if let Some((_, input_name, _)) = node.parameters.iter().find(|(name, _, _)| name.eq_ignore_ascii_case("input")) {
                if let Some(src_idx) = self.nodes.iter().position(|n| n.name == *input_name) {
                    if let (Some((sx, sy, sw, sh)), Some((ex, ey, ew, _eh))) = (self.node_rect(src_idx), self.node_rect(i)) {
                        let start_x = sx + sw / 2.0;
                        let start_y = sy + sh;
                        let end_x = ex + ew / 2.0;
                        let end_y = ey;

                        let mid_y = start_y + (end_y - start_y) / 2.0;

                        // Vertical segment 1
                        let v1_min_y = start_y.min(mid_y);
                        let v1_max_y = start_y.max(mid_y);
                        push_clipped(
                            start_x - wire_thickness / 2.0,
                            v1_min_y,
                            wire_thickness,
                            v1_max_y - v1_min_y,
                            wire_color,
                            &mut quads,
                        );

                        // Horizontal segment
                        let h_min_x = start_x.min(end_x);
                        let h_max_x = start_x.max(end_x);
                        push_clipped(
                            h_min_x,
                            mid_y - wire_thickness / 2.0,
                            h_max_x - h_min_x,
                            wire_thickness,
                            wire_color,
                            &mut quads,
                        );

                        // Vertical segment 2
                        let v2_min_y = mid_y.min(end_y);
                        let v2_max_y = mid_y.max(end_y);
                        push_clipped(
                            end_x - wire_thickness / 2.0,
                            v2_min_y,
                            wire_thickness,
                            v2_max_y - v2_min_y,
                            wire_color,
                            &mut quads,
                        );
                    }
                }
            }
        }

        if self.show_network_grid && self.grid_size_x > 0.0 && self.grid_size_y > 0.0 && !self.uniform_background {
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

                // Draw gap color as solid background color of grid
                quads.push((self.x, self.y, self.w, self.h, [self.gap_color[0], self.gap_color[1], self.gap_color[2], self.network_opacity]));

                // Draw filled cells with cell color
                for r in ry_start..=ry_end {
                    let y1 = self.grid_origin_y + (r as f32) * step_y;
                    for c in cx_start..=cx_end {
                        let x1 = self.grid_origin_x + (c as f32) * step_x;
                        let cell_x = x1.max(self.x);
                        let cell_y = y1.max(self.y);
                        let cell_w = (x1 + self.grid_size_x).min(self.x + self.w) - cell_x;
                        let cell_h = (y1 + self.grid_size_y).min(self.y + self.h) - cell_y;
                        if cell_w > 0.0 && cell_h > 0.0 {
                            quads.push((cell_x, cell_y, cell_w, cell_h, [self.cell_color[0], self.cell_color[1], self.cell_color[2], self.network_opacity]));
                        }
                    }
                }

                let grid_line_color = [0.18, 0.18, 0.22, 0.40];

                for k in ry_start..=ry_end {
                    let y1 = self.grid_origin_y + (k as f32) * step_y;
                    let y2 = y1 + self.grid_size_y;
                    if y1 < self.y + self.h {
                        if y1 >= self.y {
                            quads.push((self.x, y1, self.w, 1.0, grid_line_color));
                        }
                        if y2 >= self.y && y2 < self.y + self.h {
                            quads.push((self.x, y2, self.w, 1.0, grid_line_color));
                        }
                    }
                }

                for k in cx_start..=cx_end {
                    let x1 = self.grid_origin_x + (k as f32) * step_x;
                    let x2 = x1 + self.grid_size_x;
                    if x1 < self.x + self.w {
                        if x1 >= self.x {
                            quads.push((x1, self.y, 1.0, self.h, grid_line_color));
                        }
                        if x2 >= self.x && x2 < self.x + self.w {
                            quads.push((x2, self.y, 1.0, self.h, grid_line_color));
                        }
                    }
                }
            }
        }

        for i in 0..self.nodes.len() {
            if let Some((nx, ny, nw, nh)) = self.node_rect(i) {
                let bg_color = if self.dragging_idx == Some(i) {
                    colors::node_drag_color()
                } else if self.selected_idx == Some(i) {
                    colors::node_selected_color()
                } else {
                    colors::node_color()
                };
                push_clipped(nx, ny, nw, nh, bg_color, &mut quads);

                if let Some((tx, ty, tw, th)) = self.toggle_rect(i) {
                    let btn_color = if self.toggle_hovered_idx == Some(i) {
                        colors::TOGGLE_HOVER
                    } else {
                        colors::TOGGLE_OFF
                    };
                    push_clipped(tx, ty, tw, th, btn_color, &mut quads);

                    if self.nodes[i].geom_visible {
                        let inset = 3.0;
                        push_clipped(tx + inset, ty + inset, tw - inset * 2.0, th - inset * 2.0, colors::TOGGLE_ON, &mut quads);
                    }
                }
            }
        }

        quads
    }

    fn grid_origin(&self) -> (f32, f32) {
        (self.grid_origin_x, self.grid_origin_y)
    }

    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32) -> bool {
        if self.hit_test(px, py) {
            match delta {
                MouseScrollDelta::LineDelta(x, y) => {
                    self.grid_origin_x += *x * 15.0;
                    self.grid_origin_y += *y * 15.0;
                    true
                }
                MouseScrollDelta::PixelDelta(pos) => {
                    self.grid_origin_x += pos.x as f32;
                    self.grid_origin_y += pos.y as f32;
                    true
                }
            }
        } else {
            false
        }
    }
}


// ── Reusable Widgets added for UI standardization ──

#[derive(Debug, Clone)]
pub struct UsageBar {
    base: Widget,
    pub value: f32, // 0.0 to 1.0
    pub fill_color: [f32; 4],
    pub bg_color: [f32; 4],
}

impl UsageBar {
    pub fn new(value: f32) -> Self {
        Self {
            base: Widget::new(),
            value: value.clamp(0.0, 1.0),
            fill_color: [0.30, 0.50, 0.32, 1.0], // green-ish
            bg_color: [0.15, 0.15, 0.24, 1.0], // dark-ish
        }
    }
    
    pub fn with_colors(mut self, fill: [f32; 4], bg: [f32; 4]) -> Self {
        self.fill_color = fill;
        self.bg_color = bg;
        self
    }
    
    pub fn set_value(&mut self, value: f32) {
        self.value = value.clamp(0.0, 1.0);
    }
}

impl Element for UsageBar {
    crate::impl_widget_base!(UsageBar);
    fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }
    fn highlight_quad(&self) -> Option<(f32, f32, f32, f32, [f32; 4])> { None }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let (x, y, w, h) = self.rect();
        vec![
            (x, y, w, h, self.bg_color),
            (x, y, w * self.value, h, self.fill_color),
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewLayoutMode {
    Fullscreen,
    Cascade,
    Stack,
    Grid,
    LeftTiled,
    RightTiled,
    Equal,
    Spiral,
    Floating,
}

#[derive(Debug, Clone)]
pub struct LayoutPreview {
    base: Widget,
    pub mode: PreviewLayoutMode,
    pub is_active: bool,
}

impl LayoutPreview {
    pub fn new(mode: PreviewLayoutMode) -> Self {
        Self {
            base: Widget::new(),
            mode,
            is_active: false,
        }
    }

    pub fn with_active(mut self, active: bool) -> Self {
        self.is_active = active;
        self
    }
    
    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self
    }
}

impl Element for LayoutPreview {
    crate::impl_widget_base!(LayoutPreview);
    fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }
    fn highlight_quad(&self) -> Option<(f32, f32, f32, f32, [f32; 4])> { None }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let (x, y, w, h) = self.rect();
        let mut quads = Vec::new();

        let bg_col = if self.is_active { [0.12, 0.24, 0.14, 0.55] } else { [0.08, 0.08, 0.12, 0.35] };
        let border_col = if self.is_active { [0.36, 0.56, 0.38, 0.95] } else { [0.24, 0.24, 0.28, 0.45] };

        quads.push((x, y, w, h, bg_col));
        quads.push((x, y, w, 1.0, border_col));
        quads.push((x, y + h - 1.0, w, 1.0, border_col));
        quads.push((x, y, 1.0, h, border_col));
        quads.push((x + w - 1.0, y, 1.0, h, border_col));

        let preview_x = x + 8.0;
        let preview_y = y + 38.0;
        let preview_w = w - 16.0;
        let preview_h = h - 46.0;

        if preview_w > 0.0 && preview_h > 0.0 {
            quads.push((preview_x, preview_y, preview_w, preview_h, [0.16, 0.16, 0.20, 0.6]));
            let preview_border = [0.22, 0.22, 0.26, 0.8];
            quads.push((preview_x, preview_y, preview_w, 1.0, preview_border));
            quads.push((preview_x, preview_y + preview_h - 1.0, preview_w, 1.0, preview_border));
            quads.push((preview_x, preview_y, 1.0, preview_h, preview_border));
            quads.push((preview_x + preview_w - 1.0, preview_y, 1.0, preview_h, preview_border));

            let mut nodes = Vec::new();
            match self.mode {
                PreviewLayoutMode::Fullscreen => {
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: preview_w - 4.0, h: preview_h - 4.0, label: "F".to_string() });
                }
                PreviewLayoutMode::Cascade => {
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: preview_w - 12.0, h: preview_h - 12.0, label: "1".to_string() });
                    nodes.push(SimNode { x: 6.0, y: 6.0, w: preview_w - 12.0, h: preview_h - 12.0, label: "2".to_string() });
                    nodes.push(SimNode { x: 10.0, y: 10.0, w: preview_w - 12.0, h: preview_h - 12.0, label: "3".to_string() });
                }
                PreviewLayoutMode::Stack => {
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: preview_w - 4.0, h: preview_h - 4.0, label: "Stack".to_string() });
                }
                PreviewLayoutMode::Grid => {
                    let hw = (preview_w - 6.0) / 2.0;
                    let hh = (preview_h - 6.0) / 2.0;
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: hw, h: hh, label: "1".to_string() });
                    nodes.push(SimNode { x: 4.0 + hw, y: 2.0, w: hw, h: hh, label: "2".to_string() });
                    nodes.push(SimNode { x: 2.0, y: 4.0 + hh, w: hw, h: hh, label: "3".to_string() });
                    nodes.push(SimNode { x: 4.0 + hw, y: 4.0 + hh, w: hw, h: hh, label: "4".to_string() });
                }
                PreviewLayoutMode::LeftTiled => {
                    let mw = (preview_w - 6.0) * 0.55;
                    let sw = (preview_w - 6.0) - mw;
                    let sh = (preview_h - 6.0) / 2.0;
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: mw, h: preview_h - 4.0, label: "M".to_string() });
                    nodes.push(SimNode { x: 4.0 + mw, y: 2.0, w: sw, h: sh, label: "1".to_string() });
                    nodes.push(SimNode { x: 4.0 + mw, y: 4.0 + sh, w: sw, h: sh, label: "2".to_string() });
                }
                PreviewLayoutMode::RightTiled => {
                    let mw = (preview_w - 6.0) * 0.55;
                    let sw = (preview_w - 6.0) - mw;
                    let sh = (preview_h - 6.0) / 2.0;
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: sw, h: sh, label: "1".to_string() });
                    nodes.push(SimNode { x: 2.0, y: 4.0 + sh, w: sw, h: sh, label: "2".to_string() });
                    nodes.push(SimNode { x: 4.0 + sw, y: 2.0, w: mw, h: preview_h - 4.0, label: "M".to_string() });
                }
                PreviewLayoutMode::Equal => {
                    let ew = (preview_w - 8.0) / 3.0;
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: ew, h: preview_h - 4.0, label: "1".to_string() });
                    nodes.push(SimNode { x: 4.0 + ew, y: 2.0, w: ew, h: preview_h - 4.0, label: "2".to_string() });
                    nodes.push(SimNode { x: 6.0 + 2.0 * ew, y: 2.0, w: ew, h: preview_h - 4.0, label: "3".to_string() });
                }
                PreviewLayoutMode::Spiral => {
                    let w1 = (preview_w - 6.0) * 0.5;
                    let w2 = (preview_w - 6.0) - w1;
                    let h2 = (preview_h - 6.0) * 0.5;
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: w1, h: preview_h - 4.0, label: "1".to_string() });
                    nodes.push(SimNode { x: 4.0 + w1, y: 2.0, w: w2, h: h2, label: "2".to_string() });
                    nodes.push(SimNode { x: 4.0 + w1, y: 4.0 + h2, w: w2 * 0.5, h: h2, label: "3".to_string() });
                    nodes.push(SimNode { x: 4.0 + w1 + w2 * 0.5, y: 4.0 + h2, w: w2 * 0.5, h: h2, label: "4".to_string() });
                }
                PreviewLayoutMode::Floating => {
                    nodes.push(SimNode { x: 4.0, y: 6.0, w: preview_w * 0.45, h: preview_h * 0.5, label: "1".to_string() });
                    nodes.push(SimNode { x: preview_w * 0.4, y: 12.0, w: preview_w * 0.5, h: preview_h * 0.45, label: "2".to_string() });
                    nodes.push(SimNode { x: 8.0, y: preview_h * 0.4, w: preview_w * 0.55, h: preview_h * 0.5, label: "3".to_string() });
                }
            }

            for node in nodes {
                let rect_x = preview_x + node.x;
                let rect_y = preview_y + node.y;
                let node_bg = if self.is_active { [0.30, 0.45, 0.65, 0.45] } else { [0.20, 0.24, 0.30, 0.25] };
                let node_border = if self.is_active { [0.45, 0.65, 0.90, 0.85] } else { [0.35, 0.40, 0.45, 0.55] };

                quads.push((rect_x, rect_y, node.w, node.h, node_bg));
                quads.push((rect_x, rect_y, node.w, 1.0, node_border));
                quads.push((rect_x, rect_y + node.h - 1.0, node.w, 1.0, node_border));
                quads.push((rect_x, rect_y, 1.0, node.h, node_border));
                quads.push((rect_x + node.w - 1.0, rect_y, 1.0, node.h, node_border));
            }
        }

        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let (x, y, w, h) = self.rect();
        let mut labels = Vec::new();

        labels.push(TextLabel {
            text: self.base.label.clone().unwrap_or_else(|| "TAG".to_string()),
            x: x + 8.0,
            y: y + 8.0,
            font_size: 10.0,
            color: [140, 140, 153],
        });

        let layout_name = match self.mode {
            PreviewLayoutMode::Fullscreen => "Fullscreen",
            PreviewLayoutMode::Cascade => "Cascade",
            PreviewLayoutMode::Stack => "Stack",
            PreviewLayoutMode::Grid => "Grid",
            PreviewLayoutMode::LeftTiled => "L-Tiled",
            PreviewLayoutMode::RightTiled => "R-Tiled",
            PreviewLayoutMode::Equal => "Equal",
            PreviewLayoutMode::Spiral => "Spiral",
            PreviewLayoutMode::Floating => "Floating",
        };

        labels.push(TextLabel {
            text: layout_name.to_string(),
            x: x + 8.0,
            y: y + 20.0,
            font_size: 13.0,
            color: [230, 230, 242],
        });

        let preview_x = x + 8.0;
        let preview_y = y + 38.0;
        let preview_w = w - 16.0;
        let preview_h = h - 46.0;

        if preview_w > 0.0 && preview_h > 0.0 {
            let mut nodes = Vec::new();
            match self.mode {
                PreviewLayoutMode::Fullscreen => {
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: preview_w - 4.0, h: preview_h - 4.0, label: "F".to_string() });
                }
                PreviewLayoutMode::Cascade => {
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: preview_w - 12.0, h: preview_h - 12.0, label: "1".to_string() });
                    nodes.push(SimNode { x: 6.0, y: 6.0, w: preview_w - 12.0, h: preview_h - 12.0, label: "2".to_string() });
                    nodes.push(SimNode { x: 10.0, y: 10.0, w: preview_w - 12.0, h: preview_h - 12.0, label: "3".to_string() });
                }
                PreviewLayoutMode::Stack => {
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: preview_w - 4.0, h: preview_h - 4.0, label: "Stack".to_string() });
                }
                PreviewLayoutMode::Grid => {
                    let hw = (preview_w - 6.0) / 2.0;
                    let hh = (preview_h - 6.0) / 2.0;
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: hw, h: hh, label: "1".to_string() });
                    nodes.push(SimNode { x: 4.0 + hw, y: 2.0, w: hw, h: hh, label: "2".to_string() });
                    nodes.push(SimNode { x: 2.0, y: 4.0 + hh, w: hw, h: hh, label: "3".to_string() });
                    nodes.push(SimNode { x: 4.0 + hw, y: 4.0 + hh, w: hw, h: hh, label: "4".to_string() });
                }
                PreviewLayoutMode::LeftTiled => {
                    let mw = (preview_w - 6.0) * 0.55;
                    let sw = (preview_w - 6.0) - mw;
                    let sh = (preview_h - 6.0) / 2.0;
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: mw, h: preview_h - 4.0, label: "M".to_string() });
                    nodes.push(SimNode { x: 4.0 + mw, y: 2.0, w: sw, h: sh, label: "1".to_string() });
                    nodes.push(SimNode { x: 4.0 + mw, y: 4.0 + sh, w: sw, h: sh, label: "2".to_string() });
                }
                PreviewLayoutMode::RightTiled => {
                    let mw = (preview_w - 6.0) * 0.55;
                    let sw = (preview_w - 6.0) - mw;
                    let sh = (preview_h - 6.0) / 2.0;
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: sw, h: sh, label: "1".to_string() });
                    nodes.push(SimNode { x: 2.0, y: 4.0 + sh, w: sw, h: sh, label: "2".to_string() });
                    nodes.push(SimNode { x: 4.0 + sw, y: 2.0, w: mw, h: preview_h - 4.0, label: "M".to_string() });
                }
                PreviewLayoutMode::Equal => {
                    let ew = (preview_w - 8.0) / 3.0;
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: ew, h: preview_h - 4.0, label: "1".to_string() });
                    nodes.push(SimNode { x: 4.0 + ew, y: 2.0, w: ew, h: preview_h - 4.0, label: "2".to_string() });
                    nodes.push(SimNode { x: 6.0 + 2.0 * ew, y: 2.0, w: ew, h: preview_h - 4.0, label: "3".to_string() });
                }
                PreviewLayoutMode::Spiral => {
                    let w1 = (preview_w - 6.0) * 0.5;
                    let w2 = (preview_w - 6.0) - w1;
                    let h2 = (preview_h - 6.0) * 0.5;
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: w1, h: preview_h - 4.0, label: "1".to_string() });
                    nodes.push(SimNode { x: 4.0 + w1, y: 2.0, w: w2, h: h2, label: "2".to_string() });
                    nodes.push(SimNode { x: 4.0 + w1, y: 4.0 + h2, w: w2 * 0.5, h: h2, label: "3".to_string() });
                    nodes.push(SimNode { x: 4.0 + w1 + w2 * 0.5, y: 4.0 + h2, w: w2 * 0.5, h: h2, label: "4".to_string() });
                }
                PreviewLayoutMode::Floating => {
                    nodes.push(SimNode { x: 4.0, y: 6.0, w: preview_w * 0.45, h: preview_h * 0.5, label: "1".to_string() });
                    nodes.push(SimNode { x: preview_w * 0.4, y: 12.0, w: preview_w * 0.5, h: preview_h * 0.45, label: "2".to_string() });
                    nodes.push(SimNode { x: 8.0, y: preview_h * 0.4, w: preview_w * 0.55, h: preview_h * 0.5, label: "3".to_string() });
                }
            }

            for node in nodes {
                let rect_x = preview_x + node.x;
                let rect_y = preview_y + node.y;
                let text_sz = 9.0;
                let text_w = node.label.len() as f32 * 6.0;
                let text_color = if self.is_active { [242, 242, 255] } else { [178, 178, 191] };
                let tx_offset = ((node.w - text_w) / 2.0).max(1.0);
                let ty_offset = ((node.h - text_sz) / 2.0).max(1.0);

                labels.push(TextLabel {
                    text: node.label,
                    x: rect_x + tx_offset,
                    y: rect_y + ty_offset,
                    font_size: text_sz,
                    color: text_color,
                });
            }
        }

        labels
    }
}

struct SimNode {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    label: String,
}

#[derive(Debug, Clone)]
pub struct FontPreview {
    base: Widget,
    pub font_family: String,
}

impl FontPreview {
    pub fn new(font_family: String) -> Self {
        Self {
            base: Widget::new(),
            font_family,
        }
    }

    pub fn set_font_family(&mut self, font_family: String) {
        self.font_family = font_family;
    }
}

impl Element for FontPreview {
    crate::impl_widget_base!(FontPreview);
    fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }
    fn highlight_quad(&self) -> Option<(f32, f32, f32, f32, [f32; 4])> { None }
    fn widget_font(&self) -> Option<String> { Some(self.font_family.clone()) }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let (x, y, w, h) = self.rect();
        let border_t = 1.0;
        let card_color = [0.10, 0.10, 0.14, 0.3];
        let border_color = [0.25, 0.25, 0.35, 0.5];
        vec![
            (x, y, w, h, card_color),
            (x, y, w, border_t, border_color),
            (x, y + h - border_t, w, border_t, border_color),
            (x, y, border_t, h, border_color),
            (x + w - border_t, y, border_t, h, border_color),
            (x + 16.0, y + 44.0, w - 32.0, 1.0, [0.22, 0.22, 0.30, 0.8]),
        ]
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let (x, y, _w, _h) = self.rect();
        vec![
            TextLabel {
                text: format!("Family: {}", self.font_family),
                x: x + 16.0,
                y: y + 16.0,
                font_size: 15.0,
                color: [230, 230, 242],
            },
            TextLabel {
                text: "abcdefghijklmnopqrstuvwxyz".to_string(),
                x: x + 16.0,
                y: y + 61.0,
                font_size: 13.0,
                color: [191, 191, 204],
            },
            TextLabel {
                text: "ABCDEFGHIJKLMNOPQRSTUVWXYZ".to_string(),
                x: x + 16.0,
                y: y + 83.0,
                font_size: 13.0,
                color: [191, 191, 204],
            },
            TextLabel {
                text: "0123456789 (!@#$%&*?)".to_string(),
                x: x + 16.0,
                y: y + 105.0,
                font_size: 13.0,
                color: [191, 191, 204],
            },
            TextLabel {
                text: "The quick brown fox jumps over the lazy dog.".to_string(),
                x: x + 16.0,
                y: y + 131.0,
                font_size: 16.0,
                color: [230, 230, 242],
            },
            TextLabel {
                text: "The five boxing wizards jump quickly.".to_string(),
                x: x + 16.0,
                y: y + 163.0,
                font_size: 20.0,
                color: [255, 255, 255],
            },
        ]
    }
}

#[derive(Debug, Clone)]
pub struct InfoBox {
    base: Widget,
    pub title: String,
    pub lines: Vec<String>,
}

impl InfoBox {
    pub fn new(title: &str, lines: Vec<String>) -> Self {
        Self {
            base: Widget::new(),
            title: title.to_string(),
            lines,
        }
    }
}

impl Element for InfoBox {
    crate::impl_widget_base!(InfoBox);
    fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }
    fn highlight_quad(&self) -> Option<(f32, f32, f32, f32, [f32; 4])> { None }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let (x, y, w, h) = self.rect();
        let theme = colors::active_theme();
        let bg_color = theme.surface_bg;
        let border_color = theme.surface_border;
        let border_t = 1.0;
        vec![
            (x, y, w, h, bg_color),
            (x, y, w, border_t, border_color),
            (x, y + h - border_t, w, border_t, border_color),
            (x, y, border_t, h, border_color),
            (x + w - border_t, y, border_t, h, border_color),
        ]
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let (x, y, _w, _h) = self.rect();
        let mut labels = Vec::new();
        labels.push(TextLabel {
            text: self.title.clone(),
            x: x + 16.0,
            y: y + 12.0,
            font_size: 12.0,
            color: [89, 165, 229],
        });
        
        let mut current_y = y + 32.0;
        for (idx, line) in self.lines.iter().enumerate() {
            let color = if idx == self.lines.len() - 1 {
                [140, 140, 153]
            } else {
                [204, 204, 217]
            };
            labels.push(TextLabel {
                text: line.clone(),
                x: x + 16.0,
                y: current_y,
                font_size: 11.0,
                color,
            });
            current_y += 16.0;
        }
        labels
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DotStatus {
    Active,
    Inactive,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct StatusDot {
    base: Widget,
    pub status: DotStatus,
}

impl StatusDot {
    pub fn new(status: DotStatus) -> Self {
        Self {
            base: Widget::new(),
            status,
        }
    }

    pub fn set_status(&mut self, status: DotStatus) {
        self.status = status;
    }
}

impl Element for StatusDot {
    crate::impl_widget_base!(StatusDot);
    fn highlight_quad(&self) -> Option<(f32, f32, f32, f32, [f32; 4])> { None }

    fn color(&self) -> [f32; 4] {
        match self.status {
            DotStatus::Active => [0.20, 0.70, 0.35, 1.0],
            DotStatus::Inactive => [0.50, 0.50, 0.55, 1.0],
            DotStatus::Warning => [0.90, 0.60, 0.10, 1.0],
            DotStatus::Error => [0.85, 0.25, 0.25, 1.0],
        }
    }
}

#[derive(Debug, Clone)]
pub struct InteractiveListItem {
    base: Widget,
    pub title: String,
    pub subtitle: Option<String>,
    pub selected: bool,
    pub pressed: bool,
    pub just_clicked: bool,
}

impl InteractiveListItem {
    pub fn new(title: &str) -> Self {
        Self {
            base: Widget::new(),
            title: title.to_string(),
            subtitle: None,
            selected: false,
            pressed: false,
            just_clicked: false,
        }
    }

    pub fn with_subtitle(mut self, subtitle: &str) -> Self {
        self.subtitle = Some(subtitle.to_string());
        self
    }

    pub fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }
}

impl Element for InteractiveListItem {
    crate::impl_widget_base!(InteractiveListItem);
    fn highlight_quad(&self) -> Option<(f32, f32, f32, f32, [f32; 4])> { None }

    fn color(&self) -> [f32; 4] {
        let theme = colors::active_theme();
        if self.selected {
            let mut base_color = theme.primary_accent;
            if self.pressed { base_color[3] = (base_color[3] + theme.press_overlay[3]).min(1.0); }
            else if self.base.hovered { base_color[3] = (base_color[3] + theme.hover_overlay[3]).min(1.0); }
            base_color
        } else {
            if self.pressed {
                let mut base_color = theme.surface_bg;
                base_color[3] = (base_color[3] + theme.press_overlay[3]).min(1.0);
                base_color
            } else if self.base.hovered {
                let mut base_color = theme.surface_bg;
                base_color[3] = (base_color[3] + theme.hover_overlay[3]).min(1.0);
                base_color
            } else {
                [0.0, 0.0, 0.0, 0.0]
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
        let (x, y, _w, h) = self.rect();
        let mut labels = Vec::new();
        
        let title_y = if self.subtitle.is_some() {
            y + (h - 22.0) / 2.0
        } else {
            y + (h - 12.0) / 2.0
        };

        labels.push(TextLabel {
            text: self.title.clone(),
            x: x + 8.0,
            y: title_y,
            font_size: 12.0,
            color: [220, 220, 230],
        });

        if let Some(ref sub) = self.subtitle {
            labels.push(TextLabel {
                text: sub.clone(),
                x: x + 8.0,
                y: title_y + 13.0,
                font_size: 10.0,
                color: [140, 140, 153],
            });
        }

        labels
    }
}





