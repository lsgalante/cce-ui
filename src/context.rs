use std::collections::HashMap;
use crate::widget::{Element, WidgetId, LayoutTree, Key, MouseButton, ElementState, Event};
use crate::widget::core::hover_animation::HoverState;
use crate::widget::core::context_menu::ContextMenuState;

pub struct SpatialGrid {
    pub cell_size: f32,
    pub cells: HashMap<(i32, i32), Vec<WidgetId>>,
}

impl SpatialGrid {
    pub fn new(cell_size: f32) -> Self {
        Self {
            cell_size,
            cells: HashMap::new(),
        }
    }

    pub fn clear(&mut self) {
        self.cells.clear();
    }

    pub fn insert(&mut self, id: WidgetId, rect: (f32, f32, f32, f32)) {
        let (x, y, w, h) = rect;
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let start_x = (x / self.cell_size).floor() as i32;
        let end_x = ((x + w) / self.cell_size).floor() as i32;
        let start_y = (y / self.cell_size).floor() as i32;
        let end_y = ((y + h) / self.cell_size).floor() as i32;

        let start_x = start_x.max(-1000);
        let end_x = end_x.min(1000);
        let start_y = start_y.max(-1000);
        let end_y = end_y.min(1000);

        for cx in start_x..=end_x {
            for cy in start_y..=end_y {
                self.cells.entry((cx, cy)).or_default().push(id);
            }
        }
    }

