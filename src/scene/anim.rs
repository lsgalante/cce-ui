//! Animation primitive — Phase 4 of the core rebuild.
//!
//! The legacy toolkit has almost no animation: a single hand-rolled `hover_animation` helper (with
//! a dead duplicate), and everything else is an instant boolean flip (`hovered`, `pressed`,
//! `network_opacity` as a static multiplier). This module provides the missing spine — a small
//! [`Animated<T>`] value that eases or springs toward a target over time — so hover/press/opacity
//! and transitions become interpolated instead of instantaneous.
//!
//! It is pure time-based math over an [`Animatable`] value, fully unit-testable without a clock,
//! GPU, or event loop: drive it with [`Animated::tick`] and read [`Animated::value`]. Wiring it
//! into widgets and having the frame loop keep ticking while anything is live is the (runtime-gated)
//! follow-up; the loop already returns "still animating" from `tick`, which this feeds.

/// A value that can be interpolated and integrated for animation (scalars, colors, points).
pub trait Animatable: Copy {
    fn lerp(self, other: Self, t: f32) -> Self;
    fn add(self, other: Self) -> Self;
    fn sub(self, other: Self) -> Self;
    fn scale(self, s: f32) -> Self;
    fn zero() -> Self;
    /// Rough magnitude used for settle detection (need not be a true norm).
    fn magnitude(self) -> f32;
}

impl Animatable for f32 {
    fn lerp(self, other: Self, t: f32) -> Self {
        self + (other - self) * t
    }
    fn add(self, other: Self) -> Self {
        self + other
    }
    fn sub(self, other: Self) -> Self {
        self - other
    }
    fn scale(self, s: f32) -> Self {
        self * s
    }
    fn zero() -> Self {
        0.0
    }
    fn magnitude(self) -> f32 {
        self.abs()
    }
}

impl Animatable for [f32; 4] {
    fn lerp(self, other: Self, t: f32) -> Self {
        [
            self[0] + (other[0] - self[0]) * t,
            self[1] + (other[1] - self[1]) * t,
            self[2] + (other[2] - self[2]) * t,
            self[3] + (other[3] - self[3]) * t,
        ]
    }
    fn add(self, o: Self) -> Self {
        [self[0] + o[0], self[1] + o[1], self[2] + o[2], self[3] + o[3]]
    }
    fn sub(self, o: Self) -> Self {
        [self[0] - o[0], self[1] - o[1], self[2] - o[2], self[3] - o[3]]
    }
    fn scale(self, s: f32) -> Self {
        [self[0] * s, self[1] * s, self[2] * s, self[3] * s]
    }
    fn zero() -> Self {
        [0.0; 4]
    }
    fn magnitude(self) -> f32 {
        self[0].abs().max(self[1].abs()).max(self[2].abs()).max(self[3].abs())
    }
}

/// Easing curve applied to a tween's normalized time `t in [0,1]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Easing {
    Linear,
    EaseInQuad,
    EaseOutQuad,
    EaseInOutCubic,
}

impl Easing {
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            Easing::EaseInQuad => t * t,
            Easing::EaseOutQuad => t * (2.0 - t),
            Easing::EaseInOutCubic => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    let f = -2.0 * t + 2.0;
                    1.0 - f * f * f / 2.0
                }
            }
        }
    }
}

/// How an [`Animated`] value approaches its target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Motion {
    /// Ease from the value-at-retarget to the target over `duration` seconds.
    Tween { duration: f32, easing: Easing },
    /// Physical spring: `stiffness` pulls toward the target, `damping` bleeds velocity.
    Spring { stiffness: f32, damping: f32 },
}

/// Below this (in `Animatable::magnitude`) a spring is considered settled.
const SETTLE_EPS: f32 = 0.001;

