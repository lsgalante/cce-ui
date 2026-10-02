//! A one-line text field's model: the text, a caret, and a selection — and
//! the keymap that edits them.
//!
//! For an app that draws its own field rather than using [`TextBox`]: the
//! app keeps a `LineEdit`, feeds it keys with [`LineEdit::handle_key`], and
//! draws [`LineEdit::display`] with the caret and selection wherever its
//! own layout puts them. Enter and Escape come back as [`EditOutcome`]s for
//! the caller to act on, as does any chord the field does not own, so the
//! app's own shortcuts still reach it. The pointer selects by dragging:
//! the app hit-tests a press and each motion to a byte offset and calls
//! [`LineEdit::press`] / [`LineEdit::drag_to`] / [`LineEdit::release`]. cce-browser's URL bar and its dialog
//! fields (HTTP auth, JS prompts) are built on it.
//!
//! Indices are **byte** offsets into `text`, always on a char boundary
//! ([`prev_boundary`] / [`next_boundary`] step them), which is what slicing
//! and shaping want. [`TextEditorState`](super::TextEditorState), the model
//! behind `TextBox`, counts chars and leaves the keymap to its widget;
//! the two are not interchangeable.
//!
//! [`TextBox`]: super::TextBox

use crate::widget::{ElementState, Key, KeyEvent, NamedKey};

/// What a keystroke meant, beyond editing the text.
#[derive(Debug, PartialEq)]
pub enum EditOutcome {
    /// Nothing structural — redraw and carry on.
    Edited,
    /// Enter: the caller commits.
    Submit,
    /// Escape: the caller cancels.
    Cancel,
    /// Not ours (a chord the chrome owns).
    Ignored,
}

#[derive(Default)]
pub struct LineEdit {
    pub text: String,
    pub cursor: usize,
    /// Normalized (start < end). Any edit replaces or drops it.
    pub selection: Option<(usize, usize)>,
    /// Render as bullets. Set for password fields.
    pub masked: bool,
    /// While a pointer button is held after [`LineEdit::press`]: the byte the
    /// selection runs from. `None` when no drag is in progress.
    drag_anchor: Option<usize>,
}

/// The char boundary before byte `i` in `s` (0 at the start). For stepping a
/// caret, or trimming text to fit without splitting a character.
pub fn prev_boundary(s: &str, i: usize) -> usize {
    let mut j = i;
    while j > 0 {
        j -= 1;
        if s.is_char_boundary(j) {
            return j;
        }
    }
    0
}

/// The char boundary after byte `i` in `s` (`s.len()` at the end).
pub fn next_boundary(s: &str, i: usize) -> usize {
    let mut j = i;
    while j < s.len() {
        j += 1;
        if s.is_char_boundary(j) {
            return j;
        }
    }
    s.len()
}

impl LineEdit {
    pub fn with_text(text: impl Into<String>) -> Self {
        let text = text.into();
        Self { cursor: text.len(), text, ..Self::default() }
    }

    pub fn masked() -> Self {
        Self { masked: true, ..Self::default() }
    }

    /// What to draw. Never returns the password itself.
    pub fn display(&self) -> String {
        if self.masked {
            "\u{2022}".repeat(self.text.chars().count())
        } else {
            self.text.clone()
        }
    }

    pub fn select_all(&mut self) {
        self.cursor = self.text.len();
        self.selection = (self.cursor > 0).then_some((0, self.cursor));
    }

    // ---- the pointer ----
    //
    // The field does not know where its characters are drawn; the app does.
    // So the pointer arrives as a byte offset into `text` that the app has
    // already hit-tested (nearest char boundary to the pointer's x on the
    // same shaped run it drew), and for a masked field converted with
    // `text_index`, since what was drawn there is bullets.

    /// A press at byte `at`: the caret goes there and a drag starts from it.
    /// With `extend` (Shift held) the selection's far end stays put and the
    /// selection runs from it to `at` instead — shift+click.
    pub fn press(&mut self, at: usize, extend: bool) {
        let at = self.boundary(at);
        let anchor = if extend { self.anchor() } else { at };
        self.drag_anchor = Some(anchor);
        self.select_between(anchor, at);
    }

    /// The pointer moved to byte `at` with the button still held: the
    /// selection is now everything between the press and here, and the caret
    /// is here. True when that changed anything (a repaint is due); false,
    /// and nothing happens, when no press is in progress.
    pub fn drag_to(&mut self, at: usize) -> bool {
        let Some(anchor) = self.drag_anchor else {
            return false;
        };
        let before = (self.cursor, self.selection);
        self.select_between(anchor, self.boundary(at));
        (self.cursor, self.selection) != before
    }

    /// The button came up: the drag is over, and what it selected stays
    /// selected.
    pub fn release(&mut self) {
        self.drag_anchor = None;
    }

    /// Whether a press is being dragged — the app routes pointer motion to
    /// [`LineEdit::drag_to`] while this holds, wherever the pointer is.
    pub fn dragging(&self) -> bool {
        self.drag_anchor.is_some()
    }

