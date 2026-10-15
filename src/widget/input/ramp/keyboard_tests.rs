//! The float ramp from the keyboard and to a screen reader: one Tab stop that walks its
//! fields graph first, the graph's keys, the key pad's arrows, and its keys and controls as
//! accessibility items.

use super::*;
use crate::scene::paint::Prim;

/// A ramp registered at its natural height: the Raised curve, keys at 0, 0.2, 0.8 and 1.
fn ramp() -> (UiContext, Handle<Adapted<Ramp>>) {
    let mut ctx = UiContext::new();
    let h = ctx.insert(Ramp::new());
    let natural = ctx[h].natural_height();
    ctx.lend_h(h, |r, _| WidgetHost::set_rect(r, 0.0, 0.0, 300.0, natural));
    (ctx, h)
}

fn key(named: NamedKey, shift: bool) -> Event {
    Event::KeyInput(KeyEvent {
        logical_key: Key::Named(named),
        state: ElementState::Pressed,
        text: None,
        repeat: false,
        ctrl: false,
        shift,
        alt: false,
    })
}

/// Press `named` on the ramp, as the window hands it a key.
fn press(ctx: &mut UiContext, h: Handle<Adapted<Ramp>>, named: NamedKey) -> bool {
    ctx.propagate_event(&key(named, false), h.id())
}

fn tick(ctx: &mut UiContext, h: Handle<Adapted<Ramp>>) {
    ctx.lend_h(h, |r, ctx| WidgetHost::tick(r, 0.016, ctx));
}

fn keys(r: &Ramp) -> Vec<(f32, f32)> {
    r.keys.iter().map(|k| (k.pos, k.value)).collect()
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

/// The ramp is one Tab stop that walks its fields: the graph, Preset, Line, then the key pad
/// while a key is selected and Delete while one is and there are more than two keys. It keeps
/// Tab while it has a field to go that way, so the window's walk leaves it past the last field
/// and, Shift+Tab, before the graph.
#[test]
fn a_ramp_walks_its_fields_graph_first() {
    let (mut ctx, h) = ramp();
    let own = h.id();
    assert_eq!(ctx[h].focus_role(), crate::widget::FocusRole::Well, "a stop");
    assert!(press(&mut ctx, h, NamedKey::Tab), "a Tab the ramp is handed unfocused");
    assert_eq!(ctx.focused_widget, Some(own), "enters it on its graph");
    ctx.clear_focus();
    ctx.set_focused_id(own);
    let (preset, line, pad, del) = (ctx[h].field_id(PRESET), ctx[h].field_id(LINE), ctx[h].field_id(PAD), ctx[h].field_id(DELETE));
    let keeps = |ctx: &UiContext| (ctx[h].keeps_tab(false), ctx[h].keeps_tab(true));
    assert_eq!(keeps(&ctx), (true, false), "on the graph: Tab stays, Shift+Tab leaves");

    // No key selected: graph, Preset, Line.
    press(&mut ctx, h, NamedKey::Tab);
    assert_eq!(ctx.focused_widget, Some(preset));
    assert_eq!(keeps(&ctx), (true, true));
    press(&mut ctx, h, NamedKey::Tab);
    assert_eq!(ctx.focused_widget, Some(line));
    assert_eq!(keeps(&ctx), (false, true), "Line is the last field with no key selected");

    // A key selected, four keys: the key pad and Delete join the walk.
    ctx.propagate_event(&key(NamedKey::Tab, true), own);
    ctx.propagate_event(&key(NamedKey::Tab, true), own);
    assert_eq!(ctx.focused_widget, Some(own), "back on the graph");
    press(&mut ctx, h, NamedKey::ArrowRight);
    assert_eq!(ctx[h].selected_key_idx, Some(0));
    for want in [preset, line, pad, del] {
        press(&mut ctx, h, NamedKey::Tab);
        assert_eq!(ctx.focused_widget, Some(want));
        assert_eq!(ctx.focus_holder(), Some(own), "each held by the ramp");
    }
    assert!(ctx[h].del_button.base().focused);
    assert_eq!(keeps(&ctx), (false, true), "Delete is the last field");

    // Two keys: no Delete — not a stop, not placed, not painted.
    ctx[h].apply_preset(0);
    ctx.set_focused_id(own);
    press(&mut ctx, h, NamedKey::ArrowRight);
    assert_eq!((ctx[h].keys.len(), ctx[h].selected_key_idx), (2, Some(0)));
    for want in [preset, line, pad] {
        press(&mut ctx, h, NamedKey::Tab);
        assert_eq!(ctx.focused_widget, Some(want));
    }
    assert_eq!(keeps(&ctx), (false, true), "the key pad is the last field with two keys");
    press(&mut ctx, h, NamedKey::Tab);
    assert_eq!(ctx.focused_widget, Some(own), "handed Tab anyway, it wraps past where Delete was");
    assert!(ctx[h].del_button.rect().0 < -900.0, "Delete is parked");
    // Whether the ramp paints the button: its own prims, in a run, among the ramp's.
    let paints_delete = |r: &Ramp| {
        let mut pc = crate::scene::paint::PaintCtx::new();
        Paint::paint(r, Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 }, &mut pc);
        let all: Vec<Prim> = pc.finish().items.into_iter().map(|i| i.prim).collect();
        let mut pc = crate::scene::paint::PaintCtx::new();
        r.del_button.paint_self(&UiContext::new(), &mut pc);
        let button: Vec<Prim> = pc.finish().items.into_iter().map(|i| i.prim).collect();
        assert!(!button.is_empty(), "the button paints something where it is");
        all.windows(button.len()).any(|w| w == &button[..])
    };
    // Placed by hand where it would be, it is still not painted.
    ctx[h].del_button.set_rect(100.0, 200.0, 22.0, 22.0);
    assert!(!paints_delete(ctx[h].inner()), "two keys: Delete is not painted");
    ctx[h].keys.insert(1, RampKey { pos: 0.5, value: 0.5 });
    ctx[h].arrange_fields();
    assert!(ctx[h].del_button.rect().0 >= 0.0, "a third key: Delete is placed");
    assert!(paints_delete(ctx[h].inner()), "and painted");

    // A ramp whose controls are collapsed is its graph alone.
    ctx[h].controls_collapsed = true;
    ctx.set_focused_id(own);
    assert_eq!(keeps(&ctx), (false, false));
}

