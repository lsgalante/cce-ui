// Touchscreen input (wl_touch).
//
// A cce-ui app has no touch widgets: everything it knows is a pointer that
// hovers, presses, drags and scrolls. So a finger is translated into those,
// and which one it becomes is decided by what the finger does, the way every
// touch surface decides it:
//
// - a TAP (down and up without leaving the slop) is a click where it landed;
// - a finger that MOVES past the slop scrolls whatever is under it, content
//   following the finger, and the lift is a trackpad lift — a flick coasts
//   through the same `ScrollMotion` a two-finger trackpad swipe does;
// - a finger HELD still for `HOLD_MS` and then moved is a held left button:
//   a slider's thumb, a text selection, a drag-and-drop source.
//
// No timer is needed for the hold: nothing is sent while the finger rests
// inside the slop, so the decision is only taken at the first motion past
// it, from how long the finger had been down by then.
//
// Only the first finger is followed; others are ignored until it lifts.
// Before this module the compositor drove every cce-ui window by touch with
// an emulated pointer, so a finger drag was a held button and selected
// rather than scrolled. Binding wl_touch is what moves cce-ui windows onto
// the compositor's touch route (cce-compositor's `cursor::TouchRoute`).
//
// Not here: window moves and CSD resizes by finger. A touch serial does not
// satisfy the compositor's pointer-grab check, so `xdg_toplevel.move` from a
// touch would be refused. Overview and the Super-held adjust mode, where the
// compositor drives the pointer itself, move windows by finger instead.

use smithay_client_toolkit::reexports::client::protocol::{wl_surface::WlSurface, wl_touch::WlTouch};
use smithay_client_toolkit::reexports::client::{Connection, QueueHandle};
use smithay_client_toolkit::seat::touch::TouchHandler;

use super::window_runner::{Application, EngineState, LogicalPosition};
use crate::widget::{ElementState, MouseButton, MouseScrollDelta, Position, ScrollPhase};

/// Travel (logical px) a finger may wander and still be a tap or a hold.
pub const SLOP: f32 = 10.0;
/// How long a finger must rest before moving for the motion to be a drag
/// rather than a scroll.
pub const HOLD_MS: u32 = 400;

/// What a touch asks of the app, in pointer terms.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TouchAction {
    /// Move the hover to here (the finger arrived; nothing pressed yet).
    Hover(f32, f32),
    Press(f32, f32),
    /// Motion with the button held.
    Drag(f32, f32),
    Release(f32, f32),
    /// Scroll by this much finger travel, content following the finger.
    Scroll(f32, f32),
    /// The scrolling finger lifted: may coast.
    ScrollEnd,
    /// The finger is gone; no hover is left behind.
    Leave,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    /// Down, inside the slop: still a tap or a hold.
    Pending,
    Scrolling,
    Dragging,
}

#[derive(Debug, Clone, Copy)]
struct Finger {
    id: i32,
    phase: Phase,
    start: (f32, f32),
    last: (f32, f32),
    down_ms: u32,
}

/// The gesture of the one finger being followed. Pure: it maps touch events
/// to `TouchAction`s and holds no Wayland state, so it is tested directly.
#[derive(Debug, Default)]
pub struct TouchTracker {
    finger: Option<Finger>,
}

impl TouchTracker {
    pub fn down(&mut self, id: i32, x: f32, y: f32, time_ms: u32) -> Vec<TouchAction> {
        if self.finger.is_some() {
            return Vec::new();
        }
        self.finger = Some(Finger { id, phase: Phase::Pending, start: (x, y), last: (x, y), down_ms: time_ms });
        vec![TouchAction::Hover(x, y)]
    }

