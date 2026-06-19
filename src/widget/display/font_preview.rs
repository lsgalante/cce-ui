use crate::widget::*;
use crate::widget::display::TextLabel;

#[derive(Debug, Clone)]
pub struct FontPreview {
    base: Widget,
    pub font_family: String,
}

impl FontPreview {
    pub fn new(font_family: String) -> Self {
        Self {
            base: Widget::new(),
            font_family,
        }
    }

    pub fn set_font_family(&mut self, font_family: String) {
        self.font_family = font_family;
    }
}

impl Element for FontPreview {
    crate::impl_widget_base!(FontPreview);
    fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }
    fn highlight_quad(&self, _ctx: &UiContext) -> Option<(f32, f32, f32, f32, [f32; 4])>{ None }
    fn widget_font(&self) -> Option<String> { Some(self.font_family.clone()) }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let (x, y, w, h) = self.rect();
        let border_t = 1.0;
        let card_color = [0.10, 0.10, 0.14, 0.3];
        let border_color = [0.25, 0.25, 0.35, 0.5];
        vec![
            (x, y, w, h, card_color),
            (x, y, w, border_t, border_color),
            (x, y + h - border_t, w, border_t, border_color),
            (x, y, border_t, h, border_color),
            (x + w - border_t, y, border_t, h, border_color),
            (x + 16.0, y + 44.0, w - 32.0, 1.0, [0.22, 0.22, 0.30, 0.8]),
        ]
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let (x, y, _w, _h) = self.rect();
        vec![
            TextLabel {
                text: format!("Family: {}", self.font_family),
                x: x + 16.0,
                y: y + 16.0,
                font_size: 15.0,
                color: [230, 230, 242],
            },
            TextLabel {
                text: "abcdefghijklmnopqrstuvwxyz".to_string(),
                x: x + 16.0,
                y: y + 61.0,
                font_size: 13.0,
                color: [191, 191, 204],
            },
            TextLabel {
                text: "ABCDEFGHIJKLMNOPQRSTUVWXYZ".to_string(),
                x: x + 16.0,
                y: y + 83.0,
                font_size: 13.0,
                color: [191, 191, 204],
            },
            TextLabel {
                text: "0123456789 (!@#$%&*?)".to_string(),
                x: x + 16.0,
                y: y + 105.0,
                font_size: 13.0,
                color: [191, 191, 204],
            },
            TextLabel {
                text: "The quick brown fox jumps over the lazy dog.".to_string(),
                x: x + 16.0,
                y: y + 131.0,
                font_size: 16.0,
                color: [230, 230, 242],
            },
            TextLabel {
                text: "The five boxing wizards jump quickly.".to_string(),
                x: x + 16.0,
                y: y + 163.0,
                font_size: 20.0,
                color: [255, 255, 255],
            },
        ]
    }
}
