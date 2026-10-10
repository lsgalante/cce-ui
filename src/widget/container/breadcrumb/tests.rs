use super::*;
use crate::context::UiContext;
use crate::widget::WidgetHost;

/// Center x of the visible segment with the given logical index, derived from the
/// widget's own layout so the tests don't depend on the font's exact metrics.
fn seg_center_x(breadcrumb: &Breadcrumb, rect: Rect, logical: usize) -> f32 {
    let s = breadcrumb
        .visible_segs(rect)
        .into_iter()
        .find(|s| s.logical == Some(logical))
        .expect("segment visible");
    s.x + s.w / 2.0
}

/// The flush controls that are not dropdowns or buttons wear their edge
/// too: a breadcrumb's flush run and a font selector's field draw a
/// field that is all run (`PaintCtx::inset_plate`), never a trough.
#[test]
fn a_breadcrumb_and_a_font_selector_wear_the_runs_edge() {
    use crate::scene::paint::{PaintCtx, Prim};
    if !crate::layout::control_relief() {
        return;
    }
    let all_run = |items: Vec<crate::scene::paint::PaintItem>| {
        assert!(!items.iter().any(|i| matches!(i.prim, Prim::Trough { .. })), "no trough");
        items.iter().any(|i| matches!(i.prim, Prim::Field { rect, split, .. } if split < rect.x - 100.0))
    };
    let rect = Rect { x: 10.0, y: 20.0, width: 300.0, height: 24.0 };
    let mut b = Breadcrumb::new();
    b.set_path(&["home".to_string(), "lsgalante".to_string()]);
    b.set_rect(rect.x, rect.y, rect.width, rect.height);
    let mut pc = PaintCtx::new();
    Paint::paint(b.inner(), rect, &mut pc);
    assert!(all_run(pc.finish().items), "a flush breadcrumb run");
    let f = crate::widget::input::FontSelector::new("Sans".to_string());
    let mut pc = PaintCtx::new();
    Paint::paint(f.inner(), rect, &mut pc);
    assert!(all_run(pc.finish().items), "a font selector");
}

#[test]
fn test_breadcrumb_clicks() {
    let mut breadcrumb = Breadcrumb::new();
    breadcrumb.set_path(&["home".to_string(), "lsgalante".to_string()]);
    // Set coordinates: x=10.0, y=20.0, w=300.0, h=24.0
    let rect = Rect { x: 10.0, y: 20.0, width: 300.0, height: 24.0 };
    breadcrumb.set_rect(rect.x, rect.y, rect.width, rect.height);

    let ctx = UiContext::new();

    // Let's test hit_test
    assert!(breadcrumb.hit_test(15.0, 25.0, &ctx));

    // Segments abut (no inter-segment gap) and are sized by real font measurement, so
    // click coordinates are taken from the widget's own layout rather than hardcoded.

    // Click in segment 0 (/) — through the adapter's direct-dispatch mouse_input, the
    // same entry cce-files drives.
    let mut ui_ctx = UiContext::new();
    let x0 = seg_center_x(&breadcrumb, rect, 0);
    assert!(breadcrumb.mouse_input(crate::widget::MouseButton::Left, crate::widget::ElementState::Pressed, x0, 25.0, &mut ui_ctx));
    assert_eq!(breadcrumb.path_click(), Some(0));

    // Click in segment 1 (home/)
    let x1 = seg_center_x(&breadcrumb, rect, 1);
    assert!(breadcrumb.mouse_input(crate::widget::MouseButton::Left, crate::widget::ElementState::Pressed, x1, 25.0, &mut ui_ctx));
    assert_eq!(breadcrumb.path_click(), Some(1));

    // Click in segment 2 (lsgalante/) — the last segment is the current dir, not a link.
    let x2 = seg_center_x(&breadcrumb, rect, 2);
    assert!(!breadcrumb.mouse_input(crate::widget::MouseButton::Left, crate::widget::ElementState::Pressed, x2, 25.0, &mut ui_ctx));
    assert_eq!(breadcrumb.path_click(), None);
}

#[test]
fn test_breadcrumb_right_clicks() {
    let mut breadcrumb = Breadcrumb::new();
    breadcrumb.set_path(&["home".to_string(), "lsgalante".to_string()]);
    let rect = Rect { x: 10.0, y: 20.0, width: 300.0, height: 24.0 };
    breadcrumb.set_rect(rect.x, rect.y, rect.width, rect.height);

    let mut ui_ctx = UiContext::new();

    // Right click segment 1 (home/)
    let x1 = seg_center_x(&breadcrumb, rect, 1);
    let handled = breadcrumb.mouse_input(crate::widget::MouseButton::Right, crate::widget::ElementState::Pressed, x1, 25.0, &mut ui_ctx);
    assert!(handled);
    assert_eq!(breadcrumb.right_clicked_seg, Some(1));

    // Test path_to_seg
    assert_eq!(breadcrumb.path_to_seg(0), "/");
    assert_eq!(breadcrumb.path_to_seg(1), "/home");
    assert_eq!(breadcrumb.path_to_seg(2), "/home/lsgalante");
}

