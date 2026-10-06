//! Narrow-trait `Spreadsheet` (Phase 5l) — a read-only table with a header row, zebra rows,
//! selectable rows (a press selects one, ctrl toggles one, shift extends from the last
//! pressed — see [`Spreadsheet::press_row`]),
//! column dividers, and an inertially-scrolled body: wheel input feeds a velocity that
//! [`Input::tick`] integrates and decays each frame, the scrollbar thumb is host-drag-driven
//! through the drag surface, and arrow/page/home/end keys jump the scroll. The scroll geometry
//! (content/viewport heights, thumb position) is derived in one place ([`ScrollGeom`]) — legacy
//! re-derived it in seven. [`SpreadsheetController`] rides the `Input` capability hooks.
//!
//! **The two scrollbars cross at the body's centre** (since 2026-10-06): the vertical bar
//! rides the pane's vertical centre line and the horizontal one the body's horizontal centre
//! line, as the params pane's and the designer dialog's bars ride theirs — over the cells,
//! reserving no lane. They idle BEHIND the pane's plate ([`ScrollbarActivity`]): a scroll,
//! a key or a thumb drag raises them in front, a pointer over a raised bar keeps it there,
//! and once nothing holds them for the hold window they sink again. Sunk they take no input,
//! so a press on their lane reaches the row beneath. The fore copy is painted by
//! [`Paint::paint`] at the activity's fade; the copy behind the plate is the HOST's to
//! draw, before the plate, through [`Spreadsheet::paint_scrollbars`] — the plate is the
//! host's too.
//!
//! The `PARAM_BG` background is NOT emitted here: the designer's render path draws every
//! widget's background itself from `color()` + `corner_style()` (`push_widget_vertices`), and
//! `PARAM_BG` is translucent — emitting it again would double-blend. This widget's own
//! geometry starts at the header strip, exactly like the legacy `extra_quads`.

use crate::colors;
use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::scroll_motion::{scroll_settings, Bounds, ScrollMotion};
use crate::widget::{
    Adapted, ElementState, Event, EventCtx, Input, Key, Layout, MouseButton, MouseScrollDelta,
    NamedKey, Paint, ScrollbarActivity, SpreadsheetController,
};

const HEADER_H: f32 = 24.0;
const ROW_H: f32 = 24.0;
/// How far a track stops short of each end of what it spans, as every toolkit bar's does.
const TRACK_INSET: f32 = 4.0;
/// How far either side of a bar a press or the pointer still counts as on it.
const BAR_SLOP: f32 = 4.0;
/// Columns never squeeze below this: when they don't fit, the pane scrolls
/// horizontally instead. Sized for a 4-decimal value / a short header plus its
/// sort mark in the 12px mono label font, with the cell padding on both sides.
const MIN_COL_W: f32 = 76.0;

pub struct Spreadsheet {
    hovered: bool,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    /// Display order into `rows` — the identity permutation unless `sort` is set.
    /// Rebuilt by `apply_sort` at both mutation sites (header click, data refresh),
    /// so `order.len() == rows.len()` always holds.
    order: Vec<usize>,
    /// Active sort: `(column, ascending)`. A header click cycles
    /// ascending → descending → natural order (matching the source data).
    sort: Option<(usize, bool)>,
    /// Header cell the pointer is over — hover tint, like the Processes page's
    /// sortable headers.
    header_hover_col: Option<usize>,
    scroll_y: f32,
    dragging_scrollbar: bool,
    drag_offset_y: f32,
    scroll_x: f32,
    /// Smooth-scroll driver behind `scroll_y`/`scroll_x`: wheel notches
    /// glide, trackpad flicks coast (the widget's former velocity model,
    /// now the toolkit-wide one).
    motion: ScrollMotion,
    dragging_hscrollbar: bool,
    drag_offset_x: f32,
    /// Whether the two bars are raised in front of the plate or sunk behind
    /// it — one latch for both, since they are one cross.
    activity: ScrollbarActivity,
    /// The selected rows, as indices into `rows` — the SOURCE order, so a
    /// sort moves a selected row's place in the pane and not what it names.
    selected: std::collections::BTreeSet<usize>,
    /// The row a shift press extends from: the last one pressed without it.
    anchor: Option<usize>,
    /// Set by whatever changed `selected`, taken by the host.
    selection_changed: bool,
    /// The modifiers the host pushed in ahead of the press.
    ctrl: bool,
    shift: bool,
}

/// Scroll/scrollbar geometry for one (row count, rect) pair. Present only when the content
/// overflows the viewport — every scroll behavior is gated on that, as in legacy.
struct ScrollGeom {
    visible_h: f32,
    max_scroll: f32,
    /// `scroll_y` clamped to the current bounds (data changes can leave the raw value stale).
    scroll: f32,
    /// The bar's left edge: it is centred on the pane's width.
    bar_x: f32,
    bar_w: f32,
    track_y: f32,
    track_h: f32,
    thumb_h: f32,
    thumb_y: f32,
    track_range: f32,
}

/// [`ScrollGeom`]'s horizontal twin, for the bottom scrollbar. Present only
/// when the column run overflows the pane width.
struct HScrollGeom {
    max_scroll: f32,
    /// `scroll_x` clamped to the current bounds.
    scroll: f32,
    track_x: f32,
    track_w: f32,
    /// The bar's top edge: it is centred on the body's height.
    bar_y: f32,
    bar_h: f32,
    thumb_w: f32,
    thumb_x: f32,
    track_range: f32,
}

