//! What a `ColorSelector` draws: the field (a well, or a hairline frame), the hex text and its
//! caret, and the swatch, over a checker when it has alpha.

use super::*;

impl Paint for ColorSelector {
    fn prepare_text(&mut self, fs: &mut cosmic_text::FontSystem, _rect: Rect) {
        // Shape the drawn hex string exactly as `ctx.text` draws it (size 12,
        // default family) and record char-index → x for the caret.
        let text = if self.editing { self.edit_buffer.clone() } else { self.value_hex() };
        let clusters =
            crate::backend::text::shaped_cluster_offsets(fs, &text, 12.0, None);
        let mut offsets = vec![0.0f32; text.chars().count() + 1];
        for (byte, x) in clusters {
            let ci = text[..byte.min(text.len())].chars().count();
            if ci < offsets.len() {
                offsets[ci] = x;
            }
        }
        let mut current = 0.0;
        for off in offsets.iter_mut() {
            if *off == 0.0 {
                *off = current;
            } else {
                current = *off;
            }
        }
        self.glyph_offsets = offsets;
    }

    fn color(&self) -> [f32; 4] {
        colors::to_linear([
            self.color[0] as f32 / 255.0,
            self.color[1] as f32 / 255.0,
            self.color[2] as f32 / 255.0,
            self.alpha as f32 / 255.0,
        ])
    }

