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
    fn highlight_quad(&self, ctx: &UiContext) -> Option<(f32, f32, f32, f32, [f32; 4])>{ None }

    fn color(&self) -> [f32; 4] { colors::PROGRESS_BG }
}
