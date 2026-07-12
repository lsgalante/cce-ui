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
use crate::widget::{Adapted, Input, Layout, Paint};

pub struct StatusBar {
    rect: Rect,
    pub text: String,
    pub text_buf: Option<glyphon::Buffer>,
    pub text_offset_x: Option<f32>,
    pub text_color: Option<[f32; 4]>,
    pub bg_color: Option<[f32; 4]>,
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
        self.text_color.unwrap_or([0.6666, 0.6666, 0.7333, 1.0])
    }

    pub fn is_blur_enabled(&self) -> bool {
        false
    }

    fn bg(&self) -> [f32; 4] {
        self.bg_color.unwrap_or(colors::STATUS_BG)
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

}

impl Paint for StatusBar {
    fn color(&self) -> [f32; 4] {
        self.bg()
    }

    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        // Corners never round (the backplate-adjacency source is gone). The radius was the
        // parent's, read through a stored pointer — but nothing ever set_parent's a StatusBar,
        // so 0.0 is what production always read (6bd: the dead pointer field is gone).
        Some((0.0, (false, false, false, false)))
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
        // Always the plain background quad — the rounded-against-parent variant required a
        // backplate parent, which no longer exists.
        ctx.quad(rect, self.bg());

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

    /// The shaped-buffer lifecycle behind the old manual-host path: `set_text` drops the
    /// buffer, `prepare_text` rebuilds it. (The `get_text_items` getter that served it is
    /// deleted; the bar's rendered text is the `Paint::paint` prim.)
    #[test]
    fn manual_host_text_pipeline() {
        let mut fs = glyphon::FontSystem::new();
        let mut bar = StatusBar::new().with_text("hello").with_text_offset_x(15.0);
        Element::set_rect(&mut bar, 0.0, 570.0, 800.0, 30.0);

        assert!(bar.text_buf.is_none(), "no buffer before prepare_text");
        Element::prepare_text(&mut bar, &mut fs);
        assert!(bar.text_buf.is_some(), "one shaped buffer");

        // set_text drops the stale buffer; prepare_text reshapes.
        Element::set_text(&mut bar, "world");
        assert!(bar.text_buf.is_none(), "buffer dropped on text change");
        Element::prepare_text(&mut bar, &mut fs);
        assert!(bar.text_buf.is_some());
        assert_eq!(bar.text, "world");

        // Parentless: cornerless plain bg through the plain-quad bridge, at STATUS_BG.
        let extra = Element::extra_quads(&bar);
        assert_eq!(extra.len(), 1, "cornerless bg quad");
        assert_eq!(Element::corner_style(&bar).1, (false, false, false, false));
        assert!(!Element::blocks_backplate_drag(&bar));
    }
}
