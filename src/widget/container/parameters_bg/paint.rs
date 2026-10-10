//! What the pane draws: row floors, section outlines and their arcs, the scrollbar, the ramps and
//! trackballs a host paints through the scene path, and `impl Paint`.

use super::*;

impl ParametersBg {
    /// Each section's boxes: the title box, plus the box wrapping its rows (`None` when the
    /// section is collapsed or has no rows). The shared source for the outline's straight
    /// runs and its corner fillets, so the two halves can't disagree.
    /// The row floors (`layout::param_compression`): the pane material
    /// frosted at the configured compression, filled under every visible
    /// parameter row (headers excepted) before anything else in the pane
    /// paints, at the control corner radius — each parameter on its own
    /// tablet, the way the designer's node bodies get their own compression.
    /// Nothing when the key is unset. Relief only: the flat style has no
    /// floor language.
    pub(super) fn paint_row_floors(&self, ctx: &mut PaintCtx) {
        let Some(k) = crate::layout::param_compression() else { return };
        if !crate::layout::control_relief() {
            return;
        }
        let mut mat = crate::scene::Material::pane();
        if let crate::scene::Frost::Frosted { compression, .. } = &mut mat.frost {
            *compression = k;
        }
        let r = crate::layout::control_corner_radius();
        let hidden = self.hidden_rows();
        for (i, (x, y, w, h)) in self.get_param_rects().into_iter().enumerate() {
            if hidden[i] || h <= 0.0 || self.display_params[i].2 == "section" || self.display_params[i].2 == SEPARATOR {
                continue;
            }
            ctx.fill_material(Rect { x, y, width: w, height: h }, (r, r, r, r), &mat);
        }
    }

    /// One section's outline: a SINGLE continuous border shaped like a folder tab — around
    /// the title box, whose bottom edge is open onto the content body it sits flush on
    /// (its right side turning onto the body's top edge through the concave throat fillet,
    /// its left side running straight down into the body's left edge) — as
    /// `(straight runs, corner fillets)`.
    ///
    /// Runs are `(x, y, w, h)`; fillets are `(cx, cy, radius, start, end)` for an arc stroked
    /// `SECTION_BORDER_T` inward of `radius` (the renderer's convention). Convex corners take
    /// the stroke inside the box, so their radius is the outer one; the concave throat corner
    /// has its centre out in the empty pocket, so its carries the `+ T` that puts the
    /// ink on the far side. Every run stops a radius short of its corner, and each fillet
    /// picks it up there — the path closes.
    pub(super) fn section_outline(
        &self,
        title: (f32, f32, f32, f32),
        content: Option<(f32, f32, f32, f32)>,
    ) -> (Vec<(f32, f32, f32, f32)>, Vec<(f32, f32, f32, f32, f32)>) {
        use std::f32::consts::{PI, TAU};
        const Q: f32 = std::f32::consts::FRAC_PI_2;
        let (t, r) = (SECTION_BORDER_T, SECTION_R);
        let mut quads: Vec<(f32, f32, f32, f32)> = Vec::new();
        let mut arcs: Vec<(f32, f32, f32, f32, f32)> = Vec::new();
        fn hrun(out: &mut Vec<(f32, f32, f32, f32)>, x0: f32, x1: f32, y: f32) {
            if x1 - x0 > 0.01 {
                out.push((x0, y, x1 - x0, SECTION_BORDER_T));
            }
        }
        fn vrun(out: &mut Vec<(f32, f32, f32, f32)>, y0: f32, y1: f32, x: f32) {
            if y1 - y0 > 0.01 {
                out.push((x, y0, SECTION_BORDER_T, y1 - y0));
            }
        }

        let (tx, ty, tw, th) = title;
        let ty_b = ty + th; // the title box's bottom edge
        arcs.push((tx + r, ty + r, r, PI, PI + Q)); // title top-left
        arcs.push((tx + tw - r, ty + r, r, PI + Q, TAU)); // title top-right
        hrun(&mut quads, tx + r, tx + tw - r, ty); // title top

        let Some((cx, cy_t, cw, ch)) = content else {
            // Collapsed or empty: the title box IS the section, so it closes on itself.
            arcs.push((tx + tw - r, ty_b - r, r, 0.0, Q)); // title bottom-right
            arcs.push((tx + r, ty_b - r, r, Q, PI)); // title bottom-left
            vrun(&mut quads, ty + r, ty_b - r, tx + tw - t); // title right
            vrun(&mut quads, ty + r, ty_b - r, tx); // title left
            hrun(&mut quads, tx + r, tx + tw - r, ty_b - t); // title bottom
            return (quads, arcs);
        };

        // The tab sits flush on the body (`ty_b == cy_t` — `section_title_box` places it
        // there): its bottom edge is open. The throat fillet shrinks if the tab runs
        // close to the body's right corner.
        let f = SECTION_THROAT_R.min((cx + cw - r - (tx + tw)).max(0.0));
        vrun(&mut quads, ty + r, cy_t - f, tx + tw - t); // tab right side, down to the throat
        arcs.push((tx + tw + f, cy_t - f, f + t, Q, PI)); // throat: tab side -> body top
        hrun(&mut quads, tx + tw + f, cx + cw - r, cy_t); // body top, right of the tab

        arcs.push((cx + cw - r, cy_t + r, r, PI + Q, TAU)); // body top-right
        arcs.push((cx + cw - r, cy_t + ch - r, r, 0.0, Q)); // body bottom-right
        arcs.push((cx + r, cy_t + ch - r, r, Q, PI)); // body bottom-left
        vrun(&mut quads, cy_t + r, cy_t + ch - r, cx + cw - t); // body right
        hrun(&mut quads, cx + r, cx + cw - r, cy_t + ch - t); // body bottom
        vrun(&mut quads, ty + r, cy_t + ch - r, tx); // tab + body left, one straight run

        (quads, arcs)
    }