    /// A byte offset into [`LineEdit::display`] as the offset into `text` it
    /// stands for: the same offset unless the field is masked, where each
    /// bullet stands for one character of the text.
    pub fn text_index(&self, display_at: usize) -> usize {
        if !self.masked {
            return self.boundary(display_at);
        }
        let n = display_at / '\u{2022}'.len_utf8();
        self.text.char_indices().nth(n).map_or(self.text.len(), |(i, _)| i)
    }

    /// The other direction: a byte offset into `text` (the caret, a selection
    /// edge) as the offset into [`LineEdit::display`] where it is drawn — the
    /// same offset unless masked, where it is that many bullets in.
    pub fn display_index(&self, at: usize) -> usize {
        let at = self.boundary(at);
        if !self.masked {
            return at;
        }
        self.text[..at].chars().count() * '\u{2022}'.len_utf8()
    }

    /// The end of the selection the caret is not at — what a shift+click
    /// keeps — or the caret itself with nothing selected.
    fn anchor(&self) -> usize {
        match self.selection {
            Some((a, b)) if self.cursor == a => b,
            Some((a, _)) => a,
            None => self.cursor,
        }
    }

    fn select_between(&mut self, anchor: usize, at: usize) {
        self.cursor = at;
        self.selection = (anchor != at).then(|| (anchor.min(at), anchor.max(at)));
    }

    /// `at` clamped into the text and onto a char boundary.
    fn boundary(&self, at: usize) -> usize {
        let at = at.min(self.text.len());
        if self.text.is_char_boundary(at) {
            at
        } else {
            prev_boundary(&self.text, at)
        }
    }

    fn take_selection(&mut self) -> bool {
        match self.selection.take() {
            Some((a, b)) if a < b && b <= self.text.len() => {
                self.text.replace_range(a..b, "");
                self.cursor = a;
                true
            }
            _ => false,
        }
    }

