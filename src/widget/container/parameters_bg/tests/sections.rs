//! Sections: collapsing them, their outline as one path, and the gaps between rows and sections.

use super::*;

#[test]
fn clicking_a_section_title_collapses_its_rows() {
    let mut ctx = UiContext::new();
    let mut p = panel_with(&[
        ("Transform", "", "section"),
        ("Size", "1.00", "slider:0:2"),
        ("Shading", "", "section"),
        ("On", "true", "checkbox"),
    ]);
    let expanded_h = p.get_total_content_height();
    let below_before = p.get_param_rects()[2].1;

    // Press the first section's title box.
    let r_hdr = p.get_param_rects()[0];
    let (bx, by, _, bh) = p.section_title_box(0, r_hdr);
    p.mouse_input(MouseButton::Left, ElementState::Pressed, bx + 4.0, by + bh / 2.0, &mut ctx);

    assert!(p.section_collapsed("Transform"));
    assert_eq!(p.get_param_rects()[1].3, 0.0, "the collapsed section's row has no height");
    assert!(p.get_param_rects()[2].1 < below_before, "the next section moves up");
    assert!(p.get_total_content_height() < expanded_h);
    // The row's chrome and label are gone; the header's stay.
    assert!(!p.own_text_labels().iter().any(|l| l.text.contains("Size")));
    assert!(p.own_text_labels().iter().any(|l| l.text.contains("Transform")));

    // Clicking it again restores the section.
    let r_hdr = p.get_param_rects()[0];
    let (bx, by, _, bh) = p.section_title_box(0, r_hdr);
    p.mouse_input(MouseButton::Left, ElementState::Pressed, bx + 4.0, by + bh / 2.0, &mut ctx);
    assert!(!p.section_collapsed("Transform"));
    assert_eq!(p.get_total_content_height(), expanded_h);
}

/// Both ends of a straight run, and of an arc, as points on the path.
fn run_ends(r: &(f32, f32, f32, f32)) -> [(f32, f32); 2] {
    let (x, y, w, h) = *r;
    if w > h {
        [(x, y), (x + w, y)]
    } else {
        [(x, y), (x, y + h)]
    }
}

fn arc_ends(a: &(f32, f32, f32, f32, f32)) -> [(f32, f32); 2] {
    let (cx, cy, r, a0, a1) = *a;
    [
        (cx + r * a0.cos(), cy + r * a0.sin()),
        (cx + r * a1.cos(), cy + r * a1.sin()),
    ]
}

#[test]
fn section_outline_is_one_continuous_path() {
    let p = panel_with(&[("Transform", "", "section"), ("Size", "1.00", "slider:0:2")]);
    let (title, content) = p.section_boxes().into_iter().next().expect("one section");
    let content = content.expect("the section has a content box");
    let (runs, arcs) = p.section_outline(title, content.into());

    // The tab sits flush on the body — its bottom edge is the body's top edge.
    assert_eq!(title.1 + title.3, content.1, "the tab fuses to the body");
    // Folder-tab shape: the tab's two top corners, the concave throat where its right
    // side turns onto the body's top edge, and the body's three remaining corners
    // (its top-LEFT is the tab's left side running straight through).
    assert_eq!(arcs.len(), 6);
    let (tab_right, body_top) = (title.0 + title.2, content.1);
    let throats = arcs
        .iter()
        .filter(|(cx, cy, ..)| *cx > tab_right - 0.01 && *cy < body_top)
        .count();
    assert_eq!(throats, 1, "the throat — centred out in the pocket right of the tab");

    // Every corner hands off to a straight run — no arc dangles. (Within a stroke width:
    // runs and arcs are anchored on opposite ink sides at the concave corner.)
    let ends: Vec<(f32, f32)> = runs.iter().flat_map(run_ends).collect();
    for arc in &arcs {
        for (ax, ay) in arc_ends(arc) {
            let nearest = ends
                .iter()
                .map(|(x, y)| ((x - ax).powi(2) + (y - ay).powi(2)).sqrt())
                .fold(f32::INFINITY, f32::min);
            assert!(nearest <= SECTION_BORDER_T + 0.01, "corner at ({ax}, {ay}) dangles: {nearest}");
        }
    }

    // The tab's bottom edge is open (no run along it), and the body's top edge runs
    // only right of the throat.
    let tab_bottom = title.1 + title.3 - SECTION_BORDER_T;
    let bottom_runs = runs
        .iter()
        .filter(|(_, y, w, _)| (*y - tab_bottom).abs() < 0.01 && *w > SECTION_BORDER_T)
        .count();
    assert_eq!(bottom_runs, 0, "the tab opens onto the body");
    let top_runs: Vec<&(f32, f32, f32, f32)> = runs
        .iter()
        .filter(|(_, y, w, _)| (*y - body_top).abs() < 0.01 && *w > SECTION_BORDER_T)
        .collect();
    assert_eq!(top_runs.len(), 1, "the body's top edge starts past the tab");
    assert!(top_runs[0].0 >= tab_right, "…right of the throat");

    // The left edge is ONE straight run from the tab's top corner to the body's
    // bottom corner.
    let left_runs: Vec<&(f32, f32, f32, f32)> = runs
        .iter()
        .filter(|(x, _, _, h)| (*x - title.0).abs() < 0.01 && *h > 0.0)
        .collect();
    assert_eq!(left_runs.len(), 1, "tab + body share one left side");
    assert!((left_runs[0].1 - (title.1 + SECTION_R)).abs() < 0.01);
    assert!((left_runs[0].1 + left_runs[0].3 - (content.1 + content.3 - SECTION_R)).abs() < 0.01);
}

