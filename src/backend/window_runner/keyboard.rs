//! Keyboard input through xkb into the driver, and the text input (`text-input-v3`) — the input
//! method's way in.

use super::*;

impl<A: Application> KeyboardHandler for EngineState<A> {
    fn enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface,
        _serial: u32,
        _raw_modifiers: &[u32],
        _keysyms: &[xkeysym::Keysym],
    ) {
        if let Some(publisher) = self.a11y.as_mut() {
            publisher.window_focus(true);
        }
        let (driver, t) = self.turn();
        driver.keyboard_focus(t, true);
    }

    fn leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface,
        _serial: u32,
    ) {
        if let Some(publisher) = self.a11y.as_mut() {
            publisher.window_focus(false);
        }
        let (driver, t) = self.turn();
        driver.keyboard_focus(t, false);
    }

    fn press_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: smithay_client_toolkit::seat::keyboard::KeyEvent,
    ) {
        self.handle_key(event, ElementState::Pressed);
    }

    fn release_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: smithay_client_toolkit::seat::keyboard::KeyEvent,
    ) {
        self.handle_key(event, ElementState::Released);
    }

    fn update_modifiers(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        modifiers: smithay_client_toolkit::seat::keyboard::Modifiers,
        _layout: u32,
    ) {
        let mods = Modifiers {
            ctrl: modifiers.ctrl,
            shift: modifiers.shift,
            alt: modifiers.alt,
            logo: modifiers.logo,
        };
        self.driver.set_modifiers(self.inner.as_mut().unwrap(), mods);
    }

    fn update_repeat_info(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        info: smithay_client_toolkit::seat::keyboard::RepeatInfo,
    ) {
        match info {
            smithay_client_toolkit::seat::keyboard::RepeatInfo::Repeat { rate, delay } => {
                // Store/expose delay/rate if required by the application
                let _ = (rate, delay);
            }
            smithay_client_toolkit::seat::keyboard::RepeatInfo::Disable => {}
        }
    }
}

impl<A: Application> EngineState<A> {
    pub(super) fn handle_key(&mut self, event: smithay_client_toolkit::seat::keyboard::KeyEvent, state: ElementState) {
        let Some(logical_key) = xkb_logical_key(&event, self.driver.mods.ctrl) else { return };
        let (driver, t) = self.turn();
        driver.key(t, logical_key, event.utf8, state);
    }
}

