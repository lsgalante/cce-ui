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
