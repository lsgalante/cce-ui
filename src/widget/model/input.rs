//! The input concern: [`Input`] (hit, events, focus role, values, drags, ticks), [`FocusRole`],
//! and [`EventCtx`], what a widget handles an event with.

use super::*;

/// What an event handler may reach beyond its own state — the RFC §3.5 `EventCtx`, grown as
/// migrated widgets need capabilities: the laid-out content rect, the widget's id (scroll-gesture
/// gating keys on it), focus acquisition, and — transitionally — the raw [`UiContext`] for the
/// legacy shared state some widgets consult (`scroll_gesture_new`, …). `ui` is `None` when the
/// event was synthesized outside a routed path (the `FocusIn`/`FocusOut` from direct
/// `focus()`/`unfocus()` calls).
pub struct EventCtx<'a> {
    /// The widget's content rect (detached-label region excluded).
    pub rect: Rect,
    /// This widget's tree id.
    pub id: WidgetId,
    /// The routing context, when routed. **Transitional** — narrow widgets should only touch the
    /// legacy shared fields (scroll gesture state) until those get typed helpers here.
    pub ui: Option<&'a mut UiContext>,
    /// Where [`open_context_menu`](EventCtx::open_context_menu) asked for the menu: the
    /// adapter opens it once the widget has handled the event, handing itself over.
    menu_at: Option<(f32, f32)>,
}

impl<'a> EventCtx<'a> {
    pub(crate) fn new(rect: Rect, id: WidgetId, ui: Option<&'a mut UiContext>) -> Self {
        EventCtx { rect, id, ui, menu_at: None }
    }

    /// Open the context menu a widget asked for while it handled the event, on `host` (the
    /// widget's adapter, done handling it).
    pub(crate) fn open_requested_menu(self, host: &dyn WidgetHost) {
        if let (Some((px, py)), Some(ui)) = (self.menu_at, self.ui) {
            ui.handle_right_click(host, px, py);
        }
    }
}

impl EventCtx<'_> {
    /// Make this widget the window's focus (`UiContext::focused_widget`). Recorded, not
    /// dispatched: the widget is handling an event now and has its focus already; the
    /// widget that held focus before is told (`unfocus`). Without a context (inside
    /// `focus()` / `unfocus()`, whose caller is the context) there is nothing to record.
    pub fn request_focus(&mut self) {
        let id = self.id;
        if let Some(ui) = self.ui.as_deref_mut() {
            ui.claim_focus(id);
        }
    }

    /// Drop this widget's hold on the window's focus, if it has it (MenuBar releases focus
    /// when its dropdowns close).
    pub fn release_focus(&mut self) {
        let id = self.id;
        if let Some(ui) = self.ui.as_deref_mut() {
            if ui.focused_widget == Some(id) {
                ui.focused_widget = None;
            }
        }
    }

    /// Whether this widget holds the window's focus.
    pub fn is_focused(&self) -> bool {
        self.ui.as_deref().is_some_and(|ui| ui.focused_widget == Some(self.id))
    }

    /// Open the shared context menu on this widget (legacy `ctx.handle_right_click(self, …)`),
    /// for widgets that must do work *before* the menu opens — Breadcrumb records which segment
    /// was right-clicked first, so the menu header can show that segment's path.
    /// [`Input::opens_context_menu`] can't express that: the adapter's gate runs instead of
    /// `on_event`, not after it. The menu opens when the widget has handled the event (the
    /// adapter hands itself to the context then). No-op outside a routed path.
    pub fn open_context_menu(&mut self, px: f32, py: f32) {
        if self.ui.is_some() {
            self.menu_at = Some((px, py));
        }
    }
}

