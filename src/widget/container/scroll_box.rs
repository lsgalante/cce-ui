use crate::widget::*;

#[derive(Debug, Clone)]
pub struct ScrollBox {
    pub base: Widget,
    pub scroll_y: f32,
    pub content_h: f32,
    pub viewport_y: f32,
    pub viewport_h: f32,
    viewport_offset_y: f32,
    viewport_offset_h: f32,
    pub show_border: bool,
    pub show_background: bool,
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub children: Vec<*mut (dyn Element + 'static)>,
    pub scrollbar_dragging: bool,
    pub drag_offset_y: f32,
}

impl ScrollBox {
    pub fn new() -> Self {
        Self {
            base: Widget::new(),
            scroll_y: 0.0,
            content_h: 0.0,
            viewport_y: 0.0,
            viewport_h: 0.0,
            viewport_offset_y: 0.0,
            viewport_offset_h: 0.0,
            show_border: true,
            show_background: true,
            parent: None,
            children: Vec::new(),
            scrollbar_dragging: false,
            drag_offset_y: 0.0,
        }
    }

    pub fn update_bounds(&mut self, content_h: f32, viewport_y: f32, viewport_h: f32) {
        self.content_h = content_h;
        self.viewport_y = viewport_y;
        self.viewport_h = viewport_h;
        self.viewport_offset_y = viewport_y - self.base.y;
        self.viewport_offset_h = viewport_h - self.base.h;
        let max_scroll = (content_h - viewport_h).max(0.0);
        self.scroll_y = self.scroll_y.clamp(0.0, max_scroll);
    }

    pub fn hit_test_scrollbar(&self, px: f32, py: f32) -> bool {
        if self.content_h <= self.viewport_h {
            return false;
        }
        let sb_w = crate::layout::scrollbar_width();
        let sb_x = self.base.x + self.base.w - sb_w - 4.0;
        let sb_track_h = self.viewport_h - 8.0;
        let sb_track_y = self.viewport_y + 4.0;

        px >= sb_x - 4.0 && px <= sb_x + sb_w + 4.0
            && py >= sb_track_y && py <= sb_track_y + sb_track_h
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
    crate::impl_widget_base!(ScrollBox);
    fn is_scrollable(&self) -> bool { true }
    fn blocks_backplate_drag(&self) -> bool { true }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.base.x = x;
        self.base.y = y;
        self.base.w = w;
        self.base.h = h;
        self.viewport_y = y + self.viewport_offset_y;
        self.viewport_h = h + self.viewport_offset_h;
    }
    fn color(&self) -> [f32; 4] { crate::color::list_bg_color() }

    fn corner_radius(&self) -> f32 {
        crate::layout::list_corner_radius()
    }

    fn focus(&mut self) {
        focus::set_focused(self);
    }
    fn unfocus(&mut self) {}

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if button == MouseButton::Left {
            if state == ElementState::Pressed {
                if self.hit_test_scrollbar(px, py) {
                    self.focus();
                    self.scrollbar_dragging = true;
                    
                    let sb_track_h = self.viewport_h - 8.0;
                    let sb_track_y = self.viewport_y + 4.0;
                    let visible_ratio = self.viewport_h / self.content_h;
                    let thumb_h = if sb_track_h <= 20.0 {
                        sb_track_h
                    } else {
                        (sb_track_h * visible_ratio).clamp(20.0, sb_track_h)
                    };
                    let max_scroll = (self.content_h - self.viewport_h).max(0.0);
                    let scroll_ratio = if max_scroll > 0.0 { self.scroll_y / max_scroll } else { 0.0 };
                    let thumb_y = sb_track_y + scroll_ratio * (sb_track_h - thumb_h);
                    
                    let click_offset = py - thumb_y;
                    if click_offset >= 0.0 && click_offset <= thumb_h {
                        self.drag_offset_y = click_offset;
                    } else {
                        // Clicked outside the thumb: jump thumb center to py
                        self.drag_offset_y = thumb_h / 2.0;
                        let target_thumb_y = py - self.drag_offset_y;
                        let new_scroll_ratio = if sb_track_h - thumb_h > 0.0 {
                            ((target_thumb_y - sb_track_y) / (sb_track_h - thumb_h)).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        self.scroll_y = new_scroll_ratio * max_scroll;
                    }
                    return true;
                } else {
                    self.scrollbar_dragging = false;
                }
                if self.hit_test(px, py, ctx) {
                    self.focus();
                    return true;
                }
            } else if state == ElementState::Released {
                self.scrollbar_dragging = false;
            }
        }
        false
    }

