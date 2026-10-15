use super::*;
use crate::widget::{Button, TextBox, WidgetHost};

/// Tab walks plates and wells in reading order (row, then x), wraps, and
/// Shift+Tab walks back; a focused well opened for typing on the way.
#[test]
fn focus_step_walks_plates_and_wells_in_reading_order() {
    let mut ctx = UiContext::new();
    let mut a = Button::new(0.0, 0.0, 80.0, 24.0).with_label("A");
    let mut b = Button::new(0.0, 0.0, 80.0, 24.0).with_label("B");
    let mut t = TextBox::new("well".to_string());
    // Placed out of registration order: b is right of a on the first row (and a
    // few px lower — a shorter control centred on the row, still the same row), t below.
    WidgetHost::set_rect(&mut b, 100.0, 16.0, 80.0, 12.0);
    WidgetHost::set_rect(&mut a, 10.0, 10.0, 80.0, 24.0);
    WidgetHost::set_rect(&mut t, 10.0, 50.0, 200.0, 24.0);
    let (ib, ia) = (ctx.insert(b).id(), ctx.insert(a).id());
    let t = ctx.insert(t);
    let it = t.id();

    assert!(ctx.focus_step(false));
    assert!(ctx.is_focused_id(ia), "first stop: the top-left plate");
    assert!(ctx.focus_step(false));
    assert!(ctx.is_focused_id(ib), "then the plate to its right");
    assert!(ctx.focus_step(false));
    assert!(ctx.is_focused_id(it), "then the well on the next row");
    assert!(ctx[t].editing, "a well opens for typing when focused");
    assert!(ctx.focus_step(false));
    assert!(ctx.is_focused_id(ia), "wraps to the first stop");
    assert!(ctx.focus_step(true));
    assert!(ctx.is_focused_id(it), "Shift+Tab wraps back to the last");

    // A widget with no role is not a stop.
    let mut sep = crate::widget::Separator::new(0.0, 0.0, 10.0, 1.0, [1.0; 4]);
    WidgetHost::set_rect(&mut sep, 300.0, 10.0, 10.0, 1.0);
    assert_eq!(crate::widget::WidgetHostExt::focus_role(&sep), crate::widget::FocusRole::None);

    // A group's members walk together, where the group's first member falls:
    // grouping a and t (skipping b, which sits between them in reading order)
    // makes the walk a, t, b — and the group chord jumps a -> b -> a.
    let g = ctx.insert(crate::widget::Group::new(vec![ia, it]));
    assert_eq!(ctx.focus_clusters(), vec![vec![ia, it], vec![ib]]);
    ctx.set_focused_id(ia);
    assert!(ctx.focus_step(false));
    assert!(ctx.is_focused_id(it), "the group's second member before the ungrouped stop");
    assert!(ctx.focus_step(false));
    assert!(ctx.is_focused_id(ib));
    assert!(ctx.focus_step_group(false));
    assert!(ctx.is_focused_id(ia), "the group chord wraps to the group's first stop");
    assert!(ctx.focus_step_group(false));
    assert!(ctx.is_focused_id(ib), "then to the next run");
    ctx.remove(g);

    // A plate parked off-screen (the hidden-editor idiom) is not a stop either.
    let mut parked = Button::new(0.0, 0.0, 1.0, 1.0).with_label("parked");
    WidgetHost::set_rect(&mut parked, -1000.0, -1000.0, 1.0, 1.0);
    let pid = ctx.insert(parked).id();
    for _ in 0..4 {
        ctx.focus_step(false);
        assert!(!ctx.is_focused_id(pid), "the parked plate never takes focus");
    }
}

