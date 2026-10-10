//! The scrollbars' geometry (vertical and horizontal, at the edge or on the centre line), their hit
//! tests, and raising them on a scroll.

use super::*;

impl ScrollRegion {
    /// Scrollbar geometry (`ScrollBox::extra_quads`): (sb_x, track_y, sb_w, track_h, thumb_y, thumb_h).
    /// Centred on the region's width for a [`Self::sink_behind`] region, at the
    /// right edge (less [`Self::edge_inset`]) otherwise.
    pub(super) fn scrollbar_geom(&self) -> (f32, f32, f32, f32, f32, f32) {
        let (sb_x, sb_w) = if self.sink_behind {
            let sb_w = crate::layout::centred_scrollbar_width();
            (self.x + (self.w - sb_w) * 0.5, sb_w)
        } else {
            let sb_w = crate::layout::scrollbar_width();
            (self.x + self.w - sb_w - self.edge_inset, sb_w)
        };
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
    /// axes swapped: (track_x, sb_y, track_w, sb_h, thumb_x, thumb_w). An edge
    /// track stops short of the vertical bar's strip so the pills never
    /// overlap in the corner; a centred one runs the full width across the
    /// viewport's middle and crosses the vertical bar there.
    pub(super) fn h_scrollbar_geom(&self) -> (f32, f32, f32, f32, f32, f32) {
        let (sb_y, sb_h, right_reserve) = if self.sink_behind {
            let sb_h = crate::layout::centred_scrollbar_width();
            (self.viewport_y + (self.viewport_h - sb_h) * 0.5, sb_h, 0.0)
        } else {
            let sb_h = crate::layout::scrollbar_width();
            let reserve = if self.content_h > self.viewport_h { sb_h + 8.0 } else { 0.0 };
            (self.y + self.h - sb_h - 4.0, sb_h, reserve)
        };
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

    pub(super) fn hit_h_scrollbar(&self, px: f32, py: f32) -> bool {
        if !self.h_scroll_active() {
            return false;
        }
        if self.sink_behind && !self.activity.raised() {
            return false;
        }
        let (track_x, sb_y, track_w, sb_h, _, _) = self.h_scrollbar_geom();
        py >= sb_y - 4.0 && py <= sb_y + sb_h + 4.0 && px >= track_x && px <= track_x + track_w
    }

    /// Whether the bar overflows in either axis — the raise/sink "visible" input.
    pub(super) fn overflowing(&self) -> bool {
        self.content_h > self.viewport_h || self.h_scroll_active()
    }

    /// Refresh the raise hold and recompute immediately, so a scroll shows the
    /// bar in the same frame's redraw rather than one tick later.
    pub(super) fn raise(&mut self) {
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
}
