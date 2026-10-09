use super::*;
use crate::context::UiContext;
use crate::widget::WidgetHost;

/// A 100 x 60 lattice whose (0, 0) intersection is at (140, 120), so node
/// a's 80 x 40 body is the rect (100, 100, 80, 40) and node b's, one cell
/// down-right, is (200, 160, 80, 40).
fn two_nodes() -> Adapted<Graph> {
    let mut g = Graph::new();
    WidgetHost::set_rect(&mut g, 0.0, 0.0, 800.0, 600.0);
    g.set_grid_pitch(100.0, 60.0);
    g.set_node_size(80.0, 40.0);
    g.set_grid_origin(140.0, 120.0);
    g.set_grid_snap_enabled(true);
    let node = |id: &str, name: &str, col: f32, row: f32| GraphNode {
        id: id.into(),
        name: name.into(),
        position: (col, row),
        parameters: Vec::new(),
        geom_visible: true,
        node_type: String::new(),
        inputs: 1,
        outputs: 1,
    };
    g.set_nodes(&[node("a", "alpha", 0.0, 0.0), node("b", "beta", 1.0, 1.0)]);
    g
}

/// A host can hide the geometry toggle: no node wears the disc, and
/// nothing is there to hit.
#[test]
fn a_host_can_hide_the_geometry_toggles() {
    let mut g = two_nodes();
    assert!(g.toggle_rect(0).is_some(), "toggles show by default");
    g.set_show_toggles(false);
    assert!(g.toggle_rect(0).is_none() && g.toggle_rect(1).is_none());
    let rect = Rect { x: 0.0, y: 0.0, width: 800.0, height: 600.0 };
    let big = g.port_circles(rect).iter().filter(|c| c.2 > 5.0).count();
    assert_eq!(big, 0, "no toggle disc is drawn");
}

/// A node against the pane's right edge keeps its name: the label flips
/// to the body's left when it would not fit on the right, whatever the
/// name's length, and a node with room keeps the right-hand placement.
#[test]
fn a_node_at_the_right_edge_keeps_its_label_on_the_left() {
    let mut g = two_nodes();
    let node = |name: &str, col: f32| GraphNode {
        id: name.into(),
        name: name.into(),
        position: (col, 0.0),
        parameters: Vec::new(),
        geom_visible: true,
        node_type: String::new(),
        inputs: 1,
        outputs: 1,
    };
    // With the origin at 160, column 6's body spans 720..800 — flush
    // against the right edge of an 800 px pane, so a right-hand label
    // would BEGIN past the edge, which is exactly the screenshot that
    // found this. Column 2's spans 320..400: plenty of room.
    g.set_grid_origin(160.0, 120.0);
    g.set_nodes(&[node("w", 6.0), node("wrangle_with_a_long_name", 6.0), node("mid", 2.0)]);
    let rect = Rect { x: 0.0, y: 0.0, width: 800.0, height: 600.0 };
    let labels = g.node_labels(rect);
    assert_eq!(labels.len(), 3, "every node keeps a label: {:?}", labels.iter().map(|l| &l.text).collect::<Vec<_>>());
    let (nx, _, nw, _) = g.node_rect(0).unwrap();
    assert_eq!((nx, nx + nw), (720.0, 800.0));
    for name in ["w", "wrangle_with_a_long_name"] {
        let l = labels.iter().find(|l| l.text == name).unwrap();
        let w = TextLabel::estimate_width(name, l.font_size);
        assert!((l.x + w - (nx - 8.0)).abs() < 0.5, "{name} hangs off the left edge, right-aligned to it: x {} w {w}", l.x);
        assert!(l.x >= rect.x, "{name} stays inside the pane");
    }
    let mid = labels.iter().find(|l| l.text == "mid").unwrap();
    let (mx, _, mw, _) = g.node_rect(2).unwrap();
    assert_eq!(mid.x, mx + mw + 8.0, "a node with room keeps the right-hand placement");
}

