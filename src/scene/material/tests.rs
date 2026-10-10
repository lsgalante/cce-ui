use super::*;

fn frosted() -> Frost {
    Frost::Frosted { compression: 0.6, refraction: 0.3, radius: Frost::DEFAULT_RADIUS }
}

/// The encoding rule, stated once: root stays positive whatever the
/// frost, nested frost is the sentinel, nested opaque passes through.
#[test]
fn fill_encodes_by_role() {
    let tint = [0.1, 0.2, 0.3, 0.8];
    assert_eq!(Material::fill_tint(tint, frosted(), PlateRole::Root)[3], 0.8, "root frost is the compositor's");
    assert_eq!(Material::fill_tint(tint, Frost::Unfrosted, PlateRole::Root)[3], 0.8);
    assert_eq!(Material::fill_tint(tint, frosted(), PlateRole::Nested)[3], -0.8, "nested frost = sentinel");
    assert_eq!(Material::fill_tint(tint, Frost::Unfrosted, PlateRole::Nested), tint, "no frost, no encoding");
    // A caller that hands a negative alpha in is normalised, not doubled.
    assert_eq!(Material::fill_tint([0.0, 0.0, 0.0, -0.5], frosted(), PlateRole::Nested)[3], -0.5);
    assert_eq!(Material::fill_tint([0.0, 0.0, 0.0, -0.5], Frost::Unfrosted, PlateRole::Root)[3], 0.5);
    let m = Material::opaque(tint).with_frost(frosted());
    assert_eq!(m.fill(PlateRole::Nested), Material::fill_tint(tint, frosted(), PlateRole::Nested));
}

/// The pane rung is `param_plate_fill`'s old arithmetic exactly: the
/// tint at plate opacity, negated under plate blur.
#[test]
fn pane_resolves_like_param_plate_fill_did() {
    let _lock = crate::color::test_color_state_lock();
    let old = |blur: bool| {
        let mut c = crate::color::param_bg_color();
        c[3] *= crate::layout::plate_opacity();
        if blur {
            c[3] = -c[3].abs();
        }
        c
    };
    for blur in [false, true] {
        crate::color::set_plate_blur(blur);
        assert_eq!(Material::pane().fill(PlateRole::Nested), old(blur), "blur={blur}");
        assert_eq!(crate::color::param_plate_fill(), old(blur), "blur={blur}");
        assert_eq!(Material::pane().frost.is_frosted(), blur);
    }
    crate::color::set_plate_blur(false);
}

/// The finish defaults are the literals the shader shipped with, the DE
/// finish reads the three getters, and the push-constant layout is
/// unchanged.
///
/// Read through `from_style` unpinned, the defaults were whatever the
/// process-wide values were: the machine's config, or what a test that
/// reloads the knobs had left there — `binding_semantics` left spec 0.5
/// until 2026-10-05, so this failed whenever it ran after that one. The
/// defaults are checked as the statics' initial values and the getters
/// pinned on this thread.
#[test]
fn finish_defaults_and_layout() {
    use crate::color::{FINISH_CURVATURE_DEFAULT, FINISH_SHININESS_DEFAULT, FINISH_SPEC_DEFAULT};
    assert_eq!((FINISH_SPEC_DEFAULT, FINISH_SHININESS_DEFAULT, FINISH_CURVATURE_DEFAULT), (0.4, 24.0, 0.2));
    crate::color::set_finish_spec(0.9);
    crate::color::set_finish_shininess(30.0);
    crate::color::set_finish_curvature(0.3);
    let f = Finish::from_style();
    assert_eq!((f.spec, f.shininess, f.curvature), (0.9, 30.0, 0.3));
    assert_eq!(f.to_array(), [f.strength, f.spec, f.shininess, f.curvature]);
}

/// The frost recipe reads the two plate-rung keys and carries the default
/// kernel; the flag form is today's `blur: bool`.
///
/// All THREE knobs are pinned on this thread, the radius included: the
/// setters write a per-thread overlay, but a `reload_colors` writes the
/// process-wide globals, and an absent frost knob keeps its last value
/// by design — so a neighbour's reload of `frost radius=3.0` outlived
/// its closing empty reload, and this test read 3.0 for the default
/// whenever that neighbour ran first (one run in eight, 2026-09-28).
#[test]
fn frost_from_style_and_flag() {
    let _lock = crate::color::test_color_state_lock();
    crate::color::set_plate_backdrop_compression(0.6);
    crate::color::set_plate_refraction(0.3);
    crate::color::set_plate_frost_radius(Frost::DEFAULT_RADIUS);
    assert_eq!(Frost::from_style(), frosted());
    assert_eq!(Frost::from_flag(false), Frost::Unfrosted);
    assert!(Frost::from_flag(true).is_frosted());
    crate::color::set_plate_backdrop_compression(0.0);
    crate::color::set_plate_refraction(0.0);
}

