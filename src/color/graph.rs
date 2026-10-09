//! The node graph's colours and their slots: nodes, wires, connectors, and the grid.

use super::*;

pub(super) static NODE_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.NODE_COLOR, |s| &mut s.color.NODE_COLOR);

pub(super) static NODE_SELECTED_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.NODE_SELECTED_COLOR, |s| &mut s.color.NODE_SELECTED_COLOR);

pub(super) static NODE_DRAG_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.NODE_DRAG_COLOR, |s| &mut s.color.NODE_DRAG_COLOR);

pub(super) static GRAPH_GRID_COLOR: crate::style::StyleCell<[f32; 3]> = crate::style::StyleCell::new(|s| &s.color.GRAPH_GRID_COLOR, |s| &mut s.color.GRAPH_GRID_COLOR);

pub(super) static GRAPH_OPACITY: crate::style::StyleCell<f32> = crate::style::StyleCell::new(|s| &s.color.GRAPH_OPACITY, |s| &mut s.color.GRAPH_OPACITY);

/// Opacity of the graph's NODE-domain content (node bodies, wires, connectors,
/// node text) — `style.surface.graph.node.opacity`, deliberately independent of
/// `GRAPH_OPACITY`, which fades only the pane surface (grid cells/gaps).
pub(super) static GRAPH_NODE_OPACITY: crate::style::StyleCell<f32> = crate::style::StyleCell::new(|s| &s.color.GRAPH_NODE_OPACITY, |s| &mut s.color.GRAPH_NODE_OPACITY);

pub(super) static GRAPH_NODE_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.GRAPH_NODE_COLOR, |s| &mut s.color.GRAPH_NODE_COLOR);

pub(super) static GRAPH_NODE_SELECTED_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.GRAPH_NODE_SELECTED_COLOR, |s| &mut s.color.GRAPH_NODE_SELECTED_COLOR);

pub(super) static GRAPH_NODE_DRAG_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.GRAPH_NODE_DRAG_COLOR, |s| &mut s.color.GRAPH_NODE_DRAG_COLOR);

pub(super) static GRAPH_WIRE_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.GRAPH_WIRE_COLOR, |s| &mut s.color.GRAPH_WIRE_COLOR);

pub(super) static GRAPH_WIRE_HIGHLIGHT_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.GRAPH_WIRE_HIGHLIGHT_COLOR, |s| &mut s.color.GRAPH_WIRE_HIGHLIGHT_COLOR);

pub(super) static GRAPH_CONNECTOR_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.GRAPH_CONNECTOR_COLOR, |s| &mut s.color.GRAPH_CONNECTOR_COLOR);

pub(super) static GRAPH_CONNECTOR_HIGHLIGHT_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.GRAPH_CONNECTOR_HIGHLIGHT_COLOR, |s| &mut s.color.GRAPH_CONNECTOR_HIGHLIGHT_COLOR);

pub fn node_color() -> [f32; 4] {
    load_colors_once();
    style_read(&NODE_COLOR)
}

pub fn set_node_color(color: [f32; 4]) {
    style_write(&NODE_COLOR, color);
}

pub fn node_selected_color() -> [f32; 4] {
    load_colors_once();
    style_read(&NODE_SELECTED_COLOR)
}

pub fn set_node_selected_color(color: [f32; 4]) {
    style_write(&NODE_SELECTED_COLOR, color);
}

pub fn node_drag_color() -> [f32; 4] {
    load_colors_once();
    style_read(&NODE_DRAG_COLOR)
}

pub fn set_node_drag_color(color: [f32; 4]) {
    style_write(&NODE_DRAG_COLOR, color);
}

pub fn graph_wire_color() -> [f32; 4] {
    load_colors_once();
    style_read(&GRAPH_WIRE_COLOR)
}

pub fn set_graph_wire_color(color: [f32; 4]) {
    style_write(&GRAPH_WIRE_COLOR, color);
}

pub fn graph_wire_highlight_color() -> [f32; 4] {
    load_colors_once();
    style_read(&GRAPH_WIRE_HIGHLIGHT_COLOR)
}

pub fn set_graph_wire_highlight_color(color: [f32; 4]) {
    style_write(&GRAPH_WIRE_HIGHLIGHT_COLOR, color);
}

pub fn graph_connector_color() -> [f32; 4] {
    load_colors_once();
    style_read(&GRAPH_CONNECTOR_COLOR)
}

pub fn set_graph_connector_color(color: [f32; 4]) {
    style_write(&GRAPH_CONNECTOR_COLOR, color);
}

pub fn graph_connector_highlight_color() -> [f32; 4] {
    load_colors_once();
    style_read(&GRAPH_CONNECTOR_HIGHLIGHT_COLOR)
}

pub fn set_graph_connector_highlight_color(color: [f32; 4]) {
    style_write(&GRAPH_CONNECTOR_HIGHLIGHT_COLOR, color);
}

pub fn graph_node_color() -> [f32; 4] {
    load_colors_once();
    style_read(&GRAPH_NODE_COLOR)
}

pub fn set_graph_node_color(color: [f32; 4]) {
    style_write(&GRAPH_NODE_COLOR, color);
}

pub fn graph_node_selected_color() -> [f32; 4] {
    load_colors_once();
    style_read(&GRAPH_NODE_SELECTED_COLOR)
}

pub fn set_graph_node_selected_color(color: [f32; 4]) {
    style_write(&GRAPH_NODE_SELECTED_COLOR, color);
}

pub fn graph_node_drag_color() -> [f32; 4] {
    load_colors_once();
    style_read(&GRAPH_NODE_DRAG_COLOR)
}

pub fn set_graph_node_drag_color(color: [f32; 4]) {
    style_write(&GRAPH_NODE_DRAG_COLOR, color);
}

/// The colour of a graph's grid lines (`style.surface.graph.grid_color`).
pub fn graph_grid_color() -> [f32; 3] {
    load_colors_once();
    style_read(&GRAPH_GRID_COLOR)
}

pub fn set_graph_grid_color(color: [f32; 3]) {
    style_write(&GRAPH_GRID_COLOR, color);
}

pub fn graph_opacity() -> f32 {
    load_colors_once();
    style_read(&GRAPH_OPACITY)
}

pub fn graph_node_opacity() -> f32 {
    load_colors_once();
    style_read(&GRAPH_NODE_OPACITY)
}

pub fn set_graph_node_opacity(opacity: f32) {
    style_write(&GRAPH_NODE_OPACITY, opacity);
}

pub fn set_graph_opacity(opacity: f32) {
    style_write(&GRAPH_OPACITY, opacity);
}
