//! The input-method composition: a provisional run in the edit buffer, never committed or
//! recorded, dropped (and the input method told) on a press or when editing ends.

use super::*;

impl TextBox {

    /// `edit_buffer` without an input method's provisional run: what the box
    /// holds, as opposed to what it shows.
    pub fn committed_buffer(&self) -> String {
        match self.composing {
            Some((start, len)) => self.edit_buffer.chars().enumerate().filter(|(i, _)| *i < start || *i >= start + len).map(|(_, c)| c).collect(),
            None => self.edit_buffer.clone(),
        }
    }

    /// Take the provisional run out of the buffer, the caret back where it
    /// began. Whether there was one.
    pub(super) fn strip_composition(&mut self) -> bool {
        let Some((start, _)) = self.composing else { return false };
        self.edit_buffer = self.committed_buffer();
        self.composing = None;
        self.cursor_idx = start.min(self.edit_buffer.chars().count());
        self.select_anchor = None;
        self.all_selected = false;
        true
    }

    /// Drop a composition this box is showing, and have the input method
    /// cancel it: the box is no longer where it is going.
    pub(super) fn abandon_composition(&mut self) {
        if self.strip_composition() {
            crate::ime::request_reset();
            self.sync_editor_state();
        }
        self.ime_seen = crate::ime::generation();
    }

    /// Show the input method's current composition, if it has moved since
    /// this box last showed one: the old run out, the new one in at the
    /// caret, the caret where the input method has its cursor. A composition
    /// begun over a selection replaces it, as typing would.
    pub(super) fn sync_preedit(&mut self) {
        if !self.editing {
            return;
        }
        let generation = crate::ime::generation();
        if generation == self.ime_seen {
            return;
        }
        self.ime_seen = generation;
        self.strip_composition();
        if let Some(p) = crate::ime::preedit() {
            let before = self.snapshot();
            if before.all_selected || before.selected_range().is_some() {
                let mut state = before.clone();
                state.insert_text("");
                self.history.record(before);
                self.edit_buffer = state.buffer;
                self.cursor_idx = state.cursor_idx;
            }
            let start = self.cursor_idx.min(self.edit_buffer.chars().count());
            let at = self.edit_buffer.char_indices().nth(start).map_or(self.edit_buffer.len(), |(b, _)| b);
            self.edit_buffer.insert_str(at, &p.text);
            self.composing = Some((start, p.text.chars().count()));
            self.cursor_idx = start + p.caret_chars();
            self.select_anchor = None;
            self.all_selected = false;
        }
        self.sync_editor_state();
        self.scroll_to_cursor();
    }
}
