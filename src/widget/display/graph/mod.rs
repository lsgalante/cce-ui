//! `Graph` — the node-network editor: a pannable/zoomable grid of
//! draggable nodes with geometry toggles, input/output ports, wire routing, and interactive
//! connection dragging. [`GraphController`] rides the `Input` capability hooks.
//!
//! [`Paint::paint`] emits the graph as rounded geometry (what `render_widget` hosts — cce-files —
//! and the scene walk — cce-graph — consume); a host that draws the nodes in its own order (the
//! designer) reads [`Graph::geometry_quads_tagged`] and calls [`Graph::paint_wires`] itself. Node-name
//! text is clipped to the widget rect via [`Paint::text_bounds`]. All grid geometry is in absolute screen space
//! (hosts pan by moving `grid_origin`); the widget rect only culls and clips.
//!
//! The WIRES are not quads: they are strokes in a [`WireStyle`] — orthogonal, rounded,
//! bezier or straight — painted by [`Graph::paint_wires`], which `paint` calls after the
//! grid and a host drawing the quads itself calls in the same place. Their colour, width
//! and style are `wire_color`, `wire_size` and `wire_style` under
//! `style.surface.graph.node`.
//!
//! The grid is a LATTICE OF LINES with one size per axis — the pitch, from the centre of
//! one line to the centre of the next — and a node is centred on the intersection its
//! `position` names: node (c, r) sits on `grid_origin + (c * pitch_x, r * pitch_y)`. The
//! node body has a size of its own (`graph_node_width` / `graph_node_height`, scaled with
//! the zoom), independent of the pitch. The cell-and-gap setters
//! describe the same lattice for hosts that speak it (a cell plus its gap is a pitch).
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the node and port types, `node_wires`, `Graph` and its construction and setters, `impl Layout` |
//! | `geometry` | the lattice, node rects, ports and toggles, a free cell, zoom |
//! | `wires` | wire styles and paths, stroke and turn, painting, a wire's endpoints and splicing onto one |
//! | `paint` | background, grid, node geometry, port circles, labels, `impl Paint` |
//! | `input` | `impl Input`: presses, drags and their commit, the swap target, the zoom keys |
//! | `controller` | `impl GraphController`: what a host reads and drives |

mod controller;
mod geometry;
mod input;
mod paint;
mod wires;
#[cfg(test)]
mod tests;

pub use wires::WireStyle;
#[cfg(test)]
use wires::*;

use crate::color;
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

/// The nodes `node` reads, one per input port in order: the values of its
/// parameters of type `node`, else its parameter named `input` alone (see
/// `Graph::wire_pairs`). Empty is a port with nothing wired to it.
pub fn node_wires(node: &GraphNode) -> Vec<String> {
    let typed: Vec<String> = node
        .parameters
        .iter()
        .filter(|(_, _, ty)| ty == "node")
        .map(|(_, value, _)| value.trim().to_string())
        .collect();
    if !typed.is_empty() {
        return typed;
    }
    node.parameters
        .iter()
        .find(|(name, _, _)| name.eq_ignore_ascii_case("input"))
        .map(|(_, value, _)| vec![value.clone()])
        .unwrap_or_default()
}

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
    /// Whether nodes wear the geometry toggle (see `toggle_rect`). A host
    /// whose nodes have no geometry to show (cce-files' directory graph)
    /// turns it off with `set_show_toggles(false)`.
    show_toggles: bool,
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
    double_click_timer: Option<(web_time::Instant, String)>,
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

    network_opacity: f32,
    /// Node-domain opacity (bodies, wires, connectors) — independent of
    /// `network_opacity`, which fades the pane surface (grid cells/gaps).
    node_opacity: f32,
    grid_color: [f32; 3],

    // Connection state
    connecting_from: Option<(usize, PortType, usize)>,
    current_mouse_pos: (f32, f32),
    /// A connection finished by the pointer: (input node id, output node
    /// name, the input PORT it was dropped on).
    pending_connection: Option<(String, String, usize)>,
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
    /// Whether a node dropped onto another node SWAPS with it
    /// ([`Graph::set_swap_on_drop`]); off, it is walked to the nearest free
    /// cell, as it always was.
    swap_on_drop: bool,
    /// The node the in-flight drag would swap with — the one standing on
    /// the cell the ghost is over — by id, as `splice_target` is held.
    swap_target: Option<String>,
    /// A completed swap drop for the host: (dragged node id, the other
    /// node's id). The two have traded cells already; the host trades
    /// whatever else it keeps — wires, for the designer.
    pending_swap: Option<(String, String)>,
    /// The host's choice of wire style; `None` follows the config.
    wire_style: Option<WireStyle>,
}

impl Graph {
    pub fn new() -> Adapted<Graph> {
        crate::layout::lazy_init_style_registry();

        let pitch_x = crate::layout::graph_spacing_x();
        let pitch_y = crate::layout::graph_spacing_y();
        let node_w = crate::layout::graph_node_width();
        let node_h = crate::layout::graph_node_height();
        let grid_snap_enabled = crate::layout::graph_grid_snap();

        let grid_col = crate::color::graph_grid_color();

        Adapted::new(Graph {
            show_network_grid: false,
            show_toggles: true,
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
            network_opacity: crate::color::graph_opacity(),
            node_opacity: crate::color::graph_node_opacity(),
            grid_color: grid_col,
            connecting_from: None,
            current_mouse_pos: (0.0, 0.0),
            pending_connection: None,
            hovered_port: None,
            splice_target: None,
            pending_splice: None,
            swap_on_drop: false,
            swap_target: None,
            pending_swap: None,
            wire_style: None,
        })
    }

    pub fn set_network_opacity(&mut self, opacity: f32) {
        self.network_opacity = opacity;
    }
    /// Show or hide every node's geometry toggle — the disc at a node's
    /// right end, and its hit target with it.
    pub fn set_show_toggles(&mut self, show: bool) {
        self.show_toggles = show;
    }
    pub fn set_node_opacity(&mut self, opacity: f32) {
        self.node_opacity = opacity;
    }
    pub fn set_grid_color(&mut self, color: [f32; 3]) {
        self.grid_color = color;
    }

    /// Choose the wire style for this graph; `None` goes back to the
    /// config's `wire_style`.
    pub fn set_wire_style(&mut self, style: Option<WireStyle>) {
        self.wire_style = style;
    }

    /// Whether a node dropped onto another node swaps places with it: the
    /// dragged node takes the other's cell and the other takes the dragged
    /// node's, and the host is told ([`GraphController::take_pending_swap`])
    /// so it can trade whatever else the two own. Off by default — a drop
    /// on an occupied cell walks to the nearest free one — so a host opts
    /// in. While the ghost is over a node that node is the swap target:
    /// coloured as the dragged node is, the drop target's cell, and it wins
    /// over a wire the ghost also touches (a node's own wires run into its
    /// body, so the two meet whenever a ghost is over a node).
    pub fn set_swap_on_drop(&mut self, on: bool) {
        self.swap_on_drop = on;
        if !on {
            self.swap_target = None;
        }
    }
}

impl Layout for Graph {}
