//! Narrow-trait `Graph` (Phase 5m) — the node-network editor: a pannable/zoomable grid of
//! draggable nodes with geometry toggles, input/output ports, wire routing, and interactive
//! connection dragging. [`GraphController`] rides the `Input` capability hooks.
//!
//! Rendering serves the legacy dual-geometry contract through the adapter's escape hatch:
//! [`Paint::paint`] emits the ROUNDED view (what `render_widget` hosts — cce-files — and the
//! scene walk — cce-graph — consume), while [`Paint::legacy_plain_quads`] serves the same
//! geometry as plain quads for raw `extra_quads` readers (the designer's render path), with
//! `all_quads` emptied by the adapter so no host draws it twice. Node-name text is clipped to
//! the widget rect via [`Paint::text_bounds`]. All grid geometry is in absolute screen space
//! (hosts pan by moving `grid_origin`); the widget rect only culls and clips.

use crate::colors;
use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::display::TextLabel;
use crate::widget::{
    Adapted, ElementState, Event, EventCtx, GraphController, Input, Key, Layout, MouseButton,
    MouseScrollDelta, Paint,
};

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

/// The widget's own corner style: the legacy `WidgetHost` defaults it inherited
/// (`corner_radius` 12.0, bottom corners rounded).
const WIDGET_RADIUS: f32 = 12.0;
const WIDGET_CORNERS: (bool, bool, bool, bool) = (false, false, true, true);

pub struct Graph {
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

    // Connection state
    connecting_from: Option<(usize, PortType, usize)>,
    current_mouse_pos: (f32, f32),
    pending_connection: Option<(String, String)>,
    hovered_port: Option<(usize, PortType, usize)>,
}

impl Graph {
    pub fn new() -> Adapted<Graph> {
        crate::layout::lazy_init_style_registry();

        let grid_size_x = crate::layout::graph_spacing_x();
        let grid_size_y = crate::layout::graph_spacing_y();
        let grid_snap_enabled = crate::layout::graph_grid_snap();

        let cell_col = crate::color::graph_cell_color();
        let gap_col = crate::color::graph_gap_color();

        Adapted::new(Graph {
            show_network_grid: false,
            grid_size_x,
            grid_size_y,
            grid_origin_x: 0.0,
            grid_origin_y: 0.0,
            skipped_row_h: grid_size_y / 2.0,
            skipped_col_w: grid_size_x / 2.0,
            nodes: Vec::new(),
            selected_idx: None,
            selected_id: None,
            double_clicked_idx: None,
            double_click_timer: None,
            grid_snap_enabled,
            node_geom_toggled: None,
            dragging_idx: None,
            dragging_id: None,
            drag_ox: 0.0,
            drag_oy: 0.0,
            drag_node_pos: None,
            toggle_hovered_idx: None,
            uniform_background: false,
            network_opacity: crate::color::graph_opacity(),
            cell_color: cell_col,
            gap_color: gap_col,
            connecting_from: None,
            current_mouse_pos: (0.0, 0.0),
            pending_connection: None,
            hovered_port: None,
        })
    }

