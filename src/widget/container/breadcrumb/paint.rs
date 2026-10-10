//! What a breadcrumb draws: its plate (raised or flush), the seams, the hover wash, the elision
//! marker and each segment's text.

use super::*;

impl Paint for Breadcrumb {
    fn color(&self) -> [f32; 4] {
        // No whole-widget fill in either style: each segment draws its own
        // button plate in paint().
        [0.0; 4]
    }

    fn widget_font(&self) -> Option<String> {
        let font = crate::layout::breadcrumb_font();
        if font.is_empty() {
            None
        } else {
            Some(font)
        }
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        // The segment run is ONE flush inset plate hugging its content — the
        // dropdown trigger's exact relief (`Dropdown::paint_background`'s
        // raised style: groove ring sunk around the control, its lip rolling
        // back up, face level with the window plate) — divided into segments
        // by seams engraved across it at a "/" lean. The old full-width
        // recessed well is gone: right of the run there is plain window
        // surface now, just as there is around the dropdown.
        // ONE knob with the dropdown (style.control.dropdown.corner_radius):
        // the two controls share a silhouette by construction, not by two
        // numbers happening to agree.
        let radius = crate::layout::dropdown_corner_radius();
        let relief = crate::layout::control_relief();

        let segs = self.visible_segs(rect);
        if let Some((rx, ry, rw, rh)) = self.run_box(rect) {
            let run_rect = Rect { x: rx, y: ry, width: rw, height: rh };
            let r = radius.min(rh * 0.5);
            if relief {
                let depth = crate::layout::bevel_width().min(rh * 0.2);
                // The face comes from the DROPDOWN's fill knob, not one of
                // the breadcrumb's own: the two controls sit side by side on a
                // toolbar and must read as the same material under any config.
                // A transparent configured fill is the dropdown's degraded
                // form — edges only, the window plate showing through as the
                // face, which is what the boss run always did here; an opaque
                // one makes both controls that color. Mirrors
                // `Dropdown::paint_background`'s `face` exactly.
                let face = crate::scene::Material::control_face(crate::color::dropdown_background_color());
                let (stance, face) = if self.raised {
                    // The floating stance: the run rises out of the surface as
                    // ONE beveled plate — fill and raised roll in a single
                    // lighting pass. The face is deliberately translucent
                    // ([`Self::RAISED_FACE_OPACITY`] over the configured fill)
                    // and ALWAYS frosted (`Frost::from_style`, the blur-behind
                    // pass): a floating part shows what is under it, and
                    // at this translucency the frost is what keeps the names
                    // legible over live content beneath. A transparent
                    // configured fill keeps the boss degradation: edges only,
                    // the surface as the face.
                    let c = face.map(|m| {
                        let t = m.tint;
                        m.with_tint([t[0], t[1], t[2], t[3] * Self::RAISED_FACE_OPACITY])
                            .with_frost(crate::scene::Frost::from_style())
                    });
                    (crate::widget::PlateStance::Raised, c)
                } else {
                    (crate::widget::PlateStance::Flush, face)
                };
                ctx.control_plate(
                    &crate::widget::ControlPlate::control(run_rect, r, stance, face)
                        .with_depth(depth)
                        .with_tint(self.focused.then(crate::widget::ControlPlate::focus_tint)),
                );
                for (a, b) in self.seams(rect) {
                    ctx.groove(a, b, Self::SEAM_WIDTH, depth, run_rect);
                }
            } else if self.focused {
                // Flat: no rim to light, so the run wears a hairline ring in the highlight.
                let t = crate::widget::ControlPlate::focus_tint();
                ctx.border(run_rect, (r, r, r, r), self.bg_color(), [t[0], t[1], t[2], 1.0], 1.0);
                for (a, b) in self.seams(rect) {
                    ctx.vector(a.0, a.1, b.0, b.1, 1.0, [0.0, 0.0, 0.0, 0.25], crate::scene::paint::Cap::Flat);
                }
            } else {
                ctx.rounded_rect(run_rect, r, (true, true, true, true), self.bg_color());
                for (a, b) in self.seams(rect) {
                    ctx.vector(a.0, a.1, b.0, b.1, 1.0, [0.0, 0.0, 0.0, 0.25], crate::scene::paint::Cap::Flat);
                }
            }
        }
        // Hover wash: the WHOLE segment silhouette — flush to the slanted
        // seams, and around the run's rounded end arcs on the first/last
        // segment. No sheared primitive exists, so the wash is BANDED: one
        // thin quad per logical pixel row, each row's edges sampled from the
        // same seam-lean and corner-arc math the seams and run box use.
        // ~24 plain Quads, hover-only — and Quads survive the flat hosts'
        // rounded-quad bridge, so cce-files' mirror gets the same shape.
        // The hover wash, and the keyboard cursor's wash in the highlight while
        // the run holds focus — the same banded silhouette.
        let mut washes: Vec<(usize, [f32; 4])> = Vec::new();
        if let Some(h) = self.hovered_seg {
            washes.push((h, [1.0, 1.0, 1.0, 0.06]));
        }
        if let (true, Some(f)) = (self.focused, self.focus_seg) {
            let t = crate::widget::ControlPlate::focus_tint();
            washes.push((f, [t[0], t[1], t[2], 0.18]));
        }
        for (hovered, wash) in washes {
            if let (Some((sx0, sw)), Some((rx, ry, rw, rh))) = (
                segs.iter().find(|s| s.logical == Some(hovered)).map(|s| (s.x, s.w)),
                self.run_box(rect),
            ) {
                let (hy, hh) = Self::plate_band(rect);
                let run = SEG_SLANT * hh * 0.5;
                let first = segs.first().map(|s| s.x) == Some(sx0);
                let last = segs.last().map(|s| s.x + s.w) == Some(sx0 + sw);
                let rr = crate::layout::dropdown_corner_radius().min(rh * 0.5);
                let n = crate::layout::corner_shape();
                let arc = |yc: f32| Self::end_inset(rr, n, ry, rh, yc);
                let mut y = hy;
                while y < hy + hh {
                    let bh = 1.0f32.min(hy + hh - y);
                    let yc = y + bh * 0.5;
                    let t = ((yc - hy) / hh).clamp(0.0, 1.0);
                    let lean = run * (1.0 - 2.0 * t);
                    let l = if first { rx + arc(yc) } else { sx0 + lean };
                    let r_edge = if last { rx + rw - arc(yc) } else { sx0 + sw + lean };
                    if r_edge > l {
                        ctx.quad(Rect { x: l, y, width: r_edge - l, height: bh }, wash);
                    }
                    y += bh;
                }
            }
        }

        // The current directory (last logical segment) is drawn brightly; everything else,
        // including the "…" ellipsis marker, is dimmed. Paint at the configured font size so
        // the glyph run matches the widths `visible_segs` measured (and thus its x positions).
        let (_, size) = Self::font_and_size();
        let last_logical = self.virtual_segs().len().saturating_sub(1);
        for vs in segs {
            if vs.logical.is_none() {
                // The marker for the segments folded away: the
                // `more-horizontal` glyph in the dimmed colour, centred in
                // its box.
                let side = Self::marker_side(size);
                let g = Rect { x: vs.x + SEG_PAD_X, y: rect.y + 0.5 * (rect.height - side), width: side, height: side };
                ctx.icon("more-horizontal", g, [0x88 as f32 / 255.0, 0x88 as f32 / 255.0, 0x99 as f32 / 255.0, 1.0]);
                continue;
            }
            let color =
                if vs.logical == Some(last_logical) { [0xcc, 0xcc, 0xd4] } else { [0x88, 0x88, 0x99] };
            // `visible_segs` already drops segments behind a "…" to make the
            // run fit, but the surviving tail is still measured text against a
            // fixed bar — bound it so a mismeasure cannot escape the widget.
            ctx.text_with(
                vs.text,
                vs.x + SEG_PAD_X,
                crate::layout::center_text_y(rect.y, rect.height, size),
                size,
                color,
                None,
                Some([rect.x, rect.y, rect.x + rect.width, rect.y + rect.height]),
            );
        }
    }
}
