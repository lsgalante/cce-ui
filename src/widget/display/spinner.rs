//! `Spinner` — an indeterminate activity indicator: an arc turning round a faint ring, in the
//! ProgressBar's fill and track colours, the largest ring the rect holds. It is a shape drawn
//! each frame, not a glyph sequence, so it scales and recolours with the style and costs no
//! textures.
//!
//! It animates in `tick`, so a host must tick it (it registers with `UiContext`, which ticks the
//! visible ones), and it asks for frames only while it runs and motion is on. With the DE's
//! animations switch off (`motion::enabled`) it holds a still quarter arc at twelve o'clock and
//! asks for none; stopped (`set_running(false)`) it paints nothing and keeps its place.

use std::f32::consts::{FRAC_PI_2, TAU};

use crate::color;
use crate::scene::layout::{Rect, Size};
use crate::scene::paint::PaintCtx;
use crate::widget::{Adapted, Input, Layout, Paint};

#[derive(Debug, Clone)]
pub struct Spinner {
    running: bool,
    /// The arc's leading end, radians clockwise from three o'clock.
    angle: f32,
    /// Where the arc is in its lengthen-and-shorten cycle, 0 → 1.
    breath: f32,
    /// The last tick found motion off: the arc is held at its still pose.
    still: bool,
}

impl Spinner {
    /// The spinner's default side.
    pub const SIZE: f32 = 20.0;
    /// One turn a second.
    const TURN_RATE: f32 = TAU;
    /// The arc lengthens and shortens once every 1.6 s.
    const BREATH_PERIOD: f32 = 1.6;
    /// The arc's length at its shortest and longest, in turns.
    const MIN_ARC: f32 = 0.12;
    const MAX_ARC: f32 = 0.7;
    /// The still pose's length, in turns: a quarter, from twelve o'clock.
    const STILL_ARC: f32 = 0.25;

    pub fn new() -> Adapted<Spinner> {
        Adapted::new(Spinner { running: true, angle: -FRAC_PI_2, breath: 0.0, still: false })
    }

    pub fn running(&self) -> bool {
        self.running
    }

    /// Start or stop. A stopped spinner paints nothing and asks for no frames, but keeps its
    /// place in the layout, so the row around it does not jump when work starts.
    pub fn set_running(&mut self, running: bool) {
        self.running = running;
    }

    /// The arc as drawn: (start, end) in radians. `end` is the leading end; the arc's length
    /// follows a cosine through its cycle, so it eases at its shortest and longest.
    fn arc(&self) -> (f32, f32) {
        if self.still {
            return (-FRAC_PI_2, -FRAC_PI_2 + Self::STILL_ARC * TAU);
        }
        let s = 0.5 - 0.5 * (self.breath * TAU).cos();
        let len = (Self::MIN_ARC + (Self::MAX_ARC - Self::MIN_ARC) * s) * TAU;
        (self.angle - len, self.angle)
    }

    /// The stroke's width for a ring of outer radius `r`.
    fn thickness(r: f32) -> f32 {
        (r * 0.25).max(1.5)
    }
}

impl Layout for Spinner {
    /// A fixed small square unless the host sizes it.
    fn intrinsic_size(&self) -> Option<Size> {
        Some(Size::new(Spinner::SIZE, Spinner::SIZE))
    }
}

impl Paint for Spinner {
    fn color(&self) -> [f32; 4] {
        color::progress_fill()
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        if !self.running {
            return;
        }
        let r = rect.width.min(rect.height) * 0.5;
        if r <= 0.0 {
            return;
        }
        let (cx, cy) = (rect.x + rect.width * 0.5, rect.y + rect.height * 0.5);
        let t = Self::thickness(r);
        ctx.arc(cx, cy, r, t, 0.0, TAU, color::progress_bg());
        let (start, end) = self.arc();
        ctx.arc(cx, cy, r, t, start, end, color::progress_fill());
    }
}

impl Input for Spinner {
    fn blocks_root_plate_drag(&self) -> bool {
        false
    }

    fn wants_tick(&self) -> bool {
        true
    }

    /// Turn the arc by `dt`. A frame is asked for every tick while it runs; with motion off,
    /// one to land on the still pose and then none, until motion is back on.
    fn tick(&mut self, dt: f32, _rect: Rect) -> bool {
        if !self.running {
            return false;
        }
        if !crate::motion::enabled() {
            return !std::mem::replace(&mut self.still, true);
        }
        self.still = false;
        self.angle = (self.angle + dt * Self::TURN_RATE) % TAU;
        self.breath = (self.breath + dt / Self::BREATH_PERIOD) % 1.0;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::paint::Prim;
    use crate::widget::{UiContext, WidgetHost};

    /// A spinner's tick does not read its rect.
    const NO_RECT: Rect = Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 };

