//! "A text field is being edited": the announcement behind text-input-v3.
//!
//! cce-ui types through `wl_keyboard`; the compositor still needs to know when
//! a field is open for typing, because that is what raises the on-screen
//! keyboard after a touch (cce-compositor's `osk.rs`) and what activates an
//! input method. A widget says so by calling [`claim`] from its paint, every
//! frame it is editing; the runner reads the frame's claim once the display
//! list is built and enables the seat's text input, or disables it when no
//! widget claimed (`backend::text_input`).
//!
//! A claim per frame rather than an enable/disable pair is deliberate: a text
//! box leaves editing by Enter, Escape, a click elsewhere, a focus change or
//! being dropped with its page, and a pair would have to be closed on every
//! one of those paths. A widget that stops painting has stopped claiming.
//!
//! `TextBox` claims on its own. An app drawing its own text surface (an
//! editor, a terminal) calls `claim` from its `display_list` while it has a
//! caret.

use std::cell::Cell;

thread_local! {
    static CLAIM: Cell<Option<[f32; 4]>> = const { Cell::new(None) };
}

/// A field is editing this frame, its caret (or the field, when the caret is
/// not known) at `x, y, w, h` in logical surface coordinates. The last claim
/// of a frame wins.
pub fn claim(x: f32, y: f32, w: f32, h: f32) {
    CLAIM.with(|c| c.set(Some([x, y, w, h])));
}

/// The frame's claim, clearing it for the next frame.
pub(crate) fn take() -> Option<[f32; 4]> {
    CLAIM.with(|c| c.take())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_claim_lasts_one_frame() {
        let _ = take();
        claim(1.0, 2.0, 3.0, 4.0);
        assert_eq!(take(), Some([1.0, 2.0, 3.0, 4.0]));
        assert_eq!(take(), None, "a frame nobody claimed disables the text input");
    }
}
