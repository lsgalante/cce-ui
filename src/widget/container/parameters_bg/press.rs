//! Presses: on the scrollbar, a section's title, an open popover, a row's control (and the text
//! row's picker), and the focus a press leaves behind.

use super::*;

impl ParametersBg {
    /// A press or release, offered to each stage in turn until one claims it: the scrollbar,
    /// a section's title box, an open popover, the rows' controls — and last, focus: a left
    /// press nothing claimed focuses the field it landed on, or commits and unfocuses.
    pub(super) fn on_mouse_button(&mut self, button: &MouseButton, state: &ElementState, px: &f32, py: &f32, ectx: &mut EventCtx) -> bool {
        if !self.visible {
            return false;
        }
        let (button, state, px, py) = (*button, *state, *px, *py);
        let Some(ui) = ectx.ui.as_deref_mut() else {
            return false;
        };
        let left_press = button == MouseButton::Left && state == ElementState::Pressed;
        if button == MouseButton::Left && self.press_scrollbar(state, px, py) {
            return true;
        }
        if left_press && self.press_section_title(px, py) {
            return true;
        }
        let hidden = self.hidden_rows();
        if self.press_open_popovers(button, state, px, py, ui, &hidden)
            || self.press_row_controls(button, state, px, py, ui, &hidden)
        {
            return true;
        }
        if left_press {
            self.focus_on_press(button, state, px, py, ui, &hidden);
            return true;
        }
        false
    }

    /// A left press on a raised scrollbar grabs its thumb (a press off the thumb first jumps
    /// the thumb's centre to it); the release ends the drag. Only a raised bar can be grabbed:
    /// a sunk one is behind the plate, so the press falls through to the rows under it.
    pub(super) fn press_scrollbar(&mut self, state: ElementState, px: f32, py: f32) -> bool {
        if state == ElementState::Pressed {
            if !(self.activity.raised() && self.hit_test_scrollbar(px, py)) {
                return false;
            }
            self.scrollbar_dragging = true;
            let t = self.thumb();
            let click_offset = py - t.y_at(self.scroll_y);
            if click_offset >= 0.0 && click_offset <= t.h {
                self.drag_offset_y = click_offset;
            } else {
                self.drag_offset_y = t.h / 2.0;
                self.scroll_y = t.scroll_for_top(py - self.drag_offset_y);
                self.update_slider_rects();
            }
            true
        } else if state == ElementState::Released && self.scrollbar_dragging {
            self.scrollbar_dragging = false;
            self.activity.bump();
            true
        } else {
            false
        }
    }

    /// A press on a section's title box collapses or expands it. Asked before the rows so a
    /// header can never be shadowed by a control under it, and only on the press: the
    /// matching release lands on whatever the relayout moved under the pointer, which must
    /// not toggle it straight back.
    pub(super) fn press_section_title(&mut self, px: f32, py: f32) -> bool {
        let rects = self.get_param_rects();
        let hit = self.display_params.iter().enumerate().position(|(i, p)| {
            if p.2 != "section" {
                return false;
            }
            let (bx, by, bw, bh) = self.section_title_box(i, rects[i]);
            px >= bx && px <= bx + bw && py >= by && py <= by + bh
        });
        if let Some(i) = hit {
            let title = self.display_params[i].0.clone();
            let collapsed = self.collapsed.contains(&title);
            // Collapsing out from under a focused row would strand the editor.
            self.commit_and_unfocus();
            self.set_section_collapsed(&title, !collapsed);
            return true;
        }
        false
    }

