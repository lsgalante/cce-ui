//! Narrow-trait `Spreadsheet` (Phase 5l) — a read-only table with a header row, zebra rows,
//! column dividers, and an inertially-scrolled body: wheel input feeds a velocity that
//! [`Input::tick`] integrates and decays each frame, the scrollbar thumb is host-drag-driven
//! through the drag surface, and arrow/page/home/end keys jump the scroll. The scroll geometry
//! (content/viewport heights, thumb position) is derived in one place ([`ScrollGeom`]) — legacy
//! re-derived it in seven. [`SpreadsheetController`] rides the `Input` capability hooks.
//!
//! The `PARAM_BG` background is NOT emitted here: the designer's render path draws every
//! widget's background itself from `color()` + `corner_style()` (`push_widget_vertices`), and
//! `PARAM_BG` is translucent — emitting it again would double-blend. This widget's own
//! geometry starts at the header strip, exactly like the legacy `extra_quads`.

use crate::colors;
use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::{
    Adapted, ElementState, Event, EventCtx, Input, Key, Layout, MouseButton, MouseScrollDelta,
    NamedKey, Paint, SpreadsheetController,
};

const HEADER_H: f32 = 24.0;
const ROW_H: f32 = 24.0;
const SCROLLBAR_W: f32 = 6.0;
const SCROLLBAR_PAD: f32 = 2.0;
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
    scroll_velocity: f32,
    dragging_scrollbar: bool,
    drag_offset_y: f32,
    scrollbar_hovered: bool,
    scrollbar_thumb_hovered: bool,
    scroll_x: f32,
    hscroll_velocity: f32,
    dragging_hscrollbar: bool,
    drag_offset_x: f32,
    hscrollbar_hovered: bool,
    hscrollbar_thumb_hovered: bool,
}

/// Scroll/scrollbar geometry for one (row count, rect) pair. Present only when the content
/// overflows the viewport — every scroll behavior is gated on that, as in legacy.
struct ScrollGeom {
    visible_h: f32,
    max_scroll: f32,
    /// `scroll_y` clamped to the current bounds (data changes can leave the raw value stale).
    scroll: f32,
    scrollbar_x: f32,
    track_y: f32,
    thumb_h: f32,
    thumb_y: f32,
    track_range: f32,
}

