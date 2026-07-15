use std::collections::HashMap;
use crate::widget::{WidgetHost, WidgetId, Key, NamedKey, MouseButton, ElementState, Event};
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
    /// The widget tree + registry, consolidated into one generational store (Phase 1b of the
    /// core rebuild). Replaces the former `layout_tree` + `widget_registry` maps; see
    /// `scene/tree.rs`.
    pub tree: crate::scene::WidgetTree,
    /// The focused widget's id (Phase 6bc: stored ids, not pointers — a stale id resolves to
    /// `None` through the generational tree instead of dereferencing freed memory).
    pub focused_widget: Option<WidgetId>,
    /// Open-popover registrations, id-keyed like focus (Phase 6bc slice 2).
    pub active_popovers: Vec<WidgetId>,
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
    pub ctrl_pressed: bool,
    pub shift_pressed: bool,
    pub alt_pressed: bool,
    pub logo_pressed: bool,
}

impl UiContext {
    pub fn new() -> Self {
        Self {
            tree: crate::scene::WidgetTree::new(),
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
            ctrl_pressed: false,
            shift_pressed: false,
            alt_pressed: false,
            logo_pressed: false,
        }
    }

    pub fn get_widget(&self, id: WidgetId) -> Option<&(dyn WidgetHost + 'static)> {
        self.tree.get_ptr(id).map(|ptr| unsafe { &*ptr })
    }

    pub fn get_widget_mut(&mut self, id: WidgetId) -> Option<&mut (dyn WidgetHost + 'static)> {
        self.tree.get_ptr(id).map(|ptr| unsafe { &mut *ptr })
    }

    /// Dispatch an event into the tree rooted at `root` — a `WidgetId` resolved through the
    /// registry (the plumbing retype: the router's last raw-pointer API boundary is gone; apps
    /// name roots by id and the registry is the one place a pointer lives). The root must be
    /// registered — apps already register every widget for focus/coverage — and an
    /// unresolvable root is a loud no-op, never a deref.
    pub fn propagate_event(&mut self, event: &Event, root: WidgetId) -> bool {
        let Some(root_ptr) = self.tree.get_ptr(root) else {
            eprintln!("propagate_event: unregistered/stale root {root:?} — event dropped");
            return false;
        };
        if let Event::MouseWheel { .. } = event {
            let now = std::time::Instant::now();
            let elapsed_ms = match self.last_scroll_time {
                None => 999999,
                Some(last) => now.duration_since(last).as_millis(),
            };
            if elapsed_ms >= 5 {
                let is_new_gesture = elapsed_ms > 250;
                if is_new_gesture {
                    self.scroll_initiate_widget_id = None;
                    self.scroll_gesture_new = true;
                } else {
                    self.scroll_gesture_new = false;
                }
                self.last_scroll_time = Some(now);
            }
        }
        if let Event::KeyInput(ref key_event) = event {
            let is_scroll_key = match &key_event.logical_key {
                Key::Named(NamedKey::PageUp)
                | Key::Named(NamedKey::PageDown)
                | Key::Named(NamedKey::Home)
                | Key::Named(NamedKey::End)
                | Key::Named(NamedKey::ArrowUp)
                | Key::Named(NamedKey::ArrowDown) => true,
                _ => false,
            };
            if is_scroll_key {
                let mut handled = false;
                if let Some(focused) = self.focused_widget.and_then(|id| self.tree.get_ptr(id)) {
                    unsafe {
                        if (*focused).handle_event(event, self) {
                            (*focused).mark_dirty(self);
                            handled = true;
                        }
                    }
                }
                if handled {
                    return true;
                }
                let (cx, cy) = self.cursor_pos;
                if let Some(scrollable) = self.find_hovered_scrollable(root_ptr, cx, cy) {
                    unsafe {
                        if (*scrollable).handle_event(event, self) {
                            (*scrollable).mark_dirty(self);
                            return true;
                        }
                    }
                }
            }
        }
        self.propagate_event_impl(event, root_ptr)
    }