/// A composite's field has the keyboard (`claim_field_focus`): the composite holds it, and
/// the walk steps from the composite, both ways and by runs. The holder before the claim is
/// told it lost the keyboard; leaving the field tells the composite FocusOut, coming back to
/// the composite from its field is FocusIn alone, and removing the composite takes the
/// field's focus with it.
#[test]
fn the_walk_steps_from_the_composite_whose_field_has_the_keyboard() {
    use crate::widget::test_composite::Composite;
    let mut ctx = UiContext::new();
    let mut a = Button::new(0.0, 0.0, 80.0, 24.0).with_label("A");
    WidgetHost::set_rect(&mut a, 10.0, 10.0, 80.0, 24.0);
    let mut c = Composite::new();
    WidgetHost::set_rect(&mut c, 100.0, 10.0, 80.0, 24.0);
    let mut b = Composite::new();
    WidgetHost::set_rect(&mut b, 190.0, 10.0, 80.0, 24.0);
    let (ia, c, b) = (ctx.insert(a).id(), ctx.insert(c), ctx.insert(b));
    let field = ctx[c].fields[0];
    let claim = |ctx: &mut UiContext| {
        ctx.set_focused_id(c.id());
        ctx.claim_field_focus(c.id(), field);
    };

    ctx.set_focused_id(b.id());
    ctx.claim_field_focus(c.id(), field);
    assert_eq!(ctx[b].focus_events, ["in", "out"], "the holder before the claim lost the keyboard");
    assert!(ctx[c].focus_events.is_empty(), "the composite, claiming, is told nothing");
    assert_eq!((ctx.focused_widget, ctx.focus_holder()), (Some(field), Some(c.id())));
    assert!(ctx.focus_step(false));
    assert!(ctx.is_focused_id(b.id()), "Tab from the field: the stop after its composite, not the first");
    assert_eq!(ctx[c].focus_events.first(), Some(&"out"), "the composite is told the keyboard left");

    claim(&mut ctx);
    assert!(ctx.focus_step(true));
    assert!(ctx.is_focused_id(ia), "Shift+Tab: the stop before it");
    claim(&mut ctx);
    assert!(ctx.focus_step_group(false));
    assert!(ctx.is_focused_id(b.id()), "the group chord: the run after it");

    claim(&mut ctx);
    ctx[c].focus_events.clear();
    ctx.set_focused_id(c.id());
    assert_eq!(ctx[c].focus_events, ["in"], "back from its field: no FocusOut first");

    ctx.claim_field_focus(c.id(), field);
    ctx.remove(c);
    assert_eq!(ctx.focused_widget, None, "its field's focus went with it");
}

/// However the keyboard leaves a composite's field — cleared, claimed or focused by another
/// widget, the composite unfocused — the composite is told; and a modal opened over the
/// field gives the keyboard back to the composite when it closes.
#[test]
fn a_composite_hears_every_way_the_keyboard_leaves_its_field() {
    use crate::widget::test_composite::Composite;
    let mut ctx = UiContext::new();
    let (mut c, mut d) = (Composite::new(), Composite::new());
    WidgetHost::set_rect(&mut c, 10.0, 10.0, 80.0, 24.0);
    WidgetHost::set_rect(&mut d, 100.0, 10.0, 80.0, 24.0);
    let (c, d) = (ctx.insert(c), ctx.insert(d));
    let field = ctx[c].fields[0];
    let leaves = |ctx: &mut UiContext, how: &dyn Fn(&mut UiContext), what: &str| {
        ctx.claim_field_focus(c.id(), field);
        ctx[c].focus_events.clear();
        how(ctx);
        assert_eq!(ctx[c].focus_events.first(), Some(&"out"), "{what}");
        assert_ne!(ctx.focused_widget, Some(field), "{what}");
    };
    leaves(&mut ctx, &|ctx| ctx.clear_focus(), "cleared");
    leaves(&mut ctx, &|ctx| ctx.claim_focus(d.id()), "claimed by another");
    leaves(&mut ctx, &|ctx| ctx.focus_id(d.id()), "focused by the app");
    leaves(&mut ctx, &|ctx| ctx.unfocus_id(c.id()), "the composite unfocused");

    ctx.claim_field_focus(c.id(), field);
    ctx.open_modal(d.id(), vec![d.id()]);
    assert!(ctx.is_focused_id(d.id()));
    ctx.close_modal(d.id());
    assert!(ctx.is_focused_id(c.id()), "back to the composite, which puts it where it likes");
}
