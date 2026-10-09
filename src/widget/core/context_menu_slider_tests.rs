use super::context_menu::{ContextMenuState, MenuSlider, PAD, ROW_H, SLIDER_W};
use crate::widget::{ElementState, MouseButton, MouseScrollDelta, Position, WidgetId};

fn menu() -> ContextMenuState {
    let mut m = ContextMenuState::new();
    m.show(100.0, 50.0, vec!["Frame All".into(), "Opacity".into()], 0, WidgetId(7));
    m.set_row_slider(1, MenuSlider { value: 50.0, min: 0.0, max: 100.0, step: 5.0, decimals: 0, suffix: "%" });
    m
}
fn row_mid(m: &ContextMenuState, idx: usize) -> f32 {
    m.row_y(idx) + ROW_H * 0.5
}

/// Twenty rows: 20 * ROW_H + 2 * PAD tall, far more than the boxes below.
fn long_menu() -> ContextMenuState {
    let mut m = ContextMenuState::new();
    let rows: Vec<String> = (0..20).map(|i| format!("Row {i}")).collect();
    m.show(100.0, 50.0, rows, 0, WidgetId(7));
    m
}

/// The keyboard walks a menu: a step skips the header and the
/// separators, stops at both ends rather than wrapping, and scrolls a
/// shortened menu to the row it lands on.
#[test]
fn the_keyboard_steps_the_highlight_over_what_cannot_run() {
    let mut m = ContextMenuState::new();
    let rows = ["Header", "A", "-", "B", "C"].map(String::from).to_vec();
    m.show(100.0, 50.0, rows, 1, WidgetId(7));
    assert_eq!(m.step_hovered(1), Some(1), "the header is skipped");
    assert_eq!(m.step_hovered(1), Some(3), "and the separator");
    assert_eq!(m.step_hovered(1), Some(4));
    assert_eq!(m.step_hovered(1), Some(4), "the last row stays");
    assert_eq!(m.step_hovered(-1), Some(3));
    assert_eq!(m.step_hovered(-1), Some(1));
    assert_eq!(m.step_hovered(-1), Some(1), "the header is not reached");
    m.set_hovered_item(Some(2));
    assert_eq!(m.hovered_item, None, "a separator cannot be highlighted");
    assert_eq!(m.step_hovered(-1), Some(4), "from nothing, up starts at the bottom");

    let mut long = long_menu();
    long.place(100.0, 50.0, 200.0);
    long.set_hovered_item(Some(15));
    assert!(long.row_y(15) >= long.y && long.row_y(15) + ROW_H <= long.y + long.h, "scrolled into view");
    long.set_hovered_item(Some(0));
    assert_eq!(long.scroll, 0.0);
}

/// Placed shorter than its rows, the menu scrolls: the wheel moves the
/// rows a row a notch, up shows the rows above, it stops at both ends,
/// and the row under the pointer — hover, press — is the one DRAWN
/// there, scroll included.
#[test]
fn a_shortened_menu_scrolls_its_rows() {
    let mut m = long_menu();
    let full = m.content_h;
    assert_eq!(full, 20.0 * ROW_H + 2.0 * PAD);
    m.place(100.0, 50.0, 200.0);
    assert_eq!(m.h, 200.0);
    assert_eq!(m.max_scroll(), full - 200.0);

    let (px, py) = (130.0, m.y + PAD + ROW_H * 0.5);
    m.cursor_moved(px, py);
    assert_eq!(m.row_at(px, py), Some(0));
    // Wheel down (negative notches): three rows further on.
    assert!(m.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, -3.0), px, py));
    assert_eq!(m.scroll, 3.0 * ROW_H);
    assert_eq!(m.row_at(px, py), Some(3), "the row under the pointer moved with the scroll");
    assert_eq!(m.hovered_item, Some(3), "and the hover followed it without a motion event");
    assert_eq!(m.row_y(3), m.y + PAD, "row 3 is drawn where row 0 was");
    // Up past the top stops at the top; down past the end stops there.
    m.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, 10.0), px, py);
    assert_eq!(m.scroll, 0.0);
    m.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, -100.0), px, py);
    assert_eq!(m.scroll, m.max_scroll());
    // Nothing to scroll: the wheel is not the menu's.
    let mut short = menu();
    assert!(!short.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, -1.0), 110.0, short.row_y(0) + 1.0));
}