/// On the focused graph the arrows pick a key — Right / Down the next, Left / Up the one
/// before, wrapping, and with none selected Right takes the first and Left the last — and
/// Home / End the ends. Unfocused, the ramp declines them.
#[test]
fn the_graphs_arrows_home_and_end_pick_a_key() {
    let (mut ctx, h) = ramp();
    assert!(!press(&mut ctx, h, NamedKey::ArrowRight), "unfocused: not the ramp's key");
    assert_eq!(ctx[h].selected_key_idx, None);
    ctx.set_focused_id(h.id());
    assert!(press(&mut ctx, h, NamedKey::ArrowLeft));
    assert_eq!(ctx[h].selected_key_idx, Some(3), "none selected: Left takes the last");
    ctx[h].selected_key_idx = None;
    let mut seen = Vec::new();
    for k in [
        NamedKey::ArrowRight,
        NamedKey::ArrowRight,
        NamedKey::ArrowDown,
        NamedKey::ArrowLeft,
        NamedKey::ArrowUp,
        NamedKey::ArrowUp,
        NamedKey::ArrowUp,
        NamedKey::ArrowDown,
        NamedKey::End,
        NamedKey::Home,
    ] {
        assert!(press(&mut ctx, h, k));
        seen.push(ctx[h].selected_key_idx.unwrap());
    }
    assert_eq!(seen, [0, 1, 2, 1, 0, 3, 2, 3, 3, 0]);
    let pad = ctx[h].key_pad.inner();
    assert_eq!((pad.value_x(), pad.value_y()), (0.0, 0.5), "the key pad shows the key picked");
    assert!(ctx[h].key_pad.rect().0 >= 0.0, "and is placed");
}

