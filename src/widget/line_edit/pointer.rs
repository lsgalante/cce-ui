//! The pointer: presses, drags and double- and triple-clicks, and the word boundaries they and
//! Ctrl+arrows use.
//!
//! The field does not know where its characters are drawn; the app does.
//! So the pointer arrives as a byte offset into `text` that the app has
//! already hit-tested (nearest char boundary to the pointer's x on the
//! same shaped run it drew), and for a masked field converted with
//! `text_index`, since what was drawn there is bullets.

use super::*;

impl LineEdit {
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

    pub(super) fn press_at(&mut self, at: usize, extend: bool, now: Instant) {
        // The caret moves: a composition is left where it was, cancelled.
        self.drop_composition();
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
}
