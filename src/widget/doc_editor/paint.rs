//! Painting: laying out what shows, then the selection, the text and its decorations, embeds, and
//! the caret.

use super::*;

impl DocEditor {
    /// Paint into `rect`: lay out what shows, then selection, text, caret.
    /// `focused` draws the caret.
    pub fn paint(&mut self, pc: &mut PaintCtx, rect: Rect, focused: bool) {
        self.prepare(rect);
        self.paint_prepared(pc, focused);
    }

    /// Lay out what shows in `rect` and settle the scroll, without
    /// painting: after it, [`DocEditor::caret_rect`] is where the caret
    /// will be drawn (a popup placed before the paint needs that).
    pub fn prepare(&mut self, rect: Rect) {
        let mut width = (rect.width - 2.0 * self.pad).max(40.0);
        if self.max_width > 0.0 {
            width = width.min(self.max_width);
        }
        if (width - self.width).abs() > 0.5 {
            self.width = width;
            self.invalidate();
            for i in 0..self.heights.len() {
                self.heights[i] = 0.0;
            }
        }
        self.viewport = rect;
        self.origin = (rect.x + (rect.width - width) / 2.0, rect.y + self.pad);
        self.sync();
        self.sync_ime();
        if std::mem::take(&mut self.follow_caret) {
            self.scroll_to_caret();
        }
        // Lay out what shows (heights settle), then paint it.
        let view_h = rect.height;
        for _ in 0..2 {
            let mut i = self.line_at_y(self.scroll - self.pad);
            while i < self.buf.line_count() && self.tops[i] < self.scroll + view_h {
                self.ensure(i);
                i += 1;
            }
            self.retop();
        }
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());
    }

    /// Paint what the last [`DocEditor::prepare`] laid out.
    pub fn paint_prepared(&mut self, pc: &mut PaintCtx, focused: bool) {
        self.paint_prepared_with(pc, focused, &|_| true);
    }

    /// [`DocEditor::paint_prepared`], with `resolved` saying whether a
    /// link has a target: one that does not draws faded, as in Obsidian.
    /// Asked at paint, so a vault change needs no relayout.
    pub fn paint_prepared_with(&mut self, pc: &mut PaintCtx, focused: bool, resolved: &dyn Fn(&Target) -> bool) {
        let rect = self.viewport;
        let view_h = rect.height;
        let (ox, oy) = (self.origin.0, self.origin.1 - self.scroll);
        let sel = self.buf.selection();
        let caret = self.buf.caret;
        let caret_col = self.laid_col(caret.line, caret.col, true);
        let composition = self.composition_on(caret.line);
        let th = self.theme.clone();
        // Focused is typing: the on-screen keyboard follows
        // (`crate::text_input`). The viewport stands in until the caret's
        // line is drawn below, and stays when it is scrolled out of view.
        if focused {
            let (dx, dy) = pc.offset();
            crate::text_input::claim(rect.x + dx, rect.y + dy, rect.width, rect.height);
        }
        let first = self.line_at_y(self.scroll - self.pad);
        pc.clip(rect, |pc| {
            let mut i = first;
            while i < self.buf.line_count() && self.tops[i] < self.scroll + view_h {
                let Some(l) = self.layouts[i].as_ref() else {
                    i += 1;
                    continue;
                };
                let top = oy + self.tops[i];
                // Selection, behind everything on the line.
                // The boxes of the selected clusters, so a selection crossing a change of
                // direction is drawn where its letters are.
                if let Some((a, b)) = sel {
                    if i >= a.line && i <= b.line {
                        let from = if i == a.line { a.col } else { 0 };
                        let to = if i == b.line { b.col } else { usize::MAX };
                        for (row, x0, x1) in l.selection_rects(from, to, i < b.line) {
                            if x1 > x0 {
                                pc.quad(Rect { x: ox + x0, y: top + row as f32 * l.row_h, width: x1 - x0, height: l.row_h }, th.selection);
                            }
                        }
                    }
                }
                for d in &l.decos {
                    match d {
                        Deco::Quad(r, c) => pc.quad(Rect { x: ox + r.x, y: top + r.y, width: r.width, height: r.height }, *c),
                        Deco::Dot { cx, cy, r, color } => pc.circle(ox + cx, top + cy, *r, *color),
                        Deco::Check { cx, cy, r, checked } => crate::widget::Checkbox::paint_inline(pc, ox + cx, top + cy, *r, *checked),
                        Deco::Text { text, x, y, size, color, font } => {
                            pc.text_with(text.clone(), ox + x, top + y, *size, srgb_u8(*color), Some(font.clone()), None)
                        }
                        Deco::Image { target, rect: r } => {
                            let at = Rect { x: ox + r.x, y: top + r.y, width: r.width, height: r.height };
                            match self.images.as_ref().and_then(|f| f(target)) {
                                Some(img) => pc.image(img.id, at, 1.0),
                                None => pc.rounded_rect(at, 4.0, (true, true, true, true), th.code_bg),
                            }
                        }
                    }
                }
                for r in &l.runs {
                    let row_y = top + r.row as f32 * l.row_h;
                    if let Some(bg) = r.bg {
                        let (pad, radius) = if r.look.pill { (layout::PILL_PAD, (l.row_h - 4.0) / 2.0) } else { (2.0, 3.0) };
                        pc.rounded_rect(Rect { x: ox + r.x - pad, y: row_y + 2.0, width: r.w + 2.0 * pad, height: l.row_h - 4.0 }, radius, (true, true, true, true), bg);
                    }
                    let ty = row_y + (l.row_h - r.size) / 2.0;
                    let color = match r.link {
                        Some(k) if !resolved(&l.links[k]) => th.link_unresolved,
                        _ => r.color,
                    };
                    pc.text_attrs(r.text.clone(), ox + r.x, ty, r.size, srgb_u8(color), Some(r.font.clone()), None, r.attrs);
                    if r.strike {
                        let sy = ty + r.size * 0.55;
                        pc.vector(ox + r.x, sy, ox + r.x + r.w, sy, 1.0, r.color, Cap::Flat);
                    }
                }
                if let Some((c, len, _)) = composition {
                    if i == caret.line {
                        // The composition's underline, a row at a time.
                        let (from, to) = (l.caret_xy(c), l.caret_xy(c + len));
                        for row in from.1..=to.1 {
                            let x0 = if row == from.1 { from.0 } else { l.content_x };
                            let x1 = if row == to.1 { to.0 } else { l.runs.iter().filter(|r| r.row == row).map(|r| r.x + r.w).fold(x0, f32::max) };
                            if x1 > x0 {
                                let uy = top + (row + 1) as f32 * l.row_h - 3.0;
                                pc.quad(Rect { x: ox + x0, y: uy, width: x1 - x0, height: 1.5 }, th.caret);
                            }
                        }
                    }
                }
                if focused && i == caret.line {
                    let (x, row) = l.caret_xy(caret_col);
                    let h = l.row_h * 0.8;
                    let y = top + row as f32 * l.row_h + (l.row_h - h) / 2.0;
                    pc.quad(Rect { x: ox + x - 0.5, y, width: 2.0, height: h }, th.caret);
                    // Where an input method puts its candidates, and that
                    // text is wanted at all.
                    let (dx, dy) = pc.offset();
                    crate::ime::report_caret(ox + x - 0.5 + dx, y + dy, 2.0, h);
                }
                i += 1;
            }
        });
    }
}

pub(super) fn srgb_u8(linear: [f32; 4]) -> [u8; 3] {
    let s = crate::color::to_srgb(linear);
    [(s[0] * 255.0) as u8, (s[1] * 255.0) as u8, (s[2] * 255.0) as u8]
}
