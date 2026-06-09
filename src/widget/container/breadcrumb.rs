use crate::widget::*;
use crate::widget::input::{BREADCRUMB_PADDING, SEGMENT_GAP};
use crate::widget::display::TextLabel;

#[derive(Debug, Clone)]
pub struct Breadcrumb {
    x: f32, y: f32, w: f32, h: f32,
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
        Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, hovered: false,
               path: Vec::new(), hovered_seg: None, clicked_seg: None, network_opacity: 1.0 }
    }

    fn seg_at(&self, px: f32) -> Option<usize> {
        let mut cx = self.x + BREADCRUMB_PADDING;
        for (i, seg) in self.path.iter().enumerate() {
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
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn as_ptr(&self) -> *mut (dyn Element + 'static) {
        self as *const Self as *mut Self as *mut (dyn Element + 'static)
    }

    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }

    fn color(&self) -> [f32; 4] { [0.10, 0.10, 0.14, self.network_opacity] }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }

    fn on_cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        let was = self.hovered;
        self.hovered = self.hit_test(px, py, ctx);
        let old = self.hovered_seg;
        self.hovered_seg = if self.hovered { self.seg_at(px) } else { None };
        was != self.hovered || old != self.hovered_seg
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, _py: f32, ctx: &mut UiContext) -> bool {
        if button != MouseButton::Left || state != ElementState::Pressed { return false; }
        if let Some(i) = self.seg_at(px) {
            if i < self.path.len() - 1 {
                self.clicked_seg = Some(i);
                return true;
            }
        }
        false
    }

    fn set_path(&mut self, segments: &[String]) {
        let mut s = Vec::with_capacity(segments.len().max(1));
        if segments.is_empty() || (segments.len() == 1 && segments[0].is_empty()) {
            s.push("/".to_string());
        } else {
            s.push("/".to_string());
            for name in segments {
                s.push(format!(" \u{203A} {}", name));
            }
        }
        self.path = s;
    }

    fn path_click(&mut self) -> Option<usize> { self.clicked_seg.take() }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        if let Some(i) = self.hovered_seg {
            let mut cx = self.x + BREADCRUMB_PADDING;
            for j in 0..i {
                let w = self.path[j].len() as f32 * 7.5;
                cx += w + SEGMENT_GAP;
            }
            let w = self.path[i].len() as f32 * 7.5;
            quads.push((cx, self.y, w, self.h, [1.0, 1.0, 1.0, 0.06]));
        }
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        let mut cx = self.x + BREADCRUMB_PADDING;
        for (i, seg) in self.path.iter().enumerate() {
            labels.push(TextLabel {
                text: seg.clone(),
                x: cx,
                y: self.y + 6.0,
                font_size: 12.0,
                color: if i == self.path.len() - 1 { [0xcc, 0xcc, 0xd4] } else { [0x88, 0x88, 0x99] },
            });
            cx += seg.len() as f32 * 7.5 + SEGMENT_GAP;
        }
        labels
    }
}
