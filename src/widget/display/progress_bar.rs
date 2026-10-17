//! `ProgressBar` — a fill across a track, recessed into the plate under relief (the slider's
//! well composition) and a flat rounded track otherwise. It implements [`Layout`] + [`Paint`] +
//! [`Input`]; [`ProgressBar::new`] returns it already wrapped in [`Adapted`], ready to insert
//! (optionally `.with_label(..)`).
//!
//! **Indeterminate** ([`ProgressBar::indeterminate`], `set_indeterminate`): for work whose
//! amount is unknown, a segment a third of the track sweeps across it, easing at either end,
//! once every 2 s at the DE's normal animation speed (`motion::speed`). It ticks like the
//! Spinner (`rhythm`): with the animations switch off it holds the segment still in the middle
//! of the track and asks for no frames.

use super::rhythm::{self, Beat, Rhythm};
use crate::color;
use crate::scene::layout::{Rect, Size};
use crate::scene::paint::PaintCtx;
use crate::widget::{Adapted, Input, Layout, Paint};

pub struct ProgressBar {
    value: f32,
    /// Recessed-track style, the Slider's: the track is a well carved into the
    /// plate below — no track fill, the plate is the floor — with the progress
    /// fill inset onto that floor. Defaults to `control_relief()`; the flat
    /// style keeps the filled, rounded track.
    recessed: Option<bool>,
    /// Indeterminate: the segment sweeps (`sweep`) and `value` is not drawn.
    indeterminate: bool,
    /// Where the segment is in its sweep, 0 → 1.
    sweep: f32,
    /// Whether the segment is held at its still pose (motion off).
    rhythm: Rhythm,
}

impl ProgressBar {
    /// The style in force: the per-widget override (`with_recessed`) when set, else
    /// the DE's `control_relief`, read live so a runtime switch
    /// (`layout::set_control_relief`) restyles every control at once.
    fn recessed(&self) -> bool {
        self.recessed.unwrap_or_else(crate::layout::control_relief)
    }

    /// One sweep every 2 s, at normal speed.
    const SWEEP_PERIOD: f32 = 2.0;
    /// The segment's length, as a fraction of the track.
    const SEGMENT: f32 = 1.0 / 3.0;

    pub fn new(value: f32) -> Adapted<ProgressBar> {
        Adapted::new(ProgressBar { value, recessed: None, indeterminate: false, sweep: 0.0, rhythm: Rhythm::default() })
    }

    /// An indeterminate bar: see the module doc.
    pub fn indeterminate() -> Adapted<ProgressBar> {
        let mut bar = Self::new(0.0);
        bar.indeterminate = true;
        bar
    }

    pub fn is_indeterminate(&self) -> bool {
        self.indeterminate
    }

    /// Switch between sweeping (work of unknown size) and showing `value`. A bar inserted
    /// determinate can turn indeterminate later: every bar registers for ticks.
    pub fn set_indeterminate(&mut self, indeterminate: bool) {
        self.indeterminate = indeterminate;
    }

    /// The fill's span along the track, as fractions of its width (start, end). Determinate,
    /// it runs from the start to `value`; indeterminate, it is the segment, clipped to the
    /// track as it enters and leaves.
    fn fill_span(&self) -> (f32, f32) {
        if !self.indeterminate {
            return (0.0, self.value.clamp(0.0, 1.0));
        }
        if self.rhythm.still {
            return (0.5 - Self::SEGMENT / 2.0, 0.5 + Self::SEGMENT / 2.0);
        }
        // Ease in and out across the sweep: from wholly before the track to wholly past it.
        let e = 0.5 - 0.5 * (self.sweep * std::f32::consts::PI).cos();
        let start = -Self::SEGMENT + (1.0 + Self::SEGMENT) * e;
        (start.max(0.0), (start + Self::SEGMENT).min(1.0))
    }
}

impl Adapted<ProgressBar> {
    /// Recessed style: see the `recessed` field.
    pub fn with_recessed(mut self, recessed: bool) -> Self {
        self.recessed = Some(recessed);
        self
    }
}

impl Layout for ProgressBar {
    fn intrinsic_size(&self) -> Option<Size> {
        // Height is the bar's own; width comes from the container (legacy `preferred_height`).
        Some(Size::new(0.0, crate::layout::progressbar_height()))
    }
}

impl Paint for ProgressBar {
    fn color(&self) -> [f32; 4] {
        color::progress_bg()
    }

    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        Some((crate::layout::slider_corner_radius(), (true, true, true, true)))
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let radius = crate::layout::slider_corner_radius();
        if self.recessed() {
            // The Slider's recessed composition: the fill sits on the well's flat
            // floor (past the wall's inner half-span), the carve comes after it so
            // the walls' shading modulates what they cross.
            let depth = crate::layout::bevel_width().min(rect.height * 0.2);
            let inset = depth;
            let floor = Rect { x: rect.x + inset, y: rect.y + inset, width: rect.width - 2.0 * inset, height: rect.height - 2.0 * inset };
            self.paint_fill(floor, radius, ctx);
            let (well, radii) = crate::layout::carve_inside(rect, (radius, radius, radius, radius), depth);
            ctx.recess(well, radii, depth);
            return;
        }
        // Track.
        ctx.rounded_rect(rect, radius, (true, true, true, true), color::progress_bg());
        self.paint_fill(rect, radius, ctx);
    }
}

