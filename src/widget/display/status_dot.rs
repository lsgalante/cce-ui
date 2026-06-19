use crate::widget::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DotStatus {
    Active,
    Inactive,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct StatusDot {
    base: Widget,
    pub status: DotStatus,
}

impl StatusDot {
    pub fn new(status: DotStatus) -> Self {
        Self {
            base: Widget::new(),
            status,
        }
    }

    pub fn set_status(&mut self, status: DotStatus) {
        self.status = status;
    }
}

impl Element for StatusDot {
    crate::impl_widget_base!(StatusDot);
    fn highlight_quad(&self, _ctx: &UiContext) -> Option<(f32, f32, f32, f32, [f32; 4])>{ None }

    fn color(&self) -> [f32; 4] {
        match self.status {
            DotStatus::Active => [0.20, 0.70, 0.35, 1.0],
            DotStatus::Inactive => [0.50, 0.50, 0.55, 1.0],
            DotStatus::Warning => [0.90, 0.60, 0.10, 1.0],
            DotStatus::Error => [0.85, 0.25, 0.25, 1.0],
        }
    }
}
