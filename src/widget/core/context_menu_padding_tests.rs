use super::context_menu::{self, split_mark, ContextMenuState, PAD, ROW_H};
use crate::widget::WidgetId;

/// The shared menu is a popover for the window-drag question too: a
/// press on one of its rows must never start a window move, whatever
/// sits under the menu. It has no widget id to register, so the veto
/// asks the thread-local directly. And the free `row_at` is the
/// PAD-aware row hit test hosts dispatch by.
#[test]
fn an_open_menu_vetoes_window_drags_under_it() {
    let ctx = crate::context::UiContext::new();
    context_menu::show(100.0, 200.0, vec!["Copy".into(), "Paste".into()], 0, WidgetId(1));
    assert!(!ctx.drag_allowed_at(110.0, 200.0 + PAD + ROW_H * 0.5), "a press on a row is not a drag");
    assert_eq!(context_menu::row_at(110.0, 200.0 + PAD + ROW_H * 1.5), Some(1));
    assert_eq!(context_menu::row_y(1), 200.0 + PAD + ROW_H);
    assert!(ctx.drag_allowed_at(10.0, 10.0), "away from the menu the drag question is the widgets'");
    context_menu::hide();
    assert!(ctx.drag_allowed_at(110.0, 200.0 + PAD + ROW_H * 0.5), "hidden, it vetoes nothing");
}

/// A row's leading mark is a glyph, not a character: the label is drawn
/// without it and after the glyph's room, the glyph is the mark's
/// (`check`, `circle`, `circle-outline`), and a page row's chevron and a
/// page's back chevron are glyphs too — no row draws "✓", "●", "○", "›"
/// or "‹" as text.
#[test]
fn menu_marks_and_chevrons_are_glyphs() {
    let mut m = ContextMenuState::new();
    m.show(0.0, 0.0, vec!["✓ Show Grid".into(), "● On".into(), "○ Off".into(), "Plain".into(), "Add Tab".into()], 0, WidgetId(1));
    m.set_row_page(4);
    let labels = m.labels();
    let texts: Vec<&str> = labels.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(texts, ["Show Grid", "On", "Off", "Plain", "Add Tab"]);
    assert!(labels[0].x > labels[3].x, "a marked label stands after its mark's room");
    let names: Vec<&str> = m.glyphs().iter().map(|g| g.name).collect();
    assert_eq!(names, ["check", "circle", "circle-outline", "chevron-right"]);
    m.show_page(0.0, 0.0, Some("View"), vec!["○ Wireframe".into()], 0, WidgetId(1));
    assert_eq!(m.labels()[0].text, "View", "the back band reads its title, its chevron a glyph");
    assert_eq!(m.glyphs().iter().map(|g| g.name).collect::<Vec<_>>(), ["chevron-left", "circle-outline"]);
    for l in m.labels() {
        assert!(!l.text.contains(['✓', '●', '○', '›', '‹']), "{:?} draws a mark as text", l.text);
    }
    assert_eq!(split_mark("✓ Collapse controls"), (Some("check"), "Collapse controls"));
    assert_eq!(split_mark("Collapse controls"), (None, "Collapse controls"));
}

/// Every glyph the toolkit draws by name is in the icon set: a name
/// with no file draws NOTHING (a missing icon is not an error at paint),
/// so the miss is caught here. Skipped where the icon set is not checked
/// out beside the crate.
#[test]
fn every_glyph_the_toolkit_names_is_in_the_icon_set() {
    let dir = std::path::Path::new(&crate::icons_dir()).to_path_buf();
    if !dir.is_dir() {
        eprintln!("skipped: no icon set at {}", dir.display());
        return;
    }
    let mut names = std::collections::BTreeSet::new();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut stack = vec![root];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            if p.extension().is_none_or(|x| x != "rs") {
                continue;
            }
            let src = std::fs::read_to_string(&p).unwrap();
            for call in [".icon(\"", "upload_icon(\"", "upload_icon_tinted(\"", "with_icon_name(\"", "new_icon(\""] {
                for (i, _) in src.match_indices(call) {
                    let rest = &src[i + call.len()..];
                    if let Some(end) = rest.find('"') {
                        let n = &rest[..end];
                        if !n.is_empty() && n.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
                            names.insert(n.to_string());
                        }
                    }
                }
            }
        }
    }
    for g in ["check", "circle", "circle-outline", "chevron-left", "chevron-right", "chevron-up", "chevron-down"] {
        names.insert(g.to_string());
    }
    let missing: Vec<_> = names.iter().filter(|n| !dir.join(format!("{n}.svg")).is_file()).collect();
    assert!(missing.is_empty(), "named but not in {}: {missing:?}", dir.display());
}

/// The plate pads its rows evenly: the height is the rows plus a pad
/// above and below, the labels sit one pad in from the left with the
/// widest one a pad from the right, and the padding is plate — a pointer
/// in it hovers no row, and a pointer a row down from the top pad is on
/// row 1, not row 0 plus a fraction.
#[test]
fn rows_sit_inside_an_even_pad() {
    let mut m = ContextMenuState::new();
    m.show(100.0, 200.0, vec!["Hide Geometry".into(), "-".into(), "Delete".into()], 0, WidgetId(1));
    assert_eq!(m.h, 3.0 * ROW_H + 2.0 * PAD);
    assert!(m.w >= 2.0 * PAD);
    let labels = m.text_labels();
    assert!(labels.iter().all(|l| l.x == 100.0 + PAD), "labels start one pad in");
    assert_eq!(labels[0].y, 200.0 + PAD + (ROW_H - labels[0].font_size) / 2.0, "row 0 starts under the top pad");

    assert_eq!(m.row_at(110.0, 200.0 + PAD * 0.5), None, "the top pad is no row");
    assert_eq!(m.row_at(110.0, 200.0 + PAD + ROW_H * 0.5), Some(0));
    assert_eq!(m.row_at(110.0, 200.0 + PAD + ROW_H * 2.5), Some(2));
    assert_eq!(m.row_at(110.0, 200.0 + m.h - PAD * 0.5), None, "the bottom pad is no row");

    m.cursor_moved(110.0, 200.0 + PAD * 0.5);
    assert_eq!(m.hovered_item, None);
    m.cursor_moved(110.0, 200.0 + PAD + ROW_H * 1.5);
    assert_eq!(m.hovered_item, None, "a separator row never hovers");
    m.cursor_moved(110.0, 200.0 + PAD + ROW_H * 2.5);
    assert_eq!(m.hovered_item, Some(2));
}

/// A label with a glyph the menu face lacks — the radio marks the
/// designer's pin rows carry — is measured as the renderer shapes it,
/// fallback face and all, so the plate is wide enough for what is drawn.
/// The SVG-inked measure alone called the mark next to nothing.
#[test]
fn a_fallback_glyph_widens_the_plate_as_drawn() {
    let mut m = ContextMenuState::new();
    m.show(0.0, 0.0, vec!["● Follow Active Editor".into()], 0, WidgetId(1));
    let (family, size) = super::context_menu::label_font();
    let drawn = {
        let mut fs = crate::geometry_font_system().lock().unwrap();
        crate::backend::text::shaped_cluster_offsets(&mut fs, "● Follow Active Editor", size, Some(&family))
            .last()
            .map(|&(_, t)| t)
            .unwrap()
    };
    assert!(drawn > 0.0);
    assert!(m.w >= drawn + 2.0 * PAD, "plate {} narrower than the drawn label {} plus pads", m.w, drawn);
}
