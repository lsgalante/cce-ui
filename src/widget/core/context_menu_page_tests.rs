use super::context_menu::{self, PageTurn, PAD, ROW_H};
use crate::widget::{MouseScrollDelta, Position, WidgetId};

fn open() {
    context_menu::show(400.0, 200.0, vec!["Frame".into(), "Style".into(), "Markers".into(), "Exit".into()], 0, WidgetId(1));
    context_menu::set_row_page(1);
    context_menu::set_row_page(2);
}

fn over(idx: usize) -> (f32, f32) {
    (context_menu::x() + 20.0, context_menu::row_y(idx) + ROW_H * 0.5)
}

/// One swipe, a few events long, the fingers lifted after. The runner's
/// phase is left alone — it is one value for the whole process and
/// other tests set it.
fn swipe(dx: f64, x: f32, y: f32) -> bool {
    crate::widget::side_swipe::end_gesture();
    let mut took = false;
    for _ in 0..4 {
        took |= context_menu::mouse_wheel(&MouseScrollDelta::PixelDelta(Position { x: dx / 4.0, y: 0.0 }), x, y);
    }
    took
}

/// A page row is a row that turns the plate: a press on it, or a swipe
/// forward with the pointer on it, asks for its page, and nothing opens
/// on hover alone.
#[test]
fn a_page_row_turns_on_a_press_or_a_swipe() {
    open();
    let (x, y) = over(1);
    context_menu::cursor_moved(x, y);
    assert_eq!(context_menu::take_turn(), None, "hovering turns nothing");
    assert_eq!(context_menu::turn_at(x, y), Some(PageTurn::Into(1)));
    let (ex, ey) = over(3);
    assert_eq!(context_menu::turn_at(ex, ey), None, "an action row is no page");

    assert!(swipe(-80.0, x, y), "the swipe was the menu's");
    assert_eq!(context_menu::take_turn(), Some(PageTurn::Into(1)));
    assert_eq!(context_menu::take_turn(), None, "taken once");
    for _ in 0..4 {
        context_menu::mouse_wheel(&MouseScrollDelta::PixelDelta(Position { x: -20.0, y: 0.0 }), x, y);
    }
    assert_eq!(context_menu::take_turn(), None, "one turn a gesture");
    open();
    assert!(!swipe(-80.0, ex, ey), "forward over an action row turns nothing");
    open();
    assert!(!swipe(80.0, x, y), "back from a menu that was opened goes nowhere");
    assert_eq!(context_menu::take_turn(), None);
    context_menu::hide();
}

/// A turn is animated: the plate grows or shrinks from the size of the
/// one it replaced, a host surface is given room for both meanwhile, and
/// after `TURN_MS` it is the page's own. A plain show does not animate.
#[test]
fn a_page_turn_grows_the_plate_from_the_one_it_replaced() {
    context_menu::show(400.0, 200.0, (0..12).map(|i| format!("Row {i}")).collect(), 0, WidgetId(1));
    assert!(!context_menu::is_turning(), "a menu opened is not a turn");
    let (_, w0, h0) = context_menu::natural_geometry();
    let (mx, my) = (context_menu::x(), context_menu::y());
    context_menu::show_page(mx, my, Some("View"), vec!["One".into()], 0, WidgetId(1));
    assert!(context_menu::is_turning());
    let drawn = context_menu::with_state(|m| m.borrow().drawn_rect());
    let own = context_menu::h();
    assert!(own < h0 && drawn.height > own && drawn.height <= h0, "on its way down: {} between {own} and {h0}", drawn.height);
    let (_, w, h) = context_menu::natural_geometry();
    assert_eq!((w, h), (w0.max(context_menu::w()), h0), "room for both while it turns");

    std::thread::sleep(std::time::Duration::from_millis(context_menu::TURN_MS as u64 + 40));
    assert!(!context_menu::is_turning());
    let (_, w, h) = context_menu::natural_geometry();
    assert_eq!((w, h), (context_menu::w(), own), "its own size once it has landed");

    // A page shown in the moment the menu was put down turns from it too.
    context_menu::hide();
    context_menu::show_page(mx, my, None, (0..12).map(|i| format!("Row {i}")).collect(), 0, WidgetId(1));
    assert!(context_menu::is_turning(), "handed over from the menu just hidden");
    context_menu::hide();
}

/// A turn fades the rows' geometry too, not only their labels: the
/// separators of the plate it leaves and of the page it brings are both
/// drawn, each at part of its strength, and at the end only the page's,
/// whole.
#[test]
fn a_turn_fades_the_separators_of_both_plates() {
    let grooves = || {
        let mut pc = crate::scene::paint::PaintCtx::new();
        context_menu::paint(&mut pc);
        pc.finish()
            .items
            .into_iter()
            .filter_map(|it| match it.prim {
                crate::scene::paint::Prim::Groove { strength, .. } => Some(strength),
                _ => None,
            })
            .collect::<Vec<f32>>()
    };
    context_menu::show(400.0, 200.0, vec!["A".into(), "-".into(), "B".into()], 0, WidgetId(1));
    assert_eq!(grooves(), vec![1.0], "a menu's separator, whole");
    let (mx, my) = (context_menu::x(), context_menu::y());
    context_menu::show_page(mx, my, Some("View"), vec!["C".into(), "-".into(), "D".into(), "E".into()], 0, WidgetId(1));
    // The band's groove is drawn with the page's rows at their strength.
    let mid = grooves();
    assert!(mid.len() >= 2, "both plates' separators while it turns: {mid:?}");
    assert!(mid.iter().all(|s| *s < 1.0), "each faded: {mid:?}");
    std::thread::sleep(std::time::Duration::from_millis(context_menu::turn_ms() as u64 + 40));
    let after = grooves();
    assert!(!after.is_empty() && after.iter().all(|s| *s == 1.0), "the page's alone, whole: {after:?}");
    context_menu::hide();
}

/// A page stands where the menu stood, under a back band that a press or
/// a swipe back turns back from; its rows begin under the band.
#[test]
fn a_page_stands_in_the_menus_place_with_a_way_back() {
    open();
    let (mx, my) = (context_menu::x(), context_menu::y());
    context_menu::show_page(mx, my, Some("View"), vec!["○ Wireframe".into(), "Opacity".into()], 0, WidgetId(1));
    assert!(context_menu::is_turned());
    assert_eq!((context_menu::x(), context_menu::y()), (mx, my));
    assert_eq!(context_menu::back_title().as_deref(), Some("View"));
    assert_eq!(context_menu::row_y(0), my + PAD + ROW_H, "the rows begin under the band");
    assert_eq!(context_menu::h(), 2.0 * ROW_H + ROW_H + 2.0 * PAD);

    let band = (mx + 20.0, my + PAD + ROW_H * 0.5);
    assert_eq!(context_menu::row_at(band.0, band.1), None, "the band is no row");
    assert_eq!(context_menu::turn_at(band.0, band.1), Some(PageTurn::Back));
    let (x, y) = over(1);
    assert_eq!(context_menu::row_at(x, y), Some(1));
    assert!(swipe(80.0, x, y));
    assert_eq!(context_menu::take_turn(), Some(PageTurn::Back), "back from anywhere on the page");

    // A placement that cannot fit it below slides it, never flips it up
    // from the corner it took over.
    context_menu::constrain_to(0.0, 0.0, 2000.0, my + 10.0);
    assert!(context_menu::y() + context_menu::h() <= my + 10.0 + 0.01);
    assert!(context_menu::y() >= 0.0);
    context_menu::hide();
}
