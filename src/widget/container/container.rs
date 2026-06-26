use crate::widget::*;
use super::container_layout::{ContainerLayout, OverlayLayout};

#[derive(Clone)]
pub struct Container {
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub children: Vec<*mut (dyn Element + 'static)>,
    pub base: Widget,
    pub layout: Box<dyn ContainerLayout>,
}

impl Container {
    pub fn new() -> Self {
        Self {
            parent: None,
            children: Vec::new(),
            base: Widget::new(),
            layout: Box::new(OverlayLayout),
        }
    }

    pub fn with_layout<L: ContainerLayout + 'static>(mut self, layout: L) -> Self {
        self.layout = Box::new(layout);
        self
    }
}

impl Element for Container {
    fn base(&self) -> Option<&Widget> { Some(&self.base) }
    fn blocks_backplate_drag(&self) -> bool { false }
    fn base_mut(&mut self) -> Option<&mut Widget> { Some(&mut self.base) }
    fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }
    fn as_ptr(&self) -> *mut (dyn Element + 'static) {
        self as *const Self as *mut Self as *mut (dyn Element + 'static)
    }
    fn as_ptr_mut(&mut self) -> *mut (dyn Element + 'static) {
        self as *mut Self as *mut (dyn Element + 'static)
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let dx = x - self.base.x;
        let dy = y - self.base.y;

        self.base.x = x;
        self.base.y = y;
        self.base.w = w;
        self.base.h = h;

        if dx.abs() > 0.001 || dy.abs() > 0.001 {
            for &child in &self.children {
                unsafe {
                    let (cx, cy, cw, ch) = (*child).rect();
                    (*child).set_rect(cx + dx, cy + dy, cw, ch);
                }
            }
        }
    }

    fn measure(&self, constraints: LayoutConstraints, ctx: &UiContext) -> Size {
        self.layout.measure(constraints, &self.children, ctx)
    }

    fn layout(&mut self, origin: Point, constraints: LayoutConstraints, ctx: &mut UiContext) {
        let size = self.measure(constraints, ctx);
        self.set_rect(origin.x, origin.y, size.width, size.height);
        self.layout.layout(origin.x, origin.y, size.width, size.height, &self.children, ctx);
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
