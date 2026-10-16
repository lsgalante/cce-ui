use super::*;
use crate::widget::{WidgetHost, Widget};

/// The router's drag lifecycle drives the Input drag hooks end-to-end: a routed press
/// records the drag target, the first >3px move synthesizes DragStart, further moves
/// deliver DragUpdate (the slider value follows), and the release delivers DragEnd.
/// Regression test for the silent-drop gap: `Input::on_event` defaults ignore Drag*
/// events, so `Adapted::handle_event` must map them onto the hooks itself.
#[test]
fn routed_drag_reaches_input_drag_hooks() {
    use crate::widget::{ElementState, Event, MouseButton, Slider};

    let mut ctx = UiContext::new();
    let slider = ctx.insert(Slider::new());
    WidgetHost::set_rect(&mut ctx[slider], 0.0, 0.0, 200.0, 30.0);
    let id = slider.id();

    let press = Event::MouseButton {
        button: MouseButton::Left,
        state: ElementState::Pressed,
        x: 100.0,
        y: 15.0,
        local_x: 100.0,
        local_y: 15.0,
    };
    assert!(ctx.propagate_event(&press, id), "press in the track arms the drag");
    assert!(ctx[slider].is_dragging());
    let v0 = ctx[slider].value;

    // First move past the 3px threshold starts the drag; the next one updates it.
    let mv = |x: f32| Event::PointerMove { x, y: 15.0, local_x: x, local_y: 15.0 };
    ctx.propagate_event(&mv(110.0), id);
    assert!(ctx.is_dragging, "router crossed the drag threshold");
    ctx.propagate_event(&mv(140.0), id);
    assert!(
        ctx[slider].value > v0 + 0.05,
        "DragUpdate reached Input::drag_update (value {} -> {})",
        v0,
        ctx[slider].value
    );

    let release = Event::MouseButton {
        button: MouseButton::Left,
        state: ElementState::Released,
        x: 140.0,
        y: 15.0,
        local_x: 140.0,
        local_y: 15.0,
    };
    ctx.propagate_event(&release, id);
    assert!(!ctx[slider].is_dragging(), "DragEnd reached Input::drag_end");
    assert!(!ctx.is_dragging);
}

/// The multi-root press dispatch (how apps actually loop: one press propagated to
/// EVERY top-level root, no break): a later root's propagate call must not wipe the
/// drag target an earlier root just armed. This was live-broken in every plain-loop
/// app (the demo, colors) while the single-root test above passed — found the first
/// time a held drag could be driven headlessly (ccectl pointer-press).
#[test]
fn multi_root_press_dispatch_keeps_the_drag_target() {
    let mut ctx = UiContext::new();
    let slider = ctx.insert(crate::widget::Slider::new().with_value(0.5));
    let id = slider.id();
    ctx[slider].set_rect(0.0, 0.0, 200.0, 30.0);
    let other_id = ctx.insert(Block { base: Widget::new_rect(300.0, 300.0, 50.0, 50.0) }).id();

    let press = Event::MouseButton {
        button: MouseButton::Left,
        state: ElementState::Pressed,
        x: 100.0,
        y: 15.0,
        local_x: 100.0,
        local_y: 15.0,
    };
    // The app loop: same press to both roots, the slider first.
    assert!(ctx.propagate_event(&press, id));
    ctx.propagate_event(&press, other_id);
    assert_eq!(ctx.drag_target, Some(id), "the second root's call must not wipe the armed target");

    let v0 = ctx[slider].value;
    let mv = |x: f32| Event::PointerMove { x, y: 15.0, local_x: x, local_y: 15.0 };
    for root in [id, other_id] {
        ctx.propagate_event(&mv(110.0), root);
    }
    for root in [id, other_id] {
        ctx.propagate_event(&mv(140.0), root);
    }
    assert!(ctx.is_dragging, "threshold crossed despite multi-root dispatch");
    assert!(ctx[slider].value > v0 + 0.05, "DragUpdate drove the slider ({} -> {})", v0, ctx[slider].value);

    let release = Event::MouseButton {
        button: MouseButton::Left,
        state: ElementState::Released,
        x: 140.0,
        y: 15.0,
        local_x: 140.0,
        local_y: 15.0,
    };
    for root in [id, other_id] {
        ctx.propagate_event(&release, root);
    }
    assert!(!ctx[slider].is_dragging());
    assert!(!ctx.is_dragging);
}

/// A plain drag-blocking widget (the `WidgetHost` default) at a fixed rect.
struct Block {
    base: Widget,
}
impl WidgetHost for Block {
    crate::impl_widget_base!(Block);
}

