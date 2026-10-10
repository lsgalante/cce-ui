use super::*;
use crate::widget::UiContext;

/// The label is centred with `x + (w - est_w) / 2.0`, which goes NEGATIVE
/// relative to the button once the label is wider than the button — the
/// text then starts left of the plate and runs out the other side, over
/// whatever is next to it. Buttons are sized by their row, not by their
/// content (`SectionContext::row_layout` divides the width evenly), so a
/// narrow window or a long label reaches this in any app.
fn painted_label(label: &str, w: f32) -> (f32, Option<[f32; 4]>) {
    let b = Button::new(10.0, 20.0, w, 32.0).with_label(label);
    let mut pc = crate::scene::paint::PaintCtx::new();
    let rect = crate::scene::layout::Rect { x: 10.0, y: 20.0, width: w, height: 32.0 };
    <Button as Paint>::paint(&b, rect, &mut pc);
    let dl = pc.finish();
    for item in dl.items.iter() {
        if let crate::scene::paint::Prim::Text { text, x, bounds, .. } = &item.prim {
            if text == label {
                return (*x, *bounds);
            }
        }
    }
    panic!("button drew no label");
}

#[test]
fn button_label_stays_inside_the_button() {
    let (x, bounds) = painted_label("Force Shutdown Immediately", 60.0);
    assert!(x >= 10.0, "label started left of the button plate at x={x}");
    let b = bounds.expect("a button label must be clipped to its plate");
    assert!(b[0] >= 10.0 && b[2] <= 70.0, "label clip {b:?} escapes the button");
}

#[test]
fn a_label_that_fits_is_still_centred() {
    let (x, _) = painted_label("OK", 120.0);
    assert!(x > 10.0 && x < 130.0, "a fitting label must stay centred, got {x}");
}

/// Centred means centred on the glyphs as DRAWN: the gap either side of
/// the shaped label is equal. Measuring in a face the label is not drawn
/// in (first the inked button family, then the UI sans) left labels off
/// centre and, worse, under-measured for `intrinsic_size`.
///
/// The button is sized from the label, so it fits in whatever face this
/// machine shapes it in. At a fixed 160 px it did not everywhere: a font
/// system holding only a colour-emoji face draws every glyph ~15 px wide,
/// "Load Images" came to 164 px, and an overflowing label is left-aligned
/// and clipped by design (`button_label_stays_inside_the_button`).
#[test]
fn a_label_is_centred_on_its_drawn_width() {
    let b = Button::model(ButtonKind::Primary);
    let (_, size) = b.font();
    let font = Paint::widget_font(&b);
    for label in ["Attach...", "Load Images", "Cancel"] {
        let drawn = {
            let mut fs = crate::geometry_font_system().lock().unwrap();
            crate::text::shaped_cluster_offsets(&mut fs, label, size, font.as_deref())
                .last()
                .map(|&(_, t)| t)
                .unwrap()
        };
        let w = drawn + 80.0;
        let (x, _) = painted_label(label, w);
        let (left, right) = (x - 10.0, 10.0 + w - (x + drawn));
        assert!((left - right).abs() < 1.0, "{label:?}: {left:.1}px left vs {right:.1}px right");
    }
}

/// The measure and the frame agree on the font: the paint walk emits the
/// label prim in `widget_font` (the adapter swaps it in for `paint`'s
/// `None`), so `label_width` must shape in exactly that font, and a
/// button sized by `intrinsic_size` then holds its whole label with the
/// 8px inset each side that `paint` clamps to.
#[test]
fn intrinsic_size_holds_the_label_as_the_walk_draws_it() {
    use crate::scene::paint::Prim;
    let mut ctx = UiContext::new();
    let b = ctx.insert(Button::new(0.0, 0.0, 0.0, 0.0).with_label("Load Images"));
    let size = ctx[b].intrinsic_size().unwrap();
    WidgetHost::set_rect(&mut ctx[b], 10.0, 20.0, size.width, size.height);

    let list = crate::widget::painter::paint_tree(&ctx, &ctx[b]);
    let text = list
        .items
        .iter()
        .find_map(|it| match &it.prim {
            Prim::Text { text, x, font_size, font, .. } => Some((text.clone(), *x, *font_size, font.clone())),
            _ => None,
        })
        .expect("the walk emits the label");
    assert_eq!(text.0, "Load Images");
    assert_eq!(text.3, Paint::widget_font(&*ctx[b]), "the walk draws the label in widget_font");

    // Shape it as the renderer will (that font string, that size) and
    // check it ends 8px short of the plate's right edge, as it starts
    // 8px in from the left.
    let drawn = {
        let mut fs = crate::geometry_font_system().lock().unwrap();
        crate::text::shaped_cluster_offsets(&mut fs, &text.0, text.2, text.3.as_deref())
            .last()
            .map(|&(_, t)| t)
            .unwrap()
    };
    let right_gap = (10.0 + size.width) - (text.1 + drawn);
    assert!((text.1 - 18.0).abs() < 0.5, "label starts at the 8px inset, got x={}", text.1);
    assert!((right_gap - 8.0).abs() < 1.0, "label ends {right_gap:.1}px short of the plate, want 8");
}