/// A floor darkens the tint by the overlay's strength and keeps
/// everything else: alpha, frost, finish.
#[test]
fn floor_darkens_and_carries_the_frost() {
    let m = Material::opaque([0.5, 0.5, 0.5, 0.7]).with_frost(frosted());
    let f = m.floor(false);
    let keep = 1.0 - crate::color::WELL_FLOOR[3];
    assert!((f.tint[0] - 0.5 * keep).abs() < 1e-6);
    assert_eq!(f.tint[3], 0.7);
    assert_eq!(f.frost, m.frost);
    assert_eq!(f.finish, m.finish);
    assert!(m.floor(true).tint[0] > f.tint[0], "lifted rises toward the plate");
    assert_eq!(m.flat_control(), m);
}

/// The legacy bridge round-trips every fill the old API accepted, and
/// the role resolution agrees with the encoding function.
#[test]
fn from_fill_round_trips_and_for_role_matches_fill() {
    for c in [[0.1, 0.2, 0.3, 0.8], [0.1, 0.2, 0.3, -0.8], [0.0; 4], [0.5, 0.5, 0.5, 1.0]] {
        let m = Material::from_fill(c);
        assert_eq!(m.fill(PlateRole::Nested), c, "{c:?}");
        assert_eq!(m.frost.is_frosted(), c[3] < 0.0);
        assert!(m.tint[3] >= 0.0, "tint alpha is never negative");
        for role in [PlateRole::Root, PlateRole::Nested] {
            assert_eq!(m.for_role(role).fill(PlateRole::Nested), m.fill(role), "{c:?} {role:?}");
        }
    }
    assert_eq!(Material::from_fill([0.0, 0.0, 0.0, -0.5]).frost, Frost::from_style());
    assert!(Material::face([0.3, 0.3, 0.3, 0.0]).is_none(), "transparent = no face");
    assert!(Material::face([0.3, 0.3, 0.3, -0.5]).is_some_and(|m| m.frost.is_frosted()));
    assert_eq!(Material::face([0.3, 0.3, 0.3, 0.7]).map(|m| m.tint), Some([0.3, 0.3, 0.3, 0.7]));
}

/// The pack is exact on its own grid, monotone, and never mixes the two
/// halves; the shader's literals are the ones the Rust twin uses.
#[test]
fn frost_pack_round_trips() {
    for i in [0u32, 1, 2, 613, 614, 2047, 2048, 4094, 4095] {
        for j in [0u32, 1, 819, 4095] {
            let (c, r) = (i as f32 / Frost::PACK_MAX, j as f32 / Frost::PACK_MAX);
            let f = Frost::Frosted { compression: c, refraction: r, radius: 5.5 };
            let [z, w] = f.pack(2.0);
            let (c2, r2) = Frost::unpack(z);
            assert!((c2 - c).abs() < 1e-6 && (r2 - r).abs() < 1e-6, "{i},{j}: {c},{r} -> {c2},{r2}");
            assert_eq!(w, 11.0);
            assert!(z < (1u32 << 24) as f32, "packed value must stay an exact f32 integer");
        }
    }
    // 0.6 / 0.3 (the designer's recipe) survive to better than a 1/255 step.
    let (c, r) = Frost::unpack(Frost::Frosted { compression: 0.6, refraction: 0.3, radius: 0.0 }.pack(1.0)[0]);
    assert!((c - 0.6).abs() < 1.0 / 510.0 && (r - 0.3).abs() < 1.0 / 510.0);
    assert_eq!(Frost::Unfrosted.pack(2.0), [0.0, 0.0]);
    assert_eq!(Frost::Frosted { compression: 0.0, refraction: 0.0, radius: 0.0 }.pack(2.0), [0.0, 0.0]);

    let wgsl = include_str!("../../draw/shader2d.wgsl");
    let lit = |name: &str| -> f32 {
        let rest = wgsl.split(&format!("const {name}: f32 = ")).nth(1).unwrap_or_else(|| panic!("{name} missing"));
        rest.split(';').next().unwrap().trim().parse().unwrap()
    };
    assert_eq!(lit("FROST_PACK_MAX"), Frost::PACK_MAX);
    assert_eq!(lit("FROST_PACK_BASE"), Frost::PACK_BASE);
    // The droplet's and the raw-vertex fallback's stride is the panel's
    // default kernel in physical px: DEFAULT_RADIUS × scale 2 / 2.
    assert_eq!(lit("LEGACY_STRIDE"), Frost::DEFAULT_RADIUS * 2.0 / 2.0);
}