    pub fn set_uniform_background(&mut self, uniform: bool) {
        self.uniform_background = uniform;
    }
    pub fn set_network_opacity(&mut self, opacity: f32) {
        self.network_opacity = opacity;
    }
    pub fn grid_sizes(&self) -> (f32, f32) {
        (self.grid_size_x, self.grid_size_y)
    }
    pub fn skipped_sizes(&self) -> (f32, f32) {
        (self.skipped_row_h, self.skipped_col_w)
    }
    pub fn grid_origin(&self) -> (f32, f32) {
        (self.grid_origin_x, self.grid_origin_y)
    }
    pub fn grid_snap_enabled(&self) -> bool {
        self.grid_snap_enabled
    }
    pub fn set_cell_color(&mut self, color: [f32; 3]) {
        self.cell_color = color;
    }
    pub fn set_gap_color(&mut self, color: [f32; 3]) {
        self.gap_color = color;
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

    pub fn is_node_rect(&self, qx: f32, qy: f32, qw: f32, qh: f32) -> bool {
        for i in 0..self.nodes.len() {
            if let Some((nx, ny, nw, nh)) = self.node_rect(i) {
                if (qx - nx).abs() < 0.1 && (qy - ny).abs() < 0.1 && (qw - nw).abs() < 0.1 && (qh - nh).abs() < 0.1 {
                    return true;
                }
            }
        }
        false
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

    pub fn zoom_by_factor(&mut self, factor: f32) {
        let new_grid_x = self.grid_size_x * factor;
        if new_grid_x >= 40.0 && new_grid_x <= 400.0 {
            self.grid_size_x = new_grid_x;
            self.grid_size_y *= factor;
            self.skipped_row_h *= factor;
            self.skipped_col_w *= factor;
        }
    }

    fn bg_color(&self) -> [f32; 4] {
        let mut c = if self.uniform_background {
            [self.cell_color[0], self.cell_color[1], self.cell_color[2], self.network_opacity]
        } else {
            [0.0, 0.0, 0.0, 0.01 * self.network_opacity]
        };
        let blur_val = crate::layout::graph_blur();
        if blur_val > 0.0 {
            c[3] = -blur_val.abs() * self.network_opacity;
        }
        c
    }

    /// The wires / connection preview / grid cells / axes / node bodies / toggles as plain
    /// quads — the legacy `extra_quads` body, against `rect` instead of a stored rect.
    fn geometry_quads(&self, rect: Rect) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        let min_x = rect.x;
        let min_y = rect.y;
        let max_x = rect.x + rect.width;
        let max_y = rect.y + rect.height;
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

        // A three-segment orthogonal wire from (start_x, start_y) down/up to (end_x, end_y).
        let scale_f = self.grid_size_x / 80.0;
        let wire_thickness = (3.0 * scale_f).clamp(1.0, 15.0);
        let push_wire = |start_x: f32, start_y: f32, end_x: f32, end_y: f32, color: [f32; 4], q: &mut Vec<(f32, f32, f32, f32, [f32; 4])>| {
            let mid_y = start_y + (end_y - start_y) / 2.0;

            let v1_min_y = start_y.min(mid_y);
            let v1_max_y = start_y.max(mid_y);
            push_clipped(start_x - wire_thickness / 2.0, v1_min_y, wire_thickness, v1_max_y - v1_min_y, color, q);

            let h_min_x = start_x.min(end_x);
            let h_max_x = start_x.max(end_x);
            push_clipped(h_min_x, mid_y - wire_thickness / 2.0, h_max_x - h_min_x, wire_thickness, color, q);

            let v2_min_y = mid_y.min(end_y);
            let v2_max_y = mid_y.max(end_y);
            push_clipped(end_x - wire_thickness / 2.0, v2_min_y, wire_thickness, v2_max_y - v2_min_y, color, q);
        };

        // Connection wires: each node with an "input" parameter draws a wire from that source
        // node's first output port to its own first input port.
        let wire_color = [0.0, 0.75, 1.0, 0.7 * self.network_opacity]; // Vibrant cyan glow
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

                        push_wire(start_x, start_y, end_x, end_y, wire_color, &mut quads);
                    }
                }
            }
        }

        // Connection preview while dragging one out
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
                let preview_color = [1.0, 0.6, 0.0, 0.8]; // Golden orange preview
                push_wire(start_x, start_y, self.current_mouse_pos.0, self.current_mouse_pos.1, preview_color, &mut quads);
            }
        }

        // Grid cells + gaps, on rounded pixel boundaries to prevent seams
        if self.show_network_grid && self.grid_size_x > 0.0 && self.grid_size_y > 0.0 && !self.uniform_background {
            let step_y = self.grid_size_y + self.skipped_row_h;
            let step_x = self.grid_size_x + self.skipped_col_w;

            if step_y >= 4.0 && step_x >= 4.0 {
                let ry_start = (((rect.y - self.grid_origin_y) / step_y).floor() as i32 - 1).max(-100_000);
                let ry_end = (((rect.y + rect.height - self.grid_origin_y) / step_y).ceil() as i32 + 1).min(100_000);

                let cx_start = (((rect.x - self.grid_origin_x) / step_x).floor() as i32 - 1).max(-100_000);
                let cx_end = (((rect.x + rect.width - self.grid_origin_x) / step_x).ceil() as i32 + 1).min(100_000);

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

                        // Right gap (shares cell height), bottom gap (full step width), then the
                        // cell itself, all on identical rounded boundaries.
                        push_clipped(
                            x_cell_end,
                            y_cell_start,
                            gap_w,
                            cell_h,
                            [self.gap_color[0], self.gap_color[1], self.gap_color[2], self.network_opacity],
                            &mut quads,
                        );
                        push_clipped(
                            x_cell_start,
                            y_cell_end,
                            x2 - x_cell_start,
                            gap_h,
                            [self.gap_color[0], self.gap_color[1], self.gap_color[2], self.network_opacity],
                            &mut quads,
                        );
                        push_clipped(
                            x_cell_start,
                            y_cell_start,
                            cell_w,
                            cell_h,
                            [self.cell_color[0], self.cell_color[1], self.cell_color[2], self.network_opacity],
                            &mut quads,
                        );
                    }
                }
            }
        }

        // Origin axes, drawn in the gaps beside row/column 0
        if self.grid_size_x > 0.0 && self.grid_size_y > 0.0 {
            let thickness = 2.0;
            let y_center = self.grid_origin_y + self.grid_size_y + self.skipped_row_h / 2.0;
            push_clipped(rect.x, y_center - thickness / 2.0, rect.width, thickness, [0.0, 0.0, 0.0, 1.0 * self.network_opacity], &mut quads);

            let x_center = self.grid_origin_x - self.skipped_col_w / 2.0;
            push_clipped(x_center - thickness / 2.0, rect.y, thickness, rect.height, [0.0, 0.0, 0.0, 1.0 * self.network_opacity], &mut quads);
        }

        // Node bodies (culled, not clipped — legacy) + geometry toggles (clipped)
        for i in 0..self.nodes.len() {
            if let Some((nx, ny, nw, nh)) = self.node_rect(i) {
                let scale_f = nw / 80.0;
                let mut bg_color = if self.dragging_idx == Some(i) {
                    colors::node_drag_color()
                } else if self.selected_idx == Some(i) {
                    colors::node_selected_color()
                } else {
                    colors::node_color()
                };
                bg_color[3] *= self.network_opacity;
                if nx + nw > min_x && nx < max_x && ny + nh > min_y && ny < max_y {
                    quads.push((nx, ny, nw, nh, bg_color));
                }

                if let Some((tx, ty, tw, th)) = self.toggle_rect(i) {
                    let mut btn_color = if self.toggle_hovered_idx == Some(i) {
                        colors::TOGGLE_HOVER
                    } else {
                        colors::TOGGLE_OFF
                    };
                    btn_color[3] *= self.network_opacity;
                    push_clipped(tx, ty, tw, th, btn_color, &mut quads);

                    if self.nodes[i].geom_visible {
                        let inset = 3.0 * scale_f;
                        let mut toggle_on_color = colors::TOGGLE_ON;
                        toggle_on_color[3] *= self.network_opacity;
                        push_clipped(tx + inset, ty + inset, tw - inset * 2.0, th - inset * 2.0, toggle_on_color, &mut quads);
                    }
                }
            }
        }

        quads
    }

    /// The rounded view of the same geometry — the legacy `all_rounded_quads` conversion: the
    /// widget background, then each plain quad either as a node body (node corner radius, all
    /// corners) or with the widget's edge-corner resolution.
    fn rounded_geometry(&self, rect: Rect) -> Vec<(f32, f32, f32, f32, f32, [f32; 4], (bool, bool, bool, bool))> {
        let mut rounded = Vec::new();

        let (w_tl, w_tr, w_br, w_bl) = WIDGET_CORNERS;
        rounded.push((rect.x, rect.y, rect.width, rect.height, WIDGET_RADIUS, self.bg_color(), WIDGET_CORNERS));

        let node_radius = crate::layout::graph_node_corner_radius();
        let (wx, wy, ww, wh) = (rect.x, rect.y, rect.width, rect.height);

        for (qx, qy, qw, qh, qc) in self.geometry_quads(rect) {
            if self.is_node_rect(qx, qy, qw, qh) {
                rounded.push((qx, qy, qw, qh, node_radius, qc, (true, true, true, true)));
            } else {
                let tl = w_tl && qx <= wx + 1.5 && qy <= wy + 1.5;
                let tr = w_tr && qx + qw >= wx + ww - 1.5 && qy <= wy + 1.5;
                let br = w_br && qx + qw >= wx + ww - 1.5 && qy + qh >= wy + wh - 1.5;
                let bl = w_bl && qx <= wx + 1.5 && qy + qh >= wy + wh - 1.5;

                let r = if tl || tr || br || bl { WIDGET_RADIUS } else { 0.0 };
                rounded.push((qx, qy, qw, qh, r, qc, (tl, tr, br, bl)));
            }
        }

        rounded
    }

    /// Input/output port circles, culled to the widget rect (legacy `extra_circles`).
    fn port_circles(&self, rect: Rect) -> Vec<(f32, f32, f32, [f32; 4])> {
        let mut circles = Vec::new();
        let min_x = rect.x;
        let min_y = rect.y;
        let max_x = rect.x + rect.width;
        let max_y = rect.y + rect.height;

        let mut push_circle_clipped = |cx: f32, cy: f32, r: f32, color: [f32; 4]| {
            if cx >= min_x && cx <= max_x && cy >= min_y && cy <= max_y {
                circles.push((cx, cy, r, color));
            }
        };

        let conn_size = crate::layout::graph_connector_size();
        let mut conn_color = colors::graph_connector_color();
        let mut conn_hl_color = colors::graph_connector_highlight_color();
        conn_color[3] *= self.network_opacity;
        conn_hl_color[3] *= self.network_opacity;

        for i in 0..self.nodes.len() {
            if let Some((nx, ny, nw, nh)) = self.node_rect(i) {
                let scale_f = nw / 80.0;
                let port_size = (conn_size * scale_f).max(2.0);
                let base_r = port_size / 2.0;

                let node = &self.nodes[i];

                for k in 0..node.inputs {
                    let cx = nx + nw * (k + 1) as f32 / (node.inputs + 1) as f32;
                    let cy = ny;

                    let is_hovered = self.hovered_port == Some((i, PortType::Input, k));
                    let is_connecting = self.connecting_from == Some((i, PortType::Input, k));

                    let (r, color) = if is_hovered || is_connecting {
                        (base_r * 1.4, conn_hl_color)
                    } else {
                        (base_r, conn_color)
                    };
                    push_circle_clipped(cx, cy, r, color);
                }

                for k in 0..node.outputs {
                    let cx = nx + nw * (k + 1) as f32 / (node.outputs + 1) as f32;
                    let cy = ny + nh;

                    let is_hovered = self.hovered_port == Some((i, PortType::Output, k));
                    let is_connecting = self.connecting_from == Some((i, PortType::Output, k));

                    let (r, color) = if is_hovered || is_connecting {
                        (base_r * 1.4, conn_hl_color)
                    } else {
                        (base_r, conn_color)
                    };
                    push_circle_clipped(cx, cy, r, color);
                }
            }
        }
        circles
    }

    /// Node-name labels beside each node, scaled with the grid, included only when they
    /// intersect the widget rect (legacy `text_labels`).
    fn node_labels(&self, rect: Rect) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        for (i, node) in self.nodes.iter().enumerate() {
            if let Some((nx, ny, nw, nh)) = self.node_rect(i) {
                let scale_f = nw / 80.0;
                let font_size = (14.0 * scale_f).clamp(6.0, 48.0);
                let lx = nx + nw + 8.0 * scale_f;
                let ly = crate::layout::align_text_y(ny, nh, font_size, 0.0);
                let text_w = TextLabel::estimate_width(&node.name, font_size);
                if lx + text_w >= rect.x && lx < rect.x + rect.width && ly + font_size >= rect.y && ly < rect.y + rect.height {
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
}

fn read_zoom_bindings() -> (String, String) {
    let mut zoom_in_val = "=".to_string();
    let mut zoom_out_val = "-".to_string();
    let path = crate::config::get_config_path();
    if let Ok(content) = std::fs::read_to_string(&path) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(zoom_in) = val.pointer("/layout/zoom_in").and_then(|v| v.as_str()) {
                zoom_in_val = zoom_in.to_string();
            }
            if let Some(zoom_out) = val.pointer("/layout/zoom_out").and_then(|v| v.as_str()) {
                zoom_out_val = zoom_out.to_string();
            }
        }
    }
    (zoom_in_val, zoom_out_val)
}

