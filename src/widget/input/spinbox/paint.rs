//! What a `Spinbox` draws: the field with its -/+ run (or, relief off, its flat frame), the value
//! or the text being typed with its caret, and the - / + glyphs.

use super::*;

impl Paint for Spinbox {
    fn prepare_text(&mut self, fs: &mut cosmic_text::FontSystem, _rect: Rect) {
        // Shape the displayed value exactly as `ctx.text` draws it (size 14,
        // default family) and record char-index → x. Cluster offsets arrive
        // keyed by byte; the editor state is char-indexed.
        let text = self.value_text();
        let clusters =
            crate::text::shaped_cluster_offsets(fs, &text, 14.0, None);
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
        [0.0, 0.0, 0.0, 0.0]
    }

    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        let r = crate::layout::spinbox_corner_radius();
        if r > 0.0 {
            Some((r, (true, true, true, true)))
        } else {
            None
        }
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::control_label_font_detached())
    }

    fn sync_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let g = self.geom(rect);
        // Typing a value: the on-screen keyboard follows (`crate::text_input`).
        if self.editing {
            let (ox, oy) = ctx.offset();
            crate::text_input::claim(g.x + ox, g.y + oy, g.w, g.h);
        }
        let radius = crate::layout::spinbox_corner_radius();
        let rounded = radius > 0.0;
        let display_bg = if self.editing { [0.06, 0.10, 0.18, 1.0] } else { color::spinbox_display() };
        let inc_col = if self.hover_inc { color::spinbox_button_hover() } else { color::spinbox_button() };
        let dec_col = if self.hover_dec { color::spinbox_button_hover() } else { color::spinbox_button() };

        if crate::layout::control_relief() {
            // The DE relief style: transparent faces, the relief is the
            // chrome (see [`Self::relief_parts`]). Flat prims first — hover
            // washes and the editing cue survive a flat host's rounded-quad
            // bridge, the carves are re-emitted host-side.
            if let Some(rel) = self.relief_parts(rect) {
                // Carved inside the control as every well is.
                let r = rel.radius;
                let (field, radii) = crate::layout::carve_inside(rel.rect, (r, r, r, r), rel.depth);
                if let Some((split, _)) = rel.run {
                    let wash = [1.0, 1.0, 1.0, 0.06];
                    let sx = g.seam_x;
                    let (top, h) = (field.y, field.height);
                    if self.hover_dec {
                        ctx.rounded_rect(Rect { x: split, y: top, width: sx - split, height: h }, 0.0, (false, false, false, false), wash);
                    }
                    if self.hover_inc {
                        ctx.rounded_rect(
                            Rect { x: sx, y: top, width: field.x + field.width - sx, height: h },
                            radii.1,
                            (false, true, true, false),
                            wash,
                        );
                    }
                }
                if self.editing {
                    // Editing cue: an accent hairline on the well floor under
                    // the value, plus the caret — a flat stand-in for the
                    // tinted-recess focus treatment the pane's relief tuple
                    // cannot carry.
                    let accent = color::highlight_primary_color();
                    ctx.quad(
                        Rect { x: g.x + crate::layout::CONTROL_TEXT_INSET, y: g.y + g.h - 4.0, width: g.w * 0.55 - 8.0, height: 1.5 },
                        accent,
                    );
                    let cursor_x = (g.x + crate::layout::CONTROL_TEXT_INSET + self.caret_offset(self.cursor_idx)).min(g.x + g.w * 0.55 - 4.0);
                    let cursor_y = g.y + (g.h - 14.0) / 2.0;
                    ctx.quad(Rect { x: cursor_x, y: cursor_y, width: 1.5, height: 14.0 }, [0.80, 0.80, 0.85, 1.0]);
                }
                // A field in its two forms: the value's well ending in the
                // -/+ run, or (no room for the buttons) all well.
                use crate::scene::paint::Field;
                match rel.run {
                    Some((split, (sa, sb, sw, host))) => {
                        ctx.field(&Field::ending_in_run(field, radii, rel.depth, split));
                        ctx.groove(sa, sb, sw, rel.depth, host);
                    }
                    None => ctx.field(&Field::well(field, radii, rel.depth)),
                }
            }
        } else if rounded {
            let rc = (true, true, true, true);
            let border_color = if self.editing {
                [0.20, 0.50, 0.85, 1.0]
            } else if self.hovered {
                [0.25, 0.25, 0.35, 1.0]
            } else {
                [0.18, 0.18, 0.24, 1.0]
            };
            ctx.rounded_rect(Rect { x: g.x, y: g.y, width: g.w, height: g.h }, radius, rc, border_color);
            ctx.rounded_rect(
                Rect { x: g.x + 1.0, y: g.y + 1.0, width: g.w - 2.0, height: g.h - 2.0 },
                radius - 1.0,
                rc,
                display_bg,
            );
            if g.btn_h > 0.0 && g.btn_w > 0.0 {
                ctx.rounded_rect(
                    Rect { x: g.split_dec + g.pad, y: g.btn_y, width: g.btn_w, height: g.btn_h },
                    0.0,
                    (false, false, false, false),
                    dec_col,
                );
                ctx.rounded_rect(
                    Rect { x: g.split_dec + g.pad + g.btn_w, y: g.btn_y, width: g.btn_w, height: g.btn_h },
                    radius,
                    (false, true, true, false),
                    inc_col,
                );
            }
            if self.editing {
                let cursor_x = (g.x + crate::layout::CONTROL_TEXT_INSET + self.caret_offset(self.cursor_idx)).min(g.x + g.w * 0.55 - 4.0);
                let cursor_y = g.y + (g.h - 14.0) / 2.0;
                ctx.rounded_rect(
                    Rect { x: cursor_x, y: cursor_y, width: 1.5, height: 14.0 },
                    0.0,
                    (false, false, false, false),
                    [0.80, 0.80, 0.85, 1.0],
                );
            }
        } else {
            ctx.quad(Rect { x: g.x, y: g.y, width: g.w, height: g.h }, display_bg);
            if g.btn_h > 0.0 && g.btn_w > 0.0 {
                ctx.quad(Rect { x: g.split_dec + g.pad, y: g.btn_y, width: g.btn_w, height: g.btn_h }, dec_col);
                ctx.quad(Rect { x: g.split_dec + g.pad + g.btn_w, y: g.btn_y, width: g.btn_w, height: g.btn_h }, inc_col);
            }
            if self.editing {
                let border_color = [0.20, 0.50, 0.85, 1.0];
                ctx.quad(Rect { x: g.x, y: g.y, width: g.w, height: 1.0 }, border_color);
                ctx.quad(Rect { x: g.x, y: g.y + g.h - 1.0, width: g.w, height: 1.0 }, border_color);
                ctx.quad(Rect { x: g.x, y: g.y, width: 1.0, height: g.h }, border_color);
                ctx.quad(Rect { x: g.x + g.w - 1.0, y: g.y, width: 1.0, height: g.h }, border_color);

                let cursor_x = (g.x + crate::layout::CONTROL_TEXT_INSET + self.caret_offset(self.cursor_idx)).min(g.x + g.w * 0.55 - 4.0);
                let cursor_y = g.y + (g.h - 14.0) / 2.0;
                ctx.quad(Rect { x: cursor_x, y: cursor_y, width: 1.5, height: 14.0 }, [0.80, 0.80, 0.85, 1.0]);
            }
        }

        // Value, unit, and the minus/plus glyphs.
        let tc = color::spinbox_text_color();
        let text_color = [(tc[0] * 255.0) as u8, (tc[1] * 255.0) as u8, (tc[2] * 255.0) as u8];
        // The value and its unit live in the FIELD, which ends where the -/+
        // buttons begin (`split_dec`). The caret above is already clamped to
        // that field; the text it belongs to was not, so a long value ran
        // under the buttons and out of the control.
        let field = Some([g.x, g.y, g.text_end, g.y + g.h]);
        let value_y = crate::layout::align_text_y(g.y, g.h, 14.0, 0.0);
        ctx.text_with(self.value_text(), g.x + crate::layout::CONTROL_TEXT_INSET, value_y, 14.0, text_color, None, field);
        if let Some(ref unit) = self.unit {
            // The unit follows the value at a word space, measured off the
            // value's own shaped width (the caret's offsets, see
            // `prepare_text`). It stood at a fixed 36px, which left "5" and
            // its "s" a gap apart and would have run a wide value into it.
            // It shares the value's line top rather than being centred on its
            // own: the renderer centres each glyph run in a box one font size
            // tall, so the smaller unit, centred separately, sat ~2px below
            // the value's baseline. From one top, the two baselines meet.
            let value_w = self.caret_offset(self.value_text().chars().count());
            const UNIT_GAP: f32 = 4.0;
            let ux = g.x + crate::layout::CONTROL_TEXT_INSET + value_w + UNIT_GAP;
            ctx.text_with(unit.clone(), ux, value_y, 11.0, [0x73, 0x73, 0x7a], None, field);
        }
        if g.btn_w > 0.0 {
            let dec_center_x = g.dec_c;
            let inc_center_x = g.inc_c;
            // The `minus` and `plus` glyphs, in the value's colour, centred
            // on their buttons (they were "-" and "+" in the 12px face).
            const SIDE: f32 = 8.0;
            let gy = g.y + 0.5 * (g.h - SIDE);
            let tint = [tc[0], tc[1], tc[2], 1.0];
            let glyph = |cx: f32| crate::scene::layout::Rect { x: cx - 0.5 * SIDE, y: gy, width: SIDE, height: SIDE };
            ctx.icon("minus", glyph(dec_center_x), tint);
            ctx.icon("plus", glyph(inc_center_x), tint);
        }
    }
}
