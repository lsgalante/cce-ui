//! The spreadsheet's input: presses on headers and rows, keys, the wheel and its inertia, thumb
//! drags, and the tick that integrates the scroll.

use super::*;

impl Input for Spreadsheet {
    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::PointerMove { x: px, y: py, .. } => {
                let r = ectx.rect;
                let was_hovered = self.hovered;
                self.hovered =
                    *px >= r.x && *px <= r.x + r.width && *py >= r.y && *py <= r.y + r.height;

                // Over a bar: hover only SUSTAINS a raised bar — a sunk one
                // is behind the plate, and the pointer is on the plate.
                let over = self.over_vbar(*px, *py, r) || self.over_hbar(*px, *py, r);
                self.activity.set_hover(over);
                let flipped = self.recompute_bars(r);
                let was_header = self.header_hover_col;
                self.header_hover_col = self.header_col_at(*px, *py, r);
                was_hovered != self.hovered || flipped || was_header != self.header_hover_col
            }
            // A left press on a column header cycles that column's sort:
            // ascending → descending → back to natural order.
            Event::MouseButton {
                button: MouseButton::Left,
                state: ElementState::Pressed,
                x: px,
                y: py,
                ..
            } => {
                let Some(col) = self.header_col_at(*px, *py, ectx.rect) else {
                    // Not the header: a row of the body, or nothing of ours.
                    let Some(place) = self.body_row_at(*px, *py, ectx.rect) else {
                        return false;
                    };
                    let had = self.selection_changed;
                    self.selection_changed = false;
                    self.press_row(place, self.ctrl, self.shift);
                    let changed = self.selection_changed;
                    self.selection_changed |= had;
                    return changed;
                };
                self.sort = match self.sort {
                    Some((c, true)) if c == col => Some((col, false)),
                    Some((c, false)) if c == col => None,
                    _ => Some((col, true)),
                };
                self.apply_sort();
                true
            }
            // Hit-gated by the adapter (which also rejects hidden widgets).
            Event::MouseWheel { delta, .. } => {
                // Delta signs follow the ScrollRegion/TextBox convention (negate the
                // event delta); natural scroll is already applied upstream by libinput.
                let (dx, dy) = ScrollMotion::delta_px(delta, (ROW_H, ROW_H));
                let by = self.geom(ectx.rect).map_or(Bounds::max(0.0), |g| Bounds::max(g.max_scroll));
                let bx = self.hgeom(ectx.rect).map_or(Bounds::max(0.0), |g| Bounds::max(g.max_scroll));
                // Claimed whenever the pointed axis can scroll at all (an
                // overflowing table swallows its wheel), moved or not.
                let used = (dy != 0.0 && by.hi > 0.0) || (dx != 0.0 && bx.hi > 0.0);
                self.motion.reconcile(self.scroll_x, self.scroll_y);
                let discrete = matches!(delta, MouseScrollDelta::LineDelta(..));
                self.motion.apply_px(dx, dy, discrete, bx, by);
                self.scroll_x = self.motion.x.pos();
                self.scroll_y = self.motion.y.pos();
                if used {
                    // A scroll is what brings the bars to the fore.
                    self.activity.bump();
                    self.recompute_bars(ectx.rect);
                }
                used
            }
            Event::KeyInput(key_event) => {
                if key_event.state != ElementState::Pressed {
                    return false;
                }
                let Some(g) = self.geom(ectx.rect) else {
                    return false;
                };
                // Steps glide, and a held key accumulates from the glide's
                // target rather than the offset drawn this frame.
                self.motion.reconcile(self.scroll_x, self.scroll_y);
                let by = Bounds::max(g.max_scroll);
                let old = by.clamp(self.motion.y.target());
                let new = match &key_event.logical_key {
                    Key::Named(NamedKey::ArrowDown) => old + ROW_H,
                    Key::Named(NamedKey::ArrowUp) => old - ROW_H,
                    Key::Named(NamedKey::PageDown) => old + g.visible_h,
                    Key::Named(NamedKey::PageUp) => old - g.visible_h,
                    Key::Named(NamedKey::Home) => 0.0,
                    Key::Named(NamedKey::End) => g.max_scroll,
                    _ => return false,
                };
                let moved = self.motion.y.scroll_to(new, by, &scroll_settings());
                self.scroll_y = self.motion.y.pos();
                if moved {
                    self.activity.bump();
                    self.recompute_bars(ectx.rect);
                }
                moved
            }
            _ => false,
        }
    }

    fn scrollable(&self) -> bool {
        true
    }

    fn set_modifiers(&mut self, ctrl: bool, shift: bool, _alt: bool) {
        self.ctrl = ctrl;
        self.shift = shift;
    }

    // --- Scrollbar drag, host-driven (the designer checks `draggable()` on the pressed widget
    // and then streams `drag_update` at it). `drag_begin` decides whether the press actually
    // landed on a RAISED scrollbar; a body press, or one on a bar sunk behind the plate,
    // starts no drag.

    fn draggable(&self, rect: Rect) -> bool {
        self.geom(rect).is_some() || self.hgeom(rect).is_some()
    }

    fn is_dragging(&self) -> bool {
        self.dragging_scrollbar || self.dragging_hscrollbar
    }

    fn drag_begin(&mut self, px: f32, py: f32, rect: Rect) {
        if !self.activity.raised() {
            return;
        }
        // A grab or release cancels any glide/coast in flight.
        self.motion = ScrollMotion::at(self.scroll_x, self.scroll_y);
        // The vertical bar owns the middle of the cross, where the two meet.
        if let Some(g) = self.geom(rect) {
            if self.over_vbar(px, py, rect) {
                self.dragging_scrollbar = true;
                if py >= g.thumb_y && py <= g.thumb_y + g.thumb_h {
                    self.drag_offset_y = py - g.thumb_y;
                } else {
                    // Track click: jump the thumb's center to the pointer.
                    self.drag_offset_y = g.thumb_h / 2.0;
                    self.scroll_to_thumb(&g, py - self.drag_offset_y);
                }
                return;
            }
        }
        if let Some(g) = self.hgeom(rect) {
            if self.over_hbar(px, py, rect) {
                self.dragging_hscrollbar = true;
                if px >= g.thumb_x && px <= g.thumb_x + g.thumb_w {
                    self.drag_offset_x = px - g.thumb_x;
                } else {
                    self.drag_offset_x = g.thumb_w / 2.0;
                    self.hscroll_to_thumb(&g, px - self.drag_offset_x);
                }
            }
        }
    }

    fn drag_update(&mut self, px: f32, py: f32, rect: Rect) -> bool {
        if self.dragging_scrollbar {
            self.motion.y.jump_to(self.scroll_y);
            if let Some(g) = self.geom(rect) {
                let old = g.scroll;
                self.scroll_to_thumb(&g, py - self.drag_offset_y);
                return (self.scroll_y - old).abs() > 0.01;
            }
            return false;
        }
        if self.dragging_hscrollbar {
            self.motion.x.jump_to(self.scroll_x);
            if let Some(g) = self.hgeom(rect) {
                let old = g.scroll;
                self.hscroll_to_thumb(&g, px - self.drag_offset_x);
                return (self.scroll_x - old).abs() > 0.01;
            }
        }
        false
    }

    fn drag_end(&mut self) {
        if self.dragging_scrollbar || self.dragging_hscrollbar {
            // A release holds the bars up for the hold window, as a scroll does.
            self.activity.bump();
        }
        self.dragging_scrollbar = false;
        self.dragging_hscrollbar = false;
        // A grab or release cancels any glide/coast in flight.
        self.motion = ScrollMotion::at(self.scroll_x, self.scroll_y);
    }

    // --- Smooth scroll: the wheel/finger feed the shared motion; each frame advances it.

    fn tick(&mut self, dt: f32, rect: Rect) -> bool {
        self.motion.reconcile(self.scroll_x, self.scroll_y);
        let mut moved = false;
        if self.motion.is_animating() {
            let by = self.geom(rect).map_or(Bounds::max(0.0), |g| Bounds::max(g.max_scroll));
            let bx = self.hgeom(rect).map_or(Bounds::max(0.0), |g| Bounds::max(g.max_scroll));
            moved = self.motion.tick(dt, bx, by);
            self.scroll_x = self.motion.x.pos();
            self.scroll_y = self.motion.y.pos();
            if moved {
                // A glide or a coast is scrolling too: the bars stay up for
                // the whole of it, not only the hold after its first event.
                self.activity.bump();
            }
        }
        // The raise/sink latch and its fade. Frames keep coming while the
        // hold runs and while the fade chases the latch, so the sink is
        // actually drawn rather than frozen at the last input event.
        let holding = self.activity.holding();
        let shown = self.scrollbars_shown(rect);
        let dragging = self.dragging_scrollbar || self.dragging_hscrollbar;
        let flipped = self.activity.tick(dt, shown, dragging);
        let fade = self.activity.fade();
        let fading = if self.activity.raised() { fade < 1.0 } else { fade > 0.0 };
        moved || self.motion.is_animating() || holding || flipped || fading
    }

    fn wants_tick(&self) -> bool {
        true
    }

}