impl Layout for Graph {}

impl Paint for Graph {
    fn color(&self) -> [f32; 4] {
        self.bg_color()
    }

    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        Some((WIDGET_RADIUS, WIDGET_CORNERS))
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::graph_node_font())
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        for (qx, qy, qw, qh, r, c, corners) in self.rounded_geometry(rect) {
            ctx.rounded_rect(Rect { x: qx, y: qy, width: qw, height: qh }, r, corners, c);
        }
        for (cx, cy, r, c) in self.port_circles(rect) {
            ctx.circle(cx, cy, r, c);
        }
        for l in self.node_labels(rect) {
            ctx.text(l.text, l.x, l.y, l.font_size, l.color);
        }
    }

    fn serves_legacy_plain_quads(&self) -> bool {
        true
    }

    fn legacy_plain_quads(&self, rect: Rect) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        self.geometry_quads(rect)
    }

    fn text_bounds(&self, rect: Rect) -> Option<[f32; 4]> {
        Some([rect.x, rect.y, rect.x + rect.width, rect.y + rect.height])
    }
}

impl Input for Graph {
    /// Legacy hit test excluded the right/bottom edges.
    fn hit(&self, rect: Rect, x: f32, y: f32) -> bool {
        x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::PointerMove { x: px, y: py, .. } => {
                let (px, py) = (*px, *py);
                let mut changed = false;
                if self.connecting_from.is_some() {
                    self.current_mouse_pos = (px, py);
                    changed = true;
                }
                let was_toggle_hovered = self.toggle_hovered_idx;
                self.toggle_hovered_idx = None;

                let was_hovered_port = self.hovered_port;
                self.hovered_port = None;

                let conn_act_r = crate::layout::graph_connector_activation_radius();

                for i in 0..self.nodes.len() {
                    if let Some((nx, ny, nw, nh)) = self.node_rect(i) {
                        let scale_f = nw / 80.0;
                        let hit_radius = (conn_act_r * scale_f).max(2.0);
                        let node = &self.nodes[i];

                        for k in 0..node.inputs {
                            let cx = nx + nw * (k + 1) as f32 / (node.inputs + 1) as f32;
                            let cy = ny;
                            if (px - cx).powi(2) + (py - cy).powi(2) <= hit_radius.powi(2) {
                                self.hovered_port = Some((i, PortType::Input, k));
                            }
                        }

                        for k in 0..node.outputs {
                            let cx = nx + nw * (k + 1) as f32 / (node.outputs + 1) as f32;
                            let cy = ny + nh;
                            if (px - cx).powi(2) + (py - cy).powi(2) <= hit_radius.powi(2) {
                                self.hovered_port = Some((i, PortType::Output, k));
                            }
                        }
                    }

                    if let Some((tx, ty, tw, th)) = self.toggle_rect(i) {
                        if px >= tx && px < tx + tw && py >= ty && py < ty + th {
                            self.toggle_hovered_idx = Some(i);
                        }
                    }
                }

                if was_toggle_hovered != self.toggle_hovered_idx || was_hovered_port != self.hovered_port {
                    changed = true;
                }
                changed
            }
            Event::MouseLeave => {
                let changed = self.hovered_port.is_some() || self.toggle_hovered_idx.is_some();
                self.hovered_port = None;
                self.toggle_hovered_idx = None;
                changed
            }
            Event::MouseButton { button: MouseButton::Right, state: ElementState::Pressed, .. } => {
                if self.connecting_from.is_some() {
                    self.connecting_from = None;
                    true
                } else {
                    false
                }
            }
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, x, y, .. } => {
                self.on_left_press(*x, *y, ectx)
            }
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Released, .. } => {
                if self.dragging_idx.is_some() {
                    self.commit_drag();
                    true
                } else {
                    false
                }
            }
            Event::MouseWheel { delta, x: px, y: py, .. } => {
                let _ = (px, py); // hit-gated by the adapter
                let ctrl = ectx.ui.as_deref().map_or(false, |ui| ui.ctrl_pressed);
                if ctrl {
                    match delta {
                        MouseScrollDelta::LineDelta(_x, y) => {
                            if *y > 0.0 {
                                self.zoom_by_factor(1.1);
                            } else if *y < 0.0 {
                                self.zoom_by_factor(1.0 / 1.1);
                            }
                            true
                        }
                        MouseScrollDelta::PixelDelta(pos) => {
                            let factor = 1.0 + (pos.y as f32 * 0.015);
                            self.zoom_by_factor(factor);
                            true
                        }
                    }
                } else {
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
                }
            }
            Event::KeyInput(key_event) => {
                if key_event.state == ElementState::Pressed {
                    if let Key::Character(ref ch) = key_event.logical_key {
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
            _ => false,
        }
    }

    fn scrollable(&self) -> bool {
        false
    }

    // The graph is "draggable" only once a left press landed on a node body (`on_left_press`
    // sets `dragging_idx`); hosts then re-init via drag_begin and stream drag_update.
    fn draggable(&self, _rect: Rect) -> bool {
        self.dragging_idx.is_some()
    }
    fn is_dragging(&self) -> bool {
        self.dragging_idx.is_some()
    }

    fn drag_begin(&mut self, px: f32, py: f32, _rect: Rect) {
        if let Some(idx) = self.dragging_idx {
            if let Some((nx, ny, _, _)) = self.node_rect(idx) {
                self.drag_ox = px - nx;
                self.drag_oy = py - ny;
                self.drag_node_pos = Some((nx, ny));
            }
        }
    }

    fn drag_update(&mut self, px: f32, py: f32, _rect: Rect) -> bool {
        if self.dragging_idx.is_some() {
            let nx = px - self.drag_ox;
            let ny = py - self.drag_oy;

            let snap_x = if self.grid_snap_enabled { self.grid_size_x + self.skipped_col_w } else { 0.0 };
            let snap_y = if self.grid_snap_enabled { self.grid_size_y + self.skipped_row_h } else { 0.0 };

            let nx = if snap_x > 0.0 {
                let relative = nx - self.grid_origin_x;
                (relative / snap_x).round() * snap_x + self.grid_origin_x
            } else {
                nx
            };

            let ny = if snap_y > 0.0 {
                let relative = ny - self.grid_origin_y;
                (relative / snap_y).round() * snap_y + self.grid_origin_y
            } else {
                ny
            };

            self.drag_node_pos = Some((nx, ny));
            return true;
        }
        false
    }

    fn drag_end(&mut self) {
        self.commit_drag();
    }

}

