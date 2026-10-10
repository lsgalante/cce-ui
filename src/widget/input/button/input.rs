//! A `Button`'s input: a press arms it, a release commits or cancels, hover follows the pointer,
//! and Enter / Space press it while it has the keyboard.

use super::*;

impl Input for Button {
    /// A plate — except a ListRow or MenuItem, which wears no plate (see
    /// [`Button::plate`]): a list's rows are walked by the list, not by Tab.
    fn focus_role(&self) -> crate::widget::FocusRole {
        match self.kind {
            ButtonKind::ListRow | ButtonKind::MenuItem => crate::widget::FocusRole::None,
            _ => crate::widget::FocusRole::Plate,
        }
    }
    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, .. } => {
                // Presses are hit-gated by the adapter.
                self.pressed = true;
                true
            }
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Released, x, y, .. } => {
                // Releases arrive ungated: commit in-rect, cancel anywhere else — the legacy
                // `mouse_input` released-while-pressed contract.
                if self.pressed && self.hit(ectx.rect, *x, *y) {
                    self.just_clicked = true;
                    if let Some(ref cb) = self.on_click_cb {
                        cb();
                    }
                }
                std::mem::take(&mut self.pressed)
            }
            Event::MouseEnter => {
                self.hovered = true;
                false
            }
            Event::MouseLeave => {
                self.hovered = false;
                false
            }
            Event::FocusIn => {
                self.focused = true;
                false
            }
            Event::FocusOut => {
                self.focused = false;
                false
            }
            Event::KeyInput(key_event) => {
                // Enter/Space activate a focused button, the same chord Dropdown
                // and Menu use. Routed through `just_clicked` + `on_click_cb` so a
                // keyboard press is indistinguishable downstream from a mouse one.
                if !self.focused || key_event.state != ElementState::Pressed {
                    return false;
                }
                match key_event.logical_key {
                    Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                        self.just_clicked = true;
                        if let Some(ref cb) = self.on_click_cb {
                            cb();
                        }
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    fn take_click(&mut self) -> bool {
        std::mem::take(&mut self.just_clicked)
    }

    fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }
}
