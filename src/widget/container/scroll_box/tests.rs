use super::*;

/// A sink-behind box's bar rides the centre line, takes no press while
/// sunk (the content under its lane does), is raised by a wheel and held
/// by a pointer over it, and sinks once nothing holds it; the fore copy
/// fades rather than flips. A plain box keeps its edge bar.
#[test]
fn a_sink_behind_bar_rides_the_centre_and_sinks_until_scrolled() {
    let mut ui = UiContext::new();
    let mut sb = ScrollBox::new();
    sb.sink_behind = true;
    sb.set_rect(10.0, 20.0, 200.0, 100.0);
    sb.update_bounds(400.0, 20.0, 100.0);
    let (sb_x, _, sb_w, _, _, _) = sb.bar_geom().unwrap();
    assert!((sb_x + sb_w * 0.5 - 110.0).abs() < 0.01, "on the centre line");
    assert_eq!(sb_w, crate::layout::centred_scrollbar_width());

    // Sunk: the lane is the content's.
    assert!(!sb.scrollbar_raised());
    assert!(!sb.hit_test_scrollbar(110.0, 60.0));
    assert!(!sb.mouse_input(MouseButton::Left, ElementState::Pressed, 110.0, 60.0, &mut ui));
    assert!(!sb.scrollbar_dragging);
    // Hover never raises a sunk bar.
    sb.on_cursor_moved(110.0, 60.0, &mut ui);
    sb.tick(0.016, &mut ui);
    assert!(!sb.scrollbar_raised());

    // A wheel raises it; the fore copy fades in.
    assert!(sb.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, -1.0), 50.0, 60.0, &mut ui));
    assert!(sb.scrollbar_raised());
    assert!(sb.tick(0.05, &mut ui));
    assert!(sb.scrollbar_fade() > 0.0 && sb.scrollbar_fade() < 1.0, "fading in: {}", sb.scrollbar_fade());
    for _ in 0..1000 {
        if !sb.is_animating() { break; }
        sb.tick(1.0 / 60.0, &mut ui);
    }
    // Raised, the bar takes the press; a pointer over it holds it up.
    assert!(sb.hit_test_scrollbar(110.0, 60.0));
    sb.on_cursor_moved(110.0, 60.0, &mut ui);
    sb.tick(1.0, &mut ui);
    assert!(sb.scrollbar_raised(), "held by the pointer");
    sb.on_cursor_moved(40.0, 60.0, &mut ui);
    sb.tick(0.016, &mut ui);
    assert!(!sb.scrollbar_raised(), "unheld, it sinks");
    for _ in 0..30 {
        sb.tick(0.016, &mut ui);
    }
    assert_eq!(sb.scrollbar_fade(), 0.0);

    let mut plain = ScrollBox::new();
    plain.set_rect(10.0, 20.0, 200.0, 100.0);
    plain.update_bounds(400.0, 20.0, 100.0);
    let (sb_x, _, sb_w, _, _, _) = plain.bar_geom().unwrap();
    assert_eq!(sb_x + sb_w, 10.0 + 200.0 - 4.0, "a plain box keeps its edge bar");
    assert!(plain.scrollbar_raised() && plain.hit_test_scrollbar(sb_x + 1.0, 60.0));
}

