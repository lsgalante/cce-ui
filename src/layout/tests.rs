use crate::widget::WidgetHost;

#[test]
fn unit_slots_resolve_through_the_metric_at_read_time() {
    use crate::units::{Len, Metric, MetricSource};
    let mut reg = super::StyleRegistry::new();
    reg.set_len("probe_width", Len::mm(2.0));
    // `get_float` resolves against the PROCESS metric at read time — the
    // same number `Len::to_px` gives — so it tracks whatever the metric
    // is now, not what it was at load. (The metric itself is left alone:
    // it is process-global and the suite runs in parallel.)
    let live = reg.get_float("probe_width").unwrap();
    assert!((live - Len::mm(2.0).to_px()).abs() < 1e-4, "{live}");
    // Two metrics give two answers for the one stored length.
    let assumed = Metric::assumed(1.0);
    let panel = Metric::from_sizes(2.0, (1920.0, 1200.0), (344.0, 215.0), MetricSource::Measured).unwrap();
    let (a, b) = (Len::mm(2.0).resolve(&assumed), Len::mm(2.0).resolve(&panel));
    assert!((a - 2.0 * 96.0 / 25.4).abs() < 1e-3, "{a}");
    assert!((b - 2.0 * panel.px_per_mm).abs() < 1e-3, "{b}");
    assert_ne!(a, b);
    assert_eq!(reg.get_len("probe_width"), Some(Len::mm(2.0)));
    // A plain number written later wins, and reads back as px.
    reg.set_float("probe_width", 7.0);
    assert_eq!(reg.get_float("probe_width"), Some(7.0));
    assert_eq!(reg.get_len("probe_width"), Some(Len::px(7.0)));
}

use super::*;

/// A graph's wires and ports reach a flat host: `render_widget` hands
/// its strokes to `RenderTarget::line` (and arcs and discs to `arc` and
/// `circle`), where it used to drop them, so cce-files' Graph page drew
/// nodes and no wires.
#[test]
fn a_graphs_wires_reach_a_flat_host() {
    #[derive(Default)]
    struct Strokes {
        lines: usize,
        circles: usize,
    }
    impl RenderTarget for Strokes {
        fn rect(&mut self, _: [f32; 4], _: f32, _: f32, _: f32, _: f32) {}
        fn text(&mut self, _: &str, _: f32, _: f32, _: f32, _: [f32; 4]) {}
        fn line(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: [f32; 4], _: crate::scene::paint::Cap) {
            self.lines += 1;
        }
        fn arc(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32, _: [f32; 4]) {
            self.lines += 1;
        }
        fn circle(&mut self, _: f32, _: f32, _: f32, _: [f32; 4]) {
            self.circles += 1;
        }
    }
    use crate::widget::GraphController;
    let node = |name: &str, pos: (f32, f32), input: Option<&str>| crate::widget::GraphNode {
        id: name.to_string(),
        name: name.to_string(),
        position: pos,
        parameters: input.map(|i| vec![("input".to_string(), i.to_string(), "string".to_string())]).unwrap_or_default(),
        geom_visible: true,
        node_type: String::new(),
        inputs: 1,
        outputs: 1,
    };
    let mut graph = crate::widget::Graph::new();
    graph.set_grid_origin(100.0, 100.0);
    graph.set_nodes(&[node("a", (0.0, 0.0), None), node("b", (0.0, 1.0), Some("a"))]);
    let mut pc = Strokes::default();
    let mut ctx = crate::context::UiContext::new();
    render_widget(&mut pc, &mut graph, 0.0, 0.0, 600.0, 600.0, &mut ctx);
    assert!(pc.lines > 0, "the wire a -> b reached the host");
    assert!(pc.circles > 0, "the ports reached the host");
}

