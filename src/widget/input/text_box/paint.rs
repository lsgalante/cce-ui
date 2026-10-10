//! What the box draws: selection and composition quads, the value's text, and `impl Paint`.

use super::*;

impl Paint for TextBox {
    fn color(&self) -> [f32; 4] {
        [0.10, 0.10, 0.16, 1.0]
    }

    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        let r = crate::layout::textbox_corner_radius();
        if r > 0.0 {
            Some((r, (true, true, true, true)))
        } else {
            Some((r, (false, false, false, false)))
        }
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::control_label_font_detached())
    }

    /// Content text font for the paint walk: a TextBox whose `font_family`/`font_size` was
    /// deliberately customized (cce-text-editor's monospace editor) draws its value text in
    /// that family at the label's own size — a bare family name, so the control-font string's
    /// size suffix doesn't override `font_size`. Default boxes keep the `widget_font` string
    /// verbatim (the legacy convention, size suffix included).
    fn text_font(&self) -> Option<String> {
        if self.font_family != self.default_font_family || self.font_size != self.default_font_size {
            Some(self.font_family.clone())
        } else {
            self.widget_font()
        }
    }

    fn text_attrs(&self) -> crate::scene::paint::TextAttrs {
        self.font_attrs
    }

    fn sync_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    fn text_bounds(&self, rect: Rect) -> Option<[f32; 4]> {
        Some(self.text_clip(rect))
    }

    /// The legacy `prepare_text`: sync font family/size with the live config defaults, then
    /// shape the display text and record per-glyph advances (`map_x_to_idx` reads them).
    fn prepare_text(&mut self, fs: &mut cosmic_text::FontSystem, _rect: Rect) {
        // The input method's composition, shown before the buffer is shaped.
        self.sync_preedit();
        let (style_family, style_size) = crate::layout::control_label_font_detached_parsed();
        if self.font_size == self.default_font_size {
            self.font_size = style_size;
        }
        self.default_font_size = style_size;

        if self.font_family == self.default_font_family {
            self.font_family = style_family.clone();
        }
        self.default_font_family = style_family;

        let text_src = if self.editing { &self.edit_buffer } else { &self.text };
        let showing_placeholder = text_src.is_empty() && self.placeholder.is_some();
        let display_text = if showing_placeholder {
            self.placeholder.as_ref().unwrap().as_str()
        } else {
            text_src.as_str()
        };

        // Measured in the family the value text is DRAWN in — see `value_font`.
        let font_fam = self.value_font();
        // Read once: the key, every buffer shaped below and every division by the scale
        // take this value, so the offsets are logical px whatever the scale does meanwhile.
        // Not clamped: the glyph pass draws the buffer shaped at this same scale.
        let scale = crate::scale::scale_factor();

        // One column's advance, from the same shaping path as the labels (buffer-cached,
        // so this is a lookup after the first frame per family/size).
        let probe = crate::text::shared_text_buffer_at(
            fs,
            "MMMMMMMM",
            self.font_size,
            font_fam.as_deref(),
            self.font_attrs,
            scale,
        );
        self.shaped_char_advance = crate::text::shaped_run(&probe, "MMMMMMMM", scale).width / 8.0;

        // `char_width()` returns this frame's shaped advance from here on, so the
        // wrap below matches the one `selection_quads`/`value_labels` compute at
        // paint time.
        let wrap = self.multiline.then(|| self.wrap_width(self.rect.width).to_bits());

        let key = PrepKey {
            text: display_text.to_string(),
            placeholder: showing_placeholder,
            password: self.is_password,
            font_size_bits: self.font_size.to_bits(),
            font: font_fam.clone(),
            attrs: self.font_attrs,
            scale_bits: scale.to_bits(),
            vertical: crate::text::vertical_text().is_some(),
            wrap,
            room_bits: (self.rect.width - 2.0 * self.pad()).max(0.0).to_bits(),
        };
        if self.prep_key.as_ref() != Some(&key) {
            self.shape_columns(fs, &key);
            self.prep_key = Some(key);
        }

        let cursor_pos = self.cursor_idx.min(self.glyph_positions.len().saturating_sub(1));
        self.cursor_x_offset = self.glyph_positions.get(cursor_pos).copied().unwrap_or(0.0);
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let pad = self.pad();
        let top = self.label_top();
        let base_y = rect.y - top;
        let base_h = rect.height + top;
        let visual_h = rect.height;
        let radius = crate::layout::textbox_corner_radius();
        let border_w = self.border_width();

        // Open for typing: say so to the compositor this frame (the
        // on-screen keyboard follows it). The field stands in for the caret
        // until the caret is drawn below, which reports itself.
        if self.editing && !self.disabled {
            let (ox, oy) = ctx.offset();
            crate::text_input::claim(self.rect.x + ox, self.rect.y + top + oy, self.rect.width, visual_h);
        }

        // Keep the model's cached rect and the paint rect consistent: paint receives the
        // content rect derived from the same base the cache holds, so the bodies below read
        // `self.rect` (the legacy `self.base`) exactly as legacy did. `rect` is used only to
        // localize this frame's geometry.
        let _ = (base_y, base_h);

        if radius <= 0.0 {
            // Legacy `extra_quads`: full base span, disabled
            // special-case with early return.
            let mut quads: Vec<(f32, f32, f32, f32, [f32; 4])> = Vec::new();
            if self.disabled {
                if self.draw_bg_border {
                    quads.push((self.rect.x, self.rect.y + top, self.rect.width, visual_h, [0.12, 0.12, 0.16, 1.0]));
                    quads.push((self.rect.x + border_w, self.rect.y + top + border_w, self.rect.width - 2.0 * border_w, visual_h - 2.0 * border_w, [0.06, 0.06, 0.08, 1.0]));
                }
            } else {
                if self.draw_bg_border {
                    // One background regardless of focus — the focus treatment is
                    // the tinted recess rim (rounded path) / editing border, not a
                    // surface swap.
                    let bg_color = crate::color::textbox_background_color();
                    let border_color = crate::color::well_frame_color(self.hovered, self.editing);
                    quads.push((self.rect.x, self.rect.y + top, self.rect.width, visual_h, border_color));
                    quads.push((self.rect.x + border_w, self.rect.y + top + border_w, self.rect.width - 2.0 * border_w, visual_h - 2.0 * border_w, bg_color));
                }
                if let Some([cx, cy, cw, ch]) = self.selection_quads(self.rect.x, self.rect.width, &mut quads) {
                    let (ox, oy) = ctx.offset();
                    crate::ime::report_caret(cx + ox, cy + oy, cw, ch);
                }
            }
            for (qx, qy, qw, qh, qc) in quads {
                ctx.quad(Rect { x: qx, y: qy, width: qw, height: qh }, qc);
            }
        } else {
            // Legacy `all_rounded_quads`: no disabled special-case.
            let x = self.rect.x;
            let w = self.rect.width;

            // One background regardless of focus (see the flat path above).
            let bg_color = crate::color::textbox_background_color();
            let border_color = crate::color::well_frame_color(self.hovered, self.editing);

            if self.draw_bg_border {
                let corners = (true, true, true, true);
                // Recessed + transparent fill: the carve alone defines the
                // well — the plate below is its floor, so the flat border and
                // bg rects are skipped entirely. An opaque fill (e.g. the edit
                // color while editing) draws as usual and gets carved.
                let bare = self.recessed() && bg_color[3] <= 0.001;
                if !bare {
                    ctx.rounded_rect(Rect { x, y: self.rect.y + top, width: w, height: visual_h }, radius, corners, border_color);
                    ctx.rounded_rect(
                        Rect { x: x + border_w, y: self.rect.y + top + border_w, width: w - 2.0 * border_w, height: visual_h - 2.0 * border_w },
                        (radius - border_w).max(0.0),
                        corners,
                        bg_color,
                    );
                }
                if let Some(field) = self.well() {
                    // Focus lights the well's rim in the highlight accent (with
                    // the shader's complementary shadow) — the TreeList treatment.
                    ctx.field(&field);
                }
            }

            let mut quads: Vec<(f32, f32, f32, f32, [f32; 4])> = Vec::new();
            if let Some([cx, cy, cw, ch]) = self.selection_quads(x, w, &mut quads) {
                let (ox, oy) = ctx.offset();
                crate::ime::report_caret(cx + ox, cy + oy, cw, ch);
            }
            for (qx, qy, qw, qh, qc) in quads {
                ctx.quad(Rect { x: qx, y: qy, width: qw, height: qh }, qc);
            }

            // Relief scrollbar for overflowing multiline content — the shared
            // groove + raised-pill painter (the TreeList treatment), persistent
            // rather than activity-faded: an editor keeps its position
            // indicator. Content height mirrors clamp_scroll's math (`pad`
            // top and bottom), so the thumb tracks the scroll range exactly.
            if self.multiline {
                let line_height = self.line_height();
                let content_h = self.wrap_text(self.wrap_width(self.rect.width)).0.len() as f32 * line_height;
                crate::widget::container::scroll_box::paint_relief_scrollbar(
                    ctx,
                    Rect { x, y: self.rect.y + top, width: w, height: visual_h },
                    content_h + 2.0 * pad,
                    self.scroll_y,
                );
            }
        }

        // The content is whatever has been typed, so a line longer than the
        // well is routine rather than exceptional; the well scrolls, but
        // nothing stopped the glyphs drawing outside it. (The adapter's label
        // bridge swaps this for `text_bounds` — the same clip.)
        let well = Some(self.text_clip(Rect { x: self.rect.x, y: self.rect.y + top, width: self.rect.width, height: visual_h }));
        let font = self.value_font();
        for tl in self.value_labels() {
            ctx.text_with(tl.text, tl.x, tl.y, tl.font_size, tl.color, font.clone(), well);
        }
    }
}