/// The input concern — hit-testing and event handling against the laid-out rect. Mirrors the
/// legacy `WidgetHost::hit_test` / `handle_event` pair, but with the RFC's centralizations: the
/// default hit is plain rect containment (no per-widget address hacks), and pointer-positioned
/// events are hit-gated by the adapter *before* they reach [`on_event`](Input::on_event), so a
/// narrow widget never re-implements the "am I actually under the cursor?" boilerplate that every
/// legacy `mouse_input` override carries.
/// A widget's part in keyboard navigation — see [`Input::focus_role`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusRole {
    /// Not a stop: the traversal skips it.
    None,
    /// A plate — a thing you press. Enter / Space act on it while focused.
    Plate,
    /// A well — a thing you enter. It opens for typing when focused.
    Well,
}

pub trait Input {
    /// Whether the point `(x, y)` hits this widget, given its laid-out `rect`. Override for
    /// non-rectangular hit shapes. Default: containment (edges inclusive, matching the legacy
    /// `hit_test`).
    fn hit(&self, rect: Rect, x: f32, y: f32) -> bool {
        x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height
    }

    /// React to `event`. Return `true` to consume it (the router marks the widget dirty and
    /// stops propagation). `MouseButton` *presses* and `MouseWheel` arrive only when
    /// [`hit`](Input::hit) passed; *releases* arrive ungated (press-tracking widgets commit or
    /// cancel from anywhere); `MouseEnter` / `MouseLeave` are synthesized by the hover machinery.
    /// Default: ignore everything.
    fn on_event(&mut self, _event: &Event, _ectx: &mut EventCtx) -> bool {
        false
    }

    /// Whether pressing on this widget blocks dragging the movable root plate under it. Passive
    /// display widgets (separators, status dots) return `false` so drags pass through them.
    /// Default: `true`, matching the legacy `WidgetHost` default.
    fn blocks_root_plate_drag(&self) -> bool {
        true
    }

    /// Whether a right-click on this widget opens the shared config context menu (the adapter
    /// then routes it to `UiContext::handle_right_click`, which `on_event` can't reach — it has
    /// no ctx by design). Default: no.
    fn opens_context_menu(&self) -> bool {
        false
    }

    /// What this widget is to keyboard navigation — see "Plates, wells and
    /// seams" in `CLAUDE.md`. A [`FocusRole::Plate`] is a thing you press
    /// (Enter / Space act on it while focused); a [`FocusRole::Well`] opens
    /// for typing when focused. Both are stops for `UiContext::focus_step`.
    /// Default: [`FocusRole::None`] — skipped by the traversal. A widget that
    /// declares a role must handle `FocusIn` / `FocusOut`.
    fn focus_role(&self) -> FocusRole {
        FocusRole::None
    }

    /// Whether, focused, this widget takes Tab itself, so the toolkit's Tab walk
    /// (`Application::plate_navigation`) passes it the key instead of moving focus: a
    /// multi-line text box that is editing types it. The group chord (`focus_next_group`,
    /// Ctrl+Tab by default) still leaves it. Default false: Tab leaves a widget.
    fn keeps_tab(&self) -> bool {
        false
    }

    /// Whether the adapter hit-gates `MouseButton` presses before `on_event` (the leaf
    /// centralization). Event-proxying containers return `false`: legacy container
    /// `mouse_input` overrides saw every press — Switcher unfocuses its active child when a
    /// press lands outside it, which a gated `on_event` would never learn about.
    fn gates_presses(&self) -> bool {
        true
    }

    // --- The legacy polling/value-binding surface (`take_click`, `take_change`,
    // `get_value_string`/`set_value_string`, `value`) apps read widget state through. Kept on
    // `Input` to avoid a fourth trait bound; replaced by typed messages when RFC §3.5's EventCtx
    // lands. All default to the inert legacy defaults.

    /// Consume the "was clicked since last asked" flag.
    fn take_click(&mut self) -> bool {
        false
    }

    /// Consume the "value changed since last asked" flag.
    fn take_change(&mut self) -> bool {
        false
    }

    /// The widget's value serialized for the config system.
    fn value_string(&self) -> Option<String> {
        None
    }

    /// What this widget is to assistive technology, when the toolkit's guess from its type
    /// and [`focus_role`](Input::focus_role) is not it (`crate::a11y::role_for`). Default
    /// `None`: the guess stands.
    fn a11y_role(&self) -> Option<accesskit::Role> {
        None
    }

