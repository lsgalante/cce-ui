//! Composition, undo runs, the clipboard and a password box, and the search box's clear.

use super::*;

/// An input method's composition is a provisional run in the buffer: shown
/// at the caret with the caret where the input method has it, never held
/// (`committed_buffer`, `take_change`), replaced by the commit, which is
/// typed — and dropped, the input method told, when editing ends under it.
#[test]
fn a_composition_is_shown_in_place_and_the_commit_is_typed() {
    use crate::ime::{self, Preedit};
    let mut dummy = crate::context::UiContext::new();
    let mut tb = TextBox::new("ab".to_string()).with_update_on_type(true);
    tb.set_rect(10.0, 10.0, 300.0, 30.0);
    tb.focus();
    tb.cursor_idx = 1;
    tb.select_anchor = None;
    tb.all_selected = false;
    let _ = ime::take_reset();

    // "にほ", the input method's cursor after "に".
    ime::set_preedit(Some(Preedit::new("にほ", Some((0, 3)))));
    tb.sync_preedit();
    assert_eq!(tb.edit_buffer, "aにほb");
    assert_eq!(tb.composing, Some((1, 2)));
    assert_eq!(tb.cursor_idx, 2);
    assert_eq!(tb.committed_buffer(), "ab");
    assert!(!tb.take_change(), "a composition is not a change");
    // A key the input method lets through mid-composition is not typed.
    let typed = |t: &str| KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Character(t.to_string()),
        text: Some(t.to_string()),
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    assert!(tb.keyboard_input(&typed("x"), &mut dummy));
    assert_eq!(tb.edit_buffer, "aにほb");

    // The commit: the composition ends, then its text is typed.
    ime::set_preedit(None);
    assert!(tb.keyboard_input(&typed("日本"), &mut dummy));
    assert_eq!(tb.edit_buffer, "a日本b");
    assert_eq!((tb.composing, tb.cursor_idx), (None, 3));
    assert!(tb.take_change());
    assert_eq!(tb.text, "a日本b");

    // Editing ends mid-composition: the run goes, the input method is
    // told, and the text is what was held.
    ime::set_preedit(Some(Preedit::new("か", None)));
    tb.sync_preedit();
    assert_eq!(tb.edit_buffer, "a日本かb");
    tb.commit_editing();
    assert_eq!(tb.text, "a日本b");
    assert!(ime::take_reset());
    assert_eq!(ime::preedit(), None);
}

/// Undo/redo over an editing session: a typed word is one step, a
/// space starts the next, a cursor move splits a run, Backspace runs
/// coalesce, redo walks forward, a fresh keystroke after an undo forks,
/// and the chord reaches the box both as a `ContextAction` (the runner's
/// route) and as a raw key (the no-context fallback).
#[test]
fn typing_undoes_by_run() {
    let mut dummy = crate::context::UiContext::new();
    let mut tb = TextBox::new(String::new());
    tb.set_rect(10.0, 10.0, 300.0, 30.0);
    tb.focus();
    assert!(tb.editing);
    assert!(!tb.undo_edit(), "a fresh session has nothing to undo");

    let key = |k: &str, ctrl: bool, shift: bool| KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Character(k.to_string()),
        text: if ctrl { None } else { Some(k.to_string()) },
        repeat: false,
        ctrl,
        shift,
        alt: false,
    };
    let named = |n: NamedKey| KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Named(n),
        text: None,
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    let type_str = |tb: &mut Adapted<TextBox>, dummy: &mut crate::context::UiContext, s: &str| {
        for ch in s.chars() {
            assert!(tb.keyboard_input(&key(&ch.to_string(), false, false), dummy));
        }
    };

    type_str(&mut tb, &mut dummy, "hello world");
    assert_eq!(tb.edit_buffer, "hello world");
    assert_eq!(tb.history.undo_len(), 3, "'hello', ' ', 'world'");

    // A cursor move splits the next run off.
    assert!(tb.keyboard_input(&named(NamedKey::ArrowLeft), &mut dummy));
    type_str(&mut tb, &mut dummy, "XY");
    assert_eq!(tb.edit_buffer, "hello worlXYd");
    assert_eq!(tb.history.undo_len(), 4);

    // Backspaces coalesce into one step.
    assert!(tb.keyboard_input(&named(NamedKey::Backspace), &mut dummy));
    assert!(tb.keyboard_input(&named(NamedKey::Backspace), &mut dummy));
    assert_eq!(tb.edit_buffer, "hello world");
    assert_eq!(tb.history.undo_len(), 5);

    // Undo through the ContextAction route, then the raw-chord route.
    assert!(crate::widget::WidgetHostExt::context_action(&mut tb, crate::widget::ContextAction::Undo));
    assert_eq!(tb.edit_buffer, "hello worlXYd");
    assert!(tb.keyboard_input(&key("z", true, false), &mut dummy));
    assert_eq!(tb.edit_buffer, "hello world");
    assert!(tb.keyboard_input(&key("Z", true, true), &mut dummy), "redo via ctrl+shift+z");
    assert_eq!(tb.edit_buffer, "hello worlXYd");
    assert!(tb.keyboard_input(&key("z", true, false), &mut dummy));
    assert!(tb.keyboard_input(&key("z", true, false), &mut dummy));
    assert_eq!(tb.edit_buffer, "hello ");
    assert!(tb.keyboard_input(&key("z", true, false), &mut dummy));
    assert_eq!(tb.edit_buffer, "hello");
    assert!(tb.keyboard_input(&key("z", true, false), &mut dummy));
    assert_eq!(tb.edit_buffer, "");
    assert!(!tb.undo_edit(), "history exhausted");

    // Redo forward one, then a fresh keystroke forks the branch.
    assert!(crate::widget::WidgetHostExt::context_action(&mut tb, crate::widget::ContextAction::Redo));
    assert_eq!(tb.edit_buffer, "hello");
    type_str(&mut tb, &mut dummy, "!");
    assert_eq!(tb.edit_buffer, "hello!");
    assert!(!tb.redo_edit(), "a new edit after an undo drops the redo branch");

    // A committed value is not the box's to undo.
    tb.unfocus();
    assert!(!tb.editing);
    assert!(!crate::widget::WidgetHostExt::context_action(&mut tb, crate::widget::ContextAction::Undo));
}

