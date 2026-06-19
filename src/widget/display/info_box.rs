use crate::colors;
use crate::widget::*;
use crate::widget::display::TextLabel;

#[derive(Debug, Clone)]
pub struct InfoBox {
    base: Widget,
    pub title: String,
    pub lines: Vec<String>,
}

impl InfoBox {
    pub fn new(title: &str, lines: Vec<String>) -> Self {
        Self {
            base: Widget::new(),
            title: title.to_string(),
            lines,
        }
    }
}

impl Element for InfoBox {
    crate::impl_widget_base!(InfoBox);
    fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }
    fn highlight_quad(&self, _ctx: &UiContext) -> Option<(f32, f32, f32, f32, [f32; 4])>{ None }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let (x, y, w, h) = self.rect();
        let theme = colors::active_theme();
        let bg_color = theme.surface_bg;
        let border_color = theme.surface_border;
        let border_t = 1.0;
        vec![
            (x, y, w, h, bg_color),
            (x, y, w, border_t, border_color),
            (x, y + h - border_t, w, border_t, border_color),
            (x, y, border_t, h, border_color),
            (x + w - border_t, y, border_t, h, border_color),
        ]
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let (x, y, _w, _h) = self.rect();
        let mut labels = Vec::new();
        labels.push(TextLabel {
            text: self.title.clone(),
            x: x + 16.0,
            y: y + 12.0,
            font_size: 12.0,
            color: [89, 165, 229],
        });
        
        let mut current_y = y + 32.0;
        for (idx, line) in self.lines.iter().enumerate() {
            let color = if idx == self.lines.len() - 1 {
                [140, 140, 153]
            } else {
                [204, 204, 217]
            };
            labels.push(TextLabel {
                text: line.clone(),
                x: x + 16.0,
                y: current_y,
                font_size: 11.0,
                color,
            });
            current_y += 16.0;
        }
        labels
    }
}