#[test]
fn long_path_elides_leading_segments() {
    let mut breadcrumb = Breadcrumb::new();
    breadcrumb.set_path(&[
        "home".to_string(),
        "lsgalante".to_string(),
        "Dropbox".to_string(),
        "cce".to_string(),
        "cce-ui".to_string(),
    ]);
    // Narrow container: the full path can't fit, so leading segments get dropped.
    breadcrumb.set_rect(0.0, 0.0, 160.0, 24.0);

    let segs = breadcrumb.visible_segs(Rect { x: 0.0, y: 0.0, width: 160.0, height: 24.0 });

    // First visible segment is the "…" ellipsis marker (no logical index → not a link).
    assert_eq!(segs.first().map(|s| s.text.as_str()), Some("…"));
    assert_eq!(segs.first().and_then(|s| s.logical), None);

    // The current directory (last logical segment) is always visible.
    let last_logical = breadcrumb.path.len(); // "/" is index 0, so path.len() == last idx
    assert_eq!(segs.last().and_then(|s| s.logical), Some(last_logical));
    assert_eq!(segs.last().map(|s| s.text.as_str()), Some("cce-ui"));

    // The run hugs the well's bevel on the left and never crosses the
    // mirrored limit on the right — a segment may reach that edge, but it
    // stops flush against it rather than running under the bevel.
    assert_eq!(segs[0].x, Breadcrumb::SEG_INSET);
    for s in &segs {
        assert!(
            s.x + s.w <= 160.0 - Breadcrumb::SEG_INSET + 0.01,
            "segment {:?} crosses the right inset",
            s.text
        );
    }

    // A kept trailing segment still hit-tests to its original logical index, so clicking
    // it navigates to the correct path.
    let visible_seg = segs.iter().rev().nth(1).unwrap();
    let hit = breadcrumb.seg_at(
        Rect { x: 0.0, y: 0.0, width: 160.0, height: 24.0 },
        visible_seg.x + 6.0,
        12.0,
    );
    assert_eq!(hit, visible_seg.logical);
}

#[test]
fn short_path_is_not_elided() {
    let mut breadcrumb = Breadcrumb::new();
    breadcrumb.set_path(&["home".to_string(), "lsgalante".to_string()]);
    breadcrumb.set_rect(0.0, 0.0, 300.0, 24.0);

    let segs = breadcrumb.visible_segs(Rect { x: 0.0, y: 0.0, width: 300.0, height: 24.0 });
    // Root + two components, no ellipsis.
    assert_eq!(segs.len(), 3);
    assert!(segs.iter().all(|s| s.logical.is_some()));
    assert_eq!(segs[0].text, "/");
}

/// The seams lean like "/" — top edge to the RIGHT of the bottom — and there
/// is exactly one per interior boundary, sitting on the shared edge at
/// mid-height. The run plate spans all of them.
#[test]
fn seams_lean_right_at_the_top() {
    let mut breadcrumb = Breadcrumb::new();
    breadcrumb.set_path(&["home".to_string(), "lsgalante".to_string()]);
    let rect = Rect { x: 10.0, y: 20.0, width: 300.0, height: 24.0 };
    breadcrumb.set_rect(rect.x, rect.y, rect.width, rect.height);

    let segs = breadcrumb.visible_segs(rect);
    let seams = breadcrumb.seams(rect);
    // Root + two components ⇒ two interior boundaries.
    assert_eq!(seams.len(), 2);
    assert_eq!(seams.len(), segs.len() - 1);

    for (i, ((tx, ty), (bx, by))) in seams.iter().enumerate() {
        assert!(tx > bx, "seam {i} must lean right at the top");
        assert!(ty < by, "seam {i} top must be above its bottom");
        // Centered on the boundary it divides.
        let edge = segs[i + 1].x;
        assert!(((tx + bx) * 0.5 - edge).abs() < 0.01);
    }

    // One plate under the lot, spanning first edge to last.
    let (rx, _, rw, _) = breadcrumb.run_box(rect).expect("run laid out");
    assert_eq!(rx, segs[0].x);
    assert!((rx + rw - (segs[2].x + segs[2].w)).abs() < 0.01);
}

