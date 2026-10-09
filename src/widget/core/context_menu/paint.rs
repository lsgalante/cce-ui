//! Painting: the menu's plate, its rows (and both sets mid-turn), slider rows, labels and the
//! row glyphs.

use super::*;

/// A glyph a menu draws: name, top-left, side, colour.
#[derive(Debug, Clone)]
pub(crate) struct MenuGlyph {
    pub(crate) name: &'static str,
    x: f32,
    y: f32,
    side: f32,
    color: [f32; 4],
}

/// Draw `g` (shifted by `dx`, at `alpha`): the glyph tinted to the
/// label colour beside it — `PaintCtx::icon`.
pub(super) fn paint_glyph(ctx: &mut crate::scene::paint::PaintCtx, g: &MenuGlyph, dx: f32, alpha: f32) {
    let r = crate::scene::layout::Rect { x: g.x + dx, y: g.y, width: g.side, height: g.side };
    let [cr, cg, cb, ca] = g.color;
    ctx.icon(g.name, r, [cr, cg, cb, ca * alpha]);
}

/// A toolkit color as the `[u8; 3]` a [`TextLabel`] carries.
pub(super) fn rgb8(c: [f32; 4]) -> [u8; 3] {
    [
        (c[0] * 255.0).round().clamp(0.0, 255.0) as u8,
        (c[1] * 255.0).round().clamp(0.0, 255.0) as u8,
        (c[2] * 255.0).round().clamp(0.0, 255.0) as u8,
    ]
}

/// The menu PLATE alone, over `rect`: [`Material::menu`] on a rounded
/// face at `style.surface.menu.corner_radius` with the rolled perimeter
/// at the relief width (capped at a fifth of the height) — or, for a
/// transparent configured face, the edges-only boss. What
/// [`ContextMenuState::paint`] draws under its rows, and what any other
/// surface that should look like a menu draws under its own (the
/// designer's command palette): one function, so the two cannot be
/// configured apart.
///
/// `in_popup` says the plate is being painted into the runner's popup
/// surface, where the compositor's blur frosts but cannot COMPRESS: the
/// in-app pass pulls the backdrop's luminance a fraction `k` toward the
/// plate's key, which is what keeps the labels legible over a bright
/// scene. Over glass that only blurs, the same swing is held with
/// opacity instead — the backdrop reaches the eye at (1 - a)(1 - k)
/// either way — or a menu opened over something white washes out.
///
/// [`Material::menu`]: crate::scene::material::Material::menu
pub fn paint_menu_plate(ctx: &mut crate::scene::paint::PaintCtx, rect: crate::scene::layout::Rect, in_popup: bool) {
    let r = crate::layout::menu_corner_radius();
    let depth = crate::layout::bevel_width().min(rect.height * 0.2);
    let face = crate::color::menu_color();
    if face[3] > 0.001 {
        let material = crate::scene::material::Material::menu();
        let material = if in_popup {
            let k = crate::color::menu_compression().clamp(0.0, 1.0);
            let mut m = material.for_role(crate::scene::material::PlateRole::Root);
            m.tint[3] = 1.0 - (1.0 - m.tint[3]) * (1.0 - k);
            m
        } else {
            material
        };
        ctx.plate(rect, (r, r, r, r), &material, depth);
    } else {
        let (plateau, radii) = crate::layout::carve_inside(rect, (r, r, r, r), depth);
        ctx.boss(plateau, radii, depth);
    }
}

impl ContextMenuState {