#[test]
fn test_search_textbox_clear_option() {
    let mut dummy = crate::context::UiContext::new();
    let mut tb = TextBox::new("Some Search query".to_string()).with_placeholder("Search...");
    tb.set_rect(10.0, 10.0, 200.0, 30.0);

    // Right click on textbox
    let clicked = tb.mouse_input(MouseButton::Right, ElementState::Pressed, 50.0, 20.0, &mut dummy);
    assert!(clicked);
    assert!(dummy.is_context_menu_visible());

    // Verify "Clear" option is in options
    let opts = crate::widget::context_menu::options();
    assert!(opts.contains(&"Clear".to_string()));

    // Simulate choosing the "Clear" option
    crate::widget::WidgetHostExt::context_action(&mut tb, crate::widget::ContextAction::ClearText);
    assert_eq!(tb.text, "");
    assert_eq!(tb.edit_buffer, "");
}

/// A password box never hands its text to the clipboard -- not from a
/// selection, not by Ctrl+X, not through the menu -- and its menu offers
/// no Cut or Copy. Its Ctrl+X leaves the text in place.
#[test]
fn a_password_box_never_copies_or_cuts() {
    let mut dummy = crate::context::UiContext::new();
    let mut tb = TextBox::new(String::new()).with_password(true);
    tb.set_rect(10.0, 10.0, 200.0, 30.0);
    tb.focus();
    tb.edit_buffer = "hunter2".to_string();
    tb.cursor_idx = 7;
    tb.select_all();
    let state = TextEditorState {
        buffer: tb.edit_buffer.clone(),
        cursor_idx: tb.cursor_idx,
        select_anchor: tb.select_anchor,
        all_selected: tb.all_selected,
    };
    assert_eq!(state.selected_text().as_deref(), Some("hunter2"), "the selection is there");
    assert_eq!(tb.clipboard_text(&state), None, "but never for the clipboard");

    let ctrl_x = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Character("x".to_string()),
        text: None,
        repeat: false,
        ctrl: true,
        shift: false,
        alt: false,
    };
    tb.keyboard_input(&ctrl_x, &mut dummy);
    assert_eq!(tb.edit_buffer, "hunter2", "Ctrl+X cuts nothing");
    assert!(!crate::widget::WidgetHostExt::context_action(&mut tb, crate::widget::ContextAction::Cut));
    assert_eq!(tb.edit_buffer, "hunter2", "nor does the menu's Cut");

    dummy.hide_context_menu();
    assert!(tb.mouse_input(MouseButton::Right, ElementState::Pressed, 50.0, 20.0, &mut dummy));
    let opts = crate::widget::context_menu::options();
    assert!(!opts.iter().any(|o| o == "Cut" || o == "Copy"), "no Cut / Copy rows: {opts:?}");
    assert!(opts.iter().any(|o| o == "Paste"), "Paste stays: {opts:?}");
    dummy.hide_context_menu();
}

/// The gate is the password flag alone: an ordinary box's selection goes.
#[test]
fn an_ordinary_box_selection_is_clipboard_text() {
    let tb = TextBox::new("hello".to_string());
    let state = TextEditorState {
        buffer: "hello".to_string(),
        cursor_idx: 5,
        select_anchor: Some(0),
        all_selected: false,
    };
    assert_eq!(tb.clipboard_text(&state).as_deref(), Some("hello"));
}
