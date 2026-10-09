//! Flat prims: fills (a frosted one promoted to a zero-depth plate batch on the SDF path),
//! borders, glow, arcs, vectors and circles.

use super::*;

impl Tess {
    /// A flat prim's vertices. False when it leaves the item with nothing to batch.
    pub(super) fn flat(&mut self, it: &mut Item) -> bool {
        match &it.item.prim {
            // A frosted FLAT fill — a `Flat` control face, a menu panel, a
            // popover, an inset plate's face — is a zero-depth plate batch
            // (RFC material § 6.2): the same shader path as every plate, so
            // it carries its own frost recipe instead of a window-wide one,
            // with the configured `corner_shape` and no roll, which is what the
            // tessellated fill drew. The display list is untouched, so the
            // legacy bridges that extract RoundedRects still see one.
            Prim::Quad { rect, color } if self.shader_plates && color[3] < 0.0 => {
                self.verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, self.sw, self.sh, *color));
                it.plate = Some(flat_frost_push(rect, (0.0, 0.0, 0.0, 0.0), *color, self.scale, self.light, self.finish));
                it.promoted = true;
            }
            Prim::RoundedRect { rect, radius, corners, color } if self.shader_plates && color[3] < 0.0 => {
                let radii = (
                    if corners.0 { *radius } else { 0.0 },
                    if corners.1 { *radius } else { 0.0 },
                    if corners.2 { *radius } else { 0.0 },
                    if corners.3 { *radius } else { 0.0 },
                );
                self.verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, self.sw, self.sh, *color));
                it.plate = Some(flat_frost_push(rect, radii, *color, self.scale, self.light, self.finish));
                it.promoted = true;
            }
            Prim::Fill { rect, radii, material } if self.shader_plates && material.frost.is_frosted() => {
                // A material's flat fill: the frosted promotion above with
                // the MATERIAL's recipe (compression, refraction, radius)
                // instead of the DE default's. Zero depth, the configured
                // corner shape, no host — exactly a promoted RoundedRect.
                let color = material.fill(PlateRole::Nested);
                self.verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, self.sw, self.sh, color));
                let mut p = plate_push_raised(rect, *radii, 0.0, self.scale, self.light, self.finish, false, None);
                let [fz, fw] = material.frost.pack(self.scale);
                p.host[2] = fz;
                p.host[3] = fw;
                it.plate = Some(p);
                it.promoted = true;
            }
            Prim::Fill { rect, radii, material } => {
                // Opaque (or the legacy path): a plain rounded fill.
                let cr = crate::widget::CornerRadii::new(radii.0, radii.1, radii.2, radii.3);
                push_rounded_rect_vertices_corners(rect.x, rect.y, rect.width, rect.height, cr, self.sw, self.sh, material.fill(PlateRole::Nested), it.circle, None, &mut self.verts);
            }
            Prim::Border { rect, radii, fill, border, thickness } if self.shader_plates && fill[3] < 0.0 => {
                // The fill as its own plate batch, closed here; the stroke
                // follows as ordinary geometry in the batch the tail makes.
                self.verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, self.sw, self.sh, *fill));
                let p = flat_frost_push(rect, *radii, *fill, self.scale, self.light, self.finish);
                let end = self.verts.len() as u32;
                self.plate_stack.clear();
                self.batches.push(DlBatch { scissor: it.item.clip, clip_rrect: it.item.clip_rrect, start: it.start, end, plate: Some(p), blur_behind: true });
                it.start = end;
                it.blur_behind = false;
                let cr = crate::widget::CornerRadii::new(radii.0, radii.1, radii.2, radii.3);
                push_plate_solid_border_vertices(rect.x, rect.y, rect.width, rect.height, cr, *thickness, self.sw, self.sh, *border, it.circle, &mut self.verts);
            }
            Prim::Quad { rect, color } => {
                // Quads honor an active circle clip like circles/arcs do (the
                // Ramp's foam-cell fills draw as clipped strips).
                self.verts.extend(quad_vertices_with_clip(rect.x, rect.y, rect.width, rect.height, self.sw, self.sh, *color, it.circle));
            }
            Prim::RoundedRect { rect, radius, corners, color } => {
                let radii = crate::widget::CornerRadii::new(
                    if corners.0 { *radius } else { 0.0 },
                    if corners.1 { *radius } else { 0.0 },
                    if corners.2 { *radius } else { 0.0 },
                    if corners.3 { *radius } else { 0.0 },
                );
                push_rounded_rect_vertices_corners(rect.x, rect.y, rect.width, rect.height, radii, self.sw, self.sh, *color, it.circle, None, &mut self.verts);
            }
            Prim::Border { rect, radii, fill, border, thickness } => {
                let cr = crate::widget::CornerRadii::new(radii.0, radii.1, radii.2, radii.3);
                push_rounded_rect_vertices_corners(rect.x, rect.y, rect.width, rect.height, cr, self.sw, self.sh, *fill, it.circle, None, &mut self.verts);
                push_plate_solid_border_vertices(rect.x, rect.y, rect.width, rect.height, cr, *thickness, self.sw, self.sh, *border, it.circle, &mut self.verts);
            }
            Prim::Glow { rect, radius, reach, color } => {
                push_glow_vertices(rect.x, rect.y, rect.width, rect.height, *radius, *reach, self.sw, self.sh, *color, it.circle, &mut self.verts);
            }
            Prim::Arc { cx, cy, radius, thickness, start: sa, end: ea, color } => {
                push_arc_background_vertices(*cx, *cy, *radius, *thickness, *sa, *ea, self.sw, self.sh, *color, self.segs(*radius), it.circle, &mut self.verts);
            }
            Prim::ArcShaded { cx, cy, radius, thickness, start: sa, end: ea, inner, crest, outer } => {
                push_arc_shaded_vertices(*cx, *cy, *radius, *thickness, *sa, *ea, self.sw, self.sh, *inner, *crest, *outer, self.segs(*radius), it.circle, &mut self.verts);
            }
            Prim::Vector { x1, y1, x2, y2, thickness, color, cap } => {
                let lc = match cap {
                    Cap::Flat => LineCap::Flat,
                    Cap::Round => LineCap::Round,
                    Cap::Arrow => LineCap::Arrow,
                };
                self.verts.extend(vector_vertices(*x1, *y1, *x2, *y2, *thickness, self.sw, self.sh, *color, lc));
            }
            Prim::Circle { cx, cy, radius, color } => {
                if it.item.clip_circle.is_none() && *radius > 1.5 {
                    // Cover quad with the disc itself as the (feathered) circle
                    // clip: a per-pixel smooth silhouette instead of a hard-edged
                    // fan. The quad overhangs by 1px for the feather. Only when
                    // no ancestor clip holds the slot — then it's the fan path.
                    let own = [cx * self.scale, cy * self.scale, radius * self.scale];
                    let d = *radius + 1.0;
                    self.verts.extend(quad_vertices_with_clip(
                        cx - d, cy - d, 2.0 * d, 2.0 * d, self.sw, self.sh, *color, own,
                    ));
                } else {
                    self.verts.extend(circle_vertices(*cx, *cy, *radius, self.sw, self.sh, *color, self.segs(*radius), it.circle));
                }
            }
            _ => unreachable!("flat: not a flat prim"),
        }
        true
    }
}
