//! Glyph offsets, right-to-left text, and wrapping by shaped width.

use super::*;

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

/// The multiline caret/click math reads shaped per-line offsets; a caret
/// must land exactly where it is drawn, on every column of every line.
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