const DESIGNER_LEGACY: &str = r##"
    style {
        surface {
            param color=(rgba)"#05050840"
            plate {
                frost compression=(f64)0.6 refraction=(f64)0.3
                root corner_radius=(i64)24
            }
            relief light=(f64)0.08
        }
    }
"##;

const DESIGNER_NAMED: &str = r##"
    style {
        surface {
            material {
                glass {
                    color (rgba)"#05050840"
                    frost compression=(f64)0.6 refraction=(f64)0.3
                }
            }
            plate material="glass" {
                root corner_radius=(i64)24
            }
            relief light=(f64)0.08
        }
    }
"##;

/// The designer's frosted pane spelled with the legacy keys and as a
/// named material bound to the pane rung resolve to the SAME material —
/// the step-4 exit test: same Material, same bytes (steps 2–3).
#[test]
fn named_material_round_trips_the_legacy_spelling() {
    let _lock = crate::color::test_color_state_lock();
    crate::layout::lazy_init_style_registry();
    let _ = crate::color::plate_blur(); // fire the once-per-process load BEFORE the reload
    crate::layout::set_plate_opacity(1.0);
    crate::color::reload_colors(DESIGNER_LEGACY);
    assert_eq!(crate::color::material_binding(PlateRung::Pane), None);
    let legacy = Material::pane();
    assert!(legacy.frost.is_frosted());
    assert!((legacy.tint[3] - 0x40 as f32 / 255.0).abs() < 1e-6, "{:?}", legacy.tint);

    crate::color::reload_colors(DESIGNER_NAMED);
    assert_eq!(crate::color::material_binding(PlateRung::Pane).as_deref(), Some("glass"));
    let named = Material::pane();
    assert_eq!(named.tint, legacy.tint);
    assert_eq!(named.finish, legacy.finish);
    match (named.frost, legacy.frost) {
        (Frost::Frosted { compression: c1, refraction: r1, radius: d1 }, Frost::Frosted { compression: c2, refraction: r2, radius: d2 }) => {
            assert!((c1 - c2).abs() < 1e-6 && (r1 - r2).abs() < 1e-6 && d1 == d2, "{:?} vs {:?}", named.frost, legacy.frost);
        }
        other => panic!("{other:?}"),
    }
    // The DE recipe follows the bound pane.
    assert_eq!(Frost::from_style(), named.frost);
    assert_eq!(Material::named("glass"), Some(named));
    assert_eq!(Material::named("nope"), None);
    // The other rungs are unbound and unchanged.
    assert_eq!(Material::root(), Material::legacy(PlateRung::Root));
    assert_eq!(Material::control(), Material::legacy(PlateRung::Control));
    assert_eq!(crate::color::material_names(), vec!["glass".to_string()]);
    // Bindings and nodes are replaced wholesale by every load, so an
    // empty document unbinds every rung for the tests that follow.
    crate::color::reload_colors("");
    assert_eq!(crate::color::material_binding(PlateRung::Pane), None);
}

