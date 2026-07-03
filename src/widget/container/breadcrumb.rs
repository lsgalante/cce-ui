use crate::widget::*;
use crate::widget::input::{BREADCRUMB_PADDING, SEGMENT_GAP};
use crate::widget::display::TextLabel;

#[derive(Debug, Clone)]
pub struct Breadcrumb {
    pub base: Widget,
    hovered: bool,
    path: Vec<String>,
    hovered_seg: Option<usize>,
    clicked_seg: Option<usize>,
    pub network_opacity: f32,
}

impl Breadcrumb {
    pub fn set_network_opacity(&mut self, opacity: f32) {
        self.network_opacity = opacity;
    }

    pub fn new() -> Self {
        Self { base: Widget::new(), hovered: false,
               path: Vec::new(), hovered_seg: None, clicked_seg: None, network_opacity: 1.0 }
    }

    fn virtual_segs(&self) -> Vec<String> {
        let mut segs = vec!["/".to_string()];
        for s in &self.path {
            segs.push(format!("{}/", s));
        }
        segs
    }

    fn seg_at(&self, px: f32) -> Option<usize> {
        let mut cx = self.base.x + BREADCRUMB_PADDING;
        let segs = self.virtual_segs();
        for (i, seg) in segs.iter().enumerate() {
            let w = seg.len() as f32 * 7.5;
            if px >= cx && px < cx + w {
                return Some(i);
            }
            cx += w + SEGMENT_GAP;
        }
        None
    }
}

impl Element for Breadcrumb {
    crate::impl_widget_base!(Breadcrumb);

    fn rounded_corners(&self) -> (bool, bool, bool, bool) { (true, true, false, false) }

    fn color(&self) -> [f32; 4] {
        let c = crate::color::breadcrumb_bg_color();
        [c[0], c[1], c[2], self.network_opacity]
    }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }

    fn on_cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        let was = self.hovered;
        self.hovered = self.hit_test(px, py, ctx);
        let old = self.hovered_seg;
        self.hovered_seg = if self.hovered { self.seg_at(px) } else { None };
        was != self.hovered || old != self.hovered_seg
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, _py: f32, _ctx: &mut UiContext) -> bool {
        if button != MouseButton::Left || state != ElementState::Pressed { return false; }
        if let Some(i) = self.seg_at(px) {
            if i < self.path.len() {
                self.clicked_seg = Some(i);
                return true;
            }
        }
        false
    }

    fn as_path_controller(&self) -> Option<&dyn PathController> { Some(self) }
    fn as_path_controller_mut(&mut self) -> Option<&mut dyn PathController> { Some(self) }

    fn widget_font(&self) -> Option<String> {
        let font = crate::layout::breadcrumb_font();
        if font.is_empty() {
            None
        } else {
            Some(font)
        }
    }

    fn corner_radius(&self) -> f32 {
        crate::layout::breadcrumb_corner_radius()
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        let (x, y, w, h) = self.rect();
        quads.push((x, y, w, h, self.color()));
        if let Some(i) = self.hovered_seg {
            let mut cx = x + BREADCRUMB_PADDING;
            let segs = self.virtual_segs();
            for j in 0..i {
                let w = segs[j].len() as f32 * 7.5;
                cx += w + SEGMENT_GAP;
            }
            let w = segs[i].len() as f32 * 7.5;
            quads.push((cx, y, w, h, [1.0, 1.0, 1.0, 0.06]));
        }
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        let (x, y, _, _) = self.rect();
        let mut cx = x + BREADCRUMB_PADDING;
        let segs = self.virtual_segs();
        let len = segs.len();
        for (i, seg) in segs.iter().enumerate() {
            labels.push(TextLabel {
                text: seg.clone(),
                x: cx,
                y: y + 6.0,
                font_size: 12.0,
                color: if i == len - 1 { [0xcc, 0xcc, 0xd4] } else { [0x88, 0x88, 0x99] },
            });
            cx += seg.len() as f32 * 7.5 + SEGMENT_GAP;
        }
        labels
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

        // Click in segment 0 (/)
        let mut ui_ctx = UiContext::new();
        assert!(breadcrumb.mouse_input(crate::widget::MouseButton::Left, crate::widget::ElementState::Pressed, 20.0, 25.0, &mut ui_ctx));
        assert_eq!(breadcrumb.path_click(), Some(0));

        // Click in segment 1 (home/)
        assert!(breadcrumb.mouse_input(crate::widget::MouseButton::Left, crate::widget::ElementState::Pressed, 50.0, 25.0, &mut ui_ctx));
        assert_eq!(breadcrumb.path_click(), Some(1));

        // Click in segment 2 (lsgalante/)
        // Wait, path.len() is 2. i = 2. 2 < 2 is false.
        // So clicking the last segment should return false.
        assert!(!breadcrumb.mouse_input(crate::widget::MouseButton::Left, crate::widget::ElementState::Pressed, 100.0, 25.0, &mut ui_ctx));
        assert_eq!(breadcrumb.path_click(), None);
    }
}