/// A widget's glyph reaches a flat host: `render_widget` hands the
/// dropdown's arrow to the host's `RenderTarget::icon` by name. It used
/// to drop every image, so after the toolkit's symbols became glyphs a
/// flat host showed a dropdown with no arrow. Skipped where the icon set
/// is not checked out (no glyph uploads, so there is nothing to name).
#[test]
fn a_widget_glyph_reaches_a_flat_host() {
    if !std::path::Path::new(&crate::icons_dir()).join("chevron-down.svg").is_file() {
        eprintln!("skipped: no icon set");
        return;
    }
    #[derive(Default)]
    struct Icons(Vec<String>);
    impl RenderTarget for Icons {
        fn rect(&mut self, _: [f32; 4], _: f32, _: f32, _: f32, _: f32) {}
        fn text(&mut self, _: &str, _: f32, _: f32, _: f32, _: [f32; 4]) {}
        fn icon(&mut self, name: &str, _: crate::scene::layout::Rect, _: [f32; 4]) {
            self.0.push(name.to_string());
        }
    }
    let mut pc = Icons::default();
    let mut ctx = crate::context::UiContext::new();
    let mut dd = crate::widget::Dropdown::new(vec!["One".to_string(), "Two".to_string()], 0);
    render_widget(&mut pc, &mut dd, 10.0, 10.0, 160.0, 24.0, &mut ctx);
    assert!(pc.0.iter().any(|n| n == "chevron-down"), "the arrow reached the host: {:?}", pc.0);
}






struct MockWidget {
    base: crate::widget::Widget,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl WidgetHost for MockWidget {
    crate::impl_widget_base!(MockWidget);
    fn rect(&self) -> (f32, f32, f32, f32) {
        (self.x, self.y, self.w, self.h)
    }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.x = x;
        self.y = y;
        self.w = w;
        self.h = h;
    }
}

#[test]
fn test_grid_layout() {
    // Test single column layout (width = 200, min_col_width = 300)
    let grid1 = Grid::new(10.0, 20.0, 200.0, 300.0, 10.0, 1);
    assert_eq!(grid1.col_heights.len(), 1);
    assert_eq!(grid1.col_lefts[0], 10.0);
    assert_eq!(grid1.col_width, 200.0);

    // Test multi column layout (width = 700, min_col_width = 300, gap = 20)
    // count = floor((700 + 20) / (300 + 20)) = floor(720 / 320) = 2.
    // total_gap = 20 * 1 = 20.
    // col_width = (700 - 20) / 2 = 340.
    let mut grid2 = Grid::new(5.0, 15.0, 700.0, 300.0, 20.0, 2);
    assert_eq!(grid2.col_heights.len(), 2);
    assert_eq!(grid2.col_lefts[0], 5.0);
    assert_eq!(grid2.col_lefts[1], 365.0);
    assert_eq!(grid2.col_width, 340.0);

    assert_eq!(grid2.next_column(), 0);
    grid2.col_heights[0] += 50.0; // Column 0 height becomes 65.0
    assert_eq!(grid2.next_column(), 1);
    grid2.col_heights[1] += 30.0; // Column 1 height becomes 45.0
    assert_eq!(grid2.next_column(), 1);
    grid2.col_heights[1] += 30.0; // Column 1 height becomes 75.0
    assert_eq!(grid2.next_column(), 0);
    
    assert_eq!(grid2.max_height(), 75.0);
}


/// These keys have ONE home, the style registry (since 2026-10-08): the setter writes
/// it, the getter reads it, an unset key is its default, and a length in other units
/// resolves through the metric — which the slot each used to have, filled by a scan
/// of `key = number` lines, ignored. Writes go to this thread's test overlay, so no
/// other test sees them.
#[test]
fn a_style_key_lives_in_the_registry_alone() {
    set_button_height(33.0);
    assert_eq!(button_height(), 33.0);
    set_nested_section_label_alignment(2);
    assert_eq!(nested_section_label_alignment(), 2);
    set_dropdown_height(31.0);
    assert_eq!(dropdown_height(), 31.0);

    set_section_label_font("Circe Slab A 12");
    assert_eq!(section_label_font(), "Circe Slab A 12");
    set_section_label_font("");
    assert_eq!(section_label_font(), "Berkeley Mono", "an empty font falls back");

    // `button_padding` reads `paginator_tab_padding_y` when it has none of its own.
    get_style_registry().write().unwrap().set_float("paginator_tab_padding_y", 5.0);
    assert_eq!(button_padding(), 5.0);
    set_button_padding(9.0);
    assert_eq!(button_padding(), 9.0, "its own wins");

    let len = crate::units::Len::mm(5.0);
    get_style_registry().write().unwrap().set_len("textbox_height", len);
    assert_eq!(textbox_height(), len.to_px(), "a length in mm is honoured");
}

