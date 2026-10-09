//! Editing: the editor state, undo and redo, the clipboard, selection, and keys.

use super::*;

impl TextBox {

    pub fn sync_editor_state(&mut self) {
        self.editor_state = TextEditorState {
            buffer: self.edit_buffer.clone(),
            cursor_idx: self.cursor_idx,
            select_anchor: self.select_anchor,
            all_selected: self.all_selected,
        };
    }

    /// The editing fields as one value — what the history stores.
    pub(super) fn snapshot(&self) -> TextEditorState {
        TextEditorState {
            buffer: self.edit_buffer.clone(),
            cursor_idx: self.cursor_idx,
            select_anchor: self.select_anchor,
            all_selected: self.all_selected,
        }
    }

    pub(super) fn restore(&mut self, snap: TextEditorState) {
        self.edit_buffer = snap.buffer;
        self.cursor_idx = snap.cursor_idx;
        self.select_anchor = snap.select_anchor;
        self.all_selected = snap.all_selected;
        self.sync_editor_state();
        self.scroll_to_cursor();
    }

    /// Step the edit buffer back one recorded step. Only while editing —
    /// a committed value is the app's to undo, not the box's.
    pub fn undo_edit(&mut self) -> bool {
        if !self.editing || self.disabled {
            return false;
        }
        let current = self.snapshot();
        match self.history.undo(current) {
            Some(prev) => {
                self.restore(prev);
                self.just_changed = true;
                true
            }
            None => false,
        }
    }

    /// Step forward again — see [`undo_edit`](Self::undo_edit).
    pub fn redo_edit(&mut self) -> bool {
        if !self.editing || self.disabled {
            return false;
        }
        let current = self.snapshot();
        match self.history.redo(current) {
            Some(next) => {
                self.restore(next);
                self.just_changed = true;
                true
            }
            None => false,
        }
    }

    /// The selection as it may go to the clipboard: never a password box's.
    /// Every copy and cut goes through here -- Ctrl+C / Ctrl+X, the context
    /// menu's rows, [`copy_selection`](Self::copy_selection) and
    /// [`cut_selection`](Self::cut_selection) -- because each of them used to
    /// put a password on the clipboard in plain text, readable by any client,
    /// from the login greeter's and the polkit dialog's boxes alike.
    pub(super) fn clipboard_text(&self, state: &TextEditorState) -> Option<String> {
        if self.is_password {
            return None;
        }
        state.selected_text()
    }

    pub fn copy_selection(&self) {
        let state = TextEditorState {
            buffer: self.edit_buffer.clone(),
            cursor_idx: self.cursor_idx,
            select_anchor: self.select_anchor,
            all_selected: self.all_selected,
        };
        if let Some(text) = self.clipboard_text(&state) {
            clipboard::copy_to_clipboard(&text);
        }
    }

    /// Cut the selection to the clipboard. A password box's selection can
    /// not go there (`clipboard_text`), so it is left in place, not deleted.
    pub fn cut_selection(&mut self) -> bool {
        if self.is_password {
            return false;
        }
        let before = self.snapshot();
        let mut state = TextEditorState {
            buffer: std::mem::take(&mut self.edit_buffer),
            cursor_idx: self.cursor_idx,
            select_anchor: self.select_anchor,
            all_selected: self.all_selected,
        };
        if let Some(text) = state.selected_text() {
            clipboard::copy_to_clipboard(&text);
            state.insert_text("");
            self.history.record(before);
            self.edit_buffer = state.buffer;
            self.cursor_idx = state.cursor_idx;
            self.select_anchor = state.select_anchor;
            self.all_selected = state.all_selected;
            self.sync_editor_state();
            true
        } else {
            self.edit_buffer = state.buffer;
            self.sync_editor_state();
            false
        }
    }

    pub fn paste_from_clipboard(&mut self) -> bool {
        if let Some(text) = clipboard::read_from_clipboard() {
            let before = self.snapshot();
            let mut state = TextEditorState {
                buffer: std::mem::take(&mut self.edit_buffer),
                cursor_idx: self.cursor_idx,
                select_anchor: self.select_anchor,
                all_selected: self.all_selected,
            };
            let mut cleaned = String::new();
            for ch in text.chars() {
                if !ch.is_control() && ch != '\n' && ch != '\r' {
                    cleaned.push(ch);
                }
            }
            state.insert_text(&cleaned);
            self.history.record(before);
            self.edit_buffer = state.buffer;
            self.cursor_idx = state.cursor_idx;
            self.select_anchor = state.select_anchor;
            self.all_selected = state.all_selected;
            self.sync_editor_state();
            true
        } else {
            false
        }
    }

