//! `impl GraphController`: what a host reads and drives.

use super::*;

impl GraphController for Graph {
    fn paint_grid(&self, rect: Rect, pc: &mut PaintCtx) {
        Graph::paint_grid(self, rect, pc)
    }
    fn paint_wires(&self, rect: Rect, pc: &mut PaintCtx) {
        Graph::paint_wires(self, rect, pc)
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
                self.swap_target = None;
            }
        } else {
            self.dragging_idx = None;
            self.drag_node_pos = None;
            self.splice_target = None;
            self.swap_target = None;
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
        self.pending_connection.take().map(|(id, name, _)| (id, name))
    }
    fn take_pending_connection_to_port(&mut self) -> Option<(String, String, usize)> {
        self.pending_connection.take()
    }
    fn take_pending_swap(&mut self) -> Option<(String, String)> {
        self.pending_swap.take()
    }
    fn take_pending_splice(&mut self) -> Option<(String, String, String)> {
        self.pending_splice.take()
    }
    fn input_wire_through_cell(&self, col: f32, row: f32) -> Option<(String, String)> {
        Graph::input_wire_through_cell(self, col, row)
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
