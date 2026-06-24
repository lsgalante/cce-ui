use crate::colors;
use crate::widget::*;
use crate::widget::display::TextLabel;

#[derive(Debug, Clone)]
pub struct TextItem {
    pub buffer: glyphon::Buffer,
    pub x: f32,
    pub y: f32,
    pub color: glyphon::Color,
    pub bounds: Option<[f32; 4]>,
}

impl TextItem {
    pub fn new(
        fs: &mut glyphon::FontSystem,
        text: &str,
        size: f32,
        x: f32,
        y: f32,
        color: glyphon::Color,
        font: Option<&str>,
        bounds: Option<[f32; 4]>,
    ) -> Self {
        let buffer = crate::backend::window_runner::get_text_buffer(fs, text, size, font);
        Self {
            buffer,
            x,
            y,
            color,
            bounds,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InteractiveListItem {
    base: Widget,
    pub title: String,
    pub subtitle: Option<String>,
    pub selected: bool,
    pub pressed: bool,
    pub just_clicked: bool,
}

impl InteractiveListItem {
    pub fn new(title: &str) -> Self {
        Self {
            base: Widget::new(),
            title: title.to_string(),
            subtitle: None,
            selected: false,
            pressed: false,
            just_clicked: false,
        }
    }

    pub fn with_subtitle(mut self, subtitle: &str) -> Self {
        self.subtitle = Some(subtitle.to_string());
        self
    }

    pub fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }
}

impl Element for InteractiveListItem {
    crate::impl_widget_base!(InteractiveListItem);
    fn highlight_quad(&self, _ctx: &UiContext) -> Option<(f32, f32, f32, f32, [f32; 4])>{ None }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let col = self.color();
        if col[3] > 0.0 {
            let (x, y, w, h) = self.rect();
            vec![(x, y, w, h, col)]
        } else {
            vec![]
        }
    }

    fn color(&self) -> [f32; 4] {
        let theme = colors::active_theme();
        if self.selected {
            let mut base_color = colors::highlight_primary_color();
            if self.pressed { base_color[3] = (base_color[3] + theme.press_overlay[3]).min(1.0); }
            else if self.base.hovered { base_color[3] = (base_color[3] + theme.hover_overlay[3]).min(1.0); }
            base_color
        } else {
            if self.pressed {
                let mut base_color = theme.surface_bg;
                base_color[3] = (base_color[3] + theme.press_overlay[3]).min(1.0);
                base_color
            } else if self.base.hovered {
                let mut base_color = theme.surface_bg;
                base_color[3] = (base_color[3] + theme.hover_overlay[3]).min(1.0);
                base_color
            } else {
                [0.0, 0.0, 0.0, 0.0]
            }
        }
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if button != MouseButton::Left { return false; }
        match state {
            ElementState::Pressed => {
                if self.hit_test(px, py, ctx) {
                    self.pressed = true;
                    return true;
                }
            }
            ElementState::Released => {
                if self.pressed && self.hit_test(px, py, ctx) {
                    self.just_clicked = true;
                }
                let was = self.pressed;
                self.pressed = false;
                return was;
            }
        }
        false
    }

    fn take_click(&mut self) -> bool {
        if self.just_clicked { self.just_clicked = false; true } else { false }
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let (x, y, _w, h) = self.rect();
        let mut labels = Vec::new();
        
        let title_y = if self.subtitle.is_some() {
            crate::layout::align_text_y(y, h, 22.0, 0.0)
        } else {
            crate::layout::align_text_y(y, h, 12.0, 0.0)
        };

        labels.push(TextLabel {
            text: self.title.clone(),
            x: x + 8.0,
            y: title_y,
            font_size: 12.0,
            color: [220, 220, 230],
        });

        if let Some(ref sub) = self.subtitle {
            labels.push(TextLabel {
                text: sub.clone(),
                x: x + 8.0,
                y: title_y + 13.0,
                font_size: 10.0,
                color: [140, 140, 153],
            });
        }

        labels
    }
}
