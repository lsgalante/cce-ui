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
    Adapted, ElementState, Event, EventCtx, Input, Key, Layout, MouseScrollDelta, NamedKey,
    Paint, SpreadsheetController,
};

const HEADER_H: f32 = 24.0;
const ROW_H: f32 = 24.0;
const SCROLLBAR_W: f32 = 6.0;
const SCROLLBAR_PAD: f32 = 2.0;

pub struct Spreadsheet {
    hovered: bool,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    scroll_y: f32,
    scroll_velocity: f32,
    dragging_scrollbar: bool,
    drag_offset_y: f32,
    scrollbar_hovered: bool,
    scrollbar_thumb_hovered: bool,
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

impl Spreadsheet {
    pub fn new() -> Adapted<Spreadsheet> {
        let mut s = Adapted::new(Spreadsheet {
            hovered: false,
            headers: Vec::new(),
            rows: Vec::new(),
            scroll_y: 0.0,
            scroll_velocity: 0.0,
            dragging_scrollbar: false,
            drag_offset_y: 0.0,
            scrollbar_hovered: false,
            scrollbar_thumb_hovered: false,
        });
        // The spreadsheet pane starts hidden (the designer toggles it in later).
        crate::widget::Element::set_visible(&mut s, false);
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
    fn color(&self) -> [f32; 4] {
        colors::PARAM_BG
    }

    fn corner_style(&self) -> Option<(f32, (bool, bool, bool, bool))> {
        // Legacy: rounded_corners override (all corners) with the Element-default 12.0 radius.
        Some((12.0, (true, true, true, true)))
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

        // Vertical column dividers
        if h > 0.0 && !self.headers.is_empty() {
            let n_cols = self.headers.len();
            for i in 1..n_cols {
                let r = i as f32 / n_cols as f32;
                ctx.quad(Rect { x: x + w * r, y, width: 1.0, height: h }, [0.20, 0.20, 0.25, 0.15]);
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

        // Header + cell text. Cells render only when the row lies fully inside the body.
        if self.headers.is_empty() {
            return;
        }
        let n_cols = self.headers.len();
        for (i, header) in self.headers.iter().enumerate() {
            let cx = x + w * (i as f32 / n_cols as f32) + 8.0;
            ctx.text(header.clone(), cx, y + 6.0, 12.0, [0xdd, 0xdd, 0xee]);
        }
        for (i, row) in self.rows.iter().enumerate() {
            let ry = y + HEADER_H + i as f32 * ROW_H - scroll;
            if ry < body_top || ry + ROW_H > body_bottom {
                continue;
            }
            for (col_idx, val) in row.iter().enumerate().take(n_cols) {
                let cx = x + w * (col_idx as f32 / n_cols as f32) + 8.0;
                ctx.text(val.clone(), cx, ry + 6.0, 12.0, [0xbb, 0xbb, 0xcc]);
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
                was_hovered != self.hovered
                    || was_sb != self.scrollbar_hovered
                    || was_thumb != self.scrollbar_thumb_hovered
            }
            // Hit-gated by the adapter (which also rejects hidden widgets).
            Event::MouseWheel { delta, .. } => {
                if self.geom(ectx.rect).is_some() {
                    let scroll_amount = match delta {
                        MouseScrollDelta::LineDelta(_x, y) => *y * ROW_H,
                        MouseScrollDelta::PixelDelta(pos) => pos.y as f32,
                    };
                    self.scroll_velocity += scroll_amount * 12.0;
                    true
                } else {
                    false
                }
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
        self.geom(rect).is_some()
    }

    fn is_dragging(&self) -> bool {
        self.dragging_scrollbar
    }

    fn drag_begin(&mut self, px: f32, py: f32, rect: Rect) {
        self.scroll_velocity = 0.0;
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
            }
        }
    }

    fn drag_update(&mut self, _px: f32, py: f32, rect: Rect) -> bool {
        if !self.dragging_scrollbar {
            return false;
        }
        self.scroll_velocity = 0.0;
        if let Some(g) = self.geom(rect) {
            let old = g.scroll;
            self.scroll_to_thumb(&g, py - self.drag_offset_y);
            return (self.scroll_y - old).abs() > 0.01;
        }
        false
    }

    fn drag_end(&mut self) {
        self.dragging_scrollbar = false;
        self.scroll_velocity = 0.0;
    }

    // --- Inertial scroll: the wheel only sets velocity; each frame integrates and decays it.

    fn tick(&mut self, dt: f32, rect: Rect) -> bool {
        if self.scroll_velocity.abs() <= 0.01 {
            return false;
        }
        let content_h = self.rows.len() as f32 * ROW_H;
        let visible_h = (rect.height - HEADER_H).max(0.0);
        let max_scroll = (content_h - visible_h).max(0.0);
        let old = self.scroll_y;

        self.scroll_y = (self.scroll_y + self.scroll_velocity * dt).clamp(0.0, max_scroll);

        // Decelerate with friction (exponential decay); stop dead at the bounds or below the
        // motion threshold.
        let friction = 8.0;
        self.scroll_velocity *= (-friction * dt).exp();
        if self.scroll_y == 0.0 || self.scroll_y == max_scroll {
            self.scroll_velocity = 0.0;
        }
        if self.scroll_velocity.abs() < 5.0 {
            self.scroll_velocity = 0.0;
        }

        (self.scroll_y - old).abs() > 0.01
    }

    fn wants_tick(&self) -> bool {
        true
    }

    fn spreadsheet_controller(&self) -> Option<&dyn SpreadsheetController> {
        Some(self)
    }
    fn spreadsheet_controller_mut(&mut self) -> Option<&mut dyn SpreadsheetController> {
        Some(self)
    }
}

impl SpreadsheetController for Spreadsheet {
    fn set_spreadsheet_data(&mut self, headers: Vec<String>, rows: Vec<Vec<String>>) {
        self.headers = headers;
        self.rows = rows;
        // The raw scroll may now exceed the new content; every consumer clamps through
        // `geom()`, and the next scroll write re-clamps it for real.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::UiContext;
    use crate::widget::Element;

    fn filled(rows: usize) -> Adapted<Spreadsheet> {
        let mut s = Spreadsheet::new();
        s.set_visible(true);
        Element::set_rect(&mut s, 0.0, 0.0, 200.0, 124.0); // viewport: 100 = ~4 rows of 24
        let data: Vec<Vec<String>> =
            (0..rows).map(|i| vec![format!("r{i}"), format!("v{i}")]).collect();
        let elem: &mut dyn Element = &mut s;
        elem.as_spreadsheet_controller_mut()
            .expect("Spreadsheet exposes SpreadsheetController")
            .set_spreadsheet_data(vec!["a".into(), "b".into()], data);
        s
    }

    #[test]
    fn wheel_velocity_integrates_and_decays_through_tick() {
        let mut ctx = UiContext::new();
        let mut s = filled(50);
        let (id, ptr) = (s.id(), s.as_ptr_mut());
        ctx.register_widget(id, ptr);

        // A wheel over the body feeds velocity (scroll down = negative line delta in practice,
        // but sign just follows the delta)…
        let wheel = Event::MouseWheel {
            delta: MouseScrollDelta::LineDelta(0.0, 2.0),
            x: 50.0,
            y: 60.0,
            local_x: 50.0,
            local_y: 60.0,
        };
        assert!(s.handle_event(&wheel, &mut ctx), "in-rect wheel consumed");

        // …which tick integrates into scroll movement and decays to a stop.
        assert!(Element::tick(&mut s, 0.016, &mut ctx), "first tick moves the scroll");
        let mut guard = 0;
        while Element::tick(&mut s, 0.016, &mut ctx) {
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
        assert!(Element::draggable(&s));

        // Press on the scrollbar track (x >= 200-6-2-4): thumb jumps, drag engages.
        Element::drag_begin(&mut s, 195.0, 80.0);
        assert!(Element::is_dragging(&s));
        assert!(Element::drag_update(&mut s, 195.0, 110.0), "thumb drag scrolls");
        let dragged_to = s.inner().geom(rect).unwrap().scroll;
        assert!(dragged_to > 0.0);
        Element::drag_end(&mut s);
        assert!(!Element::is_dragging(&s));

        // A body press (left of the scrollbar) engages no drag.
        Element::drag_begin(&mut s, 50.0, 60.0);
        assert!(!Element::is_dragging(&s), "body press is not a scrollbar drag");

        // End key jumps to max; Home returns to zero. (Keys route via keyboard_input.)
        let end = crate::widget::KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::End),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
        };
        assert!(Element::keyboard_input(&mut s, &end, &mut ctx));
        let g = s.inner().geom(rect).unwrap();
        assert_eq!(g.scroll, g.max_scroll);

        // Hidden: the focused-widget keyboard path must not consume keys.
        s.set_visible(false);
        assert!(!Element::keyboard_input(&mut s, &end, &mut ctx), "hidden widget ignores keys");
    }
}
