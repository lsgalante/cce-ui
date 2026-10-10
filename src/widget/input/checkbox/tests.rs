use super::*;
use crate::widget::{WidgetHost, UiContext};

fn click_at(x: f32, y: f32) -> Event {
    Event::MouseButton {
        button: MouseButton::Left,
        state: ElementState::Pressed,
        x,
        y,
        local_x: x,
        local_y: y,
    }
}

#[test]
fn checkbox_click_toggles_and_polls_like_legacy() {
    let mut ctx = UiContext::new();
    let cb = ctx.insert(Checkbox::new());
    let id = cb.id();
    WidgetHost::set_rect(&mut ctx[cb], 0.0, 0.0, 20.0, 20.0);

    assert!(ctx.propagate_event(&click_at(10.0, 10.0), id), "in-rect click consumed");
    assert!(ctx[cb].checked(), "click checked it");
    assert!(ctx[cb].take_click(), "take_click reads once");
    assert!(!ctx[cb].take_click(), "...then clears");
    assert!(ctx[cb].take_change());

    assert!(!ctx.propagate_event(&click_at(100.0, 100.0), id), "miss is not consumed");
    assert!(ctx[cb].checked(), "miss does not toggle");
}

#[test]
fn checkbox_value_string_round_trip() {
    let mut cb = Checkbox::new();
    assert_eq!(cb.get_value_string(), Some("false".to_string()));
    assert!(cb.set_value_string("on"));
    assert!(cb.checked());
    assert_eq!(cb.value(), 1);
    assert!(!cb.set_value_string("on"), "unchanged value reports false");
    assert!(!cb.set_value_string("junk"), "unparsable reports false");
    assert!(cb.take_change(), "set_value_string marked the change");
}

/// A labelled checkbox is a square field at the left of its label: an
/// empty well, or checked a run standing in its middle.
#[test]
fn a_checkbox_is_a_well_with_a_square_plate_in_it_or_not() {
    use crate::scene::paint::Prim;
    let ctx = UiContext::new();
    let mut cb = Checkbox::new().with_label("Enable");
    WidgetHost::set_rect(&mut cb, 0.0, 0.0, 200.0, 24.0);
    let rect = Rect { x: 0.0, y: 0.0, width: 200.0, height: 24.0 };
    let prims = |cb: &Adapted<Checkbox>| -> Vec<Prim> {
        let mut pc = crate::scene::paint::PaintCtx::new();
        crate::widget::Paint::paint(cb.inner(), rect, &mut pc);
        pc.finish().items.into_iter().map(|i| i.prim).filter(|p| !matches!(p, Prim::Text { .. })).collect()
    };

    // A square at the left, as tall as the control, carved inside it.
    let f = cb.inner().field(rect);
    let side = 24.0f32.min(crate::layout::toggle_height());
    assert!((f.rect.width - f.rect.height).abs() < 1e-4, "square");
    assert!(f.rect.x >= 0.0 && f.rect.x + f.rect.width <= side, "at the left, inside its square");
    assert!(f.has_well() && f.run_span().is_none(), "all well");

    // Checked, a square plate in the middle, the well all round it: the
    // box keeps its shape — a ticked box must not read as narrower.
    cb.inner_mut().set_checked(true);
    assert_eq!(cb.inner().field(rect), f, "the well is the same either way");
    let p = Checkbox::box_plate(&f);
    assert!((p.rect.width - p.rect.height).abs() < 1e-4, "the plate is square");
    assert!((p.rect.width - f.rect.width * PLATE_SHARE).abs() < 1e-3);
    let (pcx, pcy) = (p.rect.x + p.rect.width * 0.5, p.rect.y + p.rect.height * 0.5);
    let (fcx, fcy) = (f.rect.x + f.rect.width * 0.5, f.rect.y + f.rect.height * 0.5);
    assert!((pcx - fcx).abs() < 1e-3 && (pcy - fcy).abs() < 1e-3, "centred");
    assert!(!p.has_well(), "the plate is all run");
    assert!(
        !crate::widget::shown_prims(&cb).iter().any(|p| matches!(p, crate::scene::paint::Prim::Circle { .. })),
        "no mark"
    );

    if crate::layout::control_relief() {
        cb.inner_mut().set_checked(false);
        assert!(matches!(prims(&cb)[..], [Prim::Recess { .. }]), "a well paints as the recess it groups as");
        cb.inner_mut().set_checked(true);
        assert!(matches!(prims(&cb)[..], [Prim::Recess { .. }, Prim::Field { .. }]), "the plate paints as a field in it");
        cb.focused = true;
        assert!(matches!(prims(&cb)[..], [Prim::Recess { tint: Some(_), .. }, _]), "focus lights the rim");
    }

    // The label follows the box.
    let labels = cb.own_text_labels();
    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0].text, "Enable");
    assert!((labels[0].x - (side + 8.0)).abs() < 1e-3, "{} vs {}", labels[0].x, side + 8.0);

    // Inline label => no set_rect inflation.
    assert_eq!(WidgetHost::rect(&cb), (0.0, 0.0, 200.0, 24.0));
    let _ = &ctx;
}

