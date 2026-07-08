//! Narrow-trait `Breadcrumb` (Phase 5k) — the first controller widget across: it re-exposes its
//! [`PathController`] impl through the `Input` capability hooks, so the legacy
//! `Element::as_path_controller` downcasts (cce-designer's `path_mut`) keep working. Segment
//! geometry (hit zones, hover overlay, per-segment text) is derived from the paint rect in one
//! place; the right-press records the clicked segment *before* opening the shared context menu
//! via [`EventCtx::open_context_menu`], so the menu header shows that segment's path.

use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::input::{BREADCRUMB_PADDING, SEGMENT_GAP};
use crate::widget::{
    Adapted, ElementState, Event, EventCtx, Input, Layout, MouseButton, Paint, PathController,
};

#[derive(Debug, Clone)]
pub struct Breadcrumb {
    pub path: Vec<String>,
    hovered: bool,
    hovered_seg: Option<usize>,
    clicked_seg: Option<usize>,
    pub right_clicked_seg: Option<usize>,
    pub network_opacity: f32,
}

impl Breadcrumb {
    pub fn new() -> Adapted<Breadcrumb> {
        Adapted::new(Breadcrumb {
            path: Vec::new(),
            hovered: false,
            hovered_seg: None,
            clicked_seg: None,
            right_clicked_seg: None,
            network_opacity: 1.0,
        })
    }

    pub fn set_network_opacity(&mut self, opacity: f32) {
        self.network_opacity = opacity;
    }

    pub fn path_to_seg(&self, idx: usize) -> String {
        let mut path_str = "/".to_string();
        for (i, s) in self.path.iter().enumerate() {
            if i + 1 > idx {
                break;
            }
            if path_str != "/" {
                path_str.push('/');
            }
            path_str.push_str(s);
        }
        path_str
    }

    /// The displayed segments: a root "/" then each path component with a trailing slash.
    fn virtual_segs(&self) -> Vec<String> {
        let mut segs = vec!["/".to_string()];
        for s in &self.path {
            segs.push(format!("{}/", s));
        }
        segs
    }

    /// Each segment with its left edge and width, derived from the widget's left edge — the one
    /// source for hit-testing, the hover overlay, and the text run (legacy had three copies).
    fn segs_with_x(&self, left: f32) -> Vec<(String, f32, f32)> {
        let mut cx = left + BREADCRUMB_PADDING;
        self.virtual_segs()
            .into_iter()
            .map(|seg| {
                let w = seg.len() as f32 * 7.5;
                let x = cx;
                cx += w + SEGMENT_GAP;
                (seg, x, w)
            })
            .collect()
    }

    fn seg_at(&self, left: f32, px: f32) -> Option<usize> {
        self.segs_with_x(left)
            .iter()
            .position(|(_, x, w)| px >= *x && px < *x + *w)
    }

    fn bg_color(&self) -> [f32; 4] {
        let c = crate::color::breadcrumb_bg_color();
        [c[0], c[1], c[2], self.network_opacity]
    }
}

impl Layout for Breadcrumb {}

impl Paint for Breadcrumb {
    fn color(&self) -> [f32; 4] {
        self.bg_color()
    }

    fn corner_style(&self) -> Option<(f32, (bool, bool, bool, bool))> {
        Some((crate::layout::breadcrumb_corner_radius(), (true, true, false, false)))
    }

    fn widget_font(&self) -> Option<String> {
        let font = crate::layout::breadcrumb_font();
        if font.is_empty() {
            None
        } else {
            Some(font)
        }
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        // Background, top corners rounded — replicating the legacy render path's corner
        // resolution (a radius at or below 0.1 rendered sharp).
        let radius = crate::layout::breadcrumb_corner_radius();
        if radius <= 0.1 {
            ctx.quad(rect, self.bg_color());
        } else {
            ctx.rounded_rect(rect, radius, (true, true, false, false), self.bg_color());
        }

        let segs = self.segs_with_x(rect.x);
        if let Some((_, x, w)) = self.hovered_seg.and_then(|i| segs.get(i)) {
            ctx.quad(
                Rect { x: *x, y: rect.y, width: *w, height: rect.height },
                [1.0, 1.0, 1.0, 0.06],
            );
        }

        let last = segs.len().saturating_sub(1);
        for (i, (seg, x, _)) in segs.into_iter().enumerate() {
            let color = if i == last { [0xcc, 0xcc, 0xd4] } else { [0x88, 0x88, 0x99] };
            ctx.text(seg, x, rect.y + 6.0, 12.0, color);
        }
    }
}