/// A point in a segment's top-left corner belongs to the segment on the
/// LEFT: the seam has leaned right there, so the boundary is no longer the
/// nominal edge. This is what an x-only hit test got wrong.
#[test]
fn hit_test_follows_the_seam_lean() {
    let mut breadcrumb = Breadcrumb::new();
    breadcrumb.set_path(&["home".to_string(), "lsgalante".to_string()]);
    let rect = Rect { x: 10.0, y: 20.0, width: 300.0, height: 24.0 };
    breadcrumb.set_rect(rect.x, rect.y, rect.width, rect.height);

    let segs = breadcrumb.visible_segs(rect);
    let (y, h) = Breadcrumb::plate_band(rect);
    let edge = segs[1].x; // boundary between "/" and "home"
    let lean = SEG_SLANT * h * 0.5;
    assert!(lean > 1.0, "the test needs a lean wide enough to probe");

    // Just right of the nominal edge, at the TOP: still segment 0.
    assert_eq!(breadcrumb.seg_at(rect, edge + lean * 0.5, y + 0.5), Some(0));
    // The same x at the BOTTOM, where the seam has leaned left: segment 1.
    assert_eq!(breadcrumb.seg_at(rect, edge + lean * 0.5, y + h - 0.5), Some(1));
    // At mid-height the seam sits on the nominal edge.
    assert_eq!(breadcrumb.seg_at(rect, edge + 0.5, y + h * 0.5), Some(1));
    assert_eq!(breadcrumb.seg_at(rect, edge - 0.5, y + h * 0.5), Some(0));
}

/// A segment claims its plate, not the full-height column under it. `hit()`
/// delegates to `seg_at`, so a missing vertical bound there hands the widget
/// every press sharing an x with the run — which is how a host's own content
/// menu (cce-files' file rows) lost its right-click to the copy-path menu.
#[test]
fn hit_test_stops_at_the_plate_band() {
    let mut breadcrumb = Breadcrumb::new();
    breadcrumb.set_path(&["home".to_string(), "lsgalante".to_string()]);
    let rect = Rect { x: 10.0, y: 20.0, width: 300.0, height: 24.0 };
    breadcrumb.set_rect(rect.x, rect.y, rect.width, rect.height);

    let (y, h) = Breadcrumb::plate_band(rect);
    let x = seg_center_x(&breadcrumb, rect, 1);

    // Inside the band the segment answers, at the top and bottom edges too.
    assert_eq!(breadcrumb.seg_at(rect, x, y + h * 0.5), Some(1));
    assert_eq!(breadcrumb.seg_at(rect, x, y), Some(1));
    assert_eq!(breadcrumb.seg_at(rect, x, y + h - 0.5), Some(1));

    // Above and below it, nothing — however far the seams have leaned.
    assert_eq!(breadcrumb.seg_at(rect, x, y - 0.5), None);
    assert_eq!(breadcrumb.seg_at(rect, x, y + h), None);
    assert_eq!(breadcrumb.seg_at(rect, x, y + 400.0), None);

    // And the same bound through the `hit_test` hosts actually call.
    let ctx = UiContext::new();
    assert!(breadcrumb.hit_test(x, y + h * 0.5, &ctx));
    assert!(!breadcrumb.hit_test(x, y + 400.0, &ctx));
}

/// The run's outer ends stay upright — only edges that face another segment
/// lean, so the first segment's left edge is a plain vertical boundary.
#[test]
fn outer_ends_do_not_lean() {
    let mut breadcrumb = Breadcrumb::new();
    breadcrumb.set_path(&["home".to_string()]);
    let rect = Rect { x: 10.0, y: 20.0, width: 300.0, height: 24.0 };
    breadcrumb.set_rect(rect.x, rect.y, rect.width, rect.height);

    let segs = breadcrumb.visible_segs(rect);
    let (y, h) = Breadcrumb::plate_band(rect);
    let left = segs[0].x;
    let right = segs[1].x + segs[1].w;

    for py in [y + 0.5, y + h * 0.5, y + h - 0.5] {
        assert_eq!(breadcrumb.seg_at(rect, left + 0.5, py), Some(0));
        assert_eq!(breadcrumb.seg_at(rect, left - 0.5, py), None);
        assert_eq!(breadcrumb.seg_at(rect, right - 0.5, py), Some(1));
        assert_eq!(breadcrumb.seg_at(rect, right + 0.5, py), None);
    }
}

/// The wash's rounded ends are the plate's corner family: a circle at
/// n = 2, and at a squircle's exponent the corner stays nearer the square,
/// so the inset at the top row is smaller — what a circular arc got wrong.
#[test]
fn the_wash_ends_follow_the_plates_corner_family() {
    let (r, y, h) = (10.0, 0.0, 24.0);
    let top = 0.5;
    let circle = Breadcrumb::end_inset(r, 2.0, y, h, top);
    let dy: f32 = r - top;
    assert!((circle - (r - (r * r - dy * dy).sqrt())).abs() < 1e-4);
    let squircle = Breadcrumb::end_inset(r, 4.5, y, h, top);
    assert!(squircle < circle - 2.0, "squircle {squircle} vs circle {circle}");
    // Mid-height is the upright edge in either family, and the band's
    // two ends mirror each other.
    assert_eq!(Breadcrumb::end_inset(r, 4.5, y, h, h * 0.5), 0.0);
    assert!((Breadcrumb::end_inset(r, 4.5, y, h, h - top) - squircle).abs() < 1e-4);
}

#[test]
fn path_controller_reachable_through_element() {
    let mut breadcrumb = Breadcrumb::new();
    PathController::set_path(&mut *breadcrumb, &["a".to_string()]);
    assert_eq!(breadcrumb.path, vec!["a".to_string()]);
}