/// Another program's config keeps its own name in the registry: the compositor's
/// `layout { grid_gap 18 }` (its window-tiling gap) is `layout.grid_gap`, never the
/// toolkit's `grid_gap`, which the settings app's section flow reads. A path the
/// toolkit maps still lands on its key, and a flat top-level key stays flat.
#[test]
fn another_programs_keys_do_not_become_the_toolkits() {
    let val = crate::config::parse_kdl_to_json(
        "layout {\n    grid_gap (i64)18\n    gap (i64)48\n}\ntransparency {\n    plate_opacity (f64)0.5\n}\nwindow_manager {\n    control_relief (bool)true\n}\nstyle {\n    control {\n        button height=(i64)30\n    }\n}\nplate_corner_radius (i64)14\n",
    );
    let mut flat = String::new();
    super::flatten_json_to_flat_props(&val, "", &mut flat);
    let keys: Vec<&str> = flat.lines().filter_map(|l| l.split('=').next()).map(str::trim).collect();
    for leaked in ["grid_gap", "gap", "plate_opacity"] {
        assert!(!keys.contains(&leaked), "{leaked} leaked into the toolkit's keys: {keys:?}");
    }
    for kept in ["layout.grid_gap", "layout.gap", "transparency.plate_opacity", "control_relief", "button_height", "plate_corner_radius"] {
        assert!(keys.contains(&kept), "{kept} missing: {keys:?}");
    }
}

#[test]
fn test_column_gap() {
    // A legacy key: set (by config or setter) it is honoured; unset it
    // lands on the ladder — the root plate's gap.
    let gap = column_gap();
    match registry_float("column_gap") {
        Some(v) => assert_eq!(gap, v),
        None => assert_eq!(gap, root_plate_gap()),
    }
    assert!(gap.is_finite() && gap >= 0.0, "column gap {gap}");
}

#[test]
fn spacing_ladder_falls_back_rung_by_rung() {
    // Every rung getter is finite and non-negative, and the inset is the
    // roll plus the padding, whatever the config says.
    for v in [root_plate_padding(), root_plate_gap(), root_plate_inset(), plate_padding(), plate_gap(), control_gap()] {
        assert!(v.is_finite() && v >= 0.0, "{v}");
    }
    assert_eq!(root_plate_inset(), bevel_width() + root_plate_padding());
    if registry_float("plate_gap").is_none() {
        assert_eq!(plate_gap(), root_plate_gap());
    }
    if registry_float("control_gap").is_none() {
        assert_eq!(control_gap(), CONTROL_GAP);
    }
}

#[test]
fn test_print_fonts() {
    let db = crate::widget::get_font_db();
    for face in db.faces() {
        println!("FAMILY: {:?}", face.families);
    }
}





#[test]
fn test_spinbox_button_padding_config() {
    let padding = spinbox_button_padding();
    println!("Parsed spinbox button padding: {}", padding);
    assert!(padding >= 0.0);
}

/// The relief's two shapes are nodes — `relief { wall … ; edge … }` —
/// and each of their keys flattens to the registry key the flat legacy
/// spelling always did, so nothing downstream of the registry moved.
#[test]
fn relief_wall_and_edge_nodes_flatten_to_the_legacy_registry_keys() {
    let val: serde_json::Value = serde_json::json!({
        "style": { "surface": { "relief": {
            "width": 9.3,
            "wall": { "height": "0.3mm", "profile": "smooth;0:0,1:1" },
            "edge": { "height": 4.0, "profile": "smooth;0:0,1:0.9" }
        } } }
    });
    let mut flat = String::new();
    flatten_json_to_flat_props(&val, "", &mut flat);
    // A flat line is `key = value`, strings quoted — what reload_config parses.
    let value = |k: &str| {
        flat.lines()
            .find(|l| l.starts_with(&format!("{k} = ")))
            .map(|l| l[k.len() + 3..].trim_matches('"').to_string())
    };
    assert_eq!(value("bevel_height").as_deref(), Some("0.3mm"), "{flat}");
    assert_eq!(value("roll_height").as_deref(), Some("4"), "{flat}");
    assert_eq!(value("bevel_profile_spec").as_deref(), Some("smooth;0:0,1:1"), "{flat}");
    assert_eq!(value("roll_profile_spec").as_deref(), Some("smooth;0:0,1:0.9"), "{flat}");
    // The retired spellings — the flat geometry keys, `depth`, and the
    // knob keys that are editor state — reach NO registry key: a config
    // that says only these draws the defaults, and the load-time report
    // (`color::retired_surface_keys`) is what says why.
    let old: serde_json::Value = serde_json::json!({
        "style": { "surface": { "relief": {
            "depth": 0.3,
            "height": 2.0, "edge_height": 3.0, "profile": "a", "edge_profile": "b",
            "wall": { "knobs": "1,1,1" }, "edge_knobs": "2,2,2"
        } } },
        "window_manager": { "bevel_depth": 0.3, "bevel_width": 5.0, "bevel_shader": 0 }
    });
    let mut flat = String::new();
    flatten_json_to_flat_props(&old, "", &mut flat);
    for k in ["bevel_depth", "bevel_width", "bevel_shader", "bevel_height", "roll_height", "bevel_profile_spec", "roll_profile_spec", "profile_knobs"] {
        assert!(!flat.lines().any(|l| l.starts_with(&format!("{k} = "))), "{k} landed from a retired spelling: {flat}");
    }
    // And a file carrying BOTH spellings is what its current one says,
    // with no precedence pass in between — the retired one is not read.
    let both: serde_json::Value = serde_json::json!({
        "style": { "surface": { "relief": {
            "light": 0.15, "depth": 0.9,
            "height": 2.0, "wall": { "height": 5.0 },
            "edge_profile": "old", "edge": { "profile": "new" }
        } } }
    });
    let mut flat = String::new();
    flatten_json_to_flat_props(&both, "", &mut flat);
    assert_eq!(flat.lines().filter(|l| l.starts_with("bevel_height = ")).count(), 1, "{flat}");
    assert!(flat.contains("bevel_height = 5") && flat.contains("bevel_depth = 0.15"), "{flat}");
    assert!(flat.contains("roll_profile_spec = \"new\""), "{flat}");
}