impl Graph {
    /// The legacy left-press cascade: ports (start/complete a connection), a node-body
    /// fallback for an in-flight connection, geometry toggles, then node selection + drag
    /// arming; an empty-space press clears the selection and stays unconsumed.
    fn on_left_press(&mut self, px: f32, py: f32, ectx: &mut EventCtx) -> bool {
        for i in (0..self.nodes.len()).rev() {
            if let Some((nx, ny, nw, nh)) = self.node_rect(i) {
                let scale_f = nw / 80.0;
                let conn_act_r = crate::layout::graph_connector_activation_radius();
                let port_click_radius = (conn_act_r * scale_f).max(2.0);
                let port_click_radius_sq = port_click_radius * port_click_radius;

                let node = &self.nodes[i];
                // Inputs (top edge)
                for k in 0..node.inputs {
                    let port_x = nx + nw * (k + 1) as f32 / (node.inputs + 1) as f32;
                    let port_y = ny;
                    let dx = px - port_x;
                    let dy = py - port_y;
                    if dx * dx + dy * dy <= port_click_radius_sq {
                        if let Some((src_idx, src_port_type, _src_port_idx)) = self.connecting_from {
                            if src_idx != i && src_port_type == PortType::Output {
                                let output_node = &self.nodes[src_idx];
                                let input_node = &self.nodes[i];
                                self.pending_connection = Some((input_node.id.clone(), output_node.name.clone()));
                                self.connecting_from = None;
                            } else {
                                self.connecting_from = None;
                            }
                        } else {
                            self.connecting_from = Some((i, PortType::Input, k));
                            self.current_mouse_pos = (px, py);
                        }
                        return true;
                    }
                }

                // Outputs (bottom edge)
                for k in 0..node.outputs {
                    let port_x = nx + nw * (k + 1) as f32 / (node.outputs + 1) as f32;
                    let port_y = ny + nh;
                    let dx = px - port_x;
                    let dy = py - port_y;
                    if dx * dx + dy * dy <= port_click_radius_sq {
                        if let Some((src_idx, src_port_type, _src_port_idx)) = self.connecting_from {
                            if src_idx != i && src_port_type == PortType::Input {
                                let output_node = &self.nodes[i];
                                let input_node = &self.nodes[src_idx];
                                self.pending_connection = Some((input_node.id.clone(), output_node.name.clone()));
                                self.connecting_from = None;
                            } else {
                                self.connecting_from = None;
                            }
                        } else {
                            self.connecting_from = Some((i, PortType::Output, k));
                            self.current_mouse_pos = (px, py);
                        }
                        return true;
                    }
                }
            }
        }

        // Actively connecting + clicked a target node body: connect to its closest compatible port
        if let Some((src_idx, src_port_type, _src_port_idx)) = self.connecting_from {
            for i in (0..self.nodes.len()).rev() {
                if src_idx != i {
                    if let Some((nx, ny, nw, nh)) = self.node_rect(i) {
                        if px >= nx && px < nx + nw && py >= ny && py < ny + nh {
                            let node = &self.nodes[i];
                            if src_port_type == PortType::Output && node.inputs > 0 {
                                let output_node = &self.nodes[src_idx];
                                let input_node = &self.nodes[i];
                                self.pending_connection = Some((input_node.id.clone(), output_node.name.clone()));
                                self.connecting_from = None;
                                return true;
                            } else if src_port_type == PortType::Input && node.outputs > 0 {
                                let output_node = &self.nodes[i];
                                let input_node = &self.nodes[src_idx];
                                self.pending_connection = Some((input_node.id.clone(), output_node.name.clone()));
                                self.connecting_from = None;
                                return true;
                            }
                        }
                    }
                }
            }
        }

        if self.connecting_from.is_some() {
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
                    ectx.request_focus();
                    return true;
                }
            }
        }
        self.selected_idx = None;
        self.selected_id = None;
        false
    }

    /// Drop the in-flight node drag onto the nearest free grid cell (legacy `drag_end`).
    fn commit_drag(&mut self) {
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
}