/// A double-click's two presses always straddle a host node re-sync — the
/// designer calls set_nodes on EVERY window event — so the detection state
/// must survive set_nodes. It used to be wiped there wholesale, which made
/// double-click structurally impossible outside unit tests.
#[test]
fn double_click_survives_the_between_press_node_resync() {
    let mut ctx = UiContext::new();
    let g = ctx.insert(two_nodes());

    // First press on node a, then the host re-syncs (same content),
    // then the second press: this is the real event sequence.
    assert!(ctx.lend_h(g, |w, ctx| w.mouse_input(MouseButton::Left, ElementState::Pressed, 110.0, 120.0, ctx)).unwrap());
    ctx.lend_h(g, |w, ctx| w.mouse_input(MouseButton::Left, ElementState::Released, 110.0, 120.0, ctx)).unwrap();
    let nodes = ctx[g].get_nodes();
    ctx[g].set_nodes(&nodes);
    assert!(ctx.lend_h(g, |w, ctx| w.mouse_input(MouseButton::Left, ElementState::Pressed, 110.0, 120.0, ctx)).unwrap());

    assert_eq!(ctx[g].double_clicked_node(), Some(0), "double-click lost across set_nodes");
    ctx[g].clear_double_clicked_node();
    assert_eq!(ctx[g].double_clicked_node(), None);
}

/// Two presses on DIFFERENT nodes are not a double-click, id-keyed or not.
#[test]
fn presses_on_two_nodes_are_not_a_double_click() {
    let mut ctx = UiContext::new();
    let g = ctx.insert(two_nodes());

    assert!(ctx.lend_h(g, |w, ctx| w.mouse_input(MouseButton::Left, ElementState::Pressed, 110.0, 120.0, ctx)).unwrap());
    ctx.lend_h(g, |w, ctx| w.mouse_input(MouseButton::Left, ElementState::Released, 110.0, 120.0, ctx)).unwrap();
    // Node b sits one grid step down-right of a.
    assert!(ctx.lend_h(g, |w, ctx| w.mouse_input(MouseButton::Left, ElementState::Pressed, 210.0, 180.0, ctx)).unwrap());
    assert_eq!(ctx[g].double_clicked_node(), None);
}

#[test]
fn node_press_selects_arms_drag_and_commit_snaps_to_grid() {
    let mut ctx = UiContext::new();
    let g = ctx.insert(two_nodes());

    // Node a occupies (100, 100, 80, 40). Press its body (away from ports/toggle).
    assert!(ctx.lend_h(g, |w, ctx| w.mouse_input(MouseButton::Left, ElementState::Pressed, 110.0, 120.0, ctx)).unwrap());
    assert_eq!(ctx[g].selected_node(), Some(0));
    assert!(ctx[g].is_dragging() && ctx[g].draggable());

    // Drag one pitch right (pitch_x = 100): snap puts the node at column 1, and cell
    // (1, 0) is free so it lands there.
    ctx[g].drag_begin(110.0, 120.0);
    assert!(ctx[g].drag_update(210.0, 120.0));
    assert!(ctx.lend_h(g, |w, ctx| w.mouse_input(MouseButton::Left, ElementState::Released, 210.0, 120.0, ctx)).unwrap());
    assert!(!ctx[g].is_dragging());
    assert_eq!(ctx[g].get_nodes()[0].position, (1.0, 0.0));

    // An empty-space press clears the selection and is NOT consumed (legacy contract).
    assert!(!ctx.lend_h(g, |w, ctx| w.mouse_input(MouseButton::Left, ElementState::Pressed, 700.0, 550.0, ctx)).unwrap());
    assert_eq!(ctx[g].selected_node(), None);
}

/// Dropping a dragged node onto a wire splices it in: the drop reports
/// (dragged id, the wire's upstream NAME, the wire's downstream id) for
/// the host to rewire both Input params. A drop away from every wire
/// reports nothing, and the handshake is take-once.
#[test]
fn node_dropped_on_a_wire_reports_a_splice() {
    for style in WireStyle::ALL {
        node_dropped_on_a_wire_reports_a_splice_in(style);
    }
}

