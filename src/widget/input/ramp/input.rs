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
        let popover = self.preset_dropdown.popover_rect().or_else(|| self.line_type_dropdown.popover_rect());
        if let Some((px, py, pw, ph)) = popover {
            if x >= px && x <= px + pw && y >= py && y <= py + ph {
                return true;
            }
        }
        x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height
    }

    fn tick_ctx(&mut self, dt: f32, ectx: &mut EventCtx) -> bool {
        // (The per-tick field-widget re-parenting is gone, 6bd: it was a dummy-ctx
        // `set_parent` whose every effect was discarded — legacy behaved the same.)
        let Some(ui) = ectx.ui.as_deref_mut() else {
            return false;
        };
        let mut changed = self.just_changed;
        self.just_changed = false;

        // A field the window's focus moved away from is told here: the fields are not in
        // the registry, so the focus change could not reach them itself.
        let focused = self.focused_field(ui);
        for i in 0..4 {
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
                    self.keys[idx].pos = (self.keys[idx].pos + vx * dt).clamp(0.0, 1.0);
                    self.keys[idx].value = (self.keys[idx].value + vy * dt).clamp(0.0, 1.0);
                    let settled = self.resettle_key(idx);
                    self.scroll_key_idx = Some(settled);
                    self.selected_key_idx = Some(settled);
                    self.key_pad
                        .set_values(self.keys[settled].pos, self.keys[settled].value);
                    self.sync_preset();
                    let f = (-5.0 * dt).exp();
                    self.scroll_vel = (vx * f, vy * f);
                    changed = true;
                } else {
                    self.scroll_vel = (0.0, 0.0);
                    self.last_key_scroll = None;
                }
            }
        }

        if self.preset_dropdown.tick(dt, ui) {
            let idx = self.preset_dropdown.selected;
            self.apply_preset(idx);
            changed = true;
        }
        
        if self.line_type_dropdown.tick(dt, ui) {
            changed = true;
        }
        
        if self.selected_key_idx.is_some() {
            if self.key_pad.tick(dt, ui) {
                self.apply_pad_to_selected();
                changed = true;
            }
            if self.del_button.tick(dt, ui) {
                self.sync_preset();
                changed = true;
            }
        }
        changed
    
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button, state, x, y, .. } => {
                let (button, state, px, py_event) = (*button, *state, *x, *y);
                // Right-press in the graph opening → the shared context menu
                // (the key-crossing toggle lives there). Before the ui borrow:
                // open_context_menu needs the whole EventCtx.
                if button == MouseButton::Right {
                    if state == ElementState::Pressed {
                        let gh = self.graph_h();
                        let gx = self.base.x + 10.0;
                        let gw = self.base.w - 20.0;
                        let gy = self.base.y + 10.0;
                        if px >= gx && px <= gx + gw && py_event >= gy && py_event <= gy + gh {
                            ectx.open_context_menu(px, py_event);
                            return true;
                        }
                    }
                    return false;
                }
                let Some(ui) = ectx.ui.as_deref_mut() else { return false; };
        if button != MouseButton::Left { return false; }

        if self.preset_dropdown.mouse_input(button, state, px, py_event, ui) {
            if self.preset_dropdown.take_change() {
                let idx = self.preset_dropdown.selected;
                self.apply_preset(idx);
            }
            return true;
        }
        
        if self.line_type_dropdown.mouse_input(button, state, px, py_event, ui) {
            return true;
        }
        
        let gh = self.graph_h();
        let plot = self.plot_rect();

        if state == ElementState::Pressed {
            // Any press cancels a hover-scroll glide in progress.
            self.scroll_vel = (0.0, 0.0);
            self.scroll_key_idx = None;
            self.last_key_scroll = None;
            // Grab the NEAREST key whose ring contains the press — the rings
            // are the pegs' visual extent, and nearest-center also matches the
            // foam walls (perpendicular bisectors) where rings overlap.
            let hit_r = self.key_ring_r() + 2.5;
            let mut best: Option<(usize, f32)> = None;
            for (idx, key) in self.keys.iter().enumerate() {
                let cx = plot.x + key.pos * plot.width;
                let cy = plot.y + plot.height * (1.0 - key.value);
                let dx = px - cx;
                let dy = py_event - cy;
                let d2 = dx * dx + dy * dy;
                if d2 <= hit_r * hit_r && best.is_none_or(|(_, bd)| d2 < bd) {
                    best = Some((idx, d2));
                }
            }
            if let Some((idx, _)) = best {
                self.selected_key_idx = Some(idx);
                self.is_dragging_key = true;
                self.key_pad.set_values(self.keys[idx].pos, self.keys[idx].value);
                self.arrange_fields();
                return true;
            }

            // Creation accepts the whole opening (the inset gutters included);
            // the domain mapping clamps to the plot's 0..1.
            if px >= self.base.x + 10.0 && px <= self.base.x + self.base.w - 10.0 && py_event >= self.base.y + 10.0 && py_event <= self.base.y + 10.0 + gh {
                let t = ((px - plot.x) / plot.width).clamp(0.0, 1.0);
                let val = (1.0 - (py_event - plot.y) / plot.height).clamp(0.0, 1.0);
                let new_key = RampKey { pos: t, value: val };
                self.keys.push(new_key);
                let new_idx = self.resettle_key(self.keys.len() - 1);
                self.sync_preset();
                self.just_changed = true;
                self.selected_key_idx = Some(new_idx);
                self.key_pad.set_values(t, val);
                // Arm the drag: a fresh key follows the pointer until release,
                // so create-and-place is one gesture (the grab-branch behavior).
                self.is_dragging_key = true;
                self.arrange_fields();
                return true;
            }
            
            if self.selected_key_idx.is_some() {
                if self.key_pad.mouse_input(button, state, px, py_event, ui) {
                    self.apply_pad_to_selected();
                            return true;
                }
                if self.del_button.mouse_input(button, state, px, py_event, ui) {
                            if self.del_button.take_click() {
                        if let Some(idx) = self.selected_key_idx {
                            if self.keys.len() > 2 {
                                self.keys.remove(idx);
                                self.selected_key_idx = None;
                                self.sync_preset();
                                self.just_changed = true;
                                self.arrange_fields();
                            }
                        }
                    }
                    return true;
                }
            }
        } else {
            self.is_dragging_key = false;
            if self.selected_key_idx.is_some() {
                self.key_pad.mouse_input(button, state, px, py_event, ui);
                if self.del_button.mouse_input(button, state, px, py_event, ui)
                    && self.del_button.take_click() {
                        if let Some(idx) = self.selected_key_idx {
                            if self.keys.len() > 2 {
                                self.keys.remove(idx);
                                self.selected_key_idx = None;
                                self.sync_preset();
                                self.just_changed = true;
                                self.arrange_fields();
                            }
                        }
                    }
                return true;
            }
        }
        false
    
            }
            Event::PointerMove { x, y, .. } => {
                let (px, py_event) = (*x, *y);
                let Some(ui) = ectx.ui.as_deref_mut() else { return false; };
        if self.preset_dropdown.cursor_moved(px, py_event, ui) {
            return true;
        }
        if self.line_type_dropdown.cursor_moved(px, py_event, ui) {
            return true;
        }
        
        let mut changed = false;
        let plot = self.plot_rect();

        if self.is_dragging_key {
            if let Some(idx) = self.selected_key_idx {
                let t_raw = ((px - plot.x) / plot.width).clamp(0.0, 1.0);
                let t = self.resisted_pos(idx, t_raw);
                let val = (1.0 - (py_event - plot.y) / plot.height).clamp(0.0, 1.0);
                self.keys[idx].pos = t;
                self.keys[idx].value = val;
                self.key_pad.set_values(t, val);
                let settled = self.resettle_key(idx);
                self.selected_key_idx = Some(settled);
                self.sync_preset();
                changed = true;
            }
        }
        
        if self.selected_key_idx.is_some() {
            if self.key_pad.cursor_moved(px, py_event, ui) {
                self.apply_pad_to_selected();
                changed = true;
            }
            if self.del_button.cursor_moved(px, py_event, ui) {
                changed = true;
            }
        }
        if changed {
            self.just_changed = true;
        }
        changed
    
            }
            Event::MouseWheel { delta, x, y, .. } => {
                // Wheel forwarding (6bd self-routing): dropdowns first (mirroring the press
                // order, incl. the preset drain), then the value slider with the key sync.
                let (delta, px, py) = (*delta, *x, *y);
                let Some(ui) = ectx.ui.as_deref_mut() else { return false; };
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
                    let hit_r = self.key_ring_r() + 2.5;
                    let mut best: Option<(usize, f32)> = None;
                    for (idx, key) in self.keys.iter().enumerate() {
                        let cx = plot.x + key.pos * plot.width;
                        let cy = plot.y + plot.height * (1.0 - key.value);
                        let dx = px - cx;
                        let dy = py - cy;
                        let d2 = dx * dx + dy * dy;
                        if d2 <= hit_r * hit_r && best.is_none_or(|(_, bd)| d2 < bd) {
                            best = Some((idx, d2));
                        }
                    }
                    self.scroll_key_idx = best.map(|(i, _)| i);
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
                        // Direct manipulation: the key moves WITH the scroll
                        // (runner deltas are content-motion negated, so both
                        // axes flip): scroll right → key right, down → down.
                        self.keys[idx].pos = (self.keys[idx].pos - dx).clamp(0.0, 1.0);
                        self.keys[idx].value = (self.keys[idx].value + dy).clamp(0.0, 1.0);
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
                        let settled = self.resettle_key(idx);
                        self.scroll_key_idx = Some(settled);
                        self.selected_key_idx = Some(settled);
                        self.key_pad
                            .set_values(self.keys[settled].pos, self.keys[settled].value);
                        self.sync_preset();
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
                let Some(ui) = ectx.ui.as_deref_mut() else { return false; };
        if event.state != ElementState::Pressed { return false; }
        
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
            Some(_) => self.del_button.keyboard_input(event, ui),
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
                self.preset_dropdown.unfocus();
                self.line_type_dropdown.unfocus();
                self.key_pad.unfocus();
                self.del_button.unfocus();
                false
            }
            _ => false,
        }
    }

    // Field-slider drags forward through the composite (6bd self-routing), with the key
    // value sync the old descent path never ran mid-drag.
    fn draggable(&self, _rect: Rect) -> bool {
        self.is_dragging_key || self.key_pad.is_dragging()
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
    }
}
