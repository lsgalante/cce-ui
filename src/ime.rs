//! Input-method composition, between the text widget the user types into and
//! the shell whose window system runs the input method (an IME: Japanese,
//! Chinese, Korean, an accent picker, an emoji panel).
//!
//! Three things cross, and only one of them is new to the toolkit:
//!
//! - **The commit** — text the input method has finished composing — is
//!   delivered as TYPED text (`Driver::commit_text`: a key press carrying it,
//!   then its release), so every widget that inserts what a key types takes
//!   it with no change: `TextBox`, `LineEdit`, the `DocEditor`, an app's own
//!   field.
//! - **The composition** (the preedit: text still being composed, with the
//!   input method's cursor in it) is shared here: a shell sets it
//!   ([`set_preedit`], through `Driver::preedit`), and the widget that is
//!   editing shows it at its caret — `TextBox` splices it into its buffer as
//!   a provisional run, underlined, which is never committed, recorded in
//!   its history or reported as a change.
//! - **The caret** goes the other way: a widget that is editing reports its
//!   caret's rect each frame it paints ([`report_caret`]), so the shell can
//!   place the input method's candidate window under it, and knows text
//!   input is wanted at all ([`caret`] is `None` when nothing is editing;
//!   the macOS shell then keeps keys away from the input method, so an IME
//!   left on does not swallow an app's single-key shortcuts).
//!
//! A widget that drops a composition it was showing — it stopped editing
//! mid-composition — asks the shell to cancel it in the input method too
//! ([`request_reset`], drained by the shell with [`take_reset`]).
//!
//! The shells that feed it: the Wayland shell through `text-input-v3`
//! (`backend::text_input`), the browser's through a hidden textarea, the
//! AppKit shell as an `NSTextInputClient`.
//!
//! Per thread, as the context menu is: a window's widgets, its shell and its
//! frame are one thread's.

use std::cell::RefCell;

/// Text the input method is still composing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Preedit {
    pub text: String,
    /// The input method's cursor (or selected clause) in `text`, as a byte
    /// range; `None` to show no caret in it.
    pub cursor: Option<(usize, usize)>,
}

impl Preedit {
    pub fn new(text: impl Into<String>, cursor: Option<(usize, usize)>) -> Self {
        Self { text: text.into(), cursor }
    }

    /// The caret's position in `text`, in chars: the end of the input
    /// method's cursor range, or the end of the text when it has none. A
    /// byte offset off a char boundary or past the end is clamped back.
    pub fn caret_chars(&self) -> usize {
        let end = self.cursor.map_or(self.text.len(), |(_, e)| e).min(self.text.len());
        let mut end = end;
        while !self.text.is_char_boundary(end) {
            end -= 1;
        }
        self.text[..end].chars().count()
    }
}

#[derive(Default)]
pub(crate) struct State {
    preedit: Option<Preedit>,
    /// Moves on every change of `preedit`, so a widget can tell it has
    /// applied the current one.
    generation: u64,
    /// The caret the last built frame reported, and the one this frame has
    /// so far.
    caret: Option<[f32; 4]>,
    reported: Option<[f32; 4]>,
    reset: bool,
    /// A press landed since the shell last asked ([`note_press`]).
    pressed: bool,
}

/// The current window's composition and caret (`crate::window_state`).
fn state<R>(f: impl FnOnce(&RefCell<State>) -> R) -> R {
    crate::window_state::with(|w| f(&w.ime))
}

/// The composition changed: `None` (or empty text) when there is none. A
/// shell's, through `Driver::preedit`.
pub fn set_preedit(preedit: Option<Preedit>) {
    let preedit = preedit.filter(|p| !p.text.is_empty());
    state(|s| {
        let mut s = s.borrow_mut();
        if s.preedit != preedit {
            s.preedit = preedit;
            s.generation += 1;
        }
    });
}

/// The composition, if the input method is composing.
pub fn preedit() -> Option<Preedit> {
    state(|s| s.borrow().preedit.clone())
}

/// Moves whenever the composition does.
pub fn generation() -> u64 {
    state(|s| s.borrow().generation)
}

/// A frame is being built: carets are reported afresh.
pub fn begin_frame() {
    state(|s| s.borrow_mut().reported = None);
}