/// With swap-on-drop, a node dropped on another node trades cells with
/// it and reports the swap — over the wire between them, which the
/// ghost also touches. Without it, the drop walks to a free cell as it
/// always did.
#[test]
fn a_node_dropped_on_a_node_swaps_with_it() {
    let build = |swap: bool| {
        let mut ctx = UiContext::new();
        let g = ctx.insert(Graph::new());
        WidgetHost::set_rect(&mut ctx[g], 0.0, 0.0, 800.0, 600.0);
        ctx[g].set_grid_pitch(100.0, 60.0);
        ctx[g].set_node_size(80.0, 40.0);
        ctx[g].set_grid_origin(140.0, 120.0);
        ctx[g].set_grid_snap_enabled(true);
        ctx[g].set_swap_on_drop(swap);
        let node = |id: &str, name: &str, col: f32, row: f32, input: &str| GraphNode {
            id: id.into(),
            name: name.into(),
            position: (col, row),
            parameters: vec![("Input".to_string(), input.to_string(), "node".to_string())],
            geom_visible: true,
            node_type: String::new(),
            inputs: 1,
            outputs: 1,
        };
        // alpha above beta, beta reading alpha.
        ctx[g].set_nodes(&[node("a", "alpha", 0.0, 0.0, ""), node("b", "beta", 0.0, 2.0, "alpha")]);
        // Drag alpha by its middle onto beta's middle.
        assert!(ctx.lend_h(g, |w, ctx| w.mouse_input(MouseButton::Left, ElementState::Pressed, 140.0, 120.0, ctx)).unwrap());
        ctx[g].drag_begin(140.0, 120.0);
        assert!(ctx[g].drag_update(140.0, 240.0));
        (g, ctx)
    };

    let (g, mut ctx) = build(true);
    assert_eq!(ctx[g].swap_target_idx(), Some(1), "beta is the swap target while the ghost is on it");
    assert_eq!(ctx[g].node_rect(0), ctx[g].node_rect(1), "the ghost sits on beta's cell, where a swap lands");
    assert_eq!(ctx[g].drop_target_cell_rect(), ctx[g].node_rect(1), "and its cell is where the drop lands");
    assert!(ctx.lend_h(g, |w, ctx| w.mouse_input(MouseButton::Left, ElementState::Released, 140.0, 240.0, ctx)).unwrap());
    assert_eq!(GraphController::take_pending_swap(&mut *ctx[g]), Some(("a".to_string(), "b".to_string())));
    assert_eq!(GraphController::take_pending_swap(&mut *ctx[g]), None, "take-once");
    assert_eq!(GraphController::take_pending_splice(&mut *ctx[g]), None, "a swap is not a splice");
    let nodes = GraphController::get_nodes(&*ctx[g]);
    assert_eq!((nodes[0].position, nodes[1].position), ((0.0, 2.0), (0.0, 0.0)), "the two traded cells");

    let (g, mut ctx) = build(false);
    assert_eq!(ctx[g].swap_target_idx(), None);
    // No swap to make: the ghost snaps to the free cell a drop walks to,
    // not over beta, where it could not stay.
    let ghost = ctx[g].node_rect(0).unwrap();
    assert_ne!(Some(ghost), ctx[g].node_rect(1), "the ghost does not sit on a taken cell");
    assert_eq!(Some(ghost), ctx[g].drop_target_cell_rect(), "it sits where the drop lands");
    assert!(ctx.lend_h(g, |w, ctx| w.mouse_input(MouseButton::Left, ElementState::Released, 140.0, 240.0, ctx)).unwrap());
    assert_eq!(GraphController::take_pending_swap(&mut *ctx[g]), None);
    let nodes = GraphController::get_nodes(&*ctx[g]);
    assert_eq!(nodes[1].position, (0.0, 2.0), "beta stays");
    assert_ne!(nodes[0].position, (0.0, 2.0), "alpha walks off the taken cell");
}