/// A row outside the shown plate is not under the pointer, even though
/// the rows' arithmetic would reach it — the plate ends at `h`.
#[test]
fn rows_below_a_shortened_plate_are_not_hit() {
    let mut m = long_menu();
    m.place(100.0, 50.0, 200.0);
    assert!(m.row_at(130.0, m.y + m.h + 5.0).is_none());
    assert!(!m.hit_test(130.0, m.y + m.h + 5.0));
}

/// The in-window placement, from the anchor: fits below → stays; not
/// below but above → flips to open up from the anchor; neither → slides
/// to the bottom edge; taller than the box → cut to it, scrolling. And
/// the right edge slides the menu left.
#[test]
fn constrain_flips_slides_and_shortens_like_a_positioner() {
    // Fits below.
    let mut m = menu();
    m.constrain_to(0.0, 0.0, 800.0, 600.0);
    assert_eq!((m.x, m.y, m.h), (100.0, 50.0, m.content_h));

    // Opened near the bottom: flips up from the anchor.
    let mut m = ContextMenuState::new();
    m.show(100.0, 580.0, vec!["A".into(), "B".into(), "C".into()], 0, WidgetId(7));
    m.constrain_to(0.0, 0.0, 800.0, 600.0);
    assert_eq!(m.y, 580.0 - m.content_h, "flipped to open upward");

    // No room either way: slides to the bottom edge, whole.
    let mut m = long_menu(); // 496 tall
    m.show(100.0, 300.0, (0..20).map(|i| format!("{i}")).collect(), 0, WidgetId(7));
    m.constrain_to(0.0, 0.0, 800.0, 600.0);
    assert_eq!(m.y + m.h, 600.0);
    assert_eq!(m.h, m.content_h);

    // Taller than the box: cut to it, and it scrolls.
    m.constrain_to(0.0, 0.0, 800.0, 300.0);
    assert_eq!((m.y, m.h), (0.0, 300.0));
    assert!(m.max_scroll() > 0.0);

    // The right edge: slides left to fit.
    let mut m = menu();
    m.show(790.0, 50.0, vec!["A".into()], 0, WidgetId(7));
    m.constrain_to(0.0, 0.0, 800.0, 600.0);
    assert_eq!(m.x + m.w, 800.0);

    // Re-running is stable: it works from the anchor, not from where
    // the last run put it.
    let mut m = long_menu();
    m.constrain_to(0.0, 0.0, 800.0, 300.0);
    let first = (m.x, m.y, m.h);
    m.constrain_to(0.0, 0.0, 800.0, 300.0);
    assert_eq!((m.x, m.y, m.h), first);
}

/// Hosted in the popup, the menu draws nothing into the window's list —
/// every app still calls the in-window paint — while the runner's
/// `paint_hosted` draws it at the origin. Hit testing is untouched.
#[test]
fn a_hosted_menu_paints_only_through_the_popup() {
    use super::context_menu as cm;
    cm::show(100.0, 50.0, vec!["Frame All".into(), "Opacity".into()], 0, WidgetId(7));
    cm::set_hosted(true);
    let mut pc = crate::scene::paint::PaintCtx::new();
    cm::paint_with_labels(&mut pc);
    assert!(pc.finish().items.is_empty(), "nothing in the window");
    assert!(cm::text_labels().is_empty());
    assert!(cm::hit_test(110.0, 60.0), "still hit-tested where it is");

    let mut pc = crate::scene::paint::PaintCtx::new();
    cm::paint_hosted(&mut pc);
    let dl = pc.finish();
    assert!(!dl.items.is_empty(), "the popup draws it");
    let texts: Vec<(f32, f32)> = dl
        .items
        .iter()
        .filter_map(|i| match &i.prim {
            crate::scene::paint::Prim::Text { x, y, .. } => Some((*x, *y)),
            _ => None,
        })
        .collect();
    assert!(texts.iter().all(|&(x, y)| x < 100.0 && y < 50.0 + 2.0 * ROW_H), "at the popup's origin, not the window's");
    cm::set_hosted(false);
    cm::hide();
}

/// Painted through the thread-local, as every host paints it: the slider
/// stamp is dropped while `CONTEXT_MENU` is borrowed, and its drop clears
/// widget references in that same cell. The tests above paint a bare
/// `ContextMenuState` and never held the borrow.
#[test]
fn a_slider_row_paints_through_the_shared_menu() {
    use super::context_menu as cm;
    cm::show(100.0, 50.0, vec!["Frame All".into(), "Opacity".into()], 0, WidgetId(7));
    cm::set_row_slider(1, MenuSlider { value: 50.0, min: 0.0, max: 100.0, step: 5.0, decimals: 0, suffix: "%" });
    let mut pc = crate::scene::paint::PaintCtx::new();
    cm::paint_with_labels(&mut pc);
    cm::paint(&mut pc);
    assert!(cm::is_visible(), "painting leaves the menu up");
    cm::hide();
}

