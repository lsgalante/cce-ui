use super::*;

/// A nested paint spliced in keeps its own clips under the host's: the scissors
/// intersect, and the nested item's rounded clip wins over the host's.
#[test]
fn appended_items_keep_their_clips_under_the_hosts() {
    let r = |x: f32, y: f32, w: f32, h: f32| Rect { x, y, width: w, height: h };
    let mut nested = PaintCtx::new();
    nested.quad(r(0.0, 0.0, 5.0, 5.0), [1.0; 4]);
    nested.clip_rounded(r(10.0, 10.0, 40.0, 40.0), 6.0, |pc| pc.quad(r(12.0, 12.0, 5.0, 5.0), [1.0; 4]));
    let mut host = PaintCtx::new();
    host.clip(r(0.0, 0.0, 30.0, 30.0), |pc| pc.append_items(nested.finish().items));
    let items = host.finish().items;
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].clip, Some(r(0.0, 0.0, 30.0, 30.0)), "an unclipped item takes the host's clip");
    assert_eq!(items[1].clip, Some(r(10.0, 10.0, 20.0, 20.0)), "the two scissors intersect");
    assert!(items[1].clip_rrect.is_some_and(|c| c[4] == 6.0), "its rounded clip survives");
}

/// The one flush control plate draws a face and a field that is all
/// run — the edge every flush control wears — and no trough.
#[test]
fn an_inset_plate_is_a_field_that_is_all_run() {
    let rect = Rect { x: 10.0, y: 20.0, width: 120.0, height: 24.0 };
    let face = Material::face([0.2, 0.2, 0.25, 1.0]);
    for tint in [None, Some([1.0, 0.5, 0.0])] {
        let mut pc = PaintCtx::new();
        match tint {
            Some(t) => pc.inset_plate_tinted(rect, (4.0, 4.0, 4.0, 4.0), face.as_ref(), 4.0, t),
            None => pc.inset_plate(rect, (4.0, 4.0, 4.0, 4.0), face.as_ref(), 4.0),
        }
        let prims: Vec<Prim> = pc.finish().items.into_iter().map(|i| i.prim).collect();
        assert!(!prims.iter().any(|p| matches!(p, Prim::Trough { .. })), "{prims:?}");
        assert!(prims.iter().any(|p| matches!(p, Prim::Border { .. })), "the face");
        assert!(
            prims.iter().any(|p| matches!(p, Prim::Field { rect: r, split, tint: t, .. } if *r == rect && *split <= rect.x - 100.0 && *t == tint)),
            "{prims:?}"
        );
    }
}
use crate::scene::material::Frost;

/// RFC Phase 7b: PlateSpec role mechanics — flag derivation from window
/// geometry, silhouette-vs-nominal radii selection, and the role-encoded
/// frost (root positive-alpha, nested negative-alpha sentinel).
#[test]
fn plate_spec_roles() {
    // Flags: a full-window rect is root; an inset pane has none; a pane
    // flush to the window's right edge owns the two right corners.
    let root_flags = PlateSpec::window_corner_flags(
        Rect { x: 0.0, y: 0.0, width: 800.0, height: 600.0 }, 800.0, 600.0);
    assert_eq!(root_flags, (true, true, true, true));
    let inset = PlateSpec::window_corner_flags(
        Rect { x: 20.0, y: 20.0, width: 100.0, height: 100.0 }, 800.0, 600.0);
    assert_eq!(inset, (false, false, false, false));
    let right_pane = PlateSpec::window_corner_flags(
        Rect { x: 500.0, y: 0.0, width: 300.0, height: 600.0 }, 800.0, 600.0);
    assert_eq!(right_pane, (false, true, true, false));

    // Radii: flagged corners wear the shared silhouette curve, interior
    // ones the nominal plate radius (compared against the same getters,
    // so the assertion holds for any configured values).
    let window_r =
        crate::layout::window_corner_radius() * crate::layout::corner_span_factor();
    let nominal = crate::layout::plate_corner_radius();
    let r = PlateSpec::radii_for((false, true, true, false));
    assert_eq!(r, (nominal, window_r, window_r, nominal));

    // Frost encoding by role.
    let frosted = Frost::Frosted { compression: 0.0, refraction: 0.0, radius: Frost::DEFAULT_RADIUS };
    let mut spec = PlateSpec {
        rect: Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 },
        material: Material::opaque([0.1, 0.2, 0.3, 0.8]).with_frost(frosted),
        window_corners: (true, true, true, true),
        depth: 3.0,
    };
    assert!(spec.is_root());
    assert_eq!(spec.role(), PlateRole::Root);
    assert!(spec.fill()[3] > 0.0, "root frost is the compositor's; alpha stays positive");
    spec.window_corners = (false, true, true, false);
    assert!(!spec.is_root());
    assert_eq!(spec.role(), PlateRole::Nested);
    assert!(spec.fill()[3] < 0.0, "nested frost = negative-alpha sentinel");
    spec.material.frost = Frost::Unfrosted;
    assert_eq!(spec.fill()[3], 0.8, "no frost, no encoding");

    // The detach role flip (RFC 7c): a frosted nested pane becomes a
    // root — silhouette corners, and the frost regime flips from the
    // in-app sentinel to the compositor's (alpha back to positive).
    spec.material.frost = frosted;
    assert!(spec.fill()[3] < 0.0);
    let det = spec.detached();
    assert!(det.is_root());
    assert!(det.fill()[3] > 0.0, "root frost is the compositor's again");
    let wr = crate::layout::window_silhouette_radius();
    assert_eq!(det.radii(), (wr, wr, wr, wr));
}

fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { x, y, width: w, height: h }
}

/// The emission round-trip: `plate_spec` pre-divides by the span factor so
/// `plate_push_raised(scale_corners = true)` lands each corner at exactly
/// the spec's final radius. Guards the double-span regression (7b-2), and
/// holds for any configured corner_shape because both sides use the same
/// factor.
#[test]
fn plate_spec_emission_round_trips_the_span() {
    let spec = PlateSpec {
        rect: r(0.0, 0.0, 400.0, 300.0),
        material: Material::opaque([0.1, 0.2, 0.3, 0.8]),
        window_corners: (true, true, false, false),
        depth: 4.0,
    };
    let mut pc = PaintCtx::new();
    pc.plate_spec(&spec);
    let f = crate::layout::corner_span_factor();
    let emitted = pc
        .finish()
        .items
        .iter()
        .find_map(|it| match &it.prim {
            Prim::Plate { radii, .. } => Some(*radii),
            _ => None,
        })
        .expect("plate_spec emits a Prim::Plate");
    let want = spec.radii();
    let got = (emitted.0 * f, emitted.1 * f, emitted.2 * f, emitted.3 * f);
    for (g, w) in [(got.0, want.0), (got.1, want.1), (got.2, want.2), (got.3, want.3)] {
        assert!((g - w).abs() < 1e-3, "span round-trip drifted: {g} vs {w}");
    }
}

#[test]
fn emits_in_order_unclipped() {
    let mut ctx = PaintCtx::new();
    ctx.quad(r(0.0, 0.0, 10.0, 10.0), [1.0, 0.0, 0.0, 1.0]);
    ctx.quad(r(5.0, 5.0, 10.0, 10.0), [0.0, 1.0, 0.0, 1.0]);
    let list = ctx.finish();
    assert_eq!(list.len(), 2);
    assert_eq!(list.items[0].clip, None);
    assert!(matches!(list.items[0].prim, Prim::Quad { color, .. } if color[0] == 1.0));
    assert!(matches!(list.items[1].prim, Prim::Quad { color, .. } if color[1] == 1.0));
}

#[test]
fn clip_is_recorded_and_popped() {
    let mut ctx = PaintCtx::new();
    ctx.clip(r(0.0, 0.0, 50.0, 50.0), |ctx| {
        ctx.quad(r(10.0, 10.0, 5.0, 5.0), [0.0; 4]);
    });
    ctx.quad(r(60.0, 60.0, 5.0, 5.0), [0.0; 4]); // outside any clip now
    let list = ctx.finish();
    assert_eq!(list.items[0].clip, Some(r(0.0, 0.0, 50.0, 50.0)));
    assert_eq!(list.items[1].clip, None, "clip popped after the closure");
}

#[test]
fn nested_clips_intersect() {
    let mut ctx = PaintCtx::new();
    ctx.clip(r(0.0, 0.0, 100.0, 100.0), |ctx| {
        ctx.clip(r(50.0, 50.0, 100.0, 100.0), |ctx| {
            ctx.quad(r(0.0, 0.0, 1.0, 1.0), [0.0; 4]);
        });
    });
    // Intersection of (0,0,100,100) and (50,50,100,100) = (50,50,50,50).
    assert_eq!(ctx.finish().items[0].clip, Some(r(50.0, 50.0, 50.0, 50.0)));
}

#[test]
fn non_overlapping_clips_produce_empty_scissor() {
    let mut ctx = PaintCtx::new();
    ctx.clip(r(0.0, 0.0, 10.0, 10.0), |ctx| {
        ctx.clip(r(100.0, 100.0, 10.0, 10.0), |ctx| {
            ctx.quad(r(0.0, 0.0, 1.0, 1.0), [0.0; 4]);
        });
    });
    let clip = ctx.finish().items[0].clip.unwrap();
    assert_eq!((clip.width, clip.height), (0.0, 0.0), "empty intersection");
}

