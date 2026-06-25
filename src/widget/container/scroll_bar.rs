use crate::widget::*;
use crate::context::UiContext;

pub struct ScrollBar {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    pub scroll_y: f32,
    pub content_h: f32,
    pub viewport_h: f32,
    pub hovered: bool,
    pub dragging: bool,
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub children: Vec<*mut (dyn Element + 'static)>,
}

impl std::fmt::Debug for ScrollBar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScrollBar")
            .field("x", &self.x)
            .field("y", &self.y)
            .field("w", &self.w)
            .field("h", &self.h)
            .field("scroll_y", &self.scroll_y)
            .field("content_h", &self.content_h)
            .field("viewport_h", &self.viewport_h)
            .field("hovered", &self.hovered)
            .field("dragging", &self.dragging)
            .finish()
    }
}

impl Clone for ScrollBar {
    fn clone(&self) -> Self {
        Self {
            x: self.x,
            y: self.y,
            w: self.w,
            h: self.h,
            scroll_y: self.scroll_y,
            content_h: self.content_h,
            viewport_h: self.viewport_h,
            hovered: self.hovered,
            dragging: self.dragging,
            parent: self.parent,
            children: self.children.clone(),
        }
    }
}

impl ScrollBar {
    pub fn new() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w: 6.0,
            h: 0.0,
            scroll_y: 0.0,
            content_h: 0.0,
            viewport_h: 0.0,
            hovered: false,
            dragging: false,
            parent: None,
            children: Vec::new(),
        }
    }

    pub fn update(&mut self, scroll_y: f32, content_h: f32, viewport_h: f32) {
        self.scroll_y = scroll_y;
        self.content_h = content_h;
        self.viewport_h = viewport_h;
    }

    pub fn get_thumb_rect(&self) -> Option<(f32, f32, f32, f32)> {
        if self.content_h <= self.viewport_h || self.viewport_h <= 0.0 || self.h <= 0.0 {
            return None;
        }
        let sb_x = self.x;
        let sb_w = self.w;
        let sb_track_h = self.h;
        let sb_track_y = self.y;

        let visible_ratio = self.viewport_h / self.content_h;
        let thumb_h = if sb_track_h <= 20.0 {
            sb_track_h
        } else {
            (sb_track_h * visible_ratio).clamp(20.0, sb_track_h)
        };
        let max_scroll = (self.content_h - self.viewport_h).max(0.0);
        let scroll_ratio = if max_scroll > 0.0 { self.scroll_y / max_scroll } else { 0.0 };
        let thumb_y = sb_track_y + scroll_ratio * (sb_track_h - thumb_h);

        Some((sb_x, thumb_y, sb_w, thumb_h))
    }
}

impl Element for ScrollBar {
    fn base(&self) -> Option<&Widget> { None }
    fn base_mut(&mut self) -> Option<&mut Widget> { None }

    fn rect(&self) -> (f32, f32, f32, f32) {
        (self.x, self.y, self.w, self.h)
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.x = x;
        self.y = y;
        self.w = w;
        self.h = h;
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn as_ptr(&self) -> *mut (dyn Element + 'static) {
        self as *const Self as *mut Self as *mut (dyn Element + 'static)
    }

    fn as_ptr_mut(&mut self) -> *mut (dyn Element + 'static) {
        self as *mut Self as *mut (dyn Element + 'static)
    }

    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    fn hovered(&self) -> bool {
        self.hovered
    }

    fn hit_test(&self, px: f32, py: f32, ctx: &UiContext) -> bool {
        if ctx.is_coordinate_covered(self as *const Self as *const () as usize, px, py) {
            return false;
        }
        let hit_margin = 6.0;
        px >= self.x - hit_margin && px <= self.x + self.w + hit_margin && py >= self.y && py <= self.y + self.h
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        let mut changed = false;
        let is_hit = self.hit_test(px, py, ctx);
        if self.hovered != is_hit {
            self.hovered = is_hit;
            changed = true;
        }

        if self.dragging {
            if let Some((_, _, _, thumb_h)) = self.get_thumb_rect() {
                let sb_track_y = self.y;
                let sb_track_h = self.h;
                let track_scroll_range = sb_track_h - thumb_h;
                if track_scroll_range > 0.0 {
                    let mouse_y_in_track = (py - sb_track_y).clamp(0.0, sb_track_h);
                    let scroll_ratio = (mouse_y_in_track - thumb_h / 2.0) / track_scroll_range;
                    let max_scroll = (self.content_h - self.viewport_h).max(0.0);
                    let new_scroll_y = (scroll_ratio.clamp(0.0, 1.0) * max_scroll).clamp(0.0, max_scroll);
                    if (self.scroll_y - new_scroll_y).abs() > 0.01 {
                        self.scroll_y = new_scroll_y;
                        changed = true;

                        if let Some(parent_ptr) = self.parent {
                            unsafe {
                                if let Some(page) = (*parent_ptr).as_any_mut().downcast_mut::<Page>() {
                                    page.scroll_y = new_scroll_y;
                                    let (px, py, pw, ph) = page.rect();
                                    page.set_rect(px, py, pw, ph);
                                }
                            }
                        }
                    }
                }
            }
        }
        changed
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if button == MouseButton::Left {
            match state {
                ElementState::Pressed => {
                    if self.hit_test(px, py, ctx) {
                        self.dragging = true;
                        self.on_cursor_moved(px, py, ctx);
                        return true;
                    }
                }
                ElementState::Released => {
                    if self.dragging {
                        self.dragging = false;
                        return true;
                    }
                }
            }
        }
        false
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        if self.content_h > self.viewport_h && self.h > 0.0 {
            quads.push((self.x, self.y, self.w, self.h, [0.15, 0.15, 0.20, 0.3]));

            if let Some((sb_x, thumb_y, sb_w, thumb_h)) = self.get_thumb_rect() {
                let thumb_color = if self.dragging {
                    [0.70, 0.70, 0.75, 0.6]
                } else if self.hovered {
                    [0.65, 0.65, 0.70, 0.5]
                } else {
                    [0.60, 0.60, 0.65, 0.4]
                };
                quads.push((sb_x, thumb_y, sb_w, thumb_h, thumb_color));
            }
        }
        quads
    }

    fn parent(&self, _ctx: &UiContext) -> Option<*mut (dyn Element + 'static)> {
        self.parent
    }

    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>, _ctx: &mut UiContext) {
        self.parent = parent;
    }

    fn children(&self, _ctx: &UiContext) -> Vec<*mut (dyn Element + 'static)> {
        self.children.clone()
    }

    fn add_child(&mut self, child: *mut (dyn Element + 'static), _ctx: &mut UiContext) {
        self.children.push(child);
    }

    fn clear_children(&mut self, _ctx: &mut UiContext) {
        self.children.clear();
    }
}

impl Drop for ScrollBar {
    fn drop(&mut self) {
        clear_widget_references(self);
    }
}

unsafe impl Send for ScrollBar {}
unsafe impl Sync for ScrollBar {}
