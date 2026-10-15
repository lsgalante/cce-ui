//! What the float ramp draws: its well and plot, the curve, the keys, the dropdowns, the key pad
//! and the delete button.

use super::*;

/// The axis numbers' size: small enough for the gutters the plot inset leaves.
const AXIS_FONT_PX: f32 = 9.0;

impl Ramp {
    /// The curve's colour, its under-fill and the key pegs' tint: the DE accent (`highlight
    /// primary`) — what the user acts on in the graph.
    fn accent() -> [f32; 3] {
        let a = crate::color::highlight_primary_color();
        [a[0], a[1], a[2]]
    }

    /// A peg's accent tint, unselected: light, so the curve and grid read through the glass.
    const PEG_TINT: f32 = 0.25;
    /// The selected peg's tint: strong, the one key the pad below edits.
    const PEG_TINT_SELECTED: f32 = 0.85;
    /// The flat selection ring's width, at the peg's outer edge.
    const PEG_RING_W: f32 = 1.5;
    /// The flat face's inset from the ring: the floor shows between them.
    const PEG_RING_GAP: f32 = 1.5;

    /// A flat peg's face radius, inside the selection ring's gap.
    fn flat_face_r(&self) -> f32 {
        let r = self.key_peg_r();
        (r - Self::PEG_RING_W - Self::PEG_RING_GAP).max(r * 0.5)
    }

    /// The radius key `idx`'s peg is drawn to, which [`key_at`](Self::key_at) hits: the
    /// whole disc under relief; flat, the selected peg's ring, an unselected peg's face.
    pub(super) fn peg_drawn_r(&self, idx: usize) -> f32 {
        if crate::layout::control_relief() || Some(idx) == self.selected_key_idx {
            self.key_peg_r()
        } else {
            self.flat_face_r()
        }
    }

    /// The key pegs, in [`peg_order`](Self::peg_order) — the selected one last, on top.
    ///
    /// Under relief each is a disc plate ([`PaintCtx::plate_shaped`] at `Some(2.0)`, a true
    /// circle whatever the DE's corner shape) of the accent as FROSTED glass: a frosted plate
    /// composites opaque (the blurred floor tinted at the tint's alpha) with its roll at full
    /// strength, where an unfrosted one at a light tint would show its roll at that alpha
    /// and nearly vanish. Overlapping pegs stack: the one above shows the one below blurred.
    /// The roll is a control plate's (the relief width, at most 0.4 of the radius), lit by
    /// the DE light like every plate. Flat, each is a flat accent disc, and the selected one
    /// wears the focus ring at its outer edge, the face drawn inside the ring's gap.
    fn paint_pegs(&self, pc: &mut PaintCtx) {
        let accent = Self::accent();
        let r = self.key_peg_r();
        let relief = crate::layout::control_relief();
        let frost = crate::scene::Frost::from_style();
        let depth = crate::layout::bevel_width().min(0.4 * r);
        for idx in self.peg_order() {
            let (cx, cy) = self.key_centre(idx);
            let selected = Some(idx) == self.selected_key_idx;
            let alpha = if selected { Self::PEG_TINT_SELECTED } else { Self::PEG_TINT };
            let tint = [accent[0], accent[1], accent[2], alpha];
            if relief {
                let disc = Rect { x: cx - r, y: cy - r, width: 2.0 * r, height: 2.0 * r };
                let glass = crate::scene::Material::opaque(tint).with_frost(frost);
                pc.plate_shaped(disc, (r, r, r, r), &glass, depth, Some(2.0));
            } else {
                pc.circle(cx, cy, self.flat_face_r(), tint);
                if selected {
                    let ring = crate::scene::paint::ControlPlate::focus_tint();
                    pc.arc(
                        cx,
                        cy,
                        r,
                        Self::PEG_RING_W,
                        0.0,
                        std::f32::consts::TAU,
                        [ring[0], ring[1], ring[2], 1.0],
                    );
                }
            }
        }
    }

