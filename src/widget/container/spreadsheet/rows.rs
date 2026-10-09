//! The header and body hit tests, sorting, and row selection.

use super::*;

impl Spreadsheet {
    /// The header column under `(px, py)`, if the point is inside the header band.
    pub(super) fn header_col_at(&self, px: f32, py: f32, rect: Rect) -> Option<usize> {
        if self.headers.is_empty() || rect.width <= 0.0 {
            return None;
        }
        if px < rect.x || px > rect.x + rect.width || py < rect.y || py >= rect.y + HEADER_H {
            return None;
        }
        let at = px - rect.x + self.hscroll(rect);
        let edges = self.col_edges();
        (0..self.headers.len()).find(|&c| at >= edges[c] && at < edges[c + 1])
    }

    /// Cell comparison: numeric when both cells parse (so "10" sorts after "9"),
    /// lexicographic otherwise.
    pub(super) fn cmp_cells(a: &str, b: &str) -> std::cmp::Ordering {
        match (a.parse::<f64>(), b.parse::<f64>()) {
            (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
            _ => a.cmp(b),
        }
    }

    /// Rebuild `order` from `sort`. Stable, so equal cells keep their source order.
    pub(super) fn apply_sort(&mut self) {
        // A refresh can shrink the column set out from under the sort.
        if let Some((col, _)) = self.sort {
            if col >= self.headers.len() {
                self.sort = None;
            }
        }
        self.order = (0..self.row_count).collect();
        if let Some((col, ascending)) = self.sort {
            if let Some(column) = self.columns.get(col) {
                self.order.sort_by(|&a, &b| {
                    let ord = column.cmp_rows(a, b);
                    if ascending { ord } else { ord.reverse() }
                });
            }
        }
    }

    /// What a body press at `(px, py)` lands on: `Some(Some(place))` for the
    /// row at that place in the DISPLAY order, `Some(None)` for the empty
    /// body under the last row, `None` for a press that is not the body's —
    /// outside it, or on a RAISED scrollbar, which the drag surface owns. A
    /// sunk bar is behind the plate and the press is the row's.
    pub(super) fn body_row_at(&self, px: f32, py: f32, rect: Rect) -> Option<Option<usize>> {
        let body_top = rect.y + HEADER_H;
        if px < rect.x || px > rect.x + rect.width || py < body_top || py > rect.y + rect.height {
            return None;
        }
        if self.activity.raised() && (self.over_vbar(px, py, rect) || self.over_hbar(px, py, rect)) {
            return None;
        }
        let scroll = self.geom(rect).map_or(0.0, |g| g.scroll);
        let place = ((py - body_top + scroll) / ROW_H) as usize;
        Some((place < self.order.len()).then_some(place))
    }

    /// A press on the row at `place` of the display order, or on the empty
    /// body (`None`). Plain, it selects that row alone — or nothing, where
    /// the row was the whole selection already or the press met no row.
    /// With ctrl it toggles the row and leaves the rest. With shift it
    /// selects the run from the anchor to the row AS DISPLAYED, so under a
    /// sort the run is what the eye sees between the two; with ctrl as well
    /// the run is added to what is selected.
    pub(super) fn press_row(&mut self, place: Option<usize>, ctrl: bool, shift: bool) {
        let before = self.selected.clone();
        match place {
            None => {
                if !ctrl && !shift {
                    self.selected.clear();
                    self.anchor = None;
                }
            }
            Some(place) => {
                let src = self.order[place];
                let anchor_place = self.anchor.and_then(|a| self.order.iter().position(|&r| r == a));
                match (shift, anchor_place) {
                    (true, Some(from)) => {
                        if !ctrl {
                            self.selected.clear();
                        }
                        let (lo, hi) = (from.min(place), from.max(place));
                        self.selected.extend(self.order[lo..=hi].iter().copied());
                    }
                    _ if ctrl => {
                        if !self.selected.remove(&src) {
                            self.selected.insert(src);
                        }
                        self.anchor = Some(src);
                    }
                    _ => {
                        let alone = self.selected.len() == 1 && self.selected.contains(&src);
                        self.selected.clear();
                        if !alone {
                            self.selected.insert(src);
                        }
                        self.anchor = Some(src);
                    }
                }
            }
        }
        if self.selected != before {
            self.selection_changed = true;
        }
    }
}
