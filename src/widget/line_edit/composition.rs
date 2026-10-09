//! The input method's composition: taking it up each frame, dropping it, the span to underline,
//! and the maps between offsets into `text` and into what is displayed (across the composition
//! and a masked field's bullets).

use super::*;

impl LineEdit {
    /// Take up the input method's composition, if it has moved since this
    /// field last did. Call each frame the field has the keyboard, before
    /// drawing it; true when what [`LineEdit::display`] shows changed. A
    /// composition begun over a selection replaces it, as typing would.
    pub fn sync_ime(&mut self) -> bool {
        let generation = ime::generation();
        if generation == self.ime_seen {
            return false;
        }
        self.ime_seen = generation;
        let next = ime::preedit();
        if next.is_some() && self.composition.is_none() && self.selection.is_some_and(|(a, b)| a < b) {
            let before = self.snapshot();
            self.take_selection();
            if !self.masked {
                self.history.record(before);
            }
        }
        if next.is_some() {
            self.selection = None;
        }
        let changed = next != self.composition;
        self.composition = next;
        changed
    }

    /// The field lost the keyboard, or a press moved its caret: a
    /// composition it was showing is dropped, and the input method asked to
    /// cancel it.
    pub fn drop_composition(&mut self) {
        if self.composition.take().is_some() {
            ime::request_reset();
        }
        self.ime_seen = ime::generation();
    }

    /// Whether an input method's composition is showing.
    pub fn composing(&self) -> bool {
        self.composition.is_some()
    }

    /// The composition's span in [`LineEdit::display`], to underline.
    pub fn composition_range(&self) -> Option<(usize, usize)> {
        let p = self.composition.as_ref()?;
        let at = self.display_index_held(self.cursor);
        Some((at, at + self.shown(p).len()))
    }

    /// A byte offset into [`LineEdit::display`] as the offset into `text` it
    /// stands for: the same offset unless the field is masked, where each
    /// bullet stands for one character of the text.
    /// Across a composition: a point inside it is the caret, one after it
    /// the text it is drawn after.
    pub fn text_index(&self, display_at: usize) -> usize {
        match self.composition_range() {
            Some((a, _)) if display_at <= a => self.text_index_held(display_at),
            Some((_, b)) if display_at < b => self.cursor,
            Some((a, b)) => self.text_index_held(display_at - (b - a)),
            None => self.text_index_held(display_at),
        }
    }

    /// [`LineEdit::text_index`] over the held text alone.
    pub(super) fn text_index_held(&self, display_at: usize) -> usize {
        if !self.masked {
            return self.boundary(display_at);
        }
        let n = display_at / '\u{2022}'.len_utf8();
        self.text.char_indices().nth(n).map_or(self.text.len(), |(i, _)| i)
    }

    /// The other direction: a byte offset into `text` (the caret, a selection
    /// edge) as the offset into [`LineEdit::display`] where it is drawn — the
    /// same offset unless masked, where it is that many bullets in.
    ///
    /// Across a composition: the caret is drawn where the input method has
    /// its cursor in it, and what follows the caret after it.
    pub fn display_index(&self, at: usize) -> usize {
        let held = self.display_index_held(at);
        let Some(p) = &self.composition else { return held };
        let at = self.boundary(at);
        if at > self.cursor {
            held + self.shown(p).len()
        } else if at == self.cursor {
            let caret = p.caret_chars();
            held + if self.masked { caret * '\u{2022}'.len_utf8() } else { p.text.char_indices().nth(caret).map_or(p.text.len(), |(b, _)| b) }
        } else {
            held
        }
    }

    /// [`LineEdit::display_index`] over the held text alone.
    pub(super) fn display_index_held(&self, at: usize) -> usize {
        let at = self.boundary(at);
        if !self.masked {
            return at;
        }
        self.text[..at].chars().count() * '\u{2022}'.len_utf8()
    }
}
