//! Narrow-trait `StatusBar` (Phase 5t) — a one-line text bar whose theming is parent-coupled
//! exactly like MenuBar's: when its tracked parent is a Backplate it pulls the backplate
//! statusbar color/text-color/blur and derives its rounded corners from where it sits against
//! the parent's edges ([`Paint::corner_style`] + the corners walk). Two text paths: the
//! [`Paint::paint`] prim (carrying the configured statusbar font — the legacy default-font
//! behavior on this path dropped the family and rendered sans), and pre-shaped glyphon
//! buffers through [`Paint::text_items`] (new with this migration) for manual hosts —
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
    /// Draw as a step carved into the window backplate instead of an opaque slab: no
    /// background fill of its own, just the shaded wall facing the content, so the plate
    /// shows through. `bg_color` is ignored while this is set — see
    /// [`Adapted::<StatusBar>::with_recess`].
    pub recessed: bool,
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
            recessed: false,
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

    /// Drop the bar's own background and sink it into the window backplate instead, the
    /// mirror of `MenuBar::with_recess`. A status bar always sits flush with the bottom of
    /// the plate, so it is shaded as a plateau one step down whose only wall is the top one
    /// (facing the content) — the other three sides are the plate's outer edge, which
    /// carries its own roll.
    pub fn with_recess(mut self, recessed: bool) -> Self {
        self.recessed = recessed;
        self
    }
}

impl Layout for StatusBar {
    /// The status text draws inside the bar; the base label must never inflate the rect or
    /// emit a detached label (`WidgetHost::set_text` writes both the base copy and
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

    /// `WidgetHost::set_text` lands here: swap the text and drop the shaped buffer so
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
        if self.recessed {
            // The recess shading is a light/shadow overlay — whatever the plate painted
            // here shows through modulated, so no surface color is needed.
            // Capped against the bar's own height so a deep DE-wide roll can't swallow it
            // (a single wall straddling the boundary intrudes only half its width).
            let depth = crate::layout::bar_wall_width().min(rect.height * 0.6);
            ctx.recess_edges(rect, (0.0, 0.0, 0.0, 0.0), depth, (true, false, false, false));
        } else {
            // Always the plain background quad — the rounded-against-parent variant required a
            // backplate parent, which no longer exists.
            ctx.quad(rect, self.bg());
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
            ctx.text_with(
                self.text.clone(),
                rect.x + offset_x,
                text_y,
                size,
                color,
                Some(crate::layout::statusbar_font()),
                None,
            );
        }
    }

    /// The paint walk re-fonts prim-derived labels through this (the prim's own font field
    /// is stripped by `own_labels_for_walk`) — without it the bar's text falls back to sans.
    fn text_font(&self) -> Option<String> {
        Some(crate::layout::statusbar_font())
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
    use crate::widget::WidgetHost;

    /// The shaped-buffer lifecycle behind the old manual-host path: `set_text` drops the
    /// buffer, `prepare_text` rebuilds it. (The `get_text_items` getter that served it is
    /// deleted; the bar's rendered text is the `Paint::paint` prim.)
    #[test]
    fn manual_host_text_pipeline() {
        let mut fs = glyphon::FontSystem::new();
        let mut bar = StatusBar::new().with_text("hello").with_text_offset_x(15.0);
        WidgetHost::set_rect(&mut bar, 0.0, 570.0, 800.0, 30.0);

        assert!(bar.text_buf.is_none(), "no buffer before prepare_text");
        WidgetHost::prepare_text(&mut bar, &mut fs);
        assert!(bar.text_buf.is_some(), "one shaped buffer");

        // set_text drops the stale buffer; prepare_text reshapes.
        bar.set_text("world");
        assert!(bar.text_buf.is_none(), "buffer dropped on text change");
        WidgetHost::prepare_text(&mut bar, &mut fs);
        assert!(bar.text_buf.is_some());
        assert_eq!(bar.text, "world");

        // Parentless: cornerless plain bg through the plain-quad bridge, at STATUS_BG.
        let extra = WidgetHost::extra_quads(&bar);
        assert_eq!(extra.len(), 1, "cornerless bg quad");
        assert_eq!(WidgetHost::corner_style(&bar).1, (false, false, false, false));
        assert!(!WidgetHost::blocks_backplate_drag(&bar));
    }

    /// The paint walk strips prim fonts and re-fonts labels via `Paint::text_font` — the
    /// bar's text must come out of the walk carrying the configured statusbar font.
    #[test]
    fn walk_text_carries_statusbar_font() {
        let ui = crate::context::UiContext::new();
        let mut bar = StatusBar::new().with_text("ready");
        WidgetHost::set_rect(&mut bar, 0.0, 570.0, 800.0, 30.0);

        let mut pc = PaintCtx::new();
        crate::scene::painter::paint_root_into(&ui, &bar, &mut pc);
        let fonts: Vec<_> = pc
            .finish()
            .items
            .into_iter()
            .filter_map(|item| match item.prim {
                crate::scene::paint::Prim::Text { font, .. } => Some(font),
                _ => None,
            })
            .collect();
        assert_eq!(fonts.len(), 1, "one text label out of the walk");
        assert_eq!(fonts[0].as_deref(), Some(crate::layout::statusbar_font().as_str()));
    }
}
