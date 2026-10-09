//! Carves: recesses, bosses, ridges and troughs (a full-ring untinted one grouped into its
//! host plate as a CSG feature when it can be, else an overlay), the concave fillet, the
//! groove, the lattice and its grout, and the carve union.

use super::*;

impl Tess {
    /// A carve prim. A carve grouped into its host plate adds a feature and emits no vertices
    /// (false); so does a degenerate groove or an empty union.
    pub(super) fn carves(&mut self, it: &mut Item) -> bool {
        match &it.item.prim {
            Prim::Recess { .. } | Prim::Boss { .. } | Prim::Ridge { .. } | Prim::Trough { .. } if self.shader_plates => {
                // Grouped into the enclosing plate whenever one is live: the carve becomes a
                // CSG feature of that plate's single draw — exact composite shading, real
                // junctions at the plate's rolled perimeter — instead of a shading overlay.
                let c = Carve::of(&it.item.prim);
                let host = self.carve_host(&c);
                #[cfg(debug_assertions)]
                if host.is_none() && c.groupable() {
                    self.warn_near_roll_fallback(&c);
                }
                if self.dbg.on {
                    self.note_carve_verdict(&c, host);
                }
                if let Some(bi) = host {
                    self.group_carve(bi, &c);
                    return false;
                }
                it.plate = Some(self.overlay_carve(&c));
            }
            Prim::Recess { rect, radii, depth, edges, .. } => {
                // Edges only — no fill: the shading is an overlay, so whatever is painted
                // below (fill, rim gradient, blur) shows through the carve modulated
                // rather than repainted. `light_sign = -1.0` shadows the lit-facing edges,
                // which is the raised->recessed inversion.
                push_bevel_edge_vertices_banded(
                    rect.x, rect.y, rect.width, rect.height, *radii, *depth,
                    self.sw, self.sh, [0.0; 4], it.circle, -1.0, default_bevel_bands(*depth), *edges,
                    EdgeKind::Step, &mut self.verts,
                );
            }
            Prim::Boss { rect, radii, depth, edges, .. } => {
                // Legacy raised step: the recess overlay with the light sign upright.
                push_bevel_edge_vertices_banded(
                    rect.x, rect.y, rect.width, rect.height, *radii, *depth,
                    self.sw, self.sh, [0.0; 4], it.circle, 1.0, default_bevel_bands(*depth), *edges,
                    EdgeKind::Step, &mut self.verts,
                );
            }
            Prim::Ridge { rect, radii, depth, edges } => {
                // Legacy approximation: a raised step up at the boundary plus a
                // recessed step down half a width in (the banded machinery has no
                // bump profile; the double-pass hot crest is accepted here — the
                // legacy path exists only for A/B comparison).
                let half = *depth * 0.5;
                push_bevel_edge_vertices_banded(
                    rect.x, rect.y, rect.width, rect.height, *radii, half,
                    self.sw, self.sh, [0.0; 4], it.circle, 1.0, default_bevel_bands(half), *edges,
                    EdgeKind::Step, &mut self.verts,
                );
                let ir = (radii.0 - half).max(0.0);
                push_bevel_edge_vertices_banded(
                    rect.x + half, rect.y + half,
                    rect.width - *depth, rect.height - *depth,
                    (ir, ir, ir, ir), half,
                    self.sw, self.sh, [0.0; 4], it.circle, -1.0, default_bevel_bands(half), *edges,
                    EdgeKind::Step, &mut self.verts,
                );
            }
            Prim::Trough { rect, radii, depth, edges, .. } => {
                // Legacy approximation, the Ridge arm's two steps with the light
                // signs swapped: down at the boundary, back up half a width in.
                // The banded machinery has no valley profile, so this is the old
                // stacked look — accepted here, as the legacy path exists only
                // for A/B comparison against the SDF one.
                let half = *depth * 0.5;
                push_bevel_edge_vertices_banded(
                    rect.x, rect.y, rect.width, rect.height, *radii, half,
                    self.sw, self.sh, [0.0; 4], it.circle, -1.0, default_bevel_bands(half), *edges,
                    EdgeKind::Step, &mut self.verts,
                );
                let ir = (radii.0 - half).max(0.0);
                push_bevel_edge_vertices_banded(
                    rect.x + half, rect.y + half,
                    rect.width - *depth, rect.height - *depth,
                    (ir, ir, ir, ir), half,
                    self.sw, self.sh, [0.0; 4], it.circle, 1.0, default_bevel_bands(half), *edges,
                    EdgeKind::Step, &mut self.verts,
                );
            }
            Prim::ConcaveFillet { cx, cy, radius, depth, start: a0, raised } if self.shader_plates => {
                // A quarter-arc carve wall (shader mode 6/7): one cover quad
                // over the wedge's reach; the wall straddles the arc by ±t/2
                // like every carve boundary. p_rect carries centre + radius,
                // p_radii.x the wedge start angle. Host box pushed far out —
                // an inside-corner fillet never fades.
                let m = *depth * 0.5 + 2.0;
                let r = *radius + m;
                self.verts.extend(quad_vertices(cx - r, cy - r, 2.0 * r, 2.0 * r, self.sw, self.sh, [0.0; 4]));
                it.plate = Some(crate::draw::PlatePush {
                    rect: [cx * self.scale, cy * self.scale, *radius * self.scale, 0.0],
                    radii: [*a0, 0.0, 0.0, 0.0],
                    light: [self.light[0], self.light[1], self.light[2], *depth * self.scale],
                    material: self.finish,
                    host: [0.0, 0.0, 1e6, 1e6],
                    specular_tint: [1.0, 1.0, 1.0, 0.0],
                    mode: if *raised { 7.0 } else { 6.0 },
                    shape: crate::layout::corner_shape(),
                });
            }
            // Legacy banded path has no radial wall — the composed corner
            // stays square there (A/B comparison path only).
            Prim::ConcaveFillet { .. } => {}
            Prim::Groove { a, b, width, depth, host, strength } if self.shader_plates => {
                // A slab carve about the line a–b (shader mode 8): the cover
                // quad is the segment's bounding box grown by the groove's own
                // half-width plus the wall's reach. Off-band corners of that
                // box sit at u = 1 (plateau), so the box overhang shades
                // nothing — the slab is what bounds the mark, not the quad.
                let m = *width * 0.5 + *depth * 0.5 + 2.0;
                let (x0, x1) = (a.0.min(b.0) - m, a.0.max(b.0) + m);
                let (y0, y1) = (a.1.min(b.1) - m, a.1.max(b.1) + m);
                self.verts.extend(quad_vertices(x0, y0, x1 - x0, y1 - y0, self.sw, self.sh, [0.0; 4]));
                // Unit normal of the line — the direction the slab's distance is
                // measured along. A degenerate segment falls back to vertical so
                // a zero-length groove is a no-op wall rather than a NaN.
                let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                let len = (dx * dx + dy * dy).sqrt();
                let n = if len > 1e-4 { (-dy / len, dx / len) } else { (1.0, 0.0) };
                it.plate = Some(crate::draw::PlatePush {
                    // Centre + slab half-width in physical px; .w unused.
                    rect: [
                        (a.0 + b.0) * 0.5 * self.scale,
                        (a.1 + b.1) * 0.5 * self.scale,
                        *width * 0.5 * self.scale,
                        0.0,
                    ],
                    radii: [n.0, n.1, 0.0, 0.0],
                    light: [self.light[0], self.light[1], self.light[2], *depth * self.scale],
                    // The finish's shading, specular and AO, at the groove's
                    // strength; shininess is a shape, not an amount.
                    material: {
                        let s = strength.clamp(0.0, 1.0);
                        [self.finish[0] * s, self.finish[1] * s, self.finish[2], self.finish[3] * s]
                    },
                    host: [
                        (host.x + host.width * 0.5) * self.scale,
                        (host.y + host.height * 0.5) * self.scale,
                        host.width * 0.5 * self.scale,
                        host.height * 0.5 * self.scale,
                    ],
                    specular_tint: [1.0, 1.0, 1.0, 0.0],
                    mode: 8.0,
                    shape: crate::layout::corner_shape(),
                });
            }
            Prim::Groove { a, b, width, depth, host: _, strength } => {
                // Legacy approximation. The banded tessellators walk BOX edges —
                // exactly the axis-aligned assumption a groove exists to escape —
                // so the walls are drawn directly as two feathered lines meeting
                // at the centerline: the engraved-line fake, one half in shadow
                // and one lit. Coarser than the SDF (no profile curve, no host
                // fade), but this path exists for A/B comparison, and drawing
                // NOTHING would silently delete the mark rather than degrade it
                // — see `Prim::Ridge` above, which accepts a hot crest for the
                // same reason.
                let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                let len = (dx * dx + dy * dy).sqrt();
                if len < 0.001 {
                    return false;
                }
                let n = (-dy / len, dx / len);
                // Same convention as `push_bevel_edge_vertices_banded`: the
                // light folded through `light_sign` (-1.0 — a groove is a
                // carve), dotted with each wall's OUTWARD normal, amplitude on
                // `bevel_depth`. So a groove re-lights with the DE's light
                // instead of hardcoding which side is dark.
                let rad = crate::layout::light_source_position();
                let (lx, ly) = (-rad.cos(), rad.sin());
                let v = crate::layout::bevel_depth() * (n.0 * lx + n.1 * ly);
                // Each wall covers its own half, centreline to outer edge —
                // abutting rather than overlapping. The SDF gets away with
                // walls that overlap across a sub-pixel floor because it is one
                // evaluation of |distance|; two opposite-signed overlays would
                // just blend to mud.
                let half = (*width * 0.5 + *depth * 0.5).max(0.5);
                for side in [1.0f32, -1.0] {
                    let sv = v * side;
                    let mut c = if sv >= 0.0 { overlay_light(sv) } else { overlay_dark(sv) };
                    c[3] *= strength.clamp(0.0, 1.0);
                    if c[3] <= 0.0 {
                        continue;
                    }
                    let off = side * half * 0.5;
                    push_feathered_line_vertices(
                        a.0 + n.0 * off, a.1 + n.1 * off,
                        b.0 + n.0 * off, b.1 + n.1 * off,
                        half, self.sw, self.sh, c, &mut self.verts,
                    );
                }
            }
            Prim::Lattice { rect, period, origin, cell, radius, depth } if self.shader_plates => {
                // A periodic well field (shader mode 13): one cover quad over
                // `rect`; the shader folds each pixel into the period and
                // measures the nearest cell, so the whole lattice is a single
                // evaluation. p_rect = one cell's centre + half-extents,
                // p_radii = the corner radius, p_host.xy = the period; the
                // host-box fade sides are pushed far out (a lattice never
                // fades against a host — its own rect bounds it).
                let (pw, ph) = (period.0.max(1e-3), period.1.max(1e-3));
                self.verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, self.sw, self.sh, [0.0; 4]));
                it.plate = Some(crate::draw::PlatePush {
                    rect: [origin.0 * self.scale, origin.1 * self.scale, cell.0 * 0.5 * self.scale, cell.1 * 0.5 * self.scale],
                    radii: [*radius * self.scale; 4],
                    light: [self.light[0], self.light[1], self.light[2], *depth * self.scale],
                    material: self.finish,
                    host: [pw * self.scale, ph * self.scale, 1e6, 1e6],
                    specular_tint: [1.0, 1.0, 1.0, 0.0],
                    mode: 13.0,
                    shape: crate::layout::corner_shape(),
                });
            }
            Prim::Grout { rect, period, origin, cell, radius, color } if self.shader_plates => {
                // The lattice's fold, painted flat (shader mode 15): one cover
                // quad in the grout colour; the shader keeps it outside the
                // cells. Same push layout as the lattice; light/material are
                // carried but unread.
                let (pw, ph) = (period.0.max(1e-3), period.1.max(1e-3));
                self.verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, self.sw, self.sh, *color));
                it.plate = Some(crate::draw::PlatePush {
                    rect: [origin.0 * self.scale, origin.1 * self.scale, cell.0 * 0.5 * self.scale, cell.1 * 0.5 * self.scale],
                    radii: [*radius * self.scale; 4],
                    light: [self.light[0], self.light[1], self.light[2], 0.0],
                    material: self.finish,
                    host: [pw * self.scale, ph * self.scale, 1e6, 1e6],
                    specular_tint: [1.0, 1.0, 1.0, 0.0],
                    mode: 15.0,
                    shape: crate::layout::corner_shape(),
                });
            }
            // Legacy banded path: no periodic wall — the lattice and the grout
            // draw nothing there, like the fillet (A/B comparison path only).
            Prim::Lattice { .. } | Prim::Grout { .. } => {}
            Prim::CarveUnion { boxes, depth, raised } if self.shader_plates => {
                // The union of several boxes as ONE wall (shader mode 14): the
                // boxes go into the frame's feature buffer as a contiguous run
                // and the shader takes the nearest one per pixel. The cover
                // quad is the union's bounding box grown by the wall's reach;
                // off-shape corners of it sit at the plateau and shade nothing.
                let budget = crate::draw::MAX_PLATE_FEATURES.saturating_sub(self.features.len());
                let take = boxes.len().min(budget);
                if take < boxes.len() && plate_debug() {
                    eprintln!(
                        "plate-carve: union of {} boxes gets {} — feature budget full ({} used)",
                        boxes.len(), take, self.features.len()
                    );
                }
                if take == 0 {
                    return false;
                }
                let kept = &boxes[..take];
                let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                for (r, _) in kept {
                    x0 = x0.min(r.x);
                    y0 = y0.min(r.y);
                    x1 = x1.max(r.x + r.width);
                    y1 = y1.max(r.y + r.height);
                }
                let infl = *depth * 0.5 + 2.0;
                self.verts.extend(quad_vertices(
                    x0 - infl, y0 - infl,
                    (x1 - x0) + 2.0 * infl, (y1 - y0) + 2.0 * infl,
                    self.sw, self.sh, [0.0; 4],
                ));
                let off = self.features.len() as f32;
                for (r, radii) in kept {
                    self.features.push([
                        (r.x + r.width * 0.5) * self.scale,
                        (r.y + r.height * 0.5) * self.scale,
                        r.width * 0.5 * self.scale,
                        r.height * 0.5 * self.scale,
                        radii.0 * self.scale,
                        radii.1 * self.scale,
                        radii.2 * self.scale,
                        radii.3 * self.scale,
                        *depth * self.scale,
                        0.0,
                        0.0,
                        0.0,
                    ]);
                }
                // The run is complete: a plate with an open feature run must
                // not append past it (its features would no longer be
                // contiguous), so it is closed here like any other appender.
                self.last_feature_plate = None;
                it.plate = Some(crate::draw::PlatePush {
                    rect: [
                        (x0 + x1) * 0.5 * self.scale,
                        (y0 + y1) * 0.5 * self.scale,
                        (x1 - x0) * 0.5 * self.scale,
                        (y1 - y0) * 0.5 * self.scale,
                    ],
                    // x: the raised flag; the shader reads nothing else here.
                    radii: [if *raised { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
                    light: [self.light[0], self.light[1], self.light[2], *depth * self.scale],
                    material: self.finish,
                    // Feature run [offset, count] (the renderer rebases the
                    // offset onto the frame slot, as for mode 1); zw far out
                    // so the host-box fade never applies.
                    host: [off, take as f32, 1e6, 1e6],
                    specular_tint: [1.0, 1.0, 1.0, 0.0],
                    mode: 14.0,
                    shape: crate::layout::corner_shape(),
                });
            }
            // Legacy banded path: no union — nothing is drawn there, like the
            // fillet and the lattice (A/B comparison path only).
            Prim::CarveUnion { .. } => {}
            _ => unreachable!("carves: not a carve prim"),
        }
        true
    }

    /// The open plate carve `c` groups into, if any: the innermost one that encloses it, that
    /// no later plate's pixels overlap within the carve's shaded region (its shading would
    /// land beneath them in this plate's earlier draw), and whose feature run is still open —
    /// a plate's features are one contiguous run, so only the last plate to receive one (or a
    /// plate with none yet) may take another. Only a carve that can group asks (see
    /// [`Carve::groupable`]), and only while the frame's feature budget lasts.
    fn carve_host(&self, c: &Carve) -> Option<usize> {
        if !c.groupable() || self.features.len() >= crate::draw::MAX_PLATE_FEATURES {
            return None;
        }
        let (sx0, sy0, sx1, sy1) = c.shaded();
        self.plate_stack
            .iter()
            .enumerate()
            .rev()
            .find(|(si, (bi, prect))| {
                if !c.within(prect) {
                    return false;
                }
                if self.plate_stack[si + 1..].iter().any(|(_, orect)| {
                    sx0 < orect.x + orect.width
                        && sx1 > orect.x
                        && sy0 < orect.y + orect.height
                        && sy1 > orect.y
                }) {
                    return false;
                }
                self.batches[*bi].plate.as_ref().is_some_and(|p| p.host[1] == 0.0)
                    || self.last_feature_plate == Some(*bi)
            })
            .map(|(_, &(bi, _))| bi)
    }

    /// Carve `c` as a CSG feature of plate batch `bi`, appended to that plate's feature run.
    /// A wall the carve shares with the plate's edge extends past the plate, so the carve has
    /// no wall there.
    fn group_carve(&mut self, bi: usize, c: &Carve) {
        let (x0, y0, x1, y1) = c.wall_box();
        let t_px = c.depth * self.scale;
        // The carve's drop: the material's pinned height, else the analytic ratio of the wall
        // saturating at the DE's roll width (`layout::carve_depth_px` states the rule once for
        // this path and the shader's free carves).
        let k_mag = crate::layout::carve_depth_px(c.depth) * self.scale;
        // Negative depth = raised (Boss); the shader's summed slope vectors and curvature sign
        // follow it.
        let k_px = if c.raised() { -k_mag } else { k_mag };
        if let Some(p) = self.batches[bi].plate.as_mut() {
            if p.host[1] == 0.0 {
                p.host[0] = self.features.len() as f32;
            }
            p.host[1] += 1.0;
        }
        self.last_feature_plate = Some(bi);
        self.features.push([
            (x0 + x1) * 0.5 * self.scale,
            (y0 + y1) * 0.5 * self.scale,
            (x1 - x0) * 0.5 * self.scale,
            (y1 - y0) * 0.5 * self.scale,
            c.radii.0 * self.scale,
            c.radii.1 * self.scale,
            c.radii.2 * self.scale,
            c.radii.3 * self.scale,
            t_px,
            k_px,
            0.0,
            0.0,
        ]);
    }

    /// Carve `c` as an overlay: a cover quad inflated by half the roll width (the step
    /// straddles the boundary) carrying no colour — the shader emits translucent white and
    /// black over what is beneath — and its push block. A suppressed wall is pushed past the
    /// cover quad, so its shading falls outside the drawn pixels (see `Prim::Recess` on why a
    /// flush region is a step, not a trough).
    fn overlay_carve(&mut self, c: &Carve) -> crate::draw::PlatePush {
        let rect = &c.rect;
        let infl = c.depth * 0.5 + 2.0;
        self.verts.extend(quad_vertices(
            rect.x - infl, rect.y - infl,
            rect.width + 2.0 * infl, rect.height + 2.0 * infl,
            self.sw, self.sh, [0.0; 4],
        ));
        let (x0, y0, x1, y1) = c.wall_box();
        let sdf_rect = crate::scene::layout::Rect { x: x0, y: y0, width: x1 - x0, height: y1 - y0 };
        let mut p = plate_push_raised(&sdf_rect, c.radii, c.depth, self.scale, self.light, self.finish, false, None);
        p.mode = c.mode;
        // w = 1.0 flags the free-carve shader path to composite its light in the tint and its
        // shadow in a dark tint instead of white and black (plates leave w at 0.0).
        if let Some(t) = c.tint {
            p.specular_tint = [t[0], t[1], t[2], 1.0];
        }
        // Host-plate box for the roll fade: a suppressed wall means the recess runs flush to
        // the host's edge there, so that side of the box sits at the original rect edge;
        // enabled walls face host interior, pushed to ±1e5 so no fade applies.
        const FAR: f32 = 1e5;
        let edges = c.edges;
        let (hx0, hy0) = (
            if edges.3 { rect.x - FAR } else { rect.x },
            if edges.0 { rect.y - FAR } else { rect.y },
        );
        let (hx1, hy1) = (
            if edges.1 { rect.x + rect.width + FAR } else { rect.x + rect.width },
            if edges.2 { rect.y + rect.height + FAR } else { rect.y + rect.height },
        );
        p.host = [
            (hx0 + hx1) * 0.5 * self.scale,
            (hy0 + hy1) * 0.5 * self.scale,
            (hx1 - hx0) * 0.5 * self.scale,
            (hy1 - hy0) * 0.5 * self.scale,
        ];
        p
    }

    /// Debug builds make one fallback loud without `CCE_PLATE_DEBUG`: a carve that could group
    /// failing to while an open plate encloses it and its shaded region reaches that plate's
    /// roll — see [`near_roll_fallback_reason`] on what qualifies and why this warns instead of
    /// panicking.
    #[cfg(debug_assertions)]
    fn warn_near_roll_fallback(&self, c: &Carve) {
        let enclosing = self.plate_stack.iter().enumerate().rev().find(|(_, (_, p))| c.within(p));
        let Some((si, &(bi, prect))) = enclosing else { return };
        // Host roll width rides the push's light.w (physical px).
        let roll = self.batches[bi].plate.as_ref().map_or(0.0, |p| p.light[3]) / self.scale;
        let later: Vec<crate::scene::layout::Rect> = self.plate_stack[si + 1..].iter().map(|&(_, r)| r).collect();
        let budget_full = self.features.len() >= crate::draw::MAX_PLATE_FEATURES;
        if let Some(why) = near_roll_fallback_reason(&c.rect, c.depth, &prect, roll, &later, budget_full) {
            let kind = c.kind();
            let rect = &c.rect;
            plate_carve_warn_once(format!(
                "plate-carve: near-roll {kind} ({:.0},{:.0} {:.0}x{:.0}) lost grouping — {why}; \
                 its junction with the host plate's roll shades through the overlay fallback, \
                 visually different from grouped frames (CCE_PLATE_DEBUG=1 traces verdicts) \
                 [debug-build warning, printed once]",
                rect.x, rect.y, rect.width, rect.height
            ));
        }
    }

    /// `CCE_PLATE_DEBUG`: count a grouped carve, or record why one fell back to an overlay —
    /// re-derived in the order [`Tess::carve_host`] tests its guards, so keep the two in step.
    fn note_carve_verdict(&mut self, c: &Carve, host: Option<usize>) {
        if host.is_some() {
            self.dbg.grouped += 1;
            return;
        }
        let (sx0, sy0, sx1, sy1) = c.shaded();
        let enclosing: Vec<usize> = self
            .plate_stack
            .iter()
            .enumerate()
            .filter(|(_, (_, p))| c.within(p))
            .map(|(si, _)| si)
            .collect();
        let occluded = |si: usize| {
            self.plate_stack[si + 1..].iter().any(|(_, o)| {
                sx0 < o.x + o.width && sx1 > o.x && sy0 < o.y + o.height && sy1 > o.y
            })
        };
        let edges = c.edges;
        let why = if c.mode >= 3.5 {
            "ridge — never groups (its bump profile is not a monotonic step)".into()
        } else if !c.full_ring() {
            format!("edge-suppressed {edges:?} — the extended wall would smear across the host")
        } else if c.tint.is_some() {
            "tinted — a CSG feature is geometry only, it carries no color".into()
        } else if self.features.len() >= crate::draw::MAX_PLATE_FEATURES {
            format!("feature budget full ({} used)", self.features.len())
        } else if enclosing.is_empty() {
            format!("no enclosing plate ({} open)", self.plate_stack.len())
        } else if enclosing.iter().all(|&si| occluded(si)) {
            "a later plate overlaps this carve's shaded region".into()
        } else {
            "host plate's feature run is closed (another carve appended since)".into()
        };
        let kind = c.kind();
        let rect = &c.rect;
        self.dbg.fell_back.push(format!(
            "  overlay: {kind} ({:.0},{:.0} {:.0}x{:.0}) — {why}",
            rect.x, rect.y, rect.width, rect.height
        ));
    }
}

