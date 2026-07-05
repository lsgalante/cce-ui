use crate::widget::*;

pub struct VBox {
    pub base: Widget,
    pub children: Vec<*mut (dyn Element + 'static)>,
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub gap: f32,
    pub margin: f32,
}

impl VBox {
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

impl Element for VBox {
    crate::impl_widget_base!(VBox);

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

        let mut current_y = y + self.margin;
        let child_w = w - 2.0 * self.margin;

        for &child_ptr in &self.children {
            unsafe {
                let preferred_h = (*child_ptr).preferred_height().unwrap_or(24.0);
                let label_off = (*child_ptr).base().map_or(0.0, |b| b.label_offset());
                let total_h = preferred_h + label_off;
                
                (*child_ptr).set_rect(x + self.margin, current_y, child_w, preferred_h);
                current_y += total_h + self.gap;
            }
        }
    }
}
