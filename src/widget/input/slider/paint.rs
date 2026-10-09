//! What a `Slider` draws: the band, and the readout box (its focus border) and text; while the
//! readout is being typed into, the field claim the on-screen keyboard follows.

use super::*;

impl Paint for Slider {
    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        let r = crate::layout::slider_corner_radius();
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
        // Typing into the readout: the on-screen keyboard follows
        // (`crate::text_input`).
        if self.editing {
            let (ox, oy) = ctx.offset();
            crate::text_input::claim(g.x + ox, g.y + oy, g.w, g.h);
        }
        let radius = crate::layout::slider_corner_radius();
        let rounded = radius > 0.0;
        let rc = (rounded, rounded, rounded, rounded);
        let rrect = |r: Rect, rad: f32, corners: (bool, bool, bool, bool), c: [f32; 4], ctx: &mut PaintCtx| {
            if rounded {
                ctx.rounded_rect(r, rad, corners, c);
            } else {
                ctx.quad(r, c);
            }
        };

        // Readout box (+ focus border) and its text.
        if self.show_readout {
            let readout_w = 60.0;
            let rx = g.x + g.w - readout_w;
            let bg_color = if self.editing { [0.06, 0.10, 0.18, 1.0] } else { [0.10, 0.10, 0.13, 1.0] };
            // NOTE: the legacy square path drew the focus border as 4 edge strips and the
            // rounded path as border+inset; replicate the rounded shape for both (visually
            // identical at 1px) — acceptable divergence flagged in the Phase 5h notes.
            if self.editing {
                rrect(Rect { x: rx, y: g.y, width: readout_w, height: g.h }, radius, rc, [0.20, 0.50, 0.85, 1.0], ctx);
                rrect(
                    Rect { x: rx + 1.0, y: g.y + 1.0, width: readout_w - 2.0, height: g.h - 2.0 },
                    (radius - 1.0).max(0.0),
                    rc,
                    bg_color,
                    ctx,
                );
            } else {
                rrect(Rect { x: rx, y: g.y, width: readout_w, height: g.h }, radius, rc, bg_color, ctx);
            }

            let text = if self.editing { self.edit_buffer.clone() } else { self.scaled_string() };
            // Clipped to the readout well. While `editing` this is whatever the
            // user has typed, which has no length limit at all — unbounded it
            // ran straight out of the readout and across the band beside it.
            ctx.text_with(
                text,
                rx + 8.0,
                crate::layout::align_text_y(g.y, g.h, 12.0, 0.0),
                12.0,
                [0xee, 0xee, 0xf0],
                None,
                Some([rx, g.y, rx + readout_w, g.y + g.h]),
            );
        }

        self.paint_band(&g, ctx);
    }
}