/// Delete or Backspace on the graph deletes the selected key and selects its neighbour; with
/// two keys left it deletes nothing. Insert adds a key on the curve halfway to the next key
/// (from the last, the one before) and selects it.
#[test]
fn the_graphs_delete_and_insert_edit_the_keys() {
    let (mut ctx, h) = ramp();
    ctx.set_focused_id(h.id());
    assert!(!press(&mut ctx, h, NamedKey::Delete), "nothing selected, nothing to delete");
    press(&mut ctx, h, NamedKey::ArrowRight);
    press(&mut ctx, h, NamedKey::ArrowRight);
    ctx[h].just_changed = false;
    assert!(press(&mut ctx, h, NamedKey::Delete));
    assert_eq!(keys(&ctx[h]), [(0.0, 0.5), (0.8, 1.0), (1.0, 0.5)]);
    assert_eq!(ctx[h].selected_key_idx, Some(1), "the key after it selected");
    assert!(ctx[h].just_changed);
    press(&mut ctx, h, NamedKey::End);
    assert!(press(&mut ctx, h, NamedKey::Backspace));
    assert_eq!(keys(&ctx[h]), [(0.0, 0.5), (0.8, 1.0)]);
    assert_eq!(ctx[h].selected_key_idx, Some(1), "the last gone, the one before it");
    assert!(!press(&mut ctx, h, NamedKey::Delete), "two keys: a ramp keeps them");
    assert_eq!(ctx[h].keys.len(), 2);

    // Insert from the first key: halfway to the second, on the line between them.
    press(&mut ctx, h, NamedKey::Home);
    ctx[h].just_changed = false;
    assert!(press(&mut ctx, h, NamedKey::Insert));
    let r = &ctx[h];
    assert_eq!(r.keys.len(), 3);
    assert_eq!(r.selected_key_idx, Some(1), "the new key selected");
    assert!(close(r.keys[1].pos, 0.4) && close(r.keys[1].value, 0.75), "on the curve: {:?}", keys(r));
    assert!(r.just_changed);
    // From the last key: halfway back to the one before.
    press(&mut ctx, h, NamedKey::End);
    press(&mut ctx, h, NamedKey::Insert);
    let r = &ctx[h];
    assert_eq!(r.selected_key_idx, Some(2));
    assert!(close(r.keys[2].pos, 0.6) && close(r.keys[2].value, 0.875), "{:?}", keys(r));
    assert!(r.keys.windows(2).all(|w| w[0].pos <= w[1].pos), "still in order");

    // On a smooth line the new key sits on the curve, not on the chord between its
    // neighbours: from the middle of three keys unevenly spaced.
    ctx[h].keys = vec![RampKey { pos: 0.0, value: 0.0 }, RampKey { pos: 0.2, value: 0.1 }, RampKey { pos: 1.0, value: 1.0 }];
    ctx[h].line_type_dropdown.selected = 1;
    let on_curve = ctx[h].get_interpolated_value(0.6);
    assert!((on_curve - 0.55).abs() > 0.01, "the curve leaves the chord there: {on_curve}");
    press(&mut ctx, h, NamedKey::Home);
    press(&mut ctx, h, NamedKey::ArrowRight);
    press(&mut ctx, h, NamedKey::Insert);
    let r = &ctx[h];
    assert!(close(r.keys[2].pos, 0.6) && close(r.keys[2].value, on_curve), "{:?}", keys(r));
}

/// A field that goes with the keyboard on it — the key pad as the selection goes, by a pick
/// of the mouse's — hands the keyboard back to the graph at the next tick.
#[test]
fn a_field_that_goes_hands_the_keyboard_to_the_graph() {
    let (mut ctx, h) = ramp();
    let own = h.id();
    ctx.set_focused_id(own);
    press(&mut ctx, h, NamedKey::ArrowRight);
    for _ in 0..3 {
        press(&mut ctx, h, NamedKey::Tab);
    }
    assert_eq!(ctx.focused_widget, Some(ctx[h].field_id(PAD)));
    ctx[h].apply_preset(2);
    tick(&mut ctx, h);
    assert_eq!(ctx.focused_widget, Some(own), "back on the graph");
    assert!(!ctx[h].key_pad.base().focused, "the pad let go");
    assert!(ctx[h].keeps_tab(false) && !ctx[h].keeps_tab(true));
}

