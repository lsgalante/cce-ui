//! What the dropdown draws: the trigger's plate and border, the arrow, the text, and `impl Paint`
//! (the list grown out of the trigger).

use super::*;

impl Paint for Dropdown {
    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        let r = crate::layout::dropdown_corner_radius();
        if r > 0.0 {
            Some((r, (true, true, true, true)))
        } else {
            Some((r, (false, false, false, false)))
        }
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::control_label_font_detached())
    }

    fn sync_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        if self.menu_replaces_trigger && self.open {
            // The trigger yields to the menu the moment the revealed rows
            // cover its band (and comes back as the contraction uncovers it);
            // the frosted menu plate is translucent, so a trigger left
            // painting beneath would show through blurred.
            let (_, ay, _, ah) = self.popover_geom_drawn(rect);
            if ay <= rect.y + 0.5 && ay + ah >= rect.y + rect.height - 0.5 {
                return;
            }
        }
        self.paint_background(rect, ctx);
        self.paint_text(rect, ctx);
    }

    fn popover(&self, rect: Rect) -> Option<(f32, f32, f32, f32)> {
        if self.open {
            // The ANIMATED unified box (trigger band + revealed menu), not the
            // full menu geometry: hosts replay popover text with bounds derived
            // from this rect (and the dl-text occlusion clamp reads it), so
            // reporting the drawn surface keeps labels — the band's included —
            // clipped to the traveling edge everywhere without per-app changes.
            Some(self.unified_geom_drawn(rect))
        } else {
            None
        }
    }

    fn draw_popover(&self, rect: Rect, pc: &mut dyn crate::layout::RenderTarget) {
        if !self.open {
            return;
        }

        // ONE continuous surface in the status-interface manner: the trigger
        // band grows into the menu — no detached popover plate, no drop
        // shadows. The unified box spans the trigger and the revealed menu;
        // the trigger's display text is redrawn on top of its band. Rows sit
        // at their FINAL positions (from the full geometry) and slide into
        // view as the traveling edge reveals them, clipped to the menu area by
        // hand (RenderTarget carries no clip stack); text clips through its
        // bounds.
        let (rx, _ry, rw, _rh) = self.popover_geom(rect);
        // The row strip inside the box — past the relief wall on an outer edge.
        let rows_y = self.rows_top(rect);
        let (ax, ay, aw, ah) = self.popover_geom_drawn(rect);
        if aw <= 0.5 || ah <= 0.5 {
            // Nothing revealed yet — the plain trigger stands alone.
            return;
        }
        let clip = |x: f32, y: f32, w: f32, h: f32| -> Option<(f32, f32, f32, f32)> {
            let x0 = x.max(ax);
            let y0 = y.max(ay);
            let x1 = (x + w).min(ax + aw);
            let y1 = (y + h).min(ay + ah);
            if x1 > x0 && y1 > y0 { Some((x0, y0, x1 - x0, y1 - y0)) } else { None }
        };

        let theme = colors::active_theme();
        let (ux, uy, uw, uh) = self.unified_geom_drawn(rect);

        // The ACTUAL button surface, expanded: the raised trigger's flush
        // inset plate grown over the unified box (real relief prims on a
        // PaintCtx-backed target; collector hosts degrade to a rounded fill).
        // The trigger's configured fill is usually transparent — the window
        // plate IS its face — so the expansion substitutes the plate color,
        // FROSTED: the popover material (`Material::popover`), the same the
        // context menu wears (`ContextMenuState::paint`), so the menu shows
        // the content beneath it blurred and tinted rather than covering it.
        // Encoded here because the flat-path `RenderTarget` speaks colours.
        let radius = crate::layout::dropdown_corner_radius();
        let raw_bg = colors::dropdown_background_color();
        let face = {
            let base = if raw_bg[3] > 0.001 { raw_bg } else { crate::color::page_low_color() };
            crate::scene::Material::popover(base).fill(crate::scene::PlateRole::Nested)
        };
        if self.raised() {
            let depth = crate::layout::bevel_width().min(rect.height * 0.2);
            let (t, tr) = crate::layout::carve_inside(
                crate::scene::layout::Rect { x: ux, y: uy, width: uw, height: uh },
                (radius, radius, radius, radius),
                depth,
            );
            pc.inset_plate(face, t.x, t.y, t.width, t.height, tr.0, depth);
        } else {
            pc.rect_with_radius(self.border_color(), ux, uy, uw, uh, radius);
            pc.rect_with_radius(face, ux + 1.0, uy + 1.0, uw - 2.0, uh - 2.0, (radius - 1.0).max(0.0));
        }

        // Trigger content redrawn over its band (the box covers the widget-pass
        // trigger paint) — display text left, arrow right, the paint_text palette.
        // A menu that replaces the trigger has no band to redraw on.
        if !self.menu_replaces_trigger {
            let (tx, ty) = (rect.x, rect.y);
            let th = rect.height;
            let band_bounds = Some([ux, uy, ux + uw, uy + uh]);
            let font = crate::layout::control_label_font_detached();
            let text_y = crate::layout::align_text_y(ty, th, 12.0, 0.0);
            pc.text_with_font_and_bounds(
                &self.display_text(),
                tx + 8.0,
                text_y,
                12.0,
                [0.8, 0.8, 0.85, 1.0],
                &font,
                band_bounds,
            );
            pc.icon("chevron-down", self.arrow_rect(rect), ARROW_COLOR);
        }

        if let Some(h_idx) = self.hovered_item {
            let iy = rows_y + h_idx as f32 * Self::ROW_H;
            // 4. Vibrantly colored translucent selection highlight
            if let Some((cx, cy, cw, ch)) = clip(rx + 2.0, iy + 2.0, rw - 4.0, 20.0) {
                pc.rect(theme.primary_accent, cx, cy, cw, ch);
            }
        }

        let marks = self.mark_column();
        for (idx, opt) in self.options.iter().enumerate() {
            let row_top = rows_y + idx as f32 * Self::ROW_H;
            let iy = crate::layout::align_text_y(row_top, Self::ROW_H, 12.0, 0.0);

            if opt == "-" {
                if let Some((cx, cy, cw, ch)) = clip(rx + 8.0, row_top + 11.5, rw - 16.0, 1.0) {
                    pc.rect(theme.surface_border, cx, cy, cw, ch);
                }
                continue;
            }

            // Menu-button mode (custom_display_text) has no "current" option —
            // its rows are commands, so none reads as selected.
            let text_color = if self.hovered_item == Some(idx) {
                [0xff, 0xff, 0xff]
            } else if self.selected == idx && self.custom_display_text.is_none() {
                [0x3a, 0x9a, 0xff]
            } else {
                [0xcc, 0xcc, 0xd4]
            };

            let color_f32 = [
                text_color[0] as f32 / 255.0,
                text_color[1] as f32 / 255.0,
                text_color[2] as f32 / 255.0,
                1.0,
            ];

            // Bounds = the unified popover rect EXACTLY (not the menu sub-box):
            // the dl-text occlusion clamp exempts only exact-match overlay
            // labels, and the unified box's traveling edge clips identically.
            let bounds = Some([ux, uy, ux + uw, uy + uh]);
            let font = crate::layout::control_label_font_detached();
            let (mark, label) = crate::widget::context_menu::split_mark(opt);
            if let Some(name) = mark {
                let g = Rect { x: rx + 8.0, y: row_top + 0.5 * (Self::ROW_H - MARK_SIDE), width: MARK_SIDE, height: MARK_SIDE };
                pc.push_clip_rect(ux, uy, uw, uh);
                pc.icon(name, g, color_f32);
                pc.pop_clip_rect();
            }
            pc.text_with_font_and_bounds(label, rx + 8.0 + marks, iy, 12.0, color_f32, &font, bounds);
        }
    }
}

