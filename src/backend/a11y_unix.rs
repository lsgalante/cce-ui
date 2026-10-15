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
///   (`WidgetHost::a11y_set_value`); with text (`SetTextContents` on a text field's
///   EditableText) it replaces the field's text (`WidgetHost::a11y_set_text`). Either is
///   marked changed for the app's `take_change` ([`set_value`]). An app
///   that drains changes in `tick` sees it this turn; one that drains them only in its
///   input handlers sees it at the next input.
/// - **Click** on a widget's item (a radio button, `a11y::A11yItem`) does what a press on it
///   does (`WidgetHost::a11y_select_item`) and puts the keyboard on its widget, unless the
///   widget holds it already (through a field of its: `UiContext::focus_holder`) —
///   [`item_click`]. **SetValue** with a number on an item with a range sets that part
///   (`WidgetHost::a11y_set_item_value`), focus untouched ([`set_item_value`]).
/// - **Click** on an open context menu's row runs it as a press on it would, wherever it is
///   drawn ([`menu_click`]): the row's action, a page row's turn, or — a row the app owns —
///   the pick the app takes with `context_menu::take_picked`.
/// - Anything on one of the app's own nodes (`Application::accessibility`) is the app's:
///   `Application::accessibility_action`, in its own terms (`a11y::AppAction`) — a reader
///   setting a field the app draws (`AppNodes::text_field`) is `AppAction::SetText`.
///
/// Anything else does nothing, as AccessKit requires of an action the app cannot perform.
/// A key reaches the widget only through the app's `handle_key_input`, as every key does —
/// an app that does not route keys to its focused widget answers a reader as it answers a
/// keyboard.
pub fn act<A: Application>(app: &mut A, request: &ActionRequest) -> Acted {
    if debug() {
        eprintln!("[a11y] action {:?} on {:?}", request.action, request.target_node);
    }
    if let Some(row) = crate::a11y::menu_row_of(request.target_node) {
        if request.action != Action::Click {
            return Acted::Nothing;
        }
        return menu_click(row, app.ui_context_mut());
    }
    if let Some((id, idx)) = crate::a11y::item_of(request.target_node) {
        let Some(ctx) = app.ui_context_mut() else { return Acted::Nothing };
        return match request.action {
            Action::Click => {
                let (acted, moved) = item_click(ctx, id, idx);
                if moved {
                    app.focus_stepped();
                }
                acted
            }
            Action::SetValue => set_item_value(ctx, id, idx, request.data.as_ref()),
            _ => Acted::Nothing,
        };
    }
    if let Some(n) = crate::a11y::app_node_of(request.target_node) {
        return if app_action(request).is_some_and(|action| app.accessibility_action(n, action)) { Acted::Changed } else { Acted::Nothing };
    }
    let Some(id) = crate::a11y::widget_of(request.target_node) else { return Acted::Nothing };
    let Some(ctx) = app.ui_context_mut() else { return Acted::Nothing };
    if request.action == Action::SetValue {
        return set_value(ctx, id, request.data.as_ref());
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

/// A reader's Click on item `idx` of widget `id` (a radio button, a ramp's key): what a press
/// on it does (`WidgetHost::a11y_select_item`), and the keyboard goes to the widget, as after
/// a press — unless the widget holds it already, itself or through one of its fields
/// (`UiContext::focus_holder`), whose focus the click leaves where it is. Answers what was
/// done and whether the focus moved.
pub fn item_click(ctx: &mut crate::context::UiContext, id: crate::widget::WidgetId, idx: usize) -> (Acted, bool) {
    if !ctx.get_widget_mut(id).is_some_and(|w| w.a11y_select_item(idx)) {
        return (Acted::Nothing, false);
    }
    let moved = ctx.focus_holder() != Some(id);
    if moved {
        ctx.set_focused_id(id);
    }
    (Acted::Changed, moved)
}

/// A reader's `SetValue` on item `idx` of widget `id`, a number for a part with a range
/// (`WidgetHost::a11y_set_item_value`), focus untouched as in [`set_value`].
pub fn set_item_value(ctx: &mut crate::context::UiContext, id: crate::widget::WidgetId, idx: usize, data: Option<&ActionData>) -> Acted {
    let Some(ActionData::NumericValue(value)) = data else { return Acted::Nothing };
    if ctx.get_widget_mut(id).is_some_and(|w| w.a11y_set_item_value(idx, *value)) { Acted::Changed } else { Acted::Nothing }
}

/// A reader's Click on row `row` of the open context menu: run as a press on it would be
/// (`context_menu::activate`), so a row the app owns lands in the pick the app takes
/// (`context_menu::take_picked`) as a pointer's press on it does. Nothing for a row that
/// cannot run (a header, a disabled row, a slider) or a closed menu.
pub fn menu_click(row: usize, ctx: Option<&mut crate::context::UiContext>) -> Acted {
    if crate::widget::context_menu::activate(row, ctx) { Acted::Changed } else { Acted::Nothing }
}

/// A request on one of the app's own nodes, in the app's terms (`Application::accessibility_action`).
fn app_action(request: &ActionRequest) -> Option<crate::a11y::AppAction> {
    use crate::a11y::AppAction;
    Some(match (request.action, request.data.as_ref()) {
        (Action::Focus, _) => AppAction::Focus,
        (Action::Click, _) => AppAction::Click,
        (Action::ShowContextMenu, _) => AppAction::ShowContextMenu,
        (Action::SetValue, Some(ActionData::Value(text))) => AppAction::SetText(text.to_string()),
        (Action::SetValue, Some(ActionData::NumericValue(value))) => AppAction::SetNumber(*value),
        (Action::Increment, _) => AppAction::Increment,
        (Action::Decrement, _) => AppAction::Decrement,
        _ => return None,
    })
}

/// A reader's `SetValue` on widget `id`, where it is, focus untouched (a reader adjusting a
/// value has not moved): a number for a slider or spin button (`WidgetHost::a11y_set_value`),
/// text for a text field (AT-SPI's `SetTextContents`, `WidgetHost::a11y_set_text`).
pub fn set_value(ctx: &mut crate::context::UiContext, id: crate::widget::WidgetId, data: Option<&ActionData>) -> Acted {
    let Some(w) = ctx.get_widget_mut(id) else { return Acted::Nothing };
    let changed = match data {
        Some(ActionData::NumericValue(value)) => w.a11y_set_value(*value),
        Some(ActionData::Value(text)) => w.a11y_set_text(text),
        _ => false,
    };
    if changed { Acted::Changed } else { Acted::Nothing }
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
                let tree = crate::backend::app::accessibility_tree(app, scale);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::UiContext;
    use crate::widget::{Spinbox, TextBox};

    /// A reader's SetValue reaches a text field as text (AT-SPI's `SetTextContents`) and a
    /// spin button as a number; each is reported as the user's change would be, and neither
    /// takes the other's kind. The same text again, or a disabled field, changes nothing.
    #[test]
    fn a_reader_sets_a_field_by_text_and_a_spin_button_by_number() {
        let mut ctx = UiContext::new();
        let name = ctx.insert(TextBox::new("Ada".to_string()));
        let count = ctx.insert(Spinbox::new(3, 0, 10, 1));
        let text = |s: &str| ActionData::Value(s.into());

        assert_eq!(set_value(&mut ctx, name.id(), Some(&text("Grace"))), Acted::Changed);
        assert_eq!(ctx[name].text, "Grace");
        assert!(ctx[name].take_change(), "the app hears of it as of a typed change");
        assert_eq!(set_value(&mut ctx, name.id(), Some(&text("Grace"))), Acted::Nothing, "unchanged");
        assert_eq!(set_value(&mut ctx, name.id(), Some(&ActionData::NumericValue(4.0))), Acted::Nothing);

        assert_eq!(set_value(&mut ctx, count.id(), Some(&ActionData::NumericValue(7.0))), Acted::Changed);
        assert_eq!(set_value(&mut ctx, count.id(), Some(&text("2"))), Acted::Nothing, "a spin button is set by number");

        // Being edited, the box takes it as a replacement it can undo, the caret at its end.
        ctx.set_focused_id(name.id());
        assert_eq!(set_value(&mut ctx, name.id(), Some(&text("Ada Lovelace"))), Acted::Changed);
        let t = ctx[name].a11y_text().unwrap();
        assert_eq!((t.text.as_str(), t.selection), ("Ada Lovelace", Some((12, 12))));
        assert!(ctx[name].context_action(crate::widget::ContextAction::Undo), "undoable");
        assert_eq!(ctx[name].a11y_text().unwrap().text, "Grace");

        // A colour selector takes a colour as text, and nothing that is not one.
        let accent = ctx.insert(crate::widget::ColorSelector::new([0x40, 0x80, 0xff]));
        assert_eq!(set_value(&mut ctx, accent.id(), Some(&text("#00ff00"))), Acted::Changed);
        assert_eq!(ctx[accent].a11y_value().as_deref(), Some("#00ff00"));
        assert_eq!(set_value(&mut ctx, accent.id(), Some(&text("green-ish"))), Acted::Nothing);

        ctx[name].disabled = true;
        assert_eq!(set_value(&mut ctx, name.id(), Some(&text("Hopper"))), Acted::Nothing, "a disabled field");
        assert_eq!(set_value(&mut ctx, crate::widget::WidgetId(usize::MAX), Some(&text("x"))), Acted::Nothing, "nothing there");
    }

    /// A reader's Click on a composite's item selects it and puts the keyboard on the
    /// composite — but where a field of the composite has the keyboard already, the field
    /// keeps it. SetValue sets an item with a range by number, focus untouched.
    #[test]
    fn a_readers_click_on_an_item_leaves_the_keyboard_on_its_composites_field() {
        use crate::widget::test_composite::Composite;
        let mut ctx = UiContext::new();
        let c = ctx.insert(Composite::new());
        let other = ctx.insert(TextBox::new(String::new()));
        ctx.set_focused_id(other.id());
        assert_eq!(item_click(&mut ctx, c.id(), 0), (Acted::Changed, true));
        assert!(ctx[c].key_selected && ctx.is_focused_id(c.id()), "selected, and the keyboard came with it");
        assert_eq!(item_click(&mut ctx, c.id(), 0), (Acted::Nothing, false), "already selected");

        ctx[c].key_selected = false;
        let field = ctx[c].fields[1];
        ctx.claim_field_focus(c.id(), field);
        assert_eq!(item_click(&mut ctx, c.id(), 0), (Acted::Changed, false));
        assert_eq!(ctx.focused_widget, Some(field), "the field keeps the keyboard");

        let n = ActionData::NumericValue(0.5);
        assert_eq!(set_item_value(&mut ctx, c.id(), 1, Some(&n)), Acted::Changed);
        assert_eq!(ctx[c].position, 0.5);
        assert_eq!(ctx.focused_widget, Some(field), "focus untouched");
        assert_eq!(set_item_value(&mut ctx, c.id(), 0, Some(&n)), Acted::Nothing, "a key has no value to set");
        assert_eq!(set_item_value(&mut ctx, c.id(), 1, Some(&ActionData::Value("0.7".into()))), Acted::Nothing, "set by number");
    }

    /// A reader's Click on a context-menu row the app owns lands in the pick the app takes,
    /// as a pointer's press on it does; one on a header or a disabled row does nothing, and
    /// leaves the menu up.
    #[test]
    fn a_readers_click_on_an_apps_menu_row_is_the_apps_pick() {
        use crate::widget::context_menu as cm;
        let mut ctx = UiContext::new();
        cm::show(40.0, 50.0, vec!["Title".to_string(), "Back".to_string(), "Open".to_string()], 1, crate::widget::WidgetId(0));
        cm::set_row_disabled(1);
        let generation = cm::generation();
        assert_eq!(menu_click(0, Some(&mut ctx)), Acted::Nothing, "a header");
        assert_eq!(menu_click(1, Some(&mut ctx)), Acted::Nothing, "a disabled row");
        assert!(cm::is_visible() && cm::take_picked().is_none());
        assert_eq!(menu_click(2, Some(&mut ctx)), Acted::Changed);
        assert!(!cm::is_visible());
        assert_eq!(cm::take_picked(), Some((generation, 2)));
        assert_eq!(menu_click(2, Some(&mut ctx)), Acted::Nothing, "the menu is closed");
    }

    /// What a reader asks of an app's own node reaches the app in its own terms.
    #[test]
    fn a_request_on_an_apps_node_is_the_apps_action() {
        use crate::a11y::AppAction;
        let req = |action, data| ActionRequest { action, target_tree: accesskit::TreeId::ROOT, target_node: crate::a11y::AppNodes::id(3), data };
        assert_eq!(app_action(&req(Action::SetValue, Some(ActionData::Value("x.org".into())))), Some(AppAction::SetText("x.org".into())));
        assert_eq!(app_action(&req(Action::SetValue, Some(ActionData::NumericValue(2.0)))), Some(AppAction::SetNumber(2.0)));
        assert_eq!(app_action(&req(Action::Focus, None)), Some(AppAction::Focus));
        assert_eq!(app_action(&req(Action::ShowContextMenu, None)), Some(AppAction::ShowContextMenu));
        assert_eq!(app_action(&req(Action::ScrollIntoView, None)), None);
    }
}
