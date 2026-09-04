//! Embedded scroll-math + scrollbar-chrome helper (Phase 6av: DEMOTED from `WidgetHost` to a
//! plain struct). Never registered into the ctx tree by either consumer — TreeList and
//! cce-test-interface's panel copy drive it entirely through concrete calls — so the
//! `WidgetHost` impl was pure dyn-dispatch ballast. The former WidgetHost-default entry points the
//! consumers forward (`cursor_moved`, `tick`, drag hooks, `is_dragging`) are kept as
//! inherent methods with the exact default-derived behavior.

use crate::widget::*;

/// Shared relief-scrollbar painter: the track a carved groove
/// ([`crate::scene::paint::PaintCtx::recess`]), the thumb a raised rounded
/// plate riding in it ([`crate::scene::paint::PaintCtx::bevel`], configured
/// thumb color) — the DE highlight/shadow bevel treatment. `viewport` is the
/// scrolling area's box; the bar hugs its right edge. No-op while the content
/// fits.
pub fn paint_relief_scrollbar(
    pc: &mut crate::scene::paint::PaintCtx,
    viewport: crate::scene::layout::Rect,
    content_h: f32,
    scroll_y: f32,
) {
    if content_h <= viewport.height {
        return;
    }
    let sb_w = crate::layout::scrollbar_width();
    let sb_x = viewport.x + viewport.width - sb_w - 4.0;
    let sb_track_h = viewport.height - 8.0;
    let sb_track_y = viewport.y + 4.0;

    let visible_ratio = viewport.height / content_h;
    let thumb_h = if sb_track_h <= 20.0 {
        sb_track_h
    } else {
        (sb_track_h * visible_ratio).clamp(20.0, sb_track_h)
    };
    let max_scroll = (content_h - viewport.height).max(0.0);
    let scroll_ratio = if max_scroll > 0.0 { scroll_y / max_scroll } else { 0.0 };
    let thumb_y = sb_track_y + scroll_ratio * (sb_track_h - thumb_h);

    // Pill radii; the roll width scales to the bar (the DE bevel width would
    // swallow a 14px-wide thumb whole).
    let r = sb_w * 0.5;
    let depth = crate::layout::bevel_width().min(sb_w * 0.35);
    let radii = (r, r, r, r);
    use crate::scene::layout::Rect;
    pc.recess(
        Rect { x: sb_x, y: sb_track_y, width: sb_w, height: sb_track_h },
        radii,
        depth,
    );
    pc.bevel(
        Rect { x: sb_x, y: thumb_y, width: sb_w, height: thumb_h },
        radii,
        crate::color::scrollbar_thumb_color(),
        depth,
    );
}

#[derive(Debug, Clone)]
pub struct ScrollBox {
    pub base: Widget,
    pub scroll_y: f32,
    pub content_h: f32,
    pub viewport_y: f32,
    pub viewport_h: f32,
    viewport_offset_y: f32,
    viewport_offset_h: f32,
    pub show_border: bool,
    pub show_background: bool,
    pub scrollbar_dragging: bool,
    pub drag_offset_y: f32,
    /// Smooth-scroll driver behind `scroll_y` (see `ScrollRegion::motion`).
    motion: crate::widget::scroll_motion::ScrollMotion,
}

impl ScrollBox {
    pub fn new() -> Self {
        Self {
            base: Widget::new(),
            scroll_y: 0.0,
            content_h: 0.0,
            viewport_y: 0.0,
            viewport_h: 0.0,
            viewport_offset_y: 0.0,
            viewport_offset_h: 0.0,
            show_border: true,
            show_background: true,
            scrollbar_dragging: false,
            drag_offset_y: 0.0,
            motion: crate::widget::scroll_motion::ScrollMotion::new(),
        }
    }

    pub fn update_bounds(&mut self, content_h: f32, viewport_y: f32, viewport_h: f32) {
        self.content_h = content_h;
        self.viewport_y = viewport_y;
        self.viewport_h = viewport_h;
        self.viewport_offset_y = viewport_y - self.base.y;
        self.viewport_offset_h = viewport_h - self.base.h;
        let max_scroll = (content_h - viewport_h).max(0.0);
        self.scroll_y = self.scroll_y.clamp(0.0, max_scroll);
        self.motion.set_bounds(Bounds::max(0.0), Bounds::max(max_scroll));
    }

    fn bounds_y(&self) -> Bounds {
        Bounds::max((self.content_h - self.viewport_h).max(0.0))
    }

