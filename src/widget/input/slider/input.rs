//! A `Slider`'s input: presses on the band and the readout, drags, the wheel, keys, the readout's
//! edit mode, and the tick that carries a wheel's glide on.

use super::*;

impl Input for Slider {
    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button: MouseButton::Left, state, x: px, y: py, .. } => {
                let g = self.geom(ectx.rect);
                // Readout click enters edit mode and takes focus.
                if self.show_readout {
                    let readout_w = 60.0;
                    let rx = g.x + g.w - readout_w;
                    if *px >= rx && *px <= rx + readout_w && *py >= g.y && *py <= g.y + g.h {
                        if *state == ElementState::Pressed && !self.editing {
                            self.editing = true;
                            self.edit_buffer = self.scaled_string();
                            ectx.request_focus();
                        }
                        return true;
                    }
                }
                match state {
                    ElementState::Pressed => {
                        let thumb_x = g.track_x + self.value * self.value_span(&g);
                        if *px >= g.track_x && *px <= g.track_x + g.track_w && *py >= g.y && *py <= g.y + g.h {
                            self.dragging = true;
                            self.drag_offset = px - thumb_x;
                            // A grab overrides any wheel glide in flight.
                            self.scroll_vel = 0.0;
                            self.last_wheel = None;
                            return true;
                        }
                        false
                    }
                    ElementState::Released => std::mem::take(&mut self.dragging),
                }
            }
            Event::MouseWheel { delta, x: px, y: py, .. } => {
                if !self.scroll_enabled {
                    return false;
                }
                if let Some(ui) = ectx.ui.as_deref_mut() {
                    // Band style: recognition is purely SPATIAL — anywhere in
                    // the shape halo adjusts, mid-gesture included. Trackpad
                    // swipes are one long gesture (kinetic tail included), so
                    // the initiator gate below would reject every event whose
                    // gesture began outside the halo no matter where the
                    // pointer is now — the "slider won't take my scroll" feel.
                    // The default style keeps the gate: only the widget that
                    // initiated a gesture keeps it.
                    let r = ectx.rect;
                    // Band: spatial acquisition + gesture LATCH. The halo travels
                    // with the bulge, so adjusting slides it away from the pointer
                    // — without the latch the value moves a little and stalls
                    // mid-scroll. Once a gesture engages this slider it keeps it
                    // until the gesture ends; a new gesture re-acquires by halo.
                    let latched = !ui.scroll_gesture_new && ui.scroll_initiate_widget_id == Some(ectx.id);
                    if latched || self.scroll_hit(r, *px, *py) {
                        ui.scroll_initiate_widget_id = Some(ectx.id);
                        // Up is more, for a wheel and for a finger alike.
                        let scroll_amount = delta.value_notches_y();
                        let new_val = (self.value + scroll_amount * self.notch_step()).clamp(0.0, 1.0);
                        let applied = new_val - self.value;
                        self.set_value_marking(new_val);
                        // Velocity estimate for the release glide (the Ramp
                        // hover-scroll idiom): EMA of applied delta over
                        // inter-event time. A leisurely wheel produces
                        // negligible velocity (big gaps clamp to 0.1s); fast
                        // trackpad streams build real speed. Hitting an end
                        // stops dead — no glide pinned at the bounds.
                        let now = web_time::Instant::now();
                        let idt = self
                            .last_wheel
                            .map_or(0.1, |l| now.duration_since(l).as_secs_f32())
                            .clamp(0.008, 0.1);
                        self.last_wheel = Some(now);
                        self.scroll_vel = if new_val == 0.0 || new_val == 1.0 {
                            0.0
                        } else {
                            self.scroll_vel * 0.65 + (applied / idt) * 0.35
                        };
                        if crate::scroll_debug() {
                            eprintln!(
                                "[scroll] slider {:?}: APPLY notches={scroll_amount:.3} applied={applied:.4} value={new_val:.4} idt={idt:.3} vel={:.3}",
                                self.label, self.scroll_vel
                            );
                        }
                        return true;
                    }
                    if crate::scroll_debug() {
                        eprintln!(
                            "[scroll] slider {:?}: MISS scroll_hit at ({px:.0},{py:.0}) rect={:?}",
                            self.label, ectx.rect
                        );
                    }
                }
                false
            }
            Event::MouseEnter => {
                self.hovered = true;
                true
            }
            Event::MouseLeave => {
                self.hovered = false;
                true
            }
            Event::FocusIn => {
                self.focused = true;
                true
            }
            Event::KeyInput(key_event) if !self.editing => {
                // A focused band: the arrows step the value by a wheel notch,
                // Home / End go to the ends, Enter opens the readout for typing.
                if !self.focused || key_event.state != ElementState::Pressed {
                    return false;
                }
                let target = match key_event.logical_key {
                    Key::Named(NamedKey::ArrowLeft) | Key::Named(NamedKey::ArrowDown) => self.value - self.notch_step(),
                    Key::Named(NamedKey::ArrowRight) | Key::Named(NamedKey::ArrowUp) => self.value + self.notch_step(),
                    Key::Named(NamedKey::Home) => 0.0,
                    Key::Named(NamedKey::End) => 1.0,
                    Key::Named(NamedKey::Enter) if self.show_readout => {
                        self.editing = true;
                        self.edit_buffer = self.scaled_string();
                        return true;
                    }
                    _ => return false,
                };
                self.scroll_vel = 0.0;
                self.last_wheel = None;
                self.set_value_marking(target.clamp(0.0, 1.0));
                true
            }
            Event::KeyInput(key_event) => {
                if !self.editing || key_event.state != ElementState::Pressed {
                    return false;
                }
                let mut state = TextEditorState {
                    buffer: self.edit_buffer.clone(),
                    cursor_idx: self.edit_buffer.chars().count(),
                    select_anchor: None,
                    all_selected: false,
                };
                let mut handled = false;
                match &key_event.logical_key {
                    Key::Named(NamedKey::Backspace) => {
                        state.delete_backwards();
                        handled = true;
                    }
                    Key::Named(NamedKey::Enter) => {
                        self.commit_edit();
                        handled = true;
                    }
                    Key::Named(NamedKey::Escape) => {
                        self.editing = false;
                        handled = true;
                    }
                    Key::Character(s) => {
                        for ch in s.chars() {
                            if ch.is_ascii_digit() || ch == '.' || (ch == '-' && state.buffer.is_empty()) {
                                state.insert_text(&ch.to_string());
                            }
                        }
                        handled = true;
                    }
                    _ => {}
                }
                if self.editing {
                    self.edit_buffer = state.buffer;
                }
                handled
            }
            // Focus loss commits the readout edit (legacy `unfocus` override).
            Event::FocusOut => {
                self.focused = false;
                self.commit_edit();
                true
            }
            _ => false,
        }
    }

    fn focus_role(&self) -> crate::widget::FocusRole {
        crate::widget::FocusRole::Well
    }

    fn opens_context_menu(&self) -> bool {
        true
    }

    /// Wheel-glide inertia: once the event stream stops (>60ms), the value
    /// coasts on the estimated velocity with exponential decay — the same
    /// release feel as the pane scrolls and the Ramp's hover-scroll.
    fn tick(&mut self, dt: f32, _rect: Rect) -> bool {
        let Some(last) = self.last_wheel else { return false };
        if last.elapsed().as_secs_f32() <= 0.06 {
            return false;
        }
        // Animations off: the value stops where the wheel left it.
        if self.scroll_vel.abs() > 0.02 && !self.dragging && !self.editing && crate::motion::enabled() {
            let new_val = (self.value + self.scroll_vel * dt).clamp(0.0, 1.0);
            let moved = self.set_value_marking(new_val);
            if crate::scroll_debug() {
                eprintln!(
                    "[scroll] slider {:?}: GLIDE dt={dt:.3} vel={:.3} value={new_val:.4}",
                    self.label, self.scroll_vel
                );
            }
            if new_val == 0.0 || new_val == 1.0 {
                self.scroll_vel = 0.0;
                self.last_wheel = None;
            } else {
                self.scroll_vel *= (-5.0 * dt).exp();
            }
            moved
        } else {
            self.scroll_vel = 0.0;
            self.last_wheel = None;
            false
        }
    }

    fn draggable(&self, _rect: Rect) -> bool {
        true
    }
    fn is_dragging(&self) -> bool {
        self.dragging
    }
    fn drag_begin(&mut self, px: f32, _py: f32, rect: Rect) {
        self.dragging = true;
        // A grab overrides any wheel glide in flight.
        self.scroll_vel = 0.0;
        self.last_wheel = None;
        let g = self.geom(rect);
        let thumb_x = g.track_x + self.value * self.value_span(&g);
        self.drag_offset = px - thumb_x;
    }
    fn drag_update(&mut self, px: f32, _py: f32, rect: Rect) -> bool {
        let g = self.geom(rect);
        let range = self.value_span(&g);
        if range > 0.0 {
            let new_val = ((px - self.drag_offset - g.track_x) / range).clamp(0.0, 1.0);
            return self.set_value_marking(new_val);
        }
        false
    }
    fn drag_end(&mut self) {
        self.dragging = false;
    }

    fn take_change(&mut self) -> bool {
        std::mem::take(&mut self.just_changed)
    }

    fn value_string(&self) -> Option<String> {
        Some(self.scaled_string())
    }

    fn a11y_range(&self) -> Option<(f64, f64, f64)> {
        let (lo, hi) = (self.min.min(self.max) as f64, self.min.max(self.max) as f64);
        Some((lo, hi, (self.notch_step() * (self.max - self.min).abs()) as f64))
    }

    fn a11y_set_value(&mut self, value: f64) -> bool {
        self.set_value_string(&value.to_string())
    }

    fn set_value_string(&mut self, val: &str) -> bool {
        if let Ok(new_val) = val.trim().parse::<f32>() {
            let range = self.max - self.min;
            let mapped = if range != 0.0 { ((new_val - self.min) / range).clamp(0.0, 1.0) } else { 0.0 };
            return self.set_value_marking(mapped);
        }
        false
    }

    fn value(&self) -> i32 {
        (self.value * 100.0) as i32
    }
}
