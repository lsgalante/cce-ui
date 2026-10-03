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

use std::time::{Duration, Instant};

use crate::history::History;
use crate::widget::{ElementState, Key, KeyEvent, NamedKey};

/// How close two presses at the same offset must be to count as one double
/// (or triple) click — `DocEditor`'s figure, so the two editors agree.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

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
    /// While a pointer button is held after [`LineEdit::press`]: what the
    /// press selected and how a drag grows it. `None` when no drag is in
    /// progress.
    drag: Option<Drag>,
    /// The last press — when, where, and how many presses in a row landed
    /// there — for telling a double or triple click from two clicks.
    clicks: Option<(Instant, usize, u8)>,
    /// Undo and redo for what [`LineEdit::handle_key`] changes. Never kept
    /// for a masked field (see [`LineEdit::undo`]).
    history: History<Snapshot>,
}

/// What an undo step restores: the text, and where the caret and selection
/// were, so undoing a deletion puts the caret back where it was.
#[derive(Clone, Debug)]
struct Snapshot {
    text: String,
    cursor: usize,
    selection: Option<(usize, usize)>,
}

/// A press being dragged: the span the press selected (empty for a single
/// click, the word for a double, everything for a triple) and the unit the
/// selection grows by as the pointer moves.
#[derive(Clone, Copy, Debug)]
struct Drag {
    lo: usize,
    hi: usize,
    unit: Unit,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Unit {
    Char,
    Word,
    All,
}

/// A word character for double-click and Ctrl+arrow purposes: letters and
/// digits, and `_` (as `DocEditor` counts them).
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
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
    ///
    /// Presses at the same offset within [`DOUBLE_CLICK`] of each other count
    /// up: the second selects the word there (see [`LineEdit::word_at`]) and
    /// a drag from it grows by whole words; the third selects everything —
    /// the field is one line; a fourth starts over. A masked field selects
    /// everything on the second press too: picking out a "word" would show
    /// where the password's spaces and symbols are. Shift+click never counts.
    pub fn press(&mut self, at: usize, extend: bool) {
        self.press_at(at, extend, Instant::now());
    }

    fn press_at(&mut self, at: usize, extend: bool, now: Instant) {
        // A click moves the caret: typing after it is a new undo step.
        self.history.break_group();
        let at = self.boundary(at);
        let count = match self.clicks {
            Some((t, p, n)) if !extend && p == at && now.duration_since(t) < DOUBLE_CLICK => n % 3 + 1,
            _ => 1,
        };
        self.clicks = Some((now, at, count));
        let unit = match count {
            1 => Unit::Char,
            2 if !self.masked => Unit::Word,
            _ => Unit::All,
        };
        let (lo, hi) = match unit {
            Unit::Char => {
                let anchor = if extend { self.anchor() } else { at };
                self.select_between(anchor, at);
                (anchor, anchor)
            }
            Unit::Word => {
                let (a, b) = self.word_at(at);
                self.select_between(a, b);
                (a, b)
            }
            Unit::All => {
                self.select_all();
                (0, self.text.len())
            }
        };
        self.drag = Some(Drag { lo, hi, unit });
    }

    /// The pointer moved to byte `at` with the button still held: the
    /// selection is now everything between the press and here, and the caret
    /// is here — or, after a double-click, everything from the pressed word
    /// to the whole word here. True when that changed anything (a repaint is
    /// due); false, and nothing happens, when no press is in progress.
    pub fn drag_to(&mut self, at: usize) -> bool {
        let Some(drag) = self.drag else {
            return false;
        };
        let before = (self.cursor, self.selection);
        let at = self.boundary(at);
        match drag.unit {
            Unit::Char => self.select_between(drag.lo, at),
            Unit::Word if at < drag.lo => self.select_between(drag.hi, self.word_at(at).0),
            Unit::Word if at > drag.hi => self.select_between(drag.lo, self.word_at(at).1),
            Unit::Word => self.select_between(drag.lo, drag.hi),
            Unit::All => {}
        }
        (self.cursor, self.selection) != before
    }

    /// The button came up: the drag is over, and what it selected stays
    /// selected.
    pub fn release(&mut self) {
        self.drag = None;
    }

