//! The accessibility tree, published to screen readers on Linux: AT-SPI through
//! `accesskit_unix` (`docs/rfc-accessibility-locale.md`, phase 2).
//!
//! The Wayland shell owns one [`Publisher`] per session for an app that asks for one
//! ([`wanted`]: `Application::publishes_accessibility`, or `CCE_A11Y=1` for any app). The
//! adapter calls back on ITS thread — a screen reader arrived, asked for something, or left —
//! and each callback only posts an [`Event`] into the runner's loop, as an `AppSender` does;
//! the loop does the work on its own thread:
//!
//! - **arrived** ([`Event::Activated`]): publish the whole tree NOW. AccessKit requires it "no
//!   later than the next display refresh, even if a frame would not normally be rendered", and
//!   an idle window renders nothing, so the runner publishes from the event itself.
//! - **after every frame**: publish again ([`Publisher::publish`] builds nothing while no
//!   screen reader is connected — `update_if_active`).
//! - **asked** ([`Event::Action`]): [`act`] — focus a widget, or press a context-menu row.
//! - **the window's keyboard focus**: [`Publisher::window_focus`].
//!
//! Without the `a11y` feature the same API compiles to a stub whose [`Publisher::start`] is
//! `None`, so the runner carries no `cfg`.

use accesskit::{Action, ActionRequest};

use crate::backend::app::Application;

/// `CCE_A11Y_DEBUG=1`: log each activation, action, publish and window-focus change to stderr.
pub fn debug() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("CCE_A11Y_DEBUG").is_some())
}

/// What the adapter's thread tells the runner's loop.
#[derive(Debug)]
pub enum Event {
    /// A screen reader is connected and wants the tree.
    Activated,
    /// It left; the adapter builds nothing until it is back.
    Deactivated,
    /// It asked for something.
    Action(ActionRequest),
}

/// Whether `app` publishes its tree: it says so, or `CCE_A11Y=1` says so for any app (to
/// try one that has not opted in).
pub fn wanted<A: Application>(app: &A) -> bool {
    app.publishes_accessibility() || std::env::var("CCE_A11Y").is_ok_and(|v| v == "1")
}

/// Carry out an assistive tool's request on `app`; true when something changed (redraw).
///
/// - **Focus** on a widget's node focuses it, through the app's `UiContext` like a Tab step.
/// - **Click** on an open context menu's row presses it where it is drawn, so the menu runs
///   the row's action exactly as a pointer would.
///
/// Anything else is not supported yet and does nothing, as AccessKit requires of an action
/// the app cannot perform (a click on a widget is the next step).
pub fn act<A: Application>(app: &mut A, request: &ActionRequest) -> bool {
    use crate::widget::context_menu as cm;
    if debug() {
        eprintln!("[a11y] action {:?} on {:?}", request.action, request.target_node);
    }
    match request.action {
        Action::Focus => {
            let (Some(id), Some(ctx)) = (crate::a11y::widget_of(request.target_node), app.ui_context_mut()) else {
                return false;
            };
            if ctx.tree.get_ptr(id).is_none() {
                return false;
            }
            ctx.set_focused_id(id);
            true
        }
        Action::Click => {
            let Some(row) = crate::a11y::menu_row_of(request.target_node) else { return false };
            if !cm::is_visible() || row >= cm::options().len() {
                return false;
            }
            let (x, y) = (cm::x() + cm::w() * 0.5, cm::row_y(row) + cm::ROW_H * 0.5);
            cm::mouse_input(crate::widget::MouseButton::Left, crate::widget::ElementState::Pressed, x, y, app.ui_context_mut());
            true
        }
        _ => false,
    }
}

#[cfg(feature = "a11y")]
mod imp {
    use super::Event;
    use accesskit::{ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, TreeUpdate};

    use crate::backend::app::Application;

    /// The session's adapter.
    pub struct Publisher {
        adapter: accesskit_unix::Adapter,
    }

    struct Activation(calloop::channel::Sender<Event>);
    impl ActivationHandler for Activation {
        fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
            // The tree is the loop's to build; it publishes as this event arrives.
            let _ = self.0.send(Event::Activated);
            None
        }
    }

    struct Actions(calloop::channel::Sender<Event>);
    impl ActionHandler for Actions {
        fn do_action(&mut self, request: ActionRequest) {
            let _ = self.0.send(Event::Action(request));
        }
    }

    struct Deactivation(calloop::channel::Sender<Event>);
    impl DeactivationHandler for Deactivation {
        fn deactivate_accessibility(&mut self) {
            let _ = self.0.send(Event::Deactivated);
        }
    }

    impl Publisher {
        /// Register with the accessibility bus; the adapter's callbacks post to `tx`.
        pub fn start(tx: calloop::channel::Sender<Event>) -> Option<Publisher> {
            let adapter = accesskit_unix::Adapter::new(Activation(tx.clone()), Actions(tx.clone()), Deactivation(tx));
            log::info!("[a11y] publishing the accessibility tree over AT-SPI");
            Some(Publisher { adapter })
        }

        /// Hand the adapter `app`'s whole tree, if a screen reader is connected.
        pub fn publish<A: Application>(&mut self, app: &mut A, scale: f64) {
            self.adapter.update_if_active(|| {
                let t0 = web_time::Instant::now();
                let tree = crate::a11y::app_tree(app, scale);
                if super::debug() {
                    eprintln!("[a11y] publish: {} nodes, focus {:?}, built in {:?}", tree.nodes.len(), tree.focus, t0.elapsed());
                }
                tree
            });
        }

        /// The window gained or lost the keyboard.
        pub fn window_focus(&mut self, focused: bool) {
            if super::debug() {
                eprintln!("[a11y] window focus: {focused}");
            }
            self.adapter.update_window_focus_state(focused);
        }
    }
}

#[cfg(not(feature = "a11y"))]
mod imp {
    use super::Event;
    use crate::backend::app::Application;

    /// Built without the `a11y` feature: nothing is published.
    pub enum Publisher {}

    impl Publisher {
        pub fn start(_tx: calloop::channel::Sender<Event>) -> Option<Publisher> {
            None
        }
        pub fn publish<A: Application>(&mut self, _app: &mut A, _scale: f64) {
            match *self {}
        }
        pub fn window_focus(&mut self, _focused: bool) {
            match *self {}
        }
    }
}

pub use imp::Publisher;
