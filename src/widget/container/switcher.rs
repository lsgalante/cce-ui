use crate::widget::*;
use crate::widget::display::TextLabel;

#[derive(Debug, Clone)]
pub struct Switcher {
    pub base: Widget,
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub children: Vec<*mut (dyn Element + 'static)>,
    pub active_index: Option<usize>,
    pub visible: bool,
}

impl Switcher {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            base: Widget::new_rect(x, y, w, h),
            parent: None,
            children: Vec::new(),
            active_index: None,
            visible: true,
        }
    }

    pub fn set_active_index(&mut self, index: Option<usize>) {
        self.active_index = index;
        for (i, &child_ptr) in self.children.iter().enumerate() {
            unsafe {
                (*child_ptr).set_visible(self.active_index == Some(i));
            }
        }
    }

    pub fn active_index(&self) -> Option<usize> {
        self.active_index
    }
}

impl Element for Switcher {
    crate::impl_widget_base!(Switcher);
    fn blocks_backplate_drag(&self) -> bool { false }

    fn visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn color(&self) -> [f32; 4] {
        if let Some(idx) = self.active_index {
            if idx < self.children.len() {
                return unsafe { (*self.children[idx]).color() };
            }
        }
        [0.0, 0.0, 0.0, 0.0]
    }

