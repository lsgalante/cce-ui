use cce_ui::scene::layout::Rect;
use cce_ui::scene::paint::{PaintCtx, Prim};

#[test]
fn lattice_survives_tessellation_inside_a_clip() {
    let mut pc = PaintCtx::new();
    let clip = Rect { x: 40.0, y: 40.0, width: 600.0, height: 300.0 };
    pc.clip(clip, |pc| {
        pc.lattice(clip, (85.0, 43.0), (60.0, 60.0), (71.0, 31.0), 6.0, 6.0);
        pc.groove((40.0, 100.0), (640.0, 100.0), 2.0, 2.0, clip);
    });
    let dl = pc.finish();
    let kinds: Vec<String> = dl.items.iter().map(|i| format!("{:?}", std::mem::discriminant(&i.prim))).collect();
    let n_lattice = dl.items.iter().filter(|i| matches!(i.prim, Prim::Lattice { .. })).count();
    assert_eq!(n_lattice, 1, "display list: {kinds:?}");
    let (verts, batches, _, _) = cce_ui::backend::window_runner::tessellate_display_list(&dl, 1280.0, 720.0, 2.0);
    let modes: Vec<f32> = batches.iter().filter_map(|b| b.plate.as_ref().map(|p| p.mode)).collect();
    eprintln!("verts={} batches={} plate modes={modes:?}", verts.len(), batches.len());
    assert!(modes.contains(&13.0), "no lattice batch; modes={modes:?}");
}

/// The graph grid is a LATTICE of lines (since 2026-09-22): one quad per
/// line centred on its pitch coordinate, clipped to the rect, plus the two
/// heavier axes through the (0, 0) crossing. It used to be cells with grout
/// between them, one `Grout` draw, which is why this test once asserted that.
#[test]
fn graph_paints_its_lattice_as_lines_and_axes() {
    use cce_ui::widget::GraphController;
    // The one style value this path reads. Pinned, or the user's config.kdl
    // decides the answer (a width of 0 turns the lines off entirely). This
    // binary builds cce-ui without cfg(test), so the write is to the shared
    // registry, which no other test in this file reads.
    cce_ui::layout::set_graph_line_width(1.0);
    let mut g = cce_ui::widget::Graph::new();
    g.set_grid_pitch(60.0, 50.0);
    g.set_grid_origin(50.0, 50.0);
    g.set_network_opacity(1.0);
    g.set_show_network_grid(true);
    // The designer hard-codes a uniform background; that must not hide the grid.
    g.set_uniform_background(true);
    let mut pc = PaintCtx::new();
    let rect = Rect { x: 40.0, y: 40.0, width: 600.0, height: 300.0 };
    g.paint_grid(rect, &mut pc);
    let dl = pc.finish();

    assert!(!dl.items.iter().any(|i| matches!(i.prim, Prim::Grout { .. })), "the lattice draws no grout");
    let quads: Vec<Rect> = dl
        .items
        .iter()
        .filter_map(|i| match i.prim {
            Prim::Quad { rect, .. } => Some(rect),
            _ => None,
        })
        .collect();

    // Verticals at x = 50 + 60c inside 40..640: 50, 110 … 590, ten of them;
    // horizontals at y = 50 + 50r inside 40..340: 50, 100 … 300, six. No line
    // sits within half a width of an edge, so none is clipped to a sliver.
    let mut verticals: Vec<f32> = quads
        .iter()
        .filter(|q| q.width == 1.0 && q.height == rect.height)
        .map(|q| q.x + 0.5)
        .collect();
    let mut horizontals: Vec<f32> = quads
        .iter()
        .filter(|q| q.height == 1.0 && q.width == rect.width)
        .map(|q| q.y + 0.5)
        .collect();
    verticals.sort_by(f32::total_cmp);
    horizontals.sort_by(f32::total_cmp);
    assert_eq!(verticals, (0..10).map(|c| 50.0 + 60.0 * c as f32).collect::<Vec<_>>());
    assert_eq!(horizontals, (0..6).map(|r| 50.0 + 50.0 * r as f32).collect::<Vec<_>>());

    // The axes: 2 px, across the whole rect, centred on the origin.
    let axes: Vec<&Rect> = quads.iter().filter(|q| q.width == 2.0 || q.height == 2.0).collect();
    assert_eq!(axes.len(), 2, "two origin axes; quads: {quads:?}");
    assert!(axes.iter().any(|q| q.height == 2.0 && q.y == 49.0 && q.width == rect.width));
    assert!(axes.iter().any(|q| q.width == 2.0 && q.x == 49.0 && q.height == rect.height));
    assert_eq!(quads.len(), 10 + 6 + 2, "nothing else is painted; quads: {quads:?}");
}

#[test]
fn grout_tessellates_to_a_mode_15_batch_in_its_colour() {
    let mut pc = PaintCtx::new();
    let clip = Rect { x: 40.0, y: 40.0, width: 600.0, height: 300.0 };
    pc.clip(clip, |pc| pc.grout(clip, (85.0, 43.0), (60.0, 60.0), (71.0, 31.0), 15.5, [0.5, 0.5, 0.5, 0.1]));
    let dl = pc.finish();
    let (verts, batches, _, _) = cce_ui::backend::window_runner::tessellate_display_list(&dl, 1280.0, 720.0, 2.0);
    let modes: Vec<f32> = batches.iter().filter_map(|b| b.plate.as_ref().map(|p| p.mode)).collect();
    assert_eq!(modes, vec![15.0]);
    assert_eq!(verts.len(), 6);
    assert!(verts.iter().all(|v| (v.color[3] - 0.1).abs() < 1e-6), "cover quad carries the grout colour");
}
