//! `ScrollAxis`: one direction's scroll — the drawn position, the wheel's target, the finger's
//! velocity — and how each input regime moves it, frame by frame, within its bounds.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
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
    pub(super) pos: f32,
    pub(super) target: f32,
    pub(super) vel: f32,
    pub(super) mode: Mode,
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
            // Settings that forbid the motion already in flight (animations
            // switched off mid-glide) land it rather than finish it.
            Mode::Easing if !s.smooth => {
                self.pos = self.target;
                self.mode = Mode::Idle;
            }
            Mode::Coasting if !s.kinetic => {
                self.vel = 0.0;
                self.target = self.pos;
                self.mode = Mode::Idle;
            }
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