fn node_dropped_on_a_wire_reports_a_splice_in(style: WireStyle) {
    let mut ctx = UiContext::new();
    let g = ctx.insert(Graph::new());
    ctx[g].set_wire_style(Some(style));
    WidgetHost::set_rect(&mut ctx[g], 0.0, 0.0, 800.0, 600.0);
    ctx[g].set_grid_pitch(100.0, 60.0);
    ctx[g].set_node_size(80.0, 40.0);
    ctx[g].set_grid_origin(140.0, 120.0);
    ctx[g].set_grid_snap_enabled(true);
    let node = |id: &str, name: &str, col: f32, row: f32, params: Vec<(String, String, String)>| GraphNode {
        id: id.into(),
        name: name.into(),
        position: (col, row),
        parameters: params,
        geom_visible: true,
        node_type: String::new(),
        inputs: 1,
        outputs: 1,
    };
    let p = |v: &str| vec![("Input".to_string(), v.to_string(), "text".to_string())];
    // alpha → beta wire runs through the empty cell (1, 0) between them;
    // gamma sits below, unwired.
    ctx[g].set_nodes(&[
        node("a", "alpha", 0.0, 0.0, Vec::new()),
        node("b", "beta", 2.0, 0.0, p("alpha")),
        node("c", "gamma", 0.0, 2.0, p("")),
    ]);

    // Drag gamma's body onto the wire's horizontal run (cell (1, 0)).
    assert!(ctx.lend_h(g, |w, ctx| w.mouse_input(MouseButton::Left, ElementState::Pressed, 110.0, 240.0, ctx)).unwrap());
    ctx[g].drag_begin(110.0, 240.0);
    assert!(ctx[g].drag_update(210.0, 140.0));
    assert!(ctx.lend_h(g, |w, ctx| w.mouse_input(MouseButton::Left, ElementState::Released, 210.0, 140.0, ctx)).unwrap());

    let splice = GraphController::take_pending_splice(&mut *ctx[g]);
    assert_eq!(
        splice,
        Some(("c".to_string(), "alpha".to_string(), "b".to_string())),
        "{style:?}: drop on the wire must report (dragged, upstream name, downstream id)"
    );
    assert_eq!(GraphController::take_pending_splice(&mut *ctx[g]), None, "take-once");

    // A drop in open space reports nothing. Gamma landed in cell (1, 0)
    // — the free cell its splice drop resolved to — so drag it from
    // there down to open space clear of the wire.
    assert!(ctx.lend_h(g, |w, ctx| w.mouse_input(MouseButton::Left, ElementState::Pressed, 210.0, 110.0, ctx)).unwrap());
    ctx[g].drag_begin(210.0, 110.0);
    assert!(ctx[g].drag_update(210.0, 230.0));
    assert!(ctx.lend_h(g, |w, ctx| w.mouse_input(MouseButton::Left, ElementState::Released, 210.0, 230.0, ctx)).unwrap());
    assert_eq!(GraphController::take_pending_splice(&mut *ctx[g]), None);
}

/// A cell a wire runs through names that wire, in every style; a cell
/// clear of every wire names none.
#[test]
fn a_cell_on_a_wire_names_the_wire() {
    for style in WireStyle::ALL {
        let mut g = Graph::new();
        g.set_wire_style(Some(style));
        WidgetHost::set_rect(&mut g, 0.0, 0.0, 800.0, 600.0);
        g.set_grid_pitch(100.0, 60.0);
        g.set_node_size(80.0, 40.0);
        g.set_grid_origin(140.0, 120.0);
        let node = |id: &str, name: &str, col: f32, row: f32, input: &str| GraphNode {
            id: id.into(),
            name: name.into(),
            position: (col, row),
            parameters: vec![("Input".to_string(), input.to_string(), "node".to_string())],
            geom_visible: true,
            node_type: String::new(),
            inputs: 1,
            outputs: 1,
        };
        // alpha above beta, a cell between them.
        g.set_nodes(&[node("a", "alpha", 0.0, 0.0, ""), node("b", "beta", 0.0, 2.0, "alpha")]);
        assert_eq!(
            g.input_wire_through_cell(0.0, 1.0),
            Some(("a".to_string(), "b".to_string())),
            "{style:?}"
        );
        assert_eq!(g.input_wire_through_cell(2.0, 1.0), None, "{style:?}: off to the side");
    }
}

/// Where a piece of a wire begins and ends.
fn seg_ends(seg: &WireSeg) -> ((f32, f32), (f32, f32)) {
    match *seg {
        WireSeg::Line(a, b) => (a, b),
        WireSeg::Arc { c, r, a0, a1 } => (
            (c.0 + r * a0.cos(), c.1 + r * a0.sin()),
            (c.0 + r * a1.cos(), c.1 + r * a1.sin()),
        ),
    }
}

fn near(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01
}

/// Every style but the orthogonal one is one unbroken run from the
/// output port to the input port, down the graph and back up it: each
/// piece begins where the last ended. The orthogonal one is three runs
/// meeting square, which touch without overlapping so a translucent wire
/// is one alpha throughout — the across run filling the corners.
#[test]
fn a_wire_thinner_than_a_device_pixel_is_that_pixel_fainter() {
    // At 2x a device pixel is half a logical one.
    assert_eq!(wire_stroke(3.0, 0.5), (3.0, 1.0));
    assert_eq!(wire_stroke(0.5, 0.5), (0.5, 1.0));
    assert_eq!(wire_stroke(0.25, 0.5), (0.5, 0.5));
    // At 1x the pixel is a whole one, and nothing draws narrower.
    assert_eq!(wire_stroke(0.5, 1.0), (1.0, 0.5));
    assert_eq!(wire_stroke(0.0, 1.0), (1.0, 0.0));
}

