//! Narrow-trait usage bar (Phase 5c leaf sweep). Legacy geometry lived in `extra_quads` (plain
//! bg + fill quads); the narrow [`Paint::paint`] emits the same two quads, which reach legacy
//! render loops byte-identically through the adapter's `extra_quads` reverse bridge.

use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::{Adapted, Input, Layout, Paint};

#[derive(Debug, Clone)]
pub struct UsageBar {
    pub value: f32, // 0.0 to 1.0
    pub fill_color: [f32; 4],
    pub bg_color: [f32; 4],
}

impl UsageBar {
    pub fn new(value: f32) -> Adapted<UsageBar> {
        Adapted::new(UsageBar {
            value: value.clamp(0.0, 1.0),
            fill_color: [0.30, 0.50, 0.32, 1.0], // green-ish
            bg_color: [0.15, 0.15, 0.24, 1.0],   // dark-ish
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
        ctx.quad(rect, self.bg_color);
        ctx.quad(Rect { width: rect.width * self.value, ..rect }, self.fill_color);
    }
}

impl Input for UsageBar {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::WidgetHost;

    /// Byte-identical to the legacy `extra_quads` override: full-width bg quad, then a fill quad
    /// scaled by the clamped value.
    #[test]
    fn bridge_matches_legacy_extra_quads() {
        let mut bar = UsageBar::new(0.5).with_colors([0.1, 0.2, 0.3, 1.0], [0.4, 0.5, 0.6, 1.0]);
        WidgetHost::set_rect(&mut bar, 12.0, 30.0, 200.0, 8.0);
        assert_eq!(
            WidgetHost::extra_quads(&bar),
            vec![
                (12.0, 30.0, 200.0, 8.0, [0.4, 0.5, 0.6, 1.0]),
                (12.0, 30.0, 100.0, 8.0, [0.1, 0.2, 0.3, 1.0]),
            ],
        );
        // Nothing leaks onto the rounded path (apps read both getters).
        assert!(WidgetHost::all_rounded_quads(&bar, &crate::widget::UiContext::new()).is_empty());
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
