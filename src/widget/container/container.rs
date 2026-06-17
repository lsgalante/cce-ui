use crate::widget::*;

#[derive(Clone)]
pub struct Container {
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub children: Vec<*mut (dyn Element + 'static)>,
}

impl Container {
    pub fn new() -> Self {
        Self { parent: None, children: Vec::new() }
    }
}

impl Element for Container {
    fn rect(&self) -> (f32, f32, f32, f32) { (0.0, 0.0, 0.0, 0.0) }
    fn set_rect(&mut self, _x: f32, _y: f32, _w: f32, _h: f32) {}
    fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }

    fn focus(&mut self) {
        focus::set_focused(self);
    }
    fn unfocus(&mut self) {}

    fn parent(&self, ctx: &UiContext) -> Option<*mut (dyn Element + 'static)> { self.parent }
    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>, ctx: &mut UiContext) { self.parent = parent; }
    fn children(&self, ctx: &UiContext) -> Vec<*mut (dyn Element + 'static)> { self.children.clone() }
    fn add_child(&mut self, child: *mut (dyn Element + 'static), ctx: &mut UiContext) { self.children.push(child); }
    fn clear_children(&mut self, ctx: &mut UiContext) { self.children.clear(); }
}

impl Drop for Container {
    fn drop(&mut self) {
        clear_widget_references(self);
    }
}