    /// The grid lines: the detached label colour, faint — the axis numbers' colour, so the
    /// grid and its numbers read as one scale.
    fn grid_color() -> [f32; 4] {
        let c = crate::color::control_label_color_detached();
        [c[0], c[1], c[2], 0.25]
    }
}

impl Paint for Ramp {
    /// No container box: the ramp draws its graph and fields itself, on the host's plate.
    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn popover(&self, _rect: Rect) -> Option<(f32, f32, f32, f32)> {
        self.preset_dropdown.popover_rect()
            .or_else(|| self.line_type_dropdown.popover_rect())
    
    }

    fn draw_popover(&self, _rect: Rect, pc: &mut PaintCtx) {
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
        // the graph is a canvas well cut through it, the one every canvas shares
        // (Slider2D, Trackpad, the previews): its floor here (`well_floor`, frost
        // and all), its rim after the content (`well_rim`) so the wall's shading
        // falls across the curve and keys at the edges. At the panel radius.
        let graph = {
            let gh = self.graph_h();
            Rect { x: self.base.x + 10.0, y: self.base.y + 10.0, width: self.base.w - 20.0, height: gh }
        };
        let graph_radius = crate::layout::panel_corner_radius();
        pc.well_floor(graph, graph_radius, &crate::scene::Material::pane(), false);
        let accent = Self::accent();

        let quads: Vec<(f32, f32, f32, f32, [f32; 4])> = {
        let mut quads = Vec::new();
        let plot = self.plot_rect();

        // Grid lines over the plotted 0..1 domain — 0 and 1 included, sitting
        // inside the opening (the plot is inset from the walls).
        let grid = Self::grid_color();
        for ratio in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let gy = plot.y + plot.height * (1.0 - ratio);
            quads.push((plot.x, gy, plot.width, 1.0, grid));
            let gx = plot.x + plot.width * ratio;
            quads.push((gx, plot.y, 1.0, plot.height, grid));
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
            // Faint on purpose: the graph reads as an opening behind the
            // plate — a strong fill floods the floor and flattens the depth.
            // (The shared floor is the plate darkened, lighter than the near
            // black the ramp painted: 0.10 of the accent read as a solid block.)
            quads.push((x0, sy, x1 - x0, slice_h, [accent[0], accent[1], accent[2], 0.05]));
        }

        quads

        };
        for (qx, qy, qw, qh, qc) in quads {
            pc.quad(Rect { x: qx, y: qy, width: qw, height: qh }, qc);
        }

        // Axis numbers on the gridlines — small, dim, inside the opening, in
        // the wall-side gutters the plot inset leaves free. Text is drawn after
        // all geometry, so they read over the curve and any peg in the gutter.
        let plot = self.plot_rect();
        let num_color = crate::color::control_label_color_detached_u8();
        let (num_family, _) = crate::layout::control_label_font_detached_parsed();
        for ratio in [0.0f32, 0.25, 0.5, 0.75, 1.0] {
            let gy = plot.y + plot.height * (1.0 - ratio);
            pc.text_with(
                format!("{ratio:.2}"),
                graph.x + 5.0,
                gy - 11.0,
                AXIS_FONT_PX,
                num_color,
                Some(format!("{num_family} {AXIS_FONT_PX}")),
                None,
            );
            let gx = plot.x + plot.width * ratio;
            pc.text_with(
                format!("{ratio:.2}"),
                gx - 11.0,
                graph.y + graph.height - 13.0,
                AXIS_FONT_PX,
                num_color,
                Some(format!("{num_family} {AXIS_FONT_PX}")),
                None,
            );
        }

        // The curve itself: one anti-aliased round-capped polyline — exact
        // key-to-key segments in linear mode, dense samples of the smooth line
        // (the monotone cubic, `sample_ramp_keys`). Constant-value extensions
        // reach the plot's 0/1 edges.
        let curve_color = [accent[0], accent[1], accent[2], 1.0];
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
        self.paint_pegs(pc);
        // The well's rim, after the content: the recess under relief, the frame
        // hairline flat (it carved a recess either way, and wore three shadow
        // rings no other well has).
        pc.well_rim(graph, graph_radius, crate::layout::control_relief());
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