/// [`ScrollGeom`]'s horizontal twin, for the bottom scrollbar. Present only
/// when the column run overflows the pane width.
struct HScrollGeom {
    visible_w: f32,
    max_scroll: f32,
    /// `scroll_x` clamped to the current bounds.
    scroll: f32,
    track_x: f32,
    track_y: f32,
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
            scroll_velocity: 0.0,
            dragging_scrollbar: false,
            drag_offset_y: 0.0,
            scrollbar_hovered: false,
            scrollbar_thumb_hovered: false,
            scroll_x: 0.0,
            hscroll_velocity: 0.0,
            dragging_hscrollbar: false,
            drag_offset_x: 0.0,
            hscrollbar_hovered: false,
            hscrollbar_thumb_hovered: false,
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
        let thumb_h = ((visible_h / content_h) * visible_h).clamp(15.0_f32.min(visible_h), visible_h);
        let track_y = rect.y + HEADER_H;
        let track_range = visible_h - thumb_h;
        Some(ScrollGeom {
            visible_h,
            max_scroll,
            scroll,
            scrollbar_x: rect.x + rect.width - SCROLLBAR_W - SCROLLBAR_PAD,
            track_y,
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
        let thumb_w = ((visible_w / content_w) * visible_w).clamp(15.0_f32.min(visible_w), visible_w);
        let track_x = rect.x;
        let track_range = visible_w - thumb_w;
        Some(HScrollGeom {
            visible_w,
            max_scroll,
            scroll,
            track_x,
            track_y: rect.y + rect.height - SCROLLBAR_W - SCROLLBAR_PAD,
            thumb_w,
            thumb_x: track_x + (scroll / max_scroll) * track_range,
            track_range,
        })
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

        // Scrollbar track & thumb
        if let Some(g) = self.geom(rect) {
            ctx.quad(
                Rect { x: g.scrollbar_x, y: g.track_y, width: SCROLLBAR_W, height: g.visible_h },
                [0.05, 0.05, 0.08, 0.15],
            );
            let thumb_color = if self.dragging_scrollbar {
                [0.40, 0.40, 0.48, 1.0]
            } else if self.scrollbar_thumb_hovered {
                [0.32, 0.32, 0.38, 1.0]
            } else if self.scrollbar_hovered {
                [0.24, 0.24, 0.30, 0.9]
            } else {
                [0.18, 0.18, 0.24, 0.7]
            };
            ctx.quad(
                Rect { x: g.scrollbar_x, y: g.thumb_y, width: SCROLLBAR_W, height: g.thumb_h },
                thumb_color,
            );
        }

        // Horizontal scrollbar along the bottom, the vertical bar's twin.
        if let Some(g) = self.hgeom(rect) {
            ctx.quad(
                Rect { x: g.track_x, y: g.track_y, width: g.visible_w, height: SCROLLBAR_W },
                [0.05, 0.05, 0.08, 0.15],
            );
            let thumb_color = if self.dragging_hscrollbar {
                [0.40, 0.40, 0.48, 1.0]
            } else if self.hscrollbar_thumb_hovered {
                [0.32, 0.32, 0.38, 1.0]
            } else if self.hscrollbar_hovered {
                [0.24, 0.24, 0.30, 0.9]
            } else {
                [0.18, 0.18, 0.24, 0.7]
            };
            ctx.quad(
                Rect { x: g.thumb_x, y: g.track_y, width: g.thumb_w, height: SCROLLBAR_W },
                thumb_color,
            );
        }

        // Header + cell text. Cells render only when the row lies fully inside the body.
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
            let (label, color) = match self.sort {
                Some((col, ascending)) if col == i => {
                    let mark = if ascending { '\u{25b2}' } else { '\u{25bc}' };
                    (format!("{header} {mark}"), [0xff, 0xff, 0xff])
                }
                _ => (header.clone(), [0xdd, 0xdd, 0xee]),
            };
            ctx.text_with(fit(label), cx, y + 6.0, 12.0, color, None, col_bounds(i, y, HEADER_H));
        }
        for (i, &src) in self.order.iter().enumerate() {
            let ry = y + HEADER_H + i as f32 * ROW_H - scroll;
            if ry < body_top || ry + ROW_H > body_bottom {
                continue;
            }
            for (col_idx, val) in self.rows[src].iter().enumerate().take(n_cols) {
                if !col_visible(col_idx) {
                    continue;
                }
                let cx = xoff + col_w * col_idx as f32 + 8.0;
                ctx.text_with(fit(val.clone()), cx, ry + 6.0, 12.0, [0xbb, 0xbb, 0xcc], None, col_bounds(col_idx, ry, ROW_H));
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

                let was_sb = self.scrollbar_hovered;
                let was_thumb = self.scrollbar_thumb_hovered;
                if let Some(g) = self.geom(r) {
                    self.scrollbar_hovered = *px >= g.scrollbar_x - 2.0
                        && *px <= r.x + r.width
                        && *py >= g.track_y
                        && *py <= r.y + r.height;
                    self.scrollbar_thumb_hovered = *px >= g.scrollbar_x - 2.0
                        && *px <= r.x + r.width
                        && *py >= g.thumb_y
                        && *py <= g.thumb_y + g.thumb_h;
                } else {
                    self.scrollbar_hovered = false;
                    self.scrollbar_thumb_hovered = false;
                }
                let was_hsb = self.hscrollbar_hovered;
                let was_hthumb = self.hscrollbar_thumb_hovered;
                if let Some(g) = self.hgeom(r) {
                    self.hscrollbar_hovered = *py >= g.track_y - 2.0
                        && *py <= r.y + r.height
                        && *px >= g.track_x
                        && *px <= g.track_x + g.visible_w;
                    self.hscrollbar_thumb_hovered = *py >= g.track_y - 2.0
                        && *py <= r.y + r.height
                        && *px >= g.thumb_x
                        && *px <= g.thumb_x + g.thumb_w;
                } else {
                    self.hscrollbar_hovered = false;
                    self.hscrollbar_thumb_hovered = false;
                }
                let was_header = self.header_hover_col;
                self.header_hover_col = self.header_col_at(*px, *py, r);
                was_hovered != self.hovered
                    || was_sb != self.scrollbar_hovered
                    || was_thumb != self.scrollbar_thumb_hovered
                    || was_hsb != self.hscrollbar_hovered
                    || was_hthumb != self.hscrollbar_thumb_hovered
                    || was_header != self.header_hover_col
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
                    return false;
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
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (-*x * ROW_H, -*y * ROW_H),
                    MouseScrollDelta::PixelDelta(pos) => (-pos.x as f32, -pos.y as f32),
                };
                let mut used = false;
                if dy.abs() > 0.0 && self.geom(ectx.rect).is_some() {
                    self.scroll_velocity += dy * 12.0;
                    used = true;
                }
                if dx.abs() > 0.0 && self.hgeom(ectx.rect).is_some() {
                    self.hscroll_velocity += dx * 12.0;
                    used = true;
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
                let old = g.scroll;
                let new = match &key_event.logical_key {
                    Key::Named(NamedKey::ArrowDown) => (old + ROW_H).clamp(0.0, g.max_scroll),
                    Key::Named(NamedKey::ArrowUp) => (old - ROW_H).clamp(0.0, g.max_scroll),
                    Key::Named(NamedKey::PageDown) => (old + g.visible_h).clamp(0.0, g.max_scroll),
                    Key::Named(NamedKey::PageUp) => (old - g.visible_h).clamp(0.0, g.max_scroll),
                    Key::Named(NamedKey::Home) => 0.0,
                    Key::Named(NamedKey::End) => g.max_scroll,
                    _ => return false,
                };
                self.scroll_y = new;
                (new - old).abs() > 0.01
            }
            _ => false,
        }
    }