fn press(x: f32, y: f32) -> Event {
    Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, x, y, local_x: x, local_y: y }
}
fn release(x: f32, y: f32) -> Event {
    Event::MouseButton { button: MouseButton::Left, state: ElementState::Released, x, y, local_x: x, local_y: y }
}

#[test]
fn intrinsic_size_scales_with_label_and_has_button_height() {
    let short = Button::new(0.0, 0.0, 0.0, 0.0).with_label("Hi");
    let long = Button::new(0.0, 0.0, 0.0, 0.0).with_label("A much longer button label");

    let s = short.intrinsic_size().unwrap();
    let l = long.intrinsic_size().unwrap();
    assert!(s.width > 16.0, "includes the horizontal insets");
    assert!(l.width > s.width, "longer label measures wider");
    assert_eq!(s.height, crate::layout::button_height());
}

/// The legacy press/release contract through the real router: press arms, in-rect release
/// clicks (firing the callback), out-of-rect release cancels without clicking.
#[test]
fn press_release_semantics_match_legacy() {
    let mut ctx = UiContext::new();
    let fired = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let fired2 = fired.clone();
    let b = ctx.insert(Button::new(10.0, 10.0, 80.0, 24.0)
        .with_label("Go")
        .on_click(move || { fired2.fetch_add(1, std::sync::atomic::Ordering::SeqCst); }));
    let id = b.id();

    // Press in, release in -> click.
    assert!(ctx.propagate_event(&press(20.0, 20.0), id));
    assert!(ctx.propagate_event(&release(25.0, 20.0), id), "release consumed (was pressed)");
    assert!(ctx[b].take_click());
    assert_eq!(fired.load(std::sync::atomic::Ordering::SeqCst), 1, "callback fired");

    // Press in, release OUT -> cancelled, no click, but release still consumed.
    assert!(ctx.propagate_event(&press(20.0, 20.0), id));
    assert!(ctx.propagate_event(&release(500.0, 500.0), id), "cancelling release consumed");
    assert!(!ctx[b].take_click(), "no click on out-of-rect release");
    assert_eq!(fired.load(std::sync::atomic::Ordering::SeqCst), 1, "callback not re-fired");

    // Release without a press is not consumed.
    assert!(!ctx.propagate_event(&release(20.0, 20.0), id));
}

/// Bridge parity for the default config: bg on the rounded or plain path per the configured
/// radius, and the label through the prim-derived text bridge with center justification.
#[test]
fn geometry_and_label_parity() {
    // Pin the flat style: this test is about the legacy quad-bridge paths,
    // which the config-default raised plate bypasses entirely.
    let b = Button::new(0.0, 0.0, 100.0, 24.0).with_label("Go").with_raised(false);

    let radius = crate::layout::button_corner_radius();
    let rounded = crate::widget::shown_rounded_quads(&b);
    let plain = crate::widget::shown_quads(&b);
    if radius > 0.0 {
        assert!(!rounded.is_empty() && plain.is_empty(), "rounded config -> rounded path only");
        assert_eq!(rounded[0].4, radius);
    } else {
        assert!(rounded.is_empty() && !plain.is_empty(), "square config -> plain path only");
    }

    let labels = b.own_text_labels();
    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0].text, "Go");
    let est = b.label_width("Go");
    assert_eq!(labels[0].x, (100.0 - est) / 2.0, "center-justified");

    // Selection state flows through the WidgetHost forward (list hosts push it).
    let mut b = b;
    b.set_selected(true);
    assert!(b.selected);
}
