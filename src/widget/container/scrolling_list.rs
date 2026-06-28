use crate::widget::*;
use super::scroll_box::ScrollBox;
pub use crate::widget::input::font_selector::FontSelector;


// Generic text item layout wrapper
#[derive(Debug, Clone)]
pub struct ScrollingList {
    pub scroll_box: ScrollBox,
    pub item_height: f32,
    pub item_gap: f32,
}

impl ScrollingList {
    pub fn new(item_height: f32, item_gap: f32) -> Self {
        Self {
            scroll_box: ScrollBox::new(),
            item_height,
            item_gap,
        }
    }

    pub fn update_bounds(&mut self, count: usize, viewport_y: f32, viewport_h: f32) {
        let item_height_full = self.item_height + self.item_gap;
        let content_h = count as f32 * item_height_full;
        self.scroll_box.update_bounds(content_h, viewport_y, viewport_h);
    }

    pub fn get_item_draw_y(&self, idx: usize, offset: f32) -> Option<f32> {
        let item_height_full = self.item_height + self.item_gap;
        let virtual_y = idx as f32 * item_height_full + offset;
        self.scroll_box.get_item_draw_y(virtual_y, self.item_height)
    }

    pub fn scroll_y(&self) -> f32 {
        self.scroll_box.scroll_y
    }

    pub fn set_scroll_y(&mut self, val: f32) {
        self.scroll_box.scroll_y = val;
    }
}

impl Default for ScrollingList {
    fn default() -> Self {
        Self::new(24.0, 4.0)
    }
}

impl Element for ScrollingList {
    fn rect(&self) -> (f32, f32, f32, f32) {
        self.scroll_box.rect()
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.scroll_box.set_rect(x, y, w, h);
    }

    fn color(&self) -> [f32; 4] {
        self.scroll_box.color()
    }
    fn as_ptr(&self) -> *mut (dyn Element + 'static) {
        self as *const Self as *mut Self as *mut (dyn Element + 'static)
    }
    fn as_ptr_mut(&mut self) -> *mut (dyn Element + 'static) {
        self as *mut Self as *mut (dyn Element + 'static)
    }

    fn set_hovered(&mut self, v: bool) {
        self.scroll_box.set_hovered(v);
    }

    fn hovered(&self) -> bool {
        self.scroll_box.hovered()
    }

    fn highlight_color(&self, ctx: &UiContext) -> Option<[f32; 4]> {
        self.scroll_box.highlight_color(ctx)
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        self.scroll_box.cursor_moved(px, py, ctx)
    }

    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        self.scroll_box.mouse_wheel(delta, px, py, ctx)
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        self.scroll_box.mouse_input(button, state, px, py, ctx)
    }

    fn focus(&mut self) {
        self.scroll_box.focus();
    }

    fn unfocus(&mut self) {
        self.scroll_box.unfocus();
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        self.scroll_box.extra_quads()
    }

    fn keyboard_input(&mut self, event: &KeyEvent, ctx: &mut UiContext) -> bool {
        self.scroll_box.keyboard_input(event, ctx)
    }

    fn parent(&self, ctx: &UiContext) -> Option<*mut (dyn Element + 'static)> { self.scroll_box.parent(ctx) }
    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>, ctx: &mut UiContext) { self.scroll_box.set_parent(parent, ctx); }
    fn children(&self, ctx: &UiContext) -> Vec<*mut (dyn Element + 'static)> { self.scroll_box.children(ctx) }
    fn add_child(&mut self, child: *mut (dyn Element + 'static), ctx: &mut UiContext) { self.scroll_box.add_child(child, ctx); }
    fn clear_children(&mut self, ctx: &mut UiContext) { self.scroll_box.clear_children(ctx); }

    fn as_scroll_controller(&self) -> Option<&dyn ScrollController> { Some(self) }
    fn as_scroll_controller_mut(&mut self) -> Option<&mut dyn ScrollController> { Some(self) }
}

impl ScrollController for ScrollingList {
    fn update_bounds(&mut self, count: usize, viewport_y: f32, viewport_h: f32) {
        self.update_bounds(count, viewport_y, viewport_h);
    }

    fn get_item_draw_y(&self, idx: usize, offset: f32) -> Option<f32> {
        self.get_item_draw_y(idx, offset)
    }
}

unsafe impl Send for ScrollingList {}
unsafe impl Sync for ScrollingList {}