    /// An open dropdown, or a ramp row's open field dropdown, takes the event first: they are
    /// drawn over the rows, so they may cover a neighbouring row's control.
    pub(super) fn press_open_popovers(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ui: &mut UiContext, hidden: &[bool]) -> bool {
        // 1. Check open dropdown popovers first (since they are drawn on top)
        for (i, d_opt) in self.choices.iter_mut().enumerate() {
            if hidden[i] {
                continue;
            }
            if let Some(d) = d_opt {
                if std::env::var("CCE_PARAM_DEBUG").is_ok() {
                    eprintln!("[pdbg] press ({px:.0},{py:.0}) choice[{i}] open-priority: popover_rect={:?}", d.popover_rect());
                }
                if let Some((ox, oy, ow, oh)) = d.popover_rect() {
                    // Textpick pickers are press-driven end to end
                    // (selection fires on the option PRESS): a release
                    // over the open surface is swallowed, never
                    // dispatched — mid-animation it can read as an
                    // outside press and close the menu it just opened.
                    if state != ElementState::Pressed
                        && self.display_params[i].2.starts_with("textpick")
                    {
                        if px >= ox && px <= ox + ow && py >= oy && py <= oy + oh {
                            return true;
                        }
                        continue;
                    }
                    let consumed = d.mouse_input(button, state, px, py, ui);
                    if std::env::var("CCE_PARAM_DEBUG").is_ok() {
                        eprintln!("[pdbg]   -> open dropdown consumed={consumed}");
                    }
                    if consumed {
                        if d.take_change() {
                            if let Some(val) = d.get_value_string() {
                                if self.display_params[i].2.starts_with("textpick") {
                                    fill_from_pick(&mut self.texts[i], &mut self.display_params[i].1, val);
                                } else {
                                    self.display_params[i].1 = val;
                                }
                            }
                        }
                        return true;
                    }
                }
            }
        }
        // The ramp rows' field dropdowns can pop over neighboring rows too.
        for (i, rp_opt) in self.ramps.iter_mut().enumerate() {
            if hidden[i] {
                continue;
            }
            if let Some(rp) = rp_opt {
                let ramp = rp.inner();
                if (ramp.preset_dropdown.popover_rect().is_some()
                    || ramp.line_type_dropdown.popover_rect().is_some())
                    && rp.mouse_input(button, state, px, py, ui) {
                        self.display_params[i].1 = rp.inner().spec_string();
                        return true;
                    }
            }
        }
        false
    }

    /// The rows' controls, in row order: the first that takes the event claims it.
    pub(super) fn press_row_controls(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ui: &mut UiContext, hidden: &[bool]) -> bool {
        for i in 0..self.display_params.len() {
            if !hidden[i] && self.press_row_control(i, button, state, px, py, ui) {
                return true;
            }
        }
        false
    }

    /// Row `i`'s control takes a press or release, if it is its. The row's value is written
    /// back from the control, and the row holds the param focus while its control is open or
    /// editing ([`hold_focus`]): the key path is gated on `focused_param`, so a control that
    /// never set it (an open choice dropdown, once) was out of reach of Escape, the arrows and
    /// Enter.
    pub(super) fn press_row_control(&mut self, i: usize, button: MouseButton, state: ElementState, px: f32, py: f32, ui: &mut UiContext) -> bool {
        let p = &mut self.display_params[i];
        if p.2.starts_with("choice") {
            let Some(d) = &mut self.choices[i] else { return false };
            if !d.mouse_input(button, state, px, py, ui) {
                return false;
            }
            hold_focus(&mut self.focused_param, i, d.open);
            if d.take_change() {
                if let Some(val) = d.get_value_string() {
                    p.1 = val;
                }
            }
            true
        } else if p.2 == "button" {
            let Some(b) = &mut self.buttons[i] else { return false };
            if !b.mouse_input(button, state, px, py, ui) {
                return false;
            }
            if std::env::var("CCE_PARAM_DEBUG").is_ok() {
                eprintln!("[pdbg] press ({px:.0},{py:.0}) BUTTON[{i}] '{}' consumed", p.0);
            }
            if b.take_click() {
                p.1 = "clicked".to_string();
            }
            true
        } else if is_text_row(&p.2) {
            self.press_text_row(i, button, state, px, py, ui)
        } else if p.2.starts_with("spinbox") {
            let Some(sb) = &mut self.spinboxes[i] else { return false };
            if !sb.mouse_input(button, state, px, py, ui) {
                return false;
            }
            p.1 = sb.value.to_string();
            hold_focus(&mut self.focused_param, i, sb.editing);
            true
        } else if p.2 == "toggle" || p.2 == "checkbox" {
            let Some(cb) = &mut self.toggles[i] else { return false };
            if !cb.mouse_input(button, state, px, py, ui) {
                return false;
            }
            if cb.take_change() {
                if let Some(val) = cb.get_value_string() {
                    p.1 = val;
                }
            }
            true
        } else if p.2.starts_with("color") || p.2 == "rgb" || p.2 == "rgba" {
            let Some(c) = &mut self.colors[i] else { return false };
            if !c.mouse_input(button, state, px, py, ui) {
                return false;
            }
            if let Some(val) = c.get_value_string() {
                p.1 = val;
            }
            hold_focus(&mut self.focused_param, i, c.editing);
            true
        } else if p.2 == "ramp" {
            let Some(rp) = &mut self.ramps[i] else { return false };
            if !rp.mouse_input(button, state, px, py, ui) {
                return false;
            }
            p.1 = rp.inner().spec_string();
            true
        } else {
            false
        }
    }

