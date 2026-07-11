use crate::widget::*;
use crate::context::UiContext;


#[derive(Debug, Clone)]
pub struct Backplate {
    pub base: Widget,
    pub children: Vec<*mut (dyn Element + 'static)>,
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub border_color: Option<[f32; 4]>,
    pub border_thickness: f32,
    pub radius: f32,
    pub background_color: Option<[f32; 4]>,
    pub visible: bool,
    pub movable: bool,
    pub bevel: bool,
    pub bevel_thickness: f32,
}

impl Backplate {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            base: Widget::new_rect(x, y, w, h),
            children: Vec::new(),
            parent: None,
            border_color: None,
            border_thickness: 1.0,
            radius: -1.0,
            background_color: None,
            visible: true,
            movable: true,
            bevel: false,
            bevel_thickness: 1.5,
        }
    }

    pub fn with_movable(mut self, movable: bool) -> Self {
        self.movable = movable;
        self
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

    pub fn with_bevel(mut self, bevel: bool, thickness: f32) -> Self {
        self.bevel = bevel;
        self.bevel_thickness = thickness;
        self
    }
}

impl Element for Backplate {
    fn base(&self) -> Option<&Widget> {
        Some(&self.base)
    }

    fn base_mut(&mut self) -> Option<&mut Widget> {
        Some(&mut self.base)
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
        (self.base.x, self.base.y, self.base.w, self.base.h)
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.base.x = x;
        self.base.y = y;
        self.base.w = w;
        self.base.h = h;
    }

    fn color(&self) -> [f32; 4] {
        let mut base_color = self.background_color.unwrap_or_else(|| crate::color::page_low_color());
        if base_color[3] > 0.001 {
            base_color[3] = crate::color::active_backplate_opacity();
        }
        base_color
    }

