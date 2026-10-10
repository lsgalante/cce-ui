//! The controls' chrome the pane draws: flat quads, rounded quads, reliefs (the steps a flat host
//! draws), fields, grooves, spheres and section fillets.

use super::*;

/// One relief step a flat host draws for the pane (`ParametersBg::reliefs`): (x, y, w, h,
/// per-corner radii (tl, tr, br, bl), depth, raised, walls (top, right, bottom, left)).
/// Raised maps to `PaintCtx::boss_edges`, flat to `recess_edges`; radii and walls are per
/// entry because a section's well is composed of edge-suppressed pieces.
pub type Relief = (f32, f32, f32, f32, (f32, f32, f32, f32), f32, bool, (bool, bool, bool, bool));

impl ParametersBg {
    /// The pane's plain quads: section outlines (when relief is off), every row's chrome
    /// (slider and spinbox backgrounds read via `rect()` + `color()`, a separator's hairline,
    /// the code editor's box), and the controls' own plain quads — all clipped to the
    /// viewport. The scrollbar is not here: the host draws it via `scrollbar_quads`, above or
    /// below the pane plate depending on `scrollbar_active`.
    pub(super) fn plain_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.visible {
            return Vec::new();
        }
        let mut quads = Vec::new();
        if !crate::layout::control_relief() {
            self.push_section_outline_quads(&mut quads);
        }
        let rects = self.get_param_rects();
        let hidden = self.hidden_rows();
        for i in 0..self.display_params.len() {
            if !hidden[i] {
                self.push_row_quads(i, rects[i], &mut quads);
            }
        }
        // Clipped vertically to the viewport, less 4 px at each end.
        let view_min = self.rect.y + 4.0;
        let view_max = self.rect.y + self.rect.height - 4.0;
        quads
            .into_iter()
            .filter_map(|(qx, qy, qw, qh, qc)| {
                let y1 = qy.max(view_min);
                let y2 = (qy + qh).min(view_max);
                (y1 < y2).then_some((qx, y1, qw, y2 - y1, qc))
            })
            .collect()
    }

    /// Each section is drawn as ONE continuous outline — around the title, down the
    /// neck, around the content rows — whose straight runs are these quads; its corner
    /// fillets ride `arcs()`, which the host draws under the same clip. Under
    /// `control_relief` the outline is replaced by the inset carves served
    /// through `reliefs` (title tab + content body recessed).
    pub(super) fn push_section_outline_quads(&self, out: &mut Vec<(f32, f32, f32, f32, [f32; 4])>) {
        for (title, content) in self.section_boxes() {
            let (runs, _) = self.section_outline(title, content);
            out.extend(runs.into_iter().map(|(x, y, w, h)| (x, y, w, h, SECTION_BORDER_COLOR)));
        }
    }

    /// Row `i`'s plain chrome (`r` its row rect), by row type.
    pub(super) fn push_row_quads(&self, i: usize, r: (f32, f32, f32, f32), out: &mut Vec<(f32, f32, f32, f32, [f32; 4])>) {
        let kind = self.display_params[i].2.as_str();
        if kind.starts_with("slider") {
            if let Some(s) = &self.sliders[i] {
                let (sx, sy, sw, sh) = s.rect();
                out.push((sx, sy, sw, sh, s.color()));
                out.extend(crate::widget::shown_quads(s));
            }
        } else if kind == "section" {
            // A section's line is its outline's, above.
        } else if kind == SEPARATOR {
            // A hairline across the row, inset from the pane's edges.
            let c = crate::color::active_theme().surface_border;
            out.push((r.0 + 6.0, r.1, (r.2 - 12.0).max(0.0), 1.0, [c[0], c[1], c[2], c[3] * 0.8]));
        } else if is_vec_row(kind) {
            if let Some(f) = &self.float3s[i] {
                out.extend(crate::widget::shown_quads(f));
            }
        } else if kind == "code" {
            self.push_code_row_quads(i, r, out);
        } else if is_text_row(kind) {
            if let Some(tb) = &self.texts[i] {
                out.extend(crate::widget::shown_quads(tb));
            }
            if let Some(d) = &self.choices[i] {
                out.extend(crate::widget::shown_quads(d));
            }
        } else if kind.starts_with("choice") {
            if let Some(d) = &self.choices[i] {
                out.extend(crate::widget::shown_quads(d));
            }
        } else if kind == "button" {
            if let Some(b) = &self.buttons[i] {
                out.extend(crate::widget::shown_quads(b));
            }
        } else if kind.starts_with("spinbox") {
            if let Some(sb) = &self.spinboxes[i] {
                let (bx, by, bw, bh) = sb.rect();
                out.push((bx, by, bw, bh, sb.color()));
                out.extend(crate::widget::shown_quads(sb));
            }
        } else if kind == "toggle" || kind == "checkbox" {
            if let Some(cb) = &self.toggles[i] {
                out.extend(crate::widget::shown_quads(cb));
            }
        } else if kind.starts_with("color") || kind == "rgb" || kind == "rgba" {
            if let Some(c) = &self.colors[i] {
                out.extend(crate::widget::shown_quads(c));
            }
        }
    }

    /// The rounded companion to `Self::plain_quads`: the row controls whose boxes are
    /// `Prim::RoundedRect` (textbox, dropdown, button, toggle, color selector), which the
    /// plain list does not carry. Returned unclipped; the pane's paint clips them to its
    /// scroll viewport. (x, y, w, h, radius, color, (tl, tr, br, bl)).
    pub fn rounded_quads(
        &self,
    ) -> Vec<(f32, f32, f32, f32, f32, [f32; 4], (bool, bool, bool, bool))> {
        if !self.visible {
            return Vec::new();
        }
        let mut out = Vec::new();
        let hidden = self.hidden_rows();
        for (i, p) in self.display_params.iter().enumerate() {
            if hidden[i] {
                continue;
            }
            if is_text_row(&p.2) {
                if let Some(tb) = &self.texts[i] {
                    out.extend(crate::widget::shown_rounded_quads(tb));
                }
                if let Some(d) = &self.choices[i] {
                    out.extend(crate::widget::shown_rounded_quads(d));
                }
            } else if p.2.starts_with("slider") {
                // Track (square style only — the recessed style has no track
                // background), value fill, and readout box; the thumb knob is a
                // `Prim::Sphere` and rides `spheres()` instead.
                if let Some(s) = &self.sliders[i] {
                    out.extend(crate::widget::shown_rounded_quads(s));
                }
            } else if is_vec_row(&p.2) {
                // Three slider rows: the same set per row (readout boxes,
                // square-style tracks and fills), through the group's own paint.
                if let Some(f) = &self.float3s[i] {
                    out.extend(crate::widget::shown_rounded_quads(f));
                }
            } else if p.2.starts_with("choice") {
                if let Some(d) = &self.choices[i] {
                    out.extend(crate::widget::shown_rounded_quads(d));
                }
            } else if p.2.starts_with("spinbox") {
                // The spinbox's whole chrome (frame, display well, +/- button
                // wells) is modern rounded-rect paint — its legacy color() is
                // transparent and it has no extra_quads, so skipping it here
                // renders the row as bare text.
                if let Some(sb) = &self.spinboxes[i] {
                    out.extend(crate::widget::shown_rounded_quads(sb));
                }
            } else if p.2 == "button" {
                if let Some(b) = &self.buttons[i] {
                    out.extend(crate::widget::shown_rounded_quads(b));
                }
            } else if p.2 == "toggle" || p.2 == "checkbox" {
                if let Some(cb) = &self.toggles[i] {
                    out.extend(crate::widget::shown_rounded_quads(cb));
                }
            } else if p.2.starts_with("color") || p.2 == "rgb" || p.2 == "rgba" {
                if let Some(c) = &self.colors[i] {
                    out.extend(crate::widget::shown_rounded_quads(c));
                }
            }
        }
        out
    }

    /// The relief companion to [`Self::rounded_quads`]: the row controls'
    /// raised/recessed step prims (boss rims, recess wells), which the flat
    /// views cannot carry — a host rendering this panel through the legacy
    /// views must read this getter too or the `control_relief` styling is lost
    /// entirely (with the DE's transparent control backgrounds the controls all
    /// but vanish). Edges-only variants throughout: the flat views own the
    /// faces, exactly the widgets' own transparent-fill bevel→boss degradation.
    /// Returned unclipped; the host clips to the pane's scroll viewport and
    /// draws these AFTER the flat quads, so the walls' shading modulates the
    /// fills they cross (the order the widgets' own paints use). See `Relief`.
    pub fn reliefs(&self) -> Vec<Relief> {
        if !self.visible || !crate::layout::control_relief() {
            return Vec::new();
        }
        let mut out = Vec::new();
        self.push_section_reliefs(&mut out);
        let hidden = self.hidden_rows();
        for i in 0..self.display_params.len() {
            if !hidden[i] {
                self.push_row_relief(i, &mut out);
            }
        }
        out
    }

    /// Sections as inset panels (the flat outline+fillet path is the
    /// non-relief style): ONE union-shaped recess per section — a
    /// title-text-width tab strip flush with the body's left edge, opening
    /// into the full-width body below. Composed from three edge-suppressed
    /// pieces (the tab with its bottom open, the body with its top open,
    /// and the top-wall run right of the tab's throat) so no wall crosses
    /// the union's interior and the whole section reads as a single well.
    /// Collapsed sections keep the title-box carve.
    pub(super) fn push_section_reliefs(&self, out: &mut Vec<Relief>) {
        let all = (true, true, true, true);
        let r4 = |r: f32| (r, r, r, r);
        let r = SECTION_R;
        for (title, content) in self.section_boxes() {
            let (tx, ty, tw, th) = title;
            if let Some((cx, cy, cw, ch)) = content {
                // Capped at the channel: the well's wall must roll off inside the
                // groove between it and the controls packed one CHANNEL inside,
                // not shade across their faces. `section_depth` scales wall and
                // channel cap together — past 1.0 the roll crosses the groove
                // by choice.
                let depth = section_carve_depth(ch);
                // Tab strip: the title box itself, sitting flush on the body's top
                // edge (the header row above it stays plain plate), bottom open.
                // A wall FADES OUT over the carve width approaching a suppressed
                // edge (the tessellator's host fade — meant for carves flush with
                // a real plate edge), so every piece extends past its interior
                // seam by `depth`: its fade-out then crossfades with the
                // neighbor's fade-in instead of both dying AT the seam (which
                // notched the walls there; found the hard way).
                let throat_r = tx + tw;
                let rho = SECTION_FILLET_R;
                // Right of the throat, ONE piece owns the whole right run —
                // top wall, top-right arc, right wall, bottom-right arc — so
                // both right corners are real turns. (The old top-run +
                // full-body split put the top wall and the right wall in
                // different pieces; each faded out at the seam and the
                // top-right corner rendered square.) A left piece carries the
                // left wall and the bottom-left arc; the two bottom runs
                // crossfade under the seam at the right piece's left edge.
                let body_lr = |x_run: f32, out: &mut Vec<_>| {
                    out.push((x_run, cy, cx + cw - x_run, ch, (0.0, r, r, 0.0), depth, false, (true, true, true, false)));
                    out.push((cx, cy, x_run + depth - cx, ch, (0.0, 0.0, 0.0, r), depth, false, (false, false, true, true)));
                };
                if cx + cw > throat_r + 2.0 * rho {
                    // Filleted throat ([`Self::section_fillets`]): the tab's
                    // right wall must END at the fillet's vertical tangent
                    // (crossfading out under the arc) or its straight run
                    // ghosts through the curve — so the tab piece stops there,
                    // and a left-only bridge carries the left wall across the
                    // fillet span down to the body's own fade-in.
                    out.push((tx, ty, tw, (cy - rho) - ty + depth, (r, r, 0.0, 0.0), depth, false, (true, true, false, true)));
                    out.push((tx, cy - rho, tw, rho + depth, (0.0, 0.0, 0.0, 0.0), depth, false, (false, false, false, true)));
                    body_lr(throat_r + rho - depth, out);
                } else {
                    // Too narrow for the fillet: the plain square throat.
                    out.push((tx, ty, tw, th + depth, (r, r, 0.0, 0.0), depth, false, (true, true, false, true)));
                    if cx + cw > throat_r + 0.5 {
                        body_lr(throat_r - depth, out);
                    } else {
                        // The tab spans the body: no top wall at all.
                        out.push((cx, cy, cw, ch, (0.0, 0.0, r, r), depth, false, (false, true, true, true)));
                    }
                }
            } else {
                let depth = section_carve_depth(th);
                out.push((tx, ty, tw, th, r4(SECTION_R), depth, false, all));
            }
        }
    }

    /// Row `i`'s plain well, if it has one: a text box (unless it is joined to its picker,
    /// half of a field), a spinbox with no -/+ run, a colour selector's well. The other rows
    /// draw none here: a choice trigger, a button and a toggle are fields ([`Self::fields`]),
    /// as are a textpick row and a spinbox with its run; sliders and vector rows are bands,
    /// their wells hand-shaded quads that follow the band's contour (the plain-quad view).
    /// Every well keeps its top label band outside the relief, as every host does.
    pub(super) fn push_row_relief(&self, i: usize, out: &mut Vec<Relief>) {
        let all = (true, true, true, true);
        let r4 = |r: f32| (r, r, r, r);
        let kind = self.display_params[i].2.as_str();
        if is_text_row(kind) {
            let Some(tb) = &self.texts[i] else { return };
            let (x, y, w, h) = tb.rect();
            if w <= 0.0 || h <= 0.0 || tb.inner().joined_right {
                return;
            }
            let ty = tb.label_strip();
            out.push((x, y + ty, w, h - ty, r4(crate::layout::textbox_corner_radius()), wall_depth(h - ty), false, all));
        } else if kind.starts_with("spinbox") {
            // Same side-label inset, content band and depth cap as its paint.
            let Some(sb) = &self.spinboxes[i] else { return };
            let (x, y, w, h) = sb.rect();
            let ty = sb.label_strip();
            let band = Rect { x, y: y + ty, width: w, height: h - ty };
            if w > 0.0 && h > 0.0 {
                let r = crate::layout::spinbox_corner_radius();
                if let Some(rel) = sb.inner().relief_parts(band) {
                    if rel.run.is_none() {
                        out.push((x, y + ty, w, h - ty, r4(r), wall_depth(h - ty), false, all));
                    }
                }
            }
        } else if kind.starts_with("color") || kind == "rgb" || kind == "rgba" {
            // The control's one well (`ColorSelector::field_relief`, the same geometry its
            // paint carves); the swatch is a fill on its floor and the seam a groove, both
            // on the widget's paint.
            let Some(c) = &self.colors[i] else { return };
            let (x, y, w, h) = c.rect();
            let ty = c.label_strip();
            if let Some((rx, ry, rw, rh, rr, rd)) = c.inner().field_relief(Rect { x, y: y + ty, width: w, height: h - ty }) {
                out.push((rx, ry, rw, rh, r4(rr), rd, false, all));
            }
        }
    }

    /// The rows that are ONE field with a run ([`Field`]: a sunken well holding a flush run,
    /// one outline round both), drawn AFTER [`Self::reliefs`], so every flush control in the
    /// pane has the same edge. The plain wells are not here: they are [`Self::reliefs`], which
    /// group into the pane's plate. Each field is drawn on its control's band, the wall
    /// straddling the band's outline.
    pub fn fields(&self) -> Vec<Field> {
        if !self.visible || !crate::layout::control_relief() {
            return Vec::new();
        }
        let hidden = self.hidden_rows();
        (0..self.display_params.len())
            .filter(|&i| !hidden[i])
            .filter_map(|i| self.row_field(i))
            .collect()
    }

    /// Row `i`'s field, if it is one: a textpick row, its text box's well ending in its picker
    /// ([`Field::ending_in_run`]); a spinbox, its value's well ending in its -/+ run; a button or
    /// a dropdown trigger, a field that is all run ([`Field::run`]) as its own flush plate
    /// draws it (`PaintCtx::inset_plate`); a toggle or checkbox, its own sliding field
    /// (`Toggle::field`), which IS the control (it paints no fill).
    pub(super) fn row_field(&self, i: usize) -> Option<Field> {
        let kind = self.display_params[i].2.as_str();
        if kind.starts_with("textpick") {
            let (tb, d) = (self.texts[i].as_ref()?, self.choices[i].as_ref()?);
            if !tb.inner().joined_right {
                return None;
            }
            let band = control_band(tb.rect(), tb.label_strip());
            let (dx, _, dw, _) = d.rect();
            (band.width > 0.0 && band.height > 0.0 && dw > 0.0).then(|| {
                let r = crate::layout::textbox_corner_radius();
                let outline = Rect { width: dx + dw - band.x, ..band };
                Field::ending_in_run(outline, (r, r, r, r), wall_depth(band.height), dx)
            })
        } else if kind == "button" {
            let b = self.buttons[i].as_ref()?;
            let band = control_band(b.rect(), b.label_strip());
            (band.width > 0.0 && band.height > 0.0).then(|| {
                let r = crate::layout::button_corner_radius();
                Field::run(band, (r, r, r, r), wall_depth(band.height))
            })
        } else if kind.starts_with("choice") {
            let d = self.choices[i].as_ref()?;
            let band = control_band(d.rect(), d.label_strip());
            (band.width > 0.0 && band.height > 0.0).then(|| {
                let r = crate::layout::dropdown_corner_radius();
                Field::run(band, (r, r, r, r), wall_depth(band.height))
            })
        } else if kind.starts_with("spinbox") {
            let sb = self.spinboxes[i].as_ref()?;
            let band = control_band(sb.rect(), sb.label_strip());
            let rel = sb.inner().relief_parts(band)?;
            let (split, _) = rel.run?;
            let r = rel.radius;
            Some(Field::ending_in_run(band, (r, r, r, r), rel.depth, split))
        } else if kind == "toggle" || kind == "checkbox" {
            let t = self.toggles[i].as_ref()?;
            let band = control_band(t.rect(), t.label_strip());
            // The pane's hover is the tint here, as on every field.
            (band.width > 0.0 && band.height > 0.0).then(|| t.inner().field(band).with_tint(None))
        } else {
            None
        }
    }

    /// The engraved seams companion — `(a, b, width, depth, host)` for
    /// [`crate::scene::paint::PaintCtx::groove`], drawn AFTER [`Self::fields`]
    /// (a groove engraves the surface the trough's control face provides).
    /// Today: the seam dividing a spinbox's -/+ run into its two buttons —
    /// the breadcrumb's segment-seam language at miniature scale.
    #[allow(clippy::type_complexity)]
    pub fn grooves(&self) -> Vec<((f32, f32), (f32, f32), f32, f32, Rect)> {
        if !self.visible || !crate::layout::control_relief() {
            return Vec::new();
        }
        let mut out = Vec::new();
        let hidden = self.hidden_rows();
        for (i, p) in self.display_params.iter().enumerate() {
            if hidden[i] || !p.2.starts_with("spinbox") {
                continue;
            }
            if let Some(sb) = &self.spinboxes[i] {
                let (x, y, w, h) = sb.rect();
                let ty = sb.label_strip();
                let band = Rect { x, y: y + ty, width: w, height: h - ty };
                if let Some(rel) = sb.inner().relief_parts(band) {
                    if let Some((_, (sa, sb2, sw, host))) = rel.run {
                        out.push((sa, sb2, sw, rel.depth, host));
                    }
                }
            }
        }
        out
    }

    /// The knobs a flat host draws after [`Self::reliefs`] — none: the sliders
    /// are bands (their swell is part of the band's own quads), so this is kept
    /// only for the hosts that still call it.
    pub fn spheres(&self) -> Vec<(f32, f32, f32, [f32; 4])> {
        Vec::new()
    }

    /// The section carves' concave inside-corner fillets — `(cx, cy, radius,
    /// depth, start angle)` for [`crate::scene::paint::PaintCtx::concave_fillet`]
    /// (recessed), drawn by the host AFTER [`Self::reliefs`]. The box reliefs
    /// can only round convex corners; this rounds the throat where a tab's
    /// right wall turns onto its body's top edge.
    pub fn section_fillets(&self) -> Vec<(f32, f32, f32, f32, f32)> {
        if !self.visible || !crate::layout::control_relief() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (title, content) in self.section_boxes() {
            let (tx, _ty, tw, _th) = title;
            if let Some((cx, cy, cw, ch)) = content {
                let depth = section_carve_depth(ch);
                let throat_r = tx + tw;
                if cx + cw > throat_r + 2.0 * SECTION_FILLET_R {
                    out.push((
                        throat_r + SECTION_FILLET_R,
                        cy - SECTION_FILLET_R,
                        SECTION_FILLET_R,
                        depth,
                        std::f32::consts::FRAC_PI_2,
                    ));
                }
            }
        }
        out
    }
}

/// A control's band: its rect below its top label strip — what its relief is drawn on.
fn control_band((x, y, w, h): (f32, f32, f32, f32), label_strip: f32) -> Rect {
    Rect { x, y: y + label_strip, width: w, height: h - label_strip }
}

/// The wall a control's relief carves into a band `band_h` tall: the DE's roll width, capped
/// at a fifth of the band, the cap every control's own paint uses.
fn wall_depth(band_h: f32) -> f32 {
    crate::layout::bevel_width().min(band_h * 0.2)
}