    /// The section outlines' corner fillets, as the renderer's arc tuples
    /// `(cx, cy, radius, thickness, start, end, color)`.
    ///
    /// A concrete accessor rather than prims out of [`Paint::paint`] on purpose: the host
    /// draws this panel through the legacy plain-quad hatch, and `append_widget_plate` serves
    /// a widget's arcs UNCLIPPED — these have to land inside the pane's scroll viewport, so
    /// the host draws them itself under its own clip (the straight runs ride `plain_quads`,
    /// which clips them there by hand).
    pub fn arcs(&self) -> Vec<(f32, f32, f32, f32, f32, f32, [f32; 4])> {
        if !self.visible || crate::layout::control_relief() {
            return Vec::new();
        }
        let color = SECTION_BORDER_COLOR;
        let mut out = Vec::new();
        for (title, content) in self.section_boxes() {
            let (_, arcs) = self.section_outline(title, content);
            out.extend(
                arcs.into_iter()
                    .map(|(cx, cy, r, a0, a1)| (cx, cy, r, SECTION_BORDER_T, a0, a1, color)),
            );
        }
        out
    }

    /// The scrollbar's track + thumb quads (empty when no scrollbar is needed). The host draws
    /// these either behind or in front of the pane plate per [`Self::scrollbar_active`]; they
    /// are deliberately kept out of [`Self::plain_quads`] so the host controls their depth.
    pub fn scrollbar_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.scrollbar_visible() {
            return Vec::new();
        }
        let sb_w = self.scrollbar_w();
        let sb_x = self.scrollbar_x();
        let t = self.thumb();
        vec![
            (sb_x, t.track_y, sb_w, t.track_h, crate::color::scrollbar_track_color()),
            (sb_x, t.y_at(self.scroll_y), sb_w, t.h, crate::color::scrollbar_thumb_color()),
        ]
    }

    pub fn paint_scene_rows(&self, pc: &mut PaintCtx) {
        if !self.visible {
            return;
        }
        let hidden = self.hidden_rows();
        let dummy = UiContext::new();
        for (i, p) in self.display_params.iter().enumerate() {
            if hidden[i] {
                continue;
            }
            if p.2 == "ramp" {
                if let Some(rp) = &self.ramps[i] {
                    rp.paint_self(&dummy, pc);
                }
            } else if let Some(f) = self.float3s[i].as_ref().filter(|f| f.has_trackball()) {
                // A float3's trackball: a sphere is not a prim the flat
                // views carry, so it rides the scene path like the ramp.
                f.paint_ball(pc);
            }
        }
    }
}

impl Paint for ParametersBg {
    /// Shape the hosted controls (their carets read per-glyph advances nothing
    /// else records for children of a container) and one code column's advance
    /// from the same monospace@12 path the code rows draw through.
    fn prepare_text(&mut self, fs: &mut cosmic_text::FontSystem, _rect: Rect) {
        for tb in self.texts.iter_mut().flatten() {
            tb.prepare_text(fs);
        }
        for sb in self.spinboxes.iter_mut().flatten() {
            sb.prepare_text(fs);
        }
        for c in self.colors.iter_mut().flatten() {
            c.prepare_text(fs);
        }
        let clusters = crate::backend::text::shaped_cluster_offsets(
            fs,
            "MMMMMMMM",
            12.0,
            Some("monospace"),
        );
        if let Some(&(_, total)) = clusters.last() {
            if total > 0.0 {
                self.code_char_advance = total / 8.0;
            }
        }
    }

