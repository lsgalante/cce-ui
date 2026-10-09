use super::*;

fn ev(key: Key, ctrl: bool) -> KeyEvent {
    let text = match &key {
        Key::Character(c) if !ctrl => Some(c.clone()),
        _ => None,
    };
    KeyEvent {
        state: ElementState::Pressed,
        logical_key: key,
        text,
        repeat: false,
        ctrl,
        shift: false,
        alt: false,
    }
}
fn ch(c: &str) -> KeyEvent {
    ev(Key::Character(c.into()), false)
}
fn ctrl(c: &str) -> KeyEvent {
    ev(Key::Character(c.into()), true)
}
fn named(n: NamedKey) -> KeyEvent {
    ev(Key::Named(n), false)
}

/// A composition is shown, not held: `display` has it at the caret, the
/// indices map across it, `text` never sees it; keys wait while it is up;
/// the commit is typed; a press drops it and asks the input method to
/// cancel; a masked field shows it as bullets.
#[test]
fn a_composition_is_shown_at_the_caret_and_never_held() {
    let mut e = LineEdit::with_text("ab");
    e.cursor = 1;
    let _ = ime::take_reset();
    // "にほ", the input method's cursor after "に" (three bytes in).
    ime::set_preedit(Some(Preedit::new("にほ", Some((3, 3)))));
    assert!(e.sync_ime());
    assert_eq!(e.text, "ab");
    assert_eq!(e.display(), "aにほb");
    assert_eq!(e.composition_range(), Some((1, 7)));
    assert_eq!(e.display_index(1), 4, "the caret after に");
    assert_eq!(e.display_index(2), 8, "what follows, after the composition");
    assert_eq!(e.text_index(5), 1, "inside the composition is the caret");
    assert_eq!(e.text_index(8), 2);
    assert_eq!(e.handle_key(&ch("x")), EditOutcome::Edited);
    assert_eq!(e.text, "ab", "keys wait while the input method composes");

    // The commit: the composition ends, its text is typed.
    ime::set_preedit(None);
    e.handle_key(&ch("日本"));
    assert_eq!((e.text.as_str(), e.cursor), ("a日本b", 7));
    assert_eq!(e.display(), "a日本b");

    // A press drops a composition and asks for it to be cancelled.
    ime::set_preedit(Some(Preedit::new("か", None)));
    e.sync_ime();
    assert!(e.composing());
    e.press(0, false);
    assert!(!e.composing());
    assert!(ime::take_reset());
    assert_eq!(e.text, "a日本b");

    // Masked: bullets for the composition too.
    let mut m = LineEdit::masked();
    ime::set_preedit(Some(Preedit::new("ab", None)));
    m.sync_ime();
    assert_eq!(m.display(), "\u{2022}\u{2022}");
    assert_eq!(m.text, "");
    ime::set_preedit(None);
}

fn typed(e: &mut LineEdit, s: &str) {
    for c in s.chars() {
        e.handle_key(&ch(&c.to_string()));
    }
}

#[test]
fn typing_inserts_at_the_caret() {
    let mut e = LineEdit::default();
    typed(&mut e, "abc");
    assert_eq!(e.text, "abc");
    assert_eq!(e.cursor, 3);
}

/// The URL bar's defining behaviour: entering it selects everything, so
/// the next keystroke replaces the address rather than appending to it.
#[test]
fn typing_over_a_selection_replaces_it() {
    let mut e = LineEdit::with_text("https://example.com");
    e.select_all();
    typed(&mut e, "x");
    assert_eq!(e.text, "x");
    assert_eq!(e.selection, None);
}

#[test]
fn ctrl_a_selects_all_and_ctrl_u_clears() {
    let mut e = LineEdit::with_text("abc");
    e.handle_key(&ctrl("a"));
    assert_eq!(e.selection, Some((0, 3)));
    e.handle_key(&ctrl("u"));
    assert_eq!(e.text, "");
    assert_eq!(e.selection, None);
}

#[test]
fn backspace_deletes_a_selection_whole_or_one_char() {
    let mut e = LineEdit::with_text("abc");
    e.select_all();
    e.handle_key(&named(NamedKey::Backspace));
    assert_eq!(e.text, "");

    let mut e = LineEdit::with_text("abc");
    e.handle_key(&named(NamedKey::Backspace));
    assert_eq!(e.text, "ab");
}