    pub fn select_all(&mut self) {
        let mut state = TextEditorState {
            buffer: std::mem::take(&mut self.edit_buffer),
            cursor_idx: self.cursor_idx,
            select_anchor: self.select_anchor,
            all_selected: self.all_selected,
        };
        state.select_all();
        self.edit_buffer = state.buffer;
        self.cursor_idx = state.cursor_idx;
        self.select_anchor = state.select_anchor;
        self.all_selected = state.all_selected;
        self.sync_editor_state();
    }

    /// The legacy `focus()` body minus the global-focus claim (the caller's, via
    /// `EventCtx::request_focus`).
    pub(super) fn begin_editing(&mut self) {
        if self.disabled { return; }
        // A composition still going is the input method's for wherever the
        // caret was; this box starts with none.
        self.composing = None;
        self.ime_seen = crate::ime::generation();
        self.editing = true;
        self.edit_buffer = self.text.clone();
        let len = self.edit_buffer.chars().count();
        self.cursor_idx = len;
        self.select_anchor = Some(0);
        self.all_selected = len > 0;
        self.just_focused = true;
        self.history.clear();
        self.sync_editor_state();
    }

    /// The legacy `unfocus()` body: leave edit mode and commit the buffer.
    pub(super) fn commit_editing(&mut self) {
        if self.editing {
            self.abandon_composition();
            self.editing = false;
            if self.text != self.edit_buffer {
                self.text = self.edit_buffer.clone();
                self.just_changed = true;
            }
            self.select_anchor = None;
            self.all_selected = false;
            self.sync_editor_state();
        }
    }

    /// Extend the selection to a drag position — the shared body of the legacy
    /// `on_cursor_moved` drag arm and `drag_update` (which used no label inset).
    pub(super) fn extend_selection_to(&mut self, px: f32, py: f32) -> bool {
        let drag_idx = self.position_to_idx(px, py);
        if self.cursor_idx != drag_idx {
            self.cursor_idx = drag_idx;
            self.just_focused = false;
            let len = self.edit_buffer.chars().count();
            let start = self.select_anchor.unwrap_or(0).min(self.cursor_idx);
            let end = self.select_anchor.unwrap_or(0).max(self.cursor_idx);
            self.all_selected = start == 0 && end == len && len > 0;
            true
        } else {
            false
        }
    }

