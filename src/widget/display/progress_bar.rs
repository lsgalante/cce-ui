//! The first widget migrated off `WidgetHost` onto the narrow traits (Phase 5c). `ProgressBar`
//! implements only [`Layout`] + [`Paint`] + [`Input`]; [`ProgressBar::new`] returns it already
//! wrapped in [`Adapted`], so construction sites (`Box::new(ProgressBar::new(0.65))`, optionally
//! `.with_label(..)`) are unchanged by the migration.

use crate::colors;
use crate::scene::layout::{Rect, Size};
use crate::scene::paint::PaintCtx;
use crate::widget::{Adapted, Input, Layout, Paint};

pub struct ProgressBar {
    value: f32,
    /// Recessed-track style, the Slider's: the track is a well carved into the
    /// plate below — no track fill, the plate is the floor — with the progress
    /// fill inset onto that floor. Defaults to `control_relief()`; the flat
    /// style keeps the filled, rounded track.
    recessed: bool,
}

impl ProgressBar {
    pub fn new(value: f32) -> Adapted<ProgressBar> {
        Adapted::new(ProgressBar { value, recessed: crate::layout::control_relief() })
    }
}

impl Adapted<ProgressBar> {
    /// Recessed style: see the `recessed` field.
    pub fn with_recessed(mut self, recessed: bool) -> Self {
        self.recessed = recessed;
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
        colors::progress_bg()
    }

    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        Some((crate::layout::slider_corner_radius(), (true, true, true, true)))
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let radius = crate::layout::slider_corner_radius();
        if self.recessed {
            // The Slider's recessed composition: the fill sits on the well's flat
            // floor (past the wall's inner half-span), the carve comes after it so
            // the walls' shading modulates what they cross.
            let depth = crate::layout::bevel_width().min(rect.height * 0.2);
            let inset = depth * 0.5;
            let floor = Rect { x: rect.x + inset, y: rect.y + inset, width: rect.width - 2.0 * inset, height: rect.height - 2.0 * inset };
            let fill_w = floor.width * self.value.clamp(0.0, 1.0);
            if fill_w > 0.0 {
                ctx.rounded_rect(
                    Rect { width: fill_w, ..floor },
                    radius.min(floor.height / 2.0),
                    (true, true, true, true),
                    colors::progress_fill(),
                );
            }
            ctx.recess(rect, (radius, radius, radius, radius), depth);
            return;
        }
        // Track.
        ctx.rounded_rect(rect, radius, (true, true, true, true), colors::progress_bg());
        // Fill.
        let fill_w = rect.width * self.value.clamp(0.0, 1.0);
        if fill_w > 0.0 {
            ctx.rounded_rect(
                Rect { x: rect.x, y: rect.y, width: fill_w, height: rect.height },
                radius.min(rect.height / 2.0),
                (true, true, true, true),
                colors::progress_fill(),
            );
        }
    }
}

impl Input for ProgressBar {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::{WidgetHost, UiContext};

    /// The reverse bridge reproduces the legacy `all_rounded_quads` output: track quad at the
    /// content rect, fill quad at `w * value` with the radius clamped to half the height.
    #[test]
    fn reverse_bridge_matches_legacy_geometry() {
        let ctx = UiContext::new();
        let mut bar = ProgressBar::new(0.5).with_recessed(false);
        WidgetHost::set_rect(&mut bar, 10.0, 20.0, 100.0, 8.0);

        let quads = WidgetHost::all_rounded_quads(&bar, &ctx);
        let radius = crate::layout::slider_corner_radius();
        assert_eq!(quads.len(), 2, "track + fill");
        assert_eq!(quads[0], (10.0, 20.0, 100.0, 8.0, radius, colors::progress_bg(), (true, true, true, true)));
        assert_eq!(
            quads[1],
            (10.0, 20.0, 50.0, 8.0, radius.min(4.0), colors::progress_fill(), (true, true, true, true)),
        );
    }

    /// Value is clamped like the legacy widget: over 1.0 fills the whole track, 0 emits no fill.
    #[test]
    fn fill_clamps_to_track() {
        let ctx = UiContext::new();
        let mut over = ProgressBar::new(2.0).with_recessed(false);
        WidgetHost::set_rect(&mut over, 0.0, 0.0, 100.0, 8.0);
        let quads = WidgetHost::all_rounded_quads(&over, &ctx);
        assert_eq!(quads[1].2, 100.0, "over-1 value fills the whole track");

        let mut empty = ProgressBar::new(0.0).with_recessed(false);
        WidgetHost::set_rect(&mut empty, 0.0, 0.0, 100.0, 8.0);
        assert_eq!(WidgetHost::all_rounded_quads(&empty, &ctx).len(), 1, "zero value emits track only");
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

    /// The detached-label convention survives the adapter: `set_rect` grows the widget by the
    /// label offset, and painting is inset below the label region (config-independent: the
    /// expected offset is derived from the observed rect).
    #[test]
    fn label_inflates_rect_and_insets_paint() {
        let ctx = UiContext::new();
        let mut bar = ProgressBar::new(0.5).with_recessed(false).with_label("Progress");
        WidgetHost::set_rect(&mut bar, 0.0, 10.0, 100.0, 8.0);

        let (_, y, _, h) = WidgetHost::rect(&bar);
        let offset = h - 8.0;
        assert!(offset >= 0.0, "rect grew by the label offset");
        assert_eq!(y, 10.0, "origin is unchanged");

        let quads = WidgetHost::all_rounded_quads(&bar, &ctx);
        assert_eq!(quads[0].1, 10.0 + offset, "track is painted below the label region");
        assert_eq!(quads[0].3, 8.0, "track keeps the assigned height");

        // preferred_height forwards from the narrow intrinsic size.
        assert_eq!(WidgetHost::preferred_height(&bar), Some(crate::layout::progressbar_height()));
        // Runtime type-name matching still sees "ProgressBar", not Adapted<..>.
        assert_eq!(WidgetHost::type_name(&bar), "ProgressBar");
    }
}
