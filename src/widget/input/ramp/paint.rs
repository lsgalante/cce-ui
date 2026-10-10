//! What the float ramp draws: its well and plot, the curve, the keys, the dropdowns, the key pad
//! and the delete button.

use super::*;

impl Paint for Ramp {
    fn color(&self) -> [f32; 4] {
        [0.15, 0.15, 0.18, 1.0]
    }

    fn popover(&self, _rect: Rect) -> Option<(f32, f32, f32, f32)> {
        self.preset_dropdown.popover_rect()
            .or_else(|| self.line_type_dropdown.popover_rect())
    
    }

    fn draw_popover(&self, _rect: Rect, pc: &mut dyn crate::scene::paint::RenderTarget) {
        self.preset_dropdown.render_popover(pc);
        self.line_type_dropdown.render_popover(pc);
    
    }

    // Field children are ctx-linked for event propagation but painted here — the walk
    // must not also descend (the legacy own-labels rule, now with the children too).
    fn paints_own_subtree(&self) -> bool {
        true
    }

    fn paint(&self, _rect: Rect, pc: &mut PaintCtx) {
        // No container box: the controls sit directly on the host's plate, and
        // the graph area reads as an OPENING cut through it — a dark floor
        // behind the plate, with the recess wall (drawn after the content, so
        // its shading falls across the graph's edges) as the cut's bevel.
        let graph = {
            let gh = self.graph_h();
            Rect { x: self.base.x + 10.0, y: self.base.y + 10.0, width: self.base.w - 20.0, height: gh }
        };
        let graph_radius = 6.0f32;
        pc.rounded_rect(
            graph,
            graph_radius,
            (true, true, true, true),
            [0.08, 0.08, 0.10, 1.0],
        );

        let quads: Vec<(f32, f32, f32, f32, [f32; 4])> = {
        let mut quads = Vec::new();
        let plot = self.plot_rect();

        // Grid lines over the plotted 0..1 domain — 0 and 1 included, sitting
        // inside the opening (the plot is inset from the walls).
        for ratio in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let gy = plot.y + plot.height * (1.0 - ratio);
            quads.push((plot.x, gy, plot.width, 1.0, [0.25, 0.25, 0.28, 0.5]));
            let gx = plot.x + plot.width * ratio;
            quads.push((gx, plot.y, 1.0, plot.height, [0.25, 0.25, 0.28, 0.5]));
        }

        // Curve area fill: translucent columns under the curve. The outline is
        // a real vector polyline below — these only tint the area. Columns
        // share exact edges (overlap double-blends a translucent fill into
        // visible banding; found the hard way).
        let slices = 200;
        for i in 0..slices {
            let t1 = i as f32 / slices as f32;
            let x0 = plot.x + t1 * plot.width;
            let x1 = plot.x + (i + 1) as f32 / slices as f32 * plot.width;
            let v1 = self.get_interpolated_value(t1);

            let slice_h = v1 * plot.height;
            let sy = plot.y + plot.height - slice_h;
            // Faint on purpose: the graph reads as a dark opening behind the
            // plate — a strong fill floods the floor and flattens the depth.
            quads.push((x0, sy, x1 - x0, slice_h, [0.25, 0.40, 0.55, 0.10]));
        }

        quads

        };
        for (qx, qy, qw, qh, qc) in quads {
            pc.quad(Rect { x: qx, y: qy, width: qw, height: qh }, qc);
        }

        // Axis numbers on the gridlines — small, dim, part of the graph
        // floor (under the curve and keys, inside the opening). They sit in
        // the wall-side gutters the plot inset leaves free.
        let plot = self.plot_rect();
        let num_color = [0x84u8, 0x84, 0x92];
        for ratio in [0.0f32, 0.25, 0.5, 0.75, 1.0] {
            let gy = plot.y + plot.height * (1.0 - ratio);
            pc.text_with(
                format!("{ratio:.2}"),
                graph.x + 5.0,
                gy - 11.0,
                9.0,
                num_color,
                Some("monospace".to_string()),
                None,
            );
            let gx = plot.x + plot.width * ratio;
            pc.text_with(
                format!("{ratio:.2}"),
                gx - 11.0,
                graph.y + graph.height - 13.0,
                9.0,
                num_color,
                Some("monospace".to_string()),
                None,
            );
        }

