//! Wires: the four styles, a wire's path between two ports (drawn and hit alike), its stroke
//! and turn, painting them, and finding the wire a dragged node splices onto.

use super::*;

/// How a wire runs from an output port (the bottom of its node) to an input
/// port (the top of the next): `style.surface.graph.node.wire_style` in
/// config.kdl, by [`WireStyle::name`], unless a host sets one of its own
/// ([`Graph::set_wire_style`]). Every style is drawn and hit-tested from the
/// one path `wire_path` derives, so a splice drop lands on the wire you see.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WireStyle {
    /// Down, across at half the height, down: three straight runs meeting
    /// square. What every wire was before there was a choice.
    #[default]
    Orthogonal,
    /// The same three runs with their two bends rounded off.
    Rounded,
    /// One cubic curve that leaves the output heading down and arrives at
    /// the input heading down, so a wire running back up the graph loops.
    Bezier,
    /// One straight line, port to port.
    Straight,
}

impl WireStyle {
    pub const ALL: [WireStyle; 4] = [WireStyle::Orthogonal, WireStyle::Rounded, WireStyle::Bezier, WireStyle::Straight];

    /// The config spelling.
    pub fn name(self) -> &'static str {
        match self {
            WireStyle::Orthogonal => "orthogonal",
            WireStyle::Rounded => "rounded",
            WireStyle::Bezier => "bezier",
            WireStyle::Straight => "straight",
        }
    }

    /// The spelling a menu shows.
    pub fn label(self) -> &'static str {
        match self {
            WireStyle::Orthogonal => "Orthogonal",
            WireStyle::Rounded => "Rounded",
            WireStyle::Bezier => "Bezier",
            WireStyle::Straight => "Straight",
        }
    }

    /// Either spelling, any case.
    pub fn parse(s: &str) -> Option<WireStyle> {
        let s = s.trim();
        WireStyle::ALL.into_iter().find(|w| w.name().eq_ignore_ascii_case(s))
    }

    /// The configured style; orthogonal when the key is absent or names
    /// no style.
    pub fn configured() -> WireStyle {
        crate::layout::graph_wire_style().as_deref().and_then(WireStyle::parse).unwrap_or_default()
    }
}

/// One piece of a wire's path, in window px: a straight run, or an arc about
/// a centre at a CENTRELINE radius from one angle to another (radians,
/// screen space, so y runs down).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum WireSeg {
    Line((f32, f32), (f32, f32)),
    Arc { c: (f32, f32), r: f32, a0: f32, a1: f32 },
}

/// One DEVICE pixel in logical px.
pub(super) fn device_px() -> f32 {
    1.0 / crate::scale::scale_factor().max(1.0)
}

/// A wire of `size` logical px as it is drawn: (stroke width, alpha). The
/// thinnest stroke is one device pixel (`px`), because the 2D pass has no
/// antialiasing — an axis-aligned quad narrower than a pixel covers a row of
/// pixel centres or none, and the wire would come and go as it moved. A
/// thinner wire is drawn as that pixel at the share of it the wire would
/// cover, so it reads thinner by reading fainter.
pub(super) fn wire_stroke(size: f32, px: f32) -> (f32, f32) {
    let px = px.max(f32::EPSILON);
    (size.max(px), (size / px).clamp(0.0, 1.0))
}

