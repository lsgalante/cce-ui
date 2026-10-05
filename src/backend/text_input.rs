//! `text-input-unstable-v3`: the Wayland shell's half of input-method
//! composition (`crate::ime`), as the hidden textarea is the browser's and
//! `NSTextInputClient` the AppKit shell's.
//!
//! The compositor relays between this object and the input method (an
//! `input-method-v2` client: fcitx5, IBus through its Wayland frontend, …):
//!
//! - **What we tell it.** While a widget is editing text (`ime::caret` is
//!   set) and the seat's text-input focus is on our surface (`enter`), the
//!   text input is ENABLED, with a normal content type and the caret as the
//!   cursor rectangle (surface px), re-sent when the caret moves; otherwise
//!   it is disabled. A composition a widget dropped (`ime::take_reset`) is
//!   cancelled by disabling and enabling again, which resets the input
//!   method's state. Every state change is one `commit`, counted.
//! - **What it tells us.** `preedit_string`, `commit_string` and
//!   `delete_surrounding_text` are double-buffered and applied on `done`, in
//!   the protocol's order: the old composition out, the commit typed
//!   (`Driver::commit_text`), the new composition in (`Driver::preedit`).
//!   A batch with no `preedit_string` ends the composition. We send no
//!   surrounding text, so a deletion has nothing to count its bytes in and
//!   is not applied. A `done` whose serial is behind our commits still
//!   applies its text — the protocol asks only that it not change our state.
//! - `leave` drops any composition, as the protocol asks; the compositor
//!   ignores our requests until the next `enter`.
//!
//! The pure part — what a batch does, and what state to send — is
//! [`Batch::apply_order`] and [`TextInput::plan`], tested with no
//! compositor.

use crate::ime::Preedit;

/// The double-buffered text a `done` applies.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Batch {
    pub preedit: Option<Preedit>,
    pub commit: Option<String>,
    pub delete: Option<(u32, u32)>,
}

/// One step of applying a batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Apply {
    Preedit(Option<Preedit>),
    Commit(String),
}

impl Batch {
    /// A `preedit_string` event: the protocol's cursor (bytes, -1 for
    /// hidden) as `Preedit`'s.
    pub fn set_preedit(&mut self, text: Option<String>, begin: i32, end: i32) {
        let text = text.unwrap_or_default();
        let cursor = (begin >= 0 && end >= 0).then(|| (begin as usize, end as usize));
        self.preedit = (!text.is_empty()).then(|| Preedit::new(text, cursor));
    }

    /// The batch as the protocol's `done` orders it: the old composition
    /// out, the commit typed, the new composition in.
    pub fn apply_order(self) -> Vec<Apply> {
        let mut steps = Vec::new();
        if let Some(text) = self.commit.filter(|t| !t.is_empty()) {
            steps.push(Apply::Preedit(None));
            steps.push(Apply::Commit(text));
        }
        steps.push(Apply::Preedit(self.preedit));
        steps
    }
}

/// What to send the compositor this turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Send {
    /// Nothing changed.
    Nothing,
    /// `enable` (after a `disable` if `reset`), the content type, the
    /// cursor rectangle, `commit`.
    Enable { rect: [i32; 4], reset: bool },
    /// The cursor rectangle moved: it, then `commit`.
    Move { rect: [i32; 4] },
    /// `disable`, `commit`.
    Disable,
}

/// The text input's state on our side.
#[derive(Debug, Default)]
pub struct TextInput {
    /// The seat's text-input focus is on our surface.
    pub entered: bool,
    /// We have committed an `enable` (and no `disable` since).
    pub enabled: bool,
    /// The cursor rectangle last committed.
    pub sent_rect: Option<[i32; 4]>,
    /// `commit` requests issued: what a current `done` carries as its serial.
    pub commits: u32,
    /// The batch since the last `done`.
    pub pending: Batch,
}