/// The key pad's arrows, with the keyboard on it, move the selected key with the pad and
/// mark the ramp changed; a key the pad takes without moving changes nothing.
#[test]
fn the_key_pads_arrows_move_the_selected_key() {
    let (mut ctx, h) = ramp();
    let own = h.id();
    ctx.set_focused_id(own);
    press(&mut ctx, h, NamedKey::ArrowRight);
    press(&mut ctx, h, NamedKey::ArrowRight);
    let pad = ctx[h].field_id(PAD);
    for _ in 0..3 {
        press(&mut ctx, h, NamedKey::Tab);
    }
    assert_eq!(ctx.focused_widget, Some(pad));
    ctx[h].just_changed = false;
    assert!(press(&mut ctx, h, NamedKey::ArrowRight));
    assert!(press(&mut ctx, h, NamedKey::ArrowDown));
    let r = &ctx[h];
    assert!(close(r.keys[1].pos, 0.22) && close(r.keys[1].value, 0.98), "{:?}", keys(r));
    assert!(r.just_changed, "the curve changed");
    ctx[h].just_changed = false;
    assert!(press(&mut ctx, h, NamedKey::PageUp));
    assert!(press(&mut ctx, h, NamedKey::PageUp), "taken at the top");
    assert!(close(ctx[h].keys[1].value, 1.0));
    ctx[h].just_changed = false;
    assert!(press(&mut ctx, h, NamedKey::PageUp));
    assert!(!ctx[h].just_changed, "a move that moves nothing changes nothing");
    // End takes the key past its neighbour at 0.8 (up to the last, at 1.0): it is
    // resettled, and stays the one selected.
    press(&mut ctx, h, NamedKey::End);
    let r = &ctx[h];
    assert_eq!(r.selected_key_idx, Some(2));
    assert!(close(r.keys[2].pos, 1.0) && close(r.keys[2].value, 1.0), "{:?}", keys(r));
}

/// The graph's rim is lit while the graph has the keyboard, under relief and flat; with a
/// control focused, or nothing, it is not.
#[test]
fn a_focused_graph_lights_its_rim() {
    for relief in [true, false] {
        crate::layout::set_control_relief(relief);
        let (mut ctx, h) = ramp();
        let lit = |r: &Ramp| {
            let graph = r.graph_rect();
            let hc = crate::color::highlight_primary_color();
            let inside = |q: &Rect| q.x >= graph.x && q.y >= graph.y && q.x + q.width <= graph.x + graph.width + 1e-3;
            let mut pc = crate::scene::paint::PaintCtx::new();
            Paint::paint(r, Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 }, &mut pc);
            pc.finish().items.into_iter().any(|i| match i.prim {
                Prim::Recess { rect, tint, .. } => inside(&rect) && tint == Some([hc[0], hc[1], hc[2]]),
                Prim::Border { rect, border, .. } => rect == graph && border == crate::color::well_frame_color(false, true),
                _ => false,
            })
        };
        assert!(!lit(ctx[h].inner()), "relief {relief}: unfocused, the plain rim");
        ctx.set_focused_id(h.id());
        assert!(lit(ctx[h].inner()), "relief {relief}: the graph focused, its rim lit");
        ctx.clear_focus();
        assert!(!lit(ctx[h].inner()), "relief {relief}: the keyboard gone, unlit");
        ctx.set_focused_id(h.id());
        press(&mut ctx, h, NamedKey::Tab);
        assert!(!lit(ctx[h].inner()), "relief {relief}: the preset focused, not the graph");
        ctx.clear_focus();
        assert!(!lit(ctx[h].inner()));
        // Given the keyboard with no word to the ramp, it lights at its next tick.
        ctx.claim_focus(h.id());
        tick(&mut ctx, h);
        assert!(lit(ctx[h].inner()), "relief {relief}: the record followed at the tick");
    }
}

/// A press on a dropdown's trigger, which claims the keyboard for the dropdown itself, leaves
/// it held by the ramp: the next Tab walks on from that field.
#[test]
fn a_dropdown_pressed_open_is_a_field_the_ramp_holds() {
    let (mut ctx, h) = ramp();
    // The trigger: the bottom of the dropdown's block, under its label strip.
    let (x, y, w, hgt) = ctx[h].preset_dropdown.rect();
    let (px, py) = (x + w / 2.0, y + hgt - 8.0);
    let at = |state| Event::MouseButton { button: MouseButton::Left, state, x: px, y: py, local_x: px, local_y: py };
    assert!(ctx.propagate_event(&at(ElementState::Pressed), h.id()));
    assert!(ctx[h].preset_dropdown.open);
    assert_eq!(ctx.focused_widget, Some(ctx[h].field_id(PRESET)));
    assert_eq!(ctx.focus_holder(), Some(h.id()), "the ramp holds it");
    assert!(ctx[h].keeps_tab(false) && ctx[h].keeps_tab(true), "between the graph and Line");
}

