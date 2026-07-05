use crate::widget::*;

pub struct HBox {
    pub base: Widget,
    pub children: Vec<*mut (dyn Element + 'static)>,
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub gap: f32,
    pub margin: f32,
}

impl HBox {
    pub fn new(gap: f32, margin: f32) -> Self {
        Self {
            base: Widget::new(),
            children: Vec::new(),
            parent: None,
            gap,
            margin,
        }
    }

    pub fn add_child(&mut self, child: *mut (dyn Element + 'static)) {
        self.children.push(child);
    }
}

impl Element for HBox {
    crate::impl_widget_base!(HBox);

    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn children(&self, _ctx: &UiContext) -> Vec<*mut (dyn Element + 'static)> {
        self.children.clone()
    }

    fn parent(&self, _ctx: &UiContext) -> Option<*mut (dyn Element + 'static)> {
        self.parent
    }

    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>, _ctx: &mut UiContext) {
        self.parent = parent;
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.base.x = x;
        self.base.y = y;
        self.base.w = w;
        self.base.h = h;

        let count = self.children.len();
        if count == 0 {
            return;
        }

        let total_w = w - 2.0 * self.margin;
        let child_w = (total_w - (count as f32 - 1.0) * self.gap) / count as f32;
        let child_h = h - 2.0 * self.margin;
        let mut current_x = x + self.margin;

        for &child_ptr in &self.children {
            unsafe {
                (*child_ptr).set_rect(current_x, y + self.margin, child_w, child_h);
                current_x += child_w + self.gap;
            }
        }
    }
}