#[test]
fn every_wire_style_runs_from_port_to_port() {
    let t = 6.0;
    let cases = [((100.0, 100.0), (300.0, 260.0)), ((300.0, 260.0), (100.0, 100.0)), ((100.0, 100.0), (100.0, 300.0))];
    for (start, end) in cases {
        for style in [WireStyle::Rounded, WireStyle::Bezier, WireStyle::Straight] {
            let segs = wire_path(style, start, end, t, 20.0, None);
            assert!(near(seg_ends(&segs[0]).0, start), "{style:?} {start:?}->{end:?} starts at the output");
            assert!(near(seg_ends(segs.last().unwrap()).1, end), "{style:?} {start:?}->{end:?} ends at the input");
            for w in segs.windows(2) {
                assert!(near(seg_ends(&w[0]).1, seg_ends(&w[1]).0), "{style:?} {start:?}->{end:?} is unbroken");
            }
        }
        let segs = wire_path(WireStyle::Rounded, start, end, t, 20.0, None);
        if start.0 != end.0 {
            assert_eq!(segs.iter().filter(|s| matches!(s, WireSeg::Arc { .. })).count(), 2, "two rounded bends");
        }
    }

    let segs = wire_path(WireStyle::Orthogonal, (100.0, 100.0), (300.0, 260.0), t, 20.0, None);
    assert_eq!(
        segs,
        vec![
            WireSeg::Line((100.0, 100.0), (100.0, 177.0)),
            WireSeg::Line((97.0, 180.0), (303.0, 180.0)),
            WireSeg::Line((300.0, 183.0), (300.0, 260.0)),
        ]
    );
}

/// A wire running down several rows turns on the first lattice line
/// below its source, not halfway: from row -1 to row 3 it turned on
/// row 1's line, after running down through the node standing there.
/// Between adjacent rows there is no line between the bodies, and the
/// wire turns halfway as it always did.
#[test]
fn a_wire_turns_on_the_first_line_below_its_source() {
    let mut g = Graph::new();
    WidgetHost::set_rect(&mut g, 0.0, 0.0, 1200.0, 900.0);
    g.set_grid_pitch(140.0, 70.0);
    g.set_node_size(80.0, 40.0);
    g.set_grid_origin(100.0, 200.0);
    let node = |name: &str, col: f32, row: f32, wires: &[&str]| GraphNode {
        id: name.into(),
        name: name.into(),
        position: (col, row),
        parameters: wires.iter().enumerate().map(|(k, w)| (format!("in{k}"), w.to_string(), "node".to_string())).collect(),
        geom_visible: true,
        node_type: String::new(),
        inputs: wires.len().max(1),
        outputs: 1,
    };
    // The simnet the user found it in: input1 above pull1, relax1
    // reading both, a column to the right and four rows down.
    g.set_nodes(&[
        node("input1", 3.0, -1.0, &[]),
        node("pull1", 3.0, 1.0, &["input1"]),
        node("relax1", 4.0, 3.0, &["pull1", "input1"]),
    ]);
    let line = |row: f32| 200.0 + row * 70.0;
    let across = |segs: &[WireSeg]| {
        segs.iter()
            .find_map(|s| match *s {
                WireSeg::Line(a, b) if (a.1 - b.1).abs() < 0.01 && (a.0 - b.0).abs() > 1.0 => Some(a.1),
                _ => None,
            })
            .expect("a run across")
    };
    for style in [WireStyle::Orthogonal, WireStyle::Rounded] {
        g.set_wire_style(Some(style));
        // input1 (row -1) into relax1's second port (row 3): row 0's line.
        let segs = g.wire_segments(0, 2, 1).unwrap();
        assert_eq!(across(&segs), line(0.0), "{style:?}: the first line below the source");
        // Nothing of it stands in pull1's body, under the source.
        let (px, py, pw, ph) = g.node_rect(1).unwrap();
        assert!(
            !segs.iter().any(|s| matches!(*s, WireSeg::Line(a, b) if segment_meets_rect(a, b, px, py, px + pw, py + ph))),
            "{style:?}: the wire runs through pull1"
        );
        // pull1 (row 1) into relax1 (row 3): row 2's line, as before.
        assert_eq!(across(&g.wire_segments(1, 2, 0).unwrap()), line(2.0));
    }
    // Adjacent rows: no line between the bodies, so halfway.
    g.set_nodes(&[node("a", 0.0, 0.0, &[]), node("b", 1.0, 1.0, &["a"])]);
    g.set_wire_style(Some(WireStyle::Orthogonal));
    let (start, end) = g.wire_endpoints(0, 1, 0).unwrap();
    assert_eq!(across(&g.wire_segments(0, 1, 0).unwrap()), start.1 + (end.1 - start.1) / 2.0);
    // A wire running up turns halfway too.
    g.set_nodes(&[node("a", 0.0, 3.0, &[]), node("b", 1.0, 0.0, &["a"])]);
    let (start, end) = g.wire_endpoints(0, 1, 0).unwrap();
    assert_eq!(across(&g.wire_segments(0, 1, 0).unwrap()), start.1 + (end.1 - start.1) / 2.0);
}

