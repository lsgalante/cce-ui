use crate::widget::*;

#[derive(Debug, Clone)]
pub struct ScrollBox {
    x: f32, y: f32, w: f32, h: f32,
    pub scroll_y: f32,
    pub content_h: f32,
    pub viewport_y: f32,
    pub viewport_h: f32,
    viewport_offset_y: f32,
    viewport_offset_h: f32,
    hovered: bool,
    pub show_border: bool,
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub children: Vec<*mut (dyn Element + 'static)>,
}

impl ScrollBox {
    pub fn new() -> Self {
        Self {
            x: 0.0, y: 0.0, w: 0.0, h: 0.0,
            scroll_y: 0.0,
            content_h: 0.0,
            viewport_y: 0.0,
            viewport_h: 0.0,
            viewport_offset_y: 0.0,
            viewport_offset_h: 0.0,
            hovered: false,
            show_border: true,
            parent: None,
            children: Vec::new(),
        }
    }

    pub fn update_bounds(&mut self, content_h: f32, viewport_y: f32, viewport_h: f32) {
        self.content_h = content_h;
        self.viewport_y = viewport_y;
        self.viewport_h = viewport_h;
        self.viewport_offset_y = viewport_y - self.y;
        self.viewport_offset_h = viewport_h - self.h;
        let max_scroll = (content_h - viewport_h).max(0.0);
        self.scroll_y = self.scroll_y.clamp(0.0, max_scroll);
    }