/// The default material's frost is ONE block — `plate { frost radius=…
/// compression=… refraction=… }`, the shape a named material's `frost`
/// child already had: a bare `frost` is frosted at the defaults, `frost
/// (bool)false` is not. The four scattered keys it replaced (`blur`,
/// `radius`, `backdrop_compression`, `refraction`) are RETIRED, not
/// aliases: a config carrying one is reported by path
/// (`color::retired_surface_keys`) and the key does nothing — `blur=true`
/// alone is sharp, a flat `radius` moves nothing, and the old knob name
/// inside the block is ignored too.
#[test]
fn the_frost_block_is_the_only_spelling_of_the_default_recipe() {
    let _lock = crate::color::test_color_state_lock();
    crate::layout::lazy_init_style_registry();
    let _ = crate::color::plate_blur();
    let doc = |plate: &str| format!("style {{\n surface {{\n plate {{\n {plate}\n }}\n }}\n}}\n");
    let load = |plate: &str| {
        crate::color::reload_colors(&doc(plate));
        (crate::color::plate_blur(), Frost::from_style())
    };
    let retired = |plate: &str| crate::color::retired_surface_keys(&crate::config::parse_kdl_to_json(&doc(plate)));
    let frosted = |c: f32, r: f32, rad: f32| Frost::Frosted { compression: c, refraction: r, radius: rad };
    assert_eq!(load("frost radius=(f64)3.0 compression=(f64)0.4 refraction=(f64)0.1"), (true, frosted(0.4, 0.1, 3.0)));
    assert!(retired("frost radius=(f64)3.0 compression=(f64)0.4 refraction=(f64)0.1").is_empty());
    assert_eq!(load("frost backdrop_compression=(f64)0.2"), (true, frosted(0.4, 0.1, 3.0)), "the old knob name inside the block is ignored; unset knobs keep their last value");
    assert_eq!(retired("frost backdrop_compression=(f64)0.2"), vec!["style.surface.plate.frost.backdrop_compression"]);
    assert!(load("frost").0, "a bare `frost` is frosted");
    assert!(!load("frost (bool)false").0);
    // `from_style` is the RECIPE, `plate_blur` the switch: the retired
    // keys flip neither — the switch stays off and the radius stays 3.
    assert_eq!(load("blur (bool)true\n radius (f64)2.0"), (false, frosted(0.4, 0.1, 3.0)), "the retired spelling frosts nothing and moves nothing");
    assert_eq!(retired("blur (bool)true\n radius (f64)2.0"), vec!["style.surface.plate.blur", "style.surface.plate.radius"]);
    assert_eq!(load("blur (bool)false\n frost compression=(f64)0.7"), (true, frosted(0.7, 0.1, 3.0)), "the block is read, the retired key is not");
    // Leave the globals as they were found. A reload writes them for
    // the whole process, and an unset knob KEEPS its value (asserted
    // above), so the empty reload alone left radius 3 / compression 0.7
    // / refraction 0.1 behind for every test after this one.
    crate::color::reload_colors(&doc(&format!(
        "frost radius=(f64){} compression=(f64)0.0 refraction=(f64)0.0",
        Frost::DEFAULT_RADIUS
    )));
    crate::color::reload_colors(&doc("frost (bool)false"));
    crate::color::reload_colors("");
    assert_eq!(Frost::from_style(), frosted(0.0, 0.0, Frost::DEFAULT_RADIUS), "the globals are back at their defaults");
    assert!(!crate::color::plate_blur());
}

/// `style.surface.plate.pane.color` is the pane tint WHOLE — its alpha is
/// the tint strength — where the legacy `param.color` is still multiplied
/// by the top-level `plate_opacity` line.
#[test]
fn the_pane_colour_spelling_is_the_whole_tint() {
    let _lock = crate::color::test_color_state_lock();
    crate::layout::lazy_init_style_registry();
    let _ = crate::color::plate_blur();
    let was = crate::layout::plate_opacity();
    crate::layout::set_plate_opacity(0.5);
    crate::color::reload_colors("style {\n surface {\n param color=(rgba)\"#10101880\"\n }\n}\n");
    assert!(!crate::color::pane_color_is_whole());
    let a = Material::pane().tint[3];
    assert!((a - 0.25).abs() < 1e-2, "legacy: alpha 0.5 x plate_opacity 0.5, got {a}");
    crate::color::reload_colors("style {\n surface {\n plate {\n pane color=(rgba)\"#10101880\"\n }\n }\n}\n");
    assert!(crate::color::pane_color_is_whole());
    let a = Material::pane().tint[3];
    assert!((a - 0.5).abs() < 1e-2, "spelled whole: alpha 0.5 as written, got {a}");
    crate::layout::set_plate_opacity(was);
    crate::color::reload_colors("");
}

