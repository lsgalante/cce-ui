//! A row's marks, drawn as cce-icons glyphs: the check and the radio marks a label may begin with,
//! the page row's chevron and the back band's, splitting a mark off a label, and matching an English
//! label to an action for menus that set none.

/// The mark a row that leads to a PAGE carries at its right end. Drawn
/// as the `chevron-right` glyph; the character is what a host that lays
/// out its own rows (the designer's dialog) may reserve room by.
pub const PAGE_MARK: &str = "›";

/// What a page's back band leads with, before the title of the plate it
/// goes back to. Drawn as the `chevron-left` glyph.
pub const BACK_MARK: &str = "‹";

/// A row whose label BEGINS with one of these wears the mark as a
/// cce-icons glyph at its left, and the label is drawn without it: a
/// checked item (`check`), a switch or radio that is on (`circle`), one
/// that is off (`circle-outline`). The text stays the row's identity —
/// a host matching its own labels still matches `"✓ Show Grid"` — and
/// the toolkit owns how a mark looks, so no menu draws one as a
/// character in whatever face its font falls back to.
pub const MARK_CHECK: &str = "✓ ";

/// See [`MARK_CHECK`]: a switch or radio row that is on.
pub const MARK_ON: &str = "● ";

/// See [`MARK_CHECK`]: a switch or radio row that is off.
pub const MARK_OFF: &str = "○ ";

/// The glyph a label's leading mark names, and the label without it.
/// A row's action read off its English label: the fallback for a menu built without
/// [`set_row_actions`](super::set_row_actions). It breaks the moment a label
/// is translated (`docs/rfc-accessibility-locale.md`), so the toolkit's own menus set
/// their actions and this serves only menus that do not.
pub fn legacy_action_for_label(label: &str) -> Option<crate::widget::ContextAction> {
    use crate::widget::ContextAction as CA;
    Some(match label {
        "Cut" => CA::Cut,
        "Copy" => CA::Copy,
        "Paste" => CA::Paste,
        "Select All" => CA::SelectAll,
        "Undo" => CA::Undo,
        "Redo" => CA::Redo,
        "Clear" => CA::ClearText,
        "Copy Key" => CA::CopyKey,
        "Copy Value" => CA::CopyValue,
        "Delete" => CA::DeleteKey,
        "Expand" => CA::ExpandNode,
        "Collapse" => CA::CollapseNode,
        "Expand All" => CA::ExpandAll,
        "Collapse All" => CA::CollapseAll,
        "Copy Path" => CA::CopyPath,
        // The Ramp toggle carries its check state in the label.
        "✓ Collapse controls" | "Collapse controls" => CA::ToggleRampControls,
        _ => return None,
    })
}

pub fn split_mark(label: &str) -> (Option<&'static str>, &str) {
    for (mark, glyph) in [(MARK_CHECK, "check"), (MARK_ON, "circle"), (MARK_OFF, "circle-outline")] {
        if let Some(rest) = label.strip_prefix(mark) {
            return (Some(glyph), rest);
        }
    }
    (None, label)
}

/// How wide a row mark is drawn, at menu font size `size`, and the gap
/// after it: the glyph box at the font size, so the mark (a disc fills
/// three quarters of its box) stands about as tall as a capital.
pub(super) fn mark_size(size: f32) -> f32 {
    (size * 0.95).round()
}

pub(super) const MARK_GAP: f32 = 6.0;

/// The page and back chevrons, smaller still: they point, they do not
/// label.
pub(super) fn chevron_size(size: f32) -> f32 {
    (size * 0.8).round()
}