/// The relief's shader toggle is `style.surface.relief.shader`, and a
/// `(bool)` works there: it flattens to the string "false", which the
/// getter reads (a number was the only thing the old float read saw).
#[test]
fn the_shader_toggle_lives_with_the_relief_and_takes_a_bool() {
    let val: serde_json::Value = serde_json::json!({
        "style": { "surface": { "relief": { "shader": false } } }
    });
    let mut flat = String::new();
    flatten_json_to_flat_props(&val, "", &mut flat);
    assert!(flat.lines().any(|l| l == "bevel_shader = false"), "{flat}");
    assert!(shader_on(None, None), "unset: the SDF branch");
    assert!(shader_on(Some(1.0), None) && !shader_on(Some(0.0), Some("true")), "a number decides when present");
    assert!(!shader_on(None, Some("false")) && !shader_on(None, Some("off")) && shader_on(None, Some("true")));
}

/// The control rung's key flattens to `control_corner_radius`, beside a
/// widget's own override — the config shape `control corner_radius=8 { button corner_radius=10 }`.
#[test]
fn control_rung_radius_flattens_beside_widget_overrides() {
    let val: serde_json::Value = serde_json::json!({
        "style": { "control": { "corner_radius": 8, "button": { "corner_radius": 10 } } }
    });
    let mut flat = String::new();
    flatten_json_to_flat_props(&val, "", &mut flat);
    assert!(flat.lines().any(|l| l.starts_with("control_corner_radius")), "{flat}");
    assert!(flat.lines().any(|l| l.starts_with("button_corner_radius")), "{flat}");
}

/// A registry-backed style pinned by one test is invisible to a test
/// beside it.
///
/// The graph-style test below used to pin ~20 of these and restore none
/// (it now reads the live registry instead: see
/// `graph_style_getters_resolve_their_own_registry_keys`). Before the
/// per-thread overlay that reached every test running alongside —
/// provably: `dual_geometry_views_stay_consistent` bakes
/// quads with `graph_node_corner_radius`, then re-reads the getter to
/// compare, and a write landing between the two made them disagree.
#[test]
fn a_pinned_style_is_private_to_its_thread() {
    let base = graph_node_corner_radius();
    set_graph_node_corner_radius(base + 17.0);
    assert_eq!(graph_node_corner_radius(), base + 17.0, "the pinning thread sees its own value");

    let elsewhere = std::thread::spawn(graph_node_corner_radius).join().unwrap();
    assert_eq!(elsewhere, base, "a thread beside it must still see the shared base");
}

