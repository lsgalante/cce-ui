use super::*;
use crate::scene::paint::{PaintCtx, Prim};
use crate::widget::{Event, WidgetHost};

/// The focus ring is the plate's own rim lit: focused, its edge (a field
/// that is all run) carries the highlight tint; unfocused, the same edge
/// untinted — no extra geometry. Never a trough.
#[test]
fn focus_lights_the_plate_rim() {
    let mut ctx = crate::widget::UiContext::new();
    let mut b = Button::new(0.0, 0.0, 120.0, 26.0).with_label("Plate").with_raised(true);
    WidgetHost::set_rect(&mut b, 10.0, 20.0, 120.0, 26.0);
    let rect = Rect { x: 10.0, y: 20.0, width: 120.0, height: 26.0 };
    let edges = |b: &Adapted<Button>| -> Vec<Option<[f32; 3]>> {
        let mut pc = PaintCtx::new();
        Paint::paint(b.inner(), rect, &mut pc);
        let items = pc.finish().items;
        assert!(!items.iter().any(|i| matches!(i.prim, Prim::Trough { .. })), "no trough");
        items
            .into_iter()
            .filter_map(|i| match i.prim {
                Prim::Field { rect, split, tint, .. } if split < rect.x - 100.0 => Some(tint),
                _ => None,
            })
            .collect()
    };
    assert_eq!(edges(&b), vec![None], "unfocused: one untinted edge, a field that is all run");
    b.handle_event(&Event::FocusIn, &mut ctx);
    assert_eq!(edges(&b), vec![Some(crate::widget::ControlPlate::focus_tint())], "focused: the rim lit");
    b.handle_event(&Event::FocusOut, &mut ctx);
    assert_eq!(edges(&b), vec![None]);
}
