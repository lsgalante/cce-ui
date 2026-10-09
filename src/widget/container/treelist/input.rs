//! Input: the press, motion and key handlers behind `impl Input`.

use super::*;

impl Input for TreeList {
    fn focus_role(&self) -> crate::widget::FocusRole {
        crate::widget::FocusRole::Well
    }
    fn blocks_root_plate_drag(&self) -> bool {
        true
    }

    /// The tree must see every press: outside presses dismiss the add-key popover and
    /// commit/cancel the inline rename editor (the legacy ungated `mouse_input` contract).
    fn gates_presses(&self) -> bool {
        false
    }

    fn wants_tick(&self) -> bool {
        true
    }

    fn draggable(&self, _rect: Rect) -> bool {
        self.scroll_box.draggable()
    }

    fn is_dragging(&self) -> bool {
        self.scroll_box.is_dragging()
    }

    fn drag_begin(&mut self, px: f32, py: f32, _rect: Rect) {
        self.scroll_box.drag_begin(px, py);
    }

    fn drag_update(&mut self, px: f32, py: f32, _rect: Rect) -> bool {
        self.scroll_box.drag_update(px, py)
    }

    fn drag_end(&mut self) {
        self.scroll_box.drag_end();
    }

    /// The legacy `tick` body: advances the field widgets, drains the add-key popover and
    /// search box, positions/commits the inline rename editor (re-targeting focus to the
    /// adapter on commit), and runs the scrollbar activity fade.
    fn tick_ctx(&mut self, dt: f32, ectx: &mut EventCtx) -> bool {
        let host_id = ectx.id;
        let Some(ui) = ectx.ui.as_deref_mut() else {
            return false;
        };
        let mut changed = false;
        if self.search_box.lend(ui, |w, ui| w.tick(dt, ui)) == Some(true) {
            changed = true;
        }
        if self.add_key_btn.lend(ui, |w, ui| w.tick(dt, ui)) == Some(true) {
            changed = true;
        }
        if self.add_key_popover_open {
            if self.add_key_popover_box.lend(ui, |w, ui| w.tick(dt, ui)) == Some(true) {
                changed = true;
            }
            if !self.add_key_popover_box.get(ui).editing {
                let path = self.add_key_popover_box.get(ui).text.trim().to_string();
                if !path.is_empty() {
                    self.new_key_path_request = Some(path);
                }
                self.add_key_popover_open = false;
                ui.clear_focus();
                changed = true;
            }
        }
        if self.search_box.get_mut(ui).take_change() {
            self.query = self.search_box.get(ui).text.clone();
            self.rebuild_tree();
            changed = true;
        }
        
        if self.editing_key_idx.is_some() {
            if self.edit_box.lend(ui, |w, ui| w.tick(dt, ui)) == Some(true) {
                changed = true;
            }
            if let Some(row_idx) = self.editing_key_idx {
                if row_idx < self.items.len() {
                    let list_left = self.scroll_box.base.x;
                    let list_top = self.scroll_box.viewport_y;
                    let row_y = list_top + row_idx as f32 * self.item_height - self.scroll_box.scroll_y;
                    let box_x = list_left + 5.0;
                    let box_y = row_y + 2.0;
                    self.edit_box.get_mut(ui).set_rect(box_x, box_y, 170.0, 24.0);
                }
            }
            if !self.edit_box.get(ui).editing {
                let row_idx = self.editing_key_idx.unwrap();
                if row_idx < self.items.len() {
                    let (old_path, relative_name) = match &self.items[row_idx] {
                        TreeElement::Section { path, name, .. } => (path.clone(), name.clone()),
                        TreeElement::Leaf { path, name, .. } => (path.clone(), name.clone()),
                    };
                    let new_name = self.edit_box.get(ui).text.trim().to_string();
                    if !new_name.is_empty() && new_name != relative_name {
                        let new_path = if let Some(pos) = old_path.rfind('.') {
                            format!("{}.{}", &old_path[..pos], new_name)
                        } else {
                            new_name
                        };
                        self.rename_request = Some((old_path, new_path));
                    }
                }
                self.editing_key_idx = None;
                // Undo the register+link done when editing began (see `mouse_body`). The paint
                // (`if self.editing_key_idx.is_some()`) and the `set_rect` beside it are both
                // gated on editing, but the tree link was not — so leaving it attached parked a
                // 170x24 child at the last-edited row's screen coordinates that kept its stale
                // rect, kept hit-testing (visible, gates_presses default true) and swallowed the
                // press before `mouse_body` ever ran: an invisible dead zone that ate row clicks
                // and silently re-entered editing on an unpainted box. Each rename also minted a
                // fresh TextBox id into the same field, so the child list grew monotonically.
                let eb_id = self.edit_box.id();
                ui.unlink_child(host_id, eb_id);
                self.edit_box.detach(ui);
                ui.claim_focus(host_id);
                self.focused = true;
                changed = true;
            }
        }

        // The list's own glide/coast (wheel notches and trackpad flicks land
        // in the ScrollBox; only its tick moves the drawn offset).
        if self.scroll_box.tick(dt, ui) {
            changed = true;
        }
        // A scroll the tree made itself (following the selection) raises the
        // bar as a wheel would.
        if (self.scroll_box.scroll_y - self.last_scroll_y).abs() > 0.01 {
            self.last_scroll_y = self.scroll_box.scroll_y;
            self.scroll_box.notify_scrolled();
            changed = true;
        }
        changed
    
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        let host_id = ectx.id;
        match event {
            Event::MouseButton { button, state, x, y, .. } => {
                let (button, state, px, py) = (*button, *state, *x, *y);
                let Some(ui) = ectx.ui.as_deref_mut() else { return false; };
                self.mouse_body(button, state, px, py, ui, host_id)
            }
            Event::PointerMove { x, y, .. } => {
                let (px, py) = (*x, *y);
                let Some(ui) = ectx.ui.as_deref_mut() else { return false; };
                self.move_body(px, py, ui)
            }
            Event::MouseWheel { delta, x, y, .. } => {
                let (delta, px, py) = (*delta, *x, *y);
                let Some(ui) = ectx.ui.as_deref_mut() else { return false; };
                self.scroll_box.mouse_wheel(&delta, px, py, ui)
            }
            Event::KeyInput(ev) => {
                let ev = ev.clone();
                let Some(ui) = ectx.ui.as_deref_mut() else { return false; };
                self.key_body(&ev, ui)
            }
            Event::FocusIn => {
                self.focused = true;
                false
            }
            Event::FocusOut => {
                self.focused = false;
                self.add_key_popover_open = false;
                match ectx.ui.as_deref_mut() {
                    Some(ui) => self.add_key_popover_box.get_mut(ui).unfocus(),
                    None => {
                        if let Some(b) = self.add_key_popover_box.here_mut() {
                            b.unfocus();
                        }
                    }
                }
                false
            }
            _ => false,
        }
    }