    /// A press or release on text row `i`: its picker first (a textpick row), then its box.
    /// The picker acts on PRESSES only, and a release over its button is swallowed: a release
    /// reaching the dropdown before the open animation's first frame (a fast or injected
    /// click) read as an outside press and closed the menu it had just opened.
    pub(super) fn press_text_row(&mut self, i: usize, button: MouseButton, state: ElementState, px: f32, py: f32, ui: &mut UiContext) -> bool {
        let p = &mut self.display_params[i];
        if let Some(d) = &mut self.choices[i] {
            let (bx, by, bw, bh) = d.rect();
            let on_button = px >= bx && px <= bx + bw && py >= by && py <= by + bh;
            if state == ElementState::Pressed {
                if d.mouse_input(button, state, px, py, ui) {
                    if d.take_change() {
                        if let Some(val) = d.get_value_string() {
                            fill_from_pick(&mut self.texts[i], &mut p.1, val);
                        }
                    }
                    return true;
                }
            } else if on_button {
                return true;
            }
        }
        let Some(tb) = &mut self.texts[i] else { return false };
        if !tb.mouse_input(button, state, px, py, ui) {
            return false;
        }
        hold_focus(&mut self.focused_param, i, tb.editing);
        if tb.take_change() {
            if let Some(val) = tb.get_value_string() {
                p.1 = val;
            }
        }
        true
    }

    /// A left press nothing else claimed: it focuses the code box, slider readout or vector
    /// field it landed on, and otherwise commits and unfocuses the focused row.
    pub(super) fn focus_on_press(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ui: &mut UiContext, hidden: &[bool]) {
        let rects = self.get_param_rects();
        let mut clicked_any_focusable = false;
        for i in 0..self.display_params.len() {
            if hidden[i] {
                continue;
            }
            let kind = self.display_params[i].2.as_str();
            let r = rects[i];
            let in_row_band = py >= r.1 && py <= r.1 + r.3;
            if kind == "code" {
                if px >= r.0 && px <= r.0 + r.2 && py >= r.1 + Self::CODE_BOX_TOP && py <= r.1 + r.3 {
                    self.place_code_caret(i, r, px, py, ui.shift_pressed);
                    clicked_any_focusable = true;
                    break;
                }
            } else if kind.starts_with("slider") && in_row_band {
                if let Some(s) = &mut self.sliders[i] {
                    if s.mouse_input(button, state, px, py, ui) {
                        if s.editing {
                            self.focused_param = Some(i);
                            clicked_any_focusable = true;
                        }
                        break;
                    }
                }
            } else if is_vec_row(kind) && in_row_band {
                if let Some(f) = &mut self.float3s[i] {
                    if f.mouse_input(button, state, px, py, ui) {
                        if f.editing_idx().is_some() {
                            self.focused_param = Some(i);
                            clicked_any_focusable = true;
                        }
                        break;
                    }
                }
            }
        }
        if !clicked_any_focusable {
            self.commit_and_unfocus();
        }
    }
}

/// The param focus names row `i` while its control is open or editing (`holding`), and lets
/// it go when the control stops — only if it still names `i`.
pub(super) fn hold_focus(focused: &mut Option<usize>, i: usize, holding: bool) {
    if holding {
        *focused = Some(i);
    } else if *focused == Some(i) {
        *focused = None;
    }
}

/// A pick from a textpick row's picker fills its text box: the box IS the row's value.
pub(super) fn fill_from_pick(tb: &mut Option<Adapted<TextBox>>, value: &mut String, val: String) {
    if let Some(tb) = tb {
        tb.text = val.clone();
        tb.edit_buffer = val.clone();
    }
    *value = val;
}
