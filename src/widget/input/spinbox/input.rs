//! A `Spinbox`'s input: hover and presses on the -/+ zones and the value, the wheel, keys while
//! focused or typing, and the value as an accessibility reader reads and sets it.

use super::*;

impl Input for Spinbox {
    fn focus_role(&self) -> crate::widget::FocusRole {
        crate::widget::FocusRole::Well
    }
    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::PointerMove { x: px, y: py, .. } => {
                let r = ectx.rect;
                let was = self.hovered;
                self.hovered = *px >= r.x && *px <= r.x + r.width && *py >= r.y && *py <= r.y + r.height;
                if !self.hovered {
                    let changed = self.hover_dec || self.hover_inc;
                    self.hover_dec = false;
                    self.hover_inc = false;
                    return changed || was != self.hovered;
                }
                let g = self.geom(r);
                let in_y = *py >= g.btn_y && *py < g.btn_y + g.btn_h;
                let hd = in_y && *px >= g.run_x && *px < g.seam_x;
                let hi = in_y && *px >= g.seam_x && *px < g.run_end;
                let changed = hd != self.hover_dec || hi != self.hover_inc;
                self.hover_dec = hd;
                self.hover_inc = hi;
                changed || was != self.hovered
            }
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, x: px, y: py, .. } => {
                let g = self.geom(ectx.rect);
                let in_y = *py >= g.btn_y && *py < g.btn_y + g.btn_h;
                if in_y && *px >= g.run_x && *px < g.seam_x {
                    self.step_by(-1);
                    true
                } else if in_y && *px >= g.seam_x && *px < g.run_end {
                    self.step_by(1);
                    true
                } else if *px < g.split_dec {
                    self.begin_edit(false);
                    self.cursor_idx = self
                        .x_to_idx(px - (g.x + crate::layout::CONTROL_TEXT_INSET))
                        .min(self.edit_buffer.chars().count());
                    ectx.request_focus();
                    true
                } else {
                    false
                }
            }
            Event::MouseWheel { delta, .. } => {
                // Wheel up steps up, wheel down steps down, one step per notch;
                // fractional (trackpad) notches accumulate. Always consumed, so
                // a host's page never scrolls under a spinbox mid-gesture.
                self.wheel_accum += delta.value_notches_y();
                while self.wheel_accum >= 1.0 {
                    self.wheel_accum -= 1.0;
                    self.step_by(1);
                }
                while self.wheel_accum <= -1.0 {
                    self.wheel_accum += 1.0;
                    self.step_by(-1);
                }
                true
            }
            Event::KeyInput(key_event) => {
                if key_event.state != ElementState::Pressed {
                    return false;
                }
                if !self.editing {
                    // Focused but done editing (Enter committed): the keys that edit or step
                    // the box take it back into editing; Enter just reopens it.
                    let reopens = match &key_event.logical_key {
                        Key::Named(NamedKey::Enter | NamedKey::ArrowUp | NamedKey::ArrowDown | NamedKey::Backspace) => true,
                        _ => key_event.text.as_deref().is_some_and(|t| t.chars().all(|c| c.is_ascii_digit() || c == '-' || c == '.') && !t.is_empty()),
                    };
                    if !self.focused || !reopens {
                        return false;
                    }
                    self.begin_edit(true);
                    if key_event.logical_key == Key::Named(NamedKey::Enter) {
                        return true;
                    }
                }
                let mut state = TextEditorState {
                    buffer: self.edit_buffer.clone(),
                    cursor_idx: self.cursor_idx,
                    select_anchor: None,
                    all_selected: false,
                };
                let mut handled = false;
                match &key_event.logical_key {
                    Key::Named(NamedKey::Backspace) => handled = state.delete_backwards(),
                    Key::Named(NamedKey::Delete) => handled = state.delete_forwards(),
                    Key::Named(NamedKey::ArrowLeft) => handled = state.move_cursor_left(false),
                    Key::Named(NamedKey::ArrowRight) => handled = state.move_cursor_right(false),
                    // Up / Down step the value, as the -/+ run does (and an assistive
                    // tool's Increment / Decrement, which presses them).
                    Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::ArrowDown) => {
                        self.step_by(if key_event.logical_key == Key::Named(NamedKey::ArrowUp) { 1 } else { -1 });
                        return true;
                    }
                    Key::Named(NamedKey::Enter) => {
                        let text = state.buffer.clone();
                        self.parse_into_value(&text);
                        self.editing = false;
                        handled = true;
                    }
                    Key::Named(NamedKey::Escape) => {
                        self.editing = false;
                        handled = true;
                    }
                    _ => {
                        if let Some(text) = &key_event.text {
                            for ch in text.chars() {
                                match ch {
                                    '-' if state.cursor_idx == 0 && !state.buffer.starts_with('-') => {
                                        state.insert_text("-");
                                        handled = true;
                                    }
                                    '.' if self.decimals > 0 && !state.buffer.contains('.') => {
                                        state.insert_text(".");
                                        handled = true;
                                    }
                                    '0'..='9' => {
                                        state.insert_text(&ch.to_string());
                                        handled = true;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                if self.editing {
                    self.edit_buffer = state.buffer;
                    self.cursor_idx = state.cursor_idx;
                }
                handled
            }
            // Focus gained programmatically enters edit mode (legacy `focus()` override);
            // focus loss commits (legacy `unfocus`).
            Event::FocusIn => {
                self.focused = true;
                self.begin_edit(true);
                false
            }
            Event::FocusOut => {
                self.focused = false;
                if self.editing {
                    self.editing = false;
                    let text = self.edit_buffer.clone();
                    self.parse_into_value(&text);
                }
                false
            }
            _ => false,
        }
    }

    fn opens_context_menu(&self) -> bool {
        true
    }

    fn take_change(&mut self) -> bool {
        std::mem::take(&mut self.just_changed)
    }

    fn value_string(&self) -> Option<String> {
        Some(self.formatted_value())
    }

    fn a11y_range(&self) -> Option<(f64, f64, f64)> {
        let unit = 10f64.powi(self.decimals as i32);
        Some((self.min as f64 / unit, self.max as f64 / unit, self.step as f64 / unit))
    }

    fn a11y_set_value(&mut self, value: f64) -> bool {
        let old_val = self.value;
        let unit = 10f64.powi(self.decimals as i32);
        self.value = ((value * unit).round() as i32).clamp(self.min, self.max);
        if self.value == old_val {
            return false;
        }
        self.just_changed = true;
        if self.editing {
            self.edit_buffer = self.formatted_value();
            self.cursor_idx = self.edit_buffer.chars().count();
        }
        true
    }

    fn set_value_string(&mut self, val: &str) -> bool {
        let old_val = self.value;
        self.parse_into_value(val.trim());
        if self.value != old_val {
            if self.editing {
                self.edit_buffer = self.formatted_value();
                self.cursor_idx = self.edit_buffer.chars().count();
            }
            true
        } else {
            false
        }
    }

    fn value(&self) -> i32 {
        self.value
    }
}