    /// Port of the legacy `keyboard_input` body.
    pub(super) fn handle_key(&mut self, event: &KeyEvent) -> bool {
        if !self.editing || self.disabled { return false; }
        if event.state != ElementState::Pressed { return false; }
        // While an input method composes, its keys are its own: a shell does
        // not deliver them, and one that does is not editing this text.
        self.sync_preedit();
        if self.composing.is_some() {
            return true;
        }

        let control = event.ctrl;

        // The undo/redo chords, for apps that hand keys to widgets without
        // exposing a `UiContext` (the runner's routing reaches the box
        // through `ContextAction` first when they do). Before the working
        // copy below, since a step replaces the whole editing state.
        if control {
            if match_key_shortcut(event, &crate::input::widget_chord("undo", "", "ctrl+z")) {
                return self.undo_edit();
            }
            if match_key_shortcut(event, &crate::input::widget_chord("redo", "", "ctrl+shift+z")) {
                return self.redo_edit();
            }
        }

        let before = self.snapshot();
        let mut state = before.clone();

        let handled = match &event.logical_key {
            Key::Named(NamedKey::Backspace) => {
                state.delete_backwards()
            }
            Key::Named(NamedKey::Delete) => {
                state.delete_forwards()
            }
            Key::Named(NamedKey::ArrowLeft) => {
                state.move_cursor_left(event.shift)
            }
            Key::Named(NamedKey::ArrowRight) => {
                state.move_cursor_right(event.shift)
            }
            Key::Named(NamedKey::ArrowUp) => {
                if state.all_selected {
                    state.clear_selection();
                } else if event.shift {
                    if state.select_anchor.is_none() {
                        state.select_anchor = Some(state.cursor_idx);
                    }
                } else {
                    state.clear_selection();
                }
                if self.multiline {
                    let max_w = self.wrap_width(self.rect.width);
                    let (lines, index_map) = self.wrap_text(max_w);
                    let (cursor_l, cursor_c) = index_map[state.cursor_idx.min(index_map.len() - 1)];
                    if cursor_l > 0 {
                        state.cursor_idx = self.map_2d_to_1d(&index_map, cursor_l - 1, cursor_c, lines.len() - 1);
                    } else {
                        state.cursor_idx = 0;
                    }
                } else {
                    state.cursor_idx = 0;
                }
                true
            }
            Key::Named(NamedKey::ArrowDown) => {
                if state.all_selected {
                    state.clear_selection();
                } else if event.shift {
                    if state.select_anchor.is_none() {
                        state.select_anchor = Some(state.cursor_idx);
                    }
                } else {
                    state.clear_selection();
                }
                if self.multiline {
                    let max_w = self.wrap_width(self.rect.width);
                    let (lines, index_map) = self.wrap_text(max_w);
                    let (cursor_l, cursor_c) = index_map[state.cursor_idx.min(index_map.len() - 1)];
                    if cursor_l < lines.len() - 1 {
                        state.cursor_idx = self.map_2d_to_1d(&index_map, cursor_l + 1, cursor_c, lines.len() - 1);
                    } else {
                        state.cursor_idx = state.buffer.chars().count();
                    }
                } else {
                    state.cursor_idx = state.buffer.chars().count();
                }
                true
            }
            Key::Named(NamedKey::Home) => {
                state.move_cursor_to_start(event.shift)
            }
            Key::Named(NamedKey::End) => {
                state.move_cursor_to_end(event.shift)
            }
            Key::Named(NamedKey::Enter) => {
                if self.multiline {
                    state.insert_text("\n");
                    true
                } else {
                    self.commit_editing();
                    true
                }
            }
            Key::Named(NamedKey::Escape) => {
                self.editing = false;
                state.buffer = self.text.clone();
                state.clear_selection();
                true
            }
            Key::Character(ref ch_str) if control && (ch_str == "a" || ch_str == "A") => {
                state.select_all();
                true
            }
            Key::Character(ref ch_str) if control && (ch_str == "c" || ch_str == "C") => {
                if let Some(text) = self.clipboard_text(&state) {
                    clipboard::copy_to_clipboard(&text);
                }
                true
            }
            // A password box's selection stays put: it can not be cut to the
            // clipboard, and deleting it alone would not be a cut.
            Key::Character(ref ch_str) if control && (ch_str == "x" || ch_str == "X") => {
                if let Some(text) = self.clipboard_text(&state) {
                    clipboard::copy_to_clipboard(&text);
                    state.insert_text("");
                }
                true
            }
            Key::Character(ref ch_str) if control && (ch_str == "v" || ch_str == "V") => {
                if let Some(pasted) = clipboard::read_from_clipboard() {
                    let mut cleaned = String::new();
                    for ch in pasted.chars() {
                        if !ch.is_control() && ch != '\n' && ch != '\r' {
                            cleaned.push(ch);
                        }
                    }
                    state.insert_text(&cleaned);
                } else {
                    state.clear_selection();
                }
                true
            }
            _ => {
                if let Some(text) = &event.text {
                    if !control {
                        state.insert_text(text);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
        };

        if self.editing {
            if state.buffer != before.buffer {
                // One step per typed run, per deleted run; whitespace
                // starts a new run so undo walks back a word at a time.
                // Replacing a selection is always its own step.
                let group = match &event.logical_key {
                    _ if before.selected_range().is_some() => None,
                    Key::Named(NamedKey::Backspace) => Some(2),
                    Key::Named(NamedKey::Delete) => Some(3),
                    Key::Character(_) if !control => {
                        let ws = event.text.as_deref().is_some_and(|t| t.chars().all(char::is_whitespace));
                        Some(if ws { 4 } else { 1 })
                    }
                    _ => None,
                };
                match group {
                    Some(g) => self.history.record_grouped(before, g),
                    None => self.history.record(before),
                }
            } else if handled {
                // A cursor or selection move between keystrokes splits the
                // run: "abc", move, "def" undoes as two steps.
                self.history.break_group();
            }
            self.edit_buffer = state.buffer;
            self.cursor_idx = state.cursor_idx;
            self.select_anchor = state.select_anchor;
            self.all_selected = state.all_selected;
            self.sync_editor_state();
            self.scroll_to_cursor();
        }

        handled
    }
}
