//! The graph widget's settings: spacing, node size and radius, wires and connectors, grid snap.

use super::*;

/// The graph grid's pitch along x: the distance from the centre of one
/// vertical grid line to the centre of the next. It is the grid's ONE size
/// per axis — nodes are centred on the lattice intersections. The node body
/// has a size of its own (`graph_node_width` / `graph_node_height`), so a
/// denser grid does not shrink the nodes. The pitch replaced a cell size
/// plus a gap (`spacing_*` was the cell, `gap_col_w` / `gap_row_h` the gap,
/// and a step was the two added up); the defaults are what those two used
/// to add up to, so a config that set neither draws the same lattice it did.
pub fn graph_spacing_x() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("graph_spacing_x").unwrap_or(187.5)
}

pub fn set_graph_spacing_x(spacing: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("graph_spacing_x", spacing);
    }
}

/// The graph grid's pitch along y — see [`graph_spacing_x`].
pub fn graph_spacing_y() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("graph_spacing_y").unwrap_or(112.5)
}

pub fn set_graph_spacing_y(spacing: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("graph_spacing_y", spacing);
    }
}

/// The drawn width of a graph grid line, in logical px. The pitch is
/// measured centre to centre, so this changes how heavy the lattice looks
/// and nothing about where anything sits. Not scaled by zoom — a lattice is
/// a reference, not a thing in the scene.
pub fn graph_line_width() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("graph_line_width").unwrap_or(1.0)
}

pub fn set_graph_line_width(width: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("graph_line_width", width);
    }
}

/// The node body's width at 100% zoom (`style.surface.graph.node.width`),
/// independent of the grid pitch: a node is a thing of its own size sitting
/// on a crossing, and the grid is a reference under it. The default is the
/// cell the old grid gave a node.
pub fn graph_node_width() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("graph_node_width").unwrap_or(150.0)
}

pub fn set_graph_node_width(width: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("graph_node_width", width);
    }
}

/// The node body's height at 100% zoom — see [`graph_node_width`].
pub fn graph_node_height() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("graph_node_height").unwrap_or(75.0)
}

pub fn set_graph_node_height(height: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("graph_node_height", height);
    }
}

pub fn graph_grid_snap() -> bool {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("graph_grid_snap").unwrap_or(0.0) != 0.0
}

pub fn set_graph_grid_snap(snap: bool) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("graph_grid_snap", if snap { 1.0 } else { 0.0 });
    }
}

pub fn graph_blur() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("graph_blur").unwrap_or(0.0)
}

pub fn set_graph_blur(blur: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("graph_blur", blur);
    }
}

pub fn graph_node_corner_radius() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("graph_node_corner_radius").unwrap_or(4.0)
}

pub fn set_graph_node_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("graph_node_corner_radius", radius);
    }
}

pub fn graph_node_delete() -> String {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_string("graph_node_delete").unwrap_or_else(|| "delete".to_string())
}

pub fn set_graph_node_delete(key: String) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_string("graph_node_delete", key);
    }
}

/// `style.surface.graph.node.wire_style`: how a wire runs, by
/// `WireStyle::name` — `orthogonal`, `rounded`, `bezier` or `straight`.
/// `None` when the config does not say.
pub fn graph_wire_style() -> Option<String> {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_string("graph_wire_style")
}

pub fn graph_wire_size() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("graph_wire_size").unwrap_or(6.0)
}

pub fn set_graph_wire_size(size: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("graph_wire_size", size);
    }
}

pub fn graph_wire_activation_radius() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("graph_wire_activation_radius").unwrap_or(9.0)
}

pub fn set_graph_wire_activation_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("graph_wire_activation_radius", radius);
    }
}

pub fn graph_connector_size() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("graph_connector_size").unwrap_or(8.0)
}

pub fn set_graph_connector_size(size: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("graph_connector_size", size);
    }
}

pub fn graph_connector_activation_radius() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("graph_connector_activation_radius").unwrap_or(12.0)
}

pub fn set_graph_connector_activation_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("graph_connector_activation_radius", radius);
    }
}
