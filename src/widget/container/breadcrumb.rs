//! Narrow-trait `Breadcrumb` (Phase 5k) — the first controller widget across: its
//! [`PathController`] impl is reached through the concrete `Adapted<Breadcrumb>` by deref
//! (cce-designer's `path_mut` downcasts the roster entry; Phase 6aw). Segment
//! geometry (hit zones, hover overlay, per-segment text) is derived from the paint rect in one
//! place; the right-press records the clicked segment *before* opening the shared context menu
//! via [`EventCtx::open_context_menu`], so the menu header shows that segment's path.

use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::input::BREADCRUMB_PADDING;
use crate::widget::{
    Adapted, ElementState, Event, EventCtx, Input, Layout, MouseButton, Paint, PathController,
};

/// Point size the breadcrumb text is painted at; also the size measured for segment layout so
/// the two stay in lockstep.
const BREADCRUMB_FONT_SIZE: f32 = 12.0;

/// Gap between segment buttons.
const SEG_GAP: f32 = 6.0;

/// Horizontal text inset inside each segment button.
const SEG_PAD_X: f32 = 8.0;

/// A segment as actually laid out for painting/hit-testing: its text, its BUTTON BOX's left
/// edge and width (text sits `SEG_PAD_X` in), and logical index in
/// [`Breadcrumb::virtual_segs`] (`None` for the leading "…" ellipsis marker).
struct VisibleSeg {
    text: String,
    x: f32,
    w: f32,
    logical: Option<usize>,
}

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

    /// The displayed segments: a root "/" then each path component. No trailing slashes —
    /// each segment renders as its own button, so the boxes are the separators.
    fn virtual_segs(&self) -> Vec<String> {
        let mut segs = vec!["/".to_string()];
        for s in &self.path {
            segs.push(s.clone());
        }
        segs
    }

    /// The breadcrumb font split into (family, size). The configured font string carries both
    /// (e.g. "Berkeley Mono 14"); the render path parses the size out of it and shapes at that
    /// size, so layout must measure with the SAME family and size — passing the whole string
    /// as a family name (which no font matches) or measuring at a different size mis-sizes
    /// every segment and makes them overlap or gap.
    fn font_and_size() -> (String, f32) {
        let (family, size) = crate::layout::parse_font_string(&crate::layout::breadcrumb_font());
        (family, size.unwrap_or(BREADCRUMB_FONT_SIZE))
    }

    /// The segments to actually paint, each with its BUTTON BOX left edge/width and its
    /// *logical* index (position in [`virtual_segs`]; `None` marks the leading "…" ellipsis).
    /// This is the one source for hit-testing, the hover overlay, the plates, and the text
    /// run.
    ///
    /// Each segment is its own button: box width = the segment's measured ink width plus
    /// `SEG_PAD_X` each side, boxes separated by `SEG_GAP`. (The old abutting-text layout
    /// measured cumulative prefixes so glyph side bearings cancelled; per-box padding
    /// absorbs the bearings instead, so per-segment measurement is enough.)
    ///
    /// When the full run is wider than the container, leading segments are dropped and
    /// replaced with a "…" marker button, so the trailing (current) segments stay visible.
    /// The last segment is always kept.
    fn visible_segs(&self, rect: Rect) -> Vec<VisibleSeg> {
        let all = self.virtual_segs();
        let avail = (rect.width - 2.0 * BREADCRUMB_PADDING).max(0.0);

        let (font, size) = Self::font_and_size();
        let box_w =
            |s: &str| crate::widget::display::measure_text_width(s, &font, size) + 2.0 * SEG_PAD_X;

        // Lay a list of (text, logical index) out left-to-right from the widget's left edge,
        // one button box per segment.
        let place = |items: Vec<(String, Option<usize>)>| -> Vec<VisibleSeg> {
            let mut x = rect.x + BREADCRUMB_PADDING;
            items
                .into_iter()
                .map(|(text, logical)| {
                    let w = box_w(&text);
                    let vs = VisibleSeg { text, x, w, logical };
                    x += w + SEG_GAP;
                    vs
                })
                .collect()
        };

        let full: f32 = all.iter().map(|s| box_w(s)).sum::<f32>()
            + SEG_GAP * all.len().saturating_sub(1) as f32;

        if full <= avail || all.len() <= 1 {
            return place(all.into_iter().enumerate().map(|(i, s)| (s, Some(i))).collect());
        }

        // Keep the last segment, then add trailing segments while the "…" marker button plus
        // the kept run still fits; finally prepend the marker and restore left-to-right order.
        let ell = "…".to_string();
        let mut used = box_w(&ell);
        let mut kept: Vec<(String, Option<usize>)> = Vec::new();
        for i in (0..all.len()).rev() {
            let w = SEG_GAP + box_w(&all[i]);
            if !kept.is_empty() && used + w > avail {
                break;
            }
            used += w;
            kept.push((all[i].clone(), Some(i)));
        }
        kept.push((ell, None));
        kept.reverse();
        place(kept)
    }

    fn seg_at(&self, rect: Rect, px: f32) -> Option<usize> {
        self.visible_segs(rect)
            .iter()
            .find(|s| px >= s.x && px < s.x + s.w)
            .and_then(|s| s.logical)
    }

    /// Vertical margin between the widget's recessed well and each raised
    /// segment button inside it.
    const SEG_INSET_Y: f32 = 3.0;

    /// The per-segment button boxes as laid out for `rect`: (x, y, w, h), inset
    /// vertically so the raised plates sit within the widget's full-width well.
    /// For flat-path hosts (cce-files) that mirror the plates app-side — the
    /// render_widget geometry path drops relief prims, same as the dropdown's.
    pub fn segment_boxes(&self, rect: Rect) -> Vec<(f32, f32, f32, f32)> {
        self.visible_segs(rect)
            .iter()
            .map(|vs| {
                (
                    vs.x,
                    rect.y + Self::SEG_INSET_Y,
                    vs.w,
                    (rect.height - 2.0 * Self::SEG_INSET_Y).max(0.0),
                )
            })
            .collect()
    }

    fn bg_color(&self) -> [f32; 4] {
        let c = crate::color::breadcrumb_bg_color();
        [c[0], c[1], c[2], self.network_opacity]
    }
}

