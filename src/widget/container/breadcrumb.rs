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

    fn virtual_segs(&self) -> Vec<String> {
        let mut segs = vec!["/".to_string()];
        for s in &self.path {
            segs.push(format!("{}/", s));
        }
        segs
    }

    fn seg_at(&self, px: f32) -> Option<usize> {
        let mut cx = self.x + BREADCRUMB_PADDING;
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
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn as_ptr(&self) -> *mut (dyn Element + 'static) {
        self as *const Self as *mut Self as *mut (dyn Element + 'static)
    }
    fn as_ptr_mut(&mut self) -> *mut (dyn Element + 'static) {
        self as *mut Self as *mut (dyn Element + 'static)
    }

    fn rounded_corners(&self) -> (bool, bool, bool, bool) { (true, true, false, false) }

    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }

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
        quads.push((self.x, self.y, self.w, self.h, self.color()));
        if let Some(i) = self.hovered_seg {
            let mut cx = self.x + BREADCRUMB_PADDING;
            let segs = self.virtual_segs();
            for j in 0..i {
                let w = segs[j].len() as f32 * 7.5;
                cx += w + SEGMENT_GAP;
            }
            let w = segs[i].len() as f32 * 7.5;
            quads.push((cx, self.y, w, self.h, [1.0, 1.0, 1.0, 0.06]));
        }
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        let mut cx = self.x + BREADCRUMB_PADDING;
        let segs = self.virtual_segs();
        let len = segs.len();
        for (i, seg) in segs.iter().enumerate() {
            labels.push(TextLabel {
                text: seg.clone(),
                x: cx,
                y: self.y + 6.0,
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