#[test]
fn test_scroll_box_bounds_scrolling() {
    let mut sb = ScrollBox::new();
    sb.set_rect(10.0, 20.0, 100.0, 100.0);
    
    // 1. Initially scroll is 0
    assert_eq!(sb.scroll_y, 0.0);

    // 2. Update bounds: content_h = 150 (greater than viewport_h = 100)
    sb.update_bounds(150.0, 20.0, 100.0);
    assert_eq!(sb.scroll_y, 0.0);
    assert_eq!(sb.content_h, 150.0);
    assert_eq!(sb.viewport_h, 100.0);

    // 3. Scroll inside bounds
    let delta = MouseScrollDelta::LineDelta(0.0, -2.0); // scroll down by 2 lines (48px)
    let mut dummy = UiContext::new();
    let changed = sb.mouse_wheel(&delta, 50.0, 50.0, &mut dummy);
    assert!(changed);
    // The notch glides: run the motion out before reading the offset.
    for _ in 0..1000 {
        if !sb.is_animating() { break; }
        sb.tick(1.0 / 60.0, &mut dummy);
    }
    assert_eq!(sb.scroll_y, 48.0);

    // 4. Clamps at max scroll: 150 - 100 = 50
    let delta_large = MouseScrollDelta::LineDelta(0.0, -10.0);
    sb.mouse_wheel(&delta_large, 50.0, 50.0, &mut dummy);
    for _ in 0..1000 {
        if !sb.is_animating() { break; }
        sb.tick(1.0 / 60.0, &mut dummy);
    }
    assert_eq!(sb.scroll_y, 50.0);

    // 5. Test item draw coordinates (intersection contract: partially
    // visible items are returned so callers draw them cut by the clip).
    // Virtual item at virtual_y = 10, item_h = 24
    // Screen draw y = 20 + 10 - 50 = -20; bottom = 4 < viewport_y - 1
    // (19.0): fully above the viewport, culled.
    assert!(sb.get_item_draw_y(10.0, 24.0).is_none());

    // Virtual item at virtual_y = 40, item_h = 24
    // Screen draw y = 20 + 40 - 50 = 10: straddles the viewport top
    // (bottom = 34 >= 19.0) — returned, drawn cut by the clip.
    assert_eq!(sb.get_item_draw_y(40.0, 24.0), Some(10.0));

    // Virtual item at virtual_y = 60, item_h = 24
    // Screen draw y = 20 + 60 - 50 = 30: fully inside.
    assert_eq!(sb.get_item_draw_y(60.0, 24.0), Some(30.0));
}

#[test]
fn test_scroll_box_keyboard_input() {
    let mut sb = ScrollBox::new();
    sb.set_rect(10.0, 20.0, 100.0, 100.0);
    sb.update_bounds(300.0, 20.0, 100.0); // max_scroll = 200.0

    let mut ctx = UiContext::new();
    // Hover the scroll box (the focus path took a ctx-registered WidgetHost; as a plain
    // struct the hovered branch is the live gate).
    ctx.set_cursor_pos(50.0, 50.0);

    // 1. ArrowDown key
    let event_down = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Named(NamedKey::ArrowDown),
        text: None,
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    assert!(sb.keyboard_input(&event_down, &mut ctx));
    assert_eq!(sb.scroll_y, 24.0);

    // 2. PageDown key
    let event_pgdown = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Named(NamedKey::PageDown),
        text: None,
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    assert!(sb.keyboard_input(&event_pgdown, &mut ctx));
    assert_eq!(sb.scroll_y, 124.0);

    // 3. End key
    let event_end = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Named(NamedKey::End),
        text: None,
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    assert!(sb.keyboard_input(&event_end, &mut ctx));
    assert_eq!(sb.scroll_y, 200.0); // clamps at max_scroll = 200.0

    // 4. PageUp key
    let event_pgup = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Named(NamedKey::PageUp),
        text: None,
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    assert!(sb.keyboard_input(&event_pgup, &mut ctx));
    assert_eq!(sb.scroll_y, 100.0);

    // 5. Home key
    let event_home = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Named(NamedKey::Home),
        text: None,
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    assert!(sb.keyboard_input(&event_home, &mut ctx));
    assert_eq!(sb.scroll_y, 0.0);
}

#[test]
fn test_scroll_box_keys_gated_on_hover() {
    let mut sb = ScrollBox::new();
    sb.set_rect(10.0, 20.0, 100.0, 100.0);
    sb.update_bounds(300.0, 20.0, 100.0); // max_scroll = 200.0

    let mut ctx = UiContext::new();

    let event_down = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Named(NamedKey::ArrowDown),
        text: None,
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };

    // Cursor away from the box, nothing focused: keys are ignored.
    ctx.set_cursor_pos(500.0, 500.0);
    assert!(!sb.keyboard_input(&event_down, &mut ctx));
    assert_eq!(sb.scroll_y, 0.0);

    // Hovered: keys scroll.
    ctx.set_cursor_pos(50.0, 50.0);
    assert!(sb.keyboard_input(&event_down, &mut ctx));
    assert_eq!(sb.scroll_y, 24.0);
}
