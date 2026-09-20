//! The plate-path golden: one scene through every surface that carries a
//! material — root and nested plates in both frost regimes, the roll overlay,
//! bevels, every `ControlPlate` stance with and without a face and a focus
//! tint, inset plates, wells, the sphere and the droplet — tessellated at two
//! scales on both the SDF and the legacy edge path, and dumped as text.
//!
//! This is the RFC-material migration's exit test (`docs/rfc-material.md`
//! § 7): a step that must not move a pixel proves it by tessellating the SAME
//! SCENE to the SAME BYTES as the commit before it. The scene is built through
//! the public paint API, so the builder is rewritten as the API changes while
//! the dump it must reproduce is not.
//!
//! Not a fixture check — the dump depends on the machine's live style config
//! (roll width, radii, frost recipe), so it is generated and compared on the
//! same machine across commits:
//!
//! ```sh
//! CCE_PLATE_GOLDEN_WRITE=/tmp/golden.txt cargo test -p cce-ui --test plate_golden
//! # ...migrate...
//! CCE_PLATE_GOLDEN=/tmp/golden.txt       cargo test -p cce-ui --test plate_golden
//! ```
//!
//! With neither variable set the test passes trivially (it only asserts that
//! the scene tessellates).

use cce_ui::scene::layout::Rect;
use cce_ui::scene::paint::{ControlPlate, DropletSpec, PaintCtx, PlateSpec, PlateStance};
use std::fmt::Write as _;

fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { x, y, width: w, height: h }
}

/// The scene. Every branch of the tessellator that reads a colour or the
/// material push constants is reached at least once.
fn scene(sw: f32, sh: f32) -> cce_ui::scene::paint::DisplayList {
    let mut pc = PaintCtx::new();
    let depth = cce_ui::layout::bevel_width();
    let rr = (8.0, 8.0, 8.0, 8.0);

    // Root plate: opaque, and "frosted" (alpha must stay positive — the
    // compositor's regime).
    pc.plate_spec(&PlateSpec {
        rect: r(0.0, 0.0, sw, sh),
        material: cce_ui::scene::Material::opaque([0.10, 0.10, 0.14, 0.9]),
        window_corners: (true, true, true, true),
        depth,
    });
    pc.plate_spec(&PlateSpec {
        rect: r(0.0, 0.0, sw, sh),
        material: cce_ui::scene::Material::opaque([0.10, 0.10, 0.14, 0.9]).with_frost(cce_ui::scene::Frost::from_style()),
        window_corners: (true, true, true, true),
        depth,
    });
    // A carve into the root plate: exercises the CSG feature grouping.
    pc.recess(r(20.0, 20.0, 100.0, 30.0), rr, depth.min(6.0));

    // Nested pane plates: opaque, frosted (the in-app sentinel), on a right
    // edge (mixed corners).
    pc.plate_spec(&PlateSpec {
        rect: r(20.0, 60.0, 160.0, 100.0),
        material: cce_ui::scene::Material::opaque([0.2, 0.2, 0.25, 0.8]),
        window_corners: (false, false, false, false),
        depth: depth.min(6.0),
    });
    pc.plate_spec(&PlateSpec {
        rect: r(200.0, 60.0, 160.0, 100.0),
        material: cce_ui::scene::Material::opaque([0.02, 0.02, 0.03, 0.25]).with_frost(cce_ui::scene::Frost::from_style()),
        window_corners: (false, false, false, false),
        depth: depth.min(6.0),
    });
    pc.plate_spec(&PlateSpec {
        rect: r(sw - 120.0, 0.0, 120.0, sh),
        material: cce_ui::scene::Material::opaque([0.2, 0.2, 0.25, 0.8]).with_frost(cce_ui::scene::Frost::from_style()),
        window_corners: (false, true, true, false),
        depth,
    });
    // The fill-less roll overlay (negative depth) and an explicit shape.
    pc.plate_spec(&PlateSpec {
        rect: r(0.0, 0.0, sw, sh),
        material: cce_ui::scene::Material::opaque([0.0; 4]),
        window_corners: (true, true, true, true),
        depth: -depth,
    });
    pc.plate_shaped(r(30.0, 170.0, 40.0, 40.0), (20.0, 20.0, 20.0, 20.0), [0.3, 0.3, 0.35, 1.0], 4.0, Some(2.0));

    // Bare plates the legacy callers emit: an opaque face, a frosted one
    // (negative alpha handed straight in, the menu idiom).
    pc.plate(r(80.0, 170.0, 60.0, 30.0), rr, [0.13, 0.14, 0.16, 1.0], 4.0);
    pc.plate(r(150.0, 170.0, 60.0, 30.0), rr, [0.13, 0.14, 0.16, -0.8], 4.0);

    // Bevels, plain and tinted (the focused-pane ring).
    pc.bevel(r(220.0, 170.0, 60.0, 30.0), rr, [0.25, 0.25, 0.3, 1.0], 4.0);
    pc.bevel_tinted(r(290.0, 170.0, 60.0, 30.0), rr, [0.25, 0.25, 0.3, 1.0], 4.0, [0.4, 0.6, 1.0]);
    pc.bevel_tinted(r(290.0, 205.0, 60.0, 30.0), rr, [0.0; 4], 4.0, [0.4, 0.6, 1.0]);

    // Every control stance × face × tint.
    let faces: [[f32; 4]; 3] = [[0.3, 0.3, 0.36, 1.0], [0.0; 4], [0.3, 0.3, 0.36, -0.6]];
    let mut y = 210.0;
    for stance in [PlateStance::Raised, PlateStance::Flush, PlateStance::Flat] {
        let mut x = 20.0;
        for face in faces {
            for tint in [None, Some(ControlPlate::focus_tint())] {
                let plate = ControlPlate::control(r(x, y, 36.0, 20.0), 6.0, stance, face).with_tint(tint);
                pc.control_plate(&plate);
                x += 42.0;
            }
        }
        y += 26.0;
    }

    // Inset plates and wells.
    pc.inset_plate(r(20.0, 290.0, 60.0, 24.0), rr, [0.2, 0.2, 0.24, 1.0], 4.0);
    pc.inset_plate(r(90.0, 290.0, 60.0, 24.0), rr, [0.0; 4], 4.0);
    pc.inset_plate_tinted(r(160.0, 290.0, 60.0, 24.0), rr, [0.2, 0.2, 0.24, -0.5], 4.0, [1.0, 0.5, 0.2]);
    pc.well_floor(r(230.0, 290.0, 40.0, 24.0), 6.0, false);
    pc.well_floor(r(280.0, 290.0, 40.0, 24.0), 6.0, true);
    pc.canvas_well(r(330.0, 290.0, 40.0, 24.0), 6.0, true, false);
    pc.canvas_well(r(330.0, 320.0, 40.0, 24.0), 6.0, false, true);

    // The lit ball and the water: default spec, and one with its own finish
    // and depth terms.
    pc.sphere(40.0, 340.0, 10.0, [0.4, 0.4, 0.5, 1.0]);
    pc.droplet(r(80.0, 330.0, 120.0, 30.0), [0.1, 0.1, 0.12, -0.7], DropletSpec::default());
    let wet = DropletSpec::parse("gleam=1.0 shine=16 rim=0.3 clarity=0.5 core=0.4 shadow=0.3 refr=2");
    pc.droplet(r(220.0, 330.0, 120.0, 30.0), [0.1, 0.1, 0.12, 0.9], wet);
    pc.droplet_scrim(r(220.0, 365.0, 120.0, 30.0), [0.1, 0.1, 0.12, 0.9], wet, 3.0);

    pc.finish()
}