#[test]
fn translate_applies_to_coordinates_and_restores() {
    let mut ctx = PaintCtx::new();
    ctx.translate(100.0, 200.0, |ctx| {
        ctx.quad(r(0.0, 0.0, 5.0, 5.0), [0.0; 4]);
    });
    ctx.quad(r(0.0, 0.0, 5.0, 5.0), [0.0; 4]); // back at origin
    let list = ctx.finish();
    assert!(matches!(list.items[0].prim, Prim::Quad { rect, .. } if rect.x == 100.0 && rect.y == 200.0));
    assert!(matches!(list.items[1].prim, Prim::Quad { rect, .. } if rect.x == 0.0 && rect.y == 0.0));
}

#[test]
fn nested_translate_is_cumulative() {
    let mut ctx = PaintCtx::new();
    ctx.translate(10.0, 10.0, |ctx| {
        ctx.translate(5.0, 5.0, |ctx| {
            ctx.circle(0.0, 0.0, 3.0, [0.0; 4]);
        });
    });
    assert!(matches!(ctx.finish().items[0].prim, Prim::Circle { cx, cy, .. } if cx == 15.0 && cy == 15.0));
}

#[test]
fn clip_pushed_under_translation_is_absolute() {
    let mut ctx = PaintCtx::new();
    ctx.translate(20.0, 20.0, |ctx| {
        ctx.clip(r(0.0, 0.0, 30.0, 30.0), |ctx| {
            ctx.quad(r(0.0, 0.0, 5.0, 5.0), [0.0; 4]);
        });
    });
    let item = &ctx.finish().items[0];
    // Clip translated to absolute (20,20,30,30); prim likewise at (20,20).
    assert_eq!(item.clip, Some(r(20.0, 20.0, 30.0, 30.0)));
    assert!(matches!(item.prim, Prim::Quad { rect, .. } if rect.x == 20.0 && rect.y == 20.0));
}

#[test]
fn all_primitive_kinds_emit() {
    let mut ctx = PaintCtx::new();
    ctx.quad(r(0.0, 0.0, 1.0, 1.0), [0.0; 4]);
    ctx.rounded_rect(r(0.0, 0.0, 1.0, 1.0), 2.0, (true, false, true, false), [0.0; 4]);
    ctx.border(r(0.0, 0.0, 10.0, 10.0), (2.0, 2.0, 2.0, 2.0), [0.1; 4], [0.9; 4], 1.5);
    ctx.bevel(r(0.0, 0.0, 10.0, 10.0), (2.0, 2.0, 2.0, 2.0), &Material::opaque([0.3; 4]), 2.0);
    ctx.arc(5.0, 5.0, 4.0, 1.0, 0.0, std::f32::consts::PI, [0.0; 4]);
    ctx.vector(0.0, 0.0, 10.0, 0.0, 1.0, [0.0; 4], Cap::Arrow);
    ctx.circle(5.0, 5.0, 3.0, [0.0; 4]);
    ctx.text("hi", 1.0, 2.0, 12.0, [255, 255, 255]);
    assert_eq!(ctx.finish().len(), 8);
}

#[test]
fn border_and_bevel_are_offset() {
    let mut ctx = PaintCtx::new();
    ctx.translate(10.0, 20.0, |ctx| {
        ctx.border(r(0.0, 0.0, 5.0, 5.0), (1.0, 1.0, 1.0, 1.0), [0.0; 4], [1.0; 4], 1.0);
        ctx.bevel(r(0.0, 0.0, 5.0, 5.0), (1.0, 1.0, 1.0, 1.0), &Material::opaque([0.0; 4]), 1.0);
    });
    let list = ctx.finish();
    assert!(matches!(list.items[0].prim, Prim::Border { rect, .. } if rect.x == 10.0 && rect.y == 20.0));
    assert!(matches!(list.items[1].prim, Prim::Bevel { rect, .. } if rect.x == 10.0 && rect.y == 20.0));
}
#[test]
fn text_with_translates_position_and_bounds() {
    let mut ctx = PaintCtx::new();
    ctx.translate(10.0, 20.0, |ctx| {
        ctx.text_with("hi", 1.0, 2.0, 12.0, [1, 2, 3], Some("Mono".into()), Some([0.0, 0.0, 50.0, 30.0]));
        ctx.text("plain", 3.0, 4.0, 10.0, [9, 9, 9]);
    });
    let list = ctx.finish();
    match &list.items[0].prim {
        Prim::Text { x, y, font, bounds, .. } => {
            assert_eq!((*x, *y), (11.0, 22.0), "position translated");
            assert_eq!(font.as_deref(), Some("Mono"));
            assert_eq!(*bounds, Some([10.0, 20.0, 60.0, 50.0]), "bounds translated");
        }
        other => panic!("expected Text, got {other:?}"),
    }
    match &list.items[1].prim {
        Prim::Text { font, bounds, .. } => {
            assert_eq!(*font, None, "plain text carries no font");
            assert_eq!(*bounds, None);
        }
        other => panic!("expected Text, got {other:?}"),
    }
}
