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

use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Instant;

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

static PHASE: AtomicU8 = AtomicU8::new(0);

/// Publish the phase of the wheel event about to be dispatched. Runner-side.
pub fn set_scroll_phase(phase: ScrollPhase) {
    PHASE.store(phase as u8, Ordering::Relaxed);
}

/// The phase of the wheel event currently being dispatched. Outside a
/// dispatch it reports the last one, which only matters for hosts that
/// synthesize their own wheel events (they get `Wheel` semantics unless a
/// real gesture is mid-flight).
pub fn current_scroll_phase() -> ScrollPhase {
    match PHASE.load(Ordering::Relaxed) {
        1 => ScrollPhase::Finger,
        2 => ScrollPhase::FingerEnd,
        _ => ScrollPhase::Wheel,
    }
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

/// The process-wide smooth-scroll tunables, resolved once from `input.kdl`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollSettings {
    /// Wheel notches glide toward their target (false = the legacy jump).
    pub smooth: bool,
    /// Wheel glide rate, 1/s. 12 reaches 95% of a notch in ~250ms.
    pub ease_rate: f32,
    /// Trackpad flicks coast after the lift.
    pub kinetic: bool,
    /// Coast decay, 1/s. 6 halves the speed every ~115ms.
    pub friction: f32,
}

impl Default for ScrollSettings {
    fn default() -> Self {
        Self { smooth: true, ease_rate: 12.0, kinetic: true, friction: 6.0 }
    }
}

static SETTINGS: std::sync::OnceLock<ScrollSettings> = std::sync::OnceLock::new();

/// This app's effective smooth-scroll settings (`<app>` → `cce-ui` → defaults).
pub fn scroll_settings() -> ScrollSettings {
    *SETTINGS.get_or_init(|| {
        let input = crate::input::cached();
        let app = crate::config::get_app_name().unwrap_or_default();
        let d = ScrollSettings::default();
        let flag = |key: &str, default: bool| {
            input
                .resolve_setting(&app, "", key)
                .and_then(crate::input::SettingValue::as_bool)
                .unwrap_or(default)
        };
        let rate = |key: &str, default: f32| {
            input
                .resolve_setting(&app, "", key)
                .and_then(crate::input::SettingValue::as_f64)
                .map(|v| v as f32)
                .filter(|v| v.is_finite() && *v > 0.0)
                .unwrap_or(default)
        };
        ScrollSettings {
            smooth: flag("smooth_scroll", d.smooth),
            ease_rate: rate("scroll_ease", d.ease_rate),
            kinetic: flag("kinetic_scroll", d.kinetic),
            friction: rate("scroll_friction", d.friction),
        }
    })
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Idle,
    /// Wheel glide: `pos` chases `target`.
    Easing,
    /// Finger down: `pos` is the gesture, `vel` is being estimated.
    Tracking,
    /// Finger lifted: `pos` integrates `vel` under friction.
    Coasting,
}

/// One scroll direction: the drawn offset, where it is heading, and how fast.
#[derive(Debug, Clone, Copy)]
pub struct ScrollAxis {
    pos: f32,
    target: f32,
    vel: f32,
    mode: Mode,
}

impl Default for ScrollAxis {
    fn default() -> Self {
        Self::new(0.0)
    }
}

impl ScrollAxis {
    pub fn new(pos: f32) -> Self {
        Self { pos, target: pos, vel: 0.0, mode: Mode::Idle }
    }

    /// The offset to draw at this frame.
    pub fn pos(&self) -> f32 {
        self.pos
    }

    /// Where the offset is heading (equals `pos` unless a wheel glide is in
    /// flight). Hosts that virtualize rows may prefetch toward this.
    pub fn target(&self) -> f32 {
        self.target
    }

    /// Current speed in px/s (finger estimate while tracking, coast speed after).
    pub fn velocity(&self) -> f32 {
        self.vel
    }

    /// Whether `tick` will still move the offset.
    pub fn is_animating(&self) -> bool {
        matches!(self.mode, Mode::Easing | Mode::Coasting)
    }

    /// Snap to `pos` and cancel any motion.
    pub fn jump_to(&mut self, pos: f32) {
        self.pos = pos;
        self.target = pos;
        self.vel = 0.0;
        self.mode = Mode::Idle;
    }

