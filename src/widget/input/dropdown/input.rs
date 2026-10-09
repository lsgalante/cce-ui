//! Input: keys while the list is open or the trigger focused, and `impl Input`.

use super::*;

impl Input for Dropdown {
    fn focus_role(&self) -> crate::widget::FocusRole {
        crate::widget::FocusRole::Plate
    }
    /// The legacy geometric test: the widget rect (edges inclusive), extended to the open
    /// popover. `rect` is the full base rect (label strip included), as legacy `hit_test` used.
    fn hit(&self, rect: Rect, x: f32, y: f32) -> bool {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return false;
        }
        let hit_trigger =
            x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height;
        if self.open && !self.closing {
            let top = self.label_top();
            let content = Rect { x: rect.x, y: rect.y + top, width: rect.width, height: rect.height - top };
            let (rx, ry, rw, rh) = self.popover_geom(content);
            let hit_popover = x >= rx && x <= rx + rw && y >= ry && y <= ry + rh;
            hit_trigger || hit_popover
        } else {
            hit_trigger
        }
    }

    fn opens_context_menu(&self) -> bool {
        true
    }

    /// Ungated presses (legacy `mouse_input` saw every press): an open dropdown must close on
    /// an outside click it would otherwise never learn about.
    fn gates_presses(&self) -> bool {
        false
    }

    /// Wall-clock animation bookkeeping: report "changed" while a transition is
    /// in flight (drives redraws where the app's UiContext gets ticked) and
    /// settle a landed close.
    fn tick(&mut self, _dt: f32, _rect: Rect) -> bool {
        let animating = self.anim_start.is_some();
        self.settle_anim();
        animating
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        self.settle_anim();
        match event {
            Event::MouseButton {
                button: MouseButton::Left,
                state: ElementState::Released,
                ..
            } => {
                // While the menu is open (or shrinking closed), the paired
                // release of any press this dropdown handled must not leak to
                // widgets stacked beneath the popover: an unconsumed release
                // falling through the host's dispatch landed on the button
                // whose row the menu covers — in the designer's params pane,
                // picking an "Open" entry fired the Save As button underneath
                // and opened a save chooser on top of the load.
                self.open || self.closing
            }
            Event::MouseButton {
                button: MouseButton::Left,
                state: ElementState::Pressed,
                x: px,
                y: py,
                ..
            } => {
                let content = ectx.rect;
                let top = self.label_top();
                let (bx, by, bw, bh) = (content.x, content.y - top, content.width, content.height + top);
                let (rx, ry, rw, rh) = self.popover_geom(content);

                let inside_trigger = *px >= bx && *px <= bx + bw && *py >= by && *py <= by + bh;
                let inside_popover = self.open
                    && !self.closing
                    && *px >= rx && *px <= rx + rw && *py >= ry && *py <= ry + rh;

                if std::env::var_os("CCE_DD_DEBUG").is_some() {
                    eprintln!(
                        "[dd] press ({px},{py}) rect=({:.0},{:.0},{:.0},{:.0}) popover=({rx:.0},{ry:.0},{rw:.0},{rh:.0}) open={} in_trig={inside_trigger} in_pop={inside_popover}",
                        content.x, content.y, content.width, content.height, self.open
                    );
                }
                if inside_popover {
                    if let Some(idx) = self.row_at(content, *py) {
                        if self.options[idx] == "-" {
                            return true;
                        }
                        if self.selected != idx || self.custom_display_text.is_some() {
                            self.selected = idx;
                            self.just_changed = true;
                        }
                    }
                    self.begin_close();
                    return true;
                }

                if inside_trigger {
                    if self.open && !self.closing {
                        self.begin_close();
                    } else {
                        self.begin_open();
                        // Animation frames arrive through the ctx tick loop.
                        if let Some(ui) = ectx.ui.as_deref_mut() {
                            ui.register_tick_receiver(ectx.id);
                        }
                        // Legacy `focus()` claimed only the global slot.
                        ectx.request_focus();
                    }
                    return true;
                }

                if self.open && !self.closing {
                    self.begin_close();
                    return true;
                }

                false
            }
            Event::PointerMove { x: px, y: py, .. } => {
                // The popover-item half of the legacy `on_cursor_moved`; the trigger-hover half
                // is the adapter's bookkeeping (MouseEnter/MouseLeave below).
                let was_hovered_item = self.hovered_item;
                self.hovered_item = None;
                if self.open && !self.closing {
                    let (rx, ry, rw, rh) = self.popover_geom(ectx.rect);
                    if *px >= rx && *px <= rx + rw && *py >= ry && *py <= ry + rh {
                        if let Some(idx) = self.row_at(ectx.rect, *py) {
                            if self.options[idx] != "-" {
                                self.hovered_item = Some(idx);
                            }
                        }
                    }
                }
                self.hovered_item != was_hovered_item
            }
            Event::MouseEnter => {
                self.hovered = true;
                true
            }
            Event::MouseLeave => {
                self.hovered = false;
                true
            }
            Event::KeyInput(key_event) => {
                // A key event carries no position, so hosts that broadcast one to every
                // widget root (cce-system-interface's `dispatch_page_event`) rely on each
                // widget declining what is not addressed to it — TextBox gates on
                // `editing`, Toggle has no key arm at all. A closed dropdown must gate on
                // focus for the same reason: without this it opened on any Enter/Space
                // that reached it, so the first dropdown in the host's dispatch order
                // swallowed the Return meant for whatever actually held focus. (Settings'
                // Browser page: Enter in the Homepage field opened the Page Color Scheme
                // menu instead of committing the field. Its Power page: Enter on the
                // focused Power Profile opened the last of the nine, GPU Power Limit.)
                // An open dropdown always holds focus — the trigger press and `FocusIn`
                // both claim it, and `FocusOut` closes it — so the `open` arm is reachable
                // either way; it is spelled out so arrows and Escape stay live regardless.
                if !self.open && !ectx.is_focused() {
                    return false;
                }
                let handled = self.handle_key(key_event);
                if self.open {
                    if let Some(ui) = ectx.ui.as_deref_mut() {
                        ui.register_tick_receiver(ectx.id);
                    }
                }
                handled
            }
            Event::FocusIn => {
                // Legacy `focus()` claimed the global focus slot on every direct call
                // (test-interface focuses the ramp's preset dropdown this way).
                self.focused = true;
                ectx.request_focus();
                false
            }
            Event::FocusOut => {
                // Legacy `unfocus` closed the dropdown.
                self.focused = false;
                self.begin_close();
                false
            }
            _ => false,
        }
    }

    fn take_click(&mut self) -> bool {
        self.take_change()
    }

    fn take_change(&mut self) -> bool {
        self.take_change()
    }

    fn value(&self) -> i32 {
        self.selected as i32
    }

    fn value_string(&self) -> Option<String> {
        self.options.get(self.selected).cloned()
    }

    fn set_value_string(&mut self, val: &str) -> bool {
        let val_trimmed = val.trim();
        for (idx, opt) in self.options.iter().enumerate() {
            if opt.eq_ignore_ascii_case(val_trimmed) {
                if self.selected != idx {
                    self.selected = idx;
                    self.just_changed = true;
                    return true;
                }
                return false;
            }
        }
        if let Ok(idx) = val_trimmed.parse::<usize>() {
            if idx < self.options.len() {
                if self.selected != idx {
                    self.selected = idx;
                    self.just_changed = true;
                    return true;
                }
                return false;
            }
        }
        false
    }
}