/// Arrows collapse to the edge they move toward rather than stepping from
/// the caret — otherwise Left after a select-all lands in the wrong place.
#[test]
fn arrows_collapse_a_selection_to_its_edge() {
    let mut e = LineEdit::with_text("abc");
    e.select_all();
    e.handle_key(&named(NamedKey::ArrowLeft));
    assert_eq!((e.cursor, e.selection), (0, None));

    e.select_all();
    e.handle_key(&named(NamedKey::ArrowRight));
    assert_eq!((e.cursor, e.selection), (3, None));
}

#[test]
fn enter_and_escape_are_reported_not_swallowed() {
    let mut e = LineEdit::with_text("x");
    assert_eq!(e.handle_key(&named(NamedKey::Enter)), EditOutcome::Submit);
    assert_eq!(e.handle_key(&named(NamedKey::Escape)), EditOutcome::Cancel);
}

/// Multi-byte text must not be split mid-character.
#[test]
fn caret_moves_by_character_not_byte() {
    let mut e = LineEdit::with_text("é1");
    e.handle_key(&named(NamedKey::Home));
    e.handle_key(&named(NamedKey::ArrowRight));
    assert_eq!(e.cursor, 2, "é is two bytes");
    e.handle_key(&named(NamedKey::Backspace));
    assert_eq!(e.text, "1");
}

#[test]
fn a_drag_selects_from_the_press_to_the_pointer_either_way() {
    let mut e = LineEdit::with_text("hello world");
    e.press(2, false);
    assert!(e.dragging());
    assert_eq!((e.cursor, e.selection), (2, None), "a press alone selects nothing");
    assert!(e.drag_to(7));
    assert_eq!((e.cursor, e.selection), (7, Some((2, 7))));
    assert!(!e.drag_to(7), "no move, no repaint");
    // Back past the press: the selection flips to the other side of it.
    e.drag_to(0);
    assert_eq!((e.cursor, e.selection), (0, Some((0, 2))));
    // Back onto the press point: nothing selected, caret there.
    e.drag_to(2);
    assert_eq!((e.cursor, e.selection), (2, None));
    e.drag_to(11);
    e.release();
    assert!(!e.dragging());
    assert_eq!(e.selection, Some((2, 11)), "release keeps what was dragged");
    assert!(!e.drag_to(4), "motion after release is not a drag");
    assert_eq!(e.selection, Some((2, 11)));
}

#[test]
fn shift_click_extends_from_the_far_end() {
    let mut e = LineEdit::with_text("hello world");
    e.press(3, false);
    e.release();
    e.press(8, true);
    assert_eq!((e.cursor, e.selection), (8, Some((3, 8))));
    e.release();
    // Shift+click on the other side of the anchor keeps the anchor.
    e.press(1, true);
    assert_eq!((e.cursor, e.selection), (1, Some((1, 3))));
    e.release();
    // After a select-all the caret is at the end, so the start is kept.
    e.select_all();
    e.press(5, true);
    assert_eq!(e.selection, Some((0, 5)));
}

#[test]
fn a_dragged_selection_is_edited_like_any_other() {
    let mut e = LineEdit::with_text("hello world");
    e.press(0, false);
    e.drag_to(6);
    e.release();
    typed(&mut e, "big ");
    assert_eq!(e.text, "big world");
    assert_eq!(e.cursor, 4);
}

/// The app's offset is clamped into the text and never splits a char.
#[test]
fn pointer_offsets_land_on_char_boundaries() {
    let mut e = LineEdit::with_text("aé");
    e.press(2, false); // inside é's two bytes
    assert_eq!(e.cursor, 1);
    e.drag_to(99);
    assert_eq!((e.cursor, e.selection), (3, Some((1, 3))));
}

/// Each bullet is three bytes of display and one char of text.
#[test]
fn a_masked_field_maps_bullets_back_to_the_text() {
    let mut e = LineEdit::masked();
    typed(&mut e, "pé!");
    let bullet = '\u{2022}'.len_utf8();
    assert_eq!(e.text_index(0), 0);
    assert_eq!(e.text_index(bullet), 1);
    assert_eq!(e.text_index(2 * bullet), 3, "past é's two bytes");
    assert_eq!(e.text_index(3 * bullet), 4);
    assert_eq!(e.text_index(99), 4);
    assert_eq!(LineEdit::with_text("abc").text_index(2), 2, "unmasked is the identity");
    // And back: every text boundary round-trips through the bullets.
    for at in [0, 1, 3, 4] {
        assert_eq!(e.text_index(e.display_index(at)), at, "{at}");
    }
    assert_eq!(e.display_index(3), 2 * bullet);
    assert_eq!(LineEdit::with_text("abc").display_index(2), 2);
}

