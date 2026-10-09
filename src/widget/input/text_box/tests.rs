use super::*;

/// The multiline caret/click math reads shaped per-line offsets; a caret
/// must land exactly where it is drawn, on every column of every line.
/// A right-to-left word in a one-line box is set against the box's right edge, and
/// edited where it is drawn: the caret before each letter stands at its right edge, so
/// the offsets fall from the right end to the word's left; a click at the right end is
/// the start, at the word's left the end; the drawn text starts where the offsets do.
/// A selection from inside English into Hebrew is two pieces. The bidi levels come from
/// the text, so this holds whatever face draws the letters.
#[test]
fn a_right_to_left_word_is_edited_where_it_is_drawn() {
    let mut fs = crate::create_font_system();
    let mut tb = TextBox::new("שלום".to_string());
    tb.set_rect(10.0, 10.0, 300.0, 30.0);
    tb.prepare_text(&mut fs);
    let offs = tb.glyph_positions.clone();
    assert_eq!(offs.len(), 5, "four letters and the end");
    if tb.total_text_width == 0.0 {
        return; // nothing shaped (no fonts at all): nothing to check
    }
    let pad = tb.inner().pad();
    let (left, room) = (10.0 + pad, 300.0 - 2.0 * pad);
    assert!(offs.windows(2).all(|w| w[1] < w[0]), "carets fall right to left: {offs:?}");
    assert!((offs[0] - room).abs() < 0.01, "set against the right edge: {offs:?}");
    assert!((offs[4] - (room - tb.total_text_width)).abs() < 0.01, "{offs:?}");
    assert_eq!(tb.inner().map_x_to_idx(left + room), 0, "the right end is the start");
    assert_eq!(tb.inner().map_x_to_idx(left + offs[4]), 4, "the word's left is the end");
    let label = &tb.inner().value_labels()[0];
    assert!((label.x - (left + offs[4])).abs() < 0.01, "drawn where the offsets are: {} vs {}", label.x, left + offs[4]);

    // English then Hebrew: a left-to-right line, as it was; a selection from the "b" to
    // the Hebrew word's first letter is the b and that letter at the word's far right.
    let mut tb = TextBox::new("ab שלום".to_string());
    tb.set_rect(10.0, 10.0, 300.0, 30.0);
    tb.inner_mut().editing = true;
    tb.inner_mut().edit_buffer = "ab שלום".to_string();
    tb.prepare_text(&mut fs);
    assert!(tb.glyph_positions[0].abs() < 0.01, "a left-to-right line starts at the left");
    tb.inner_mut().select_anchor = Some(1);
    tb.inner_mut().cursor_idx = 4;
    let mut quads = Vec::new();
    tb.inner().selection_quads(10.0, 300.0, &mut quads);
    let sel: Vec<_> = quads.iter().filter(|q| q.2 > 2.0).collect();
    assert_eq!(sel.len(), 2, "two pieces: {sel:?}");
}

/// In a multiline box a Hebrew paragraph's lines are set against the right edge and an
/// English paragraph's are not; the drawn lines carry the same shift as the offsets.
#[test]
fn a_right_to_left_paragraph_is_set_against_the_right() {
    let mut fs = crate::create_font_system();
    let mut tb = TextBox::new("hello there\nשלום עולם".to_string()).with_multiline(true).with_line_wrap(true);
    tb.set_rect(10.0, 10.0, 300.0, 200.0);
    tb.prepare_text(&mut fs);
    let room = 300.0 - 2.0 * tb.inner().pad();
    let lines = tb.line_glyph_positions.clone();
    assert_eq!(lines.len(), 2);
    if lines[1].iter().all(|x| *x == 0.0) {
        return; // nothing shaped
    }
    assert!(lines[0][0].abs() < 0.01, "English starts at the left");
    let widest = lines[1].iter().copied().fold(0.0f32, f32::max);
    assert!((widest - room).abs() < 0.01, "Hebrew ends at the right: {:?}", lines[1]);
    let labels = tb.inner().value_labels();
    assert!(labels[1].x > labels[0].x + 50.0, "the Hebrew line is drawn shifted: {} vs {}", labels[1].x, labels[0].x);
}