impl Input for Breadcrumb {
    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::PointerMove { x: px, y: py, .. } => {
                let r = ectx.rect;
                let was = self.hovered;
                self.hovered =
                    *px >= r.x && *px <= r.x + r.width && *py >= r.y && *py <= r.y + r.height;
                let old = self.hovered_seg;
                self.hovered_seg = if self.hovered { self.seg_at(r.x, *px) } else { None };
                was != self.hovered || old != self.hovered_seg
            }
            Event::MouseLeave => {
                let changed = self.hovered || self.hovered_seg.is_some();
                self.hovered = false;
                self.hovered_seg = None;
                changed
            }
            Event::MouseButton {
                button: MouseButton::Right,
                state: ElementState::Pressed,
                x: px,
                y: py,
                ..
            } => {
                // Record the segment first: the shared menu's header reads it (via the
                // `as_any` downcast in `UiContext::handle_right_click`) to title itself with
                // that segment's path, and "Copy Path" copies it.
                self.right_clicked_seg = self.seg_at(ectx.rect.x, *px);
                ectx.open_context_menu(*px, *py);
                true
            }
            Event::MouseButton {
                button: MouseButton::Left,
                state: ElementState::Pressed,
                x: px,
                ..
            } => {
                if let Some(i) = self.seg_at(ectx.rect.x, *px) {
                    if i < self.path.len() {
                        self.clicked_seg = Some(i);
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }

    fn path_controller(&self) -> Option<&dyn PathController> {
        Some(self)
    }
    fn path_controller_mut(&mut self) -> Option<&mut dyn PathController> {
        Some(self)
    }

    fn copy_path(&self) {
        let idx = self.right_clicked_seg.unwrap_or(self.path.len());
        let path_str = self.path_to_seg(idx);
        crate::widget::clipboard::copy_to_clipboard(&path_str);
    }
}

impl PathController for Breadcrumb {
    fn set_path(&mut self, segments: &[String]) {
        self.path = segments.to_vec();
    }
    fn path_click(&mut self) -> Option<usize> {
        self.clicked_seg.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::UiContext;
    use crate::widget::Element;

    #[test]
    fn test_breadcrumb_clicks() {
        let mut breadcrumb = Breadcrumb::new();
        breadcrumb.set_path(&["home".to_string(), "lsgalante".to_string()]);
        // Set coordinates: x=10.0, y=20.0, w=300.0, h=24.0
        breadcrumb.set_rect(10.0, 20.0, 300.0, 24.0);

        let ctx = UiContext::new();

        // Let's test hit_test
        assert!(breadcrumb.hit_test(15.0, 25.0, &ctx));

        // Let's test seg_at:
        // x + padding = 10.0 + 8.0 = 18.0
        // Segment 0 ("/"): length 1. width = 1 * 7.5 = 7.5. range: [18.0, 25.5)
        // GAP = 4.0. next = 25.5 + 4.0 = 29.5
        // Segment 1 ("home/"): length 5. width = 5 * 7.5 = 37.5. range: [29.5, 67.0)
        // GAP = 4.0. next = 67.0 + 4.0 = 71.0
        // Segment 2 ("lsgalante/"): length 10. width = 10 * 7.5 = 75.0. range: [71.0, 146.0)

        // Click in segment 0 (/) — through the adapter's direct-dispatch mouse_input, the
        // same entry cce-files drives.
        let mut ui_ctx = UiContext::new();
        assert!(breadcrumb.mouse_input(crate::widget::MouseButton::Left, crate::widget::ElementState::Pressed, 20.0, 25.0, &mut ui_ctx));
        assert_eq!(breadcrumb.path_click(), Some(0));

        // Click in segment 1 (home/)
        assert!(breadcrumb.mouse_input(crate::widget::MouseButton::Left, crate::widget::ElementState::Pressed, 50.0, 25.0, &mut ui_ctx));
        assert_eq!(breadcrumb.path_click(), Some(1));

        // Click in segment 2 (lsgalante/) — the last segment is the current dir, not a link.
        assert!(!breadcrumb.mouse_input(crate::widget::MouseButton::Left, crate::widget::ElementState::Pressed, 100.0, 25.0, &mut ui_ctx));
        assert_eq!(breadcrumb.path_click(), None);
    }

    #[test]
    fn test_breadcrumb_right_clicks() {
        let mut breadcrumb = Breadcrumb::new();
        breadcrumb.set_path(&["home".to_string(), "lsgalante".to_string()]);
        breadcrumb.set_rect(10.0, 20.0, 300.0, 24.0);

        let mut ui_ctx = UiContext::new();

        // Right click segment 1 (home/)
        let handled = breadcrumb.mouse_input(crate::widget::MouseButton::Right, crate::widget::ElementState::Pressed, 50.0, 25.0, &mut ui_ctx);
        assert!(handled);
        assert_eq!(breadcrumb.right_clicked_seg, Some(1));

        // Test path_to_seg
        assert_eq!(breadcrumb.path_to_seg(0), "/");
        assert_eq!(breadcrumb.path_to_seg(1), "/home");
        assert_eq!(breadcrumb.path_to_seg(2), "/home/lsgalante");
    }

    #[test]
    fn path_controller_reachable_through_element() {
        let mut breadcrumb = Breadcrumb::new();
        let elem: &mut dyn Element = &mut breadcrumb;
        elem.as_path_controller_mut()
            .expect("Breadcrumb exposes PathController through the adapter")
            .set_path(&["a".to_string()]);
        assert_eq!(breadcrumb.path, vec!["a".to_string()]);
    }
}
