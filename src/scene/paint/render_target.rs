//! `PaintCtx` as a flat host's `RenderTarget` (`layout::bridge`): a legacy colour-typed target
//! painting into the display list.

use super::*;

/// `PaintCtx` as a popover render target: display-list hosts pass their frame
/// ctx straight into `render_popover`, so popovers draw REAL prims — relief
/// plates, rounded rects, bounded text — instead of the flattened
/// `PopoverCollector` view (which stays for legacy tuple hosts).
impl crate::scene::paint::RenderTarget for PaintCtx {
    fn icon(&mut self, name: &str, rect: Rect, color: [f32; 4]) {
        PaintCtx::icon(self, name, rect, color);
    }
    fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, thickness: f32, color: [f32; 4], cap: Cap) {
        PaintCtx::vector(self, x1, y1, x2, y2, thickness, color, cap);
    }
    fn arc(&mut self, cx: f32, cy: f32, radius: f32, thickness: f32, start: f32, end: f32, color: [f32; 4]) {
        PaintCtx::arc(self, cx, cy, radius, thickness, start, end, color);
    }
    fn circle(&mut self, cx: f32, cy: f32, radius: f32, color: [f32; 4]) {
        PaintCtx::circle(self, cx, cy, radius, color);
    }

    fn rect(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32) {
        self.quad(Rect { x, y, width: w, height: h }, color);
    }
    fn rect_with_radius(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32, radius: f32) {
        self.rounded_rect(Rect { x, y, width: w, height: h }, radius, (true, true, true, true), color);
    }
    fn rect_with_radius_corners(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32, radius: f32, corners: (bool, bool, bool, bool)) {
        self.rounded_rect(Rect { x, y, width: w, height: h }, radius, corners, color);
    }
    fn text(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4]) {
        let c = [
            (color[0] * 255.0).clamp(0.0, 255.0) as u8,
            (color[1] * 255.0).clamp(0.0, 255.0) as u8,
            (color[2] * 255.0).clamp(0.0, 255.0) as u8,
        ];
        PaintCtx::text(self, content, x, y, size, c);
    }
    fn text_with_font(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], font: &str) {
        crate::scene::paint::RenderTarget::text_with_font_and_bounds(self, content, x, y, size, color, font, None);
    }
    fn text_with_bounds(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], bounds: Option<[f32; 4]>) {
        let c = [
            (color[0] * 255.0).clamp(0.0, 255.0) as u8,
            (color[1] * 255.0).clamp(0.0, 255.0) as u8,
            (color[2] * 255.0).clamp(0.0, 255.0) as u8,
        ];
        self.text_with(content, x, y, size, c, None, bounds);
    }
    fn text_with_font_and_bounds(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], font: &str, bounds: Option<[f32; 4]>) {
        let c = [
            (color[0] * 255.0).clamp(0.0, 255.0) as u8,
            (color[1] * 255.0).clamp(0.0, 255.0) as u8,
            (color[2] * 255.0).clamp(0.0, 255.0) as u8,
        ];
        self.text_with(content, x, y, size, c, Some(font.to_string()), bounds);
    }
    fn push_clip_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.push_clip(Rect { x, y, width: w, height: h });
    }
    fn pop_clip_rect(&mut self) {
        self.pop_clip();
    }
    fn inset_plate(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32, radius: f32, depth: f32) {
        PaintCtx::inset_plate(self, Rect { x, y, width: w, height: h }, (radius, radius, radius, radius), Material::face(color).as_ref(), depth);
    }
    fn inset_plate_tinted(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32, radius: f32, depth: f32, tint: [f32; 3]) {
        PaintCtx::inset_plate_tinted(self, Rect { x, y, width: w, height: h }, (radius, radius, radius, radius), Material::face(color).as_ref(), depth, tint);
    }
    fn relief_carve(&mut self, carve: &crate::scene::paint::ReliefCarve) {
        PaintCtx::carve(self, carve);
    }
}
