//! "A text field is being edited": the announcement behind text input.
//!
//! The compositor needs to know when a field is open for typing, because that
//! is what raises the on-screen keyboard after a touch (cce-compositor's
//! `osk.rs`) and what activates an input method. A widget says so by calling
//! [`claim`] from its paint, every frame it is editing; a widget that stops
//! painting has stopped claiming, so there is no enable/disable pair to close
//! on every path out of editing (Enter, Escape, a click elsewhere, a focus
//! change, its page dropped).
//!
//! A claim is the caret the input method sees: it is [`crate::ime::report_caret`]
//! by its older name, and the shells read it through [`crate::ime::caret`]
//! once the frame is built (Wayland's `text-input-v3` in
//! `backend::text_input`, the browser's keyboard sink, the AppKit shell's
//! `NSTextInputClient`). A widget that knows its caret claims the caret; one
//! that does not claims its field. The last claim of a frame wins, so a
//! widget may claim its field first and its caret once it has drawn it.
//!
//! `TextBox`, `Spinbox`, `Slider`'s readout, `ColorSelector`, the params
//! pane's code rows and a focused `DocEditor` claim on their own. An app
//! drawing its own text (a `LineEdit`, a terminal, an editor) claims from its
//! `display_list` while it has a caret.

/// A field is editing this frame, its caret (or the field, when the caret is
/// not known) at `x, y, w, h` in the window's logical px. The last claim of a
/// frame wins.
pub fn claim(x: f32, y: f32, w: f32, h: f32) {
    crate::ime::report_caret(x, y, w, h);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_claim_lasts_one_frame() {
        crate::ime::begin_frame();
        claim(1.0, 2.0, 3.0, 4.0);
        crate::ime::end_frame();
        assert_eq!(crate::ime::caret(), Some([1.0, 2.0, 3.0, 4.0]));
        crate::ime::begin_frame();
        crate::ime::end_frame();
        assert_eq!(crate::ime::caret(), None, "a frame nobody claimed disables the text input");
    }
}
