use crate::widget::*;

#[derive(Debug, Clone)]
pub struct UsageBar {
    base: Widget,
    pub value: f32, // 0.0 to 1.0
    pub fill_color: [f32; 4],
    pub bg_color: [f32; 4],
}

impl UsageBar {
    pub fn new(value: f32) -> Self {
        Self {
            base: Widget::new(),
            value: value.clamp(0.0, 1.0),
            fill_color: [0.30, 0.50, 0.32, 1.0], // green-ish
            bg_color: [0.15, 0.15, 0.24, 1.0], // dark-ish
        }
    }
    
    pub fn with_colors(mut self, fill: [f32; 4], bg: [f32; 4]) -> Self {
        self.fill_color = fill;
        self.bg_color = bg;
        self
    }
    
    pub fn set_value(&mut self, value: f32) {
        self.value = value.clamp(0.0, 1.0);
    }
}

impl Element for UsageBar {
    crate::impl_widget_base!(UsageBar);
    fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }
    fn highlight_quad(&self, ctx: &UiContext) -> Option<(f32, f32, f32, f32, [f32; 4])>{ None }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let (x, y, w, h) = self.rect();
        vec![
            (x, y, w, h, self.bg_color),
            (x, y, w * self.value, h, self.fill_color),
        ]
    }
}