impl Dropdown {

    pub(super) fn border_color(&self) -> [f32; 4] {
        if self.open && !self.closing {
            [0.30, 0.50, 0.32, 1.0]
        } else if self.hovered {
            let bc = colors::dropdown_border_color();
            [(bc[0] + 0.15).min(1.0), (bc[1] + 0.15).min(1.0), (bc[2] + 0.15).min(1.0), bc[3]]
        } else {
            colors::dropdown_border_color()
        }
    }

    /// Emit the border + background geometry — the legacy `all_rounded_quads` body (rounded,
    /// with the root plate-concentric corner adjustment) or `extra_quads` (plain) depending on
    /// the configured radius, byte-for-byte on the same content rect.
    pub(super) fn paint_background(&self, content: Rect, ctx: &mut PaintCtx) {
        let x = content.x;
        let w = content.width;
        let y = content.y;
        let visual_h = content.height;

        let raw_bg = colors::dropdown_background_color();
        let mut bg_color = raw_bg;
        bg_color[3] = 1.0; // Force opaque background to prevent subpixel blending artifacts
        let border_color = self.border_color();

        let radius = crate::layout::dropdown_corner_radius();
        // Raised style: one lit Bevel plate owns fill and edge (the concentric
        // corner_frame adjustment keeps the legacy path — it exists to nest
        // flat outlines, which a rolled edge replaces). A transparent
        // configured fill degrades to a Boss: edges only, plate as the face —
        // judged on the RAW alpha, before the opacity force above.
        if self.flat {
            let plate = crate::widget::ControlPlate::control(
                Rect { x, y, width: w, height: visual_h },
                radius,
                crate::widget::PlateStance::Flat,
                self.face.or_else(|| crate::scene::Material::control_face(raw_bg)),
            )
            .with_tint(self.focused.then(crate::widget::ControlPlate::focus_tint));
            ctx.control_plate(&plate);
            return;
        }
        if self.raised() {
            let depth = crate::layout::bevel_width().min(visual_h * 0.2);
            // Concentric corner_frame adjustment applies to the relief too: a
            // corner nested at equal gaps into the frame follows its curve.
            // The window corner is span-widened (corner_span_factor, diagonal
            // curvature = pr), so its parallel curve at inset g has diagonal
            // curvature pr - g — which as a NOMINAL widget-scale squircle
            // radius is factor * (pr - g). Exactly pr - g for circular
            // corners (factor 1).
            let mut r4 = [radius; 4];
            if let Some(((px, py, pw, ph), pr, (pr1, pr2, pr3, pr4))) = self.corner_frame {
                let cf = crate::layout::corner_span_factor();
                let g_left = x - px;
                let g_top = y - py;
                let g_right = (px + pw) - (x + w);
                let g_bottom = (py + ph) - (y + visual_h);
                if pr1 && (g_left - g_top).abs() < 1.0 && g_left >= 0.0 {
                    r4[0] = (pr - g_left).max(0.0) * cf;
                }
                if pr2 && (g_right - g_top).abs() < 1.0 && g_right >= 0.0 {
                    r4[1] = (pr - g_right).max(0.0) * cf;
                }
                if pr3 && (g_right - g_bottom).abs() < 1.0 && g_right >= 0.0 {
                    r4[2] = (pr - g_right).max(0.0) * cf;
                }
                if pr4 && (g_left - g_bottom).abs() < 1.0 && g_left >= 0.0 {
                    r4[3] = (pr - g_left).max(0.0) * cf;
                }
            }
            if let Some((a, b, c, d)) = self.radii {
                r4 = [a, b, c, d];
            }
            // The trigger is a flush control plate (groove ring down, beveled
            // lip back up, face level with the surface; a transparent raw
            // fill = edges only), its footprint the trigger's rect and its
            // silhouette the frame-adjusted radii.
            let plate = crate::widget::ControlPlate::control(
                Rect { x, y, width: w, height: visual_h },
                radius,
                crate::widget::PlateStance::Flush,
                self.face.or_else(|| crate::scene::Material::control_face(raw_bg)),
            )
            .with_radii((r4[0], r4[1], r4[2], r4[3]))
            .with_depth(depth)
            .with_tint(self.focused.then(crate::widget::ControlPlate::focus_tint));
            ctx.control_plate(&plate);
            return;
        }
        if radius <= 0.0 {
            ctx.quad(Rect { x, y, width: w, height: visual_h }, border_color);
            ctx.quad(
                Rect { x: x + 1.0, y: y + 1.0, width: w - 2.0, height: visual_h - 2.0 },
                bg_color,
            );
            return;
        }

        let inner_radius = (radius - 1.0).max(0.0);
        let mut adjusted = false;
        let mut outer_radii = [radius; 4];
        let mut inner_radii = [inner_radius; 4];

        // Only an explicit corner_frame adjusts concentric corners now — the legacy fallback
        // walked ancestors for a root plate, which no longer exists.
        let frame = self.corner_frame;
        if let Some(((px, py, pw, ph), pr, (pr1, pr2, pr3, pr4))) = frame {
            let g_left = x - px;
            let g_top = y - py;
            let g_right = (px + pw) - (x + w);
            let g_bottom = (py + ph) - (y + visual_h);

            if pr1 && (g_left - g_top).abs() < 1.0 && g_left >= 0.0 {
                outer_radii[0] = (pr - g_left).max(0.0);
                inner_radii[0] = (outer_radii[0] - 1.0).max(0.0);
                adjusted = true;
            }
            if pr2 && (g_right - g_top).abs() < 1.0 && g_right >= 0.0 {
                outer_radii[1] = (pr - g_right).max(0.0);
                inner_radii[1] = (outer_radii[1] - 1.0).max(0.0);
                adjusted = true;
            }
            if pr3 && (g_right - g_bottom).abs() < 1.0 && g_right >= 0.0 {
                outer_radii[2] = (pr - g_right).max(0.0);
                inner_radii[2] = (outer_radii[2] - 1.0).max(0.0);
                adjusted = true;
            }
            if pr4 && (g_left - g_bottom).abs() < 1.0 && g_left >= 0.0 {
                outer_radii[3] = (pr - g_left).max(0.0);
                inner_radii[3] = (outer_radii[3] - 1.0).max(0.0);
                adjusted = true;
            }
        }

        if adjusted {
            for (qx, qy, qw, qh, qr, qc, corners) in crate::layout::partition_concentric_corners(
                x, y, w, visual_h, radius, outer_radii, border_color,
            ) {
                ctx.rounded_rect(Rect { x: qx, y: qy, width: qw, height: qh }, qr, corners, qc);
            }
            for (qx, qy, qw, qh, qr, qc, corners) in crate::layout::partition_concentric_corners(
                x + 1.0, y + 1.0, w - 2.0, visual_h - 2.0, inner_radius, inner_radii, bg_color,
            ) {
                ctx.rounded_rect(Rect { x: qx, y: qy, width: qw, height: qh }, qr, corners, qc);
            }
        } else {
            let corners = (true, true, true, true);
            ctx.rounded_rect(Rect { x, y, width: w, height: visual_h }, radius, corners, border_color);
            ctx.rounded_rect(
                Rect { x: x + 1.0, y: y + 1.0, width: w - 2.0, height: visual_h - 2.0 },
                inner_radius,
                corners,
                bg_color,
            );
        }
    }

