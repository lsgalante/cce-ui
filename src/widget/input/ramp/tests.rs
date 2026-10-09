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
