//! Narrow-trait `Switcher` (Phase 5n) — the first container across: it owns externally-managed
//! child pointers (pages) and exposes exactly one of them at a time. The adapter's container
//! concern does the subtree plumbing (geometry/text aggregation, tick/popover/text-item
//! recursion, hit-through-children), filtered to the active child by
//! [`Layout::child_visible`]; the model keeps the legacy specifics: parent-clamped rects,
//! active-child arrangement, event proxying (via `EventCtx::ui`), paint-property proxies, and
//! the [`MenuController`] delegation to the active child.

use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::{
    Adapted, Element, ElementState, Event, EventCtx, Input, Layout, MenuController, Paint,
    UiContext,
};

#[derive(Debug, Clone)]
pub struct Switcher {
    children: Vec<*mut (dyn Element + 'static)>,
    parent: Option<*mut (dyn Element + 'static)>,
    active_index: Option<usize>,
}

impl Switcher {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Adapted<Switcher> {
        let mut s = Adapted::new(Switcher { children: Vec::new(), parent: None, active_index: None });
        Element::set_rect(&mut s, x, y, w, h);
        s
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

    fn active_child(&self) -> Option<*mut (dyn Element + 'static)> {
        self.children.get(self.active_index?).copied()
    }
}

impl Layout for Switcher {
    fn has_container_children(&self) -> bool {
        true
    }

    fn container_children(&self) -> Vec<*mut (dyn Element + 'static)> {
        self.children.clone()
    }

    fn child_added(&mut self, child: *mut (dyn Element + 'static)) {
        self.children.push(child);
        // Sync visibility of the newly added child with the active selection.
        let idx = self.children.len() - 1;
        unsafe {
            (*child).set_visible(self.active_index == Some(idx));
        }
    }

    fn children_cleared(&mut self) {
        self.children.clear();
    }

    fn parent_changed(&mut self, parent: Option<*mut (dyn Element + 'static)>) {
        self.parent = parent;
    }

    fn child_visible(&self, child: *mut (dyn Element + 'static)) -> bool {
        self.active_child().map_or(false, |active| std::ptr::eq(active, child))
    }

    /// Legacy `set_rect` clamped the switcher into its parent's rect.
    fn adjust_rect(&self, requested: Rect) -> Rect {
        let Some(parent_ptr) = self.parent else {
            return requested;
        };
        let (px, py, pw, ph) = unsafe { (*parent_ptr).rect() };
        let cx = requested.x.clamp(px, px + pw.max(0.0));
        let cy = requested.y.clamp(py, py + ph.max(0.0));
        let cw = requested.width.min((px + pw.max(0.0) - cx).max(0.0));
        let ch = requested.height.min((py + ph.max(0.0) - cy).max(0.0));
        Rect { x: cx, y: cy, width: cw, height: ch }
    }

    /// The active child fills the switcher's rect.
    fn arrange_children(&mut self, rect: Rect) {
        if let Some(child) = self.active_child() {
            unsafe {
                (*child).set_rect(rect.x, rect.y, rect.width, rect.height);
            }
        }
    }

    fn layout_children_ctx(&mut self, rect: Rect, ctx: &mut UiContext) {
        if let Some(child) = self.active_child() {
            unsafe {
                (*child).layout(
                    crate::widget::Point { x: rect.x, y: rect.y },
                    crate::widget::LayoutConstraints::new(rect.width, rect.width, rect.height, rect.height),
                    ctx,
                );
            }
        }
    }
}

impl Paint for Switcher {
    /// The switcher shows as whatever its active child shows as (legacy proxied `color`,
    /// `rounded_corners`, and `solid_border` — style-property painters read these).
    fn color(&self) -> [f32; 4] {
        match self.active_child() {
            Some(child) => unsafe { (*child).color() },
            None => [0.0, 0.0, 0.0, 0.0],
        }
    }

    fn corner_style(&self) -> Option<(f32, (bool, bool, bool, bool))> {
        // Legacy kept the Element-default 12.0 radius and proxied the corner flags.
        let corners = match self.active_child() {
            Some(child) => unsafe { (*child).rounded_corners() },
            None => (false, false, false, false),
        };
        Some((12.0, corners))
    }

    fn solid_border(&self) -> Option<([f32; 4], f32)> {
        self.active_child().and_then(|child| unsafe { (*child).solid_border() })
    }

    /// No own geometry: the background is the active child's own business, and the adapter's
    /// container aggregation carries the subtree on the legacy getters (the scene walk
    /// recurses `children()` itself).
    fn paint(&self, _rect: Rect, _ctx: &mut PaintCtx) {}
}

impl Input for Switcher {
    fn blocks_backplate_drag(&self) -> bool {
        false
    }

    fn hits_through_children(&self) -> bool {
        true
    }

    /// Legacy `mouse_input` saw every press to unfocus the child on an outside click.
    fn gates_presses(&self) -> bool {
        false
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        let Some(child_ptr) = self.active_child() else {
            return false;
        };
        let widget = unsafe { &mut *child_ptr };
        match event {
            Event::MouseButton { button, state, x: px, y: py, .. } => {
                let Some(ui) = ectx.ui.as_deref_mut() else {
                    return false;
                };
                // Faithful port of the legacy body, including the double dispatch while a
                // popover is open.
                if widget.popover_rect().is_some() {
                    if widget.mouse_input(*button, *state, *px, *py, ui) {
                        return true;
                    }
                }
                if widget.mouse_input(*button, *state, *px, *py, ui) {
                    return true;
                }
                if *state == ElementState::Pressed && !widget.hit_test(*px, *py, ui) {
                    widget.unfocus();
                }
                false
            }
            Event::PointerMove { x: px, y: py, .. } => {
                if widget.is_dragging() {
                    return widget.drag_update(*px, *py);
                }
                let Some(ui) = ectx.ui.as_deref_mut() else {
                    return false;
                };
                widget.cursor_moved(*px, *py, ui)
            }
            Event::MouseWheel { delta, x: px, y: py, .. } => {
                let Some(ui) = ectx.ui.as_deref_mut() else {
                    return false;
                };
                widget.mouse_wheel(delta, *px, *py, ui)
            }
            Event::KeyInput(key_event) => {
                let Some(ui) = ectx.ui.as_deref_mut() else {
                    return false;
                };
                widget.keyboard_input(key_event, ui)
            }
            _ => false,
        }
    }

    fn menu_controller(&self) -> Option<&dyn MenuController> {
        Some(self)
    }
    fn menu_controller_mut(&mut self) -> Option<&mut dyn MenuController> {
        Some(self)
    }
}

impl MenuController for Switcher {
    fn menu_click(&mut self) -> Option<(usize, usize)> {
        unsafe { &mut *self.active_child()? }.as_menu_controller_mut()?.menu_click()
    }
    fn trigger_menu_click(&mut self, menu_idx: usize, item_idx: usize) {
        if let Some(child) = self.active_child() {
            if let Some(mc) = unsafe { &mut *child }.as_menu_controller_mut() {
                mc.trigger_menu_click(menu_idx, item_idx);
            }
        }
    }
    fn set_item_checked(&mut self, menu_idx: usize, item_idx: usize, checked: bool) {
        if let Some(child) = self.active_child() {
            if let Some(mc) = unsafe { &mut *child }.as_menu_controller_mut() {
                mc.set_item_checked(menu_idx, item_idx, checked);
            }
        }
    }
    fn set_menu_items(&mut self, menu_idx: usize, items: &[String]) {
        if let Some(child) = self.active_child() {
            if let Some(mc) = unsafe { &mut *child }.as_menu_controller_mut() {
                mc.set_menu_items(menu_idx, items);
            }
        }
    }
    fn is_menu_bar(&self) -> bool {
        self.active_child()
            .and_then(|c| unsafe { &*c }.as_menu_controller().map(|mc| mc.is_menu_bar()))
            .unwrap_or(false)
    }
    fn is_menu_open(&self) -> bool {
        self.active_child()
            .and_then(|c| unsafe { &*c }.as_menu_controller().map(|mc| mc.is_menu_open()))
            .unwrap_or(false)
    }
    fn menu_items(&self) -> Vec<String> {
        self.active_child()
            .and_then(|c| unsafe { &*c }.as_menu_controller().map(|mc| mc.menu_items()))
            .unwrap_or_default()
    }
    fn menu_item_checked(&self) -> Vec<Option<bool>> {
        self.active_child()
            .and_then(|c| unsafe { &*c }.as_menu_controller().map(|mc| mc.menu_item_checked()))
            .unwrap_or_default()
    }
    fn is_vertical(&self) -> bool {
        self.active_child()
            .and_then(|c| unsafe { &*c }.as_menu_controller().map(|mc| mc.is_vertical()))
            .unwrap_or(false)
    }
    fn menu_names(&self) -> Vec<String> {
        self.active_child()
            .and_then(|c| unsafe { &*c }.as_menu_controller().map(|mc| mc.menu_names()))
            .unwrap_or_default()
    }
    fn menu_items_list(&self) -> Vec<Vec<String>> {
        self.active_child()
            .and_then(|c| unsafe { &*c }.as_menu_controller().map(|mc| mc.menu_items_list()))
            .unwrap_or_default()
    }
    fn menu_checked_list(&self) -> Vec<Vec<Option<bool>>> {
        self.active_child()
            .and_then(|c| unsafe { &*c }.as_menu_controller().map(|mc| mc.menu_checked_list()))
            .unwrap_or_default()
    }
    fn take_context_change(&mut self) -> Option<usize> {
        unsafe { &mut *self.active_child()? }.as_menu_controller_mut()?.take_context_change()
    }
    fn set_context_selected(&mut self, selected: usize) {
        if let Some(child) = self.active_child() {
            if let Some(mc) = unsafe { &mut *child }.as_menu_controller_mut() {
                mc.set_context_selected(selected);
            }
        }
    }
    fn set_center_items(&mut self, center: bool) {
        if let Some(child) = self.active_child() {
            if let Some(mc) = unsafe { &mut *child }.as_menu_controller_mut() {
                mc.set_center_items(center);
            }
        }
    }
    fn get_menu_items_at(&self, px: f32, py: f32) -> Option<(usize, String, Vec<String>, f32, f32, f32, f32)> {
        unsafe { &*self.active_child()? }.as_menu_controller()?.get_menu_items_at(px, py)
    }
}

unsafe impl Send for Switcher {}
unsafe impl Sync for Switcher {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::UiContext;
    use crate::widget::{Checkbox, Label};

    #[test]
    fn switcher_exposes_only_the_active_child() {
        let mut ctx = UiContext::new();
        let mut a = Label::new("page a");
        let mut b = Checkbox::new();
        let mut sw = Switcher::new(0.0, 0.0, 200.0, 100.0);
        let (sw_id, sw_ptr) = (sw.id(), sw.as_ptr_mut());
        ctx.register_widget(sw_id, sw_ptr);
        Element::add_child(&mut sw, a.as_ptr_mut(), &mut ctx);
        Element::add_child(&mut sw, b.as_ptr_mut(), &mut ctx);

        // add_child parented both children back to the switcher and left them hidden (no
        // active selection yet).
        assert!(!Element::visible(&a) && !Element::visible(&b));
        assert!(Element::children(&sw, &ctx).len() == 2);

        // Activating a child shows it, arranges it into the switcher's rect, and routes the
        // subtree getters through it alone.
        sw.set_active_index(Some(1));
        assert!(!Element::visible(&a) && Element::visible(&b));
        Element::set_rect(&mut sw, 10.0, 20.0, 300.0, 150.0);
        assert_eq!(Element::rect(&b), (10.0, 20.0, 300.0, 150.0), "active child fills the rect");
        assert_ne!(Element::rect(&a), (10.0, 20.0, 300.0, 150.0), "inactive child untouched");

        // Aggregation and hit-testing go through the active child only.
        assert!(Element::is_child_visible(&sw, b.id()));
        assert!(!Element::is_child_visible(&sw, a.id()));
        assert!(Element::hit_test(&sw, 15.0, 25.0, &ctx), "hit lands on the active child");

        // The menu-controller delegation returns None-ish defaults for non-menu children.
        let elem: &dyn Element = &sw;
        assert!(elem.as_menu_controller().unwrap().menu_items().is_empty());
    }
}