fn shifted(n: NamedKey) -> KeyEvent {
    KeyEvent { shift: true, ..named(n) }
}

#[test]
fn shift_arrows_grow_and_shrink_the_selection_from_its_anchor() {
    let mut e = LineEdit::with_text("hello");
    e.handle_key(&named(NamedKey::Home));
    e.handle_key(&shifted(NamedKey::ArrowRight));
    e.handle_key(&shifted(NamedKey::ArrowRight));
    assert_eq!((e.cursor, e.selection), (2, Some((0, 2))));
    e.handle_key(&shifted(NamedKey::ArrowLeft));
    assert_eq!((e.cursor, e.selection), (1, Some((0, 1))), "shrinks back toward the anchor");
    e.handle_key(&shifted(NamedKey::ArrowLeft));
    assert_eq!((e.cursor, e.selection), (0, None), "back on the anchor: nothing selected");
    // Past the anchor it flips to the other side.
    let mut e = LineEdit::with_text("hello");
    e.handle_key(&named(NamedKey::ArrowLeft)); // caret 4
    e.handle_key(&shifted(NamedKey::ArrowRight));
    e.handle_key(&shifted(NamedKey::ArrowLeft));
    e.handle_key(&shifted(NamedKey::ArrowLeft));
    assert_eq!((e.cursor, e.selection), (3, Some((3, 4))));
    // At the ends it stops.
    e.handle_key(&shifted(NamedKey::End));
    e.handle_key(&shifted(NamedKey::ArrowRight));
    assert_eq!((e.cursor, e.selection), (5, Some((4, 5))));
}

#[test]
fn shift_home_and_end_select_to_the_ends() {
    let mut e = LineEdit::with_text("hello world");
    e.press(6, false);
    e.release();
    e.handle_key(&shifted(NamedKey::End));
    assert_eq!((e.cursor, e.selection), (11, Some((6, 11))));
    e.handle_key(&shifted(NamedKey::Home));
    assert_eq!((e.cursor, e.selection), (0, Some((0, 6))), "the anchor stays where the caret was");
    // A select-all keeps its start; Shift+Left then trims its end.
    e.select_all();
    e.handle_key(&shifted(NamedKey::ArrowLeft));
    assert_eq!(e.selection, Some((0, 10)));
    // A plain arrow still collapses to the edge it points at.
    e.handle_key(&named(NamedKey::ArrowLeft));
    assert_eq!((e.cursor, e.selection), (0, None));
}

/// Multi-byte text: Shift+arrow steps a character, not a byte.
#[test]
fn shift_arrows_step_whole_characters() {
    let mut e = LineEdit::with_text("aé");
    e.handle_key(&shifted(NamedKey::ArrowLeft));
    assert_eq!((e.cursor, e.selection), (1, Some((1, 3))));
}

fn ctrl_key(n: NamedKey, shift: bool) -> KeyEvent {
    KeyEvent { ctrl: true, shift, ..named(n) }
}

#[test]
fn ctrl_arrows_jump_by_word() {
    let mut e = LineEdit::with_text("https://example.com/a_b  c");
    e.handle_key(&named(NamedKey::Home));
    let mut stops = Vec::new();
    for _ in 0..6 {
        e.handle_key(&ctrl_key(NamedKey::ArrowRight, false));
        stops.push(e.cursor);
    }
    assert_eq!(stops, [5, 15, 19, 23, 26, 26], "ends of https, example, com, a_b, c; then stays");
    let mut back = Vec::new();
    for _ in 0..6 {
        e.handle_key(&ctrl_key(NamedKey::ArrowLeft, false));
        back.push(e.cursor);
    }
    assert_eq!(back, [25, 20, 16, 8, 0, 0], "starts of c, a_b, com, example, https; then stays");
    // From inside a word: to that word's own edge.
    e.cursor = 11;
    e.handle_key(&ctrl_key(NamedKey::ArrowLeft, false));
    assert_eq!(e.cursor, 8);
}