/// A value that animates toward a target. Retarget with [`set_target`](Animated::set_target); each
/// frame call [`tick`](Animated::tick) with the elapsed seconds and read [`value`](Animated::value).
#[derive(Clone, Copy, Debug)]
pub struct Animated<T: Animatable> {
    current: T,
    start: T,
    target: T,
    velocity: T,
    elapsed: f32,
    motion: Motion,
    animating: bool,
}

impl<T: Animatable> Animated<T> {
    pub fn new(value: T, motion: Motion) -> Self {
        Animated {
            current: value,
            start: value,
            target: value,
            velocity: T::zero(),
            elapsed: 0.0,
            motion,
            animating: false,
        }
    }

    pub fn tween(value: T, duration: f32, easing: Easing) -> Self {
        Self::new(value, Motion::Tween { duration, easing })
    }

    pub fn spring(value: T, stiffness: f32, damping: f32) -> Self {
        Self::new(value, Motion::Spring { stiffness, damping })
    }

    pub fn value(&self) -> T {
        self.current
    }

    pub fn target(&self) -> T {
        self.target
    }

    pub fn is_animating(&self) -> bool {
        self.animating
    }

    /// Aim at a new target and start animating (unless already there). A tween restarts from the
    /// current value; a spring keeps its velocity for continuous motion.
    pub fn set_target(&mut self, target: T) {
        if target.sub(self.current).magnitude() < SETTLE_EPS
            && self.velocity.magnitude() < SETTLE_EPS
        {
            self.jump_to(target);
            return;
        }
        self.target = target;
        self.start = self.current;
        self.elapsed = 0.0;
        self.animating = true;
    }

    /// Snap instantly to `value`, cancelling any in-flight animation.
    pub fn jump_to(&mut self, value: T) {
        self.current = value;
        self.start = value;
        self.target = value;
        self.velocity = T::zero();
        self.elapsed = 0.0;
        self.animating = false;
    }

