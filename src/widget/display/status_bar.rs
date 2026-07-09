//! Narrow-trait `StatusBar` (Phase 5t) — a one-line text bar whose theming is parent-coupled
//! exactly like MenuBar's: when its tracked parent is a Backplate it pulls the backplate
//! statusbar color/text-color/blur and derives its rounded corners from where it sits against
//! the parent's edges ([`Paint::corner_style`] + the corners walk). Two text paths, both
//! legacy: `TextLabel`s out of [`Paint::paint`] (container aggregation — deliberately with NO
//! `widget_font`, matching the legacy default-font behavior on that path), and pre-shaped
//! glyphon buffers through [`Paint::text_items`] (new with this migration) for manual hosts —
//! cce-status-interface calls `prepare_text` then `get_text_items` into its own paint.

use crate::colors;
use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::display::make_widget_text_buffer;
use crate::widget::{Adapted, Element, Input, Layout, Paint};

pub struct StatusBar {
    rect: Rect,
    pub text: String,
    pub text_buf: Option<glyphon::Buffer>,
    pub text_offset_x: Option<f32>,
    pub text_color: Option<[f32; 4]>,
    pub bg_color: Option<[f32; 4]>,
    pub parent: Option<*mut (dyn Element + 'static)>,
}

impl StatusBar {
    pub fn new() -> Adapted<StatusBar> {
        Adapted::new(StatusBar {
            rect: Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 },
            text: String::new(),
            text_buf: None,
            text_offset_x: None,
            text_color: None,
            bg_color: None,
            parent: None,
        })
    }

    pub fn set_text_offset_x(&mut self, offset: f32) {
        self.text_offset_x = Some(offset);
    }
    pub fn set_text_color(&mut self, color: [f32; 4]) {
        self.text_color = Some(color);
    }
    pub fn set_bg_color(&mut self, color: [f32; 4]) {
        self.bg_color = Some(color);
    }

    pub fn get_actual_text_color(&self) -> [f32; 4] {
        if let Some(p_ptr) = self.parent {
            if unsafe { (*p_ptr).is_backplate() } {
                return crate::colors::backplate_statusbar_text_color();
            }
        }
        self.text_color.unwrap_or([0.6666, 0.6666, 0.7333, 1.0])
    }

    pub fn is_blur_enabled(&self) -> bool {
        if let Some(p_ptr) = self.parent {
            if unsafe { (*p_ptr).is_backplate() } {
                return crate::colors::backplate_statusbar_blur();
            }
        }
        false
    }

    fn bg(&self) -> [f32; 4] {
        if let Some(p_ptr) = self.parent {
            if unsafe { (*p_ptr).is_backplate() } {
                let theme_color = crate::colors::backplate_statusbar_color();
                if theme_color[3] > 0.001 {
                    return theme_color;
                }
            }
        }
        self.bg_color.unwrap_or(colors::STATUS_BG)
    }

    fn corners_against_parent(&self, rect: Rect) -> (bool, bool, bool, bool) {
        if let Some(p_ptr) = self.parent {
            let is_bp = unsafe { (*p_ptr).is_backplate() };
            if is_bp {
                let (px, py, pw, ph) = unsafe { (*p_ptr).rect() };
                let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);
                let is_at_top = (y - py).abs() < 0.1;
                let is_at_bottom = (y + h - (py + ph)).abs() < 0.1;

                if is_at_top && is_at_bottom {
                    let is_at_left = (x - px).abs() < 0.1;
                    let is_at_right = (x + w - (px + pw)).abs() < 0.1;
                    return (is_at_left, is_at_right, is_at_right, is_at_left);
                } else if is_at_top {
                    return (true, true, false, false);
                } else if is_at_bottom {
                    return (false, false, true, true);
                }
            }
        }
        (false, false, false, false)
    }

    fn statusbar_font_size(&self) -> f32 {
        let (_, font_size) = crate::layout::statusbar_font_parsed();
        if font_size > 0.0 { font_size } else { 12.0 }
    }
}

impl Adapted<StatusBar> {
    pub fn with_text(mut self, text: &str) -> Self {
        self.text = text.to_string();
        self
    }
    pub fn with_text_offset_x(mut self, offset: f32) -> Self {
        self.text_offset_x = Some(offset);
        self
    }
    pub fn with_text_color(mut self, color: [f32; 4]) -> Self {
        self.text_color = Some(color);
        self
    }
    pub fn with_bg_color(mut self, color: [f32; 4]) -> Self {
        self.bg_color = Some(color);
        self
    }
}

impl Layout for StatusBar {
    /// The status text draws inside the bar; the base label must never inflate the rect or
    /// emit a detached label (`Element::set_text` writes both the base copy and
    /// [`Paint::sync_label`]).
    fn inline_label(&self) -> bool {
        true
    }

    fn rect_assigned(&mut self, rect: Rect) {
        self.rect = rect;
    }

