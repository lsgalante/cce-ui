//! What the graph draws: the background and grid, node geometry (tagged quads and rounded),
//! port circles, node labels, and `impl Paint`.

use super::*;

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
        // The background is the first entry; the grid lines and then the
        // wires go over it before the nodes.
        for (i, (qx, qy, qw, qh, r, c, corners)) in self.rounded_geometry(rect).into_iter().enumerate() {
            ctx.rounded_rect(Rect { x: qx, y: qy, width: qw, height: qh }, r, corners, c);
            if i == 0 {
                self.paint_grid(rect, ctx);
                self.paint_wires(rect, ctx);
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

    fn text_bounds(&self, rect: Rect) -> Option<[f32; 4]> {
        Some([rect.x, rect.y, rect.x + rect.width, rect.y + rect.height])
    }
}

impl Graph {

    /// The graph's plate, as the colour-typed host paints it: a
    /// near-transparent black, so the cells between the grid lines are
    /// whatever the graph is painted on, and under `graph_blur` a frosted
    /// material whose tint alpha IS the blur value — the knob doubles as
    /// the frost's opacity. Until 2026-09-29 a graph could fill itself
    /// with a cell colour of its own (`graph.cell_color`, chosen by
    /// `uniform_background`); both went together.
    pub(super) fn bg_color(&self) -> [f32; 4] {
        use crate::scene::{Frost, Material, PlateRole};
        let c = [0.0, 0.0, 0.0, 0.01 * self.network_opacity];
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
        // show_network_grid silently squared those
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
    /// pane plate. A lattice of lines one pitch apart in the grid colour at
    /// the network opacity, each centred on its coordinate (the pitch is
    /// measured centre to centre, and `graph_line_width` only thickens
    /// them), so the intersections are exactly where the node centres go.
    /// Plus the origin axes: the two lines through the (0, 0) intersection,
    /// 2px, in the axis colour. Gated on the grid's visibility only.
    /// (Rounded cells with grout between them came before
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
            let color = [self.grid_color[0], self.grid_color[1], self.grid_color[2], self.network_opacity];
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

    /// The node bodies / toggles as plain quads — the legacy
    /// `extra_quads` body, against `rect` instead of a stored rect. The [`TaggedQuad`] cell
    /// tag is always `None` now: the lattice is `paint_grid`'s, and it has no cells.
    pub fn geometry_quads_tagged(&self, rect: Rect) -> Vec<TaggedQuad> {
        let mut quads = Vec::new();
        let min_x = rect.x;
        let min_y = rect.y;
        let max_x = rect.x + rect.width;
        let max_y = rect.y + rect.height;
        // The wires and the connection preview are `paint_wires`' — strokes,
        // not quads, since only one of the styles is axis-aligned.

        // The grid lines and the origin axes are `paint_grid`'s — hosts that
        // draw these quads themselves call it at the same point in their walk.

        // Node bodies (culled, not clipped — legacy) + geometry toggles (clipped)
        for i in 0..self.nodes.len() {
            if let Some((nx, ny, nw, nh)) = self.node_rect(i) {
                let scale_f = nw / 80.0;
                let mut bg_color = if self.dragging_idx == Some(i) || self.swap_target_idx() == Some(i) {
                    color::node_drag_color()
                } else if self.selected_idx == Some(i) {
                    color::node_selected_color()
                } else {
                    color::node_color()
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

    /// The rounded view of the same geometry — the legacy `all_rounded_quads` conversion: the
    /// widget background, then each plain quad either as a grid cell (superellipse cell arcs),
    /// a node body (node corner radius, all corners), or with the widget's edge-corner
    /// resolution.
    pub(super) fn rounded_geometry(&self, rect: Rect) -> Vec<(f32, f32, f32, f32, f32, [f32; 4], (bool, bool, bool, bool))> {
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
    pub(super) fn port_circles(&self, rect: Rect) -> Vec<(f32, f32, f32, [f32; 4])> {
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
                let mut c = color::TOGGLE_ON;
                if !self.nodes[i].geom_visible {
                    c[3] *= 0.25;
                }
                c[3] *= self.node_opacity;
                push_circle_clipped(cx, cy, r, c);
            }
        }

        let conn_size = crate::layout::graph_connector_size();
        let mut conn_color = color::graph_connector_color();
        let mut conn_hl_color = color::graph_connector_highlight_color();
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
    pub(super) fn node_labels(&self, rect: Rect) -> Vec<TextLabel> {
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
