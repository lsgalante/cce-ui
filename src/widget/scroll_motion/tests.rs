use super::*;

fn smooth() -> ScrollSettings {
    ScrollSettings { smooth: true, ease_rate: 12.0, kinetic: true, friction: 6.0 }
}

fn settle(a: &mut ScrollAxis, b: Bounds, s: &ScrollSettings) -> u32 {
    let mut frames = 0;
    while a.is_animating() && frames < 10_000 {
        a.tick(1.0 / 60.0, b, s);
        frames += 1;
    }
    frames
}

#[test]
fn wheel_glides_to_target_and_settles() {
    let s = smooth();
    let b = Bounds::max(1000.0);
    let mut a = ScrollAxis::new(0.0);
    assert!(a.wheel(24.0, b, &s));
    assert_eq!(a.target(), 24.0);
    assert_eq!(a.pos(), 0.0, "the wheel moves the target, not the drawn offset");
    assert!(a.tick(1.0 / 60.0, b, &s));
    assert!(a.pos() > 0.0 && a.pos() < 24.0);
    let frames = settle(&mut a, b, &s);
    assert_eq!(a.pos(), 24.0);
    assert!(frames > 3 && frames < 60, "settled in {frames} frames");
}

#[test]
fn notches_accumulate_into_one_glide() {
    let s = smooth();
    let b = Bounds::max(1000.0);
    let mut a = ScrollAxis::new(0.0);
    a.wheel(24.0, b, &s);
    a.tick(1.0 / 60.0, b, &s);
    a.wheel(24.0, b, &s);
    assert_eq!(a.target(), 48.0);
    settle(&mut a, b, &s);
    assert_eq!(a.pos(), 48.0);
}

#[test]
fn wheel_target_clamps_to_bounds() {
    let s = smooth();
    let b = Bounds::max(30.0);
    let mut a = ScrollAxis::new(0.0);
    a.wheel(100.0, b, &s);
    assert_eq!(a.target(), 30.0);
    assert!(!a.wheel(100.0, b, &s), "a notch past the end moves nothing");
    settle(&mut a, b, &s);
    assert_eq!(a.pos(), 30.0);
}

#[test]
fn smoothing_off_jumps() {
    let s = ScrollSettings { smooth: false, ..smooth() };
    let b = Bounds::max(1000.0);
    let mut a = ScrollAxis::new(0.0);
    a.wheel(24.0, b, &s);
    assert_eq!(a.pos(), 24.0);
    assert!(!a.is_animating());
}

#[test]
fn finger_tracks_one_to_one_then_flings() {
    let s = smooth();
    let b = Bounds::max(10_000.0);
    let mut a = ScrollAxis::new(0.0);
    // A steady 15px every 8ms swipe.
    for _ in 0..10 {
        assert!(a.finger(15.0, 0.008, b));
    }
    assert_eq!(a.pos(), 150.0);
    assert!(!a.is_animating(), "no motion of its own while the finger is down");
    assert!(a.finger_end(0.01, &s));
    let before = a.pos();
    let frames = settle(&mut a, b, &s);
    assert!(a.pos() > before + 50.0, "coasted from {before} to {}", a.pos());
    assert!(frames > 5);
    assert_eq!(a.velocity(), 0.0);
}

#[test]
fn resting_finger_does_not_fling() {
    let s = smooth();
    let b = Bounds::max(10_000.0);
    let mut a = ScrollAxis::new(0.0);
    for _ in 0..10 {
        a.finger(15.0, 0.008, b);
    }
    assert!(!a.finger_end(0.5, &s), "a finger held still before lifting stops dead");
    assert_eq!(a.pos(), 150.0);
}

#[test]
fn the_animations_switch_stops_the_glide_and_not_the_coast() {
    let off = with_animations(smooth(), false);
    assert!(!off.smooth, "a notch jumps");
    assert!(off.kinetic, "a flick still coasts");
    assert_eq!((off.ease_rate, off.friction), (smooth().ease_rate, smooth().friction));
    assert_eq!(with_animations(smooth(), true), smooth());
    // What input.kdl turned off stays off.
    let plain = ScrollSettings { smooth: false, kinetic: false, ..smooth() };
    assert_eq!(with_animations(plain, true), plain);
}

