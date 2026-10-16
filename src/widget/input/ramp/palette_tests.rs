//! The colour palette: its swatches picked, added, deleted, dragged into order and edited by
//! pointer and keyboard; one Tab stop that walks its fields strip first; what it tells its
//! host; and its parts to a screen reader.

use super::*;

/// A palette of red, green and blue, at the gallery's exhibit width.
fn palette() -> (UiContext, Handle<Adapted<ColorPalette>>) {
    palette_of(vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]])
}

fn palette_of(colors: Vec<[f32; 3]>) -> (UiContext, Handle<Adapted<ColorPalette>>) {
    let mut ctx = UiContext::new();
    let h = ctx.insert(ColorPalette::with_colors(colors));
    ctx.lend_h(h, |p, _| WidgetHost::set_rect(p, 0.0, 0.0, 390.0, 260.0));
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

fn press(ctx: &mut UiContext, h: Handle<Adapted<ColorPalette>>, named: NamedKey) -> bool {
    ctx.propagate_event(&key(named, false), h.id())
}

fn button(state: ElementState, (x, y): (f32, f32)) -> Event {
    Event::MouseButton { button: MouseButton::Left, state, x, y, local_x: x, local_y: y }
}

fn click(ctx: &mut UiContext, h: Handle<Adapted<ColorPalette>>, at: (f32, f32)) {
    ctx.propagate_event(&button(ElementState::Pressed, at), h.id());
    ctx.propagate_event(&button(ElementState::Released, at), h.id());
}

fn tick(ctx: &mut UiContext, h: Handle<Adapted<ColorPalette>>) {
    ctx.lend_h(h, |p, ctx| WidgetHost::tick(p, 0.016, ctx));
}

fn sliders(p: &ColorPalette) -> [f32; 3] {
    [p.r_slider.inner().value(), p.g_slider.inner().value(), p.b_slider.inner().value()]
}

/// A new palette has six hues, none selected, and declares one height whether or not one is
/// selected: the slider rows are reserved.
#[test]
fn a_new_palette_has_six_hues_and_one_height() {
    let p = ColorPalette::new();
    assert_eq!(p.inner().colors().len(), 6);
    assert_eq!(p.inner().selected_color(), None);
    let (mut ctx, h) = palette();
    let before = ctx[h].natural_height();
    let at = ctx[h].slot_centre(1);
    click(&mut ctx, h, at);
    assert_eq!(ctx[h].selected_idx, Some(1));
    assert_eq!(ctx[h].natural_height(), before, "selecting a swatch keeps the height");
    assert_eq!(Layout::intrinsic_size(ctx[h].inner()).map(|s| s.height), Some(before));
}

/// A press on a swatch selects it and shows its colour on the sliders, which then edit it;
/// the host hears of the edit once, and not of the selection.
#[test]
fn a_pressed_swatch_is_edited_by_the_sliders() {
    let (mut ctx, h) = palette();
    let at = ctx[h].slot_centre(1);
    click(&mut ctx, h, at);
    assert_eq!(sliders(&ctx[h]), [0.0, 1.0, 0.0]);
    assert!(!ctx[h].take_change(), "a selection is no change");

    ctx.set_focused_id(h.id());
    press(&mut ctx, h, NamedKey::Tab); // to Red
    assert_eq!(ctx.focused_widget, Some(ctx[h].field_id(ColorPalette::RED)));
    press(&mut ctx, h, NamedKey::ArrowRight);
    let red = ctx[h].swatches[1][0];
    assert!(red > 0.0, "Red moved the swatch's red: {red}");
    assert_eq!(red, ctx[h].r_slider.inner().value());
    assert!(ctx[h].take_change(), "the host hears of the edit");
    assert!(!ctx[h].take_change(), "once");
}

/// A press on the add slot appends a copy of the last swatch and selects it; with many
/// swatches the slots close up so the add slot stays on the strip.
#[test]
fn the_add_slot_appends_a_swatch() {
    let (mut ctx, h) = palette();
    let add = ctx[h].slot_centre(3);
    click(&mut ctx, h, add);
    assert_eq!(ctx[h].swatches.len(), 4);
    assert_eq!(ctx[h].swatches[3], [0.0, 0.0, 1.0], "a copy of the last");
    assert_eq!(ctx[h].selected_idx, Some(3));
    assert!(ctx[h].take_change());

    for _ in 0..40 {
        let n = ctx[h].swatches.len();
        let at = ctx[h].slot_centre(n);
        click(&mut ctx, h, at);
    }
    let strip = ctx[h].strip_rect();
    let n = ctx[h].swatches.len();
    assert_eq!(n, 44, "every press on the add slot added one");
    let (ax, _) = ctx[h].slot_centre(n);
    assert!(ax + ColorPalette::SWATCH_R <= strip.x + strip.width + 0.01, "the add slot is on the strip: {ax}");
}

/// The delete button deletes the selected swatch, and is not shown on the last one left.
#[test]
fn the_last_swatch_cannot_be_deleted() {
    let (mut ctx, h) = palette_of(vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
    let at = ctx[h].slot_centre(0);
    click(&mut ctx, h, at);
    let (x, y, w, bh) = ctx[h].del_button.rect();
    assert!(w > 0.0, "shown with two swatches");
    click(&mut ctx, h, (x + w / 2.0, y + bh / 2.0));
    assert_eq!((ctx[h].swatches.clone(), ctx[h].selected_idx), (vec![[0.0, 1.0, 0.0]], None));

    let at = ctx[h].slot_centre(0);
    click(&mut ctx, h, at);
    assert!(!ctx[h].deletable());
    assert_eq!(ctx[h].del_button.rect().2, 0.0, "parked on the last swatch");
    ctx[h].inner_mut().delete_selected();
    assert_eq!(ctx[h].swatches.len(), 1, "and the call keeps it");
}

/// A swatch held from its press is dragged along the strip, the others closing up behind it.
#[test]
fn a_dragged_swatch_reorders_the_palette() {
    let (mut ctx, h) = palette();
    let (x0, y) = ctx[h].slot_centre(0);
    ctx.propagate_event(&button(ElementState::Pressed, (x0, y)), h.id());
    assert!(ctx[h].is_dragging());
    let (x2, _) = ctx[h].slot_centre(2);
    ctx.propagate_event(&Event::PointerMove { x: x2, y, local_x: x2, local_y: y }, h.id());
    ctx.propagate_event(&button(ElementState::Released, (x2, y)), h.id());
    assert!(!ctx[h].is_dragging());
    assert_eq!(ctx[h].swatches, vec![[0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]]);
    assert_eq!(ctx[h].selected_idx, Some(2), "the selection went with it");
    assert!(ctx[h].take_change());
}

/// On the strip: the arrows select, Shift+arrows move the selected swatch, Insert adds a copy
/// after it, Delete deletes it and selects its neighbour.
#[test]
fn the_strips_keys_pick_move_add_and_delete() {
    let (mut ctx, h) = palette();
    let own = h.id();
    assert_eq!(ctx[h].focus_role(), crate::widget::FocusRole::Well);
    ctx.set_focused_id(own);
    press(&mut ctx, h, NamedKey::ArrowRight);
    assert_eq!(ctx[h].selected_idx, Some(0));
    ctx.propagate_event(&key(NamedKey::ArrowRight, true), own);
    assert_eq!(ctx[h].swatches[1], [1.0, 0.0, 0.0], "Shift+Right moved red one along");
    assert_eq!(ctx[h].selected_idx, Some(1));
    ctx.propagate_event(&key(NamedKey::ArrowLeft, true), own);
    ctx.propagate_event(&key(NamedKey::ArrowLeft, true), own);
    assert_eq!(ctx[h].selected_idx, Some(0), "and stops at the start");

    press(&mut ctx, h, NamedKey::Insert);
    assert_eq!(ctx[h].swatches.len(), 4);
    assert_eq!((ctx[h].selected_idx, ctx[h].swatches[1]), (Some(1), [1.0, 0.0, 0.0]), "a copy after it");

    press(&mut ctx, h, NamedKey::Delete);
    assert_eq!(ctx[h].swatches.len(), 3);
    assert_eq!(ctx[h].selected_idx, Some(1), "its neighbour selected");
    for _ in 0..5 {
        press(&mut ctx, h, NamedKey::Delete);
    }
    assert_eq!(ctx[h].swatches.len(), 1, "one kept");
}

/// One Tab stop: the strip alone with nothing selected; with a swatch selected the sliders
/// and Delete join the walk, and the walk leaves past the last.
#[test]
fn a_palette_walks_its_fields_strip_first() {
    let (mut ctx, h) = palette();
    let own = h.id();
    assert!(press(&mut ctx, h, NamedKey::Tab));
    assert_eq!(ctx.focused_widget, Some(own), "enters on its strip");
    assert_eq!((ctx[h].keeps_tab(false), ctx[h].keeps_tab(true)), (false, false));
    press(&mut ctx, h, NamedKey::ArrowRight);
    for f in ColorPalette::RED..=ColorPalette::DELETE {
        assert!(ctx[h].keeps_tab(false));
        press(&mut ctx, h, NamedKey::Tab);
        assert_eq!(ctx.focused_widget, Some(ctx[h].field_id(f)));
    }
    assert!(!ctx[h].keeps_tab(false), "Delete is the last field");
    tick(&mut ctx, h);
}

/// To a screen reader: a radio button per swatch, the add slot, and while one is selected
/// its sliders and Delete. Clicking the add item adds.
#[test]
fn a_palette_reads_as_its_swatches_and_controls() {
    let (mut ctx, h) = palette();
    let items = Input::a11y_items(ctx[h].inner(), Rect { x: 0.0, y: 0.0, width: 390.0, height: 260.0 });
    assert_eq!(items.len(), 4, "three swatches and the add slot");
    assert_eq!(items[0].label, "Colour 1 of 3: #ff0000");
    assert_eq!(items[3].label, "Add colour");
    assert!(Input::a11y_select_item(ctx[h].inner_mut(), 3));
    assert_eq!((ctx[h].swatches.len(), ctx[h].selected_idx), (4, Some(3)));
    assert_eq!(Input::a11y_items(ctx[h].inner(), Rect { x: 0.0, y: 0.0, width: 390.0, height: 260.0 }).len(), 5 + 4, "four swatches, add, R, G, B, Delete");
}
