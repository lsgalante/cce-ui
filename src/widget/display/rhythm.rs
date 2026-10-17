//! The tick every activity indicator shares (the Spinner, the indeterminate ProgressBar): one
//! answer to "what does this frame do", so they all follow the animations switch and the
//! DE's animation speed (`motion::speed`) the same way.

/// What an indicator's tick does this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Beat {
    /// Motion is on: advance the cycle by this many seconds of the indicator's own rhythm,
    /// the frame's `dt` scaled by the DE's speed. Ask for a frame.
    Advance(f32),
    /// Motion just went off: draw the still pose once. Ask for a frame.
    Land,
    /// Motion is off and the still pose is drawn: ask for nothing.
    Rest,
}

/// An indicator's standing with the animations switch: whether it is held at its still pose.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Rhythm {
    pub still: bool,
}

impl Rhythm {
    pub fn beat(&mut self, dt: f32) -> Beat {
        if !crate::motion::enabled() {
            return if std::mem::replace(&mut self.still, true) { Beat::Rest } else { Beat::Land };
        }
        self.still = false;
        Beat::Advance(dt * crate::motion::speed())
    }

    /// Whether a beat asks for a frame.
    pub fn redraws(beat: Beat) -> bool {
        beat != Beat::Rest
    }
}

/// A cycle's phase moved on by `seconds` of a `period`-second cycle, kept in 0..1.
pub(crate) fn advance(phase: f32, seconds: f32, period: f32) -> f32 {
    (phase + seconds / period).rem_euclid(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_beat_scales_by_the_speed_and_rests_with_motion_off() {
        crate::motion::force_for_test(true);
        crate::motion::force_speed_for_test(0.5);
        let mut r = Rhythm::default();
        assert_eq!(r.beat(0.2), Beat::Advance(0.1), "half speed: half the rhythm time");
        crate::motion::force_for_test(false);
        assert_eq!(r.beat(0.2), Beat::Land);
        assert_eq!(r.beat(0.2), Beat::Rest);
        assert!(r.still);
        crate::motion::force_for_test(true);
        crate::motion::force_speed_for_test(1.0);
        assert_eq!(r.beat(0.2), Beat::Advance(0.2));
        assert!(!r.still);
    }

    #[test]
    fn a_phase_wraps_into_its_cycle() {
        assert_eq!(advance(0.75, 1.0, 2.0), 0.25);
        assert_eq!(advance(0.0, 0.5, 2.0), 0.25);
    }
}
