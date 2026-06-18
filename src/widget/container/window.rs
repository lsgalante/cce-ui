use crate::widget::*;
use crate::context::UiContext;
use crate::widget::display::TextLabel;

#[derive(Debug, Clone)]
pub struct Window {
    pub base: Layer,
    pub border_color: Option<[f32; 4]>,
    pub border_thickness: f32,
    pub radius: f32,
    pub background_color: Option<[f32; 4]>,
    pub visible: bool,
}

impl Window {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            base: Layer::new(x, y, w, h),
            border_color: None,
            border_thickness: 1.0,
            radius: 12.0,
            background_color: None,
            visible: true,
        }
    }

    pub fn with_border(mut self, color: [f32; 4], thickness: f32) -> Self {
        self.border_color = Some(color);
        self.border_thickness = thickness;
        self
    }

    pub fn with_background(mut self, color: [f32; 4]) -> Self {
        self.background_color = Some(color);
        self
    }

    pub fn with_radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }
}

impl Element for Window {
    fn base(&self) -> Option<&Widget> {
        Some(&self.base.base)
    }

    fn base_mut(&mut self) -> Option<&mut Widget> {
        Some(&mut self.base.base)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

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
    }

    fn color(&self) -> [f32; 4] {
        self.background_color.unwrap_or([0.0, 0.0, 0.0, 0.0])
    }

    fn visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
        self.base.visible = visible;
    }

    fn add_child(&mut self, child: *mut (dyn Element + 'static), ctx: &mut UiContext) {
        let id = self.base.base.id();
        let self_ptr = self.as_ptr();
        self.base.children.push(child);
        unsafe {
            if let Some(c_id) = (*child).base().map(|b| b.id()) {
                ctx.register_widget(c_id, child);
                ctx.link_ids(id, c_id);
                (*child).set_parent(Some(self_ptr), ctx);
            }
        }
    }

    fn clear_children(&mut self, ctx: &mut UiContext) {
        self.base.clear_children(ctx);
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

    fn set_modifiers(&mut self, ctrl: bool, shift: bool, alt: bool) {
        for &child_ptr in &self.base.children {
            unsafe {
                (*child_ptr).set_modifiers(ctrl, shift, alt);
            }
        }
    }

    fn solid_border(&self) -> Option<([f32; 4], f32)> {
        self.border_color.map(|c| (c, self.border_thickness))
    }

    fn corner_radius(&self) -> f32 {
        self.radius
    }

    fn rounded_corners(&self) -> (bool, bool, bool, bool) {
        if self.radius > 0.1 {
            (true, true, true, true)
        } else {
            (false, false, false, false)
        }
    }

    fn all_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.visible {
            return Vec::new();
        }
        let mut quads = Vec::new();
        let bg_color = self.color();
        if bg_color[3] != 0.0 {
            let (wx, wy, ww, wh) = self.rect();
            quads.push((wx, wy, ww, wh, bg_color));
        }
        quads.extend(self.base.all_quads(ctx));
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        self.base.text_labels()
    }

    fn text_labels_with_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<[f32; 4]>)> {
        self.base.text_labels_with_bounds(ctx)
    }

    fn text_labels_with_font_and_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        self.base.text_labels_with_font_and_bounds(ctx)
    }

    fn get_text_items(&self) -> Vec<(&glyphon::Buffer, f32, f32, glyphon::Color)> {
        self.base.get_text_items()
    }

    fn hit_test(&self, px: f32, py: f32, ctx: &UiContext) -> bool {
        if ctx.is_coordinate_covered(self as *const Self as *const () as usize, px, py) {
            return false;
        }
        let (x, y, w, h) = self.rect();
        if px < x || px >= x + w || py < y || py >= y + h {
            return false;
        }
        let r = self.radius.min(w * 0.5).min(h * 0.5);
        if r <= 0.1 {
            return true;
        }
        if px < x + r && py < y + r {
            let dx = px - (x + r);
            let dy = py - (y + r);
            return dx * dx + dy * dy <= r * r;
        }
        if px >= x + w - r && py < y + r {
            let dx = px - (x + w - r);
            let dy = py - (y + r);
            return dx * dx + dy * dy <= r * r;
        }
        if px >= x + w - r && py >= y + h - r {
            let dx = px - (x + w - r);
            let dy = py - (y + h - r);
            return dx * dx + dy * dy <= r * r;
        }
        if px < x + r && py >= y + h - r {
            let dx = px - (x + r);
            let dy = py - (y + h - r);
            return dx * dx + dy * dy <= r * r;
        }
        true
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        clear_widget_references(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_creation_and_builders() {
        let win = Window::new(10.0, 20.0, 100.0, 200.0)
            .with_background([0.1, 0.2, 0.3, 0.4])
            .with_border([1.0, 0.0, 0.0, 1.0], 2.5)
            .with_radius(8.0);

        assert_eq!(win.rect(), (10.0, 20.0, 100.0, 200.0));
        assert_eq!(win.color(), [0.1, 0.2, 0.3, 0.4]);
        assert_eq!(win.solid_border(), Some(([1.0, 0.0, 0.0, 1.0], 2.5)));
        assert_eq!(win.corner_radius(), 8.0);
        assert_eq!(win.rounded_corners(), (true, true, true, true));
    }

    #[test]
    fn test_window_visibility() {
        let mut win = Window::new(0.0, 0.0, 100.0, 100.0);
        assert!(win.visible());
        assert!(win.base.visible);

        win.set_visible(false);
        assert!(!win.visible());
        assert!(!win.base.visible);
    }

    #[test]
    fn test_window_children_and_parent() {
        let mut ctx = UiContext::new();
        let mut win = Window::new(0.0, 0.0, 100.0, 100.0);
        let child = Layer::new(10.0, 10.0, 50.0, 50.0);

        assert_eq!(win.children(&ctx).len(), 0);

        win.add_child(child.as_ptr(), &mut ctx);
        assert_eq!(win.children(&ctx).len(), 1);
        assert_eq!(unsafe { (*win.children(&ctx)[0]).rect() }, (10.0, 10.0, 50.0, 50.0));

        // Child's parent should point to Window
        assert_eq!(unsafe { (*child.as_ptr()).parent(&ctx) }, Some(win.as_ptr()));

        win.clear_children(&mut ctx);
        assert_eq!(win.children(&ctx).len(), 0);
    }
}
