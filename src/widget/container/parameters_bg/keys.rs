//! Keys: a press routed to the focused row's control.

use super::*;

impl ParametersBg {
    /// A key, to the focused row: the code editor on a code row, else the row's control.
    pub(super) fn on_key(&mut self, event: &crate::widget::KeyEvent, ectx: &mut EventCtx) -> bool {
        if !self.visible {
            return false;
        }
        let Some(ui) = ectx.ui.as_deref_mut() else {
            return false;
        };
        let Some(idx) = self.focused_param else {
            return false;
        };
        if event.state != ElementState::Pressed {
            return false;
        }
        if self.display_params[idx].2 == "code" {
            return self.code_key(idx, event);
        }
        self.row_key(idx, event, ui)
    }

    /// A key to the focused row's control (any row but code); its value is written back
    /// into the row, and the row unfocused once the control stops editing.
    pub(super) fn row_key(&mut self, idx: usize, event: &crate::widget::KeyEvent, ui: &mut UiContext) -> bool {
        let p = &mut self.display_params[idx];
        if is_text_row(&p.2) {
            if let Some(d) = &mut self.choices[idx] {
                if d.open && d.keyboard_input(event, ui) {
                    if d.take_change() {
                        if let Some(val) = d.get_value_string() {
                            fill_from_pick(&mut self.texts[idx], &mut p.1, val);
                        }
                    }
                    return true;
                }
            }
            if let Some(tb) = &mut self.texts[idx] {
                if tb.keyboard_input(event, ui) {
                    if !tb.editing {
                        p.1 = tb.text.clone();
                        self.focused_param = None;
                    } else {
                        p.1 = tb.edit_buffer.clone();
                    }
                    return true;
                }
            }
        } else if p.2.starts_with("choice") {
            if let Some(d) = &mut self.choices[idx] {
                if d.keyboard_input(event, ui) {
                    if !d.open {
                        if let Some(val) = d.get_value_string() {
                            p.1 = val;
                        }
                        self.focused_param = None;
                    }
                    return true;
                }
            }
        } else if p.2.starts_with("spinbox") {
            if let Some(sb) = &mut self.spinboxes[idx] {
                if sb.keyboard_input(event, ui) {
                    if !sb.editing {
                        p.1 = sb.value.to_string();
                        self.focused_param = None;
                    } else {
                        p.1 = sb.edit_buffer.clone();
                    }
                    return true;
                }
            }
        } else if p.2.starts_with("slider") {
            if let Some(s) = &mut self.sliders[idx] {
                if s.keyboard_input(event, ui) {
                    let new_val = s.get_scaled_value();
                    p.1 = format!("{:.*}", slider_decimals(&p.2), new_val);
                    if !s.editing {
                        self.focused_param = None;
                    }
                    return true;
                }
            }
        } else if p.2.starts_with("color") || p.2 == "rgb" || p.2 == "rgba" {
            if let Some(c) = &mut self.colors[idx] {
                if c.keyboard_input(event, ui) {
                    if let Some(val) = c.get_value_string() {
                        p.1 = val;
                    }
                    if !c.editing {
                        self.focused_param = None;
                    }
                    return true;
                }
            }
        } else if is_vec_row(&p.2) {
            if let Some(f) = &mut self.float3s[idx] {
                if f.keyboard_input(event, ui) {
                    p.1 = f.value_string();
                    if f.editing_idx().is_none() {
                        self.focused_param = None;
                    }
                    return true;
                }
            }
        }
        false
    }
}
