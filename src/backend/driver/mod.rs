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
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the vocabulary a shell speaks (`Turn`, `Modifiers`, `PressSite`, `ScrollFrame`, …), the `Driver`, its tick, and the small helpers (CSD edges, scroll deltas, popover close) |
//! | `pointer` | the cursor at a point, the pointer entering, moving, leaving, pressing and releasing |
//! | `scroll` | wheel frames, touch (a finger as a click, a scroll or a held button), the pinch fallback |
//! | `keys` | modifiers and keyboard focus, keys, committed and composed text, key repeat, the Tab and undo/redo chords |

mod keys;
mod pointer;
mod scroll;
#[cfg(test)]
mod tests;

use web_time::Instant;

use super::app::{Application, LogicalPosition, LogicalSize};
use cursor_icon::CursorIcon;
use crate::widget::{ElementState, Key, KeyEvent, MouseButton, MouseScrollDelta, NamedKey, Position, ScrollPhase, WidgetHostExt};

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
    matches!(
        key,
        Key::Named(NamedKey::Backspace)
            | Key::Named(NamedKey::Delete)
            | Key::Named(NamedKey::ArrowLeft)
            | Key::Named(NamedKey::ArrowRight)
            | Key::Named(NamedKey::ArrowUp)
            | Key::Named(NamedKey::ArrowDown)
            | Key::Named(NamedKey::Home)
            | Key::Named(NamedKey::End)
            | Key::Character(_)
    )
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
    /// The window system resizes the window from edges of its own (AppKit's
    /// window frame), and offers no way to start a resize from a press: the
    /// CSD resize band is then no grab, and a press there goes on to the
    /// move checks and the app.
    pub own_edges: bool,
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
    /// When the last input event of any kind arrived — what makes a redraw
    /// interactive, which is the redraw the runner's warm-down is for
    /// (`shell::Pacer`).
    pub last_input: Option<Instant>,
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

/// Close the open popovers a left press at (lx, ly) misses, BEFORE the app's
/// dispatch: apps commonly region-gate their routing, so an open menu's owner
/// may never hear about a press elsewhere.
fn close_popovers_missed_by<A: Application>(app: &mut A, lx: f32, ly: f32) {
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
            last_input: None,
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

    /// The pointer entered at `pos`. Enter carries the pointer's position but
    /// no motion follows until it actually moves — without this the app's
    /// hover state is stale from enter to first move, and a press in that
    /// window can misroute (e.g. a divider press falling through to the
    /// movable-root plate window drag).
    fn note_input(&mut self) {
        self.last_input = Some(Instant::now());
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