impl Spreadsheet {
    pub fn new() -> Adapted<Spreadsheet> {
        let mut s = Adapted::new(Spreadsheet {
            hovered: false,
            headers: Vec::new(),
            rows: Vec::new(),
            order: Vec::new(),
            sort: None,
            header_hover_col: None,
            scroll_y: 0.0,
            dragging_scrollbar: false,
            drag_offset_y: 0.0,
            scroll_x: 0.0,
            motion: ScrollMotion::new(),
            dragging_hscrollbar: false,
            drag_offset_x: 0.0,
            activity: ScrollbarActivity::new(),
            selected: std::collections::BTreeSet::new(),
            anchor: None,
            selection_changed: false,
            ctrl: false,
            shift: false,
        });
        // The spreadsheet pane starts hidden (the designer toggles it in later).
        crate::widget::WidgetHost::set_visible(&mut s, false);
        s
    }

    fn geom(&self, rect: Rect) -> Option<ScrollGeom> {
        let content_h = self.rows.len() as f32 * ROW_H;
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

    /// One column's width: an even share of the pane, floored at [`MIN_COL_W`] —
    /// past the floor the content overflows into the horizontal scroll.
    fn col_w(&self, rect: Rect) -> f32 {
        let n = self.headers.len().max(1) as f32;
        (rect.width / n).max(MIN_COL_W)
    }

    /// Horizontal counterpart of [`geom`]: present only when the column run is
    /// wider than the pane.
    fn hgeom(&self, rect: Rect) -> Option<HScrollGeom> {
        let content_w = self.col_w(rect) * self.headers.len() as f32;
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

    /// A bar's thickness — the DE `scrollbar_width` widened, as the params
    /// pane's bar is, since it sits over cells rather than in a lane.
    fn bar_w() -> f32 {
        crate::layout::scrollbar_width() * 1.6
    }

    /// A thumb's length on a track `track` long showing `ratio` of the content.
    fn thumb_len(track: f32, ratio: f32) -> f32 {
        if track <= 20.0 {
            track
        } else {
            (track * ratio).clamp(20.0, track)
        }
    }

    /// Whether `(px, py)` is on the vertical bar's strip (with slop).
    fn over_vbar(&self, px: f32, py: f32, rect: Rect) -> bool {
        self.geom(rect).is_some_and(|g| {
            px >= g.bar_x - BAR_SLOP
                && px <= g.bar_x + g.bar_w + BAR_SLOP
                && py >= g.track_y
                && py <= g.track_y + g.track_h
        })
    }

    /// Whether `(px, py)` is on the horizontal bar's strip (with slop).
    fn over_hbar(&self, px: f32, py: f32, rect: Rect) -> bool {
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
    fn recompute_bars(&mut self, rect: Rect) -> bool {
        let shown = self.scrollbars_shown(rect);
        let dragging = self.dragging_scrollbar || self.dragging_hscrollbar;
        self.activity.recompute(shown, dragging)
    }

    /// The clamped horizontal scroll — 0 while everything fits.
    fn hscroll(&self, rect: Rect) -> f32 {
        self.hgeom(rect).map_or(0.0, |g| g.scroll)
    }

    /// The header column under `(px, py)`, if the point is inside the header band.
    fn header_col_at(&self, px: f32, py: f32, rect: Rect) -> Option<usize> {
        if self.headers.is_empty() || rect.width <= 0.0 {
            return None;
        }
        if px < rect.x || px > rect.x + rect.width || py < rect.y || py >= rect.y + HEADER_H {
            return None;
        }
        let n = self.headers.len();
        let col = ((px - rect.x + self.hscroll(rect)) / self.col_w(rect)) as usize;
        if col >= n {
            return None;
        }
        Some(col)
    }

    /// Scroll to where a thumb dragged to `thumb_x` puts the columns.
    fn hscroll_to_thumb(&mut self, g: &HScrollGeom, thumb_x: f32) {
        let ratio = if g.track_range > 0.0 {
            ((thumb_x - g.track_x) / g.track_range).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.scroll_x = ratio * g.max_scroll;
    }

    /// Cell comparison: numeric when both cells parse (so "10" sorts after "9"),
    /// lexicographic otherwise.
    fn cmp_cells(a: &str, b: &str) -> std::cmp::Ordering {
        match (a.parse::<f64>(), b.parse::<f64>()) {
            (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
            _ => a.cmp(b),
        }
    }

    /// Rebuild `order` from `sort`. Stable, so equal cells keep their source order.
    fn apply_sort(&mut self) {
        // A refresh can shrink the column set out from under the sort.
        if let Some((col, _)) = self.sort {
            if col >= self.headers.len() {
                self.sort = None;
            }
        }
        self.order = (0..self.rows.len()).collect();
        if let Some((col, ascending)) = self.sort {
            let rows = &self.rows;
            let cell = |r: usize| rows[r].get(col).map(String::as_str).unwrap_or("");
            self.order.sort_by(|&a, &b| {
                let ord = Self::cmp_cells(cell(a), cell(b));
                if ascending { ord } else { ord.reverse() }
            });
        }
    }

    /// What a body press at `(px, py)` lands on: `Some(Some(place))` for the
    /// row at that place in the DISPLAY order, `Some(None)` for the empty
    /// body under the last row, `None` for a press that is not the body's —
    /// outside it, or on a RAISED scrollbar, which the drag surface owns. A
    /// sunk bar is behind the plate and the press is the row's.
    fn body_row_at(&self, px: f32, py: f32, rect: Rect) -> Option<Option<usize>> {
        let body_top = rect.y + HEADER_H;
        if px < rect.x || px > rect.x + rect.width || py < body_top || py > rect.y + rect.height {
            return None;
        }
        if self.activity.raised() && (self.over_vbar(px, py, rect) || self.over_hbar(px, py, rect)) {
            return None;
        }
        let scroll = self.geom(rect).map_or(0.0, |g| g.scroll);
        let place = ((py - body_top + scroll) / ROW_H) as usize;
        Some((place < self.order.len()).then_some(place))
    }

    /// A press on the row at `place` of the display order, or on the empty
    /// body (`None`). Plain, it selects that row alone — or nothing, where
    /// the row was the whole selection already or the press met no row.
    /// With ctrl it toggles the row and leaves the rest. With shift it
    /// selects the run from the anchor to the row AS DISPLAYED, so under a
    /// sort the run is what the eye sees between the two; with ctrl as well
    /// the run is added to what is selected.
    fn press_row(&mut self, place: Option<usize>, ctrl: bool, shift: bool) {
        let before = self.selected.clone();
        match place {
            None => {
                if !ctrl && !shift {
                    self.selected.clear();
                    self.anchor = None;
                }
            }
            Some(place) => {
                let src = self.order[place];
                let anchor_place = self.anchor.and_then(|a| self.order.iter().position(|&r| r == a));
                match (shift, anchor_place) {
                    (true, Some(from)) => {
                        if !ctrl {
                            self.selected.clear();
                        }
                        let (lo, hi) = (from.min(place), from.max(place));
                        self.selected.extend(self.order[lo..=hi].iter().copied());
                    }
                    _ if ctrl => {
                        if !self.selected.remove(&src) {
                            self.selected.insert(src);
                        }
                        self.anchor = Some(src);
                    }
                    _ => {
                        let alone = self.selected.len() == 1 && self.selected.contains(&src);
                        self.selected.clear();
                        if !alone {
                            self.selected.insert(src);
                        }
                        self.anchor = Some(src);
                    }
                }
            }
        }
        if self.selected != before {
            self.selection_changed = true;
        }
    }

    /// Scroll to where a thumb dragged to `thumb_y` puts the content.
    fn scroll_to_thumb(&mut self, g: &ScrollGeom, thumb_y: f32) {
        let ratio = if g.track_range > 0.0 {
            ((thumb_y - g.track_y) / g.track_range).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.scroll_y = ratio * g.max_scroll;
    }
}

impl Layout for Spreadsheet {}

impl Paint for Spreadsheet {
    /// The pane IS its own background plate, wearing the parameter plate's fill —
    /// same tint, opacity, and blur-behind marker (`param_plate_fill`) — so it
    /// bevels like the params plate and tracks a live retint / opacity / blur
    /// toggle with it.
    fn color(&self) -> [f32; 4] {
        colors::param_plate_fill()
    }

    /// The shared plate corner radius (rounded on all four corners when non-zero),
    /// matching the rounded clip hosts carve for the pane's content.
    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        let r = crate::layout::plate_corner_radius();
        let on = r > 0.0;
        Some((r, (on, on, on, on)))
    }

    /// The shared plate border — under `control_relief` the host promotes this to
    /// the plate bevel, like `ParametersBg`.
    fn solid_border(&self) -> Option<([f32; 4], f32)> {
        colors::plate_border_color().map(|bc| (bc, colors::plate_border_thickness()))
    }

    /// Subtree painter: `paint` authors the pane's complete text with
    /// per-column clamp bounds, so its Text prims must pass through
    /// `paint_self` verbatim — the own-labels re-derivation drops per-prim
    /// bounds, which is exactly how long cell values used to overlap into
    /// their neighbor columns on narrow panes.
    fn paints_own_subtree(&self) -> bool {
        true
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);
        let scroll = self.geom(rect).map_or(0.0, |g| g.scroll);

        // Header bg
        ctx.quad(Rect { x, y, width: w, height: HEADER_H }, [0.12, 0.12, 0.16, 0.4]);

        // Zebra rows + separators, clipped to the body band
        let body_top = y + HEADER_H;
        let body_bottom = y + h;
        let selected_fill = {
            let [r, g, b, _] = colors::highlight_primary_color();
            [r, g, b, 0.28]
        };
        for i in 0..self.rows.len() {
            let ry = y + HEADER_H + i as f32 * ROW_H - scroll;
            if ry + ROW_H <= body_top || ry >= body_bottom {
                continue;
            }
            let draw_y = ry.max(body_top);
            let draw_h = (ry + ROW_H).min(body_bottom) - draw_y;
            if draw_h > 0.0 {
                let row_color = if i % 2 == 0 {
                    [0.10, 0.10, 0.13, 0.15]
                } else {
                    [0.08, 0.08, 0.11, 0.05]
                };
                ctx.quad(Rect { x, y: draw_y, width: w, height: draw_h }, row_color);
                if self.order.get(i).is_some_and(|src| self.selected.contains(src)) {
                    ctx.quad(Rect { x, y: draw_y, width: w, height: draw_h }, selected_fill);
                }

                let sep_y = ry + ROW_H;
                if sep_y >= body_top && sep_y < body_bottom {
                    ctx.quad(Rect { x, y: sep_y, width: w, height: 1.0 }, [0.20, 0.20, 0.25, 0.15]);
                }
            }
        }

        // Header separator
        ctx.quad(Rect { x, y: y + HEADER_H, width: w, height: 1.0 }, [0.20, 0.20, 0.25, 0.25]);

        // Vertical column dividers, at scrolled column edges, kept inside the pane.
        let hscroll = self.hscroll(rect);
        if h > 0.0 && !self.headers.is_empty() {
            let n_cols = self.headers.len();
            let cw = self.col_w(rect);
            for i in 1..n_cols {
                let dx = x + cw * i as f32 - hscroll;
                if dx <= x || dx >= x + w {
                    continue;
                }
                ctx.quad(Rect { x: dx, y, width: 1.0, height: h }, [0.20, 0.20, 0.25, 0.15]);
            }
        }

        // The scrollbars' fore copy, over the cells, at the activity's fade —
        // the fade rather than the latch, so it draws all the way out. The
        // copy behind the plate is the host's (`paint_scrollbars`).
        self.paint_scrollbars(rect, ctx, self.activity.fade());

        // Header + cell text, each cell clamped to its column and the body band.
        if self.headers.is_empty() {
            return;
        }
        let n_cols = self.headers.len();
        let col_w = self.col_w(rect);
        // Scrolled column origin; columns fully outside the pane skip.
        let xoff = x - hscroll;
        let col_visible = |col: usize| -> bool {
            let cx0 = xoff + col_w * col as f32;
            cx0 + col_w > x && cx0 < x + w
        };
        if let Some(hc) = self.header_hover_col {
            // Subtle hover tint on the clickable header cell (the Processes-page
            // sortable-header convention), clamped to the pane.
            let hx0 = (xoff + col_w * hc as f32).max(x);
            let hx1 = (xoff + col_w * (hc + 1) as f32).min(x + w);
            if hx1 > hx0 {
                ctx.quad(Rect { x: hx0, y, width: hx1 - hx0, height: HEADER_H }, [1.0, 1.0, 1.0, 0.05]);
            }
        }
        // Every label clamps to its own column (a 4px gutter short of the
        // divider) AND to the pane, so a long value cuts off instead of
        // running under its neighbor, and half-scrolled edge columns stop at
        // the plate instead of bleeding past it.
        let col_bounds = |col: usize, top: f32, height: f32| -> Option<[f32; 4]> {
            let x0 = (xoff + col_w * col as f32).max(x);
            let x1 = (xoff + col_w * (col + 1) as f32 - 4.0).min(x + w);
            if x1 <= x0 {
                return None;
            }
            Some([x0, top, x1, top + height])
        };
        // Ellipsize what the clamp would cut, so truncation reads as
        // deliberate. A char budget from ONE cached measurement is exact
        // because the DE label font is monospace; the clamp bounds stay on
        // as the backstop for any fallback-font drift. Below three columns'
        // worth of budget the mark would REPLACE the content (a 19px column
        // fits one glyph — a bare "…" says less than a clipped digit), so
        // very narrow columns keep the raw string and let the clamp cut it.
        let fam = crate::layout::control_label_font();
        let char_w =
            (crate::widget::display::measure_text_width("0123456789", &fam, 12.0) / 10.0).max(1.0);
        let budget = ((col_w - 12.0) / char_w).floor() as usize;
        let fit = move |s: String| -> String {
            if budget < 3 || s.chars().count() <= budget {
                return s;
            }
            let mut out: String = s.chars().take(budget - 1).collect();
            out.push('\u{2026}');
            out
        };
        for (i, header) in self.headers.iter().enumerate() {
            if !col_visible(i) {
                continue;
            }
            let cx = xoff + col_w * i as f32 + 8.0;
            match self.sort {
                Some((col, ascending)) if col == i => {
                    // The sorted column: its name, then the `chevron-up` or
                    // `chevron-down` glyph a cell after it — the room of
                    // the " ▲" it was until 2026-10-05, taken out of the
                    // name's budget.
                    let b = budget.saturating_sub(2);
                    let shown: String = if b < 3 || header.chars().count() <= b {
                        header.clone()
                    } else {
                        let mut out: String = header.chars().take(b - 1).collect();
                        out.push('\u{2026}');
                        out
                    };
                    let tx = cx + shown.chars().count() as f32 * char_w + 0.5 * char_w;
                    ctx.text_with(shown, cx, y + 6.0, 12.0, [0xff, 0xff, 0xff], None, col_bounds(i, y, HEADER_H));
                    const SIDE: f32 = 8.0;
                    let r = Rect { x: tx, y: y + 0.5 * (HEADER_H - SIDE), width: SIDE, height: SIDE };
                    ctx.icon(if ascending { "chevron-up" } else { "chevron-down" }, r, [1.0, 1.0, 1.0, 1.0]);
                }
                _ => ctx.text_with(fit(header.clone()), cx, y + 6.0, 12.0, [0xdd, 0xdd, 0xee], None, col_bounds(i, y, HEADER_H)),
            }
        }
        // A row half scrolled under the header or off the bottom draws its
        // text CUT at the body band, as its zebra fill is. Until 2026-09-30
        // it drew no text until it was wholly inside, so a scrolling row's
        // band arrived empty and its values popped in a row's height late.
        for (i, &src) in self.order.iter().enumerate() {
            let ry = y + HEADER_H + i as f32 * ROW_H - scroll;
            let top = ry.max(body_top);
            let bottom = (ry + ROW_H).min(body_bottom);
            if bottom <= top {
                continue;
            }
            for (col_idx, val) in self.rows[src].iter().enumerate().take(n_cols) {
                if !col_visible(col_idx) {
                    continue;
                }
                let cx = xoff + col_w * col_idx as f32 + 8.0;
                ctx.text_with(fit(val.clone()), cx, ry + 6.0, 12.0, [0xbb, 0xbb, 0xcc], None, col_bounds(col_idx, top, bottom - top));
            }
        }
    }
}

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

impl SpreadsheetController for Spreadsheet {
    fn set_spreadsheet_data(&mut self, headers: Vec<String>, rows: Vec<Vec<String>>) {
        self.headers = headers;
        self.rows = rows;
        // Re-derive the display order so an active sort survives a data refresh
        // (the designer re-sets the whole table on selection/param changes).
        self.apply_sort();
        // The raw scroll may now exceed the new content; every consumer clamps through
        // `geom()`, and the next scroll write re-clamps it for real.
        //
        // The selection is of rows by index, and stands across a refresh:
        // the designer re-sets the table on every frame of a playback, and
        // the rows selected are still the elements they were. What a
        // shorter table no longer has goes.
        let n = self.rows.len();
        let kept = self.selected.len();
        self.selected.retain(|&r| r < n);
        if self.selected.len() != kept {
            self.selection_changed = true;
        }
        if self.anchor.is_some_and(|a| a >= n) {
            self.anchor = None;
        }
    }

    fn selected_rows(&self) -> Vec<usize> {
        self.selected.iter().copied().collect()
    }

    fn set_selected_rows(&mut self, rows: &[usize]) {
        let n = self.rows.len();
        let next: std::collections::BTreeSet<usize> = rows.iter().copied().filter(|&r| r < n).collect();
        if next != self.selected {
            self.selected = next;
            self.selection_changed = true;
        }
        if self.anchor.is_some_and(|a| !self.selected.contains(&a)) {
            self.anchor = None;
        }
    }

    fn take_selection_change(&mut self) -> bool {
        std::mem::take(&mut self.selection_changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::UiContext;
    use crate::widget::WidgetHost;

    fn filled(rows: usize) -> Adapted<Spreadsheet> {
        let mut s = Spreadsheet::new();
        s.set_visible(true);
        WidgetHost::set_rect(&mut s, 0.0, 0.0, 200.0, 124.0); // viewport: 100 = ~4 rows of 24
        let data: Vec<Vec<String>> =
            (0..rows).map(|i| vec![format!("r{i}"), format!("v{i}")]).collect();
        SpreadsheetController::set_spreadsheet_data(&mut *s, vec!["a".into(), "b".into()], data);
        s
    }

    fn wide(cols: usize) -> Adapted<Spreadsheet> {
        let mut s = Spreadsheet::new();
        s.set_visible(true);
        WidgetHost::set_rect(&mut s, 0.0, 0.0, 200.0, 124.0);
        let headers: Vec<String> = (0..cols).map(|i| format!("c{i}")).collect();
        let rows = vec![(0..cols).map(|i| format!("v{i}")).collect::<Vec<String>>(); 2];
        SpreadsheetController::set_spreadsheet_data(&mut *s, headers, rows);
        s
    }

    /// A row half scrolled out at either edge of the body draws its text cut
    /// at the band — it used to draw none until it was wholly inside.
    #[test]
    fn a_half_scrolled_row_draws_its_text_cut_at_the_body() {
        let rect = Rect { x: 0.0, y: 0.0, width: 200.0, height: 124.0 };
        let mut s = filled(20);
        (*s).scroll_y = ROW_H * 0.5;
        let mut pc = crate::scene::paint::PaintCtx::new();
        Paint::paint(&*s, rect, &mut pc);
        let items = pc.finish().items;
        let cell = |want: &str| {
            items.iter().find_map(|item| match &item.prim {
                crate::scene::paint::Prim::Text { text, bounds, .. } if text == want => *bounds,
                _ => None,
            })
        };
        let body_top = HEADER_H;
        let body_bottom = rect.height;
        // r0 spans -12..12 of the body; r4 runs 12 px past its bottom.
        let top = cell("r0").expect("the row under the header draws its text");
        assert_eq!((top[1], top[3]), (body_top, body_top + ROW_H * 0.5), "cut at the header");
        let bottom = cell("r4").expect("the row off the bottom draws its text");
        assert_eq!(bottom[3], body_bottom, "cut at the pane's bottom");
        assert!(bottom[1] < bottom[3]);
        assert!(cell("r5").is_none(), "a row wholly out of the body draws nothing");
    }

    /// Columns floor at MIN_COL_W instead of squeezing: past the floor the run
    /// overflows into the horizontal scroll, and within it there is none.
    #[test]
    fn columns_floor_at_min_width_and_overflow_scrolls() {
        let rect = Rect { x: 0.0, y: 0.0, width: 200.0, height: 124.0 };
        let s = wide(6);
        assert_eq!((*s).col_w(rect), MIN_COL_W);
        let g = (*s).hgeom(rect).expect("6 floored columns overflow a 200px pane");
        assert!((g.max_scroll - (6.0 * MIN_COL_W - 200.0)).abs() < 0.01);

        let fits = wide(2);
        assert!((*fits).hgeom(rect).is_none(), "2 columns share the pane, no h-scroll");
        assert_eq!((*fits).col_w(rect), 100.0, "fitting columns still split the width evenly");
    }

    /// The sort hit-test must look up columns through the scrolled origin, or
    /// clicking a header would sort the column that USED to be under the pointer.
    #[test]
    fn header_hit_test_tracks_horizontal_scroll() {
        let rect = Rect { x: 0.0, y: 0.0, width: 200.0, height: 124.0 };
        let mut s = wide(6);
        assert_eq!((*s).header_col_at(10.0, 5.0, rect), Some(0));
        (*s).scroll_x = MIN_COL_W;
        assert_eq!((*s).header_col_at(10.0, 5.0, rect), Some(1));
    }

    /// A horizontal wheel feeds hscroll velocity, tick integrates and decays it,
    /// and the scroll clamps inside the overflow.
    #[test]
    fn horizontal_wheel_integrates_and_decays_through_tick() {
        let mut ctx = UiContext::new();
        let mut s = wide(6);
        let (id, ptr) = (s.id(), s.as_ptr_mut());
        ctx.register_widget(id, ptr);

        let wheel = Event::MouseWheel {
            delta: MouseScrollDelta::LineDelta(-2.0, 0.0),
            x: 50.0,
            y: 60.0,
            local_x: 50.0,
            local_y: 60.0,
        };
        assert!(s.handle_event(&wheel, &mut ctx), "in-rect horizontal wheel consumed");
        assert!(WidgetHost::tick(&mut s, 0.016, &mut ctx), "first tick moves the h-scroll");
        let mut guard = 0;
        while WidgetHost::tick(&mut s, 0.016, &mut ctx) {
            guard += 1;
            assert!(guard < 1000, "h-inertia must decay to a stop");
        }
        let rect = Rect { x: 0.0, y: 0.0, width: 200.0, height: 124.0 };
        let max = (*s).hgeom(rect).unwrap().max_scroll;
        assert!((*s).scroll_x >= 0.0 && (*s).scroll_x <= max, "h-scroll stays clamped");
        assert!((*s).scroll_x > 0.0, "negative dx scrolled the columns (ScrollRegion sign convention)");

        // A pane whose columns fit ignores horizontal wheels.
        let mut fits = wide(2);
        let (fid, fptr) = (fits.id(), fits.as_ptr_mut());
        ctx.register_widget(fid, fptr);
        assert!(!fits.handle_event(&wheel, &mut ctx), "no overflow, wheel passes through");
    }

    #[test]
    fn wheel_velocity_integrates_and_decays_through_tick() {
        let mut ctx = UiContext::new();
        let mut s = filled(50);
        let (id, ptr) = (s.id(), s.as_ptr_mut());
        ctx.register_widget(id, ptr);

        // A wheel over the body feeds velocity (negated delta, the ScrollRegion
        // convention: a negative line delta scrolls the view down)…
        let wheel = Event::MouseWheel {
            delta: MouseScrollDelta::LineDelta(0.0, -2.0),
            x: 50.0,
            y: 60.0,
            local_x: 50.0,
            local_y: 60.0,
        };
        assert!(s.handle_event(&wheel, &mut ctx), "in-rect wheel consumed");

        // …which tick integrates into scroll movement and decays to a stop.
        assert!(WidgetHost::tick(&mut s, 0.016, &mut ctx), "first tick moves the scroll");
        let mut guard = 0;
        while WidgetHost::tick(&mut s, 0.016, &mut ctx) {
            guard += 1;
            assert!(guard < 1000, "inertia must decay to a stop");
        }

        // Hidden spreadsheets are not hittable, so the wheel passes through.
        s.set_visible(false);
        assert!(!s.handle_event(&wheel, &mut ctx), "hidden widget ignores wheel");
    }

    #[test]
    fn scrollbar_drag_and_keys_move_the_scroll() {
        let mut ctx = UiContext::new();
        let mut s = filled(50);
        let (id, ptr) = (s.id(), s.as_ptr_mut());
        ctx.register_widget(id, ptr);
        let rect = Rect { x: 0.0, y: 0.0, width: 200.0, height: 124.0 };

        // content 1200, viewport 100 -> overflowing, so the host may drag it.
        assert!(s.draggable());

        // Sunk behind the plate, the bar takes no press.
        s.drag_begin(100.0, 80.0);
        assert!(!s.is_dragging(), "a sunk bar is not grabbed");

        // Raised, a press on its track (the pane's centre line) jumps the
        // thumb and engages the drag.
        s.inner_mut().activity.bump();
        s.inner_mut().recompute_bars(rect);
        s.drag_begin(100.0, 80.0);
        assert!(s.is_dragging());
        assert!(s.drag_update(100.0, 90.0), "thumb drag scrolls");
        let dragged_to = s.inner().geom(rect).unwrap().scroll;
        assert!(dragged_to > 0.0);
        s.drag_end();
        assert!(!s.is_dragging());

        // A body press (left of the scrollbar) engages no drag.
        s.drag_begin(50.0, 60.0);
        assert!(!s.is_dragging(), "body press is not a scrollbar drag");

        // End key jumps to max; Home returns to zero. (Keys route via keyboard_input.)
        let end = crate::widget::KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::End),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
            alt: false,
        };
        assert!(s.keyboard_input(&end, &mut ctx));
        // The key glides: run the motion out before reading the offset.
        for _ in 0..1000 {
            if !Input::tick(&mut *s.inner_mut(), 1.0 / 60.0, rect) {
                break;
            }
        }
        let g = s.inner().geom(rect).unwrap();
        assert_eq!(g.scroll, g.max_scroll);

        // Hidden: the focused-widget keyboard path must not consume keys.
        s.set_visible(false);
        assert!(!s.keyboard_input(&end, &mut ctx), "hidden widget ignores keys");
    }

    fn header_click(x: f32) -> Event {
        Event::MouseButton {
            button: MouseButton::Left,
            state: ElementState::Pressed,
            x,
            y: 10.0,
            local_x: x,
            local_y: 10.0,
        }
    }

    #[test]
    fn header_click_cycles_ascending_descending_natural() {
        let mut ctx = UiContext::new();
        let mut s = Spreadsheet::new();
        s.set_visible(true);
        WidgetHost::set_rect(&mut s, 0.0, 0.0, 200.0, 124.0);
        let (id, ptr) = (s.id(), s.as_ptr_mut());
        ctx.register_widget(id, ptr);
        // Numeric strings out of lexicographic order: "10" must sort after "9".
        let rows = vec![
            vec!["10".to_string(), "b".to_string()],
            vec!["9".to_string(), "c".to_string()],
            vec!["2".to_string(), "a".to_string()],
        ];
        SpreadsheetController::set_spreadsheet_data(&mut *s, vec!["n".into(), "s".into()], rows);
        assert_eq!(s.inner().order, vec![0, 1, 2], "unsorted = natural order");

        // Column 0 spans x 0..100. Click 1: ascending, numeric.
        assert!(s.handle_event(&header_click(50.0), &mut ctx));
        assert_eq!(s.inner().sort, Some((0, true)));
        assert_eq!(s.inner().order, vec![2, 1, 0], "2 < 9 < 10 numerically");

        // Click 2: descending.
        assert!(s.handle_event(&header_click(50.0), &mut ctx));
        assert_eq!(s.inner().sort, Some((0, false)));
        assert_eq!(s.inner().order, vec![0, 1, 2]);

        // Click 3: back to natural order.
        assert!(s.handle_event(&header_click(50.0), &mut ctx));
        assert_eq!(s.inner().sort, None);
        assert_eq!(s.inner().order, vec![0, 1, 2]);

        // Column 1 (lexicographic), then a body click changes nothing.
        assert!(s.handle_event(&header_click(150.0), &mut ctx));
        assert_eq!(s.inner().order, vec![2, 0, 1], "a < b < c");
        let body = Event::MouseButton {
            button: MouseButton::Left,
            state: ElementState::Pressed,
            x: 50.0,
            y: 60.0,
            local_x: 50.0,
            local_y: 60.0,
        };
        s.handle_event(&body, &mut ctx);
        assert_eq!(s.inner().sort, Some((1, true)), "body press is not a sort");
    }

    fn body_click(y: f32) -> Event {
        Event::MouseButton {
            button: MouseButton::Left,
            state: ElementState::Pressed,
            x: 50.0,
            y,
            local_x: 50.0,
            local_y: y,
        }
    }

    /// A press selects a row, ctrl toggles one, shift extends from the last
    /// pressed; the rows are named by their place in the DATA, so a sort
    /// moves the highlight and not what is selected; and a press on the
    /// scrollbar is not a press on a row.
    #[test]
    fn rows_select_alone_toggled_and_in_runs() {
        let mut ctx = UiContext::new();
        let mut s = filled(50);
        let (id, ptr) = (s.id(), s.as_ptr_mut());
        ctx.register_widget(id, ptr);
        // Rows are 24 tall under a 24 header: row k spans 24 + 24k.
        let row_y = |k: usize| 24.0 + 24.0 * k as f32 + 12.0;

        assert!(s.handle_event(&body_click(row_y(1)), &mut ctx));
        assert_eq!(s.inner().selected_rows(), vec![1]);
        assert!(s.inner_mut().take_selection_change());
        assert!(!s.inner_mut().take_selection_change(), "taken once");

        // Plain press elsewhere replaces it.
        s.handle_event(&body_click(row_y(2)), &mut ctx);
        assert_eq!(s.inner().selected_rows(), vec![2]);

        // Ctrl adds and removes.
        WidgetHost::set_modifiers(&mut s, true, false, false);
        s.handle_event(&body_click(row_y(0)), &mut ctx);
        assert_eq!(s.inner().selected_rows(), vec![0, 2]);
        s.handle_event(&body_click(row_y(2)), &mut ctx);
        assert_eq!(s.inner().selected_rows(), vec![0]);

        // Shift runs from the last row pressed without it (row 2, the ctrl
        // press) to this one.
        WidgetHost::set_modifiers(&mut s, false, true, false);
        s.handle_event(&body_click(row_y(0)), &mut ctx);
        assert_eq!(s.inner().selected_rows(), vec![0, 1, 2]);

        // A plain press on the one selected row clears it.
        WidgetHost::set_modifiers(&mut s, false, false, false);
        s.handle_event(&body_click(row_y(3)), &mut ctx);
        s.handle_event(&body_click(row_y(3)), &mut ctx);
        assert!(s.inner().selected_rows().is_empty());

        // A RAISED scrollbar's lane is the drag surface's.
        let rect = Rect { x: 0.0, y: 0.0, width: 200.0, height: 124.0 };
        s.inner_mut().activity.bump();
        s.inner_mut().recompute_bars(rect);
        s.handle_event(&Event::MouseButton {
            button: MouseButton::Left,
            state: ElementState::Pressed,
            x: 100.0,
            y: row_y(1),
            local_x: 100.0,
            local_y: row_y(1),
        }, &mut ctx);
        assert!(s.inner().selected_rows().is_empty(), "a scrollbar press selects nothing");

        // Under a sort the row pressed is the row SHOWN there, and a shift
        // run is the rows shown between.
        let mut t = Spreadsheet::new();
        t.set_visible(true);
        WidgetHost::set_rect(&mut t, 0.0, 0.0, 200.0, 124.0);
        let (id, ptr) = (t.id(), t.as_ptr_mut());
        ctx.register_widget(id, ptr);
        let rows = vec![
            vec!["10".to_string(), "b".to_string()],
            vec!["9".to_string(), "c".to_string()],
            vec!["2".to_string(), "a".to_string()],
        ];
        SpreadsheetController::set_spreadsheet_data(&mut *t, vec!["n".into(), "s".into()], rows.clone());
        t.handle_event(&header_click(50.0), &mut ctx); // ascending: 2, 9, 10 = rows 2, 1, 0
        t.handle_event(&body_click(row_y(0)), &mut ctx);
        assert_eq!(t.inner().selected_rows(), vec![2], "the first row shown is the data's third");
        WidgetHost::set_modifiers(&mut t, false, true, false);
        t.handle_event(&body_click(row_y(1)), &mut ctx);
        assert_eq!(t.inner().selected_rows(), vec![1, 2]);

        // A refresh keeps what is selected, less what the table lost.
        SpreadsheetController::set_spreadsheet_data(&mut *t, vec!["n".into(), "s".into()], rows[..2].to_vec());
        assert_eq!(t.inner().selected_rows(), vec![1]);
        t.inner_mut().set_selected_rows(&[]);
        assert!(t.inner().selected_rows().is_empty());
        assert!(t.inner_mut().take_selection_change());
    }

    /// The two bars cross at the middle of the body; they idle behind the
    /// plate, where a press on them is the row's; a scroll brings them to the
    /// fore, a pointer over a raised bar holds it there past the hold, and
    /// with nothing holding them they sink again. Hover alone never raises.
    #[test]
    fn the_scrollbars_cross_at_the_body_and_sink_until_scrolled() {
        let mut ctx = UiContext::new();
        let mut s = Spreadsheet::new();
        s.set_visible(true);
        let rect = Rect { x: 0.0, y: 0.0, width: 200.0, height: 124.0 };
        WidgetHost::set_rect(&mut s, rect.x, rect.y, rect.width, rect.height);
        let (id, ptr) = (s.id(), s.as_ptr_mut());
        ctx.register_widget(id, ptr);
        let headers: Vec<String> = (0..6).map(|i| format!("c{i}")).collect();
        let rows: Vec<Vec<String>> = (0..50).map(|r| (0..6).map(|c| format!("{r}.{c}")).collect()).collect();
        SpreadsheetController::set_spreadsheet_data(&mut *s, headers, rows);

        // A cross: the vertical bar centred on the width, the horizontal on
        // the body's height, each spanning its axis less the inset.
        let v = s.inner().geom(rect).expect("50 rows overflow");
        let h = s.inner().hgeom(rect).expect("6 floored columns overflow");
        let body_mid = HEADER_H + (rect.height - HEADER_H) * 0.5;
        assert!((v.bar_x + v.bar_w * 0.5 - rect.width * 0.5).abs() < 0.01, "vertical bar on the centre line");
        assert!((h.bar_y + h.bar_h * 0.5 - body_mid).abs() < 0.01, "horizontal bar on the body's centre line");
        assert_eq!((v.track_y, v.track_h), (HEADER_H + TRACK_INSET, rect.height - HEADER_H - 2.0 * TRACK_INSET));
        assert_eq!((h.track_x, h.track_w), (TRACK_INSET, rect.width - 2.0 * TRACK_INSET));

        let at = |x: f32, y: f32| Event::PointerMove { x, y, local_x: x, local_y: y };
        let mid = (rect.width * 0.5, body_mid);

        // Sunk: hovering raises nothing, and a press on the middle of the
        // cross is a press on the row there.
        assert!(!s.inner().scrollbars_raised());
        s.handle_event(&at(mid.0, mid.1), &mut ctx);
        assert!(!s.inner().scrollbars_raised(), "hover never raises a sunk bar");
        assert!(s.inner().body_row_at(mid.0, mid.1, rect).is_some(), "a sunk bar's lane is the row's");

        // A scroll raises both; the lane is the bars' now, and the fore copy
        // fades in over the next frames.
        let wheel = Event::MouseWheel {
            delta: MouseScrollDelta::LineDelta(0.0, -1.0),
            x: 30.0,
            y: 40.0,
            local_x: 30.0,
            local_y: 40.0,
        };
        assert!(s.handle_event(&wheel, &mut ctx));
        assert!(s.inner().scrollbars_raised(), "a scroll raises the bars");
        assert!(s.inner().body_row_at(mid.0, mid.1, rect).is_none(), "a raised bar's lane is the drag's");
        assert!(s.inner().body_row_at(mid.0, mid.1 + 30.0, rect).is_none(), "the vertical bar off the middle too");
        for _ in 0..20 {
            Input::tick(&mut *s.inner_mut(), 0.016, rect);
        }
        assert_eq!(s.inner().scrollbar_fade(), 1.0, "faded all the way in");

        // The pointer on a bar holds it up long past the hold…
        s.handle_event(&at(mid.0, mid.1 + 30.0), &mut ctx);
        for _ in 0..200 {
            Input::tick(&mut *s.inner_mut(), 0.016, rect);
        }
        assert!(s.inner().scrollbars_raised(), "hovered, a raised bar stays in front");

        // …and off it, the bars sink once the hold runs out, and fade away.
        s.handle_event(&at(30.0, 40.0), &mut ctx);
        let mut guard = 0;
        while Input::tick(&mut *s.inner_mut(), 0.016, rect) {
            guard += 1;
            assert!(guard < 1000, "the sink settles");
        }
        assert!(!s.inner().scrollbars_raised(), "unheld, the bars sink");
        assert_eq!(s.inner().scrollbar_fade(), 0.0);

        // Painted: nothing of the bars while sunk (the host draws that copy
        // behind its plate), four pills while raised.
        let pills = |s: &Adapted<Spreadsheet>| {
            let mut pc = crate::scene::paint::PaintCtx::new();
            Paint::paint(&**s, rect, &mut pc);
            pc.finish().items.iter().filter(|i| matches!(i.prim, crate::scene::paint::Prim::RoundedRect { .. })).count()
        };
        assert_eq!(pills(&s), 0, "a sunk bar is not painted over the cells");
        s.handle_event(&wheel, &mut ctx);
        for _ in 0..20 {
            Input::tick(&mut *s.inner_mut(), 0.016, rect);
        }
        assert_eq!(pills(&s), 4, "two tracks and two thumbs");
    }

    #[test]
    fn data_refresh_reapplies_sort_and_column_shrink_clears_it() {
        let mut ctx = UiContext::new();
        let mut s = Spreadsheet::new();
        s.set_visible(true);
        WidgetHost::set_rect(&mut s, 0.0, 0.0, 200.0, 124.0);
        let (id, ptr) = (s.id(), s.as_ptr_mut());
        ctx.register_widget(id, ptr);
        SpreadsheetController::set_spreadsheet_data(
            &mut *s,
            vec!["a".into(), "b".into()],
            vec![vec!["1".into(), "x".into()], vec!["2".into(), "y".into()]],
        );
        assert!(s.handle_event(&header_click(150.0), &mut ctx)); // sort col 1 asc

        // A refresh with new rows keeps the sort and re-derives the order.
        SpreadsheetController::set_spreadsheet_data(
            &mut *s,
            vec!["a".into(), "b".into()],
            vec![vec!["1".into(), "z".into()], vec!["2".into(), "w".into()]],
        );
        assert_eq!(s.inner().sort, Some((1, true)));
        assert_eq!(s.inner().order, vec![1, 0], "w < z");

        // A refresh that drops the sorted column clears the sort.
        SpreadsheetController::set_spreadsheet_data(
            &mut *s,
            vec!["a".into()],
            vec![vec!["1".into()], vec!["2".into()]],
        );
        assert_eq!(s.inner().sort, None);
        assert_eq!(s.inner().order, vec![0, 1]);
    }
}
