use super::*;

/// The ParametersBg drive pattern: readout click opens the row's edit, Enter/unfocus
/// commits back into the normalized value, a track press starts a drag.
#[test]
fn readout_edit_commits_on_unfocus() {
    let mut ctx = UiContext::new();
    let mut f = Float3::new().with_values([0.5, 0.5, 0.5]).with_range(0.0, 10.0);
    WidgetHost::set_rect(&mut f, 0.0, 0.0, 300.0, Float3::preferred_height(false));

    let rows = f.get_row_rects();
    assert_eq!(rows.len(), 3);
    assert_eq!(f.value_string(), "5.00:5.00:5.00");
    // Click row 1's readout (the 60px box at the row's right end).
    let rx = rows[1].0 + rows[1].2 - 30.0;
    let ry = rows[1].1 + rows[1].3 * 0.5;
    assert!(f.mouse_input(MouseButton::Left, ElementState::Pressed, rx, ry, &mut ctx));
    assert_eq!(f.editing_idx(), Some(1));

    f.sliders[1].set_value_string("7.5");
    WidgetHost::unfocus(&mut f);
    assert_eq!(f.editing_idx(), None);
    assert!((f.values()[1] - 0.75).abs() < 1e-4, "7.5 of 0..10 normalizes to 0.75");

    // Track press starts a drag; drag_update moves the value; release ends it.
    let track_x = rows[0].0 + 20.0;
    let track_y = rows[0].1 + rows[0].3 * 0.5;
    assert!(f.mouse_input(MouseButton::Left, ElementState::Pressed, track_x, track_y, &mut ctx));
    assert!(f.is_dragging());
    f.drag_update(track_x + 100.0, track_y);
    assert!(f.values()[0] > 0.5, "drag right raises the value");
    f.drag_end();
    assert!(!f.is_dragging());
}

