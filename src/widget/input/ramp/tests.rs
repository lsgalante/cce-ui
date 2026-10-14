/// The ramp hands its fields the keyboard itself: the window's focus record names the
/// field (it checks the record before taking a key), Tab walks them, and none of them
/// enters the registry.
#[test]
fn the_ramp_hands_its_fields_the_keyboard() {
    use crate::widget::{Event, KeyEvent};
    let mut ctx = UiContext::new();
    let h = ctx.insert(Ramp::new());
    ctx.lend_h(h, |r, _| WidgetHost::set_rect(r, 0.0, 0.0, 300.0, 260.0));
    let (preset, line) = (ctx[h].field_id(0), ctx[h].field_id(1));
    ctx.set_focused_id(h.id());
    assert_eq!(ctx.focused_widget, Some(preset), "focused, the ramp gives the preset dropdown the keys");
    assert!(ctx[h].preset_dropdown.base().focused);
    let key = |named, shift| Event::KeyInput(KeyEvent {
        logical_key: Key::Named(named),
        state: ElementState::Pressed,
        text: None,
        repeat: false,
        ctrl: false,
        shift,
        alt: false,
    });
    assert!(ctx.propagate_event(&key(NamedKey::Tab, false), h.id()));
    assert_eq!(ctx.focused_widget, Some(line), "Tab walks to the line dropdown");
    assert!(!ctx[h].preset_dropdown.base().focused && ctx[h].line_type_dropdown.base().focused);
    ctx.propagate_event(&key(NamedKey::Tab, false), h.id());
    assert_eq!(ctx.focused_widget, Some(preset), "two fields with no key selected: it wraps");
    ctx.propagate_event(&key(NamedKey::Tab, true), h.id());
    assert_eq!(ctx.focused_widget, Some(line), "Shift+Tab walks back");
    assert!(ctx.propagate_event(&key(NamedKey::Enter, false), h.id()), "the focused dropdown takes Enter");
    assert!(ctx[h].line_type_dropdown.open, "and opens");
    // Focus moving on from the ramp reaches the field at the ramp's next tick.
    ctx.clear_focus();
    ctx.lend_h(h, |r, ctx| WidgetHost::tick(r, 0.016, ctx));
    assert!(!ctx[h].line_type_dropdown.base().focused, "the field let go");
    assert!(!ctx.tree.is_registered(line) && !ctx.tree.is_registered(preset), "no field entered the registry");
}

use super::*;

/// The Preset dropdown lists the presets and nothing else. A curve
/// edited by hand leaves the trigger blank rather than naming a preset
/// it no longer is, and picking a preset, the one last shown included,
/// puts it back.
#[test]
fn the_preset_dropdown_lists_only_presets() {
    let mut ramp = Ramp::new();
    let r = ramp.inner_mut();
    let names: Vec<&str> = RAMP_PRESETS.iter().map(|(n, _)| *n).collect();
    assert_eq!(r.preset_dropdown.options, names);
    assert!(!r.preset_dropdown.options.iter().any(|o| o == "Custom"));
    // A new ramp is the Raised curve, and says so.
    assert_eq!(r.preset_dropdown.options[r.preset_dropdown.selected], "Raised");
    assert_eq!(r.preset_dropdown.custom_display_text, None);

    // A hand edit: the curve is no preset, and the trigger is blank.
    r.keys[1].value = 0.3;
    r.sync_preset();
    assert_eq!(r.preset_dropdown.custom_display_text.as_deref(), Some(""));
    // Picking Raised again restores it.
    r.apply_preset(1);
    assert_eq!(r.keys.len(), 4);
    assert_eq!(r.preset_dropdown.custom_display_text, None);
    assert_eq!(r.preset_dropdown.options[r.preset_dropdown.selected], "Raised");

    // Every preset applies to its own curve and names itself.
    for (i, (name, keys)) in RAMP_PRESETS.iter().enumerate() {
        r.apply_preset(i);
        let got: Vec<(f32, f32)> = r.keys.iter().map(|k| (k.pos, k.value)).collect();
        assert_eq!(&got[..], *keys, "{name}");
        assert_eq!(r.preset_dropdown.options[r.preset_dropdown.selected], *name);
    }

    // A spec that is a preset shows it; one that is none goes blank.
    r.apply_preset(0);
    assert!(r.set_spec("linear;0.000:1.000,0.500:0.000,1.000:1.000"));
    assert_eq!(r.preset_dropdown.options[r.preset_dropdown.selected], "Valley");
    assert_eq!(r.preset_dropdown.custom_display_text, None);
    assert!(r.set_spec("linear;0.000:0.100,1.000:0.900"));
    assert_eq!(r.preset_dropdown.custom_display_text.as_deref(), Some(""));
}

