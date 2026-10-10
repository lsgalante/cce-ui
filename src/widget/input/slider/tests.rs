use super::*;
use crate::widget::{MouseScrollDelta, WidgetHost, UiContext};

/// A soft range takes a typed value past its end and widens to hold
/// it; a hard one clamps the same number to the end.
#[test]
fn a_soft_range_widens_to_a_typed_value() {
    let typed = |soft: bool, text: &str| {
        let mut sl = Slider::new().with_range(-10.0, 10.0).with_readout(true).with_soft(soft);
        sl.editing = true;
        sl.edit_buffer = text.to_string();
        sl.commit_edit();
        (sl.get_scaled_value(), sl.range(), sl.just_changed)
    };
    let (v, range, changed) = typed(true, "500");
    assert!((v - 500.0).abs() < 1e-3 && range == (-10.0, 500.0) && changed, "{v} {range:?}");
    let (v, range, _) = typed(true, "-42");
    assert!((v + 42.0).abs() < 1e-3 && range == (-42.0, 10.0));
    let (v, range, _) = typed(true, "3");
    assert!((v - 3.0).abs() < 1e-3 && range == (-10.0, 10.0), "inside, nothing widens");
    let (v, range, _) = typed(false, "500");
    assert!((v - 10.0).abs() < 1e-3 && range == (-10.0, 10.0), "a hard range clamps");
}

/// The legacy rangeslider interaction test, driven through the WidgetHost drag forwards
/// (hosts call these directly): thumb selection by proximity, constrained updates.
#[test]
fn rangeslider_interaction() {
    let mut rs = RangeSlider::new();
    WidgetHost::set_rect(&mut rs, 10.0, 10.0, 200.0, 20.0);
    assert_eq!(rs.values(), (0.2, 0.8));

    // Thumb size 18, range 182; low center = 55.4.
    rs.drag_begin(55.4, 20.0);
    assert_eq!(rs.active_thumb, Some(ActiveThumb::Low));
    assert!(rs.drag_update(100.9, 20.0));
    assert!((rs.values().0 - 0.45).abs() < 0.01);
    assert_eq!(rs.values().1, 0.8);
    rs.drag_end();
    assert_eq!(rs.active_thumb, None);

    // High thumb 0.8 -> 0.6.
    rs.drag_begin(164.6, 20.0);
    assert_eq!(rs.active_thumb, Some(ActiveThumb::High));
    assert!(rs.drag_update(128.2, 20.0));
    assert!((rs.values().1 - 0.6).abs() < 0.01);
    rs.drag_end();
}

#[test]
fn rangeslider_overlap_and_constraint() {
    let mut rs = RangeSlider::new().with_values(0.5, 0.5);
    WidgetHost::set_rect(&mut rs, 10.0, 10.0, 200.0, 20.0);

    rs.drag_begin(109.0, 20.0);
    assert_eq!(rs.active_thumb, Some(ActiveThumb::Low));
    rs.drag_end();

    rs.drag_begin(111.0, 20.0);
    assert_eq!(rs.active_thumb, Some(ActiveThumb::High));
    rs.drag_end();

    rs.drag_begin(110.0, 20.0);
    rs.drag_update(150.0, 20.0);
    assert_eq!(rs.values().0, 0.5, "low constrained to high");
    rs.drag_end();
}

/// Slider press-on-track begins a drag through the routed path; wheel adjusts the value
/// with the scroll-gesture gating intact.
#[test]
fn slider_press_drag_and_wheel() {
    let mut ctx = UiContext::new();
    let sl = ctx.insert(Slider::new().with_value(0.5));
    let id = sl.id();
    WidgetHost::set_rect(&mut ctx[sl], 0.0, 0.0, 100.0, 20.0);

    // Press on the track grabs the thumb.
    assert!(ctx.propagate_event(
        &Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, x: 50.0, y: 10.0, local_x: 50.0, local_y: 10.0 },
        id,
    ));
    assert!(ctx[sl].is_dragging());
    assert!(ctx[sl].drag_update(80.0, 10.0));
    assert!(ctx[sl].inner().value() > 0.5);
    ctx[sl].drag_end();

    // Wheel adjusts value when the gesture starts fresh.
    ctx.scroll_gesture_new = true;
    let before = ctx[sl].inner().value();
    assert!(ctx.lend_h(sl, |w, ctx| w.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, 1.0), 50.0, 10.0, ctx)).unwrap());
    assert!(ctx[sl].inner().value() > before, "a wheel notch up is more");
    assert!(ctx[sl].take_change());
}

