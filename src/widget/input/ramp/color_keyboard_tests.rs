//! The colour ramp from the keyboard and to a screen reader: one Tab stop that walks its
//! fields track first, the track's keys, the sliders writing the selected key's colour, and
//! its keys and controls as accessibility items.

use super::*;
use crate::scene::paint::Prim;

/// A colour ramp at the gallery's editor size: black at 0, red at 0.5, green at 0.6, white
/// at 1.
fn color_ramp() -> (UiContext, Handle<Adapted<ColorRamp>>) {
    color_ramp_of(ColorRamp::new())
}

/// [`color_ramp`] from `cr`, a colour ramp as built.
fn color_ramp_of(mut cr: Adapted<ColorRamp>) -> (UiContext, Handle<Adapted<ColorRamp>>) {
    let mut ctx = UiContext::new();
    cr.keys = vec![
        ColorRampKey { pos: 0.0, color: [0.0, 0.0, 0.0] },
        ColorRampKey { pos: 0.5, color: [1.0, 0.0, 0.0] },
        ColorRampKey { pos: 0.6, color: [0.0, 1.0, 0.0] },
        ColorRampKey { pos: 1.0, color: [1.0, 1.0, 1.0] },
    ];
    let h = ctx.insert(cr);
    ctx.lend_h(h, |r, _| WidgetHost::set_rect(r, 0.0, 0.0, 450.0, 390.0));
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
fn press(ctx: &mut UiContext, h: Handle<Adapted<ColorRamp>>, named: NamedKey) -> bool {
    ctx.propagate_event(&key(named, false), h.id())
}

fn tick(ctx: &mut UiContext, h: Handle<Adapted<ColorRamp>>) {
    ctx.lend_h(h, |r, ctx| WidgetHost::tick(r, 0.016, ctx));
}

fn sliders(r: &ColorRamp) -> [f32; 3] {
    [r.r_slider.inner().value(), r.g_slider.inner().value(), r.b_slider.inner().value()]
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

fn close3(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| close(*x, y))
}

/// The colour ramp is one Tab stop that walks its fields: the track, then the Red, Green and
/// Blue sliders while a key is selected and Delete while one is and there are more than two
/// keys. It keeps Tab while it has a field to go that way, so the window's walk leaves it past
/// the last field and, Shift+Tab, before the track.
#[test]
fn a_color_ramp_walks_its_fields_track_first() {
    let (mut ctx, h) = color_ramp();
    let own = h.id();
    assert_eq!(ctx[h].focus_role(), crate::widget::FocusRole::Well, "a stop");
    assert!(press(&mut ctx, h, NamedKey::Tab), "a Tab the ramp is handed unfocused");
    assert_eq!(ctx.focused_widget, Some(own), "enters it on its track");
    let keeps = |ctx: &UiContext| (ctx[h].keeps_tab(false), ctx[h].keeps_tab(true));
    assert_eq!(keeps(&ctx), (false, false), "no key selected: the track alone");

    // A key selected, four keys: the sliders and Delete join the walk.
    press(&mut ctx, h, NamedKey::ArrowRight);
    assert_eq!(ctx[h].selected_key_idx, Some(0));
    assert_eq!(keeps(&ctx), (true, false), "on the track: Tab stays, Shift+Tab leaves");
    let ids: Vec<WidgetId> = (ColorRamp::RED..=ColorRamp::DELETE).map(|i| ctx[h].field_id(i)).collect();
    for &want in &ids {
        press(&mut ctx, h, NamedKey::Tab);
        assert_eq!(ctx.focused_widget, Some(want));
        assert_eq!(ctx.focus_holder(), Some(own), "each held by the ramp");
    }
    assert!(ctx[h].del_button.base().focused);
    assert!(!ctx[h].b_slider.base().focused, "the slider before it let go");
    assert_eq!(keeps(&ctx), (false, true), "Delete is the last field");
    ctx.propagate_event(&key(NamedKey::Tab, true), own);
    assert_eq!(ctx.focused_widget, Some(ids[2]), "Shift+Tab walks back to Blue");

    // Two keys: no Delete — not a stop, not placed, not painted.
    ctx[h].keys.drain(1..3);
    ctx.set_focused_id(own);
    press(&mut ctx, h, NamedKey::Home);
    assert_eq!((ctx[h].keys.len(), ctx[h].selected_key_idx), (2, Some(0)));
    for &want in &ids[..3] {
        press(&mut ctx, h, NamedKey::Tab);
        assert_eq!(ctx.focused_widget, Some(want));
    }
    assert_eq!(keeps(&ctx), (false, true), "Blue is the last field with two keys");
    press(&mut ctx, h, NamedKey::Tab);
    assert_eq!(ctx.focused_widget, Some(own), "handed Tab anyway, it wraps past where Delete was");
    assert!(ctx[h].del_button.rect().0 < -900.0, "Delete is parked");
    // Whether the ramp paints the button: its own prims, in a run, among the ramp's.
    let paints_delete = |r: &ColorRamp| {
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
    ctx[h].del_button.set_rect(340.0, 120.0, 100.0, 24.0);
    assert!(!paints_delete(ctx[h].inner()), "two keys: Delete is not painted");
    ctx[h].keys.insert(1, ColorRampKey { pos: 0.5, color: [0.5, 0.5, 0.5] });
    ctx[h].arrange_fields();
    assert!(ctx[h].del_button.rect().0 >= 0.0, "a third key: Delete is placed");
    assert!(paints_delete(ctx[h].inner()), "and painted");
}

/// On the focused track the arrows pick a key — Right / Down the next, Left / Up the one
/// before, wrapping, and with none selected Right takes the first and Left the last — and
/// Home / End the ends; the sliders show the key picked. Unfocused, the ramp declines them.
#[test]
fn the_tracks_arrows_home_and_end_pick_a_key() {
    let (mut ctx, h) = color_ramp();
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
    press(&mut ctx, h, NamedKey::ArrowRight);
    assert_eq!(sliders(&ctx[h]), [1.0, 0.0, 0.0], "the sliders show the key picked");
    assert!(ctx[h].r_slider.rect().0 >= 0.0, "and are placed");
}

/// Delete or Backspace on the track deletes the selected key and selects its neighbour; with
/// two keys left it deletes nothing. Insert adds a key of the gradient's colour halfway to
/// the next key (from the last, the one before) and selects it.
#[test]
fn the_tracks_delete_and_insert_edit_the_keys() {
    let (mut ctx, h) = color_ramp();
    ctx.set_focused_id(h.id());
    assert!(!press(&mut ctx, h, NamedKey::Delete), "nothing selected, nothing to delete");

    // Insert from the first key: halfway to red, half red.
    press(&mut ctx, h, NamedKey::Home);
    ctx[h].just_changed = false;
    assert!(press(&mut ctx, h, NamedKey::Insert));
    let r = &ctx[h];
    assert_eq!((r.keys.len(), r.selected_key_idx), (5, Some(1)), "the new key selected");
    assert!(close(r.keys[1].pos, 0.25) && close3(r.keys[1].color, [0.5, 0.0, 0.0]), "{:?}", r.keys[1]);
    assert!(close3(sliders(r), [0.5, 0.0, 0.0]), "the sliders show it");
    assert!(r.just_changed);
    // From the last key: halfway back to green, between green and white.
    press(&mut ctx, h, NamedKey::End);
    press(&mut ctx, h, NamedKey::Insert);
    let r = &ctx[h];
    assert_eq!(r.selected_key_idx, Some(4));
    assert!(close(r.keys[4].pos, 0.8) && close3(r.keys[4].color, [0.5, 1.0, 0.5]), "{:?}", r.keys[4]);
    assert!(r.keys.windows(2).all(|w| w[0].pos <= w[1].pos), "still in order");

    // Delete the key at 0.25: the one after it, red, is selected.
    press(&mut ctx, h, NamedKey::Home);
    press(&mut ctx, h, NamedKey::ArrowRight);
    ctx[h].just_changed = false;
    assert!(press(&mut ctx, h, NamedKey::Delete));
    let r = &ctx[h];
    assert_eq!(r.keys.len(), 5);
    assert_eq!(r.selected_key_idx, Some(1));
    assert_eq!(r.keys[1].color, [1.0, 0.0, 0.0], "red, the key after it");
    assert_eq!(sliders(r), [1.0, 0.0, 0.0]);
    assert!(r.just_changed);
    // The last gone, the one before it; down to two, nothing more goes.
    press(&mut ctx, h, NamedKey::End);
    assert!(press(&mut ctx, h, NamedKey::Backspace));
    assert_eq!(ctx[h].selected_key_idx, Some(3), "the one before the last");
    for _ in 0..2 {
        assert!(press(&mut ctx, h, NamedKey::Delete));
    }
    assert!(!press(&mut ctx, h, NamedKey::Delete), "two keys: a ramp keeps them");
    assert_eq!(ctx[h].keys.len(), 2);
}

/// A slider with the keyboard keeps its own keys and writes the selected key's channel; a
/// key it takes without moving changes nothing.
#[test]
fn a_sliders_keys_write_the_selected_keys_channel() {
    let (mut ctx, h) = color_ramp();
    let own = h.id();
    ctx.set_focused_id(own);
    press(&mut ctx, h, NamedKey::ArrowRight);
    press(&mut ctx, h, NamedKey::ArrowRight);
    press(&mut ctx, h, NamedKey::Tab);
    press(&mut ctx, h, NamedKey::Tab);
    assert_eq!(ctx.focused_widget, Some(ctx[h].field_id(ColorRamp::RED + 1)), "Green");
    ctx[h].just_changed = false;
    assert!(press(&mut ctx, h, NamedKey::End));
    assert_eq!(ctx[h].keys[1].color, [1.0, 1.0, 0.0], "red's green at the top");
    assert!(ctx[h].just_changed);
    ctx[h].just_changed = false;
    assert!(press(&mut ctx, h, NamedKey::End), "taken at the top");
    assert!(!ctx[h].just_changed, "a move that moves nothing changes nothing");
    assert!(!press(&mut ctx, h, NamedKey::Insert), "the track's keys are the track's");
    assert_eq!(ctx[h].keys.len(), 4);
}

/// A field that goes with the keyboard on it — the delete button as a click of the mouse's
/// deletes the key and leaves none selected — hands the keyboard back to the track at the
/// next tick.
#[test]
fn a_field_that_goes_hands_the_keyboard_to_the_track() {
    let (mut ctx, h) = color_ramp();
    let own = h.id();
    ctx.set_focused_id(own);
    press(&mut ctx, h, NamedKey::ArrowRight);
    for _ in 0..4 {
        press(&mut ctx, h, NamedKey::Tab);
    }
    assert_eq!(ctx.focused_widget, Some(ctx[h].field_id(ColorRamp::DELETE)));
    let (x, y, w, bh) = ctx[h].del_button.rect();
    let (px, py) = (x + w / 2.0, y + bh / 2.0);
    let at = |state| Event::MouseButton { button: MouseButton::Left, state, x: px, y: py, local_x: px, local_y: py };
    ctx.propagate_event(&at(ElementState::Pressed), own);
    ctx.propagate_event(&at(ElementState::Released), own);
    assert_eq!((ctx[h].keys.len(), ctx[h].selected_key_idx), (3, None), "the mouse's delete");
    tick(&mut ctx, h);
    assert_eq!(ctx.focused_widget, Some(own), "back on the track");
    assert!(!ctx[h].del_button.base().focused, "the button let go");
    assert!(!ctx[h].keeps_tab(false) && !ctx[h].keeps_tab(true));
}

/// The track's rim is lit while the track has the keyboard, under relief and flat; with a
/// control focused, or nothing, it is not.
#[test]
fn a_focused_track_lights_its_rim() {
    for relief in [true, false] {
        crate::layout::set_control_relief(relief);
        let (mut ctx, h) = color_ramp();
        let lit = |r: &ColorRamp| {
            let track = r.track_rect();
            let hc = crate::color::highlight_primary_color();
            let inside = |q: &Rect| q.x >= track.x && q.y >= track.y && q.x + q.width <= track.x + track.width + 1e-3;
            let mut pc = crate::scene::paint::PaintCtx::new();
            Paint::paint(r, Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 }, &mut pc);
            pc.finish().items.into_iter().any(|i| match i.prim {
                Prim::Recess { rect, tint, .. } => inside(&rect) && tint == Some([hc[0], hc[1], hc[2]]),
                Prim::Border { rect, border, .. } => rect == track && border == crate::color::well_frame_color(false, true),
                _ => false,
            })
        };
        assert!(!lit(ctx[h].inner()), "relief {relief}: unfocused, the plain rim");
        ctx.set_focused_id(h.id());
        assert!(lit(ctx[h].inner()), "relief {relief}: the track focused, its rim lit");
        press(&mut ctx, h, NamedKey::ArrowRight);
        press(&mut ctx, h, NamedKey::Tab);
        assert!(!lit(ctx[h].inner()), "relief {relief}: Red focused, not the track");
        ctx.clear_focus();
        assert!(!lit(ctx[h].inner()), "relief {relief}: the keyboard gone, unlit");
        // Given the keyboard with no word to the ramp, it lights at its next tick.
        ctx.claim_focus(h.id());
        tick(&mut ctx, h);
        assert!(lit(ctx[h].inner()), "relief {relief}: the record followed at the tick");
    }
}

/// A press that takes or makes a key while one of the ramp's controls has the keyboard gives
/// the track the keyboard, so the track's keys act on that key, as they do when the press
/// brings the keyboard from outside the ramp.
#[test]
fn a_press_on_the_track_gives_it_the_keyboard_from_a_control() {
    let (mut ctx, h) = color_ramp();
    let own = h.id();
    ctx.set_focused_id(own);
    press(&mut ctx, h, NamedKey::ArrowRight);
    press(&mut ctx, h, NamedKey::Tab);
    assert_eq!(ctx.focused_widget, Some(ctx[h].field_id(ColorRamp::RED)));
    let click = |ctx: &mut UiContext, (px, py): (f32, f32)| {
        for state in [ElementState::Pressed, ElementState::Released] {
            let at = Event::MouseButton { button: MouseButton::Left, state, x: px, y: py, local_x: px, local_y: py };
            ctx.propagate_event(&at, own);
        }
    };

    // A key's peg, from Red.
    let peg = ctx[h].peg_centre(2);
    click(&mut ctx, peg);
    assert_eq!(ctx[h].selected_key_idx, Some(2));
    assert_eq!(ctx.focused_widget, Some(own), "the track has the keyboard");
    assert!(!ctx[h].r_slider.base().focused, "Red let go");
    assert!(press(&mut ctx, h, NamedKey::Delete), "the track's Delete");
    assert_eq!(ctx[h].keys.len(), 3, "the clicked key went");

    // A key made on the track, from Red.
    press(&mut ctx, h, NamedKey::Tab);
    assert_eq!(ctx.focused_widget, Some(ctx[h].field_id(ColorRamp::RED)));
    let t = ctx[h].track_rect();
    click(&mut ctx, (t.x + t.width / 4.0, t.y + t.height / 2.0));
    assert_eq!(ctx[h].keys.len(), 4, "a key made");
    assert_eq!(ctx.focused_widget, Some(own), "the track has the keyboard");
    tick(&mut ctx, h);
    assert_eq!(ctx.focused_widget, Some(own), "and keeps it");
}

/// A key shown on the sliders stops a slider's wheel glide: left running, the glide went on
/// into the key now shown.
#[test]
fn a_key_shown_on_the_sliders_stops_their_glide() {
    crate::motion::force_for_test(true);
    let (mut ctx, h) = color_ramp();
    let own = h.id();
    ctx[h].keys[1].color = [0.5, 0.5, 0.5];
    ctx.set_focused_id(own);
    press(&mut ctx, h, NamedKey::ArrowRight);
    press(&mut ctx, h, NamedKey::ArrowRight);
    assert_eq!(ctx[h].selected_key_idx, Some(1));
    // A quick run of notches on Blue, then the stream stops.
    let (x, y, w, sh) = ctx[h].b_slider.rect();
    let (px, py) = (x + w / 2.0, y + sh / 2.0);
    let delta = MouseScrollDelta::LineDelta(0.0, 1.0);
    for _ in 0..3 {
        ctx.propagate_event(&Event::MouseWheel { delta, x: px, y: py, local_x: px, local_y: py }, own);
    }
    std::thread::sleep(std::time::Duration::from_millis(80));
    let wheeled = ctx[h].keys[1].color[2];
    assert!(!close(wheeled, 0.5), "the wheel moved Blue");
    tick(&mut ctx, h);
    assert!(!close(ctx[h].keys[1].color[2], wheeled), "left alone, the glide goes on");

    assert!(press(&mut ctx, h, NamedKey::ArrowRight));
    for _ in 0..3 {
        tick(&mut ctx, h);
    }
    assert_eq!(ctx[h].keys[2].color, [0.0, 1.0, 0.0], "the key now shown is not glided");
    assert!(close3(sliders(&ctx[h]), [0.0, 1.0, 0.0]), "and the sliders show it");
}

/// The node at `id` in a tree update.
fn node(update: &accesskit::TreeUpdate, id: accesskit::NodeId) -> &accesskit::Node {
    &update.nodes.iter().find(|(n, _)| *n == id).expect("node in the update").1
}

/// To a screen reader the colour ramp is a group named by its label: a radio button per key,
/// named with its place, position and colour and on while selected; while a key is selected
/// the Red, Green and Blue sliders with their numbers; Delete while it is shown. The controls
/// stay out of the registry.
#[test]
fn a_color_ramp_is_a_group_of_its_keys_and_sliders() {
    use accesskit::{Role, Toggled};
    use crate::a11y::{item_id, node_id, tree_update};
    let (mut ctx, h) = color_ramp_of(ColorRamp::new().with_label("Gradient"));
    let own = h.id();
    let update = tree_update(&ctx, "", 1.0);
    let ramp = node(&update, node_id(own));
    assert_eq!((ramp.role(), ramp.label()), (Role::Group, Some("Gradient")));
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
    let keys = [
        rb("Key 1 of 4: position 0.00, #000000"),
        rb("Key 2 of 4: position 0.50, #ff0000"),
        rb("Key 3 of 4: position 0.60, #00ff00"),
        rb("Key 4 of 4: position 1.00, #ffffff"),
    ];
    assert_eq!(roles(&update), keys, "no key selected: the keys alone");
    assert_eq!(node(&update, item_id(own, 1)).toggled(), Some(Toggled::False));

    ctx.set_focused_id(own);
    press(&mut ctx, h, NamedKey::ArrowRight);
    press(&mut ctx, h, NamedKey::ArrowRight);
    let update = tree_update(&ctx, "", 1.0);
    let slider = |s: &str| (Role::Slider, s.to_string());
    let tr = crate::l10n::tr;
    assert_eq!(
        roles(&update)[4..],
        [slider(&tr("ramp-red")), slider(&tr("ramp-green")), slider(&tr("ramp-blue")), (Role::Button, tr("ramp-delete-key"))]
    );
    assert_eq!(node(&update, item_id(own, 1)).toggled(), Some(Toggled::True), "the selected key is on");
    let red = node(&update, item_id(own, 4));
    assert_eq!((red.numeric_value(), red.min_numeric_value(), red.max_numeric_value()), (Some(1.0), Some(0.0), Some(1.0)));
    assert!(red.supports_action(accesskit::Action::SetValue));
    assert_eq!(node(&update, item_id(own, 5)).numeric_value(), Some(0.0));

    // Two keys, one selected: no Delete.
    ctx[h].keys.drain(1..3);
    press(&mut ctx, h, NamedKey::Home);
    let update = tree_update(&ctx, "", 1.0);
    assert_eq!(roles(&update).last(), Some(&slider("Blue")));
    for i in ColorRamp::RED..=ColorRamp::DELETE {
        assert!(!ctx.tree.is_registered(ctx[h].field_id(i)), "field {i} stays out of the registry");
    }
}

/// The colour ramp's controls are named from the toolkit's catalogue: the sliders' labels
/// are its Red, Green and Blue, and the delete button's item is its Delete whether the button
/// wears the x icon (and so has no label) or falls back to the label.
#[test]
fn a_color_ramps_controls_are_named_from_the_catalogue() {
    use crate::l10n::tr;
    let (mut ctx, h) = color_ramp();
    let r = &ctx[h];
    let labels = [&r.r_slider, &r.g_slider, &r.b_slider].map(|s| s.label());
    assert_eq!(labels, [Some(tr("ramp-red")), Some(tr("ramp-green")), Some(tr("ramp-blue"))]);
    for icon in [true, false] {
        // An icon face, as `with_icon_name` gives where the icon set is installed: no label.
        let button = Button::new(0.0, 0.0, 0.0, 0.0);
        ctx[h].del_button = if icon { button.with_icon(1, 32.0, 32.0) } else { button.with_label(&tr("ramp-delete-key")) };
        ctx.set_focused_id(h.id());
        press(&mut ctx, h, NamedKey::Home);
        let items = Input::a11y_items(ctx[h].inner(), Rect { x: 0.0, y: 0.0, width: 450.0, height: 390.0 });
        let delete = items.last().expect("Delete's item");
        assert_eq!((delete.role, delete.label.as_str()), (accesskit::Role::Button, tr("ramp-delete-key").as_str()), "icon {icon}");
    }
}

/// The reader's focus is where the keyboard is in the colour ramp: the selected key's item
/// while the track has it (the ramp itself with no key selected), and each control's item as
/// Tab reaches it.
#[test]
fn the_readers_focus_follows_the_keyboard_through_the_color_ramp() {
    use crate::a11y::{item_id, node_id, tree_update};
    let (mut ctx, h) = color_ramp();
    let own = h.id();
    let focus = |ctx: &UiContext| tree_update(ctx, "", 1.0).focus;
    ctx.set_focused_id(own);
    assert_eq!(focus(&ctx), node_id(own), "the track, no key selected: the ramp");
    press(&mut ctx, h, NamedKey::End);
    assert_eq!(focus(&ctx), item_id(own, 3), "the selected key");
    let mut seen = Vec::new();
    for _ in 0..4 {
        press(&mut ctx, h, NamedKey::Tab);
        seen.push(focus(&ctx));
    }
    assert_eq!(seen, [item_id(own, 4), item_id(own, 5), item_id(own, 6), item_id(own, 7)], "Red, Green, Blue, Delete");
}

/// A reader's Click on a key selects it and puts the keyboard on the track; on a slider it
/// gives the slider the keyboard, and on Delete presses it — at the next tick. Its number on
/// Red, Green or Blue moves the selected key's channel.
#[cfg(not(any(target_arch = "wasm32", target_os = "macos")))]
#[test]
fn a_readers_clicks_and_numbers_reach_the_color_ramp() {
    use crate::backend::a11y_unix::{item_click, set_item_value, Acted};
    let (mut ctx, h) = color_ramp();
    let own = h.id();
    let other = ctx.insert(crate::widget::Button::new(0.0, 420.0, 80.0, 24.0).with_label("Other"));
    ctx.set_focused_id(other.id());

    assert_eq!(item_click(&mut ctx, own, 1), (Acted::Changed, true));
    tick(&mut ctx, h);
    assert_eq!(ctx[h].selected_key_idx, Some(1));
    assert_eq!(sliders(&ctx[h]), [1.0, 0.0, 0.0], "the sliders show it");
    assert_eq!(ctx.focused_widget, Some(own), "the keyboard on the track");

    // Green (item 5): the keyboard, nothing pressed.
    assert_eq!(item_click(&mut ctx, own, 5), (Acted::Changed, false));
    tick(&mut ctx, h);
    assert_eq!(ctx.focused_widget, Some(ctx[h].field_id(ColorRamp::RED + 1)));
    assert!(ctx[h].g_slider.base().focused);
    assert_eq!(ctx[h].keys[1].color, [1.0, 0.0, 0.0], "unchanged");

    // A key again, from a control: back to the track.
    assert_eq!(item_click(&mut ctx, own, 2), (Acted::Changed, false));
    tick(&mut ctx, h);
    assert_eq!((ctx[h].selected_key_idx, ctx.focused_widget), (Some(2), Some(own)));

    // Red (item 4) and Blue (item 6) set by number.
    let n = |v| accesskit::ActionData::NumericValue(v);
    assert_eq!(set_item_value(&mut ctx, own, 4, Some(&n(0.25))), Acted::Changed);
    assert_eq!(set_item_value(&mut ctx, own, 6, Some(&n(2.0))), Acted::Changed, "clamped to the slider's range");
    assert!(close3(ctx[h].keys[2].color, [0.25, 1.0, 1.0]), "{:?}", ctx[h].keys[2]);
    assert!(close(ctx[h].r_slider.inner().value(), 0.25), "the slider with it");
    assert_eq!(set_item_value(&mut ctx, own, 7, Some(&n(0.5))), Acted::Nothing, "a button takes no number");
    assert_eq!(set_item_value(&mut ctx, own, 1, Some(&n(0.5))), Acted::Nothing, "nor a key");
    assert_eq!(ctx.focused_widget, Some(own), "numbers leave the keyboard where it is");

    // Delete (item 7): the key goes, its neighbour selected, the keyboard on Delete.
    assert_eq!(item_click(&mut ctx, own, 7), (Acted::Changed, false));
    tick(&mut ctx, h);
    assert_eq!(ctx[h].keys.len(), 3);
    assert_eq!(ctx[h].selected_key_idx, Some(2));
    assert_eq!(ctx.focused_widget, Some(ctx[h].field_id(ColorRamp::DELETE)));
    // Down to two keys, Delete is gone and the keyboard back on the track.
    assert_eq!(item_click(&mut ctx, own, 6), (Acted::Changed, false));
    tick(&mut ctx, h);
    assert_eq!(ctx[h].keys.len(), 2);
    assert_eq!(ctx.focused_widget, Some(own));
    assert_eq!(item_click(&mut ctx, own, 5), (Acted::Nothing, false), "two keys: no item 5");
}
