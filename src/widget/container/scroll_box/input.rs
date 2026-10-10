//! A scroll box's input, forwarded by its host: a press (a thumb grab or a track jump), the thumb
//! drag, the pointer over the bar, the wheel, keys while hovered or focused, and the tick that
//! runs the scroll motion and the bar's fade.

use super::*;

impl ScrollBox {
    /// The legacy `WidgetHost` default hit test over the base rect (ScrollBox never carried a
    /// label or row expansion, so those branches are folded away).
    pub(super) fn hit_test(&self, px: f32, py: f32, ctx: &UiContext) -> bool {
        if ctx.is_coordinate_covered(self.base.id(), px, py) {
            return false;
        }
        let (x, y, w, h) = (self.base.x, self.base.y, self.base.w, self.base.h);
        if w <= 0.0 || h <= 0.0 {
            return false;
        }
        px >= x && px <= x + w && py >= y && py <= y + h
    }

    /// The legacy focus claim on scrollbar/list clicks: its only observable effect was
    /// unfocusing the previously focused widget (nothing ever queried focus ON the scroll
    /// box through the thread-local, and its own `unfocus` was a no-op) — so just release
    /// the current holder instead of storing a pointer to a non-WidgetHost.
    pub(super) fn claim_focus(&self, ctx: &mut UiContext) {
        ctx.clear_focus();
    }

