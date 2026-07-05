use crate::colors;
use crate::widget::*;

pub struct ProgressBar {
    base: Widget,
    _value: f32,
}

impl ProgressBar {
    pub fn new(value: f32) -> Self {
        Self {
            base: Widget::new(),
            _value: value,
        }
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self
    }
}

impl Element for ProgressBar {
    crate::impl_widget_base!(ProgressBar);

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.base.x = x;
        self.base.y = y;
        self.base.w = w;
        self.base.h = h + self.base.label_offset();
    }

    fn highlight_quad(&self, _ctx: &UiContext) -> Option<(f32, f32, f32, f32, [f32; 4])>{ None }

    fn color(&self) -> [f32; 4] { colors::PROGRESS_BG }
}