    fn parent(&self, _ctx: &UiContext) -> Option<*mut (dyn Element + 'static)> {
        self.parent
    }

    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>, _ctx: &mut UiContext) {
        self.parent = parent;
    }

    fn children(&self, _ctx: &UiContext) -> Vec<*mut (dyn Element + 'static)> {
        self.children.clone()
    }

    fn add_child(&mut self, child: *mut (dyn Element + 'static), ctx: &mut UiContext) {
        self.children.push(child);
        let id = self.base.id();
        let self_ptr = self.as_ptr();
        if let Some(c_base) = unsafe { (*child).base() } {
            let c_id = c_base.id();
            ctx.register_widget(id, self_ptr);
            ctx.register_widget(c_id, child);
            ctx.link_ids(id, c_id);
        }
        unsafe {
            (*child).set_parent(Some(self_ptr), ctx);
        }
        // Sync visibility of newly added child
        let idx = self.children.len() - 1;
        unsafe {
            (*child).set_visible(self.active_index == Some(idx));
        }
    }

    fn clear_children(&mut self, ctx: &mut UiContext) {
        self.children.clear();
        let id = self.base.id();
        ctx.clear_children_ids(id);
    }

    fn rect(&self) -> (f32, f32, f32, f32) {
        (self.base.x, self.base.y, self.base.w, self.base.h)
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let (clamped_x, clamped_y, clamped_w, clamped_h) = if let Some(parent_ptr) = self.parent {
            let (px, py, pw, ph) = unsafe { (*parent_ptr).rect() };
            let cx = x.clamp(px, px + pw.max(0.0));
            let cy = y.clamp(py, py + ph.max(0.0));
            let cw = w.min((px + pw.max(0.0) - cx).max(0.0));
            let ch = h.min((py + ph.max(0.0) - cy).max(0.0));
            (cx, cy, cw, ch)
        } else {
            (x, y, w, h)
        };

        self.base.x = clamped_x;
        self.base.y = clamped_y;
        self.base.w = clamped_w;
        self.base.h = clamped_h;

        if !self.visible {
            return;
        }

        if let Some(idx) = self.active_index {
            if idx < self.children.len() {
                unsafe {
                    (*self.children[idx]).set_rect(clamped_x, clamped_y, clamped_w, clamped_h);
                }
            }
        }
    }

    fn layout(&mut self, origin: Point, constraints: LayoutConstraints, ctx: &mut UiContext) {
        let size = self.measure(constraints, ctx);
        self.set_rect(origin.x, origin.y, size.width, size.height);

        if self.visible {
            if let Some(idx) = self.active_index {
                if idx < self.children.len() {
                    unsafe {
                        (*self.children[idx]).layout(
                            Point { x: self.base.x, y: self.base.y },
                            LayoutConstraints::new(self.base.w, self.base.w, self.base.h, self.base.h),
                            ctx,
                        );
                    }
                }
            }
        }
    }

    fn hit_test(&self, px: f32, py: f32, ctx: &UiContext) -> bool {
        if !self.visible {
            return false;
        }
        if let Some(idx) = self.active_index {
            if idx < self.children.len() {
                return unsafe { (*self.children[idx]).hit_test(px, py, ctx) };
            }
        }
        false
    }

    fn all_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.visible {
            return Vec::new();
        }
        let mut quads = Vec::new();
        if let Some(idx) = self.active_index {
            if idx < self.children.len() {
                let widget = unsafe { &*self.children[idx] };
                let c = widget.color();
                if c[3] > 0.0 {
                    let (wx, wy, ww, wh) = widget.rect();
                    quads.push((wx, wy, ww, wh, c));
                }
                quads.extend(widget.all_quads(ctx));
            }
        }
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        if !self.visible {
            return Vec::new();
        }
        if let Some(idx) = self.active_index {
            if idx < self.children.len() {
                return unsafe { (*self.children[idx]).text_labels() };
            }
        }
        Vec::new()
    }

    fn text_labels_with_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<[f32; 4]>)> {
        if !self.visible {
            return Vec::new();
        }
        if let Some(idx) = self.active_index {
            if idx < self.children.len() {
                return unsafe { (*self.children[idx]).text_labels_with_bounds(ctx) };
            }
        }
        Vec::new()
    }

    fn text_labels_with_font_and_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        if !self.visible {
            return Vec::new();
        }
        if let Some(idx) = self.active_index {
            if idx < self.children.len() {
                return unsafe { (*self.children[idx]).text_labels_with_font_and_bounds(ctx) };
            }
        }
        Vec::new()
    }

    fn get_text_items(&self) -> Vec<(&glyphon::Buffer, f32, f32, glyphon::Color)> {
        if !self.visible {
            return Vec::new();
        }
        if let Some(idx) = self.active_index {
            if idx < self.children.len() {
                return unsafe { (*self.children[idx]).get_text_items() };
            }
        }
        Vec::new()
    }

    fn prepare_text(&mut self, fs: &mut glyphon::FontSystem) {
        if !self.visible {
            return;
        }
        if let Some(idx) = self.active_index {
            if idx < self.children.len() {
                unsafe {
                    (*self.children[idx]).prepare_text(fs);
                }
            }
        }
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if !self.visible {
            return false;
        }
        if let Some(idx) = self.active_index {
            if idx < self.children.len() {
                let widget = unsafe { &mut *self.children[idx] };
                if widget.is_dragging() {
                    return widget.drag_update(px, py);
                } else {
                    return widget.cursor_moved(px, py, ctx);
                }
            }
        }
        false
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if !self.visible {
            return false;
        }
        if let Some(idx) = self.active_index {
            if idx < self.children.len() {
                let widget = unsafe { &mut *self.children[idx] };
                if widget.popover_rect().is_some() {
                    if widget.mouse_input(button, state, px, py, ctx) {
                        return true;
                    }
                }
                if widget.mouse_input(button, state, px, py, ctx) {
                    return true;
                }
                if state == ElementState::Pressed && !widget.hit_test(px, py, ctx) {
                    widget.unfocus();
                }
            }
        }
        false
    }

    fn keyboard_input(&mut self, event: &KeyEvent, ctx: &mut UiContext) -> bool {
        if !self.visible {
            return false;
        }
        if let Some(idx) = self.active_index {
            if idx < self.children.len() {
                return unsafe { (*self.children[idx]).keyboard_input(event, ctx) };
            }
        }
        false
    }

    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if !self.visible {
            return false;
        }
        if let Some(idx) = self.active_index {
            if idx < self.children.len() {
                return unsafe { (*self.children[idx]).mouse_wheel(delta, px, py, ctx) };
            }
        }
        false
    }

    fn popover_rect(&self) -> Option<(f32, f32, f32, f32)> {
        if !self.visible {
            return None;
        }
        if let Some(idx) = self.active_index {
            if idx < self.children.len() {
                return unsafe { (*self.children[idx]).popover_rect() };
            }
        }
        None
    }

    fn render_popover(&self, pc: &mut dyn crate::layout::RenderTarget) {
        if !self.visible {
            return;
        }
        if let Some(idx) = self.active_index {
            if idx < self.children.len() {
                unsafe {
                    (*self.children[idx]).render_popover(pc);
                }
            }
        }
    }

    fn tick(&mut self, dt: f32, ctx: &mut UiContext) -> bool {
        if !self.visible {
            return false;
        }
        if let Some(idx) = self.active_index {
            if idx < self.children.len() {
                return unsafe { (*self.children[idx]).tick(dt, ctx) };
            }
        }
        false
    }

    fn rounded_corners(&self) -> (bool, bool, bool, bool) {
        if let Some(idx) = self.active_index {
            if idx < self.children.len() {
                return unsafe { (*self.children[idx]).rounded_corners() };
            }
        }
        (false, false, false, false)
    }

    fn solid_border(&self) -> Option<([f32; 4], f32)> {
        if let Some(idx) = self.active_index {
            if idx < self.children.len() {
                return unsafe { (*self.children[idx]).solid_border() };
            }
        }
        None
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.visible {
            return Vec::new();
        }
        if let Some(idx) = self.active_index {
            if idx < self.children.len() {
                return unsafe { (*self.children[idx]).extra_quads() };
            }
        }
        Vec::new()
    }

    fn extra_arcs(&self) -> Vec<(f32, f32, f32, f32, f32, f32, [f32; 4])> {
        if !self.visible {
            return Vec::new();
        }
        if let Some(idx) = self.active_index {
            if idx < self.children.len() {
                return unsafe { (*self.children[idx]).extra_arcs() };
            }
        }
        Vec::new()
    }

    fn as_menu_controller(&self) -> Option<&dyn MenuController> { Some(self) }
    fn as_menu_controller_mut(&mut self) -> Option<&mut dyn MenuController> { Some(self) }
}

