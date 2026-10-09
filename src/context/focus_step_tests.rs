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