/// An inline check — a list row's, a markdown task item's — is the
/// widget's box at the size it is given: the same field, its corner the
/// toggle's in proportion, so a 14px box is a rounded square and not a
/// disc, and a box a toggle tall has the toggle's corner exactly.
#[test]
fn an_inline_check_is_the_widgets_box() {
    use crate::scene::paint::Prim;
    let h = Checkbox::INLINE_HALF;
    let square = Rect { x: 50.0 - h, y: 20.0 - h, width: 2.0 * h, height: 2.0 * h };
    for checked in [false, true] {
        let mut pc = crate::scene::paint::PaintCtx::new();
        Checkbox::paint_inline(&mut pc, 50.0, 20.0, h, checked);
        let prims: Vec<Prim> = pc.finish().items.into_iter().map(|i| i.prim).collect();
        let want = Checkbox::box_field(square);
        if crate::layout::control_relief() {
            let mut expect = crate::scene::paint::PaintCtx::new();
            expect.field(&want);
            if checked {
                expect.field(&Checkbox::box_plate(&want));
            }
            let expect: Vec<Prim> = expect.finish().items.into_iter().map(|i| i.prim).collect();
            assert_eq!(format!("{prims:?}"), format!("{expect:?}"), "the box_field and box_plate, painted");
        }
        assert!(want.radii.0 < want.rect.width * 0.5 - 0.5, "a rounded square, not a disc: {:?}", want.radii);
    }
    // A box a toggle tall has the toggle's corner, as the widget's always had.
    let th = crate::layout::toggle_height();
    let big = Checkbox::box_field(Rect { x: 0.0, y: 0.0, width: th, height: th });
    let r = crate::layout::toggle_corner_radius().min(th * 0.5);
    let (_, radii) = crate::layout::carve_inside(Rect { x: 0.0, y: 0.0, width: th, height: th }, (r, r, r, r), big.depth);
    assert_eq!(big.radii, radii);
}

#[test]
fn toggle_click_glides_the_run_across_its_field() {
    let mut ctx = UiContext::new();
    let t = ctx.insert(Toggle::new());
    let id = t.id();
    WidgetHost::set_rect(&mut ctx[t], 0.0, 0.0, 60.0, 30.0);

    let rect = Rect { x: 0.0, y: 0.0, width: 60.0, height: 30.0 };
    let painted = |t: &Adapted<Toggle>| {
        let mut pc = crate::scene::paint::PaintCtx::new();
        crate::widget::Paint::paint(t.inner(), rect, &mut pc);
        pc.finish().items.into_iter().map(|i| format!("{:?}", i.prim)).collect::<Vec<_>>()
    };
    let before = painted(&ctx[t]);
    let plate_x = |t: &Adapted<Toggle>| t.inner().field(rect).run_span().unwrap().0;
    let left = plate_x(&ctx[t]);

    assert!(ctx.propagate_event(&click_at(30.0, 15.0), id), "toggle consumed the click");
    assert!(ctx[t].toggled());
    assert!(ctx[t].take_click());

    // A click sets the target; the plate GLIDES there (`tick`), so the
    // geometry only moves once time passes — the rocker's halves used to
    // swap on the press itself.
    assert_eq!(plate_x(&ctx[t]), left, "the click alone does not move the plate");
    for _ in 0..60 {
        crate::widget::Input::tick(ctx[t].inner_mut(), 1.0 / 60.0, rect);
    }
    assert!(plate_x(&ctx[t]) > left, "the plate glided toward the on end");
    assert!(painted(&ctx[t]) != before, "toggling changes the emitted geometry");

    // preferred_height forwards the legacy toggle height.
    assert_eq!(crate::widget::WidgetHostExt::preferred_height(&ctx[t]), Some(crate::layout::toggle_height()));
}