    fn parent_changed(&mut self, parent: Option<*mut (dyn Element + 'static)>) {
        self.parent = parent;
    }

    fn tracked_parent(&self) -> Option<Option<*mut (dyn Element + 'static)>> {
        Some(self.parent)
    }
}

impl Paint for StatusBar {
    fn color(&self) -> [f32; 4] {
        self.bg()
    }

    fn corner_style(&self, rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        let radius = match self.parent {
            Some(p_ptr) => unsafe { (*p_ptr).corner_radius() },
            None => 0.0,
        };
        Some((radius, self.corners_against_parent(rect)))
    }

    /// `Element::set_text` lands here: swap the text and drop the shaped buffer so
    /// `prepare_text` rebuilds it.
    fn sync_label(&mut self, label: &str) {
        if self.text != label {
            self.text = label.to_string();
            self.text_buf = None;
        }
    }

    /// Background exactly on the legacy split: a plain quad when cornerless (the legacy
    /// `extra_quads` body), a rounded rect against the parent's corners otherwise (the legacy
    /// default `all_rounded_quads` path) — plus the text label (the legacy `text_labels`
    /// body; deliberately no `widget_font`, see module docs).
    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let corners = self.corners_against_parent(rect);
        let bg = self.bg();
        if corners == (false, false, false, false) {
            ctx.quad(rect, bg);
        } else if bg[3].abs() > 0.001 {
            let radius = match self.parent {
                Some(p_ptr) => unsafe { (*p_ptr).corner_radius() },
                None => 0.0,
            };
            ctx.rounded_rect(rect, radius, corners, bg);
        }

        if !self.text.is_empty() {
            let offset_x = self.text_offset_x.unwrap_or(12.0);
            let c = self.get_actual_text_color();
            let color = [
                (c[0] * 255.0) as u8,
                (c[1] * 255.0) as u8,
                (c[2] * 255.0) as u8,
            ];
            let size = self.statusbar_font_size();
            let text_y = crate::layout::align_text_y(rect.y, rect.height, size, 0.0);
            ctx.text(self.text.clone(), rect.x + offset_x, text_y, size, color);
        }
    }

    fn prepare_text(&mut self, fs: &mut glyphon::FontSystem, _rect: Rect) {
        if !self.text.is_empty() && self.text_buf.is_none() {
            let (font_fam, font_size) = crate::layout::statusbar_font_parsed();
            let size = if font_size > 0.0 { font_size } else { 12.0 };
            let fam = if font_fam.is_empty() { "Berkeley Mono".to_string() } else { font_fam };
            self.text_buf = Some(make_widget_text_buffer(fs, &self.text, size, &fam));
        }
    }

    fn text_items(&self) -> Vec<(&glyphon::Buffer, f32, f32, glyphon::Color)> {
        if let Some(ref text_buf) = self.text_buf {
            let offset_x = self.text_offset_x.unwrap_or(12.0);
            let c = self.get_actual_text_color();
            let color = glyphon::Color::rgb(
                (c[0] * 255.0) as u8,
                (c[1] * 255.0) as u8,
                (c[2] * 255.0) as u8,
            );
            let size = self.statusbar_font_size();
            let text_y = crate::layout::align_text_y(self.rect.y, self.rect.height, size, 0.0);
            vec![(text_buf, self.rect.x + offset_x, text_y, color)]
        } else {
            Vec::new()
        }
    }
}

impl Input for StatusBar {
    fn blocks_backplate_drag(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::Element;

    /// The manual-host path cce-status-interface drives by hand: `set_text` drops the shaped
    /// buffer, `prepare_text` rebuilds it, `get_text_items` serves it (the new
    /// `Paint::text_items` hook) at the bar's rect.
    #[test]
    fn manual_host_text_pipeline() {
        let mut fs = glyphon::FontSystem::new();
        let mut bar = StatusBar::new().with_text("hello").with_text_offset_x(15.0);
        Element::set_rect(&mut bar, 0.0, 570.0, 800.0, 30.0);

        assert!(Element::get_text_items(&bar).is_empty(), "no buffer before prepare_text");
        Element::prepare_text(&mut bar, &mut fs);
        let items = Element::get_text_items(&bar);
        assert_eq!(items.len(), 1, "one shaped buffer");
        assert_eq!(items[0].1, 15.0, "x = rect.x + text_offset_x");

        // set_text drops the stale buffer; prepare_text reshapes.
        Element::set_text(&mut bar, "world");
        assert!(Element::get_text_items(&bar).is_empty(), "buffer dropped on text change");
        Element::prepare_text(&mut bar, &mut fs);
        assert_eq!(Element::get_text_items(&bar).len(), 1);
        assert_eq!(bar.text, "world");

        // Parentless: cornerless plain bg through the plain-quad bridge, at STATUS_BG.
        let extra = Element::extra_quads(&bar);
        assert_eq!(extra.len(), 1, "cornerless bg quad");
        assert_eq!(Element::rounded_corners(&bar), (false, false, false, false));
        assert!(!Element::blocks_backplate_drag(&bar));
    }
}