        // The curve itself: one anti-aliased round-capped polyline — exact
        // key-to-key segments in linear mode, dense samples under smoothstep
        // blending. Constant-value extensions reach the plot's 0/1 edges.
        let curve_color = [0.5, 0.75, 1.0, 1.0];
        let px_of = |t: f32, v: f32| {
            (plot.x + t * plot.width, plot.y + plot.height * (1.0 - v))
        };
        let mut pts: Vec<(f32, f32)> = Vec::new();
        if self.line_type_dropdown.selected == 1 {
            let n = 64;
            for i in 0..=n {
                let t = i as f32 / n as f32;
                pts.push(px_of(t, self.get_interpolated_value(t)));
            }
        } else {
            if let Some(first) = self.keys.first() {
                if first.pos > 0.0 {
                    pts.push(px_of(0.0, first.value));
                }
            }
            for k in &self.keys {
                pts.push(px_of(k.pos, k.value));
            }
            if let Some(last) = self.keys.last() {
                if last.pos < 1.0 {
                    pts.push(px_of(1.0, last.value));
                }
            }
        }
        for pair in pts.windows(2) {
            pc.vector(pair[0].0, pair[0].1, pair[1].0, pair[1].1, 2.0, curve_color, Cap::Round);
        }
        // Key pegs: glassy translucent fills (solid when selected) in thin
        // white rings. Overlapping pegs render as foam cells: each pair's
        // shared wall is the chord through the two points where the ring
        // circles cross (equal radii, so it lies on the perpendicular
        // bisector of the centers); rings are cut at the wall, the wall is
        // stroked once, and each fill keeps to its own side.
        {
            let plot = self.plot_rect();
            let ring_r = self.key_ring_r(); // roll-band centerline
            // The disc surface: flat top out to the roll band's inner edge,
            // then the rolled perimeter out to ring_r + 2.5. Band and rim
            // shrink with the ring so a small peg keeps a flat top.
            let base = [0.5f32, 0.75, 1.0];
            let fill_r = (ring_r - 3.0).max(ring_r * 0.5);
            let rim_t = 6.0f32.min(ring_r * 0.25).max(1.0);
            // Bevel light: the DE light azimuth the plate shading uses.
            let az = crate::layout::light_source_position();
            let tau = std::f32::consts::TAU;

            let centers: Vec<(f32, f32)> = self
                .keys
                .iter()
                .map(|k| (plot.x + k.pos * plot.width, plot.y + plot.height * (1.0 - k.value)))
                .collect();

            // Every intersecting pair: wall midpoint M + unit normal n toward
            // the neighbor per key, and the chord endpoints once per pair.
            let mut cuts: Vec<Vec<((f32, f32), (f32, f32))>> = vec![Vec::new(); centers.len()];
            let mut walls: Vec<((f32, f32), (f32, f32), (f32, f32))> = Vec::new();
            for i in 0..centers.len() {
                for j in (i + 1)..centers.len() {
                    let (dx, dy) = (centers[j].0 - centers[i].0, centers[j].1 - centers[i].1);
                    let d = (dx * dx + dy * dy).sqrt();
                    if d < 1e-3 || d >= 2.0 * ring_r {
                        continue;
                    }
                    let n = (dx / d, dy / d);
                    let m =
                        ((centers[i].0 + centers[j].0) / 2.0, (centers[i].1 + centers[j].1) / 2.0);
                    cuts[i].push((m, n));
                    cuts[j].push((m, (-n.0, -n.1)));
                    let h = (ring_r * ring_r - (d / 2.0) * (d / 2.0)).sqrt();
                    walls.push((
                        (m.0 - h * n.1, m.1 + h * n.0),
                        (m.0 + h * n.1, m.1 - h * n.0),
                        n,
                    ));
                }
            }

            // Fills. Uncut: one disc. Cut: the cell — vertical strips bounded
            // by the wall half-planes, the round edge from the circle clip.
            for (idx, &(cx, cy)) in centers.iter().enumerate() {
                let selected = Some(idx) == self.selected_key_idx;
                let fill = [base[0], base[1], base[2], if selected { 0.85 } else { 0.22 }];
                if cuts[idx].is_empty() {
                    pc.circle(cx, cy, fill_r, fill);
                    continue;
                }
                pc.push_clip_circle([cx, cy, fill_r]);
                let step = 1.5f32;
                let mut x = cx - fill_r;
                while x < cx + fill_r {
                    let mid = x + step / 2.0;
                    let (mut ylo, mut yhi) = (cy - fill_r, cy + fill_r);
                    let mut visible = true;
                    for &((mx, my), (nx, ny)) in &cuts[idx] {
                        // Keep (p − M)·n ≤ 0 — this key's side of the wall.
                        let c = nx * (mid - mx);
                        if ny.abs() < 1e-4 {
                            if c > 0.0 {
                                visible = false;
                                break;
                            }
                        } else {
                            let yb = my - c / ny;
                            if ny > 0.0 {
                                yhi = yhi.min(yb);
                            } else {
                                ylo = ylo.max(yb);
                            }
                        }
                    }
                    if visible && ylo < yhi {
                        pc.quad(Rect { x, y: ylo, width: step, height: yhi - ylo }, fill);
                    }
                    x += step;
                }
                pc.pop_clip_circle();
            }

            // Walls: the shared boundary as the surface rolling into the
            // seam and back out — surface-tinted slopes (lit side leans to
            // the light, far side into shadow) around a slightly lifted
            // crest, in the discs\' own color like the rims.
            let (lx, ly) = (az.cos(), -az.sin());
            let wall_tint = |sv: f32, k: f32| -> [f32; 3] {
                [
                    (base[0] + k * sv).clamp(0.0, 1.0),
                    (base[1] + k * sv).clamp(0.0, 1.0),
                    (base[2] + k * sv).clamp(0.0, 1.0),
                ]
            };
            for &((x1, y1), (x2, y2), (nx, ny)) in &walls {
                let facing = nx * lx + ny * ly;
                let cp = wall_tint(facing, 0.38);
                let cm = wall_tint(-facing, 0.38);
                let cc = wall_tint(facing, 0.12);
                pc.vector(
                    x1 + nx * 1.6, y1 + ny * 1.6, x2 + nx * 1.6, y2 + ny * 1.6,
                    1.6, [cp[0], cp[1], cp[2], 0.78], Cap::Round,
                );
                pc.vector(
                    x1 - nx * 1.6, y1 - ny * 1.6, x2 - nx * 1.6, y2 - ny * 1.6,
                    1.6, [cm[0], cm[1], cm[2], 0.78], Cap::Round,
                );
                pc.vector(x1, y1, x2, y2, 1.8, [cc[0], cc[1], cc[2], 0.85], Cap::Round);
            }

            // Rims: beveled circles minus the angular span facing each wall
            // (no drawn border — the shaded edge IS the ring).
            for (idx, &(cx, cy)) in centers.iter().enumerate() {
                let top_a = if Some(idx) == self.selected_key_idx { 0.85 } else { 0.22 };
                if cuts[idx].is_empty() {
                    Self::rolled_rim_arc(pc, cx, cy, ring_r + 2.5, rim_t, 0.0, tau, az, base, top_a);
                    continue;
                }
                // Excluded spans [θ−α, θ+α] toward each neighbor, normalized
                // into [0, τ) (wrapping spans split), then merged.
                let mut segs: Vec<(f32, f32)> = Vec::new();
                for &((mx, my), (nx, ny)) in &cuts[idx] {
                    let theta = ny.atan2(nx);
                    let half = (mx - cx) * nx + (my - cy) * ny;
                    let alpha = (half / ring_r).clamp(-1.0, 1.0).acos();
                    let (a, b) = ((theta - alpha).rem_euclid(tau), (theta + alpha).rem_euclid(tau));
                    if a <= b {
                        segs.push((a, b));
                    } else {
                        segs.push((a, tau));
                        segs.push((0.0, b));
                    }
                }
                segs.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap());
                let mut merged: Vec<(f32, f32)> = Vec::new();
                for s in segs {
                    match merged.last_mut() {
                        Some(last) if s.0 <= last.1 => last.1 = last.1.max(s.1),
                        _ => merged.push(s),
                    }
                }
                // Stroke the complement (the two pieces meeting at θ=0 join
                // seamlessly when no span covers 0).
                let mut prev = 0.0f32;
                for &(a, b) in &merged {
                    if a > prev + 1e-3 {
                        Self::rolled_rim_arc(pc, cx, cy, ring_r + 2.5, rim_t, prev, a, az, base, top_a);
                    }
                    prev = prev.max(b);
                }
                if prev < tau - 1e-3 {
                    Self::rolled_rim_arc(pc, cx, cy, ring_r + 2.5, rim_t, prev, tau, az, base, top_a);
                }
            }
        }
        // The opening's cut edge: drawn after the graph content so the wall's
        // shading falls across the curve and keys where they pass behind the
        // plate's rim. Nested translucent border rings first — the contact
        // shadow the plate casts down into the opening — then the recess wall
        // itself as the cut's bevel.
        let radii = (graph_radius, graph_radius, graph_radius, graph_radius);
        for (t, a) in [(7.0, 0.08), (4.0, 0.10), (2.0, 0.14)] {
            pc.border(graph, radii, [0.0; 4], [0.0, 0.0, 0.0, a], t);
        }
        let depth = crate::layout::bevel_width().min(graph.height * 0.2);
        let (well, radii) = crate::layout::carve_inside(graph, radii, depth);
        pc.recess(well, radii, depth);
        if !self.controls_collapsed {
            let dummy = UiContext::new();
            self.preset_dropdown.paint_self(&dummy, pc);
            self.line_type_dropdown.paint_self(&dummy, pc);
            if self.selected_key_idx.is_some() {
                self.key_pad.paint_self(&dummy, pc);
                self.del_button.paint_self(&dummy, pc);
            }
        }
    }
}