/// A recess, boss, ridge or trough on the SDF path: what the grouping decision and both ways of
/// drawing it read. Recess carves down into the surface; Boss raises a plateau out of it (the
/// same machinery, depth sign flipped); Ridge is a raised rim straddling the boundary and Trough
/// the sunken valley twin (their own overlay profiles — never grouped, the CSG features only
/// model monotonic steps).
pub(super) struct Carve {
    rect: crate::scene::layout::Rect,
    radii: (f32, f32, f32, f32),
    depth: f32,
    edges: (bool, bool, bool, bool),
    /// The shader mode: 2 recess, 3 boss, 4 ridge, 9 trough.
    mode: f32,
    tint: Option<[f32; 3]>,
}

impl Carve {
    fn of(prim: &Prim) -> Carve {
        let (rect, radii, depth, edges, mode, tint) = match prim {
            Prim::Recess { rect, radii, depth, edges, tint } => (rect, radii, depth, edges, 2.0, *tint),
            Prim::Boss { rect, radii, depth, edges, tint } => (rect, radii, depth, edges, 3.0, *tint),
            Prim::Ridge { rect, radii, depth, edges } => (rect, radii, depth, edges, 4.0, None),
            Prim::Trough { rect, radii, depth, edges, tint } => (rect, radii, depth, edges, 9.0, *tint),
            _ => unreachable!("Carve::of: not a carve"),
        };
        Carve { rect: *rect, radii: *radii, depth: *depth, edges: *edges, mode, tint }
    }

