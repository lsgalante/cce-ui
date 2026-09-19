//! The shared app-side scroll region — scroll state, virtualization math,
//! scrollbar geometry/input, and frame/scrollbar emission for a list whose ROWS
//! the app draws itself.
//!
//! Lifted from the per-app copies (cce-system-interface's Phase 6q dissolution
//! original, re-copied into cce-fonts, cce-mail, cce-cloud, and
//! cce-layout-interface) so the virtualization CONTRACT lives in one place:
//!
//! **`get_item_draw_y`/`get_draw_y` return every row that INTERSECTS the
//! viewport, partially visible rows included. Callers draw those rows under a
//! clip (the `PaintCtx` clip stack, or per-quad clamping), so an edge row
//! renders cut — never culled.** The copies' original full-containment test
//! made cards vanish the moment they touched the viewport edge, separately in
//! every app; hit-testing must accept the same partial rows the draw shows
//! (a visible sliver that ignores clicks is the same bug mirrored).
//!
//! The frame/scrollbar can be emitted two ways, matching the two app styles:
//! [`ScrollRegion::push_prims`] draws the bordered frame + pill scrollbars onto
//! a [`crate::layout::RenderTarget`]; [`ScrollRegion::push_quads`] /
//! [`ScrollRegion::push_scrollbar_quads`] emit flat quads for hosts on the
//! tuple pipeline (split so the scrollbar can draw AFTER the rows — drawn
//! together, rows paint over the thumb and it peeks through the inter-row gaps
//! as dotted segments).

use crate::widget::scroll_motion::{scroll_settings, Bounds, ScrollMotion, LINE_PX};
use crate::widget::{ElementState, Key, KeyEvent, MouseScrollDelta, NamedKey};

/// How long (seconds) a raise/sink scrollbar stays raised after the last wheel
/// scroll or drag release.
pub const SCROLL_ACTIVE_HOLD: f32 = 0.7;

/// How long (seconds) the raise and the sink take to cross-fade. Coming to the
/// fore is a fade, not a flip: the bar reads as rising through the host's
/// frosted plate rather than being swapped for a copy of itself.
pub const SCROLL_FADE_SECS: f32 = 0.18;

/// The raise/sink hysteresis for scrollbars that idle BEHIND their host's
/// translucent plate — the designer parameter-pane treatment, shared so every
/// app's bar behaves the same way. The bar has two depths: *raised* it draws in
/// front of the content and takes input; *sunk* it draws under the host's plate
/// (dimly visible through a translucent one) and is non-interactive, because
/// the plate occludes it.
///
/// The rules: a scroll (wheel, keyboard) or an active thumb drag raises the
/// bar, and a drag release refreshes the hold. Hover only *sustains* a bar
/// that is already raised — it can never raise a sunk one, since the pointer
/// is really over the plate, not the bar. Once nothing holds it up for
/// [`SCROLL_ACTIVE_HOLD`] seconds it sinks, and only scrolling brings it back.
///
/// The owner drives it: [`Self::bump`] on scrolls and drag releases,
/// [`Self::set_hover`] from pointer moves, [`Self::tick`] once per frame
/// (which decays the hold and recomputes — a `true` return is the repaint
/// signal for the raise/sink flip).
#[derive(Debug, Clone, Default)]
pub struct ScrollbarActivity {
    /// Seconds left in the "recently scrolled" window that keeps the bar raised.
    activity: f32,
    hover: bool,
    raised: bool,
    /// How far the FORE copy has faded in, 0..=1. Chases `raised` over
    /// [`SCROLL_FADE_SECS`]; only [`Self::tick`] advances it, so a host that
    /// drives the latch through `recompute` alone keeps the old hard flip.
    fade: f32,
}

impl ScrollbarActivity {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the bar is currently raised in front of the plate. While false
    /// it sits behind the plate and must not take input. This is the LATCH —
    /// it flips at once, so input never waits on the fade.
    pub fn raised(&self) -> bool {
        self.raised
    }

    /// Opacity of the fore copy, 0..=1: 0 while fully sunk (only the copy
    /// behind the plate shows), 1 once risen. Paint with this; gate input on
    /// [`Self::raised`].
    pub fn fade(&self) -> f32 {
        self.fade
    }

    /// Refresh the hold window: call on a wheel/keyboard scroll and on a drag
    /// release.
    pub fn bump(&mut self) {
        self.activity = SCROLL_ACTIVE_HOLD;
    }

    /// Track whether the pointer sits over the bar (raw geometry — the caller
    /// does not gate this on raised; the hysteresis is what limits hover to
    /// sustaining).
    pub fn set_hover(&mut self, over: bool) {
        self.hover = over;
    }