    /// Paint the menu as a lit plate: a rounded face in the DE's plate color,
    /// translucent and frosted (the negative-alpha blur-behind sentinel), with
    /// the rolled perimeter — the material every other floating surface in the
    /// DE wears. Hosts on the display-list path call this INSTEAD of iterating
    /// [`ContextMenuState::extra_quads`], then draw [`text_labels`] over it.
    ///
    /// A transparent configured plate color degrades to the edges-only boss,
    /// as the breadcrumb's raised run does: with no face to tint, a plate
    /// would paint a hole.
    ///
    /// [`text_labels`]: ContextMenuState::text_labels
    pub fn paint(&self, ctx: &mut crate::scene::paint::PaintCtx) {
        if !self.visible || self.hosted {
            return;
        }
        let r = crate::layout::menu_corner_radius();
        if let (Some(e), Some(t)) = (self.turn_progress(), self.turning.as_ref()) {
            // Turning: the plate on its way between the two sizes; the
            // rows it turned from — sliders, separators — sliding away
            // and fading, the page's sliding in from the side the turn
            // comes from and coming up. Each set is drawn aside and
            // replayed moved and faded (`Prim::faded`); their labels
            // are `paint_with_labels`', at the same strengths.
            let rect = self.drawn_rect();
            let depth = crate::layout::bevel_width().min(rect.height * 0.2);
            paint_menu_plate(ctx, rect, self.in_popup);
            let (out, into) = turn_fades(e);
            let (lo, hi) = (-t.dir * e * TURN_SLIDE, t.dir * (1.0 - e) * TURN_SLIDE);
            ctx.clip_rounded(rect, r, |ctx| {
                for (rows, dx, alpha) in [(&*t.from, lo, out), (self, hi, into)] {
                    let mut aside = crate::scene::paint::PaintCtx::new();
                    rows.paint_rows(&mut aside, rect, r, depth);
                    ctx.translate(dx, 0.0, |ctx| {
                        for item in aside.finish().items {
                            if let Some(c) = item.clip {
                                ctx.push_clip(c);
                            }
                            // The rows draw no text; a label would be
                            // doubled with `paint_with_labels`'.
                            let _ = ctx.replay(item.prim.faded(alpha));
                            if item.clip.is_some() {
                                ctx.pop_clip();
                            }
                        }
                    });
                }
            });
            return;
        }
        let rect = crate::scene::layout::Rect {
            x: self.x,
            y: self.y,
            width: self.w,
            height: self.h,
        };
        let depth = crate::layout::bevel_width().min(self.h * 0.2);
        paint_menu_plate(ctx, rect, self.in_popup);

        // What the rows draw — hover, separators, slider bands — is cut at
        // the plate, so a row scrolled half out of a shortened menu stops
        // at its edge instead of hanging off it.
        ctx.clip_rounded(rect, r, |ctx| self.paint_rows(ctx, rect, r, depth));
        if self.max_scroll() > 0.0 {
            // A scrolled menu says so: a thumb in the right padding, as
            // long against the plate as the view is against the rows.
            let track = (rect.y + PAD, rect.height - 2.0 * PAD);
            let len = (track.1 * self.h / self.content_h).max(12.0).min(track.1);
            let at = track.0 + (track.1 - len) * (self.scroll / self.max_scroll());
            let tw = 3.0;
            let c = crate::color::TEXT_DIM;
            ctx.rounded_rect(
                crate::scene::layout::Rect { x: rect.x + rect.width - PAD * 0.5 - tw * 0.5, y: at, width: tw, height: len },
                tw * 0.5,
                (true, true, true, true),
                [c[0], c[1], c[2], 0.6],
            );
        }
    }

    pub(super) fn paint_rows(&self, ctx: &mut crate::scene::paint::PaintCtx, rect: crate::scene::layout::Rect, r: f32, depth: f32) {
        if self.back.is_some() {
            // The back band: lit like a row under the pointer, and cut off
            // from the page's rows by the separator's groove.
            let top = self.back_band_y();
            if self.back_hovered {
                let inset = (depth * 0.5).max(2.0).max(PAD * 0.5);
                ctx.rounded_rect(
                    crate::scene::layout::Rect { x: self.x + inset, y: top + 2.0, width: self.w - 2.0 * inset, height: ROW_H - 4.0 },
                    (r - inset).max(0.0),
                    (true, true, true, true),
                    [0.20, 0.40, 0.65, 0.6],
                );
            }
            let inset = (depth * 0.5).max(PAD);
            let cy = top + ROW_H;
            ctx.groove((self.x + inset, cy), (self.x + self.w - inset, cy), 0.75, depth, rect);
        }
        if let Some(h_idx) = self.hovered_item {
            // Inset off the roll so the fill sits on the face instead of
            // climbing the lit edge, and round the corners it actually meets:
            // the first and last rows touch the plate's, and a header row is
            // never hovered, so the top pair only rounds when there is no
            // header above.
            // Inside the padding on every side — the rows no longer
            // touch the plate's edge, so the fill is its own rounded
            // tablet on the face rather than a band that meets the roll.
            let iy = self.row_y(h_idx);
            let inset = (depth * 0.5).max(2.0).max(PAD * 0.5);
            ctx.rounded_rect(
                crate::scene::layout::Rect {
                    x: self.x + inset,
                    y: iy + 2.0,
                    width: self.w - 2.0 * inset,
                    height: ROW_H - 4.0,
                },
                (r - inset).max(0.0),
                (true, true, true, true),
                [0.20, 0.40, 0.65, 0.6],
            );
        }

        // Separator rows ("-"): an engraved line across the face at the
        // row's vertical centre — the breadcrumb seam's language, cut
        // into the menu plate instead of a printed dash.
        for (idx, opt) in self.options.iter().enumerate() {
            if opt == "-" {
                let cy = self.row_y(idx) + ROW_H * 0.5;
                let inset = (depth * 0.5).max(PAD);
                ctx.groove(
                    (self.x + inset, cy),
                    (self.x + self.w - inset, cy),
                    0.75,
                    depth,
                    rect,
                );
            }
        }
        self.paint_sliders(ctx);
    }

