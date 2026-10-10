//! The inline code editor: edits reach the value on Ctrl+Enter, indenting, selection and undo, the
//! error line, and context actions.

use super::*;

fn key(k: Key, ctrl: bool, shift: bool) -> Event {
    Event::KeyInput(crate::widget::KeyEvent { state: ElementState::Pressed, logical_key: k, text: None, repeat: false, ctrl, shift, alt: false })
}

fn chr(c: &str) -> Event {
    key(Key::Character(c.into()), false, false)
}

fn ctrl(c: &str) -> Event {
    key(Key::Character(c.into()), true, false)
}

fn named(k: NamedKey) -> Event {
    key(Key::Named(k), false, false)
}

fn shift_named(k: NamedKey) -> Event {
    key(Key::Named(k), false, true)
}

/// A code row focused by a click at its first character.
fn code_panel(src: &str) -> (Adapted<ParametersBg>, UiContext) {
    let mut ctx = UiContext::new();
    let mut p = panel_with(&[("Src", src, "code")]);
    let r = p.get_param_rects()[0];
    let x = p.code_text_x(r);
    p.mouse_input(MouseButton::Left, ElementState::Pressed, x, r.1 + ParametersBg::CODE_TOP + 2.0, &mut ctx);
    assert_eq!(p.focused_param, Some(0));
    (p, ctx)
}

fn send(p: &mut Adapted<ParametersBg>, ctx: &mut UiContext, ev: Event) -> bool {
    WidgetHost::handle_event(p, &ev, ctx)
}

/// Typing edits the buffer and leaves the VALUE alone until ctrl+enter
/// applies it — a script would otherwise re-run on every keystroke — and
/// the row says so: dirty while they differ, clean once applied.
#[test]
fn code_edits_apply_on_ctrl_enter_not_per_keystroke() {
    let (mut p, mut ctx) = code_panel("let x = 1;");
    assert!(!p.code_is_dirty());
    send(&mut p, &mut ctx, chr("/"));
    send(&mut p, &mut ctx, chr("/"));
    assert_eq!(p.code_editor.as_ref().unwrap().buffer, "//let x = 1;");
    assert_eq!(ParamController::node_params(&*p)[0].1, "let x = 1;", "the value waits");
    assert!(p.code_is_dirty());
    send(&mut p, &mut ctx, key(Key::Named(NamedKey::Enter), true, false));
    assert_eq!(ParamController::node_params(&*p)[0].1, "//let x = 1;", "ctrl+enter applies");
    assert!(!p.code_is_dirty());
    assert_eq!(p.focused_param, Some(0), "and keeps editing");
    // Escape applies too, and leaves the row.
    send(&mut p, &mut ctx, chr("!"));
    send(&mut p, &mut ctx, named(NamedKey::Escape));
    assert_eq!(ParamController::node_params(&*p)[0].1, "//!let x = 1;");
    assert_eq!(p.focused_param, None);
}

/// Tab indents, shift+tab dedents, and Enter carries the indentation —
/// one level deeper after an opening brace.
#[test]
fn code_editor_indents_like_an_editor() {
    let (mut p, mut ctx) = code_panel("");
    send(&mut p, &mut ctx, chr("i"));
    send(&mut p, &mut ctx, chr("f"));
    send(&mut p, &mut ctx, chr(" "));
    send(&mut p, &mut ctx, chr("{"));
    send(&mut p, &mut ctx, named(NamedKey::Enter));
    assert_eq!(p.code_editor.as_ref().unwrap().buffer, "if {\n    ", "a brace opens a level");
    send(&mut p, &mut ctx, chr("x"));
    send(&mut p, &mut ctx, named(NamedKey::Enter));
    assert_eq!(p.code_editor.as_ref().unwrap().buffer, "if {\n    x\n    ", "the level carries");
    send(&mut p, &mut ctx, shift_named(NamedKey::Tab));
    assert_eq!(p.code_editor.as_ref().unwrap().buffer, "if {\n    x\n", "shift+tab dedents the line");
    send(&mut p, &mut ctx, chr("}"));
    send(&mut p, &mut ctx, named(NamedKey::Tab));
    assert_eq!(p.code_editor.as_ref().unwrap().buffer, "if {\n    x\n}    ", "tab inserts a level");
    assert!(send(&mut p, &mut ctx, named(NamedKey::Tab)), "tab is the editor's: it does not fall through to the host");
}