    fn context_action(&mut self, action: crate::widget::ContextAction) -> bool {
        use crate::widget::ContextAction as CA;
        match action {
            CA::CopyKey => self.copy_key(),
            CA::CopyValue => self.copy_value(),
            CA::DeleteKey => self.delete_key(),
            CA::ExpandNode => self.expand_node(),
            CA::CollapseNode => self.collapse_node(),
            CA::ExpandAll => self.expand_all_nodes(),
            CA::CollapseAll => self.collapse_all_nodes(),
            _ => return false,
        }
        true
    }
}

impl TreeList {

    pub(super) fn mouse_body(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ui: &mut UiContext, host_id: WidgetId) -> bool {
        let _ = host_id;
        if self.editing_key_idx.is_some() {
            if button == MouseButton::Left && state == ElementState::Pressed {
                let (ex, ey, ew, eh) = self.edit_box.get(ui).rect();
                if px >= ex && px <= ex + ew && py >= ey && py <= ey + eh {
                    if self.edit_box.lend(ui, |w, ui| w.mouse_input(button, state, px, py, ui)) == Some(true) {
                        return true;
                    }
                } else {
                    ui.clear_focus();
                    return true;
                }
            }
            return false;
        }

        let mut changed = false;

        if self.add_key_popover_open {
            let (px_rect, py_rect, pw, ph) = self.popover_rect_geom();
            if button == MouseButton::Left && state == ElementState::Pressed {
                if px < px_rect || px > px_rect + pw || py < py_rect || py > py_rect + ph {
                    self.add_key_popover_open = false;
                    ui.clear_focus();
                    changed = true;
                } else {
                    if self.add_key_popover_box.lend(ui, |w, ui| w.mouse_input(button, state, px, py, ui)) == Some(true) {
                        ui.set_focused_id(self.add_key_popover_box.id());
                        changed = true;
                    }
                }
            }
            let (px_rect, py_rect, pw, ph) = self.popover_rect_geom();
            if px >= px_rect && px <= px_rect + pw && py >= py_rect && py <= py_rect + ph {
                return true;
            }
        }

        if self.scroll_box.mouse_input(button, state, px, py, ui) {
            changed = true;
        }
        if self.search_box.lend(ui, |w, ui| w.mouse_input(button, state, px, py, ui)) == Some(true) {
            ui.set_focused_id(self.search_box.id());
            changed = true;
        }
        if let Some(Some(clicked)) = self.add_key_btn.lend(ui, |w, ui| w.mouse_input(button, state, px, py, ui).then(|| w.take_click())) {
            if clicked {
                self.add_key_popover_open = !self.add_key_popover_open;
                if self.add_key_popover_open {
                    let b = self.add_key_popover_box.get_mut(ui);
                    WidgetHost::set_visible(b, true);
                    b.text.clear();
                    b.edit_buffer.clear();
                    b.cursor_idx = 0;
                    b.select_anchor = None;
                    b.all_selected = false;
                    b.editing = true;
                    ui.set_focused_id(self.add_key_popover_box.id());
                    self.add_key_popover_box.get_mut(ui).focus();
                } else {
                    ui.clear_focus();
                }
            }
            changed = true;
        }
        
        let list_left = self.scroll_box.base.x;
        let list_width = self.scroll_box.base.w;
        let list_top = self.scroll_box.viewport_y;
        let list_bottom = self.scroll_box.viewport_y + self.scroll_box.viewport_h;

        // The tree receives every press UNGATED (see gates_presses) for its
        // dismiss/commit semantics, which skips the router's popover-coverage
        // check — honor it here for row interactions, or a click on a menu
        // floating over the tree (the File dropdown) also selects the row
        // beneath it. The dismiss paths above deliberately stay: a covered
        // press IS an outside press for the rename editor and add-key popover.
        let covered = ui.is_coordinate_covered(host_id, px, py);

        if button == MouseButton::Left && state == ElementState::Pressed && !covered {
            let on_scrollbar = self.scroll_box.hit_test_scrollbar(px, py) || self.scroll_box.scrollbar_dragging;
            if !on_scrollbar && px >= list_left && px <= list_left + list_width && py >= list_top && py <= list_bottom {
                // Focus claimed from inside this event (`UiContext::claim_focus`): a FocusIn
                // through the registry would re-enter this widget while it holds `&mut self`.
                ui.claim_focus(host_id);
                self.focused = true;
                let relative_y = py - list_top + self.scroll_box.scroll_y;
                let row_idx = (relative_y / self.item_height) as usize;
                if row_idx < self.items.len() {
                    let item = self.items[row_idx].clone();
                    
                    let mut is_double = false;
                    let now = web_time::Instant::now();
                    if let Some((prev_time, prev_row)) = self.double_click_timer {
                        if prev_row == row_idx && now.duration_since(prev_time).as_millis() < 300 {
                            is_double = true;
                        }
                    }
                    self.double_click_timer = Some((now, row_idx));

                    if is_double {
                        let (_path_to_edit, relative_name) = match &item {
                            TreeElement::Section { path, name, .. } => {
                                if self.collapsed_sections.contains(path) {
                                    self.collapsed_sections.remove(path);
                                } else {
                                    self.collapsed_sections.insert(path.clone());
                                }
                                self.rebuild_tree();
                                (path.clone(), name.clone())
                            }
                            TreeElement::Leaf { path, name, .. } => (path.clone(), name.clone()),
                        };
                        self.editing_key_idx = Some(row_idx);
                        // A fresh editor for this rename, the context's while it is up
                        // (linked under the tree) and given back when it commits (`tick_ctx`).
                        self.edit_box.detach(ui);
                        let mut eb = TextBox::new(relative_name).with_multiline(false).with_draw_bg_border(true);
                        eb.editing = true;
                        eb.cursor_idx = eb.text.chars().count();
                        eb.select_anchor = Some(0);
                        self.edit_box = Embedded::new(eb);
                        self.edit_box.attach(ui);

                        let eb_id = self.edit_box.id();
                        ui.link_ids(host_id, eb_id);
                        ui.set_focused_id(eb_id);
                        return true;
                    }

                    match item {
                        TreeElement::Section { ref path, .. } => {
                            if self.collapsed_sections.contains(path) {
                                self.collapsed_sections.remove(path);
                            } else {
                                self.collapsed_sections.insert(path.clone());
                            }
                            self.rebuild_tree();
                            self.clicked_item = Some(TreeElement::Section {
                                path: path.clone(),
                                name: String::new(),
                                indent: 0,
                                collapsed: self.collapsed_sections.contains(path),
                            });
                            changed = true;
                        }
                        TreeElement::Leaf { original_idx, ref path, ref name, indent, ref val } => {
                            self.selected_key_idx = Some(original_idx);
                            self.clicked_item = Some(TreeElement::Leaf {
                                path: path.clone(),
                                name: name.clone(),
                                indent,
                                val: val.clone(),
                                original_idx,
                            });
                            changed = true;
                        }
                    }
                }
            }
        }

        if button == MouseButton::Right && state == ElementState::Pressed && !covered
            && px >= list_left && px <= list_left + list_width && py >= list_top && py <= list_bottom {
                // Focus claimed from inside this event (`UiContext::claim_focus`): a FocusIn
                // through the registry would re-enter this widget while it holds `&mut self`.
                ui.claim_focus(host_id);
                self.focused = true;
                let relative_y = py - list_top + self.scroll_box.scroll_y;
                let row_idx = (relative_y / self.item_height) as usize;
                if row_idx < self.items.len() {
                    let item = self.items[row_idx].clone();
                    match item {
                        TreeElement::Section { ref path, collapsed, .. } => {
                            self.right_clicked_section = Some(path.clone());
                            
                            use crate::widget::ContextAction as CA;
                            let rows = vec![
                                (path.clone(), None),
                                if collapsed { (tr("tree-expand"), Some(CA::ExpandNode)) } else { (tr("tree-collapse"), Some(CA::CollapseNode)) },
                                (tr("tree-expand-all"), Some(CA::ExpandAll)),
                                (tr("tree-collapse-all"), Some(CA::CollapseAll)),
                            ];

                            let scroll_offset = crate::widget::hover_animation::get_scroll_offset();
                            ui.show_context_menu_rows(px, py - scroll_offset, rows, 1, host_id);
                            changed = true;
                        }
                        TreeElement::Leaf { original_idx, ref path, ref name, indent, ref val } => {
                            self.selected_key_idx = Some(original_idx);
                            self.clicked_item = Some(TreeElement::Leaf {
                                path: path.clone(),
                                name: name.clone(),
                                indent,
                                val: val.clone(),
                                original_idx,
                            });
                            
                            use crate::widget::ContextAction as CA;
                            let rows = vec![
                                (path.clone(), None),
                                (tr("tree-copy-key"), Some(CA::CopyKey)),
                                (tr("tree-copy-value"), Some(CA::CopyValue)),
                                (tr("tree-delete"), Some(CA::DeleteKey)),
                            ];
                            let scroll_offset = crate::widget::hover_animation::get_scroll_offset();
                            ui.show_context_menu_rows(px, py - scroll_offset, rows, 1, host_id);
                            changed = true;
                        }
                    }
                }
            }
        changed
    
    }