    pub fn motion(&mut self, id: i32, x: f32, y: f32, time_ms: u32) -> Vec<TouchAction> {
        let Some(f) = self.finger.as_mut().filter(|f| f.id == id) else { return Vec::new() };
        let prev = f.last;
        f.last = (x, y);
        match f.phase {
            Phase::Pending => {
                let (dx, dy) = (x - f.start.0, y - f.start.1);
                if dx.hypot(dy) < SLOP {
                    return Vec::new();
                }
                if time_ms.wrapping_sub(f.down_ms) >= HOLD_MS {
                    f.phase = Phase::Dragging;
                    vec![TouchAction::Press(f.start.0, f.start.1), TouchAction::Drag(x, y)]
                } else {
                    // The whole travel from the down point, not just past the
                    // slop: the content stays under the finger.
                    f.phase = Phase::Scrolling;
                    vec![TouchAction::Scroll(dx, dy)]
                }
            }
            Phase::Scrolling => {
                let (dx, dy) = (x - prev.0, y - prev.1);
                if dx == 0.0 && dy == 0.0 {
                    Vec::new()
                } else {
                    vec![TouchAction::Scroll(dx, dy)]
                }
            }
            Phase::Dragging => vec![TouchAction::Drag(x, y)],
        }
    }

    pub fn up(&mut self, id: i32) -> Vec<TouchAction> {
        if self.finger.map_or(true, |f| f.id != id) {
            return Vec::new();
        }
        let f = self.finger.take().unwrap();
        match f.phase {
            Phase::Pending => vec![
                TouchAction::Press(f.start.0, f.start.1),
                TouchAction::Release(f.start.0, f.start.1),
                TouchAction::Leave,
            ],
            Phase::Scrolling => vec![TouchAction::ScrollEnd, TouchAction::Leave],
            Phase::Dragging => vec![TouchAction::Release(f.last.0, f.last.1), TouchAction::Leave],
        }
    }

    /// The compositor took the sequence back (`wl_touch.cancel`). A tap that
    /// never became anything clicks nothing; a held button is released where
    /// it is, as a pointer leaving mid-drag is (`PointerEventKind::Leave`).
    pub fn cancel(&mut self) -> Vec<TouchAction> {
        let Some(f) = self.finger.take() else { return Vec::new() };
        match f.phase {
            Phase::Pending => vec![TouchAction::Leave],
            Phase::Scrolling => vec![TouchAction::ScrollEnd, TouchAction::Leave],
            Phase::Dragging => vec![TouchAction::Release(f.last.0, f.last.1), TouchAction::Leave],
        }
    }

    pub fn id(&self) -> Option<i32> {
        self.finger.map(|f| f.id)
    }
}

impl<A: Application> EngineState<A> {
    /// A surface-local touch position in the app's window coordinates: the
    /// forced-scale divide and the menu popup's offset, as the pointer path
    /// applies them.
    fn touch_pos(&self, (x, y): (f64, f64)) -> (f32, f32) {
        let forced = crate::scale::forced_scale().unwrap_or(1.0);
        let (ox, oy) = self.touch_offset;
        (x as f32 / forced + ox, y as f32 / forced + oy)
    }

    /// The seat stopped offering touch: end the finger's gesture as a
    /// cancel would, and drop the object.
    pub(crate) fn touch_lost(&mut self) {
        if let Some(touch) = self.touch.take() {
            touch.release();
        }
        self.cancel_touch();
    }

    fn cancel_touch(&mut self) {
        let actions = self.touch_tracker.cancel();
        self.run_touch_actions(actions);
        self.touch_scroll_at = None;
    }

    fn run_touch_actions(&mut self, actions: Vec<TouchAction>) {
        for action in actions {
            let mut rebuild = false;
            match action {
                TouchAction::Hover(x, y) | TouchAction::Drag(x, y) => {
                    self.inner.as_mut().unwrap().handle_pointer_move(LogicalPosition::new(x, y), &mut rebuild);
                }
                TouchAction::Leave => {
                    self.inner.as_mut().unwrap().handle_pointer_move(LogicalPosition::new(-10000.0, -10000.0), &mut rebuild);
                }
                TouchAction::Press(x, y) => {
                    // Outside-press close for open popovers, as the pointer's
                    // press does before the app's own dispatch.
                    let app = self.inner.as_mut().unwrap();
                    let offsets: Vec<_> = app
                        .ui_context()
                        .map(|ctx| ctx.popover_owners())
                        .unwrap_or_default()
                        .into_iter()
                        .map(|id| (id, app.popover_offset(id)))
                        .collect();
                    if let Some(ctx) = app.ui_context_mut() {
                        ctx.close_popovers_missed_by_press_with(x, y, |id| {
                            offsets.iter().find(|(o, _)| *o == id).map_or((0.0, 0.0), |&(_, d)| d)
                        });
                    }
                    self.touch_button(ElementState::Pressed, x, y, &mut rebuild);
                }
                TouchAction::Release(x, y) => self.touch_button(ElementState::Released, x, y, &mut rebuild),
                TouchAction::Scroll(dx, dy) => {
                    self.touch_wheel(ScrollPhase::Finger, dx, dy, &mut rebuild);
                }
                TouchAction::ScrollEnd => self.touch_wheel(ScrollPhase::FingerEnd, 0.0, 0.0, &mut rebuild),
            }
            if rebuild {
                self.redraw = true;
            }
        }
        // A press may queue an app-driven window move; it cannot be honoured
        // from a touch (see the module comment), and left queued it would
        // run on the next pointer press with that press's serial.
        let _ = self.inner.as_mut().unwrap().take_window_action();
    }

