//! Lit plates: a bevel, a frame (a plate turned inside out), a plate, a field and a sphere —
//! each one SDF-lit cover quad and its push block, or the banded path's vertex shading.

use super::*;

impl Tess {
    /// A plate prim. On the SDF path a filled plate opens a carve host (`Item::made_plate`).
    /// False when it leaves the item with nothing to batch.
    pub(super) fn plates(&mut self, it: &mut Item) -> bool {
        match &it.item.prim {
            Prim::Bevel { rect, radii, material, depth, tint } if self.shader_plates => {
                let color = material.fill(PlateRole::Nested);
                let mat = material.finish.to_array();
                // SDF-lit raised plate: one cover quad; the shader owns fill,
                // roll shading, corners, and silhouette AA. Nominal corner
                // radii (scale_corners false): a Bevel is a WIDGET-scale plate
                // whose silhouette must match the nominal-radius squircles of
                // the controls around it — only window-scale `Plate`s get the
                // curvature-matched span.
                self.verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, self.sw, self.sh, color));
                let mut p = plate_push_raised(rect, *radii, *depth, self.scale, self.light, mat, false, None);
                // The plate's own frost recipe rides host.zw (see PlatePush).
                let [fz, fw] = material.frost.pack(self.scale);
                p.host[2] = fz;
                p.host[3] = fw;
                // w = 1 marks an accent-tinted plate (the focused-pane
                // treatment): the shader keeps the roll's light and shadow
                // and recolours them — light toward the tint, shadow toward
                // a dark tint — matching the free-carve path's tinted-well
                // convention. Neutral white keeps w = 0 (a no-op multiply).
                let full = if *tint == [1.0, 1.0, 1.0] { 0.0 } else { 1.0 };
                p.specular_tint = [tint[0], tint[1], tint[2], full];
                it.plate = Some(p);
                it.made_plate = Some(*rect);
            }
            Prim::Frame { rect, hole, hole_radii, material, depth } if self.shader_plates => {
                // The Bevel branch turned inside out (shader MODE_FRAME): the
                // cover quad is the face's bound, the SDF box is the HOLE,
                // and the shader reads the distance outside it as the
                // plate's depth. Nominal corner radii, as a Bevel's. A carve
                // host over `rect`, like any filled plate.
                let color = material.fill(PlateRole::Nested);
                let mat = material.finish.to_array();
                self.verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, self.sw, self.sh, color));
                let mut p = plate_push_raised(hole, *hole_radii, *depth, self.scale, self.light, mat, false, None);
                let [fz, fw] = material.frost.pack(self.scale);
                p.host[2] = fz;
                p.host[3] = fw;
                p.mode = 17.0; // MODE_FRAME
                it.plate = Some(p);
                it.made_plate = Some(*rect);
            }
            Prim::Plate { rect, radii, material, depth, shape } if self.shader_plates => {
                let color = material.fill(PlateRole::Nested);
                let mat = material.finish.to_array();
                if *depth < 0.0 {
                    // Negative depth = fill-less roll overlay (MODE_ROLL): the
                    // window-edge roll shading alone, screened over whatever is
                    // beneath — for a root plate whose face is not a fill (the
                    // designer's 3D canvas). The cover quad carries no color,
                    // and the batch is NOT opened as a carve host: an overlay
                    // owns no surface for a CSG feature to cut into.
                    self.verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, self.sw, self.sh, [0.0; 4]));
                    let mut p = plate_push_raised(rect, *radii, -*depth, self.scale, self.light, mat, true, *shape);
                    p.mode = 11.0; // MODE_ROLL
                    it.plate = Some(p);
                } else {
                    // Same lit-plate branch; the cover quad is the exact rect so the
                    // silhouette and the compositor's rounded window corners agree.
                    self.verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, self.sw, self.sh, color));
                    let mut p = plate_push_raised(rect, *radii, *depth, self.scale, self.light, mat, true, *shape);
                    let [fz, fw] = material.frost.pack(self.scale);
                    p.host[2] = fz;
                    p.host[3] = fw;
                    it.plate = Some(p);
                    it.made_plate = Some(*rect);
                }
            }
            // A sunken well ending in a flush run (see `Prim::Field`): one
            // outline, one overlay — never grouped, its profile is not a
            // monotonic step. The cover quad inflates by half the wall, as a
            // free carve's does; the host-box slot carries the run's two
            // ends (physical px), since nothing fades against a host here.
            Prim::Field { rect, radii, depth, split, end, tint } if self.shader_plates => {
                let infl = *depth * 0.5 + 2.0;
                self.verts.extend(quad_vertices(
                    rect.x - infl, rect.y - infl,
                    rect.width + 2.0 * infl, rect.height + 2.0 * infl,
                    self.sw, self.sh, [0.0; 4],
                ));
                let mut p = plate_push_raised(rect, *radii, *depth, self.scale, self.light, self.finish, false, None);
                p.mode = 16.0; // MODE_FIELD
                if let Some(t) = tint {
                    p.specular_tint = [t[0], t[1], t[2], 1.0];
                }
                p.host = [*split * self.scale, *end * self.scale, 0.0, 0.0];
                it.plate = Some(p);
            }
            Prim::Bevel { rect, radii, material, depth, tint: _ } => {
                let color = material.fill(PlateRole::Nested);
                // Full-size fill: the lip is now a shading overlay, not a paint of the
                // outer ring, so the fill must cover the whole rect (the old inset fill
                // would leave the ring showing whatever lay beneath).
                let corners = crate::widget::CornerRadii {
                    top_left: radii.0, top_right: radii.1,
                    bottom_right: radii.2, bottom_left: radii.3,
                };
                push_rounded_rect_vertices_corners(rect.x, rect.y, rect.width, rect.height, corners, self.sw, self.sh, color, it.circle, None, &mut self.verts);
                push_plate_bevel_vertices(rect.x, rect.y, rect.width, rect.height, radii.0, *depth, self.sw, self.sh, color, it.circle, &mut self.verts);
            }
            // The legacy banded path has no inside-out SDF: the face square,
            // and only below the hole, so the coves' rows are left to what
            // is beneath (A/B comparison path only).
            Prim::Frame { rect, hole, material, .. } => {
                let color = material.fill(PlateRole::Nested);
                let y0 = rect.y.max(hole.y + hole.height);
                let y1 = rect.y + rect.height;
                if y1 > y0 {
                    self.verts.extend(quad_vertices(rect.x, y0, rect.width, y1 - y0, self.sw, self.sh, color));
                }
            }
            Prim::Plate { rect, radii, material, depth, .. } => {
                let color = material.fill(PlateRole::Nested);
                if *depth < 0.0 {
                    // Fill-less roll overlay (negative-depth sentinel): the banded
                    // legacy tessellation has no overlay compositing, so the roll
                    // is simply absent here — the A/B path draws nothing rather
                    // than a wrong fill.
                    return false;
                }
                // Fill at full size (no inset — see Prim::Plate), then light the face,
                // then roll the perimeter. The lip rides on top of the fill's outer band
                // rather than replacing it, so the plate's silhouette and the
                // compositor's rounded window corners still agree exactly.
                let corners = crate::widget::CornerRadii {
                    top_left: radii.0, top_right: radii.1,
                    bottom_right: radii.2, bottom_left: radii.3,
                };
                push_rounded_rect_vertices_corners(
                    rect.x, rect.y, rect.width, rect.height, corners, self.sw, self.sh, color, it.circle, None, &mut self.verts,
                );
                push_plate_face_vertices(rect.x, rect.y, rect.width, rect.height, self.sw, self.sh, it.circle, &mut self.verts);
                push_bevel_edge_vertices_radii(
                    rect.x, rect.y, rect.width, rect.height, *radii, *depth,
                    self.sw, self.sh, color, it.circle, 1.0, &mut self.verts,
                );
            }
            Prim::Field { rect, radii, depth, split, end, .. } => {
                // Legacy approximation: the two-box form `Prim::Field`
                // replaced — a well either side of the run a step down, the
                // run the Trough arm's down-then-up stack. The banded
                // machinery has no blended outline, and the legacy path
                // exists only for A/B comparison.
                let all = (true, true, true, true);
                let (fl, fr) = (rect.x, rect.x + rect.width);
                let (well_l, well_r) = (*split > fl, *end < fr);
                let rx = split.max(fl);
                let rw = (end.min(fr) - rx).max(0.0);
                if well_l {
                    push_bevel_edge_vertices_banded(
                        fl, rect.y, rx - fl, rect.height, (radii.0, 0.0, 0.0, radii.3), *depth,
                        self.sw, self.sh, [0.0; 4], it.circle, -1.0, default_bevel_bands(*depth), all,
                        EdgeKind::Step, &mut self.verts,
                    );
                }
                if well_r {
                    push_bevel_edge_vertices_banded(
                        rx + rw, rect.y, fr - rx - rw, rect.height, (0.0, radii.1, radii.2, 0.0), *depth,
                        self.sw, self.sh, [0.0; 4], it.circle, -1.0, default_bevel_bands(*depth), all,
                        EdgeKind::Step, &mut self.verts,
                    );
                }
                // A run's own corners where it reaches the outline; square at a seam.
                let (l0, l3) = if well_l { (0.0, 0.0) } else { (radii.0, radii.3) };
                let (r1, r2) = if well_r { (0.0, 0.0) } else { (radii.1, radii.2) };
                let half = *depth * 0.5;
                push_bevel_edge_vertices_banded(
                    rx, rect.y, rw, rect.height, (l0, r1, r2, l3), half,
                    self.sw, self.sh, [0.0; 4], it.circle, -1.0, default_bevel_bands(half), all,
                    EdgeKind::Step, &mut self.verts,
                );
                let ir = if well_r { 0.0 } else { (radii.1 - half).max(0.0) };
                let il = if well_l { 0.0 } else { (radii.0 - half).max(0.0) };
                push_bevel_edge_vertices_banded(
                    rx + half, rect.y + half, rw - *depth, rect.height - *depth, (il, ir, ir, il), half,
                    self.sw, self.sh, [0.0; 4], it.circle, 1.0, default_bevel_bands(half), all,
                    EdgeKind::Step, &mut self.verts,
                );
            }
            Prim::Sphere { cx, cy, radius, material } if self.shader_plates => {
                let color = material.fill(PlateRole::Nested);
                let mat = material.finish.to_array();
                // A hemisphere lit per pixel by the plate branch (mode 5): one
                // cover quad, its own never-merged batch. The quad overhangs
                // the disc by 1px for the shader's silhouette anti-aliasing.
                let d = *radius + 1.0;
                self.verts.extend(quad_vertices(cx - d, cy - d, 2.0 * d, 2.0 * d, self.sw, self.sh, color));
                it.plate = Some(crate::draw::PlatePush {
                    // Center + radius in physical px; the SDF box machinery is
                    // unused in this mode, so .w is free.
                    rect: [cx * self.scale, cy * self.scale, radius * self.scale, 0.0],
                    radii: [0.0; 4],
                    light: [self.light[0], self.light[1], self.light[2], 0.0],
                    material: mat,
                    host: [0.0; 4],
                    specular_tint: [1.0, 1.0, 1.0, 0.0],
                    mode: 5.0,
                    shape: 2.0,
                });
            }
            Prim::Sphere { cx, cy, radius, material } => {
                let color = material.fill(PlateRole::Nested);
                // Legacy path: the flat disc, exactly a Circle.
                self.verts.extend(circle_vertices(*cx, *cy, *radius, self.sw, self.sh, color, self.segs(*radius), it.circle));
            }
            _ => unreachable!("plates: not a plate prim"),
        }
        true
    }
}
