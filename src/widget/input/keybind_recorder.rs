use crate::color;
use crate::scene::layout::{Rect, Size};
use crate::scene::paint::PaintCtx;
use crate::widget::model::{Adapted, EventCtx, Input, Layout, Paint};
use crate::widget::*;

/// Keybinding capture field (narrow-trait model, Phase 6as leaf sweep): click to arm,
/// then the next chord/key commits into `value`. Escape, or focus leaving, cancels the
/// recording and leaves `value` as it was. The detached control label rides the
/// adapter's base-label machinery; the model paints only the field itself.
#[derive(Debug, Clone)]
pub struct KeybindRecorder {
    /// The committed chord: changed only by a commit (which raises the change flag) or by
    /// the app.
    pub value: String,
    pub recording: bool,
    pub just_changed: bool,
    /// The modifier-only chord held so far while recording, committed when the last
    /// modifier comes up with no key pressed; dropped when the recording ends otherwise.
    pending: String,
    pressed: bool,
    hovered: bool,
    /// Recessed style, the TextBox's: the field is a well carved into the
    /// plate below with no fill of its own, its rim lit in the highlight
    /// accent while recording (the TextBox's editing treatment). Defaults to
    /// `control_relief()`; the flat style is the shared well frame.
    recessed: Option<bool>,
    /// Keyboard focus (FocusIn / FocusOut): Enter / Space arm recording.
    focused: bool,
}

impl KeybindRecorder {
    /// The style in force: the per-widget override (`with_recessed`) when set, else
    /// the DE's `control_relief`, read live so a runtime switch
    /// (`layout::set_control_relief`) restyles every control at once.
    fn recessed(&self) -> bool {
        self.recessed.unwrap_or_else(crate::layout::control_relief)
    }

    pub fn new(value: String) -> Adapted<KeybindRecorder> {
        Adapted::new(KeybindRecorder {
            value,
            recording: false,
            just_changed: false,
            pending: String::new(),
            pressed: false,
            hovered: false,
            recessed: None,
            focused: false,
        })
    }

    pub fn take_change(&mut self) -> bool {
        let changed = self.just_changed;
        self.just_changed = false;
        changed
    }

    fn commit(&mut self, parts: Vec<&str>) {
        self.value = parts.join("+");
        self.just_changed = true;
        self.stop();
    }

    /// Start recording: nothing held yet.
    fn arm(&mut self) {
        self.recording = true;
        self.pending.clear();
    }

    /// Stop recording. Whatever modifiers were held go with it; `value` is whatever was
    /// last committed, so a cancelled recording shows the binding the document has.
    fn stop(&mut self) {
        self.recording = false;
        self.pending.clear();
    }
}

impl Adapted<KeybindRecorder> {
    /// Recessed style: see the `recessed` field.
    pub fn with_recessed(mut self, recessed: bool) -> Self {
        self.recessed = Some(recessed);
        self
    }
}

impl Layout for KeybindRecorder {
    fn intrinsic_size(&self) -> Option<Size> {
        Some(Size::new(0.0, crate::layout::textbox_height()))
    }
}

impl Paint for KeybindRecorder {
    fn color(&self) -> [f32; 4] {
        // A well: the plate is its floor, in both styles.
        [0.0, 0.0, 0.0, 0.0]
    }

    /// The field text in the TextBox's font: both are wells you type into.
    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::control_label_font_detached())
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        if self.recessed() {
            let radius = crate::layout::textbox_corner_radius();
            let depth = crate::layout::bevel_width().min(rect.height * 0.2);
            let (well, radii) = crate::layout::carve_inside(rect, (radius, radius, radius, radius), depth);
            if self.recording {
                let hc = crate::color::highlight_primary_color();
                ctx.recess_tinted(well, radii, depth, [hc[0], hc[1], hc[2]]);
            } else {
                ctx.recess(well, radii, depth);
            }
            self.paint_text(rect, ctx);
            return;
        }
        // The flat style: the one well frame (`color::well_frame_color`), lit
        // while recording, over the plate — no floor of its own, like the
        // TextBox it stands beside. Rounded like the text wells.
        let radius = crate::layout::textbox_corner_radius();
        let frame = color::well_frame_color(self.hovered, self.recording || self.pressed);
        ctx.border(rect, (radius, radius, radius, radius), [0.0; 4], frame, 1.0);

        self.paint_text(rect, ctx);
    }
}

