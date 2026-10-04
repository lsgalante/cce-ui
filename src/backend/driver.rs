//! The runner's platform-neutral half: what happens to input once a shell has
//! it in cce-ui's own terms. A shell (the Wayland runner today) translates its
//! window system's events — evdev buttons, xkb keysyms, `wl_pointer` axis
//! frames — into the calls here, and carries out what they ask back of the
//! window (a cursor, an interactive move or resize). Everything between —
//! modifier tracking, the undo/redo and plate-navigation chords, key repeat,
//! the CSD hit zones, the outside-press popover close, held-button release on
//! a lost pointer, the scroll phase, the pinch fallback — lives here once, so
//! a second shell routes exactly as the first does.
//!
//! The app is never owned here: each call takes a [`Turn`], the app plus the
//! runner's `redraw` / `exit` flags, borrowed from wherever the shell keeps
//! them (the Wayland runner keeps them on `EngineState`, where clients'
//! `register_sources` callbacks reach `inner` and `redraw` directly).

use web_time::Instant;

use super::app::{Application, LogicalPosition, LogicalSize};
use cursor_icon::CursorIcon;
use crate::widget::{ElementState, Key, KeyEvent, MouseButton, MouseScrollDelta, NamedKey, Position};

/// A key held down, for the runner's own key repeat.
pub struct PressedKey {
    pub logical_key: Key,
    pub text: Option<String>,
    pub first_pressed: Instant,
    pub last_repeated: Instant,
}

/// Held this long before the first repeat…
pub const KEY_REPEAT_DELAY: std::time::Duration = std::time::Duration::from_millis(500);
/// …then one repeat per this.
pub const KEY_REPEAT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(50);

fn is_repeatable_key(key: &Key) -> bool {
    match key {
        Key::Named(NamedKey::Backspace) |
        Key::Named(NamedKey::Delete) |
        Key::Named(NamedKey::ArrowLeft) |
        Key::Named(NamedKey::ArrowRight) |
        Key::Named(NamedKey::ArrowUp) |
        Key::Named(NamedKey::ArrowDown) |
        Key::Named(NamedKey::Home) |
        Key::Named(NamedKey::End) |
        Key::Character(_) => true,
        _ => false,
    }
}

/// The modifier keys as the keyboard last reported them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub logo: bool,
}

/// A window edge an interactive resize grabs: the window system's own enum,
/// spelled without it. The Wayland shell maps it onto `xdg_toplevel`'s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeEdge {
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// What a press turned out to be: the app's, or a grab of the window itself
/// that the shell carries out (and the app never hears about).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    Dispatched,
    Resize(ResizeEdge),
    Move,
}

/// Where a press landed, in the window's terms.
#[derive(Debug, Clone, Copy)]
pub struct PressSite {
    /// The surface's logical size: the CSD borders are measured from it.
    pub size: LogicalSize,
    /// The press came through the context menu's popup surface: it is the
    /// menu's, never a border or a movable plate of the window.
    pub on_popup: bool,
    /// The shell can start an interactive move / resize right now (a window
    /// and a seat to grab with). When it cannot, a press that would have been
    /// a grab is the app's instead — which is what the Wayland runner did on
    /// a layer surface, and what a shell with no grabs at all always does.
    pub can_grab: bool,
}

/// Where a scroll came from, as far as the phase cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollSource {
    Wheel,
    Finger,
    Continuous,
    WheelTilt,
}

/// One frame of scroll input, coalesced: the shell sums a frame's axis events
/// (in its own window system's units — `wl_pointer` axis values here) and
/// hands the total over once.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScrollFrame {
    pub h: f64,
    pub v: f64,
    pub discrete_h: i32,
    pub discrete_v: i32,
    /// The frame's source, if it named one.
    pub source: Option<ScrollSource>,
    /// A finger lifted (an axis stop) in this frame.
    pub stop: bool,
}

/// The app and the runner's two flags, for the length of one dispatch.
pub struct Turn<'a, A: Application> {
    pub app: &'a mut A,
    pub redraw: &'a mut bool,
    pub exit: &'a mut bool,
}

impl<A: Application> Turn<'_, A> {
    /// Hand a message from an input handler to `update`, and fold the two
    /// rebuild requests into the redraw flag — the shape every dispatch had.
    fn deliver(&mut self, msg: Option<A::Message>, mut rebuild: bool) {
        if let Some(msg) = msg {
            let mut update_rebuild = false;
            self.app.update(msg, &mut update_rebuild, self.exit);
            if update_rebuild {
                rebuild = true;
            }
        }
        if rebuild {
            *self.redraw = true;
        }
    }
}