/// Every graph style getter resolves the registry key it is named for,
/// falling back to its own documented default — checked against the live
/// registry as it stands. Nothing is written: the style registry and the
/// colour statics are process-global and the suite runs in parallel, so a
/// set/assert here would be visible to every other test (this one used to
/// set all twenty-one and restore none).
#[test]
fn graph_style_getters_resolve_their_own_registry_keys() {
    fn stored(key: &str) -> Option<f32> {
        crate::layout::lazy_init_style_registry();
        let reg = crate::layout::get_style_registry().read().unwrap();
        reg.get_float(key)
    }

    assert_eq!(graph_spacing_x(), stored("graph_spacing_x").unwrap_or(187.5));
    assert_eq!(graph_spacing_y(), stored("graph_spacing_y").unwrap_or(112.5));
    assert_eq!(graph_line_width(), stored("graph_line_width").unwrap_or(1.0));
    assert_eq!(graph_node_width(), stored("graph_node_width").unwrap_or(150.0));
    assert_eq!(graph_node_height(), stored("graph_node_height").unwrap_or(75.0));
    assert_eq!(graph_grid_snap(), stored("graph_grid_snap").unwrap_or(0.0) != 0.0);
    assert_eq!(graph_blur(), stored("graph_blur").unwrap_or(0.0));
    assert_eq!(graph_node_corner_radius(), stored("graph_node_corner_radius").unwrap_or(4.0));
    assert_eq!(graph_wire_size(), stored("graph_wire_size").unwrap_or(6.0));
    assert_eq!(
        graph_wire_activation_radius(),
        stored("graph_wire_activation_radius").unwrap_or(9.0)
    );
    assert_eq!(graph_connector_size(), stored("graph_connector_size").unwrap_or(8.0));
    assert_eq!(
        graph_connector_activation_radius(),
        stored("graph_connector_activation_radius").unwrap_or(12.0)
    );

    // The graph colours live behind statics in `color`, out of this
    // module's reach; what is checkable without writing them is that each
    // resolves to a real, in-gamut colour rather than an unparsed or
    // uninitialised one.
    let rgb: [(&str, [f32; 3]); 1] = [("graph_grid", crate::color::graph_grid_color())];
    for (name, c) in rgb {
        assert!(c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)), "{name}: {c:?}");
    }
    let rgba: [(&str, [f32; 4]); 10] = [
        ("graph_node", crate::color::graph_node_color()),
        ("graph_node_selected", crate::color::graph_node_selected_color()),
        ("graph_node_drag", crate::color::graph_node_drag_color()),
        ("node", crate::color::node_color()),
        ("node_selected", crate::color::node_selected_color()),
        ("node_drag", crate::color::node_drag_color()),
        ("graph_wire", crate::color::graph_wire_color()),
        ("graph_wire_highlight", crate::color::graph_wire_highlight_color()),
        ("graph_connector", crate::color::graph_connector_color()),
        ("graph_connector_highlight", crate::color::graph_connector_highlight_color()),
    ];
    for (name, c) in rgba {
        assert!(c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)), "{name}: {c:?}");
    }
}

