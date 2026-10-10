//! What the spreadsheet draws: the header (its hover and the sort glyph), zebra rows and the
//! selection, column dividers, the fore copy of the scrollbars, and the cells on screen, each
//! clamped and ellipsized to its column.

use super::*;

impl Paint for Spreadsheet {
    /// The pane IS its own background plate, wearing the parameter plate's fill —
    /// same tint, opacity, and blur-behind marker (`param_plate_fill`) — so it
    /// bevels like the params plate and tracks a live retint / opacity / blur
    /// toggle with it.
    fn color(&self) -> [f32; 4] {
        color::param_plate_fill()
    }

    /// The shared plate corner radius (rounded on all four corners when non-zero),
    /// matching the rounded clip hosts carve for the pane's content.
    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        let r = crate::layout::plate_corner_radius();
        let on = r > 0.0;
        Some((r, (on, on, on, on)))
    }

    /// The shared plate border — under `control_relief` the host promotes this to
    /// the plate bevel, like `ParametersBg`.
    fn solid_border(&self) -> Option<([f32; 4], f32)> {
        color::plate_border_color().map(|bc| (bc, color::plate_border_thickness()))
    }

    /// Subtree painter: `paint` authors the pane's complete text with
    /// per-column clamp bounds, so its Text prims must pass through
    /// `paint_self` verbatim — the own-labels re-derivation drops per-prim
    /// bounds, which is exactly how long cell values used to overlap into
    /// their neighbor columns on narrow panes.
    fn paints_own_subtree(&self) -> bool {
        true
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);
        let scroll = self.geom(rect).map_or(0.0, |g| g.scroll);

        // Header bg
        ctx.quad(Rect { x, y, width: w, height: HEADER_H }, [0.12, 0.12, 0.16, 0.4]);

        // Zebra rows + separators, clipped to the body band
        let body_top = y + HEADER_H;
        let body_bottom = y + h;
        let selected_fill = {
            let [r, g, b, _] = color::highlight_primary_color();
            [r, g, b, 0.28]
        };
        for i in 0..self.row_count {
            let ry = y + HEADER_H + i as f32 * ROW_H - scroll;
            if ry + ROW_H <= body_top || ry >= body_bottom {
                continue;
            }
            let draw_y = ry.max(body_top);
            let draw_h = (ry + ROW_H).min(body_bottom) - draw_y;
            if draw_h > 0.0 {
                let row_color = if i % 2 == 0 {
                    [0.10, 0.10, 0.13, 0.15]
                } else {
                    [0.08, 0.08, 0.11, 0.05]
                };
                ctx.quad(Rect { x, y: draw_y, width: w, height: draw_h }, row_color);
                if self.order.get(i).is_some_and(|src| self.selected.contains(src)) {
                    ctx.quad(Rect { x, y: draw_y, width: w, height: draw_h }, selected_fill);
                }

                let sep_y = ry + ROW_H;
                if sep_y >= body_top && sep_y < body_bottom {
                    ctx.quad(Rect { x, y: sep_y, width: w, height: 1.0 }, [0.20, 0.20, 0.25, 0.15]);
                }
            }
        }

        // Header separator
        ctx.quad(Rect { x, y: y + HEADER_H, width: w, height: 1.0 }, [0.20, 0.20, 0.25, 0.25]);

        // Vertical column dividers, at scrolled column edges, kept inside the pane.
        let hscroll = self.hscroll(rect);
        // The last edge too: the table stops short of a wide pane, and the
        // line says where.
        let edges = self.col_edges();
        if h > 0.0 && !self.headers.is_empty() {
            for &edge in &edges[1..] {
                let dx = x + edge - hscroll;
                if dx <= x || dx >= x + w {
                    continue;
                }
                ctx.quad(Rect { x: dx, y, width: 1.0, height: h }, [0.20, 0.20, 0.25, 0.15]);
            }
        }

        // The scrollbars' fore copy, over the cells, at the activity's fade —
        // the fade rather than the latch, so it draws all the way out. The
        // copy behind the plate is the host's (`paint_scrollbars`).
        self.paint_scrollbars(rect, ctx, self.activity.fade());

        // Header + cell text, each cell clamped to its column and the body band.
        if self.headers.is_empty() {
            return;
        }
        let n_cols = self.headers.len();
        // Scrolled column origin; columns fully outside the pane skip.
        let xoff = x - hscroll;
        let col_visible = |col: usize| -> bool {
            xoff + edges[col + 1] > x && xoff + edges[col] < x + w
        };
        if let Some(hc) = self.header_hover_col.filter(|&hc| hc < n_cols) {
            // Subtle hover tint on the clickable header cell (the Processes-page
            // sortable-header convention), clamped to the pane.
            let hx0 = (xoff + edges[hc]).max(x);
            let hx1 = (xoff + edges[hc + 1]).min(x + w);
            if hx1 > hx0 {
                ctx.quad(Rect { x: hx0, y, width: hx1 - hx0, height: HEADER_H }, [1.0, 1.0, 1.0, 0.05]);
            }
        }
        // Every label clamps to its own column (a 4px gutter short of the
        // divider) AND to the pane, so a long value cuts off instead of
        // running under its neighbor, and half-scrolled edge columns stop at
        // the plate instead of bleeding past it.
        let col_bounds = |col: usize, top: f32, height: f32| -> Option<[f32; 4]> {
            let x0 = (xoff + edges[col]).max(x);
            let x1 = (xoff + edges[col + 1] - 4.0).min(x + w);
            if x1 <= x0 {
                return None;
            }
            Some([x0, top, x1, top + height])
        };
        // Ellipsize what the clamp would cut, so truncation reads as
        // deliberate. A char budget from ONE cached measurement is exact
        // because the DE label font is monospace; the clamp bounds stay on
        // as the backstop for any fallback-font drift. Below three columns'
        // worth of budget the mark would REPLACE the content (a 19px column
        // fits one glyph — a bare "…" says less than a clipped digit), so
        // very narrow columns keep the raw string and let the clamp cut it.
        let char_w = Self::char_w();
        // Columns fit their content, so the budget is met but for a
        // fallback font's drift.
        let budget_of = |col: usize| (((edges[col + 1] - edges[col]) - 12.0) / char_w).floor() as usize;
        let fit = |s: String, budget: usize| -> String {
            if budget < 3 || s.chars().count() <= budget {
                return s;
            }
            let mut out: String = s.chars().take(budget - 1).collect();
            out.push('\u{2026}');
            out
        };
        for (i, header) in self.headers.iter().enumerate() {
            if !col_visible(i) {
                continue;
            }
            let cx = xoff + edges[i] + 8.0;
            let budget = budget_of(i);
            match self.sort {
                Some((col, ascending)) if col == i => {
                    // The sorted column: its name, then the `chevron-up` or
                    // `chevron-down` glyph a cell after it — the room of
                    // the " ▲" it was until 2026-10-05, taken out of the
                    // name's budget.
                    let b = budget.saturating_sub(2);
                    let shown: String = if b < 3 || header.chars().count() <= b {
                        header.clone()
                    } else {
                        let mut out: String = header.chars().take(b - 1).collect();
                        out.push('\u{2026}');
                        out
                    };
                    let tx = cx + shown.chars().count() as f32 * char_w + 0.5 * char_w;
                    ctx.text_with(shown, cx, y + 6.0, 12.0, [0xff, 0xff, 0xff], None, col_bounds(i, y, HEADER_H));
                    const SIDE: f32 = 8.0;
                    let r = Rect { x: tx, y: y + 0.5 * (HEADER_H - SIDE), width: SIDE, height: SIDE };
                    ctx.icon(if ascending { "chevron-up" } else { "chevron-down" }, r, [1.0, 1.0, 1.0, 1.0]);
                }
                _ => ctx.text_with(fit(header.clone(), budget), cx, y + 6.0, 12.0, [0xdd, 0xdd, 0xee], None, col_bounds(i, y, HEADER_H)),
            }
        }
        // A row half scrolled under the header or off the bottom draws its
        // text CUT at the body band, as its zebra fill is. Until 2026-09-30
        // it drew no text until it was wholly inside, so a scrolling row's
        // band arrived empty and its values popped in a row's height late.
        for (i, &src) in self.order.iter().enumerate() {
            let ry = y + HEADER_H + i as f32 * ROW_H - scroll;
            let top = ry.max(body_top);
            let bottom = (ry + ROW_H).min(body_bottom);
            if bottom <= top {
                continue;
            }
            // Written here, for the cells on screen only (`SheetColumn`).
            for (col_idx, column) in self.columns.iter().enumerate().take(n_cols) {
                if !col_visible(col_idx) {
                    continue;
                }
                let cx = xoff + edges[col_idx] + 8.0;
                ctx.text_with(fit(column.cell(src), budget_of(col_idx)), cx, ry + 6.0, 12.0, [0xbb, 0xbb, 0xcc], None, col_bounds(col_idx, top, bottom - top));
            }
        }
    }
}
