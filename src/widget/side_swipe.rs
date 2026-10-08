//! A two-finger swipe TO THE SIDE, read as a page turn: forward into what a
//! row leads to, or back to where the plate was before.
//!
//! The runner delivers a trackpad's two fingers as wheel events with a pixel
//! delta, and a mouse's tilt wheel as horizontal notches. A plate that turns
//! pages (a context menu's page rows, a dialog opened from one) feeds every
//! wheel event it takes to [`feed`] (a [`SideSwipe`]), which answers once per gesture:
//! nothing for a vertical scroll or a small sideways drift, and one
//! [`SwipeDir`] when the fingers have travelled far enough to the side to
//! mean it. The rest of that gesture turns nothing, so one swipe is one page.
//!
//! **The direction follows the content**, as a horizontal list does: the
//! delta that would scroll a list to show what is to its RIGHT is
//! [`SwipeDir::Forward`] — the page a row leads to comes in from the right,
//! the side its `›` points to — and the other way is [`SwipeDir::Back`]. The
//! compositor applies natural scrolling before the delta reaches the app, so
//! under natural scrolling forward is the fingers going LEFT (the page is
//! pushed aside) and back is the fingers going right, as on every touch
//! surface; under traditional scrolling the two swap with the user's own
//! setting, and nothing here asks which it is.
//!
//! **One gesture is one turn across every plate**: the plate a swipe turns
//! is replaced by another under the same fingers, and a recognizer of its
//! own would see the rest of the gesture as a new one and turn again. So
//! the plates of a thread share one, [`feed`], which only the lift (or a
//! pause) readies for the next turn.

use crate::widget::scroll_motion::{current_scroll_phase, ScrollPhase};
use crate::widget::MouseScrollDelta;
use std::time::Duration;
use web_time::Instant;

/// How far the fingers travel to the side, in the runner's pixel delta,
/// before a swipe is a page turn. A deliberate flick covers this in two or
/// three events; a vertical scroll's sideways wander does not reach it.
pub const SWIPE_PX: f32 = 40.0;
/// How much more sideways than vertical a swipe has to be.
pub const SWIPE_SLOPE: f32 = 1.5;
/// A pause this long between wheel events ends a gesture, for a source that
/// sends no finger lift (a tilt wheel, a trackpad whose stop was dropped).
pub const SWIPE_GAP: Duration = Duration::from_millis(250);

/// Which way a side swipe turns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwipeDir {
    /// Into what the row under the pointer leads to.
    Forward,
    /// Back to the plate this one was turned to from.
    Back,
}

/// One gesture's sideways travel. See the module docs.
#[derive(Debug, Clone, Default)]
pub struct SideSwipe {
    dx: f32,
    dy: f32,
    /// This gesture has turned a page already.
    fired: bool,
    last: Option<Instant>,
}

impl SideSwipe {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed a wheel event in the phase the runner published for it.
    pub fn feed(&mut self, delta: &MouseScrollDelta) -> Option<SwipeDir> {
        self.feed_at(delta, current_scroll_phase(), Instant::now())
    }

    /// [`Self::feed`] in a phase and at a time the caller names — the tests',
    /// which run in parallel and cannot share the runner's phase.
    pub fn feed_at(&mut self, delta: &MouseScrollDelta, phase: ScrollPhase, now: Instant) -> Option<SwipeDir> {
        if phase == ScrollPhase::FingerEnd {
            self.reset();
            return None;
        }
        if self.last.is_some_and(|t| now.duration_since(t) > SWIPE_GAP) {
            self.reset();
        }
        self.last = Some(now);
        let (x, y, whole) = match delta {
            MouseScrollDelta::LineDelta(x, y) => (*x, *y, true),
            MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32, false),
        };
        self.dx += x;
        self.dy += y;
        if self.fired {
            return None;
        }
        // A notch of a tilt wheel is a whole swipe; fingers have to travel.
        let far = if whole { self.dx.abs() >= 1.0 } else { self.dx.abs() >= SWIPE_PX };
        if !far || self.dx.abs() < SWIPE_SLOPE * self.dy.abs() {
            return None;
        }
        self.fired = true;
        // A negative x scrolls a list to show what is to its right.
        Some(if self.dx < 0.0 { SwipeDir::Forward } else { SwipeDir::Back })
    }

    /// Forget the gesture in progress.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// The current window's recognizer (`crate::window_state`), one per window.
fn shared<R>(f: impl FnOnce(&std::cell::RefCell<SideSwipe>) -> R) -> R {
    crate::window_state::with(|w| f(&w.swipe))
}

