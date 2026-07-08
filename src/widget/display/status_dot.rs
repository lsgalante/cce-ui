//! Narrow-trait status dot (Phase 5c leaf sweep).
//!
//! **Deliberate behavior fix:** the legacy `Element` impl only set `color()` and never emitted
//! geometry on any render path (`all_quads` and `all_rounded_quads` were both empty for it, and
//! `render_widget` never reads `color()` directly), so the dot was **invisible** — a probe test
//! against the legacy widget confirmed zero rects emitted through `render_widget`. The narrow
//! [`Paint`] default emits the color quad, so the dot now actually shows. The probe test at the
//! bottom documents the fix.

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
    pub fn new(status: DotStatus) -> Adapted<StatusDot> {
        Adapted::new(StatusDot { status })
    }

    pub fn set_status(&mut self, status: DotStatus) {
        self.status = status;
    }
}

impl Layout for StatusDot {}

impl Paint for StatusDot {
    fn color(&self) -> [f32; 4] {
        // `Paint::paint`'s default emits this as a plain quad — which is the fix: the legacy
        // impl never got its color onto any render path.
        match self.status {
            DotStatus::Active => [0.20, 0.70, 0.35, 1.0],
            DotStatus::Inactive => [0.50, 0.50, 0.55, 1.0],
            DotStatus::Warning => [0.90, 0.60, 0.10, 1.0],
            DotStatus::Error => [0.85, 0.25, 0.25, 1.0],
        }
    }
}

impl Input for StatusDot {
    fn blocks_backplate_drag(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::{Element, UiContext};

    #[test]
    fn emits_its_color_quad_through_the_bridge() {
        let mut dot = StatusDot::new(DotStatus::Warning);
        Element::set_rect(&mut dot, 5.0, 6.0, 10.0, 10.0);
        assert_eq!(
            Element::extra_quads(&dot),
            vec![(5.0, 6.0, 10.0, 10.0, [0.90, 0.60, 0.10, 1.0])],
        );
        // Drags pass through, as legacy declared.
        assert!(!Element::blocks_backplate_drag(&dot));
        // State mutation through Deref, as call sites write it.
        dot.set_status(DotStatus::Error);
        assert_eq!(dot.status, DotStatus::Error);
    }

    /// Documents the behavior fix: the legacy `StatusDot` emitted **zero** rects through
    /// `render_widget` (probe run against the pre-migration widget), i.e. the dot was invisible
    /// wherever it was used. The migrated widget emits exactly one.
    #[test]
    fn render_widget_now_draws_the_dot() {
        struct Probe {
            rects: Vec<(f32, f32, f32, f32, [f32; 4])>,
        }
        impl crate::layout::RenderTarget for Probe {
            fn rect(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32) {
                self.rects.push((x, y, w, h, color));
            }
            fn text(&mut self, _c: &str, _x: f32, _y: f32, _s: f32, _col: [f32; 4]) {}
        }

        let mut ctx = UiContext::new();
        let mut probe = Probe { rects: Vec::new() };
        let mut dot = StatusDot::new(DotStatus::Active);
        crate::layout::render_widget(&mut probe, &mut dot, 10.0, 10.0, 10.0, 10.0, &mut ctx);
        assert_eq!(probe.rects.len(), 1, "the dot is visible now (legacy emitted 0 here)");
        assert_eq!(probe.rects[0].4, [0.20, 0.70, 0.35, 1.0], "active-status green");
    }
}