/// The path of a wire from `start` to `end` drawn `t` px thick. `bend` is
/// the largest radius a Rounded bend takes and the least a Bezier lead
/// runs straight down before curving — both scale with the node, so the
/// shape keeps its proportions under zoom. `turn` is the height of an
/// Orthogonal or Rounded wire's run across ([`Graph::wire_turn_y`]); `None`
/// is halfway between the two ends.
pub(super) fn wire_path(style: WireStyle, start: (f32, f32), end: (f32, f32), t: f32, bend: f32, turn: Option<f32>) -> Vec<WireSeg> {
    let ((sx, sy), (ex, ey)) = (start, end);
    let my = turn.unwrap_or(sy + (ey - sy) / 2.0);
    let orthogonal = || {
        // Square joins with no overlap, so a translucent wire is one alpha
        // throughout: the across run is widened by half a thickness at each
        // end to fill the corners, and the down runs stop at its edge.
        let s = if ey >= sy { 1.0 } else { -1.0 };
        let mut out = Vec::with_capacity(3);
        let v1_end = my - s * t / 2.0;
        if s * (v1_end - sy) > 0.0 {
            out.push(WireSeg::Line((sx, sy), (sx, v1_end)));
        }
        out.push(WireSeg::Line((sx.min(ex) - t / 2.0, my), (sx.max(ex) + t / 2.0, my)));
        let v2_start = my + s * t / 2.0;
        if s * (ey - v2_start) > 0.0 {
            out.push(WireSeg::Line((ex, v2_start), (ex, ey)));
        }
        out
    };
    match style {
        WireStyle::Straight => vec![WireSeg::Line(start, end)],
        WireStyle::Orthogonal => orthogonal(),
        WireStyle::Rounded => {
            let (dx, dy) = (ex - sx, ey - sy);
            if dx.abs() < 0.5 {
                return vec![WireSeg::Line(start, end)];
            }
            // The bends fit the legs they turn between: the run across
            // need not be halfway down.
            let r = bend.min(dx.abs() / 2.0).min((my - sy).abs()).min((ey - my).abs());
            // A bend tighter than half the stroke has no inside edge.
            if r < t / 2.0 {
                return orthogonal();
            }
            // Bend at (bx, by) turning from heading u1 to heading u2.
            let bend_at = |bx: f32, by: f32, u1: (f32, f32), u2: (f32, f32)| {
                let t1 = (bx - u1.0 * r, by - u1.1 * r);
                let t2 = (bx + u2.0 * r, by + u2.1 * r);
                let c = (t1.0 + u2.0 * r, t1.1 + u2.1 * r);
                let a0 = (t1.1 - c.1).atan2(t1.0 - c.0);
                let a1 = (t2.1 - c.1).atan2(t2.0 - c.0);
                let mut sweep = a1 - a0;
                if sweep > std::f32::consts::PI {
                    sweep -= std::f32::consts::TAU;
                } else if sweep < -std::f32::consts::PI {
                    sweep += std::f32::consts::TAU;
                }
                (t1, t2, WireSeg::Arc { c, r, a0, a1: a0 + sweep })
            };
            let down = (0.0, dy.signum());
            let across = (dx.signum(), 0.0);
            let (p1a, p1b, arc1) = bend_at(sx, my, down, across);
            let (p2a, p2b, arc2) = bend_at(ex, my, across, down);
            vec![
                WireSeg::Line(start, p1a),
                arc1,
                WireSeg::Line(p1b, p2a),
                arc2,
                WireSeg::Line(p2b, end),
            ]
        }
        WireStyle::Bezier => {
            let lead = ((ey - sy).abs() * 0.5).max(bend);
            let (c1, c2) = ((sx, sy + lead), (ex, ey - lead));
            // Fine enough that the flat-capped pieces meet without a visible
            // notch: about one piece per 6 px of the control net.
            let net = lead * 2.0 + ((c2.0 - c1.0).powi(2) + (c2.1 - c1.1).powi(2)).sqrt();
            let n = ((net / 6.0).ceil() as usize).clamp(8, 96);
            let at = |u: f32| {
                let v = 1.0 - u;
                let (a, b, c, d) = (v * v * v, 3.0 * v * v * u, 3.0 * v * u * u, u * u * u);
                (a * sx + b * c1.0 + c * c2.0 + d * ex, a * sy + b * c1.1 + c * c2.1 + d * ey)
            };
            let mut prev = start;
            (1..=n)
                .map(|i| {
                    let p = at(i as f32 / n as f32);
                    let seg = WireSeg::Line(prev, p);
                    prev = p;
                    seg
                })
                .collect()
        }
    }
}