    fn scrollable(&self) -> bool {
        true
    }

    // --- Scrollbar drag, host-driven (the designer checks `draggable()` on the pressed widget
    // and then streams `drag_update` at it). `drag_begin` decides whether the press actually
    // landed on the scrollbar; a body press starts no drag, exactly like legacy.

    fn draggable(&self, rect: Rect) -> bool {
        self.geom(rect).is_some() || self.hgeom(rect).is_some()
    }

    fn is_dragging(&self) -> bool {
        self.dragging_scrollbar || self.dragging_hscrollbar
    }

    fn drag_begin(&mut self, px: f32, py: f32, rect: Rect) {
        self.scroll_velocity = 0.0;
        self.hscroll_velocity = 0.0;
        // The vertical bar owns the shared bottom-right corner (it was here
        // first); the horizontal bar takes what's left of the bottom band.
        if let Some(g) = self.geom(rect) {
            if px >= g.scrollbar_x - 4.0
                && px <= rect.x + rect.width
                && py >= g.track_y
                && py <= rect.y + rect.height
            {
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
            if py >= g.track_y - 4.0
                && py <= rect.y + rect.height
                && px >= g.track_x
                && px <= g.track_x + g.visible_w
            {
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
            self.scroll_velocity = 0.0;
            if let Some(g) = self.geom(rect) {
                let old = g.scroll;
                self.scroll_to_thumb(&g, py - self.drag_offset_y);
                return (self.scroll_y - old).abs() > 0.01;
            }
            return false;
        }
        if self.dragging_hscrollbar {
            self.hscroll_velocity = 0.0;
            if let Some(g) = self.hgeom(rect) {
                let old = g.scroll;
                self.hscroll_to_thumb(&g, px - self.drag_offset_x);
                return (self.scroll_x - old).abs() > 0.01;
            }
        }
        false
    }

    fn drag_end(&mut self) {
        self.dragging_scrollbar = false;
        self.dragging_hscrollbar = false;
        self.scroll_velocity = 0.0;
        self.hscroll_velocity = 0.0;
    }

    // --- Inertial scroll: the wheel only sets velocity; each frame integrates and decays it.

    fn tick(&mut self, dt: f32, rect: Rect) -> bool {
        let mut moved = false;
        let friction = 8.0;
        if self.scroll_velocity.abs() > 0.01 {
            let content_h = self.rows.len() as f32 * ROW_H;
            let visible_h = (rect.height - HEADER_H).max(0.0);
            let max_scroll = (content_h - visible_h).max(0.0);
            let old = self.scroll_y;

            self.scroll_y = (self.scroll_y + self.scroll_velocity * dt).clamp(0.0, max_scroll);

            // Decelerate with friction (exponential decay); stop dead at the bounds or below
            // the motion threshold.
            self.scroll_velocity *= (-friction * dt).exp();
            if self.scroll_y == 0.0 || self.scroll_y == max_scroll {
                self.scroll_velocity = 0.0;
            }
            if self.scroll_velocity.abs() < 5.0 {
                self.scroll_velocity = 0.0;
            }
            moved |= (self.scroll_y - old).abs() > 0.01;
        }
        if self.hscroll_velocity.abs() > 0.01 {
            let max_scroll = self.hgeom(rect).map_or(0.0, |g| g.max_scroll);
            let old = self.scroll_x;
            self.scroll_x = (self.scroll_x + self.hscroll_velocity * dt).clamp(0.0, max_scroll);
            self.hscroll_velocity *= (-friction * dt).exp();
            if self.scroll_x == 0.0 || self.scroll_x == max_scroll {
                self.hscroll_velocity = 0.0;
            }
            if self.hscroll_velocity.abs() < 5.0 {
                self.hscroll_velocity = 0.0;
            }
            moved |= (self.scroll_x - old).abs() > 0.01;
        }
        moved
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

        // Press on the scrollbar track (x >= 200-6-2-4): thumb jumps, drag engages.
        s.drag_begin(195.0, 80.0);
        assert!(s.is_dragging());
        assert!(s.drag_update(195.0, 110.0), "thumb drag scrolls");
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
        assert!(!s.handle_event(&body, &mut ctx), "body press is not a sort");
        assert_eq!(s.inner().sort, Some((1, true)));
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