impl Layout for Breadcrumb {}

impl Paint for Breadcrumb {
    fn color(&self) -> [f32; 4] {
        // No whole-widget fill in either style: each segment draws its own
        // button plate in paint().
        [0.0; 4]
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
        // The full-width recessed well defines the bar (as it always did);
        // each segment is a RAISED button plate within it — the dropdown
        // pairing (recessed surround + raised face), per-segment.
        let radius = crate::layout::breadcrumb_corner_radius();
        let relief = crate::layout::control_relief();
        if relief {
            let depth = crate::layout::bevel_width().min(rect.height * 0.2);
            ctx.recess(rect, (radius, radius, radius, radius), depth);
        }

        let segs = self.visible_segs(rect);
        let boxes = self.segment_boxes(rect);
        for (vs, &(sx, sy, sw, sh)) in segs.iter().zip(boxes.iter()) {
            let seg_rect = Rect { x: sx, y: sy, width: sw, height: sh };
            let r = radius.min(sh * 0.5);
            if relief {
                let seg_depth = crate::layout::bevel_width().min(sh * 0.2);
                ctx.boss(seg_rect, (r, r, r, r), seg_depth);
            } else {
                ctx.rounded_rect(seg_rect, r, (true, true, true, true), self.bg_color());
            }
            if vs.logical.is_some() && vs.logical == self.hovered_seg {
                ctx.quad(seg_rect, [1.0, 1.0, 1.0, 0.06]);
            }
        }

        // The current directory (last logical segment) is drawn brightly; everything else,
        // including the "…" ellipsis marker, is dimmed. Paint at the configured font size so
        // the glyph run matches the widths `visible_segs` measured (and thus its x positions).
        let (_, size) = Self::font_and_size();
        let last_logical = self.virtual_segs().len().saturating_sub(1);
        for vs in segs {
            let color =
                if vs.logical == Some(last_logical) { [0xcc, 0xcc, 0xd4] } else { [0x88, 0x88, 0x99] };
            ctx.text(
                vs.text,
                vs.x + SEG_PAD_X,
                crate::layout::center_text_y(rect.y, rect.height, size),
                size,
                color,
            );
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
                self.hovered_seg = if self.hovered { self.seg_at(r, *px) } else { None };
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
                self.right_clicked_seg = self.seg_at(ectx.rect, *px);
                ectx.open_context_menu(*px, *py);
                true
            }
            Event::MouseButton {
                button: MouseButton::Left,
                state: ElementState::Pressed,
                x: px,
                ..
            } => {
                if let Some(i) = self.seg_at(ectx.rect, *px) {
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


    fn context_action(&mut self, action: crate::widget::ContextAction) -> bool {
        if action != crate::widget::ContextAction::CopyPath {
            return false;
        }
        let idx = self.right_clicked_seg.unwrap_or(self.path.len());
        let path_str = self.path_to_seg(idx);
        crate::widget::clipboard::copy_to_clipboard(&path_str);
        true
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
    use crate::widget::WidgetHost;

    /// Center x of the visible segment with the given logical index, derived from the
    /// widget's own layout so the tests don't depend on the font's exact metrics.
    fn seg_center_x(breadcrumb: &Breadcrumb, rect: Rect, logical: usize) -> f32 {
        let s = breadcrumb
            .visible_segs(rect)
            .into_iter()
            .find(|s| s.logical == Some(logical))
            .expect("segment visible");
        s.x + s.w / 2.0
    }

    #[test]
    fn test_breadcrumb_clicks() {
        let mut breadcrumb = Breadcrumb::new();
        breadcrumb.set_path(&["home".to_string(), "lsgalante".to_string()]);
        // Set coordinates: x=10.0, y=20.0, w=300.0, h=24.0
        let rect = Rect { x: 10.0, y: 20.0, width: 300.0, height: 24.0 };
        breadcrumb.set_rect(rect.x, rect.y, rect.width, rect.height);

        let ctx = UiContext::new();

        // Let's test hit_test
        assert!(breadcrumb.hit_test(15.0, 25.0, &ctx));

        // Segments abut (no inter-segment gap) and are sized by real font measurement, so
        // click coordinates are taken from the widget's own layout rather than hardcoded.

        // Click in segment 0 (/) — through the adapter's direct-dispatch mouse_input, the
        // same entry cce-files drives.
        let mut ui_ctx = UiContext::new();
        let x0 = seg_center_x(&breadcrumb, rect, 0);
        assert!(breadcrumb.mouse_input(crate::widget::MouseButton::Left, crate::widget::ElementState::Pressed, x0, 25.0, &mut ui_ctx));
        assert_eq!(breadcrumb.path_click(), Some(0));

        // Click in segment 1 (home/)
        let x1 = seg_center_x(&breadcrumb, rect, 1);
        assert!(breadcrumb.mouse_input(crate::widget::MouseButton::Left, crate::widget::ElementState::Pressed, x1, 25.0, &mut ui_ctx));
        assert_eq!(breadcrumb.path_click(), Some(1));

        // Click in segment 2 (lsgalante/) — the last segment is the current dir, not a link.
        let x2 = seg_center_x(&breadcrumb, rect, 2);
        assert!(!breadcrumb.mouse_input(crate::widget::MouseButton::Left, crate::widget::ElementState::Pressed, x2, 25.0, &mut ui_ctx));
        assert_eq!(breadcrumb.path_click(), None);
    }

    #[test]
    fn test_breadcrumb_right_clicks() {
        let mut breadcrumb = Breadcrumb::new();
        breadcrumb.set_path(&["home".to_string(), "lsgalante".to_string()]);
        let rect = Rect { x: 10.0, y: 20.0, width: 300.0, height: 24.0 };
        breadcrumb.set_rect(rect.x, rect.y, rect.width, rect.height);

        let mut ui_ctx = UiContext::new();

        // Right click segment 1 (home/)
        let x1 = seg_center_x(&breadcrumb, rect, 1);
        let handled = breadcrumb.mouse_input(crate::widget::MouseButton::Right, crate::widget::ElementState::Pressed, x1, 25.0, &mut ui_ctx);
        assert!(handled);
        assert_eq!(breadcrumb.right_clicked_seg, Some(1));

        // Test path_to_seg
        assert_eq!(breadcrumb.path_to_seg(0), "/");
        assert_eq!(breadcrumb.path_to_seg(1), "/home");
        assert_eq!(breadcrumb.path_to_seg(2), "/home/lsgalante");
    }

    #[test]
    fn long_path_elides_leading_segments() {
        let mut breadcrumb = Breadcrumb::new();
        breadcrumb.set_path(&[
            "home".to_string(),
            "lsgalante".to_string(),
            "Dropbox".to_string(),
            "cce".to_string(),
            "cce-ui".to_string(),
        ]);
        // Narrow container: the full path can't fit, so leading segments get dropped.
        breadcrumb.set_rect(0.0, 0.0, 160.0, 24.0);

        let segs = breadcrumb.visible_segs(Rect { x: 0.0, y: 0.0, width: 160.0, height: 24.0 });

        // First visible segment is the "…" ellipsis marker (no logical index → not a link).
        assert_eq!(segs.first().map(|s| s.text.as_str()), Some("…"));
        assert_eq!(segs.first().and_then(|s| s.logical), None);

        // The current directory (last logical segment) is always visible.
        let last_logical = breadcrumb.path.len(); // "/" is index 0, so path.len() == last idx
        assert_eq!(segs.last().and_then(|s| s.logical), Some(last_logical));
        assert_eq!(segs.last().map(|s| s.text.as_str()), Some("cce-ui"));

        // Everything painted stays within the container's right edge.
        for s in &segs {
            assert!(s.x + s.w <= 160.0 + 0.01, "segment {:?} overflows", s.text);
        }

        // A kept trailing segment still hit-tests to its original logical index, so clicking
        // it navigates to the correct path.
        let visible_seg = segs.iter().rev().nth(1).unwrap();
        let hit = breadcrumb
            .seg_at(Rect { x: 0.0, y: 0.0, width: 160.0, height: 24.0 }, visible_seg.x + 1.0);
        assert_eq!(hit, visible_seg.logical);
    }

    #[test]
    fn short_path_is_not_elided() {
        let mut breadcrumb = Breadcrumb::new();
        breadcrumb.set_path(&["home".to_string(), "lsgalante".to_string()]);
        breadcrumb.set_rect(0.0, 0.0, 300.0, 24.0);

        let segs = breadcrumb.visible_segs(Rect { x: 0.0, y: 0.0, width: 300.0, height: 24.0 });
        // Root + two components, no ellipsis.
        assert_eq!(segs.len(), 3);
        assert!(segs.iter().all(|s| s.logical.is_some()));
        assert_eq!(segs[0].text, "/");
    }

    #[test]
    fn path_controller_reachable_through_element() {
        let mut breadcrumb = Breadcrumb::new();
        PathController::set_path(&mut *breadcrumb, &["a".to_string()]);
        assert_eq!(breadcrumb.path, vec!["a".to_string()]);
    }
}