    /// Where the arrow is drawn on a trigger band: the `chevron-down` glyph,
    /// [`ARROW_SIDE`] square, centred in the arrow slot at the band's right
    /// end ([`arrow_slot`]), or in the whole band under `center_arrow`.
    /// Until 2026-10-02 it stood 18 px in from the right end, and the
    /// picker's centred arrow stood two pixels left of every other; until
    /// 2026-10-05 it was a "▼" in the control font.
    pub(crate) fn arrow_rect(&self, band: Rect) -> Rect {
        let slot = if self.center_arrow { band.width } else { arrow_slot(band.height).min(band.width) };
        Rect {
            x: band.x + band.width - 0.5 * (slot + ARROW_SIDE),
            y: band.y + 0.5 * (band.height - ARROW_SIDE),
            width: ARROW_SIDE,
            height: ARROW_SIDE,
        }
    }

    /// Emit the selected-text (per-character fade against the right edge) and the arrow glyph —
    /// the legacy `text_labels` body minus the control label (the adapter's base-label
    /// machinery draws that, with the +4px `detached_label_inset`).
    pub(super) fn paint_text(&self, content: Rect, ctx: &mut PaintCtx) {
        let selected_text = if let Some(ref custom_text) = self.custom_display_text {
            custom_text.clone()
        } else {
            // A marked option (see `MARK_SIDE`) shows on the trigger without
            // its mark: the mark is the list's, not the value's.
            let opt = self.options.get(self.selected).map(String::as_str).unwrap_or_default();
            crate::widget::context_menu::split_mark(opt).1.to_string()
        };

        let (font_family, font_size) = crate::layout::control_label_font_detached_parsed();
        let x = content.x;
        let w = content.width;
        let start_x = x + 8.0;
        let right_limit = x + w - 28.0; // 10px margin before the arrow
        let fade_start_x = (right_limit - 24.0).max(start_x); // Fade out over the last 24px
        let text_y = crate::layout::center_text_y(content.y, content.height, font_size);
        let tc = colors::dropdown_text_color();
        let default_color = [
            (colors::linear_to_srgb(tc[0]) * 255.0).round() as u8,
            (colors::linear_to_srgb(tc[1]) * 255.0).round() as u8,
            (colors::linear_to_srgb(tc[2]) * 255.0).round() as u8,
        ];
        let bg_color = colors::dropdown_background_color();
        let mut parent_color = colors::page_color();
        if let Some(snap) = self.parent_snapshot {
            parent_color = snap.color;
        }
        let alpha = 1.0; // The dropdown background is drawn fully opaque
        let bg_rgb = [
            ((parent_color[0] * (1.0 - alpha) + bg_color[0] * alpha) * 255.0).round().clamp(0.0, 255.0) as u8,
            ((parent_color[1] * (1.0 - alpha) + bg_color[1] * alpha) * 255.0).round().clamp(0.0, 255.0) as u8,
            ((parent_color[2] * (1.0 - alpha) + bg_color[2] * alpha) * 255.0).round().clamp(0.0, 255.0) as u8,
        ];

        // The pieces the text is drawn in — `(text, offset, advance)` — each faded on its own.
        // Shaped when there is a font system (a cluster each, at the offset the shaper gives
        // it, which is where the whole string's glyphs would land); measured otherwise.
        let total_advance = text_advance(&selected_text, &font_family, font_size);
        let pieces: Vec<(String, f32, f32)> = match shaped_clusters(&selected_text, font_size) {
            Some(clusters) => clusters
                .windows(2)
                .map(|w| (selected_text[w[0].0..w[1].0].to_string(), w[0].1, w[1].1 - w[0].1))
                .collect(),
            None => measured_pieces(&selected_text, &font_family, font_size, total_advance),
        };

        for (piece, offset, c_w) in pieces {
            let cur_x = start_x + offset;

            if cur_x >= right_limit {
                break;
            }

            let char_mid_x = cur_x + c_w / 2.0;
            let mut skip_char = false;
            let text_end_x = start_x + total_advance;
            let color = if text_end_x > right_limit && char_mid_x > fade_start_x {
                let factor = ((char_mid_x - fade_start_x) / (right_limit - fade_start_x)).clamp(0.0, 1.0);
                if factor >= 0.9 {
                    skip_char = true;
                    default_color
                } else {
                    [
                        (default_color[0] as f32 + (bg_rgb[0] as f32 - default_color[0] as f32) * factor).round() as u8,
                        (default_color[1] as f32 + (bg_rgb[1] as f32 - default_color[1] as f32) * factor).round() as u8,
                        (default_color[2] as f32 + (bg_rgb[2] as f32 - default_color[2] as f32) * factor).round() as u8,
                    ]
                }
            } else {
                default_color
            };

            if !skip_char && !piece.trim().is_empty() {
                // The font named here and not left to the host: a dropdown
                // painted as a stamp by another widget (the designer's
                // palette paints its choice rows so) took that widget's
                // font, and its menu then opened in this one. The whole
                // configured string, as `widget_font` and `draw_popover`
                // pass it — the runner reads its size too.
                ctx.text_with(piece, cur_x, text_y, font_size, color, Some(crate::layout::control_label_font_detached()), None);
            }
        }

        // Only the chevron is bounded here. The trigger TEXT above fades
        // character by character toward `right_limit` and drops anything past
        // 90% — an overflow treatment of its own, which a hard clip would
        // fight rather than help.
        // The glyph is drawn at the configured font's size (the runner
        // reads it off the font string), so that is the size it is measured at.
        let _ = (&font_family, font_size);
        ctx.icon("chevron-down", self.arrow_rect(content), ARROW_COLOR);
    }
}
