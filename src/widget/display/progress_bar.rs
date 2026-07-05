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

    fn color(&self) -> [f32; 4] { colors::progress_bg() }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let top = self.base.label_offset();
        let visual_h = self.base.h - top;
        let fill_w = self.base.w * self._value.clamp(0.0, 1.0);
        vec![(self.base.x, self.base.y + top, fill_w, visual_h, colors::progress_fill())]
    }
}