#[test]
fn a_collapsed_section_outline_closes_on_itself() {
    let mut p = panel_with(&[("Transform", "", "section"), ("Size", "1.00", "slider:0:2")]);
    p.set_section_collapsed("Transform", true);
    let (title, content) = p.section_boxes().into_iter().next().expect("one section");
    assert!(content.is_none(), "nothing to wrap below a collapsed header");
    let (runs, arcs) = p.section_outline(title, None);
    assert_eq!(arcs.len(), 4, "a plain rounded rect");
    assert_eq!(runs.len(), 4);
}

#[test]
fn rows_pack_on_the_channel_and_sections_separate_wider() {
    let p = panel_with(&[
        ("Transform", "", "section"),
        ("Size", "1.00", "slider:0:2"),
        ("Shading", "", "section"),
        ("On", "true", "checkbox"),
    ]);
    let rects = p.get_param_rects();
    let content_bottom = |i: usize| rects[i].1 + rects[i].3 + CONTENT_BOX_PAD;
    let title_top = |i: usize| rects[i].1 - TITLE_BOX_INSET;

    // Inside a section everything packs on the channel: each tab sits flush on its
    // content box, and the rows keep one channel from the box's walls.
    for (title, content) in p.section_boxes() {
        let content = content.expect("both sections have content");
        assert_eq!(title.1 + title.3, content.1, "tab flush on its body");
    }
    let (_, content0) = p.section_boxes()[0];
    let content0 = content0.unwrap();
    assert_eq!(rects[1].1 - content0.1, CHANNEL, "row -> its box's top wall");
    assert_eq!(rects[1].0 - content0.0, CHANNEL, "row -> its box's side wall");

    // Section to section stays far wider, so the blocks still read apart.
    assert_eq!(title_top(2) - content_bottom(1), SECTION_GAP, "section -> next section");
    const { assert!(SECTION_GAP > 2.0 * CHANNEL, "sections separate wider than any channel") };
}

#[test]
fn a_collapsed_section_keeps_the_same_gap_to_the_next_one() {
    // With no content box under it, the collapsed section's bottom edge is its own title
    // box — a fixed row-pitch bump would leave a double gap here.
    let mut p = panel_with(&[
        ("Transform", "", "section"),
        ("Size", "1.00", "slider:0:2"),
        ("Shading", "", "section"),
        ("On", "true", "checkbox"),
    ]);
    p.set_section_collapsed("Transform", true);
    let rects = p.get_param_rects();
    let collapsed_bottom = rects[0].1 - TITLE_BOX_INSET + TITLE_BOX_H;
    let next_title_top = rects[2].1 - TITLE_BOX_INSET;
    assert_eq!(next_title_top - collapsed_bottom, SECTION_GAP);
}

#[test]
fn collapse_survives_a_param_rebuild_and_swallows_row_clicks() {
    let mut ctx = UiContext::new();
    let mut p = panel_with(&[("Shading", "", "section"), ("On", "false", "checkbox")]);
    p.set_section_collapsed("Shading", true);

    // A click where the toggle used to sit must not reach it.
    let (cx, cy, _, ch) = p.toggles[1].as_ref().unwrap().rect();
    p.mouse_input(MouseButton::Left, ElementState::Pressed, cx + 6.0, cy + ch / 2.0, &mut ctx);
    p.mouse_input(MouseButton::Left, ElementState::Released, cx + 6.0, cy + ch / 2.0, &mut ctx);
    assert_eq!(ParamController::node_params(&*p)[1].1, "false", "hidden row ignores clicks");

    // The host re-syncs the panel (node change): collapse is keyed by title, so it holds.
    let params: Vec<(String, String, String)> = [("Shading", "", "section"), ("On", "false", "checkbox"), ("Extra", "1", "int")]
        .iter()
        .map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string()))
        .collect();
    ParamController::set_display_params(&mut *p, &params);
    assert!(p.section_collapsed("Shading"));
    assert_eq!(p.get_param_rects()[1].3, 0.0);
}

#[test]
fn the_row_chrome_is_the_panes_and_the_plate_the_hosts() {
    let ctx = UiContext::new();
    let p = panel_with(&[("Size", "1.00", "slider:0:2")]);
    // The pane's plain quads carry the row chrome (clipped), including the slider
    // background it reads via rect()+color()...
    let plain = p.plain_quads();
    assert!(!plain.is_empty(), "row chrome in the pane's plain quads");
    // ...but NOT the panel's own PARAM_BG plate (the host draws that from color()).
    let (x, y, w, h) = WidgetHost::rect(&p);
    assert!(
        !plain.iter().any(|q| (q.0, q.1, q.2, q.3) == (x, y, w, h)),
        "panel bg plate is the host's, not the pane's"
    );
    // Per-label hatch: the walk's text prims carry the widget font and viewport bounds.
    let mut scratch = crate::scene::paint::PaintCtx::new();
    crate::scene::painter::append_widget_text(&ctx, &p, &mut scratch);
    let labels: Vec<_> = scratch
        .finish()
        .items
        .into_iter()
        .filter_map(|item| match item.prim {
            crate::scene::paint::Prim::Text { font, bounds, .. } => Some((font, bounds)),
            _ => None,
        })
        .collect();
    assert!(!labels.is_empty());
    assert!(labels.iter().all(|(font, bounds)| font.is_some() && bounds.is_some()));
}
