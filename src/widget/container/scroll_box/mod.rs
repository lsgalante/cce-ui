//! `ScrollBox`: scroll maths and scrollbar chrome for a host to embed — a plain struct, never
//! registered as a widget. Its hosts (the tree list, and apps' own lists in cce-files, cce-fonts,
//! cce-gallery and cce-system-interface) drive it through these calls: the bounds and the
//! virtualization, the scrollbar (at the edge, or riding the centre line and sinking behind the
//! host's plate with `sink_behind`), presses and thumb drags, the pointer, the wheel, keys and
//! the tick.
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the struct, its rect and bounds, the bar's geometry and raise state, the virtualization |
//! | `input` | presses and thumb drags, the pointer, the wheel, keys, the tick |
//! | `paint` | the relief scrollbar (also the text box's), the centred pills, the flat quads |

mod input;
mod paint;
#[cfg(test)]
mod tests;

pub use paint::paint_relief_scrollbar;

use crate::widget::*;

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

    pub fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.base.x = x;
        self.base.y = y;
        self.base.w = w;
        self.base.h = h;
        self.viewport_y = y + self.viewport_offset_y;
        self.viewport_h = h + self.viewport_offset_h;
    }
}

unsafe impl Send for ScrollBox {}

unsafe impl Sync for ScrollBox {}
