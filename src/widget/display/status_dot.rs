//! `StatusDot` — an indicator: a disc in its status colour, the largest circle the rect holds.
//! It stays a shape, not a glyph (an indicator, not a symbol). The tests below hold that it
//! draws its disc where a host places it.

use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::{Adapted, Input, Layout, Paint};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DotStatus {
    Active,
    Inactive,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct StatusDot {
    pub status: DotStatus,
}

impl StatusDot {
    /// The dot's default side.
    pub const SIZE: f32 = 12.0;

    pub fn new(status: DotStatus) -> Adapted<StatusDot> {
        Adapted::new(StatusDot { status })
    }

    pub fn set_status(&mut self, status: DotStatus) {
        self.status = status;
    }
}

impl Layout for StatusDot {
    /// A dot: a fixed small disc unless the host sizes it.
    fn intrinsic_size(&self) -> Option<crate::scene::layout::Size> {
        Some(crate::scene::layout::Size::new(StatusDot::SIZE, StatusDot::SIZE))
    }
}

impl Paint for StatusDot {
    fn color(&self) -> [f32; 4] {
        match self.status {
            DotStatus::Active => [0.20, 0.70, 0.35, 1.0],
            DotStatus::Inactive => [0.50, 0.50, 0.55, 1.0],
            DotStatus::Warning => [0.90, 0.60, 0.10, 1.0],
            DotStatus::Error => [0.85, 0.25, 0.25, 1.0],
        }
    }

    /// A disc: the colour as a rounded rect whose radius is half the short side. A rounded
    /// rect rather than a circle prim, so the rounded-quad view (`shown_rounded_quads`), which
    /// carries rounded rects but not circles, sees it too.
    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let r = rect.width.min(rect.height) * 0.5;
        ctx.rounded_rect(rect, r, (true, true, true, true), self.color());
    }
}

impl Input for StatusDot {
    fn blocks_root_plate_drag(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::{WidgetHost, UiContext};

    #[test]
    fn emits_its_disc_through_the_rounded_bridge() {
        let mut dot = StatusDot::new(DotStatus::Warning);
        WidgetHost::set_rect(&mut dot, 5.0, 6.0, 10.0, 10.0);
        assert_eq!(
            crate::widget::shown_rounded_quads(&dot),
            vec![(5.0, 6.0, 10.0, 10.0, 5.0, [0.90, 0.60, 0.10, 1.0], (true, true, true, true))],
            "a disc: the rect at half-side radius",
        );
        assert!(crate::widget::shown_quads(&dot).is_empty(), "nothing on the plain path (apps read both)");
        // Drags pass through, as legacy declared.
        assert!(!crate::widget::WidgetHostExt::blocks_root_plate_drag(&dot));
        // State mutation through Deref, as call sites write it.
        dot.set_status(DotStatus::Error);
        assert_eq!(dot.status, DotStatus::Error);
    }

    /// A dot placed in a row's cell paints its one disc there, in its status colour (the
    /// pre-migration widget painted nothing at all).
    #[test]
    fn a_placed_dot_paints_its_disc() {
        let mut ctx = UiContext::new();
        let dot = ctx.insert(StatusDot::new(DotStatus::Active));
        crate::compose::place_h(dot, 10.0, 10.0, 10.0, 10.0, &mut ctx);
        let mut pc = crate::scene::paint::PaintCtx::new();
        crate::widget::painter::paint_root_into(&ctx, &ctx[dot], &mut pc);
        let prims: Vec<_> = pc.finish().items.into_iter().map(|i| i.prim).collect();
        let rect = crate::scene::layout::Rect { x: 10.0, y: 10.0, width: 10.0, height: 10.0 };
        assert_eq!(
            prims,
            [crate::scene::paint::Prim::RoundedRect { rect, radius: 5.0, corners: (true, true, true, true), color: [0.20, 0.70, 0.35, 1.0] }],
            "one disc, active-status green"
        );
    }
}