/// A press that takes or makes a key while one of the ramp's controls has the keyboard gives
/// the graph the keyboard, so the graph's keys act on that key, as they do when the press
/// brings the keyboard from outside the ramp.
#[test]
fn a_press_on_the_graph_gives_it_the_keyboard_from_a_control() {
    let (mut ctx, h) = ramp();
    let own = h.id();
    ctx.set_focused_id(own);
    press(&mut ctx, h, NamedKey::Tab);
    assert_eq!(ctx.focused_widget, Some(ctx[h].field_id(PRESET)));
    let click = |ctx: &mut UiContext, (px, py): (f32, f32)| {
        for state in [ElementState::Pressed, ElementState::Released] {
            let at = Event::MouseButton { button: MouseButton::Left, state, x: px, y: py, local_x: px, local_y: py };
            ctx.propagate_event(&at, own);
        }
    };

    // A key taken, from Preset.
    let peg = ctx[h].key_centre(2);
    click(&mut ctx, peg);
    assert_eq!(ctx[h].selected_key_idx, Some(2));
    assert_eq!(ctx.focused_widget, Some(own), "the graph has the keyboard");
    assert!(!ctx[h].preset_dropdown.base().focused, "Preset let go");
    assert!(press(&mut ctx, h, NamedKey::Delete), "the graph's Delete");
    assert_eq!(ctx[h].keys.len(), 3, "the clicked key went");

    // A key made, from the key pad.
    for _ in 0..3 {
        press(&mut ctx, h, NamedKey::Tab);
    }
    assert_eq!(ctx.focused_widget, Some(ctx[h].field_id(PAD)));
    let g = ctx[h].graph_rect();
    click(&mut ctx, (g.x + g.width / 2.0, g.y + g.height / 2.0));
    assert_eq!(ctx[h].keys.len(), 4, "a key made");
    assert_eq!(ctx.focused_widget, Some(own), "the graph has the keyboard");
    assert!(!ctx[h].key_pad.base().focused, "the key pad let go");
    tick(&mut ctx, h);
    assert_eq!(ctx.focused_widget, Some(own), "and keeps it");
}

/// A hover-scroll coast on key `idx`: its stream stopped, moving right at one plot unit a
/// second.
fn coast(r: &mut Ramp, idx: usize) {
    r.scroll_key_idx = Some(idx);
    r.scroll_vel = (1.0, 0.0);
    r.last_key_scroll = web_time::Instant::now().checked_sub(std::time::Duration::from_millis(100));
}

/// A key the ramp takes and a reader's action stop a hover-scroll coast, as a press does: the
/// coast moves the key it latched by index and selects it, so it must not go on into a
/// neighbour that took a deleted key's place, or take back a selection the keyboard moved.
#[test]
fn a_key_or_a_readers_action_stops_a_coast() {
    crate::motion::force_for_test(true);
    let (mut ctx, h) = ramp();
    ctx.set_focused_id(h.id());
    coast(&mut ctx[h], 1);
    tick(&mut ctx, h);
    assert!(ctx[h].keys[1].pos > 0.2, "left alone, the coast moves its key: {:?}", keys(&ctx[h]));
    assert_eq!(ctx[h].selected_key_idx, Some(1), "and selects it");

    // Delete: the neighbour takes the key's index and stays where it is.
    coast(&mut ctx[h], 1);
    let neighbour = keys(&ctx[h])[2];
    assert!(press(&mut ctx, h, NamedKey::Delete));
    tick(&mut ctx, h);
    assert_eq!(keys(&ctx[h])[1], neighbour, "the neighbour is not moved");
    // An arrow: the key it picks stays picked.
    coast(&mut ctx[h], 1);
    assert!(press(&mut ctx, h, NamedKey::ArrowRight));
    tick(&mut ctx, h);
    assert_eq!(ctx[h].selected_key_idx, Some(2), "the arrow's pick holds");

    // A reader's Click on a key, and its number for the selected key's Value.
    coast(&mut ctx[h], 1);
    assert!(ctx[h].inner_mut().a11y_select_item(0));
    tick(&mut ctx, h);
    assert_eq!(ctx[h].selected_key_idx, Some(0), "the reader's pick holds");
    coast(&mut ctx[h], 0);
    let n = ctx[h].keys.len();
    assert!(ctx[h].inner_mut().a11y_set_item_value(n + 3, 0.25));
    tick(&mut ctx, h);
    assert_eq!(keys(&ctx[h])[0], (0.0, 0.25), "set, and not moved after");
    // A reader's Click on Delete, which deletes at the next tick: the neighbour stays.
    coast(&mut ctx[h], 0);
    let neighbour = keys(&ctx[h])[1];
    assert!(ctx[h].inner_mut().a11y_select_item(n + 4));
    tick(&mut ctx, h);
    assert_eq!(ctx[h].keys.len(), n - 1, "deleted");
    assert_eq!(keys(&ctx[h])[0], neighbour, "the neighbour is not moved");
}

