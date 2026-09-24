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
//!
//! The grid is a LATTICE OF LINES with one size per axis — the pitch, from the centre of
//! one line to the centre of the next — and a node is centred on the intersection its
//! `position` names: node (c, r) sits on `grid_origin + (c * pitch_x, r * pitch_y)`. The
//! node body has a size of its own (`graph_node_width` / `graph_node_height`, scaled with
//! the zoom), independent of the pitch. It used to be a grid of CELLS — a cell size that was also the node size, plus a gap between cells,
//! with a node filling its cell — and the cell-and-gap setters survive as a description of
//! the same lattice for hosts that still speak it (a cell plus its gap is a pitch).

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

/// One flat-geometry quad `(x, y, w, h, color, cell)` from
/// [`Graph::geometry_quads_tagged`]: `cell` is `Some(corner flags)` for a grid
/// cell — per-corner `(tl, tr, br, bl)` rounding that survived the pane clip —
/// and `None` for everything else (wires, gaps, axes, nodes, toggles).
pub type TaggedQuad = (f32, f32, f32, f32, [f32; 4], Option<(bool, bool, bool, bool)>);

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
    /// The pitch: centre of one grid line to the centre of the next, per
    /// axis. The grid's one size.
    pitch_x: f32,
    pitch_y: f32,
    /// The node body's size — its own (`graph_node_width` / `_height` at
    /// 100%), independent of the pitch; the cell-model setters set it too.
    node_w: f32,
    node_h: f32,
    /// The lattice intersection node (0, 0) is centred on, window-absolute.
    grid_origin_x: f32,
    grid_origin_y: f32,
    /// Smooth-scroll driver behind the pan origin: notches glide, a trackpad
    /// flick coasts across the unbounded canvas.
    pan_motion: crate::widget::ScrollMotion,
    nodes: Vec<GraphNode>,
    selected_idx: Option<usize>,
    selected_id: Option<String>,
    double_clicked_id: Option<String>,
    /// Keyed by node ID, not index: hosts (the designer) re-sync nodes on
    /// EVERY window event, and set_nodes used to wipe this state wholesale —
    /// the first press's timer never survived to the second press, so
    /// double-click detection could not fire at all. Same id-keyed survival
    /// as `selected_id` and the hovered-port remap.
    double_click_timer: Option<(std::time::Instant, String)>,
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
    /// Node-domain opacity (bodies, wires, connectors) — independent of
    /// `network_opacity`, which fades the pane surface (grid cells/gaps).
    node_opacity: f32,
    cell_color: [f32; 3],
    gap_color: [f32; 3],

    // Connection state
    connecting_from: Option<(usize, PortType, usize)>,
    current_mouse_pos: (f32, f32),
    pending_connection: Option<(String, String)>,
    hovered_port: Option<(usize, PortType, usize)>,

    /// The wire the in-flight node drag would splice into, as (src node id,
    /// dest node id) — ids, not indices, because hosts re-sync nodes on
    /// every window event and an index would go stale between drag_update
    /// and the release (the hovered_port lesson). Drawn highlighted while it
    /// holds; resolved into `pending_splice` on drop.
    splice_target: Option<(String, String)>,
    /// A completed splice drop for the host: (dragged node id, the wire's
    /// upstream node NAME — what Input params store, the wire's downstream
    /// node id). The host rewires: dragged.Input = upstream name,
    /// downstream.Input = dragged's name.
    pending_splice: Option<(String, String, String)>,
}

impl Graph {
    pub fn new() -> Adapted<Graph> {
        crate::layout::lazy_init_style_registry();

        let pitch_x = crate::layout::graph_spacing_x();
        let pitch_y = crate::layout::graph_spacing_y();
        let node_w = crate::layout::graph_node_width();
        let node_h = crate::layout::graph_node_height();
        let grid_snap_enabled = crate::layout::graph_grid_snap();

        let cell_col = crate::color::graph_cell_color();
        let gap_col = crate::color::graph_gap_color();

        Adapted::new(Graph {
            show_network_grid: false,
            pitch_x,
            pitch_y,
            node_w,
            node_h,
            grid_origin_x: 0.0,
            grid_origin_y: 0.0,
            pan_motion: crate::widget::ScrollMotion::new(),
            nodes: Vec::new(),
            selected_idx: None,
            selected_id: None,
            double_clicked_id: None,
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
            node_opacity: crate::color::graph_node_opacity(),
            cell_color: cell_col,
            gap_color: gap_col,
            connecting_from: None,
            current_mouse_pos: (0.0, 0.0),
            pending_connection: None,
            hovered_port: None,
            splice_target: None,
            pending_splice: None,
        })
    }