impl MenuController for Switcher {
    fn menu_click(&mut self) -> Option<(usize, usize)> {
        let idx = self.active_index?;
        let child_ptr = *self.children.get(idx)?;
        unsafe { &mut *child_ptr }.as_menu_controller_mut()?.menu_click()
    }
    fn trigger_menu_click(&mut self, menu_idx: usize, item_idx: usize) {
        if let Some(idx) = self.active_index {
            if let Some(child) = self.children.get(idx) {
                if let Some(mc) = unsafe { &mut **child }.as_menu_controller_mut() {
                    mc.trigger_menu_click(menu_idx, item_idx);
                }
            }
        }
    }
    fn set_item_checked(&mut self, menu_idx: usize, item_idx: usize, checked: bool) {
        if let Some(idx) = self.active_index {
            if let Some(child) = self.children.get(idx) {
                if let Some(mc) = unsafe { &mut **child }.as_menu_controller_mut() {
                    mc.set_item_checked(menu_idx, item_idx, checked);
                }
            }
        }
    }
    fn set_menu_items(&mut self, menu_idx: usize, items: &[String]) {
        if let Some(idx) = self.active_index {
            if let Some(child) = self.children.get(idx) {
                if let Some(mc) = unsafe { &mut **child }.as_menu_controller_mut() {
                    mc.set_menu_items(menu_idx, items);
                }
            }
        }
    }
    fn is_menu_bar(&self) -> bool {
        if let Some(idx) = self.active_index {
            if let Some(child) = self.children.get(idx) {
                if let Some(mc) = unsafe { &**child }.as_menu_controller() {
                    return mc.is_menu_bar();
                }
            }
        }
        false
    }
    fn is_menu_open(&self) -> bool {
        if let Some(idx) = self.active_index {
            if let Some(child) = self.children.get(idx) {
                if let Some(mc) = unsafe { &**child }.as_menu_controller() {
                    return mc.is_menu_open();
                }
            }
        }
        false
    }
    fn menu_items(&self) -> Vec<String> {
        if let Some(idx) = self.active_index {
            if let Some(child) = self.children.get(idx) {
                if let Some(mc) = unsafe { &**child }.as_menu_controller() {
                    return mc.menu_items();
                }
            }
        }
        Vec::new()
    }
    fn menu_item_checked(&self) -> Vec<Option<bool>> {
        if let Some(idx) = self.active_index {
            if let Some(child) = self.children.get(idx) {
                if let Some(mc) = unsafe { &**child }.as_menu_controller() {
                    return mc.menu_item_checked();
                }
            }
        }
        Vec::new()
    }
    fn is_vertical(&self) -> bool {
        if let Some(idx) = self.active_index {
            if let Some(child) = self.children.get(idx) {
                if let Some(mc) = unsafe { &**child }.as_menu_controller() {
                    return mc.is_vertical();
                }
            }
        }
        false
    }
    fn menu_names(&self) -> Vec<String> {
        if let Some(idx) = self.active_index {
            if let Some(child) = self.children.get(idx) {
                if let Some(mc) = unsafe { &**child }.as_menu_controller() {
                    return mc.menu_names();
                }
            }
        }
        Vec::new()
    }
    fn menu_items_list(&self) -> Vec<Vec<String>> {
        if let Some(idx) = self.active_index {
            if let Some(child) = self.children.get(idx) {
                if let Some(mc) = unsafe { &**child }.as_menu_controller() {
                    return mc.menu_items_list();
                }
            }
        }
        Vec::new()
    }
    fn menu_checked_list(&self) -> Vec<Vec<Option<bool>>> {
        if let Some(idx) = self.active_index {
            if let Some(child) = self.children.get(idx) {
                if let Some(mc) = unsafe { &**child }.as_menu_controller() {
                    return mc.menu_checked_list();
                }
            }
        }
        Vec::new()
    }
    fn take_context_change(&mut self) -> Option<usize> {
        let idx = self.active_index?;
        let child_ptr = *self.children.get(idx)?;
        unsafe { &mut *child_ptr }.as_menu_controller_mut()?.take_context_change()
    }
    fn set_context_selected(&mut self, selected: usize) {
        if let Some(idx) = self.active_index {
            if let Some(child) = self.children.get(idx) {
                if let Some(mc) = unsafe { &mut **child }.as_menu_controller_mut() {
                    mc.set_context_selected(selected);
                }
            }
        }
    }
    fn set_center_items(&mut self, center: bool) {
        if let Some(idx) = self.active_index {
            if let Some(child) = self.children.get(idx) {
                if let Some(mc) = unsafe { &mut **child }.as_menu_controller_mut() {
                    mc.set_center_items(center);
                }
            }
        }
    }
    fn get_menu_items_at(&self, px: f32, py: f32) -> Option<(usize, String, Vec<String>, f32, f32, f32, f32)> {
        let idx = self.active_index?;
        let child_ptr = *self.children.get(idx)?;
        unsafe { &*child_ptr }.as_menu_controller()?.get_menu_items_at(px, py)
    }
}

unsafe impl Send for Switcher {}
unsafe impl Sync for Switcher {}