    /// Whether the post-scroll hold window is still running — owners whose tick
    /// chain only runs while frames are being drawn use this to keep frames
    /// coming until the sink actually renders.
    pub fn holding(&self) -> bool {
        self.activity > 0.0
    }

    /// Recompute the latched raised state, returning whether it changed.
    pub fn recompute(&mut self, visible: bool, dragging: bool) -> bool {
        let raised = visible && (dragging || self.activity > 0.0 || (self.raised && self.hover));
        let changed = raised != self.raised;
        self.raised = raised;
        changed
    }

    /// Per-frame decay + recompute. Returns whether the raised state flipped —
    /// the owner's repaint signal.
    pub fn tick(&mut self, dt: f32, visible: bool, dragging: bool) -> bool {
        if self.activity > 0.0 {
            self.activity = (self.activity - dt).max(0.0);
        }
        let flipped = self.recompute(visible, dragging);
        // Chase the latch. The step is over the WHOLE range, so a fade
        // reversed halfway takes proportionally less time rather than
        // restarting — a flick-scroll-flick does not stutter.
        let target = if self.raised { 1.0 } else { 0.0 };
        let step = if SCROLL_FADE_SECS > 0.0 { dt / SCROLL_FADE_SECS } else { 1.0 };
        let moved = if (self.fade - target).abs() <= step {
            let done = self.fade != target;
            self.fade = target;
            done
        } else {
            self.fade += step * (target - self.fade).signum();
            true
        };
        flipped || moved
    }
}

#[derive(Debug, Clone)]
pub struct ScrollRegion {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Row height, with `List::new`'s silent adjustment to `max(item_height, list_font + 14)`.
    pub item_height: f32,
    pub item_gap: f32,
    pub scroll_y: f32,
    pub content_h: f32,
    pub viewport_y: f32,
    pub viewport_h: f32,
    /// Horizontal scrolling is opt-in per list: it activates only when a page
    /// declares a content width wider than the box (`set_content_w`). The
    /// default 0 keeps every vertical-only list exactly as it was.
    pub scroll_x: f32,
    pub content_w: f32,
    pub dragging: bool,
    dragging_h: bool,
    drag_offset_y: f32,
    drag_offset_x: f32,
    pub hovered: bool,
    /// Local stand-in for the legacy global focus flag (`ScrollBox::focus()` on any press
    /// inside the frame): set on a press that hits the region, cleared on one that misses.
    pub focused: bool,
    /// Draw the border + background plate in `push_prims`. Off = frameless: rows
    /// sit directly on the window plate (the scrollbar still draws).
    pub draw_frame: bool,
    /// Gap between the vertical bar's right edge and the region's right edge.
    /// The default 4.0 hugs a framed list's border; page-level bars floating
    /// over a window plate use [`crate::layout::scrollbar_inset`] for the
    /// designer's stood-off look.
    pub edge_inset: f32,
    /// Opt-in raise/sink behavior ([`ScrollbarActivity`]): the bar idles sunk
    /// (host draws it behind its plate via [`Self::push_scrollbar_prims`]) and
    /// is non-interactive until a scroll raises it. Off (the default), the bar
    /// is always drawn and always grabbable — existing hosts unchanged.
    pub sink_behind: bool,
    activity: ScrollbarActivity,
    /// The smooth-scroll driver behind `scroll_x`/`scroll_y`: wheel notches
    /// glide, trackpad flicks coast. The pub offsets stay the DRAWN values —
    /// hosts keep reading them — and any host write to them is adopted on the
    /// next `wheel`/`tick` via `reconcile`.
    motion: ScrollMotion,
}

impl Default for ScrollRegion {
    /// A region with no geometry yet — `set_rect`/`update_bounds` supply that on
    /// the first view pass. `new(0.0, ..)` floors `item_height` at the list
    /// font's line box, so a host that forgets to size its rows still gets a
    /// legible one rather than a zero-height row that never draws.
    fn default() -> Self {
        Self::new(0.0, 4.0)
    }
}

impl ScrollRegion {
    pub fn new(item_height: f32, item_gap: f32) -> Self {
        let (_, font_size) = crate::layout::list_font_parsed();
        Self {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            item_height: item_height.max(font_size + 14.0),
            item_gap,
            scroll_y: 0.0,
            content_h: 0.0,
            viewport_y: 0.0,
            viewport_h: 0.0,
            scroll_x: 0.0,
            content_w: 0.0,
            dragging: false,
            dragging_h: false,
            drag_offset_y: 0.0,
            drag_offset_x: 0.0,
            hovered: false,
            focused: false,
            draw_frame: true,
            edge_inset: 4.0,
            sink_behind: false,
            activity: ScrollbarActivity::new(),
            motion: ScrollMotion::new(),
        }
    }

    pub fn with_frame(mut self, draw_frame: bool) -> Self {
        self.draw_frame = draw_frame;
        self
    }

