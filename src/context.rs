use std::collections::HashMap;
use crate::widget::{Element, WidgetId, LayoutTree, Key, KeyEvent, MouseButton, ElementState, MouseScrollDelta};
use crate::widget::core::hover_animation::HoverState;
use crate::widget::core::context_menu::ContextMenuState;
use crate::widget::TextBox;

pub struct UiContext {
    pub layout_tree: LayoutTree,
    pub widget_registry: HashMap<WidgetId, *mut (dyn Element + 'static)>,
    pub focused_widget: Option<*mut (dyn Element + 'static)>,
    pub active_popovers: Vec<*const (dyn Element + 'static)>,
    pub hover_state: HoverState,
    pub cursor_pos: (f32, f32),
    pub context_menu: ContextMenuState,
}

impl UiContext {
    pub fn new() -> Self {
        Self {
            layout_tree: LayoutTree {
                parents: HashMap::new(),
                children: HashMap::new(),
            },
            widget_registry: HashMap::new(),
            focused_widget: None,
            active_popovers: Vec::new(),
            hover_state: HoverState::new(),
            cursor_pos: (0.0, 0.0),
            context_menu: ContextMenuState::new(),
        }
    }

    // --- Focus management ---
    pub fn set_focused(&mut self, w: &mut dyn Element) {
        let new_ptr = unsafe {
            std::mem::transmute::<*mut dyn Element, *mut (dyn Element + 'static)>(w as *mut dyn Element)
        };
        self.set_focused_ptr(new_ptr);
    }

    pub fn set_focused_ptr(&mut self, new_ptr: *mut (dyn Element + 'static)) {
        if let Some(old_ptr) = self.focused_widget {
            let old_data = old_ptr as *mut () as usize;
            let new_data = new_ptr as *mut () as usize;
            if old_data != new_data {
                unsafe {
                    (*old_ptr).unfocus();
                }
                self.focused_widget = Some(new_ptr);
            }
        } else {
            self.focused_widget = Some(new_ptr);
        }
    }

    pub fn is_focused(&self, w: &dyn Element) -> bool {
        let addr = w as *const dyn Element as *const () as usize;
        self.is_focused_addr(addr)
    }

    pub fn is_focused_addr(&self, addr: usize) -> bool {
        if let Some(ptr) = self.focused_widget {
            let current_data = ptr as *const () as usize;
            current_data == addr
        } else {
            false
        }
    }

    pub fn clear_focus(&mut self) {
        if let Some(ptr) = self.focused_widget.take() {
            unsafe {
                (*ptr).unfocus();
            }
        }
    }

    pub fn clear_if_matches(&mut self, w: &dyn Element) {
        let query_data = w as *const dyn Element as *const () as usize;
        if let Some(ptr) = self.focused_widget {
            let current_data = ptr as *const () as usize;
            if current_data == query_data {
                self.focused_widget = None;
            }
        }
    }

    pub fn has_focus(&self) -> bool {
        self.focused_widget.is_some()
    }

    pub fn navigate_focus(&mut self, key: &Key, ctrl: bool) -> bool {
        let ptr = match self.focused_widget {
            Some(p) => p,
            None => return false,
        };

        unsafe {
            match (key, ctrl) {
                (Key::Character(c), true) if c == "u" || c == "U" => {
                    if let Some(parent_ptr) = (*ptr).parent(self) {
                        let parent_ref = &mut *parent_ptr;
                        self.set_focused(parent_ref);
                        parent_ref.focus();
                        return true;
                    }
                }
                (Key::Character(c), true) if c == "i" || c == "I" => {
                    let mut children = (*ptr).children(self);
                    if !children.is_empty() {
                        let child_ref = &mut *children[0];
                        self.set_focused(child_ref);
                        child_ref.focus();
                        return true;
                    }
                }
                (Key::Character(c), true) if c == "j" || c == "J" => {
                    if let Some(parent_ptr) = (*ptr).parent(self) {
                        let mut siblings = (*parent_ptr).children(self);
                        let current_idx = siblings.iter().position(|&x| {
                            let a = x as *mut () as usize;
                            let b = ptr as *mut () as usize;
                            a == b
                        });
                        if let Some(idx) = current_idx {
                            let next_idx = (idx + 1) % siblings.len();
                            let sibling_ref = &mut *siblings[next_idx];
                            self.set_focused(sibling_ref);
                            sibling_ref.focus();
                            return true;
                        }
                    }
                }
                (Key::Character(c), true) if c == "k" || c == "K" => {
                    if let Some(parent_ptr) = (*ptr).parent(self) {
                        let mut siblings = (*parent_ptr).children(self);
                        let current_idx = siblings.iter().position(|&x| {
                            let a = x as *mut () as usize;
                            let b = ptr as *mut () as usize;
                            a == b
                        });
                        if let Some(idx) = current_idx {
                            let prev_idx = if idx == 0 { siblings.len() - 1 } else { idx - 1 };
                            let sibling_ref = &mut *siblings[prev_idx];
                            self.set_focused(sibling_ref);
                            sibling_ref.focus();
                            return true;
                        }
                    }
                }
                _ => {}
            }
        }
        false
    }

    // --- Registry ---
    pub fn register_widget(&mut self, id: WidgetId, ptr: *mut (dyn Element + 'static)) {
        self.widget_registry.insert(id, ptr);
    }

    pub fn link_ids(&mut self, parent: WidgetId, child: WidgetId) {
        self.layout_tree.parents.insert(child, parent);
        let children = self.layout_tree.children.entry(parent).or_default();
        if !children.contains(&child) {
            children.push(child);
        }
    }

    pub fn unlink_child(&mut self, parent: WidgetId, child: WidgetId) {
        self.layout_tree.parents.remove(&child);
        if let Some(children) = self.layout_tree.children.get_mut(&parent) {
            children.retain(|&x| x != child);
        }
    }

    pub fn clear_children_ids(&mut self, parent: WidgetId) {
        if let Some(children) = self.layout_tree.children.remove(&parent) {
            for child in children {
                self.layout_tree.parents.remove(&child);
            }
        }
    }

    pub fn clear_hierarchy(&mut self) {
        self.layout_tree.parents.clear();
        self.layout_tree.children.clear();
        self.widget_registry.clear();
    }

    // --- Popovers ---
    pub fn clear_popovers(&mut self) {
        self.active_popovers.clear();
    }

    pub fn register_popover(&mut self, w: &(dyn Element + 'static)) {
        let ptr = w as *const (dyn Element + 'static);
        if !self.active_popovers.contains(&ptr) {
            self.active_popovers.push(ptr);
        }
    }

    pub fn register_popover_ptr(&mut self, ptr: *mut (dyn Element + 'static)) {
        let const_ptr = ptr as *const (dyn Element + 'static);
        if !self.active_popovers.contains(&const_ptr) {
            self.active_popovers.push(const_ptr);
        }
    }

    pub fn is_coordinate_covered(&self, query_address: usize, px: f32, py: f32) -> bool {
        for popover_ptr in self.active_popovers.iter() {
            let current_data = *popover_ptr as *const () as usize;
            if query_address == current_data {
                continue;
            }
            unsafe {
                if let Some(popover) = popover_ptr.as_ref() {
                    if let Some((x, y, width, height)) = popover.popover_rect() {
                        if px >= x && px <= x + width && py >= y && py <= y + height {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    // --- Hover State ---
    pub fn set_cursor_pos(&mut self, x: f32, y: f32) {
        self.cursor_pos = (x, y);
    }

    pub fn reset_frame_registration(&mut self) {
        self.hover_state.registered_this_frame = false;
    }

    pub fn set_scroll_offset(&mut self, offset: f32) {
        self.hover_state.scroll_offset = offset;
    }

    pub fn get_scroll_offset(&self) -> f32 {
        self.hover_state.scroll_offset
    }

    pub fn register_hovered(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        self.hover_state.target_x = Some(x);
        self.hover_state.target_y = Some(y);
        self.hover_state.target_w = Some(w);
        self.hover_state.target_h = Some(h);
        self.hover_state.target_alpha = color[3];
        self.hover_state.registered_this_frame = true;
    }

    pub fn post_render_check(&mut self) {
        if !self.hover_state.registered_this_frame {
            self.hover_state.target_alpha = 0.0;
            let (cx, cy) = self.cursor_pos;
            self.hover_state.target_x = Some(cx);
            self.hover_state.target_y = Some(cy + self.hover_state.scroll_offset);
            self.hover_state.target_w = Some(0.0);
            self.hover_state.target_h = Some(0.0);
        }
    }

    pub fn tick_hover(&mut self, dt: f32) -> bool {
        let s = &mut self.hover_state;
        let decay = 15.0;
        let mut changed = false;

        if s.current_alpha <= 0.001 && s.target_alpha > 0.0 {
            if let (Some(tx), Some(ty), Some(tw), Some(th)) = (s.target_x, s.target_y, s.target_w, s.target_h) {
                s.current_x = tx;
                s.current_y = ty;
                s.current_w = tw;
                s.current_h = th;
            }
        }

        if (s.current_alpha - s.target_alpha).abs() > 0.001 {
            s.current_alpha += (s.target_alpha - s.current_alpha) * (1.0 - (-decay * dt).exp());
            changed = true;
        } else if s.current_alpha != s.target_alpha {
            s.current_alpha = s.target_alpha;
            changed = true;
        }

        if let (Some(tx), Some(ty), Some(tw), Some(th)) = (s.target_x, s.target_y, s.target_w, s.target_h) {
            if (s.current_x - tx).abs() > 0.1 {
                s.current_x += (tx - s.current_x) * (1.0 - (-decay * dt).exp());
                changed = true;
            } else if s.current_x != tx {
                s.current_x = tx;
                changed = true;
            }

            if (s.current_y - ty).abs() > 0.1 {
                s.current_y += (ty - s.current_y) * (1.0 - (-decay * dt).exp());
                changed = true;
            } else if s.current_y != ty {
                s.current_y = ty;
                changed = true;
            }

            if (s.current_w - tw).abs() > 0.1 {
                s.current_w += (tw - s.current_w) * (1.0 - (-decay * dt).exp());
                changed = true;
            } else if s.current_w != tw {
                s.current_w = tw;
                changed = true;
            }

            if (s.current_h - th).abs() > 0.1 {
                s.current_h += (th - s.current_h) * (1.0 - (-decay * dt).exp());
                changed = true;
            } else if s.current_h != th {
                s.current_h = th;
                changed = true;
            }
        }

        changed
    }

    pub fn get_hover_quad(&self) -> Option<(f32, f32, f32, f32, [f32; 4])> {
        let s = &self.hover_state;
        if s.current_alpha > 0.001 {
            Some((
                s.current_x,
                s.current_y,
                s.current_w,
                s.current_h,
                [1.0, 1.0, 1.0, s.current_alpha],
            ))
        } else {
            None
        }
    }

    // --- Context Menu ---
    pub fn is_context_menu_visible(&self) -> bool {
        self.context_menu.visible
    }

    pub fn show_context_menu(&mut self, x: f32, y: f32, options: Vec<String>, target: *mut TextBox) {
        self.context_menu.show(x, y, options, target);
    }

    pub fn hide_context_menu(&mut self) {
        self.context_menu.hide();
    }

    pub fn hit_test_context_menu(&self, px: f32, py: f32) -> bool {
        self.context_menu.hit_test(px, py)
    }

    pub fn cursor_moved_context_menu(&mut self, px: f32, py: f32) -> bool {
        self.context_menu.cursor_moved(px, py)
    }

    pub fn mouse_input_context_menu(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        self.context_menu.mouse_input(button, state, px, py)
    }

    pub fn context_menu_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        self.context_menu.extra_quads()
    }

    pub fn context_menu_labels(&self) -> Vec<crate::widget::display::TextLabel> {
        self.context_menu.text_labels()
    }
}