impl TextInput {
    /// What the state should become, given the editing widget's caret
    /// (`ime::caret`, in the app's logical px), the surface's px per
    /// logical px, and whether a widget asked for its composition to be
    /// cancelled. Records the new state as sent.
    pub fn plan(&mut self, caret: Option<[f32; 4]>, surface_scale: f32, reset: bool) -> Send {
        let rect = caret.map(|[x, y, w, h]| {
            let s = surface_scale;
            [(x * s).round() as i32, (y * s).round() as i32, ((w * s).round() as i32).max(1), ((h * s).round() as i32).max(1)]
        });
        let send = match (self.entered, rect) {
            (true, Some(rect)) if !self.enabled || reset => Send::Enable { rect, reset: self.enabled && reset },
            (true, Some(rect)) if self.sent_rect != Some(rect) => Send::Move { rect },
            (true, Some(_)) => Send::Nothing,
            (true, None) if self.enabled => Send::Disable,
            _ => Send::Nothing,
        };
        match send {
            Send::Enable { rect, reset } => {
                self.enabled = true;
                self.sent_rect = Some(rect);
                // A reset commits its disable before the enable.
                self.commits += if reset { 2 } else { 1 };
            }
            Send::Move { rect } => {
                self.sent_rect = Some(rect);
                self.commits += 1;
            }
            Send::Disable => {
                self.enabled = false;
                self.sent_rect = None;
                self.commits += 1;
            }
            Send::Nothing => {}
        }
        send
    }

    /// `enter`: our surface has the text-input focus; the next plan enables
    /// if a widget is editing.
    pub fn enter(&mut self) {
        self.entered = true;
        self.enabled = false;
        self.sent_rect = None;
    }

    /// `leave`: the focus went; the compositor disabled us, and ignores us
    /// until the next `enter`.
    pub fn leave(&mut self) {
        self.entered = false;
        self.enabled = false;
        self.sent_rect = None;
        self.pending = Batch::default();
    }

    /// `done`: the batch to apply, the pending state back to initial.
    pub fn done(&mut self) -> Batch {
        std::mem::take(&mut self.pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_done_ends_the_composition_types_the_commit_and_starts_the_next() {
        let mut b = Batch::default();
        b.commit = Some("日本".into());
        b.set_preedit(Some("ご".into()), 3, 3);
        assert_eq!(
            b.apply_order(),
            vec![
                Apply::Preedit(None),
                Apply::Commit("日本".into()),
                Apply::Preedit(Some(Preedit::new("ご", Some((3, 3))))),
            ]
        );
        // A batch with no preedit_string ends the composition; a hidden
        // cursor is none.
        assert_eq!(Batch::default().apply_order(), vec![Apply::Preedit(None)]);
        let mut b = Batch::default();
        b.set_preedit(Some("か".into()), -1, -1);
        assert_eq!(b.apply_order(), vec![Apply::Preedit(Some(Preedit::new("か", None)))]);
    }

    #[test]
    fn enabled_only_while_focused_and_editing_and_every_change_one_commit() {
        let mut ti = TextInput::default();
        let caret = Some([10.0, 20.0, 1.5, 16.0]);
        // Editing, but not yet entered: nothing to say.
        assert_eq!(ti.plan(caret, 1.0, false), Send::Nothing);
        ti.enter();
        assert_eq!(ti.plan(caret, 1.0, false), Send::Enable { rect: [10, 20, 2, 16], reset: false });
        assert_eq!(ti.plan(caret, 1.0, false), Send::Nothing);
        // The caret moves; at a forced scale of 2 the surface is twice the app.
        assert_eq!(ti.plan(Some([30.0, 20.0, 1.5, 16.0]), 2.0, false), Send::Move { rect: [60, 40, 3, 32] });
        // A widget dropped its composition: disable and enable again.
        assert_eq!(ti.plan(Some([30.0, 20.0, 1.5, 16.0]), 2.0, true), Send::Enable { rect: [60, 40, 3, 32], reset: true });
        // Nothing editing.
        assert_eq!(ti.plan(None, 1.0, false), Send::Disable);
        assert_eq!(ti.plan(None, 1.0, false), Send::Nothing);
        assert_eq!(ti.commits, 5, "enable, move, disable + enable, disable");
        // The focus leaves; editing again enables nothing until it is back.
        ti.leave();
        assert_eq!(ti.plan(caret, 1.0, false), Send::Nothing);
        ti.enter();
        assert!(matches!(ti.plan(caret, 1.0, false), Send::Enable { reset: false, .. }));
    }

    #[test]
    fn a_reset_with_nothing_enabled_is_an_ordinary_enable() {
        let mut ti = TextInput::default();
        ti.enter();
        assert_eq!(ti.plan(Some([0.0, 0.0, 1.0, 10.0]), 1.0, true), Send::Enable { rect: [0, 0, 1, 10], reset: false });
    }
}
