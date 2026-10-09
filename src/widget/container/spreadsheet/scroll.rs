//! The scroll geometry, vertical and horizontal, and the scrollbars: where they ride, whether the
//! pointer is over one, whether they are raised, painting them, and moving the scroll by a thumb.

use super::*;

impl Spreadsheet {
    pub(super) fn geom(&self, rect: Rect) -> Option<ScrollGeom> {
        let content_h = self.row_count as f32 * ROW_H;
        let visible_h = (rect.height - HEADER_H).max(0.0);
        if visible_h <= 0.0 || content_h <= visible_h {
            return None;
        }
        let max_scroll = content_h - visible_h;
        let scroll = self.scroll_y.clamp(0.0, max_scroll);
        let bar_w = Self::bar_w();
        let track_y = rect.y + HEADER_H + TRACK_INSET;
        let track_h = (visible_h - 2.0 * TRACK_INSET).max(0.0);
        let thumb_h = Self::thumb_len(track_h, visible_h / content_h);
        let track_range = track_h - thumb_h;
        Some(ScrollGeom {
            visible_h,
            max_scroll,
            scroll,
            bar_x: rect.x + (rect.width - bar_w) * 0.5,
            bar_w,
            track_y,
            track_h,
            thumb_h,
            thumb_y: track_y + (scroll / max_scroll) * track_range,
            track_range,
        })
    }

    /// Horizontal counterpart of [`geom`]: present only when the column run is
    /// wider than the pane.
    pub(super) fn hgeom(&self, rect: Rect) -> Option<HScrollGeom> {
        let content_w = self.col_edges().last().copied().unwrap_or(0.0);
        let visible_w = rect.width;
        if visible_w <= 0.0 || content_w <= visible_w {
            return None;
        }
        let max_scroll = content_w - visible_w;
        let scroll = self.scroll_x.clamp(0.0, max_scroll);
        let bar_h = Self::bar_w();
        let track_x = rect.x + TRACK_INSET;
        let track_w = (visible_w - 2.0 * TRACK_INSET).max(0.0);
        let thumb_w = Self::thumb_len(track_w, visible_w / content_w);
        let track_range = track_w - thumb_w;
        let body_h = (rect.height - HEADER_H).max(0.0);
        Some(HScrollGeom {
            max_scroll,
            scroll,
            track_x,
            track_w,
            bar_y: rect.y + HEADER_H + (body_h - bar_h) * 0.5,
            bar_h,
            thumb_w,
            thumb_x: track_x + (scroll / max_scroll) * track_range,
            track_range,
        })
    }

    /// A bar's thickness — the DE's centred width, as the params pane's bar
    /// is, since it sits over cells rather than in a lane.
    pub(super) fn bar_w() -> f32 {
        crate::layout::centred_scrollbar_width()
    }

    /// A thumb's length on a track `track` long showing `ratio` of the content.
    pub(super) fn thumb_len(track: f32, ratio: f32) -> f32 {
        if track <= 20.0 {
            track
        } else {
            (track * ratio).clamp(20.0, track)
        }
    }

    /// Whether `(px, py)` is on the vertical bar's strip (with slop).
    pub(super) fn over_vbar(&self, px: f32, py: f32, rect: Rect) -> bool {
        self.geom(rect).is_some_and(|g| {
            px >= g.bar_x - BAR_SLOP
                && px <= g.bar_x + g.bar_w + BAR_SLOP
                && py >= g.track_y
                && py <= g.track_y + g.track_h
        })
    }

    /// Whether `(px, py)` is on the horizontal bar's strip (with slop).
    pub(super) fn over_hbar(&self, px: f32, py: f32, rect: Rect) -> bool {
        self.hgeom(rect).is_some_and(|g| {
            py >= g.bar_y - BAR_SLOP
                && py <= g.bar_y + g.bar_h + BAR_SLOP
                && px >= g.track_x
                && px <= g.track_x + g.track_w
        })
    }

    /// Whether the pane has either bar at all.
    pub fn scrollbars_shown(&self, rect: Rect) -> bool {
        self.geom(rect).is_some() || self.hgeom(rect).is_some()
    }

    /// Whether the bars are raised in front of the plate — the latch, which
    /// gates input. Paint the fore copy with [`Self::scrollbar_fade`].
    pub fn scrollbars_raised(&self) -> bool {
        self.activity.raised()
    }

    /// How far the fore copy has faded in, 0..=1.
    pub fn scrollbar_fade(&self) -> f32 {
        self.activity.fade()
    }

    /// The cross of pill scrollbars on `rect`, scaled to `alpha`: both
    /// tracks, then both thumbs, so neither track covers the other's thumb
    /// where they cross. [`Paint::paint`] draws the fore copy; the host draws
    /// the copy that idles behind its plate, at full alpha, BEFORE the plate.
    pub fn paint_scrollbars(&self, rect: Rect, ctx: &mut PaintCtx, alpha: f32) {
        let a = alpha.clamp(0.0, 1.0);
        if a <= 0.001 {
            return;
        }
        let dim = |mut c: [f32; 4]| {
            c[3] *= a;
            c
        };
        let all = (true, true, true, true);
        let track = dim(crate::color::scrollbar_track_color());
        let thumb = dim(crate::color::scrollbar_thumb_color());
        let pill = |ctx: &mut PaintCtx, r: Rect, c: [f32; 4]| {
            if r.width > 0.0 && r.height > 0.0 {
                ctx.rounded_rect(r, r.width.min(r.height) * 0.5, all, c);
            }
        };
        let v = self.geom(rect);
        let h = self.hgeom(rect);
        if let Some(g) = &v {
            pill(ctx, Rect { x: g.bar_x, y: g.track_y, width: g.bar_w, height: g.track_h }, track);
        }
        if let Some(g) = &h {
            pill(ctx, Rect { x: g.track_x, y: g.bar_y, width: g.track_w, height: g.bar_h }, track);
        }
        if let Some(g) = &v {
            pill(ctx, Rect { x: g.bar_x, y: g.thumb_y, width: g.bar_w, height: g.thumb_h }, thumb);
        }
        if let Some(g) = &h {
            pill(ctx, Rect { x: g.thumb_x, y: g.bar_y, width: g.thumb_w, height: g.bar_h }, thumb);
        }
    }

    /// Re-latch the raise/sink, returning whether it flipped.
    pub(super) fn recompute_bars(&mut self, rect: Rect) -> bool {
        let shown = self.scrollbars_shown(rect);
        let dragging = self.dragging_scrollbar || self.dragging_hscrollbar;
        self.activity.recompute(shown, dragging)
    }

    /// The clamped horizontal scroll — 0 while everything fits.
    pub(super) fn hscroll(&self, rect: Rect) -> f32 {
        self.hgeom(rect).map_or(0.0, |g| g.scroll)
    }

    /// Scroll to where a thumb dragged to `thumb_x` puts the columns.
    pub(super) fn hscroll_to_thumb(&mut self, g: &HScrollGeom, thumb_x: f32) {
        let ratio = if g.track_range > 0.0 {
            ((thumb_x - g.track_x) / g.track_range).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.scroll_x = ratio * g.max_scroll;
    }

    /// Scroll to where a thumb dragged to `thumb_y` puts the content.
    pub(super) fn scroll_to_thumb(&mut self, g: &ScrollGeom, thumb_y: f32) {
        let ratio = if g.track_range > 0.0 {
            ((thumb_y - g.track_y) / g.track_range).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.scroll_y = ratio * g.max_scroll;
    }
}
