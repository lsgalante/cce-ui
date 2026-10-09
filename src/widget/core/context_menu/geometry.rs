//! Where the menu is and where its rows are: placement (written back from a popup's configure,
//! or constrained to a window), scrolling a menu cut short, and rows as drawn.

use super::*;

impl ContextMenuState {

    /// Put the menu at `(x, y)`, shown at most `max_h` tall: the rows
    /// scroll when that cuts them off. Never shorter than one row, so a
    /// placement that leaves no room still shows something to scroll.
    /// Where the popup's configure lands the menu, and the in-window
    /// fallback's [`constrain_to`](Self::constrain_to).
    pub fn place(&mut self, x: f32, y: f32, max_h: f32) {
        self.x = x;
        self.y = y;
        let floor = self.content_h.min(ROW_H + 2.0 * PAD);
        self.h = self.content_h.min(max_h).max(floor);
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());
    }

    /// Keep the menu inside `(bx, by, bw, bh)` the way an xdg positioner
    /// with flip-y, slide-x, slide-y and resize-y does, from the anchor
    /// `show` was given: it opens down and right; if it does not fit
    /// below, it flips to open UP from the anchor; if it fits neither
    /// way it slides to the bottom edge, and if it is taller than the
    /// whole box it is cut to the box and scrolls. Recomputed from the
    /// anchor every call, so it can run every frame. For hosts with no
    /// popup surface (a layer surface, or the popup disabled) — there the
    /// window is the only room there is.
    pub fn constrain_to(&mut self, bx: f32, by: f32, bw: f32, bh: f32) {
        let (ax, ay) = self.anchor;
        let x = if ax + self.w > bx + bw { (bx + bw - self.w).max(bx) } else { ax.max(bx) };
        let (y, max_h) = if ay + self.content_h <= by + bh {
            (ay.max(by), self.content_h)
        } else if !self.turned && ay - self.content_h >= by {
            (ay - self.content_h, self.content_h)
        } else {
            ((by + bh - self.content_h).max(by), bh)
        };
        self.place(x, y, max_h);
    }

    /// How far the rows can scroll: zero when the menu shows them all.
    pub fn max_scroll(&self) -> f32 {
        (self.content_h - self.h).max(0.0)
    }

    /// Scroll so row `idx` is wholly inside the plate. Unlike
    /// [`Self::scroll_by`] this does not re-hover the row under the
    /// pointer: the keyboard put the highlight where it is.
    pub(super) fn scroll_into_view(&mut self, idx: usize) {
        let top = self.band_h() + idx as f32 * ROW_H;
        let bottom = top + ROW_H + 2.0 * PAD;
        if top < self.scroll {
            self.scroll = top;
        } else if bottom > self.scroll + self.h {
            self.scroll = bottom - self.h;
        }
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());
    }

    /// Scroll the rows by `dy` px (positive shows rows further down),
    /// clamped; re-hovers whatever row the pointer now sits on. `true`
    /// when anything moved.
    pub fn scroll_by(&mut self, dy: f32) -> bool {
        let next = (self.scroll + dy).clamp(0.0, self.max_scroll());
        if (next - self.scroll).abs() < f32::EPSILON {
            return false;
        }
        self.scroll = next;
        if let Some((px, py)) = self.last_cursor {
            self.rehover(px, py);
        }
        true
    }

    pub fn hit_test(&self, px: f32, py: f32) -> bool {
        if !self.visible { return false; }
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }

    /// The top of row `idx`, where it is drawn: scrolled, so a row above
    /// the view lies above `y`.
    pub fn row_y(&self, idx: usize) -> f32 {
        self.y + PAD + self.band_h() + idx as f32 * ROW_H - self.scroll
    }

    /// The row under `(px, py)`, or `None` outside the plate or in its
    /// padding — the padding is plate, not a row, so a press there
    /// neither hovers nor fires row 0.
    pub fn row_at(&self, px: f32, py: f32) -> Option<usize> {
        if px < self.x || px > self.x + self.w || py < self.y || py > self.y + self.h {
            return None;
        }
        let rel = py - self.y - PAD - self.band_h() + self.scroll;
        if rel < 0.0 {
            return None;
        }
        let idx = (rel / ROW_H) as usize;
        (idx < self.options.len()).then_some(idx)
    }
}
