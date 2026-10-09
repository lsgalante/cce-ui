use super::*;

/// The hex that pins a built-in colour is NOT its floats times 255.
///
/// The style loader gamma-decodes every config hex; the constants in this
/// file are already linear. So `PARAM_BG = [0.10, 0.10, 0.14]` is spelled
/// `#595969` in config, and the naive `#1a1a24` decodes to a plate ten
/// times darker — silently, both being valid config. Pinned here because
/// it cost a measurement round: a sweep meant to hold the tint constant
/// was quietly sweeping it, and the two halves disagreed by 3x.
#[test]
fn the_hex_that_pins_a_default_round_trips_through_the_gamma_decode() {
    let decoded = parse_hex_rgba_linear("#595969").expect("valid hex");
    // RGB only — a 6-digit hex carries no alpha, which the asserts at the
    // bottom cover as its own hazard.
    for (got, want) in decoded.iter().take(3).zip(PARAM_BG.iter().take(3)) {
        assert!(
            (got - want).abs() < 0.005,
            "#595969 decodes to {decoded:?}, PARAM_BG is {PARAM_BG:?}"
        );
    }

    // The naive spelling, and how far off it lands.
    let naive = parse_hex_rgba_linear("#1a1a24").expect("valid hex");
    assert!(
        naive[0] < PARAM_BG[0] / 5.0,
        "#1a1a24 was supposed to be nowhere near PARAM_BG, got {naive:?}"
    );

    // Alpha is NOT decoded, and a 6-digit hex means OPAQUE — dropping the
    // last byte off a translucent plate colour does not leave it alone.
    assert!((parse_hex_rgba_linear("#05050840").unwrap()[3] - 0.251).abs() < 0.002);
    assert_eq!(parse_hex_rgba_linear("#050508").unwrap()[3], 1.0);
}

/// Rim refraction defaults OFF and clamps, like its neighbour.
/// The pane plates roll over the relief width — the one roll width —
/// and nothing, a loaded `plate.bevel_width` included, moves them off it.
#[test]
fn the_pane_roll_is_the_relief_width() {
    let _lock = test_color_state_lock();
    crate::layout::lazy_init_style_registry();
    let _ = plate_blur();
    reload_colors("style {\n surface {\n plate bevel_width=(f64)12.0\n }\n}\n");
    assert_eq!(plate_bevel_width(), crate::layout::bevel_width(), "a retired key moves nothing");
    assert_eq!(
        retired_surface_keys(&crate::config::parse_kdl_to_json("style {\n surface {\n plate bevel_width=(f64)12.0\n }\n}\n")),
        vec!["style.surface.plate.bevel_width"]
    );
    reload_colors("");
}

/// The four flat frost keys, the `backdrop_compression` spelling inside
/// a `frost` child, `plate.bevel_width`, `depth` (on the relief, or in a
/// material's `finish`), the relief's flat geometry keys and the
/// window_manager bevel spellings are reported by path; the blocks
/// themselves and a material's `compression` / `light` are not.
#[test]
fn retired_surface_keys_are_named_by_path_and_the_block_is_not() {
    let clean: serde_json::Value = serde_json::json!({ "style": { "surface": {
        "plate": { "frost": { "radius": 5.5, "compression": 0.0, "refraction": 0.0 }, "color": "#6c6c7bf2" },
        "relief": { "light": 0.15, "wall": { "height": 1.0, "profile": "a" }, "edge": { "height": 2.0, "profile": "b" } },
        "material": { "glass": { "frost": { "compression": 0.6 }, "finish": { "light": 0.2 } } }
    } }, "window_manager": { "corner_shape": 4.5, "control_relief": true } });
    assert!(retired_surface_keys(&clean).is_empty());
    let old: serde_json::Value = serde_json::json!({ "style": { "surface": {
        "plate": { "blur": true, "radius": 1.5, "backdrop_compression": 0.85, "refraction": 0.3,
                   "bevel_width": 12.0, "frost": { "backdrop_compression": 0.2 } },
        "relief": { "depth": 0.15, "height": 1.0, "profile": "a", "edge_height": 2.0, "edge_profile": "b" },
        "material": { "glass": { "frost": { "backdrop_compression": 0.6 }, "finish": { "depth": 0.2 } } }
    } }, "window_manager": { "bevel_depth": 0.15, "bevel_width": 9.3, "bevel_shader": 0 } });
    let mut old = old;
    old["style"]["surface"]["graph"] = serde_json::json!({ "cell_color": "#545467", "gap_color": "#48485b", "grid_color": "#48485b", "uniform_background": false });
    assert_eq!(retired_surface_keys(&old), vec![
        "style.surface.plate.blur",
        "style.surface.plate.radius",
        "style.surface.plate.backdrop_compression",
        "style.surface.plate.refraction",
        "style.surface.plate.bevel_width",
        "style.surface.plate.frost.backdrop_compression",
        "style.surface.relief.depth",
        "style.surface.relief.height",
        "style.surface.relief.profile",
        "style.surface.relief.edge_height",
        "style.surface.relief.edge_profile",
        "window_manager.bevel_depth",
        "window_manager.bevel_width",
        "window_manager.bevel_shader",
        "style.surface.material.glass.frost.backdrop_compression",
        "style.surface.material.glass.finish.depth",
        "style.surface.graph.cell_color",
        "style.surface.graph.gap_color",
        "style.surface.graph.uniform_background",
    ]);
    assert!(retired_surface_keys(&serde_json::json!({})).is_empty());
}

#[test]
fn plate_refraction_defaults_off_and_clamps() {
    assert_eq!(plate_refraction(), 0.0, "off unless a config asks");
    set_plate_refraction(0.6);
    assert_eq!(plate_refraction(), 0.6);
    set_plate_refraction(9.0);
    assert_eq!(plate_refraction(), 1.0);
    set_plate_refraction(-0.5);
    assert_eq!(plate_refraction(), 0.0);
}

/// The plate's backdrop compression defaults OFF and clamps.
///
/// Off is load-bearing: it is the behaviour every config already in the
/// wild has, and a toolkit-wide default that changed how every frosted
/// surface in the DE looks would arrive unannounced in eighteen apps.
/// Opting in is a per-app `style.surface.plate { frost compression=… }`.
#[test]
fn backdrop_compression_defaults_off_and_clamps() {
    assert_eq!(plate_backdrop_compression(), 0.0, "off unless a config asks");

    set_plate_backdrop_compression(0.85);
    assert_eq!(plate_backdrop_compression(), 0.85);

    // The shader clamps too, but a nonsense config value should not be
    // able to reach it and make the plate flat or inverted.
    set_plate_backdrop_compression(4.0);
    assert_eq!(plate_backdrop_compression(), 1.0);
    set_plate_backdrop_compression(-1.0);
    assert_eq!(plate_backdrop_compression(), 0.0);
}

/// A colour pinned by one test is invisible to a test beside it.
///
/// The invariant that ends the parallel-flake class here. These statics
/// are process-wide, and `test_graph_style_configuration` pins a dozen of
/// them without restoring any; before `style_write` became per-thread
/// under `cfg(test)`, every test running alongside it saw those values.
#[test]
fn a_pinned_colour_is_private_to_its_thread() {
    let base = node_color();
    let pinned = [0.123, 0.456, 0.789, 1.0];
    assert_ne!(base, pinned, "pick a value the config cannot already hold");

    set_node_color(pinned);
    assert_eq!(node_color(), pinned, "the pinning thread sees its own value");

    let elsewhere = std::thread::spawn(node_color).join().unwrap();
    assert_eq!(elsewhere, base, "a thread beside it must still see the shared base");
}
