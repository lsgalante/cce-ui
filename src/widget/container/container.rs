use crate::widget::*;

#[derive(Clone)]
pub struct Container {
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub children: Vec<*mut (dyn Element + 'static)>,
    pub base: Widget,
}

impl Container {
    pub fn new() -> Self {
        Self { parent: None, children: Vec::new(), base: Widget::new() }
    }
}

impl Element for Container {
    fn base(&self) -> Option<&Widget> { Some(&self.base) }
    fn base_mut(&mut self) -> Option<&mut Widget> { Some(&mut self.base) }
    fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }
    fn as_ptr(&self) -> *mut (dyn Element + 'static) {
        self as *const Self as *mut Self as *mut (dyn Element + 'static)
    }
    fn as_ptr_mut(&mut self) -> *mut (dyn Element + 'static) {
        self as *mut Self as *mut (dyn Element + 'static)
    }

    fn measure(&self, constraints: LayoutConstraints, ctx: &UiContext) -> Size {
        let mut max_w = 0.0f32;
        let mut max_h = 0.0f32;
        for &child in &self.children {
            unsafe {
                let size = (*child).measure(constraints, ctx);
                max_w = max_w.max(size.width);
                max_h = max_h.max(size.height);
            }
        }
        Size {
            width: max_w.clamp(constraints.min_width, constraints.max_width),
            height: max_h.clamp(constraints.min_height, constraints.max_height),
        }
    }

    fn layout(&mut self, origin: Point, constraints: LayoutConstraints, ctx: &mut UiContext) {
        let size = self.measure(constraints, ctx);
        self.set_rect(origin.x, origin.y, size.width, size.height);
        for &child in &self.children {
            unsafe {
                (*child).layout(origin, constraints, ctx);
            }
        }
    }

    fn focus(&mut self) {
        focus::set_focused(self);
    }
    fn unfocus(&mut self) {}

    fn parent(&self, _ctx: &UiContext) -> Option<*mut (dyn Element + 'static)> { self.parent }
    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>, _ctx: &mut UiContext) { self.parent = parent; }
    fn children(&self, _ctx: &UiContext) -> Vec<*mut (dyn Element + 'static)> { self.children.clone() }
    fn add_child(&mut self, child: *mut (dyn Element + 'static), _ctx: &mut UiContext) { self.children.push(child); }
    fn clear_children(&mut self, _ctx: &mut UiContext) { self.children.clear(); }
}

impl Drop for Container {
    fn drop(&mut self) {
        clear_widget_references(self);
    }
}
