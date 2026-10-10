//! The keyboard: modifiers, keyboard focus, keys (the plate-navigation and undo/redo chords
//! routed before the app's own handling), committed and composed input-method text, and the
//! runner's key repeat.

use super::*;

impl Driver {
    /// The keyboard reported new modifier state.
    pub fn set_modifiers<A: Application>(&mut self, app: &mut A, mods: Modifiers) {
        self.mods = mods;
        self.sync_mods(app);
    }

    /// The window gained (`true`) or lost keyboard focus. Losing it drops a
    /// held key and the held modifiers, whose releases go elsewhere.
    pub fn keyboard_focus<A: Application>(&mut self, t: Turn<'_, A>, focused: bool) {
        self.note_input();
        if !focused {
            self.pressed_key = None;
            self.mods.ctrl = false;
            self.mods.shift = false;
            self.mods.alt = false;
        }
        let mut rebuild = false;
        t.app.handle_focus_change(focused, &mut rebuild);
        if rebuild {
            *t.redraw = true;
        }
    }

    /// A key went down or up, already in cce-ui's terms: the shell maps its
    /// window system's key (an xkb keysym here) to `logical_key`, and passes
    /// the text the key types, if any.
    pub fn key<A: Application>(
        &mut self,
        mut t: Turn<'_, A>,
        logical_key: Key,
        text: Option<String>,
        state: ElementState,
    ) {
        self.note_input();
        let event = KeyEvent {
            state,
            logical_key,
            text,
            repeat: false,
            ctrl: self.mods.ctrl,
            shift: self.mods.shift,
            alt: self.mods.alt,
        };

        if state == ElementState::Pressed {
            if is_repeatable_key(&event.logical_key) {
                self.pressed_key = Some(PressedKey {
                    logical_key: event.logical_key.clone(),
                    text: event.text.clone(),
                    first_pressed: Instant::now(),
                    last_repeated: Instant::now(),
                });
            } else {
                self.pressed_key = None;
            }
        } else if state == ElementState::Released {
            if let Some(ref pk) = self.pressed_key {
                if pk.logical_key == event.logical_key {
                    self.pressed_key = None;
                }
            }
        }

        self.sync_mods(t.app);

        // Escape dismisses the shared context menu before app dispatch — the
        // toolkit-wide default, mirroring the click-outside dismissal. Consumed:
        // while a menu is open, Escape means "close it", nothing else.
        if state == ElementState::Pressed
            && event.logical_key == Key::Named(NamedKey::Escape)
            && crate::widget::context_menu::is_visible()
        {
            crate::widget::context_menu::hide();
            *t.redraw = true;
            return;
        }

        let mut rebuild = false;
        if self.route_history_chord(t.app, &event, &mut rebuild)
            || self.route_plate_navigation(t.app, &event, &mut rebuild)
        {
            *t.redraw = true;
            return;
        }
        let msg = t.app.handle_key_input(&event, &mut rebuild);
        t.deliver(msg, rebuild);
    }

    /// Text an input method committed. It is delivered as TYPED — a press
    /// of a key whose text it is, then that key's release — so a widget
    /// that inserts what a key types takes it as it is (see `crate::ime`).
    /// Never a shortcut (no Ctrl, no Alt, whatever is held: an input method
    /// commits on its own keys), never repeated, and past the chords: a
    /// commit of "z" is a "z", not half of an undo.
    pub fn commit_text<A: Application>(&mut self, t: Turn<'_, A>, text: String) {
        self.note_input();
        if text.is_empty() {
            return;
        }
        let mut rebuild = false;
        for state in [ElementState::Pressed, ElementState::Released] {
            let event = KeyEvent {
                state,
                logical_key: Key::Character(text.clone()),
                text: (state == ElementState::Pressed).then(|| text.clone()),
                repeat: false,
                ctrl: false,
                shift: self.mods.shift,
                alt: false,
            };
            let msg = t.app.handle_key_input(&event, &mut rebuild);
            if let Some(msg) = msg {
                let mut update_rebuild = false;
                t.app.update(msg, &mut update_rebuild, t.exit);
                rebuild |= update_rebuild;
            }
        }
        *t.redraw |= rebuild;
        // The editing widget shows the text it now holds.
        *t.redraw = true;
    }

    /// Press and release a named key on the app's behalf — an assistive tool's Click or
    /// Increment (`backend::a11y_unix::act`), after it has focused the widget. Through the
    /// app's own `handle_key_input` like any key, but past the chords (a reader's Space is
    /// not Tab), with no modifiers and no text, and never repeated.
    pub fn press_named_key<A: Application>(&mut self, t: Turn<'_, A>, key: NamedKey) {
        self.note_input();
        let mut rebuild = false;
        for state in [ElementState::Pressed, ElementState::Released] {
            let event = KeyEvent {
                state,
                logical_key: Key::Named(key),
                text: None,
                repeat: false,
                ctrl: false,
                shift: false,
                alt: false,
            };
            let msg = t.app.handle_key_input(&event, &mut rebuild);
            if let Some(msg) = msg {
                let mut update_rebuild = false;
                t.app.update(msg, &mut update_rebuild, t.exit);
                rebuild |= update_rebuild;
            }
        }
        *t.redraw = true;
    }

