//! The shared app-side scroll region — scroll state, virtualization math,
//! scrollbar geometry/input, and frame/scrollbar emission for a list whose ROWS
//! the app draws itself.
//!
//! It is the one place the virtualization CONTRACT lives, shared by every app that draws its
//! own rows (cce-system-interface, cce-fonts, cce-mail, cce-cloud, …):
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
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the region: construction, rect and bounds, scroll position, the virtualization maths |
//! | `activity` | `ScrollbarActivity`, the raise/sink hysteresis of a bar behind a plate, and its timings |
//! | `scrollbar` | the bars' geometry and hit tests, and raising them |
//! | `input` | presses, drags, the pointer, the wheel, keys, and the tick |
//! | `emit` | drawing the frame and the bars: prims on a `RenderTarget`, or flat quads |

mod activity;
mod emit;
mod input;
mod scrollbar;
#[cfg(test)]
mod tests;

pub use activity::*;

use crate::widget::scroll_motion::{scroll_settings, Bounds, ScrollMotion, LINE_PX};
use crate::widget::{ElementState, Key, KeyEvent, MouseScrollDelta, NamedKey};

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
    /// The default 4.0 hugs a framed list's border. Edge bars only: a
    /// [`Self::sink_behind`] region's bars ride the centre lines and ignore it.
    pub edge_inset: f32,
    /// Opt-in raise/sink behavior ([`ScrollbarActivity`]): the bar idles sunk
    /// (host draws it behind its plate via [`Self::push_scrollbar_prims`]) and
    /// is non-interactive until a scroll raises it. Off (the default), the bar
    /// is always drawn and always grabbable at the right/bottom edge.
    ///
    /// **A sink-behind bar rides the CENTRE line of what it scrolls** (since
    /// 2026-10-06): the vertical bar down the region's middle, the horizontal
    /// one across the viewport's, crossing there when both scroll — over the
    /// rows, reserving no lane, [`crate::layout::centred_scrollbar_width`]
    /// thick. It is the DE's one scrollbar design (the params pane, the
    /// spreadsheet, the designer's dialog): sunk it is behind the plate and a
    /// press on its lane is a press on the row under it, so the middle of
    /// the list costs nothing until a scroll raises the bar there.
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
}
