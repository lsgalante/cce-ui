use crate::widget::*;
use crate::context::UiContext;


#[derive(Debug, Clone)]
pub struct Layer {
    pub base: Widget,
    pub children: Vec<*mut (dyn Element + 'static)>,
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub visible: bool,
    pub padding: Option<f32>,
}

impl Layer {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            base: Widget::new_rect(x, y, w, h),
            children: Vec::new(),
            parent: None,
            visible: true,
            padding: None,
        }
    }

    pub fn with_padding(mut self, padding: f32) -> Self {
        self.padding = Some(padding);
        self
    }
}

impl Element for Layer {
    fn base(&self) -> Option<&Widget> { Some(&self.base) }
    fn blocks_backplate_drag(&self) -> bool { false }

    fn base_mut(&mut self) -> Option<&mut Widget> { Some(&mut self.base) }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn as_ptr(&self) -> *mut (dyn Element + 'static) {
        self as *const Self as *mut Self as *mut (dyn Element + 'static)
    }
    fn as_ptr_mut(&mut self) -> *mut (dyn Element + 'static) {
        self as *mut Self as *mut (dyn Element + 'static)
    }

    fn rect(&self) -> (f32, f32, f32, f32) {
        (self.base.x, self.base.y, self.base.w, self.base.h)
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.base.x = x;
        self.base.y = y;
        self.base.w = w;
        self.base.h = h;
    }

    fn color(&self) -> [f32; 4] {
        let mut c = crate::colors::layer_color();
        c[3] *= crate::layout::layer_opacity();
        c
    }


    fn visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn add_child(&mut self, child: *mut (dyn Element + 'static), ctx: &mut UiContext) {
        let id = self.base.id();
        let self_ptr = self as *mut Layer as *mut (dyn Element + 'static);
        self.children.push(child);
        unsafe {
            let c_id = (*child).base().map(|b| b.id()).unwrap();
            ctx.register_widget(c_id, child);
            ctx.link_ids(id, c_id);
            (*child).set_parent(Some(self_ptr), ctx);
        }
    }

    fn clear_children(&mut self, ctx: &mut UiContext) {
        self.children.clear();
        let id = self.base.id();
        ctx.clear_children_ids(id);
    }

    fn children(&self, _ctx: &UiContext) -> Vec<*mut (dyn Element + 'static)> {
        self.children.clone()
    }

    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>, _ctx: &mut UiContext) {
        self.parent = parent;
    }

    fn parent(&self, _ctx: &UiContext) -> Option<*mut (dyn Element + 'static)> {
        self.parent
    }

    fn all_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.visible {
            return Vec::new();
        }
        let mut quads = Vec::new();
        let mut has_page_parent = false;
        if let Some(parent_ptr) = self.parent {
            unsafe {
                if (*parent_ptr).is_page() {
                    has_page_parent = true;
                }
            }
        }
        if !has_page_parent {
            let c = self.color();
            if c[3] > 0.0 {
                let (x, y, w, h) = self.rect();
                quads.push((x, y, w, h, c));
            }
        }
        for &child_ptr in &self.children {
            let widget = unsafe { &*child_ptr };
            let (wx, wy, ww, wh) = widget.rect();
            let has_rounded = widget.rounded_corners() != (false, false, false, false);
            for (qx, qy, qw, qh, qc) in widget.all_quads(ctx) {
                if has_rounded && (qx - wx).abs() < 0.1 && (qy - wy).abs() < 0.1 && (qw - ww).abs() < 0.1 && (qh - wh).abs() < 0.1 {
                    continue;
                }
                quads.push((qx, qy, qw, qh, qc));
            }
        }
        quads
    }

    fn hit_test(&self, px: f32, py: f32, ctx: &UiContext) -> bool {
        if ctx.is_coordinate_covered(self as *const Self as *const () as usize, px, py) {
            return false;
        }
        for &child_ptr in &self.children {
            let widget = unsafe { &*child_ptr };
            if widget.hit_test(px, py, ctx) {
                return true;
            }
        }
        false
    }
}
