//! The input method's composition: taken up onto the caret's line and laid out in place, dropped,
//! and the columns mapped between the line as laid out and its source.

use super::*;

impl DocEditor {
    /// Take up the input method's composition, if it has moved since the
    /// editor last did: the caret line relaid out with it in. A composition
    /// begun over a selection replaces it, as typing would.
    pub(super) fn sync_ime(&mut self) {
        let generation = crate::ime::generation();
        if generation == self.ime_seen {
            return;
        }
        self.ime_seen = generation;
        let next = crate::ime::preedit();
        if next.is_some() && self.composition.is_none() && self.buf.delete_selection() {
            self.sync();
        }
        if let Some((at, _)) = self.composition.take() {
            self.relayout(at.line);
        }
        if let Some(p) = next {
            let at = self.buf.caret;
            self.buf.anchor = None;
            self.composition = Some((at, p));
            self.relayout(at.line);
            self.follow_caret = true;
        }
    }

    /// The editor lost the keyboard, or a press moved its caret: a
    /// composition it was showing is dropped, and the input method asked to
    /// cancel it.
    pub fn drop_composition(&mut self) {
        if let Some((at, _)) = self.composition.take() {
            self.relayout(at.line);
            crate::ime::request_reset();
        }
        self.ime_seen = crate::ime::generation();
    }

    /// Whether an input method's composition is showing.
    pub fn composing(&self) -> bool {
        self.composition.is_some()
    }

    pub(super) fn relayout(&mut self, line: usize) {
        if let Some(l) = self.layouts.get_mut(line) {
            *l = None;
        }
        self.tops_dirty = true;
    }

    /// The composition on `line`: its source column, its length in bytes,
    /// and the bytes into it the input method has its cursor.
    pub(super) fn composition_on(&self, line: usize) -> Option<(usize, usize, usize)> {
        let (at, p) = self.composition.as_ref().filter(|(at, _)| at.line == line)?;
        let caret = p.text.char_indices().nth(p.caret_chars()).map_or(p.text.len(), |(b, _)| b);
        Some((at.col, p.text.len(), caret))
    }

    /// A source column of `line` as a column of its layout: past a
    /// composition there, moved over it; the caret itself, where the input
    /// method has its cursor in it.
    pub(super) fn laid_col(&self, line: usize, col: usize, is_caret: bool) -> usize {
        match self.composition_on(line) {
            Some((c, _, caret)) if is_caret && col == c => c + caret,
            Some((c, len, _)) if col > c => col + len,
            _ => col,
        }
    }

    /// A column of `line`'s layout as a source column: inside a composition
    /// is where it began.
    pub(super) fn source_col(&self, line: usize, col: usize) -> usize {
        match self.composition_on(line) {
            Some((c, len, _)) if col > c && col < c + len => c,
            Some((c, len, _)) if col >= c + len => col - len,
            _ => col,
        }
    }
}