    /// The range an assistive tool may set this widget's value in, as `(min, max, step)` in
    /// the units [`value_string`](Input::value_string) reads in — a slider's or spin
    /// button's. Default `None`: the value is not a number a reader can set.
    fn a11y_range(&self) -> Option<(f64, f64, f64)> {
        None
    }

    /// The parts of this widget a screen reader should see as nodes of their own, under the
    /// widget's node: a radio group's radio buttons. `rect` is the widget's content rect.
    /// Default none.
    fn a11y_items(&self, _rect: Rect) -> Vec<crate::a11y::A11yItem> {
        Vec::new()
    }

    /// An assistive tool clicked item `idx` of [`a11y_items`](Input::a11y_items): do what a
    /// press on it does, reported as a change. Returns whether anything changed.
    fn a11y_select_item(&mut self, _idx: usize) -> bool {
        false
    }

    /// The text a reader reads and edits, when this widget is a text field: what it shows,
    /// its caret and selection (`crate::a11y::A11yText`). The tree publishes it as text runs,
    /// which is what gives the field AT-SPI's Text and EditableText interfaces. Default
    /// `None`: not a text field (its [`value_string`](Input::value_string) is its value).
    fn a11y_text(&self) -> Option<crate::a11y::A11yText> {
        None
    }

    /// Replace the field's text with what an assistive tool asked for (AT-SPI's
    /// `SetTextContents`), as the user replacing it would — undoable, the caret at its end —
    /// and mark it changed, so the host's `take_change` reports it. Returns whether it
    /// changed. Default: not settable.
    fn a11y_set_text(&mut self, _text: &str) -> bool {
        false
    }

    /// Set the value an assistive tool asked for (AT-SPI's `SetCurrentValue`), clamped to
    /// [`a11y_range`](Input::a11y_range), and mark it changed as a typed value would be, so
    /// the host's `take_change` reports it. Returns whether it changed. Default: not
    /// settable.
    fn a11y_set_value(&mut self, _value: f64) -> bool {
        false
    }

    /// Set the widget's value from a config string. Returns whether it parsed and changed.
    fn set_value_string(&mut self, _val: &str) -> bool {
        false
    }

    /// The widget's value as an integer (legacy `WidgetHost::value`).
    fn value(&self) -> i32 {
        0
    }

    // --- Clipboard/selection surface (the context menu's Cut/Copy/Paste/Select-All actions
    // call these on their target WidgetHost). The defaults replicate the `WidgetHost` defaults
    // byte-for-byte (whole-value copy through the value-string pair), so widgets migrated
    // before these hooks existed keep their exact behavior; TextBox overrides with real
    // selection-aware implementations.