#[test]
fn ctrl_shift_arrows_select_by_word() {
    let mut e = LineEdit::with_text("one two three");
    e.handle_key(&named(NamedKey::Home));
    e.handle_key(&ctrl_key(NamedKey::ArrowRight, true));
    e.handle_key(&ctrl_key(NamedKey::ArrowRight, true));
    assert_eq!((e.cursor, e.selection), (7, Some((0, 7))));
    e.handle_key(&ctrl_key(NamedKey::ArrowLeft, true));
    assert_eq!((e.cursor, e.selection), (4, Some((0, 4))), "shrinks a word back");
    // A plain Ctrl+arrow leaves from the selection's edge and drops it.
    e.handle_key(&ctrl_key(NamedKey::ArrowRight, false));
    assert_eq!((e.cursor, e.selection), (7, None));
    e.select_all();
    e.handle_key(&ctrl_key(NamedKey::ArrowLeft, false));
    assert_eq!((e.cursor, e.selection), (0, None));
}

#[test]
fn ctrl_arrows_in_a_password_go_to_the_ends() {
    let mut e = LineEdit::masked();
    typed(&mut e, "pass word");
    e.handle_key(&ctrl_key(NamedKey::ArrowLeft, false));
    assert_eq!(e.cursor, 0, "no stop at the space");
    e.handle_key(&ctrl_key(NamedKey::ArrowRight, true));
    assert_eq!(e.selection, Some((0, 9)));
}

#[test]
fn ctrl_backspace_and_delete_take_a_word() {
    let mut e = LineEdit::with_text("https://example.com/drag");
    e.handle_key(&ctrl_key(NamedKey::Backspace, false));
    assert_eq!((e.text.as_str(), e.cursor), ("https://example.com/", 20));
    e.handle_key(&ctrl_key(NamedKey::Backspace, false));
    assert_eq!((e.text.as_str(), e.cursor), ("https://example.", 16), "the / and com go together");
    // From inside a word: just its first half.
    let mut e = LineEdit::with_text("hello world");
    e.cursor = 8;
    e.handle_key(&ctrl_key(NamedKey::Backspace, false));
    assert_eq!((e.text.as_str(), e.cursor), ("hello rld", 6));
    // Ctrl+Delete: forward to the end of the word.
    e.cursor = 0;
    e.handle_key(&ctrl_key(NamedKey::Delete, false));
    assert_eq!((e.text.as_str(), e.cursor), (" rld", 0));
    // At the ends nothing happens.
    let mut e = LineEdit::with_text("abc");
    e.handle_key(&ctrl_key(NamedKey::Delete, false));
    assert_eq!(e.text, "abc");
    e.cursor = 0;
    e.handle_key(&ctrl_key(NamedKey::Backspace, false));
    assert_eq!(e.text, "abc");
}

#[test]
fn ctrl_backspace_takes_a_selection_not_a_word() {
    let mut e = LineEdit::with_text("one two three");
    e.press(4, false);
    e.drag_to(6);
    e.release();
    e.handle_key(&ctrl_key(NamedKey::Backspace, false));
    assert_eq!((e.text.as_str(), e.cursor), ("one o three", 4));
}

#[test]
fn ctrl_backspace_in_a_password_clears_to_the_start() {
    let mut e = LineEdit::masked();
    typed(&mut e, "pass word");
    e.handle_key(&named(NamedKey::ArrowLeft));
    e.handle_key(&named(NamedKey::ArrowLeft)); // before "rd"
    e.handle_key(&ctrl_key(NamedKey::Backspace, false));
    assert_eq!((e.text.as_str(), e.cursor), ("rd", 0), "no stop at the space");
}

#[test]
fn undo_walks_back_a_word_at_a_time_and_redo_forward() {
    let mut e = LineEdit::default();
    typed(&mut e, "hello world");
    assert!(e.can_undo());
    assert!(e.undo());
    assert_eq!((e.text.as_str(), e.cursor), ("hello ", 6), "the word after the space");
    assert!(e.undo());
    assert_eq!(e.text, "hello", "then the space");
    assert!(e.undo());
    assert_eq!((e.text.as_str(), e.cursor), ("", 0));
    assert!(!e.undo(), "nothing left");
    assert!(e.redo());
    assert!(e.redo());
    assert!(e.redo());
    assert_eq!((e.text.as_str(), e.cursor), ("hello world", 11));
    assert!(!e.redo());
}

