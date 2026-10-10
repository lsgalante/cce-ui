//! A `MenuBar`'s input: its hit, presses on the title, the strip and the dropdowns, the pointer
//! moving over them, and keys.

use super::*;

impl Input for MenuBar {
    fn blocks_root_plate_drag(&self) -> bool {
        false
    }

    /// Legacy `mouse_input` saw every press: any press closes an open context dropdown, even
    /// outside the bar.
    fn gates_presses(&self) -> bool {
        false
    }

    fn hit(&self, rect: Rect, px: f32, py: f32) -> bool {
        if !self.visible {
            return false;
        }
        if let Some((dx, dy, dw, dh)) = self.context_popover_rect(rect) {
            if px >= dx && px < dx + dw && py >= dy && py < dy + dh {
                return true;
            }
        }
        if let Some((dx, dy, dw, dh)) = self.menu_dropdown_rect() {
            if px >= dx && px < dx + dw && py >= dy && py < dy + dh {
                return true;
            }
        }
        if px >= rect.x && px <= rect.x + rect.width && py >= rect.y && py <= rect.y + rect.height {
            return true;
        }
        // The strip may extend past the assigned rect (clamped layouts).
        let (sx, sy, sw, sh) = self.menus.rect();
        px >= sx && px <= sx + sw && py >= sy && py <= sy + sh
    }

    fn set_modifiers(&mut self, ctrl: bool, shift: bool, alt: bool) {
        self.menus.set_modifiers(ctrl, shift, alt);
    }

    fn visibility_changed(&mut self, visible: bool) {
        self.visible = visible;
        self.menus.set_visible(visible);
        self.layout_dirty = true;
    }

    fn is_focused(&self, _base_focused: bool) -> bool {
        self.focused || self.context_dropdown_open || self.menus.selected.is_some()
    }