    pub fn with_edge_inset(mut self, inset: f32) -> Self {
        self.edge_inset = inset;
        self
    }

    pub fn with_sink_behind(mut self, sink: bool) -> Self {
        self.sink_behind = sink;
        self
    }

    pub fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.x = x;
        self.y = y;
        self.w = w;
        self.h = h;
    }

    /// The `List::update_bounds` count math: `content_h = count * (item_height + gap) + 4`.
    pub fn update_bounds(&mut self, count: usize, viewport_y: f32, viewport_h: f32) {
        self.content_h = count as f32 * (self.item_height + self.item_gap) + 4.0;
        self.viewport_y = viewport_y;
        self.viewport_h = viewport_h;
        self.scroll_y = self.scroll_y.clamp(0.0, self.max_scroll());
        self.motion.set_bounds(self.bounds_x(), self.bounds_y());
    }

    /// `ScrollBox::update_bounds` shape: raw content height, not a row count.
    pub fn update_bounds_raw(&mut self, content_h: f32, viewport_y: f32, viewport_h: f32) {
        self.content_h = content_h;
        self.viewport_y = viewport_y;
        self.viewport_h = viewport_h;
        self.scroll_y = self.scroll_y.clamp(0.0, self.max_scroll());
        self.motion.set_bounds(self.bounds_x(), self.bounds_y());
    }

    /// Jump the offset (no glide) — cancels any motion in flight.
    pub fn set_scroll_y(&mut self, val: f32) {
        self.scroll_y = val;
        self.motion.y.jump_to(val);
    }

    /// Glide the offset to `val` (a "scroll to selection" that should read as
    /// motion, not a cut). Falls back to a jump with smoothing off.
    pub fn scroll_to_y(&mut self, val: f32) -> bool {
        self.motion.reconcile(self.scroll_x, self.scroll_y);
        let moved = self.motion.y.scroll_to(val, self.bounds_y(), &scroll_settings());
        self.sync_from_motion();
        if moved {
            self.raise();
        }
        moved
    }

    /// Whether a glide or coast is still moving the offset — hosts whose tick
    /// chain is conditional use this to keep frames coming.
    pub fn is_animating(&self) -> bool {
        self.motion.is_animating()
    }

    fn bounds_y(&self) -> Bounds {
        Bounds::max(self.max_scroll())
    }

    fn bounds_x(&self) -> Bounds {
        Bounds::max(self.max_scroll_x())
    }

    fn sync_from_motion(&mut self) {
        self.scroll_x = self.motion.x.pos();
        self.scroll_y = self.motion.y.pos();
    }

    /// Declare how wide the content really is. Wider than the box = the list
    /// scrolls horizontally (bottom scrollbar, x wheel deltas, arrow keys).
    pub fn set_content_w(&mut self, w: f32) {
        self.content_w = w;
        self.scroll_x = self.scroll_x.clamp(0.0, self.max_scroll_x());
        self.motion.x.set_bounds(self.bounds_x());
    }

    /// Whether horizontal scrolling is live (content declared wider than the
    /// box). Pages use this to reserve bottom room for the h-bar.
    pub fn h_scroll_active(&self) -> bool {
        self.content_w > self.w
    }

    pub fn max_scroll(&self) -> f32 {
        (self.content_h - self.viewport_h).max(0.0)
    }

    pub fn max_scroll_x(&self) -> f32 {
        (self.content_w - self.w).max(0.0)
    }

    pub fn hit(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }

    /// Row virtualization (`List::get_item_draw_y`): screen y for row `idx`, or `None`
    /// when the row doesn't intersect the viewport at all. Partially visible rows
    /// ARE returned — callers draw under a clip rect, so they render cut, not culled.
    pub fn get_item_draw_y(&self, idx: usize, offset: f32) -> Option<f32> {
        let virtual_y = idx as f32 * (self.item_height + self.item_gap) + offset;
        self.get_draw_y(virtual_y, self.item_height)
    }

    /// `ScrollBox::get_item_draw_y` shape: a precomputed virtual y + item
    /// height. Same intersection contract as [`Self::get_item_draw_y`].
    pub fn get_draw_y(&self, virtual_y: f32, item_h: f32) -> Option<f32> {
        let draw_y = self.viewport_y + virtual_y - self.scroll_y;
        if draw_y + item_h >= self.viewport_y - 1.0
            && draw_y <= self.viewport_y + self.viewport_h + 1.0
        {
            Some(draw_y)
        } else {
            None
        }
    }

    /// Scrollbar geometry (`ScrollBox::extra_quads`): (sb_x, track_y, sb_w, track_h, thumb_y, thumb_h).
    fn scrollbar_geom(&self) -> (f32, f32, f32, f32, f32, f32) {
        let sb_w = crate::layout::scrollbar_width();
        let sb_x = self.x + self.w - sb_w - self.edge_inset;
        let track_h = self.viewport_h - 8.0;
        let track_y = self.viewport_y + 4.0;
        let visible_ratio = self.viewport_h / self.content_h.max(1.0);
        let thumb_h = if track_h <= 20.0 {
            track_h
        } else {
            (track_h * visible_ratio).clamp(20.0, track_h)
        };
        let scroll_ratio = if self.max_scroll() > 0.0 { self.scroll_y / self.max_scroll() } else { 0.0 };
        let thumb_y = track_y + scroll_ratio * (track_h - thumb_h);
        (sb_x, track_y, sb_w, track_h, thumb_y, thumb_h)
    }

    pub fn hit_scrollbar(&self, px: f32, py: f32) -> bool {
        if self.content_h <= self.viewport_h {
            return false;
        }
        // A sunk bar is behind the host's plate: the plate occludes it, so the
        // pointer can neither grab nor jump-scroll it.
        if self.sink_behind && !self.activity.raised() {
            return false;
        }
        let (sb_x, track_y, sb_w, track_h, _, _) = self.scrollbar_geom();
        px >= sb_x - 4.0 && px <= sb_x + sb_w + 4.0 && py >= track_y && py <= track_y + track_h
    }

    /// Bottom scrollbar geometry, mirroring [`Self::scrollbar_geom`] with the
    /// axes swapped: (track_x, sb_y, track_w, sb_h, thumb_x, thumb_w). The
    /// track stops short of the vertical bar's strip so the pills never
    /// overlap in the corner.
    fn h_scrollbar_geom(&self) -> (f32, f32, f32, f32, f32, f32) {
        let sb_h = crate::layout::scrollbar_width();
        let sb_y = self.y + self.h - sb_h - 4.0;
        let right_reserve = if self.content_h > self.viewport_h { sb_h + 8.0 } else { 0.0 };
        let track_x = self.x + 4.0;
        let track_w = self.w - 8.0 - right_reserve;
        let visible_ratio = self.w / self.content_w.max(1.0);
        let thumb_w = if track_w <= 20.0 {
            track_w
        } else {
            (track_w * visible_ratio).clamp(20.0, track_w)
        };
        let ratio = if self.max_scroll_x() > 0.0 { self.scroll_x / self.max_scroll_x() } else { 0.0 };
        let thumb_x = track_x + ratio * (track_w - thumb_w);
        (track_x, sb_y, track_w, sb_h, thumb_x, thumb_w)
    }

    fn hit_h_scrollbar(&self, px: f32, py: f32) -> bool {
        if !self.h_scroll_active() {
            return false;
        }
        if self.sink_behind && !self.activity.raised() {
            return false;
        }
        let (track_x, sb_y, track_w, sb_h, _, _) = self.h_scrollbar_geom();
        py >= sb_y - 4.0 && py <= sb_y + sb_h + 4.0 && px >= track_x && px <= track_x + track_w
    }

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

    fn drag_move(&mut self, py: f32) -> bool {
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

    /// Whether the bar overflows in either axis — the raise/sink "visible" input.
    fn overflowing(&self) -> bool {
        self.content_h > self.viewport_h || self.h_scroll_active()
    }

    /// Refresh the raise hold and recompute immediately, so a scroll shows the
    /// bar in the same frame's redraw rather than one tick later.
    fn raise(&mut self) {
        if self.sink_behind {
            self.activity.bump();
            self.activity.recompute(self.overflowing(), self.dragging || self.dragging_h);
        }
    }

    /// A host scrolled the region programmatically (selection auto-snap, jump
    /// to a search hit): raise a sink-behind bar the same way a wheel scroll
    /// does. No-op for regions without [`Self::sink_behind`].
    pub fn notify_scrolled(&mut self) {
        self.raise();
    }

    /// Whether the scrollbar currently draws in front of the content and takes
    /// input. Always true for a region without [`Self::sink_behind`].
    pub fn scrollbar_raised(&self) -> bool {
        !self.sink_behind || self.activity.raised()
    }

    /// Opacity of the fore copy of the bar, 0..=1 — see
    /// [`ScrollbarActivity::fade`]. Always 1 for a region that never sinks.
    pub fn scrollbar_fade(&self) -> f32 {
        if self.sink_behind {
            self.activity.fade()
        } else {
            1.0
        }
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

    /// The legacy frame, single-drawn: 1px rounded border (focus/hover tinted, from
    /// `List::solid_border`), inset rounded bg, then the scrollbar track and thumb ON TOP.
    pub fn push_prims(&self, pc: &mut dyn crate::layout::RenderTarget) {
        if self.draw_frame {
            let radius = crate::layout::list_corner_radius();
            let border_color = if self.focused {
                [0.30, 0.50, 0.32, 1.0]
            } else if self.hovered {
                [0.25, 0.25, 0.35, 1.0]
            } else {
                [0.18, 0.18, 0.24, 1.0]
            };
            let all = (true, true, true, true);
            pc.rect_with_radius_corners(border_color, self.x, self.y, self.w, self.h, radius, all);
            // A sunk sink-behind bar draws here, UNDER the translucent bg fill
            // (list_bg_color's alpha is 0.3): it shows through dimly, sunk into
            // the list plate — the designer parameter-pane look, self-contained
            // for framed regions.
            if self.sink_behind && !self.activity.raised() {
                self.push_scrollbar_prims(pc);
            }
            pc.rect_with_radius_corners(
                crate::color::list_bg_color(),
                self.x + 1.0,
                self.y + 1.0,
                self.w - 2.0,
                self.h - 2.0,
                (radius - 1.0).max(0.0),
                all,
            );
        }
        // Raised (or plain always-on): the bar rides on top. A FRAMELESS
        // sink-behind region draws no sunk layer here — its rows sit directly
        // on the host's plate, so the host owns the under-plate emission via
        // `push_scrollbar_prims`.
        // The fore copy fades rather than flips, and keeps drawing all the way
        // out — gating this on `scrollbar_raised` would cut the fade off at the
        // latch. The tuple path below is deliberately left on the hard flip:
        // its hosts have no frosted plate for a sunk bar to show through.
        let fade = self.scrollbar_fade();
        if fade > 0.001 {
            self.push_scrollbar_prims_alpha(pc, fade);
        }
    }

    /// The pill scrollbars alone (track + thumb, both axes), drawn wherever the
    /// host calls it. A sink-behind host emits this twice a frame at most:
    /// under its plate while the bar is sunk, over the content while raised.
    pub fn push_scrollbar_prims(&self, pc: &mut dyn crate::layout::RenderTarget) {
        self.push_scrollbar_prims_alpha(pc, 1.0);
    }

    /// [`Self::push_scrollbar_prims`] with the track and thumb scaled to
    /// `alpha` — what a host draws the FORE copy with while it fades in and
    /// out. The copy that idles behind the plate is drawn at full alpha; the
    /// plate over it is what dims and frosts it.
    pub fn push_scrollbar_prims_alpha(&self, pc: &mut dyn crate::layout::RenderTarget, alpha: f32) {
        let a = alpha.clamp(0.0, 1.0);
        if a <= 0.001 {
            return;
        }
        let dim = |mut c: [f32; 4]| {
            c[3] *= a;
            c
        };
        if self.content_h > self.viewport_h {
            // Track and thumb are pills — half-width radius (the designer look).
            let (sb_x, track_y, sb_w, track_h, thumb_y, thumb_h) = self.scrollbar_geom();
            let all = (true, true, true, true);
            pc.rect_with_radius_corners(dim(crate::color::scrollbar_track_color()), sb_x, track_y, sb_w, track_h, sb_w.min(track_h) * 0.5, all);
            pc.rect_with_radius_corners(dim(crate::color::scrollbar_thumb_color()), sb_x, thumb_y, sb_w, thumb_h, sb_w.min(thumb_h) * 0.5, all);
        }
        if self.h_scroll_active() {
            let (track_x, sb_y, track_w, sb_h, thumb_x, thumb_w) = self.h_scrollbar_geom();
            let all = (true, true, true, true);
            pc.rect_with_radius_corners(dim(crate::color::scrollbar_track_color()), track_x, sb_y, track_w, sb_h, sb_h.min(track_w) * 0.5, all);
            pc.rect_with_radius_corners(dim(crate::color::scrollbar_thumb_color()), thumb_x, sb_y, thumb_w, sb_h, sb_h.min(thumb_w) * 0.5, all);
        }
    }

    /// Flat background fill for hosts on the tuple pipeline. The scrollbar is
    /// split into [`Self::push_scrollbar_quads`] so the host can emit it AFTER
    /// the rows — drawn together, the rows paint over the thumb and it peeks
    /// through the inter-row gaps as dotted segments.
    ///
    /// For a sink-behind region, a sunk bar is emitted here FIRST, under the
    /// translucent bg fill (alpha 0.3), so it shows through dimly — and
    /// [`Self::push_scrollbar_quads`] goes quiet. The host's existing
    /// bg → rows → scrollbar order needs no change to adopt the treatment.
    pub fn push_quads(&self, quads: &mut Vec<(f32, f32, f32, f32, [f32; 4])>) {
        if self.sink_behind && !self.activity.raised() {
            self.push_scrollbar_quads_always(quads);
        }
        quads.push((self.x, self.y, self.w, self.h, crate::color::list_bg_color()));
    }

    /// Scrollbar track + thumb when the content overflows; emit after the rows.
    /// For a sink-behind region this is the RAISED layer only — while sunk the
    /// bar was already emitted under the bg by [`Self::push_quads`].
    pub fn push_scrollbar_quads(&self, quads: &mut Vec<(f32, f32, f32, f32, [f32; 4])>) {
        if self.scrollbar_raised() {
            self.push_scrollbar_quads_always(quads);
        }
    }

    fn push_scrollbar_quads_always(&self, quads: &mut Vec<(f32, f32, f32, f32, [f32; 4])>) {
        if self.content_h > self.viewport_h {
            let (sb_x, track_y, sb_w, track_h, thumb_y, thumb_h) = self.scrollbar_geom();
            quads.push((sb_x, track_y, sb_w, track_h, crate::color::scrollbar_track_color()));
            quads.push((sb_x, thumb_y, sb_w, thumb_h, crate::color::scrollbar_thumb_color()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Coming to the fore is a fade, not a flip: the latch moves at once (so
    /// input never waits) while the drawn opacity ramps, in and back out.
    #[test]
    fn scrollbar_fade_ramps_instead_of_flipping() {
        let mut a = ScrollbarActivity::new();
        a.bump();
        // dt = SCROLL_FADE_SECS / 3, so one tick is a third of the way in.
        let dt = SCROLL_FADE_SECS / 3.0;
        a.tick(dt, true, false);
        assert!(a.raised(), "the latch flips immediately");
        assert!(a.fade() > 0.0 && a.fade() < 1.0, "part-way faded in, got {}", a.fade());
        for _ in 0..3 {
            a.tick(dt, true, false);
        }
        assert_eq!(a.fade(), 1.0, "fully in after the fade duration");

        // Let the post-scroll hold expire: the latch drops, then the fade
        // runs back out rather than vanishing with it.
        let ticks = (SCROLL_ACTIVE_HOLD / dt).ceil() as i32 + 1;
        for _ in 0..ticks {
            a.tick(dt, true, false);
        }
        assert!(!a.raised(), "hold expired");
        assert!(a.fade() < 1.0 && a.fade() >= 0.0, "fading out, got {}", a.fade());
        for _ in 0..4 {
            a.tick(dt, true, false);
        }
        assert_eq!(a.fade(), 0.0, "fully out");
    }

    fn region() -> ScrollRegion {
        // item_height clamps to list_font + 14, so pick one comfortably above any config.
        let mut r = ScrollRegion::new(40.0, 4.0);
        r.set_rect(10.0, 20.0, 200.0, 100.0);
        r
    }

    /// Run the glide out (a no-op with smoothing off in the test host's config).
    fn settle(r: &mut ScrollRegion) {
        let mut n = 0;
        while r.is_animating() && n < 1000 {
            r.tick(1.0 / 60.0);
            n += 1;
        }
    }

    #[test]
    fn wheel_scrolls_and_clamps() {
        let mut r = region();
        r.update_bounds(10, 20.0, 100.0); // content_h = 444 > 100
        assert!(r.wheel(&MouseScrollDelta::LineDelta(0.0, -2.0), 50.0, 50.0));
        settle(&mut r);
        assert_eq!(r.scroll_y, 48.0);
        assert!(!r.wheel(&MouseScrollDelta::LineDelta(0.0, -2.0), 500.0, 50.0)); // miss
        r.wheel(&MouseScrollDelta::LineDelta(0.0, -100.0), 50.0, 50.0);
        settle(&mut r);
        assert_eq!(r.scroll_y, 344.0); // clamped to max_scroll
    }

    #[test]
    fn virtualization_matches_list_math() {
        let mut r = region();
        r.update_bounds(10, 20.0, 100.0);
        r.set_scroll_y(0.0);
        // Row 0 at viewport_y + 0*(44) + 4 = 24; fits (24 + 40 <= 121).
        assert_eq!(r.get_item_draw_y(0, 4.0), Some(24.0));
        // Row 2 at 20 + 92 - 0 = 112: extends past the viewport bottom (121) but
        // still intersects it — returned so the caller draws it cut by the clip.
        assert_eq!(r.get_item_draw_y(2, 4.0), Some(112.0));
        // Row 3 at 20 + 136 = 156: fully below the viewport → culled.
        assert!(r.get_item_draw_y(3, 4.0).is_none());
    }

    #[test]
    fn virtualization_keeps_partial_row_at_top() {
        let mut r = region();
        r.update_bounds(10, 20.0, 100.0);
        // Scrolled so row 0 (virtual 4..44) is half above the viewport top:
        // draw_y = 20 + 4 - 24 = 0 < viewport_y, but its bottom (40) intersects.
        r.set_scroll_y(24.0);
        assert_eq!(r.get_item_draw_y(0, 4.0), Some(0.0));
        // A row whose bottom ends above the viewport top would be culled; with
        // this geometry row 0 always intersects, so scroll far and check row 0.
        r.set_scroll_y(80.0);
        assert!(r.get_item_draw_y(0, 4.0).is_none());
    }

    #[test]
    fn raw_shim_shares_the_intersection_contract() {
        let mut r = region();
        r.update_bounds_raw(444.0, 20.0, 100.0);
        r.set_scroll_y(50.0);
        // draw_y = 20 + 10 - 50 = -20; bottom = 4 < 19 → fully above, culled.
        assert!(r.get_draw_y(10.0, 24.0).is_none());
        // draw_y = 20 + 40 - 50 = 10: straddles the top edge → returned.
        assert_eq!(r.get_draw_y(40.0, 24.0), Some(10.0));
        // draw_y = 20 + 60 - 50 = 30: fully inside.
        assert_eq!(r.get_draw_y(60.0, 24.0), Some(30.0));
    }

    #[test]
    fn press_focuses_and_grabs_only_scrollbar() {
        let mut r = region();
        r.update_bounds(10, 20.0, 100.0);
        // Press in the rows area: focused, not dragging, falls through.
        assert!(!r.press(50.0, 50.0));
        assert!(r.focused && !r.dragging);
        // Press on the scrollbar strip (x + w - sb_w - 4 ± 4): consumed.
        let sb_x = 10.0 + 200.0 - crate::layout::scrollbar_width() - 4.0;
        assert!(r.press(sb_x + 1.0, 50.0));
        assert!(r.dragging);
        assert!(r.release());
        // Press outside: unfocuses.
        assert!(!r.press(500.0, 500.0));
        assert!(!r.focused);
    }

    #[test]
    fn horizontal_scroll_is_opt_in_and_clamps() {
        let mut r = region();
        r.update_bounds(10, 20.0, 100.0);
        // No content width declared: x wheel deltas change nothing and the
        // vertical-only behavior (including the y component) is untouched.
        assert!(!r.wheel(&MouseScrollDelta::LineDelta(-2.0, 0.0), 50.0, 50.0));
        assert_eq!(r.scroll_x, 0.0);
        assert!(!r.h_scroll_active());

        // Content wider than the 200px box: x deltas pan and clamp.
        r.set_content_w(500.0);
        assert!(r.h_scroll_active());
        assert!(r.wheel(&MouseScrollDelta::LineDelta(-2.0, 0.0), 50.0, 50.0));
        settle(&mut r);
        assert_eq!(r.scroll_x, 48.0);
        r.wheel(&MouseScrollDelta::LineDelta(-100.0, 0.0), 50.0, 50.0);
        settle(&mut r);
        assert_eq!(r.scroll_x, 300.0); // max = 500 - 200
        r.wheel(&MouseScrollDelta::LineDelta(100.0, 0.0), 50.0, 50.0);
        settle(&mut r);
        assert_eq!(r.scroll_x, 0.0);
    }

    #[test]
    fn h_thumb_press_grabs_and_releases() {
        let mut r = region();
        r.update_bounds(2, 20.0, 100.0); // no vertical overflow
        r.set_content_w(500.0);
        // The bottom strip: y + h - sb_w - 4, thumb starts at track_x.
        let sb_y = 20.0 + 100.0 - crate::layout::scrollbar_width() - 4.0;
        assert!(r.press(20.0, sb_y + 1.0));
        // Drag right: scroll_x follows.
        assert!(r.cursor_moved(120.0, sb_y + 1.0));
        assert!(r.scroll_x > 0.0);
        assert!(r.release());
        // A rows-area press still falls through (no h-bar hit).
        assert!(!r.press(50.0, 50.0));
    }

    #[test]
    fn edge_inset_moves_the_bar_off_the_edge() {
        let mut r = region().with_edge_inset(20.0);
        r.update_bounds(10, 20.0, 100.0);
        // Bar right edge sits edge_inset in from the region's right edge; the
        // old 4px position no longer hits.
        let sb_x = 10.0 + 200.0 - crate::layout::scrollbar_width() - 20.0;
        assert!(r.press(sb_x + 1.0, 50.0));
        assert!(r.release());
        let old_x = 10.0 + 200.0 - crate::layout::scrollbar_width() - 4.0 + 1.0;
        assert!(!r.press(old_x + 4.1, 50.0)); // past the ±4 slop of the inset bar
    }

    #[test]
    fn sink_behind_gates_input_until_a_scroll_raises() {
        let mut r = region().with_sink_behind(true);
        r.update_bounds(10, 20.0, 100.0);
        assert!(!r.scrollbar_raised());
        // Sunk: a press on the bar strip falls through (the plate occludes it).
        let sb_x = 10.0 + 200.0 - crate::layout::scrollbar_width() - 4.0;
        assert!(!r.press(sb_x + 1.0, 50.0));
        assert!(!r.dragging);
        // A wheel scroll raises it in the same frame…
        assert!(r.wheel(&MouseScrollDelta::LineDelta(0.0, -2.0), 50.0, 50.0));
        assert!(r.scrollbar_raised());
        // …and now the bar takes the grab.
        assert!(r.press(sb_x + 1.0, 50.0));
        assert!(r.dragging);
        assert!(r.release());
        // The release refreshed the hold: still raised, and the hold keeps the
        // repaint signal up so the frame loop keeps ticking toward the sink.
        assert!(r.scrollbar_raised());
        assert!(r.tick(0.3)); // holding → keep frames coming
        assert!(r.scrollbar_raised());
        assert!(r.tick(SCROLL_ACTIVE_HOLD)); // hold lapses → sink flip reported
        assert!(!r.scrollbar_raised());
        assert!(!r.tick(0.016)); // settled sunk: quiet again
    }

    #[test]
    fn hover_sustains_but_never_raises() {
        let mut r = region().with_sink_behind(true);
        r.update_bounds(10, 20.0, 100.0);
        let sb_x = 10.0 + 200.0 - crate::layout::scrollbar_width() - 4.0;
        // Hovering the sunk bar's strip does not raise it.
        r.cursor_moved(sb_x + 1.0, 50.0);
        assert!(!r.tick(0.016));
        assert!(!r.scrollbar_raised());
        // Raise by scrolling (and let the glide land, so the ticks below
        // measure only the raise/sink state), hover it, and let the hold
        // lapse: hover sustains.
        r.wheel(&MouseScrollDelta::LineDelta(0.0, -1.0), 50.0, 50.0);
        settle(&mut r);
        r.cursor_moved(sb_x + 1.0, 50.0);
        r.tick(SCROLL_ACTIVE_HOLD + 0.1); // hold lapses, hover keeps it raised
        assert!(r.scrollbar_raised());
        assert!(!r.tick(0.016)); // sustained by hover alone: no repaint churn
        // Pointer leaves: the next tick sinks it.
        r.cursor_moved(50.0, 50.0);
        assert!(r.tick(0.016));
        assert!(!r.scrollbar_raised());
    }

    #[test]
    fn tuple_emission_layers_by_raised_state() {
        let mut r = region().with_sink_behind(true);
        r.update_bounds(10, 20.0, 100.0);
        // Sunk: bar quads come UNDER the bg (push_quads emits bar then bg, the
        // raised-layer call is quiet).
        let mut under = Vec::new();
        r.push_quads(&mut under);
        assert_eq!(under.len(), 3); // track + thumb + bg
        assert_eq!(under[2].2, 200.0); // last quad is the full-width bg fill
        let mut over = Vec::new();
        r.push_scrollbar_quads(&mut over);
        assert!(over.is_empty());
        // Raised: bg alone below, bar above.
        r.wheel(&MouseScrollDelta::LineDelta(0.0, -1.0), 50.0, 50.0);
        let mut under = Vec::new();
        r.push_quads(&mut under);
        assert_eq!(under.len(), 1);
        let mut over = Vec::new();
        r.push_scrollbar_quads(&mut over);
        assert_eq!(over.len(), 2);
        // A non-sink region keeps the legacy shape: bg alone, bar always.
        let mut plain = region();
        plain.update_bounds(10, 20.0, 100.0);
        let (mut under, mut over) = (Vec::new(), Vec::new());
        plain.push_quads(&mut under);
        plain.push_scrollbar_quads(&mut over);
        assert_eq!((under.len(), over.len()), (1, 2));
    }

    #[test]
    fn non_sink_regions_are_unchanged() {
        let mut r = region();
        r.update_bounds(10, 20.0, 100.0);
        assert!(r.scrollbar_raised()); // always interactive
        assert!(!r.tick(1.0)); // tick is a no-op
        let sb_x = 10.0 + 200.0 - crate::layout::scrollbar_width() - 4.0;
        assert!(r.press(sb_x + 1.0, 50.0));
    }

    #[test]
    fn keyboard_is_hover_or_focus_scoped() {
        let mut r = region();
        r.update_bounds(10, 20.0, 100.0);
        let down = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::ArrowDown),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
            alt: false,
        };
        assert!(!r.keyboard(&down)); // neither hovered nor focused
        r.cursor_moved(50.0, 50.0);
        assert!(r.hovered);
        assert!(r.keyboard(&down));
        settle(&mut r);
        assert_eq!(r.scroll_y, 24.0);
    }
}