fn dump(out: &mut String, label: &str, dl: &cce_ui::scene::paint::DisplayList, sw: f32, sh: f32, scale: f32) {
    let (verts, batches, images, features) =
        cce_ui::backend::window_runner::tessellate_display_list(dl, sw, sh, scale);
    writeln!(out, "== {label} scale={scale} verts={} batches={} images={} features={}",
        verts.len(), batches.len(), images.len(), features.len()).unwrap();
    for (i, v) in verts.iter().enumerate() {
        writeln!(out, "v{i} {:?} {:?} {:?}", v.position, v.color, v.clip_circle).unwrap();
    }
    for (i, b) in batches.iter().enumerate() {
        writeln!(out, "b{i} scissor={:?} clip={:?} {}..{} blur={} plate={:?}",
            b.scissor, b.clip_rrect, b.start, b.end, b.blur_behind, b.plate).unwrap();
    }
    for (i, f) in features.iter().enumerate() {
        writeln!(out, "f{i} {f:?}").unwrap();
    }
}

#[test]
fn plate_paths_tessellate_to_the_golden() {
    let (sw, sh) = (400.0, 400.0);
    let mut out = String::new();
    for shader in [true, false] {
        cce_ui::layout::get_style_registry().write().unwrap().set_float("bevel_shader", if shader { 1.0 } else { 0.0 });
        let dl = scene(sw, sh);
        for scale in [1.0, 2.0] {
            dump(&mut out, if shader { "sdf" } else { "legacy" }, &dl, sw, sh, scale);
        }
    }
    cce_ui::layout::get_style_registry().write().unwrap().set_float("bevel_shader", 1.0);
    assert!(out.lines().count() > 100, "the scene tessellated to almost nothing");

    if let Ok(path) = std::env::var("CCE_PLATE_GOLDEN_WRITE") {
        std::fs::write(&path, &out).expect("write golden");
        eprintln!("wrote {} lines to {path}", out.lines().count());
        return;
    }
    if let Ok(path) = std::env::var("CCE_PLATE_GOLDEN") {
        let want = std::fs::read_to_string(&path).expect("read golden");
        if want != out {
            let mut shown = 0;
            for (n, (a, b)) in want.lines().zip(out.lines()).enumerate() {
                if a != b {
                    eprintln!("line {}:\n  golden: {a}\n  now:    {b}", n + 1);
                    shown += 1;
                    if shown >= 12 {
                        break;
                    }
                }
            }
            panic!(
                "tessellation drifted from {path}: {} vs {} lines, first differences above",
                want.lines().count(),
                out.lines().count()
            );
        }
    }
}