    /// Adopt a host-side write to the drawn offset (a scrollbar drag, an
    /// auto-snap to a selection): if the host's value differs from ours, the
    /// host moved it and any motion in flight is abandoned.
    pub fn reconcile(&mut self, host_pos: f32) {
        if (host_pos - self.pos).abs() > 1e-3 {
            self.jump_to(host_pos);
        }
    }

    /// Re-clamp after the content or viewport changed size.
    pub fn set_bounds(&mut self, b: Bounds) {
        let p = b.clamp(self.pos);
        let t = b.clamp(self.target);
        if p != self.pos || t != self.target {
            self.pos = p;
            self.target = t;
            if p == t && self.mode == Mode::Easing {
                self.mode = Mode::Idle;
            }
        }
    }

    /// A wheel notch worth `delta` pixels: move the target and glide there
    /// (or jump, with smoothing off). Returns whether anything will move.
    pub fn wheel(&mut self, delta: f32, b: Bounds, s: &ScrollSettings) -> bool {
        if delta == 0.0 {
            return false;
        }
        // A wheel click during a coast redirects it rather than adding to a
        // fling the user has visibly abandoned.
        if self.mode == Mode::Coasting {
            self.vel = 0.0;
            self.target = self.pos;
        }
        let new_target = b.clamp(self.target + delta);
        if (new_target - self.target).abs() < 1e-3 {
            // Already heading there (or pinned at the bound): nothing new moves.
            return false;
        }
        self.target = new_target;
        if s.smooth {
            self.mode = Mode::Easing;
        } else {
            self.pos = new_target;
            self.mode = Mode::Idle;
        }
        true
    }

    /// Finger motion worth `delta` pixels, `dt` seconds after the previous
    /// finger event: the offset follows 1:1 and the velocity estimate blends
    /// in this sample. Returns whether the offset moved.
    pub fn finger(&mut self, delta: f32, dt: f32, b: Bounds) -> bool {
        let old = self.pos;
        let new_pos = b.clamp(self.pos + delta);
        self.pos = new_pos;
        self.target = new_pos;
        self.mode = Mode::Tracking;
        let applied = new_pos - old;
        let sample = applied / dt.clamp(0.004, 0.1);
        self.vel = if applied == 0.0 && delta != 0.0 {
            // Pinned against a bound: no fling into the wall.
            0.0
        } else {
            self.vel * (1.0 - VEL_BLEND) + sample * VEL_BLEND
        };
        (self.pos - old).abs() > 1e-3
    }

    /// The finger lifted `since_last` seconds after its last motion: coast on
    /// the estimated velocity (or stop dead, with kinetic scrolling off or a
    /// finger that had come to rest). Returns whether a coast started.
    pub fn finger_end(&mut self, since_last: f32, s: &ScrollSettings) -> bool {
        if self.mode != Mode::Tracking {
            return false;
        }
        if !s.kinetic || since_last > FLING_STALE_S || self.vel.abs() < COAST_STOP_SPEED {
            self.vel = 0.0;
            self.mode = Mode::Idle;
            return false;
        }
        self.mode = Mode::Coasting;
        true
    }

    /// Glide to an absolute offset (keyboard paging, "scroll to selection").
    /// Returns whether anything will move.
    pub fn scroll_to(&mut self, target: f32, b: Bounds, s: &ScrollSettings) -> bool {
        let t = b.clamp(target);
        if (t - self.pos).abs() < 1e-3 && (t - self.target).abs() < 1e-3 {
            return false;
        }
        self.vel = 0.0;
        self.target = t;
        if s.smooth {
            self.mode = Mode::Easing;
        } else {
            self.pos = t;
            self.mode = Mode::Idle;
        }
        true
    }

    /// Advance `dt` seconds. Returns whether the drawn offset changed — the
    /// host's repaint signal; check [`Self::is_animating`] to keep frames
    /// coming.
    pub fn tick(&mut self, dt: f32, b: Bounds, s: &ScrollSettings) -> bool {
        let old = self.pos;
        match self.mode {
            Mode::Idle | Mode::Tracking => return false,
            Mode::Easing => {
                let remaining = self.target - self.pos;
                if remaining.abs() <= SNAP_PX {
                    self.pos = self.target;
                    self.mode = Mode::Idle;
                } else {
                    // Frame-rate independent: the same fraction of the remaining
                    // distance per unit time whatever the frame pacing.
                    self.pos += remaining * (1.0 - (-s.ease_rate * dt).exp());
                }
            }
            Mode::Coasting => {
                let p = b.clamp(self.pos + self.vel * dt);
                self.pos = p;
                self.target = p;
                self.vel *= (-s.friction * dt).exp();
                if p == b.lo || p == b.hi || self.vel.abs() < COAST_STOP_SPEED {
                    self.vel = 0.0;
                    self.mode = Mode::Idle;
                }
            }
        }
        (self.pos - old).abs() > 1e-4
    }
}

