//! A composite's parts as items: their values, ranges and focus, and the ramps' role.

use super::*;
use crate::widget::test_composite::Composite;

fn node(update: &TreeUpdate, id: NodeId) -> &Node {
    &update.nodes.iter().find(|(n, _)| *n == id).expect("node in the update").1
}

/// The ramps are groups: their keys and controls are items under them.
#[test]
fn a_ramp_is_a_group() {
    assert_eq!(role_for("Ramp", FocusRole::Well, None), Role::Group);
    assert_eq!(role_for("ColorRamp", FocusRole::Well, None), Role::Group);
}

/// An item carries a value as a widget of its role does — a slider's number with its range,
/// settable; a combo box's choice as text — and the keyboard's node is the item the widget
/// gives its focus to, or the item standing for the field that has it; with a field no item
/// stands for, the composite that holds it.
#[test]
fn items_carry_values_ranges_and_the_keyboard() {
    let mut ctx = UiContext::new();
    let c = ctx.insert(Composite::new());
    ctx[c].set_rect(10.0, 10.0, 200.0, 100.0);
    let id = c.id();

    let update = tree_update(&ctx, "Ramp", 1.0);
    let key = node(&update, item_id(id, 0));
    assert_eq!((key.role(), key.toggled()), (Role::RadioButton, Some(Toggled::False)));
    assert!(!key.supports_action(Action::SetValue), "no range, nothing to set");
    let pos = node(&update, item_id(id, 1));
    assert_eq!((pos.role(), pos.label(), pos.numeric_value()), (Role::Slider, Some("Position"), Some(0.25)));
    assert_eq!((pos.min_numeric_value(), pos.max_numeric_value(), pos.numeric_value_step()), (Some(0.0), Some(1.0), Some(0.02)));
    assert!(pos.supports_action(Action::SetValue) && pos.supports_action(Action::Click));
    let line = node(&update, item_id(id, 2));
    assert_eq!((line.role(), line.value()), (Role::ComboBox, Some("Smooth")));
    assert_eq!(update.focus, WINDOW, "nothing has the keyboard");

    ctx.set_focused_id(id);
    assert_eq!(tree_update(&ctx, "Ramp", 1.0).focus, node_id(id), "the composite, no key selected");
    ctx[c].key_selected = true;
    assert_eq!(tree_update(&ctx, "Ramp", 1.0).focus, item_id(id, 0), "its selected key");
    let (position, line) = (ctx[c].fields[0], ctx[c].fields[1]);
    ctx.claim_field_focus(id, position);
    assert_eq!(tree_update(&ctx, "Ramp", 1.0).focus, item_id(id, 1), "the field's item, not the key's");
    ctx.claim_field_focus(id, line);
    assert_eq!(tree_update(&ctx, "Ramp", 1.0).focus, item_id(id, 2));
    let unlisted = Composite::new().base().id();
    ctx.claim_field_focus(id, unlisted);
    assert_eq!(tree_update(&ctx, "Ramp", 1.0).focus, node_id(id), "a field with no item: its composite");
}
