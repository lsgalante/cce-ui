//! The keymap: one key at a time, with edits recorded for undo in runs; undo and redo; and the
//! text and its edits as an accessibility reader sees them.

use super::*;

impl LineEdit {
    pub(super) fn take_selection(&mut self) -> bool {
        match self.selection.take() {
            Some((a, b)) if a < b && b <= self.text.len() => {
                self.text.replace_range(a..b, "");
                self.cursor = a;
                true
            }
            _ => false,
        }
    }

    /// One key. Edits are recorded for [`LineEdit::undo`] the way `TextBox`
    /// records them: a typed run is one step and whitespace starts the next,
    /// so undo walks back a word at a time; a run of Backspace (or Delete)
    /// is one step; replacing a selection, a paste, a cut, a Ctrl word-delete
    /// and Ctrl+U are each their own; and a caret move — by key or click —
    /// ends a run.
    ///
    /// Undo and redo are mostly not keys here: the window runner routes the
    /// DE's `undo` / `redo` chords (`input.kdl`, Ctrl+Z / Ctrl+Shift+Z by
    /// default) to the app, which calls [`LineEdit::undo`] /
    /// [`LineEdit::redo`] on the field that has focus. The one exception is
    /// **Ctrl+Y**, the other redo hands expect, which is not a DE chord and
    /// so arrives here as a key: it redoes, or is `Ignored` with nothing to
    /// redo. (A DE chord bound to Ctrl+Y is offered to the app first, as
    /// any chord is, and never gets this far.)
    pub fn handle_key(&mut self, event: &KeyEvent) -> EditOutcome {
        // While an input method composes, its keys are its own: a shell does
        // not deliver them, and one that does is not editing this text.
        self.sync_ime();
        if self.composition.is_some() {
            return EditOutcome::Edited;
        }
        // Before the recording below, which would file the redo as a fresh
        // edit and so drop everything left to redo.
        if event.state == ElementState::Pressed
            && event.ctrl
            && !event.shift
            && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("y"))
        {
            return if self.redo() { EditOutcome::Edited } else { EditOutcome::Ignored };
        }
        let before = (!self.masked).then(|| self.snapshot());
        let outcome = self.apply_key(event);
        if let Some(before) = before {
            if self.text != before.text {
                let replaced = before.selection.is_some_and(|(a, b)| a < b);
                let group = match &event.logical_key {
                    _ if replaced || event.ctrl => None,
                    Key::Named(NamedKey::Backspace) => Some(2),
                    Key::Named(NamedKey::Delete) => Some(3),
                    Key::Named(NamedKey::Space) => Some(4),
                    _ => {
                        let ws = event.text.as_deref().is_some_and(|t| t.chars().all(char::is_whitespace));
                        Some(if ws { 4 } else { 1 })
                    }
                };
                match group {
                    Some(g) => self.history.record_grouped(before, g),
                    None => self.history.record(before),
                }
            } else if outcome == EditOutcome::Edited {
                // A caret or selection move between keystrokes splits the
                // run: "abc", Left, "d" undoes as two steps.
                self.history.break_group();
            }
        }
        outcome
    }

    /// Step back to before the last edit; true when there was one. The caret
    /// and selection come back with the text. A masked field keeps no
    /// history — holding past versions of a password in memory is a cost
    /// with nothing to show for it — so this is always false there.
    pub fn undo(&mut self) -> bool {
        let current = self.snapshot();
        let Some(prev) = self.history.undo(current) else {
            return false;
        };
        self.restore(prev);
        true
    }

    /// Step forward again after [`LineEdit::undo`]; true when there was a
    /// step to redo. Any new edit after an undo drops what could be redone.
    pub fn redo(&mut self) -> bool {
        let current = self.snapshot();
        let Some(next) = self.history.redo(current) else {
            return false;
        };
        self.restore(next);
        true
    }

    /// The field for a screen reader (`a11y::AppNodes::text_field`): what it holds — a
    /// masked field as bullets, never an input method's composition — and, while it has the
    /// keyboard (`editing`), its selection and caret. The caller adds what the field does
    /// not know: its placeholder, whether it may be set.
    pub fn a11y_text(&self, editing: bool) -> crate::a11y::A11yText {
        let chars = |byte: usize| self.text[..byte.min(self.text.len())].chars().count();
        let len = self.text.chars().count();
        let text = if self.masked { "\u{2022}".repeat(len) } else { self.text.clone() };
        let selection = editing.then(|| match self.selection {
            // A selection is normalized; the caret is at one of its ends.
            Some((a, b)) if self.cursor == a => (chars(b), chars(a)),
            Some((a, b)) => (chars(a), chars(b)),
            None => (chars(self.cursor), chars(self.cursor)),
        });
        crate::a11y::A11yText { text, selection, password: self.masked, editable: true, ..Default::default() }
    }

    /// Replace the text with what a screen reader set (AT-SPI's `SetTextContents`), as the
    /// user replacing it would: undoable (a masked field keeps no history), the caret at its
    /// end, any composition dropped. Whether it changed.
    pub fn a11y_set_text(&mut self, text: &str) -> bool {
        self.drop_composition();
        if self.text == text {
            return false;
        }
        if !self.masked {
            let before = self.snapshot();
            self.history.record(before);
        }
        self.text = text.to_string();
        self.cursor = self.text.len();
        self.selection = None;
        true
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub(super) fn snapshot(&self) -> Snapshot {
        Snapshot { text: self.text.clone(), cursor: self.cursor, selection: self.selection }
    }

    pub(super) fn restore(&mut self, s: Snapshot) {
        self.text = s.text;
        self.cursor = s.cursor;
        self.selection = s.selection;
        self.drag = None;
        self.clicks = None;
    }

    pub(super) fn apply_key(&mut self, event: &KeyEvent) -> EditOutcome {
        if event.state != ElementState::Pressed {
            return EditOutcome::Ignored;
        }
        // Typing between two clicks makes them two clicks, not a double.
        self.clicks = None;
        match &event.logical_key {
            Key::Named(NamedKey::Enter) => return EditOutcome::Submit,
            Key::Named(NamedKey::Escape) => return EditOutcome::Cancel,
            // Ctrl deletes a word: back to where Ctrl+Left would go, or on to
            // where Ctrl+Right would. A selection goes instead, as it does
            // for the plain keys. On a masked field the word edges are the
            // ends, so these clear to the start or the end.
            Key::Named(NamedKey::Backspace) if event.ctrl => {
                if !self.take_selection() {
                    let from = self.word_left(self.cursor);
                    self.text.replace_range(from..self.cursor, "");
                    self.cursor = from;
                }
            }
            Key::Named(NamedKey::Delete) if event.ctrl => {
                if !self.take_selection() {
                    let to = self.word_right(self.cursor);
                    self.text.replace_range(self.cursor..to, "");
                }
            }
            Key::Named(NamedKey::Backspace) => {
                if !self.take_selection() && self.cursor > 0 {
                    let prev = prev_boundary(&self.text, self.cursor);
                    self.text.replace_range(prev..self.cursor, "");
                    self.cursor = prev;
                }
            }
            Key::Named(NamedKey::Delete) => {
                if !self.take_selection() && self.cursor < self.text.len() {
                    let next = next_boundary(&self.text, self.cursor);
                    self.text.replace_range(self.cursor..next, "");
                }
            }
            // Arrows collapse a selection to the edge they move toward.
            // With Shift the caret moves and the selection follows it from
            // its anchor — the end the caret is not at, the same one a
            // shift+click keeps — so it grows, shrinks, or flips past the
            // anchor as the caret goes.
            // Ctrl moves by word, plain or with Shift. Plain, it starts from
            // the selection's edge in the direction it goes, as a plain
            // arrow collapses to that edge.
            Key::Named(NamedKey::ArrowLeft) if event.ctrl => {
                if event.shift {
                    let to = self.word_left(self.cursor);
                    self.select_between(self.anchor(), to);
                } else {
                    let from = self.selection.take().map_or(self.cursor, |(a, _)| a);
                    self.cursor = self.word_left(from);
                }
            }
            Key::Named(NamedKey::ArrowRight) if event.ctrl => {
                if event.shift {
                    let to = self.word_right(self.cursor);
                    self.select_between(self.anchor(), to);
                } else {
                    let from = self.selection.take().map_or(self.cursor, |(_, b)| b);
                    self.cursor = self.word_right(from);
                }
            }
            Key::Named(NamedKey::ArrowLeft) if event.shift => {
                let to = prev_boundary(&self.text, self.cursor);
                self.select_between(self.anchor(), to);
            }
            Key::Named(NamedKey::ArrowRight) if event.shift => {
                let to = next_boundary(&self.text, self.cursor);
                self.select_between(self.anchor(), to);
            }
            Key::Named(NamedKey::Home) if event.shift => self.select_between(self.anchor(), 0),
            Key::Named(NamedKey::End) if event.shift => self.select_between(self.anchor(), self.text.len()),
            Key::Named(NamedKey::ArrowLeft) => {
                self.cursor = match self.selection.take() {
                    Some((a, _)) => a,
                    None => prev_boundary(&self.text, self.cursor),
                };
            }
            Key::Named(NamedKey::ArrowRight) => {
                self.cursor = match self.selection.take() {
                    Some((_, b)) => b,
                    None => next_boundary(&self.text, self.cursor),
                };
            }
            Key::Named(NamedKey::Home) => {
                self.selection = None;
                self.cursor = 0;
            }
            Key::Named(NamedKey::End) => {
                self.selection = None;
                self.cursor = self.text.len();
            }
            Key::Character(c) if event.ctrl => match c.as_str() {
                "a" => self.select_all(),
                "u" => {
                    self.text.clear();
                    self.cursor = 0;
                    self.selection = None;
                }
                // Copy and cut are deliberately absent on a masked field:
                // a password should not leave through the clipboard by a
                // chord the user may not have meant. Paste is allowed, since
                // that is how password managers hand one over.
                "v" => {
                    if let Some(t) = crate::widget::clipboard::read_from_clipboard() {
                        let flat: String = t.chars().filter(|c| !c.is_control()).collect();
                        if !flat.is_empty() {
                            self.take_selection();
                            self.text.insert_str(self.cursor, &flat);
                            self.cursor += flat.len();
                        }
                    }
                }
                "c" | "x" if !self.masked => {
                    if let Some((a, b)) = self.selection.filter(|&(a, b)| a < b) {
                        crate::widget::clipboard::copy_to_clipboard(&self.text[a..b]);
                        if c == "x" {
                            self.take_selection();
                        }
                    }
                }
                _ => return EditOutcome::Ignored,
            },
            _ => {
                let insert = match (&event.text, &event.logical_key) {
                    (Some(t), _) if !event.ctrl && !t.chars().any(char::is_control) => {
                        Some(t.clone())
                    }
                    (None, Key::Named(NamedKey::Space)) => Some(" ".to_string()),
                    (None, Key::Character(c)) if !event.ctrl => Some(c.clone()),
                    _ => return EditOutcome::Ignored,
                };
                if let Some(t) = insert {
                    self.take_selection();
                    self.text.insert_str(self.cursor, &t);
                    self.cursor += t.len();
                }
            }
        }
        EditOutcome::Edited
    }
}
