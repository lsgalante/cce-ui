use super::*;
use crate::widget::{WidgetHost, UiContext};

/// The value's well and the -/+ run are ONE field: one outline round
/// both, its seam where the run begins — not a well and a trough side by
/// side, each turning its own corner there — with the engraved seam
/// between - and + on top.
#[test]
fn a_spinbox_is_one_field_with_its_run_at_the_right_end() {
    use crate::scene::paint::{PaintCtx, Prim};
    if !crate::layout::control_relief() {
        return;
    }
    let sb = Spinbox::new(0, -100, 100, 1);
    let rect = Rect { x: 10.0, y: 20.0, width: 200.0, height: 26.0 };
    let rel = sb.inner().relief_parts(rect).expect("a relief");
    assert_eq!(rel.rect, rect, "the outline is the control as handed over");
    let (split, (sa, sb2, _, _)) = rel.run.expect("a -/+ run");
    assert!(split > rect.x + rect.width * 0.5 && split < rect.x + rect.width, "the run is the right end");
    assert!(sa.0 > split && sa.0 == sb2.0, "the -/+ seam crosses the run");
    let mut pc = PaintCtx::new();
    Paint::paint(sb.inner(), rect, &mut pc);
    let prims: Vec<Prim> = pc.finish().items.into_iter().map(|i| i.prim).collect();
    let fields: Vec<&Prim> = prims.iter().filter(|p| matches!(p, Prim::Field { .. })).collect();
    assert_eq!(fields.len(), 1, "{prims:?}");
    if let Prim::Field { split: s, .. } = fields[0] {
        assert_eq!(*s, split);
    }
    assert!(
        !prims.iter().any(|p| matches!(p, Prim::Recess { .. } | Prim::Trough { .. })),
        "no well or ring of its own beside the field"
    );
    assert!(prims.iter().any(|p| matches!(p, Prim::Groove { .. })), "the -/+ seam");
}

/// Up / Down step a focused spinbox, as its -/+ run does: the keys a keyboard user
/// steps it with, and what an assistive tool's Increment / Decrement press.
#[test]
fn up_and_down_step_a_focused_spinbox() {
    let key = |k| {
        Event::KeyInput(crate::widget::KeyEvent { logical_key: Key::Named(k), state: ElementState::Pressed, text: None, repeat: false, ctrl: false, shift: false, alt: false })
    };
    let mut ctx = UiContext::new();
    let mut sb = Spinbox::new(5, 0, 10, 2);
    WidgetHost::set_rect(&mut sb, 10.0, 20.0, 100.0, 26.0);
    assert!(!sb.handle_event(&key(NamedKey::ArrowUp), &mut ctx), "unfocused: not this box's key");
    sb.handle_event(&Event::FocusIn, &mut ctx);
    assert!(sb.handle_event(&key(NamedKey::ArrowUp), &mut ctx));
    assert_eq!(sb.value, 7);
    assert!(sb.take_change());
    assert!(sb.handle_event(&key(NamedKey::ArrowDown), &mut ctx));
    assert!(sb.handle_event(&key(NamedKey::ArrowDown), &mut ctx));
    assert_eq!(sb.value, 3);
    assert_eq!(sb.edit_buffer, "3", "the shown text follows while editing");
    for _ in 0..5 {
        sb.handle_event(&key(NamedKey::ArrowDown), &mut ctx);
    }
    assert_eq!(sb.value, 0, "clamped at the bottom");
}

/// Enter commits and leaves the box focused; the keys that edit or step it take it back
/// into editing, where until 2026-10-08 a focused box ignored every key after Enter.
#[test]
fn a_focused_spinbox_takes_keys_again_after_enter() {
    let key = |k: Key, text: Option<&str>| {
        Event::KeyInput(crate::widget::KeyEvent { logical_key: k, state: ElementState::Pressed, text: text.map(str::to_string), repeat: false, ctrl: false, shift: false, alt: false })
    };
    let mut ctx = UiContext::new();
    let mut sb = Spinbox::new(5, 0, 100, 1);
    sb.handle_event(&Event::FocusIn, &mut ctx);
    assert!(sb.handle_event(&key(Key::Named(NamedKey::Enter), None), &mut ctx));
    assert!(!sb.editing, "Enter commits");
    assert!(sb.handle_event(&key(Key::Named(NamedKey::ArrowUp), None), &mut ctx), "Up steps it again");
    assert_eq!(sb.value, 6);
    sb.handle_event(&key(Key::Named(NamedKey::Enter), None), &mut ctx);
    assert!(sb.handle_event(&key(Key::Character("4".into()), Some("4")), &mut ctx), "a digit reopens it");
    assert_eq!(sb.edit_buffer, "64", "typed at the end of the shown value");
    sb.handle_event(&Event::FocusOut, &mut ctx);
    assert!(!sb.handle_event(&key(Key::Named(NamedKey::ArrowUp), None), &mut ctx), "unfocused: not its key");
}

