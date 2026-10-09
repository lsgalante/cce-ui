use super::*;
use crate::widget::ElementState;

fn key(k: Key, text: Option<&str>, ctrl: bool, shift: bool) -> KeyEvent {
    KeyEvent { state: ElementState::Pressed, logical_key: k, text: text.map(String::from), repeat: false, ctrl, shift, alt: false }
}

fn typed(c: &str) -> KeyEvent {
    key(Key::Character(c.into()), Some(c), false, false)
}

fn named(n: NamedKey) -> KeyEvent {
    key(Key::Named(n), None, false, false)
}

fn editor(text: &str) -> DocEditor {
    let mut e = DocEditor::new(text, EditorTheme::new(14.0), false);
    let mut pc = PaintCtx::new();
    e.paint(&mut pc, Rect { x: 0.0, y: 0.0, width: 600.0, height: 400.0 }, true);
    e
}

/// A composition is in the caret line's layout and never in the buffer:
/// drawn as typed and underlined, the caret where the input method has
/// its cursor, columns read off the line mapped across it; keys wait
/// while it is up; the commit is typed; a press drops it, asks the input
/// method to cancel, and places the caret; the caret is reported.
#[test]
fn a_composition_is_laid_out_in_place_and_never_held() {
    use crate::ime::{self, Preedit};
    let rect = Rect { x: 0.0, y: 0.0, width: 600.0, height: 400.0 };
    let mut e = editor("ab\ncd");
    e.buf.caret = Pos::new(0, 1);
    e.buf.anchor = None;
    let mut pc = PaintCtx::new();
    e.paint(&mut pc, rect, true);
    let plain_caret = e.caret_rect().x;
    let _ = ime::take_reset();

    ime::set_preedit(Some(Preedit::new("にほ", Some((3, 3)))));
    ime::begin_frame();
    e.paint(&mut pc, rect, true);
    ime::end_frame();
    assert_eq!(e.text(), "ab\ncd", "never in the buffer");
    let l = e.layouts[0].as_ref().unwrap();
    let drawn: String = l.runs.iter().map(|r| r.text.as_str()).collect();
    assert_eq!(drawn, "aにほb");
    assert!(e.caret_rect().x > plain_caret, "the caret is after に");
    assert!(ime::caret().is_some(), "the caret is reported");
    // Past the composition on its line is the source's end of line.
    assert_eq!(e.pos_at(rect.x + 590.0, rect.y + e.pad + 2.0), Pos::new(0, 2));
    assert_eq!(e.key(&typed("x")), Response::None);
    assert_eq!(e.text(), "ab\ncd");

    // The commit: the composition ends, its text is typed.
    ime::set_preedit(None);
    assert_eq!(e.key(&typed("日本")), Response::Changed);
    assert_eq!(e.text(), "a日本b\ncd");
    assert_eq!(e.buf.caret, Pos::new(0, 7));

    // A press drops a composition, asks for it to be cancelled, and
    // places the caret.
    ime::set_preedit(Some(Preedit::new("か", None)));
    e.paint(&mut pc, rect, true);
    assert!(e.composing());
    e.press(rect.x + 590.0, rect.y + e.pad + 2.0, false, false);
    e.release();
    assert!(!e.composing());
    assert!(ime::take_reset());
    assert_eq!(e.text(), "a日本b\ncd");
    assert_eq!(e.buf.caret, Pos::new(0, "a日本b".len()));
}

#[test]
fn an_embed_line_shows_its_image_and_the_raw_text_with_the_caret() {
    let mut e = DocEditor::new("top\n![[pic.png|200]]\nend", EditorTheme::new(14.0), false);
    e.set_images(Box::new(|t: &str| (t == "pic.png").then_some(EmbedImage { id: 3, width: 800, height: 400 })));
    let rect = Rect { x: 0.0, y: 0.0, width: 600.0, height: 400.0 };
    let mut pc = PaintCtx::new();
    e.paint(&mut pc, rect, true);
    let l = e.layouts[1].as_ref().unwrap();
    assert!(l.runs.is_empty(), "inactive: the image alone");
    assert!(matches!(&l.decos[0], Deco::Image { rect, .. } if rect.width == 200.0 && rect.height == 100.0));
    assert_eq!(l.height, 100.0 + 2.0 * layout::IMAGE_PAD);
    // The caret on it: raw text, image below.
    e.buf.caret = Pos::new(1, 0);
    e.buf.anchor = None;
    e.paint(&mut pc, rect, true);
    let l = e.layouts[1].as_ref().unwrap();
    assert!(!l.runs.is_empty());
    assert!(l.decos.iter().any(|d| matches!(d, Deco::Image { .. })));
    // Source mode: just text.
    e.set_preview(false);
    e.paint(&mut pc, rect, true);
    assert!(!e.layouts[1].as_ref().unwrap().decos.iter().any(|d| matches!(d, Deco::Image { .. })));
}