    fn widget_font(&self) -> Option<String> {
        Some(self.font_family.clone())
    }

    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        let r = crate::layout::color_selector_corner_radius();
        if r > 0.0 {
            Some((r, (true, true, true, true)))
        } else {
            None
        }
    }

    /// The legacy `extra_quads` body against the laid-out rect (field, caret while
    /// editing, and the soft-glow rounded color preview), plus the hex readout label.
    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        // The well is color_selector_height tall, seated at the content rect's TOP —
        // the label rides in the strip above it and the row's bottom band belongs
        // to the NEXT row's label. Hosts that hand
        // over a whole param row (ParametersBg's 40px color rows) get a
        // standard control-height well instead of a row-tall one.
        let well_h = crate::layout::color_selector_height().min(rect.height);
        let rect = Rect { x: rect.x, y: rect.y, width: rect.width, height: well_h };
        let mut quads: Vec<(f32, f32, f32, f32, [f32; 4])> = Vec::new();
        let visual_h = rect.height;
        let pick_x = rect.x + rect.width * 0.65;
        let pick_w = rect.width * 0.35;
        // Typing a hex value: the on-screen keyboard follows
        // (`crate::text_input`).
        if self.editing {
            let (ox, oy) = ctx.offset();
            crate::text_input::claim(rect.x + ox, rect.y + oy, pick_x - rect.x, visual_h);
        }

        // The text field has NO face of its own — a frame over the host plate,
        // like a relief TextBox well (transparent fill, the outline defines
        // it) and the closed-dropdown convention. The first colorless pass
        // used the textbox background here, but under the DE's relief themes
        // real text wells draw no fill, so even a neutral one read as "the
        // color selector has a background". Neutral greys for the frame; the
        // caret is the editing affordance.
        let border_color = crate::colors::well_frame_color(self.hovered, self.editing);

        // A real frame, not a border-quad-under-fill-quad: with no fill, the
        // old full-rect border quad would read as a solid slab. Rounded at the
        // selector's radius like the well it stands in for.
        if !self.recessed() {
            let fr = crate::layout::color_selector_corner_radius();
            ctx.border(rect, (fr, fr, fr, fr), [0.0; 4], border_color, 1.0);
        }

        if self.editing {
            let font_size = 12.0;
            let text_w = self.glyph_offsets.get(self.cursor_idx).copied().unwrap_or_else(|| {
                let cursor_text: String = self.edit_buffer.chars().take(self.cursor_idx).collect();
                crate::widget::display::measure_text(&cursor_text, font_size)
            });
            let caret_x = rect.x + crate::layout::CONTROL_TEXT_INSET + text_w;
            let caret_h = font_size * 1.15;
            let caret_y = rect.y + (visual_h - caret_h) / 2.0;
            quads.push((caret_x, caret_y, 1.5, caret_h, [0.80, 0.80, 0.85, 1.0]));
        }

        let linear_c = colors::to_linear([
            self.color[0] as f32 / 255.0,
            self.color[1] as f32 / 255.0,
            self.color[2] as f32 / 255.0,
            self.alpha as f32 / 255.0,
        ]);
        let border_w = 1.0;
        let border_c = colors::color_borders_color();
        
        let r = border_c[0];
        let g = border_c[1];
        let b = border_c[2];

        let preview_radius = crate::layout::color_selector_preview_corner_radius();
        let preview_margin = crate::layout::color_selector_preview_margin();

        let px = pick_x + preview_margin;
        let py = rect.y + preview_margin;
        let pw = (pick_w - 2.0 * preview_margin).max(0.0);
        let ph = (visual_h - 2.0 * preview_margin).max(0.0);

        let add_rounded_rect = |quads: &mut Vec<(f32, f32, f32, f32, [f32; 4])>, color: [f32; 4], x: f32, y: f32, w: f32, h: f32, radius: f32| {
            let radius = radius.min(w * 0.5).min(h * 0.5);
            if radius <= 0.5 {
                quads.push((x, y, w, h, color));
                return;
            }
            
            quads.push((x + radius, y, w - 2.0 * radius, h, color));
            quads.push((x, y + radius, radius, h - 2.0 * radius, color));
            quads.push((x + w - radius, y + radius, radius, h - 2.0 * radius, color));
            
            let steps = radius.round() as i32;
            for i in 0..steps {
                let dy = i as f32;
                let next_dy = (i + 1) as f32;
                
                let cx = (radius * radius - (radius - dy) * (radius - dy)).sqrt();
                let next_cx = (radius * radius - (radius - next_dy) * (radius - next_dy)).sqrt();
                let avg_cx = (cx + next_cx) * 0.5;
                
                let strip_w = avg_cx;
                let strip_h = 1.0f32;
                
                if strip_w > 0.0 {
                    quads.push((x + radius - strip_w, y + dy, strip_w, strip_h, color));
                    quads.push((x + w - radius, y + dy, strip_w, strip_h, color));
                    quads.push((x + radius - strip_w, y + h - dy - strip_h, strip_w, strip_h, color));
                    quads.push((x + w - radius, y + h - dy - strip_h, strip_w, strip_h, color));
                }
            }
        };

        if self.recessed() {
            // The Breadcrumb composition: ONE well across the whole control,
            // the hex text on its floor at the left and the swatch as the
            // colour laid flush on the floor's right segment (the well's
            // rounded end is its own), the two parted by a seam groove. The
            // caret quad first (it rides on the floor), the carve after the
            // fills so the walls' shading falls over both, the seam last so it
            // dies into the well's rolled edge. Over a checker where the
            // colour carries alpha, so the transparency reads through.
            for (qx, qy, qw, qh, qc) in quads.drain(..) {
                ctx.quad(Rect { x: qx, y: qy, width: qw, height: qh }, qc);
            }
            let Some((wx, wy, ww, wh, radius, depth)) = self.field_relief(rect) else {
                return;
            };
            let well = Rect { x: wx, y: wy, width: ww, height: wh };
            // The floor: past the wall's inner half-span (the ProgressBar's inset).
            let floor = Rect {
                x: rect.x + depth,
                y: rect.y + depth,
                width: (rect.width - 2.0 * depth).max(0.0),
                height: (visual_h - 2.0 * depth).max(0.0),
            };
            let seam_x = pick_x;
            let swatch = Rect { x: seam_x, y: floor.y, width: (floor.x + floor.width - seam_x).max(0.0), height: floor.height };
            let sr = crate::layout::textbox_corner_radius().min(swatch.height * 0.5);
            let right_end = (false, true, true, false);
            if self.with_alpha {
                ctx.rounded_rect(swatch, sr, right_end, [0.8, 0.8, 0.8, 1.0]);
                // The white cells, those at the right edge trimmed to the end's
                // arc at their own row (the Breadcrumb's banded-wash sampling).
                //
                // Zero-radius rounded rects, NOT plain quads. The adapter's
                // legacy plain-quad view (`own_plain_quads`) is this paint
                // filtered to `Prim::Quad`, and a host that reads that view
                // beside the paint — the designer, through ParametersBg's
                // `plain_quads` — draws it AFTER the prims. As quads the
                // cells came back a second time OVER the colour fill below,
                // so every colour but white showed a checker as if it were
                // transparent (invisible with the default white, 2026-09-21).
                let grid = 6.0;
                let cols = (swatch.width / grid).ceil() as i32;
                let rows = (swatch.height / grid).ceil() as i32;
                let arc = |yc: f32| -> f32 {
                    let dy = if yc < swatch.y + sr {
                        sr - (yc - swatch.y)
                    } else if yc > swatch.y + swatch.height - sr {
                        yc - (swatch.y + swatch.height - sr)
                    } else {
                        return 0.0;
                    };
                    sr - (sr * sr - dy * dy).max(0.0).sqrt()
                };
                for r in 0..rows {
                    for c in 0..cols {
                        if (r + c) % 2 == 1 {
                            let qx = swatch.x + c as f32 * grid;
                            let qy = swatch.y + r as f32 * grid;
                            let qh = grid.min(swatch.y + swatch.height - qy);
                            let right_edge = swatch.x + swatch.width - arc(qy + qh * 0.5);
                            let qw = grid.min(right_edge - qx);
                            if qw > 0.0 && qh > 0.0 {
                                ctx.rounded_rect(
                                    Rect { x: qx, y: qy, width: qw, height: qh },
                                    0.0,
                                    (false, false, false, false),
                                    [1.0, 1.0, 1.0, 1.0],
                                );
                            }
                        }
                    }
                }
            }
            ctx.rounded_rect(swatch, sr, right_end, linear_c);
            let radii = (radius, radius, radius, radius);
            if self.editing {
                let hc = crate::color::highlight_primary_color();
                ctx.recess_tinted(well, radii, depth, [hc[0], hc[1], hc[2]]);
            } else {
                ctx.recess(well, radii, depth);
            }
            ctx.groove(
                (seam_x, well.y),
                (seam_x, well.y + well.height),
                crate::widget::Breadcrumb::SEAM_WIDTH,
                depth,
                well,
            );
            let hex = if self.editing { self.edit_buffer.clone() } else { self.value_hex() };
            // Bounded by the seam: the field is the well left of it, and while
            // `editing` this holds whatever has been typed, not a 7-character
            // hex code.
            ctx.text_with(
                hex,
                rect.x + crate::layout::CONTROL_TEXT_INSET,
                crate::layout::align_text_y(rect.y, rect.height, 12.0, 0.0),
                12.0,
                [0xcc, 0xcc, 0xd4],
                None,
                Some([rect.x, rect.y, seam_x, rect.y + rect.height]),
            );
            return;
        }

        let steps = 6;
        for i in (1..=steps).rev() {
            let offset = i as f32 * 0.75;
            let rx = px - offset;
            let ry = py - offset;
            let rw = pw + 2.0 * offset;
            let rh = ph + 2.0 * offset;
            let alpha = 0.08 * (1.0 - (i as f32 / steps as f32).powf(1.5));
            if alpha > 0.001 {
                add_rounded_rect(&mut quads, [r, g, b, alpha], rx, ry, rw, rh, preview_radius + offset);
            }
        }

        add_rounded_rect(&mut quads, border_c, px, py, pw, ph, preview_radius);

        if self.with_alpha {
            add_rounded_rect(
                &mut quads,
                [0.8, 0.8, 0.8, 1.0],
                px + border_w,
                py + border_w,
                (pw - 2.0 * border_w).max(0.0),
                (ph - 2.0 * border_w).max(0.0),
                (preview_radius - border_w).max(0.0),
            );
            
            let grid_size = 6.0;
            let start_x = px + border_w;
            let start_y = py + border_w;
            let inner_w = (pw - 2.0 * border_w).max(0.0);
            let inner_h = (ph - 2.0 * border_w).max(0.0);
            
            let cols = (inner_w / grid_size).ceil() as i32;
            let rows = (inner_h / grid_size).ceil() as i32;
            for r in 0..rows {
                for c in 0..cols {
                    if (r + c) % 2 == 1 {
                        let qx = start_x + c as f32 * grid_size;
                        let qy = start_y + r as f32 * grid_size;
                        let qw = grid_size.min(start_x + inner_w - qx);
                        let qh = grid_size.min(start_y + inner_h - qy);
                        if qw > 0.0 && qh > 0.0 {
                            quads.push((qx, qy, qw, qh, [1.0, 1.0, 1.0, 1.0]));
                        }
                    }
                }
            }
        }

        add_rounded_rect(
            &mut quads,
            linear_c,
            px + border_w,
            py + border_w,
            (pw - 2.0 * border_w).max(0.0),
            (ph - 2.0 * border_w).max(0.0),
            (preview_radius - border_w).max(0.0),
        );
    
        for (qx, qy, qw, qh, qc) in quads {
            ctx.quad(Rect { x: qx, y: qy, width: qw, height: qh }, qc);
        }

        let hex = if self.editing { self.edit_buffer.clone() } else { self.value_hex() };
        ctx.text_with(
            hex,
            rect.x + crate::layout::CONTROL_TEXT_INSET,
            crate::layout::align_text_y(rect.y, rect.height, 12.0, 0.0),
            12.0,
            [0xcc, 0xcc, 0xd4],
            None,
            Some([rect.x, rect.y, rect.x + rect.width, rect.y + rect.height]),
        );
    }
}