/// `drag_allowed_at` — the window-drag question: allowed on empty surface, denied over a
/// drag-blocking widget.
#[test]
fn drag_allowed_everywhere_except_blocking_widgets() {
    let mut ctx = UiContext::new();
    ctx.insert(Block { base: Widget::new_rect(10.0, 10.0, 50.0, 50.0) });
    ctx.rebuild_spatial_grid();

    assert!(ctx.drag_allowed_at(200.0, 200.0), "empty surface is draggable");
    assert!(!ctx.drag_allowed_at(20.0, 20.0), "a drag-blocking widget denies the drag");
}

/// A widget whose popover is open over a fixed rect.
struct Popped {
    base: Widget,
    popover: (f32, f32, f32, f32),
}
impl WidgetHost for Popped {
    crate::impl_widget_base!(Popped);
    fn popover_rect(&self) -> Option<(f32, f32, f32, f32)> {
        Some(self.popover)
    }
}

/// The coverage memo is built from the widgets the context can see, which leaves out one
/// that is lent. A query made during a lend (a widget's own hit gate asks one) is memoized
/// without the lent widget's popover; once the widget is back, the next query at the same
/// point must see that popover again — through `lend_h` and through `lend` by id.
#[test]
fn a_lent_widgets_popover_covers_again_once_it_is_back() {
    let mut ctx = UiContext::new();
    let menu = ctx.insert(Popped { base: Widget::new_rect(0.0, 0.0, 40.0, 20.0), popover: (0.0, 20.0, 100.0, 100.0) });
    let under = ctx.insert(Block { base: Widget::new_rect(10.0, 40.0, 50.0, 20.0) }).id();
    let (px, py) = (20.0, 50.0);
    assert!(ctx.is_coordinate_covered(under, px, py), "the open popover covers the widget under it");

    ctx.invalidate_coverage_cache();
    let during = ctx.lend_h(menu, |_, ctx| ctx.is_coordinate_covered(under, px, py));
    assert_eq!(during, Some(false), "while lent the menu is out of the context's sight");
    assert!(ctx.is_coordinate_covered(under, px, py), "back from lend_h, its popover covers again");

    ctx.invalidate_coverage_cache();
    ctx.lend(menu.id(), |_, ctx| ctx.is_coordinate_covered(under, px, py));
    assert!(ctx.is_coordinate_covered(under, px, py), "back from lend, its popover covers again");
}

/// A widget with no popover adds nothing to the memo, so lending it keeps the memo: a
/// pointer move lends every widget in turn, and a rebuild after each is the N×N scan the
/// memo exists to avoid.
#[test]
fn lending_a_widget_with_no_popover_keeps_the_coverage_memo() {
    let mut ctx = UiContext::new();
    ctx.insert(Popped { base: Widget::new_rect(0.0, 0.0, 40.0, 20.0), popover: (0.0, 20.0, 100.0, 100.0) });
    let plain = ctx.insert(Block { base: Widget::new_rect(10.0, 40.0, 50.0, 20.0) });
    let other = ctx.insert(Block { base: Widget::new_rect(200.0, 0.0, 10.0, 10.0) }).id();
    let before = ctx.covered_cache.borrow().builds;
    ctx.lend_h(plain, |w, ctx| ctx.is_coordinate_covered(w.base().id(), 20.0, 50.0));
    assert!(ctx.is_coordinate_covered(other, 20.0, 50.0));
    assert_eq!(ctx.covered_cache.borrow().builds, before + 1, "one build serves the lend and after it");
}

/// A key for a composite's field (`claim_field_focus`) reaches the composite, which holds
/// the keyboard, though the focus record names a part nothing can deliver to and the key
/// was dispatched into another tree: a scroll key and any other.
#[test]
fn a_key_for_a_field_reaches_the_composite_that_holds_it() {
    use crate::widget::test_composite::Composite;
    use crate::widget::{Button, ElementState, Event, Key, KeyEvent, NamedKey};
    let mut ctx = UiContext::new();
    let c = ctx.insert(Composite::new());
    WidgetHost::set_rect(&mut ctx[c], 10.0, 10.0, 200.0, 100.0);
    let elsewhere = ctx.insert(Button::new(10.0, 200.0, 80.0, 24.0).with_label("Elsewhere"));
    let field = ctx[c].fields[1];
    ctx.claim_field_focus(c.id(), field);
    let key = |k: Key| Event::KeyInput(KeyEvent { logical_key: k, state: ElementState::Pressed, text: None, repeat: false, ctrl: false, shift: false, alt: false });

    assert!(ctx.propagate_event(&key(Key::Named(NamedKey::ArrowDown)), elsewhere.id()));
    assert!(ctx.propagate_event(&key(Key::Character("a".into())), elsewhere.id()));
    assert_eq!(ctx[c].keys, [Key::Named(NamedKey::ArrowDown), Key::Character("a".into())]);
}

