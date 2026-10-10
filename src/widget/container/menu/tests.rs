use super::*;
use crate::context::UiContext;

fn bar() -> Adapted<MenuBar> {
    MenuBar::new(0.0, 0.0, 400.0, 24.0)
        .with_title("Test")
        .with_item("File", &["New", "Save"])
        .with_item("Edit", &["Undo"])
}

#[test]
fn menu_open_click_and_controller_roundtrip() {
    let mut ctx = UiContext::new();
    let mb = ctx.insert(bar());
    WidgetHost::set_rect(&mut ctx[mb], 0.0, 0.0, 400.0, 24.0);

    // Click the "File" strip button (the strip commits selection on release): the dropdown
    // opens, the bar reports focused (conditional focus), and a popover rect exists.
    let (bx, by, bw, bh) = ctx[mb].menus.item_rect(0);
    assert!(bw > 0.0, "strip laid out");
    assert!(ctx.lend_h(mb, |w, ctx| w.mouse_input(MouseButton::Left, ElementState::Pressed, bx + bw / 2.0, by + bh / 2.0, ctx)).unwrap());
    assert!(ctx.lend_h(mb, |w, ctx| w.mouse_input(MouseButton::Left, ElementState::Released, bx + bw / 2.0, by + bh / 2.0, ctx)).unwrap());
    assert!(MenuController::is_menu_open(&*ctx[mb]), "dropdown open");
    assert!(WidgetHost::focused(&ctx[mb], &ctx), "bar holds focus while open");
    let (dx, dy, _, _) = WidgetHost::popover_rect(&ctx[mb]).expect("dropdown popover");

    // Click the second item ("Save"): menu_click reports (0, 1) and everything closes.
    assert!(ctx.lend_h(mb, |w, ctx| w.mouse_input(MouseButton::Left, ElementState::Pressed, dx + 10.0, dy + DROPDOWN_ITEM_H * 1.5, ctx)).unwrap());
    assert_eq!(MenuController::menu_click(&mut *ctx[mb]), Some((0, 1)));
    assert!(!MenuController::is_menu_open(&*ctx[mb]));
    assert!(!WidgetHost::focused(&ctx[mb], &ctx), "focus released after the click");

    // The PageSelector capability is reached through the concrete adapter too.
    assert!(PageSelector::sidebar_w(&*ctx[mb]) > 0.0);
}

#[test]
fn hidden_menubar_reports_no_menu_and_rejects_hits() {
    let mut ctx = UiContext::new();
    let mb = ctx.insert(bar());
    WidgetHost::set_rect(&mut ctx[mb], 0.0, 0.0, 400.0, 24.0);

    WidgetHost::set_visible(&mut ctx[mb], false);
    assert!(!MenuController::is_menu_bar(&*ctx[mb]), "hidden bar is not a menu bar");
    assert!(!WidgetHost::hit_test(&ctx[mb], 10.0, 10.0, &ctx));
    assert!(MenuController::get_menu_items_at(&*ctx[mb], 10.0, 10.0).is_none());
}
