use super::*;
use crate::draw::PlatePush;

fn plate(rect: [f32; 4], t: f32, host: [f32; 4]) -> DlBatch {
    DlBatch {
        scissor: None,
        clip_rrect: None,
        start: 0,
        end: 0,
        plate: Some(PlatePush {
            rect,
            radii: [4.0; 4],
            light: [0.0, 0.0, 1.0, t],
            material: [1.0, 0.4, 24.0, 0.2],
            host,
            specular_tint: [1.0; 4],
            mode: 1.0,
            shape: 2.0,
        }),
        blur_behind: false,
    }
}

#[test]
fn a_plate_stands_one_rise_above_nothing_and_rolls_to_its_rim() {
    // 100×100 window, plate 60×60 centred, roll 10.
    let b = plate([50.0, 50.0, 30.0, 30.0], 10.0, [0.0; 4]);
    let hf = HeightField::from_frame(&[b], &[], 100, 100, 1.0);
    let face = hf.px[50 * 100 + 50];
    assert!((face - 10.0).abs() < 1e-3, "face {face}");
    assert_eq!(hf.px[5 * 100 + 5], 0.0, "outside the plate is the base");
    // Just inside the silhouette (pixel centre 21.5, silhouette at 20:
    // d = 1.5, f = 0.85) the rim is cut off at the ROLL_CUT height.
    let rim = hf.px[50 * 100 + 21];
    let expected = 10.0 * (1.0 - (ROLL_CUT * 0.85f32).powi(2)).sqrt();
    assert!((rim - expected).abs() < 0.3, "rim {rim} vs {expected}");
    assert!(rim < face);
}

#[test]
fn a_csg_recess_etches_its_depth_into_the_face() {
    let b = plate([50.0, 50.0, 40.0, 40.0], 8.0, [0.0, 1.0, 0.0, 0.0]);
    // A 20×20 recess at the centre, wall 6, dropping 4.
    let feat = [50.0, 50.0, 10.0, 10.0, 2.0, 2.0, 2.0, 2.0, 6.0, 4.0, 0.0, 0.0];
    let hf = HeightField::from_frame(&[b], &[feat], 100, 100, 1.0);
    assert!((hf.px[50 * 100 + 50] - 4.0).abs() < 1e-3, "floor {}", hf.px[50 * 100 + 50]);
    assert!((hf.px[50 * 100 + 75] - 8.0).abs() < 1e-3, "face {}", hf.px[50 * 100 + 75]);
    // The wall lands between.
    let wall = hf.px[50 * 100 + 60];
    assert!(wall > 4.0 && wall < 8.0, "wall {wall}");
}

#[test]
fn a_free_recess_carves_the_analytic_ratio_of_its_wall() {
    let mut b = plate([50.0, 50.0, 20.0, 20.0], 10.0, [-1e5, -1e5, 1e5, 1e5]);
    b.plate.as_mut().unwrap().mode = 2.0;
    let hf = HeightField::from_frame(&[b], &[], 100, 100, 1.0);
    let floor = hf.px[50 * 100 + 50];
    assert!((floor + RECESS_DEPTH * 10.0).abs() < 1e-3, "floor {floor}");
    assert_eq!(hf.px[5 * 100 + 5], 0.0);
}

#[test]
fn a_lattice_is_one_surface_with_plateau_rails_and_mitred_crossings() {
    // Period 50, cells 30 wide (gap 20), wall 10 = the half-gap, one
    // cell centred at (25, 25). p_rect = cell centre + half-extents,
    // p_host.xy = period, radii 4 (the fixture's).
    let mut b = plate([25.0, 25.0, 15.0, 15.0], 10.0, [50.0, 50.0, 1e6, 1e6]);
    b.plate.as_mut().unwrap().mode = 13.0;
    let hf = HeightField::from_frame(&[b], &[], 100, 100, 1.0);
    let at = |x: usize, y: usize| hf.px[y * 100 + x];
    let drop = RECESS_DEPTH * 10.0;
    // Every cell floor, not just the one the push names: (25,25) and
    // its period neighbour (75,75).
    assert!((at(25, 25) + drop).abs() < 1e-3, "floor {}", at(25, 25));
    assert!((at(75, 75) + drop).abs() < 1e-3, "neighbour floor {}", at(75, 75));
    // The rail centre line between two cells is exactly one run from
    // either edge: plateau, not a doubled wall. Pixel centres straddle
    // the line by half a px (49.5 and 50.5 are each 9.5 from a cell), so
    // the two samples sit a hair into opposite walls — equal, and within
    // the wall's first half-px of drop.
    assert!((at(49, 25) - at(50, 25)).abs() < 1e-3, "rail centre symmetric {} {}", at(49, 25), at(50, 25));
    assert!(at(50, 25) > -0.1 && at(50, 25) <= 0.0, "rail centre {}", at(50, 25));
    assert!(at(50, 25) > at(45, 25), "rail centre above the wall");
    // The crossing where four cells meet: the four sharp-cornered outer
    // contours meet exactly at the centre, so the crest is a point there
    // (the pixel centre sits half a px into one quadrant's wall) — no
    // flat lozenge, the mitre the per-cell rings never gave.
    assert!(at(50, 50) <= 0.0 && at(50, 50) > -0.1, "crossing {}", at(50, 50));
    // Just off the centre along the diagonal the hip is already falling:
    // inside the outer box by 1.5 px on both axes, a definite carve —
    // where an offset-curve outer contour (13.7 px from the cell, past
    // the 10 px run) would be flat.
    assert!(at(48, 48) < -0.1 && at(48, 48) > -drop, "hip {}", at(48, 48));
    // On the rail centre line the outer contour is straight: plateau all
    // the way up to the crossing's corner rounding (the same half-px
    // straddle as above: 49.5 and 50.5 sit a hair into opposite walls).
    assert!((at(49, 45) - at(50, 45)).abs() < 1e-3, "rail centre near crossing symmetric");
    assert!(at(50, 45) > -0.1 && at(50, 45) <= 0.0, "rail centre near crossing {}", at(50, 45));
    // Halfway out the wall is between floor and plateau, on both sides
    // of the rail (one wall from each cell, symmetric). Pixel centres:
    // 45.5 is 5.5 past the first cell's edge at 40, 54.5 is 5.5 before
    // the neighbour's edge at 60.
    let w1 = at(45, 25);
    let w2 = at(54, 25);
    assert!(w1 < 0.0 && w1 > -drop, "wall {w1}");
    assert!((w1 - w2).abs() < 1e-3, "walls symmetric {w1} {w2}");
}

