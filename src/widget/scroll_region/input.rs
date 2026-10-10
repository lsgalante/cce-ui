//! The region's input: a press (a thumb grab), its release and drag, the pointer moving, the wheel,
//! keys, and the tick that runs the scroll motion and the bars' fade.

use super::*;

impl ScrollRegion {
    /// Left press: scrollbar thumb grab or track jump (`ScrollBox::mouse_input`), plus the
    /// press-inside focus / press-outside unfocus bookkeeping. Returns true only when the
    /// scrollbar consumed the press — a press on the rows falls through to them.
    pub fn press(&mut self, px: f32, py: f32) -> bool {
        self.focused = self.hit(px, py);
        // The bottom bar first: its ±4 slop strip sits inside the box, where
        // the vertical hit test can never claim it.
        if self.hit_h_scrollbar(px, py) {
            self.dragging_h = true;
            let (track_x, _, track_w, _, thumb_x, thumb_w) = self.h_scrollbar_geom();
            let click_offset = px - thumb_x;
            if click_offset >= 0.0 && click_offset <= thumb_w {
                self.drag_offset_x = click_offset;
            } else {
                self.drag_offset_x = thumb_w / 2.0;
                let target = px - self.drag_offset_x;
                let ratio = if track_w - thumb_w > 0.0 {
                    ((target - track_x) / (track_w - thumb_w)).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                self.scroll_x = ratio * self.max_scroll_x();
            }
            return true;
        }
        if !self.hit_scrollbar(px, py) {
            self.dragging = false;
            return false;
        }
        self.dragging = true;
        let (_, track_y, _, track_h, thumb_y, thumb_h) = self.scrollbar_geom();
        let click_offset = py - thumb_y;
        if click_offset >= 0.0 && click_offset <= thumb_h {
            self.drag_offset_y = click_offset;
        } else {
            self.drag_offset_y = thumb_h / 2.0;
            let target = py - self.drag_offset_y;
            let ratio = if track_h - thumb_h > 0.0 {
                ((target - track_y) / (track_h - thumb_h)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            self.scroll_y = ratio * self.max_scroll();
        }
        true
    }

    /// Returns whether a thumb drag was in progress (the caller's redraw signal).
    pub fn release(&mut self) -> bool {
        // Bitwise on purpose: both drags must reset even when the first
        // operand is already true (|| would short-circuit the take).
        let was_dragging = std::mem::take(&mut self.dragging) | std::mem::take(&mut self.dragging_h);
        if was_dragging && self.sink_behind {
            // A drag release starts the hold window, so the bar lingers
            // briefly instead of sinking the instant the button lifts.
            self.activity.bump();
        }
        was_dragging
    }

    pub(super) fn drag_move(&mut self, py: f32) -> bool {
        let (_, track_y, _, track_h, _, thumb_h) = self.scrollbar_geom();
        let target = py - self.drag_offset_y;
        let ratio = if track_h - thumb_h > 0.0 {
            ((target - track_y) / (track_h - thumb_h)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let old = self.scroll_y;
        self.scroll_y = ratio * self.max_scroll();
        (self.scroll_y - old).abs() > 0.01
    }

    /// Pointer-move bookkeeping: forwards to an active thumb drag (returns true so the host
    /// treats it as a high-priority drag override), else just tracks hover for the border
    /// tint and the keyboard scope.
    pub fn cursor_moved(&mut self, px: f32, py: f32) -> bool {
        self.hovered = self.hit(px, py);
        if self.sink_behind {
            // Gated hit tests: a sunk bar reports no hover, so hover can only
            // sustain a raised bar (the hysteresis contract).
            self.activity
                .set_hover(self.hit_scrollbar(px, py) || self.hit_h_scrollbar(px, py));
        }
        if self.dragging {
            self.drag_move(py);
            return true;
        }
        if self.dragging_h {
            let (track_x, _, track_w, _, _, thumb_w) = self.h_scrollbar_geom();
            let target = px - self.drag_offset_x;
            let ratio = if track_w - thumb_w > 0.0 {
                ((target - track_x) / (track_w - thumb_w)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            self.scroll_x = ratio * self.max_scroll_x();
            return true;
        }
        false
    }

    pub fn wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32) -> bool {
        if !self.hit(px, py) {
            return false;
        }
        // Sideways wheel/trackpad deltas pan an h-scrollable list; a
        // vertical-only list ignores them (max_scroll_x = 0 clamps to 0).
        self.motion.reconcile(self.scroll_x, self.scroll_y);
        let changed = self.motion.apply(delta, (LINE_PX, LINE_PX), self.bounds_x(), self.bounds_y());
        self.sync_from_motion();
        if changed {
            self.raise();
        }
        changed
    }

    /// Per-frame raise/sink upkeep for sink-behind regions; a `true` return is
    /// the host's repaint signal. True while the post-scroll hold is running,
    /// not just on the flip: the demand-driven frame loop only keeps ticking
    /// while frames flow, so the hold must keep them coming or the sink would
    /// stall until the next input event. No-op (false) without `sink_behind`.
    pub fn tick(&mut self, dt: f32) -> bool {
        // The glide/coast first: a host write to the pub offsets since the
        // last frame (thumb drag, auto-snap) is adopted, then the motion
        // advances and the drawn offsets follow it.
        self.motion.reconcile(self.scroll_x, self.scroll_y);
        let moved = self.motion.tick(dt, self.bounds_x(), self.bounds_y());
        self.sync_from_motion();
        let animating = self.motion.is_animating();
        if !self.sink_behind {
            return moved || animating;
        }
        let holding = self.activity.holding();
        let flipped = self
            .activity
            .tick(dt, self.overflowing(), self.dragging || self.dragging_h);
        moved || animating || flipped || holding
    }

    /// Hover/focus-scoped keyboard scrolling (`ScrollBox::keyboard_input` reached the boxes
    /// when focused or hovered; the dissolved region keeps both via its local flags).
    pub fn keyboard(&mut self, event: &KeyEvent) -> bool {
        if (!self.hovered && !self.focused) || event.state != ElementState::Pressed {
            return false;
        }
        // Keyboard steps ride the same glide as wheel notches (a held arrow
        // accumulates into one motion); pages and Home/End glide to their
        // absolute target.
        let s = scroll_settings();
        self.motion.reconcile(self.scroll_x, self.scroll_y);
        let by = self.bounds_y();
        let bx = self.bounds_x();
        let max_x = self.max_scroll_x();
        let changed = if event.ctrl {
            match &event.logical_key {
                Key::Character(c) if c == "n" || c == "N" => self.motion.y.wheel(LINE_PX, by, &s),
                Key::Character(c) if c == "p" || c == "P" => self.motion.y.wheel(-LINE_PX, by, &s),
                _ => return false,
            }
        } else {
            match &event.logical_key {
                Key::Named(NamedKey::ArrowDown) => self.motion.y.wheel(LINE_PX, by, &s),
                Key::Named(NamedKey::ArrowUp) => self.motion.y.wheel(-LINE_PX, by, &s),
                Key::Named(NamedKey::PageDown) => {
                    let t = self.motion.y.target() + self.viewport_h;
                    self.motion.y.scroll_to(t, by, &s)
                }
                Key::Named(NamedKey::PageUp) => {
                    let t = self.motion.y.target() - self.viewport_h;
                    self.motion.y.scroll_to(t, by, &s)
                }
                Key::Named(NamedKey::Home) => self.motion.y.scroll_to(0.0, by, &s),
                Key::Named(NamedKey::End) => self.motion.y.scroll_to(by.hi, by, &s),
                // Only an h-scrollable list claims the horizontal arrows —
                // elsewhere they keep falling through to other handlers.
                Key::Named(NamedKey::ArrowRight) if max_x > 0.0 => self.motion.x.wheel(LINE_PX, bx, &s),
                Key::Named(NamedKey::ArrowLeft) if max_x > 0.0 => self.motion.x.wheel(-LINE_PX, bx, &s),
                _ => return false,
            }
        };
        self.sync_from_motion();
        if changed {
            self.raise();
        }
        changed
    }
}
