//! `ScrollMotion`: a pair of axes, and the entry points a host feeds events through and ticks
//! once a frame.

use super::*;

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
        let phase = if discrete { ScrollPhase::Wheel } else { current_scroll_phase() };
        self.apply_phase(phase, dx, dy, discrete, bx, by)
    }

    /// [`Self::apply_px`] in a phase the caller names instead of the
    /// runner-published one — which is a process global, so a test that set
    /// it would change what every other test's wheel means while the suite
    /// runs in parallel. A host that reads the phase itself hands it on
    /// through this.
    pub fn apply_phase(&mut self, phase: ScrollPhase, dx: f32, dy: f32, discrete: bool, bx: Bounds, by: Bounds) -> bool {
        let s = scroll_settings();
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