/// The frame is built: what was reported is the caret.
pub fn end_frame() {
    state(|s| {
        let mut s = s.borrow_mut();
        s.caret = s.reported;
    });
}

/// A widget editing text has its caret at `x, y` (`w` x `h`), in the
/// window's logical px. Called as it paints.
pub fn report_caret(x: f32, y: f32, w: f32, h: f32) {
    state(|s| s.borrow_mut().reported = Some([x, y, w, h]));
}

/// Where the editing widget's caret was in the last built frame, or `None`
/// when no widget is editing text.
pub fn caret() -> Option<[f32; 4]> {
    state(|s| s.borrow().caret)
}

/// A widget dropped a composition it was showing: the input method should
/// cancel it too. Clears the composition.
pub fn request_reset() {
    set_preedit(None);
    state(|s| s.borrow_mut().reset = true);
}

/// What this frame has reported so far, replaced by `caret`. For
/// `text_input::capture`, which reads what one painting reported.
pub(crate) fn swap_reported(caret: Option<[f32; 4]>) -> Option<[f32; 4]> {
    state(|s| std::mem::replace(&mut s.borrow_mut().reported, caret))
}

/// A pointer or touch press reached the window. A field that is still
/// editing in the next frame is announced to the shell again
/// ([`take_press`]): the compositor raises its on-screen keyboard on an
/// announcement that follows a touch, and a field that was already open —
/// a focused terminal, a text box still editing — would otherwise say
/// nothing new when tapped. True when a field is editing, so the caller
/// builds that frame.
pub fn note_press() -> bool {
    state(|s| {
        let mut s = s.borrow_mut();
        s.pressed = true;
        s.caret.is_some()
    })
}

/// Whether a press landed since the last call. A shell's.
pub fn take_press() -> bool {
    state(|s| std::mem::replace(&mut s.borrow_mut().pressed, false))
}

/// Whether a reset was asked for since the last call. A shell's.
pub fn take_reset() -> bool {
    state(|s| std::mem::replace(&mut s.borrow_mut().reset, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_composition_moves_the_generation_only_when_it_changes() {
        let g0 = generation();
        set_preedit(Some(Preedit::new("に", None)));
        let g1 = generation();
        assert!(g1 > g0);
        set_preedit(Some(Preedit::new("に", None)));
        assert_eq!(generation(), g1);
        // Empty text is no composition.
        set_preedit(Some(Preedit::new("", None)));
        assert_eq!(preedit(), None);
        assert!(generation() > g1);
    }

    #[test]
    fn the_caret_is_the_end_of_the_cursor_range_in_chars() {
        assert_eq!(Preedit::new("にほん", None).caret_chars(), 3);
        // "に" is three bytes.
        assert_eq!(Preedit::new("にほん", Some((0, 3))).caret_chars(), 1);
        // Off a boundary, clamped back.
        assert_eq!(Preedit::new("にほん", Some((0, 4))).caret_chars(), 1);
        assert_eq!(Preedit::new("ab", Some((0, 9))).caret_chars(), 2);
    }

    #[test]
    fn the_caret_is_what_the_last_built_frame_reported() {
        begin_frame();
        report_caret(10.0, 20.0, 1.5, 16.0);
        assert_eq!(caret(), None, "not until the frame is built");
        end_frame();
        assert_eq!(caret(), Some([10.0, 20.0, 1.5, 16.0]));
        begin_frame();
        end_frame();
        assert_eq!(caret(), None, "a frame with nothing editing");
    }

    #[test]
    fn a_press_wants_a_frame_only_while_a_field_is_editing() {
        begin_frame();
        end_frame();
        assert!(!note_press(), "nothing editing: no frame owed");
        assert!(take_press(), "but the press is still noted");
        assert!(!take_press(), "once");
        begin_frame();
        report_caret(1.0, 2.0, 3.0, 4.0);
        end_frame();
        assert!(note_press());
        assert!(take_press());
    }

    #[test]
    fn a_reset_clears_the_composition_and_is_taken_once() {
        set_preedit(Some(Preedit::new("ka", None)));
        request_reset();
        assert_eq!(preedit(), None);
        assert!(take_reset());
        assert!(!take_reset());
    }
}