    pub(super) fn move_body(&mut self, px: f32, py: f32, ui: &mut UiContext) -> bool {
        let mut changed = self.scroll_box.on_cursor_moved(px, py, ui);
        if self.search_box.lend(ui, |w, ui| w.on_cursor_moved(px, py, ui)) == Some(true) {
            changed = true;
        }
        if self.add_key_btn.lend(ui, |w, ui| w.on_cursor_moved(px, py, ui)) == Some(true) {
            changed = true;
        }
        if self.add_key_popover_open
            && self.add_key_popover_box.lend(ui, |w, ui| w.on_cursor_moved(px, py, ui)) == Some(true) {
                changed = true;
            }

        let list_left = self.scroll_box.base.x;
        let list_width = self.scroll_box.base.w;
        let list_top = self.scroll_box.viewport_y;
        let list_bottom = self.scroll_box.viewport_y + self.scroll_box.viewport_h;

        let old_hovered = self.hovered_row_idx;
        self.hovered_row_idx = None;

        let on_scrollbar = self.scroll_box.hit_test_scrollbar(px, py) || self.scroll_box.scrollbar_dragging;
        if !on_scrollbar && px >= list_left && px <= list_left + list_width && py >= list_top && py <= list_bottom {
            let relative_y = py - list_top + self.scroll_box.scroll_y;
            let row_idx = (relative_y / self.item_height) as usize;
            if row_idx < self.items.len() {
                self.hovered_row_idx = Some(row_idx);
            }
        }

        if old_hovered != self.hovered_row_idx {
            changed = true;
        }
        changed
    
    }

    pub(super) fn key_body(&mut self, event: &KeyEvent, ui: &mut UiContext) -> bool {
        if self.editing_key_idx.is_some()
            && self.edit_box.lend(ui, |w, ui| w.keyboard_input(event, ui)) == Some(true) {
                return true;
            }
        if self.add_key_popover_open
            && self.add_key_popover_box.lend(ui, |w, ui| w.keyboard_input(event, ui)) == Some(true) {
                return true;
            }
        if self.search_box.lend(ui, |w, ui| w.keyboard_input(event, ui)) == Some(true) {
            return true;
        }
        false
    
    }
}
