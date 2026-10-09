//! The lattice and what stands on it: pitch and node size, cell origins, node and port
//! geometry, the toggles, the nearest free cell, and zoom.

use super::*;

impl Graph {
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

    /// The top-left corner of a node body centred on lattice cell (col, row).
    pub(super) fn cell_origin(&self, col: f32, row: f32) -> (f32, f32) {
        (
            self.grid_origin_x + col * self.pitch_x - self.node_w * 0.5,
            self.grid_origin_y + row * self.pitch_y - self.node_h * 0.5,
        )
    }

    /// The lattice cell whose intersection is nearest the CENTRE of a node
    /// body whose top-left is (nx, ny) — the one snapping rule, shared by the
    /// drag preview, the drop-target highlight and the drop itself. None on a
    /// degenerate pitch.
    pub(super) fn nearest_cell(&self, nx: f32, ny: f32) -> Option<(f32, f32)> {
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
        // A swap lands ON the other node's cell; anything else walks off
        // an occupied one.
        let (c, r) = if self.swap_target_idx().is_some() { (c, r) } else { self.find_empty_cell(c, r, Some(idx)) };
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
    pub(super) fn port_offset(scale_f: f32) -> f32 {
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
        if !self.show_toggles {
            return None;
        }
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

    pub(super) fn find_empty_cell(&self, start_x: f32, start_y: f32, skip_idx: Option<usize>) -> (f32, f32) {
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
    pub(super) fn scale_by(&mut self, factor: f32) {
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
        if (40.0..=400.0).contains(&new_w) {
            self.scale_by(factor);
        }
    }
}