    pub fn query(&self, px: f32, py: f32) -> &[WidgetId] {
        let cx = (px / self.cell_size).floor() as i32;
        let cy = (py / self.cell_size).floor() as i32;
        self.cells.get(&(cx, cy)).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

pub struct UiContext {
    pub layout_tree: LayoutTree,
    pub widget_registry: HashMap<WidgetId, *mut (dyn Element + 'static)>,
    pub focused_widget: Option<*mut (dyn Element + 'static)>,
    pub active_popovers: Vec<*const (dyn Element + 'static)>,
    pub hover_state: HoverState,
    pub cursor_pos: (f32, f32),
    pub context_menu: ContextMenuState,
    pub active_grab: Option<WidgetId>,
    pub drag_start_pos: Option<(f32, f32)>,
    pub drag_target: Option<WidgetId>,
    pub is_dragging: bool,
    pub any_dirty: bool,
    pub tick_receivers: Vec<WidgetId>,
    pub spatial_grid: SpatialGrid,
    pub last_scroll_time: Option<std::time::Instant>,
    pub scroll_initiate_widget_id: Option<WidgetId>,
    pub scroll_gesture_new: bool,
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
            active_grab: None,
            drag_start_pos: None,
            drag_target: None,
            is_dragging: false,
            any_dirty: false,
            tick_receivers: Vec::new(),
            spatial_grid: SpatialGrid::new(100.0),
            last_scroll_time: None,
            scroll_initiate_widget_id: None,
            scroll_gesture_new: false,
        }
    }

    pub fn get_widget(&self, id: WidgetId) -> Option<&(dyn Element + 'static)> {
        self.widget_registry.get(&id).map(|&ptr| unsafe { &*ptr })
    }

    pub fn get_widget_mut(&mut self, id: WidgetId) -> Option<&mut (dyn Element + 'static)> {
        self.widget_registry.get(&id).map(|&ptr| unsafe { &mut *ptr })
    }

    pub fn propagate_event(&mut self, event: &Event, root: *mut (dyn Element + 'static)) -> bool {
        if let Event::MouseWheel { .. } = event {
            let now = std::time::Instant::now();
            let is_new_gesture = match self.last_scroll_time {
                None => true,
                Some(last) => now.duration_since(last).as_millis() > 250,
            };
            if is_new_gesture {
                self.scroll_initiate_widget_id = None;
                self.scroll_gesture_new = true;
            } else {
                self.scroll_gesture_new = false;
            }
            self.last_scroll_time = Some(now);
        }
        self.propagate_event_impl(event, root)
    }

    fn propagate_event_impl(&mut self, event: &Event, root: *mut (dyn Element + 'static)) -> bool {
        if root.is_null() {
            return false;
        }
        if let Event::Tick(_) = event {
            return false;
        }
        unsafe {
            // Track drag gestures based on mouse events
            match event {
                Event::MouseButton { button, state, x, y, .. } if *button == MouseButton::Left => {
                    if *state == ElementState::Pressed {
                        self.drag_start_pos = Some((*x, *y));
                        self.is_dragging = false;
                        self.drag_target = None;
                    } else if *state == ElementState::Released {
                        if self.is_dragging {
                            if let Some(target_id) = self.drag_target {
                                if let Some(target_ptr) = self.widget_registry.get(&target_id).copied() {
                                    (*target_ptr).handle_event(&Event::DragEnd, self);
                                    (*target_ptr).mark_dirty(self);
                                }
                            }
                            self.active_grab = None;
                        }
                        self.drag_start_pos = None;
                        self.drag_target = None;
                        self.is_dragging = false;
                    }
                }
                Event::PointerMove { x, y, .. } => {
                    if let Some((sx, sy)) = self.drag_start_pos {
                        if let Some(target_id) = self.drag_target {
                            if self.is_dragging {
                                let dx = *x - sx;
                                let dy = *y - sy;
                                if let Some(target_ptr) = self.widget_registry.get(&target_id).copied() {
                                    let (cx, cy, _, _) = (*target_ptr).rect();
                                    let drag_evt = Event::DragUpdate { dx, dy, x: *x, y: *y, local_x: *x - cx, local_y: *y - cy };
                                    let adjusted = (*root).transform_event_for_child(target_ptr, drag_evt, self);
                                    (*target_ptr).handle_event(&adjusted, self);
                                    (*target_ptr).mark_dirty(self);
                                }
                            } else {
                                let dx = *x - sx;
                                let dy = *y - sy;
                                if (dx * dx + dy * dy).sqrt() > 3.0 {
                                    self.is_dragging = true;
                                    self.active_grab = Some(target_id);
                                    if let Some(target_ptr) = self.widget_registry.get(&target_id).copied() {
                                        (*target_ptr).handle_event(&Event::DragStart { start_x: sx, start_y: sy }, self);
                                        (*target_ptr).mark_dirty(self);
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }

            // Normal grab redirection for mouse events if active
            if let Some(grabbed_id) = self.active_grab {
                if let Event::PointerMove { .. }
                | Event::MouseButton { .. }
                | Event::MouseWheel { .. }
                | Event::DragStart { .. }
                | Event::DragUpdate { .. }
                | Event::DragEnd = event
                {
                    if let Some(grabbed_ptr) = self.widget_registry.get(&grabbed_id).copied() {
                        let handled = (*grabbed_ptr).handle_event(event, self);
                        if handled {
                            (*grabbed_ptr).mark_dirty(self);
                        }
                        return handled;
                    }
                }
            }

            if (*root).check_out_of_bounds(event, self) {
                return false;
            }

            // 1. Capture Phase: parent intercepts
            if (*root).capture_event(event, self) {
                (*root).mark_dirty(self);
                return true;
            }

            // For KeyInput, send directly to focused widget if it exists
            if let Event::KeyInput(_) = event {
                if let Some(focused) = self.focused_widget {
                    if (*focused).handle_event(event, self) {
                        (*focused).mark_dirty(self);
                        return true;
                    }
                }
            }

            let mut handled = false;
            let children = (*root).children(self);

            // Determine if we should record a drag target candidate
            let mut check_drag_target = false;
            if let Event::MouseButton { button, state, .. } = event {
                if *button == MouseButton::Left && *state == ElementState::Pressed {
                    check_drag_target = true;
                }
            }

            match event {
                Event::PointerMove { .. } | Event::Tick(_) => {
                    for child in children.into_iter().rev() {
                        let (cx, cy, _, _) = (*child).rect();
                        let mut local_adjusted = event.clone();
                        match &mut local_adjusted {
                            Event::PointerMove { local_x, local_y, .. }
                            | Event::MouseButton { local_x, local_y, .. }
                            | Event::MouseWheel { local_x, local_y, .. }
                            | Event::DragUpdate { local_x, local_y, .. } => {
                                *local_x -= cx;
                                *local_y -= cy;
                            }
                            _ => {}
                        }
                        let adjusted_event = (*root).transform_event_for_child(child, local_adjusted, self);
                        if self.propagate_event_impl(&adjusted_event, child) {
                            handled = true;
                        }
                    }
                    if (*root).handle_event(event, self) {
                        (*root).mark_dirty(self);
                        handled = true;
                    }
                }
                _ => {
                    for child in children.into_iter().rev() {
                        let (cx, cy, _, _) = (*child).rect();
                        let mut local_adjusted = event.clone();
                        match &mut local_adjusted {
                            Event::PointerMove { local_x, local_y, .. }
                            | Event::MouseButton { local_x, local_y, .. }
                            | Event::MouseWheel { local_x, local_y, .. }
                            | Event::DragUpdate { local_x, local_y, .. } => {
                                *local_x -= cx;
                                *local_y -= cy;
                            }
                            _ => {}
                        }
                        let adjusted_event = (*root).transform_event_for_child(child, local_adjusted, self);
                        if self.propagate_event_impl(&adjusted_event, child) {
                            if check_drag_target {
                                if let Some(b) = (*child).base() {
                                    self.drag_target = Some(b.id());
                                }
                            }
                            return true;
                        }
                    }
                    if (*root).handle_event(event, self) {
                        (*root).mark_dirty(self);
                        if check_drag_target {
                            if let Some(b) = (*root).base() {
                                self.drag_target = Some(b.id());
                            }
                        }
                        return true;
                    }
                }
            }
            handled
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.any_dirty
    }

    pub fn clear_dirty(&mut self) {
        self.any_dirty = false;
        for &ptr in self.widget_registry.values() {
            unsafe {
                if let Some(b) = (*ptr).base_mut() {
                    b.dirty = false;
                }
            }
        }
        self.rebuild_spatial_grid();
    }

    pub fn rebuild_spatial_grid(&mut self) {
        self.spatial_grid.clear();
        for (&id, &ptr) in &self.widget_registry {
            unsafe {
                if !ptr.is_null() {
                    let rect = (*ptr).rect();
                    self.spatial_grid.insert(id, rect);
                }
            }
        }
    }

    pub fn register_tick_receiver(&mut self, id: WidgetId) {
        if !self.tick_receivers.contains(&id) {
            self.tick_receivers.push(id);
        }
    }

    pub fn unregister_tick_receiver(&mut self, id: WidgetId) {
        self.tick_receivers.retain(|&x| x != id);
    }

    pub fn is_widget_visible(&self, id: WidgetId) -> bool {
        let mut curr = id;
        loop {
            if let Some(w_ptr) = self.widget_registry.get(&curr) {
                unsafe {
                    if !(*(*w_ptr)).visible() {
                        return false;
                    }
                }
            } else {
                return false;
            }
            if let Some(&parent_id) = self.layout_tree.parents.get(&curr) {
                if let Some(parent_ptr) = self.widget_registry.get(&parent_id) {
                    unsafe {
                        if !(*(*parent_ptr)).is_child_visible(curr) {
                            return false;
                        }
                    }
                }
                curr = parent_id;
            } else {
                break;
            }
        }
        true
    }

    pub fn tick(&mut self, dt: f32) -> bool {
        let mut changed = false;
        let ids = self.tick_receivers.clone();
        for id in ids {
            if self.is_widget_visible(id) {
                if let Some(ptr) = self.widget_registry.get(&id).copied() {
                    unsafe {
                        if (*ptr).tick(dt, self) {
                            (*ptr).mark_dirty(self);
                            changed = true;
                        }
                    }
                }
            }
        }
        changed
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
                    (*old_ptr).handle_event(&Event::FocusOut, self);
                }
                self.focused_widget = Some(new_ptr);
                unsafe {
                    (*new_ptr).handle_event(&Event::FocusIn, self);
                }
            }
        } else {
            self.focused_widget = Some(new_ptr);
            unsafe {
                (*new_ptr).handle_event(&Event::FocusIn, self);
            }
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
                (*ptr).handle_event(&Event::FocusOut, self);
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
        unsafe {
            if !ptr.is_null() && (*ptr).wants_tick() {
                self.register_tick_receiver(id);
            }
        }
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
        self.tick_receivers.clear();
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
        crate::widget::context_menu::is_visible()
    }

    pub fn show_context_menu(&mut self, x: f32, y: f32, options: Vec<String>, header_count: usize, target: *mut (dyn Element + 'static)) {
        crate::widget::context_menu::show(x, y, options, header_count, target);
    }

    pub fn handle_right_click(&mut self, target: *mut (dyn Element + 'static), px: f32, py: f32) {
        if target.is_null() {
            return;
        }
        let name = unsafe { (*target).type_name() };
        let label = unsafe { (*target).label() };
        let header = if let Some(lbl) = label {
            format!("[{}]: {}", name, lbl)
        } else {
            format!("[{}]", name)
        };

        let mut config_info = None;
        if let Some(b) = unsafe { (*target).base() } {
            if let (Some(ref file), Some(ref key)) = (&b.config_file, &b.config_key) {
                config_info = Some((file.clone(), key.clone()));
            }
        }

        let mut options = Vec::new();
        options.push(header);
        let mut header_count = 1;

        if let Some((file, key)) = config_info {
            options.push(format!("File: {}", file));
            options.push(format!("Key: {}", key));
            header_count = 3;
        }

        if name == "TextBox" {
            options.extend(vec!["Cut".to_string(), "Copy".to_string(), "Paste".to_string(), "Select All".to_string()]);
        } else {
            options.extend(vec!["Copy".to_string(), "Paste".to_string()]);
        }

        let scroll_y = crate::widget::hover_animation::get_scroll_offset();
        let adjusted_py = py - scroll_y;
        crate::widget::context_menu::show(px, adjusted_py, options, header_count, target);
    }

    pub fn hide_context_menu(&mut self) {
        crate::widget::context_menu::hide();
    }

    pub fn hit_test_context_menu(&self, px: f32, py: f32) -> bool {
        crate::widget::context_menu::hit_test(px, py)
    }

    pub fn cursor_moved_context_menu(&mut self, px: f32, py: f32) -> bool {
        crate::widget::context_menu::cursor_moved(px, py)
    }

    pub fn mouse_input_context_menu(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        crate::widget::context_menu::mouse_input(button, state, px, py)
    }

    pub fn context_menu_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        crate::widget::context_menu::extra_quads()
    }

    pub fn context_menu_labels(&self) -> Vec<crate::widget::display::TextLabel> {
        crate::widget::context_menu::text_labels()
    }

    pub fn is_movable_backplate_at(&self, px: f32, py: f32) -> bool {
        let mut hit_backplate = false;
        let scroll_y = crate::widget::hover_animation::get_scroll_offset();
        let mut candidate_ids = self.spatial_grid.query(px, py).to_vec();
        if scroll_y != 0.0 {
            candidate_ids.extend_from_slice(self.spatial_grid.query(px, py + scroll_y));
            candidate_ids.sort_unstable();
            candidate_ids.dedup();
        }
        for &id in &candidate_ids {
            if let Some(&ptr) = self.widget_registry.get(&id) {
                unsafe {
                    if !ptr.is_null() {
                        let w = &*ptr;
                        let is_hit = if w.is_backplate() {
                            w.hit_test(px, py, self)
                        } else {
                            w.hit_test(px, py, self) || (scroll_y != 0.0 && w.hit_test(px, py + scroll_y, self))
                        };
                        if is_hit {
                            if w.is_backplate() {
                                if w.is_movable_backplate() {
                                    hit_backplate = true;
                                }
                            } else if w.blocks_backplate_drag() {
                                return false;
                            }
                        }
                    }
                }
            }
        }
        hit_backplate
    }
}
