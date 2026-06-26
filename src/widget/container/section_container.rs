use crate::widget::*;
use crate::context::UiContext;
use crate::widget::display::TextLabel;
use super::container::Container;
use super::container_layout::ContainerLayout;

#[derive(Clone)]
pub struct SectionContainer {
    pub header: SectionHeader,
    pub container: Container,
    pub base: Widget,
    pub parent: Option<*mut (dyn Element + 'static)>,
}

impl SectionContainer {
    pub fn new(title: &str) -> Self {
        Self {
            header: SectionHeader::new(title),
            container: Container::new(),
            base: Widget::new(),
            parent: None,
        }
    }

    pub fn with_layout<L: ContainerLayout + 'static>(mut self, layout: L) -> Self {
        let container = std::mem::replace(&mut self.container, Container::new());
        self.container = container.with_layout(layout);
        self
    }
}

impl Default for SectionContainer {
    fn default() -> Self {
        Self::new("")
    }
}

impl Element for SectionContainer {
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
        self.base.x = x;
        self.base.y = y;
        self.base.w = w;
        self.base.h = h;

        self.header.set_rect(x, y, w, 24.0);
        self.container.set_rect(x, y + 28.0, w, (h - 28.0).max(0.0));
    }

    fn measure(&self, constraints: LayoutConstraints, ctx: &UiContext) -> Size {
        let remaining_constraints = LayoutConstraints::new(
            constraints.min_width,
            constraints.max_width,
            (constraints.min_height - 28.0).max(0.0),
            (constraints.max_height - 28.0).max(0.0),
        );
        let container_size = self.container.measure(remaining_constraints, ctx);
        Size {
            width: container_size.width,
            height: container_size.height + 28.0,
        }
    }

    fn layout(&mut self, origin: Point, constraints: LayoutConstraints, ctx: &mut UiContext) {
        let size = self.measure(constraints, ctx);
        self.set_rect(origin.x, origin.y, size.width, size.height);
        
        let self_ptr = self.as_ptr_mut();
        self.header.set_parent(Some(self_ptr), ctx);
        self.container.set_parent(Some(self_ptr), ctx);

        self.header.layout(origin, LayoutConstraints::new(size.width, size.width, 24.0, 24.0), ctx);
        self.container.layout(
            Point { x: origin.x, y: origin.y + 28.0 },
            LayoutConstraints::new(size.width, size.width, (size.height - 28.0).max(0.0), (size.height - 28.0).max(0.0)),
            ctx,
        );
    }

    fn parent(&self, _ctx: &UiContext) -> Option<*mut (dyn Element + 'static)> { self.parent }
    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>, _ctx: &mut UiContext) { self.parent = parent; }
    
    fn children(&self, _ctx: &UiContext) -> Vec<*mut (dyn Element + 'static)> {
        vec![
            &self.header as *const SectionHeader as *mut SectionHeader as *mut (dyn Element + 'static),
            &self.container as *const Container as *mut Container as *mut (dyn Element + 'static),
        ]
    }
    
    fn add_child(&mut self, child: *mut (dyn Element + 'static), ctx: &mut UiContext) {
        self.container.add_child(child, ctx);
        let id = self.base.id();
        unsafe {
            let c_id = (*child).base().map(|b| b.id()).unwrap();
            ctx.register_widget(c_id, child);
            ctx.link_ids(id, c_id);
            (*child).set_parent(Some(self.container.as_ptr_mut()), ctx);
        }
    }
    
    fn clear_children(&mut self, ctx: &mut UiContext) {
        self.container.clear_children(ctx);
        ctx.clear_children_ids(self.base.id());
    }

    fn all_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        self.container.all_quads(ctx)
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        self.container.text_labels()
    }

    fn text_labels_with_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<[f32; 4]>)> {
        self.container.text_labels_with_bounds(ctx)
    }

    fn text_labels_with_font_and_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        self.container.text_labels_with_font_and_bounds(ctx)
    }

    fn get_text_items(&self) -> Vec<(&glyphon::Buffer, f32, f32, glyphon::Color)> {
        self.container.get_text_items()
    }

    fn hit_test(&self, px: f32, py: f32, ctx: &UiContext) -> bool {
        self.header.hit_test(px, py, ctx) || self.container.hit_test(px, py, ctx)
    }
}

impl Drop for SectionContainer {
    fn drop(&mut self) {
        clear_widget_references(self);
    }
}