    pub fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if button == MouseButton::Left {
            if state == ElementState::Pressed {
                if self.hit_test_scrollbar(px, py) {
                    self.claim_focus(ctx);
                    self.scrollbar_dragging = true;
                    let (_, _, _, _, thumb_y, thumb_h) = self.bar_geom().expect("a hit bar has geometry");
                    let click_offset = py - thumb_y;
                    if click_offset >= 0.0 && click_offset <= thumb_h {
                        self.drag_offset_y = click_offset;
                    } else {
                        // Clicked outside the thumb: jump thumb center to py
                        self.drag_offset_y = thumb_h / 2.0;
                        self.drag_thumb_to(py);
                    }
                    return true;
                } else {
                    self.scrollbar_dragging = false;
                }
                if self.hit_test(px, py, ctx) {
                    self.claim_focus(ctx);
                }
            } else if state == ElementState::Released {
                self.end_thumb_drag();
            }
        }
        false
    }

    pub fn draggable(&self) -> bool {
        self.scrollbar_dragging
    }

    /// Legacy `WidgetHost` default parity: ScrollBox never overrode `is_dragging` — TreeList
    /// forwards it and always got `false`.
    pub fn is_dragging(&self) -> bool {
        false
    }

    pub fn drag_begin(&mut self, _px: f32, _py: f32) {}

    pub fn drag_update(&mut self, _px: f32, py: f32) -> bool {
        if !self.scrollbar_dragging {
            return false;
        }
        self.drag_thumb_to(py)
    }

    pub fn drag_end(&mut self) {
        self.end_thumb_drag();
    }

    /// A thumb drag let go: a sink-behind bar lingers for the hold window
    /// rather than sinking the instant the button lifts.
    pub(super) fn end_thumb_drag(&mut self) {
        if std::mem::take(&mut self.scrollbar_dragging) && self.sink_behind {
            self.activity.bump();
        }
    }

    /// The legacy `WidgetHost` default `cursor_moved` entry (cce-test-interface's panel copy
    /// calls it): cover-check clears hover, otherwise falls into `on_cursor_moved`. The
    /// MouseLeave dispatch the default performed was a no-op for ScrollBox.
    pub fn cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        ctx.set_cursor_pos(px, py);
        if ctx.is_coordinate_covered(self.base.id(), px, py) {
            let was = self.base.hovered;
            if was {
                self.base.hovered = false;
            }
            return was;
        }
        self.on_cursor_moved(px, py, ctx)
    }

    pub fn on_cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        let mut changed = false;
        if self.scrollbar_dragging && self.drag_thumb_to(py) {
            changed = true;
        }
        if self.sink_behind {
            // Gated on the latch: a sunk bar reports no hover, so hover only
            // ever holds up a bar a scroll raised.
            self.activity.set_hover(self.hit_test_scrollbar(px, py));
        }

        let was = self.base.hovered;
        self.base.hovered = self.hit_test(px, py, ctx);
        if was != self.base.hovered {
            changed = true;
        }
        changed
    }

    /// Per-frame smooth-scroll upkeep: adopts host writes to `scroll_y`,
    /// advances a wheel glide or trackpad coast, and returns the repaint
    /// signal (true while anything is still moving).
    pub fn tick(&mut self, dt: f32, _ctx: &mut UiContext) -> bool {
        self.motion.reconcile(0.0, self.scroll_y);
        let moved = self.motion.tick(dt, Bounds::max(0.0), self.bounds_y());
        self.scroll_y = self.motion.y.pos();
        let animating = self.motion.is_animating();
        if !self.sink_behind {
            return moved || animating;
        }
        // A glide or coast in motion holds the bar up as the scroll that
        // began it did; the hold keeps frames coming until the sink renders.
        if moved {
            self.activity.bump();
        }
        let holding = self.activity.holding();
        let flipped = self.activity.tick(dt, self.content_h > self.viewport_h, self.scrollbar_dragging);
        moved || animating || flipped || holding
    }

    pub fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if self.hit_test(px, py, ctx) {
            self.motion.reconcile(0.0, self.scroll_y);
            let changed = self.motion.apply(delta, (LINE_PX, LINE_PX), Bounds::max(0.0), self.bounds_y());
            self.scroll_y = self.motion.y.pos();
            if changed {
                self.notify_scrolled();
            }
            changed
        } else {
            false
        }
    }

    pub fn keyboard_input(&mut self, event: &KeyEvent, ctx: &mut UiContext) -> bool {
        let moved = self.keyboard_scroll(event, ctx);
        if moved {
            self.notify_scrolled();
        }
        moved
    }

    pub(super) fn keyboard_scroll(&mut self, event: &KeyEvent, ctx: &mut UiContext) -> bool {
        // Focus never lands on the box itself (post-6av it is not an `WidgetHost`), and its id is
        // never a tree ancestor of the focused widget — like the legacy address walk, this
        // gate only ever passes via the hover check below.
        let self_id = self.base.id();
        let has_focus = ctx.is_focused_id(self_id) || {
            let mut current = ctx.focused_widget;
            let mut found = false;
            while let Some(id) = current {
                if id == self_id {
                    found = true;
                    break;
                }
                current = ctx.tree.parent_id(id);
            }
            found
        };

        let is_hovered = self.hit_test(ctx.cursor_pos.0, ctx.cursor_pos.1, ctx);
        if !has_focus && !is_hovered {
            return false;
        }
        if event.state != ElementState::Pressed {
            return false;
        }
        let max_scroll = (self.content_h - self.viewport_h).max(0.0);
        if event.ctrl {
            match &event.logical_key {
                Key::Character(c) if c == "n" || c == "N" => {
                    let old_scroll = self.scroll_y;
                    self.scroll_y = (self.scroll_y + 24.0).clamp(0.0, max_scroll);
                    (self.scroll_y - old_scroll).abs() > 0.01
                }
                Key::Character(c) if c == "p" || c == "P" => {
                    let old_scroll = self.scroll_y;
                    self.scroll_y = (self.scroll_y - 24.0).clamp(0.0, max_scroll);
                    (self.scroll_y - old_scroll).abs() > 0.01
                }
                _ => false,
            }
        } else {
            match &event.logical_key {
                Key::Named(NamedKey::ArrowDown) => {
                    let old_scroll = self.scroll_y;
                    self.scroll_y = (self.scroll_y + 24.0).clamp(0.0, max_scroll);
                    (self.scroll_y - old_scroll).abs() > 0.01
                }
                Key::Named(NamedKey::ArrowUp) => {
                    let old_scroll = self.scroll_y;
                    self.scroll_y = (self.scroll_y - 24.0).clamp(0.0, max_scroll);
                    (self.scroll_y - old_scroll).abs() > 0.01
                }
                Key::Named(NamedKey::PageDown) => {
                    let old_scroll = self.scroll_y;
                    self.scroll_y = (self.scroll_y + self.viewport_h).clamp(0.0, max_scroll);
                    (self.scroll_y - old_scroll).abs() > 0.01
                }
                Key::Named(NamedKey::PageUp) => {
                    let old_scroll = self.scroll_y;
                    self.scroll_y = (self.scroll_y - self.viewport_h).clamp(0.0, max_scroll);
                    (self.scroll_y - old_scroll).abs() > 0.01
                }
                Key::Named(NamedKey::Home) => {
                    let old_scroll = self.scroll_y;
                    self.scroll_y = 0.0;
                    (self.scroll_y - old_scroll).abs() > 0.01
                }
                Key::Named(NamedKey::End) => {
                    let old_scroll = self.scroll_y;
                    self.scroll_y = max_scroll;
                    (self.scroll_y - old_scroll).abs() > 0.01
                }
                _ => false,
            }
        }
    }
}
