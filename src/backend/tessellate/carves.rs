//! Carves: recesses, bosses, ridges and troughs (a full-ring untinted one grouped into its
//! host plate as a CSG feature when it can be, else an overlay), the concave fillet, the
//! groove, the lattice and its grout, and the carve union.

use super::*;

impl Tess {
    /// A carve prim. A carve grouped into its host plate adds a feature and emits no vertices
    /// (false); so does a degenerate groove or an empty union.
    pub(super) fn carves(&mut self, it: &mut Item) -> bool {
        match &it.item.prim {
            Prim::Recess { rect, radii, depth, edges, .. }
            | Prim::Boss { rect, radii, depth, edges, .. }
            | Prim::Ridge { rect, radii, depth, edges }
            | Prim::Trough { rect, radii, depth, edges, .. }
                if self.shader_plates =>
            {
                let tint = match &it.item.prim {
                    Prim::Recess { tint, .. } => *tint,
                    Prim::Boss { tint, .. } => *tint,
                    Prim::Trough { tint, .. } => *tint,
                    _ => None,
                };
                // Recess carves down into the surface; Boss raises a plateau out
                // of it (same machinery, depth sign flipped); Ridge is a raised
                // rim straddling the boundary and Trough the sunken valley twin
                // (their own overlay profiles — never grouped, the CSG features
                // only model monotonic steps).
                let mode = match &it.item.prim {
                    Prim::Boss { .. } => 3.0f32,
                    Prim::Ridge { .. } => 4.0,
                    Prim::Trough { .. } => 9.0,
                    _ => 2.0,
                };
                let raised = mode > 2.5;
                // Grouped into the enclosing plate whenever one is live: the
                // carve becomes a CSG feature of that plate's single draw —
                // exact composite shading, real junctions at the plate's rolled
                // perimeter — instead of a shading overlay (the fallback below).
                //
                // Edge-suppressed carves NEVER group: a suppressed wall's rect
                // extends past the carve (below), relying on the overlay cover
                // quad to keep that shading out of the drawn pixels — a clip
                // the plate's whole-surface draw does not have, so grouped it
                // smears the extended walls across the plate. Union pieces
                // (section wells, a spinbox's field and button run) are
                // exactly these.
                // A tinted carve also never groups: a CSG feature is geometry only,
                // so the tint could only land on the whole plate's specular.
                let full_ring = *edges == (true, true, true, true);
                let host_plate = if mode < 3.5 && full_ring && tint.is_none() && self.features.len() < crate::draw::MAX_PLATE_FEATURES {
                    // The carve's shaded region, for the occlusion test below
                    // (the overlay path's cover-quad inflation).
                    let infl = *depth * 0.5 + 2.0;
                    let (sx0, sy0) = (rect.x - infl, rect.y - infl);
                    let (sx1, sy1) = (rect.x + rect.width + infl, rect.y + rect.height + infl);
                    self.plate_stack
                        .iter()
                        .enumerate()
                        .rev()
                        .find(|(si, (bi, prect))| {
                            let inside = rect.x >= prect.x - 0.5
                                && rect.y >= prect.y - 0.5
                                && rect.x + rect.width <= prect.x + prect.width + 0.5
                                && rect.y + rect.height <= prect.y + prect.height + 0.5;
                            if !inside {
                                return false;
                            }
                            // Pixels drawn since this plate (a LATER plate in the
                            // stack) must not overlap the carve — its shading would
                            // land beneath them in this plate's earlier draw.
                            if self.plate_stack[si + 1..].iter().any(|(_, orect)| {
                                sx0 < orect.x + orect.width
                                    && sx1 > orect.x
                                    && sy0 < orect.y + orect.height
                                    && sy1 > orect.y
                            }) {
                                return false;
                            }
                            // Contiguity: only the last feature-receiving plate (or
                            // one with no features yet) may take another.
                            self.batches[*bi].plate.as_ref().is_some_and(|p| p.host[1] == 0.0)
                                || self.last_feature_plate == Some(*bi)
                        })
                        .map(|(_, &(bi, _))| bi)
                } else {
                    None
                };
                // Debug-build loudness for the silent grouped→overlay flip —
                // see `near_roll_fallback_reason` on what qualifies and why
                // this warns instead of panicking.
                #[cfg(debug_assertions)]
                if host_plate.is_none() && mode < 3.5 && full_ring && tint.is_none() {
                    let enclosing = self.plate_stack.iter().enumerate().rev().find(|(_, (_, p))| {
                        rect.x >= p.x - 0.5
                            && rect.y >= p.y - 0.5
                            && rect.x + rect.width <= p.x + p.width + 0.5
                            && rect.y + rect.height <= p.y + p.height + 0.5
                    });
                    if let Some((si, &(bi, prect))) = enclosing {
                        // Host roll width rides the push's light.w (physical px).
                        let roll = self.batches[bi].plate.as_ref().map_or(0.0, |p| p.light[3]) / self.scale;
                        let later: Vec<crate::scene::layout::Rect> =
                            self.plate_stack[si + 1..].iter().map(|&(_, r)| r).collect();
                        let budget_full = self.features.len() >= crate::draw::MAX_PLATE_FEATURES;
                        if let Some(why) =
                            near_roll_fallback_reason(rect, *depth, &prect, roll, &later, budget_full)
                        {
                            let kind = if mode > 2.5 { "boss" } else { "recess" };
                            plate_carve_warn_once(format!(
                                "plate-carve: near-roll {kind} ({:.0},{:.0} {:.0}x{:.0}) lost grouping — {why}; \
                                 its junction with the host plate's roll shades through the overlay fallback, \
                                 visually different from grouped frames (CCE_PLATE_DEBUG=1 traces verdicts) \
                                 [debug-build warning, printed once]",
                                rect.x, rect.y, rect.width, rect.height
                            ));
                        }
                    }
                }
                if self.dbg.on {
                    match host_plate {
                        Some(_) => self.dbg.grouped += 1,
                        None => {
                            // Re-derive WHY, in the same order the guard tests
                            // them. Debug-only: the hot path above is untouched.
                            let kind = match &it.item.prim {
                                Prim::Boss { .. } => "boss",
                                Prim::Ridge { .. } => "ridge",
                                Prim::Trough { .. } => "trough",
                                _ => "recess",
                            };
                            let infl = *depth * 0.5 + 2.0;
                            let (sx0, sy0) = (rect.x - infl, rect.y - infl);
                            let (sx1, sy1) = (rect.x + rect.width + infl, rect.y + rect.height + infl);
                            let enclosing: Vec<usize> = self.plate_stack
                                .iter()
                                .enumerate()
                                .filter(|(_, (_, p))| {
                                    rect.x >= p.x - 0.5
                                        && rect.y >= p.y - 0.5
                                        && rect.x + rect.width <= p.x + p.width + 0.5
                                        && rect.y + rect.height <= p.y + p.height + 0.5
                                })
                                .map(|(si, _)| si)
                                .collect();
                            let occluded = |si: usize| {
                                self.plate_stack[si + 1..].iter().any(|(_, o)| {
                                    sx0 < o.x + o.width && sx1 > o.x && sy0 < o.y + o.height && sy1 > o.y
                                })
                            };
                            let why = if mode >= 3.5 {
                                "ridge — never groups (its bump profile is not a monotonic step)".into()
                            } else if !full_ring {
                                format!("edge-suppressed {edges:?} — the extended wall would smear across the host")
                            } else if tint.is_some() {
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
                            self.dbg.fell_back.push(format!(
                                "  overlay: {kind} ({:.0},{:.0} {:.0}x{:.0}) — {why}",
                                rect.x, rect.y, rect.width, rect.height
                            ));
                        }
                    }
                }
                if let Some(bi) = host_plate {
                    {
                        // A wall the carve shares with the plate's edge extends
                        // past the plate, so the carve has no wall there.
                        let ext = *depth + 4.0;
                        let (mut x0, mut y0) = (rect.x, rect.y);
                        let (mut x1, mut y1) = (rect.x + rect.width, rect.y + rect.height);
                        if !edges.0 { y0 -= ext; }
                        if !edges.1 { x1 += ext; }
                        if !edges.2 { y1 += ext; }
                        if !edges.3 { x0 -= ext; }
                        let t_px = *depth * self.scale;
                        // The carve's drop: the material's pinned height, else
                        // the analytic ratio of the wall saturating at the DE's
                        // roll width (`layout::carve_depth_px` states the rule
                        // once for this path and the shader's free carves).
                        let k_mag = crate::layout::carve_depth_px(*depth) * self.scale;
                        // Negative depth = raised (Boss); the shader's summed
                        // slope vectors and curvature sign follow it.
                        let k_px = if raised { -k_mag } else { k_mag };
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
                            radii.0 * self.scale,
                            radii.1 * self.scale,
                            radii.2 * self.scale,
                            radii.3 * self.scale,
                            t_px,
                            k_px,
                            0.0,
                            0.0,
                        ]);
                        return false;
                    }
                }
                // Overlay-only carve: the cover quad inflates by half the roll
                // width (the step straddles the boundary) and carries no color —
                // the shader emits translucent white/black over what's beneath.
                let infl = *depth * 0.5 + 2.0;
                self.verts.extend(quad_vertices(
                    rect.x - infl, rect.y - infl,
                    rect.width + 2.0 * infl, rect.height + 2.0 * infl,
                    self.sw, self.sh, [0.0; 4],
                ));
                // A suppressed wall is pushed past the cover quad, so its
                // shading falls outside the drawn pixels (see Prim::Recess on
                // why a flush region is a step, not a trough).
                let ext = *depth + 4.0;
                let (mut x0, mut y0) = (rect.x, rect.y);
                let (mut x1, mut y1) = (rect.x + rect.width, rect.y + rect.height);
                if !edges.0 { y0 -= ext; }
                if !edges.1 { x1 += ext; }
                if !edges.2 { y1 += ext; }
                if !edges.3 { x0 -= ext; }
                let sdf_rect = crate::scene::layout::Rect { x: x0, y: y0, width: x1 - x0, height: y1 - y0 };
                let mut p = plate_push_raised(&sdf_rect, *radii, *depth, self.scale, self.light, self.finish, false, None);
                p.mode = mode;
                // w = 1.0 flags the free-carve shader path to composite its
                // light in the tint and its shadow in a dark tint instead of
                // white and black (plates leave w at 0.0).
                if let Some(t) = tint {
                    p.specular_tint = [t[0], t[1], t[2], 1.0];
                }
                // Host-plate box for the roll fade: a suppressed wall means the
                // recess runs flush to the host's edge there, so that side of
                // the box sits at the original rect edge; enabled walls face
                // host interior, pushed to ±1e5 so no fade applies.
                const FAR: f32 = 1e5;
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
                it.plate = Some(p);
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
}