/// Feed a wheel event to the current window's recognizer (`crate::window_state`) — see the
/// module docs.
pub fn feed(delta: &MouseScrollDelta) -> Option<SwipeDir> {
    shared(|s| s.borrow_mut().feed(delta))
}

/// Whether `delta` continues a gesture that has turned a page already. The
/// rest of that gesture is the turn's: the plate it turned into may be
/// smaller than the one it replaced, and what is under the fingers then is a
/// list to scroll or a scene to orbit, which would take it as theirs. A host
/// asks this ahead of its own wheel handling and drops the event on `true`;
/// the event is fed, so the lift or a pause still ends the gesture.
pub fn swallow(delta: &MouseScrollDelta) -> bool {
    swallow_at(delta, current_scroll_phase(), Instant::now())
}

fn swallow_at(delta: &MouseScrollDelta, phase: ScrollPhase, now: Instant) -> bool {
    shared(|s| {
        let mut s = s.borrow_mut();
        let live = s.fired && phase != ScrollPhase::FingerEnd && s.last.is_some_and(|t| now.duration_since(t) <= SWIPE_GAP);
        if live {
            s.feed_at(delta, phase, now);
        }
        live
    })
}

/// End the gesture in progress, as the fingers lifting does: what a test
/// that swipes twice calls between, since the runner's phase is one value
/// for the whole process and not the test's to set.
pub fn end_gesture() {
    shared(|s| s.borrow_mut().reset());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::Position;

    fn px(x: f64, y: f64) -> MouseScrollDelta {
        MouseScrollDelta::PixelDelta(Position { x, y })
    }

    /// Fingers that travel far enough to the side turn one page, the way the
    /// content goes; the rest of the gesture turns nothing, and the lift
    /// readies the next.
    #[test]
    fn a_side_swipe_turns_one_page_per_gesture() {
        let mut s = SideSwipe::new();
        let t = Instant::now();
        let f = ScrollPhase::Finger;
        assert_eq!(s.feed_at(&px(-15.0, 1.0), f, t), None, "not far enough yet");
        assert_eq!(s.feed_at(&px(-15.0, 1.0), f, t), None);
        assert_eq!(s.feed_at(&px(-15.0, 2.0), f, t), Some(SwipeDir::Forward));
        assert_eq!(s.feed_at(&px(-80.0, 0.0), f, t), None, "one turn a gesture");
        assert_eq!(s.feed_at(&px(0.0, 0.0), ScrollPhase::FingerEnd, t), None);
        assert_eq!(s.feed_at(&px(60.0, 0.0), f, t), Some(SwipeDir::Back));
    }

    /// What is left of a gesture after it turned is swallowed, until the
    /// lift; before it turns, nothing is.
    #[test]
    fn the_rest_of_a_turning_gesture_is_swallowed() {
        end_gesture();
        let t = Instant::now();
        let f = ScrollPhase::Finger;
        assert!(!swallow_at(&px(-30.0, 0.0), f, t), "nothing has turned");
        shared(|s| s.borrow_mut().feed_at(&px(-50.0, 0.0), f, t));
        assert!(swallow_at(&px(-30.0, 0.0), f, t), "the rest of the turn");
        assert!(!swallow_at(&px(0.0, 0.0), ScrollPhase::FingerEnd, t), "the lift is not");
        shared(|s| s.borrow_mut().feed_at(&px(0.0, 0.0), ScrollPhase::FingerEnd, t));
        assert!(!swallow_at(&px(-30.0, 0.0), f, t), "a new gesture");
        end_gesture();
    }

    /// A scroll that is mostly vertical never turns, however far it drifts.
    #[test]
    fn a_vertical_scroll_is_not_a_swipe() {
        let mut s = SideSwipe::new();
        let t = Instant::now();
        for _ in 0..20 {
            assert_eq!(s.feed_at(&px(-10.0, 30.0), ScrollPhase::Finger, t), None);
        }
    }

    /// A tilt-wheel notch is a swipe; notches in a burst are one, and a
    /// pause readies the next.
    #[test]
    fn a_tilt_wheel_notch_turns_and_a_pause_ends_the_gesture() {
        let mut s = SideSwipe::new();
        let t = Instant::now();
        let w = ScrollPhase::Wheel;
        let notch = MouseScrollDelta::LineDelta(-1.0, 0.0);
        assert_eq!(s.feed_at(&notch, w, t), Some(SwipeDir::Forward));
        assert_eq!(s.feed_at(&notch, w, t + Duration::from_millis(50)), None);
        let later = t + Duration::from_millis(50) + SWIPE_GAP + Duration::from_millis(1);
        assert_eq!(s.feed_at(&MouseScrollDelta::LineDelta(1.0, 0.0), w, later), Some(SwipeDir::Back));
    }
}
