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

#[test]
fn graph_emits_flat_grid_lines() {
    use cce_ui::widget::GraphController;
    let mut g = cce_ui::widget::Graph::new();
    g.set_grid_sizes(71.0, 31.0);
    g.set_skipped_sizes(12.0, 14.0);
    g.set_grid_origin(50.0, 50.0);
    g.set_show_network_grid(true);
    // The designer hard-codes a uniform background; that must not hide the relief.
    g.set_uniform_background(true);
    let mut pc = PaintCtx::new();
    let rect = Rect { x: 40.0, y: 40.0, width: 600.0, height: 300.0 };
    g.paint_grid(rect, &mut pc);
    let dl = pc.finish();
    let n_quad = dl.items.iter().filter(|i| matches!(i.prim, Prim::Quad { .. })).count();
    // ~8 columns x ~10 rows of gap strips (two per cell) plus the two axes.
    assert!(n_quad > 100, "only {n_quad} quads");
    assert!(dl.items.iter().all(|i| matches!(i.prim, Prim::Quad { .. })), "flat lines only");
}