    fn draggable(&self) -> bool {
        self.scrollbar_dragging
    }

    fn drag_begin(&mut self, _px: f32, _py: f32) {}

    fn drag_update(&mut self, _px: f32, py: f32) -> bool {
        if !self.scrollbar_dragging {
            return false;
        }
        let sb_track_h = self.viewport_h - 8.0;
        let sb_track_y = self.viewport_y + 4.0;
        let visible_ratio = self.viewport_h / self.content_h;
        let thumb_h = if sb_track_h <= 20.0 {
            sb_track_h
        } else {
            (sb_track_h * visible_ratio).clamp(20.0, sb_track_h)
        };
        let max_scroll = (self.content_h - self.viewport_h).max(0.0);
        
        let target_thumb_y = py - self.drag_offset_y;
        let new_scroll_ratio = if sb_track_h - thumb_h > 0.0 {
            ((target_thumb_y - sb_track_y) / (sb_track_h - thumb_h)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        
        let old_scroll = self.scroll_y;
        self.scroll_y = new_scroll_ratio * max_scroll;
        (self.scroll_y - old_scroll).abs() > 0.01
    }

    fn drag_end(&mut self) {
        self.scrollbar_dragging = false;
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        let was = self.base.hovered;
        self.base.hovered = self.hit_test(px, py, ctx);
        was != self.base.hovered
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
        if self.show_background {
            quads.push((self.base.x, self.base.y, self.base.w, self.base.h, crate::color::list_bg_color()));
        }



        // Scrollbar
        if self.content_h > self.viewport_h {
            let sb_w = crate::layout::scrollbar_width();
            let sb_x = self.base.x + self.base.w - sb_w - 4.0;
            let sb_track_h = self.viewport_h - 8.0;
            let sb_track_y = self.viewport_y + 4.0;

            // Track
            quads.push((sb_x, sb_track_y, sb_w, sb_track_h, crate::color::scrollbar_track_color()));

            // Thumb
            let visible_ratio = self.viewport_h / self.content_h;
            let thumb_h = if sb_track_h <= 20.0 {
                sb_track_h
            } else {
                (sb_track_h * visible_ratio).clamp(20.0, sb_track_h)
            };
            let max_scroll = (self.content_h - self.viewport_h).max(0.0);
            let scroll_ratio = if max_scroll > 0.0 { self.scroll_y / max_scroll } else { 0.0 };
            let thumb_y = sb_track_y + scroll_ratio * (sb_track_h - thumb_h);

            quads.push((sb_x, thumb_y, sb_w, thumb_h, crate::color::scrollbar_thumb_color()));
        }

        quads
    }

    fn keyboard_input(&mut self, event: &KeyEvent, ctx: &mut UiContext) -> bool {
        let self_addr = self as *const Self as *const () as usize;
        let has_focus = ctx.is_focused_addr(self_addr) || {
            let mut current = ctx.focused_widget;
            let mut found = false;
            while let Some(ptr) = current {
                let ptr_addr = ptr as *const () as usize;
                if ptr_addr == self_addr {
                    found = true;
                    break;
                }
                current = unsafe { (*ptr).parent(ctx) };
            }
            found
        };

        let is_hovered = self.hit_test(ctx.cursor_pos.0, ctx.cursor_pos.1, ctx);
        if !has_focus && !is_hovered {
            return false;
        }
        if event.state != ElementState::Pressed {
            return false;
        }
        let max_scroll = (self.content_h - self.viewport_h).max(0.0);
        if event.ctrl {
            match &event.logical_key {
                Key::Character(c) if c == "n" || c == "N" => {
                    let old_scroll = self.scroll_y;
                    self.scroll_y = (self.scroll_y + 24.0).clamp(0.0, max_scroll);
                    (self.scroll_y - old_scroll).abs() > 0.01
                }
                Key::Character(c) if c == "p" || c == "P" => {
                    let old_scroll = self.scroll_y;
                    self.scroll_y = (self.scroll_y - 24.0).clamp(0.0, max_scroll);
                    (self.scroll_y - old_scroll).abs() > 0.01
                }
                _ => false,
            }
        } else {
            match &event.logical_key {
                Key::Named(NamedKey::ArrowDown) => {
                    let old_scroll = self.scroll_y;
                    self.scroll_y = (self.scroll_y + 24.0).clamp(0.0, max_scroll);
                    (self.scroll_y - old_scroll).abs() > 0.01
                }
                Key::Named(NamedKey::ArrowUp) => {
                    let old_scroll = self.scroll_y;
                    self.scroll_y = (self.scroll_y - 24.0).clamp(0.0, max_scroll);
                    (self.scroll_y - old_scroll).abs() > 0.01
                }
                Key::Named(NamedKey::PageDown) => {
                    let old_scroll = self.scroll_y;
                    self.scroll_y = (self.scroll_y + self.viewport_h).clamp(0.0, max_scroll);
                    (self.scroll_y - old_scroll).abs() > 0.01
                }
                Key::Named(NamedKey::PageUp) => {
                    let old_scroll = self.scroll_y;
                    self.scroll_y = (self.scroll_y - self.viewport_h).clamp(0.0, max_scroll);
                    (self.scroll_y - old_scroll).abs() > 0.01
                }
                Key::Named(NamedKey::Home) => {
                    let old_scroll = self.scroll_y;
                    self.scroll_y = 0.0;
                    (self.scroll_y - old_scroll).abs() > 0.01
                }
                Key::Named(NamedKey::End) => {
                    let old_scroll = self.scroll_y;
                    self.scroll_y = max_scroll;
                    (self.scroll_y - old_scroll).abs() > 0.01
                }
                _ => false,
            }
        }
    }

    fn parent(&self, _ctx: &UiContext) -> Option<*mut (dyn Element + 'static)> { self.parent }
    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>, _ctx: &mut UiContext) { self.parent = parent; }
    fn children(&self, _ctx: &UiContext) -> Vec<*mut (dyn Element + 'static)> { self.children.clone() }
    fn add_child(&mut self, child: *mut (dyn Element + 'static), _ctx: &mut UiContext) { self.children.push(child); }
    fn clear_children(&mut self, _ctx: &mut UiContext) { self.children.clear(); }

    fn layout_ignore(&self) -> bool {
        true
    }
}

impl Drop for ScrollBox {
    fn drop(&mut self) {
        clear_widget_references(self);
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

    #[test]
    fn test_scroll_box_keyboard_input() {
        let mut sb = ScrollBox::new();
        sb.set_rect(10.0, 20.0, 100.0, 100.0);
        sb.update_bounds(300.0, 20.0, 100.0); // max_scroll = 200.0
        
        let mut ctx = UiContext::new();
        // Focus the scroll box
        ctx.set_focused(&mut sb);
        
        // 1. ArrowDown key
        let event_down = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::ArrowDown),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
        };
        assert!(sb.keyboard_input(&event_down, &mut ctx));
        assert_eq!(sb.scroll_y, 24.0);

        // 2. PageDown key
        let event_pgdown = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::PageDown),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
        };
        assert!(sb.keyboard_input(&event_pgdown, &mut ctx));
        assert_eq!(sb.scroll_y, 124.0);

        // 3. End key
        let event_end = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::End),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
        };
        assert!(sb.keyboard_input(&event_end, &mut ctx));
        assert_eq!(sb.scroll_y, 200.0); // clamps at max_scroll = 200.0

        // 4. PageUp key
        let event_pgup = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::PageUp),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
        };
        assert!(sb.keyboard_input(&event_pgup, &mut ctx));
        assert_eq!(sb.scroll_y, 100.0);

        // 5. Home key
        let event_home = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::Home),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
        };
        assert!(sb.keyboard_input(&event_home, &mut ctx));
        assert_eq!(sb.scroll_y, 0.0);
    }

    #[test]
    fn test_scroll_box_non_focused_hovered_scrolling() {
        let mut sb = ScrollBox::new();
        sb.set_rect(10.0, 20.0, 100.0, 100.0);
        sb.update_bounds(300.0, 20.0, 100.0); // max_scroll = 200.0

        let mut ctx = UiContext::new();
        ctx.register_widget(sb.base.id(), &mut sb);
        
        // Set cursor position over the scroll box
        ctx.set_cursor_pos(50.0, 50.0);

        let event_down = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::ArrowDown),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
        };

        let root_ptr = &mut sb as *mut ScrollBox as *mut (dyn Element + 'static);
        assert!(ctx.propagate_event(&Event::KeyInput(event_down), root_ptr));
        assert_eq!(sb.scroll_y, 24.0);
    }
}
