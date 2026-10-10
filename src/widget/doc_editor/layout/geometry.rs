//! What a laid-out line answers: a line made of an image (or carrying images below its text), the
//! caret's x and row for a column, the selection's rects, the column at an x on a row, and the
//! link at a point.

use super::*;

/// Space above and below an embedded image.
pub const IMAGE_PAD: f32 = 4.0;

impl LineLayout {
    /// A line shown as nothing but its embedded image (live preview, the
    /// caret elsewhere): one row as tall as the image, so a click on it
    /// lands on the line and reveals the raw text.
    pub fn image(target: &str, w: f32, h: f32, text_size: f32) -> LineLayout {
        let height = h + 2.0 * IMAGE_PAD;
        LineLayout {
            height,
            row_h: height,
            rows: 1,
            runs: Vec::new(),
            decos: vec![Deco::Image { target: target.to_string(), rect: Rect { x: 0.0, y: IMAGE_PAD, width: w, height: h } }],
            links: Vec::new(),
            task: None,
            toggle: None,
            content_x: 0.0,
            content_start: 0,
            text_size,
        }
    }

    /// The line with the images embedded inside it shown in a row below
    /// it, left to right, wrapping at `width` (live preview cannot sit a
    /// picture inside a text row: rows are one height).
    pub fn with_images_below(mut self, images: &[(String, f32, f32)], width: f32) -> LineLayout {
        let (mut x, mut y, mut row_h) = (0.0f32, self.height + IMAGE_PAD, 0.0f32);
        for (target, w, h) in images {
            if x > 0.0 && x + w > width {
                y += row_h + IMAGE_PAD;
                x = 0.0;
                row_h = 0.0;
            }
            self.decos.push(Deco::Image { target: target.clone(), rect: Rect { x, y, width: *w, height: *h } });
            x += w + 2.0 * IMAGE_PAD;
            row_h = row_h.max(*h);
        }
        if !images.is_empty() {
            self.height = y + row_h + IMAGE_PAD;
        }
        self
    }

    /// The raw line with its image shown below it (the caret on it).
    pub fn with_image_below(mut self, target: &str, w: f32, h: f32) -> LineLayout {
        let y = self.height + IMAGE_PAD;
        self.decos.push(Deco::Image { target: target.to_string(), rect: Rect { x: 0.0, y, width: w, height: h } });
        self.height = y + h + IMAGE_PAD;
        self
    }

    /// Where the caret before byte `col` is drawn: (x, row).
    pub fn caret_xy(&self, col: usize) -> (f32, usize) {
        let mut best: Option<(f32, usize)> = None;
        for r in &self.runs {
            if col < r.src.start {
                break;
            }
            if col <= r.src.end {
                let x = r.xs.iter().rev().find(|(b, _)| *b <= col).map(|(_, x)| *x).unwrap_or(0.0);
                best = Some((r.x + x, r.row));
                if col < r.src.end {
                    break;
                }
            } else {
                // Past the run's end: its trailing side, the left of a right-to-left run.
                best = Some((if r.rtl { r.x } else { r.x + r.w }, r.row));
            }
        }
        best.unwrap_or_else(|| match self.runs.first() {
            Some(r) => (r.x, r.row),
            None => (self.content_x, 0),
        })
    }

    /// What the bytes `a..b` of the line cover, as `(row, x0, x1)` rects left to right: the
    /// boxes of the clusters in that range, merged where they touch (and across the gap
    /// between two runs of a row). A range crossing a change of direction is visually apart,
    /// so it can be several on one row. `to_end` stretches the last row's selection a little
    /// past the line's end, for a selection that goes on to the next line.
    pub fn selection_rects(&self, a: usize, b: usize, to_end: bool) -> Vec<(usize, f32, f32)> {
        let mut boxes: Vec<(usize, f32, f32)> = Vec::new();
        for r in &self.runs {
            for c in r.clusters.iter().filter(|c| c.start < b && c.end > a) {
                boxes.push((r.row, r.x + c.x0, r.x + c.x1));
            }
        }
        boxes.sort_by(|p, q| p.0.cmp(&q.0).then(p.1.total_cmp(&q.1)));
        let gap = 2.0 * PILL_PAD + PILL_GAP + 1.0;
        let mut out: Vec<(usize, f32, f32)> = Vec::new();
        for (row, x0, x1) in boxes {
            match out.last_mut() {
                Some(last) if last.0 == row && x0 <= last.2 + gap => last.2 = last.2.max(x1),
                _ => out.push((row, x0, x1)),
            }
        }
        if to_end {
            let last_row = self.rows.saturating_sub(1);
            let end = self.runs.iter().map(|r| r.x + r.w).fold(self.content_x, f32::max) + 6.0;
            match out.iter_mut().rev().find(|s| s.0 == last_row) {
                Some(s) => s.2 = s.2.max(end),
                None => out.push((last_row, end - 6.0, end)),
            }
        }
        out
    }

    /// The byte nearest a point (x, row) of the line.
    pub fn col_at(&self, x: f32, row: usize) -> usize {
        let in_row: Vec<&Run> = self.runs.iter().filter(|r| r.row == row).collect();
        if in_row.is_empty() {
            return match self.runs.iter().rev().find(|r| r.row < row) {
                Some(r) => r.src.end,
                None => self.content_start,
            };
        }
        let mut best = (f32::INFINITY, in_row[0].src.start);
        for r in in_row {
            for (b, bx) in &r.xs {
                let d = (r.x + bx - x).abs();
                if d < best.0 {
                    best = (d, *b);
                }
            }
        }
        best.1
    }

    /// The link under a point of the line, by index into `links`.
    pub fn link_at(&self, x: f32, y: f32) -> Option<usize> {
        let row = (y / self.row_h).floor().max(0.0) as usize;
        self.runs.iter().find(|r| r.row == row && r.link.is_some() && x >= r.x && x <= r.x + r.w).and_then(|r| r.link)
    }
}