    /// The panel IS its own background plate (the host draws it from `color()` + the corner
    /// style via `append_widget_plate`) — there is no separate plate widget behind it, so
    /// this carries the full plate treatment: `PARAM_BG` scaled by the global plate opacity,
    /// with the alpha negated as the scenefx blur marker when plate blur is on. Transparent
    /// while hidden. It must not ALSO be emitted as a quad anywhere or it would double-blend.
    fn color(&self) -> [f32; 4] {
        if !self.visible {
            return [0.0, 0.0, 0.0, 0.0];
        }
        colors::param_plate_fill()
    }

    /// The shared plate corner radius (rounded on all four corners when non-zero).
    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        let r = crate::layout::plate_corner_radius();
        let on = r > 0.0;
        Some((r, (on, on, on, on)))
    }

    /// The shared plate border (folded in from the retired backing plate widget).
    fn solid_border(&self) -> Option<([f32; 4], f32)> {
        colors::plate_border_color().map(|bc| (bc, colors::plate_border_thickness()))
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::control_label_font())
    }

    /// The COMPLETE row chrome — everything the legacy hatches carry, in the hosts' canonical
    /// draw order: the controls' rounded wells, the flat-style section outline fillets, the
    /// relief steps (after the fills so the walls shade what they cross), the section carves'
    /// concave throat fillets, the slider-thumb spheres, the scene-path rows, the flat
    /// plain-quad chrome, then the hosted controls' glyphs (arrows, −/+) over it. What stays OUT, deliberately: the background plate (see
    /// [`color`](Paint::color)), the scrollbar (hosts place its depth — the designer straddles
    /// it around the pane plate), and text (`paint_self`'s own-labels bridge carries the
    /// per-row fonts and code-box bounds). Hosts clip this to their pane viewport — a rect
    /// clip pushed here would not survive `paint_self`'s replay.
    fn paint_ui(&self, _ui: &UiContext, _rect: Rect, ctx: &mut PaintCtx) {
        if !self.visible {
            return;
        }
        self.claim_typing(ctx);
        self.paint_row_floors(ctx);
        for (qx, qy, qw, qh, qr, qc, corners) in self.rounded_quads() {
            ctx.rounded_rect(Rect { x: qx, y: qy, width: qw, height: qh }, qr, corners, qc);
        }
        for (acx, acy, ar, at, a0, a1, ac) in self.arcs() {
            ctx.arc(acx, acy, ar, at, a0, a1, ac);
        }
        // The hovered row's carves are TINTED — the focus treatment's channel
        // with a neutral light instead of the accent: the rim's own light and
        // shadow, recoloured and gained, so the control under the pointer
        // reads lit while every other carve keeps the plate's grouped shading.
        // A tinted carve shades through the overlay path by design, and a
        // section's well spans many rows, so it never qualifies.
        let hover_rect = self.hover_row.and_then(|i| self.get_param_rects().get(i).copied()).filter(|r| r.3 > 0.0);
        let hovered = |x: f32, y: f32, w: f32, h: f32| {
            hover_rect.is_some_and(|(rx, ry, rw, rh)| {
                x >= rx - 0.5 && y >= ry - 0.5 && x + w <= rx + rw + 0.5 && y + h <= ry + rh + 0.5
            })
        };
        for (rx, ry, rw, rh, radii, rd, raised, edges) in self.reliefs() {
            let rect = Rect { x: rx, y: ry, width: rw, height: rh };
            match (raised, hovered(rx, ry, rw, rh)) {
                (true, true) => ctx.boss_edges_tinted(rect, radii, rd, edges, Self::HOVER_TINT),
                (true, false) => ctx.boss_edges(rect, radii, rd, edges),
                (false, true) => ctx.recess_edges_tinted(rect, radii, rd, edges, Self::HOVER_TINT),
                (false, false) => ctx.recess_edges(rect, radii, rd, edges),
            }
        }
        for field in self.fields() {
            let r = field.rect;
            let tint = hovered(r.x, r.y, r.width, r.height).then_some(Self::HOVER_TINT);
            ctx.field(&field.with_tint(tint));
        }
        for (ga, gb, gw, gd, ghost) in self.grooves() {
            ctx.groove(ga, gb, gw, gd, ghost);
        }
        for (fcx, fcy, fr, fd, fs) in self.section_fillets() {
            ctx.concave_fillet(fcx, fcy, fr, fd, fs, false);
        }
        for (scx, scy, sr, sc) in self.spheres() {
            ctx.sphere(scx, scy, sr, &crate::scene::material::Material::from_fill(sc));
        }
        self.paint_scene_rows(ctx);
        for (qx, qy, qw, qh, qc) in self.plain_quads() {
            ctx.quad(Rect { x: qx, y: qy, width: qw, height: qh }, qc);
        }
        self.paint_child_glyphs(ctx);
    }

    /// Ui-less emission (the [`paint_ui`](Paint::paint_ui) override above is what `paint_self`
    /// runs): the flat subset plus the scrollbar, kept for direct callers only. The background
    /// plate stays out — see [`color`](Paint::color).
    fn paint(&self, _rect: Rect, ctx: &mut PaintCtx) {
        self.claim_typing(ctx);
        for (qx, qy, qw, qh, qc) in self.plain_quads() {
            ctx.quad(Rect { x: qx, y: qy, width: qw, height: qh }, qc);
        }
        self.paint_scene_rows(ctx);
        self.paint_child_glyphs(ctx);
        // Scene-path hosts get the scrollbar on top (the designer instead straddles it around
        // the pane plate through `scrollbar_quads`).
        for (qx, qy, qw, qh, qc) in self.scrollbar_quads() {
            ctx.quad(Rect { x: qx, y: qy, width: qw, height: qh }, qc);
        }
        let view_min = self.rect.y + 4.0;
        let view_max = self.rect.y + self.rect.height - 4.0;
        // The y test above culls the scrolled-away rows; it is the pane's
        // WIDTH that nothing enforced, so a long parameter name ran out of the
        // pane sideways.
        let pane = Some([
            self.rect.x,
            self.rect.y,
            self.rect.x + self.rect.width,
            self.rect.y + self.rect.height,
        ]);
        for l in self.own_text_labels() {
            if l.y >= view_min - 20.0 && l.y <= view_max + 20.0 {
                ctx.text_with(l.text, l.x, l.y, l.font_size, l.color, None, pane);
            }
        }
    }

    fn serves_legacy_labels(&self) -> bool {
        true
    }

    /// The legacy `text_labels_with_font_and_bounds` body: every label clipped to the panel
    /// viewport in the control-label font, except labels inside a code row — those clip to the
    /// code box (or hide when it's scrolled out) and render monospace.
    fn legacy_labels_with_font_and_bounds(&self, _rect: Rect, _ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        let view_min = self.rect.y + 4.0;
        let view_max = self.rect.y + self.rect.height - 4.0;
        let mut result = Vec::new();
        let font = Paint::widget_font(self);
        let rects = self.get_param_rects();
        for l in self.own_text_labels() {
            if l.y < view_min - 20.0 || l.y > view_max + 20.0 {
                continue;
            }
            let mut bounds = Some([self.rect.x + 4.0, view_min, self.rect.x + self.rect.width - 4.0, view_max]);
            let mut label_font = font.clone();
            for (i, p) in self.display_params.iter().enumerate() {
                if p.2 == "code" {
                    let r = rects[i];
                    if l.y >= r.1 + 18.0 && l.y <= r.1 + r.3 {
                        let code_min = (r.1 + 19.0).max(view_min);
                        let code_max = (r.1 + r.3 - 1.0).min(view_max);
                        if code_min < code_max {
                            bounds = Some([r.0 + 1.0, code_min, r.0 + r.2 - 1.0, code_max]);
                        } else {
                            bounds = Some([0.0, 0.0, 0.0, 0.0]); // hidden
                        }
                        label_font = Some("monospace".to_string());
                        break;
                    }
                }
            }
            result.push((l, label_font, bounds));
        }
        result
    }

    fn popover(&self, _rect: Rect) -> Option<(f32, f32, f32, f32)> {
        self.choices_popover_rect()
    }

    /// The dropdown rows' popovers; the raw children's are the adapter's recursion.
    fn draw_popover(&self, _rect: Rect, pc: &mut dyn crate::layout::RenderTarget) {
        for d in self.choices.iter().flatten() {
            d.render_popover(pc);
        }
        for rp in self.ramps.iter().flatten() {
            rp.inner().preset_dropdown.render_popover(pc);
            rp.inner().line_type_dropdown.render_popover(pc);
        }
    }
}
