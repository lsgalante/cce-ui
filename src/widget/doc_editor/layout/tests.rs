use super::*;
use crate::widget::doc_editor::preview::{style_line, Context};

fn lay(text: &str, active: bool, width: f32) -> LineLayout {
    let th = EditorTheme::new(14.0);
    let mut m = ShapingMeasure::new(false);
    let line = style_line(text, Context::Normal, active);
    layout_line(text, &line, active, width, &th, &mut m)
}

#[test]
fn every_shown_byte_has_an_x_and_clicks_come_back() {
    let t = "plain **bold** and [[Link|shown]] end";
    for active in [false, true] {
        let l = lay(t, active, 2000.0);
        assert_eq!(l.rows, 1);
        let mut last = -1.0;
        for r in &l.runs {
            for (b, x) in &r.xs {
                assert!(t.is_char_boundary(*b));
                let ax = r.x + x;
                assert!(ax >= last - 0.01, "x runs backwards at {b}: {ax} < {last}");
                last = ax;
            }
        }
        // Every visible boundary maps to a caret x that maps back to it.
        for r in &l.runs {
            for (b, _) in &r.xs {
                let (x, row) = l.caret_xy(*b);
                let back = l.col_at(x, row);
                let (x2, _) = l.caret_xy(back);
                assert!((x - x2).abs() < 0.5, "{b} -> {x} -> {back}");
            }
        }
    }
}

/// A Hebrew word in a Latin line: its carets fall from its right end, a click on its
/// right edge is its first letter, and a selection from inside the Latin into it is two
/// rects, the Latin end and the Hebrew word's right side (its first letters).
#[test]
fn right_to_left_words_are_laid_out_and_selected_where_they_are() {
    let t = "ab שלום cd";
    let l = lay(t, false, 2000.0);
    let w0 = t.find('ש').unwrap();
    let w_end = w0 + "שלום".len();
    let (first, _) = l.caret_xy(w0);
    let (last, _) = l.caret_xy(w0 + 6);
    assert!(first > last, "the word's first letter is right of its last: {first} vs {last}");
    assert_eq!(l.col_at(first, 0), w0, "a click at its right edge is its first letter");
    let rects = l.selection_rects(1, w0 + 2, false);
    assert_eq!(rects.len(), 2, "the Latin end and the word's right side: {rects:?}");
    let (x_b, _) = l.caret_xy(1);
    assert!((rects[0].1 - x_b).abs() < 0.5, "the first starts at the b: {rects:?}");
    assert!(rects[1].2 <= first + 0.5 && rects[1].1 > last, "the second is inside the word, at its right");
    let whole = l.selection_rects(0, w_end, false);
    assert_eq!(whole.len(), 1, "a range ending at the word's end is one strip: {whole:?}");
}

/// A Hebrew paragraph is set against the right edge with its runs placed from the
/// right: its first word rightmost, a bold word after it to its left. In an English line,
/// two Hebrew runs side by side swap places (the right-to-left sequence reads from the
/// right), the English around them staying where it was.
#[test]
fn styled_runs_are_placed_in_visual_order() {
    let width = 600.0;
    let rtl = "שלום **עולם** טוב";
    let l = lay(rtl, false, width);
    let run_of = |l: &LineLayout, needle: &str| l.runs.iter().find(|r| r.text.contains(needle)).map(|r| (r.x, r.x + r.w)).unwrap();
    let (first_x0, first_x1) = run_of(&l, "שלום");
    let (bold_x0, bold_x1) = run_of(&l, "עולם");
    let (last_x0, _) = run_of(&l, "טוב");
    assert!((first_x1 - width).abs() < 1.0, "set against the right edge: {first_x1}");
    assert!(bold_x1 <= first_x0 + 0.5 && last_x0 < bold_x0, "placed from the right: {:?}", l.runs.iter().map(|r| (&r.text, r.x)).collect::<Vec<_>>());

    let mixed = "say שלום **עולם** now";
    let m = lay(mixed, false, width);
    let (say_x0, _) = run_of(&m, "say");
    let (shalom_x0, _) = run_of(&m, "שלום");
    let (olam_x0, olam_x1) = run_of(&m, "עולם");
    let (now_x0, _) = run_of(&m, "now");
    assert!(say_x0 < olam_x0 && olam_x1 <= shalom_x0 + 0.5 && shalom_x0 < now_x0, "the Hebrew pair swapped, between the English: {:?}", m.runs.iter().map(|r| (&r.text, r.x)).collect::<Vec<_>>());
    assert!(say_x0 < 50.0, "a left-to-right line still starts at the left");
}

#[test]
fn long_lines_wrap_with_a_hanging_list_indent() {
    let t = "- one two three four five six seven eight nine ten eleven twelve";
    let l = lay(t, false, 160.0);
    assert!(l.rows > 2, "rows {}", l.rows);
    let first_x: Vec<f32> = (0..l.rows).map(|row| l.runs.iter().find(|r| r.row == row).unwrap().x).collect();
    assert!(first_x.iter().all(|x| (*x - l.content_x).abs() < 0.5), "{first_x:?} vs {}", l.content_x);
    assert!(matches!(l.decos[0], Deco::Dot { .. }));
}

#[test]
fn the_active_lines_marker_ends_where_the_content_starts() {
    let t = "- [ ] task text";
    let off = lay(t, false, 1000.0);
    let on = lay(t, true, 1000.0);
    let text_x = |l: &LineLayout| l.caret_xy(6).0;
    assert!((text_x(&off) - text_x(&on)).abs() < 1.5, "{} vs {}", text_x(&off), text_x(&on));
    assert!(off.task.is_some() && matches!(off.decos[0], Deco::Check { checked: false, .. }));
    // Hidden prefix: a click left of the text lands at its start.
    assert_eq!(off.col_at(0.0, 0), 6);
}

#[test]
fn empty_and_link_lines() {
    let l = lay("", false, 500.0);
    assert_eq!((l.rows, l.caret_xy(0)), (1, (0.0, 0)));
    let t = "see [[Target]]";
    let l = lay(t, false, 500.0);
    let r = l.runs.iter().find(|r| r.link.is_some()).unwrap();
    assert_eq!(l.link_at(r.x + 2.0, 2.0), Some(0));
    assert_eq!(l.link_at(1.0, 2.0), None);
}
