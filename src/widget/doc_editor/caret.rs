//! The caret's geometry: its point and rect, the text position under a point, scrolling it into
//! view, and moving it up and down by rows.

use super::*;

impl DocEditor {
    /// The caret's (x, y) in content space and its row height.
    pub(super) fn caret_point(&mut self, p: Pos) -> (f32, f32, f32) {
        self.ensure(p.line);
        self.retop();
        let col = self.laid_col(p.line, p.col, p == self.buf.caret);
        let l = self.layouts[p.line].as_ref().expect("laid out");
        let (x, row) = l.caret_xy(col);
        (x, self.tops[p.line] + row as f32 * l.row_h, l.row_h)
    }

    /// The caret's rect on screen as last painted — for a popup at it.
    pub fn caret_rect(&mut self) -> Rect {
        let (x, y, h) = self.caret_point(self.buf.caret);
        Rect { x: self.origin.0 + x, y: self.origin.1 + y - self.scroll, width: 2.0, height: h }
    }

    /// The position under a screen point.
    /// The text position under a window point (as painted last), for a
    /// host placing something there — a drop.
    pub fn pos_at(&mut self, sx: f32, sy: f32) -> Pos {
        let (x, y) = (sx - self.origin.0, sy - self.origin.1 + self.scroll);
        if y < 0.0 {
            return Pos::default();
        }
        let i = self.line_at_y(y);
        self.ensure(i);
        self.retop();
        let l = self.layouts[i].as_ref().unwrap();
        let row = (((y - self.tops[i]) / l.row_h).floor().max(0.0) as usize).min(l.rows - 1);
        let col = l.col_at(x, row);
        Pos::new(i, self.source_col(i, col))
    }

    pub(super) fn scroll_to_caret(&mut self) {
        let (_, y, h) = self.caret_point(self.buf.caret);
        let view = self.viewport.height - 2.0 * self.pad;
        if view <= 0.0 {
            return;
        }
        if y < self.scroll {
            self.scroll = y;
        } else if y + h > self.scroll + view {
            self.scroll = y + h - view;
        }
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());
        self.motion.y.jump_to(self.scroll);
    }

    /// Move the caret one visual row (or `rows`) up or down, keeping `want_x`.
    pub(super) fn vertical(&mut self, rows: i32, select: bool) {
        let p = self.buf.caret;
        self.ensure(p.line);
        let (cx, row) = self.layouts[p.line].as_ref().unwrap().caret_xy(p.col);
        let x = *self.want_x.get_or_insert(cx);
        let (mut line, mut row) = (p.line as i64, row as i64);
        let mut left = rows.unsigned_abs();
        while left > 0 {
            row += rows.signum() as i64;
            let rows_here = self.layouts[line as usize].as_ref().map_or(1, |l| l.rows) as i64;
            if row < 0 {
                if line == 0 {
                    row = 0;
                    break;
                }
                line -= 1;
                self.ensure(line as usize);
                row = self.layouts[line as usize].as_ref().unwrap().rows as i64 - 1;
            } else if row >= rows_here {
                if line as usize + 1 >= self.buf.line_count() {
                    row = rows_here - 1;
                    break;
                }
                line += 1;
                self.ensure(line as usize);
                row = 0;
            }
            left -= 1;
        }
        let l = self.layouts[line as usize].as_ref().unwrap();
        let col = l.col_at(x, row as usize);
        let keep = self.want_x;
        self.buf.set_caret(Pos::new(line as usize, col), select);
        self.want_x = keep;
    }
}