impl Dropdown {

    /// Port of the legacy `keyboard_input` body.
    pub(super) fn handle_key(&mut self, event: &crate::widget::KeyEvent) -> bool {
        if event.state != ElementState::Pressed {
            return false;
        }
        if !self.open || self.closing {
            if let Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) = event.logical_key {
                self.begin_open();
                let mut start_idx = self.selected;
                if start_idx < self.options.len() && self.options[start_idx] == "-" {
                    for i in 0..self.options.len() {
                        if self.options[i] != "-" {
                            start_idx = i;
                            break;
                        }
                    }
                }
                self.hovered_item = Some(start_idx);
                return true;
            }
            return false;
        }

        match event.logical_key {
            Key::Named(NamedKey::ArrowDown) => {
                let current = self.hovered_item.unwrap_or(self.selected);
                let mut next = (current + 1) % self.options.len();
                for _ in 0..self.options.len() {
                    if self.options[next] != "-" {
                        self.hovered_item = Some(next);
                        break;
                    }
                    next = (next + 1) % self.options.len();
                }
                true
            }
            Key::Named(NamedKey::ArrowUp) => {
                let current = self.hovered_item.unwrap_or(self.selected);
                let mut prev = if current == 0 { self.options.len() - 1 } else { current - 1 };
                for _ in 0..self.options.len() {
                    if self.options[prev] != "-" {
                        self.hovered_item = Some(prev);
                        break;
                    }
                    prev = if prev == 0 { self.options.len() - 1 } else { prev - 1 };
                }
                true
            }
            Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                if std::env::var_os("CCE_DD_DEBUG").is_some() {
                    eprintln!("[dd] key-select hovered={:?} selected={}", self.hovered_item, self.selected);
                }
                if let Some(idx) = self.hovered_item {
                    if idx < self.options.len() && self.options[idx] != "-" {
                        if self.selected != idx || self.custom_display_text.is_some() {
                            self.selected = idx;
                            self.just_changed = true;
                        }
                        self.begin_close();
                    }
                }
                true
            }
            Key::Named(NamedKey::Escape) => {
                self.begin_close();
                true
            }
            _ => false,
        }
    }
}