/// A one-line box shows the START of an overflowing line: the left end of English, the
/// right end of Hebrew, whose characters run from the right. Editing follows the caret.
#[test]
fn an_overflowing_right_to_left_line_shows_its_start() {
    let mut fs = crate::create_font_system();
    let long = "שלום עולם ".repeat(8);
    let mut tb = TextBox::new(long).with_multiline(false);
    tb.set_rect(10.0, 10.0, 120.0, 28.0);
    tb.prepare_text(&mut fs);
    let room = 120.0 - 2.0 * tb.inner().pad();
    let total = tb.inner().total_text_width;
    if total <= room {
        return; // nothing shaped
    }
    assert!((tb.inner().scroll_x - (total - room)).abs() < 0.01, "scrolled to the right end: {} of {}", tb.inner().scroll_x, total - room);
    // The first character's stop lies in view.
    let first = tb.inner().glyph_positions[0];
    let x = first - tb.inner().scroll_x;
    assert!((0.0..=room + 0.5).contains(&x), "the line's first character is shown: {x}");

    let mut english = TextBox::new("hello there ".repeat(8)).with_multiline(false);
    english.set_rect(10.0, 10.0, 120.0, 28.0);
    english.prepare_text(&mut fs);
    assert_eq!(english.inner().scroll_x, 0.0, "English shows its left end");
}

/// A multiline box wraps by the shaped width of what it holds, not a count of chars:
/// every line fits the box (a hanging space aside) and none breaks early — the next
/// line's first word would not have fitted on it. Text of mixed widths (narrow, wide,
/// CJK) is where a column count and the drawn width disagree.
#[test]
fn a_multiline_box_wraps_where_its_text_is_wide() {
    let text = "iiii WWWW lll MMM 漢字漢字 iii WW 漢字漢字漢字 il WM 漢字".repeat(3);
    let mut tb = TextBox::new(text.clone()).with_multiline(true).with_line_wrap(true);
    crate::widget::WidgetHost::set_rect(&mut tb, 0.0, 0.0, 160.0, 400.0);
    let inner = tb.inner();
    let max_w = inner.wrap_width(160.0);
    let (lines, map) = inner.wrap_text(max_w);
    assert!(lines.len() > 2, "{lines:?}");
    let width = |s: &str| inner.char_advances(s).iter().sum::<f32>();
    for (i, line) in lines.iter().enumerate() {
        let shown = line.trim_end();
        assert!(width(shown) <= max_w + 0.5 || shown.chars().count() == 1, "line {i} {shown:?} is {} wide, past {max_w}", width(shown));
        if let Some(next) = lines.get(i + 1) {
            let word: String = next.chars().take_while(|c| !c.is_whitespace()).collect();
            if !word.is_empty() && line.ends_with(' ') {
                assert!(width(&format!("{line}{word}")) > max_w + 0.5, "line {i} {line:?} broke before {word:?}, which fitted");
            }
        }
    }
    let rejoined: String = lines.concat();
    assert_eq!(rejoined, text, "the lines are the text, cut");
    assert_eq!(map.len(), text.chars().count() + 1);
}

#[test]
fn multiline_shaped_offsets_round_trip() {
    let mut fs = cosmic_text::FontSystem::new();
    let mut tb = TextBox::new("wim wim wim\niiii WWWW mm ii".to_string())
        .with_multiline(true)
        .with_line_wrap(false);
    tb.set_rect(10.0, 10.0, 300.0, 100.0);
    tb.prepare_text(&mut fs);

    assert_eq!(tb.line_glyph_positions.len(), 2, "one offset row per line");
    for line in &tb.line_glyph_positions {
        for w in line.windows(2) {
            assert!(w[1] >= w[0], "offsets must be non-decreasing: {:?}", line);
        }
    }

    // Round-trip caret→pixel→column. Skipped in a font-less environment
    // (no glyphs shape, offsets stay zero — nothing to verify).
    for (l, line) in tb.line_glyph_positions.clone().iter().enumerate() {
        if line.last().copied().unwrap_or(0.0) == 0.0 {
            continue;
        }
        for c in 0..line.len() {
            assert_eq!(
                tb.line_x_to_col(l, tb.line_col_x(l, c)),
                c,
                "line {l} col {c} must round-trip"
            );
        }
    }
}