/// The trackball stands left of the rows, as tall as they are, and
/// dragging it rolls the vector: a quarter turn of the ball's surface
/// to the right carries a vector pointing at the viewer onto +X, one
/// downward onto -Y, and the length is kept. The rows read to a third
/// decimal, and a vector of no length is given one on the first drag.
#[test]
fn the_trackball_turns_the_vector_and_keeps_its_length() {
    let mut ctx = UiContext::new();
    let quarter = |r: f32| r * std::f32::consts::FRAC_PI_2;
    let group = |v: [f32; 3]| {
        // Range -10..10: a scaled value v is (v + 10) / 20 normalized.
        let mut f = Float3::new().with_range(-10.0, 10.0).with_values(v.map(|c| (c + 10.0) / 20.0)).with_trackball(true);
        WidgetHost::set_rect(&mut f, 0.0, 0.0, 400.0, Float3::preferred_height(false));
        f
    };
    let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 2e-3);

    let plain = {
        let mut f = Float3::new();
        WidgetHost::set_rect(&mut f, 0.0, 0.0, 400.0, Float3::preferred_height(false));
        f.get_row_rects()[0]
    };
    let mut f = group([0.0, 0.0, 2.0]);
    let (cx, cy, r) = f.ball_circle().expect("a ball");
    assert_eq!(r * 2.0, Float3::ball_diameter());
    let row = f.get_row_rects()[0];
    assert_eq!(row.0, plain.0 + Float3::trackball_chrome(), "the rows start past the ball");
    assert_eq!(row.2, plain.2 - Float3::trackball_chrome());
    assert_eq!(f.value_string(), "0.000:0.000:2.000", "three decimals with the ball on");

    // A press on the ball takes hold; the drag rolls it a quarter turn right.
    assert!(f.mouse_input(MouseButton::Left, ElementState::Pressed, cx, cy, &mut ctx));
    assert!(f.is_dragging());
    assert!(f.drag_update(cx + quarter(r), cy));
    assert!(close(f.vector(), [2.0, 0.0, 0.0]), "{:?}", f.vector());
    f.drag_end();
    assert!(!f.is_dragging());

    // Downward: the near point turns toward -Y.
    let mut f = group([0.0, 0.0, 2.0]);
    f.drag_begin(cx, cy);
    assert!(f.drag_update(cx, cy + quarter(r)));
    assert!(close(f.vector(), [0.0, -2.0, 0.0]), "{:?}", f.vector());

    // Many small moves add up to what one large one does: the drag
    // turns its own full-precision copy, not the rounded rows.
    let mut f = group([0.0, 0.0, 0.06]);
    f.drag_begin(cx, cy);
    for i in 1..=100 {
        f.drag_update(cx + quarter(r) * i as f32 / 100.0, cy);
    }
    assert!(close(f.vector(), [0.06, 0.0, 0.0]), "{:?}", f.vector());

    // No length: the first drag gives it one.
    let mut f = group([0.0, 0.0, 0.0]);
    f.drag_begin(cx, cy);
    assert!(f.drag_update(cx + quarter(r), cy));
    assert!(close(f.vector(), [1.0, 0.0, 0.0]), "{:?}", f.vector());

    // Off the ball a press is the rows', as before.
    let mut f = group([0.0, 0.0, 2.0]);
    let row = f.get_row_rects()[0];
    assert!(!f.ball_hit(row.0 + 20.0, row.1 + row.3 * 0.5));
    assert!(f.mouse_input(MouseButton::Left, ElementState::Pressed, row.0 + 20.0, row.1 + row.3 * 0.5, &mut ctx));
    assert!(f.is_dragging());
    f.drag_update(row.0 + 60.0, row.1 + row.3 * 0.5);
    assert!(f.vector()[1] == 0.0 && f.vector()[2] == 2.0, "only X moved: {:?}", f.vector());

    // A scroll rolls the ball as content is scrolled: a notch is
    // fifteen degrees, the wheel in one axis and a two-finger gesture in
    // both. Wheel DOWN moves content up, and the near point with it.
    let turn = std::f32::consts::PI / 12.0;
    let mut f = group([0.0, 0.0, 2.0]);
    ctx.scroll_gesture_new = true;
    ctx.scroll_initiate_widget_id = None;
    assert!(f.wheel(&MouseScrollDelta::LineDelta(0.0, -1.0), cx, cy, &mut ctx));
    assert!(close(f.vector(), [0.0, 2.0 * turn.sin(), 2.0 * turn.cos()]), "{:?}", f.vector());
    assert_eq!(ctx.scroll_initiate_widget_id, Some(f.ball_id()), "the ball owns the gesture");
    // Latched: the pointer has drifted onto a band, and the ball still turns.
    ctx.scroll_gesture_new = false;
    let row = f.get_row_rects()[0];
    let before = f.vector();
    assert!(f.wheel(&MouseScrollDelta::LineDelta(0.0, 1.0), row.0 + 20.0, row.1 + row.3 * 0.5, &mut ctx));
    assert!(close(f.vector(), [0.0, 0.0, 2.0]), "rolled back: {:?} from {before:?}", f.vector());

    // A trackpad sends a pixel at a time. Sixty of them to the right are
    // one notch, on a vector short enough that a single pixel turns it
    // by less than the rows can hold.
    let mut f = group([0.0, 0.0, 0.06]);
    ctx.scroll_gesture_new = true;
    ctx.scroll_initiate_widget_id = None;
    for _ in 0..60 {
        assert!(f.wheel(&MouseScrollDelta::PixelDelta(crate::widget::Position { x: 1.0, y: 0.0 }), cx, cy, &mut ctx));
        ctx.scroll_gesture_new = false;
    }
    assert!(close(f.vector(), [0.06 * turn.sin(), 0.0, 0.06 * turn.cos()]), "{:?}", f.vector());
    // A typed component ends the scroll's copy: the next scroll turns
    // what the rows hold.
    f.sliders[1].set_scaled_value(3.0);
    ctx.scroll_gesture_new = true;
    f.wheel(&MouseScrollDelta::LineDelta(1.0, 0.0), cx, cy, &mut ctx);
    assert!((f.vector()[1] - 3.0).abs() < 2e-3, "Y is what was typed: {:?}", f.vector());

    // Off the ball a scroll is the bands', and a gesture a band holds
    // stays the band's over the ball.
    let mut f = group([0.0, 0.0, 2.0]);
    let row = f.get_row_rects()[0];
    ctx.scroll_gesture_new = true;
    ctx.scroll_initiate_widget_id = None;
    let band_x = row.0 + (row.2 - 68.0) * 0.5;
    assert!(f.wheel(&MouseScrollDelta::LineDelta(0.0, -1.0), band_x, row.1 + row.3 * 0.5, &mut ctx));
    assert_ne!(f.vector()[0], 0.0, "the X band turned");
    let (y, z) = (f.vector()[1], f.vector()[2]);
    ctx.scroll_gesture_new = false;
    assert!(f.wheel(&MouseScrollDelta::LineDelta(0.0, -1.0), cx, cy, &mut ctx));
    assert_eq!((f.vector()[1], f.vector()[2]), (y, z), "the ball did not take a band's gesture");
    // Over the readouts, past the bands' halos, nothing takes a scroll;
    // and a group without a ball has no ball to hit.
    assert_eq!(f.wheel_zone(395.0, cy), None);
    let mut plain = Float3::new().with_range(-10.0, 10.0);
    WidgetHost::set_rect(&mut plain, 0.0, 0.0, 400.0, Float3::preferred_height(false));
    assert!(!plain.ball_hit(cx, cy));

    // Seen from a camera: one out along +X, looking back at the origin,
    // with -Z to its right. A vector along +X points at it, so on the
    // ball it faces the viewer, tip at the centre; rolled a quarter to
    // the right it swings to the right of the SCREEN, which in the
    // scene is -Z; and the rows hold the scene's numbers throughout.
    let camera = [[0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]];
    let mut f = group([2.0, 0.0, 0.0]);
    assert!(f.set_view(camera));
    let tip = |f: &Adapted<Float3>| {
        let mut pc = PaintCtx::new();
        f.paint_ball(&mut pc);
        pc.finish().items.into_iter().find_map(|i| match i.prim {
            crate::scene::paint::Prim::Vector { x2, y2, thickness: 2.0, .. } => Some((x2, y2)),
            _ => None,
        }).expect("the vector's stroke")
    };
    let (tx, ty) = tip(&f);
    assert!((tx - cx).abs() < 1e-3 && (ty - cy).abs() < 1e-3, "pointing at the camera: ({tx}, {ty})");
    f.drag_begin(cx, cy);
    assert!(f.drag_update(cx + quarter(r), cy));
    assert!(close(f.vector(), [0.0, 0.0, -2.0]), "screen right is the scene's -Z: {:?}", f.vector());
    f.drag_end();
    let (tx, _) = tip(&f);
    assert!(tx > cx + r * 0.5, "and it is drawn to the right");
    // A scroll rolls about the camera's axes too, and a vector of no
    // length starts toward the camera.
    let mut f = group([2.0, 0.0, 0.0]);
    f.set_view(camera);
    ctx.scroll_gesture_new = true;
    ctx.scroll_initiate_widget_id = None;
    f.wheel(&MouseScrollDelta::LineDelta(0.0, -6.0), cx, cy, &mut ctx);
    assert!(close(f.vector(), [0.0, 2.0, 0.0]), "six notches up: {:?}", f.vector());
    let mut f = group([0.0, 0.0, 0.0]);
    f.set_view(camera);
    f.drag_begin(cx, cy);
    f.drag_update(cx + 0.001, cy);
    assert!(close(f.vector(), [1.0, 0.0, 0.0]), "toward the camera: {:?}", f.vector());
    // A view that is not square is made so; one with no direction is refused.
    let mut f = group([0.0, 0.0, 2.0]);
    assert!(f.set_view([[2.0, 0.0, 1.0], [0.0, 9.0, 0.0], [0.0, 0.0, 3.0]]));
    assert_eq!(f.view(), IDENTITY_VIEW);
    assert!(!f.set_view([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 0.0]]));
    assert_eq!(f.view(), IDENTITY_VIEW);

    // The rings are circles of latitude about the vector: every point
    // on one is the same angle from it, whichever way it points.
    for dir in [[0.0f32, 0.0, 1.0], [0.3, 0.5, 0.4], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]] {
        let l = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        let d = dir.map(|c: f32| c / l);
        for degrees in RING_ANGLES {
            let ring = Float3::ring(dir, degrees, 24);
            assert_eq!(ring.len(), 25);
            assert!(close(ring[0], ring[24]), "the ring closes");
            for p in &ring {
                let dot = p[0] * d[0] + p[1] * d[1] + p[2] * d[2];
                assert!((dot - degrees.to_radians().cos()).abs() < 1e-5, "{degrees} degrees from {dir:?}: {p:?}");
                assert!(((p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt() - 1.0).abs() < 1e-5, "on the ball");
            }
        }
    }
    // Pointing at the viewer they are concentric circles about the
    // centre; turned a quarter onto +X the equator's ring is seen edge
    // on, a line down the middle.
    let facing = Float3::ring([0.0, 0.0, 1.0], 30.0, 24);
    assert!(facing.iter().all(|p| ((p[0] * p[0] + p[1] * p[1]).sqrt() - 0.5).abs() < 1e-5 && p[2] > 0.0));
    let edge_on = Float3::ring([1.0, 0.0, 0.0], 90.0, 24);
    assert!(edge_on.iter().all(|p| p[0].abs() < 1e-5));

    // Painted: strokes for the near halves of the rings, and they move
    // when the vector turns.
    let strokes = |v: [f32; 3]| -> Vec<(f32, f32)> {
        let mut pc = PaintCtx::new();
        group(v).paint_ball(&mut pc);
        pc.finish()
            .items
            .into_iter()
            .filter_map(|i| match i.prim {
                crate::scene::paint::Prim::Vector { x1, y1, thickness: 1.0, .. } => Some((x1, y1)),
                _ => None,
            })
            .collect()
    };
    let facing = strokes([0.0, 0.0, 2.0]);
    assert!(facing.len() > 60, "rings are drawn: {}", facing.len());
    assert!(facing.iter().all(|(x, y)| (x - cx).powi(2) + (y - cy).powi(2) <= r * r + 0.5), "on the ball");
    assert_ne!(facing, strokes([2.0, 0.0, 0.0]), "a turned vector turns its rings");
    assert_ne!(strokes([0.0, 0.0, 2.0]).len(), 0);

    // The ball paints a sphere and the vector on it; without one, nothing.
    let mut pc = PaintCtx::new();
    group([0.0, 0.0, 2.0]).paint_ball(&mut pc);
    let prims: Vec<_> = pc.finish().items.into_iter().map(|i| i.prim).collect();
    assert!(prims.iter().any(|p| matches!(p, crate::scene::paint::Prim::Sphere { .. })), "{prims:?}");
    assert!(prims.iter().any(|p| matches!(p, crate::scene::paint::Prim::Vector { .. })));
    let mut pc = PaintCtx::new();
    Float3::new().paint_ball(&mut pc);
    assert!(pc.finish().items.is_empty());
}