    /// Whether a glide or coast is still moving the offset.
    pub fn is_animating(&self) -> bool {
        self.motion.is_animating()
    }

    pub fn hit_test_scrollbar(&self, px: f32, py: f32) -> bool {
        if self.content_h <= self.viewport_h {
            return false;
        }
        let sb_w = crate::layout::scrollbar_width();
        let sb_x = self.base.x + self.base.w - sb_w - 4.0;
        let sb_track_h = self.viewport_h - 8.0;
        let sb_track_y = self.viewport_y + 4.0;

        px >= sb_x - 4.0 && px <= sb_x + sb_w + 4.0
            && py >= sb_track_y && py <= sb_track_y + sb_track_h
    }

    /// Screen y for an item at `virtual_y`, or `None` when it doesn't
    /// intersect the viewport at all. Partially visible items ARE returned —
    /// callers draw under a clip rect (or clamp per quad), so an edge item
    /// renders cut, not culled, and hit-testing must accept the same partial
    /// items the draw shows. (The original full-containment test here is what
    /// made list rows vanish the moment they touched the viewport edge, in
    /// every app that copied it.)
    pub fn get_item_draw_y(&self, virtual_y: f32, item_h: f32) -> Option<f32> {
        let draw_y = self.viewport_y + virtual_y - self.scroll_y;
        if draw_y + item_h >= self.viewport_y - 1.0
            && draw_y <= self.viewport_y + self.viewport_h + 1.0
        {
            Some(draw_y)
        } else {
            None
        }
    }
}