/// A ramp registered at its natural height, with an event sender.
fn ramp_at_natural_height() -> (UiContext, Handle<Adapted<Ramp>>) {
    let mut ctx = UiContext::new();
    let h = ctx.insert(Ramp::new());
    let natural = ctx[h].natural_height();
    ctx.lend_h(h, |r, _| WidgetHost::set_rect(r, 0.0, 0.0, 300.0, natural));
    (ctx, h)
}

fn press(x: f32, y: f32) -> crate::widget::Event {
    crate::widget::Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, x, y, local_x: x, local_y: y }
}

fn release(x: f32, y: f32) -> crate::widget::Event {
    crate::widget::Event::MouseButton { button: MouseButton::Left, state: ElementState::Released, x, y, local_x: x, local_y: y }
}

fn key_event(named: NamedKey) -> crate::widget::Event {
    crate::widget::Event::KeyInput(crate::widget::KeyEvent {
        logical_key: Key::Named(named),
        state: ElementState::Pressed,
        text: None,
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    })
}

/// Key `i`'s centre on screen.
fn key_centre(r: &Ramp, i: usize) -> (f32, f32) {
    let plot = r.plot_rect();
    (plot.x + r.keys[i].pos * plot.width, plot.y + plot.height * (1.0 - r.keys[i].value))
}

/// At its natural height the ramp's graph is the height it declares for it, its
/// controls folded or not, and its key rings stay inside the opening.
#[test]
fn at_its_natural_height_the_graph_is_whole_and_holds_its_rings() {
    let (mut ctx, h) = ramp_at_natural_height();
    assert_eq!(ctx[h].graph_h(), Ramp::GRAPH_H);
    assert!(ctx[h].key_ring_r() + 2.5 <= Ramp::PLOT_INSET, "a key at the plot's edge keeps its rim off the wall");
    ctx[h].controls_collapsed = true;
    let folded = ctx[h].natural_height();
    ctx.lend_h(h, |r, _| WidgetHost::set_rect(r, 0.0, 0.0, 300.0, folded));
    assert_eq!(ctx[h].graph_h(), Ramp::GRAPH_H);
}

/// Opening the preset dropdown is no pick: a hand-edited curve survives it, and the
/// trigger stays blank. A pick from the keyboard does apply.
#[test]
fn opening_the_preset_dropdown_keeps_a_hand_edited_curve() {
    let (mut ctx, h) = ramp_at_natural_height();
    ctx[h].keys[1].value = 0.3;
    ctx[h].sync_preset();
    ctx.set_focused_id(h.id());
    assert!(ctx.propagate_event(&key_event(NamedKey::Enter), h.id()));
    assert!(ctx[h].preset_dropdown.open, "Enter opens the preset dropdown");
    for _ in 0..30 {
        ctx.lend_h(h, |r, ctx| WidgetHost::tick(r, 0.016, ctx));
    }
    ctx.propagate_event(&key_event(NamedKey::Escape), h.id());
    for _ in 0..30 {
        ctx.lend_h(h, |r, ctx| WidgetHost::tick(r, 0.016, ctx));
    }
    assert_eq!(ctx[h].keys[1].value, 0.3, "the edit survived opening and closing the dropdown");
    assert_eq!(ctx[h].preset_dropdown.custom_display_text.as_deref(), Some(""));

    // Enter on an open dropdown picks: the curve becomes that preset.
    ctx.propagate_event(&key_event(NamedKey::Enter), h.id());
    ctx.propagate_event(&key_event(NamedKey::Enter), h.id());
    ctx.lend_h(h, |r, ctx| WidgetHost::tick(r, 0.016, ctx));
    let picked = ctx[h].preset_dropdown.selected;
    let got: Vec<(f32, f32)> = ctx[h].keys.iter().map(|k| (k.pos, k.value)).collect();
    assert_eq!(&got[..], RAMP_PRESETS[picked].1, "a keyboard pick applies its preset");
}