impl KeybindRecorder {
    fn paint_text(&self, rect: Rect, ctx: &mut PaintCtx) {
        let (display_text, color) = if self.recording {
            (crate::l10n::tr("keybind-recording"), [135, 135, 153])
        } else if self.value.is_empty() {
            (crate::l10n::tr("keybind-none"), [127, 127, 127])
        } else {
            (self.value.clone(), [221, 221, 226])
        };
        let (_, font_size) = crate::layout::control_label_font_detached_parsed();
        let text_y = crate::layout::align_text_y(rect.y, rect.height, font_size, 0.0);
        // A recorded chord is as long as the keys pressed into it.
        ctx.text_with(
            display_text,
            rect.x + 8.0,
            text_y,
            font_size,
            color,
            None,
            Some([rect.x, rect.y, rect.x + rect.width, rect.y + rect.height]),
        );
    }
}

impl Input for KeybindRecorder {
    fn focus_role(&self) -> crate::widget::FocusRole {
        crate::widget::FocusRole::Well
    }

    /// The inherent drain, forwarded: the trait's default answers `false`, and with it
    /// neither `ctx[h].take_change()` nor `UiContext::take_events` sees a recorded chord.
    fn take_change(&mut self) -> bool {
        self.take_change()
    }
    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button, state, x, y, .. } => {
                if *button != MouseButton::Left {
                    return false;
                }
                match state {
                    // Presses are adapter hit-gated; releases arrive regardless (the
                    // legacy commit/cancel contract).
                    ElementState::Pressed => {
                        self.pressed = true;
                        true
                    }
                    ElementState::Released => {
                        let r = ectx.rect;
                        let inside = *x >= r.x && *x <= r.x + r.width && *y >= r.y && *y <= r.y + r.height;
                        if self.pressed && inside {
                            self.pressed = false;
                            self.arm();
                            ectx.request_focus();
                            return true;
                        }
                        let was = self.pressed;
                        self.pressed = false;
                        was
                    }
                }
            }
            Event::KeyInput(key_event) if !self.recording => {
                // A focused well not yet recording: Enter / Space arm it (the
                // click's job, by key). Anything else is not this field's.
                if !self.focused || key_event.state != ElementState::Pressed {
                    return false;
                }
                match key_event.logical_key {
                    Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                        self.arm();
                        true
                    }
                    _ => false,
                }
            }
            Event::KeyInput(key_event) => {
                let Some(ui) = ectx.ui.as_deref_mut() else {
                    return false;
                };
                let mut parts = Vec::new();
                if ui.logo_pressed {
                    parts.push("super");
                }
                if ui.ctrl_pressed {
                    parts.push("ctrl");
                }
                if ui.alt_pressed {
                    parts.push("alt");
                }
                if ui.shift_pressed {
                    parts.push("shift");
                }

                match key_event.state {
                    ElementState::Pressed => match &key_event.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            self.stop();
                            true
                        }
                        Key::Named(NamedKey::Control)
                        | Key::Named(NamedKey::Shift)
                        | Key::Named(NamedKey::Alt)
                        | Key::Named(NamedKey::Super) => {
                            // Modifier pressed: track the active-modifier chord so far.
                            if !parts.is_empty() {
                                self.pending = parts.join("+");
                            }
                            true
                        }
                        Key::Named(key) => {
                            let key_str = match key {
                                NamedKey::Backspace => "backspace",
                                NamedKey::Tab => "tab",
                                NamedKey::Enter => "enter",
                                NamedKey::Space => "space",
                                NamedKey::ArrowDown => "down",
                                NamedKey::ArrowLeft => "left",
                                NamedKey::ArrowRight => "right",
                                NamedKey::ArrowUp => "up",
                                NamedKey::End => "end",
                                NamedKey::Home => "home",
                                NamedKey::PageDown => "pagedown",
                                NamedKey::PageUp => "pageup",
                                NamedKey::Delete => "delete",
                                _ => "",
                            };
                            if !key_str.is_empty() {
                                parts.push(key_str);
                                self.commit(parts);
                            }
                            true
                        }
                        Key::Character(ch) => {
                            let ch = ch.clone();
                            parts.push(ch.as_str());
                            self.commit(parts);
                            true
                        }
                    },
                    ElementState::Released => match &key_event.logical_key {
                        Key::Named(NamedKey::Control)
                        | Key::Named(NamedKey::Shift)
                        | Key::Named(NamedKey::Alt)
                        | Key::Named(NamedKey::Super) => {
                            // Modifier released with nothing else held: commit the
                            // recorded modifier-only binding.
                            if !self.pending.is_empty()
                                && !ui.ctrl_pressed
                                && !ui.shift_pressed
                                && !ui.alt_pressed
                                && !ui.logo_pressed
                            {
                                let chord = std::mem::take(&mut self.pending);
                                self.commit(vec![chord.as_str()]);
                            }
                            true
                        }
                        _ => true,
                    },
                }
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
                self.stop();
                false
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A recorded chord reaches the host's `take_change`: the `Input` impl forwards the
    /// inherent drain.
    #[test]
    fn a_recorded_chord_reaches_the_hosts_poll() {
        let mut k = KeybindRecorder::new("ctrl+a".to_string());
        k.inner_mut().commit(vec!["ctrl", "b"]);
        assert!(Adapted::take_change(&mut k));
        assert!(!Adapted::take_change(&mut k), "drained");
    }

    fn key(state: ElementState, logical_key: Key) -> KeyEvent {
        KeyEvent { state, logical_key, text: None, repeat: false, ctrl: false, shift: false, alt: false }
    }

    /// A recorder holding `ctrl+a`, armed, with Ctrl pressed into it.
    fn armed_with_ctrl_held() -> (crate::context::UiContext, Handle<Adapted<KeybindRecorder>>) {
        let mut ctx = crate::context::UiContext::new();
        let k = ctx.insert(KeybindRecorder::new("ctrl+a".to_string()));
        ctx.focus_id(k.id());
        let enter = key(ElementState::Pressed, Key::Named(NamedKey::Enter));
        ctx.lend_h(k, |w, ctx| w.keyboard_input(&enter, ctx));
        assert!(ctx[k].recording, "Enter arms the focused field");
        ctx.ctrl_pressed = true;
        let ctrl = key(ElementState::Pressed, Key::Named(NamedKey::Control));
        ctx.lend_h(k, |w, ctx| w.keyboard_input(&ctrl, ctx));
        (ctx, k)
    }

    /// Escape cancels: the field shows the committed binding again, not the modifier it
    /// was holding, and raises no change (the document still has `ctrl+a`).
    #[test]
    fn escape_keeps_the_committed_binding() {
        let (mut ctx, k) = armed_with_ctrl_held();
        let esc = key(ElementState::Pressed, Key::Named(NamedKey::Escape));
        ctx.lend_h(k, |w, ctx| w.keyboard_input(&esc, ctx));
        assert!(!ctx[k].recording);
        assert_eq!(ctx[k].value, "ctrl+a");
        // Ctrl coming up after the cancel commits nothing either.
        ctx.ctrl_pressed = false;
        let up = key(ElementState::Released, Key::Named(NamedKey::Control));
        ctx.lend_h(k, |w, ctx| w.keyboard_input(&up, ctx));
        assert_eq!(ctx[k].value, "ctrl+a");
        assert!(!ctx[k].take_change(), "a cancelled recording changes nothing");
    }

    /// Focus leaving mid-recording cancels the same way.
    #[test]
    fn focus_leaving_keeps_the_committed_binding() {
        let (mut ctx, k) = armed_with_ctrl_held();
        ctx.unfocus_id(k.id());
        assert!(!ctx[k].recording);
        assert_eq!(ctx[k].value, "ctrl+a");
        assert!(!ctx[k].take_change());
    }

    /// A modifier-only chord still commits when its last modifier comes up.
    #[test]
    fn a_modifier_only_chord_commits_on_release() {
        let (mut ctx, k) = armed_with_ctrl_held();
        ctx.ctrl_pressed = false;
        let up = key(ElementState::Released, Key::Named(NamedKey::Control));
        ctx.lend_h(k, |w, ctx| w.keyboard_input(&up, ctx));
        assert!(!ctx[k].recording);
        assert_eq!(ctx[k].value, "ctrl");
        assert!(ctx[k].take_change());
    }
}
