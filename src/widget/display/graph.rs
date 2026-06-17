use crate::colors;
use crate::widget::*;
use crate::widget::display::TextLabel;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PortType {
    Input,
    Output,
}

fn default_outputs() -> usize { 1 }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct GraphNode {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub position: (f32, f32), // (column, row)
    pub parameters: Vec<(String, String, String)>, // (name, value, type)
    pub geom_visible: bool,
    #[serde(default)]
    pub node_type: String,
    #[serde(default)]
    pub inputs: usize,
    #[serde(default = "default_outputs")]
    pub outputs: usize,
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
    cell_opacity: f32,
    gap_opacity: f32,
    cell_color: [f32; 3],
    gap_color: [f32; 3],

    // Connection state
    connecting_from: Option<(usize, PortType, usize)>,
    current_mouse_pos: (f32, f32),
    pending_connection: Option<(String, String)>,
}

impl Graph {
    pub fn set_uniform_background(&mut self, uniform: bool) {
        self.uniform_background = uniform;
    }
    pub fn set_network_opacity(&mut self, opacity: f32) {
        self.network_opacity = opacity;
        self.cell_opacity = opacity;
        self.gap_opacity = opacity;
    }
    pub fn set_cell_opacity(&mut self, opacity: f32) {
        self.cell_opacity = opacity;
    }
    pub fn set_gap_opacity(&mut self, opacity: f32) {
        self.gap_opacity = opacity;
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
            cell_opacity: 0.95,
            gap_opacity: 0.95,
            cell_color: [0.13, 0.13, 0.16],
            gap_color: [0.07, 0.07, 0.09],
            connecting_from: None,
            current_mouse_pos: (0.0, 0.0),
            pending_connection: None,
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
        if let Some(node) = self.nodes.get(idx) {
            if node.node_type == "utility" {
                return None;
            }
        }
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

    pub fn zoom_in(&mut self) {
        if self.grid_size_x < 400.0 {
            self.grid_size_x *= 1.1;
            self.grid_size_y *= 1.1;
            self.skipped_row_h *= 1.1;
            self.skipped_col_w *= 1.1;
        }
    }

    pub fn zoom_out(&mut self) {
        if self.grid_size_x > 40.0 {
            self.grid_size_x /= 1.1;
            self.grid_size_y /= 1.1;
            self.skipped_row_h /= 1.1;
            self.skipped_col_w /= 1.1;
        }
    }
}

fn read_zoom_bindings() -> (String, String) {
    let mut zoom_in_val = "=".to_string();
    let mut zoom_out_val = "-".to_string();
    let paths = [
        "/home/lsgalante/.config/cce/config.toml",
        "/home/lsgalante/.config/ccec/config.toml",
    ];
    for path in &paths {
        if let Ok(content) = std::fs::read_to_string(path) {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("zoom_in") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val = rest.trim_end_matches('"').to_string();
                    if !val.is_empty() {
                        zoom_in_val = val;
                    }
                } else if let Some(rest) = trimmed.strip_prefix("zoom_out") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val = rest.trim_end_matches('"').to_string();
                    if !val.is_empty() {
                        zoom_out_val = val;
                    }
                }
            }
            break;
        }
    }
    (zoom_in_val, zoom_out_val)
}