    pub fn set_uniform_background(&mut self, uniform: bool) {
        self.uniform_background = uniform;
    }
    pub fn set_network_opacity(&mut self, opacity: f32) {
        self.network_opacity = opacity;
    }
    pub fn set_node_opacity(&mut self, opacity: f32) {
        self.node_opacity = opacity;
    }
    /// The pitch: centre of one grid line to the centre of the next, per axis.
    pub fn grid_pitch(&self) -> (f32, f32) {
        (self.pitch_x, self.pitch_y)
    }
    /// The node body's size at the current zoom.
    pub fn node_size(&self) -> (f32, f32) {
        (self.node_w, self.node_h)
    }
    /// Set the node body's size — what the cell-model setters do too, since
    /// there the cell IS the node.
    pub fn set_node_size(&mut self, w: f32, h: f32) {
        self.node_w = w;
        self.node_h = h;
    }
    /// The cell-model view of the lattice: the node body (its "cell").
    pub fn grid_sizes(&self) -> (f32, f32) {
        (self.node_w, self.node_h)
    }
    /// The cell-model view of the lattice: what a pitch has beyond the node
    /// body, as (row gap, column gap) — the order `set_skipped_sizes` takes.
    pub fn skipped_sizes(&self) -> (f32, f32) {
        (self.pitch_y - self.node_h, self.pitch_x - self.node_w)
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

    /// The top-left corner of a node body centred on lattice cell (col, row).
    fn cell_origin(&self, col: f32, row: f32) -> (f32, f32) {
        (
            self.grid_origin_x + col * self.pitch_x - self.node_w * 0.5,
            self.grid_origin_y + row * self.pitch_y - self.node_h * 0.5,
        )
    }

    /// The lattice cell whose intersection is nearest the CENTRE of a node
    /// body whose top-left is (nx, ny) — the one snapping rule, shared by the
    /// drag preview, the drop-target highlight and the drop itself. None on a
    /// degenerate pitch.
    fn nearest_cell(&self, nx: f32, ny: f32) -> Option<(f32, f32)> {
        if self.pitch_x <= 0.0 || self.pitch_y <= 0.0 {
            return None;
        }
        let c = ((nx + self.node_w * 0.5 - self.grid_origin_x) / self.pitch_x).round();
        let r = ((ny + self.node_h * 0.5 - self.grid_origin_y) / self.pitch_y).round();
        Some((c, r))
    }

    pub fn node_rect(&self, idx: usize) -> Option<(f32, f32, f32, f32)> {
        let node = self.nodes.get(idx)?;
        let at_cell = self.cell_origin(node.position.0, node.position.1);
        let (nx, ny) = if self.dragging_idx == Some(idx) {
            self.drag_node_pos.unwrap_or(at_cell)
        } else {
            at_cell
        };
        Some((nx, ny, self.node_w, self.node_h))
    }

    /// The rect the in-flight node drag will deposit its body on — hosts
    /// highlight it as the drop target. Runs the SAME resolution as
    /// `commit_drag` (nearest intersection, then `find_empty_cell` walks off
    /// occupied ones), so the highlight never lies about where the node
    /// actually lands. None outside a node drag.
    pub fn drop_target_cell_rect(&self) -> Option<(f32, f32, f32, f32)> {
        let idx = self.dragging_idx?;
        let (nx, ny) = self.drag_node_pos?;
        let (c, r) = self.nearest_cell(nx, ny)?;
        let (c, r) = self.find_empty_cell(c, r, Some(idx));
        let (x, y) = self.cell_origin(c, r);
        Some((x, y, self.node_w, self.node_h))
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

    /// The topmost node whose body contains (px, py), in the same
    /// window-absolute space `node_rect` reports (grid_origin = pane + pan).
    /// Reverse order so a later-drawn node wins where bodies overlap.
    pub fn node_at(&self, px: f32, py: f32) -> Option<usize> {
        for i in (0..self.nodes.len()).rev() {
            if let Some((nx, ny, nw, nh)) = self.node_rect(i) {
                if px >= nx && px < nx + nw && py >= ny && py < ny + nh {
                    return Some(i);
                }
            }
        }
        None
    }

    /// How far a port's center floats off its node edge: the connector's own
    /// radius plus a small gap, so the circle sits fully OUTSIDE the node's
    /// bounding box rather than straddling its border.
    fn port_offset(scale_f: f32) -> f32 {
        (crate::layout::graph_connector_size() * scale_f).max(2.0) / 2.0 + 2.0 * scale_f
    }

    /// A port's center in graph coordinates — the ONE source for drawing,
    /// hover, click hit-testing, and wire endpoints, so they cannot drift.
    /// Inputs float above the node's top edge, outputs below its bottom.
    pub fn port_center(&self, idx: usize, port_type: PortType, k: usize) -> Option<(f32, f32)> {
        let (nx, ny, nw, nh) = self.node_rect(idx)?;
        let node = self.nodes.get(idx)?;
        let offset = Self::port_offset(nw / 80.0);
        match port_type {
            PortType::Input => (k < node.inputs)
                .then(|| (nx + nw * (k + 1) as f32 / (node.inputs + 1) as f32, ny - offset)),
            PortType::Output => (k < node.outputs)
                .then(|| (nx + nw * (k + 1) as f32 / (node.outputs + 1) as f32, ny + nh + offset)),
        }
    }

    pub fn toggle_rect(&self, idx: usize) -> Option<(f32, f32, f32, f32)> {
        if let Some(node) = self.nodes.get(idx) {
            // Settings containers have no geometry to toggle: utility nodes,
            // the designer's session node that now nests them, and the
            // per-node meta (preferences) node.
            if node.node_type == "utility" || node.node_type == "session" || node.node_type == "meta" {
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

    /// Scale the lattice and the node bodies together; the limits are on the
    /// node width, as they always were.
    fn scale_by(&mut self, factor: f32) {
        self.pitch_x *= factor;
        self.pitch_y *= factor;
        self.node_w *= factor;
        self.node_h *= factor;
    }

    pub fn zoom_in(&mut self) {
        if self.node_w < 400.0 {
            self.scale_by(1.1);
        }
    }

    pub fn zoom_out(&mut self) {
        if self.node_w > 40.0 {
            self.scale_by(1.0 / 1.1);
        }
    }

    pub fn zoom_by_factor(&mut self, factor: f32) {
        let new_w = self.node_w * factor;
        if new_w >= 40.0 && new_w <= 400.0 {
            self.scale_by(factor);
        }
    }

    /// The graph's plate, as the colour-typed host paints it: the cell
    /// colour (or a near-transparent black) at the network opacity, and
    /// under `graph_blur` a frosted material whose tint alpha IS the blur
    /// value — the knob doubles as the frost's opacity.
    fn bg_color(&self) -> [f32; 4] {
        use crate::scene::{Frost, Material, PlateRole};
        let c = if self.uniform_background {
            [self.cell_color[0], self.cell_color[1], self.cell_color[2], self.network_opacity]
        } else {
            [0.0, 0.0, 0.0, 0.01 * self.network_opacity]
        };
        let blur_val = crate::layout::graph_blur();
        let m = if blur_val > 0.0 {
            Material::opaque([c[0], c[1], c[2], blur_val.abs() * self.network_opacity]).with_frost(Frost::from_style())
        } else {
            Material::opaque(c)
        };
        m.fill(PlateRole::Nested)
    }

    /// The corner radius of anything node-shaped at the current zoom — the
    /// hosts' empty-cell cursor and drop-target highlight read it. Clamped to
    /// a quarter sweep of the node body; 0 when the body is degenerate.
    pub fn cell_corner_radius(&self) -> f32 {
        // Pure GEOMETRY — no display gating: the cursor and the highlight
        // exist whether or not the lattice is drawn. Gating on
        // show_network_grid/uniform_background silently squared those
        // consumers whenever the grid was hidden.
        if self.node_w <= 0.0 || self.node_h <= 0.0 {
            return 0.0;
        }
        let body = self.node_w.min(self.node_h);
        // The NODE radius, so the cursor sitting on a node's cell traces the
        // same silhouette the node does.
        crate::layout::graph_node_corner_radius().min(body / 2.0)
    }

    /// The grid lines, flat, over whatever the graph is painted on — the
    /// pane plate. A lattice of lines one pitch apart in the gap colour at
    /// the network opacity, each centred on its coordinate (the pitch is
    /// measured centre to centre, and `graph_line_width` only thickens
    /// them), so the intersections are exactly where the node centres go.
    /// Plus the origin axes: the two lines through the (0, 0) intersection,
    /// 2px, in the axis colour. Gated on the grid's visibility only:
    /// `uniform_background` describes the widget's own background fill and
    /// the designer hard-codes it true, which is how its grid went undrawn
    /// until 2026-09-20. (Rounded cells with grout between them came before
    /// the lattice; a node then FILLED a cell rather than sitting on a
    /// crossing.)
    pub fn paint_grid(&self, rect: Rect, pc: &mut PaintCtx) {
        if self.pitch_x <= 0.0 || self.pitch_y <= 0.0 {
            return;
        }
        let (min_x, min_y) = (rect.x, rect.y);
        let (max_x, max_y) = (rect.x + rect.width, rect.y + rect.height);
        let clipped = |qx: f32, qy: f32, qw: f32, qh: f32, c: [f32; 4], pc: &mut PaintCtx| {
            let x1 = qx.max(min_x);
            let y1 = qy.max(min_y);
            let x2 = (qx + qw).min(max_x);
            let y2 = (qy + qh).min(max_y);
            if x2 > x1 && y2 > y1 {
                pc.quad(Rect { x: x1, y: y1, width: x2 - x1, height: y2 - y1 }, c);
            }
        };

        let line = crate::layout::graph_line_width().max(0.0);
        if self.show_network_grid && line > 0.0 && self.pitch_x >= 4.0 && self.pitch_y >= 4.0 {
            let color = [self.gap_color[0], self.gap_color[1], self.gap_color[2], self.network_opacity];
            // The line indices that can cross the rect, one past each edge so
            // a line's own width never pops at the boundary.
            let c0 = ((min_x - self.grid_origin_x) / self.pitch_x).floor() as i32 - 1;
            let c1 = ((max_x - self.grid_origin_x) / self.pitch_x).ceil() as i32 + 1;
            for c in c0..=c1 {
                let x = self.grid_origin_x + c as f32 * self.pitch_x;
                clipped(x - line / 2.0, rect.y, line, rect.height, color, pc);
            }
            let r0 = ((min_y - self.grid_origin_y) / self.pitch_y).floor() as i32 - 1;
            let r1 = ((max_y - self.grid_origin_y) / self.pitch_y).ceil() as i32 + 1;
            for r in r0..=r1 {
                let y = self.grid_origin_y + r as f32 * self.pitch_y;
                clipped(rect.x, y - line / 2.0, rect.width, line, color, pc);
            }
        }

        // Origin axes: the lattice lines through the (0, 0) intersection.
        let axis = [0.0, 0.0, 0.0, self.network_opacity];
        let thickness = 2.0;
        clipped(rect.x, self.grid_origin_y - thickness / 2.0, rect.width, thickness, axis, pc);
        clipped(self.grid_origin_x - thickness / 2.0, rect.y, thickness, rect.height, axis, pc);
    }

    /// The wires / connection preview / node bodies / toggles as plain quads — the legacy
    /// `extra_quads` body, against `rect` instead of a stored rect. The [`TaggedQuad`] cell
    /// tag is always `None` now: the lattice is `paint_grid`'s, and it has no cells.
    pub fn geometry_quads_tagged(&self, rect: Rect) -> Vec<TaggedQuad> {
        let mut quads = Vec::new();
        let min_x = rect.x;
        let min_y = rect.y;
        let max_x = rect.x + rect.width;
        let max_y = rect.y + rect.height;
        let push_clipped = |qx: f32, qy: f32, qw: f32, qh: f32, qc: [f32; 4], q: &mut Vec<TaggedQuad>| {
            let rx1 = qx.max(min_x);
            let ry1 = qy.max(min_y);
            let rx2 = (qx + qw).min(max_x);
            let ry2 = (qy + qh).min(max_y);
            let rw = rx2 - rx1;
            let rh = ry2 - ry1;
            if rw > 0.0 && rh > 0.0 {
                q.push((rx1, ry1, rw, rh, qc, None));
            }
        };

        // A three-segment orthogonal wire from (start_x, start_y) down/up to (end_x, end_y).
        let scale_f = self.node_w / 80.0;
        let wire_thickness = (3.0 * scale_f).clamp(1.0, 15.0);
        let push_wire = |start_x: f32, start_y: f32, end_x: f32, end_y: f32, color: [f32; 4], q: &mut Vec<TaggedQuad>| {
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

        // Connection wires: each node with an "input" parameter draws a wire
        // from that source node's first output port to its own first input
        // port (pairs and endpoints from the shared helpers the splice hit
        // test also reads). The wire an in-flight node drag would splice
        // into draws in the highlight color — the drop affordance.
        let wire_color = [0.0, 0.75, 1.0, 0.7 * self.node_opacity]; // Vibrant cyan glow
        let hl = crate::color::graph_wire_highlight_color();
        let splice_color = [hl[0], hl[1], hl[2], hl[3] * self.node_opacity];
        for (src_idx, i) in self.wire_pairs() {
            if let Some(((start_x, start_y), (end_x, end_y))) = self.wire_endpoints(src_idx, i) {
                let is_splice_target = self
                    .splice_target
                    .as_ref()
                    .map(|(s, d)| self.nodes[src_idx].id == *s && self.nodes[i].id == *d)
                    .unwrap_or(false);
                let color = if is_splice_target { splice_color } else { wire_color };
                push_wire(start_x, start_y, end_x, end_y, color, &mut quads);
            }
        }

        // Connection preview while dragging one out
        if let Some((node_idx, port_type, port_idx)) = self.connecting_from {
            if let Some((start_x, start_y)) = self.port_center(node_idx, port_type, port_idx) {
                let preview_color = [1.0, 0.6, 0.0, 0.8]; // Golden orange preview
                push_wire(start_x, start_y, self.current_mouse_pos.0, self.current_mouse_pos.1, preview_color, &mut quads);
            }
        }

        // The grid lines and the origin axes are `paint_grid`'s — hosts that
        // draw these quads themselves call it at the same point in their walk.

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
                bg_color[3] *= self.node_opacity;
                if nx + nw > min_x && nx < max_x && ny + nh > min_y && ny < max_y {
                    quads.push((nx, ny, nw, nh, bg_color, None));
                }

                // The geometry toggle is a single-color circle now — it draws
                // through the circles channel (see port_circles), not as
                // quads: a filled dot when the geometry is visible, the same
                // color faded when hidden. The old look was a two-tone square
                // (state square inside a hover-tinted well).
                let _ = scale_f;
            }
        }

        quads
    }

    /// [`geometry_quads_tagged`](Self::geometry_quads_tagged) with the cell tags
    /// stripped — the unchanged legacy `extra_quads` shape.
    fn geometry_quads(&self, rect: Rect) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        self.geometry_quads_tagged(rect)
            .into_iter()
            .map(|(qx, qy, qw, qh, qc, _)| (qx, qy, qw, qh, qc))
            .collect()
    }

    /// The rounded view of the same geometry — the legacy `all_rounded_quads` conversion: the
    /// widget background, then each plain quad either as a grid cell (superellipse cell arcs),
    /// a node body (node corner radius, all corners), or with the widget's edge-corner
    /// resolution.
    fn rounded_geometry(&self, rect: Rect) -> Vec<(f32, f32, f32, f32, f32, [f32; 4], (bool, bool, bool, bool))> {
        let mut rounded = Vec::new();

        let (w_tl, w_tr, w_br, w_bl) = WIDGET_CORNERS;
        rounded.push((rect.x, rect.y, rect.width, rect.height, WIDGET_RADIUS, self.bg_color(), WIDGET_CORNERS));

        let node_radius = crate::layout::graph_node_corner_radius();
        let cell_radius = self.cell_corner_radius();
        let (wx, wy, ww, wh) = (rect.x, rect.y, rect.width, rect.height);

        for (qx, qy, qw, qh, qc, cell) in self.geometry_quads_tagged(rect) {
            if self.is_node_rect(qx, qy, qw, qh) {
                rounded.push((qx, qy, qw, qh, node_radius, qc, (true, true, true, true)));
            } else {
                let tl = w_tl && qx <= wx + 1.5 && qy <= wy + 1.5;
                let tr = w_tr && qx + qw >= wx + ww - 1.5 && qy <= wy + 1.5;
                let br = w_br && qx + qw >= wx + ww - 1.5 && qy + qh >= wy + wh - 1.5;
                let bl = w_bl && qx <= wx + 1.5 && qy + qh >= wy + wh - 1.5;

                if let Some((ctl, ctr, cbr, cbl)) = cell {
                    // A cell cut by the pane's own rounded corner wears the
                    // widget arc there; its interior corners keep the cell arc.
                    let r = if tl || tr || br || bl { cell_radius.max(WIDGET_RADIUS) } else { cell_radius };
                    rounded.push((qx, qy, qw, qh, r, qc, (ctl || tl, ctr || tr, cbr || br, cbl || bl)));
                } else {
                    let r = if tl || tr || br || bl { WIDGET_RADIUS } else { 0.0 };
                    rounded.push((qx, qy, qw, qh, r, qc, (tl, tr, br, bl)));
                }
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

        // Geometry toggles: one circle per toggleable node, a SINGLE color —
        // TOGGLE_ON at full alpha when visible, the same color faded when
        // hidden; hover grows the radius the way port dots do, so no second
        // hover tint is needed. Hit-testing stays toggle_rect's square (the
        // circle is inscribed in it).
        for i in 0..self.nodes.len() {
            if let Some((tx, ty, tw, th)) = self.toggle_rect(i) {
                let cx = tx + tw / 2.0;
                let cy = ty + th / 2.0;
                let mut r = tw.min(th) / 2.0;
                if self.toggle_hovered_idx == Some(i) {
                    r *= 1.15;
                }
                let mut c = colors::TOGGLE_ON;
                if !self.nodes[i].geom_visible {
                    c[3] *= 0.25;
                }
                c[3] *= self.node_opacity;
                push_circle_clipped(cx, cy, r, c);
            }
        }

        let conn_size = crate::layout::graph_connector_size();
        let mut conn_color = colors::graph_connector_color();
        let mut conn_hl_color = colors::graph_connector_highlight_color();
        conn_color[3] *= self.node_opacity;
        conn_hl_color[3] *= self.node_opacity;

        for i in 0..self.nodes.len() {
            if let Some((_, _, nw, _)) = self.node_rect(i) {
                let scale_f = nw / 80.0;
                let port_size = (conn_size * scale_f).max(2.0);
                let base_r = port_size / 2.0;

                let node = &self.nodes[i];

                for (port_type, count) in
                    [(PortType::Input, node.inputs), (PortType::Output, node.outputs)]
                {
                    for k in 0..count {
                        let Some((cx, cy)) = self.port_center(i, port_type, k) else { continue };

                        let is_hovered = self.hovered_port == Some((i, port_type, k));
                        let is_connecting = self.connecting_from == Some((i, port_type, k));

                        let (r, color) = if is_hovered || is_connecting {
                            (base_r * 1.4, conn_hl_color)
                        } else {
                            (base_r, conn_color)
                        };
                        push_circle_clipped(cx, cy, r, color);
                    }
                }
            }
        }
        circles
    }

    /// Node-name labels beside each node, scaled with the grid, included only when they
    /// intersect the widget rect (legacy `text_labels`).
    /// A node's name hangs off its body's RIGHT edge — an 8 px gap and a
    /// 14 px font, both scaled with the body against its 80 px baseline —
    /// unless it would not fit there and fits on the LEFT, where it hangs
    /// off the left edge instead, right-aligned to it. A node parked against
    /// the pane's right edge used to draw with no name at all: the label
    /// began past the edge and the cull dropped it whole, whatever its
    /// length. Frame All assumes the right-hand placement, which is safe —
    /// after framing every label fits on the right and none flips.
    fn node_labels(&self, rect: Rect) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        for (i, node) in self.nodes.iter().enumerate() {
            if let Some((nx, ny, nw, nh)) = self.node_rect(i) {
                let scale_f = nw / 80.0;
                let font_size = (14.0 * scale_f).clamp(6.0, 48.0);
                let gap = 8.0 * scale_f;
                let ly = crate::layout::align_text_y(ny, nh, font_size, 0.0);
                let text_w = TextLabel::estimate_width(&node.name, font_size);
                let right = nx + nw + gap;
                let left = nx - gap - text_w;
                let fits_right = right + text_w <= rect.x + rect.width;
                let fits_left = left >= rect.x;
                let lx = if !fits_right && fits_left { left } else { right };
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
        // The background is the first entry; the grid lines go over it
        // before the wires and nodes.
        for (i, (qx, qy, qw, qh, r, c, corners)) in self.rounded_geometry(rect).into_iter().enumerate() {
            ctx.rounded_rect(Rect { x: qx, y: qy, width: qw, height: qh }, r, corners, c);
            if i == 0 {
                self.paint_grid(rect, ctx);
            }
        }
        for (cx, cy, r, c) in self.port_circles(rect) {
            ctx.circle(cx, cy, r, c);
        }
        // Node names are arbitrary and the canvas is fixed, so a long name on a
        // node near the right edge used to draw off the graph entirely.
        let canvas = Some([rect.x, rect.y, rect.x + rect.width, rect.y + rect.height]);
        for l in self.node_labels(rect) {
            ctx.text_with(l.text, l.x, l.y, l.font_size, l.color, None, canvas);
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
    /// Advances the pan glide/coast behind the grid origin. Idle is a no-op.
    fn tick(&mut self, dt: f32, _rect: Rect) -> bool {
        self.pan_motion.reconcile(self.grid_origin_x, self.grid_origin_y);
        if !self.pan_motion.is_animating() {
            return false;
        }
        let free = crate::widget::Bounds::UNBOUNDED;
        let moved = self.pan_motion.tick(dt, free, free);
        self.grid_origin_x = self.pan_motion.x.pos();
        self.grid_origin_y = self.pan_motion.y.pos();
        moved || self.pan_motion.is_animating()
    }

    fn wants_tick(&self) -> bool {
        true
    }

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
                    if let Some((_, _, nw, _)) = self.node_rect(i) {
                        let scale_f = nw / 80.0;
                        let hit_radius = (conn_act_r * scale_f).max(2.0);
                        let node = &self.nodes[i];

                        for (port_type, count) in
                            [(PortType::Input, node.inputs), (PortType::Output, node.outputs)]
                        {
                            for k in 0..count {
                                let Some((cx, cy)) = self.port_center(i, port_type, k) else {
                                    continue;
                                };
                                if (px - cx).powi(2) + (py - cy).powi(2) <= hit_radius.powi(2) {
                                    self.hovered_port = Some((i, port_type, k));
                                }
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
                    // Pan: the origin moves WITH the wheel sign (no negation —
                    // the canvas follows the gesture), across an unbounded plane.
                    let (dx, dy) = match delta {
                        MouseScrollDelta::LineDelta(x, y) => (*x * 15.0, *y * 15.0),
                        MouseScrollDelta::PixelDelta(pos) => (pos.x as f32, pos.y as f32),
                    };
                    let discrete = matches!(delta, MouseScrollDelta::LineDelta(..));
                    let free = crate::widget::Bounds::UNBOUNDED;
                    self.pan_motion.reconcile(self.grid_origin_x, self.grid_origin_y);
                    self.pan_motion.apply_px(dx, dy, discrete, free, free);
                    self.grid_origin_x = self.pan_motion.x.pos();
                    self.grid_origin_y = self.pan_motion.y.pos();
                    true
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

            // Snapping centres the body on the nearest intersection.
            let (nx, ny) = match self.nearest_cell(nx, ny) {
                Some((c, r)) if self.grid_snap_enabled => self.cell_origin(c, r),
                _ => (nx, ny),
            };

            self.drag_node_pos = Some((nx, ny));
            // The wire the ghost sits on right now, held by id (hosts
            // re-sync between events) and drawn highlighted — the drop
            // affordance the user aims by.
            self.splice_target = self
                .dragging_idx
                .and_then(|i| self.splice_wire_at(i, nx, ny))
                .map(|(s, d)| (self.nodes[s].id.clone(), self.nodes[d].id.clone()));
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
            if let Some((_, _, nw, _)) = self.node_rect(i) {
                let scale_f = nw / 80.0;
                let conn_act_r = crate::layout::graph_connector_activation_radius();
                let port_click_radius = (conn_act_r * scale_f).max(2.0);
                let port_click_radius_sq = port_click_radius * port_click_radius;

                let node = &self.nodes[i];
                for (port_type, count) in
                    [(PortType::Input, node.inputs), (PortType::Output, node.outputs)]
                {
                    for k in 0..count {
                        let Some((port_x, port_y)) = self.port_center(i, port_type, k) else {
                            continue;
                        };
                        let dx = px - port_x;
                        let dy = py - port_y;
                        if dx * dx + dy * dy <= port_click_radius_sq {
                            if let Some((src_idx, src_port_type, _src_port_idx)) = self.connecting_from {
                                // A click on the opposite port kind of ANOTHER
                                // node completes the connection; anything else
                                // cancels it.
                                if src_idx != i && src_port_type != port_type {
                                    let (out_idx, in_idx) = if port_type == PortType::Input {
                                        (src_idx, i)
                                    } else {
                                        (i, src_idx)
                                    };
                                    let output_node = &self.nodes[out_idx];
                                    let input_node = &self.nodes[in_idx];
                                    self.pending_connection =
                                        Some((input_node.id.clone(), output_node.name.clone()));
                                }
                                self.connecting_from = None;
                            } else {
                                self.connecting_from = Some((i, port_type, k));
                                self.current_mouse_pos = (px, py);
                            }
                            return true;
                        }
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
                    let clicked_id = self.nodes[i].id.clone();
                    if let Some((prev_time, prev_id)) = self.double_click_timer.take() {
                        if prev_id == clicked_id && now.duration_since(prev_time) < std::time::Duration::from_millis(500) {
                            self.double_clicked_id = Some(clicked_id.clone());
                        }
                    }
                    self.double_click_timer = Some((now, clicked_id));
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

    /// The wire pairs the draw pass renders: (src idx, dest idx), one per
    /// node whose "Input" parameter names another node — the ONE derivation,
    /// shared with the splice hit test so the two cannot disagree about
    /// where a wire is.
    fn wire_pairs(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for i in 0..self.nodes.len() {
            let node = &self.nodes[i];
            if let Some((_, input_name, _)) =
                node.parameters.iter().find(|(name, _, _)| name.eq_ignore_ascii_case("input"))
            {
                if let Some(src_idx) = self.nodes.iter().position(|n| n.name == *input_name) {
                    out.push((src_idx, i));
                }
            }
        }
        out
    }

    /// A wire's two attachment points — the port circles' centers, falling
    /// back to the node edge midpoints for portless nodes. The three-segment
    /// shape (down, across, down) derives from these in both the draw pass
    /// and [`Self::wire_segment_rects`].
    fn wire_endpoints(&self, src_idx: usize, dest_idx: usize) -> Option<((f32, f32), (f32, f32))> {
        let (sx, sy, sw, sh) = self.node_rect(src_idx)?;
        let (ex, ey, ew, _eh) = self.node_rect(dest_idx)?;
        let start = self
            .port_center(src_idx, PortType::Output, 0)
            .unwrap_or((sx + sw / 2.0, sy + sh));
        let end = self
            .port_center(dest_idx, PortType::Input, 0)
            .unwrap_or((ex + ew / 2.0, ey));
        Some((start, end))
    }

    /// The wire's three segments as axis-aligned rects at the drawn
    /// thickness — `push_wire`'s exact shape, unclipped.
    fn wire_segment_rects(&self, src_idx: usize, dest_idx: usize) -> Option<[(f32, f32, f32, f32); 3]> {
        let ((start_x, start_y), (end_x, end_y)) = self.wire_endpoints(src_idx, dest_idx)?;
        let scale_f = self.node_w / 80.0;
        let t = (3.0 * scale_f).clamp(1.0, 15.0);
        let mid_y = start_y + (end_y - start_y) / 2.0;
        let v1 = (start_x - t / 2.0, start_y.min(mid_y), t, (start_y - mid_y).abs());
        let h = (start_x.min(end_x), mid_y - t / 2.0, (end_x - start_x).abs(), t);
        let v2 = (end_x - t / 2.0, mid_y.min(end_y), t, (end_y - mid_y).abs());
        Some([v1, h, v2])
    }

    /// The wire the dragged node's ghost at (nx, ny) would splice into —
    /// the first pair (draw order) whose segment run touches the ghost rect,
    /// inflated by the wire activation radius so a near miss still takes.
    /// The dragged node's own wires never count (dropping a node on a wire
    /// it is already an end of is a move, not a rewire), and a node with no
    /// "Input" parameter or no output port cannot sit mid-chain.
    fn splice_wire_at(&self, idx: usize, nx: f32, ny: f32) -> Option<(usize, usize)> {
        let node = self.nodes.get(idx)?;
        let has_input = node
            .parameters
            .iter()
            .any(|(name, _, _)| name.eq_ignore_ascii_case("input"));
        if !has_input || node.outputs == 0 {
            return None;
        }
        let (_, _, nw, nh) = self.node_rect(idx)?;
        let pad = crate::layout::graph_wire_activation_radius().max(0.0);
        let (gx1, gy1) = (nx - pad, ny - pad);
        let (gx2, gy2) = (nx + nw + pad, ny + nh + pad);
        for (src, dest) in self.wire_pairs() {
            if src == idx || dest == idx {
                continue;
            }
            let Some(segs) = self.wire_segment_rects(src, dest) else { continue };
            let hit = segs.iter().any(|&(x, y, w, h)| {
                x < gx2 && x + w > gx1 && y < gy2 && y + h > gy1
            });
            if hit {
                return Some((src, dest));
            }
        }
        None
    }

    /// Drop the in-flight node drag onto the nearest free intersection (legacy `drag_end`),
    /// resolving a held splice target into `pending_splice` for the host.
    fn commit_drag(&mut self) {
        let target = self.splice_target.take();
        if let Some((nx, ny)) = self.drag_node_pos.take() {
            if let Some(idx) = self.dragging_idx.take() {
                if let Some((c, r)) = self.nearest_cell(nx, ny) {
                    let (c, r) = self.find_empty_cell(c, r, Some(idx));
                    self.nodes[idx].position = (c, r);
                }
                if let Some((src_id, dest_id)) = target {
                    let src_name = self.nodes.iter().find(|n| n.id == src_id).map(|n| n.name.clone());
                    let dest_ok = self.nodes.iter().any(|n| n.id == dest_id);
                    if let (Some(src_name), true) = (src_name, dest_ok) {
                        self.pending_splice = Some((self.nodes[idx].id.clone(), src_name, dest_id));
                    }
                }
            }
        } else {
            self.dragging_idx = None;
        }
        self.dragging_id = None;
    }
}

impl GraphController for Graph {
    fn paint_grid(&self, rect: Rect, pc: &mut PaintCtx) {
        Graph::paint_grid(self, rect, pc)
    }
    fn set_nodes(&mut self, nodes: &[GraphNode]) {
        // Hover carries a node INDEX, so remap it by id across the rebuild
        // instead of clearing — hosts (the designer) re-sync nodes on EVERY
        // window event, so a clear here wipes the hover in the same event
        // pass that set it and port highlights never survive to a draw.
        // Exactly the id-remap `selected_idx` gets below.
        self.hovered_port = self.hovered_port.take().and_then(|(idx, pt, k)| {
            let id = &self.nodes.get(idx)?.id;
            let new_idx = nodes.iter().position(|n| &n.id == id)?;
            let count = match pt {
                PortType::Input => nodes[new_idx].inputs,
                PortType::Output => nodes[new_idx].outputs,
            };
            (k < count).then_some((new_idx, pt, k))
        });
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
                self.splice_target = None;
            }
        } else {
            self.dragging_idx = None;
            self.drag_node_pos = None;
            self.splice_target = None;
        }

        // double_clicked_id / double_click_timer survive deliberately: they
        // are keyed by node id, and clearing them here (as this used to)
        // guaranteed no double-click could ever complete — the host re-syncs
        // between the two presses. A stale id simply resolves to None.
        self.toggle_hovered_idx = None;
    }
    fn get_nodes(&self) -> Vec<GraphNode> { self.nodes.clone() }
    fn cell_corner_radius(&self) -> f32 { Graph::cell_corner_radius(self) }
    fn geometry_quads_tagged(&self, rect: Rect) -> Vec<TaggedQuad> { Graph::geometry_quads_tagged(self, rect) }
    fn drop_target_cell_rect(&self) -> Option<(f32, f32, f32, f32)> { Graph::drop_target_cell_rect(self) }
    fn selected_node(&self) -> Option<usize> { self.selected_idx }
    fn set_selected_node(&mut self, idx: Option<usize>) {
        self.selected_idx = idx;
        self.selected_id = idx.and_then(|i| self.nodes.get(i).map(|n| n.id.clone()));
    }
    fn double_clicked_node(&self) -> Option<usize> {
        self.double_clicked_id
            .as_ref()
            .and_then(|id| self.nodes.iter().position(|n| &n.id == id))
    }
    fn clear_double_clicked_node(&mut self) { self.double_clicked_id = None; }
    fn set_grid_snap_enabled(&mut self, enabled: bool) { self.grid_snap_enabled = enabled; }
    fn take_node_geom_toggle(&mut self) -> Option<(usize, bool)> { self.node_geom_toggled.take() }
    fn set_grid_snap(&mut self, gx: f32, gy: f32) { self.set_grid_sizes(gx, gy) }
    fn set_grid_pitch(&mut self, px: f32, py: f32) {
        self.pitch_x = px;
        self.pitch_y = py;
    }
    fn set_node_size(&mut self, w: f32, h: f32) { Graph::set_node_size(self, w, h) }
    /// Cell model: the cell is the node body, and the gap it had stays, so
    /// the two setters commute (a cell plus its gap is a pitch).
    fn set_grid_sizes(&mut self, gx: f32, gy: f32) {
        let (gap_row, gap_col) = self.skipped_sizes();
        self.set_node_size(gx, gy);
        self.pitch_x = gx + gap_col;
        self.pitch_y = gy + gap_row;
    }
    fn set_skipped_sizes(&mut self, row_h: f32, col_w: f32) {
        self.pitch_x = self.node_w + col_w;
        self.pitch_y = self.node_h + row_h;
    }
    fn set_grid_origin(&mut self, ox: f32, oy: f32) { self.grid_origin_x = ox; self.grid_origin_y = oy; }
    fn grid_origin(&self) -> (f32, f32) { (self.grid_origin_x, self.grid_origin_y) }
    fn set_show_network_grid(&mut self, show: bool) { self.show_network_grid = show; }
    fn take_pending_connection(&mut self) -> Option<(String, String)> {
        self.pending_connection.take()
    }
    fn take_pending_splice(&mut self) -> Option<(String, String, String)> {
        self.pending_splice.take()
    }
    fn cancel_connecting(&mut self) {
        self.connecting_from = None;
    }
    fn is_node_rect(&self, qx: f32, qy: f32, qw: f32, qh: f32) -> bool {
        self.is_node_rect(qx, qy, qw, qh)
    }
    fn node_at(&self, px: f32, py: f32) -> Option<usize> {
        self.node_at(px, py)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::UiContext;
    use crate::widget::WidgetHost;

    /// A 100 x 60 lattice whose (0, 0) intersection is at (140, 120), so node
    /// a's 80 x 40 body is the rect (100, 100, 80, 40) and node b's, one cell
    /// down-right, is (200, 160, 80, 40).
    fn two_nodes() -> Adapted<Graph> {
        let mut g = Graph::new();
        WidgetHost::set_rect(&mut g, 0.0, 0.0, 800.0, 600.0);
        g.set_grid_pitch(100.0, 60.0);
        g.set_node_size(80.0, 40.0);
        g.set_grid_origin(140.0, 120.0);
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

    /// A node against the pane's right edge keeps its name: the label flips
    /// to the body's left when it would not fit on the right, whatever the
    /// name's length, and a node with room keeps the right-hand placement.
    #[test]
    fn a_node_at_the_right_edge_keeps_its_label_on_the_left() {
        let mut g = two_nodes();
        let node = |name: &str, col: f32| GraphNode {
            id: name.into(),
            name: name.into(),
            position: (col, 0.0),
            parameters: Vec::new(),
            geom_visible: true,
            node_type: String::new(),
            inputs: 1,
            outputs: 1,
        };
        // With the origin at 160, column 6's body spans 720..800 — flush
        // against the right edge of an 800 px pane, so a right-hand label
        // would BEGIN past the edge, which is exactly the screenshot that
        // found this. Column 2's spans 320..400: plenty of room.
        g.set_grid_origin(160.0, 120.0);
        g.set_nodes(&[node("w", 6.0), node("wrangle_with_a_long_name", 6.0), node("mid", 2.0)]);
        let rect = Rect { x: 0.0, y: 0.0, width: 800.0, height: 600.0 };
        let labels = g.node_labels(rect);
        assert_eq!(labels.len(), 3, "every node keeps a label: {:?}", labels.iter().map(|l| &l.text).collect::<Vec<_>>());
        let (nx, _, nw, _) = g.node_rect(0).unwrap();
        assert_eq!((nx, nx + nw), (720.0, 800.0));
        for name in ["w", "wrangle_with_a_long_name"] {
            let l = labels.iter().find(|l| l.text == name).unwrap();
            let w = TextLabel::estimate_width(name, l.font_size);
            assert!((l.x + w - (nx - 8.0)).abs() < 0.5, "{name} hangs off the left edge, right-aligned to it: x {} w {w}", l.x);
            assert!(l.x >= rect.x, "{name} stays inside the pane");
        }
        let mid = labels.iter().find(|l| l.text == "mid").unwrap();
        let (mx, _, mw, _) = g.node_rect(2).unwrap();
        assert_eq!(mid.x, mx + mw + 8.0, "a node with room keeps the right-hand placement");
    }

    /// A double-click's two presses always straddle a host node re-sync — the
    /// designer calls set_nodes on EVERY window event — so the detection state
    /// must survive set_nodes. It used to be wiped there wholesale, which made
    /// double-click structurally impossible outside unit tests.
    #[test]
    fn double_click_survives_the_between_press_node_resync() {
        let mut ctx = UiContext::new();
        let mut g = two_nodes();
        let (id, ptr) = (g.id(), g.as_ptr_mut());
        ctx.register_widget(id, ptr);

        // First press on node a, then the host re-syncs (same content),
        // then the second press: this is the real event sequence.
        assert!(g.mouse_input(MouseButton::Left, ElementState::Pressed, 110.0, 120.0, &mut ctx));
        g.mouse_input(MouseButton::Left, ElementState::Released, 110.0, 120.0, &mut ctx);
        let nodes = g.get_nodes();
        g.set_nodes(&nodes);
        assert!(g.mouse_input(MouseButton::Left, ElementState::Pressed, 110.0, 120.0, &mut ctx));

        assert_eq!(g.double_clicked_node(), Some(0), "double-click lost across set_nodes");
        g.clear_double_clicked_node();
        assert_eq!(g.double_clicked_node(), None);
    }

    /// Two presses on DIFFERENT nodes are not a double-click, id-keyed or not.
    #[test]
    fn presses_on_two_nodes_are_not_a_double_click() {
        let mut ctx = UiContext::new();
        let mut g = two_nodes();
        let (id, ptr) = (g.id(), g.as_ptr_mut());
        ctx.register_widget(id, ptr);

        assert!(g.mouse_input(MouseButton::Left, ElementState::Pressed, 110.0, 120.0, &mut ctx));
        g.mouse_input(MouseButton::Left, ElementState::Released, 110.0, 120.0, &mut ctx);
        // Node b sits one grid step down-right of a.
        assert!(g.mouse_input(MouseButton::Left, ElementState::Pressed, 210.0, 180.0, &mut ctx));
        assert_eq!(g.double_clicked_node(), None);
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
        assert!(g.is_dragging() && g.draggable());

        // Drag one pitch right (pitch_x = 100): snap puts the node at column 1, and cell
        // (1, 0) is free so it lands there.
        g.drag_begin(110.0, 120.0);
        assert!(g.drag_update(210.0, 120.0));
        assert!(g.mouse_input(MouseButton::Left, ElementState::Released, 210.0, 120.0, &mut ctx));
        assert!(!g.is_dragging());
        assert_eq!(g.get_nodes()[0].position, (1.0, 0.0));

        // An empty-space press clears the selection and is NOT consumed (legacy contract).
        assert!(!g.mouse_input(MouseButton::Left, ElementState::Pressed, 700.0, 550.0, &mut ctx));
        assert_eq!(g.selected_node(), None);
    }

    /// Dropping a dragged node onto a wire splices it in: the drop reports
    /// (dragged id, the wire's upstream NAME, the wire's downstream id) for
    /// the host to rewire both Input params. A drop away from every wire
    /// reports nothing, and the handshake is take-once.
    #[test]
    fn node_dropped_on_a_wire_reports_a_splice() {
        let mut ctx = UiContext::new();
        let mut g = Graph::new();
        WidgetHost::set_rect(&mut g, 0.0, 0.0, 800.0, 600.0);
        g.set_grid_pitch(100.0, 60.0);
        g.set_node_size(80.0, 40.0);
        g.set_grid_origin(140.0, 120.0);
        g.set_grid_snap_enabled(true);
        let node = |id: &str, name: &str, col: f32, row: f32, params: Vec<(String, String, String)>| GraphNode {
            id: id.into(),
            name: name.into(),
            position: (col, row),
            parameters: params,
            geom_visible: true,
            node_type: String::new(),
            inputs: 1,
            outputs: 1,
        };
        let p = |v: &str| vec![("Input".to_string(), v.to_string(), "text".to_string())];
        // alpha → beta wire runs through the empty cell (1, 0) between them;
        // gamma sits below, unwired.
        g.set_nodes(&[
            node("a", "alpha", 0.0, 0.0, Vec::new()),
            node("b", "beta", 2.0, 0.0, p("alpha")),
            node("c", "gamma", 0.0, 2.0, p("")),
        ]);
        let (id, ptr) = (g.id(), g.as_ptr_mut());
        ctx.register_widget(id, ptr);

        // Drag gamma's body onto the wire's horizontal run (cell (1, 0)).
        assert!(g.mouse_input(MouseButton::Left, ElementState::Pressed, 110.0, 240.0, &mut ctx));
        g.drag_begin(110.0, 240.0);
        assert!(g.drag_update(210.0, 140.0));
        assert!(g.mouse_input(MouseButton::Left, ElementState::Released, 210.0, 140.0, &mut ctx));

        let splice = GraphController::take_pending_splice(&mut *g);
        assert_eq!(
            splice,
            Some(("c".to_string(), "alpha".to_string(), "b".to_string())),
            "drop on the wire must report (dragged, upstream name, downstream id)"
        );
        assert_eq!(GraphController::take_pending_splice(&mut *g), None, "take-once");

        // A drop in open space reports nothing. Gamma landed in cell (1, 0)
        // — the free cell its splice drop resolved to — so drag it from
        // there down to open space clear of the wire.
        assert!(g.mouse_input(MouseButton::Left, ElementState::Pressed, 210.0, 110.0, &mut ctx));
        g.drag_begin(210.0, 110.0);
        assert!(g.drag_update(210.0, 230.0));
        assert!(g.mouse_input(MouseButton::Left, ElementState::Released, 210.0, 230.0, &mut ctx));
        assert_eq!(GraphController::take_pending_splice(&mut *g), None);
    }

    /// The grid has one size per axis, the pitch, and a node is CENTRED on
    /// the intersection its position names — its body straddles the lines
    /// rather than filling a cell between them. The body's size is its own:
    /// changing the pitch moves nodes apart without resizing them.
    #[test]
    fn nodes_are_centred_on_lattice_intersections() {
        let mut g = two_nodes();
        assert_eq!(g.grid_pitch(), (100.0, 60.0));
        assert_eq!(g.node_size(), (80.0, 40.0));
        g.set_grid_pitch(200.0, 90.0);
        assert_eq!(g.node_size(), (80.0, 40.0), "the pitch does not size the node");
        g.set_grid_pitch(100.0, 60.0);
        // Node a is on the (140, 120) intersection: its 80 x 40 body is
        // centred there.
        let (x, y, w, h) = g.node_rect(0).unwrap();
        assert_eq!((x + w / 2.0, y + h / 2.0), (140.0, 120.0));
        assert_eq!((x, y, w, h), (100.0, 100.0, 80.0, 40.0));
        // Node b, at (1, 1), is one pitch away along each axis.
        let (x, y, w, h) = g.node_rect(1).unwrap();
        assert_eq!((x + w / 2.0, y + h / 2.0), (240.0, 180.0));
    }

    /// The cell-and-gap setters describe the same lattice — a cell plus its
    /// gap is a pitch, and the cell is the node body — in either order, so a
    /// host still speaking them gets exactly the geometry it asked for.
    #[test]
    fn cell_and_gap_setters_describe_the_same_lattice() {
        let mut g = Graph::new();
        g.set_grid_sizes(140.0, 70.0);
        g.set_skipped_sizes(35.0, 35.0);
        assert_eq!(g.grid_pitch(), (175.0, 105.0));
        assert_eq!(g.node_size(), (140.0, 70.0));
        assert_eq!(g.grid_sizes(), (140.0, 70.0));
        assert_eq!(g.skipped_sizes(), (35.0, 35.0));

        let mut h = Graph::new();
        h.set_skipped_sizes(35.0, 35.0);
        h.set_grid_sizes(140.0, 70.0);
        assert_eq!(h.grid_pitch(), (175.0, 105.0));
        assert_eq!(h.node_size(), (140.0, 70.0));
    }

    #[test]
    fn port_click_starts_and_completes_a_connection() {
        let mut ctx = UiContext::new();
        let mut g = two_nodes();
        let (id, ptr) = (g.id(), g.as_ptr_mut());
        ctx.register_widget(id, ptr);

        // Ports float OUTSIDE the node box (port_center): node a's output
        // hangs below the bottom-center of (100,100,80,40), node b's input
        // above the top-center of (200,160,80,40).
        let (ax, ay) = g.port_center(0, PortType::Output, 0).expect("node a output port");
        assert!(ay > 140.0, "output port sits below the node's bottom edge");
        assert!(g.mouse_input(MouseButton::Left, ElementState::Pressed, ax, ay, &mut ctx));
        let (bx, by) = g.port_center(1, PortType::Input, 0).expect("node b input port");
        assert!(by < 160.0, "input port sits above the node's top edge");
        assert!(g.mouse_input(MouseButton::Left, ElementState::Pressed, bx, by, &mut ctx));

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