    /// The dispatch body. Private — `root` is the registry-resolved pointer from
    /// `propagate_event`, live for the duration of this call.
    fn propagate_event_impl(&mut self, event: &Event, root: *mut (dyn WidgetHost + 'static)) -> bool {
        if let Event::Tick(_) = event {
            return false;
        }
        unsafe {
            // Track drag gestures based on mouse events
            match event {
                Event::MouseButton { button, state, x, y, .. } if *button == MouseButton::Left => {
                    if *state == ElementState::Pressed {
                        // Apps re-dispatch the SAME press to several roots (a plain loop
                        // over their top-level widgets); only the first call for a given
                        // press may reset the drag bookkeeping — a later call would wipe
                        // the target an earlier root just armed, killing the drag before
                        // its first move. drag_start_pos is cleared on release, so an
                        // equal position here means "same press, next root".
                        if self.drag_start_pos != Some((*x, *y)) {
                            // A fresh press while a grab is still armed means the
                            // release never arrived (lost to a focus change or eaten
                            // compositor-side). End the stale drag and drop the grab —
                            // otherwise active_grab redirects every event to the old
                            // target forever and the whole UI stops responding.
                            if self.active_grab.is_some() {
                                if self.is_dragging {
                                    if let Some(target_ptr) = self.drag_target.and_then(|id| self.tree.get_ptr(id)) {
                                        (*target_ptr).handle_event(&Event::DragEnd, self);
                                        (*target_ptr).mark_dirty(self);
                                    }
                                }
                                self.active_grab = None;
                            }
                            self.drag_start_pos = Some((*x, *y));
                            self.is_dragging = false;
                            self.drag_target = None;
                        }
                    } else if *state == ElementState::Released {
                        if self.is_dragging {
                            if let Some(target_id) = self.drag_target {
                                if let Some(target_ptr) = self.tree.get_ptr(target_id) {
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
                                if let Some(target_ptr) = self.tree.get_ptr(target_id) {
                                    let (cx, cy, _, _) = (*target_ptr).rect();
                                    let drag_evt = Event::DragUpdate { dx, dy, x: *x, y: *y, local_x: *x - cx, local_y: *y - cy };
                                    (*target_ptr).handle_event(&drag_evt, self);
                                    (*target_ptr).mark_dirty(self);
                                }
                            } else {
                                let dx = *x - sx;
                                let dy = *y - sy;
                                if (dx * dx + dy * dy).sqrt() > 3.0 {
                                    self.is_dragging = true;
                                    self.active_grab = Some(target_id);
                                    if let Some(target_ptr) = self.tree.get_ptr(target_id) {
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
                    if let Some(grabbed_ptr) = self.tree.get_ptr(grabbed_id) {
                        let handled = (*grabbed_ptr).handle_event(event, self);
                        if handled {
                            (*grabbed_ptr).mark_dirty(self);
                        }
                        return handled;
                    }
                }
            }

            // For KeyInput, send directly to focused widget if it exists
            if let Event::KeyInput(_) = event {
                if let Some(focused) = self.focused_widget.and_then(|id| self.tree.get_ptr(id)) {
                    if (*focused).handle_event(event, self) {
                        (*focused).mark_dirty(self);
                        return true;
                    }
                }
            }

            let mut handled = false;
            let mut children = self.tree.children_ptrs((*root).base().id());
            children.sort_by_key(|&child_ptr| (*child_ptr).z_index());

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
                        if self.propagate_event_impl(&local_adjusted, child) {
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
                        if self.propagate_event_impl(&local_adjusted, child) {
                            if check_drag_target {
                                self.drag_target = Some((*child).base().id());
                            }
                            return true;
                        }
                    }
                    if (*root).handle_event(event, self) {
                        (*root).mark_dirty(self);
                        if check_drag_target {
                            self.drag_target = Some((*root).base().id());
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
        let ptrs: Vec<*mut (dyn WidgetHost + 'static)> =
            self.tree.iter_registered().map(|(_, ptr)| ptr).collect();
        for ptr in ptrs {
            unsafe {
                (*ptr).base_mut().dirty = false;
            }
        }
        self.rebuild_spatial_grid();
    }

    pub fn rebuild_spatial_grid(&mut self) {
        self.spatial_grid.clear();
        let entries: Vec<(WidgetId, *mut (dyn WidgetHost + 'static))> =
            self.tree.iter_registered().collect();
        for (id, ptr) in entries {
            unsafe {
                let rect = (*ptr).rect();
                self.spatial_grid.insert(id, rect);
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
            if let Some(w_ptr) = self.tree.get_ptr(curr) {
                unsafe {
                    if !(*w_ptr).visible() {
                        return false;
                    }
                }
            } else {
                return false;
            }
            if let Some(parent_id) = self.tree.parent_id(curr) {
                if let Some(parent_ptr) = self.tree.get_ptr(parent_id) {
                    unsafe {
                        if !(*parent_ptr).is_child_visible(curr) {
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
                if let Some(ptr) = self.tree.get_ptr(id) {
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

    // --- Focus management (id-keyed; Phase 6bc) ---
    pub fn set_focused(&mut self, w: &mut dyn WidgetHost) {
        let id = w.base().id();
        // Refresh the registry with the pointer we were just handed, so focus on a
        // not-yet-registered widget keeps working (the legacy code stored this pointer
        // directly; the id must resolve for FocusOut/KeyInput dispatch to reach it).
        let new_ptr = unsafe {
            std::mem::transmute::<*mut dyn WidgetHost, *mut (dyn WidgetHost + 'static)>(w as *mut dyn WidgetHost)
        };
        self.tree.register(id, new_ptr);
        self.set_focused_id(id);
    }

    /// Transitional pointer form (TreeList focuses its adapter via `EventCtx::host_ptr`). The
    /// pointer must be live at the call — it is only used to derive the id and refresh the
    /// registry, never stored.
    pub fn set_focused_ptr(&mut self, new_ptr: *mut (dyn WidgetHost + 'static)) {
        if new_ptr.is_null() {
            return;
        }
        let id = unsafe { (*new_ptr).base().id() };
        self.tree.register(id, new_ptr);
        self.set_focused_id(id);
    }

    pub fn set_focused_id(&mut self, id: WidgetId) {
        if let Some(old_id) = self.focused_widget {
            if old_id != id {
                if let Some(old_ptr) = self.tree.get_ptr(old_id) {
                    unsafe {
                        (*old_ptr).unfocus();
                        (*old_ptr).handle_event(&Event::FocusOut, self);
                    }
                }
                self.focused_widget = Some(id);
                if let Some(new_ptr) = self.tree.get_ptr(id) {
                    unsafe {
                        (*new_ptr).handle_event(&Event::FocusIn, self);
                    }
                }
            }
        } else {
            self.focused_widget = Some(id);
            if let Some(new_ptr) = self.tree.get_ptr(id) {
                unsafe {
                    (*new_ptr).handle_event(&Event::FocusIn, self);
                }
            }
        }
    }

    pub fn is_focused(&self, w: &dyn WidgetHost) -> bool {
        self.is_focused_id(w.base().id())
    }

    pub fn is_focused_id(&self, id: WidgetId) -> bool {
        self.focused_widget == Some(id)
    }

    pub fn clear_focus(&mut self) {
        if let Some(id) = self.focused_widget.take() {
            if let Some(ptr) = self.tree.get_ptr(id) {
                unsafe {
                    (*ptr).unfocus();
                    (*ptr).handle_event(&Event::FocusOut, self);
                }
            }
        }
    }

    pub fn clear_if_matches(&mut self, w: &dyn WidgetHost) {
        if self.focused_widget == Some(w.base().id()) {
            self.focused_widget = None;
        }
    }

    pub fn has_focus(&self) -> bool {
        self.focused_widget.is_some()
    }

    // `navigate_focus` (tree-walk ctrl-nav) is DELETED (the plumbing retype): it had
    // zero callers — its `focus::navigate_focus` twin was the one wired up, and that one
    // walked an empty dummy context (provably inert). Section-level keyboard nav lives
    // app-side (settings' focused_section machinery).

    pub fn register_widget(&mut self, id: WidgetId, ptr: *mut (dyn WidgetHost + 'static)) {
        self.tree.register(id, ptr);
        unsafe {
            if !ptr.is_null() && (*ptr).wants_tick() {
                self.register_tick_receiver(id);
            }
        }
    }

    pub fn link_ids(&mut self, parent: WidgetId, child: WidgetId) {
        self.tree.link(parent, child);
    }

    pub fn unlink_child(&mut self, parent: WidgetId, child: WidgetId) {
        self.tree.unlink(parent, child);
    }

    pub fn clear_children_ids(&mut self, parent: WidgetId) {
        self.tree.clear_children(parent);
    }

    pub fn clear_hierarchy(&mut self) {
        self.tree.clear_all();
    }

    // --- Popovers ---
    pub fn clear_popovers(&mut self) {
        self.active_popovers.clear();
    }

    /// Register an open popover. Takes `&mut` so the registry can be refreshed with the
    /// pointer we are handed (the occlusion walks resolve the stored id through the tree).
    pub fn register_popover(&mut self, w: &mut (dyn WidgetHost + 'static)) {
        let id = w.base().id();
        self.tree.register(id, w as *mut (dyn WidgetHost + 'static));
        if !self.active_popovers.contains(&id) {
            self.active_popovers.push(id);
        }
    }

    /// Whether `(px, py)` is covered by an open popover or a popover-carrying widget other
    /// than `query_id` (the querying widget excludes itself). Every widget has a base id
    /// now (the flip) — the old `WidgetId(0)` no-base sentinel is gone.
    pub fn is_coordinate_covered(&self, query_id: WidgetId, px: f32, py: f32) -> bool {
        for &pop_id in self.active_popovers.iter() {
            if pop_id == query_id {
                continue;
            }
            if let Some(ptr) = self.tree.get_ptr(pop_id) {
                unsafe {
                    if let Some((x, y, width, height)) = (*ptr).popover_rect() {
                        if px >= x && px <= x + width && py >= y && py <= y + height {
                            return true;
                        }
                    }
                }
            }
        }
        for (id, ptr) in self.tree.iter_registered() {
            if id == query_id {
                continue;
            }
            unsafe {
                if let Some(w) = ptr.as_ref() {
                    if w.visible() {
                        if let Some((x, y, width, height)) = w.popover_rect() {
                            if px >= x && px <= x + width && py >= y && py <= y + height {
                                return true;
                            }
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

    // NOTE: the animated hover-highlight for this context previously lived here as
    // `tick_hover` / `get_hover_quad`, duplicating the live thread-local implementation in
    // widget/core.rs (`hover_animation`). Both were dead (zero callers workspace-wide) and were
    // removed; the single source of truth is `hover_animation`. This will be folded into the
    // Animated<T> primitive in the core rebuild (see cce-ui/docs/rfc-core-rebuild.md, Phase 4).

    // --- Context Menu ---
    pub fn is_context_menu_visible(&self) -> bool {
        crate::widget::context_menu::is_visible()
    }

    pub fn show_context_menu(&mut self, x: f32, y: f32, options: Vec<String>, header_count: usize, target: *mut (dyn WidgetHost + 'static)) {
        if target.is_null() {
            return;
        }
        let id = unsafe { (*target).base().id() };
        self.tree.register(id, target);
        crate::widget::context_menu::show(x, y, options, header_count, id);
    }

    pub fn handle_right_click(&mut self, target: *mut (dyn WidgetHost + 'static), px: f32, py: f32) {
        if target.is_null() {
            return;
        }
        let name = unsafe { (*target).type_name() };
        let label = if name == "Breadcrumb" {
            unsafe {
                if let Some(bc) = (*target).as_any().downcast_ref::<crate::widget::container::Breadcrumb>() {
                    let idx = bc.right_clicked_seg.unwrap_or(bc.path.len());
                    Some(bc.path_to_seg(idx))
                } else {
                    None
                }
            }
        } else {
            unsafe { (*target).label() }
        };
        let header = if let Some(lbl) = label {
            format!("[{}]: {}", name, lbl)
        } else {
            format!("[{}]", name)
        };

        let mut config_info = None;
        {
            let b = unsafe { (*target).base() };
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
            let is_search = unsafe {
                if let Some(tb) = (*target).as_any().downcast_ref::<crate::widget::input::TextBox>() {
                    tb.placeholder.as_deref() == Some("Search...")
                } else {
                    false
                }
            };
            if is_search {
                options.push("Cear".to_string());
            }
        } else if name == "Breadcrumb" {
            options.push("Copy Path".to_string());
        } else {
            options.extend(vec!["Copy".to_string(), "Paste".to_string()]);
        }

        let scroll_y = crate::widget::hover_animation::get_scroll_offset();
        let adjusted_py = py - scroll_y;
        self.show_context_menu(px, adjusted_py, options, header_count, target);
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
        crate::widget::context_menu::mouse_input(button, state, px, py, Some(self))
    }

    pub fn context_menu_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        crate::widget::context_menu::extra_quads()
    }

    pub fn context_menu_labels(&self) -> Vec<crate::widget::display::TextLabel> {
        crate::widget::context_menu::text_labels()
    }

    /// Whether the point lies inside an OPEN popover's plate. Popovers are drawn on top of
    /// everything and are interactive UI, but they are not spatial-grid widgets — a press
    /// there must never start a window move (the widgets beneath may not block dragging,
    /// e.g. Graph's edge-exclusive canvas hit test).
    fn point_in_active_popover(&self, px: f32, py: f32) -> bool {
        for &pop_id in &self.active_popovers {
            if let Some(ptr) = self.tree.get_ptr(pop_id) {
                unsafe {
                    if let Some((x, y, w, h)) = (*ptr).popover_rect() {
                        if px >= x && px <= x + w && py >= y && py <= y + h {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// The window-drag question (Phase 6: every root `Backplate` is dissolved, so the surface
    /// itself is the movable plate): a drag may start anywhere no drag-blocking widget sits
    /// under the cursor.
    pub fn drag_allowed_at(&self, px: f32, py: f32) -> bool {
        if self.point_in_active_popover(px, py) {
            return false;
        }
        let scroll_y = crate::widget::hover_animation::get_scroll_offset();
        let mut candidate_ids = self.spatial_grid.query(px, py).to_vec();
        if scroll_y != 0.0 {
            candidate_ids.extend_from_slice(self.spatial_grid.query(px, py + scroll_y));
            candidate_ids.sort_unstable();
            candidate_ids.dedup();
        }
        for &id in &candidate_ids {
            if let Some(ptr) = self.tree.get_ptr(id) {
                unsafe {
                    if !ptr.is_null() {
                        let w = &*ptr;
                        let is_hit = w.hit_test(px, py, self)
                            || (scroll_y != 0.0 && w.hit_test(px, py + scroll_y, self));
                        if is_hit && w.blocks_backplate_drag() {
                            return false;
                        }
                    }
                }
            }
        }
        true
    }

    pub fn is_widget_at(&self, px: f32, py: f32) -> bool {
        let scroll_y = crate::widget::hover_animation::get_scroll_offset();
        let mut candidate_ids = self.spatial_grid.query(px, py).to_vec();
        if scroll_y != 0.0 {
            candidate_ids.extend_from_slice(self.spatial_grid.query(px, py + scroll_y));
            candidate_ids.sort_unstable();
            candidate_ids.dedup();
        }
        for &id in &candidate_ids {
            if let Some(ptr) = self.tree.get_ptr(id) {
                unsafe {
                    if !ptr.is_null() {
                        let w = &*ptr;
                        let is_hit = w.hit_test(px, py, self) || (scroll_y != 0.0 && w.hit_test(px, py + scroll_y, self));
                        if is_hit && w.blocks_backplate_drag() {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    fn find_hovered_scrollable(&self, root: *mut (dyn WidgetHost + 'static), cx: f32, cy: f32) -> Option<*mut (dyn WidgetHost + 'static)> {
        unsafe {
            if root.is_null() {
                return None;
            }
            if !(*root).visible() {
                return None;
            }
            if !(*root).hit_test(cx, cy, self) {
                return None;
            }
            for child in self.tree.children_ptrs((*root).base().id()).into_iter().rev() {
                if let Some(scrollable) = self.find_hovered_scrollable(child, cx, cy) {
                    return Some(scrollable);
                }
            }
            if (*root).is_scrollable() {
                return Some(root);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::{WidgetHost, Widget};

    /// The router's drag lifecycle drives the Input drag hooks end-to-end: a routed press
    /// records the drag target, the first >3px move synthesizes DragStart, further moves
    /// deliver DragUpdate (the slider value follows), and the release delivers DragEnd.
    /// Regression test for the silent-drop gap: `Input::on_event` defaults ignore Drag*
    /// events, so `Adapted::handle_event` must map them onto the hooks itself.
    #[test]
    fn routed_drag_reaches_input_drag_hooks() {
        use crate::widget::{ElementState, Event, MouseButton, Slider};

        let mut ctx = UiContext::new();
        let mut slider = Slider::new();
        WidgetHost::set_rect(&mut slider, 0.0, 0.0, 200.0, 30.0);
        let ptr = slider.as_ptr_mut();
        let id = slider.base().id();
        ctx.register_widget(id, ptr);

        let press = Event::MouseButton {
            button: MouseButton::Left,
            state: ElementState::Pressed,
            x: 100.0,
            y: 15.0,
            local_x: 100.0,
            local_y: 15.0,
        };
        assert!(ctx.propagate_event(&press, id), "press in the track arms the drag");
        assert!(slider.is_dragging());
        let v0 = slider.value;

        // First move past the 3px threshold starts the drag; the next one updates it.
        let mv = |x: f32| Event::PointerMove { x, y: 15.0, local_x: x, local_y: 15.0 };
        ctx.propagate_event(&mv(110.0), id);
        assert!(ctx.is_dragging, "router crossed the drag threshold");
        ctx.propagate_event(&mv(140.0), id);
        assert!(
            slider.value > v0 + 0.05,
            "DragUpdate reached Input::drag_update (value {} -> {})",
            v0,
            slider.value
        );

        let release = Event::MouseButton {
            button: MouseButton::Left,
            state: ElementState::Released,
            x: 140.0,
            y: 15.0,
            local_x: 140.0,
            local_y: 15.0,
        };
        ctx.propagate_event(&release, id);
        assert!(!slider.is_dragging(), "DragEnd reached Input::drag_end");
        assert!(!ctx.is_dragging);
    }

    /// The multi-root press dispatch (how apps actually loop: one press propagated to
    /// EVERY top-level root, no break): a later root's propagate call must not wipe the
    /// drag target an earlier root just armed. This was live-broken in every plain-loop
    /// app (the demo, colors) while the single-root test above passed — found the first
    /// time a held drag could be driven headlessly (ccectl pointer-press).
    #[test]
    fn multi_root_press_dispatch_keeps_the_drag_target() {
        let mut ctx = UiContext::new();
        let mut slider = crate::widget::Slider::new().with_value(0.5);
        let (id, ptr) = (slider.id(), slider.as_ptr_mut());
        ctx.register_widget(id, ptr);
        slider.set_rect(0.0, 0.0, 200.0, 30.0);
        let mut other = Block { base: Widget::new_rect(300.0, 300.0, 50.0, 50.0) };
        let other_ptr = &mut other as *mut _ as *mut (dyn crate::widget::WidgetHost + 'static);
        let other_id = other.base.id();
        ctx.register_widget(other_id, other_ptr);

        let press = Event::MouseButton {
            button: MouseButton::Left,
            state: ElementState::Pressed,
            x: 100.0,
            y: 15.0,
            local_x: 100.0,
            local_y: 15.0,
        };
        // The app loop: same press to both roots, slider first.
        assert!(ctx.propagate_event(&press, id));
        ctx.propagate_event(&press, other_id);
        assert_eq!(ctx.drag_target, Some(id), "the second root's call must not wipe the armed target");

        let v0 = slider.value;
        let mv = |x: f32| Event::PointerMove { x, y: 15.0, local_x: x, local_y: 15.0 };
        for root in [id, other_id] {
            ctx.propagate_event(&mv(110.0), root);
        }
        for root in [id, other_id] {
            ctx.propagate_event(&mv(140.0), root);
        }
        assert!(ctx.is_dragging, "threshold crossed despite multi-root dispatch");
        assert!(slider.value > v0 + 0.05, "DragUpdate drove the slider ({} -> {})", v0, slider.value);

        let release = Event::MouseButton {
            button: MouseButton::Left,
            state: ElementState::Released,
            x: 140.0,
            y: 15.0,
            local_x: 140.0,
            local_y: 15.0,
        };
        for root in [id, other_id] {
            ctx.propagate_event(&release, root);
        }
        assert!(!slider.is_dragging());
        assert!(!ctx.is_dragging);
    }

    /// A plain drag-blocking widget (the `WidgetHost` default) at a fixed rect.
    struct Block {
        base: Widget,
    }
    impl WidgetHost for Block {
        crate::impl_widget_base!(Block);
        fn color(&self) -> [f32; 4] {
            [0.0, 0.0, 0.0, 1.0]
        }
    }

    /// `drag_allowed_at` — the window-drag question: allowed on empty surface, denied over a
    /// drag-blocking widget.
    #[test]
    fn drag_allowed_everywhere_except_blocking_widgets() {
        let mut ctx = UiContext::new();
        let mut w = Block { base: Widget::new_rect(10.0, 10.0, 50.0, 50.0) };
        let ptr = &mut w as *mut _ as *mut (dyn crate::widget::WidgetHost + 'static);
        ctx.register_widget(w.base.id(), ptr);
        ctx.rebuild_spatial_grid();

        assert!(ctx.drag_allowed_at(200.0, 200.0), "empty surface is draggable");
        assert!(!ctx.drag_allowed_at(20.0, 20.0), "a drag-blocking widget denies the drag");
    }
}