    /// Whether a press is being dragged — the app routes pointer motion to
    /// [`LineEdit::drag_to`] while this holds, wherever the pointer is.
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// The "word" a double-click at byte `at` selects: the run of characters
    /// of one kind around it — letters and digits (with `_`, as `DocEditor`
    /// counts them), spaces, or anything else. So in a URL `example` is a
    /// word and so is `://`. On the edge of a word the word wins, so a click
    /// just past a word's last letter still selects the word.
    /// Where Ctrl+Left goes from byte `at`: back over anything that is not
    /// a word character, then to the start of the word before it — the same
    /// word characters as [`LineEdit::word_at`] (letters, digits, `_`). On a
    /// masked field, straight to the start: stopping at word edges would show
    /// where a password's spaces and symbols are.
    pub fn word_left(&self, at: usize) -> usize {
        if self.masked {
            return 0;
        }
        let s = &self.text;
        let mut i = self.boundary(at);
        for want_word in [false, true] {
            while let Some(c) = s[..i].chars().next_back() {
                if is_word_char(c) != want_word {
                    break;
                }
                i -= c.len_utf8();
            }
        }
        i
    }

    /// Where Ctrl+Right goes from byte `at`: forward over anything that is
    /// not a word character, then to the end of the word after it. On a
    /// masked field, straight to the end (see [`LineEdit::word_left`]).
    pub fn word_right(&self, at: usize) -> usize {
        if self.masked {
            return self.text.len();
        }
        let s = &self.text;
        let mut i = self.boundary(at);
        for want_word in [false, true] {
            while let Some(c) = s[i..].chars().next() {
                if is_word_char(c) != want_word {
                    break;
                }
                i += c.len_utf8();
            }
        }
        i
    }

