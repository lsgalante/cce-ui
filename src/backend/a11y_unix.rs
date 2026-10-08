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
//! - **asked** ([`Event::Action`]): [`act`] — focus a widget, press or step one (by the key a
//!   keyboard user would press, through the app's own key handling), or press a context-menu
//!   row.
//! - **the window's keyboard focus**: [`Publisher::window_focus`].
//!
//! Without the `a11y` feature the same API compiles to a stub whose [`Publisher::start`] is
//! `None`, so the runner carries no `cfg`.

use crate::widget::WidgetHostExt;
use accesskit::{Action, ActionData, ActionRequest};

use crate::widget::NamedKey;

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

/// What [`act`] did, and what is left for the runner to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Acted {
    /// Nothing: the target is gone, or the action is not one it takes.
    Nothing,
    /// Done; redraw and republish.
    Changed,
    /// The target is focused; now press this key as a keyboard user would
    /// (`Driver::press_named_key`), so the widget and the app answer it as they answer one.
    Key(NamedKey),
}

/// Carry out an assistive tool's request on `app`.
///
/// - **Focus** on a widget's node focuses it, through the app's `UiContext` like a Tab step.
/// - **Click** on a widget focuses it and presses Space: what activates a plate from the
///   keyboard (`FocusRole::Plate`), so the app learns of it the way it learns of a key.
/// - **Increment / Decrement** focus a slider or spin button and press the key that steps
///   it: Right / Left on a slider (a `RangeSlider` uses Up / Down to change ends), Up / Down
///   on a spin button.
/// - **SetValue** with a number (AT-SPI's `SetCurrentValue`, how a reader adjusts a slider
///   or spin button on Linux, where AccessKit offers no Increment) sets it on the widget
///   (`WidgetHost::a11y_set_value`), marked changed for the app's `take_change`. An app
///   that drains changes in `tick` sees it this turn; one that drains them only in its
///   input handlers sees it at the next input.
/// - **Click** on a widget's item (a radio button, `a11y::A11yItem`) does what a press on it
///   does (`WidgetHost::a11y_select_item`) and puts the keyboard on its widget.
/// - **Click** on an open context menu's row presses it where it is drawn, so the menu runs
///   the row's action exactly as a pointer would.
///
/// Anything else does nothing, as AccessKit requires of an action the app cannot perform.
/// A key reaches the widget only through the app's `handle_key_input`, as every key does —
/// an app that does not route keys to its focused widget answers a reader as it answers a
/// keyboard.
pub fn act<A: Application>(app: &mut A, request: &ActionRequest) -> Acted {
    use crate::widget::context_menu as cm;
    if debug() {
        eprintln!("[a11y] action {:?} on {:?}", request.action, request.target_node);
    }
    if let Some(row) = crate::a11y::menu_row_of(request.target_node) {
        if request.action != Action::Click || !cm::is_visible() || row >= cm::options().len() {
            return Acted::Nothing;
        }
        let (x, y) = (cm::x() + cm::w() * 0.5, cm::row_y(row) + cm::ROW_H * 0.5);
        cm::mouse_input(crate::widget::MouseButton::Left, crate::widget::ElementState::Pressed, x, y, app.ui_context_mut());
        return Acted::Changed;
    }
    if let Some((id, idx)) = crate::a11y::item_of(request.target_node) {
        // A click on a widget's item (a radio button): what a press on it does, and the
        // keyboard goes to its widget, as after a press.
        if request.action != Action::Click {
            return Acted::Nothing;
        }
        let Some(ctx) = app.ui_context_mut() else { return Acted::Nothing };
        if !ctx.get_widget_mut(id).is_some_and(|w| w.a11y_select_item(idx)) {
            return Acted::Nothing;
        }
        if ctx.focused_widget != Some(id) {
            ctx.set_focused_id(id);
            app.focus_stepped();
        }
        return Acted::Changed;
    }
    let Some(id) = crate::a11y::widget_of(request.target_node) else { return Acted::Nothing };
    let Some(ctx) = app.ui_context_mut() else { return Acted::Nothing };
    if request.action == Action::SetValue {
        // Set where it is, focus untouched: a reader adjusting a value has not moved.
        let Some(ActionData::NumericValue(value)) = request.data else { return Acted::Nothing };
        let changed = ctx.get_widget_mut(id).is_some_and(|w| w.a11y_set_value(value));
        return if changed { Acted::Changed } else { Acted::Nothing };
    }
    let Some(w) = ctx.get_widget(id) else { return Acted::Nothing };
    let key = match request.action {
        Action::Focus => None,
        action => match crate::a11y::key_for(w, action) {
            Some(key) => Some(key),
            None => return Acted::Nothing,
        },
    };
    if ctx.focused_widget != Some(id) {
        ctx.set_focused_id(id);
        app.focus_stepped();
    }
    key.map_or(Acted::Changed, Acted::Key)
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