/// A notch steps 2% of the range up to a 20-wide one, exactly as it
/// always did; a wider range steps by the value's magnitude — as a
/// 20-wide slider near zero, by the full 2% far from it — for the wheel
/// and the arrow keys alike.
#[test]
fn a_wide_range_steps_by_the_values_magnitude() {
    let notch = |min: f32, max: f32, at: f32| -> f32 {
        let mut ctx = UiContext::new();
        let sl = ctx.insert(Slider::new().with_range(min, max));
        WidgetHost::set_rect(&mut ctx[sl], 0.0, 0.0, 200.0, 20.0);
        ctx[sl].inner_mut().set_scaled_value(at);
        let before = ctx[sl].inner().get_scaled_value();
        ctx.scroll_gesture_new = true;
        // Over the band at the value, where the halo is.
        let x = 200.0 * ctx[sl].inner().value();
        assert!(ctx.lend_h(sl, |w, ctx| w.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, 1.0), x.clamp(1.0, 199.0), 10.0, ctx)).unwrap());
        ctx[sl].inner().get_scaled_value() - before
    };
    let close = |got: f32, want: f32| (got - want).abs() <= want * 0.01 + 1e-3;
    // Ordinary ranges: 2% of the range, untouched.
    assert!(close(notch(0.0, 2.0, 1.0), 0.04), "{}", notch(0.0, 2.0, 1.0));
    assert!(close(notch(-10.0, 10.0, 0.0), 0.4), "{}", notch(-10.0, 10.0, 0.0));
    assert!(close(notch(-10.0, 10.0, 8.0), 0.4));
    // -1000..1000: near zero a notch is what a 20-wide slider's is…
    assert!(close(notch(-1000.0, 1000.0, 0.0), 0.4), "{}", notch(-1000.0, 1000.0, 0.0));
    assert!(close(notch(-1000.0, 1000.0, 0.5), 0.4));
    // …it grows with the magnitude, on either side of zero…
    assert!(close(notch(-1000.0, 1000.0, 10.0), 4.0), "{}", notch(-1000.0, 1000.0, 10.0));
    assert!(close(notch(-1000.0, 1000.0, -10.0), 4.0));
    // …and is capped at the 2% of the range it used to be everywhere.
    assert!(close(notch(-1000.0, 1000.0, 500.0), 40.0), "{}", notch(-1000.0, 1000.0, 500.0));

    // End to end it is still a few dozen notches, not thousands.
    let mut sl = Slider::new().with_range(-1000.0, 1000.0);
    sl.inner_mut().set_scaled_value(0.0);
    let mut notches = 0;
    while sl.inner().get_scaled_value() < 999.0 && notches < 1000 {
        let step = sl.inner().notch_step();
        let v = (sl.inner().value() + step).clamp(0.0, 1.0);
        sl.inner_mut().set_value(v);
        notches += 1;
    }
    assert!(notches < 60, "0 to 1000 took {notches} notches");
}

/// A slider hovers like every other control: the adapter's hover
/// bookkeeping turns a move over the row into `MouseEnter`, a move away
/// into `MouseLeave`, and the band reads the flag. A float3's three rows
/// each hover on their own, since the group forwards the move to them.
#[test]
fn a_slider_hovers_under_the_pointer() {
    let mut ctx = UiContext::new();
    let sl = ctx.insert(Slider::new());
    WidgetHost::set_rect(&mut ctx[sl], 0.0, 0.0, 100.0, 20.0);
    assert!(!ctx[sl].inner().hovered());
    assert!(ctx.lend_h(sl, |w, ctx| w.on_cursor_moved(50.0, 10.0, ctx)).unwrap(), "entering is a change");
    assert!(ctx[sl].inner().hovered());
    assert!(!ctx.lend_h(sl, |w, ctx| w.on_cursor_moved(60.0, 10.0, ctx)).unwrap(), "moving within is not");
    assert!(ctx.lend_h(sl, |w, ctx| w.on_cursor_moved(500.0, 10.0, ctx)).unwrap(), "leaving is");
    assert!(!ctx[sl].inner().hovered());

    let mut f = crate::widget::display::Float3::new();
    WidgetHost::set_rect(&mut f, 0.0, 0.0, 200.0, crate::widget::display::Float3::preferred_height(false));
    let rows = f.inner().get_row_rects();
    let (_, y1, _, h1) = rows[1];
    assert!(f.on_cursor_moved(100.0, y1 + h1 * 0.5, &mut ctx));
    let hovered: Vec<bool> = f.inner().sliders().iter().map(|s| s.inner().hovered()).collect();
    assert_eq!(hovered, vec![false, true, false], "the row under the pointer, and only it");
}
