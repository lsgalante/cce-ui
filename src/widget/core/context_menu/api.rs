//! The menu's free-function API: what apps and the runner call (`show`, `hide`, `row_at`,
//! `mouse_input`, `paint`, …), each acting on the current window's menu (`with_state`).

use super::*;

/// Run `f` on the current window's menu (`crate::window_state`): the one menu every
/// function here acts on. It was a thread-local, `CONTEXT_MENU`, until 2026-10-08;
/// `with_state(..)` callers read it through this now.
pub fn with_state<R>(f: impl FnOnce(&RefCell<ContextMenuState>) -> R) -> R {
    crate::window_state::with(|w| f(&w.context_menu))
}

/// Make row `idx` of the shown menu lead to a page — see [`PageTurn`].
/// Call after [`show`] / [`show_page`], which clear every row back to an
/// action.
/// Give the open menu's rows their actions — see [`ContextMenuState::set_row_actions`].
pub fn set_row_actions(actions: Vec<Option<crate::widget::ContextAction>>) {
    with_state(|m| m.borrow_mut().set_row_actions(actions));
}

/// The action row `idx` of the open menu runs, if it was given one.
pub fn row_action(idx: usize) -> Option<crate::widget::ContextAction> {
    with_state(|m| m.borrow().actions.get(idx).copied().flatten())
}

/// How many of the open menu's first rows are headers (a title, `File:` / `Key:`).
pub fn header_count() -> usize {
    with_state(|m| m.borrow().header_count)
}

pub fn set_row_page(idx: usize) {
    with_state(|m| m.borrow_mut().set_row_page(idx));
}

pub fn leads_to_page(idx: usize) -> bool {
    with_state(|m| m.borrow().leads_to_page(idx))
}

/// See [`ContextMenuState::show_page`].
pub fn show_page(x: f32, y: f32, back: Option<&str>, options: Vec<String>, header_count: usize, target: WidgetId) {
    with_state(|m| m.borrow_mut().show_page(x, y, back, options, header_count, target));
}

/// See [`ContextMenuState::refill`].
pub fn refill(options: Vec<String>, sliders: &[Option<MenuSlider>]) -> bool {
    with_state(|m| m.borrow_mut().refill(options, sliders))
}

/// See [`ContextMenuState::turn_at`].
pub fn turn_at(px: f32, py: f32) -> Option<PageTurn> {
    with_state(|m| m.borrow().turn_at(px, py))
}

/// See [`ContextMenuState::take_turn`].
pub fn take_turn() -> Option<PageTurn> {
    with_state(|m| m.borrow_mut().take_turn())
}

/// Whether the shown menu is a page turned to in place of another plate.
pub fn is_turned() -> bool {
    with_state(|m| m.borrow().turned)
}

/// The title of the plate the shown page goes back to.
pub fn back_title() -> Option<String> {
    with_state(|m| m.borrow().back.clone())
}

pub fn is_visible() -> bool {
    with_state(|m| m.borrow().visible)
}

pub fn show(x: f32, y: f32, options: Vec<String>, header_count: usize, target: WidgetId) {
    with_state(|m| m.borrow_mut().show(x, y, options, header_count, target));
}

pub fn hide() {
    with_state(|m| m.borrow_mut().hide());
}

pub fn clear_if_matches(w: &dyn WidgetHost) {
    let id = w.base().id();
    with_state(|m| {
        // Already borrowed means the widget is being dropped from INSIDE
        // the menu's own code — the slider rows' paint stamp, dropped at
        // the end of `paint` under `paint_with_labels`' borrow. A widget
        // the menu made for itself cannot be its target, so there is
        // nothing to clear; `borrow_mut` here panicked on every paint of
        // a menu with a slider row.
        let Ok(mut menu) = m.try_borrow_mut() else { return };
        if menu.target == Some(id) {
            menu.hide();
        }
    });
}

/// Whether the runner draws the menu in its own popup surface — see
/// [`ContextMenuState::hosted`]. Set by the runner, never by an app.
pub fn set_hosted(hosted: bool) {
    with_state(|m| m.borrow_mut().hosted = hosted);
}

pub fn is_hosted() -> bool {
    with_state(|m| m.borrow().hosted)
}

/// See [`ContextMenuState::generation`].
pub fn generation() -> u64 {
    with_state(|m| m.borrow().generation)
}

/// See [`ContextMenuState::place`].
pub fn place(x: f32, y: f32, max_h: f32) {
    with_state(|m| m.borrow_mut().place(x, y, max_h));
}

/// See [`ContextMenuState::constrain_to`].
pub fn constrain_to(bx: f32, by: f32, bw: f32, bh: f32) {
    with_state(|m| m.borrow_mut().constrain_to(bx, by, bw, bh));
}

/// `(anchor, w, content_h)` — what a popup positioner is built from.
pub fn natural_geometry() -> ((f32, f32), f32, f32) {
    with_state(|m| {
        let m = m.borrow();
        let (w, h) = m.surface_size();
        (m.anchor, w, h)
    })
}

/// Whether a page turn is being animated: a host drawing the menu
/// itself asks for frames while it is.
pub fn is_turning() -> bool {
    with_state(|m| m.borrow().turn_progress().is_some())
}