    pub fn word_at(&self, at: usize) -> (usize, usize) {
        let at = self.boundary(at);
        let s = &self.text;
        let kind = |c: char| {
            if is_word_char(c) {
                0
            } else if c.is_whitespace() {
                1
            } else {
                2
            }
        };
        let after = s[at..].chars().next();
        let before = s[..at].chars().next_back();
        let k = match (before.map(kind), after.map(kind)) {
            (_, Some(0)) | (Some(0), _) => 0,
            (_, Some(k)) | (Some(k), None) => k,
            (None, None) => return (at, at),
        };
        let mut a = at;
        while let Some(c) = s[..a].chars().next_back() {
            if kind(c) != k {
                break;
            }
            a -= c.len_utf8();
        }
        let mut b = at;
        while let Some(c) = s[b..].chars().next() {
            if kind(c) != k {
                break;
            }
            b += c.len_utf8();
        }
        (a, b)
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

    /// One key. Edits are recorded for [`LineEdit::undo`] the way `TextBox`
    /// records them: a typed run is one step and whitespace starts the next,
    /// so undo walks back a word at a time; a run of Backspace (or Delete)
    /// is one step; replacing a selection, a paste, a cut, a Ctrl word-delete
    /// and Ctrl+U are each their own; and a caret move — by key or click —
    /// ends a run.
    ///
    /// Undo and redo themselves are not keys here: the window runner routes
    /// the DE's `undo` / `redo` chords (`input.kdl`, Ctrl+Z / Ctrl+Shift+Z
    /// by default) to the app, which calls [`LineEdit::undo`] /
    /// [`LineEdit::redo`] on the field that has focus.
    pub fn handle_key(&mut self, event: &KeyEvent) -> EditOutcome {
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

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { text: self.text.clone(), cursor: self.cursor, selection: self.selection }
    }

    fn restore(&mut self, s: Snapshot) {
        self.text = s.text;
        self.cursor = s.cursor;
        self.selection = s.selection;
        self.drag = None;
        self.clicks = None;
    }

    fn apply_key(&mut self, event: &KeyEvent) -> EditOutcome {
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

    fn shifted(n: NamedKey) -> KeyEvent {
        KeyEvent { shift: true, ..named(n) }
    }

    #[test]
    fn shift_arrows_grow_and_shrink_the_selection_from_its_anchor() {
        let mut e = LineEdit::with_text("hello");
        e.handle_key(&named(NamedKey::Home));
        e.handle_key(&shifted(NamedKey::ArrowRight));
        e.handle_key(&shifted(NamedKey::ArrowRight));
        assert_eq!((e.cursor, e.selection), (2, Some((0, 2))));
        e.handle_key(&shifted(NamedKey::ArrowLeft));
        assert_eq!((e.cursor, e.selection), (1, Some((0, 1))), "shrinks back toward the anchor");
        e.handle_key(&shifted(NamedKey::ArrowLeft));
        assert_eq!((e.cursor, e.selection), (0, None), "back on the anchor: nothing selected");
        // Past the anchor it flips to the other side.
        let mut e = LineEdit::with_text("hello");
        e.handle_key(&named(NamedKey::ArrowLeft)); // caret 4
        e.handle_key(&shifted(NamedKey::ArrowRight));
        e.handle_key(&shifted(NamedKey::ArrowLeft));
        e.handle_key(&shifted(NamedKey::ArrowLeft));
        assert_eq!((e.cursor, e.selection), (3, Some((3, 4))));
        // At the ends it stops.
        e.handle_key(&shifted(NamedKey::End));
        e.handle_key(&shifted(NamedKey::ArrowRight));
        assert_eq!((e.cursor, e.selection), (5, Some((4, 5))));
    }

    #[test]
    fn shift_home_and_end_select_to_the_ends() {
        let mut e = LineEdit::with_text("hello world");
        e.press(6, false);
        e.release();
        e.handle_key(&shifted(NamedKey::End));
        assert_eq!((e.cursor, e.selection), (11, Some((6, 11))));
        e.handle_key(&shifted(NamedKey::Home));
        assert_eq!((e.cursor, e.selection), (0, Some((0, 6))), "the anchor stays where the caret was");
        // A select-all keeps its start; Shift+Left then trims its end.
        e.select_all();
        e.handle_key(&shifted(NamedKey::ArrowLeft));
        assert_eq!(e.selection, Some((0, 10)));
        // A plain arrow still collapses to the edge it points at.
        e.handle_key(&named(NamedKey::ArrowLeft));
        assert_eq!((e.cursor, e.selection), (0, None));
    }

    /// Multi-byte text: Shift+arrow steps a character, not a byte.
    #[test]
    fn shift_arrows_step_whole_characters() {
        let mut e = LineEdit::with_text("aé");
        e.handle_key(&shifted(NamedKey::ArrowLeft));
        assert_eq!((e.cursor, e.selection), (1, Some((1, 3))));
    }

    fn ctrl_key(n: NamedKey, shift: bool) -> KeyEvent {
        KeyEvent { ctrl: true, shift, ..named(n) }
    }

    #[test]
    fn ctrl_arrows_jump_by_word() {
        let mut e = LineEdit::with_text("https://example.com/a_b  c");
        e.handle_key(&named(NamedKey::Home));
        let mut stops = Vec::new();
        for _ in 0..6 {
            e.handle_key(&ctrl_key(NamedKey::ArrowRight, false));
            stops.push(e.cursor);
        }
        assert_eq!(stops, [5, 15, 19, 23, 26, 26], "ends of https, example, com, a_b, c; then stays");
        let mut back = Vec::new();
        for _ in 0..6 {
            e.handle_key(&ctrl_key(NamedKey::ArrowLeft, false));
            back.push(e.cursor);
        }
        assert_eq!(back, [25, 20, 16, 8, 0, 0], "starts of c, a_b, com, example, https; then stays");
        // From inside a word: to that word's own edge.
        e.cursor = 11;
        e.handle_key(&ctrl_key(NamedKey::ArrowLeft, false));
        assert_eq!(e.cursor, 8);
    }

    #[test]
    fn ctrl_shift_arrows_select_by_word() {
        let mut e = LineEdit::with_text("one two three");
        e.handle_key(&named(NamedKey::Home));
        e.handle_key(&ctrl_key(NamedKey::ArrowRight, true));
        e.handle_key(&ctrl_key(NamedKey::ArrowRight, true));
        assert_eq!((e.cursor, e.selection), (7, Some((0, 7))));
        e.handle_key(&ctrl_key(NamedKey::ArrowLeft, true));
        assert_eq!((e.cursor, e.selection), (4, Some((0, 4))), "shrinks a word back");
        // A plain Ctrl+arrow leaves from the selection's edge and drops it.
        e.handle_key(&ctrl_key(NamedKey::ArrowRight, false));
        assert_eq!((e.cursor, e.selection), (7, None));
        e.select_all();
        e.handle_key(&ctrl_key(NamedKey::ArrowLeft, false));
        assert_eq!((e.cursor, e.selection), (0, None));
    }

    #[test]
    fn ctrl_arrows_in_a_password_go_to_the_ends() {
        let mut e = LineEdit::masked();
        typed(&mut e, "pass word");
        e.handle_key(&ctrl_key(NamedKey::ArrowLeft, false));
        assert_eq!(e.cursor, 0, "no stop at the space");
        e.handle_key(&ctrl_key(NamedKey::ArrowRight, true));
        assert_eq!(e.selection, Some((0, 9)));
    }

    #[test]
    fn ctrl_backspace_and_delete_take_a_word() {
        let mut e = LineEdit::with_text("https://example.com/drag");
        e.handle_key(&ctrl_key(NamedKey::Backspace, false));
        assert_eq!((e.text.as_str(), e.cursor), ("https://example.com/", 20));
        e.handle_key(&ctrl_key(NamedKey::Backspace, false));
        assert_eq!((e.text.as_str(), e.cursor), ("https://example.", 16), "the / and com go together");
        // From inside a word: just its first half.
        let mut e = LineEdit::with_text("hello world");
        e.cursor = 8;
        e.handle_key(&ctrl_key(NamedKey::Backspace, false));
        assert_eq!((e.text.as_str(), e.cursor), ("hello rld", 6));
        // Ctrl+Delete: forward to the end of the word.
        e.cursor = 0;
        e.handle_key(&ctrl_key(NamedKey::Delete, false));
        assert_eq!((e.text.as_str(), e.cursor), (" rld", 0));
        // At the ends nothing happens.
        let mut e = LineEdit::with_text("abc");
        e.handle_key(&ctrl_key(NamedKey::Delete, false));
        assert_eq!(e.text, "abc");
        e.cursor = 0;
        e.handle_key(&ctrl_key(NamedKey::Backspace, false));
        assert_eq!(e.text, "abc");
    }

    #[test]
    fn ctrl_backspace_takes_a_selection_not_a_word() {
        let mut e = LineEdit::with_text("one two three");
        e.press(4, false);
        e.drag_to(6);
        e.release();
        e.handle_key(&ctrl_key(NamedKey::Backspace, false));
        assert_eq!((e.text.as_str(), e.cursor), ("one o three", 4));
    }

    #[test]
    fn ctrl_backspace_in_a_password_clears_to_the_start() {
        let mut e = LineEdit::masked();
        typed(&mut e, "pass word");
        e.handle_key(&named(NamedKey::ArrowLeft));
        e.handle_key(&named(NamedKey::ArrowLeft)); // before "rd"
        e.handle_key(&ctrl_key(NamedKey::Backspace, false));
        assert_eq!((e.text.as_str(), e.cursor), ("rd", 0), "no stop at the space");
    }

    #[test]
    fn undo_walks_back_a_word_at_a_time_and_redo_forward() {
        let mut e = LineEdit::default();
        typed(&mut e, "hello world");
        assert!(e.can_undo());
        assert!(e.undo());
        assert_eq!((e.text.as_str(), e.cursor), ("hello ", 6), "the word after the space");
        assert!(e.undo());
        assert_eq!(e.text, "hello", "then the space");
        assert!(e.undo());
        assert_eq!((e.text.as_str(), e.cursor), ("", 0));
        assert!(!e.undo(), "nothing left");
        assert!(e.redo());
        assert!(e.redo());
        assert!(e.redo());
        assert_eq!((e.text.as_str(), e.cursor), ("hello world", 11));
        assert!(!e.redo());
    }

    #[test]
    fn a_caret_move_or_click_splits_a_run() {
        let mut e = LineEdit::default();
        typed(&mut e, "abc");
        e.handle_key(&named(NamedKey::ArrowLeft));
        typed(&mut e, "X");
        e.undo();
        assert_eq!((e.text.as_str(), e.cursor), ("abc", 2), "X alone, caret back before c");
        let mut e = LineEdit::default();
        typed(&mut e, "abc");
        e.press(1, false);
        e.release();
        typed(&mut e, "X");
        e.undo();
        assert_eq!(e.text, "abc");
    }

    #[test]
    fn deleting_runs_and_replacements_are_their_own_steps() {
        let mut e = LineEdit::with_text("hello world");
        for _ in 0..3 {
            e.handle_key(&named(NamedKey::Backspace));
        }
        assert_eq!(e.text, "hello wo");
        e.undo();
        assert_eq!(e.text, "hello world", "three Backspaces, one step");
        // Typing over a selection: the replacement is one step, then the
        // rest of the typed run another.
        e.select_all();
        typed(&mut e, "xyz");
        e.undo();
        assert_eq!(e.text, "x");
        e.undo();
        assert_eq!((e.text.as_str(), e.selection), ("hello world", Some((0, 11))), "the selection comes back too");
        // Ctrl word-delete is a step of its own.
        e.selection = None;
        e.handle_key(&ctrl_key(NamedKey::Backspace, false));
        e.handle_key(&ctrl_key(NamedKey::Backspace, false));
        assert_eq!(e.text, "");
        e.undo();
        assert_eq!(e.text, "hello ");
    }

    #[test]
    fn a_new_edit_after_undo_drops_the_redo() {
        let mut e = LineEdit::default();
        typed(&mut e, "one two");
        e.undo();
        typed(&mut e, "six");
        assert!(!e.can_redo());
        assert!(!e.redo());
        assert_eq!(e.text, "one six");
    }

    #[test]
    fn a_masked_field_keeps_no_history() {
        let mut e = LineEdit::masked();
        typed(&mut e, "hunter2");
        assert!(!e.can_undo());
        assert!(!e.undo());
        assert_eq!(e.text, "hunter2");
    }

    fn ms(t0: Instant, n: u64) -> Instant {
        t0 + Duration::from_millis(n)
    }

    fn click(e: &mut LineEdit, at: usize, now: Instant) {
        e.press_at(at, false, now);
        e.release();
    }

    #[test]
    fn a_double_click_selects_the_word_and_a_triple_everything() {
        let t0 = Instant::now();
        let mut e = LineEdit::with_text("https://example.com/drag");
        click(&mut e, 10, t0);
        assert_eq!(e.selection, None);
        click(&mut e, 10, ms(t0, 150));
        assert_eq!((e.selection, e.cursor), (Some((8, 15)), 15), "example");
        click(&mut e, 10, ms(t0, 300));
        assert_eq!(e.selection, Some((0, 24)), "the third click takes the line");
        click(&mut e, 10, ms(t0, 450));
        assert_eq!((e.selection, e.cursor), (None, 10), "the fourth starts over");
    }

    #[test]
    fn slow_or_moved_clicks_are_two_clicks() {
        let t0 = Instant::now();
        let mut e = LineEdit::with_text("hello world");
        click(&mut e, 2, t0);
        click(&mut e, 2, ms(t0, 500));
        assert_eq!(e.selection, None, "too slow");
        click(&mut e, 3, ms(t0, 600));
        assert_eq!(e.selection, None, "somewhere else");
        // Shift+click is never half of a double-click.
        let mut e = LineEdit::with_text("hello world");
        click(&mut e, 2, t0);
        e.press_at(2, true, ms(t0, 100));
        assert_eq!(e.selection, None);
        // Nor is a click after typing.
        let mut e = LineEdit::with_text("hello world");
        click(&mut e, 11, t0);
        typed(&mut e, "!");
        e.handle_key(&named(NamedKey::Backspace));
        click(&mut e, 11, ms(t0, 100));
        assert_eq!(e.selection, None);
    }

    #[test]
    fn a_word_is_a_run_of_one_kind() {
        let e = LineEdit::with_text("https://example.com/a_b  c");
        assert_eq!(e.word_at(1), (0, 5), "https");
        assert_eq!(e.word_at(6), (5, 8), "the :// between words");
        assert_eq!(e.word_at(15), (8, 15), "just past a word's end is still the word");
        assert_eq!(e.word_at(21), (20, 23), "_ joins a word");
        assert_eq!(e.word_at(24), (23, 25), "a run of spaces");
        assert_eq!(e.word_at(26), (25, 26), "the end of the text");
        assert_eq!(LineEdit::default().word_at(0), (0, 0));
        // Multi-byte letters are letters.
        assert_eq!(LineEdit::with_text("é1 x").word_at(1), (0, 3));
    }

    #[test]
    fn dragging_a_double_click_grows_by_words() {
        let t0 = Instant::now();
        let mut e = LineEdit::with_text("one two three four");
        click(&mut e, 5, t0);
        e.press_at(5, false, ms(t0, 100));
        assert_eq!(e.selection, Some((4, 7)), "two");
        e.drag_to(10); // into "three"
        assert_eq!((e.selection, e.cursor), (Some((4, 13)), 13));
        e.drag_to(6); // back inside "two": just the word again
        assert_eq!(e.selection, Some((4, 7)));
        e.drag_to(1); // into "one": from its start to the end of "two"
        assert_eq!((e.selection, e.cursor), (Some((0, 7)), 0));
        e.release();
        assert!(!e.drag_to(16));
    }

    #[test]
    fn a_masked_double_click_selects_it_all() {
        let t0 = Instant::now();
        let mut e = LineEdit::masked();
        typed(&mut e, "pass word");
        click(&mut e, 2, t0);
        click(&mut e, 2, ms(t0, 100));
        assert_eq!(e.selection, Some((0, 9)), "no word boundaries in a password");
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