    pub fn handle_key(&mut self, event: &KeyEvent) -> EditOutcome {
        if event.state != ElementState::Pressed {
            return EditOutcome::Ignored;
        }
        match &event.logical_key {
            Key::Named(NamedKey::Enter) => return EditOutcome::Submit,
            Key::Named(NamedKey::Escape) => return EditOutcome::Cancel,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(key: Key, ctrl: bool) -> KeyEvent {
        let text = match &key {
            Key::Character(c) if !ctrl => Some(c.clone()),
            _ => None,
        };
        KeyEvent {
            state: ElementState::Pressed,
            logical_key: key,
            text,
            repeat: false,
            ctrl,
            shift: false,
            alt: false,
        }
    }
    fn ch(c: &str) -> KeyEvent {
        ev(Key::Character(c.into()), false)
    }
    fn ctrl(c: &str) -> KeyEvent {
        ev(Key::Character(c.into()), true)
    }
    fn named(n: NamedKey) -> KeyEvent {
        ev(Key::Named(n), false)
    }

    fn typed(e: &mut LineEdit, s: &str) {
        for c in s.chars() {
            e.handle_key(&ch(&c.to_string()));
        }
    }

    #[test]
    fn typing_inserts_at_the_caret() {
        let mut e = LineEdit::default();
        typed(&mut e, "abc");
        assert_eq!(e.text, "abc");
        assert_eq!(e.cursor, 3);
    }

    /// The URL bar's defining behaviour: entering it selects everything, so
    /// the next keystroke replaces the address rather than appending to it.
    #[test]
    fn typing_over_a_selection_replaces_it() {
        let mut e = LineEdit::with_text("https://example.com");
        e.select_all();
        typed(&mut e, "x");
        assert_eq!(e.text, "x");
        assert_eq!(e.selection, None);
    }

    #[test]
    fn ctrl_a_selects_all_and_ctrl_u_clears() {
        let mut e = LineEdit::with_text("abc");
        e.handle_key(&ctrl("a"));
        assert_eq!(e.selection, Some((0, 3)));
        e.handle_key(&ctrl("u"));
        assert_eq!(e.text, "");
        assert_eq!(e.selection, None);
    }

    #[test]
    fn backspace_deletes_a_selection_whole_or_one_char() {
        let mut e = LineEdit::with_text("abc");
        e.select_all();
        e.handle_key(&named(NamedKey::Backspace));
        assert_eq!(e.text, "");

        let mut e = LineEdit::with_text("abc");
        e.handle_key(&named(NamedKey::Backspace));
        assert_eq!(e.text, "ab");
    }

    /// Arrows collapse to the edge they move toward rather than stepping from
    /// the caret — otherwise Left after a select-all lands in the wrong place.
    #[test]
    fn arrows_collapse_a_selection_to_its_edge() {
        let mut e = LineEdit::with_text("abc");
        e.select_all();
        e.handle_key(&named(NamedKey::ArrowLeft));
        assert_eq!((e.cursor, e.selection), (0, None));

        e.select_all();
        e.handle_key(&named(NamedKey::ArrowRight));
        assert_eq!((e.cursor, e.selection), (3, None));
    }

    #[test]
    fn enter_and_escape_are_reported_not_swallowed() {
        let mut e = LineEdit::with_text("x");
        assert_eq!(e.handle_key(&named(NamedKey::Enter)), EditOutcome::Submit);
        assert_eq!(e.handle_key(&named(NamedKey::Escape)), EditOutcome::Cancel);
    }

    /// Multi-byte text must not be split mid-character.
    #[test]
    fn caret_moves_by_character_not_byte() {
        let mut e = LineEdit::with_text("é1");
        e.handle_key(&named(NamedKey::Home));
        e.handle_key(&named(NamedKey::ArrowRight));
        assert_eq!(e.cursor, 2, "é is two bytes");
        e.handle_key(&named(NamedKey::Backspace));
        assert_eq!(e.text, "1");
    }

    #[test]
    fn a_drag_selects_from_the_press_to_the_pointer_either_way() {
        let mut e = LineEdit::with_text("hello world");
        e.press(2, false);
        assert!(e.dragging());
        assert_eq!((e.cursor, e.selection), (2, None), "a press alone selects nothing");
        assert!(e.drag_to(7));
        assert_eq!((e.cursor, e.selection), (7, Some((2, 7))));
        assert!(!e.drag_to(7), "no move, no repaint");
        // Back past the press: the selection flips to the other side of it.
        e.drag_to(0);
        assert_eq!((e.cursor, e.selection), (0, Some((0, 2))));
        // Back onto the press point: nothing selected, caret there.
        e.drag_to(2);
        assert_eq!((e.cursor, e.selection), (2, None));
        e.drag_to(11);
        e.release();
        assert!(!e.dragging());
        assert_eq!(e.selection, Some((2, 11)), "release keeps what was dragged");
        assert!(!e.drag_to(4), "motion after release is not a drag");
        assert_eq!(e.selection, Some((2, 11)));
    }

    #[test]
    fn shift_click_extends_from_the_far_end() {
        let mut e = LineEdit::with_text("hello world");
        e.press(3, false);
        e.release();
        e.press(8, true);
        assert_eq!((e.cursor, e.selection), (8, Some((3, 8))));
        e.release();
        // Shift+click on the other side of the anchor keeps the anchor.
        e.press(1, true);
        assert_eq!((e.cursor, e.selection), (1, Some((1, 3))));
        e.release();
        // After a select-all the caret is at the end, so the start is kept.
        e.select_all();
        e.press(5, true);
        assert_eq!(e.selection, Some((0, 5)));
    }

    #[test]
    fn a_dragged_selection_is_edited_like_any_other() {
        let mut e = LineEdit::with_text("hello world");
        e.press(0, false);
        e.drag_to(6);
        e.release();
        typed(&mut e, "big ");
        assert_eq!(e.text, "big world");
        assert_eq!(e.cursor, 4);
    }

    /// The app's offset is clamped into the text and never splits a char.
    #[test]
    fn pointer_offsets_land_on_char_boundaries() {
        let mut e = LineEdit::with_text("aé");
        e.press(2, false); // inside é's two bytes
        assert_eq!(e.cursor, 1);
        e.drag_to(99);
        assert_eq!((e.cursor, e.selection), (3, Some((1, 3))));
    }

    /// Each bullet is three bytes of display and one char of text.
    #[test]
    fn a_masked_field_maps_bullets_back_to_the_text() {
        let mut e = LineEdit::masked();
        typed(&mut e, "pé!");
        let bullet = '\u{2022}'.len_utf8();
        assert_eq!(e.text_index(0), 0);
        assert_eq!(e.text_index(bullet), 1);
        assert_eq!(e.text_index(2 * bullet), 3, "past é's two bytes");
        assert_eq!(e.text_index(3 * bullet), 4);
        assert_eq!(e.text_index(99), 4);
        assert_eq!(LineEdit::with_text("abc").text_index(2), 2, "unmasked is the identity");
        // And back: every text boundary round-trips through the bullets.
        for at in [0, 1, 3, 4] {
            assert_eq!(e.text_index(e.display_index(at)), at, "{at}");
        }
        assert_eq!(e.display_index(3), 2 * bullet);
        assert_eq!(LineEdit::with_text("abc").display_index(2), 2);
    }

    /// A password must not leave through a chord the user may not have meant.
    #[test]
    fn a_masked_field_hides_its_text_and_refuses_copy() {
        let mut e = LineEdit::masked();
        typed(&mut e, "hunter2");
        assert_eq!(e.display(), "•".repeat(7));
        assert_ne!(e.display(), e.text);
        e.select_all();
        assert_eq!(e.handle_key(&ctrl("c")), EditOutcome::Ignored);
        assert_eq!(e.handle_key(&ctrl("x")), EditOutcome::Ignored);
        assert_eq!(e.text, "hunter2", "cut must not have removed it");
    }
}