impl TextBox {

    /// Selection highlight + caret quads, shared by both render branches. `x`/`w` are the
    /// (possibly label-inset) horizontal span the branch draws in — the legacy paths differed
    /// (non-rounded and rounded alike use the full base span).
    /// The selection highlight, an input method's composition underlined, and
    /// the caret, as quads. Returns the caret's rect while editing (unclipped,
    /// in the box's coordinates), which the painter reports to the input method.
    pub(super) fn selection_quads(&self, x: f32, w: f32, out: &mut Vec<(f32, f32, f32, f32, [f32; 4])>) -> Option<[f32; 4]> {
        let pad = self.pad();
        if !(self.editing || self.select_anchor.is_some()) {
            return None;
        }
        let mut caret = None;
        let top = self.label_top();
        let char_width = self.char_width();
        let line_height = self.line_height();

        let highlight_color = [0.20, 0.50, 0.85, 0.3];
        let cursor_color = if self.draw_bg_border {
            [0.80, 0.80, 0.85, 1.0]
        } else {
            [0.10, 0.10, 0.15, 1.0]
        };

        let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
        let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);

        if self.multiline {
            let max_w = self.wrap_width(w);
            let (_lines, index_map) = self.wrap_text(max_w);

            let view_top = self.rect.y + top;
            let view_bottom = self.rect.y + self.rect.height;

            if start != end {
                let start_pos = index_map[start.min(index_map.len() - 1)];
                let end_pos = index_map[end.min(index_map.len() - 1)];

                for line_idx in start_pos.0..=end_pos.0 {
                    let mut line_start_col = None;
                    let mut line_end_col = None;
                    for idx in start..end {
                        if idx < index_map.len() {
                            let (l, c) = index_map[idx];
                            if l == line_idx {
                                if line_start_col.is_none() || c < line_start_col.unwrap() {
                                    line_start_col = Some(c);
                                }
                                if line_end_col.is_none() || c > line_end_col.unwrap() {
                                    line_end_col = Some(c);
                                }
                            }
                        }
                    }
                    if let (Some(sc), Some(ec)) = (line_start_col, line_end_col) {
                        let highlight_y = self.rect.y + top + pad + (line_idx as f32 * line_height) - self.scroll_y;
                        let clipped_y = highlight_y.max(view_top);
                        let clipped_bottom = (highlight_y + line_height).min(view_bottom);
                        let shift = self.line_shift.get(line_idx).copied().unwrap_or(0.0);
                        // The boxes of the selected clusters: two pieces where the selection
                        // crosses a change of direction. Without a shaped run, the column span.
                        let spans = match (self.line_runs.get(line_idx), _lines.get(line_idx)) {
                            (Some(run), Some(line)) => {
                                let byte = |col: usize| line.char_indices().nth(col).map_or(line.len(), |(b, _)| b);
                                run.spans(byte(sc), byte(ec + 1)).into_iter().map(|(a, b)| (a + shift, b + shift)).collect()
                            }
                            _ => {
                                let (a, b) = (self.line_col_x(line_idx, sc), self.line_col_x(line_idx, ec + 1));
                                vec![(a.min(b), a.max(b))]
                            }
                        };
                        for (a, b) in spans {
                            let h_left = (x + pad + a - self.scroll_x).max(x + pad);
                            let h_right = (x + pad + b - self.scroll_x).min(x + w - pad);
                            if h_left < h_right && clipped_y < clipped_bottom {
                                out.push((h_left, clipped_y, h_right - h_left, clipped_bottom - clipped_y, highlight_color));
                            }
                        }
                    }
                }
            }

            if let Some((cs, cl)) = self.composing {
                // The composition's underline, a line at a time, under the
                // glyphs it covers.
                let last_line = index_map.get((cs + cl).saturating_sub(1).min(index_map.len() - 1)).map_or(0, |p| p.0);
                let first_line = index_map.get(cs.min(index_map.len() - 1)).map_or(0, |p| p.0);
                for line_idx in first_line..=last_line {
                    let cols: Vec<usize> = (cs..cs + cl)
                        .filter_map(|i| index_map.get(i).filter(|p| p.0 == line_idx).map(|p| p.1))
                        .collect();
                    if let (Some(&a), Some(&b)) = (cols.iter().min(), cols.iter().max()) {
                        let (xa, xb) = (self.line_col_x(line_idx, a), self.line_col_x(line_idx, b + 1));
                        let ux = x + pad + xa.min(xb) - self.scroll_x;
                        let uw = (xb - xa).abs();
                        let uy = self.rect.y + top + pad + ((line_idx + 1) as f32 * line_height) - 2.0 - self.scroll_y;
                        let left = ux.max(x + pad);
                        let right = (ux + uw).min(x + w - pad);
                        if left < right && uy >= view_top && uy + 1.5 <= view_bottom {
                            out.push((left, uy, right - left, 1.5, cursor_color));
                        }
                    }
                }
            }

            if self.editing {
                let caret_h = self.font_size * 1.15;
                let (cursor_l, cursor_c) = index_map[self.cursor_idx.min(index_map.len() - 1)];
                let cursor_x = x + pad + self.line_col_x(cursor_l, cursor_c) - self.scroll_x;
                let cursor_y = self.rect.y + top + pad + (cursor_l as f32 * line_height) + (line_height - caret_h) / 2.0 - self.scroll_y;
                caret = Some([cursor_x, cursor_y, 1.5, caret_h]);
                let clipped_y = cursor_y.max(view_top);
                let clipped_bottom = (cursor_y + caret_h).min(view_bottom);
                if cursor_x >= x + pad && cursor_x <= x + w - pad
                    && clipped_y < clipped_bottom {
                        out.push((cursor_x, clipped_y, 1.5, clipped_bottom - clipped_y, cursor_color));
                    }
            }
        } else {
            let caret_h = self.font_size * 1.15;
            if start != end {
                // The boxes of the selected clusters: two pieces where the selection crosses
                // a change of direction. Without a shaped run, the column span.
                let shown = if self.is_password { "•".repeat(self.edit_buffer.chars().count()) } else { self.edit_buffer.clone() };
                let spans: Vec<(f32, f32)> = match &self.glyph_run {
                    Some(run) => {
                        let byte = |col: usize| shown.char_indices().nth(col).map_or(shown.len(), |(b, _)| b);
                        run.spans(byte(start), byte(end)).into_iter().map(|(a, b)| (a + self.glyph_shift, b + self.glyph_shift)).collect()
                    }
                    None => {
                        let at = |i: usize| self.glyph_positions.get(i).copied().unwrap_or(i as f32 * char_width);
                        vec![(at(start).min(at(end)), at(start).max(at(end)))]
                    }
                };
                for (a, b) in spans {
                    let h_left = (x + pad + a - self.scroll_x).max(x + pad);
                    let h_right = (x + pad + b - self.scroll_x).min(x + w - pad);
                    if h_left < h_right {
                        out.push((
                            h_left,
                            crate::layout::align_text_y(self.rect.y, self.rect.height, self.font_size, top),
                            h_right - h_left,
                            crate::layout::line_height(self.font_size),
                            highlight_color,
                        ));
                    }
                }
            }

            if let Some((cs, cl)) = self.composing {
                let at = |i: usize| self.glyph_positions.get(i).copied().unwrap_or(i as f32 * char_width);
                let (ca, cb) = (at(cs).min(at(cs + cl)), at(cs).max(at(cs + cl)));
                let left = (x + pad + ca - self.scroll_x).max(x + pad);
                let right = (x + pad + cb - self.scroll_x).min(x + w - pad);
                if left < right {
                    let text_y = crate::layout::align_text_y(self.rect.y, self.rect.height, self.font_size, top);
                    out.push((left, text_y + self.font_size + 1.0, right - left, 1.5, cursor_color));
                }
            }

            if self.editing {
                let offset = if self.glyph_positions.is_empty() {
                    self.cursor_idx as f32 * char_width
                } else {
                    self.cursor_x_offset
                };
                let cursor_x = x + pad + offset - self.scroll_x;
                let text_y = crate::layout::align_text_y(self.rect.y, self.rect.height, self.font_size, top);
                let cursor_y = text_y + (self.font_size - caret_h) / 2.0;
                caret = Some([cursor_x, cursor_y, 1.5, caret_h]);
                if cursor_x >= x + pad && cursor_x <= x + w - pad {
                    out.push((cursor_x, cursor_y, 1.5, caret_h, cursor_color));
                }
            }
        }
        caret
    }

    /// The value/placeholder text lines — the legacy `text_labels` body minus the control
    /// label (the adapter's base-label machinery draws that).
    pub(super) fn value_labels(&self) -> Vec<TextLabel> {
        let pad = self.pad();
        let mut labels = Vec::new();
        let top = self.label_top();
        let mut val_text = if self.editing {
            self.edit_buffer.clone()
        } else {
            self.text.clone()
        };
        if self.is_password {
            val_text = "•".repeat(val_text.chars().count());
        }

        let is_placeholder = val_text.is_empty() && self.placeholder.is_some();
        let display_text = if is_placeholder {
            self.placeholder.as_ref().unwrap().clone()
        } else {
            val_text
        };

        let label_color = if is_placeholder {
            crate::color::textbox_placeholder_text_color()
        } else if let Some(custom_color) = self.text_color {
            custom_color
        } else if self.disabled {
            [0x53, 0x53, 0x5a]
        } else if self.all_selected {
            [0xff, 0xff, 0xff]
        } else if self.editing {
            [0xee, 0xee, 0xf5]
        } else {
            [0xcc, 0xcc, 0xd4]
        };

        let x = self.rect.x;
        let w = self.rect.width;

        if self.multiline {
            let line_height = self.line_height();
            let max_w = self.wrap_width(w);
            let (lines, _) = self.wrap_text(max_w);
            let lines_to_draw = if is_placeholder {
                self.wrap_str(self.placeholder.as_deref().unwrap_or(""), max_w).0
            } else {
                lines
            };
            for (line_idx, line_text) in lines_to_draw.iter().enumerate() {
                let shift = if is_placeholder { 0.0 } else { self.line_shift.get(line_idx).copied().unwrap_or(0.0) };
                labels.push(TextLabel {
                    text: line_text.clone(),
                    x: x + pad + shift - self.scroll_x,
                    y: self.rect.y + top + pad + (line_idx as f32 * line_height) + (line_height - self.font_size) / 2.0 - self.scroll_y,
                    font_size: self.font_size,
                    color: label_color,
                });
            }
        } else {
            labels.push(TextLabel {
                text: display_text,
                x: x + pad + self.glyph_shift - self.scroll_x,
                y: crate::layout::align_text_y(self.rect.y, self.rect.height, self.font_size, top),
                font_size: self.font_size,
                color: label_color,
            });
        }
        labels
    }
}