#[test]
fn kinetic_off_stops_dead() {
    let s = ScrollSettings { kinetic: false, ..smooth() };
    let b = Bounds::max(10_000.0);
    let mut a = ScrollAxis::new(0.0);
    for _ in 0..10 {
        a.finger(15.0, 0.008, b);
    }
    assert!(!a.finger_end(0.01, &s));
    assert!(!a.is_animating());
}

#[test]
fn coast_stops_at_the_bound() {
    let s = smooth();
    let b = Bounds::max(200.0);
    let mut a = ScrollAxis::new(0.0);
    for _ in 0..10 {
        a.finger(15.0, 0.008, b);
    }
    a.finger_end(0.01, &s);
    settle(&mut a, b, &s);
    assert_eq!(a.pos(), 200.0);
    assert_eq!(a.velocity(), 0.0);
}

#[test]
fn wheel_during_coast_redirects() {
    let s = smooth();
    let b = Bounds::max(10_000.0);
    let mut a = ScrollAxis::new(0.0);
    for _ in 0..10 {
        a.finger(15.0, 0.008, b);
    }
    a.finger_end(0.01, &s);
    a.tick(1.0 / 60.0, b, &s);
    let p = a.pos();
    a.wheel(-24.0, b, &s);
    assert_eq!(a.velocity(), 0.0);
    assert!((a.target() - (p - 24.0)).abs() < 1e-3);
}

#[test]
fn reconcile_adopts_host_writes() {
    let s = smooth();
    let b = Bounds::max(1000.0);
    let mut a = ScrollAxis::new(0.0);
    a.wheel(240.0, b, &s);
    a.tick(1.0 / 60.0, b, &s);
    // The host dragged the thumb to 500 behind our back.
    a.reconcile(500.0);
    assert_eq!(a.pos(), 500.0);
    assert_eq!(a.target(), 500.0);
    assert!(!a.is_animating());
    // An unchanged host value is not a write.
    a.wheel(24.0, b, &s);
    a.reconcile(500.0);
    assert!(a.is_animating());
}

#[test]
fn bounds_shrink_reclamps_and_settles() {
    let s = smooth();
    let mut a = ScrollAxis::new(0.0);
    a.wheel(900.0, Bounds::max(1000.0), &s);
    settle(&mut a, Bounds::max(1000.0), &s);
    a.set_bounds(Bounds::max(100.0));
    assert_eq!(a.pos(), 100.0);
    assert_eq!(a.target(), 100.0);
}

#[test]
fn scroll_to_glides_keyboard_pages() {
    let s = smooth();
    let b = Bounds::max(1000.0);
    let mut a = ScrollAxis::new(0.0);
    assert!(a.scroll_to(400.0, b, &s));
    assert_eq!(a.pos(), 0.0);
    settle(&mut a, b, &s);
    assert_eq!(a.pos(), 400.0);
}

#[test]
fn ease_is_frame_rate_independent() {
    let s = smooth();
    let b = Bounds::max(1000.0);
    let mut fast = ScrollAxis::new(0.0);
    let mut slow = ScrollAxis::new(0.0);
    fast.wheel(500.0, b, &s);
    slow.wheel(500.0, b, &s);
    for _ in 0..12 {
        fast.tick(1.0 / 120.0, b, &s);
    }
    slow.tick(0.1, b, &s);
    assert!((fast.pos() - slow.pos()).abs() < 1.0, "120Hz {} vs 10Hz {}", fast.pos(), slow.pos());
}

#[test]
fn delta_conversion_matches_the_legacy_convention() {
    let (dx, dy) = ScrollMotion::delta_px(&MouseScrollDelta::LineDelta(0.0, -2.0), (LINE_PX, LINE_PX));
    assert_eq!((dx, dy), (0.0, 48.0));
    let (dx, dy) = ScrollMotion::delta_px(
        &MouseScrollDelta::PixelDelta(crate::widget::Position { x: 3.0, y: -10.0 }),
        (LINE_PX, LINE_PX),
    );
    assert_eq!((dx, dy), (-3.0, 10.0));
}

#[test]
fn unbounded_axis_pans_negative() {
    let s = smooth();
    let mut a = ScrollAxis::new(0.0);
    a.wheel(-300.0, Bounds::UNBOUNDED, &s);
    settle(&mut a, Bounds::UNBOUNDED, &s);
    assert_eq!(a.pos(), -300.0);
}
