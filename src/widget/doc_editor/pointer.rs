//! The pointer: presses (with multi-clicks, a task toggle and a link to follow), drags, the wheel
//! and the scroll's tick, and revealing a line.

use super::*;

impl DocEditor {
    /// A left press at a screen point. A rendered link follows; a task's
    /// box toggles; Ctrl+click follows a raw link; otherwise the caret
    /// moves (Shift extends; a double press selects a word, a triple the
    /// line).
    pub fn press(&mut self, sx: f32, sy: f32, shift: bool, ctrl: bool) -> Response {
        self.sync();
        let rev = self.buf.revision;
        let before = (self.buf.caret, self.buf.anchor);
        // A press mid-composition places the caret, read through the line
        // as it is drawn, and the composition is left there, cancelled.
        if self.composition.is_some() {
            let p = self.pos_at(sx, sy);
            self.drop_composition();
            self.buf.set_caret(p, false);
            self.dragging = true;
            self.want_x = None;
            return self.after_edit(rev, before);
        }
        let (x, y) = (sx - self.origin.0, sy - self.origin.1 + self.scroll);
        if y >= 0.0 && !shift {
            let i = self.line_at_y(y);
            self.ensure(i);
            self.retop();
            let active = self.is_active(i, self.shown_active);
            let l = self.layouts[i].as_ref().unwrap();
            let ly = y - self.tops[i];
            if let Some((r, at)) = l.task {
                if r.width > 0.0 && x >= r.x && x <= r.x + r.width && ly >= r.y && ly <= r.y + r.height {
                    let status = self.buf.line(i)[at..].chars().next().unwrap_or(' ');
                    let next = if status == ' ' { "x" } else { " " };
                    let end = at + status.len_utf8();
                    let keep = (self.buf.caret, self.buf.anchor);
                    self.buf.replace(Pos::new(i, at), Pos::new(i, end), next, EditKind::Other);
                    (self.buf.caret, self.buf.anchor) = keep;
                    return self.after_edit(rev, before);
                }
            }
            if let Some((r, bytes, on)) = l.toggle.clone() {
                if x >= r.x && x <= r.x + r.width && ly >= r.y && ly <= r.y + r.height {
                    // A boolean property's box: flip it in place.
                    let keep = (self.buf.caret, self.buf.anchor);
                    let next = if on { "false" } else { "true" };
                    self.buf.replace(Pos::new(i, bytes.start), Pos::new(i, bytes.end), next, EditKind::Other);
                    (self.buf.caret, self.buf.anchor) = keep;
                    return self.after_edit(rev, before);
                }
            }
            if !active || ctrl {
                if let Some(k) = l.link_at(x, ly) {
                    return Response::Follow(l.links[k].clone());
                }
            }
        }
        let p = self.pos_at(sx, sy);
        let now = Instant::now();
        let count = match self.clicks {
            Some((t, at, n)) if now.duration_since(t) < DOUBLE_CLICK && at == p => n % 3 + 1,
            _ => 1,
        };
        self.clicks = Some((now, p, count));
        match count {
            2 => {
                let (a, b) = self.buf.word_at(p);
                self.buf.set_caret(a, false);
                self.buf.set_caret(b, true);
            }
            3 => {
                self.buf.set_caret(Pos::new(p.line, 0), false);
                self.buf.set_caret(Pos::new(p.line, self.buf.line(p.line).len()), true);
            }
            _ => self.buf.set_caret(p, shift),
        }
        self.dragging = true;
        self.want_x = None;
        self.after_edit(rev, before)
    }

    /// Pointer motion; extends the selection while a press is held.
    pub fn drag(&mut self, sx: f32, sy: f32) -> Response {
        if !self.dragging {
            return Response::None;
        }
        let before = (self.buf.caret, self.buf.anchor);
        let p = self.pos_at(sx, sy);
        if p != self.buf.caret {
            self.buf.set_caret(p, true);
        }
        let r = self.after_edit(self.buf.revision, before);
        // Selecting by drag does not drag the view to the caret: only an
        // edge does, and the wheel.
        self.follow_caret = sy < self.viewport.y || sy > self.viewport.y + self.viewport.height;
        r
    }

    pub fn release(&mut self) {
        self.dragging = false;
    }

    /// Whether a press is being dragged (the host keeps routing motion).
    pub fn dragging(&self) -> bool {
        self.dragging
    }

    /// A link under a screen point, for a hover cursor.
    pub fn link_at(&mut self, sx: f32, sy: f32) -> bool {
        let (x, y) = (sx - self.origin.0, sy - self.origin.1 + self.scroll);
        if y < 0.0 || self.layouts.is_empty() {
            return false;
        }
        let i = self.line_at_y(y);
        if self.is_active(i, self.shown_active) {
            return false;
        }
        self.ensure(i);
        self.retop();
        let l = self.layouts[i].as_ref().unwrap();
        l.link_at(x, y - self.tops[i]).is_some() || l.task.is_some_and(|(r, _)| r.width > 0.0 && x >= r.x && x <= r.x + r.width && y - self.tops[i] <= r.y + r.height)
    }

    pub fn wheel(&mut self, delta: &MouseScrollDelta) -> bool {
        let line = (self.theme.size * self.theme.spacing).max(1.0);
        self.motion.reconcile(0.0, self.scroll);
        let moved = self.motion.apply(delta, (line, line * 2.0), Bounds::max(0.0), Bounds::max(self.max_scroll()));
        self.scroll = self.motion.y.pos();
        moved
    }

    /// Advance a wheel glide; true while it moves.
    pub fn tick(&mut self, dt: f32) -> bool {
        if !self.motion.is_animating() {
            return false;
        }
        self.motion.tick(dt, Bounds::max(0.0), Bounds::max(self.max_scroll()));
        self.scroll = self.motion.y.pos();
        true
    }

    /// Bring a line to the top of the view (search hits, outline clicks).
    pub fn reveal_line(&mut self, line: usize) {
        self.sync();
        let line = line.min(self.buf.line_count() - 1);
        self.retop();
        self.scroll = self.tops[line].clamp(0.0, self.max_scroll());
        self.motion.y.jump_to(self.scroll);
        self.buf.set_caret(Pos::new(line, 0), false);
        self.sync();
    }
}