    fn raised(&self) -> bool {
        self.mode > 2.5
    }

    fn full_ring(&self) -> bool {
        self.edges == (true, true, true, true)
    }

    /// Whether the carve can become a CSG feature at all. A ridge or trough cannot (not a
    /// monotonic step). An edge-suppressed carve never groups: a suppressed wall's rect extends
    /// past the carve, relying on the overlay cover quad to keep that shading out of the drawn
    /// pixels — a clip the plate's whole-surface draw does not have, so grouped it smears the
    /// extended walls across the plate (union pieces, such as section wells and a spinbox's
    /// field and button run, are exactly these). A tinted carve never groups either: a CSG
    /// feature is geometry only, so the tint could only land on the whole plate's specular.
    fn groupable(&self) -> bool {
        self.mode < 3.5 && self.full_ring() && self.tint.is_none()
    }

    /// The name `CCE_PLATE_DEBUG` and the near-roll warning give it.
    fn kind(&self) -> &'static str {
        if self.mode == 3.0 {
            "boss"
        } else if self.mode == 4.0 {
            "ridge"
        } else if self.mode == 9.0 {
            "trough"
        } else {
            "recess"
        }
    }

    /// Whether plate rect `p` encloses the carve (half a pixel's slack).
    fn within(&self, p: &crate::scene::layout::Rect) -> bool {
        let r = &self.rect;
        r.x >= p.x - 0.5 && r.y >= p.y - 0.5 && r.x + r.width <= p.x + p.width + 0.5 && r.y + r.height <= p.y + p.height + 0.5
    }

    /// The carve's shaded region (the overlay's cover-quad inflation), as x0, y0, x1, y1.
    fn shaded(&self) -> (f32, f32, f32, f32) {
        let r = &self.rect;
        let infl = self.depth * 0.5 + 2.0;
        (r.x - infl, r.y - infl, r.x + r.width + infl, r.y + r.height + infl)
    }

    /// The box its walls are drawn on: the rect, with each suppressed side pushed out past it.
    fn wall_box(&self) -> (f32, f32, f32, f32) {
        let r = &self.rect;
        let ext = self.depth + 4.0;
        let (mut x0, mut y0) = (r.x, r.y);
        let (mut x1, mut y1) = (r.x + r.width, r.y + r.height);
        if !self.edges.0 { y0 -= ext; }
        if !self.edges.1 { x1 += ext; }
        if !self.edges.2 { y1 += ext; }
        if !self.edges.3 { x0 -= ext; }
        (x0, y0, x1, y1)
    }
}