/// The input state a session carries, and the routing over it.
pub struct Driver {
    pub mods: Modifiers,
    pub pressed_key: Option<PressedKey>,
    /// The pointer's last position, window-logical (popup events translated).
    pub cursor_pos: (f32, f32),
    /// Mouse buttons held, as a bitmask (1 Left / 2 Right / 4 Middle). On a
    /// lost pointer the real release goes to whatever surface takes the
    /// pointer next (fullscreen switches, layout animations), so
    /// [`pointer_leave`](Self::pointer_leave) synthesizes releases for the
    /// held set — a drag must end, not stay armed and steered by later
    /// motion — and only then runs the off-screen hover-clear (which would
    /// otherwise corrupt the drag: a ramp key snapped to the graph corner).
    pub buttons_down: u32,
    pub last_pinch_scale: f32,
    /// The `undo` / `redo` chords, resolved from `input.kdl` when the
    /// session starts.
    pub undo_chord: String,
    pub redo_chord: String,
    /// `focus_next_group` / `focus_prev_group` (input.kdl, cce-ui domain):
    /// the plate-navigation group jump, for apps that opt in.
    pub group_next_chord: String,
    pub group_prev_chord: String,
}

impl Default for Driver {
    fn default() -> Self {
        Self::new()
    }
}

/// The CSD border's width, in logical px.
const CSD_BORDER: f32 = 8.0;

fn button_bit(btn: MouseButton) -> u32 {
    match btn {
        MouseButton::Left => 1,
        MouseButton::Right => 2,
        _ => 4,
    }
}

impl Driver {
    pub fn new() -> Self {
        Self {
            mods: Modifiers::default(),
            pressed_key: None,
            cursor_pos: (0.0, 0.0),
            buttons_down: 0,
            last_pinch_scale: 1.0,
            undo_chord: crate::input::app_chord("undo", "ctrl+z"),
            redo_chord: crate::input::app_chord("redo", "ctrl+shift+z"),
            group_next_chord: crate::input::app_chord("focus_next_group", "ctrl+tab"),
            group_prev_chord: crate::input::app_chord("focus_prev_group", "ctrl+shift+tab"),
        }
    }

    /// Copy the modifiers into the app's widget context, where widgets read them.
    fn sync_mods<A: Application>(&self, app: &mut A) {
        if let Some(ctx) = app.ui_context_mut() {
            ctx.ctrl_pressed = self.mods.ctrl;
            ctx.shift_pressed = self.mods.shift;
            ctx.alt_pressed = self.mods.alt;
            ctx.logo_pressed = self.mods.logo;
        }
    }

    /// The cursor for the pointer at (lx, ly): the app's
    /// [`Application::cursor_icon`] override, else the standard-CSD edge
    /// cursors (status bars and non-standard-CSD apps fall back to Default).
    pub fn cursor_icon_at<A: Application>(&self, app: &A, lx: f32, ly: f32, size: LogicalSize) -> CursorIcon {
        // Over the context menu the pointer is the menu's,
        // whatever of the app lies at that place under it (a splitter, a
        // resize border) — and in their popups that place may be outside
        // the window altogether.
        if crate::widget::context_menu::is_visible() && crate::widget::context_menu::hit_test(lx, ly) {
            return CursorIcon::Default;
        }
        if let Some(icon) = app.cursor_icon(lx, ly) {
            return icon;
        }
        if app.settings().app_id.starts_with("cce-status")
            || !app.standard_csd()
            || !app.csd_resize_borders()
        {
            return CursorIcon::Default;
        }
        match csd_edge(lx, ly, size) {
            Some(ResizeEdge::TopLeft) => CursorIcon::NwResize,
            Some(ResizeEdge::TopRight) => CursorIcon::NeResize,
            Some(ResizeEdge::Top) => CursorIcon::NResize,
            Some(ResizeEdge::BottomLeft) => CursorIcon::SwResize,
            Some(ResizeEdge::BottomRight) => CursorIcon::SeResize,
            Some(ResizeEdge::Bottom) => CursorIcon::SResize,
            Some(ResizeEdge::Left) => CursorIcon::WResize,
            Some(ResizeEdge::Right) => CursorIcon::EResize,
            None => CursorIcon::Default,
        }
    }

    /// The pointer entered at `pos`. Enter carries the pointer's position but
    /// no motion follows until it actually moves — without this the app's
    /// hover state is stale from enter to first move, and a press in that
    /// window can misroute (e.g. a divider press falling through to the
    /// movable-root plate window drag).
    pub fn pointer_enter<A: Application>(&mut self, t: Turn<'_, A>, pos: LogicalPosition) {
        self.pointer_motion(t, pos);
    }

    /// The pointer moved to `pos`.
    pub fn pointer_motion<A: Application>(&mut self, t: Turn<'_, A>, pos: LogicalPosition) {
        let mut rebuild = false;
        t.app.handle_pointer_move(pos, &mut rebuild);
        if rebuild {
            *t.redraw = true;
        }
    }