impl Element for Graph {
    fn keyboard_input(&mut self, event: &KeyEvent, _ctx: &mut UiContext) -> bool {
        if event.state == ElementState::Pressed {
            if let Key::Character(ref ch) = event.logical_key {
                let (zoom_in_binding, zoom_out_binding) = read_zoom_bindings();
                if ch == &zoom_in_binding {
                    self.zoom_in();
                    return true;
                } else if ch == &zoom_out_binding {
                    self.zoom_out();
                    return true;
                }
            }
        }
        false
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn as_ptr(&self) -> *mut (dyn Element + 'static) {
        self as *const Self as *mut Self as *mut (dyn Element + 'static)
    }

    fn rounded_corners(&self) -> (bool, bool, bool, bool) { (false, false, true, true) }

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
                let ly = crate::layout::align_text_y(ny, nh, font_size, 0.0);
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

    fn text_labels_with_bounds(&self, _ctx: &UiContext) -> Vec<(TextLabel, Option<[f32; 4]>)> {
        let bounds = Some([self.x, self.y, self.x + self.w, self.y + self.h]);
        self.text_labels().into_iter().map(|l| (l, bounds)).collect()
    }

    fn text_labels_with_font_and_bounds(&self, _ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        let font = self.widget_font();
        let bounds = Some([self.x, self.y, self.x + self.w, self.y + self.h]);
        self.text_labels().into_iter().map(|l| (l, font.clone(), bounds)).collect()
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
        let mut changed = false;
        if self.connecting_from.is_some() {
            self.current_mouse_pos = (px, py);
            changed = true;
        }
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
        if was_toggle_hovered != self.toggle_hovered_idx {
            changed = true;
        }
        changed
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if button == MouseButton::Right && state == ElementState::Pressed {
            if self.connecting_from.is_some() {
                self.connecting_from = None;
                return true;
            }
        }
        if button != MouseButton::Left { return false; }
        match state {
            ElementState::Pressed => {
                println!("DEBUG Graph::mouse_input: Pressed px={}, py={}, connecting_from={:?}", px, py, self.connecting_from);
                // First, check direct port clicks
                for i in (0..self.nodes.len()).rev() {
                    if let Some((nx, ny, nw, nh)) = self.node_rect(i) {
                        let scale_f = nw / 80.0;
                        let port_click_radius = (20.0 * scale_f).max(12.0);
                        let port_click_radius_sq = port_click_radius * port_click_radius;

                        let node = &self.nodes[i];
                        // Check inputs (top edge)
                        for k in 0..node.inputs {
                            let port_x = nx + nw * (k + 1) as f32 / (node.inputs + 1) as f32;
                            let port_y = ny;
                            let dx = px - port_x;
                            let dy = py - port_y;
                            println!("  Checking input node={} port={} port_x={} port_y={} dist_sq={}", node.name, k, port_x, port_y, dx*dx + dy*dy);
                            if dx * dx + dy * dy <= port_click_radius_sq {
                                if let Some((src_idx, src_port_type, _src_port_idx)) = self.connecting_from {
                                    if src_idx != i && src_port_type == PortType::Output {
                                        let output_node = &self.nodes[src_idx];
                                        let input_node = &self.nodes[i];
                                        self.pending_connection = Some((input_node.id.clone(), output_node.name.clone()));
                                        self.connecting_from = None;
                                        println!("  Port connection created: Input={} from Output={}", input_node.name, output_node.name);
                                        return true;
                                    } else {
                                        self.connecting_from = None;
                                        println!("  Port connection aborted (same node or incompatible ports)");
                                        return true;
                                    }
                                } else {
                                    self.connecting_from = Some((i, PortType::Input, k));
                                    self.current_mouse_pos = (px, py);
                                    println!("  Start connecting from Input port of node={}", node.name);
                                    return true;
                                }
                            }
                        }

                        // Check outputs (bottom edge)
                        for k in 0..node.outputs {
                            let port_x = nx + nw * (k + 1) as f32 / (node.outputs + 1) as f32;
                            let port_y = ny + nh;
                            let dx = px - port_x;
                            let dy = py - port_y;
                            println!("  Checking output node={} port={} port_x={} port_y={} dist_sq={}", node.name, k, port_x, port_y, dx*dx + dy*dy);
                            if dx * dx + dy * dy <= port_click_radius_sq {
                                if let Some((src_idx, src_port_type, _src_port_idx)) = self.connecting_from {
                                    if src_idx != i && src_port_type == PortType::Input {
                                        let output_node = &self.nodes[i];
                                        let input_node = &self.nodes[src_idx];
                                        self.pending_connection = Some((input_node.id.clone(), output_node.name.clone()));
                                        self.connecting_from = None;
                                        println!("  Port connection created: Input={} from Output={}", input_node.name, output_node.name);
                                        return true;
                                    } else {
                                        self.connecting_from = None;
                                        println!("  Port connection aborted (same node or incompatible ports)");
                                        return true;
                                    }
                                } else {
                                    self.connecting_from = Some((i, PortType::Output, k));
                                    self.current_mouse_pos = (px, py);
                                    println!("  Start connecting from Output port of node={}", node.name);
                                    return true;
                                }
                            }
                        }
                    }
                }

                // Fallback: If we are actively connecting and clicked on a target node body, connect to its closest compatible port
                if let Some((src_idx, src_port_type, _src_port_idx)) = self.connecting_from {
                    for i in (0..self.nodes.len()).rev() {
                        if src_idx != i {
                            if let Some((nx, ny, nw, nh)) = self.node_rect(i) {
                                println!("  Checking fallback body node={} nx={} ny={} nw={} nh={}", self.nodes[i].name, nx, ny, nw, nh);
                                if px >= nx && px < nx + nw && py >= ny && py < ny + nh {
                                    let node = &self.nodes[i];
                                    if src_port_type == PortType::Output && node.inputs > 0 {
                                        let output_node = &self.nodes[src_idx];
                                        let input_node = &self.nodes[i];
                                        self.pending_connection = Some((input_node.id.clone(), output_node.name.clone()));
                                        self.connecting_from = None;
                                        println!("  Fallback connection created: Input={} from Output={}", input_node.name, output_node.name);
                                        return true;
                                    } else if src_port_type == PortType::Input && node.outputs > 0 {
                                        let output_node = &self.nodes[i];
                                        let input_node = &self.nodes[src_idx];
                                        self.pending_connection = Some((input_node.id.clone(), output_node.name.clone()));
                                        self.connecting_from = None;
                                        println!("  Fallback connection created: Input={} from Output={}", input_node.name, output_node.name);
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }

                if self.connecting_from.is_some() {
                    println!("  Clearing connecting_from because it didn't hit any ports or node bodies");
                    self.connecting_from = None;
                }

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
                        let src_outputs = self.nodes[src_idx].outputs;
                        let target_inputs = node.inputs;

                        let start_x = if src_outputs > 0 {
                            sx + sw * 1.0 / (src_outputs + 1) as f32
                        } else {
                            sx + sw / 2.0
                        };
                        let start_y = sy + sh;

                        let end_x = if target_inputs > 0 {
                            ex + ew * 1.0 / (target_inputs + 1) as f32
                        } else {
                            ex + ew / 2.0
                        };
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

        // Draw connection wire preview if in progress
        if let Some((node_idx, port_type, port_idx)) = self.connecting_from {
            if let Some((nx, ny, nw, nh)) = self.node_rect(node_idx) {
                let start_x = match port_type {
                    PortType::Input => nx + nw * (port_idx + 1) as f32 / (self.nodes[node_idx].inputs + 1) as f32,
                    PortType::Output => nx + nw * (port_idx + 1) as f32 / (self.nodes[node_idx].outputs + 1) as f32,
                };
                let start_y = match port_type {
                    PortType::Input => ny,
                    PortType::Output => ny + nh,
                };

                let end_x = self.current_mouse_pos.0;
                let end_y = self.current_mouse_pos.1;

                let preview_color = [1.0, 0.6, 0.0, 0.8]; // Golden orange preview
                let mid_y = start_y + (end_y - start_y) / 2.0;

                // Vertical segment 1
                let v1_min_y = start_y.min(mid_y);
                let v1_max_y = start_y.max(mid_y);
                push_clipped(
                    start_x - wire_thickness / 2.0,
                    v1_min_y,
                    wire_thickness,
                    v1_max_y - v1_min_y,
                    preview_color,
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
                    preview_color,
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
                    preview_color,
                    &mut quads,
                );
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

                // Draw gap rectangles and cells in a single loop with matching rounded coordinate boundaries to prevent seams
                for r in ry_start..=ry_end {
                    let y_cell_start = (self.grid_origin_y + (r as f32) * step_y).round();
                    let y_cell_end = (self.grid_origin_y + (r as f32) * step_y + self.grid_size_y).round();
                    let y2 = (self.grid_origin_y + ((r + 1) as f32) * step_y).round();

                    let cell_h = y_cell_end - y_cell_start;
                    let gap_h = y2 - y_cell_end;

                    for c in cx_start..=cx_end {
                        let x_cell_start = (self.grid_origin_x + (c as f32) * step_x).round();
                        let x_cell_end = (self.grid_origin_x + (c as f32) * step_x + self.grid_size_x).round();
                        let x2 = (self.grid_origin_x + ((c + 1) as f32) * step_x).round();

                        let cell_w = x_cell_end - x_cell_start;
                        let gap_w = x2 - x_cell_end;

                        // Draw right gap rectangle (shares cell height, sits to the right of the cell)
                        push_clipped(
                            x_cell_end,
                            y_cell_start,
                            gap_w,
                            cell_h,
                            [self.gap_color[0], self.gap_color[1], self.gap_color[2], self.gap_opacity],
                            &mut quads,
                        );

                        // Draw bottom gap rectangle (shares the full step width, sits below the cell/right gap)
                        push_clipped(
                            x_cell_start,
                            y_cell_end,
                            x2 - x_cell_start,
                            gap_h,
                            [self.gap_color[0], self.gap_color[1], self.gap_color[2], self.gap_opacity],
                            &mut quads,
                        );

                        // Draw cell rectangle with the exact same bounding boundaries
                        push_clipped(
                            x_cell_start,
                            y_cell_start,
                            cell_w,
                            cell_h,
                            [self.cell_color[0], self.cell_color[1], self.cell_color[2], self.cell_opacity],
                            &mut quads,
                        );
                    }
                }
            }
        }

        if self.grid_size_x > 0.0 && self.grid_size_y > 0.0 {
            let thickness = 2.0;
            // X axis (horizontal) in the gap below row 0
            let y_center = self.grid_origin_y + self.grid_size_y + self.skipped_row_h / 2.0;
            push_clipped(self.x, y_center - thickness / 2.0, self.w, thickness, [0.0, 0.0, 0.0, 1.0], &mut quads);

            // Y axis (vertical) in the gap to the left of col 0
            let x_center = self.grid_origin_x - self.skipped_col_w / 2.0;
            push_clipped(x_center - thickness / 2.0, self.y, thickness, self.h, [0.0, 0.0, 0.0, 1.0], &mut quads);
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

                // Draw input ports on top edge
                let port_size = (6.0 * scale_f).max(2.0);
                let port_color = [0.1, 0.8, 0.4, 1.0]; // Bright green/emerald
                let node = &self.nodes[i];
                for k in 0..node.inputs {
                    let px = nx + nw * (k + 1) as f32 / (node.inputs + 1) as f32 - port_size / 2.0;
                    let py = ny - port_size / 2.0;
                    push_clipped(px, py, port_size, port_size, port_color, &mut quads);
                }

                // Draw output ports on bottom edge
                for k in 0..node.outputs {
                    let px = nx + nw * (k + 1) as f32 / (node.outputs + 1) as f32 - port_size / 2.0;
                    let py = ny + nh - port_size / 2.0;
                    push_clipped(px, py, port_size, port_size, port_color, &mut quads);
                }

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
    fn take_pending_connection(&mut self) -> Option<(String, String)> {
        self.pending_connection.take()
    }
    fn cancel_connecting(&mut self) {
        self.connecting_from = None;
    }
}