    fn arcs(s: &Adapted<Spinner>) -> Vec<(f32, f32, f32, f32, f32, f32)> {
        let mut pc = PaintCtx::new();
        let (x, y, w, h) = s.rect();
        Paint::paint(&**s, Rect { x, y, width: w, height: h }, &mut pc);
        pc.finish()
            .items
            .into_iter()
            .filter_map(|i| match i.prim {
                Prim::Arc { cx, cy, radius, thickness, start, end, .. } => Some((cx, cy, radius, thickness, start, end)),
                _ => None,
            })
            .collect()
    }

    /// A ring and its arc, centred in the rect at the largest radius the short side holds.
    #[test]
    fn a_spinner_draws_its_ring_and_arc_in_the_middle_of_its_rect() {
        let mut s = Spinner::new();
        WidgetHost::set_rect(&mut s, 10.0, 20.0, 40.0, 20.0);
        let a = arcs(&s);
        assert_eq!(a.len(), 2, "the track ring, then the arc");
        let (cx, cy, r, t, start, end) = a[0];
        assert_eq!((cx, cy, r, t), (30.0, 30.0, 10.0, 2.5));
        assert_eq!((start, end), (0.0, TAU), "the track is the whole ring");
        assert_eq!((a[1].0, a[1].1, a[1].2), (30.0, 30.0, 10.0));
        assert!(a[1].4 < a[1].5, "the arc runs clockwise from its tail to its lead");
    }

    /// While it runs, every tick moves the arc and asks for a frame.
    #[test]
    fn a_running_spinner_turns_and_asks_for_frames() {
        crate::motion::force_for_test(true);
        let mut s = Spinner::new();
        let before = s.arc();
        assert!(Input::tick(&mut *s, 1.0 / 60.0, NO_RECT));
        assert_ne!(s.arc(), before, "the arc moved");
        // A long run stays in range rather than growing without bound.
        for _ in 0..10_000 {
            Input::tick(&mut *s, 1.0 / 60.0, NO_RECT);
        }
        assert!(s.angle.abs() <= TAU && (0.0..1.0).contains(&s.breath));
    }

    /// With motion off it lands on the still pose in one frame and then asks for none; motion
    /// back on, it turns again.
    #[test]
    fn with_motion_off_a_spinner_holds_still_and_asks_for_no_frames() {
        crate::motion::force_for_test(true);
        let mut s = Spinner::new();
        Input::tick(&mut *s, 0.3, NO_RECT);
        crate::motion::force_for_test(false);
        assert!(Input::tick(&mut *s, 1.0 / 60.0, NO_RECT), "one frame, to land on the still pose");
        assert_eq!(s.arc(), (-FRAC_PI_2, 0.0), "a quarter from twelve o'clock");
        assert!(!Input::tick(&mut *s, 1.0 / 60.0, NO_RECT), "then nothing to draw");
        assert_eq!(s.arc(), (-FRAC_PI_2, 0.0));
        crate::motion::force_for_test(true);
        assert!(Input::tick(&mut *s, 1.0 / 60.0, NO_RECT), "motion back: it turns again");
        assert_ne!(s.arc(), (-FRAC_PI_2, 0.0));
    }

    /// Stopped, it paints nothing and asks for no frames, and keeps its size.
    #[test]
    fn a_stopped_spinner_paints_nothing_and_keeps_its_place() {
        crate::motion::force_for_test(true);
        let mut s = Spinner::new();
        WidgetHost::set_rect(&mut s, 0.0, 0.0, 20.0, 20.0);
        s.set_running(false);
        assert!(!s.running());
        assert!(arcs(&s).is_empty());
        assert!(!Input::tick(&mut *s, 1.0 / 60.0, NO_RECT));
        assert_eq!(Layout::intrinsic_size(&*s), Some(Size::new(Spinner::SIZE, Spinner::SIZE)));
    }

    /// Inserted, it is ticked by the context: `UiContext::tick` reports it changed.
    #[test]
    fn the_context_ticks_a_spinner_it_holds() {
        crate::motion::force_for_test(true);
        let mut ctx = UiContext::new();
        let h = ctx.insert(Spinner::new());
        assert!(ctx.tick(1.0 / 60.0), "a registered spinner drives the frame");
        ctx[h].set_running(false);
        assert!(!ctx.tick(1.0 / 60.0), "a stopped one does not");
    }
}
