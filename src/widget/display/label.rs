use crate::widget::*;
use crate::widget::display::{TextLabel, TextItem};

#[derive(Debug, Clone)]
pub struct Label {
    base: Widget,
    font_size: f32,
    color: [u8; 3],
}

impl Label {
    pub fn new(text: &str) -> Self {
        let mut base = Widget::new();
        base.label = Some(text.to_string());
        Self {
            base,
            font_size: 12.0,
            color: [0x83, 0x83, 0x8a],
        }
    }

    pub fn with_font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    pub fn with_color(mut self, color: [u8; 3]) -> Self {
        self.color = color;
        self
    }

    pub fn set_text(&mut self, text: &str) {
        self.base.label = Some(text.to_string());
    }

    pub fn set_color(&mut self, color: [u8; 3]) {
        self.color = color;
    }
}

impl Element for Label {
    crate::impl_widget_base!(Label);

    fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }

    fn text_labels(&self) -> Vec<TextLabel> {
        vec![TextLabel {
            text: self.base.label.clone().unwrap_or_default(),
            x: self.base.x,
            y: crate::layout::align_text_y(self.base.y, self.base.h, self.font_size, 0.0),
            font_size: self.font_size,
            color: self.color,
        }]
    }
}

#[derive(Clone)]
pub struct SectionHeader {
    base: Widget,
}

impl SectionHeader {
    pub fn new(title: &str) -> Self {
        let mut base = Widget::new();
        base.label = Some(title.to_string());
        Self { base }
    }
}

impl Element for SectionHeader {
    crate::impl_widget_base!(SectionHeader);
    fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }
    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        vec![(self.base.x + 8.0, self.base.y + 22.0, self.base.w - 16.0, 1.0, [0.18, 0.18, 0.27, 1.0])]
    }
    fn text_labels(&self) -> Vec<TextLabel> {
        vec![TextLabel {
            text: self.base.label.clone().unwrap_or_default(),
            x: self.base.x + 12.0,
            y: self.base.y,
            font_size: 14.0,
            color: [212, 212, 212],
        }]
    }
}

// Styled label builder with optional strikethrough
#[derive(Debug)]
pub struct StyledLabel {
    pub buffer: glyphon::Buffer,
    pub w: f32,
    pub color: [f32; 4],
    pub g_color: glyphon::Color,
    pub strikethrough: bool,
    pub strikethrough_color: Option<[f32; 4]>,
}

impl StyledLabel {
    pub fn new(fs: &mut glyphon::FontSystem, text: &str, size: f32, color: [f32; 4]) -> Self {
        Self::new_with_family(fs, text, size, color, "sans-serif")
    }

    pub fn new_with_family(fs: &mut glyphon::FontSystem, text: &str, size: f32, color: [f32; 4], family: &str) -> Self {
        let scale = crate::scale::scale_factor();
        let physical_size = size * scale;
        let metrics = glyphon::Metrics::new(physical_size, physical_size * 1.4);
        let mut buffer = glyphon::Buffer::new(fs, metrics);
        let family_enum = match family {
            "monospace" => glyphon::Family::Name(crate::layout::get_system_monospace_font()),
            "sans-serif" => glyphon::Family::SansSerif,
            "serif" => glyphon::Family::Serif,
            name => glyphon::Family::Name(name),
        };
        let attrs = glyphon::Attrs::new().family(family_enum);
        buffer.set_text(fs, text, attrs, glyphon::Shaping::Advanced);
        buffer.shape_until_scroll(fs, true);
        let w = buffer.layout_runs().next().map(|r| r.line_w).unwrap_or(0.0) / scale;
        let g_color = glyphon::Color::rgb(
            (color[0] * 255.0) as u8,
            (color[1] * 255.0) as u8,
            (color[2] * 255.0) as u8,
        );
        Self {
            buffer,
            w,
            color,
            g_color,
            strikethrough: false,
            strikethrough_color: None,
        }
    }

    pub fn with_strikethrough(mut self, enabled: bool) -> Self {
        self.strikethrough = enabled;
        self
    }

    pub fn with_strikethrough_color(mut self, color: [f32; 4]) -> Self {
        self.strikethrough_color = Some(color);
        self
    }

    pub fn draw(self, text_items: &mut Vec<TextItem>, x: f32, y: f32) -> f32 {
        let w = self.w;
        text_items.push(TextItem {
            buffer: self.buffer,
            x,
            y,
            color: self.g_color,
            bounds: None,
        });
        w
    }

    pub fn strikethrough_rect(&self, x: f32, y: f32, scale: f32) -> Option<(f32, f32, f32, f32, [f32; 4])> {
        if self.strikethrough {
            let col = self.strikethrough_color.unwrap_or(self.color);
            let font_size = self.buffer.metrics().font_size / scale;
            let line_y = self.buffer.layout_runs().next().map(|r| r.line_y).unwrap_or(font_size * scale * 1.05) / scale;
            let offset_y = line_y - 0.28 * font_size;
            let padding = 4.0;
            Some((
                x - padding,
                y + offset_y,
                self.w + 2.0 * padding,
                1.0,
                col,
            ))
        } else {
            None
        }
    }
}