    /// The pointer left the window. Focus can move mid-gesture (a fullscreen
    /// switch, a relayout sliding the window away): the real release then
    /// lands on another surface, and an armed drag would live forever. End
    /// held gestures with synthetic releases at the last known position
    /// first; then clear hover with an off-screen move — safe now that no
    /// drag is held.
    pub fn pointer_leave<A: Application>(&mut self, mut t: Turn<'_, A>) {
        if self.buttons_down != 0 {
            let (px, py) = self.cursor_pos;
            for btn in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
                if self.buttons_down & button_bit(btn) == 0 {
                    continue;
                }
                let mut rebuild = false;
                let msg = t.app.handle_mouse_input(
                    btn,
                    ElementState::Released,
                    LogicalPosition::new(px, py),
                    &mut rebuild,
                );
                t.deliver(msg, rebuild);
            }
            self.buttons_down = 0;
        }
        let mut rebuild = false;
        t.app.handle_pointer_move(LogicalPosition::new(-10000.0, -10000.0), &mut rebuild);
        if rebuild {
            *t.redraw = true;
        }
    }

    /// A button went down at `pos`. Returns a grab for the shell to start
    /// when the press is the window's (a CSD border, the titlebar band, a
    /// movable root plate); otherwise the press has been dispatched.
    pub fn pointer_press<A: Application>(
        &mut self,
        mut t: Turn<'_, A>,
        btn: MouseButton,
        pos: LogicalPosition,
        site: PressSite,
    ) -> Press {
        self.buttons_down |= button_bit(btn);
        let (lx, ly) = (pos.x, pos.y);

        // Client-side decorations: drag and resize. Never on the menu popup:
        // its presses are the menu's, and its coordinates, translated into the
        // window's, would otherwise read as a resize border or a movable plate.
        if site.can_grab
            && btn == MouseButton::Left
            && !site.on_popup
            && !t.app.settings().app_id.starts_with("cce-status")
            && t.app.standard_csd()
        {
            // Resize borders off: the compositor's own band outside the
            // window handles it; the move checks still run, so drag-to-move
            // still works.
            if t.app.csd_resize_borders() {
                if let Some(edge) = csd_edge(lx, ly, site.size) {
                    return Press::Resize(edge);
                }
            }
            // The titlebar band: y in [8, 32), clear of the top-right buttons.
            let is_widget = t.app.ui_context().is_some_and(|ctx| ctx.is_widget_at(lx, ly));
            if (!is_widget
                && t.app.csd_titlebar_move()
                && ly >= CSD_BORDER
                && ly < 32.0
                && lx < site.size.width - 70.0)
                || t.app.is_movable_root_plate_at(lx, ly)
            {
                return Press::Move;
            }
        }

        // Outside-press close for open popovers, BEFORE the app's dispatch:
        // apps commonly region-gate their routing, so an open menu's owner may
        // never hear about a press elsewhere.
        if btn == MouseButton::Left {
            let app = &mut *t.app;
            let offsets: Vec<_> = app
                .ui_context()
                .map(|ctx| ctx.popover_owners())
                .unwrap_or_default()
                .into_iter()
                .map(|id| (id, app.popover_offset(id)))
                .collect();
            if let Some(ctx) = app.ui_context_mut() {
                ctx.close_popovers_missed_by_press_with(lx, ly, |id| {
                    offsets.iter().find(|(o, _)| *o == id).map_or((0.0, 0.0), |&(_, d)| d)
                });
            }
        }

        let mut rebuild = false;
        let msg = t.app.handle_mouse_input(btn, ElementState::Pressed, pos, &mut rebuild);
        t.deliver(msg, rebuild);
        Press::Dispatched
    }

    /// A button came up at `pos`.
    pub fn pointer_release<A: Application>(&mut self, mut t: Turn<'_, A>, btn: MouseButton, pos: LogicalPosition) {
        self.buttons_down &= !button_bit(btn);
        let mut rebuild = false;
        let msg = t.app.handle_mouse_input(btn, ElementState::Released, pos, &mut rebuild);
        t.deliver(msg, rebuild);
    }

    /// One coalesced frame of scrolling at `pos`. Publishes the frame's
    /// phase (`scroll_motion::set_scroll_phase`) before the app sees it.
    pub fn scroll<A: Application>(&mut self, t: Turn<'_, A>, frame: ScrollFrame, pos: LogicalPosition) {
        let ScrollFrame { h, v, discrete_h, discrete_v, source, stop } = frame;
        // Per-app scroll factors from input.kdl (`<app>`/`cce-ui` domain
        // `input { }` blocks); the compositor's global device scaling has
        // already been applied at the source.
        let factors = crate::input::scroll_factors();
        let (phase, delta) = scroll_delta(&frame, factors);
        crate::widget::scroll_motion::set_scroll_phase(phase);
        if crate::scroll_debug() {
            static T0: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
            let ms = T0.get_or_init(Instant::now).elapsed().as_millis();
            eprintln!(
                "[scroll {ms}ms] runner: coalesced=({h:.2},{v:.2}) discrete=({discrete_h},{discrete_v}) source={source:?} stop={stop} phase={phase:?} factors=(tp {:.2}, m {:.2}) -> {delta:?} at ({:.0},{:.0})",
                factors.trackpad, factors.mouse, pos.x, pos.y
            );
        }
        let mut rebuild = false;
        self.sync_mods(t.app);
        t.app.handle_mouse_wheel(&delta, pos, &mut rebuild);
        if rebuild {
            *t.redraw = true;
        }
    }

    /// A pinch gesture began.
    pub fn pinch_begin(&mut self) {
        self.last_pinch_scale = 1.0;
    }

    /// A pinch gesture ended.
    pub fn pinch_end(&mut self) {
        self.last_pinch_scale = 1.0;
    }

    /// The pinch's cumulative `scale` moved, with the pointer where it last was.
    pub fn pinch_update<A: Application>(&mut self, t: Turn<'_, A>, scale: f32) {
        let factor = scale / self.last_pinch_scale;
        self.last_pinch_scale = scale;

        let (px, py) = self.cursor_pos;
        let mut rebuild = false;

        // First offer the gesture as-is: apps with true pinch surfaces (the
        // designer's 3D viewport) consume it here at 1:1 scale instead of
        // through the wheel synthesis below.
        if t.app.handle_pinch(factor, LogicalPosition::new(px, py), &mut rebuild) {
            if rebuild {
                *t.redraw = true;
            }
            return;
        }

        // Calculate the y_delta for PixelDelta mapping.
        // Since cce-graph interprets factor = 1.0 + y_delta * 0.015, we reverse it:
        let y_delta = (factor - 1.0) / 0.015;
        let delta = MouseScrollDelta::PixelDelta(Position {
            x: 0.0,
            y: y_delta as f64,
        });

        if let Some(ctx) = t.app.ui_context_mut() {
            ctx.ctrl_pressed = true; // Force ctrl_pressed = true for the pinch event
        }
        // A synthesized delta, not a scroll gesture: no glide, no fling.
        crate::widget::scroll_motion::set_scroll_phase(crate::widget::ScrollPhase::Wheel);

        t.app.handle_mouse_wheel(&delta, LogicalPosition::new(px, py), &mut rebuild);

        if let Some(ctx) = t.app.ui_context_mut() {
            ctx.ctrl_pressed = self.mods.ctrl; // Restore original state
        }

        if rebuild {
            *t.redraw = true;
        }
    }

    /// The keyboard reported new modifier state.
    pub fn set_modifiers<A: Application>(&mut self, app: &mut A, mods: Modifiers) {
        self.mods = mods;
        self.sync_mods(app);
    }

    /// The window gained (`true`) or lost keyboard focus. Losing it drops a
    /// held key and the held modifiers, whose releases go elsewhere.
    pub fn keyboard_focus<A: Application>(&mut self, t: Turn<'_, A>, focused: bool) {
        if !focused {
            self.pressed_key = None;
            self.mods.ctrl = false;
            self.mods.shift = false;
            self.mods.alt = false;
        }
        let mut rebuild = false;
        t.app.handle_focus_change(focused, &mut rebuild);
        if rebuild {
            *t.redraw = true;
        }
    }

    /// A key went down or up, already in cce-ui's terms: the shell maps its
    /// window system's key (an xkb keysym here) to `logical_key`, and passes
    /// the text the key types, if any.
    pub fn key<A: Application>(
        &mut self,
        mut t: Turn<'_, A>,
        logical_key: Key,
        text: Option<String>,
        state: ElementState,
    ) {
        let event = KeyEvent {
            state,
            logical_key,
            text,
            repeat: false,
            ctrl: self.mods.ctrl,
            shift: self.mods.shift,
            alt: self.mods.alt,
        };

        if state == ElementState::Pressed {
            if is_repeatable_key(&event.logical_key) {
                self.pressed_key = Some(PressedKey {
                    logical_key: event.logical_key.clone(),
                    text: event.text.clone(),
                    first_pressed: Instant::now(),
                    last_repeated: Instant::now(),
                });
            } else {
                self.pressed_key = None;
            }
        } else if state == ElementState::Released {
            if let Some(ref pk) = self.pressed_key {
                if pk.logical_key == event.logical_key {
                    self.pressed_key = None;
                }
            }
        }

        self.sync_mods(t.app);

        // Escape dismisses the shared context menu before app dispatch — the
        // toolkit-wide default, mirroring the click-outside dismissal. Consumed:
        // while a menu is open, Escape means "close it", nothing else.
        if state == ElementState::Pressed
            && event.logical_key == Key::Named(NamedKey::Escape)
            && crate::widget::context_menu::is_visible()
        {
            crate::widget::context_menu::hide();
            *t.redraw = true;
            return;
        }

        let mut rebuild = false;
        if self.route_history_chord(t.app, &event, &mut rebuild)
            || self.route_plate_navigation(t.app, &event, &mut rebuild)
        {
            *t.redraw = true;
            return;
        }
        let msg = t.app.handle_key_input(&event, &mut rebuild);
        t.deliver(msg, rebuild);
    }

    /// The runner's key repeat: once a held key has been down
    /// [`KEY_REPEAT_DELAY`], deliver it again every [`KEY_REPEAT_INTERVAL`].
    /// Called once per loop turn.
    pub fn repeat_keys<A: Application>(&mut self, mut t: Turn<'_, A>) {
        let Some(ref mut pk) = self.pressed_key else { return };
        let now = Instant::now();
        if now.duration_since(pk.first_pressed) < KEY_REPEAT_DELAY
            || now.duration_since(pk.last_repeated) < KEY_REPEAT_INTERVAL
        {
            return;
        }
        pk.last_repeated = now;
        let event = KeyEvent {
            state: ElementState::Pressed,
            logical_key: pk.logical_key.clone(),
            text: pk.text.clone(),
            repeat: true,
            ctrl: self.mods.ctrl,
            shift: self.mods.shift,
            alt: self.mods.alt,
        };

        self.sync_mods(t.app);

        let mut rebuild = false;
        if self.route_history_chord(t.app, &event, &mut rebuild)
            || self.route_plate_navigation(t.app, &event, &mut rebuild)
        {
            *t.redraw = true;
        } else {
            let msg = t.app.handle_key_input(&event, &mut rebuild);
            t.deliver(msg, false);
        }
        if rebuild {
            *t.redraw = true;
        }
    }

    /// Advance the app by `dt` seconds: its own `tick`, then its retained
    /// `UiContext`'s (widget tick receivers — e.g. an animating Dropdown
    /// popover) — but only when the app's own tick did not already tick the
    /// context this turn. Receivers integrate `dt` (scroll glides, slider
    /// inertia), so the old "double-ticking is harmless" assumption ran
    /// every glide at twice its configured rate in apps that tick the
    /// context themselves.
    pub fn tick<A: Application>(&mut self, t: Turn<'_, A>, dt: f32) {
        let mut rebuild = false;
        let roster_ticks_before = t.app.ui_context_mut().map(|ctx| ctx.tick_count());
        t.app.tick(dt, &mut rebuild);
        if rebuild {
            *t.redraw = true;
        }
        if let Some(ctx) = t.app.ui_context_mut() {
            if Some(ctx.tick_count()) == roster_ticks_before && ctx.tick(dt) {
                *t.redraw = true;
            }
        }
    }

    /// The toolkit's Tab traversal, for apps that opt in
    /// (`Application::plate_navigation`): a bare Tab / Shift+Tab press moves
    /// keyboard focus to the next / previous plate or well. Returns whether it
    /// moved; otherwise the key is dispatched as usual.
    fn route_plate_navigation<A: Application>(&self, app: &mut A, event: &KeyEvent, rebuild: &mut bool) -> bool {
        if event.state != ElementState::Pressed {
            return false;
        }
        // The group jump first (its chords carry ctrl); then a bare Tab.
        let group_next = crate::widget::match_key_shortcut(event, &self.group_next_chord);
        let group_prev = !group_next && crate::widget::match_key_shortcut(event, &self.group_prev_chord);
        let bare_tab = event.logical_key == Key::Named(NamedKey::Tab)
            && !self.mods.ctrl
            && !self.mods.alt
            && !self.mods.logo;
        if !group_next && !group_prev && !bare_tab {
            return false;
        }
        let reverse = if bare_tab { self.mods.shift } else { group_prev };
        if !app.plate_navigation() {
            return false;
        }
        let moved = app
            .ui_context_mut()
            .is_some_and(|ctx| if bare_tab { ctx.focus_step(reverse) } else { ctx.focus_step_group(reverse) });
        if moved {
            app.focus_stepped();
            *rebuild = true;
        }
        moved
    }

    /// The toolkit-wide undo/redo routing: a press matching the `undo` /
    /// `redo` chord goes to the focused widget first (`ContextAction::Undo`
    /// / `Redo` — a text box that is editing steps its own typing), then to
    /// the app's `Application::undo` / `redo`. Returns whether either took
    /// it; otherwise the key is dispatched as usual, so an app with its own
    /// scheme is undisturbed. Runs for repeats too — holding the chord walks
    /// the history like holding Backspace walks the text.
    fn route_history_chord<A: Application>(&self, app: &mut A, event: &KeyEvent, rebuild: &mut bool) -> bool {
        if event.state != ElementState::Pressed {
            return false;
        }
        let undo = crate::widget::match_key_shortcut(event, &self.undo_chord);
        let redo = !undo && crate::widget::match_key_shortcut(event, &self.redo_chord);
        if !undo && !redo {
            return false;
        }
        let action = if undo { crate::widget::ContextAction::Undo } else { crate::widget::ContextAction::Redo };
        if let Some(ctx) = app.ui_context_mut() {
            if ctx.focused_context_action(action) {
                *rebuild = true;
                return true;
            }
        }
        let taken = if undo { app.undo(rebuild) } else { app.redo(rebuild) };
        if taken {
            *rebuild = true;
        }
        taken
    }
}