#[test]
fn a_wire_style_is_named_either_way_in_any_case() {
    for style in WireStyle::ALL {
        assert_eq!(WireStyle::parse(style.name()), Some(style));
        assert_eq!(WireStyle::parse(style.label()), Some(style));
        assert_eq!(WireStyle::parse(&style.name().to_uppercase()), Some(style));
    }
    assert_eq!(WireStyle::parse("wiggly"), None);
}

/// The grid has one size per axis, the pitch, and a node is CENTRED on
/// the intersection its position names — its body straddles the lines
/// rather than filling a cell between them. The body's size is its own:
/// changing the pitch moves nodes apart without resizing them.
#[test]
fn nodes_are_centred_on_lattice_intersections() {
    let mut g = two_nodes();
    assert_eq!(g.grid_pitch(), (100.0, 60.0));
    assert_eq!(g.node_size(), (80.0, 40.0));
    g.set_grid_pitch(200.0, 90.0);
    assert_eq!(g.node_size(), (80.0, 40.0), "the pitch does not size the node");
    g.set_grid_pitch(100.0, 60.0);
    // Node a is on the (140, 120) intersection: its 80 x 40 body is
    // centred there.
    let (x, y, w, h) = g.node_rect(0).unwrap();
    assert_eq!((x + w / 2.0, y + h / 2.0), (140.0, 120.0));
    assert_eq!((x, y, w, h), (100.0, 100.0, 80.0, 40.0));
    // Node b, at (1, 1), is one pitch away along each axis.
    let (x, y, w, h) = g.node_rect(1).unwrap();
    assert_eq!((x + w / 2.0, y + h / 2.0), (240.0, 180.0));
}

/// The cell-and-gap setters describe the same lattice — a cell plus its
/// gap is a pitch, and the cell is the node body — in either order, so a
/// host still speaking them gets exactly the geometry it asked for.
#[test]
fn cell_and_gap_setters_describe_the_same_lattice() {
    let mut g = Graph::new();
    g.set_grid_sizes(140.0, 70.0);
    g.set_skipped_sizes(35.0, 35.0);
    assert_eq!(g.grid_pitch(), (175.0, 105.0));
    assert_eq!(g.node_size(), (140.0, 70.0));
    assert_eq!(g.grid_sizes(), (140.0, 70.0));
    assert_eq!(g.skipped_sizes(), (35.0, 35.0));

    let mut h = Graph::new();
    h.set_skipped_sizes(35.0, 35.0);
    h.set_grid_sizes(140.0, 70.0);
    assert_eq!(h.grid_pitch(), (175.0, 105.0));
    assert_eq!(h.node_size(), (140.0, 70.0));
}

#[test]
fn port_click_starts_and_completes_a_connection() {
    let mut ctx = UiContext::new();
    let g = ctx.insert(two_nodes());

    // Ports float OUTSIDE the node box (port_center): node a's output
    // hangs below the bottom-center of (100,100,80,40), node b's input
    // above the top-center of (200,160,80,40).
    let (ax, ay) = ctx[g].port_center(0, PortType::Output, 0).expect("node a output port");
    assert!(ay > 140.0, "output port sits below the node's bottom edge");
    assert!(ctx.lend_h(g, |w, ctx| w.mouse_input(MouseButton::Left, ElementState::Pressed, ax, ay, ctx)).unwrap());
    let (bx, by) = ctx[g].port_center(1, PortType::Input, 0).expect("node b input port");
    assert!(by < 160.0, "input port sits above the node's top edge");
    assert!(ctx.lend_h(g, |w, ctx| w.mouse_input(MouseButton::Left, ElementState::Pressed, bx, by, ctx)).unwrap());

    let pending = GraphController::take_pending_connection(&mut *ctx[g]);
    assert_eq!(pending, Some(("b".to_string(), "alpha".to_string())));
}

