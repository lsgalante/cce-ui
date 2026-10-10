use super::*;

/// Every char 10 px at size 10, scaled with the size.
struct Fixed;
impl Measure for Fixed {
    fn width(&mut self, text: &str, size: f32, _font: &str, _attrs: TextAttrs) -> f32 {
        text.chars().count() as f32 * size
    }
}

fn theme() -> Theme {
    Theme { body_font: "sans-serif".into(), mono_font: "monospace".into(), size: 10.0 }
}

fn texts(l: &Layout) -> Vec<(String, f32, f32)> {
    l.draws
        .iter()
        .filter_map(|d| match d {
            Draw::Text { text, x, y, .. } => Some((text.clone(), *x, *y)),
            _ => None,
        })
        .collect()
}

#[test]
fn wraps_words_and_merges_runs() {
    // "aaa bbb ccc" at 10px/char: 110 px; at width 80 the third word wraps.
    let doc = blocks("aaa bbb ccc\n");
    let l = layout(&doc, 80.0, &theme(), &mut Fixed, &|_| true);
    let t = texts(&l);
    assert_eq!(t.len(), 2, "{t:?}");
    assert_eq!(t[0].0, "aaa bbb");
    assert_eq!(t[1].0, "ccc");
    assert_eq!(t[1].1, 0.0);
    assert!(t[1].2 > t[0].2);
    assert_eq!(l.height, 30.0);
}

#[test]
fn styles_split_runs_and_links_hit() {
    let doc = blocks("go **bold** to [[Note]] now\n");
    let l = layout(&doc, 1000.0, &theme(), &mut Fixed, &|_| true);
    let t: Vec<String> = texts(&l).into_iter().map(|t| t.0).collect();
    assert_eq!(t, ["go", "bold", "to", "Note", "now"]);
    let (r, hit) = &l.hits[0];
    assert_eq!(hit, &Hit::Link(SpanLink::Note { target: "Note".into(), subpath: None }));
    assert!(l.hit(r.x + 1.0, r.y + 1.0).is_some());
    assert!(l.hit(0.0, 1.0).is_none());
}

#[test]
fn punctuation_glued_to_a_styled_word_wraps_with_it() {
    // Code is 9 px/char. At width 80 (layout's minimum) "aaaa bbb"
    // fits (77 px) but "aaaa bbb," does not (87): unglued, the comma
    // alone would start the next line.
    let doc = blocks("aaaa `bbb`, cc\n");
    let l = layout(&doc, 80.0, &theme(), &mut Fixed, &|_| true);
    let t = texts(&l);
    let bbb = t.iter().find(|t| t.0 == "bbb").unwrap();
    let comma = t.iter().find(|t| t.0.starts_with(',')).unwrap();
    assert_eq!((bbb.1, comma.1), (0.0, 27.0), "{t:?}");
    assert!(bbb.2 > t[0].2, "{t:?}");
}

#[test]
fn long_word_breaks_by_chars() {
    let doc = blocks("abcdefghij\n");
    let l = layout(&doc, 80.0, &theme(), &mut Fixed, &|_| true);
    let t: Vec<String> = texts(&l).into_iter().map(|t| t.0).collect();
    assert_eq!(t, ["abcdefgh", "ij"]);
}

#[test]
fn tasks_hit_with_their_line_and_blocks_map_lines() {
    let doc = blocks("# Head\n\n- [ ] one\n- [x] two\n\nend\n");
    let l = layout(&doc, 400.0, &theme(), &mut Fixed, &|_| true);
    let tasks: Vec<_> = l.hits.iter().filter_map(|(_, h)| match h {
        Hit::Task { line, status } => Some((*line, *status)),
        _ => None,
    }).collect();
    assert_eq!(tasks, [(2, ' '), (3, 'x')]);
    assert_eq!(l.lines.iter().map(|(l, _)| *l).collect::<Vec<_>>(), [0, 2, 3, 5]);
    assert!(l.y_of_line(5) > l.y_of_line(2));
}

#[test]
fn image_embeds_draw_sized_and_others_stay_links() {
    let doc = blocks("![[a.png|200]]\n\n![[big.png]]\n\n![[Note]]\n");
    let image = |t: &str| (t != "Note").then_some(EmbedImage { id: 7, width: 1000, height: 500 });
    let l = layout_with(&doc, 400.0, &theme(), &mut Fixed, &|_| true, &image);
    let rects: Vec<Rect> = l.draws.iter().filter_map(|d| match d {
        Draw::Image { rect, .. } => Some(*rect),
        _ => None,
    }).collect();
    assert_eq!((rects[0].width, rects[0].height), (200.0, 100.0));
    // Wider than the column: scaled down whole.
    assert_eq!((rects[1].width, rects[1].height), (400.0, 200.0));
    assert_eq!(rects.len(), 2);
    assert!(texts(&l).iter().any(|(t, _, _)| t.contains("Note")));
}

#[test]
fn inline_images_flow_with_the_text_and_heighten_their_line() {
    let doc = blocks("ab ![[i.png|30]] cd\n\nnext\n");
    let image = |t: &str| (t == "i.png").then_some(EmbedImage { id: 1, width: 60, height: 40 });
    let l = layout_with(&doc, 1000.0, &theme(), &mut Fixed, &|_| true, &image);
    let img = l.draws.iter().find_map(|d| match d {
        Draw::Image { rect, .. } => Some(*rect),
        _ => None,
    });
    let img = img.expect("an inline image");
    // |30 keeps the aspect: 30 x 20, after "ab " (Fixed: 10 px a char).
    assert_eq!((img.width, img.height), (30.0, 20.0));
    let t = texts(&l);
    let ab = t.iter().find(|(s, _, _)| s == "ab").unwrap();
    let cd = t.iter().find(|(s, _, _)| s == "cd").unwrap();
    assert!(img.x > ab.1 && cd.1 > img.x + img.width, "{t:?} {img:?}");
    // The line grew for the image: text sits below its top.
    assert!(ab.2 > img.y);
    // Without the image the embed shows as its link: the file name, not
    // the size alias.
    let l2 = layout(&doc, 1000.0, &theme(), &mut Fixed, &|_| true);
    assert!(texts(&l2).iter().any(|(s, _, _)| s.contains("i.png")), "{:?}", texts(&l2));
    assert!(!texts(&l2).iter().any(|(s, _, _)| s.contains("30")));
    let next_with = t.iter().find(|(s, _, _)| s == "next").unwrap().2;
    let next_without = texts(&l2).iter().find(|(s, _, _)| s == "next").unwrap().2;
    assert!(next_with > next_without, "the taller line pushes what follows down");
}

#[test]
fn hard_breaks_and_empty_input() {
    let doc = blocks("one\ntwo\n");
    let l = layout(&doc, 400.0, &theme(), &mut Fixed, &|_| true);
    assert_eq!(texts(&l).len(), 2);
    assert_eq!(layout(&[], 400.0, &theme(), &mut Fixed, &|_| true).height, 0.0);
}