#[test]
fn embeds_inside_a_line_show_below_it() {
    let mut e = DocEditor::new("text ![[a.png|100]] and ![[b.png|100]] more\nend", EditorTheme::new(14.0), false);
    e.set_images(Box::new(|_| Some(EmbedImage { id: 3, width: 200, height: 100 })));
    let mut pc = PaintCtx::new();
    e.buf.caret = Pos::new(1, 0);
    e.paint(&mut pc, Rect { x: 0.0, y: 0.0, width: 600.0, height: 400.0 }, true);
    let l = e.layouts[0].as_ref().unwrap();
    assert!(!l.runs.is_empty(), "the text stays");
    let rects: Vec<Rect> = l.decos.iter().filter_map(|d| match d {
        Deco::Image { rect, .. } => Some(*rect),
        _ => None,
    }).collect();
    assert_eq!(rects.len(), 2);
    assert_eq!((rects[0].width, rects[0].height), (100.0, 50.0));
    assert!(rects[1].x > rects[0].x && rects[1].y == rects[0].y, "side by side");
    assert!(l.height >= l.row_h + 50.0);
}

#[test]
fn typing_enter_and_list_continuation() {
    let mut e = editor("");
    for c in ["-", " ", "[", " ", "]", " ", "a"] {
        e.key(&typed(c));
    }
    assert_eq!(e.key(&named(NamedKey::Enter)), Response::Changed);
    e.key(&typed("b"));
    assert_eq!(e.text(), "- [ ] a\n- [ ] b");
    e.key(&named(NamedKey::Enter));
    // Enter on an empty item ends the list.
    e.key(&named(NamedKey::Enter));
    assert_eq!(e.text(), "- [ ] a\n- [ ] b\n");
    let mut e = editor("3. three");
    e.buf.caret = Pos::new(0, 8);
    e.key(&named(NamedKey::Enter));
    assert_eq!(e.text(), "3. three\n4. ");
}

#[test]
fn tab_indents_list_items_and_undo_works() {
    let mut e = editor("- a\n- b");
    e.buf.caret = Pos::new(1, 3);
    e.key(&named(NamedKey::Tab));
    assert_eq!(e.text(), "- a\n\t- b");
    assert_eq!(e.buf.caret, Pos::new(1, 4));
    e.key(&key(Key::Named(NamedKey::Tab), None, false, true));
    assert_eq!(e.text(), "- a\n- b");
    assert!(e.undo());
    assert_eq!(e.text(), "- a\n\t- b");
}

#[test]
fn arrows_words_home_end_and_vertical() {
    let mut e = editor("- [ ] first item\nsecond");
    e.buf.caret = Pos::new(0, 10);
    e.key(&named(NamedKey::Home));
    assert_eq!(e.buf.caret, Pos::new(0, 6), "smart home: the content start");
    e.key(&named(NamedKey::Home));
    assert_eq!(e.buf.caret, Pos::new(0, 0));
    e.key(&named(NamedKey::End));
    assert_eq!(e.buf.caret, Pos::new(0, 16));
    e.key(&named(NamedKey::ArrowDown));
    assert_eq!(e.buf.caret.line, 1);
    e.key(&key(Key::Named(NamedKey::ArrowLeft), None, true, true));
    assert_eq!(e.buf.selected_text().as_deref(), Some("second"));
}