/// A notch over the slider row steps it by `step`, up is more, and the
/// change is reported once; over an action row the wheel is not the
/// menu's. A trackpad's fractions add up to whole steps.
#[test]
fn the_wheel_steps_a_slider_row() {
    let mut m = menu();
    let y = row_mid(&m, 1);
    assert!(m.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, 1.0), 150.0, y));
    assert_eq!(m.slider(1).unwrap().value, 55.0);
    assert_eq!(m.take_slider_change(), Some((1, 55.0)));
    assert_eq!(m.take_slider_change(), None, "reported once");
    m.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, -2.0), 150.0, y);
    assert_eq!(m.slider(1).unwrap().value, 45.0);

    assert!(!m.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, 1.0), 150.0, row_mid(&m, 0)), "an action row does not take the wheel");

    // 30 px is half a notch: nothing yet, then the second half lands a step.
    let half = MouseScrollDelta::PixelDelta(Position { x: 0.0, y: 30.0 });
    assert!(!m.mouse_wheel(&half, 150.0, y));
    assert!(m.mouse_wheel(&half, 150.0, y));
    assert_eq!(m.slider(1).unwrap().value, 50.0);

    // Clamped at the ends, and a clamp that moves nothing reports nothing.
    for _ in 0..30 {
        m.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, 1.0), 150.0, y);
    }
    assert_eq!(m.slider(1).unwrap().value, 100.0);
    m.take_slider_change();
    assert!(!m.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, 1.0), 150.0, y));
    assert_eq!(m.take_slider_change(), None);
}

/// A press on the band jumps to the pointer (snapped to the step) and
/// drags; the menu stays open through it, and the release ends the drag.
/// A press on an action row still fires and closes, as before.
#[test]
fn a_press_on_the_band_drags_and_keeps_the_menu_open() {
    let mut m = menu();
    let y = row_mid(&m, 1);
    let band = m.slider_band(1);
    assert!((band.x + SLIDER_W - (m.x + m.w - PAD)).abs() < 1e-3, "the band ends at the padding");
    assert!(m.mouse_input(MouseButton::Left, ElementState::Pressed, band.x + band.width * 0.8, y, None));
    assert!(m.visible, "a slider row does not close the menu");
    assert_eq!(m.slider(1).unwrap().value, 80.0);
    m.cursor_moved(band.x + band.width * 0.21, y + 200.0);
    assert_eq!(m.slider(1).unwrap().value, 20.0, "the drag follows off the plate, snapped to 5");
    assert!(m.mouse_input(MouseButton::Left, ElementState::Released, 0.0, 0.0, None));
    assert!(m.slider_drag.is_none());
    m.cursor_moved(band.x, y);
    assert_eq!(m.slider(1).unwrap().value, 20.0, "released: motion is hover again");

    // A press on the row's label end: the slider's, nothing moves.
    assert!(m.mouse_input(MouseButton::Left, ElementState::Pressed, m.x + PAD + 2.0, y, None));
    assert!(m.visible);
    assert_eq!(m.slider(1).unwrap().value, 20.0);

    m.mouse_input(MouseButton::Left, ElementState::Pressed, m.x + PAD + 2.0, row_mid(&m, 0), None);
    assert!(!m.visible, "an action row still fires and closes");
}

/// The plate widens for the label, the readout and the band; a fresh
/// `show` clears every slider back to an action row.
#[test]
fn a_slider_row_widens_the_plate_and_show_clears_it() {
    let mut m = ContextMenuState::new();
    m.show(0.0, 0.0, vec!["Opacity".into()], 0, WidgetId(7));
    let narrow = m.w;
    m.set_row_slider(0, MenuSlider { value: 1.0, min: 0.0, max: 100.0, step: 1.0, decimals: 0, suffix: "%" });
    assert!(m.w >= narrow.max(SLIDER_W + 2.0 * PAD));
    let labels = m.text_labels();
    assert!(labels.iter().any(|l| l.text == "1%"), "the readout is a label: {:?}", labels.iter().map(|l| &l.text).collect::<Vec<_>>());
    m.show(0.0, 0.0, vec!["Opacity".into()], 0, WidgetId(7));
    assert!(m.slider(0).is_none());
}