    fn context_action(&mut self, action: crate::widget::ContextAction) -> bool {
        match action {
            crate::widget::ContextAction::Cut => {
                if let Some(val) = self.value_string() {
                    crate::widget::clipboard::copy_to_clipboard(&val);
                    self.set_value_string("")
                } else {
                    false
                }
            }
            crate::widget::ContextAction::Copy => {
                if let Some(val) = self.value_string() {
                    crate::widget::clipboard::copy_to_clipboard(&val);
                    true
                } else {
                    false
                }
            }
            crate::widget::ContextAction::Paste => {
                if let Some(text) = crate::widget::clipboard::read_from_clipboard() {
                    self.set_value_string(&text)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Whether direct `focus()`/`unfocus()` calls flip the base `focused` flag. Legacy widgets
    /// differ: most set it in their `focus` overrides, but TextBox never did — its detached
    /// label must not color as focused. Default: flip it (what every widget migrated so far
    /// has shipped with).
    fn tracks_base_focus(&self) -> bool {
        true
    }

    /// Selection state pushed in by list/row hosts (legacy `WidgetHost::set_selected`).
    fn set_selected(&mut self, _selected: bool) {}

    // --- Drag surface: legacy hosts (designer, control_panel, parameters_bg, graph, audio…)
    // drive drags by calling these directly on the widget, not through events.

    /// Whether a press on this widget starts a host-driven drag. Receives the laid-out rect:
    /// scroll widgets (Spreadsheet) are draggable only while their content overflows it.
    fn draggable(&self, _rect: Rect) -> bool {
        false
    }
    fn is_dragging(&self) -> bool {
        false
    }
    fn drag_begin(&mut self, _px: f32, _py: f32, _rect: Rect) {}
    /// Returns whether the drag changed the widget's value (drives redraw).
    fn drag_update(&mut self, _px: f32, _py: f32, _rect: Rect) -> bool {
        false
    }
    /// For self-moving widgets (Panel, Splitter): the new origin this drag step wants, or `None`
    /// if unmoved. The adapter applies it to the base rect (the model cannot reach it).
    fn drag_reposition(&mut self, _px: f32, _py: f32, _rect: Rect) -> Option<(f32, f32)> {
        None
    }
    fn drag_end(&mut self) {}
    /// Movement bounds pushed in by hosts (reached via the inherent `Adapted::set_drag_bounds`).
    fn set_drag_bounds(&mut self, _bx: f32, _by: f32, _bw: f32, _bh: f32) {}

    // --- Tick surface: hosts broadcast `WidgetHost::tick(dt)` every frame (the designer's render
    // loop) to advance time-based widget state — inertial scroll velocity, here. Transitional:
    // §3.6 `Animated<T>` + arena-driven frame requests replace hand-ticked state.

    /// Advance time-based state by `dt` seconds against the laid-out rect. Return whether
    /// anything observable changed (drives redraw).
    fn tick(&mut self, _dt: f32, _rect: Rect) -> bool {
        false
    }

    /// Context-carrying tick for legacy stateful containers whose per-frame work needs the
    /// routing context — TreeList commits its inline rename editor, re-targets focus, and
    /// drains its search box on tick. Runs right after [`tick`](Input::tick) with a routed
    /// [`EventCtx`] (ui + the adapter's id/pointer). Transitional, like the capability hooks.
    fn tick_ctx(&mut self, _dt: f32, _ectx: &mut EventCtx) -> bool {
        false
    }

    /// Whether this widget wants `tick` calls from tick-gating hosts (legacy
    /// `WidgetHost::wants_tick`; the designer ticks unconditionally and ignores this).
    fn wants_tick(&self) -> bool {
        false
    }

    /// Whether this widget consumes scroll gestures (legacy `WidgetHost::is_scrollable`, read by
    /// the router's scroll-gesture gating).
    fn scrollable(&self) -> bool {
        false
    }

    // --- Controller capabilities (transitional, like the polling surface above). The legacy
    // tree reaches a widget's typed API through the `WidgetHost::as_*_controller` downcast pairs;
    // `WidgetHost` is implemented exactly once (for `Adapted<W>`), so a migrated controller widget
    // re-exposes its controller impl through these hooks instead — `Some(self)` when `W`
    // implements the trait. Dies with `WidgetHost`: the end state reaches a controller through the
    // concrete `Adapted<W>` (or a `&dyn XController` held directly), per RFC §3.5.


    /// Keyboard modifier state pushed in by hosts before dispatch (legacy
    /// `WidgetHost::set_modifiers`).
    fn set_modifiers(&mut self, _ctrl: bool, _shift: bool, _alt: bool) {}

    /// The widget's visibility flag changed through `WidgetHost::set_visible` (the adapter owns
    /// the flag) — legacy hideable widgets used the setter for side effects (MenuBar closes
    /// its dropdowns and invalidates layout).
    fn visibility_changed(&mut self, _visible: bool) {}

    /// The widget's `WidgetHost::focused` answer, given the base flag — MenuBar reports focused
    /// while any of its dropdowns is open, beyond the flag itself. Default: the flag.
    fn is_focused(&self, base_focused: bool) -> bool {
        base_focused
    }

}