    /// The slider rows' bands — the toolkit's own `Slider`, one stamp set
    /// to each row's range and value, so a slider in a menu is the slider
    /// everywhere else. Its readout is off: the menu draws the readout as
    /// a label, so it wears the menu font and clears the popover clamp.
    pub(super) fn paint_sliders(&self, ctx: &mut crate::scene::paint::PaintCtx) {
        for idx in 0..self.options.len() {
            let Some(s) = self.slider(idx) else { continue };
            let mut stamp = Slider::new().with_readout(false);
            stamp.set_scroll(false);
            stamp.set_range(s.min, s.max);
            stamp.set_scaled_value(s.value);
            crate::widget::model::Paint::paint(&*stamp, self.slider_band(idx), ctx);
        }
    }

    /// The flat-quad menu: a 1px border rect, a near-black fill and the hover
    /// row. Superseded by [`ContextMenuState::paint`], which draws the menu as
    /// the lit plate the rest of the DE's floating surfaces wear; this stays
    /// for hosts that have not migrated, and renders as it always has.
    pub fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        if !self.visible || self.hosted { return quads; }

        // border
        quads.push((self.x, self.y, self.w, self.h, [0.22, 0.22, 0.28, 1.0]));
        // bg
        quads.push((self.x + 1.0, self.y + 1.0, self.w - 2.0, self.h - 2.0, [0.06, 0.06, 0.09, 1.0]));

        if let Some(h_idx) = self.hovered_item {
            let iy = self.row_y(h_idx);
            quads.push((self.x + PAD * 0.5, iy + 2.0, self.w - PAD, ROW_H - 4.0, [0.20, 0.40, 0.65, 0.6]));
        }

        // Separator rows ("-"): a hairline in place of the engraved
        // groove the plate path cuts.
        for (idx, opt) in self.options.iter().enumerate() {
            if opt == "-" {
                let cy = self.row_y(idx) + ROW_H * 0.5;
                quads.push((self.x + PAD, cy, self.w - 2.0 * PAD, 1.0, [0.22, 0.22, 0.28, 1.0]));
            }
        }