    fn set_selected(&mut self, selected: bool) {
        self.focused = selected;
        if !selected {
            self.menus.inner_mut().set_selected(None);
        }
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::PointerMove { x: px, y: py, .. } => {
                if !self.visible {
                    return false;
                }
                let (px, py) = (*px, *py);
                let rect = ectx.rect;
                self.layout_strip(rect);

                let mut changed = false;

                let old_title_hovered = self.context_title_hovered;
                self.context_title_hovered = false;
                if !self.context_options.is_empty() {
                    let tr = self.title_rect(rect);
                    if px >= tr.0 && px <= tr.0 + tr.2 && py >= tr.1 && py <= tr.1 + tr.3 {
                        self.context_title_hovered = true;
                    }
                }
                if old_title_hovered != self.context_title_hovered {
                    changed = true;
                }

                let old_hovered_item = self.context_hovered_item;
                self.context_hovered_item = None;
                if let Some((dx, dy, dw, dh)) = self.context_popover_rect(rect) {
                    if px >= dx && px < dx + dw && py >= dy && py < dy + dh {
                        let di = ((py - dy) / DROPDOWN_ITEM_H) as usize;
                        if di < self.context_options.len() {
                            self.context_hovered_item = Some(di);
                        }
                    }
                }
                if old_hovered_item != self.context_hovered_item {
                    changed = true;
                }

                let old_hovered_dropdown = self.hovered_dropdown_item;
                self.hovered_dropdown_item = None;
                if let Some((dx, dy, dw, dh)) = self.menu_dropdown_rect() {
                    if px >= dx && px < dx + dw && py >= dy && py < dy + dh {
                        let di = ((py - dy) / DROPDOWN_ITEM_H) as usize;
                        if let Some(menu_idx) = self.menus.selected {
                            if let Some(items) = self.menu_dropdowns.get(menu_idx) {
                                if di < items.len() {
                                    self.hovered_dropdown_item = Some(di);
                                }
                            }
                        }
                    }
                }
                if old_hovered_dropdown != self.hovered_dropdown_item {
                    changed = true;
                }

                if let Some(ui) = ectx.ui.as_deref_mut() {
                    if self.menus.cursor_moved(px, py, ui) {
                        changed = true;
                    }
                }
                changed
            }
            Event::MouseButton { button, state, x: px, y: py, .. } => {
                if !self.visible {
                    return false;
                }
                if *button != MouseButton::Left {
                    return false;
                }
                let (px, py, state) = (*px, *py, *state);
                let rect = ectx.rect;
                self.layout_strip(rect);

                let mut changed = false;

                if let Some((dx, dy, dw, dh)) = self.menu_dropdown_rect() {
                    if px >= dx && px < dx + dw && py >= dy && py < dy + dh {
                        if state == ElementState::Pressed {
                            let di = ((py - dy) / DROPDOWN_ITEM_H) as usize;
                            if let Some(menu_idx) = self.menus.selected {
                                if let Some(items) = self.menu_dropdowns.get(menu_idx) {
                                    if di < items.len() {
                                        self.clicked_dropdown_item = Some((menu_idx, di));
                                        self.close_all();
                                        ectx.release_focus();
                                        return true;
                                    }
                                }
                            }
                        }
                        return true;
                    }
                }

                if let Some((dx, dy, dw, dh)) = self.context_popover_rect(rect) {
                    if px >= dx && px < dx + dw && py >= dy && py < dy + dh
                        && state == ElementState::Pressed {
                            let di = ((py - dy) / DROPDOWN_ITEM_H) as usize;
                            if di < self.context_options.len() {
                                self.context_selected = di;
                                self.context_just_changed = true;
                                self.close_all();
                                ectx.release_focus();
                                if let Some(ref cb) = self.on_context_change_cb {
                                    cb(di);
                                }
                                return true;
                            }
                        }
                }

                if !self.context_options.is_empty() {
                    let tr = self.title_rect(rect);
                    if px >= tr.0 && px <= tr.0 + tr.2 && py >= tr.1 && py <= tr.1 + tr.3 {
                        if state == ElementState::Pressed {
                            if self.context_dropdown_open {
                                self.close_all();
                                ectx.release_focus();
                            } else {
                                self.menus.unfocus();
                                self.context_dropdown_open = true;
                                self.sync_focus(ectx);
                            }
                        }
                        return true;
                    }
                }

                if self.context_dropdown_open && state == ElementState::Pressed {
                    self.close_all();
                    ectx.release_focus();
                    changed = true;
                }

                let old_menu_selected = self.menus.selected;
                if let Some(ui) = ectx.ui.as_deref_mut() {
                    if self.menus.mouse_input(MouseButton::Left, state, px, py, ui) {
                        changed = true;
                        if self.menus.selected.is_some() && old_menu_selected != self.menus.selected {
                            self.sync_focus(ectx);
                        }
                    }
                }
                changed
            }
            Event::KeyInput(key_event) => {
                if key_event.state != ElementState::Pressed {
                    return false;
                }
                if self.context_dropdown_open {
                    match key_event.logical_key {
                        Key::Named(NamedKey::ArrowDown) => {
                            let current = self.context_hovered_item.unwrap_or(self.context_selected);
                            if current + 1 < self.context_options.len() {
                                self.context_hovered_item = Some(current + 1);
                            } else {
                                self.context_hovered_item = Some(0);
                            }
                            return true;
                        }
                        Key::Named(NamedKey::ArrowUp) => {
                            let current = self.context_hovered_item.unwrap_or(self.context_selected);
                            if current > 0 {
                                self.context_hovered_item = Some(current - 1);
                            } else {
                                self.context_hovered_item = Some(self.context_options.len() - 1);
                            }
                            return true;
                        }
                        Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                            if let Some(idx) = self.context_hovered_item {
                                self.context_selected = idx;
                                self.context_just_changed = true;
                            }
                            self.close_all();
                            ectx.release_focus();
                            return true;
                        }
                        Key::Named(NamedKey::Escape) => {
                            self.close_all();
                            ectx.release_focus();
                            return true;
                        }
                        _ => {}
                    }
                }
                match ectx.ui.as_deref_mut() {
                    Some(ui) => self.menus.keyboard_input(key_event, ui),
                    None => false,
                }
            }
            // Hosts call `focus()`/`unfocus()` directly; legacy semantics: focus is claimed
            // conditionally (only while something is open), unfocus closes everything.
            Event::FocusIn => {
                self.sync_focus(ectx);
                true
            }
            Event::FocusOut => {
                self.close_all();
                ectx.release_focus();
                true
            }
            _ => false,
        }
    }

}