/// A coalesced scroll frame as the delta the app is handed, and the phase it
/// belongs to. Smooth-scroll phase: a finger lift is a stop frame (no
/// delta); finger/continuous sources track 1:1 and may fling on the lift;
/// everything else is a wheel notch that glides.
fn scroll_delta(frame: &ScrollFrame, factors: crate::input::ScrollFactors) -> (crate::widget::ScrollPhase, MouseScrollDelta) {
    let ScrollFrame { h, v, discrete_h, discrete_v, source, stop } = *frame;
    let no_delta = h == 0.0 && v == 0.0 && discrete_h == 0 && discrete_v == 0;
    let phase = if stop && no_delta {
        crate::widget::ScrollPhase::FingerEnd
    } else if discrete_h == 0
        && discrete_v == 0
        && matches!(source, None | Some(ScrollSource::Finger) | Some(ScrollSource::Continuous))
    {
        crate::widget::ScrollPhase::Finger
    } else {
        crate::widget::ScrollPhase::Wheel
    };
    let delta = if discrete_h == 0 && discrete_v == 0 {
        // Pixel scroll event from touchpad / smooth mouse
        MouseScrollDelta::PixelDelta(Position {
            x: -h * factors.trackpad,
            y: -v * factors.trackpad,
        })
    } else {
        // Discrete scroll event (e.g. wheel clicks)
        let h_lines = if discrete_h != 0 { discrete_h as f32 } else { h as f32 / 10.0 };
        let v_lines = if discrete_v != 0 { discrete_v as f32 } else { v as f32 / 10.0 };
        MouseScrollDelta::LineDelta(-h_lines * factors.mouse as f32, -v_lines * factors.mouse as f32)
    };
    (phase, delta)
}