    fn touch_button(&mut self, state: ElementState, x: f32, y: f32, rebuild: &mut bool) {
        let app = self.inner.as_mut().unwrap();
        if let Some(msg) = app.handle_mouse_input(MouseButton::Left, state, LogicalPosition::new(x, y), rebuild) {
            let mut update_rebuild = false;
            app.update(msg, &mut update_rebuild, &mut self.exit);
            *rebuild |= update_rebuild;
        }
    }

    /// Finger travel as a trackpad pixel scroll. A finger is always
    /// "natural" — the content goes where it is pushed — so the dispatch runs
    /// with natural scrolling on whatever the trackpad's setting, and a value
    /// control (`MouseScrollDelta::value_notches_y`) reads the finger's real
    /// direction. 1:1, without the trackpad's per-app factor: the content
    /// stays under the finger.
    fn touch_wheel(&mut self, phase: ScrollPhase, dx: f32, dy: f32, rebuild: &mut bool) {
        let Some((x, y)) = self.touch_scroll_at else { return };
        crate::widget::scroll_motion::set_scroll_phase(phase);
        let delta = MouseScrollDelta::PixelDelta(Position { x: dx as f64, y: dy as f64 });
        if crate::scroll_debug() {
            eprintln!("[scroll] touch: phase={phase:?} -> {delta:?} at ({x:.0},{y:.0})");
        }
        if let Some(ctx) = self.inner.as_mut().unwrap().ui_context_mut() {
            ctx.ctrl_pressed = self.ctrl_pressed;
            ctx.shift_pressed = self.shift_pressed;
            ctx.alt_pressed = self.alt_pressed;
            ctx.logo_pressed = self.logo_pressed;
        }
        let app = self.inner.as_mut().unwrap();
        crate::input::with_natural_scroll(true, || {
            app.handle_mouse_wheel(&delta, LogicalPosition::new(x, y), rebuild);
        });
    }
}

impl<A: Application> TouchHandler for EngineState<A> {
    fn down(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _touch: &WlTouch,
        _serial: u32,
        time: u32,
        surface: WlSurface,
        id: i32,
        position: (f64, f64),
    ) {
        if self.touch_tracker.id().is_some() {
            return;
        }
        // An event on the context menu's popup surface is the app's too, at
        // the popup's offset from the window; fixed for the finger's life,
        // since every later event of it is about the same surface.
        self.touch_offset = self.menu_popup_offset(&surface).unwrap_or((0.0, 0.0));
        let (x, y) = self.touch_pos(position);
        // A scroll is dispatched where the finger went down: the gesture
        // belongs to what it began on, as a trackpad scroll does
        // (`scroll_initiate_widget_id`), however far the content moves.
        self.touch_scroll_at = Some((x, y));
        let actions = self.touch_tracker.down(id, x, y, time);
        self.run_touch_actions(actions);
    }

    fn up(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _touch: &WlTouch, _serial: u32, _time: u32, id: i32) {
        let actions = self.touch_tracker.up(id);
        self.run_touch_actions(actions);
        if self.touch_tracker.id().is_none() {
            self.touch_scroll_at = None;
        }
    }

