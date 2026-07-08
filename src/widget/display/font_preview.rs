//! Narrow-trait font preview card (Phase 5i leaf sweep). Its `widget_font` forward makes every
//! text prim render in the previewed family on the legacy text paths.

use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::{Adapted, Input, Layout, Paint};

#[derive(Debug, Clone)]
pub struct FontPreview {
    pub font_family: String,
}

impl FontPreview {
    pub fn new(font_family: String) -> Adapted<FontPreview> {
        Adapted::new(FontPreview { font_family })
    }

    pub fn set_font_family(&mut self, font_family: String) {
        self.font_family = font_family;
    }
}

impl Layout for FontPreview {
    fn inline_label(&self) -> bool {
        true
    }
}

impl Paint for FontPreview {
    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn widget_font(&self) -> Option<String> {
        Some(self.font_family.clone())
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);
        let border_t = 1.0;
        let card_color = [0.10, 0.10, 0.14, 0.3];
        let border_color = [0.25, 0.25, 0.35, 0.5];
        ctx.quad(rect, card_color);
        ctx.quad(Rect { x, y, width: w, height: border_t }, border_color);
        ctx.quad(Rect { x, y: y + h - border_t, width: w, height: border_t }, border_color);
        ctx.quad(Rect { x, y, width: border_t, height: h }, border_color);
        ctx.quad(Rect { x: x + w - border_t, y, width: border_t, height: h }, border_color);
        ctx.quad(Rect { x: x + 16.0, y: y + 44.0, width: w - 32.0, height: 1.0 }, [0.22, 0.22, 0.30, 0.8]);

        ctx.text(format!("Family: {}", self.font_family), x + 16.0, y + 16.0, 15.0, [230, 230, 242]);
        ctx.text("abcdefghijklmnopqrstuvwxyz".to_string(), x + 16.0, y + 61.0, 13.0, [191, 191, 204]);
        ctx.text("ABCDEFGHIJKLMNOPQRSTUVWXYZ".to_string(), x + 16.0, y + 83.0, 13.0, [191, 191, 204]);
        ctx.text("0123456789 (!@#$%&*?)".to_string(), x + 16.0, y + 105.0, 13.0, [191, 191, 204]);
        ctx.text("The quick brown fox jumps over the lazy dog.".to_string(), x + 16.0, y + 131.0, 16.0, [230, 230, 242]);
        ctx.text("The five boxing wizards jump quickly.".to_string(), x + 16.0, y + 163.0, 20.0, [255, 255, 255]);
    }
}

impl Input for FontPreview {}