/// Whether the segment a→b passes through the rect (x1, y1)-(x2, y2):
/// Liang–Barsky clipping of the segment to the rect.
pub(super) fn segment_meets_rect(a: (f32, f32), b: (f32, f32), x1: f32, y1: f32, x2: f32, y2: f32) -> bool {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for (p, q) in [(-dx, a.0 - x1), (dx, x2 - a.0), (-dy, a.1 - y1), (dy, y2 - a.1)] {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                lo = lo.max(r);
            } else {
                hi = hi.min(r);
            }
            if lo > hi {
                return false;
            }
        }
    }
    true
}

impl Graph {

    /// The wire style in effect: the host's, else the config's.
    pub fn wire_style(&self) -> WireStyle {
        self.wire_style.unwrap_or_else(WireStyle::configured)
    }

    /// The host's own choice of wire style, if it made one.
    pub fn chosen_wire_style(&self) -> Option<WireStyle> {
        self.wire_style
    }

    /// A wire's width as asked for: `graph_wire_size` px at 100%, scaled
    /// with the node body as the zoom scales it, and at most half a body.
    pub(super) fn wire_size_at_zoom(&self) -> f32 {
        let base = crate::layout::graph_node_width();
        let zoom = if base > 0.0 { self.node_w / base } else { 1.0 };
        (crate::layout::graph_wire_size() * zoom).clamp(0.0, (self.node_h * 0.5).max(1.0))
    }

    /// A wire's stroke as drawn (see [`wire_stroke`]).
    pub(super) fn wire_thickness(&self) -> f32 {
        wire_stroke(self.wire_size_at_zoom(), device_px()).0
    }

    /// The alpha a wire's colour is drawn at (see [`wire_stroke`]).
    pub(super) fn wire_fade(&self) -> f32 {
        wire_stroke(self.wire_size_at_zoom(), device_px()).1
    }

    /// What a Rounded bend's radius and a Bezier's straight lead are made of.
    pub(super) fn wire_bend(&self) -> f32 {
        self.node_h * 0.5
    }

    /// The wire from `src_idx`'s output to `dest_idx`'s input, as drawn.
    pub(super) fn wire_segments(&self, src_idx: usize, dest_idx: usize, port: usize) -> Option<Vec<WireSeg>> {
        let (start, end) = self.wire_endpoints(src_idx, dest_idx, port)?;
        let turn = self.wire_turn_y(start, end);
        Some(wire_path(self.wire_style(), start, end, self.wire_thickness(), self.wire_bend(), turn))
    }

    /// Where a wire running DOWN turns across: on the first lattice line
    /// below its source — the line the row under the source stands on — so
    /// it leaves the source's column at once. Halfway down, which is where
    /// every wire used to turn, a wire spanning several rows ran straight
    /// down through whatever node stood under its source on the way: a wire
    /// from row -1 to row 3 turned on row 1's line, through the node there.
    /// `None` (halfway) where that line is not between the two ends with
    /// room for the bends — the nodes are in adjacent rows, and the turn
    /// belongs between the bodies — and for a wire running up, whose first
    /// line past its source is the source's own.
    pub(super) fn wire_turn_y(&self, start: (f32, f32), end: (f32, f32)) -> Option<f32> {
        let (sy, ey) = (start.1, end.1);
        if self.pitch_y <= 0.0 || ey <= sy {
            return None;
        }
        let line = self.grid_origin_y + (((sy - self.grid_origin_y) / self.pitch_y).floor() + 1.0) * self.pitch_y;
        let room = self.wire_thickness().max(1.0);
        (line - sy >= room && ey - line >= room).then_some(line)
    }

