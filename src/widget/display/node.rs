use crate::colors;
use crate::widget::*;

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

    pub fn set_grid_snap(&mut self, gx: f32, gy: f32) { self.grid_snap_x = gx; self.grid_snap_y = gy; }
    pub fn set_grid_origin(&mut self, ox: f32, oy: f32) { self.grid_origin_x = ox; self.grid_origin_y = oy; }
    pub fn set_node_name(&mut self, name: &str) { self.name = name.to_string(); }
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

    fn as_param_controller(&self) -> Option<&dyn ParamController> { Some(self) }
    fn as_param_controller_mut(&mut self) -> Option<&mut dyn ParamController> { Some(self) }
    fn as_geom_controller(&self) -> Option<&dyn GeomController> { Some(self) }
    fn as_geom_controller_mut(&mut self) -> Option<&mut dyn GeomController> { Some(self) }

    fn text_labels(&self) -> Vec<TextLabel> {
        vec![TextLabel {
            text: self.name.clone(),
            x: self.x + self.w + 8.0,
            y: crate::layout::align_text_y(self.y, self.h, 14.0, 0.0),
            font_size: 14.0,
            color: [0xcc, 0xcc, 0xd4],
        }]
    }

    fn set_drag_bounds(&mut self, bx: f32, by: f32, bw: f32, bh: f32) {
        self.bounds = Some((bx, by, bw, bh));
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        let was_hovered = self.hovered;
        self.hovered = self.hit_test(px, py, ctx);

        let was_toggle_hovered = self.toggle_hovered;
        let (tx, ty, tw, th) = self.toggle_rect();
        self.toggle_hovered = px >= tx && px < tx + tw && py >= ty && py < ty + th;

        was_hovered != self.hovered || was_toggle_hovered != self.toggle_hovered
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if button != MouseButton::Left { return false; }
        match state {
            ElementState::Pressed => {
                if self.hit_test(px, py, ctx) {
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

}

impl ParamController for Node {
    fn node_params(&self) -> Vec<(String, String, String)> { self.parameters.clone() }
    fn set_display_params(&mut self, params: &[(String, String, String)]) {
        self.parameters = params.to_vec();
    }
}

impl GeomController for Node {
    fn set_geom_visible(&mut self, visible: bool) { self.geom_visible = visible; }
    fn geom_visible(&self) -> bool { self.geom_visible }
    fn take_geom_toggle(&mut self) -> bool { std::mem::take(&mut self.geom_toggled) }
}

impl Drop for Node {
    fn drop(&mut self) {
        clear_widget_references(self);
    }
}