    pub fn get_item_draw_y(&self, virtual_y: f32, item_h: f32) -> Option<f32> {
        let draw_y = self.viewport_y + virtual_y - self.scroll_y;
        if draw_y >= self.viewport_y - 1.0 && draw_y + item_h <= self.viewport_y + self.viewport_h + 1.0 {
            Some(draw_y)
        } else {
            None
        }
    }
}

impl Element for ScrollBox {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.x = x;
        self.y = y;
        self.w = w;
        self.h = h;
        self.viewport_y = y + self.viewport_offset_y;
        self.viewport_h = h + self.viewport_offset_h;
    }
    fn color(&self) -> [f32; 4] { crate::color::scrollinglist_bg_color() }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }
    fn highlight_color(&self, ctx: &UiContext) -> Option<[f32; 4]> { None }

    fn focus(&mut self) {
        focus::set_focused(self);
    }
    fn unfocus(&mut self) {}

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if button == MouseButton::Left && state == ElementState::Pressed {
            if self.hit_test(px, py, ctx) {
                self.focus();
                return true;
            }
        }
        false
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        let was = self.hovered;
        self.hovered = self.hit_test(px, py, ctx);
        was != self.hovered
    }

    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if self.hit_test(px, py, ctx) {
            let scroll_speed = 24.0;
            let dy = match delta {
                MouseScrollDelta::LineDelta(_, y) => -y * scroll_speed,
                MouseScrollDelta::PixelDelta(pos) => -pos.y as f32,
            };
            let old_scroll = self.scroll_y;
            let max_scroll = (self.content_h - self.viewport_h).max(0.0);
            self.scroll_y = (self.scroll_y + dy).clamp(0.0, max_scroll);
            (self.scroll_y - old_scroll).abs() > 0.01
        } else {
            false
        }
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        
        // Background
        quads.push((self.x, self.y, self.w, self.h, crate::color::scrollinglist_bg_color()));

        // Border lines
        let box_border_color = if focus::is_focused(self) {
            [0.30, 0.50, 0.32, 1.0] // Focused green
        } else if self.hovered {
            [0.25, 0.25, 0.35, 1.0] // Hovered
        } else {
            [0.18, 0.18, 0.24, 1.0] // Default
        };
        quads.push((self.x, self.y, self.w, 1.0, box_border_color)); // Top
        quads.push((self.x, self.y + self.h - 1.0, self.w, 1.0, box_border_color)); // Bottom
        quads.push((self.x, self.y, 1.0, self.h, box_border_color)); // Left
        quads.push((self.x + self.w - 1.0, self.y, 1.0, self.h, box_border_color)); // Right

        // Scrollbar
        if self.content_h > self.viewport_h {
            let sb_x = self.x + self.w - 8.0;
            let sb_w = 4.0;
            let sb_track_h = self.viewport_h - 8.0;
            let sb_track_y = self.viewport_y + 4.0;

            // Track
            quads.push((sb_x, sb_track_y, sb_w, sb_track_h, [0.15, 0.15, 0.20, 0.3]));

            // Thumb
            let visible_ratio = self.viewport_h / self.content_h;
            let thumb_h = (sb_track_h * visible_ratio).clamp(20.0, sb_track_h);
            let max_scroll = (self.content_h - self.viewport_h).max(0.0);
            let scroll_ratio = if max_scroll > 0.0 { self.scroll_y / max_scroll } else { 0.0 };
            let thumb_y = sb_track_y + scroll_ratio * (sb_track_h - thumb_h);

            quads.push((sb_x, thumb_y, sb_w, thumb_h, [0.60, 0.60, 0.65, 0.4]));
        }

        quads
    }

    fn keyboard_input(&mut self, event: &KeyEvent, ctx: &mut UiContext) -> bool {
        if !focus::is_focused(self) {
            return false;
        }
        if event.state != ElementState::Pressed {
            return false;
        }
        if event.ctrl {
            match &event.logical_key {
                Key::Character(c) if c == "n" || c == "N" => {
                    let old_scroll = self.scroll_y;
                    let max_scroll = (self.content_h - self.viewport_h).max(0.0);
                    self.scroll_y = (self.scroll_y + 24.0).clamp(0.0, max_scroll);
                    (self.scroll_y - old_scroll).abs() > 0.01
                }
                Key::Character(c) if c == "p" || c == "P" => {
                    let old_scroll = self.scroll_y;
                    let max_scroll = (self.content_h - self.viewport_h).max(0.0);
                    self.scroll_y = (self.scroll_y - 24.0).clamp(0.0, max_scroll);
                    (self.scroll_y - old_scroll).abs() > 0.01
                }
                _ => false,
            }
        } else {
            match &event.logical_key {
                Key::Named(NamedKey::ArrowDown) => {
                    let old_scroll = self.scroll_y;
                    let max_scroll = (self.content_h - self.viewport_h).max(0.0);
                    self.scroll_y = (self.scroll_y + 24.0).clamp(0.0, max_scroll);
                    (self.scroll_y - old_scroll).abs() > 0.01
                }
                Key::Named(NamedKey::ArrowUp) => {
                    let old_scroll = self.scroll_y;
                    let max_scroll = (self.content_h - self.viewport_h).max(0.0);
                    self.scroll_y = (self.scroll_y - 24.0).clamp(0.0, max_scroll);
                    (self.scroll_y - old_scroll).abs() > 0.01
                }
                _ => false,
            }
        }
    }

    fn parent(&self, ctx: &UiContext) -> Option<*mut (dyn Element + 'static)> { self.parent }
    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>, ctx: &mut UiContext) { self.parent = parent; }
    fn children(&self, ctx: &UiContext) -> Vec<*mut (dyn Element + 'static)> { self.children.clone() }
    fn add_child(&mut self, child: *mut (dyn Element + 'static), ctx: &mut UiContext) { self.children.push(child); }
    fn clear_children(&mut self, ctx: &mut UiContext) { self.children.clear(); }
}

impl Drop for ScrollBox {
    fn drop(&mut self) {
        focus::clear_if_matches(self);
    }
}

unsafe impl Send for ScrollBox {}
unsafe impl Sync for ScrollBox {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scroll_box_bounds_scrolling() {
        let mut sb = ScrollBox::new();
        sb.set_rect(10.0, 20.0, 100.0, 100.0);
        
        // 1. Initially scroll is 0
        assert_eq!(sb.scroll_y, 0.0);

        // 2. Update bounds: content_h = 150 (greater than viewport_h = 100)
        sb.update_bounds(150.0, 20.0, 100.0);
        assert_eq!(sb.scroll_y, 0.0);
        assert_eq!(sb.content_h, 150.0);
        assert_eq!(sb.viewport_h, 100.0);

        // 3. Scroll inside bounds
        let delta = MouseScrollDelta::LineDelta(0.0, -2.0); // scroll down by 2 lines (48px)
        let mut dummy = UiContext::new();
        let changed = sb.mouse_wheel(&delta, 50.0, 50.0, &mut dummy);
        assert!(changed);
        assert_eq!(sb.scroll_y, 48.0);

        // 4. Clamps at max scroll: 150 - 100 = 50
        let delta_large = MouseScrollDelta::LineDelta(0.0, -10.0);
        sb.mouse_wheel(&delta_large, 50.0, 50.0, &mut dummy);
        assert_eq!(sb.scroll_y, 50.0);

        // 5. Test item draw coordinates
        // Virtual item at virtual_y = 10, item_h = 24
        // Screen draw y = viewport_y + virtual_y - scroll_y = 20 + 10 - 50 = -20
        // -20 < viewport_y + 2.0 (22.0), so it should return None (not visible)
        assert!(sb.get_item_draw_y(10.0, 24.0).is_none());

        // Virtual item at virtual_y = 60, item_h = 24
        // Screen draw y = 20 + 60 - 50 = 30
        // 30 >= 22.0 and 30 + 24 <= 118.0, so it should return Some(30.0)
        assert_eq!(sb.get_item_draw_y(60.0, 24.0), Some(30.0));
    }
}
