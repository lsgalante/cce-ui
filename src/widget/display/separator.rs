//! Narrow-trait separator line (Phase 5c leaf sweep). The legacy struct carried its own public
//! x/y/w/h fields instead of a `Widget` base; the rect now lives on the [`Adapted`] base, so
//! callers position it via `set_rect`/`rect` (cce-status-interface's rotation loop was updated
//! accordingly).
//!
//! **Relief.** Under `control_relief` the rule is a groove carved into the plate it sits
//! on — the seam vocabulary (`CLAUDE.md`): the rect is the groove's floor, the walls
//! rise either side of it into the plate, and the cut dies out at the rect's two ends.
//! Flat, it is the coloured hairline it always was.

use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::{Adapted, WidgetHost, Input, Layout, Paint};

#[derive(Debug, Clone)]
pub struct Separator {
    pub color: [f32; 4],
}

impl Separator {
    pub fn new(x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> Adapted<Separator> {
        let mut sep = Adapted::new(Separator { color });
        WidgetHost::set_rect(&mut sep, x, y, w, h);
        sep
    }
}

impl Separator {
    /// The groove's wall run: the seam depth a control-height plate gets
    /// (`bevel_width` capped at a fifth of the height — Breadcrumb's, ColorSelector's).
    pub fn groove_depth() -> f32 {
        crate::layout::bevel_width().min(crate::layout::button_height() * 0.2)
    }

    /// The relief cut for `rect`: the groove's two ends, floor width, depth and host.
    /// The line runs along the rect's long axis through its centre; the floor is the
    /// rect's thickness (a hairline at least); the host is the rect stretched across
    /// the line by three depths, so the walls stand clear of the host's own roll while
    /// the cut still fades out at the ends.
    pub fn groove(&self, rect: Rect) -> ((f32, f32), (f32, f32), f32, f32, Rect) {
        let depth = Self::groove_depth();
        let horizontal = rect.width >= rect.height;
        let floor = rect.width.min(rect.height).max(crate::widget::Breadcrumb::SEAM_WIDTH);
        let reach = 3.0 * depth;
        if horizontal {
            let cy = rect.y + rect.height * 0.5;
            let host = Rect { x: rect.x, y: cy - reach, width: rect.width, height: 2.0 * reach };
            ((rect.x, cy), (rect.x + rect.width, cy), floor, depth, host)
        } else {
            let cx = rect.x + rect.width * 0.5;
            let host = Rect { x: cx - reach, y: rect.y, width: 2.0 * reach, height: rect.height };
            ((cx, rect.y), (cx, rect.y + rect.height), floor, depth, host)
        }
    }
}

impl Layout for Separator {}

impl Paint for Separator {
    /// Flat: the rule's colour. Relief: none — the groove is shading on the plate.
    fn color(&self) -> [f32; 4] {
        if crate::layout::control_relief() { [0.0, 0.0, 0.0, 0.0] } else { self.color }
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        if crate::layout::control_relief() {
            let (a, b, width, depth, host) = self.groove(rect);
            ctx.groove(a, b, width, depth, host);
        } else if self.color[3].abs() > 0.001 {
            ctx.quad(rect, self.color);
        }
    }
}

impl Input for Separator {
    fn blocks_root_plate_drag(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructor_places_the_rect_and_bridge_emits_it() {
        let sep = Separator::new(100.0, 0.0, 1.0, 24.0, [0.3, 0.3, 0.3, 1.0]);
        assert_eq!(WidgetHost::rect(&sep), (100.0, 0.0, 1.0, 24.0));
        assert!(!WidgetHost::blocks_root_plate_drag(&sep));
        if !crate::layout::control_relief() {
            assert_eq!(WidgetHost::color(&sep), [0.3, 0.3, 0.3, 1.0]);
            assert_eq!(WidgetHost::extra_quads(&sep), vec![(100.0, 0.0, 1.0, 24.0, [0.3, 0.3, 0.3, 1.0])]);
        }
    }

    /// The relief cut runs along the long axis through the centre, the floor is the
    /// rect's thickness, and the host spans the line without outrunning its ends.
    #[test]
    fn the_groove_follows_the_long_axis() {
        let h = Separator::new(20.0, 100.0, 200.0, 1.0, [0.5; 4]);
        let (a, b, floor, depth, host) = h.inner().groove(Rect { x: 20.0, y: 100.0, width: 200.0, height: 1.0 });
        assert_eq!((a, b), ((20.0, 100.5), (220.0, 100.5)));
        assert_eq!(floor, 1.0);
        assert_eq!(depth, Separator::groove_depth());
        assert_eq!((host.x, host.width), (20.0, 200.0), "the cut dies at the rect's ends");
        assert!(host.y < 100.5 - depth && host.y + host.height > 100.5 + depth, "the walls stand inside the host");
        let v = Separator::new(100.0, 0.0, 1.0, 24.0, [0.5; 4]);
        let (a, b, _, _, host) = v.inner().groove(Rect { x: 100.0, y: 0.0, width: 1.0, height: 24.0 });
        assert_eq!((a, b), ((100.5, 0.0), (100.5, 24.0)));
        assert_eq!((host.y, host.height), (0.0, 24.0));
    }

    /// The status bar's vertical-rotation pattern, post-migration: transpose via rect/set_rect.
    #[test]
    fn rotation_via_set_rect() {
        let mut sep = Separator::new(100.0, 0.0, 1.0, 24.0, [0.3, 0.3, 0.3, 1.0]);
        let (x, y, w, h) = WidgetHost::rect(&sep);
        WidgetHost::set_rect(&mut sep, y, x, h, w);
        assert_eq!(WidgetHost::rect(&sep), (0.0, 100.0, 24.0, 1.0));
    }
}