    /// Advance by `dt` seconds. Returns whether the value is still animating (the signal the frame
    /// loop uses to keep requesting frames).
    pub fn tick(&mut self, dt: f32) -> bool {
        if !self.animating {
            return false;
        }
        match self.motion {
            Motion::Tween { duration, easing } => {
                self.elapsed += dt;
                let t = if duration > 0.0 { (self.elapsed / duration).clamp(0.0, 1.0) } else { 1.0 };
                self.current = self.start.lerp(self.target, easing.apply(t));
                if t >= 1.0 {
                    self.current = self.target;
                    self.animating = false;
                }
            }
            Motion::Spring { stiffness, damping } => {
                // Semi-implicit Euler.
                let disp = self.target.sub(self.current);
                let force = disp.scale(stiffness).sub(self.velocity.scale(damping));
                self.velocity = self.velocity.add(force.scale(dt));
                self.current = self.current.add(self.velocity.scale(dt));
                if disp.magnitude() < SETTLE_EPS && self.velocity.magnitude() < SETTLE_EPS {
                    self.current = self.target;
                    self.velocity = T::zero();
                    self.animating = false;
                }
            }
        }
        self.animating
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_to_settle<T: Animatable>(a: &mut Animated<T>, dt: f32, max_steps: usize) -> usize {
        let mut n = 0;
        while a.tick(dt) && n < max_steps {
            n += 1;
        }
        n
    }

    #[test]
    fn new_is_idle_at_value() {
        let a = Animated::tween(5.0f32, 0.3, Easing::Linear);
        assert_eq!(a.value(), 5.0);
        assert!(!a.is_animating());
    }

    #[test]
    fn linear_tween_reaches_target_and_stops() {
        let mut a = Animated::tween(0.0f32, 1.0, Easing::Linear);
        a.set_target(10.0);
        assert!(a.is_animating());
        // Halfway: linear => 5.0.
        a.tick(0.5);
        assert!((a.value() - 5.0).abs() < 1e-4);
        // Finish.
        let still = a.tick(0.5);
        assert!(!still);
        assert_eq!(a.value(), 10.0);
        assert!(!a.is_animating());
    }

    #[test]
    fn tween_overshoot_dt_clamps_to_target() {
        let mut a = Animated::tween(0.0f32, 0.2, Easing::Linear);
        a.set_target(1.0);
        assert!(!a.tick(10.0)); // dt far exceeds duration
        assert_eq!(a.value(), 1.0);
    }

    #[test]
    fn retarget_restarts_tween_from_current() {
        let mut a = Animated::tween(0.0f32, 1.0, Easing::Linear);
        a.set_target(10.0);
        a.tick(0.5); // now at 5.0
        a.set_target(0.0); // reverse
        assert!((a.value() - 5.0).abs() < 1e-4, "keeps current value at retarget");
        a.tick(0.5); // halfway back from 5 -> 0
        assert!((a.value() - 2.5).abs() < 1e-4);
    }

    #[test]
    fn easing_endpoints_and_shape() {
        for e in [Easing::Linear, Easing::EaseInQuad, Easing::EaseOutQuad, Easing::EaseInOutCubic] {
            assert!((e.apply(0.0) - 0.0).abs() < 1e-6, "{e:?} at 0");
            assert!((e.apply(1.0) - 1.0).abs() < 1e-6, "{e:?} at 1");
        }
        // EaseInQuad starts slow: at t=0.5 it's below linear (0.25 < 0.5).
        assert!(Easing::EaseInQuad.apply(0.5) < 0.5);
        // EaseOutQuad starts fast: above linear at t=0.5.
        assert!(Easing::EaseOutQuad.apply(0.5) > 0.5);
    }

    #[test]
    fn spring_converges_and_settles() {
        let mut a = Animated::spring(0.0f32, 120.0, 20.0);
        a.set_target(1.0);
        let steps = run_to_settle(&mut a, 1.0 / 60.0, 100_000);
        assert!(!a.is_animating(), "spring settled within {steps} steps");
        assert!((a.value() - 1.0).abs() < 0.01, "converged to target, got {}", a.value());
        assert!(steps > 1, "took a few frames, not instant");
    }

    #[test]
    fn spring_keeps_velocity_across_retarget() {
        let mut a = Animated::spring(0.0f32, 100.0, 15.0);
        a.set_target(1.0);
        for _ in 0..5 {
            a.tick(1.0 / 60.0);
        }
        let moving = a.value();
        a.set_target(2.0);
        // Still animating and continues past the intermediate value toward the new target.
        assert!(a.is_animating());
        let steps = run_to_settle(&mut a, 1.0 / 60.0, 100_000);
        assert!((a.value() - 2.0).abs() < 0.01, "reached new target in {steps} steps from {moving}");
    }

    #[test]
    fn jump_to_is_instant_and_idle() {
        let mut a = Animated::tween(0.0f32, 1.0, Easing::Linear);
        a.set_target(10.0);
        a.tick(0.3);
        a.jump_to(7.0);
        assert_eq!(a.value(), 7.0);
        assert!(!a.is_animating());
        assert!(!a.tick(1.0), "no motion after jump");
    }

    #[test]
    fn set_target_equal_to_current_does_not_animate() {
        let mut a = Animated::tween(3.0f32, 1.0, Easing::Linear);
        a.set_target(3.0);
        assert!(!a.is_animating());
    }

    #[test]
    fn animates_a_color_via_tween() {
        let mut a = Animated::tween([0.0, 0.0, 0.0, 1.0], 1.0, Easing::Linear);
        a.set_target([1.0, 0.5, 0.0, 1.0]);
        a.tick(0.5);
        let v = a.value();
        assert!((v[0] - 0.5).abs() < 1e-4 && (v[1] - 0.25).abs() < 1e-4 && (v[2] - 0.0).abs() < 1e-4);
        assert!(!a.tick(0.5));
        assert_eq!(a.value(), [1.0, 0.5, 0.0, 1.0]);
    }
}