/// A binding wins over the legacy keys, a node without `frost` is
/// unfrosted whatever `plate blur` says, unset fields fall back to the
/// rung, and a binding to an undefined name degrades to the legacy
/// material.
#[test]
fn binding_semantics() {
    let _lock = crate::color::test_color_state_lock();
    crate::layout::lazy_init_style_registry();
    let _ = crate::color::plate_blur(); // fire the once-per-process load BEFORE the reload
    // The DE finish this reload changes is process-wide, and an absent
    // knob keeps its value through the empty reload below: put back
    // what was there. (`set_finish_*` cannot — under `cfg(test)` a
    // setter writes this thread's overlay, not the globals.)
    let finish_before = Finish::from_style();
    crate::color::reload_colors(r##"
        style {
            surface {
                material {
                    plastic {
                        finish spec=(f64)0.25 shininess=(f64)12.0
                    }
                    matte {
                        color (rgba)"#20202080"
                        finish light=(f64)0.3
                    }
                }
                plate blur=(bool)true material="plastic"
                relief light=(f64)0.15 spec=(f64)0.5 shininess=(f64)20.0 curvature=(f64)0.1
            }
            control material="ghost"
        }
    "##);
    let pane = Material::pane();
    assert_eq!(pane.frost, Frost::Unfrosted, "no frost child = unfrosted, blur flag or not");
    assert_eq!(pane.tint, Material::legacy(PlateRung::Pane).tint, "no color = the rung's tint");
    assert_eq!(pane.finish.spec, 0.25);
    assert_eq!(pane.finish.shininess, 12.0);
    assert_eq!(pane.finish.curvature, 0.1, "unset finish keys take the DE's (relief curvature)");
    assert_eq!(Finish::from_style().spec, 0.5, "style.surface.relief.spec is the DE finish");
    assert_eq!(Material::named("matte").map(|m| m.finish.strength), Some(0.3 / 0.15));
    assert_eq!(Material::control(), Material::legacy(PlateRung::Control), "unknown name = unbound");
    assert!(Frost::from_style().is_frosted(), "an opaque bound pane leaves the DE recipe to the keys");
    // Multi-line: `a { b }` on one line does not parse, and the loader
    // reads that as an empty document.
    crate::color::reload_colors(&format!(
        "style {{\n surface {{\n relief spec=(f64){:?} shininess=(f64){:?} curvature=(f64){:?}\n }}\n}}\n",
        finish_before.spec, finish_before.shininess, finish_before.curvature,
    ));
    crate::color::reload_colors("");
    assert_eq!(Finish::from_style(), finish_before, "the DE finish is back for the tests after this one");
}

/// A popover is the base colour at menu opacity, frosted — the bytes the
/// three menu sites used to write by negating an alpha.
#[test]
fn popover_is_the_menu_recipe() {
    let _lock = crate::color::test_color_state_lock();
    let m = Material::popover([0.1, 0.2, 0.3, 1.0]);
    let mut old = [0.1, 0.2, 0.3, 1.0];
    old[3] = -crate::color::menu_opacity();
    assert_eq!(m.fill(PlateRole::Nested), old);
    assert!(m.frost.is_frosted());
}

/// A popover's frost compresses at the menu key, not the DE recipe's:
/// the two are independent dials.
#[test]
fn popover_compresses_at_the_menu_key() {
    let _lock = crate::color::test_color_state_lock();
    crate::color::set_plate_backdrop_compression(0.1);
    crate::color::set_menu_compression(0.7);
    let m = Material::popover([0.1, 0.2, 0.3, 1.0]);
    let Frost::Frosted { compression, .. } = m.frost else { panic!("popover is frosted") };
    assert!((compression - 0.7).abs() < 1e-6, "popover compression {compression}");
    let Frost::Frosted { compression: pane, .. } = Frost::from_style() else { panic!("recipe is frosted") };
    assert!((pane - 0.1).abs() < 1e-6, "recipe compression {pane}");
    crate::color::set_plate_backdrop_compression(0.0);
    crate::color::set_menu_compression(0.6);
}

/// The control-face rule: opaque or nothing.
#[test]
fn control_face_is_opaque_or_none() {
    let face = Material::control_face([0.2, 0.3, 0.4, 0.5]).expect("a fill is a face");
    assert_eq!(face.tint, [0.2, 0.3, 0.4, 1.0]);
    assert_eq!(face.frost, Frost::Unfrosted);
    assert!(Material::control_face([0.2, 0.3, 0.4, 0.0]).is_none());
    assert_eq!(Material::control().frost, Frost::Unfrosted);
    assert_eq!(Material::root().frost, Frost::Unfrosted);
}