    /// The wires, and the one being dragged out of a port, in the style in
    /// effect ([`Self::wire_style`]), `wire_color` at the node opacity and
    /// `wire_size` thick; the wire an in-flight node drag would splice into
    /// is in `wire_highlight_color` — the drop affordance. Clipped to `rect`.
    /// Hosts that draw the graph's quads themselves call it between
    /// [`Self::paint_grid`] and the node bodies.
    pub fn paint_wires(&self, rect: Rect, pc: &mut PaintCtx) {
        let t = self.wire_thickness();
        let stroke = |segs: &[WireSeg], color: [f32; 4], pc: &mut PaintCtx| {
            for seg in segs {
                match *seg {
                    WireSeg::Line(a, b) => pc.vector(a.0, a.1, b.0, b.1, t, color, crate::scene::paint::Cap::Flat),
                    // The arc's radius is its OUTER edge; the path's is the
                    // centreline.
                    WireSeg::Arc { c, r, a0, a1 } => pc.arc(c.0, c.1, r + t / 2.0, t, a0, a1, color),
                }
            }
        };
        let fade = self.node_opacity * self.wire_fade();
        let wc = crate::color::graph_wire_color();
        let wire_color = [wc[0], wc[1], wc[2], wc[3] * fade];
        let hl = crate::color::graph_wire_highlight_color();
        let splice_color = [hl[0], hl[1], hl[2], hl[3] * fade];
        pc.clip(rect, |pc| {
            for (src_idx, i, port) in self.wire_pairs() {
                let Some(segs) = self.wire_segments(src_idx, i, port) else { continue };
                let is_splice_target = self
                    .splice_target
                    .as_ref()
                    .is_some_and(|(s, d)| self.nodes[src_idx].id == *s && self.nodes[i].id == *d);
                stroke(&segs, if is_splice_target { splice_color } else { wire_color }, pc);
            }
            // The connection being dragged out of a port.
            if let Some((node_idx, port_type, port_idx)) = self.connecting_from {
                if let Some(start) = self.port_center(node_idx, port_type, port_idx) {
                    let segs = wire_path(self.wire_style(), start, self.current_mouse_pos, t, self.wire_bend(), None);
                    stroke(&segs, [1.0, 0.6, 0.0, 0.8], pc); // Golden orange preview
                }
            }
        });
    }

    /// The wires the draw pass renders: (src idx, dest idx, input port) —
    /// the ONE derivation, shared with the splice hit test so the two cannot
    /// disagree about where a wire is.
    ///
    /// A node's wires are its parameters of type `node`, in order, the k-th
    /// into input port k: every one a host marks so, not just the first
    /// (the designer's Switch reads four, a Boolean two). A node with none
    /// so marked has its parameter NAMED `input` as its one wire, into port
    /// 0 — what every host passed before the type said it (cce-files,
    /// cce-graph).
    pub(super) fn wire_pairs(&self) -> Vec<(usize, usize, usize)> {
        let mut out = Vec::new();
        for i in 0..self.nodes.len() {
            for (port, source) in node_wires(&self.nodes[i]).into_iter().enumerate() {
                if source.is_empty() {
                    continue;
                }
                if let Some(src_idx) = self.nodes.iter().position(|n| n.name == source) {
                    out.push((src_idx, i, port));
                }
            }
        }
        out
    }

    /// A wire's two attachment points — the port circles' centers, falling
    /// back to the node edge midpoints for portless nodes. The three-segment
    /// shape (down, across, down) derives from these in both the draw pass
    /// and [`Self::wire_segment_rects`].
    pub(super) fn wire_endpoints(&self, src_idx: usize, dest_idx: usize, port: usize) -> Option<((f32, f32), (f32, f32))> {
        let (sx, sy, sw, sh) = self.node_rect(src_idx)?;
        let (ex, ey, ew, _eh) = self.node_rect(dest_idx)?;
        let start = self
            .port_center(src_idx, PortType::Output, 0)
            .unwrap_or((sx + sw / 2.0, sy + sh));
        // Into its own port; a wire past the node's ports (a host that
        // declared fewer) lands on the first, and a portless node's edge.
        let end = self
            .port_center(dest_idx, PortType::Input, port)
            .or_else(|| self.port_center(dest_idx, PortType::Input, 0))
            .unwrap_or((ex + ew / 2.0, ey));
        Some((start, end))
    }