#[test]
fn test_mosaic_layout() {
    use crate::widget::MosaicLayout;
    use crate::widget::ContainerLayout;
    
    let layout = MosaicLayout {
        gap: 10.0,
        padding_x: 5.0,
        padding_y: 5.0,
    };

    let mut dummy = crate::context::UiContext::new();
    let mut w1 = MockWidget { base: crate::widget::Widget::new(), x: 0.0, y: 0.0, w: 100.0, h: 50.0 };
    let mut w2 = MockWidget { base: crate::widget::Widget::new(), x: 0.0, y: 0.0, w: 100.0, h: 80.0 };
    let mut w3 = MockWidget { base: crate::widget::Widget::new(), x: 0.0, y: 0.0, w: 80.0, h: 40.0 };
    
    let children = vec![
        &mut w1 as *mut MockWidget as *mut (dyn WidgetHost + 'static),
        &mut w2 as *mut MockWidget as *mut (dyn WidgetHost + 'static),
        &mut w3 as *mut MockWidget as *mut (dyn WidgetHost + 'static),
    ];

    let _ = layout.layout(10.0, 20.0, 250.0, 500.0, &children, &mut dummy);

    assert_eq!(w1.x, 15.0);
    assert_eq!(w1.y, 25.0);
    assert_eq!(w2.x, 15.0);
    assert_eq!(w2.y, 85.0);
    assert_eq!(w3.x, 125.0);
    assert_eq!(w3.y, 25.0);
}

#[test]
fn test_reverse_mosaic_layout() {
    use crate::widget::ReverseMosaicLayout;
    use crate::widget::ContainerLayout;
    
    let layout = ReverseMosaicLayout {
        gap: 10.0,
        padding_x: 5.0,
        padding_y: 5.0,
    };

    let mut dummy = crate::context::UiContext::new();
    let mut w1 = MockWidget { base: crate::widget::Widget::new(), x: 0.0, y: 0.0, w: 100.0, h: 50.0 };
    let mut w2 = MockWidget { base: crate::widget::Widget::new(), x: 0.0, y: 0.0, w: 100.0, h: 80.0 };
    let mut w3 = MockWidget { base: crate::widget::Widget::new(), x: 0.0, y: 0.0, w: 80.0, h: 40.0 };
    
    let children = vec![
        &mut w1 as *mut MockWidget as *mut (dyn WidgetHost + 'static),
        &mut w2 as *mut MockWidget as *mut (dyn WidgetHost + 'static),
        &mut w3 as *mut MockWidget as *mut (dyn WidgetHost + 'static),
    ];

    let _ = layout.layout(10.0, 20.0, 250.0, 300.0, &children, &mut dummy);

    assert_eq!(w1.x, 15.0);
    assert_eq!(w1.y, 25.0);
    assert!((w1.w - 126.315).abs() < 0.01);
    assert!((w1.h - 103.57).abs() < 0.01);

    assert!((w3.x - 153.947).abs() < 0.01);
    assert_eq!(w3.y, 25.0);
    assert!((w3.w - 101.05).abs() < 0.01);
    assert!((w3.h - 82.857).abs() < 0.01);
}

/// The LUT is checked on `ramp_profile_lut`, the pure half of
/// `set_bevel_profile_keys` / `set_roll_profile_keys` — installing it
/// would restyle every wall in the process, and the suite runs in
/// parallel.
#[test]
fn bevel_profile_lut_integrates_to_the_curves_net_rise() {
    let n = crate::layout::BEVEL_PROFILE_SAMPLES as f32;
    let net_rise = |slopes: [f32; crate::layout::BEVEL_PROFILE_SAMPLES]| -> f32 {
        slopes.iter().map(|s| s / n).sum()
    };

    // The identity 0→1 curve: slopes sum/N to its net rise of 1 (the same
    // total step the analytic smoothstep carries).
    let slopes = super::ramp_profile_lut(&[(0.0, 0.0), (1.0, 1.0)], true).expect("a curve");
    let rise = net_rise(slopes);
    assert!((rise - 1.0).abs() < 0.001, "net rise {rise}");

    // A rim curve that returns to its start height nets zero.
    let slopes =
        super::ramp_profile_lut(&[(0.0, 0.5), (0.2, 1.0), (0.8, 1.0), (1.0, 0.5)], false)
            .expect("a curve");
    let rise = net_rise(slopes);
    assert!(rise.abs() < 0.001, "net rise {rise}");

    // Degenerate key lists are no curve at all — the installers take that
    // `None` as "clear back to the analytic profile".
    assert!(super::ramp_profile_lut(&[(0.0, 1.0)], false).is_none());
    assert!(super::ramp_profile_lut(&[], true).is_none());
}

#[test]
fn relief_profile_specs_parse_with_identity_sentinel() {
    // Absent and identity-smooth mean "analytic" — nothing to install.
    assert!(crate::layout::parse_relief_profile_spec(None).is_none());
    assert!(crate::layout::parse_relief_profile_spec(Some(
        crate::layout::RELIEF_PROFILE_IDENTITY_SPEC
    ))
    .is_none());
    // Garbage falls back to analytic instead of poisoning the walls.
    assert!(crate::layout::parse_relief_profile_spec(Some("not a spec")).is_none());
    // A real curve installs: keys and line type round-trip.
    let (keys, smooth) = crate::layout::parse_relief_profile_spec(Some(
        "linear;0.000:0.200,0.500:1.000,1.000:0.800",
    ))
    .expect("custom spec parses");
    assert!(!smooth);
    assert_eq!(keys.len(), 3);
    assert!((keys[1].0 - 0.5).abs() < 0.001 && (keys[1].1 - 1.0).abs() < 0.001);
    // Identity under a LINEAR line type is a real profile (a straight
    // chamfer), not the sentinel — only the smooth spelling is analytic.
    assert!(crate::layout::parse_relief_profile_spec(Some(
        "linear;0.000:0.000,1.000:1.000"
    ))
    .is_some());
}