#[test]
fn clicks_place_the_caret_and_toggle_tasks() {
    let mut e = editor("- [ ] todo\nplain line");
    // Line 0 is not active (the caret is on it? it starts at 0,0 — move it).
    e.buf.caret = Pos::new(1, 0);
    let mut pc = PaintCtx::new();
    e.paint(&mut pc, Rect { x: 0.0, y: 0.0, width: 600.0, height: 400.0 }, true);
    let l = e.layouts[0].as_ref().unwrap();
    let (r, _) = l.task.unwrap();
    let (sx, sy) = (e.origin.0 + r.x + r.width / 2.0, e.origin.1 + r.y + r.height / 2.0);
    assert_eq!(e.press(sx, sy, false, false), Response::Changed);
    assert_eq!(e.text(), "- [x] todo\nplain line");
    assert_eq!(e.buf.caret, Pos::new(1, 0), "ticking does not move the caret");
    // A click at the far left of the second line puts the caret there.
    let y1 = e.origin.1 + e.tops[1] + 3.0;
    e.release();
    e.press(e.origin.0 + 1.0, y1, false, false);
    assert_eq!(e.buf.caret, Pos::new(1, 0));
}

#[test]
fn rendered_links_follow() {
    let mut e = editor("see [[Target|it]] now\nsecond");
    e.buf.caret = Pos::new(1, 0);
    let mut pc = PaintCtx::new();
    e.paint(&mut pc, Rect { x: 0.0, y: 0.0, width: 600.0, height: 400.0 }, true);
    let l = e.layouts[0].as_ref().unwrap();
    let run = l.runs.iter().find(|r| r.link.is_some()).unwrap();
    let (sx, sy) = (e.origin.0 + run.x + 2.0, e.origin.1 + 4.0);
    assert_eq!(e.press(sx, sy, false, false), Response::Follow(Target::Note { target: "Target".into(), subpath: None }));
}

#[test]
fn the_frontmatter_is_a_table_until_the_caret_enters_it() {
    let mut e = editor("---\ndone: false\ntags: [x]\n---\nbody");
    e.buf.caret = Pos::new(4, 0);
    let mut pc = PaintCtx::new();
    e.paint(&mut pc, Rect { x: 0.0, y: 0.0, width: 600.0, height: 400.0 }, true);
    let header = e.layouts[0].as_ref().unwrap();
    assert!(matches!(&header.decos[0], Deco::Text { text, .. } if text == "Properties"));
    let row = e.layouts[1].as_ref().unwrap();
    let (r, _, on) = row.toggle.clone().expect("a checkbox");
    assert!(!on);
    // Ticking the box writes `true`, and the caret stays put.
    let (sx, sy) = (e.origin.0 + r.x + r.width / 2.0, e.origin.1 + e.tops[1] + r.y + r.height / 2.0);
    assert_eq!(e.press(sx, sy, false, false), Response::Changed);
    assert_eq!(e.text(), "---\ndone: true\ntags: [x]\n---\nbody");
    assert_eq!(e.buf.caret, Pos::new(4, 0));
    e.release();
    // The pill is padded inside the value column.
    e.paint(&mut pc, Rect { x: 0.0, y: 0.0, width: 600.0, height: 400.0 }, true);
    let tags = e.layouts[2].as_ref().unwrap();
    assert_eq!(tags.runs.len(), 1);
    assert!(tags.runs[0].look.pill && tags.runs[0].x >= tags.content_x + layout::PILL_PAD - 0.01);
    // The caret in the block shows all of it raw.
    e.buf.set_caret(Pos::new(2, 0), false);
    e.paint(&mut pc, Rect { x: 0.0, y: 0.0, width: 600.0, height: 400.0 }, true);
    for i in 0..4 {
        let l = e.layouts[i].as_ref().unwrap();
        assert_eq!(l.runs.iter().map(|r| r.text.as_str()).collect::<String>(), e.buf.line(i), "line {i} raw");
    }
}

#[test]
fn a_long_document_shapes_only_what_shows() {
    let text: String = (0..5000).map(|i| format!("line {i} with **some** text\n")).collect();
    let e = editor(&text);
    let laid = e.layouts.iter().filter(|l| l.is_some()).count();
    assert!(laid < 60, "{laid} lines shaped");
    assert!(e.content_height() > 5000.0 * 14.0);
}