/// See [`ContextMenuState::turn_from_size`].
pub fn turn_from_size(w: f32, h: f32, forward: bool) {
    with_state(|m| m.borrow_mut().turn_from_size(w, h, forward));
}

/// Paint the menu with its top-left at the origin, whether or not it is
/// [hosted](set_hosted) — the runner's popup surface draws it this way.
/// Paints a COPY, so no borrow of the menu is held while the paint runs
/// (the slider stamp's drop reaches back into this cell).
pub fn paint_hosted(ctx: &mut crate::scene::paint::PaintCtx) {
    let mut menu = with_state(|m| m.borrow().clone());
    menu.hosted = false;
    menu.in_popup = true;
    let (x, y) = (menu.x, menu.y);
    ctx.translate(-x, -y, |ctx| menu.paint_with_labels(ctx));
}

pub fn x() -> f32 { with_state(|m| m.borrow().x) }

pub fn y() -> f32 { with_state(|m| m.borrow().y) }

pub fn w() -> f32 { with_state(|m| m.borrow().w) }

pub fn h() -> f32 { with_state(|m| m.borrow().h) }

pub fn hovered_item() -> Option<usize> { with_state(|m| m.borrow().hovered_item) }

pub fn options() -> Vec<String> { with_state(|m| m.borrow().options.clone()) }

/// Highlight a row from the keyboard — see
/// [`ContextMenuState::set_hovered_item`]. A host that walks a menu
/// with the arrow keys (a dropdown) sets the open row with this and
/// moves with [`step_hovered`], and runs [`hovered_item`] on Enter.
pub fn set_hovered_item(idx: Option<usize>) {
    with_state(|m| m.borrow_mut().set_hovered_item(idx));
}

/// See [`ContextMenuState::step_hovered`].
pub fn step_hovered(dir: i32) -> Option<usize> {
    with_state(|m| m.borrow_mut().step_hovered(dir))
}

/// The row under a point, PAD-aware — the ONE row hit test. Every host
/// that dispatches the menu itself should ask this rather than divide
/// `(py - y()) / ROW_H`: the rows start `PAD` below the plate's top, so
/// that division names the row below over the bottom third of every
/// row, and runs off the end on the last one (2026-09-22 audit: six
/// call sites across five apps had it).
pub fn row_at(px: f32, py: f32) -> Option<usize> {
    with_state(|m| m.borrow().row_at(px, py))
}

/// A row's top, PAD-aware — for a host painting the rows itself.
pub fn row_y(idx: usize) -> f32 {
    with_state(|m| m.borrow().row_y(idx))
}

pub fn hit_test(px: f32, py: f32) -> bool {
    with_state(|m| m.borrow().hit_test(px, py))
}

pub fn cursor_moved(px: f32, py: f32) -> bool {
    with_state(|m| m.borrow_mut().cursor_moved(px, py))
}

/// Make row `idx` of the shown menu a slider — see [`MenuSlider`]. Call
/// after [`show`], which clears every row back to an action.
pub fn set_row_slider(idx: usize, slider: MenuSlider) {
    with_state(|m| m.borrow_mut().set_row_slider(idx, slider));
}

pub fn slider(idx: usize) -> Option<MenuSlider> {
    with_state(|m| m.borrow().slider(idx))
}

/// The wheel, for hosts that route it: steps the slider under the
/// pointer. `false` when no slider row is there — let it scroll the page.
pub fn mouse_wheel(delta: &MouseScrollDelta, px: f32, py: f32) -> bool {
    with_state(|m| m.borrow_mut().mouse_wheel(delta, px, py))
}

/// A left press, for hosts that dispatch the menu themselves: `true` when
/// it landed on a slider row, which the host must then NOT treat as an
/// action or a dismissal.
pub fn slider_press(px: f32, py: f32) -> bool {
    with_state(|m| m.borrow_mut().slider_press(px, py))
}

pub fn slider_dragging() -> bool {
    with_state(|m| m.borrow().slider_drag.is_some())
}

pub fn slider_release() -> bool {
    with_state(|m| m.borrow_mut().slider_release())
}

/// `(row, value)` of the last slider change since the last call.
pub fn take_slider_change() -> Option<(usize, f32)> {
    with_state(|m| m.borrow_mut().take_slider_change())
}

pub fn mouse_input(button: MouseButton, state: ElementState, px: f32, py: f32, ctx: Option<&mut crate::context::UiContext>) -> bool {
    with_state(|m| m.borrow_mut().mouse_input(button, state, px, py, ctx))
}

/// Paint the menu as a lit plate — see [`ContextMenuState::paint`]. Hosts on
/// the display-list path call this in place of the [`extra_quads`] loop.
pub fn paint(ctx: &mut crate::scene::paint::PaintCtx) {
    with_state(|m| m.borrow().paint(ctx));
}

pub fn extra_quads() -> Vec<(f32, f32, f32, f32, [f32; 4])> {
    with_state(|m| m.borrow().extra_quads())
}

pub fn text_labels() -> Vec<TextLabel> {
    with_state(|m| m.borrow().text_labels())
}

/// Plate and labels in one call — see
/// [`ContextMenuState::paint_with_labels`].
pub fn paint_with_labels(ctx: &mut crate::scene::paint::PaintCtx) {
    with_state(|m| m.borrow().paint_with_labels(ctx));
}
