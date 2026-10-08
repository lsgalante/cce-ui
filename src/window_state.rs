//! A window's interaction state: what the user is doing in one window right now, apart from
//! the widgets themselves (`docs/rfc-global-state.md`, phase 2).
//!
//! - the open context menu (`widget::context_menu`),
//! - the hover highlight and the cursor it follows (`widget::hover_animation`),
//! - the side swipe being recognized (`widget::side_swipe`),
//! - the input method's composition and the caret it is drawn at (`ime`).
//!
//! Each lived in a thread-local of its own until 2026-10-08, so every window on a thread
//! shared one menu, one highlight and one composition. Now a window OWNS a
//! [`WindowState`], and its shell makes it the current one ([`enter`]) for as long as it
//! runs that window's code; the modules' free functions — `context_menu::show`,
//! `hover_animation::tick`, `ime::caret`, … — act on the current one, so the many apps
//! and widgets that call them (an app need not have a `UiContext` to show a menu) are
//! unchanged. With none entered — a test, a tool that draws no window — each thread has a
//! default one, which is what the thread-locals were.

use std::cell::RefCell;
use std::rc::Rc;

use crate::widget::context_menu::ContextMenuState;
use crate::widget::hover_animation::HoverState;
use crate::widget::side_swipe::SideSwipe;

/// One window's interaction state (see the module docs).
pub struct WindowState {
    pub(crate) context_menu: RefCell<ContextMenuState>,
    pub(crate) hover: RefCell<HoverState>,
    pub(crate) cursor: RefCell<(f32, f32)>,
    pub(crate) swipe: RefCell<SideSwipe>,
    pub(crate) ime: RefCell<crate::ime::State>,
}

impl WindowState {
    /// A window's state with nothing open, hovered, swiped or composed.
    pub fn new() -> Rc<WindowState> {
        Rc::new(WindowState {
            context_menu: RefCell::new(ContextMenuState::new()),
            hover: RefCell::new(HoverState::new()),
            cursor: RefCell::new((0.0, 0.0)),
            swipe: RefCell::new(SideSwipe::new()),
            ime: RefCell::new(crate::ime::State::default()),
        })
    }
}

thread_local! {
    static CURRENT: RefCell<Option<Rc<WindowState>>> = const { RefCell::new(None) };
    static DEFAULT: Rc<WindowState> = WindowState::new();
}

/// Run `f` on the current window's state: the one entered last on this thread, else the
/// thread's default.
pub fn with<R>(f: impl FnOnce(&WindowState) -> R) -> R {
    let state = CURRENT.with(|c| c.borrow().clone()).unwrap_or_else(|| DEFAULT.with(Rc::clone));
    f(&state)
}

/// While this lives, `state` is the current window's (see [`enter`]).
#[must_use = "the window's state is current only while this guard lives"]
pub struct Entered {
    previous: Option<Rc<WindowState>>,
}

impl Drop for Entered {
    fn drop(&mut self) {
        let previous = self.previous.take();
        CURRENT.with(|c| *c.borrow_mut() = previous);
    }
}

/// Make `state` the current window's until the guard drops (then the one before it is
/// again). A shell enters its window's state around everything it runs for that window.
pub fn enter(state: &Rc<WindowState>) -> Entered {
    let previous = CURRENT.with(|c| c.replace(Some(Rc::clone(state))));
    Entered { previous }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two windows keep two menus: what one shows the other does not, and the one entered
    /// before comes back when the guard drops.
    #[test]
    fn each_window_has_its_own_menu() {
        use crate::widget::context_menu as cm;
        let (a, b) = (WindowState::new(), WindowState::new());
        let _in_a = enter(&a);
        cm::show(10.0, 10.0, vec!["Copy".into()], 0, crate::widget::WidgetId(1));
        assert!(cm::is_visible());
        {
            let _in_b = enter(&b);
            assert!(!cm::is_visible(), "the other window has no menu open");
            cm::show(20.0, 20.0, vec!["Paste".into(), "Cut".into()], 0, crate::widget::WidgetId(2));
            assert_eq!(cm::options().len(), 2);
        }
        assert!(cm::is_visible());
        assert_eq!(cm::options(), ["Copy"], "back in the first window, its own menu");
        cm::hide();
    }
}