/// Single-line offsets must be non-decreasing for MULTI-WORD text. Guards
/// the `normalized_glyph_starts` workaround for cosmic-text 0.12's
/// `Shaping::Basic` bug (span-relative `LayoutGlyph::start`, resetting at
/// every word): without it, each word's glyphs overwrite the low columns
/// and a mid-text caret lands inside the wrong word.
#[test]
fn single_line_multi_word_offsets_monotonic() {
    let mut fs = cosmic_text::FontSystem::new();
    let mut tb = TextBox::new("wim wim wim".to_string());
    tb.set_rect(10.0, 10.0, 300.0, 30.0);
    tb.prepare_text(&mut fs);

    for w in tb.glyph_positions.windows(2) {
        assert!(w[1] >= w[0], "offsets must be non-decreasing: {:?}", tb.glyph_positions);
    }
    // In a font-bearing environment the mid-text columns are real advances:
    // strictly inside (0, total). Skipped font-less (all zeros).
    if tb.glyph_positions.last().copied().unwrap_or(0.0) > 0.0 {
        let total = *tb.glyph_positions.last().unwrap();
        for (i, &x) in tb.glyph_positions.iter().enumerate().skip(1).take(tb.glyph_positions.len() - 2) {
            assert!(x > 0.0 && x < total, "col {i} offset {x} must sit inside the text run");
        }
    }
}

/// Multi-byte text gets one offset per CHAR (not per byte), and the
/// per-frame memo notices a change of text: the second shape must not
/// serve the first one's offsets.
#[test]
fn offsets_count_chars_and_reshape_on_change() {
    let mut fs = cosmic_text::FontSystem::new();
    let mut tb = TextBox::new("héllo wörld".to_string());
    tb.set_rect(10.0, 10.0, 300.0, 30.0);
    tb.prepare_text(&mut fs);
    assert_eq!(tb.glyph_positions.len(), "héllo wörld".chars().count() + 1);
    for w in tb.glyph_positions.windows(2) {
        assert!(w[1] >= w[0], "offsets must be non-decreasing: {:?}", tb.glyph_positions);
    }

    let first = tb.glyph_positions.clone();
    tb.prepare_text(&mut fs);
    assert_eq!(tb.glyph_positions, first, "an unchanged box keeps its offsets");

    tb.text = "hé".to_string();
    tb.prepare_text(&mut fs);
    assert_eq!(tb.glyph_positions.len(), 3, "a changed text is reshaped");
}

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

#[test]
fn test_textbox_line_wrap_disabled_horizontal_scrolling() {
    let _dummy = crate::context::UiContext::new();

    // Wrap is stated on the widget (`line_wrap_override`), not on the
    // process-global `textbox_line_wrap` — this box is single-line, which
    // is already unwrapped, and the override says so whatever the config
    // does. The global would be visible to every other test in parallel.
    let mut tb = TextBox::new("Very long text that should not wrap and instead scroll horizontally".to_string())
        .with_line_wrap(false);
    tb.set_rect(10.0, 10.0, 100.0, 30.0);
    assert!(!tb.line_wrap_enabled());

    assert_eq!(tb.scroll_x, 0.0);

    tb.focus();
    tb.cursor_idx = tb.edit_buffer.chars().count();
    tb.scroll_to_cursor();

    assert!(tb.scroll_x > 0.0, "scroll_x should be scrolled horizontally to keep the cursor visible");
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

/// A single-line box's border is always the hairline; a multiline box's
/// is whatever `textbox_multiline_border_width` resolves to. Read, never
/// written: that width is a process global and the suite runs in
/// parallel, so setting it here would widen every other test's borders.
#[test]
fn test_multiline_textbox_border_width() {
    let _dummy = crate::context::UiContext::new();
    let tb_single = TextBox::new("Singleline".to_string()).with_multiline(false);
    let tb_multi = TextBox::new("Multiline".to_string()).with_multiline(true);

    let configured = crate::layout::textbox_multiline_border_width();
    assert_eq!(tb_single.border_width(), 1.0);
    assert_eq!(tb_multi.border_width(), configured);
}
/// A tall recessed box's wall is the full `bevel_width`, deeper than the
/// old fixed 8px inset: its text starts on the well's floor, past the
/// wall, not on it (cce-fonts' preview box drew its sample over its relief).
#[test]
fn tall_box_text_starts_past_its_relief_wall() {
    let _dummy = crate::context::UiContext::new();
    let mut tb = TextBox::new("The quick brown fox".to_string())
        .with_multiline(true)
        .with_recessed(true);
    tb.set_rect(10.0, 10.0, 400.0, 180.0);
    let field = tb.well().expect("a recessed box with rounded corners carves a well");
    let floor_x = field.rect.x + field.depth;
    let floor_y = field.rect.y + field.depth;
    let first = &tb.value_labels()[0];
    assert!(first.x >= floor_x, "text x {} is on the wall (floor at {})", first.x, floor_x);
    assert!(first.y >= floor_y, "text y {} is on the wall (floor at {})", first.y, floor_y);
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