impl ProgressBar {
    /// The fill over `track` (the flat track, or the well's floor): its span, rounded at the
    /// bar's radius clamped to half its height.
    fn paint_fill(&self, track: Rect, radius: f32, ctx: &mut PaintCtx) {
        let (a, b) = self.fill_span();
        let width = track.width * (b - a);
        if width > 0.0 {
            ctx.rounded_rect(
                Rect { x: track.x + track.width * a, width, ..track },
                radius.min(track.height / 2.0),
                (true, true, true, true),
                color::progress_fill(),
            );
        }
    }
}

impl Input for ProgressBar {
    /// Every bar, so one inserted determinate still sweeps once it is made indeterminate
    /// (`UiContext` registers a widget for ticks when it is inserted). A determinate bar's
    /// tick is a branch.
    fn wants_tick(&self) -> bool {
        true
    }

    /// Sweep the segment by `dt` at the DE's speed, as the Spinner turns: a frame every tick
    /// while indeterminate; with motion off one to land on the still pose, then none.
    fn tick(&mut self, dt: f32, _rect: Rect) -> bool {
        if !self.indeterminate {
            return false;
        }
        let beat = self.rhythm.beat(dt);
        if let Beat::Advance(t) = beat {
            self.sweep = rhythm::advance(self.sweep, t, Self::SWEEP_PERIOD);
        }
        Rhythm::redraws(beat)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::WidgetHost;

    /// The reverse bridge reproduces the legacy `all_rounded_quads` output: track quad at the
    /// content rect, fill quad at `w * value` with the radius clamped to half the height.
    #[test]
    fn reverse_bridge_matches_legacy_geometry() {
        let mut bar = ProgressBar::new(0.5).with_recessed(false);
        WidgetHost::set_rect(&mut bar, 10.0, 20.0, 100.0, 8.0);

        let quads = crate::widget::shown_rounded_quads(&bar);
        let radius = crate::layout::slider_corner_radius();
        assert_eq!(quads.len(), 2, "track + fill");
        assert_eq!(quads[0], (10.0, 20.0, 100.0, 8.0, radius, color::progress_bg(), (true, true, true, true)));
        assert_eq!(
            quads[1],
            (10.0, 20.0, 50.0, 8.0, radius.min(4.0), color::progress_fill(), (true, true, true, true)),
        );
    }

    /// Value is clamped like the legacy widget: over 1.0 fills the whole track, 0 emits no fill.
    #[test]
    fn fill_clamps_to_track() {
        let mut over = ProgressBar::new(2.0).with_recessed(false);
        WidgetHost::set_rect(&mut over, 0.0, 0.0, 100.0, 8.0);
        let quads = crate::widget::shown_rounded_quads(&over);
        assert_eq!(quads[1].2, 100.0, "over-1 value fills the whole track");

        let mut empty = ProgressBar::new(0.0).with_recessed(false);
        WidgetHost::set_rect(&mut empty, 0.0, 0.0, 100.0, 8.0);
        assert_eq!(crate::widget::shown_rounded_quads(&empty).len(), 1, "zero value emits track only");
    }

    /// The recessed style draws no track of its own: the fill on the well floor, then
    /// the carve — and nothing else, so the plate below is the floor.
    #[test]
    fn recessed_style_is_fill_then_carve() {
        use crate::scene::paint::Prim;
        let bar = ProgressBar::new(0.5).with_recessed(true);
        let mut pc = PaintCtx::new();
        Paint::paint(bar.inner(), Rect { x: 0.0, y: 0.0, width: 100.0, height: 16.0 }, &mut pc);
        let prims: Vec<Prim> = pc.finish().items.into_iter().map(|i| i.prim).collect();
        assert_eq!(prims.len(), 2, "fill + carve: {prims:?}");
        assert!(matches!(prims[0], Prim::RoundedRect { .. }), "the fill first");
        assert!(matches!(prims[1], Prim::Recess { .. }), "then the well");
        if let Prim::RoundedRect { rect, .. } = &prims[0] {
            assert!(rect.x > 0.0 && rect.width < 50.0, "the fill is inset onto the floor: {rect:?}");
        }
    }

    /// The fill's span along a 100 px flat track, for `bar`.
    fn fill_x(bar: &Adapted<ProgressBar>) -> Option<(f32, f32)> {
        let quads = crate::widget::shown_rounded_quads(bar);
        quads.get(1).map(|q| (q.0, q.0 + q.2))
    }

    /// Indeterminate, the segment sweeps the track: it enters at the start, crosses it and
    /// leaves at the end, never past either, and the value is not drawn.
    #[test]
    fn an_indeterminate_bar_sweeps_its_segment_across_the_track() {
        crate::motion::force_for_test(true);
        let mut bar = ProgressBar::indeterminate().with_recessed(false);
        assert!(bar.is_indeterminate());
        WidgetHost::set_rect(&mut bar, 0.0, 0.0, 100.0, 8.0);
        assert_eq!(fill_x(&bar), None, "at the start of a sweep it has not entered");
        let mut seen = Vec::new();
        for _ in 0..20 {
            assert!(Input::tick(&mut *bar, 0.1, Rect { x: 0.0, y: 0.0, width: 100.0, height: 8.0 }), "it asks for frames");
            if let Some(span) = fill_x(&bar) {
                assert!(span.0 >= 0.0 && span.1 <= 100.0 + 1e-3, "inside the track: {span:?}");
                assert!(span.1 - span.0 <= 100.0 / 3.0 + 1e-3, "no longer than its segment: {span:?}");
                seen.push(span);
            }
        }
        assert!(seen.first().unwrap().0 < 1e-3, "it enters at the start");
        assert!(seen.iter().any(|s| s.0 > 30.0 && s.1 < 70.0), "it crosses the middle whole");
        assert!(seen.iter().any(|s| (s.1 - 100.0).abs() < 1e-3), "it leaves at the end");
    }

    /// With motion off it lands on a still segment in the middle and asks for no more frames;
    /// at half speed it sweeps half as far a tick.
    #[test]
    fn an_indeterminate_bar_holds_still_without_motion_and_follows_the_speed() {
        let r = Rect { x: 0.0, y: 0.0, width: 100.0, height: 8.0 };
        crate::motion::force_for_test(false);
        let mut bar = ProgressBar::indeterminate().with_recessed(false);
        WidgetHost::set_rect(&mut bar, 0.0, 0.0, 100.0, 8.0);
        assert!(Input::tick(&mut *bar, 0.1, r), "one frame to land");
        assert!(!Input::tick(&mut *bar, 0.1, r), "then none");
        let (a, b) = fill_x(&bar).unwrap();
        assert!((a - 100.0 / 3.0).abs() < 1e-3 && (b - 200.0 / 3.0).abs() < 1e-3, "the middle third: {a} {b}");
        crate::motion::force_for_test(true);
        crate::motion::force_speed_for_test(0.5);
        let mut slow = ProgressBar::indeterminate();
        Input::tick(&mut *slow, 0.5, r);
        crate::motion::force_speed_for_test(1.0);
        assert!((slow.sweep - 0.125).abs() < 1e-6, "a quarter of a sweep's 2 s at half speed: {}", slow.sweep);
    }

    /// Determinate bars do not animate; one switched to indeterminate sweeps, and back shows
    /// its value again.
    #[test]
    fn a_determinate_bar_asks_for_no_frames_until_it_is_made_indeterminate() {
        crate::motion::force_for_test(true);
        let r = Rect { x: 0.0, y: 0.0, width: 100.0, height: 8.0 };
        let mut bar = ProgressBar::new(0.5).with_recessed(false);
        WidgetHost::set_rect(&mut bar, 0.0, 0.0, 100.0, 8.0);
        assert!(!Input::tick(&mut *bar, 0.1, r));
        bar.set_indeterminate(true);
        assert!(Input::tick(&mut *bar, 0.1, r));
        bar.set_indeterminate(false);
        assert!(!Input::tick(&mut *bar, 0.1, r));
        assert_eq!(fill_x(&bar), Some((0.0, 50.0)), "its value again");
    }

    /// The one detached-label convention: the assigned rect is the whole block, the
    /// label strip at its top and the track painted below it (config-independent: the
    /// strip is read back from the widget).
    #[test]
    fn label_strip_heads_the_block_and_insets_paint() {
        let mut bar = ProgressBar::new(0.5).with_recessed(false).with_label("Progress");
        let offset = WidgetHost::label_strip(&bar);
        assert!(offset > 0.0, "a labeled bar carries a strip");
        WidgetHost::set_rect(&mut bar, 0.0, 10.0, 100.0, 8.0 + offset);

        let (_, y, _, h) = WidgetHost::rect(&bar);
        assert_eq!(h, 8.0 + offset, "the rect is the block it was given");
        assert_eq!(y, 10.0, "origin is unchanged");

        let quads = crate::widget::shown_rounded_quads(&bar);
        assert_eq!(quads[0].1, 10.0 + offset, "track is painted below the label region");
        assert_eq!(quads[0].3, 8.0, "track keeps the assigned height");

        // preferred_height is the content height; the label strip is `label_strip`.
        assert_eq!(crate::widget::WidgetHostExt::preferred_height(&bar), Some(crate::layout::progressbar_height()));
        assert_eq!(WidgetHost::label_strip(&bar), offset);
        // Runtime type-name matching still sees "ProgressBar", not Adapted<..>.
        assert_eq!(WidgetHost::type_name(&bar), "ProgressBar");
    }
}