/// The toggle is ONE field, the form a text row's picker and a spinbox's
/// -/+ run take: off, its run is the left half and the well the right;
/// on, the other way round; between, a well either side. The run's face
/// stands half a wall inside the run, so the run reaches the well.
#[test]
fn a_toggle_is_a_field_whose_run_glides() {
    use crate::scene::paint::{Prim, FIELD_RUN_ONLY};
    let rect = Rect { x: 10.0, y: 4.0, width: 120.0, height: 24.0 };
    let mut t = Toggle::new().with_raised(true);
    let Field { rect: field, radii, depth, .. } = t.inner().field(rect);
    assert!(field.x >= rect.x && field.y >= rect.y, "the field carves inside the rect");
    let (fl, fr) = (field.x, field.x + field.width);

    let fields = |t: &Adapted<Toggle>| -> Vec<Prim> {
        let mut pc = crate::scene::paint::PaintCtx::new();
        crate::widget::Paint::paint(t.inner(), rect, &mut pc);
        pc.finish().items.into_iter().map(|i| i.prim).filter(|p| !matches!(p, Prim::Text { .. })).collect()
    };
    let off = fields(&t);
    assert_eq!(off.len(), 1, "one prim, the field: {off:?}");
    let Prim::Field { rect: r, radii: rr, depth: d, split, end, tint } = off[0] else { panic!("{off:?}") };
    assert_eq!((r, rr, d, tint), (field, radii, depth, None));
    assert!(split <= fl - FIELD_RUN_ONLY + 1.0, "off: the run reaches the field's left end — no well there");
    assert!((end - (fl + field.width * 0.5)).abs() < 1e-4, "half the field, the well beyond it");

    t.set_toggled(true); // programmatic syncs snap, so this is the on-end geometry
    let Prim::Field { split, end, .. } = fields(&t)[0] else { panic!() };
    assert!((split - (fl + field.width * 0.5)).abs() < 1e-4, "on: the right half");
    assert!(end >= fr + FIELD_RUN_ONLY - 1.0, "reaching the field's right end");

    t.slide_t = 0.5;
    let Prim::Field { split, end, .. } = fields(&t)[0] else { panic!() };
    assert!(split > fl + 1.0 && end < fr - 1.0, "mid-glide, a well either side of the run");
    assert!(end < fr, "and the run's end is its own, not the field's");

    let (face, face_r) = t.inner().face(rect);
    let (a, b) = t.inner().field(rect).run_span().unwrap();
    assert!((face.x - (a + depth * 0.5)).abs() < 1e-4 && (face.x + face.width - (b - depth * 0.5)).abs() < 1e-4);
    assert!((face_r - (radii.0 - depth * 0.5).max(0.0)).abs() < 1e-4, "concentric with the field");

    // Focus lights the field's rim.
    t.focused = true;
    assert!(matches!(fields(&t)[0], Prim::Field { tint: Some(_), .. }), "focus tints the field");
}

#[test]
fn toggle_set_label_via_deref_reaches_paint() {
    let mut t = Toggle::new();
    WidgetHost::set_rect(&mut t, 0.0, 0.0, 60.0, 30.0);
    t.set_label("ON"); // the network.rs pattern: live label updates through Deref
    let labels = t.own_text_labels();
    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0].text, "ON");
}
