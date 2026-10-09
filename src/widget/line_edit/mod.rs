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
//! **An input method's composition** (see `crate::ime`) is SHOWN, never
//! held: `text` keeps what was typed, and [`LineEdit::display`] splices the
//! composition in at the caret, with [`LineEdit::display_index`] /
//! [`LineEdit::text_index`] mapping across it (the caret lands where the
//! input method has its cursor; a press inside the composition is the caret)
//! and [`LineEdit::composition_range`] the span to underline. The app calls
//! [`LineEdit::sync_ime`] each frame the field has the keyboard, before
//! drawing it, reports the caret it draws with `ime::report_caret` (that is
//! also what tells the shell text input is wanted), and calls
//! [`LineEdit::drop_composition`] when the field loses the keyboard. The
//! commit arrives as typed text through [`LineEdit::handle_key`], which
//! takes no other key while a composition is up — they are the input
//! method's.
//!
//! Indices are **byte** offsets into `text`, always on a char boundary
//! ([`prev_boundary`] / [`next_boundary`] step them), which is what slicing
//! and shaping want. [`TextEditorState`](super::TextEditorState), the model
//! behind `TextBox`, counts chars and leaves the keymap to its widget;
//! the two are not interchangeable.
//!
//! [`TextBox`]: super::TextBox
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the model, its outcomes, char boundaries, construction, the display, selection helpers |
//! | `composition` | the composition: taking it up, dropping it, and the index maps across it and a mask |
//! | `pointer` | presses, drags and multi-clicks, and the word boundaries they and Ctrl+arrows use |
//! | `keys` | the keymap, undo and redo with their runs, and the text as a reader sees it |

mod composition;
mod keys;
mod pointer;
#[cfg(test)]
mod tests;

use std::time::Duration;
use web_time::Instant;

use crate::history::History;
use crate::ime::{self, Preedit};
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
    /// The input method's composition, shown at the caret (see the module
    /// doc), and the `ime::generation` it was taken at.
    composition: Option<Preedit>,
    ime_seen: u64,
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

    /// What to draw: the text, with an input method's composition at the
    /// caret. Never returns the password itself.
    pub fn display(&self) -> String {
        let mut shown = if self.masked {
            "\u{2022}".repeat(self.text.chars().count())
        } else {
            self.text.clone()
        };
        if let Some(p) = &self.composition {
            shown.insert_str(self.display_index_held(self.cursor), &self.shown(p));
        }
        shown
    }

    /// A composition as drawn: its text, or a bullet a char when masked.
    fn shown(&self, p: &Preedit) -> String {
        if self.masked {
            "\u{2022}".repeat(p.text.chars().count())
        } else {
            p.text.clone()
        }
    }

    pub fn select_all(&mut self) {
        self.cursor = self.text.len();
        self.selection = (self.cursor > 0).then_some((0, self.cursor));
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
}