#[test]
fn a_carve_union_is_one_wall_around_the_union_of_its_boxes() {
    // An L: a 60×20 bar across the top and a 20×60 bar down the left,
    // sharing the corner square (10..30). Wall 8, radii 4 (fixture).
    let bar_h = [40.0, 20.0, 30.0, 10.0, 4.0, 4.0, 4.0, 4.0, 8.0, 0.0, 0.0, 0.0];
    let bar_v = [20.0, 40.0, 10.0, 30.0, 4.0, 4.0, 4.0, 4.0, 8.0, 0.0, 0.0, 0.0];
    let mut b = plate([40.0, 40.0, 30.0, 30.0], 8.0, [0.0, 2.0, 1e6, 1e6]);
    b.plate.as_mut().unwrap().mode = 14.0;
    b.plate.as_mut().unwrap().radii = [0.0; 4];
    let hf = HeightField::from_frame(&[b], &[bar_h, bar_v], 100, 100, 1.0);
    let at = |x: usize, y: usize| hf.px[y * 100 + x];
    let drop = RECESS_DEPTH * 8.0;
    // Deep inside either bar: the full drop, once.
    assert!((at(55, 20) + drop).abs() < 1e-3, "top bar {}", at(55, 20));
    assert!((at(20, 55) + drop).abs() < 1e-3, "left bar {}", at(20, 55));
    // The shared corner square lies inside BOTH boxes. With one recess
    // per box, each box's wall would run straight through the other's
    // interior here (the vertical bar's right wall at x = 30 crosses the
    // top bar). As a union the interior is flat floor: exactly one drop.
    assert!((at(25, 20) + drop).abs() < 1e-3, "corner interior {}", at(25, 20));
    assert!((at(20, 25) + drop).abs() < 1e-3, "corner interior {}", at(20, 25));
    // Well outside: the base.
    assert_eq!(at(80, 80), 0.0);
    // The wall straddles the union outline by ±t/2 = 4: sampled 2 px
    // outside the top bar's lower edge (y = 30), on the wall.
    let wall = at(55, 32);
    assert!(wall < 0.0 && wall > -drop, "wall {wall}");
    // Mitred outer corner: the band's outer contour is the bar grown by
    // t/2 at the bar's own radius 4, so (72.5, 8.5) — 1.1 px inside that
    // contour's corner arc — carries a hair of wall, where an offset
    // contour (radius 8, the point 4.5 px from the bar) would be flat.
    let corner = at(72, 8);
    assert!(corner < 0.0 && corner > -0.5 * drop, "mitred corner {corner}");
}

#[test]
fn resample_keeps_the_face_height() {
    let b = plate([50.0, 50.0, 40.0, 40.0], 8.0, [0.0; 4]);
    let mut hf = HeightField::from_frame(&[b], &[], 100, 100, 1.0);
    hf.px_per_mm = 10.0; // 10 px per mm → 100 px = 10 mm
    let (data, w, h, pitch) = hf.resampled(Some(0.5));
    assert_eq!((w, h), (20, 20));
    assert!((pitch - 0.5).abs() < 1e-6);
    assert!((data[10 * 20 + 10] - 8.0).abs() < 1e-3);
}

#[test]
fn export_writes_png_and_sidecar() {
    let b = plate([50.0, 50.0, 40.0, 40.0], 8.0, [0.0; 4]);
    let hf = HeightField::from_frame(&[b], &[], 100, 100, 1.0);
    let dir = std::env::temp_dir().join(format!("cce-heightfield-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("map.png");
    export_png(&hf, &path, None).unwrap();
    let png = std::fs::read(&path).unwrap();
    assert_eq!(&png[1..4], b"PNG");
    let side: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("map.png.json")).unwrap()).unwrap();
    assert_eq!(side["width"], 100);
    assert_eq!(side["metric_source"], "assumed");
    let _ = std::fs::remove_dir_all(&dir);
}
