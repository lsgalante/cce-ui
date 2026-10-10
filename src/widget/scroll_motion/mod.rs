//! Smooth scrolling — the one place wheel/trackpad deltas turn into an
//! animated scroll offset, shared by every scrolling widget and available to
//! apps that own their offsets themselves.
//!
//! Three input regimes, decided by [`ScrollPhase`] (which the runner sets from
//! the Wayland `axis_source` / `axis_stop` events before each dispatch):
//!
//! - **Wheel** (discrete clicks, `LineDelta`): each notch moves the *target*;
//!   the drawn offset eases toward it with a frame-rate-independent
//!   exponential approach. Rapid notches accumulate into one glide instead of
//!   a staircase.
//! - **Finger** (trackpad, `PixelDelta` with a finger/continuous source): the
//!   offset follows the gesture 1:1 — nothing is smoother than the hand — while
//!   a velocity estimate is kept.
//! - **FingerEnd** (`axis_stop`, the finger lift): the estimated velocity
//!   carries the offset on, decaying under friction, so a flick coasts.
//!
//! The model is one [`ScrollAxis`] per direction, paired as a
//! [`ScrollMotion`]. A host keeps its existing `scroll_y: f32` field as the
//! *drawn* offset and lets the motion drive it: feed events with
//! [`ScrollMotion::apply`], advance with [`ScrollMotion::tick`] once per
//! frame, and copy [`ScrollAxis::pos`] out. Hosts that also write the field
//! directly (drag, keyboard, auto-snap to a selection) call
//! [`ScrollMotion::reconcile`] first so the motion adopts the external write
//! instead of fighting it.
//!
//! Everything is pure time-based math — no clock, GPU, or loop — except the
//! finger-velocity estimate, which timestamps events with `Instant`.
//!
//! Tunables come from `input.kdl` (`<app>` domain, then `cce-ui`):
//!
//! ```text
//! cce-ui {
//!     input {
//!         smooth_scroll true      // wheel notches glide (false = instant)
//!         scroll_ease 12.0        // wheel glide rate, 1/s (higher = snappier)
//!         kinetic_scroll true     // trackpad flicks coast after the lift
//!         scroll_friction 6.0     // coast decay, 1/s (higher = shorter coast)
//!     }
//! }
//! ```
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the scroll phase the runner publishes, the shared constants, `Bounds` |
//! | `settings` | `ScrollSettings` from `input.kdl`, the animations switch, and a test's per-thread pin |
//! | `axis` | `ScrollAxis`: one direction's wheel glide, finger tracking, coast and settling |
//! | `motion` | `ScrollMotion`: the two axes, and the event entry points (`apply`, `apply_px`, `apply_phase`, `tick`) |

mod axis;
mod motion;
mod settings;
#[cfg(test)]
mod tests;

pub use axis::*;
pub use motion::*;
pub use settings::*;


use web_time::Instant;

use crate::widget::MouseScrollDelta;


/// Which stage of a scroll gesture the current wheel event belongs to. The
/// runner sets this from the Wayland axis source/stop before dispatching;
/// consumers read it through [`current_scroll_phase`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollPhase {
    /// A discrete wheel click (or a synthesized delta with no gesture).
    Wheel,
    /// Continuous finger/trackpad motion; the gesture is still in progress.
    Finger,
    /// The finger lifted (`axis_stop`) — the event carries no delta.
    FingerEnd,
}

/// Publish the phase of the wheel event about to be dispatched, for the window being run
/// (`crate::window_state`; a thread's own with none entered). Runner-side. It was one
/// process-wide atomic until 2026-10-08, which a test setting it changed for every test
/// running beside it.
pub fn set_scroll_phase(phase: ScrollPhase) {
    crate::window_state::with(|w| w.scroll_phase.set(phase));
}

/// The phase of the wheel event currently being dispatched. Outside a
/// dispatch it reports the last one, which only matters for hosts that
/// synthesize their own wheel events (they get `Wheel` semantics unless a
/// real gesture is mid-flight).
pub fn current_scroll_phase() -> ScrollPhase {
    crate::window_state::with(|w| w.scroll_phase.get())
}

/// Pixels one wheel notch moves a list — the toolkit's line unit, shared so
/// every scrolling host steps the same distance per click.
pub const LINE_PX: f32 = 24.0;

/// Exponential-approach convergence: the drawn offset snaps to its target
/// once within this many pixels.
const SNAP_PX: f32 = 0.5;

/// A coast below this speed (px/s) stops.
const COAST_STOP_SPEED: f32 = 5.0;

/// A finger held still this long (seconds) before lifting yields no fling.
const FLING_STALE_S: f32 = 0.08;

/// Velocity-estimate blend per finger event (new sample weight).
const VEL_BLEND: f32 = 0.35;

/// The range an axis may occupy. Lists are `0..=max_scroll`; a canvas that
/// pans freely is [`Bounds::UNBOUNDED`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub lo: f32,
    pub hi: f32,
}

impl Bounds {
    pub const UNBOUNDED: Bounds = Bounds { lo: f32::NEG_INFINITY, hi: f32::INFINITY };

    /// `0..=max`, with a negative `max` (content shorter than the viewport)
    /// collapsing to `0..=0`.
    pub fn max(max: f32) -> Bounds {
        Bounds { lo: 0.0, hi: max.max(0.0) }
    }

    pub fn clamp(&self, v: f32) -> f32 {
        v.clamp(self.lo, self.hi)
    }
}
