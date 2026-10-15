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
/// controls folded or not, and its key pegs stay inside the opening.
#[test]
fn at_its_natural_height_the_graph_is_whole_and_holds_its_pegs() {
    let (mut ctx, h) = ramp_at_natural_height();
    assert_eq!(ctx[h].graph_h(), Ramp::GRAPH_H);
    assert!(ctx[h].key_peg_r() <= Ramp::PLOT_INSET, "a key at the plot's edge keeps its peg off the wall");
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

/// A colour ramp registered at the gallery's editor size, with keys at 0, 0.5, 0.6, 1.
fn color_ramp() -> (UiContext, Handle<Adapted<ColorRamp>>) {
    let mut ctx = UiContext::new();
    let mut cr = ColorRamp::new();
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

/// Key `i`'s peg on screen.
fn peg(r: &ColorRamp, i: usize) -> (f32, f32) {
    r.peg_centre(i)
}

/// Dragging a key onto its neighbour's position keeps the dragged key selected: the
/// re-sort found the key again by its position, and took the neighbour.
#[test]
fn a_color_ramp_drag_onto_a_neighbour_keeps_its_key() {
    let (mut ctx, h) = color_ramp();
    let (gx, gy) = peg(&ctx[h], 2);
    let (rx, _) = peg(&ctx[h], 1);
    assert!(ctx.propagate_event(&press(gx, gy), h.id()));
    assert_eq!(ctx[h].selected_key_idx, Some(2));
    ctx.propagate_event(&crate::widget::Event::PointerMove { x: rx, y: gy, local_x: rx, local_y: gy }, h.id());
    let sel = ctx[h].selected_key_idx.expect("still a selection");
    assert_eq!(ctx[h].keys[sel].color, [0.0, 1.0, 0.0], "the green key is still the one selected");
}

/// An arrow key on the focused Red slider moves the selected key's red with it.
#[test]
fn a_color_ramp_slider_moved_from_the_keyboard_moves_the_key() {
    let (mut ctx, h) = color_ramp();
    let (x, y) = peg(&ctx[h], 1);
    ctx.propagate_event(&press(x, y), h.id());
    ctx.propagate_event(&release(x, y), h.id());
    let red = ctx[h].r_slider.base().id();
    ctx.claim_focus(red);
    ctx[h].r_slider.focus();
    ctx[h].just_changed = false;
    assert!(ctx.propagate_event(&key_event(NamedKey::ArrowLeft), h.id()));
    let slider = ctx[h].r_slider.inner().value();
    assert!(slider < 1.0, "the slider moved: {slider}");
    assert_eq!(ctx[h].keys[1].color[0], slider, "and the key's red with it");
    assert!(ctx[h].just_changed);
}

/// Each slider row has its label strip and a whole slider under it, the rows do not
/// overlap, and the delete button is the toolkit's height.
#[test]
fn a_color_ramp_lays_its_sliders_out_whole() {
    let (mut ctx, h) = color_ramp();
    let (x, y) = peg(&ctx[h], 1);
    ctx.propagate_event(&press(x, y), h.id());
    let r = &ctx[h];
    let rows: Vec<_> = [&r.r_slider, &r.g_slider, &r.b_slider].iter().map(|s| s.rect()).collect();
    for (_, _, _, rh) in &rows {
        assert_eq!(*rh, crate::layout::control_label_strip() + crate::layout::slider_height());
    }
    for pair in rows.windows(2) {
        assert!(pair[1].1 >= pair[0].1 + pair[0].3, "rows overlap: {pair:?}");
    }
    assert_eq!(r.del_button.rect().3, crate::layout::button_height());
}

/// The sliders and the delete button are painted once: what a selected key adds to the
/// ramp's paint is exactly what the four fields paint themselves.
#[test]
fn a_color_ramp_paints_its_fields_once() {
    let (mut ctx, h) = color_ramp();
    let count = |ctx: &UiContext| {
        let mut pc = crate::scene::paint::PaintCtx::new();
        let r = &ctx[h];
        Paint::paint(r.inner(), Rect { x: 0.0, y: 0.0, width: 450.0, height: 390.0 }, &mut pc);
        pc.finish().len()
    };
    let bare = count(&ctx);
    let (x, y) = peg(&ctx[h], 1);
    ctx.propagate_event(&press(x, y), h.id());
    // The selected peg wears its ring too: one arc.
    let selected = count(&ctx) - 1;
    let fields: usize = {
        let r = &ctx[h];
        let dummy = UiContext::new();
        let mut pc = crate::scene::paint::PaintCtx::new();
        r.r_slider.paint_self(&dummy, &mut pc);
        r.g_slider.paint_self(&dummy, &mut pc);
        r.b_slider.paint_self(&dummy, &mut pc);
        r.del_button.paint_self(&dummy, &mut pc);
        pc.finish().len()
    };
    assert_eq!(selected - bare, fields);
}

/// A colour ramp's release is its own only when it used it: the end of a key drag is,
/// a stray release with a key selected is not.
#[test]
fn a_color_ramp_release_is_its_own_only_when_it_used_it() {
    let (mut ctx, h) = color_ramp();
    let (x, y) = peg(&ctx[h], 1);
    assert!(ctx.propagate_event(&press(x, y), h.id()));
    assert!(ctx.propagate_event(&release(x, y), h.id()), "the drag's end is the ramp's");
    assert_eq!(ctx[h].selected_key_idx, Some(1), "the key stays selected");
    assert!(!ctx.propagate_event(&release(2000.0, 2000.0), h.id()), "a stray release is not");
}

/// Enter on a colour ramp's focused delete button deletes the selected key then and
/// there, and leaves no click armed for the next press.
#[test]
fn a_color_ramp_deletes_from_the_keyboard() {
    let (mut ctx, h) = color_ramp();
    let (x, y) = peg(&ctx[h], 1);
    ctx.propagate_event(&press(x, y), h.id());
    ctx.propagate_event(&release(x, y), h.id());
    let del = ctx[h].del_button.base().id();
    ctx.claim_focus(del);
    ctx[h].del_button.focus();
    assert!(ctx.propagate_event(&key_event(NamedKey::Enter), h.id()));
    assert_eq!(ctx[h].keys.len(), 3, "the key is gone");
    assert_eq!(ctx[h].selected_key_idx, None);
    assert!(!ctx[h].del_button.take_click(), "no click left armed for the next press");
}

/// Everything `paint` emits, in order.
fn painted<T: Paint>(w: &T) -> Vec<crate::scene::paint::Prim> {
    let mut pc = crate::scene::paint::PaintCtx::new();
    Paint::paint(w, Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 }, &mut pc);
    pc.finish().items.into_iter().map(|i| i.prim).collect()
}

/// The disc plates among `prims` (a `Plate` at the circular corner shape), by centre, with
/// their material.
fn disc_plates(prims: &[crate::scene::paint::Prim]) -> Vec<((f32, f32), crate::scene::Material)> {
    use crate::scene::paint::Prim;
    prims
        .iter()
        .filter_map(|p| match p {
            Prim::Plate { rect, radii, material, shape: Some(s), .. } if *s == 2.0 => {
                assert!((radii.0 - rect.width / 2.0).abs() < 1e-4 && rect.width == rect.height, "a disc: radii are half its side");
                Some(((rect.x + rect.width / 2.0, rect.y + rect.height / 2.0), *material))
            }
            _ => None,
        })
        .collect()
}

fn near(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3
}

/// Under relief each key's peg is a disc plate of frosted accent glass — the selected one
/// tinted harder — and the selected peg is emitted last, on top.
#[test]
fn ramp_pegs_under_relief_are_frosted_disc_plates_the_selected_on_top() {
    crate::layout::set_control_relief(true);
    let (mut ctx, h) = ramp_at_natural_height();
    let pegs = disc_plates(&painted(ctx[h].inner()));
    assert_eq!(pegs.len(), ctx[h].keys.len(), "one disc plate per key");
    for (_, m) in &pegs {
        assert!(m.frost.is_frosted(), "frosted glass");
    }
    let (x, y) = key_centre(&ctx[h], 1);
    ctx.propagate_event(&press(x, y), h.id());
    ctx.propagate_event(&release(x, y), h.id());
    let pegs = disc_plates(&painted(ctx[h].inner()));
    let (last, glass) = pegs.last().copied().unwrap();
    assert!(near(last, key_centre(&ctx[h], 1)), "the selected peg is drawn last");
    assert!(pegs[..pegs.len() - 1].iter().all(|(_, m)| m.tint[3] < glass.tint[3]), "and tinted harder than the rest");
}

/// Flat, a peg is a flat disc, no plate; the selected one is still drawn last, and wears the
/// focus ring.
#[test]
fn ramp_pegs_flat_are_circles_the_selected_ringed_last() {
    use crate::scene::paint::Prim;
    crate::layout::set_control_relief(false);
    let (mut ctx, h) = ramp_at_natural_height();
    let (x, y) = key_centre(&ctx[h], 2);
    ctx.propagate_event(&press(x, y), h.id());
    ctx.propagate_event(&release(x, y), h.id());
    let prims = painted(ctx[h].inner());
    assert!(disc_plates(&prims).is_empty(), "no plates");
    let centres: Vec<(f32, f32)> = (0..ctx[h].keys.len()).map(|i| key_centre(&ctx[h], i)).collect();
    let discs: Vec<(f32, f32)> = prims
        .iter()
        .filter_map(|p| match p {
            Prim::Circle { cx, cy, .. } if centres.iter().any(|&c| near(c, (*cx, *cy))) => Some((*cx, *cy)),
            _ => None,
        })
        .collect();
    assert_eq!(discs.len(), centres.len(), "one flat disc per key");
    assert!(near(*discs.last().unwrap(), centres[2]), "the selected peg is drawn last");
    let ring = prims.iter().position(|p| matches!(p, Prim::Arc { cx, cy, .. } if near((*cx, *cy), centres[2])));
    let last_disc = prims.iter().rposition(|p| matches!(p, Prim::Circle { cx, cy, .. } if near((*cx, *cy), centres[2])));
    assert!(ring.is_some_and(|r| Some(r) > last_disc), "ringed, over its face");
}

/// Where pegs overlap, a press takes the one drawn on top: the later key, or the selected one
/// whatever its index. Nearest-centre took the key under the pointer's nearer centre, which
/// lies under the other's glass.
#[test]
fn a_press_where_ramp_pegs_overlap_takes_the_topmost() {
    let (mut ctx, h) = ramp_at_natural_height();
    ctx[h].keys = vec![
        RampKey { pos: 0.0, value: 0.0 },
        RampKey { pos: 0.5, value: 0.5 },
        RampKey { pos: 0.52, value: 0.5 },
        RampKey { pos: 1.0, value: 1.0 },
    ];
    let (x, y) = key_centre(&ctx[h], 1);
    // Nearer key 1's centre, on key 2's peg as well.
    ctx.propagate_event(&press(x - 2.0, y), h.id());
    ctx.propagate_event(&release(x - 2.0, y), h.id());
    assert_eq!(ctx[h].selected_key_idx, Some(2), "the later key is on top");
    ctx[h].selected_key_idx = Some(1);
    let (x2, _) = key_centre(&ctx[h], 2);
    ctx.propagate_event(&press(x2 + 2.0, y), h.id());
    ctx.propagate_event(&release(x2 + 2.0, y), h.id());
    assert_eq!(ctx[h].selected_key_idx, Some(1), "the selected key is on top");
}

/// A colour ramp declares the height of its track, peg row and all three slider rows, key
/// selected or not; at that height the sliders and the button lie inside it.
#[test]
fn a_color_ramp_declares_its_slider_rows_and_holds_them() {
    let mut ctx = UiContext::new();
    let h = ctx.insert(ColorRamp::new());
    let natural = ctx[h].natural_height();
    let rows = 3.0 * (crate::layout::control_label_strip() + crate::layout::slider_height());
    assert!(natural >= crate::layout::ramp_height() + rows, "the slider rows are reserved: {natural}");
    assert_eq!(Layout::intrinsic_size(ctx[h].inner()).map(|s| s.height), Some(natural));
    ctx.lend_h(h, |r, _| WidgetHost::set_rect(r, 5.0, 7.0, 300.0, natural));
    let (x, y) = peg(&ctx[h], 0);
    ctx.propagate_event(&press(x, y), h.id());
    assert_eq!(ctx[h].selected_key_idx, Some(0));
    assert_eq!(ctx[h].natural_height(), natural, "selecting a key does not change it");
    let r = &ctx[h];
    let rect = (r.base.x, r.base.y, r.base.w, r.base.h);
    for (fx, fy, fw, fh) in [r.r_slider.rect(), r.g_slider.rect(), r.b_slider.rect(), r.del_button.rect()] {
        assert!(
            fx >= rect.0 && fy >= rect.1 && fx + fw <= rect.0 + rect.2 + 1e-3 && fy + fh <= rect.1 + rect.3 + 1e-3,
            "({fx}, {fy}, {fw}, {fh}) lies outside {rect:?}"
        );
    }
}

/// Under relief a colour ramp's pegs are disc plates of the key's own colour, opaque; the
/// selected one is drawn last and ringed outside its face.
#[test]
fn color_ramp_pegs_under_relief_are_opaque_disc_plates_the_selected_on_top() {
    use crate::scene::paint::Prim;
    crate::layout::set_control_relief(true);
    let (mut ctx, h) = color_ramp();
    let (x, y) = peg(&ctx[h], 1);
    ctx.propagate_event(&press(x, y), h.id());
    ctx.propagate_event(&release(x, y), h.id());
    let prims = painted(ctx[h].inner());
    let pegs = disc_plates(&prims);
    assert_eq!(pegs.len(), 4, "one disc plate per key");
    for (centre, m) in &pegs {
        let i = (0..4).find(|&i| near(peg(&ctx[h], i), *centre)).expect("a key's peg");
        let c = ctx[h].keys[i].color;
        assert_eq!(m.tint, [c[0], c[1], c[2], 1.0], "the key's colour, opaque");
        assert!(!m.frost.is_frosted());
    }
    assert!(near(pegs.last().unwrap().0, peg(&ctx[h], 1)), "the selected peg is drawn last");
    let plate = prims.iter().rposition(|p| matches!(p, Prim::Plate { .. })).unwrap();
    assert!(
        matches!(prims[plate + 1], Prim::Arc { radius, .. } if radius > ColorRamp::PEG_R),
        "its ring follows it, outside the face"
    );
}

/// Flat, a colour ramp's pegs are flat discs of the key's colour, no plates, each face drawn
/// over a hairline of the wells' frame colour a pixel wider.
#[test]
fn color_ramp_pegs_flat_are_circles_in_a_hairline() {
    use crate::scene::paint::Prim;
    crate::layout::set_control_relief(false);
    let (ctx, h) = color_ramp();
    let prims = painted(ctx[h].inner());
    assert!(disc_plates(&prims).is_empty(), "no plates");
    let frame = crate::color::well_frame_color(false, false);
    for i in 0..4 {
        let (px, py) = peg(&ctx[h], i);
        let c = ctx[h].keys[i].color;
        let face = prims
            .iter()
            .position(|p| matches!(p, Prim::Circle { cx, cy, color, .. }
                if near((*cx, *cy), (px, py)) && *color == [c[0], c[1], c[2], 1.0]))
            .unwrap_or_else(|| panic!("key {i}'s face"));
        assert!(matches!(prims[face], Prim::Circle { radius, .. } if radius == ColorRamp::PEG_R), "key {i}'s face radius");
        assert!(
            matches!(prims[face - 1], Prim::Circle { cx, cy, radius, color }
                if near((cx, cy), (px, py)) && radius == ColorRamp::PEG_R + 1.0 && color == frame),
            "key {i}'s hairline, under its face"
        );
    }
}

/// Where a colour ramp's pegs overlap, a press takes the one drawn on top: the later key, or
/// the selected one.
#[test]
fn a_press_where_color_ramp_pegs_overlap_takes_the_topmost() {
    let (mut ctx, h) = color_ramp();
    ctx[h].keys[2].pos = 0.505;
    let (x, y) = peg(&ctx[h], 1);
    ctx.propagate_event(&press(x - 1.0, y), h.id());
    ctx.propagate_event(&release(x - 1.0, y), h.id());
    assert_eq!(ctx[h].selected_key_idx, Some(2), "the later key is on top");
    ctx[h].selected_key_idx = Some(1);
    let (x2, _) = peg(&ctx[h], 2);
    ctx.propagate_event(&press(x2 + 1.0, y), h.id());
    ctx.propagate_event(&release(x2 + 1.0, y), h.id());
    assert_eq!(ctx[h].selected_key_idx, Some(1), "the selected key is on top");
}

/// A press on one colour-ramp peg's face, beside a neighbour whose face does not overlap it,
/// takes the peg under it in either style: each peg is hit within the radius it is drawn to,
/// so the neighbour above it in the paint order does not reach past its own face.
#[test]
fn a_press_on_a_color_ramp_peg_beside_its_neighbour_takes_that_peg() {
    for relief in [true, false] {
        crate::layout::set_control_relief(relief);
        let (mut ctx, h) = color_ramp();
        ctx[h].keys[2].pos = 0.54;
        let ((x1, y), (x2, _)) = (peg(&ctx[h], 1), peg(&ctx[h], 2));
        assert!(x2 - x1 > 2.0 * (ColorRamp::PEG_R + 1.0), "the faces do not overlap, hairlines included");
        let x = x1 + ColorRamp::PEG_R - 0.1;
        assert!(x2 - x < ColorRamp::peg_outer_r(), "the press lies within the neighbour's ring radius");
        ctx.propagate_event(&press(x, y), h.id());
        ctx.propagate_event(&release(x, y), h.id());
        assert_eq!(ctx[h].selected_key_idx, Some(1), "relief {relief}: the face under the press");
    }
}

/// Flat, a press on a Ramp peg's face beside a neighbour whose face does not overlap it takes
/// the peg under it: an unselected flat peg is hit at its face, not the ring radius around it.
#[test]
fn a_press_on_a_flat_ramp_peg_beside_its_neighbour_takes_that_peg() {
    crate::layout::set_control_relief(false);
    let (mut ctx, h) = ramp_at_natural_height();
    let (face_r, peg_r) = (ctx[h].peg_drawn_r(0), ctx[h].key_peg_r());
    assert!(face_r + 3.0 <= peg_r, "a flat face sits inside the ring radius");
    let plot = ctx[h].plot_rect();
    let gap = (2.0 * face_r + 1.0) / plot.width;
    ctx[h].keys = vec![
        RampKey { pos: 0.0, value: 0.0 },
        RampKey { pos: 0.4, value: 0.5 },
        RampKey { pos: 0.4 + gap, value: 0.5 },
        RampKey { pos: 1.0, value: 1.0 },
    ];
    let ((x1, y), (x2, _)) = (key_centre(&ctx[h], 1), key_centre(&ctx[h], 2));
    let x = x1 + face_r - 1.0;
    assert!(x2 - x < peg_r, "the press lies within the neighbour's ring radius");
    ctx.propagate_event(&press(x, y), h.id());
    ctx.propagate_event(&release(x, y), h.id());
    assert_eq!(ctx[h].selected_key_idx, Some(1), "the face under the press");
}

/// A labelled ramp lays its graph out below its label: the adapter's block carries the
/// detached label strip above the content, and the ramp arranges in the content.
#[test]
fn a_labelled_ramp_lays_its_graph_below_its_label() {
    let mut ramp = Ramp::new().with_label("Ramp");
    let strip = ramp.label_strip();
    assert!(strip > 0.0, "labelled, the ramp has a strip");
    let natural = ramp.inner().natural_height();
    WidgetHost::set_rect(&mut ramp, 0.0, 0.0, 300.0, natural + strip);
    let r = ramp.inner();
    assert_eq!(r.base.y, strip, "the ramp's own rect starts below the strip");
    assert_eq!(r.base.h, natural, "and is the content's height");
    assert!(r.plot_rect().y >= strip, "the plot sits below the label");
}

/// A labelled colour ramp lays its track below its label, as the Ramp does.
#[test]
fn a_labelled_color_ramp_lays_its_track_below_its_label() {
    let mut cr = ColorRamp::new().with_label("ColorRamp");
    let strip = cr.label_strip();
    assert!(strip > 0.0, "labelled, the colour ramp has a strip");
    let natural = cr.inner().natural_height();
    WidgetHost::set_rect(&mut cr, 0.0, 0.0, 300.0, natural + strip);
    let r = cr.inner();
    assert_eq!(r.base.y, strip, "the colour ramp's own rect starts below the strip");
    assert!(r.track_rect().y >= strip, "the track sits below the label");
}