    fn visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn add_child(&mut self, child: *mut (dyn Element + 'static), ctx: &mut UiContext) {
        let id = self.base.id();
        let self_ptr = self.as_ptr_mut();
        self.children.push(child);
        unsafe {
            if let Some(c_id) = (*child).base().map(|b| b.id()) {
                ctx.register_widget(c_id, child);
                ctx.link_ids(id, c_id);
                (*child).set_parent(Some(self_ptr), ctx);
            }
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

    fn set_modifiers(&mut self, ctrl: bool, shift: bool, alt: bool) {
        for &child_ptr in &self.children {
            unsafe {
                (*child_ptr).set_modifiers(ctrl, shift, alt);
            }
        }
    }

    fn solid_border(&self) -> Option<([f32; 4], f32)> {
        self.border_color.map(|c| (c, self.border_thickness))
    }

    fn plate_bevel(&self) -> Option<f32> {
        if self.bevel {
            Some(self.bevel_thickness)
        } else {
            None
        }
    }

    fn corner_radius(&self) -> f32 {
        if self.radius < 0.0 {
            crate::color::backplate_corner_radius()
        } else {
            self.radius
        }
    }

    fn rounded_corners(&self) -> (bool, bool, bool, bool) {
        if self.corner_radius() > 0.1 {
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
        let (wx, wy, ww, wh) = self.rect();
        let has_rounded = self.rounded_corners() != (false, false, false, false);
        if bg_color[3] != 0.0 && !has_rounded {
            quads.push((wx, wy, ww, wh, bg_color));
        }
        for &child_ptr in &self.children {
            let widget = unsafe { &*child_ptr };
            let mut child_quads = Vec::new();
            let (cx, cy, cw, ch) = widget.rect();
            let has_rounded = widget.rounded_corners() != (false, false, false, false);
            for (qx, qy, qw, qh, qc) in widget.all_quads(ctx) {
                if has_rounded && (qx - cx).abs() < 0.1 && (qy - cy).abs() < 0.1 && (qw - cw).abs() < 0.1 && (qh - ch).abs() < 0.1 {
                    continue;
                }
                child_quads.push((qx, qy, qw, qh, qc));
            }

            for (qx, qy, qw, qh, qc) in child_quads {
                let x0 = qx.max(wx);
                let y0 = qy.max(wy);
                let x1 = (qx + qw).min(wx + ww);
                let y1 = (qy + qh).min(wy + wh);
                if x1 > x0 && y1 > y0 {
                    quads.push((x0, y0, x1 - x0, y1 - y0, qc));
                }
            }
        }
        quads
    }
    
    fn all_rounded_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, f32, [f32; 4], (bool, bool, bool, bool))> {
        if !self.visible {
            return Vec::new();
        }
        let mut quads = Vec::new();
        let (wx, wy, ww, wh) = self.rect();
        let radius = self.corner_radius();
        let (r1, r2, r3, r4) = self.rounded_corners();
        if r1 || r2 || r3 || r4 {
            let bg_color = self.color();
            if bg_color[3] > 0.001 {
                quads.push((wx, wy, ww, wh, radius, bg_color, (r1, r2, r3, r4)));
            }
        }
        for &child_ptr in &self.children {
            let widget = unsafe { &*child_ptr };
            let mut child_quads = Vec::new();
            for (qx, qy, qw, qh, qr, qc, (cr1, cr2, cr3, cr4)) in widget.all_rounded_quads(ctx) {
                child_quads.push((qx, qy, qw, qh, qr, qc, (cr1, cr2, cr3, cr4)));
            }

            for (qx, qy, qw, qh, qr, qc, (cr1, cr2, cr3, cr4)) in child_quads {
                let x0 = qx.max(wx);
                let y0 = qy.max(wy);
                let x1 = (qx + qw).min(wx + ww);
                let y1 = (qy + qh).min(wy + wh);
                if x1 > x0 && y1 > y0 {
                    quads.push((x0, y0, x1 - x0, y1 - y0, qr, qc, (cr1, cr2, cr3, cr4)));
                }
            }
        }
        quads
    }

    fn hit_test(&self, px: f32, py: f32, ctx: &UiContext) -> bool {
        if ctx.is_coordinate_covered(self as *const Self as *const () as usize, px, py) {
            return false;
        }
        let (x, y, w, h) = self.rect();
        if px < x || px >= x + w || py < y || py >= y + h {
            return false;
        }
        let r = self.corner_radius().min(w * 0.5).min(h * 0.5);
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

    fn is_backplate(&self) -> bool {
        true
    }

    fn is_movable_backplate(&self) -> bool {
        self.movable
    }
}

impl Drop for Backplate {
    fn drop(&mut self) {
        clear_widget_references(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backplate_creation_and_builders() {
        let win = Backplate::new(10.0, 20.0, 100.0, 200.0)
            .with_background([0.1, 0.2, 0.3, 0.4])
            .with_border([1.0, 0.0, 0.0, 1.0], 2.5)
            .with_radius(8.0);

        assert_eq!(win.rect(), (10.0, 20.0, 100.0, 200.0));
        assert_eq!(win.background_color, Some([0.1, 0.2, 0.3, 0.4]));
        assert_eq!(win.solid_border(), Some(([1.0, 0.0, 0.0, 1.0], 2.5)));
        assert_eq!(win.corner_radius(), 8.0);
        assert_eq!(win.rounded_corners(), (true, true, true, true));
        assert!(win.is_movable_backplate());

        let non_movable_win = win.with_movable(false);
        assert!(!non_movable_win.is_movable_backplate());
    }

    #[test]
    fn test_backplate_visibility() {
        let mut win = Backplate::new(0.0, 0.0, 100.0, 100.0);
        assert!(win.visible());
        assert!(win.visible);

        win.set_visible(false);
        assert!(!win.visible());
        assert!(!win.visible);
    }

    #[test]
    fn test_backplate_children_and_parent() {
        let mut ctx = UiContext::new();
        let mut win = Backplate::new(0.0, 0.0, 100.0, 100.0);
        let child = Layer::new(10.0, 10.0, 50.0, 50.0);

        assert_eq!(win.children(&ctx).len(), 0);

        win.add_child(child.as_ptr(), &mut ctx);
        assert_eq!(win.children(&ctx).len(), 1);
        assert_eq!(unsafe { (*win.children(&ctx)[0]).rect() }, (10.0, 10.0, 50.0, 50.0));

        // Child's parent should point to Backplate
        assert_eq!(unsafe { (*child.as_ptr()).parent(&ctx) }, Some(win.as_ptr()));

        win.clear_children(&mut ctx);
        assert_eq!(win.children(&ctx).len(), 0);
    }

    struct TestRenderTarget;
    impl crate::layout::RenderTarget for TestRenderTarget {
        fn rect(&mut self, _color: [f32; 4], _x: f32, _y: f32, _w: f32, _h: f32) {}
        fn text(&mut self, _content: &str, _x: f32, _y: f32, _size: f32, _color: [f32; 4]) {}
    }

    #[test]
    fn test_paginator_blocks_backplate_drag() {
        let mut ctx = UiContext::new();
        let mut win = Backplate::new(0.0, 0.0, 800.0, 600.0);
        let mut paginator = Paginator::new(vec!["Page 1".to_string(), "Page 2".to_string()]);
        
        win.add_child(paginator.as_ptr_mut(), &mut ctx);
        
        // Let's set the rect
        crate::layout::render_widget(&mut TestRenderTarget, &mut paginator, 0.0, 0.0, 54.0, 600.0, &mut ctx);
        
        // Let's tick to register children
        ctx.tick(0.016);
        ctx.clear_dirty(); // This triggers rebuild_spatial_grid()
        
        // Now, click in the sidebar at x=10, y=20
        let is_movable = ctx.is_movable_backplate_at(10.0, 20.0);
        assert!(!is_movable, "Clicking the sidebar should block backplate drag!");
    }

    #[test]
    fn test_menubar_statusbar_do_not_block_backplate_drag() {
        let mut ctx = UiContext::new();
        let mut win = Backplate::new(0.0, 0.0, 800.0, 600.0).with_movable(true);
        let mut menubar = MenuBar::new(0.0, 0.0, 800.0, 42.0);
        let mut statusbar = StatusBar::new();
        statusbar.set_rect(0.0, 570.0, 800.0, 30.0);

        ctx.register_widget(win.base().unwrap().id(), win.as_ptr_mut());
        win.add_child(menubar.as_ptr_mut(), &mut ctx);
        win.add_child(statusbar.as_ptr_mut(), &mut ctx);

        // Render to assign coordinates and register with layout hierarchy
        crate::layout::render_widget(&mut TestRenderTarget, &mut menubar, 0.0, 0.0, 800.0, 42.0, &mut ctx);
        crate::layout::render_widget(&mut TestRenderTarget, &mut statusbar, 0.0, 570.0, 800.0, 30.0, &mut ctx);

        ctx.tick(0.016);
        ctx.clear_dirty(); // Triggers rebuild_spatial_grid()

        // Check if clicking menubar allows dragging
        let drag_menubar = ctx.is_movable_backplate_at(100.0, 20.0);
        assert!(drag_menubar, "Clicking the menubar should not block backplate drag!");

        // Check if clicking statusbar allows dragging
        let drag_statusbar = ctx.is_movable_backplate_at(100.0, 580.0);
        assert!(drag_statusbar, "Clicking the statusbar should not block backplate drag!");
    }
}
