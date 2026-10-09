//! What the pane draws: row floors, section outlines and fillets, its controls' chrome
//! (quads, reliefs, fields, grooves), their text and glyphs, and `impl Paint`.

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
        let sb_track_h = self.rect.height - 8.0;
        let sb_track_y = self.rect.y + 4.0;

        let visible_ratio = self.rect.height / self.content_h;
        let thumb_h = if sb_track_h <= 20.0 {
            sb_track_h
        } else {
            (sb_track_h * visible_ratio).clamp(20.0, sb_track_h)
        };
        let max_scroll = self.content_h - self.rect.height;
        let scroll_ratio = if max_scroll > 0.0 { self.scroll_y / max_scroll } else { 0.0 };
        let thumb_y = sb_track_y + scroll_ratio * (sb_track_h - thumb_h);

        vec![
            (sb_x, sb_track_y, sb_w, sb_track_h, crate::color::scrollbar_track_color()),
            (sb_x, thumb_y, sb_w, thumb_h, crate::color::scrollbar_thumb_color()),
        ]
    }

    pub(super) fn own_text_labels(&self) -> Vec<TextLabel> {
        let rects = self.get_param_rects();
        let hidden = self.hidden_rows();
        let mut labels = Vec::new();
        let lw = self.label_col_w();
        let (family, size) = Self::inline_label_font();
        for (i, (name, value, ptype)) in self.display_params.iter().enumerate() {
            if hidden[i] {
                continue;
            }
            let r = rects[i];
            if self.inline_row(i) {
                // The pane's own label, in the column beside an unlabelled
                // control: vertically centred on the row, tail-truncated to
                // the column — by the measure the column is sized with, so
                // a label it was sized to hold is never cut (it was cut to
                // as many chars as the column holds M's).
                labels.push(TextLabel {
                    text: fit_tail(name, lw - Self::LABEL_GAP, &family, size),
                    x: r.0,
                    y: r.1 + (r.3 - size) * 0.5,
                    font_size: size,
                    color: if self.hover_row == Some(i) { Self::LABEL_HOVER } else { Self::LABEL },
                });
            }
            if ptype.starts_with("slider") {
                if let Some(s) = &self.sliders[i] {
                    labels.extend(s.own_text_labels());
                }
            } else if is_vec_row(ptype) {
                if let Some(f) = &self.float3s[i] {
                    labels.extend(f.own_text_labels());
                }
            } else if ptype == "section" {
                // Centered within the tab itself — `section_title_box` is the one
                // source for where the tab sits (flush on the body; collapsed and
                // empty sections carry their own placements there).
                let font_size = 13.0;
                let (bx, by, _, _) = self.section_title_box(i, r);
                labels.push(TextLabel {
                    text: name.clone(),
                    // 8px in from the title box's left edge — the inset
                    // `section_title_box`'s width math centers against.
                    x: bx + 8.0,
                    y: by + (TITLE_BOX_H - font_size) / 2.0,
                    font_size,
                    color: [0xee, 0xee, 0xf0],
                });
            } else if ptype == "code" {
                labels.push(TextLabel {
                    text: format!("{}:", name),
                    x: self.rect.x + 12.0,
                    y: r.1,
                    font_size: 12.0,
                    color: [0xaa, 0xaa, 0xbb],
                });
                let val_text = if self.focused_param == Some(i) {
                    if let Some(ref editor) = self.code_editor {
                        editor.buffer.clone()
                    } else {
                        value.clone()
                    }
                } else {
                    value.clone()
                };
                // One label PER LINE, at the same pitch the cursor math uses
                // (`plain_quads`' cursor_y) — a single multi-line label would depend on
                // the consumer's buffer line-height matching that pitch, and never
                // exactly did. A line number sits in the gutter before each.
                let text_x = self.code_text_x(r);
                for (line_i, line) in val_text.split('\n').enumerate() {
                    let y = r.1 + Self::CODE_TOP + line_i as f32 * Self::CODE_LINE_H;
                    let number = format!("{:>3}", line_i + 1);
                    labels.push(TextLabel {
                        text: number,
                        x: r.0 + 12.0,
                        y,
                        font_size: 12.0,
                        color: if self.code_error_line == Some(line_i) { [0xff, 0x80, 0x70] } else { [0x66, 0x66, 0x78] },
                    });
                    if line.is_empty() {
                        continue;
                    }
                    labels.push(TextLabel {
                        text: line.to_string(),
                        x: text_x,
                        y,
                        font_size: 12.0,
                        color: [0xee, 0xee, 0xf0],
                    });
                }
                if self.focused_param == Some(i) && self.code_is_dirty() {
                    labels.push(TextLabel {
                        text: "ctrl+enter applies".to_string(),
                        x: r.0 + r.2 - 12.0 - 18.0 * self.code_col_w(),
                        y: r.1 + r.3 - 15.0,
                        font_size: 12.0,
                        color: [0xd8, 0xa0, 0x50],
                    });
                }
            } else if ptype.starts_with("spinbox") {
                if let Some(sb) = &self.spinboxes[i] {
                    labels.extend(sb.own_text_labels());
                }
            } else if is_text_row(ptype) {
                if let Some(tb) = &self.texts[i] {
                    labels.extend(tb.own_text_labels());
                }
                if let Some(d) = &self.choices[i] {
                    labels.extend(d.own_text_labels());
                }
            } else if ptype.starts_with("choice") {
                if let Some(d) = &self.choices[i] {
                    labels.extend(d.own_text_labels());
                }
            } else if ptype == "button" {
                if let Some(b) = &self.buttons[i] {
                    labels.extend(b.own_text_labels());
                }
            } else if ptype == "toggle" || ptype == "checkbox" {
                if let Some(cb) = &self.toggles[i] {
                    labels.extend(cb.own_text_labels());
                }
            } else if ptype.starts_with("color") || ptype == "rgb" || ptype == "rgba" {
                if let Some(c) = &self.colors[i] {
                    labels.extend(c.own_text_labels());
                }
            } else if ptype == SEPARATOR {
                // A rule says nothing.
            } else if ptype == "ramp" {
                // The name label only — the ramp's own control labels ride its
                // scene-path paint (paint_scene_rows).
                labels.push(TextLabel {
                    text: name.clone(),
                    x: r.0,
                    y: r.1,
                    font_size: 12.0,
                    color: [0xaa, 0xaa, 0xbb],
                });
            } else {
                labels.push(TextLabel {
                    text: format!("{}: {}", name, value),
                    x: self.rect.x + ROW_X_INSET,
                    y: r.1,
                    font_size: 12.0,
                    color: [0xaa, 0xaa, 0xbb],
                });
            }
        }
        labels
    }

    /// The hosted controls' GLYPHS — a dropdown's or a picker's arrow, a
    /// spinbox's −/+ — as `(image, rect, alpha)`, row by row as
    /// [`own_text_labels`](Self::own_text_labels) collects their text. The
    /// pane paints its controls' chrome itself and never runs their
    /// `Paint::paint` into the frame, so a symbol a control draws reaches
    /// the pane only through here. Ramps are not asked: they paint whole
    /// through `paint_scene_rows`.
    pub(super) fn child_glyphs(&self) -> Vec<(u32, Rect, f32)> {
        let hidden = self.hidden_rows();
        let mut glyphs = Vec::new();
        for (i, (_, _, ptype)) in self.display_params.iter().enumerate() {
            if hidden[i] {
                continue;
            }
            if ptype.starts_with("slider") {
                if let Some(s) = &self.sliders[i] {
                    glyphs.extend(s.own_glyphs());
                }
            } else if is_vec_row(ptype) {
                if let Some(f) = &self.float3s[i] {
                    glyphs.extend(f.own_glyphs());
                }
            } else if ptype.starts_with("spinbox") {
                if let Some(sb) = &self.spinboxes[i] {
                    glyphs.extend(sb.own_glyphs());
                }
            } else if is_text_row(ptype) {
                if let Some(tb) = &self.texts[i] {
                    glyphs.extend(tb.own_glyphs());
                }
                if let Some(d) = &self.choices[i] {
                    glyphs.extend(d.own_glyphs());
                }
            } else if ptype.starts_with("choice") {
                if let Some(d) = &self.choices[i] {
                    glyphs.extend(d.own_glyphs());
                }
            } else if ptype == "button" {
                if let Some(b) = &self.buttons[i] {
                    glyphs.extend(b.own_glyphs());
                }
            } else if ptype == "toggle" || ptype == "checkbox" {
                if let Some(cb) = &self.toggles[i] {
                    glyphs.extend(cb.own_glyphs());
                }
            } else if ptype.starts_with("color") || ptype == "rgb" || ptype == "rgba" {
                if let Some(c) = &self.colors[i] {
                    glyphs.extend(c.own_glyphs());
                }
            }
        }
        glyphs
    }

    /// Paint [`child_glyphs`](Self::child_glyphs), over the chrome.
    pub(super) fn paint_child_glyphs(&self, ctx: &mut PaintCtx) {
        for (image, rect, alpha) in self.child_glyphs() {
            ctx.image(image, rect, alpha);
        }
    }

    /// The pane's plain quads: section border boxes, every row's chrome (slider/spinbox
    /// backgrounds read via `rect()`+`color()`, the code editor's box/border/cursor), the
    /// controls' own plain quads, all clipped to the viewport — plus the unclipped scrollbar.
    pub(super) fn plain_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.visible {
            return Vec::new();
        }
        let mut quads = Vec::new();
        let rects = self.get_param_rects();
        let view_min = self.rect.y + 4.0;
        let view_max = self.rect.y + self.rect.height - 4.0;

        let clip_quad = |q: (f32, f32, f32, f32, [f32; 4])| -> Option<(f32, f32, f32, f32, [f32; 4])> {
            let (qx, qy, qw, qh, qc) = q;
            let y1 = qy.max(view_min);
            let y2 = (qy + qh).min(view_max);
            if y1 < y2 {
                Some((qx, y1, qw, y2 - y1, qc))
            } else {
                None
            }
        };

        let mut param_quads = Vec::new();

        // Each section is drawn as ONE continuous outline — around the title, down the
        // neck, around the content rows — whose straight runs are these quads; its corner
        // fillets ride `arcs()`, which the host draws under the same clip. Under
        // `control_relief` the outline is replaced by the inset carves served
        // through `reliefs` (title tab + content body recessed).
        if !crate::layout::control_relief() {
            for (title, content) in self.section_boxes() {
                let (runs, _) = self.section_outline(title, content);
                param_quads.extend(runs.into_iter().map(|(x, y, w, h)| (x, y, w, h, SECTION_BORDER_COLOR)));
            }
        }

        let hidden = self.hidden_rows();
        for (i, p) in self.display_params.iter().enumerate() {
            if hidden[i] {
                continue;
            }
            let r = rects[i];
            if p.2.starts_with("slider") {
                if let Some(s) = &self.sliders[i] {
                    let (sx, sy, sw, sh) = s.rect();
                    param_quads.push((sx, sy, sw, sh, s.color()));
                    param_quads.extend(crate::widget::shown_quads(s));
                }
            } else if p.2 == "section" {
                // Section header line is handled by the border box top border now
            } else if p.2 == SEPARATOR {
                // A hairline across the row, inset from the pane's edges.
                let c = crate::colors::active_theme().surface_border;
                param_quads.push((r.0 + 6.0, r.1, (r.2 - 12.0).max(0.0), 1.0, [c[0], c[1], c[2], c[3] * 0.8]));
            } else if is_vec_row(&p.2) {
                if let Some(f) = &self.float3s[i] {
                    param_quads.extend(crate::widget::shown_quads(f));
                }
            } else if p.2 == "code" {
                let (bx, by, bw, bh) = (r.0, r.1 + Self::CODE_BOX_TOP, r.2, r.3 - Self::CODE_BOX_TOP);
                param_quads.push((bx, by, bw, bh, [0.08, 0.08, 0.10, 1.0]));
                let focused = self.focused_param == Some(i);
                // Amber while edits are pending, blue while focused and
                // applied, grey at rest: the border says whether what the
                // node runs is what the box shows.
                let border_color = if focused && self.code_is_dirty() {
                    [0.85, 0.60, 0.25, 1.0]
                } else if focused {
                    [0.25, 0.45, 0.85, 1.0]
                } else {
                    [0.20, 0.20, 0.25, 1.0]
                };
                let text_x = self.code_text_x(r);
                let col_w = self.code_col_w();
                let line_y = |l: usize| by + (Self::CODE_TOP - Self::CODE_BOX_TOP) + l as f32 * Self::CODE_LINE_H;
                // The gutter's edge.
                param_quads.push((text_x - col_w * 0.5, by + 1.0, 1.0, bh - 2.0, [0.16, 0.16, 0.20, 1.0]));
                // The error band, under the flagged line.
                if let Some(err_line) = self.code_error_line {
                    let y = line_y(err_line);
                    if y >= by && y + Self::CODE_LINE_H <= by + bh {
                        param_quads.push((bx + 1.0, y, bw - 2.0, Self::CODE_LINE_H, [0.45, 0.12, 0.10, 1.0]));
                    }
                }
                if focused {
                    if let Some(ref editor) = self.code_editor {
                        // Selection: one band per line it covers.
                        if let Some((start, end)) = editor.selected_range() {
                            let (sl, sc) = get_cursor_line_col(&editor.buffer, start);
                            let (el, ec) = get_cursor_line_col(&editor.buffer, end);
                            let line_len = |l: usize| editor.buffer.split('\n').nth(l).map_or(0, |s| s.chars().count());
                            for l in sl..=el {
                                let c0 = if l == sl { sc } else { 0 };
                                let c1 = if l == el { ec } else { line_len(l) + 1 };
                                let y = line_y(l);
                                if y >= by && y + Self::CODE_LINE_H <= by + bh && c1 > c0 {
                                    param_quads.push((
                                        text_x + c0 as f32 * col_w,
                                        y,
                                        (c1 - c0) as f32 * col_w,
                                        Self::CODE_LINE_H,
                                        [0.22, 0.32, 0.55, 1.0],
                                    ));
                                }
                            }
                        }
                        let (cursor_l, cursor_c) = get_cursor_line_col(&editor.buffer, editor.cursor_idx);
                        let cursor_x = text_x + (cursor_c as f32 * col_w);
                        let cursor_y = line_y(cursor_l) + (Self::CODE_LINE_H - 13.0) / 2.0;
                        if cursor_y >= by && cursor_y + 13.0 <= by + bh {
                            param_quads.push((cursor_x, cursor_y, 1.5, 13.0, [0.80, 0.80, 0.85, 1.0]));
                        }
                    }
                }
                param_quads.push((bx, by, bw, 1.0, border_color));
                param_quads.push((bx, by + bh - 1.0, bw, 1.0, border_color));
                param_quads.push((bx, by, 1.0, bh, border_color));
                param_quads.push((bx + bw - 1.0, by, 1.0, bh, border_color));
            } else if is_text_row(&p.2) {
                if let Some(tb) = &self.texts[i] {
                    param_quads.extend(crate::widget::shown_quads(tb));
                }
                if let Some(d) = &self.choices[i] {
                    param_quads.extend(crate::widget::shown_quads(d));
                }
            } else if p.2.starts_with("choice") {
                if let Some(d) = &self.choices[i] {
                    param_quads.extend(crate::widget::shown_quads(d));
                }
            } else if p.2 == "button" {
                if let Some(b) = &self.buttons[i] {
                    param_quads.extend(crate::widget::shown_quads(b));
                }
            } else if p.2.starts_with("spinbox") {
                if let Some(sb) = &self.spinboxes[i] {
                    { let (bx, by, bw, bh) = sb.rect(); param_quads.push((bx, by, bw, bh, sb.color())); }
                    param_quads.extend(crate::widget::shown_quads(sb));
                }
            } else if p.2 == "toggle" || p.2 == "checkbox" {
                if let Some(cb) = &self.toggles[i] {
                    param_quads.extend(crate::widget::shown_quads(cb));
                }
            } else if p.2.starts_with("color") || p.2 == "rgb" || p.2 == "rgba" {
                if let Some(c) = &self.colors[i] {
                    param_quads.extend(crate::widget::shown_quads(c));
                }
            }
        }

        // Clip all parameter quads vertically
        for q in param_quads {
            if let Some(clipped) = clip_quad(q) {
                quads.push(clipped);
            }
        }

        // The scrollbar is NOT emitted here: the host draws it via `scrollbar_quads`, above
        // or below the pane plate depending on `scrollbar_active`.

        quads
    }

    /// The rounded companion to [`Self::plain_quads`]: the row controls whose boxes are
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
    /// fills they cross (the order the widgets' own paints use). Tuple:
    /// (x, y, w, h, per-corner radii, depth, raised, walls) — raised maps to
    /// `PaintCtx::boss_edges`, flat to `recess_edges`; a toggle's well and
    /// the plate standing in it are why radii/walls are per-entry.
    #[allow(clippy::type_complexity)]
    pub fn reliefs(
        &self,
    ) -> Vec<(f32, f32, f32, f32, (f32, f32, f32, f32), f32, bool, (bool, bool, bool, bool))> {
        if !self.visible || !crate::layout::control_relief() {
            return Vec::new();
        }
        let mut out = Vec::new();

        // Sections as inset panels (the flat outline+fillet path is the
        // non-relief style): ONE union-shaped recess per section — a
        // title-text-width tab strip flush with the body's left edge, opening
        // into the full-width body below. Composed from three edge-suppressed
        // pieces (the tab with its bottom open, the body with its top open,
        // and the top-wall run right of the tab's throat) so no wall crosses
        // the union's interior and the whole section reads as a single well.
        // Collapsed sections keep the title-box carve.
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
                    body_lr(throat_r + rho - depth, &mut out);
                } else {
                    // Too narrow for the fillet: the plain square throat.
                    out.push((tx, ty, tw, th + depth, (r, r, 0.0, 0.0), depth, false, (true, true, false, true)));
                    if cx + cw > throat_r + 0.5 {
                        body_lr(throat_r - depth, &mut out);
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

        let hidden = self.hidden_rows();
        for (i, p) in self.display_params.iter().enumerate() {
            if hidden[i] {
                continue;
            }
            // (control, its configured corner radius, raised vs recessed)
            let ctl: Option<(&dyn WidgetHost, f32, bool)> = if is_text_row(&p.2) {
                // The textpick picker is NOT in this list: with it the row is
                // a field ([`Self::fields`]) — a well ending in a flush run.
                self.texts[i].as_ref().map(|w| (w as &dyn WidgetHost, crate::layout::textbox_corner_radius(), false))
            } else if p.2.starts_with("choice") {
                // The dropdown trigger is a FLUSH control, a field that is all
                // run ([`Self::fields`]). A boss here read as a raised island
                // the widget itself never draws.
                None
            } else if p.2 == "button" {
                // A flush control with a field run's edge, as its own paint
                // draws it ([`Self::fields`]) — it was a boss here, a raised
                // island the button itself never drew.
                None
            } else if p.2 == "toggle" || p.2 == "checkbox" {
                // A toggle is a field ([`Self::fields`]): its well and the
                // flush run gliding in it, one outline round both.
                None
            } else if p.2.starts_with("spinbox") {
                // A plain well only when there is no -/+ run; with one the
                // control is a field ([`Self::fields`]) and its -/+ seam a
                // groove ([`Self::grooves`]). Same side-label inset, same
                // content band, same depth cap as its paint.
                if let Some(sb) = &self.spinboxes[i] {
                    let (x, y, w, h) = sb.rect();
                    let ty = sb.label_strip();
                    let band = Rect { x, y: y + ty, width: w, height: h - ty };
                    if w > 0.0 && h > 0.0 {
                        let r = crate::layout::spinbox_corner_radius();
                        let depth = crate::layout::bevel_width().min((h - ty) * 0.2);
                        // With its -/+ run the control is a field
                        // ([`Self::fields`]); without, a plain well.
                        if let Some(rel) = sb.inner().relief_parts(band) {
                            if rel.run.is_none() {
                                out.push((x, y + ty, w, h - ty, r4(r), depth, false, all));
                            }
                        }
                    }
                }
                None
            } else if p.2.starts_with("color") || p.2 == "rgb" || p.2 == "rgba" {
                // The control's one well (`ColorSelector::field_relief`, the
                // same geometry its paint carves); the swatch is a fill on its
                // floor and the seam a groove, both on the widget's paint.
                if let Some(c) = &self.colors[i] {
                    let (x, y, w, h) = c.rect();
                    let ty = c.label_strip();
                    if let Some((rx, ry, rw, rh, rr, rd)) =
                        c.inner().field_relief(Rect { x, y: y + ty, width: w, height: h - ty })
                    {
                        out.push((rx, ry, rw, rh, r4(rr), rd, false, all));
                    }
                }
                None
            } else {
                // Sliders (and Float3's three rows) are bands: their well is
                // hand-shaded quads that follow the band's contour, which reach
                // a flat host through the plain-quad view — no rect carve.
                None
            };
            if let Some((w, radius, raised)) = ctl {
                let (x, y, ww, h) = w.rect();
                if ww <= 0.0 || h <= 0.0 {
                    continue;
                }
                // The top-label band stays outside the relief like every other host.
                let ty = w.label_strip();
                let depth = crate::layout::bevel_width().min((h - ty) * 0.2);
                // A text box joined to its picker is half of a field
                // ([`Self::fields`]), drawn there.
                if is_text_row(&p.2) && self.texts[i].as_ref().is_some_and(|t| t.inner().joined_right) {
                    continue;
                }
                out.push((x, y + ty, ww, h - ty, r4(radius), depth, raised, all));
            }
        }
        out
    }

    /// The rows that are ONE field with a run ([`Field`]: a sunken well
    /// holding a flush run, one outline round both), drawn AFTER
    /// [`Self::reliefs`]. A textpick row is its text box's well ending in its
    /// picker, a spinbox its value's well ending in its -/+ run
    /// ([`Field::ending_in_run`]); a dropdown or button row, a field that is
    /// all run ([`Field::run`]); a toggle, its own sliding field
    /// (`Toggle::field`). So every flush control in the pane has the same
    /// edge. The plain wells are not here — they are [`Self::reliefs`], which
    /// group into the pane's plate. They replaced
    /// `troughs()` on 2026-10-01. On the pane's own rule for wells and
    /// troughs: the content band as the outline, the wall straddling it.
    #[allow(clippy::type_complexity)]
    pub fn fields(&self) -> Vec<Field> {
        if !self.visible || !crate::layout::control_relief() {
            return Vec::new();
        }
        let mut out = Vec::new();
        let hidden = self.hidden_rows();
        for (i, p) in self.display_params.iter().enumerate() {
            if hidden[i] {
                continue;
            }
            if p.2.starts_with("textpick") {
                if let (Some(tb), Some(d)) = (&self.texts[i], &self.choices[i]) {
                    if !tb.inner().joined_right {
                        continue;
                    }
                    let (x, y, w, h) = tb.rect();
                    let (dx, _, dw, _) = d.rect();
                    let ty = tb.label_strip();
                    if w > 0.0 && h - ty > 0.0 && dw > 0.0 {
                        let depth = crate::layout::bevel_width().min((h - ty) * 0.2);
                        let r = crate::layout::textbox_corner_radius();
                        let outline = Rect { x, y: y + ty, width: dx + dw - x, height: h - ty };
                        out.push(Field::ending_in_run(outline, (r, r, r, r), depth, dx));
                    }
                }
            } else if p.2 == "button" {
                // A button: a field that is all run, as its own flush plate
                // (`Button::plate`, `PaintCtx::inset_plate`) on its
                // band, at the button radius.
                if let Some(b) = &self.buttons[i] {
                    let (x, y, w, h) = b.rect();
                    let ty = b.label_strip();
                    if w > 0.0 && h - ty > 0.0 {
                        let depth = crate::layout::bevel_width().min((h - ty) * 0.2);
                        let r = crate::layout::button_corner_radius();
                        out.push(Field::run(Rect { x, y: y + ty, width: w, height: h - ty }, (r, r, r, r), depth));
                    }
                }
            } else if p.2.starts_with("choice") {
                // The dropdown trigger: a field that is all run — the
                // widget's own raised paint (`PaintCtx::inset_plate`)
                // on the same band, radius and depth cap. Until 2026-10-01
                // a trough, whose edge differed from the run's at the end of
                // a text row's field.
                if let Some(d) = &self.choices[i] {
                    let (x, y, w, h) = d.rect();
                    let ty = d.label_strip();
                    if w > 0.0 && h - ty > 0.0 {
                        let depth = crate::layout::bevel_width().min((h - ty) * 0.2);
                        let r = crate::layout::dropdown_corner_radius();
                        out.push(Field::run(Rect { x, y: y + ty, width: w, height: h - ty }, (r, r, r, r), depth));
                    }
                }
            } else if p.2.starts_with("spinbox") {
                if let Some(sb) = &self.spinboxes[i] {
                    let (x, y, w, h) = sb.rect();
                    let ty = sb.label_strip();
                    let band = Rect { x, y: y + ty, width: w, height: h - ty };
                    if let Some(rel) = sb.inner().relief_parts(band) {
                        if let Some((split, _)) = rel.run {
                            let r = rel.radius;
                            out.push(Field::ending_in_run(band, (r, r, r, r), rel.depth, split));
                        }
                    }
                }
            } else if p.2 == "toggle" || p.2 == "checkbox" {
                // The toggle's own field (`Toggle::field`) —
                // exactly what its paint draws, on its band. It paints no
                // fill, so this IS the control.
                if let Some(t) = &self.toggles[i] {
                    let (x, y, w, h) = t.rect();
                    let ty = t.label_strip();
                    if w > 0.0 && h - ty > 0.0 {
                        // The pane's hover is the tint here, as on every field.
                        let band = Rect { x, y: y + ty, width: w, height: h - ty };
                        out.push(t.inner().field(band).with_tint(None));
                    }
                }
            }
        }
        out
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

/// `text` whole if it measures within `avail` px, else its longest head
/// with "..." that does.
pub(super) fn fit_tail(text: &str, avail: f32, family: &str, size: f32) -> String {
    use crate::widget::display::{measure_text_width, truncate_tail};
    let fits = |t: &str| measure_text_width(t, family, size) <= avail;
    if fits(text) {
        return text.to_string();
    }
    (0..text.chars().count())
        .rev()
        .map(|n| truncate_tail(text, n))
        .find(|t| fits(t))
        .unwrap_or_default()
}