/// Every parameter a host types `node` is a wire, the k-th into input
/// port k, and a connection dropped on a port says which; a host that
/// types none keeps its one `input` wire.
#[test]
fn every_node_parameter_is_a_wire_into_its_own_port() {
    let mut ctx = UiContext::new();
    let g = ctx.insert(two_nodes());
    let wire = |n: &str, v: &str| (n.to_string(), v.to_string(), "node".to_string());
    let node = |id: &str, col: f32, row: f32, parameters: Vec<(String, String, String)>, inputs: usize| GraphNode {
        id: id.into(),
        name: id.into(),
        position: (col, row),
        parameters,
        geom_visible: true,
        node_type: String::new(),
        inputs,
        outputs: 1,
    };
    ctx[g].set_nodes(&[
        node("a", 0.0, 0.0, vec![], 0),
        node("b", 2.0, 0.0, vec![], 0),
        node("sw", 1.0, 2.0, vec![wire("Input", "a"), wire("Input 2", "b"), wire("Input 3", ""), ("Index".into(), "1".into(), "spinbox".into())], 3),
        node("old", 3.0, 2.0, vec![("input".into(), "b".into(), "string".into())], 1),
    ]);
    assert_eq!(ctx[g].wire_pairs(), vec![(0, 2, 0), (1, 2, 1), (1, 3, 0)], "two wires into the switch, each its port; the untyped host's one");
    let (_, end) = ctx[g].wire_endpoints(1, 2, 1).unwrap();
    assert_eq!(Some(end), ctx[g].port_center(2, PortType::Input, 1), "into its own port");

    // Dropped on the switch's third port: port 2.
    let (ax, ay) = ctx[g].port_center(0, PortType::Output, 0).unwrap();
    assert!(ctx.lend_h(g, |w, ctx| w.mouse_input(MouseButton::Left, ElementState::Pressed, ax, ay, ctx)).unwrap());
    let (px, py) = ctx[g].port_center(2, PortType::Input, 2).unwrap();
    assert!(ctx.lend_h(g, |w, ctx| w.mouse_input(MouseButton::Left, ElementState::Pressed, px, py, ctx)).unwrap());
    assert_eq!(GraphController::take_pending_connection_to_port(&mut *ctx[g]), Some(("sw".to_string(), "a".to_string(), 2)));
}

#[test]
fn the_painted_geometry_is_the_tagged_geometry_rounded() {
    use crate::widget::WidgetHostExt;
    let g = two_nodes();

    // What a host drawing the nodes itself reads (the designer) and what the graph paints
    // describe the same quads: the paint adds only the widget background up front.
    let plain: Vec<_> = g
        .geometry_quads_tagged(g.content_rect())
        .into_iter()
        .map(|(qx, qy, qw, qh, qc, _)| (qx, qy, qw, qh, qc))
        .collect();
    let rounded: Vec<_> = g
        .painted_prims()
        .into_iter()
        .filter_map(|prim| match prim {
            crate::scene::paint::Prim::RoundedRect { rect, radius, corners, color } => {
                Some((rect.x, rect.y, rect.width, rect.height, radius, color, corners))
            }
            _ => None,
        })
        .collect();
    assert!(!plain.is_empty());
    assert_eq!(rounded.len(), plain.len() + 1);
    for ((px, py, pw, ph, pc), (rx, ry, rw, rh, _, rc, _)) in plain.iter().zip(rounded.iter().skip(1)) {
        assert_eq!((px, py, pw, ph, pc), (rx, ry, rw, rh, rc));
    }

    // Node bodies carry the node corner radius.
    let node_radius = crate::layout::graph_node_corner_radius();
    let node_entries: Vec<_> = rounded
        .iter()
        .filter(|(qx, qy, qw, qh, ..)| g.is_node_rect(*qx, *qy, *qw, *qh))
        .collect();
    assert_eq!(node_entries.len(), 2, "both node bodies present");
    for entry in node_entries {
        assert_eq!(entry.4, node_radius);
        assert_eq!(entry.6, (true, true, true, true));
    }
}