        quads
    }

    /// [`paint`](Self::paint) plus the label run, in the menu font.
    ///
    /// A [`TextLabel`] carries a size but no family, so a consumer that
    /// hand-rolls `paint()` + a `text_labels()` loop has to remember to
    /// pass [`label_font`]'s family itself — and every one of them passed
    /// `None`, which is why menus rendered in the default sans over lists
    /// wearing the configured face. This is the call that cannot forget
    /// it; prefer it over the pair.
    pub fn paint_with_labels(&self, ctx: &mut crate::scene::paint::PaintCtx) {
        self.paint(ctx);
        if !self.visible || self.hosted {
            return;
        }
        let (family, _) = label_font();
        if let (Some(e), Some(t)) = (self.turn_progress(), self.turning.as_ref()) {
            // The labels of both, cut at the plate as it is drawn: the
            // ones it turned from sliding away and fading, the page's
            // sliding in and coming up.
            let rect = self.drawn_rect();
            let bounds = Some([rect.x, rect.y, rect.x + rect.width, rect.y + rect.height]);
            let (lo, hi) = (-t.dir * e * TURN_SLIDE, t.dir * (1.0 - e) * TURN_SLIDE);
            let (out, into) = turn_fades(e);
            for (rows, dx, alpha) in [(&*t.from, lo, out), (self, hi, into)] {
                for label in rows.labels() {
                    ctx.text_faded(label.text, label.x + dx, label.y, label.font_size, label.color, alpha, Some(family.clone()), bounds);
                }
                ctx.clip(rect, |ctx| {
                    for g in rows.glyphs() {
                        paint_glyph(ctx, &g, dx, alpha);
                    }
                });
            }
            return;
        }
        // The menu's own rect: the engine's popover clamp exempts exactly
        // these bounds, so the labels render inside the plate instead of
        // being clipped to the page content beneath it.
        let bounds = Some([self.x, self.y, self.x + self.w, self.y + self.h]);
        for label in self.text_labels() {
            ctx.text_with(
                label.text,
                label.x,
                label.y,
                label.font_size,
                label.color,
                Some(family.clone()),
                bounds,
            );
        }
        let plate = crate::scene::layout::Rect { x: self.x, y: self.y, width: self.w, height: self.h };
        ctx.clip(plate, |ctx| {
            for g in self.glyphs() {
                paint_glyph(ctx, &g, 0.0, 1.0);
            }
        });
    }

    pub fn text_labels(&self) -> Vec<TextLabel> {
        if !self.visible || self.hosted {
            return Vec::new();
        }
        self.labels()
    }

    /// The labels as the rows stand, shown or not.
    pub(crate) fn labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();

        if let Some(title) = &self.back {
            let (_, label_size) = label_font();
            labels.push(TextLabel {
                text: title.clone(),
                x: self.x + PAD + chevron_size(label_size) + MARK_GAP,
                y: self.back_band_y() + (ROW_H - label_size) / 2.0,
                font_size: label_size,
                color: rgb8(if self.back_hovered { crate::color::TEXT_HEADER } else { crate::color::TEXT_DIM }),
            });
        }
        for (idx, opt) in self.options.iter().enumerate() {
            if opt == "-" {
                continue;
            }
            // Scrolled wholly out of a shortened menu: nothing to draw.
            // A row partly in view is drawn and cut at the plate by the
            // label's bounds.
            let top = self.row_y(idx);
            if top + ROW_H < self.y || top > self.y + self.h {
                continue;
            }
            let (_, label_size) = label_font();
            let iy = self.row_y(idx) + (ROW_H - label_size) / 2.0;
            // The toolkit's semantic colors rather than greys hand-mixed
            // against the old near-black fill: on the plate's mid-slate the
            // header's 0x70 was a step above its background and read as
            // nothing.
            let text_color = if idx < self.header_count {
                rgb8(crate::color::TEXT_DIM)
            } else if self.hovered_item == Some(idx) {
                rgb8(crate::color::TEXT_HEADER)
            } else {
                rgb8(crate::color::TEXT_FG)
            };

            // A leading mark is drawn as a glyph (see `glyphs`); the
            // label follows it.
            let (mark, text) = split_mark(opt);
            let mark_w = if mark.is_some() { mark_size(label_size) + MARK_GAP } else { 0.0 };
            labels.push(TextLabel {
                text: text.to_string(),
                x: self.x + PAD + mark_w,
                y: iy,
                font_size: label_size,
                color: text_color,
            });
            // A slider row's readout, right-aligned against its band.
            if let Some(s) = self.slider(idx) {
                let (family, _) = label_font();
                let text = s.readout();
                let tw = crate::widget::display::measure_text_width(&text, &family, label_size);
                labels.push(TextLabel {
                    text,
                    x: self.slider_band(idx).x - SLIDER_GAP - tw,
                    y: iy,
                    font_size: label_size,
                    color: text_color,
                });
            }
        }
        labels
    }

    /// The glyphs the rows wear, as they stand: each row's leading mark
    /// (see [`MARK_CHECK`]), the chevron at the right end of a row that
    /// leads to a page, and the back band's chevron. Each takes its
    /// row's text colour.
    pub(crate) fn glyphs(&self) -> Vec<MenuGlyph> {
        let mut glyphs = Vec::new();
        let (_, size) = label_font();
        let (ms, cs) = (mark_size(size), chevron_size(size));
        if self.back.is_some() {
            glyphs.push(MenuGlyph {
                name: "chevron-left",
                x: self.x + PAD,
                y: self.back_band_y() + (ROW_H - cs) / 2.0,
                side: cs,
                color: if self.back_hovered { crate::color::TEXT_HEADER } else { crate::color::TEXT_DIM },
            });
        }
        for (idx, opt) in self.options.iter().enumerate() {
            if opt == "-" {
                continue;
            }
            let top = self.row_y(idx);
            if top + ROW_H < self.y || top > self.y + self.h {
                continue;
            }
            let color = if idx < self.header_count {
                crate::color::TEXT_DIM
            } else if self.hovered_item == Some(idx) {
                crate::color::TEXT_HEADER
            } else {
                crate::color::TEXT_FG
            };
            if let (Some(name), _) = split_mark(opt) {
                glyphs.push(MenuGlyph { name, x: self.x + PAD, y: top + (ROW_H - ms) / 2.0, side: ms, color });
            }
            if self.leads_to_page(idx) {
                glyphs.push(MenuGlyph {
                    name: "chevron-right",
                    x: self.x + self.w - PAD - cs,
                    y: top + (ROW_H - cs) / 2.0,
                    side: cs,
                    color,
                });
            }
        }
        glyphs
    }
}
