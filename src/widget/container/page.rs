use crate::widget::*;
use crate::context::UiContext;
use crate::widget::display::TextLabel;
use super::layer::Layer;

pub trait PageLayout {
    fn layout(&self, x: f32, y: f32, w: f32, h: f32, children: &[*mut (dyn Element + 'static)]);
}

#[derive(Debug, Clone, Copy)]
pub struct VerticalLayout {
    pub padding_x: f32,
    pub padding_y: f32,
    pub spacing: f32,
}

impl Default for VerticalLayout {
    fn default() -> Self {
        Self {
            padding_x: 8.0,
            padding_y: 10.0,
            spacing: 8.0,
        }
    }
}

impl PageLayout for VerticalLayout {
    fn layout(&self, x: f32, y: f32, w: f32, _h: f32, children: &[*mut (dyn Element + 'static)]) {
        let padding_x = self.padding_x;
        let padding_y = self.padding_y;
        let left_x = x + padding_x;
        let available_w = (w - 2.0 * padding_x).max(1.0);
        let mut current_y = y + padding_y;
        let spacing = self.spacing;

        for &child_ptr in children {
            let child = unsafe { &mut *child_ptr };
            let (_, _, _, ch) = child.rect();
            let use_h = if ch > 0.0 { ch } else { 42.0 };
            child.set_rect(left_x, current_y, available_w, use_h);
            current_y += use_h + spacing;
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ColumnsLayout {
    pub padding_x: f32,
    pub padding_y: f32,
    pub spacing: f32,
}

impl Default for ColumnsLayout {
    fn default() -> Self {
        Self {
            padding_x: 8.0,
            padding_y: 10.0,
            spacing: 12.0,
        }
    }
}

impl PageLayout for ColumnsLayout {
    fn layout(&self, x: f32, y: f32, w: f32, h: f32, children: &[*mut (dyn Element + 'static)]) {
        let count = children.len();
        if count == 0 {
            return;
        }
        let total_spacing = self.spacing * (count - 1) as f32;
        let total_padding = self.padding_x * 2.0;
        let available_w = (w - total_padding - total_spacing).max(1.0);
        let col_w = available_w / count as f32;
        let use_h = (h - 2.0 * self.padding_y).max(1.0);
        let start_y = y + self.padding_y;

        let mut current_x = x + self.padding_x;
        for &child_ptr in children {
            let child = unsafe { &mut *child_ptr };
            child.set_rect(current_x, start_y, col_w, use_h);
            current_x += col_w + self.spacing;
        }
    }
}

pub struct Page {
    pub base: Layer,
    pub visible: bool,
    pub owned_children: Vec<Box<dyn Element>>,
    pub layout: Box<dyn PageLayout>,
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
            layout: Box::new(VerticalLayout::default()),
        }
    }

    pub fn with_layout(mut self, layout: Box<dyn PageLayout>) -> Self {
        self.layout = layout;
        self
    }

    pub fn add_child_owned(&mut self, child: Box<dyn Element>, ctx: &mut UiContext) {
        let ptr = &*child as *const (dyn Element + 'static) as *mut (dyn Element + 'static);
        self.owned_children.push(child);
        self.add_child(ptr, ctx);
    }
}

impl Element for Page {
    fn base(&self) -> Option<&Widget> { Some(&self.base.base) }
    fn is_page(&self) -> bool { true }

    fn base_mut(&mut self) -> Option<&mut Widget> { Some(&mut self.base.base) }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn as_ptr(&self) -> *mut (dyn Element + 'static) {
        self as *const Self as *mut Self as *mut (dyn Element + 'static)
    }
    fn as_ptr_mut(&mut self) -> *mut (dyn Element + 'static) {
        self as *mut Self as *mut (dyn Element + 'static)
    }

    fn rect(&self) -> (f32, f32, f32, f32) {
        self.base.rect()
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.base.set_rect(x, y, w, h);
        if !self.visible {
            return;
        }
        self.layout.layout(x, y, w, h, &self.base.children);
    }

    fn color(&self) -> [f32; 4] {
        let mut c = crate::colors::page_color();
        c[3] *= crate::layout::page_opacity();
        c
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
        let mut quads = Vec::new();
        let c = self.color();
        if c[3] > 0.0 {
            let (x, y, w, h) = self.rect();
            quads.push((x, y, w, h, c));
        }
        quads.extend(self.base.all_quads(ctx));
        quads
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