/// shift+arrows select, typing replaces the selection, ctrl+z undoes a
/// typing run as one step and ctrl+shift+z redoes it.
#[test]
fn code_editor_selects_and_undoes() {
    let (mut p, mut ctx) = code_panel("abc\ndef");
    send(&mut p, &mut ctx, named(NamedKey::End));
    send(&mut p, &mut ctx, shift_named(NamedKey::ArrowDown));
    let e = p.code_editor.as_ref().unwrap();
    assert_eq!(e.selected_text().as_deref(), Some("\ndef"), "shift+down from the end of line 1 selects to the same column of line 2");
    send(&mut p, &mut ctx, chr("Z"));
    assert_eq!(p.code_editor.as_ref().unwrap().buffer, "abcZ", "typing replaces the selection");
    send(&mut p, &mut ctx, chr("Y"));
    send(&mut p, &mut ctx, chr("X"));
    send(&mut p, &mut ctx, ctrl("z"));
    assert_eq!(p.code_editor.as_ref().unwrap().buffer, "abc\ndef", "one typing run, one undo");
    send(&mut p, &mut ctx, key(Key::Character("z".into()), true, true));
    assert_eq!(p.code_editor.as_ref().unwrap().buffer, "abcZYX", "and one redo");
    send(&mut p, &mut ctx, key(Key::Character("a".into()), true, true));
    assert_eq!(p.code_editor.as_ref().unwrap().selected_text().as_deref(), Some("abcZYX"), "ctrl+shift+a selects all");
    assert_eq!(ParamController::node_params(&*p)[0].1, "abc\ndef", "none of it applied yet");
}

/// The host flags an error line and the gutter number turns red for it;
/// a click lands the caret past the gutter, on the column it named.
#[test]
fn code_row_flags_an_error_line_and_clicks_land_past_the_gutter() {
    let (mut p, mut ctx) = code_panel("one\ntwo\nthree");
    p.set_code_error_line(Some(1));
    let labels = p.own_text_labels();
    let two = labels.iter().find(|l| l.text == "  2").expect("a gutter number for line 2");
    assert_eq!(two.color, [0xff, 0x80, 0x70]);
    let one = labels.iter().find(|l| l.text == "  1").unwrap();
    assert_eq!(one.color, [0x66, 0x66, 0x78]);
    let r = p.get_param_rects()[0];
    let x = p.code_text_x(r) + 2.0 * p.code_col_w();
    p.mouse_input(MouseButton::Left, ElementState::Pressed, x, r.1 + ParametersBg::CODE_TOP + 2.0 * ParametersBg::CODE_LINE_H + 2.0, &mut ctx);
    let e = p.code_editor.as_ref().unwrap();
    assert_eq!(get_cursor_line_col(&e.buffer, e.cursor_idx), (2, 2), "line 3, column 2");
    p.set_code_error_line(None);
    assert!(p.own_text_labels().iter().all(|l| l.color != [0xff, 0x80, 0x70]));
}

/// The context actions reach the editor: undo through the runner's
/// routing is the editor's own history, and select-all selects the
/// buffer. Outside an edit the pane declines them.
#[test]
fn code_editor_answers_context_actions_while_editing() {
    use crate::widget::ContextAction;
    let (mut p, mut ctx) = code_panel("abc");
    send(&mut p, &mut ctx, named(NamedKey::End));
    send(&mut p, &mut ctx, chr("d"));
    assert_eq!(p.code_editor.as_ref().unwrap().buffer, "abcd");
    assert!(Input::context_action(&mut *p, ContextAction::Undo));
    assert_eq!(p.code_editor.as_ref().unwrap().buffer, "abc", "undo through the context action");
    assert!(Input::context_action(&mut *p, ContextAction::Redo));
    assert_eq!(p.code_editor.as_ref().unwrap().buffer, "abcd");
    assert!(Input::context_action(&mut *p, ContextAction::SelectAll));
    assert_eq!(p.code_editor.as_ref().unwrap().selected_text().as_deref(), Some("abcd"));
    WidgetHost::unfocus(&mut p);
    assert!(!Input::context_action(&mut *p, ContextAction::Undo), "nothing to act on once the editor is closed");
}
