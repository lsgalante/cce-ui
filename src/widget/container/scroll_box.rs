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
    let (track, track_radii) =
        crate::layout::carve_inside(Rect { x: sb_x, y: sb_track_y, width: sb_w, height: sb_track_h }, radii, depth);
    pc.recess(track, track_radii, depth);
    pc.bevel(
        Rect { x: sb_x, y: thumb_y, width: sb_w, height: thumb_h },
        radii,
        &crate::scene::material::Material::from_fill(crate::color::scrollbar_thumb_color()),
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
    /// Opt-in: the bar rides the box's CENTRE line and idles behind the
    /// host's plate, raised in front by a scroll ([`ScrollbarActivity`]) —
    /// the DE's one scrollbar design, as a sink-behind `ScrollRegion` has it.
    /// The host draws the idle copy before its plate and the fore copy after
    /// its content, both through [`Self::paint_scrollbar_pills`]. Off, the
    /// bar hugs the right edge and is always grabbable, as it always was.
    pub sink_behind: bool,
    activity: crate::widget::ScrollbarActivity,
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
            sink_behind: false,
            activity: crate::widget::ScrollbarActivity::new(),
            motion: crate::widget::scroll_motion::ScrollMotion::new(),
        }
    }

    /// The bar's geometry, `(sb_x, track_y, sb_w, track_h, thumb_y,
    /// thumb_h)`, or `None` while the content fits — the one source for the
    /// paint, the hit test, the press and the drag. Centred on the box's
    /// width at [`crate::layout::centred_scrollbar_width`] for a
    /// [`Self::sink_behind`] box, at the right edge otherwise.
    fn bar_geom(&self) -> Option<(f32, f32, f32, f32, f32, f32)> {
        if self.content_h <= self.viewport_h {
            return None;
        }
        let (sb_x, sb_w) = if self.sink_behind {
            let sb_w = crate::layout::centred_scrollbar_width();
            (self.base.x + (self.base.w - sb_w) * 0.5, sb_w)
        } else {
            let sb_w = crate::layout::scrollbar_width();
            (self.base.x + self.base.w - sb_w - 4.0, sb_w)
        };
        let track_h = self.viewport_h - 8.0;
        let track_y = self.viewport_y + 4.0;
        let visible_ratio = self.viewport_h / self.content_h;
        let thumb_h = if track_h <= 20.0 { track_h } else { (track_h * visible_ratio).clamp(20.0, track_h) };
        let max_scroll = (self.content_h - self.viewport_h).max(0.0);
        let scroll_ratio = if max_scroll > 0.0 { self.scroll_y / max_scroll } else { 0.0 };
        let thumb_y = track_y + scroll_ratio * (track_h - thumb_h);
        Some((sb_x, track_y, sb_w, track_h, thumb_y, thumb_h))
    }

    /// Move the thumb so its grab point is at `py`, returning whether the
    /// offset moved.
    fn drag_thumb_to(&mut self, py: f32) -> bool {
        let Some((_, track_y, _, track_h, _, thumb_h)) = self.bar_geom() else { return false };
        let max_scroll = (self.content_h - self.viewport_h).max(0.0);
        let target = py - self.drag_offset_y;
        let ratio = if track_h - thumb_h > 0.0 { ((target - track_y) / (track_h - thumb_h)).clamp(0.0, 1.0) } else { 0.0 };
        let old = self.scroll_y;
        self.scroll_y = ratio * max_scroll;
        (self.scroll_y - old).abs() > 0.01
    }

    /// Whether the bar is raised in front of the host's plate — the latch,
    /// which gates input. Always true for a box that does not sink.
    pub fn scrollbar_raised(&self) -> bool {
        !self.sink_behind || self.activity.raised()
    }

    /// How far the fore copy has faded in, 0..=1. Always 1 for a box that
    /// does not sink.
    pub fn scrollbar_fade(&self) -> f32 {
        if self.sink_behind { self.activity.fade() } else { 1.0 }
    }

    /// The host moved `scroll_y` itself (a scroll to the selection): raise a
    /// sink-behind bar as a wheel would.
    pub fn notify_scrolled(&mut self) {
        if self.sink_behind {
            self.activity.bump();
            self.activity.recompute(self.content_h > self.viewport_h, self.scrollbar_dragging);
        }
    }

    /// The bar as flat pills in the track and thumb colours, scaled to
    /// `alpha` — what a sink-behind host draws twice: the idle copy at 1
    /// BEFORE its plate, and the fore copy at [`Self::scrollbar_fade`]
    /// after its content. (The relief bar is not used for a sinking one:
    /// shader-lit relief does not fade with a vertex alpha.)
    pub fn paint_scrollbar_pills(&self, pc: &mut crate::scene::paint::PaintCtx, alpha: f32) {
        let a = alpha.clamp(0.0, 1.0);
        let Some((sb_x, track_y, sb_w, track_h, thumb_y, thumb_h)) = self.bar_geom() else { return };
        if a <= 0.001 {
            return;
        }
        let dim = |mut c: [f32; 4]| {
            c[3] *= a;
            c
        };
        use crate::scene::layout::Rect;
        let all = (true, true, true, true);
        pc.rounded_rect(Rect { x: sb_x, y: track_y, width: sb_w, height: track_h }, sb_w.min(track_h) * 0.5, all, dim(crate::color::scrollbar_track_color()));
        pc.rounded_rect(Rect { x: sb_x, y: thumb_y, width: sb_w, height: thumb_h }, sb_w.min(thumb_h) * 0.5, all, dim(crate::color::scrollbar_thumb_color()));
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

    /// Whether `(px, py)` is on the bar's strip (±4px slop). A sunk bar is
    /// behind the host's plate and is never hit: a press on its lane is a
    /// press on the content under it.
    pub fn hit_test_scrollbar(&self, px: f32, py: f32) -> bool {
        if !self.scrollbar_raised() {
            return false;
        }
        self.bar_geom().is_some_and(|(sb_x, track_y, sb_w, track_h, _, _)| {
            px >= sb_x - 4.0 && px <= sb_x + sb_w + 4.0 && py >= track_y && py <= track_y + track_h
        })
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
    fn end_thumb_drag(&mut self) {
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

    pub fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();

        // Background
        if self.show_background {
            quads.push((self.base.x, self.base.y, self.base.w, self.base.h, crate::color::list_bg_color()));
        }



        // Scrollbar
        if let Some((sb_x, track_y, sb_w, track_h, thumb_y, thumb_h)) = self.bar_geom() {
            quads.push((sb_x, track_y, sb_w, track_h, crate::color::scrollbar_track_color()));
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
        let moved = self.keyboard_scroll(event, ctx);
        if moved {
            self.notify_scrolled();
        }
        moved
    }

    fn keyboard_scroll(&mut self, event: &KeyEvent, ctx: &mut UiContext) -> bool {
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

    /// A sink-behind box's bar rides the centre line, takes no press while
    /// sunk (the content under its lane does), is raised by a wheel and held
    /// by a pointer over it, and sinks once nothing holds it; the fore copy
    /// fades rather than flips. A plain box keeps its edge bar.
    #[test]
    fn a_sink_behind_bar_rides_the_centre_and_sinks_until_scrolled() {
        let mut ui = UiContext::new();
        let mut sb = ScrollBox::new();
        sb.sink_behind = true;
        sb.set_rect(10.0, 20.0, 200.0, 100.0);
        sb.update_bounds(400.0, 20.0, 100.0);
        let (sb_x, _, sb_w, _, _, _) = sb.bar_geom().unwrap();
        assert!((sb_x + sb_w * 0.5 - 110.0).abs() < 0.01, "on the centre line");
        assert_eq!(sb_w, crate::layout::centred_scrollbar_width());

        // Sunk: the lane is the content's.
        assert!(!sb.scrollbar_raised());
        assert!(!sb.hit_test_scrollbar(110.0, 60.0));
        assert!(!sb.mouse_input(MouseButton::Left, ElementState::Pressed, 110.0, 60.0, &mut ui));
        assert!(!sb.scrollbar_dragging);
        // Hover never raises a sunk bar.
        sb.on_cursor_moved(110.0, 60.0, &mut ui);
        sb.tick(0.016, &mut ui);
        assert!(!sb.scrollbar_raised());

        // A wheel raises it; the fore copy fades in.
        assert!(sb.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, -1.0), 50.0, 60.0, &mut ui));
        assert!(sb.scrollbar_raised());
        assert!(sb.tick(0.05, &mut ui));
        assert!(sb.scrollbar_fade() > 0.0 && sb.scrollbar_fade() < 1.0, "fading in: {}", sb.scrollbar_fade());
        for _ in 0..1000 {
            if !sb.is_animating() { break; }
            sb.tick(1.0 / 60.0, &mut ui);
        }
        // Raised, the bar takes the press; a pointer over it holds it up.
        assert!(sb.hit_test_scrollbar(110.0, 60.0));
        sb.on_cursor_moved(110.0, 60.0, &mut ui);
        sb.tick(1.0, &mut ui);
        assert!(sb.scrollbar_raised(), "held by the pointer");
        sb.on_cursor_moved(40.0, 60.0, &mut ui);
        sb.tick(0.016, &mut ui);
        assert!(!sb.scrollbar_raised(), "unheld, it sinks");
        for _ in 0..30 {
            sb.tick(0.016, &mut ui);
        }
        assert_eq!(sb.scrollbar_fade(), 0.0);

        let mut plain = ScrollBox::new();
        plain.set_rect(10.0, 20.0, 200.0, 100.0);
        plain.update_bounds(400.0, 20.0, 100.0);
        let (sb_x, _, sb_w, _, _, _) = plain.bar_geom().unwrap();
        assert_eq!(sb_x + sb_w, 10.0 + 200.0 - 4.0, "a plain box keeps its edge bar");
        assert!(plain.scrollbar_raised() && plain.hit_test_scrollbar(sb_x + 1.0, 60.0));
    }

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
