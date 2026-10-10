//! A window's interaction state: what the user is doing in one window right now, apart from
//! the widgets themselves (`docs/rfc-global-state.md`, phase 2).
//!
//! - the open context menu (`widget::context_menu`),
//! - the hover highlight and the cursor it follows (`widget::hover_animation`),
//! - the side swipe being recognized (`widget::side_swipe`),
//! - the input method's composition and the caret it is drawn at (`ime`),
//! - the phase of the wheel event being dispatched (`widget::scroll_motion`),
//! - and the window's own properties ([`Props`]): its HiDPI scale, the display metric,
//!   its app id, whether it is fullscreen or maximized, and vertical text (`scale`,
//!   `units`, `backend::text`).
//!
//! Each lived in a thread-local of its own until 2026-10-08, so every window on a thread
//! shared one menu, one highlight and one composition. Now a window OWNS a
//! [`WindowState`], and its shell makes it the current one ([`enter`]) for as long as it
//! runs that window's code; the modules' free functions — `context_menu::show`,
//! `hover_animation::tick`, `ime::caret`, … — act on the current one, so the many apps
//! and widgets that call them (an app need not have a `UiContext` to show a menu) are
//! unchanged. With none entered — a test, a tool that draws no window — each thread has a
//! default one, which is what the thread-locals were.
//!
//! **The window's properties read differently off a window.** A worker thread asking for the
//! scale (a page rasterized at it, a tile decoded for it) has no window entered; it reads the
//! process-wide value, which every window's setter also writes — the last one set. So a
//! process with one window reads exactly what it did when these were only process-wide,
//! and two windows on one thread each read their own (`docs/rfc-global-state.md`, phase 4).
//! Under `cfg(test)` the process-wide scale is per thread, so a test's write reaches no other
//! test (`scale.rs`).

use std::cell::{Cell, RefCell};
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
    pub(crate) scroll_phase: Cell<crate::widget::ScrollPhase>,
    pub(crate) props: RefCell<Props>,
}

/// A window's own properties (see the module docs).
#[derive(Debug, Clone)]
pub struct Props {
    /// The HiDPI scale it is drawn at (`scale::scale_factor`).
    pub scale: f32,
    /// The display it is on, in logical px per mm (`units::metric`).
    pub metric: Option<crate::units::Metric>,
    pub app_id: String,
    pub fullscreen: bool,
    pub maximized: bool,
    /// Set while it stands on a screen edge as a vertical bar of this thickness
    /// (`backend::text::vertical_text`).
    pub vertical_text: Option<u32>,
}

impl Props {
    /// The process-wide values: what a window starts from.
    fn from_process() -> Props {
        Props {
            scale: crate::scale::process_scale_factor(),
            metric: None,
            app_id: crate::scale::process_app_id(),
            fullscreen: false,
            maximized: false,
            vertical_text: crate::backend::text::process_vertical_text(),
        }
    }
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
            scroll_phase: Cell::new(crate::widget::ScrollPhase::Wheel),
            props: RefCell::new(Props::from_process()),
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

/// Run `f` on the window entered on this thread, if one is: `None` off any window's thread
/// (where a window's properties read as the process-wide values).
pub fn entered<R>(f: impl FnOnce(&WindowState) -> R) -> Option<R> {
    let state = CURRENT.with(|c| c.borrow().clone())?;
    Some(f(&state))
}

/// The display metric of the window whose code is running: what `units::metric` answers
/// first ([`crate::units::set_metric_resolver`]).
fn window_metric() -> Option<crate::units::Metric> {
    entered(|w| w.props.borrow().metric).flatten()
}

/// The display metric changed: the current window's, if one is entered, and the
/// process-wide one (a forced PPI applied to both, `units::effective`).
pub fn set_metric(m: crate::units::Metric) {
    entered(|w| w.props.borrow_mut().metric = Some(crate::units::effective(m)));
    crate::units::set_metric(m);
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
    crate::units::set_metric_resolver(window_metric);
    let previous = CURRENT.with(|c| c.replace(Some(Rc::clone(state))));
    Entered { previous }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two windows draw at their own scales; off a window, the thread reads the last one
    /// set — and, the process-wide scale being per thread in tests, no other test's thread
    /// sees it; and the scroll phase is the window's.
    #[test]
    fn each_window_has_its_own_scale_and_a_worker_reads_the_last() {
        let (a, b) = (WindowState::new(), WindowState::new());
        {
            let _in_a = enter(&a);
            crate::scale::set_scale_factor(2.0);
            crate::widget::scroll_motion::set_scroll_phase(crate::widget::ScrollPhase::Finger);
        }
        {
            let _in_b = enter(&b);
            crate::scale::set_scale_factor(1.5);
            assert_eq!(crate::scale::scale_factor(), 1.5);
            assert_eq!(crate::widget::scroll_motion::current_scroll_phase(), crate::widget::ScrollPhase::Wheel, "b's own phase");
        }
        assert_eq!(crate::scale::scale_factor(), 1.5, "on no window, the last scale any window set");
        let other = std::thread::spawn(crate::scale::scale_factor).join().unwrap();
        assert_eq!(other, 1.0, "another test's thread keeps the default scale");
        let _in_a = enter(&a);
        assert_eq!(crate::scale::scale_factor(), 2.0, "a keeps its own");
        assert_eq!(crate::widget::scroll_motion::current_scroll_phase(), crate::widget::ScrollPhase::Finger);
        crate::widget::scroll_motion::set_scroll_phase(crate::widget::ScrollPhase::Wheel);
    }

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