impl GraphController for Graph {
    fn set_nodes(&mut self, nodes: &[GraphNode]) {
        self.nodes = nodes.to_vec();
        self.hovered_port = None;

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
    fn is_node_rect(&self, qx: f32, qy: f32, qw: f32, qh: f32) -> bool {
        self.is_node_rect(qx, qy, qw, qh)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::UiContext;
    use crate::widget::WidgetHost;

    fn two_nodes() -> Adapted<Graph> {
        let mut g = Graph::new();
        WidgetHost::set_rect(&mut g, 0.0, 0.0, 800.0, 600.0);
        g.set_grid_sizes(80.0, 40.0);
        g.set_skipped_sizes(20.0, 20.0);
        g.set_grid_origin(100.0, 100.0);
        g.set_grid_snap_enabled(true);
        let node = |id: &str, name: &str, col: f32, row: f32| GraphNode {
            id: id.into(),
            name: name.into(),
            position: (col, row),
            parameters: Vec::new(),
            geom_visible: true,
            node_type: String::new(),
            inputs: 1,
            outputs: 1,
        };
        g.set_nodes(&[node("a", "alpha", 0.0, 0.0), node("b", "beta", 1.0, 1.0)]);
        g
    }

    #[test]
    fn node_press_selects_arms_drag_and_commit_snaps_to_grid() {
        let mut ctx = UiContext::new();
        let mut g = two_nodes();
        let (id, ptr) = (g.id(), g.as_ptr_mut());
        ctx.register_widget(id, ptr);

        // Node a occupies (100, 100, 80, 40). Press its body (away from ports/toggle).
        assert!(g.mouse_input(MouseButton::Left, ElementState::Pressed, 110.0, 120.0, &mut ctx));
        assert_eq!(g.selected_node(), Some(0));
        assert!(WidgetHost::is_dragging(&g) && WidgetHost::draggable(&g));

        // Drag one grid step right (step_x = 100): snap puts the node at column 1, but cell
        // (1, 0) is free so it lands there.
        g.drag_begin(110.0, 120.0);
        assert!(g.drag_update(210.0, 120.0));
        assert!(g.mouse_input(MouseButton::Left, ElementState::Released, 210.0, 120.0, &mut ctx));
        assert!(!WidgetHost::is_dragging(&g));
        assert_eq!(g.get_nodes()[0].position, (1.0, 0.0));

        // An empty-space press clears the selection and is NOT consumed (legacy contract).
        assert!(!g.mouse_input(MouseButton::Left, ElementState::Pressed, 700.0, 550.0, &mut ctx));
        assert_eq!(g.selected_node(), None);
    }

    #[test]
    fn port_click_starts_and_completes_a_connection() {
        let mut ctx = UiContext::new();
        let mut g = two_nodes();
        let (id, ptr) = (g.id(), g.as_ptr_mut());
        ctx.register_widget(id, ptr);

        // Node a's output port sits at the bottom-center of (100,100,80,40) => (140, 140).
        assert!(g.mouse_input(MouseButton::Left, ElementState::Pressed, 140.0, 140.0, &mut ctx));
        // Node b's input port: node b at (200, 160, 80, 40) => top-center (240, 160).
        assert!(g.mouse_input(MouseButton::Left, ElementState::Pressed, 240.0, 160.0, &mut ctx));

        let pending = GraphController::take_pending_connection(&mut *g);
        assert_eq!(pending, Some(("b".to_string(), "alpha".to_string())));
    }

    #[test]
    fn dual_geometry_views_stay_consistent() {
        let mut g = two_nodes();
        let ctx = UiContext::new();

        // The plain view (designer path) and the rounded view (render_widget path) describe
        // the same quads: the rounded view adds only the widget background entry up front.
        let plain = WidgetHost::extra_quads(&g);
        let rounded = WidgetHost::all_rounded_quads(&g, &ctx);
        assert!(!plain.is_empty());
        assert_eq!(rounded.len(), plain.len() + 1);
        for ((px, py, pw, ph, pc), (rx, ry, rw, rh, _, rc, _)) in plain.iter().zip(rounded.iter().skip(1)) {
            assert_eq!((px, py, pw, ph, pc), (rx, ry, rw, rh, rc));
        }

        // Node bodies carry the node corner radius in the rounded view.
        let node_radius = crate::layout::graph_node_corner_radius();
        let node_entries: Vec<_> = rounded
            .iter()
            .filter(|(qx, qy, qw, qh, ..)| g.is_node_rect(*qx, *qy, *qw, *qh))
            .collect();
        assert_eq!(node_entries.len(), 2, "both node bodies present");
        for entry in node_entries {
            assert_eq!(entry.4, node_radius);
            assert_eq!(entry.6, (true, true, true, true));
        }

        // And `all_quads` stays empty so render_widget hosts (reading BOTH getters) never
        // draw the geometry twice — the legacy Graph override's contract.
        assert!(WidgetHost::all_quads(&g, &ctx).is_empty());
    }
}