/// What a screen reader sets (AT-SPI SetCurrentValue) lands in the box's own units,
/// clamped, and is reported as a change; the range is read in the same units.
#[test]
fn a_reader_sets_a_spinbox_in_its_units() {
    let mut sb = Spinbox::new(150, 0, 500, 25).with_decimals(2);
    assert_eq!(sb.inner().a11y_range(), Some((0.0, 5.0, 0.25)));
    assert!(Input::a11y_set_value(sb.inner_mut(), 2.5));
    assert_eq!(sb.value, 250);
    assert!(sb.take_change());
    assert!(Input::a11y_set_value(sb.inner_mut(), 9.0));
    assert_eq!(sb.value, 500, "clamped to the top");
    assert!(!Input::a11y_set_value(sb.inner_mut(), 5.0), "no change, none reported");
}

#[test]
fn spinbox_button_zones_step_the_value() {
    let mut ctx = UiContext::new();
    let sb = ctx.insert(Spinbox::new(0, -100, 100, 1));
    WidgetHost::set_rect(&mut ctx[sb], 10.0, 20.0, 100.0, 26.0);

    // Legacy test: click at (75, 33) lands in the decrement zone.
    assert!(ctx.lend_h(sb, |w, ctx| w.mouse_input(MouseButton::Left, ElementState::Pressed, 75.0, 33.0, ctx)).unwrap());
    assert_eq!(ctx[sb].value, -1);
    assert!(ctx[sb].take_change());

    // Increment zone (past 77.5% of the width).
    assert!(ctx.lend_h(sb, |w, ctx| w.mouse_input(MouseButton::Left, ElementState::Pressed, 92.0, 33.0, ctx)).unwrap());
    assert_eq!(ctx[sb].value, 0);
}

#[test]
fn spinbox_buttons_step_visibly_while_editing() {
    // Row-selection focus puts the spinbox in edit mode (FocusIn →
    // begin_edit): the display shows edit_buffer. Stepping must commit
    // and refresh the buffer, or the value moves invisibly and the next
    // FocusOut commit resets it to the stale text.
    let mut ctx = UiContext::new();
    let sb = ctx.insert(Spinbox::new(6, 0, 100, 1));
    WidgetHost::set_rect(&mut ctx[sb], 10.0, 20.0, 100.0, 26.0);
    ctx[sb].begin_edit(true);
    assert_eq!(ctx[sb].edit_buffer, "6");

    assert!(ctx.lend_h(sb, |w, ctx| w.mouse_input(MouseButton::Left, ElementState::Pressed, 92.0, 33.0, ctx)).unwrap());
    assert_eq!(ctx[sb].value, 7);
    assert_eq!(ctx[sb].edit_buffer, "7");
    assert!(ctx[sb].take_change());

    // FocusOut now commits the refreshed buffer — the step survives.
    ctx.lend_h(sb, |w, ctx| w.handle_event(&Event::FocusOut, ctx)).unwrap();
    assert_eq!(ctx[sb].value, 7);
}

#[test]
fn spinbox_wheel_steps_by_notch_and_accumulates_fractions() {
    use crate::widget::MouseScrollDelta;
    let mut ctx = UiContext::new();
    let sb = ctx.insert(Spinbox::new(10, 0, 100, 5));
    WidgetHost::set_rect(&mut ctx[sb], 10.0, 20.0, 100.0, 26.0);

    // One notch up steps up, one notch down steps down — and the wheel is consumed.
    assert!(ctx.lend_h(sb, |w, ctx| w.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, 1.0), 50.0, 33.0, ctx)).unwrap());
    assert_eq!(ctx[sb].value, 15);
    assert!(ctx.lend_h(sb, |w, ctx| w.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, -1.0), 50.0, 33.0, ctx)).unwrap());
    assert_eq!(ctx[sb].value, 10);
    assert!(ctx[sb].take_change());

    // Fractional (trackpad) notches accumulate to a whole step, consumed all the while.
    assert!(ctx.lend_h(sb, |w, ctx| w.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, 0.5), 50.0, 33.0, ctx)).unwrap());
    assert_eq!(ctx[sb].value, 10, "half a notch: no step yet");
    assert!(ctx.lend_h(sb, |w, ctx| w.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, 0.5), 50.0, 33.0, ctx)).unwrap());
    assert_eq!(ctx[sb].value, 15, "the second half completes the notch");

    // Outside the rect the wheel is not the spinbox's (hit-gated by the adapter).
    assert!(!ctx.lend_h(sb, |w, ctx| w.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, 1.0), 200.0, 200.0, ctx)).unwrap());
    assert_eq!(ctx[sb].value, 15);
}

#[test]
fn spinbox_value_string_decimals_round_trip() {
    let mut sb = Spinbox::new(150, 0, 1000, 5).with_decimals(2);
    assert_eq!(sb.get_value_string(), Some("1.50".to_string()));
    assert!(sb.set_value_string("2.75"));
    assert_eq!(sb.value, 275);
    assert_eq!(sb.value(), 275);
}