/// The CSD resize edge under (lx, ly), if the point is within the border.
fn csd_edge(lx: f32, ly: f32, size: LogicalSize) -> Option<ResizeEdge> {
    let b = CSD_BORDER;
    if ly < b {
        Some(if lx < b {
            ResizeEdge::TopLeft
        } else if lx > size.width - b {
            ResizeEdge::TopRight
        } else {
            ResizeEdge::Top
        })
    } else if ly > size.height - b {
        Some(if lx < b {
            ResizeEdge::BottomLeft
        } else if lx > size.width - b {
            ResizeEdge::BottomRight
        } else {
            ResizeEdge::Bottom
        })
    } else if lx < b {
        Some(ResizeEdge::Left)
    } else if lx > size.width - b {
        Some(ResizeEdge::Right)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    //! The routing, driven with no window system: a mock app records what
    //! reaches it.
    use super::*;
    use crate::backend::app::{AppSender, WindowSettings};

    #[derive(Debug, Clone, PartialEq)]
    enum Seen {
        Move(f32, f32),
        Button(MouseButton, ElementState, f32, f32),
        Wheel(MouseScrollDelta),
        Key(Key, bool),
        Focus(bool),
        Undo,
        Update(u32),
    }

    struct Mock {
        seen: Vec<Seen>,
        csd: bool,
        takes_undo: bool,
        /// A press makes this message, which `update` records.
        press_msg: Option<u32>,
    }

    impl Application for Mock {
        type Message = u32;
        fn create(_: AppSender<u32>) -> Self {
            unreachable!("built directly")
        }
        fn settings(&self) -> WindowSettings {
            WindowSettings {
                title: String::new(),
                app_id: "mock".into(),
                width: 400,
                height: 300,
                fullscreen: false,
                min_size: None,
            }
        }
        fn update(&mut self, msg: u32, needs_rebuild: &mut bool, _exit: &mut bool) {
            self.seen.push(Seen::Update(msg));
            *needs_rebuild = true;
        }
        fn tick(&mut self, _dt: f32, _needs_rebuild: &mut bool) {}
        fn handle_pointer_move(&mut self, pos: LogicalPosition, _: &mut bool) {
            self.seen.push(Seen::Move(pos.x, pos.y));
        }
        fn handle_mouse_input(
            &mut self,
            button: MouseButton,
            state: ElementState,
            pos: LogicalPosition,
            _: &mut bool,
        ) -> Option<u32> {
            self.seen.push(Seen::Button(button, state, pos.x, pos.y));
            if state == ElementState::Pressed { self.press_msg } else { None }
        }
        fn handle_mouse_wheel(&mut self, delta: &MouseScrollDelta, _: LogicalPosition, _: &mut bool) {
            self.seen.push(Seen::Wheel(delta.clone()));
        }
        fn handle_key_input(&mut self, event: &KeyEvent, _: &mut bool) -> Option<u32> {
            self.seen.push(Seen::Key(event.logical_key.clone(), event.repeat));
            None
        }
        fn handle_focus_change(&mut self, focused: bool, _: &mut bool) {
            self.seen.push(Seen::Focus(focused));
        }
        fn undo(&mut self, _: &mut bool) -> bool {
            self.seen.push(Seen::Undo);
            self.takes_undo
        }
        fn csd_resize_borders(&self) -> bool {
            self.csd
        }
        fn csd_titlebar_move(&self) -> bool {
            self.csd
        }
    }

    fn mock() -> Mock {
        Mock { seen: Vec::new(), csd: false, takes_undo: false, press_msg: None }
    }

    /// A driver with known chords, whatever the machine's input.kdl says.
    fn driver() -> Driver {
        Driver {
            undo_chord: "ctrl+z".into(),
            redo_chord: "ctrl+shift+z".into(),
            group_next_chord: "ctrl+tab".into(),
            group_prev_chord: "ctrl+shift+tab".into(),
            ..Driver::new()
        }
    }

    struct Flags {
        redraw: bool,
        exit: bool,
    }

    fn turn<'a>(app: &'a mut Mock, f: &'a mut Flags) -> Turn<'a, Mock> {
        Turn { app, redraw: &mut f.redraw, exit: &mut f.exit }
    }

    const SIZE: LogicalSize = LogicalSize { width: 400.0, height: 300.0 };

    fn site(can_grab: bool) -> PressSite {
        PressSite { size: SIZE, on_popup: false, can_grab }
    }

    #[test]
    fn a_lost_pointer_releases_what_was_held_then_clears_hover() {
        let (mut d, mut app, mut f) = (driver(), mock(), Flags { redraw: false, exit: false });
        let at = LogicalPosition::new(50.0, 60.0);
        d.cursor_pos = (50.0, 60.0);
        d.pointer_press(turn(&mut app, &mut f), MouseButton::Left, at, site(false));
        d.pointer_press(turn(&mut app, &mut f), MouseButton::Middle, at, site(false));
        assert_eq!(d.buttons_down, 1 | 4);
        app.seen.clear();

        d.pointer_leave(turn(&mut app, &mut f));
        assert_eq!(
            app.seen,
            vec![
                Seen::Button(MouseButton::Left, ElementState::Released, 50.0, 60.0),
                Seen::Button(MouseButton::Middle, ElementState::Released, 50.0, 60.0),
                Seen::Move(-10000.0, -10000.0),
            ]
        );
        assert_eq!(d.buttons_down, 0);
    }

    #[test]
    fn a_press_on_the_border_is_the_windows_only_when_the_shell_can_grab() {
        let (mut d, mut app, mut f) = (driver(), mock(), Flags { redraw: false, exit: false });
        app.csd = true;
        let corner = LogicalPosition::new(2.0, 2.0);
        let r = d.pointer_press(turn(&mut app, &mut f), MouseButton::Left, corner, site(true));
        assert_eq!(r, Press::Resize(ResizeEdge::TopLeft));
        assert!(app.seen.is_empty(), "a grab never reaches the app: {:?}", app.seen);

        let band = LogicalPosition::new(100.0, 20.0);
        let r = d.pointer_press(turn(&mut app, &mut f), MouseButton::Left, band, site(true));
        assert_eq!(r, Press::Move);

        // No grab to start (a layer surface, a shell without grabs): the app's.
        let r = d.pointer_press(turn(&mut app, &mut f), MouseButton::Left, corner, site(false));
        assert_eq!(r, Press::Dispatched);
        assert_eq!(app.seen, vec![Seen::Button(MouseButton::Left, ElementState::Pressed, 2.0, 2.0)]);

        // Nor is a press through the menu popup ever a grab.
        app.seen.clear();
        let popup = PressSite { on_popup: true, ..site(true) };
        assert_eq!(d.pointer_press(turn(&mut app, &mut f), MouseButton::Left, corner, popup), Press::Dispatched);
        assert_eq!(app.seen.len(), 1);
    }

    #[test]
    fn a_pressed_message_reaches_update_and_asks_for_a_frame() {
        let (mut d, mut app, mut f) = (driver(), mock(), Flags { redraw: false, exit: false });
        app.press_msg = Some(7);
        d.pointer_press(turn(&mut app, &mut f), MouseButton::Right, LogicalPosition::new(9.0, 9.0), site(true));
        assert_eq!(app.seen.last(), Some(&Seen::Update(7)));
        assert!(f.redraw);
    }

    #[test]
    fn the_undo_chord_goes_to_the_app_hook_before_key_dispatch() {
        let (mut d, mut app, mut f) = (driver(), mock(), Flags { redraw: false, exit: false });
        d.set_modifiers(&mut app, Modifiers { ctrl: true, ..Modifiers::default() });
        app.takes_undo = true;
        d.key(turn(&mut app, &mut f), Key::Character("z".into()), None, ElementState::Pressed);
        assert_eq!(app.seen, vec![Seen::Undo]);
        assert!(f.redraw);

        // Declined by the app, the key is dispatched as usual.
        app.seen.clear();
        app.takes_undo = false;
        d.key(turn(&mut app, &mut f), Key::Character("z".into()), None, ElementState::Pressed);
        assert_eq!(app.seen, vec![Seen::Undo, Seen::Key(Key::Character("z".into()), false)]);
    }

    #[test]
    fn a_held_key_repeats_after_the_delay_and_stops_on_release() {
        let (mut d, mut app, mut f) = (driver(), mock(), Flags { redraw: false, exit: false });
        let bs = Key::Named(NamedKey::Backspace);
        d.key(turn(&mut app, &mut f), bs.clone(), None, ElementState::Pressed);
        app.seen.clear();

        // Inside the delay: nothing.
        d.repeat_keys(turn(&mut app, &mut f));
        assert!(app.seen.is_empty());

        // Past it: one repeat per turn that is an interval on.
        let pk = d.pressed_key.as_mut().unwrap();
        pk.first_pressed -= KEY_REPEAT_DELAY;
        pk.last_repeated -= KEY_REPEAT_INTERVAL;
        d.repeat_keys(turn(&mut app, &mut f));
        d.repeat_keys(turn(&mut app, &mut f));
        assert_eq!(app.seen, vec![Seen::Key(bs.clone(), true)]);

        d.key(turn(&mut app, &mut f), bs, None, ElementState::Released);
        assert!(d.pressed_key.is_none());
    }

    #[test]
    fn losing_focus_drops_the_held_key_and_modifiers() {
        let (mut d, mut app, mut f) = (driver(), mock(), Flags { redraw: false, exit: false });
        d.set_modifiers(&mut app, Modifiers { ctrl: true, shift: true, alt: true, logo: true });
        d.key(turn(&mut app, &mut f), Key::Character("a".into()), Some("a".into()), ElementState::Pressed);
        assert!(d.pressed_key.is_some());

        d.keyboard_focus(turn(&mut app, &mut f), false);
        assert!(d.pressed_key.is_none());
        // Logo is left as it was, as the runner always left it.
        assert_eq!(d.mods, Modifiers { ctrl: false, shift: false, alt: false, logo: true });
        assert_eq!(app.seen.last(), Some(&Seen::Focus(false)));
    }

    #[test]
    fn a_scroll_frame_is_its_phase_and_delta() {
        // The pure half of `scroll`: the dispatch itself publishes the phase
        // process-wide, which a parallel suite must not race.
        use crate::input::ScrollFactors;
        use crate::widget::ScrollPhase;
        let unit = ScrollFactors { mouse: 1.0, trackpad: 1.0 };
        let notch = ScrollFrame { v: 15.0, discrete_v: 1, source: Some(ScrollSource::Wheel), ..ScrollFrame::default() };
        assert_eq!(scroll_delta(&notch, unit), (ScrollPhase::Wheel, MouseScrollDelta::LineDelta(-0.0, -1.0)));

        let finger = ScrollFrame { v: 4.0, source: Some(ScrollSource::Finger), ..ScrollFrame::default() };
        assert_eq!(
            scroll_delta(&finger, ScrollFactors { mouse: 1.0, trackpad: 2.0 }),
            (ScrollPhase::Finger, MouseScrollDelta::PixelDelta(Position { x: -0.0, y: -8.0 }))
        );
        // No source named is a finger too; a lift with no delta ends it.
        let unnamed = ScrollFrame { h: 3.0, ..ScrollFrame::default() };
        assert_eq!(scroll_delta(&unnamed, unit).0, ScrollPhase::Finger);
        let lift = ScrollFrame { stop: true, source: Some(ScrollSource::Finger), ..ScrollFrame::default() };
        assert_eq!(scroll_delta(&lift, unit).0, ScrollPhase::FingerEnd);
        // A tilt wheel, or anything newer, glides like a wheel.
        let tilt = ScrollFrame { h: 5.0, source: Some(ScrollSource::WheelTilt), ..ScrollFrame::default() };
        assert_eq!(scroll_delta(&tilt, unit).0, ScrollPhase::Wheel);
    }
}
