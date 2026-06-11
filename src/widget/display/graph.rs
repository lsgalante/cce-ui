use crate::colors;
use crate::widget::*;
use crate::widget::display::TextLabel;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct GraphNode {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub position: (f32, f32), // (column, row)
    pub parameters: Vec<(String, String, String)>, // (name, value, type)
    pub geom_visible: bool,
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
    selected_id: Option<String>,
    double_clicked_idx: Option<usize>,
    double_click_timer: Option<(std::time::Instant, usize)>,
    grid_snap_enabled: bool,
    node_geom_toggled: Option<(usize, bool)>,

    // For dragging a node
    dragging_idx: Option<usize>,
    dragging_id: Option<String>,
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
    pub fn set_uniform_background(&mut self, uniform: bool) {
        self.uniform_background = uniform;
    }
    pub fn set_network_opacity(&mut self, opacity: f32) {
        self.network_opacity = opacity;
    }
    pub fn set_cell_color(&mut self, color: [f32; 3]) {
        self.cell_color = color;
    }
    pub fn set_gap_color(&mut self, color: [f32; 3]) {
        self.gap_color = color;
    }

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
            selected_id: None,
            double_clicked_idx: None,
            double_click_timer: None,
            grid_snap_enabled: false,
            node_geom_toggled: None,
            dragging_idx: None,
            dragging_id: None,
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
        let scale_f = nw / 80.0;
        let size = (18.0 * scale_f).clamp(6.0, 50.0);
        Some((nx + nw - size - 6.0 * scale_f, ny + (nh - size) / 2.0, size, size))
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
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn as_ptr(&self) -> *mut (dyn Element + 'static) {
        self as *const Self as *mut Self as *mut (dyn Element + 'static)
    }

    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }

    fn color(&self) -> [f32; 4] {
        if self.uniform_background {
            [0.10, 0.10, 0.13, self.network_opacity]
        } else {
            [0.0, 0.0, 0.0, 0.0]
        }
    }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }
    fn hit_test(&self, px: f32, py: f32, ctx: &UiContext) -> bool {
        if ctx.is_coordinate_covered(self as *const Self as *const () as usize, px, py) {
            return false;
        }
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }

    fn as_graph_controller(&self) -> Option<&dyn GraphController> { Some(self) }
    fn as_graph_controller_mut(&mut self) -> Option<&mut dyn GraphController> { Some(self) }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        for (i, node) in self.nodes.iter().enumerate() {
            if let Some((nx, ny, nw, nh)) = self.node_rect(i) {
                let scale_f = nw / 80.0;
                let font_size = (14.0 * scale_f).clamp(6.0, 48.0);
                let lx = nx + nw + 8.0 * scale_f;
                let ly = ny + (nh - font_size) / 2.0;
                if lx >= self.x && lx < self.x + self.w && ly >= self.y && ly < self.y + self.h {
                    labels.push(TextLabel {
                        text: node.name.clone(),
                        x: lx,
                        y: ly,
                        font_size,
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
        self.dragging_id = None;
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
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

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
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
                            self.selected_id = Some(self.nodes[i].id.clone());
                            self.dragging_idx = Some(i);
                            self.dragging_id = Some(self.nodes[i].id.clone());
                            self.drag_ox = px - nx;
                            self.drag_oy = py - ny;
                            self.drag_node_pos = Some((nx, ny));
                            self.focus();
                            return true;
                        }
                    }
                }
                self.selected_idx = None;
                self.selected_id = None;
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
        let scale_f = self.grid_size_x / 80.0;
        let wire_color = [0.0, 0.75, 1.0, 0.7]; // Vibrant cyan glow
        let wire_thickness = (3.0 * scale_f).clamp(1.0, 15.0);
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
                let ry_start = (((self.y - self.grid_origin_y) / step_y).floor() as i32 - 1).max(-100_000);
                let ry_end = (((self.y + self.h - self.grid_origin_y) / step_y).ceil() as i32 + 1).min(100_000);

                let cx_start = (((self.x - self.grid_origin_x) / step_x).floor() as i32 - 1).max(-100_000);
                let cx_end = (((self.x + self.w - self.grid_origin_x) / step_x).ceil() as i32 + 1).min(100_000);

                // Draw gap color as solid background color of grid
                push_clipped(self.x, self.y, self.w, self.h, [self.gap_color[0], self.gap_color[1], self.gap_color[2], self.network_opacity], &mut quads);

                // Draw filled cells with cell color
                for r in ry_start..=ry_end {
                    let y1 = (self.grid_origin_y + (r as f32) * step_y).round();
                    for c in cx_start..=cx_end {
                        let x1 = (self.grid_origin_x + (c as f32) * step_x).round();
                        push_clipped(
                            x1,
                            y1,
                            self.grid_size_x.round(),
                            self.grid_size_y.round(),
                            [self.cell_color[0], self.cell_color[1], self.cell_color[2], self.network_opacity],
                            &mut quads,
                        );
                    }
                }
            }
        }

        for i in 0..self.nodes.len() {
            if let Some((nx, ny, nw, nh)) = self.node_rect(i) {
                let scale_f = nw / 80.0;
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
                        let inset = 3.0 * scale_f;
                        push_clipped(tx + inset, ty + inset, tw - inset * 2.0, th - inset * 2.0, colors::TOGGLE_ON, &mut quads);
                    }
                }
            }
        }

        quads
    }



    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if self.hit_test(px, py, ctx) {
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

impl Drop for Graph {
    fn drop(&mut self) {
        focus::clear_if_matches(self);
    }
}

impl GraphController for Graph {
    fn set_nodes(&mut self, nodes: &[GraphNode]) {
        self.nodes = nodes.to_vec();
        
        // Sync selected_idx from selected_id
        if let Some(ref id) = self.selected_id {
            self.selected_idx = self.nodes.iter().position(|n| n.id == *id);
            if self.selected_idx.is_none() {
                self.selected_id = None;
            }
        } else {
            self.selected_idx = None;
        }

        // Sync dragging_idx from dragging_id
        if let Some(ref id) = self.dragging_id {
            self.dragging_idx = self.nodes.iter().position(|n| n.id == *id);
            if self.dragging_idx.is_none() {
                self.dragging_id = None;
                self.drag_node_pos = None;
            }
        } else {
            self.dragging_idx = None;
            self.drag_node_pos = None;
        }

        self.double_clicked_idx = None;
        self.double_click_timer = None;
        self.toggle_hovered_idx = None;
    }
    fn get_nodes(&self) -> Vec<GraphNode> { self.nodes.clone() }
    fn selected_node(&self) -> Option<usize> { self.selected_idx }
    fn set_selected_node(&mut self, idx: Option<usize>) {
        self.selected_idx = idx;
        self.selected_id = idx.and_then(|i| self.nodes.get(i).map(|n| n.id.clone()));
    }
    fn double_clicked_node(&self) -> Option<usize> { self.double_clicked_idx }
    fn clear_double_clicked_node(&mut self) { self.double_clicked_idx = None; }
    fn set_grid_snap_enabled(&mut self, enabled: bool) { self.grid_snap_enabled = enabled; }
    fn take_node_geom_toggle(&mut self) -> Option<(usize, bool)> { self.node_geom_toggled.take() }
    fn set_grid_snap(&mut self, gx: f32, gy: f32) { self.grid_size_x = gx; self.grid_size_y = gy; }
    fn set_grid_sizes(&mut self, gx: f32, gy: f32) { self.grid_size_x = gx; self.grid_size_y = gy; }
    fn set_skipped_sizes(&mut self, row_h: f32, col_w: f32) { self.skipped_row_h = row_h; self.skipped_col_w = col_w; }
    fn set_grid_origin(&mut self, ox: f32, oy: f32) { self.grid_origin_x = ox; self.grid_origin_y = oy; }
    fn grid_origin(&self) -> (f32, f32) { (self.grid_origin_x, self.grid_origin_y) }
    fn set_show_network_grid(&mut self, show: bool) { self.show_network_grid = show; }
}