/// The node at `id` in a tree update.
fn node(update: &accesskit::TreeUpdate, id: accesskit::NodeId) -> &accesskit::Node {
    &update.nodes.iter().find(|(n, _)| *n == id).expect("node in the update").1
}

/// To a screen reader the ramp is a group named by its label: a radio button per key, named
/// with its reading and on while selected; Preset and Line as combo boxes with their choice;
/// while a key is selected the key pad's Position and Value as sliders; Delete while it is
/// shown. The controls stay out of the registry.
#[test]
fn a_ramp_is_a_group_of_its_keys_and_controls() {
    use accesskit::{Role, Toggled};
    use crate::a11y::{item_id, node_id, tree_update};
    let mut ctx = UiContext::new();
    let h = ctx.insert(Ramp::new().with_label("Bevel"));
    let natural = ctx[h].natural_height();
    let strip = ctx[h].label_strip();
    ctx.lend_h(h, |r, _| WidgetHost::set_rect(r, 0.0, 0.0, 300.0, natural + strip));
    let own = h.id();
    let update = tree_update(&ctx, "", 1.0);
    let ramp = node(&update, node_id(own));
    assert_eq!((ramp.role(), ramp.label()), (Role::Group, Some("Bevel")));
    assert!(ramp.supports_action(accesskit::Action::Focus));
    let roles = |update: &accesskit::TreeUpdate| -> Vec<(Role, String)> {
        node(update, node_id(own))
            .children()
            .iter()
            .map(|c| node(update, *c))
            .map(|n| (n.role(), n.label().unwrap_or_default().to_string()))
            .collect()
    };
    let rb = |s: &str| (Role::RadioButton, s.to_string());
    let cb = |s: &str| (Role::ComboBox, s.to_string());
    assert_eq!(
        roles(&update),
        [
            rb("Key 1 of 4: position 0.00, value 0.50"),
            rb("Key 2 of 4: position 0.20, value 1.00"),
            rb("Key 3 of 4: position 0.80, value 1.00"),
            rb("Key 4 of 4: position 1.00, value 0.50"),
            cb("Preset"),
            cb("Line"),
        ]
    );
    assert_eq!(node(&update, item_id(own, 1)).toggled(), Some(Toggled::False));
    assert_eq!(node(&update, item_id(own, 4)).value(), Some("Raised"));
    assert_eq!(node(&update, item_id(own, 5)).value(), Some("Linear"));

    ctx.set_focused_id(own);
    press(&mut ctx, h, NamedKey::ArrowRight);
    press(&mut ctx, h, NamedKey::ArrowRight);
    let update = tree_update(&ctx, "", 1.0);
    assert_eq!(
        roles(&update)[4..],
        [cb("Preset"), cb("Line"), (Role::Slider, "Position".into()), (Role::Slider, "Value".into()), (Role::Button, "Delete".into())]
    );
    assert_eq!(node(&update, item_id(own, 1)).toggled(), Some(Toggled::True), "the selected key is on");
    let position = node(&update, item_id(own, 6));
    assert_eq!((position.numeric_value(), position.max_numeric_value()), (Some(0.2), Some(1.0)));
    assert!(position.supports_action(accesskit::Action::SetValue));
    assert_eq!(node(&update, item_id(own, 7)).numeric_value(), Some(1.0));

    // Two keys, one selected: no Delete.
    ctx[h].apply_preset(0);
    press(&mut ctx, h, NamedKey::ArrowRight);
    let update = tree_update(&ctx, "", 1.0);
    assert_eq!(roles(&update).last(), Some(&(Role::Slider, "Value".to_string())));
    for i in PRESET..FIELD_TOTAL {
        assert!(!ctx.tree.is_registered(ctx[h].field_id(i)), "field {i} stays out of the registry");
    }
}