/// A release is the ramp's only when it used it: the end of a key drag is, a release
/// elsewhere with a key selected is not — it belongs to whatever was pressed.
#[test]
fn a_release_is_the_ramps_only_when_it_used_it() {
    let (mut ctx, h) = ramp_at_natural_height();
    let (x, y) = key_centre(&ctx[h], 1);
    assert!(ctx.propagate_event(&press(x, y), h.id()));
    assert_eq!(ctx[h].selected_key_idx, Some(1));
    assert!(ctx.propagate_event(&release(x, y), h.id()), "the drag's end is the ramp's");
    assert_eq!(ctx[h].selected_key_idx, Some(1), "the key stays selected");
    assert!(!ctx.propagate_event(&release(2000.0, 2000.0), h.id()), "a stray release is not");
}

/// Enter on the focused delete button deletes the selected key then and there, and
/// leaves nothing armed: the next mouse press on the button deletes nothing.
#[test]
fn the_delete_button_deletes_from_the_keyboard() {
    let (mut ctx, h) = ramp_at_natural_height();
    let (x, y) = key_centre(&ctx[h], 1);
    ctx.propagate_event(&press(x, y), h.id());
    ctx.propagate_event(&release(x, y), h.id());
    ctx.lend_h(h, |r, ctx| r.inner_mut().focus_field(3, ctx));
    assert!(ctx.propagate_event(&key_event(NamedKey::Enter), h.id()));
    assert_eq!(ctx[h].keys.len(), 3, "the key is gone");
    assert_eq!(ctx[h].selected_key_idx, None);
    assert!(!ctx[h].del_button.take_click(), "no click left armed for the next press");
}

/// Grabbing a key off-centre keeps the offset: a 3 px move moves it 3 px, not to the
/// pointer.
#[test]
fn a_dragged_key_keeps_where_it_was_grabbed() {
    let (mut ctx, h) = ramp_at_natural_height();
    let (x, y) = key_centre(&ctx[h], 1);
    let before = ctx[h].keys[1].pos;
    let width = ctx[h].plot_rect().width;
    ctx.propagate_event(&press(x + 6.0, y), h.id());
    ctx.propagate_event(&crate::widget::Event::PointerMove { x: x + 9.0, y, local_x: x + 9.0, local_y: y }, h.id());
    let moved = (ctx[h].keys[1].pos - before) * width;
    assert!((moved - 3.0).abs() < 0.01, "moved {moved} px");
}

/// Dragging toward a close neighbour never moves a key back past where it started,
/// moves it toward the neighbour as the pointer goes, and never leaves 0..1.
#[test]
fn a_drag_toward_a_close_neighbour_never_backs_away() {
    let mut ramp = Ramp::new();
    let r = ramp.inner_mut();
    r.keys = vec![RampKey { pos: 0.0, value: 0.0 }, RampKey { pos: 0.5, value: 0.5 }, RampKey { pos: 0.52, value: 0.5 }, RampKey { pos: 1.0, value: 1.0 }];
    let mut last = 0.5;
    for step in 1..=20 {
        let t_raw = 0.5 + step as f32 * 0.005;
        r.keys[1].pos = 0.5;
        let t = r.resisted_pos(1, t_raw, 0.5);
        assert!(t >= last - 1e-6, "{t_raw}: {t} went back from {last}");
        last = t;
    }
    let t = { r.keys[1].pos = 0.5; r.resisted_pos(1, 0.505, 0.5) };
    assert!(t > 0.5 && t < 0.52, "a nudge moves it toward the neighbour, short of it: {t}");
    // Every key, every cursor: a place in 0..1.
    for idx in 0..r.keys.len() {
        for i in 0..=100 {
            let t_raw = i as f32 / 100.0;
            let origin = r.keys[idx].pos;
            let t = r.resisted_pos(idx, t_raw, origin);
            assert!((0.0..=1.0).contains(&t), "key {idx} at {t_raw}: {t}");
        }
    }
}
