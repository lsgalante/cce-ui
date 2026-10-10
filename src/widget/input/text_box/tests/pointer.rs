//! Focus, clicks, drags, selection and its highlight, and the context menu.

use super::*;

#[test]
fn test_textbox_selection_highlight() {
    let mut dummy = crate::context::UiContext::new();
    let mut tb = TextBox::new("Initial Text".to_string());
    tb.set_rect(10.0, 10.0, 200.0, 30.0);

    // 1. Initial state
    assert!(!tb.editing);
    assert!(!tb.all_selected);

    // 2. Keyboard focus triggers highlighting. (A CLICK deliberately does not
    //    — it places the caret; see `prefilled_single_line_click_does_not_wipe_the_value`.)
    tb.focus();
    assert!(tb.editing);
    assert!(tb.all_selected);
    assert_eq!(tb.edit_buffer, "Initial Text");

    // 3. Typing a key replaces all text
    let key_ev = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Character("A".to_string()),
        text: Some("A".to_string()),
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    let handled = tb.keyboard_input(&key_ev, &mut dummy);
    assert!(handled);
    assert!(!tb.all_selected);
    assert_eq!(tb.edit_buffer, "A");

    // 4. Pressing Enter commits change
    let enter_ev = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Named(NamedKey::Enter),
        text: None,
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    let handled_enter = tb.keyboard_input(&enter_ev, &mut dummy);
    assert!(handled_enter);
    assert!(!tb.editing);
    assert_eq!(tb.text, "A");
    assert!(tb.take_change());
}

/// The other half of the click change: keyboard focus must still arm
/// select-all, so tabbing into a field and typing replaces it. Losing this
/// would make every form field tedious to retype.
#[test]
fn keyboard_focus_still_selects_all() {
    let mut tb = TextBox::new("imap.example.org:993".to_string());
    tb.set_rect(10.0, 10.0, 200.0, 30.0);

    tb.focus();

    assert!(tb.editing);
    assert!(tb.all_selected, "tabbing in selects the whole value");
    assert_eq!(tb.select_anchor, Some(0));
    assert_eq!(tb.cursor_idx, "imap.example.org:993".chars().count());
}

/// A prefilled single-line box is the case that motivated the change: the
/// focusing click must leave the value intact so the first keystroke edits
/// rather than erases. Same guarantee the multiline test below asserts.
#[test]
fn prefilled_single_line_click_does_not_wipe_the_value() {
    let mut dummy = crate::context::UiContext::new();
    let mut tb = TextBox::new("imap.example.org:993".to_string());
    tb.set_rect(10.0, 10.0, 300.0, 30.0);

    let click_x = 10.0 + 8.0 + 4.0 * tb.char_width();
    assert!(tb.mouse_input(MouseButton::Left, ElementState::Pressed, click_x, 20.0, &mut dummy));
    assert!(tb.editing);
    assert!(!tb.all_selected);

    let key_ev = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Character("X".to_string()),
        text: Some("X".to_string()),
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    assert!(tb.keyboard_input(&key_ev, &mut dummy));
    assert_eq!(
        tb.edit_buffer.chars().count(),
        21,
        "typing must insert into the prefilled value, not replace it"
    );
    assert!(tb.edit_buffer.starts_with("imap"), "the existing value survives the first keystroke");
}

#[test]
fn empty_box_click_ignores_placeholder_glyphs() {
    let mut dummy = crate::context::UiContext::new();
    let mut tb = TextBox::new(String::new()).with_placeholder("Search...");
    tb.set_rect(10.0, 10.0, 200.0, 30.0);

    // `prepare_text` shapes the placeholder when the value is empty; the
    // caret math must still treat the box as zero-length. Simulate the
    // shaped placeholder ("Search...", 9 cols) directly so the test does
    // not depend on a font being present.
    tb.glyph_positions = (0..=9).map(|i| i as f32 * 7.0).collect();

    // Click deep into the painted placeholder.
    let click_x = 10.0 + 8.0 + 8.0 * 7.0;
    assert!(tb.mouse_input(MouseButton::Left, ElementState::Pressed, click_x, 20.0, &mut dummy));
    assert!(tb.editing);
    assert_eq!(tb.cursor_idx, 0, "empty box: the caret lands at the start, not on a placeholder column");
}

#[test]
fn multiline_focus_click_places_caret_instead_of_select_all() {
    let mut dummy = crate::context::UiContext::new();
    let mut tb = TextBox::new("abcdef".to_string()).with_multiline(true);
    tb.set_rect(10.0, 10.0, 200.0, 100.0);

    // The focusing click must NOT arm select-all (a first keystroke would wipe
    // prefilled content, e.g. a reply quote) — it places the caret like any click.
    let clicked = tb.mouse_input(MouseButton::Left, ElementState::Pressed, 12.0, 20.0, &mut dummy);
    assert!(clicked);
    assert!(tb.editing);
    assert!(!tb.all_selected);
    let caret_after_click = tb.cursor_idx;
    let key_ev = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Character("X".to_string()),
        text: Some("X".to_string()),
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    let handled = tb.keyboard_input(&key_ev, &mut dummy);
    assert!(handled);
    assert_eq!(tb.edit_buffer.chars().count(), 7, "typing must insert, not replace the buffer");
    assert!(tb.edit_buffer.contains("X"));
    assert_eq!(tb.cursor_idx, caret_after_click + 1);
}