    /// The input method's composition changed (`None`: it ended without a
    /// commit, or the commit follows). The editing widget shows it from the
    /// next frame.
    pub fn preedit<A: Application>(&mut self, t: Turn<'_, A>, preedit: Option<crate::ime::Preedit>) {
        self.note_input();
        crate::ime::set_preedit(preedit);
        *t.redraw = true;
    }

    /// The runner's key repeat: once a held key has been down
    /// [`KEY_REPEAT_DELAY`], deliver it again every [`KEY_REPEAT_INTERVAL`].
    /// Called once per loop turn.
    pub fn repeat_keys<A: Application>(&mut self, mut t: Turn<'_, A>) {
        let Some(ref mut pk) = self.pressed_key else { return };
        let now = Instant::now();
        if now.duration_since(pk.first_pressed) < KEY_REPEAT_DELAY
            || now.duration_since(pk.last_repeated) < KEY_REPEAT_INTERVAL
        {
            return;
        }
        pk.last_repeated = now;
        let event = KeyEvent {
            state: ElementState::Pressed,
            logical_key: pk.logical_key.clone(),
            text: pk.text.clone(),
            repeat: true,
            ctrl: self.mods.ctrl,
            shift: self.mods.shift,
            alt: self.mods.alt,
        };

        self.sync_mods(t.app);

        let mut rebuild = false;
        if self.route_history_chord(t.app, &event, &mut rebuild)
            || self.route_plate_navigation(t.app, &event, &mut rebuild)
        {
            *t.redraw = true;
        } else {
            let msg = t.app.handle_key_input(&event, &mut rebuild);
            t.deliver(msg, false);
        }
        if rebuild {
            *t.redraw = true;
        }
    }

    /// The toolkit's Tab traversal, for apps that opt in
    /// (`Application::plate_navigation`): a bare Tab / Shift+Tab press moves
    /// keyboard focus to the next / previous plate or well. Returns whether it
    /// moved; otherwise the key is dispatched as usual.
    pub(super) fn route_plate_navigation<A: Application>(&self, app: &mut A, event: &KeyEvent, rebuild: &mut bool) -> bool {
        if event.state != ElementState::Pressed {
            return false;
        }
        // The group jump first (its chords carry ctrl); then a bare Tab.
        let group_next = crate::widget::match_key_shortcut(event, &self.group_next_chord);
        let group_prev = !group_next && crate::widget::match_key_shortcut(event, &self.group_prev_chord);
        let bare_tab = event.logical_key == Key::Named(NamedKey::Tab)
            && !self.mods.ctrl
            && !self.mods.alt
            && !self.mods.logo;
        if !group_next && !group_prev && !bare_tab {
            return false;
        }
        let reverse = if bare_tab { self.mods.shift } else { group_prev };
        if !app.plate_navigation() {
            return false;
        }
        // A focused widget that types Tab (a multi-line text box) keeps a bare Tab; the
        // group chord still leaves it.
        if bare_tab
            && app.ui_context_mut().is_some_and(|ctx| {
                ctx.focused_widget.and_then(|id| ctx.get_widget(id)).is_some_and(|w| w.keeps_tab())
            })
        {
            return false;
        }
        let moved = app
            .ui_context_mut()
            .is_some_and(|ctx| if bare_tab { ctx.focus_step(reverse) } else { ctx.focus_step_group(reverse) });
        if moved {
            app.focus_stepped();
            *rebuild = true;
        }
        moved
    }

    /// The toolkit-wide undo/redo routing: a press matching the `undo` /
    /// `redo` chord goes to the focused widget first (`ContextAction::Undo`
    /// / `Redo` — a text box that is editing steps its own typing), then to
    /// the app's `Application::undo` / `redo`. Returns whether either took
    /// it; otherwise the key is dispatched as usual, so an app with its own
    /// scheme is undisturbed. Runs for repeats too — holding the chord walks
    /// the history like holding Backspace walks the text.
    pub(super) fn route_history_chord<A: Application>(&self, app: &mut A, event: &KeyEvent, rebuild: &mut bool) -> bool {
        if event.state != ElementState::Pressed {
            return false;
        }
        let undo = crate::widget::match_key_shortcut(event, &self.undo_chord);
        let redo = !undo && crate::widget::match_key_shortcut(event, &self.redo_chord);
        if !undo && !redo {
            return false;
        }
        let action = if undo { crate::widget::ContextAction::Undo } else { crate::widget::ContextAction::Redo };
        if let Some(ctx) = app.ui_context_mut() {
            if ctx.focused_context_action(action) {
                *rebuild = true;
                return true;
            }
        }
        let taken = if undo { app.undo(rebuild) } else { app.redo(rebuild) };
        if taken {
            *rebuild = true;
        }
        taken
    }
}