/// A two-axis scroll offset with the event-to-motion mapping shared by every
/// host: `LineDelta` notches scale by the line unit, `PixelDelta`s are pixels,
/// and the phase decides wheel-glide vs finger-track vs fling.
#[derive(Debug, Clone, Copy)]
pub struct ScrollMotion {
    pub x: ScrollAxis,
    pub y: ScrollAxis,
    /// Timestamp of the last finger event, for the velocity estimate and the
    /// stale-fling check.
    last_finger: Option<Instant>,
}

impl Default for ScrollMotion {
    fn default() -> Self {
        Self::new()
    }
}

impl ScrollMotion {
    pub fn new() -> Self {
        Self { x: ScrollAxis::default(), y: ScrollAxis::default(), last_finger: None }
    }

    pub fn at(x: f32, y: f32) -> Self {
        Self { x: ScrollAxis::new(x), y: ScrollAxis::new(y), last_finger: None }
    }

    pub fn is_animating(&self) -> bool {
        self.x.is_animating() || self.y.is_animating()
    }

    /// Adopt host-side writes to both drawn offsets (see [`ScrollAxis::reconcile`]).
    pub fn reconcile(&mut self, x: f32, y: f32) {
        self.x.reconcile(x);
        self.y.reconcile(y);
    }

    pub fn set_bounds(&mut self, bx: Bounds, by: Bounds) {
        self.x.set_bounds(bx);
        self.y.set_bounds(by);
    }

    /// The wheel delta as content pixels, sign-flipped into "offset grows
    /// when the content moves up" — the convention every host used inline
    /// (`-y * 24.0`, `-pos.y`). `line_px` is the per-notch unit for each axis.
    pub fn delta_px(delta: &MouseScrollDelta, line_px: (f32, f32)) -> (f32, f32) {
        match delta {
            MouseScrollDelta::LineDelta(x, y) => (-x * line_px.0, -y * line_px.1),
            MouseScrollDelta::PixelDelta(pos) => (-pos.x as f32, -pos.y as f32),
        }
    }

    /// Feed one wheel event, using the runner-published phase. Returns
    /// whether the offset or its target moved (the host's "raise the
    /// scrollbar" signal, and a repaint request when true).
    pub fn apply(&mut self, delta: &MouseScrollDelta, line_px: (f32, f32), bx: Bounds, by: Bounds) -> bool {
        let (dx, dy) = Self::delta_px(delta, line_px);
        self.apply_px(dx, dy, matches!(delta, MouseScrollDelta::LineDelta(..)), bx, by)
    }

    /// [`Self::apply`] with the conversion already done. `discrete` marks a
    /// wheel-notch delta; a pixel delta takes the finger path only while the
    /// runner reports a finger gesture, else it is applied instantly.
    pub fn apply_px(&mut self, dx: f32, dy: f32, discrete: bool, bx: Bounds, by: Bounds) -> bool {
        let s = scroll_settings();
        let phase = if discrete { ScrollPhase::Wheel } else { current_scroll_phase() };
        match phase {
            ScrollPhase::Wheel => {
                let mut moved = false;
                if discrete {
                    moved |= self.x.wheel(dx, bx, &s);
                    moved |= self.y.wheel(dy, by, &s);
                } else {
                    // A pixel delta outside any gesture (a synthesized or
                    // sourceless event): direct, like the finger path, but
                    // never flings.
                    moved |= self.x.finger(dx, 1.0, bx);
                    moved |= self.y.finger(dy, 1.0, by);
                    self.x.vel = 0.0;
                    self.y.vel = 0.0;
                    self.x.mode = Mode::Idle;
                    self.y.mode = Mode::Idle;
                }
                moved
            }
            ScrollPhase::Finger => {
                let now = Instant::now();
                let dt = self.last_finger.map_or(0.016, |t| now.duration_since(t).as_secs_f32());
                self.last_finger = Some(now);
                let mut moved = false;
                moved |= self.x.finger(dx, dt, bx);
                moved |= self.y.finger(dy, dt, by);
                moved
            }
            ScrollPhase::FingerEnd => {
                let since = self.last_finger.map_or(1.0, |t| t.elapsed().as_secs_f32());
                let mut coasting = false;
                coasting |= self.x.finger_end(since, &s);
                coasting |= self.y.finger_end(since, &s);
                self.last_finger = None;
                coasting
            }
        }
    }