    /// The wire the dragged node's ghost at (nx, ny) would splice into —
    /// the first pair (draw order) whose path, as drawn, touches the ghost rect,
    /// inflated by the wire activation radius so a near miss still takes.
    /// The dragged node's own wires never count (dropping a node on a wire
    /// it is already an end of is a move, not a rewire), and a node with no
    /// "Input" parameter or no output port cannot sit mid-chain. Only a
    /// wire into port 0 — the Input — is spliced into, since the splice
    /// rewires the Inputs.
    pub(super) fn splice_wire_at(&self, idx: usize, nx: f32, ny: f32) -> Option<(usize, usize)> {
        let node = self.nodes.get(idx)?;
        let has_input = node
            .parameters
            .iter()
            .any(|(name, _, _)| name.eq_ignore_ascii_case("input"));
        if !has_input || node.outputs == 0 {
            return None;
        }
        let (_, _, nw, nh) = self.node_rect(idx)?;
        self.input_wire_meeting(nx, ny, nw, nh, Some(idx))
    }

    /// The wire into an Input (port 0) that runs through the body a node
    /// would have at lattice cell (`col`, `row`), as (upstream id,
    /// downstream id) — by the hit test a dragged node's splice drop uses,
    /// so a host placing a NEW node there (the designer's Add Node at its
    /// grid cursor) wires it in exactly where a drop would have. Whether
    /// the cell is free is the host's to ask; a node's own wires touch it.
    pub fn input_wire_through_cell(&self, col: f32, row: f32) -> Option<(String, String)> {
        let (x, y) = self.cell_origin(col, row);
        let (src, dest) = self.input_wire_meeting(x, y, self.node_w, self.node_h, None)?;
        Some((self.nodes[src].id.clone(), self.nodes[dest].id.clone()))
    }

    /// The first wire into a port 0 (draw order) whose path, as drawn,
    /// touches the body rect at (x, y), inflated by the wire activation
    /// radius so a near miss still takes. `skip`'s own wires never count.
    pub(super) fn input_wire_meeting(&self, x: f32, y: f32, w: f32, h: f32, skip: Option<usize>) -> Option<(usize, usize)> {
        // Half the stroke on top of the activation radius, so the wire's
        // edge counts and not only its centreline.
        let pad = crate::layout::graph_wire_activation_radius().max(0.0) + self.wire_thickness() / 2.0;
        let (gx1, gy1) = (x - pad, y - pad);
        let (gx2, gy2) = (x + w + pad, y + h + pad);
        for (src, dest, port) in self.wire_pairs() {
            if Some(src) == skip || Some(dest) == skip || port != 0 {
                continue;
            }
            let Some(segs) = self.wire_segments(src, dest, port) else { continue };
            let hit = segs.iter().any(|seg| match *seg {
                WireSeg::Line(a, b) => segment_meets_rect(a, b, gx1, gy1, gx2, gy2),
                // An arc, as the chords of its eighths.
                WireSeg::Arc { c, r, a0, a1 } => (0..8).any(|k| {
                    let at = |u: f32| {
                        let a = a0 + (a1 - a0) * u;
                        (c.0 + r * a.cos(), c.1 + r * a.sin())
                    };
                    segment_meets_rect(at(k as f32 / 8.0), at((k + 1) as f32 / 8.0), gx1, gy1, gx2, gy2)
                }),
            });
            if hit {
                return Some((src, dest));
            }
        }
        None
    }
}