impl ScrollBox {
    pub fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.base.x = x;
        self.base.y = y;
        self.base.w = w;
        self.base.h = h;
        self.viewport_y = y + self.viewport_offset_y;
        self.viewport_h = h + self.viewport_offset_h;
    }

    /// The legacy `WidgetHost` default hit test over the base rect (ScrollBox never carried a
    /// label or row expansion, so those branches are folded away).
    fn hit_test(&self, px: f32, py: f32, ctx: &UiContext) -> bool {
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
    fn claim_focus(&self, ctx: &mut UiContext) {
        focus::clear_focus(Some(ctx));
    }

    pub fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if button == MouseButton::Left {
            if state == ElementState::Pressed {
                if self.hit_test_scrollbar(px, py) {
                    self.claim_focus(ctx);
                    self.scrollbar_dragging = true;
                    
                    let sb_track_h = self.viewport_h - 8.0;
                    let sb_track_y = self.viewport_y + 4.0;
                    let visible_ratio = self.viewport_h / self.content_h;
                    let thumb_h = if sb_track_h <= 20.0 {
                        sb_track_h
                    } else {
                        (sb_track_h * visible_ratio).clamp(20.0, sb_track_h)
                    };
                    let max_scroll = (self.content_h - self.viewport_h).max(0.0);
                    let scroll_ratio = if max_scroll > 0.0 { self.scroll_y / max_scroll } else { 0.0 };
                    let thumb_y = sb_track_y + scroll_ratio * (sb_track_h - thumb_h);
                    
                    let click_offset = py - thumb_y;
                    if click_offset >= 0.0 && click_offset <= thumb_h {
                        self.drag_offset_y = click_offset;
                    } else {
                        // Clicked outside the thumb: jump thumb center to py
                        self.drag_offset_y = thumb_h / 2.0;
                        let target_thumb_y = py - self.drag_offset_y;
                        let new_scroll_ratio = if sb_track_h - thumb_h > 0.0 {
                            ((target_thumb_y - sb_track_y) / (sb_track_h - thumb_h)).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        self.scroll_y = new_scroll_ratio * max_scroll;
                    }
                    return true;
                } else {
                    self.scrollbar_dragging = false;
                }
                if self.hit_test(px, py, ctx) {
                    self.claim_focus(ctx);
                }
            } else if state == ElementState::Released {
                self.scrollbar_dragging = false;
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
        let sb_track_h = self.viewport_h - 8.0;
        let sb_track_y = self.viewport_y + 4.0;
        let visible_ratio = self.viewport_h / self.content_h;
        let thumb_h = if sb_track_h <= 20.0 {
            sb_track_h
        } else {
            (sb_track_h * visible_ratio).clamp(20.0, sb_track_h)
        };
        let max_scroll = (self.content_h - self.viewport_h).max(0.0);
        
        let target_thumb_y = py - self.drag_offset_y;
        let new_scroll_ratio = if sb_track_h - thumb_h > 0.0 {
            ((target_thumb_y - sb_track_y) / (sb_track_h - thumb_h)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        
        let old_scroll = self.scroll_y;
        self.scroll_y = new_scroll_ratio * max_scroll;
        (self.scroll_y - old_scroll).abs() > 0.01
    }

    pub fn drag_end(&mut self) {
        self.scrollbar_dragging = false;
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
        if self.scrollbar_dragging {
            let sb_track_h = self.viewport_h - 8.0;
            let sb_track_y = self.viewport_y + 4.0;
            let visible_ratio = self.viewport_h / self.content_h;
            let thumb_h = if sb_track_h <= 20.0 {
                sb_track_h
            } else {
                (sb_track_h * visible_ratio).clamp(20.0, sb_track_h)
            };
            let max_scroll = (self.content_h - self.viewport_h).max(0.0);
            
            let target_thumb_y = py - self.drag_offset_y;
            let new_scroll_ratio = if sb_track_h - thumb_h > 0.0 {
                ((target_thumb_y - sb_track_y) / (sb_track_h - thumb_h)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            
            let old_scroll = self.scroll_y;
            self.scroll_y = new_scroll_ratio * max_scroll;
            if (self.scroll_y - old_scroll).abs() > 0.01 {
                changed = true;
            }
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
        moved || self.motion.is_animating()
    }

    pub fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if self.hit_test(px, py, ctx) {
            self.motion.reconcile(0.0, self.scroll_y);
            let changed = self.motion.apply(delta, (LINE_PX, LINE_PX), Bounds::max(0.0), self.bounds_y());
            self.scroll_y = self.motion.y.pos();
            changed
        } else {
            false
        }
    }

    pub fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();

        // Background
        if self.show_background {
            quads.push((self.base.x, self.base.y, self.base.w, self.base.h, crate::color::list_bg_color()));
        }



        // Scrollbar
        if self.content_h > self.viewport_h {
            let sb_w = crate::layout::scrollbar_width();
            let sb_x = self.base.x + self.base.w - sb_w - 4.0;
            let sb_track_h = self.viewport_h - 8.0;
            let sb_track_y = self.viewport_y + 4.0;

            // Track
            quads.push((sb_x, sb_track_y, sb_w, sb_track_h, crate::color::scrollbar_track_color()));

            // Thumb
            let visible_ratio = self.viewport_h / self.content_h;
            let thumb_h = if sb_track_h <= 20.0 {
                sb_track_h
            } else {
                (sb_track_h * visible_ratio).clamp(20.0, sb_track_h)
            };
            let max_scroll = (self.content_h - self.viewport_h).max(0.0);
            let scroll_ratio = if max_scroll > 0.0 { self.scroll_y / max_scroll } else { 0.0 };
            let thumb_y = sb_track_y + scroll_ratio * (sb_track_h - thumb_h);

            quads.push((sb_x, thumb_y, sb_w, thumb_h, crate::color::scrollbar_thumb_color()));
        }

        quads
    }

    /// The scrollbar in the DE's relief language — see
    /// [`paint_relief_scrollbar`]. The flat-quad view stays available through
    /// [`extra_quads`](Self::extra_quads) for legacy paths.
    pub fn paint_scrollbar_relief(&self, pc: &mut crate::scene::paint::PaintCtx) {
        paint_relief_scrollbar(
            pc,
            crate::scene::layout::Rect {
                x: self.base.x,
                y: self.viewport_y,
                width: self.base.w,
                height: self.viewport_h,
            },
            self.content_h,
            self.scroll_y,
        );
    }

    pub fn keyboard_input(&mut self, event: &KeyEvent, ctx: &mut UiContext) -> bool {
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

unsafe impl Send for ScrollBox {}
unsafe impl Sync for ScrollBox {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scroll_box_bounds_scrolling() {
        let mut sb = ScrollBox::new();
        sb.set_rect(10.0, 20.0, 100.0, 100.0);
        
        // 1. Initially scroll is 0
        assert_eq!(sb.scroll_y, 0.0);

        // 2. Update bounds: content_h = 150 (greater than viewport_h = 100)
        sb.update_bounds(150.0, 20.0, 100.0);
        assert_eq!(sb.scroll_y, 0.0);
        assert_eq!(sb.content_h, 150.0);
        assert_eq!(sb.viewport_h, 100.0);

        // 3. Scroll inside bounds
        let delta = MouseScrollDelta::LineDelta(0.0, -2.0); // scroll down by 2 lines (48px)
        let mut dummy = UiContext::new();
        let changed = sb.mouse_wheel(&delta, 50.0, 50.0, &mut dummy);
        assert!(changed);
        // The notch glides: run the motion out before reading the offset.
        for _ in 0..1000 {
            if !sb.is_animating() { break; }
            sb.tick(1.0 / 60.0, &mut dummy);
        }
        assert_eq!(sb.scroll_y, 48.0);

        // 4. Clamps at max scroll: 150 - 100 = 50
        let delta_large = MouseScrollDelta::LineDelta(0.0, -10.0);
        sb.mouse_wheel(&delta_large, 50.0, 50.0, &mut dummy);
        for _ in 0..1000 {
            if !sb.is_animating() { break; }
            sb.tick(1.0 / 60.0, &mut dummy);
        }
        assert_eq!(sb.scroll_y, 50.0);

        // 5. Test item draw coordinates (intersection contract: partially
        // visible items are returned so callers draw them cut by the clip).
        // Virtual item at virtual_y = 10, item_h = 24
        // Screen draw y = 20 + 10 - 50 = -20; bottom = 4 < viewport_y - 1
        // (19.0): fully above the viewport, culled.
        assert!(sb.get_item_draw_y(10.0, 24.0).is_none());

        // Virtual item at virtual_y = 40, item_h = 24
        // Screen draw y = 20 + 40 - 50 = 10: straddles the viewport top
        // (bottom = 34 >= 19.0) — returned, drawn cut by the clip.
        assert_eq!(sb.get_item_draw_y(40.0, 24.0), Some(10.0));

        // Virtual item at virtual_y = 60, item_h = 24
        // Screen draw y = 20 + 60 - 50 = 30: fully inside.
        assert_eq!(sb.get_item_draw_y(60.0, 24.0), Some(30.0));
    }

    #[test]
    fn test_scroll_box_keyboard_input() {
        let mut sb = ScrollBox::new();
        sb.set_rect(10.0, 20.0, 100.0, 100.0);
        sb.update_bounds(300.0, 20.0, 100.0); // max_scroll = 200.0

        let mut ctx = UiContext::new();
        // Hover the scroll box (the focus path took a ctx-registered WidgetHost; as a plain
        // struct the hovered branch is the live gate).
        ctx.set_cursor_pos(50.0, 50.0);

        // 1. ArrowDown key
        let event_down = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::ArrowDown),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
            alt: false,
        };
        assert!(sb.keyboard_input(&event_down, &mut ctx));
        assert_eq!(sb.scroll_y, 24.0);

        // 2. PageDown key
        let event_pgdown = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::PageDown),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
            alt: false,
        };
        assert!(sb.keyboard_input(&event_pgdown, &mut ctx));
        assert_eq!(sb.scroll_y, 124.0);

        // 3. End key
        let event_end = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::End),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
            alt: false,
        };
        assert!(sb.keyboard_input(&event_end, &mut ctx));
        assert_eq!(sb.scroll_y, 200.0); // clamps at max_scroll = 200.0

        // 4. PageUp key
        let event_pgup = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::PageUp),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
            alt: false,
        };
        assert!(sb.keyboard_input(&event_pgup, &mut ctx));
        assert_eq!(sb.scroll_y, 100.0);

        // 5. Home key
        let event_home = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::Home),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
            alt: false,
        };
        assert!(sb.keyboard_input(&event_home, &mut ctx));
        assert_eq!(sb.scroll_y, 0.0);
    }

    #[test]
    fn test_scroll_box_keys_gated_on_hover() {
        let mut sb = ScrollBox::new();
        sb.set_rect(10.0, 20.0, 100.0, 100.0);
        sb.update_bounds(300.0, 20.0, 100.0); // max_scroll = 200.0

        let mut ctx = UiContext::new();

        let event_down = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::ArrowDown),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
            alt: false,
        };

        // Cursor away from the box, nothing focused: keys are ignored.
        ctx.set_cursor_pos(500.0, 500.0);
        assert!(!sb.keyboard_input(&event_down, &mut ctx));
        assert_eq!(sb.scroll_y, 0.0);

        // Hovered: keys scroll.
        ctx.set_cursor_pos(50.0, 50.0);
        assert!(sb.keyboard_input(&event_down, &mut ctx));
        assert_eq!(sb.scroll_y, 24.0);
    }
}