#[test]
fn test_textbox_drag_and_modifier_selection() {
    let mut dummy = crate::context::UiContext::new();
    let mut tb = TextBox::new("Hello World".to_string());
    tb.set_rect(10.0, 10.0, 200.0, 30.0);

    // 1. Initial click focuses and places the caret where it landed — it does
    //    NOT select all. Selecting all belongs to keyboard focus (see
    //    `keyboard_focus_still_selects_all`).
    let click_x3 = 10.0 + 8.0 + 3.0 * tb.char_width();
    let pressed = tb.mouse_input(MouseButton::Left, ElementState::Pressed, click_x3, 20.0, &mut dummy);
    assert!(pressed);
    let released = tb.mouse_input(MouseButton::Left, ElementState::Released, click_x3, 20.0, &mut dummy);
    assert!(released);
    assert!(tb.editing);
    assert!(!tb.all_selected);
    assert_eq!(tb.cursor_idx, 3);
    // The release collapses the zero-width selection the press opened.
    assert_eq!(tb.select_anchor, None);

    // 2. Click inside placed caret at index 5
    let click_x5 = 10.0 + 8.0 + 5.0 * tb.char_width();
    let pressed_inside = tb.mouse_input(MouseButton::Left, ElementState::Pressed, click_x5, 20.0, &mut dummy);
    assert!(pressed_inside);
    assert_eq!(tb.cursor_idx, 5);
    assert_eq!(tb.select_anchor, Some(5));
    assert!(!tb.all_selected);

    // 3. Drag to index 11
    let drag_x11 = 10.0 + 8.0 + 11.0 * tb.char_width();
    tb.drag_begin(click_x5, 20.0);
    let updated = tb.drag_update(drag_x11, 20.0);
    assert!(updated);
    assert_eq!(tb.cursor_idx, 11);
    assert_eq!(tb.select_anchor, Some(5));
    tb.drag_end();

    // 4. Keyboard ArrowLeft with Shift shrinks selection from 11 to 10
    let left_shift_ev = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Named(NamedKey::ArrowLeft),
        text: None,
        repeat: false,
        ctrl: false,
        shift: true,
        alt: false,
    };
    let handled = tb.keyboard_input(&left_shift_ev, &mut dummy);
    assert!(handled);
    assert_eq!(tb.cursor_idx, 10);
    assert_eq!(tb.select_anchor, Some(5));

    // 5. Keyboard ArrowLeft without Shift collapses selection to start (index 5)
    let left_ev = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Named(NamedKey::ArrowLeft),
        text: None,
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    let handled = tb.keyboard_input(&left_ev, &mut dummy);
    assert!(handled);
    assert_eq!(tb.cursor_idx, 5);
    assert_eq!(tb.select_anchor, None);

    // 6. Keyboard Shift+Up highlights to beginning (cursor 0, anchor 5)
    let up_shift_ev = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Named(NamedKey::ArrowUp),
        text: None,
        repeat: false,
        ctrl: false,
        shift: true,
        alt: false,
    };
    let handled = tb.keyboard_input(&up_shift_ev, &mut dummy);
    assert!(handled);
    assert_eq!(tb.cursor_idx, 0);
    assert_eq!(tb.select_anchor, Some(5));

    // 7. Typing a key replaces selected range "Hello" with "Rust"
    let rust_ev = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Character("Rust".to_string()),
        text: Some("Rust".to_string()),
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    let handled = tb.keyboard_input(&rust_ev, &mut dummy);
    assert!(handled);
    assert_eq!(tb.edit_buffer, "Rust World");
    assert_eq!(tb.cursor_idx, 4);
    assert_eq!(tb.select_anchor, None);
}

#[test]
fn test_textbox_right_click_context_menu() {
    let mut dummy = crate::context::UiContext::new();
    let mut tb = TextBox::new("Context Menu Text".to_string());
    tb.set_rect(10.0, 10.0, 200.0, 30.0);

    // Hide context menu initially
    dummy.hide_context_menu();
    assert!(!dummy.is_context_menu_visible());

    // Right click on textbox
    let clicked = tb.mouse_input(MouseButton::Right, ElementState::Pressed, 50.0, 20.0, &mut dummy);
    assert!(clicked);
    assert!(dummy.is_context_menu_visible());
    assert!(tb.editing);
 }

#[test]
fn test_textbox_multiline_selection_highlight() {
    let mut tb = TextBox::new("Line 1\nLine 2\nLine 3".to_string()).with_multiline(true);
    tb.set_rect(10.0, 10.0, 200.0, 100.0);
    tb.select_anchor = Some(7); // starts at "Line 2"
    tb.cursor_idx = 13;        // ends at end of "Line 2"

    use crate::scene::paint::Prim;
    let highlight = [0.20, 0.50, 0.85, 0.3];
    let has_highlight = crate::widget::shown_prims(&tb).iter().any(|p| match p {
        Prim::Quad { color, .. } | Prim::RoundedRect { color, .. } => *color == highlight,
        _ => false,
    });
    assert!(has_highlight, "Should have a highlight quad!");
}