/// An xkb key event as one of cce-ui's keys, or `None` for a key with no
/// meaning to it (no name here and no text).
pub(super) fn xkb_logical_key(event: &smithay_client_toolkit::seat::keyboard::KeyEvent, ctrl: bool) -> Option<Key> {
    Some(match event.keysym {
        xkeysym::Keysym::Escape => Key::Named(NamedKey::Escape),
        xkeysym::Keysym::Return => Key::Named(NamedKey::Enter),
        xkeysym::Keysym::BackSpace => Key::Named(NamedKey::Backspace),
        xkeysym::Keysym::Down => Key::Named(NamedKey::ArrowDown),
        xkeysym::Keysym::Up => Key::Named(NamedKey::ArrowUp),
        xkeysym::Keysym::Left => Key::Named(NamedKey::ArrowLeft),
        xkeysym::Keysym::Right => Key::Named(NamedKey::ArrowRight),
        // xkb reports Shift+Tab as ISO_Left_Tab; apps see plain Tab plus
        // the shift modifier, matching winit.
        xkeysym::Keysym::Tab | xkeysym::Keysym::ISO_Left_Tab => Key::Named(NamedKey::Tab),
        xkeysym::Keysym::Delete => Key::Named(NamedKey::Delete),
        xkeysym::Keysym::space => Key::Named(NamedKey::Space),
        xkeysym::Keysym::Page_Up => Key::Named(NamedKey::PageUp),
        xkeysym::Keysym::Page_Down => Key::Named(NamedKey::PageDown),
        xkeysym::Keysym::Home => Key::Named(NamedKey::Home),
        xkeysym::Keysym::End => Key::Named(NamedKey::End),
        xkeysym::Keysym::Super_L | xkeysym::Keysym::Super_R => Key::Named(NamedKey::Super),
        xkeysym::Keysym::Alt_L | xkeysym::Keysym::Alt_R => Key::Named(NamedKey::Alt),
        xkeysym::Keysym::Control_L | xkeysym::Keysym::Control_R => Key::Named(NamedKey::Control),
        xkeysym::Keysym::Shift_L | xkeysym::Keysym::Shift_R => Key::Named(NamedKey::Shift),
        xkeysym::Keysym::F1 => Key::Named(NamedKey::F1),
        xkeysym::Keysym::F2 => Key::Named(NamedKey::F2),
        xkeysym::Keysym::F3 => Key::Named(NamedKey::F3),
        xkeysym::Keysym::F4 => Key::Named(NamedKey::F4),
        xkeysym::Keysym::F5 => Key::Named(NamedKey::F5),
        xkeysym::Keysym::F6 => Key::Named(NamedKey::F6),
        xkeysym::Keysym::F7 => Key::Named(NamedKey::F7),
        xkeysym::Keysym::F8 => Key::Named(NamedKey::F8),
        xkeysym::Keysym::F9 => Key::Named(NamedKey::F9),
        xkeysym::Keysym::F10 => Key::Named(NamedKey::F10),
        xkeysym::Keysym::F11 => Key::Named(NamedKey::F11),
        xkeysym::Keysym::F12 => Key::Named(NamedKey::F12),
        _ => {
            // With Ctrl held, xkb's utf8 goes through the legacy control-character
            // transformation (ctrl+j = "\n", ctrl+a = 0x01, ...); the keysym is
            // untransformed, so prefer it there or ctrl+<letter> shortcuts can
            // never match their letter.
            let from_keysym = || event.keysym.key_char().map(|ch| ch.to_string());
            let text = if ctrl { from_keysym().or_else(|| event.utf8.clone()) } else { event.utf8.clone().or_else(from_keysym) };
            Key::Character(text?)
        }
    })
}

impl<A: Application> wayland_client::Dispatch<ZwpTextInputManagerV3, ()> for EngineState<A> {
    fn event(
        _state: &mut Self,
        _proxy: &ZwpTextInputManagerV3,
        _event: <ZwpTextInputManagerV3 as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

/// The input method's events (see `backend::text_input`): the focus, and
/// the double-buffered composition, commit and deletion, applied on `done`.
impl<A: Application> wayland_client::Dispatch<ZwpTextInputV3, ()> for EngineState<A> {
    fn event(
        state: &mut Self,
        _proxy: &ZwpTextInputV3,
        event: zwp_text_input_v3::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        use crate::backend::text_input::Apply;
        match event {
            zwp_text_input_v3::Event::Enter { .. } => {
                state.text_input_state.enter();
                // At once, at the last frame's caret: an idle window builds
                // no frame to do it.
                state.sync_text_input();
            }
            zwp_text_input_v3::Event::Leave { .. } => {
                if state.text_input_state.leave() {
                    if let Some(ti) = &state.text_input {
                        ti.disable();
                        ti.commit();
                    }
                }
                let (driver, t) = state.turn();
                driver.preedit(t, None);
            }
            zwp_text_input_v3::Event::PreeditString { text, cursor_begin, cursor_end } => {
                state.text_input_state.pending.set_preedit(text, cursor_begin, cursor_end);
            }
            zwp_text_input_v3::Event::CommitString { text } => {
                state.text_input_state.pending.commit = text;
            }
            zwp_text_input_v3::Event::DeleteSurroundingText { before_length, after_length } => {
                state.text_input_state.pending.delete = Some((before_length, after_length));
            }
            zwp_text_input_v3::Event::Done { .. } => {
                for step in state.text_input_state.done().apply_order() {
                    let (driver, t) = state.turn();
                    match step {
                        Apply::Preedit(p) => driver.preedit(t, p),
                        Apply::Commit(text) => driver.commit_text(t, text),
                    }
                }
            }
            _ => {}
        }
    }
}
