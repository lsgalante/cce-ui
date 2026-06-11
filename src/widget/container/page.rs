use crate::widget::*;
use crate::context::UiContext;
use crate::widget::display::TextLabel;
use super::layer::Layer;

pub struct Page {
    pub base: Layer,
    pub visible: bool,
    pub owned_children: Vec<Box<dyn Element>>,
}

impl std::fmt::Debug for Page {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Page")
            .field("base", &self.base)
            .field("visible", &self.visible)
            .finish()
    }
}

impl Page {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            base: Layer::new(x, y, w, h),
            visible: true,
            owned_children: Vec::new(),
        }
    }

    pub fn add_child_owned(&mut self, child: Box<dyn Element>, ctx: &mut UiContext) {
        let ptr = &*child as *const (dyn Element + 'static) as *mut (dyn Element + 'static);
        self.owned_children.push(child);
        self.add_child(ptr, ctx);
    }
}

impl Element for Page {
    fn base(&self) -> Option<&Widget> { Some(&self.base.base) }
    fn base_mut(&mut self) -> Option<&mut Widget> { Some(&mut self.base.base) }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn as_ptr(&self) -> *mut (dyn Element + 'static) {
        self as *const Self as *mut Self as *mut (dyn Element + 'static)
    }

    fn rect(&self) -> (f32, f32, f32, f32) {
        self.base.rect()
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.base.set_rect(x, y, w, h);
        if !self.visible {
            return;
        }
        let padding_x = 8.0;
        let padding_y = 10.0;
        let left_x = x + padding_x;
        let available_w = (w - 2.0 * padding_x).max(1.0);
        let mut current_y = y + padding_y;
        let spacing = 8.0;

        for &child_ptr in &self.base.children {
            let child = unsafe { &mut *child_ptr };
            let (_, _, _, ch) = child.rect();
            let use_h = if ch > 0.0 { ch } else { 42.0 };
            child.set_rect(left_x, current_y, available_w, use_h);
            current_y += use_h + spacing;
        }
    }

    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn add_child(&mut self, child: *mut (dyn Element + 'static), ctx: &mut UiContext) {
        let id = self.base.base.id();
        let self_ptr = self as *mut Page as *mut (dyn Element + 'static);
        self.base.children.push(child);
        unsafe {
            let c_id = (*child).base().map(|b| b.id()).unwrap();
            ctx.register_widget(c_id, child);
            ctx.link_ids(id, c_id);
            (*child).set_parent(Some(self_ptr), ctx);
        }
    }

    fn clear_children(&mut self, ctx: &mut UiContext) {
        self.base.clear_children(ctx);
        self.owned_children.clear();
    }

    fn children(&self, ctx: &UiContext) -> Vec<*mut (dyn Element + 'static)> {
        self.base.children(ctx)
    }

    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>, ctx: &mut UiContext) {
        self.base.set_parent(parent, ctx);
    }

    fn parent(&self, ctx: &UiContext) -> Option<*mut (dyn Element + 'static)> {
        self.base.parent(ctx)
    }

    fn all_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.visible {
            return Vec::new();
        }
        self.base.all_quads(ctx)
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        if !self.visible {
            return Vec::new();
        }
        self.base.text_labels()
    }

    fn text_labels_with_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<[f32; 4]>)> {
        if !self.visible {
            return Vec::new();
        }
        self.base.text_labels_with_bounds(ctx)
    }

    fn text_labels_with_font_and_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        if !self.visible {
            return Vec::new();
        }
        self.base.text_labels_with_font_and_bounds(ctx)
    }

    fn get_text_items(&self) -> Vec<(&glyphon::Buffer, f32, f32, glyphon::Color)> {
        if !self.visible {
            return Vec::new();
        }
        self.base.get_text_items()
    }

    fn hit_test(&self, px: f32, py: f32, ctx: &UiContext) -> bool {
        if !self.visible {
            return false;
        }
        self.base.hit_test(px, py, ctx)
    }
}