    /// Advance both axes. Returns whether either drawn offset changed.
    pub fn tick(&mut self, dt: f32, bx: Bounds, by: Bounds) -> bool {
        let s = scroll_settings();
        let mut moved = false;
        moved |= self.x.tick(dt, bx, &s);
        moved |= self.y.tick(dt, by, &s);
        moved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn smooth() -> ScrollSettings {
        ScrollSettings { smooth: true, ease_rate: 12.0, kinetic: true, friction: 6.0 }
    }

    fn settle(a: &mut ScrollAxis, b: Bounds, s: &ScrollSettings) -> u32 {
        let mut frames = 0;
        while a.is_animating() && frames < 10_000 {
            a.tick(1.0 / 60.0, b, s);
            frames += 1;
        }
        frames
    }

    #[test]
    fn wheel_glides_to_target_and_settles() {
        let s = smooth();
        let b = Bounds::max(1000.0);
        let mut a = ScrollAxis::new(0.0);
        assert!(a.wheel(24.0, b, &s));
        assert_eq!(a.target(), 24.0);
        assert_eq!(a.pos(), 0.0, "the wheel moves the target, not the drawn offset");
        assert!(a.tick(1.0 / 60.0, b, &s));
        assert!(a.pos() > 0.0 && a.pos() < 24.0);
        let frames = settle(&mut a, b, &s);
        assert_eq!(a.pos(), 24.0);
        assert!(frames > 3 && frames < 60, "settled in {frames} frames");
    }

    #[test]
    fn notches_accumulate_into_one_glide() {
        let s = smooth();
        let b = Bounds::max(1000.0);
        let mut a = ScrollAxis::new(0.0);
        a.wheel(24.0, b, &s);
        a.tick(1.0 / 60.0, b, &s);
        a.wheel(24.0, b, &s);
        assert_eq!(a.target(), 48.0);
        settle(&mut a, b, &s);
        assert_eq!(a.pos(), 48.0);
    }

    #[test]
    fn wheel_target_clamps_to_bounds() {
        let s = smooth();
        let b = Bounds::max(30.0);
        let mut a = ScrollAxis::new(0.0);
        a.wheel(100.0, b, &s);
        assert_eq!(a.target(), 30.0);
        assert!(!a.wheel(100.0, b, &s), "a notch past the end moves nothing");
        settle(&mut a, b, &s);
        assert_eq!(a.pos(), 30.0);
    }

    #[test]
    fn smoothing_off_jumps() {
        let s = ScrollSettings { smooth: false, ..smooth() };
        let b = Bounds::max(1000.0);
        let mut a = ScrollAxis::new(0.0);
        a.wheel(24.0, b, &s);
        assert_eq!(a.pos(), 24.0);
        assert!(!a.is_animating());
    }

    #[test]
    fn finger_tracks_one_to_one_then_flings() {
        let s = smooth();
        let b = Bounds::max(10_000.0);
        let mut a = ScrollAxis::new(0.0);
        // A steady 15px every 8ms swipe.
        for _ in 0..10 {
            assert!(a.finger(15.0, 0.008, b));
        }
        assert_eq!(a.pos(), 150.0);
        assert!(!a.is_animating(), "no motion of its own while the finger is down");
        assert!(a.finger_end(0.01, &s));
        let before = a.pos();
        let frames = settle(&mut a, b, &s);
        assert!(a.pos() > before + 50.0, "coasted from {before} to {}", a.pos());
        assert!(frames > 5);
        assert_eq!(a.velocity(), 0.0);
    }

    #[test]
    fn resting_finger_does_not_fling() {
        let s = smooth();
        let b = Bounds::max(10_000.0);
        let mut a = ScrollAxis::new(0.0);
        for _ in 0..10 {
            a.finger(15.0, 0.008, b);
        }
        assert!(!a.finger_end(0.5, &s), "a finger held still before lifting stops dead");
        assert_eq!(a.pos(), 150.0);
    }

    #[test]
    fn kinetic_off_stops_dead() {
        let s = ScrollSettings { kinetic: false, ..smooth() };
        let b = Bounds::max(10_000.0);
        let mut a = ScrollAxis::new(0.0);
        for _ in 0..10 {
            a.finger(15.0, 0.008, b);
        }
        assert!(!a.finger_end(0.01, &s));
        assert!(!a.is_animating());
    }

    #[test]
    fn coast_stops_at_the_bound() {
        let s = smooth();
        let b = Bounds::max(200.0);
        let mut a = ScrollAxis::new(0.0);
        for _ in 0..10 {
            a.finger(15.0, 0.008, b);
        }
        a.finger_end(0.01, &s);
        settle(&mut a, b, &s);
        assert_eq!(a.pos(), 200.0);
        assert_eq!(a.velocity(), 0.0);
    }

    #[test]
    fn wheel_during_coast_redirects() {
        let s = smooth();
        let b = Bounds::max(10_000.0);
        let mut a = ScrollAxis::new(0.0);
        for _ in 0..10 {
            a.finger(15.0, 0.008, b);
        }
        a.finger_end(0.01, &s);
        a.tick(1.0 / 60.0, b, &s);
        let p = a.pos();
        a.wheel(-24.0, b, &s);
        assert_eq!(a.velocity(), 0.0);
        assert!((a.target() - (p - 24.0)).abs() < 1e-3);
    }

    #[test]
    fn reconcile_adopts_host_writes() {
        let s = smooth();
        let b = Bounds::max(1000.0);
        let mut a = ScrollAxis::new(0.0);
        a.wheel(240.0, b, &s);
        a.tick(1.0 / 60.0, b, &s);
        // The host dragged the thumb to 500 behind our back.
        a.reconcile(500.0);
        assert_eq!(a.pos(), 500.0);
        assert_eq!(a.target(), 500.0);
        assert!(!a.is_animating());
        // An unchanged host value is not a write.
        a.wheel(24.0, b, &s);
        a.reconcile(500.0);
        assert!(a.is_animating());
    }

    #[test]
    fn bounds_shrink_reclamps_and_settles() {
        let s = smooth();
        let mut a = ScrollAxis::new(0.0);
        a.wheel(900.0, Bounds::max(1000.0), &s);
        settle(&mut a, Bounds::max(1000.0), &s);
        a.set_bounds(Bounds::max(100.0));
        assert_eq!(a.pos(), 100.0);
        assert_eq!(a.target(), 100.0);
    }

    #[test]
    fn scroll_to_glides_keyboard_pages() {
        let s = smooth();
        let b = Bounds::max(1000.0);
        let mut a = ScrollAxis::new(0.0);
        assert!(a.scroll_to(400.0, b, &s));
        assert_eq!(a.pos(), 0.0);
        settle(&mut a, b, &s);
        assert_eq!(a.pos(), 400.0);
    }

    #[test]
    fn ease_is_frame_rate_independent() {
        let s = smooth();
        let b = Bounds::max(1000.0);
        let mut fast = ScrollAxis::new(0.0);
        let mut slow = ScrollAxis::new(0.0);
        fast.wheel(500.0, b, &s);
        slow.wheel(500.0, b, &s);
        for _ in 0..12 {
            fast.tick(1.0 / 120.0, b, &s);
        }
        slow.tick(0.1, b, &s);
        assert!((fast.pos() - slow.pos()).abs() < 1.0, "120Hz {} vs 10Hz {}", fast.pos(), slow.pos());
    }

    #[test]
    fn delta_conversion_matches_the_legacy_convention() {
        let (dx, dy) = ScrollMotion::delta_px(&MouseScrollDelta::LineDelta(0.0, -2.0), (LINE_PX, LINE_PX));
        assert_eq!((dx, dy), (0.0, 48.0));
        let (dx, dy) = ScrollMotion::delta_px(
            &MouseScrollDelta::PixelDelta(crate::widget::Position { x: 3.0, y: -10.0 }),
            (LINE_PX, LINE_PX),
        );
        assert_eq!((dx, dy), (-3.0, 10.0));
    }

    #[test]
    fn unbounded_axis_pans_negative() {
        let s = smooth();
        let mut a = ScrollAxis::new(0.0);
        a.wheel(-300.0, Bounds::UNBOUNDED, &s);
        settle(&mut a, Bounds::UNBOUNDED, &s);
        assert_eq!(a.pos(), -300.0);
    }
}
