//! Input: `impl Input`, the left press, committing a drag (with the swap or splice it lands on),
//! and the zoom key bindings.

use super::*;

pub(super) fn read_zoom_bindings() -> (String, String) {
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
                let ctrl = ectx.ui.as_deref().is_some_and(|ui| ui.ctrl_pressed);
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

            // Snapping centres the body on the nearest intersection a drop
            // could LAND on: an empty one, or another node's when a drop
            // there swaps the two (`set_swap_on_drop`); otherwise the
            // nearest free one, which is where `commit_drag` would walk it
            // — so the body never sits where it cannot stay. It snapped to
            // the nearest crossing whatever stood there until 2026-10-06,
            // and showed a node over another until the release moved it.
            let (nx, ny) = match self.nearest_cell(nx, ny) {
                Some((c, r)) if self.grid_snap_enabled => {
                    let dragged = self.dragging_idx.unwrap_or(usize::MAX);
                    let (c, r) = if self.swap_candidate(dragged, nx, ny).is_some() {
                        (c, r)
                    } else {
                        self.find_empty_cell(c, r, self.dragging_idx)
                    };
                    self.cell_origin(c, r)
                }
                _ => (nx, ny),
            };

            self.drag_node_pos = Some((nx, ny));
            // A node under the ghost is a swap, which wins over a wire.
            self.swap_target = self
                .dragging_idx
                .and_then(|i| self.swap_candidate(i, nx, ny))
                .map(|o| self.nodes[o].id.clone());
            // The wire the ghost sits on right now, held by id (hosts
            // re-sync between events) and drawn highlighted — the drop
            // affordance the user aims by.
            self.splice_target = if self.swap_target.is_some() {
                None
            } else {
                self.dragging_idx
                    .and_then(|i| self.splice_wire_at(i, nx, ny))
                    .map(|(s, d)| (self.nodes[s].id.clone(), self.nodes[d].id.clone()))
            };
            return true;
        }
        false
    }

    fn drag_end(&mut self) {
        self.commit_drag();
    }

}

impl Graph {

    /// The node the in-flight drag would swap with, by index.
    pub fn swap_target_idx(&self) -> Option<usize> {
        let id = self.swap_target.as_ref()?;
        self.nodes.iter().position(|n| &n.id == id)
    }

    /// The node standing on the cell the dragged node's ghost at (nx, ny)
    /// is nearest — the one a drop there would swap with.
    pub(super) fn swap_candidate(&self, dragged: usize, nx: f32, ny: f32) -> Option<usize> {
        if !self.swap_on_drop {
            return None;
        }
        let (c, r) = self.nearest_cell(nx, ny)?;
        self.nodes
            .iter()
            .enumerate()
            .find(|(i, n)| *i != dragged && n.position.0.round() == c && n.position.1.round() == r)
            .map(|(i, _)| i)
    }
    /// The legacy left-press cascade: ports (start/complete a connection), a node-body
    /// fallback for an in-flight connection, geometry toggles, then node selection + drag
    /// arming; an empty-space press clears the selection and stays unconsumed.
    pub(super) fn on_left_press(&mut self, px: f32, py: f32, ectx: &mut EventCtx) -> bool {
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
                            if let Some((src_idx, src_port_type, src_port_idx)) = self.connecting_from {
                                // A click on the opposite port kind of ANOTHER
                                // node completes the connection; anything else
                                // cancels it.
                                if src_idx != i && src_port_type != port_type {
                                    let (out_idx, in_idx, in_port) = if port_type == PortType::Input {
                                        (src_idx, i, k)
                                    } else {
                                        (i, src_idx, src_port_idx)
                                    };
                                    let output_node = &self.nodes[out_idx];
                                    let input_node = &self.nodes[in_idx];
                                    self.pending_connection =
                                        Some((input_node.id.clone(), output_node.name.clone(), in_port));
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
        if let Some((src_idx, src_port_type, src_port_idx)) = self.connecting_from {
            for i in (0..self.nodes.len()).rev() {
                if src_idx != i {
                    if let Some((nx, ny, nw, nh)) = self.node_rect(i) {
                        if px >= nx && px < nx + nw && py >= ny && py < ny + nh {
                            let node = &self.nodes[i];
                            if src_port_type == PortType::Output && node.inputs > 0 {
                                let output_node = &self.nodes[src_idx];
                                let input_node = &self.nodes[i];
                                // A body, not a port: its first input.
                                self.pending_connection = Some((input_node.id.clone(), output_node.name.clone(), 0));
                                self.connecting_from = None;
                                return true;
                            } else if src_port_type == PortType::Input && node.outputs > 0 {
                                let output_node = &self.nodes[i];
                                let input_node = &self.nodes[src_idx];
                                self.pending_connection = Some((input_node.id.clone(), output_node.name.clone(), src_port_idx));
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
                    let now = web_time::Instant::now();
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

    /// Drop the in-flight node drag onto the nearest free intersection (legacy `drag_end`),
    /// resolving a held splice target into `pending_splice` for the host.
    pub(super) fn commit_drag(&mut self) {
        let target = self.splice_target.take();
        let swap = self.swap_target_idx();
        self.swap_target = None;
        if let Some((nx, ny)) = self.drag_node_pos.take() {
            if let Some(idx) = self.dragging_idx.take() {
                // A swap: the two trade cells — the dragged node's own
                // position is still the cell it was picked up from.
                if let Some(other) = swap.filter(|&o| o != idx) {
                    let from = self.nodes[idx].position;
                    self.nodes[idx].position = self.nodes[other].position;
                    self.nodes[other].position = from;
                    self.pending_swap = Some((self.nodes[idx].id.clone(), self.nodes[other].id.clone()));
                    self.dragging_id = None;
                    return;
                }
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