/// A scroll gesture belongs to the first thing a wheel of it has an effect on: every other
/// widget declines the rest of it, wherever the pointer is — a page scrolled under the pointer
/// does not hand the gesture to the control it carries there — and the owner keeps it wherever
/// the pointer goes. A new gesture is anyone's.
#[test]
fn a_scroll_gesture_belongs_to_what_it_first_moved() {
    use crate::widget::{Event, MouseScrollDelta, Slider};
    let mut ctx = UiContext::new();
    let a = ctx.insert(Slider::new().with_value(0.5));
    let b = ctx.insert(Slider::new().with_value(0.5));
    WidgetHost::set_rect(&mut ctx[a], 0.0, 0.0, 200.0, 30.0);
    WidgetHost::set_rect(&mut ctx[b], 0.0, 100.0, 200.0, 30.0);
    let wheel = |x: f32, y: f32| Event::MouseWheel { delta: MouseScrollDelta::LineDelta(0.0, 1.0), x, y, local_x: x, local_y: y };
    // As an app dispatches it: to each root in turn, a wheel's interval after the last (the
    // router reads wheels under 5 ms apart as one).
    let dispatch = |ctx: &mut UiContext, x: f32, y: f32| {
        std::thread::sleep(std::time::Duration::from_millis(8));
        let ev = wheel(x, y);
        let took_a = ctx.propagate_event(&ev, a.id());
        let took_b = ctx.propagate_event(&ev, b.id());
        (took_a, took_b)
    };

    // The gesture starts on A: A moves and owns it.
    assert_eq!(dispatch(&mut ctx, 100.0, 15.0), (true, false));
    assert_eq!(ctx.scroll_gesture_owner(), Some(a.id()));
    let (va, vb) = (ctx[a].inner().value(), ctx[b].inner().value());
    assert!(va > 0.5 && vb == 0.5);
    // The same gesture over B: B declines it, A keeps it.
    assert_eq!(dispatch(&mut ctx, 100.0, 115.0), (true, false), "the owner keeps it off its rect");
    assert_eq!(ctx[b].inner().value(), 0.5, "B declined");
    assert!(ctx[a].inner().value() > va, "A moved on");

    // Something outside the widgets owns a gesture (a page an app scrolls): both decline.
    ctx.last_scroll_time = None;
    ctx.note_scroll_event();
    let page = crate::widget::Widget::new().id();
    ctx.claim_scroll_gesture(page);
    assert_eq!(dispatch(&mut ctx, 100.0, 115.0), (false, false), "the page's gesture is no slider's");
    assert_eq!(ctx[b].inner().value(), 0.5);

    // A new gesture over B is B's.
    ctx.last_scroll_time = None;
    assert_eq!(dispatch(&mut ctx, 100.0, 115.0), (false, true));
    assert_eq!(ctx.scroll_gesture_owner(), Some(b.id()));
}

/// A ScrollBox an app scrolls itself takes part in the rule: a wheel that moves it claims the
/// gesture, so a control its scroll carries under the pointer declines the rest; while the
/// gesture is another's, the box stays put.
#[test]
fn a_scroll_box_claims_the_gesture_it_moves() {
    use crate::widget::{Event, MouseScrollDelta, Slider};
    let mut ctx = UiContext::new();
    let mut page = crate::widget::ScrollBox::new();
    page.set_rect(0.0, 0.0, 300.0, 200.0);
    page.update_bounds(1000.0, 0.0, 200.0);
    let delta = MouseScrollDelta::LineDelta(0.0, -1.0);
    assert!(page.mouse_wheel(&delta, 100.0, 100.0, &mut ctx), "the page scrolls");
    assert_eq!(ctx.scroll_gesture_owner(), Some(page.base.id()));
    // The control now under the pointer declines the same gesture.
    let s = ctx.insert(Slider::new().with_value(0.5));
    WidgetHost::set_rect(&mut ctx[s], 0.0, 90.0, 200.0, 30.0);
    let ev = Event::MouseWheel { delta, x: 100.0, y: 100.0, local_x: 100.0, local_y: 100.0 };
    let pause = || std::thread::sleep(std::time::Duration::from_millis(8));
    pause();
    assert!(!ctx.propagate_event(&ev, s.id()));
    assert_eq!(ctx[s].inner().value(), 0.5);
    pause();
    assert!(page.mouse_wheel(&delta, 100.0, 100.0, &mut ctx), "and the page goes on");

    // A gesture the slider took: the page stays put under it.
    ctx.last_scroll_time = None;
    assert!(ctx.propagate_event(&ev, s.id()));
    let y = page.scroll_y;
    pause();
    assert!(!page.mouse_wheel(&delta, 100.0, 100.0, &mut ctx));
    assert_eq!(page.scroll_y, y);
}