/// The reader's focus is where the keyboard is in the ramp: the selected key's item while the
/// graph has it (the ramp itself with no key selected), and each control's item as Tab
/// reaches it — the key pad's as its Position.
#[test]
fn the_readers_focus_follows_the_keyboard_through_the_ramp() {
    use crate::a11y::{item_id, node_id, tree_update};
    let (mut ctx, h) = ramp();
    let own = h.id();
    let focus = |ctx: &UiContext| tree_update(ctx, "", 1.0).focus;
    ctx.set_focused_id(own);
    assert_eq!(focus(&ctx), node_id(own), "the graph, no key selected: the ramp");
    press(&mut ctx, h, NamedKey::End);
    assert_eq!(focus(&ctx), item_id(own, 3), "the selected key");
    let mut seen = Vec::new();
    for _ in 0..4 {
        press(&mut ctx, h, NamedKey::Tab);
        seen.push(focus(&ctx));
    }
    assert_eq!(seen, [item_id(own, 4), item_id(own, 5), item_id(own, 6), item_id(own, 8)], "Preset, Line, Position, Delete");
}

/// A reader's Click on a key selects it and puts the keyboard on the graph; on a control it
/// gives the control the keyboard and does what Space does there — Preset opens, Delete
/// deletes — at the next tick. Its number on Position or Value moves the selected key.
#[cfg(not(any(target_arch = "wasm32", target_os = "macos")))]
#[test]
fn a_readers_clicks_and_numbers_reach_the_ramp() {
    use crate::backend::a11y_unix::{item_click, set_item_value, Acted};
    let (mut ctx, h) = ramp();
    let own = h.id();
    let other = ctx.insert(crate::widget::Button::new(0.0, 400.0, 80.0, 24.0).with_label("Other"));
    ctx.set_focused_id(other.id());

    assert_eq!(item_click(&mut ctx, own, 1), (Acted::Changed, true));
    tick(&mut ctx, h);
    assert_eq!(ctx[h].selected_key_idx, Some(1));
    assert_eq!(ctx.focused_widget, Some(own), "the keyboard on the graph");

    // Preset (item 4): the keyboard, and open.
    assert_eq!(item_click(&mut ctx, own, 4), (Acted::Changed, false));
    tick(&mut ctx, h);
    assert_eq!(ctx.focused_widget, Some(ctx[h].field_id(PRESET)));
    assert!(ctx[h].preset_dropdown.open, "Space opened it");
    press(&mut ctx, h, NamedKey::Escape);

    // A key again, from a control: back to the graph.
    assert_eq!(item_click(&mut ctx, own, 2), (Acted::Changed, false));
    tick(&mut ctx, h);
    assert_eq!((ctx[h].selected_key_idx, ctx.focused_widget), (Some(2), Some(own)));

    // Value (item 7) set by number; Position (item 6) past the next key resettles it.
    let n = |v| accesskit::ActionData::NumericValue(v);
    assert_eq!(set_item_value(&mut ctx, own, 7, Some(&n(0.25))), Acted::Changed);
    assert!(close(ctx[h].keys[2].value, 0.25));
    assert_eq!(set_item_value(&mut ctx, own, 6, Some(&n(0.1))), Acted::Changed);
    assert_eq!(ctx[h].selected_key_idx, Some(1), "resettled, still selected");
    assert!(close(ctx[h].keys[1].pos, 0.1) && close(ctx[h].keys[1].value, 0.25), "{:?}", keys(&ctx[h]));
    assert_eq!(set_item_value(&mut ctx, own, 4, Some(&n(0.5))), Acted::Nothing, "a combo box takes no number");
    assert_eq!(ctx.focused_widget, Some(own), "numbers leave the keyboard where it is");

    // Delete (item 8): the key goes, its neighbour selected, the keyboard on Delete.
    assert_eq!(item_click(&mut ctx, own, 8), (Acted::Changed, false));
    tick(&mut ctx, h);
    assert_eq!(ctx[h].keys.len(), 3);
    assert_eq!(ctx[h].selected_key_idx, Some(1));
    assert_eq!(ctx.focused_widget, Some(ctx[h].field_id(DELETE)));
    // Down to two keys, Delete is gone and the keyboard back on the graph.
    assert_eq!(item_click(&mut ctx, own, 7), (Acted::Changed, false));
    tick(&mut ctx, h);
    assert_eq!(ctx[h].keys.len(), 2);
    assert_eq!(ctx.focused_widget, Some(own));
    assert_eq!(item_click(&mut ctx, own, 6), (Acted::Nothing, false), "two keys: no item 6");
}