#[test]
fn a_caret_move_or_click_splits_a_run() {
    let mut e = LineEdit::default();
    typed(&mut e, "abc");
    e.handle_key(&named(NamedKey::ArrowLeft));
    typed(&mut e, "X");
    e.undo();
    assert_eq!((e.text.as_str(), e.cursor), ("abc", 2), "X alone, caret back before c");
    let mut e = LineEdit::default();
    typed(&mut e, "abc");
    e.press(1, false);
    e.release();
    typed(&mut e, "X");
    e.undo();
    assert_eq!(e.text, "abc");
}

#[test]
fn deleting_runs_and_replacements_are_their_own_steps() {
    let mut e = LineEdit::with_text("hello world");
    for _ in 0..3 {
        e.handle_key(&named(NamedKey::Backspace));
    }
    assert_eq!(e.text, "hello wo");
    e.undo();
    assert_eq!(e.text, "hello world", "three Backspaces, one step");
    // Typing over a selection: the replacement is one step, then the
    // rest of the typed run another.
    e.select_all();
    typed(&mut e, "xyz");
    e.undo();
    assert_eq!(e.text, "x");
    e.undo();
    assert_eq!((e.text.as_str(), e.selection), ("hello world", Some((0, 11))), "the selection comes back too");
    // Ctrl word-delete is a step of its own.
    e.selection = None;
    e.handle_key(&ctrl_key(NamedKey::Backspace, false));
    e.handle_key(&ctrl_key(NamedKey::Backspace, false));
    assert_eq!(e.text, "");
    e.undo();
    assert_eq!(e.text, "hello ");
}

#[test]
fn a_new_edit_after_undo_drops_the_redo() {
    let mut e = LineEdit::default();
    typed(&mut e, "one two");
    e.undo();
    typed(&mut e, "six");
    assert!(!e.can_redo());
    assert!(!e.redo());
    assert_eq!(e.text, "one six");
}

#[test]
fn ctrl_y_redoes_and_keeps_the_rest_redoable() {
    let mut e = LineEdit::default();
    typed(&mut e, "one two three");
    e.undo();
    e.undo();
    e.undo();
    assert_eq!(e.text, "one ");
    assert_eq!(e.handle_key(&ctrl("y")), EditOutcome::Edited);
    assert_eq!(e.text, "one two");
    assert!(e.can_redo(), "a redo by key is not a new edit");
    e.handle_key(&ctrl("y"));
    e.handle_key(&ctrl("y"));
    assert_eq!(e.text, "one two three");
    assert_eq!(e.handle_key(&ctrl("y")), EditOutcome::Ignored, "nothing left to redo");
    assert_eq!(e.text, "one two three", "and no stray y typed");
    // Undo still steps back over what Ctrl+Y redid.
    e.undo();
    assert_eq!(e.text, "one two ");
}

#[test]
fn a_masked_field_keeps_no_history() {
    let mut e = LineEdit::masked();
    typed(&mut e, "hunter2");
    assert!(!e.can_undo());
    assert!(!e.undo());
    assert_eq!(e.text, "hunter2");
}

fn ms(t0: Instant, n: u64) -> Instant {
    t0 + Duration::from_millis(n)
}

fn click(e: &mut LineEdit, at: usize, now: Instant) {
    e.press_at(at, false, now);
    e.release();
}

#[test]
fn a_double_click_selects_the_word_and_a_triple_everything() {
    let t0 = Instant::now();
    let mut e = LineEdit::with_text("https://example.com/drag");
    click(&mut e, 10, t0);
    assert_eq!(e.selection, None);
    click(&mut e, 10, ms(t0, 150));
    assert_eq!((e.selection, e.cursor), (Some((8, 15)), 15), "example");
    click(&mut e, 10, ms(t0, 300));
    assert_eq!(e.selection, Some((0, 24)), "the third click takes the line");
    click(&mut e, 10, ms(t0, 450));
    assert_eq!((e.selection, e.cursor), (None, 10), "the fourth starts over");
}

