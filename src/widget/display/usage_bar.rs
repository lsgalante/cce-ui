//! Narrow-trait usage bar (Phase 5c leaf sweep). The narrow [`Paint::paint`] emits a rounded
//! track and fill (the ProgressBar's composition, in both styles), which reach legacy render
//! loops through the adapter's `all_rounded_quads` reverse bridge.

use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::{Adapted, Input, Layout, Paint};

#[derive(Debug, Clone)]
pub struct UsageBar {
    pub value: f32, // 0.0 to 1.0
    pub fill_color: [f32; 4],
    pub bg_color: [f32; 4],
    /// Recessed-track style, the ProgressBar's: a well carved into the plate,
    /// `bg_color` unused (the plate is the floor), the fill inset onto it.
    /// Defaults to `control_relief()`.
    recessed: Option<bool>,
}

impl UsageBar {
    /// The style in force: the per-widget override (`with_recessed`) when set, else
    /// the DE's `control_relief`, read live so a runtime switch
    /// (`layout::set_control_relief`) restyles every control at once.
    fn recessed(&self) -> bool {
        self.recessed.unwrap_or_else(crate::layout::control_relief)
    }

    pub fn new(value: f32) -> Adapted<UsageBar> {
        Adapted::new(UsageBar {
            value: value.clamp(0.0, 1.0),
            fill_color: [0.30, 0.50, 0.32, 1.0], // green-ish
            bg_color: [0.15, 0.15, 0.24, 1.0],   // dark-ish
            recessed: None,
        })
    }

    pub fn set_value(&mut self, value: f32) {
        self.value = value.clamp(0.0, 1.0);
    }
}

/// By-value builders can't flow through `Deref`, so the legacy `with_colors` chain
/// (`UsageBar::new(v).with_colors(..)`) is mirrored on the wrapped type.
impl Adapted<UsageBar> {
    pub fn with_colors(mut self, fill: [f32; 4], bg: [f32; 4]) -> Self {
        self.fill_color = fill;
        self.bg_color = bg;
        self
    }

    /// Recessed style: see the `recessed` field.
    pub fn with_recessed(mut self, recessed: bool) -> Self {
        self.recessed = Some(recessed);
        self
    }
}

impl Layout for UsageBar {
    /// A track, the progress bar's height.
    fn intrinsic_size(&self) -> Option<crate::scene::layout::Size> {
        Some(crate::scene::layout::Size::new(0.0, crate::layout::progressbar_height()))
    }
}

impl Paint for UsageBar {
    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        if self.recessed() {
            // The ProgressBar's recessed composition: fill on the well floor, then
            // the carve, rounded like the sliders' tracks.
            let radius = crate::layout::slider_corner_radius();
            let depth = crate::layout::bevel_width().min(rect.height * 0.2);
            let inset = depth;
            let floor = Rect { x: rect.x + inset, y: rect.y + inset, width: rect.width - 2.0 * inset, height: rect.height - 2.0 * inset };
            let fill_w = floor.width * self.value;
            if fill_w > 0.0 {
                ctx.rounded_rect(Rect { width: fill_w, ..floor }, radius.min(floor.height / 2.0), (true, true, true, true), self.fill_color);
            }
            let (well, radii) = crate::layout::carve_inside(rect, (radius, radius, radius, radius), depth);
            ctx.recess(well, radii, depth);
            return;
        }
        // The flat style: the ProgressBar's rounded track and fill.
        let radius = crate::layout::slider_corner_radius();
        ctx.rounded_rect(rect, radius, (true, true, true, true), self.bg_color);
        let fill_w = rect.width * self.value;
        if fill_w > 0.0 {
            ctx.rounded_rect(Rect { width: fill_w, ..rect }, radius.min(rect.height / 2.0), (true, true, true, true), self.fill_color);
        }
    }
}

impl Input for UsageBar {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::WidgetHost;

    /// The flat style is the ProgressBar's: a full-width rounded track, then a fill scaled by
    /// the clamped value with its radius clamped to half the height — on the rounded getter,
    /// nothing on the plain one (apps read both).
    #[test]
    fn flat_style_is_rounded_track_then_fill() {
        let mut bar = UsageBar::new(0.5).with_recessed(false).with_colors([0.1, 0.2, 0.3, 1.0], [0.4, 0.5, 0.6, 1.0]);
        WidgetHost::set_rect(&mut bar, 12.0, 30.0, 200.0, 8.0);
        let radius = crate::layout::slider_corner_radius();
        let all = (true, true, true, true);
        assert_eq!(
            WidgetHost::all_rounded_quads(&bar, &crate::widget::UiContext::new()),
            vec![
                (12.0, 30.0, 200.0, 8.0, radius, [0.4, 0.5, 0.6, 1.0], all),
                (12.0, 30.0, 100.0, 8.0, radius.min(4.0), [0.1, 0.2, 0.3, 1.0], all),
            ],
        );
        assert!(WidgetHost::extra_quads(&bar).is_empty(), "no plain quad leaks to flat hosts");
    }

    /// Recessed: no bg quad at all — the fill on the floor, then the carve.
    #[test]
    fn recessed_style_is_fill_then_carve() {
        use crate::scene::paint::{PaintCtx, Prim};
        let bar = UsageBar::new(0.5).with_recessed(true);
        let mut pc = PaintCtx::new();
        Paint::paint(bar.inner(), Rect { x: 0.0, y: 0.0, width: 100.0, height: 16.0 }, &mut pc);
        let prims: Vec<Prim> = pc.finish().items.into_iter().map(|i| i.prim).collect();
        assert_eq!(prims.len(), 2, "{prims:?}");
        assert!(matches!(prims[0], Prim::RoundedRect { .. }));
        assert!(matches!(prims[1], Prim::Recess { .. }));
        assert!(WidgetHost::extra_quads(&bar).is_empty(), "no plain bg quad leaks to flat hosts");
    }

    #[test]
    fn value_clamps_on_both_paths() {
        let bar = UsageBar::new(7.0);
        assert_eq!(bar.value, 1.0, "constructor clamps");
        let mut bar = UsageBar::new(0.5);
        bar.set_value(-3.0);
        assert_eq!(bar.value, 0.0, "setter clamps (through Deref)");
    }
}
