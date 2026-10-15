//! The float ramp's input: dragging keys, the hover-scroll and its coast, and the controls with
//! the keyboard.

use super::*;

impl Input for Ramp {
    fn wants_tick(&self) -> bool {
        true
    }

    /// The graph context menu's actions. Overriding loses the trait-default
    /// clipboard arms, so Copy/Paste (the spec string) are restated here.
    fn context_action(&mut self, action: ContextAction) -> bool {
        match action {
            ContextAction::ToggleRampControls => {
                self.controls_collapsed = !self.controls_collapsed;
                self.just_changed = true;
                self.arrange_fields();
                true
            }
            ContextAction::Copy => {
                crate::widget::clipboard::copy_to_clipboard(&self.spec_string());
                true
            }
            ContextAction::Paste => {
                if let Some(text) = crate::widget::clipboard::read_from_clipboard() {
                    let changed = self.set_spec(&text);
                    if changed {
                        self.just_changed = true;
                    }
                    changed
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// The curve as a ramp spec string ([`format_ramp_spec`]) — the value hosts
    /// poll and persist for ramp-valued params.
    fn value_string(&self) -> Option<String> {
        Some(self.spec_string())
    }

    fn set_value_string(&mut self, val: &str) -> bool {
        self.set_spec(val)
    }

    /// The open dropdown popover extends the hit area (the 5p Dropdown pattern).
    fn hit(&self, rect: Rect, x: f32, y: f32) -> bool {
        if let Some((px, py, pw, ph)) = self.open_popover() {
            if within(Rect { x: px, y: py, width: pw, height: ph }, x, y) {
                return true;
            }
        }
        within(rect, x, y)
    }

    fn tick_ctx(&mut self, dt: f32, ectx: &mut EventCtx) -> bool {
        let Some(ui) = ectx.ui.as_deref_mut() else {
            return false;
        };
        let mut changed = self.just_changed;
        self.just_changed = false;

        // A field the window's focus moved away from is told here: the fields are not in
        // the registry, so the focus change could not reach them itself.
        let focused = self.focused_field(ui);
        for i in 0..FIELD_TOTAL {
            if Some(i) != focused && self.field(i).base().focused {
                self.field(i).unfocus();
                changed = true;
            }
        }

        // Hover-scroll inertia: once the finger stream stops (>60ms without
        // an event), the latched key coasts on the estimated velocity with
        // exponential decay, still resettling and syncing like live scrolls.
        if let (Some(idx), Some(last)) = (self.scroll_key_idx, self.last_key_scroll) {
            if last.elapsed().as_secs_f32() > 0.06 && idx < self.keys.len() {
                let (vx, vy) = self.scroll_vel;
                // Animations off: the key stops where the scroll left it.
                if (vx.abs() > 0.02 || vy.abs() > 0.02) && crate::motion::enabled() {
                    let pos = (self.keys[idx].pos + vx * dt).clamp(0.0, 1.0);
                    let value = (self.keys[idx].value + vy * dt).clamp(0.0, 1.0);
                    self.scroll_key_idx = Some(self.move_key(idx, pos, value));
                    let f = (-5.0 * dt).exp();
                    self.scroll_vel = (vx * f, vy * f);
                    changed = true;
                } else {
                    self.scroll_vel = (0.0, 0.0);
                    self.last_key_scroll = None;
                }
            }
        }

        // The dropdown's tick reports its open/close animation running, not a pick: read
        // as a pick, opening the trigger re-applied the preset it still pointed at and
        // wiped a hand-edited curve. A pick is its change flag — the keyboard's land here,
        // the mouse's are drained in `on_event`.
        if self.preset_dropdown.tick(dt, ui) {
            changed = true;
        }
        if self.preset_dropdown.take_change() {
            let idx = self.preset_dropdown.selected;
            self.apply_preset(idx);
            changed = true;
        }
        
        if self.line_type_dropdown.tick(dt, ui) {
            changed = true;
        }
        // The key pad and the delete button have no tick of their own: neither animates,
        // and a pad change arrives through the event that made it.
        changed
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button, state, x, y, .. } => {
                let (button, state, px, py) = (*button, *state, *x, *y);
                // Right-press in the graph opening → the shared context menu (Collapse
                // controls, Copy, Paste). Before the ui borrow: open_context_menu needs the
                // whole EventCtx.
                if button == MouseButton::Right {
                    if state == ElementState::Pressed && within(self.graph_rect(), px, py) {
                        ectx.open_context_menu(px, py);
                        return true;
                    }
                    return false;
                }
                let Some(ui) = ectx.ui.as_deref_mut() else { return false };
                if button != MouseButton::Left {
                    return false;
                }

                if self.preset_dropdown.mouse_input(button, state, px, py, ui) {
                    if self.preset_dropdown.take_change() {
                        let idx = self.preset_dropdown.selected;
                        self.apply_preset(idx);
                    }
                    return true;
                }
                if self.line_type_dropdown.mouse_input(button, state, px, py, ui) {
                    return true;
                }

                if state == ElementState::Pressed {
                    // Any press cancels a hover-scroll glide in progress.
                    self.scroll_vel = (0.0, 0.0);
                    self.scroll_key_idx = None;
                    self.last_key_scroll = None;
                    // Grab the key whose peg is under the press, the topmost where pegs overlap —
                    // the one drawn on top is the one taken.
                    if let Some(idx) = self.key_at(px, py) {
                        self.select_key(idx);
                        self.is_dragging_key = true;
                        // The key keeps where on its peg it was taken: its centre snapping to
                        // the pointer jumped it up to a peg's radius on the first move.
                        let key = &self.keys[idx];
                        let (t, v) = self.plot_unit(px, py);
                        self.drag_grab = Some(DragGrab { origin: key.pos, offset: (key.pos - t, key.value - v) });
                        self.arrange_fields();
                        return true;
                    }

                    // Creation accepts the whole opening (the inset gutters included);
                    // the domain mapping clamps to the plot's 0..1.
                    if within(self.graph_rect(), px, py) {
                        let (t, v) = self.plot_unit(px, py);
                        let (t, val) = (t.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
                        self.keys.push(RampKey { pos: t, value: val });
                        let new_idx = self.resettle_key(self.keys.len() - 1);
                        self.sync_preset();
                        self.just_changed = true;
                        self.select_key(new_idx);
                        // Arm the drag: a fresh key follows the pointer until release,
                        // so create-and-place is one gesture (the grab-branch behavior).
                        self.is_dragging_key = true;
                        self.drag_grab = Some(DragGrab { origin: t, offset: (0.0, 0.0) });
                        self.arrange_fields();
                        return true;
                    }

                    if self.selected_key_idx.is_some() {
                        if self.key_pad.mouse_input(button, state, px, py, ui) {
                            self.apply_pad_to_selected();
                            return true;
                        }
                        // Pressed only: the button clicks on its release, below.
                        if self.del_button.mouse_input(button, state, px, py, ui) {
                            return true;
                        }
                    }
                    false
                } else {
                    // A release is the ramp's only when something of it used it: a key drag
                    // ending, the key pad or the delete button letting go. Releases reach
                    // widgets without a hit test, and claiming every one while a key was
                    // selected kept them from whatever the press was on — a host's next
                    // row never clicked. Both fields are told regardless of the selection,
                    // so neither is left holding a press.
                    let dragged = std::mem::take(&mut self.is_dragging_key);
                    self.drag_grab = None;
                    let pad = self.key_pad.mouse_input(button, state, px, py, ui);
                    let del = self.del_button.mouse_input(button, state, px, py, ui);
                    if del && self.del_button.take_click() {
                        self.delete_selected();
                    }
                    dragged || pad || del
                }
            }
            Event::PointerMove { x, y, .. } => {
                let (px, py) = (*x, *y);
                let Some(ui) = ectx.ui.as_deref_mut() else { return false };
                if self.preset_dropdown.cursor_moved(px, py, ui) {
                    return true;
                }
                if self.line_type_dropdown.cursor_moved(px, py, ui) {
                    return true;
                }

                let mut changed = false;
                if self.is_dragging_key {
                    if let Some(idx) = self.selected_key_idx {
                        let grab = self.drag_grab.unwrap_or(DragGrab { origin: self.keys[idx].pos, offset: (0.0, 0.0) });
                        let (t, v) = self.plot_unit(px, py);
                        let t_raw = (t + grab.offset.0).clamp(0.0, 1.0);
                        let t = self.resisted_pos(idx, t_raw, grab.origin);
                        let val = (v + grab.offset.1).clamp(0.0, 1.0);
                        self.move_key(idx, t, val);
                        changed = true;
                    }
                }

                if self.selected_key_idx.is_some() {
                    if self.key_pad.cursor_moved(px, py, ui) {
                        self.apply_pad_to_selected();
                        changed = true;
                    }
                    if self.del_button.cursor_moved(px, py, ui) {
                        changed = true;
                    }
                }
                if changed {
                    self.just_changed = true;
                }
                changed
            }
            Event::MouseWheel { delta, x, y, .. } => {
                // The dropdowns first (mirroring the press order, the preset's pick drained),
                // then the hover-scroll, then the key pad with the key sync.
                let (delta, px, py) = (*delta, *x, *y);
                let Some(ui) = ectx.ui.as_deref_mut() else { return false };
                if self.preset_dropdown.mouse_wheel(&delta, px, py, ui) {
                    if self.preset_dropdown.take_change() {
                        let idx = self.preset_dropdown.selected;
                        self.apply_preset(idx);
                    }
                    return true;
                }
                if self.line_type_dropdown.mouse_wheel(&delta, px, py, ui) {
                    return true;
                }
                // Hover-scroll: a gesture STARTING over a key latches it and
                // steers it on both axes — following the fingers like a drag
                // — until the stream pauses (fingers lifted). Mid-gesture the
                // latch holds even if the key slides out from under the
                // cursor. Latching also selects the key, so the pad tracks.
                let plot = self.plot_rect();
                if ui.scroll_gesture_new {
                    self.scroll_key_idx = self.key_at(px, py);
                    self.scroll_vel = (0.0, 0.0);
                }
                if let Some(idx) = self.scroll_key_idx {
                    if idx < self.keys.len() {
                        ui.scroll_initiate_widget_id = Some(ectx.id);
                        // Damped well below 1:1 — hover-scroll is for fine
                        // adjustment; the drag paths cover coarse moves.
                        let (dx, dy) = match &delta {
                            MouseScrollDelta::LineDelta(x, y) => (*x * 0.005, *y * 0.005),
                            MouseScrollDelta::PixelDelta(pos) => (
                                0.2 * pos.x as f32 / plot.width,
                                0.2 * pos.y as f32 / plot.height,
                            ),
                        };
                        // Velocity estimate for the release glide: EMA of
                        // applied delta over inter-event time. A leisurely
                        // wheel produces negligible velocity (big gaps clamp
                        // to 0.1s); fast trackpad streams build real speed.
                        let now = web_time::Instant::now();
                        let dt_ev = self
                            .last_key_scroll
                            .map(|t| now.duration_since(t).as_secs_f32())
                            .unwrap_or(0.016)
                            .clamp(0.004, 0.1);
                        self.last_key_scroll = Some(now);
                        let (ivx, ivy) = (-dx / dt_ev, dy / dt_ev);
                        self.scroll_vel = (
                            self.scroll_vel.0 * 0.65 + ivx * 0.35,
                            self.scroll_vel.1 * 0.65 + ivy * 0.35,
                        );
                        // Direct manipulation: the key moves WITH the scroll
                        // (runner deltas are content-motion negated, so both
                        // axes flip): scroll right → key right, down → down.
                        let pos = (self.keys[idx].pos - dx).clamp(0.0, 1.0);
                        let value = (self.keys[idx].value + dy).clamp(0.0, 1.0);
                        self.scroll_key_idx = Some(self.move_key(idx, pos, value));
                        self.just_changed = true;
                        self.arrange_fields();
                        return true;
                    }
                    self.scroll_key_idx = None;
                }
                if self.selected_key_idx.is_some() && self.key_pad.mouse_wheel(&delta, px, py, ui) {
                    self.apply_pad_to_selected();
                    return true;
                }
                false
            }
            Event::KeyInput(event) => {
                let Some(ui) = ectx.ui.as_deref_mut() else { return false };
                if event.state != ElementState::Pressed {
                    return false;
                }

                let count = self.field_count();
                let current = self.focused_field(ui).filter(|&i| i < count);

                if event.logical_key == Key::Named(NamedKey::Tab) {
                    let next = match current {
                        Some(curr) if event.shift => if curr == 0 { count - 1 } else { curr - 1 },
                        Some(curr) => (curr + 1) % count,
                        // Tab into the ramp: its first field takes the keyboard.
                        None => 0,
                    };
                    self.focus_field(next, ui);
                    return true;
                }

                match current {
                    Some(0) => self.preset_dropdown.keyboard_input(event, ui),
                    Some(1) => self.line_type_dropdown.keyboard_input(event, ui),
                    Some(2) => self.key_pad.keyboard_input(event, ui),
                    Some(_) => {
                        // Enter/Space press the button: its click is read here, at once. Left
                        // unread, it fired on the next mouse PRESS on the button.
                        let used = self.del_button.keyboard_input(event, ui);
                        if self.del_button.take_click() {
                            self.delete_selected();
                        }
                        used
                    }
                    None => false,
                }
            }
            Event::FocusIn => {
                // Focused itself, the ramp gives the keyboard to its preset dropdown.
                if let Some(ui) = ectx.ui.as_deref_mut() {
                    self.focus_field(0, ui);
                }
                false
            }
            Event::FocusOut => {
                self.base.focused = false;
                for i in 0..FIELD_TOTAL {
                    self.field(i).unfocus();
                }
                false
            }
            _ => false,
        }
    }

    // The key drag and the key pad's own drag both run through the ramp: the router records
    // the ramp as the drag target once a press is handled here, and the pad's moves sync the
    // selected key as they go.
    fn draggable(&self, _rect: Rect) -> bool {
        self.is_dragging()
    }
    fn is_dragging(&self) -> bool {
        self.is_dragging_key || self.key_pad.is_dragging()
    }
    fn drag_update(&mut self, px: f32, py: f32, _rect: Rect) -> bool {
        if self.key_pad.is_dragging() && self.key_pad.drag_update(px, py) {
            self.apply_pad_to_selected();
            return true;
        }
        false
    }
    fn drag_end(&mut self) {
        self.key_pad.drag_end();
        self.is_dragging_key = false;
        self.drag_grab = None;
    }
}