    fn motion(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _touch: &WlTouch, time: u32, id: i32, position: (f64, f64)) {
        if self.touch_tracker.id() != Some(id) {
            return;
        }
        let (x, y) = self.touch_pos(position);
        let actions = self.touch_tracker.motion(id, x, y, time);
        self.run_touch_actions(actions);
    }

    fn shape(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlTouch, _: i32, _: f64, _: f64) {}

    fn orientation(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlTouch, _: i32, _: f64) {}

    fn cancel(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _touch: &WlTouch) {
        self.cancel_touch();
    }
}

smithay_client_toolkit::delegate_touch!(@<A: Application> EngineState<A>);

#[cfg(test)]
mod tests {
    use super::*;
    use TouchAction::*;

    #[test]
    fn a_tap_clicks_where_it_landed() {
        let mut t = TouchTracker::default();
        assert_eq!(t.down(1, 50.0, 60.0, 1000), vec![Hover(50.0, 60.0)]);
        // Jitter inside the slop is nothing yet.
        assert!(t.motion(1, 53.0, 62.0, 1050).is_empty());
        assert_eq!(t.up(1), vec![Press(50.0, 60.0), Release(50.0, 60.0), Leave]);
        assert_eq!(t.id(), None);
    }

    #[test]
    fn a_quick_drag_scrolls_with_the_finger_and_coasts_on_the_lift() {
        let mut t = TouchTracker::default();
        t.down(1, 100.0, 300.0, 0);
        // Past the slop: the whole travel from the down point.
        assert_eq!(t.motion(1, 100.0, 280.0, 80), vec![Scroll(0.0, -20.0)]);
        assert_eq!(t.motion(1, 95.0, 250.0, 100), vec![Scroll(-5.0, -30.0)]);
        // A repeated position is not an empty scroll.
        assert!(t.motion(1, 95.0, 250.0, 110).is_empty());
        assert_eq!(t.up(1), vec![ScrollEnd, Leave]);
    }

    #[test]
    fn a_hold_then_move_is_a_held_button_drag() {
        let mut t = TouchTracker::default();
        t.down(1, 10.0, 10.0, 5000);
        assert!(t.motion(1, 12.0, 10.0, 5200).is_empty());
        assert_eq!(t.motion(1, 40.0, 10.0, 5000 + HOLD_MS), vec![Press(10.0, 10.0), Drag(40.0, 10.0)]);
        assert_eq!(t.motion(1, 60.0, 15.0, 5600), vec![Drag(60.0, 15.0)]);
        assert_eq!(t.up(1), vec![Release(60.0, 15.0), Leave]);
    }

    #[test]
    fn the_hold_survives_the_clock_wrapping() {
        let mut t = TouchTracker::default();
        t.down(1, 0.0, 0.0, u32::MAX - 100);
        assert_eq!(t.motion(1, 30.0, 0.0, HOLD_MS)[0], Press(0.0, 0.0));
    }

    #[test]
    fn only_the_first_finger_is_followed() {
        let mut t = TouchTracker::default();
        t.down(1, 0.0, 0.0, 0);
        assert!(t.down(2, 100.0, 100.0, 10).is_empty());
        assert!(t.motion(2, 300.0, 300.0, 20).is_empty());
        assert!(t.up(2).is_empty());
        assert_eq!(t.id(), Some(1));
        // Once it lifts, a new finger is followed.
        t.up(1);
        assert_eq!(t.down(2, 5.0, 5.0, 30), vec![Hover(5.0, 5.0)]);
    }

    #[test]
    fn a_cancel_clicks_nothing_and_releases_a_held_button() {
        let mut t = TouchTracker::default();
        t.down(1, 0.0, 0.0, 0);
        assert_eq!(t.cancel(), vec![Leave]);

        t.down(1, 0.0, 0.0, 0);
        t.motion(1, 0.0, 50.0, 10);
        assert_eq!(t.cancel(), vec![ScrollEnd, Leave]);

        t.down(1, 0.0, 0.0, 0);
        t.motion(1, 50.0, 0.0, HOLD_MS);
        assert_eq!(t.cancel(), vec![Release(50.0, 0.0), Leave]);
        assert!(t.cancel().is_empty());
    }
}