#[test]
fn slow_or_moved_clicks_are_two_clicks() {
    let t0 = Instant::now();
    let mut e = LineEdit::with_text("hello world");
    click(&mut e, 2, t0);
    click(&mut e, 2, ms(t0, 500));
    assert_eq!(e.selection, None, "too slow");
    click(&mut e, 3, ms(t0, 600));
    assert_eq!(e.selection, None, "somewhere else");
    // Shift+click is never half of a double-click.
    let mut e = LineEdit::with_text("hello world");
    click(&mut e, 2, t0);
    e.press_at(2, true, ms(t0, 100));
    assert_eq!(e.selection, None);
    // Nor is a click after typing.
    let mut e = LineEdit::with_text("hello world");
    click(&mut e, 11, t0);
    typed(&mut e, "!");
    e.handle_key(&named(NamedKey::Backspace));
    click(&mut e, 11, ms(t0, 100));
    assert_eq!(e.selection, None);
}

#[test]
fn a_word_is_a_run_of_one_kind() {
    let e = LineEdit::with_text("https://example.com/a_b  c");
    assert_eq!(e.word_at(1), (0, 5), "https");
    assert_eq!(e.word_at(6), (5, 8), "the :// between words");
    assert_eq!(e.word_at(15), (8, 15), "just past a word's end is still the word");
    assert_eq!(e.word_at(21), (20, 23), "_ joins a word");
    assert_eq!(e.word_at(24), (23, 25), "a run of spaces");
    assert_eq!(e.word_at(26), (25, 26), "the end of the text");
    assert_eq!(LineEdit::default().word_at(0), (0, 0));
    // Multi-byte letters are letters.
    assert_eq!(LineEdit::with_text("é1 x").word_at(1), (0, 3));
}

#[test]
fn dragging_a_double_click_grows_by_words() {
    let t0 = Instant::now();
    let mut e = LineEdit::with_text("one two three four");
    click(&mut e, 5, t0);
    e.press_at(5, false, ms(t0, 100));
    assert_eq!(e.selection, Some((4, 7)), "two");
    e.drag_to(10); // into "three"
    assert_eq!((e.selection, e.cursor), (Some((4, 13)), 13));
    e.drag_to(6); // back inside "two": just the word again
    assert_eq!(e.selection, Some((4, 7)));
    e.drag_to(1); // into "one": from its start to the end of "two"
    assert_eq!((e.selection, e.cursor), (Some((0, 7)), 0));
    e.release();
    assert!(!e.drag_to(16));
}

#[test]
fn a_masked_double_click_selects_it_all() {
    let t0 = Instant::now();
    let mut e = LineEdit::masked();
    typed(&mut e, "pass word");
    click(&mut e, 2, t0);
    click(&mut e, 2, ms(t0, 100));
    assert_eq!(e.selection, Some((0, 9)), "no word boundaries in a password");
}

/// A password must not leave through a chord the user may not have meant.
#[test]
fn a_masked_field_hides_its_text_and_refuses_copy() {
    let mut e = LineEdit::masked();
    typed(&mut e, "hunter2");
    assert_eq!(e.display(), "•".repeat(7));
    assert_ne!(e.display(), e.text);
    e.select_all();
    assert_eq!(e.handle_key(&ctrl("c")), EditOutcome::Ignored);
    assert_eq!(e.handle_key(&ctrl("x")), EditOutcome::Ignored);
    assert_eq!(e.text, "hunter2", "cut must not have removed it");
}

/// A reader's edit replaces the text as the user's would: the caret at its end, a
/// selection dropped, one undo step back — and none for a masked field, which keeps no
/// history. The same text again changes nothing.
#[test]
fn a_readers_edit_replaces_the_text_undoably() {
    let mut e = LineEdit::with_text("old");
    e.selection = Some((0, 3));
    assert!(e.a11y_set_text("new.org"));
    assert_eq!((e.text.as_str(), e.cursor, e.selection), ("new.org", 7, None));
    assert!(!e.a11y_set_text("new.org"));
    assert!(e.undo());
    assert_eq!(e.text, "old");
    let mut p = LineEdit::masked();
    assert!(p.a11y_set_text("hunter2"));
    assert!(!p.undo(), "no history for a secret");
    assert_eq!(p.a11y_text(false).text, "\u{2022}".repeat(7));
}
