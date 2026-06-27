use crate::widget::*;
use crate::context::UiContext;
use crate::widget::display::TextLabel;
use super::layer::Layer;

pub trait PageLayout {
    fn layout(&self, x: f32, y: f32, w: f32, h: f32, children: &[*mut (dyn Element + 'static)], ctx: &mut UiContext) -> f32;
}

#[derive(Debug, Clone, Copy)]
pub struct VerticalLayout {
    pub padding_x: f32,
    pub padding_y: f32,
    pub spacing: f32,
}

impl Default for VerticalLayout {
    fn default() -> Self {
        Self {
            padding_x: 8.0,
            padding_y: 10.0,
            spacing: 8.0,
        }
    }
}

impl PageLayout for VerticalLayout {
    fn layout(&self, x: f32, y: f32, w: f32, _h: f32, children: &[*mut (dyn Element + 'static)], ctx: &mut UiContext) -> f32 {
        let padding_x = self.padding_x;
        let padding_y = self.padding_y;
        let left_x = x + padding_x;
        let available_w = (w - 2.0 * padding_x).max(1.0);
        let mut current_y = y + padding_y;
        let spacing = self.spacing;

        for &child_ptr in children {
            let child = unsafe { &mut *child_ptr };
            let (_, _, _, ch) = child.rect();
            let use_h = if ch > 0.0 { ch } else { 42.0 };
            child.layout(
                Point { x: left_x, y: current_y },
                LayoutConstraints::new(available_w, available_w, use_h, use_h),
                ctx,
            );
            current_y += use_h + spacing;
        }
        current_y - y
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ColumnsLayout {
    pub padding_x: f32,
    pub padding_y: f32,
    pub spacing: f32,
}

impl Default for ColumnsLayout {
    fn default() -> Self {
        Self {
            padding_x: 8.0,
            padding_y: 10.0,
            spacing: 12.0,
        }
    }
}

impl PageLayout for ColumnsLayout {
    fn layout(&self, x: f32, y: f32, w: f32, h: f32, children: &[*mut (dyn Element + 'static)], ctx: &mut UiContext) -> f32 {
        let count = children.len();
        if count == 0 {
            return 0.0;
        }
        let total_spacing = self.spacing * (count - 1) as f32;
        let total_padding = self.padding_x * 2.0;
        let available_w = (w - total_padding - total_spacing).max(1.0);
        let col_w = available_w / count as f32;
        let use_h = (h - 2.0 * self.padding_y).max(1.0);
        let start_y = y + self.padding_y;

        let mut current_x = x + self.padding_x;
        for &child_ptr in children {
            let child = unsafe { &mut *child_ptr };
            child.layout(
                Point { x: current_x, y: start_y },
                LayoutConstraints::new(col_w, col_w, use_h, use_h),
                ctx,
            );
            current_x += col_w + self.spacing;
        }
        use_h + 2.0 * self.padding_y
    }
}

pub struct Page {
    pub base: Layer,
    pub visible: bool,
    pub owned_children: Vec<Box<dyn Element>>,
    pub layout: Box<dyn PageLayout>,
    pub scroll_y: f32,
    pub content_h: f32,
    pub scroll_bar: ScrollBar,
}

impl std::fmt::Debug for Page {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Page")
            .field("base", &self.base)
            .field("visible", &self.visible)
            .field("scroll_y", &self.scroll_y)
            .field("content_h", &self.content_h)
            .finish()
    }
}

impl Page {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            base: Layer::new(x, y, w, h),
            visible: true,
            owned_children: Vec::new(),
            layout: Box::new(VerticalLayout::default()),
            scroll_y: 0.0,
            content_h: 0.0,
            scroll_bar: ScrollBar::new(),
        }
    }

    pub fn with_layout(mut self, layout: Box<dyn PageLayout>) -> Self {
        self.layout = layout;
        self
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.base.base.label = Some(label.to_string());
        self
    }

    pub fn add_child_owned(&mut self, child: Box<dyn Element>, ctx: &mut UiContext) {
        let ptr = &*child as *const (dyn Element + 'static) as *mut (dyn Element + 'static);
        self.owned_children.push(child);
        self.add_child(ptr, ctx);
    }
}

fn clip_quad(
    quad: (f32, f32, f32, f32, [f32; 4]),
    bounds: (f32, f32, f32, f32),
) -> Option<(f32, f32, f32, f32, [f32; 4])> {
    let (qx, qy, qw, qh, qc) = quad;
    let (bx, by, bw, bh) = bounds;

    let x1 = qx.max(bx);
    let y1 = qy.max(by);
    let x2 = (qx + qw).min(bx + bw);
    let y2 = (qy + qh).min(by + bh);

    let w = x2 - x1;
    let h = y2 - y1;

    if w > 0.0 && h > 0.0 {
        Some((x1, y1, w, h, qc))
    } else {
        None
    }
}

impl Element for Page {
    fn base(&self) -> Option<&Widget> { Some(&self.base.base) }
    fn is_page(&self) -> bool { true }
    fn blocks_backplate_drag(&self) -> bool { false }

    fn check_out_of_bounds(&self, event: &Event, _ctx: &UiContext) -> bool {
        if self.scroll_bar.dragging {
            return false;
        }
        if let Event::PointerMove { x, y, .. }
        | Event::MouseButton { x, y, .. }
        | Event::MouseWheel { x, y, .. } = event
        {
            let (rx, ry, rw, rh) = self.rect();
            let screen_y = *y - self.scroll_y;
            if *x < rx || *x > rx + rw || screen_y < ry || screen_y > ry + rh {
                return true;
            }
        }
        false
    }

    fn transform_event_for_child(&self, child: *mut (dyn Element + 'static), mut event: Event, _ctx: &UiContext) -> Event {
        let sb_ptr = &self.scroll_bar as *const ScrollBar as *mut ScrollBar as *mut (dyn Element + 'static);
        if std::ptr::addr_eq(child, sb_ptr) {
            match &mut event {
                Event::PointerMove { y, local_y, .. }
                | Event::MouseButton { y, local_y, .. }
                | Event::MouseWheel { y, local_y, .. } => {
                    *y -= self.scroll_y;
                    *local_y -= self.scroll_y;
                }
                _ => {}
            }
        }
        event
    }

    fn base_mut(&mut self) -> Option<&mut Widget> { Some(&mut self.base.base) }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn as_ptr(&self) -> *mut (dyn Element + 'static) {
        self as *const Self as *mut Self as *mut (dyn Element + 'static)
    }
    fn as_ptr_mut(&mut self) -> *mut (dyn Element + 'static) {
        self as *mut Self as *mut (dyn Element + 'static)
    }

    fn rect(&self) -> (f32, f32, f32, f32) {
        self.base.rect()
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.base.set_rect(x, y, w, h);
    }

    fn layout(&mut self, origin: Point, constraints: LayoutConstraints, ctx: &mut UiContext) {
        let size = self.measure(constraints, ctx);
        self.set_rect(origin.x, origin.y, size.width, size.height);
        
        if self.visible {
            let layout_content_h = self.layout.layout(origin.x, origin.y, size.width, size.height, &self.base.children, ctx);
            self.content_h = self.content_h.max(layout_content_h);

            let max_scroll = (self.content_h - size.height).max(0.0);
            self.scroll_y = self.scroll_y.clamp(0.0, max_scroll);

            let sb_w = 6.0;
            let sb_padding = 2.0;
            let sb_x = origin.x + size.width - sb_w - sb_padding;
            self.scroll_bar.set_rect(sb_x, origin.y + 4.0, sb_w, size.height - 8.0);
            self.scroll_bar.update(self.scroll_y, self.content_h, size.height);
            
            let self_ptr = self as *mut Page as *mut (dyn Element + 'static);
            self.scroll_bar.parent = Some(self_ptr);
        }
    }

    fn color(&self) -> [f32; 4] {
        let mut c = crate::colors::page_color();
        c[3] *= crate::layout::page_opacity();
        c
    }

    fn visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn add_child(&mut self, child: *mut (dyn Element + 'static), ctx: &mut UiContext) {
        let id = self.base.base.id();
        let self_ptr = self as *mut Page as *mut (dyn Element + 'static);
        self.base.children.push(child);
        unsafe {
            let c_id = (*child).base().map(|b| b.id()).unwrap();
            ctx.register_widget(c_id, child);
            ctx.link_ids(id, c_id);
            (*child).set_parent(Some(self_ptr), ctx);
        }
    }

    fn clear_children(&mut self, ctx: &mut UiContext) {
        self.base.clear_children(ctx);
        self.owned_children.clear();
    }

    fn children(&self, ctx: &UiContext) -> Vec<*mut (dyn Element + 'static)> {
        let mut list = self.base.children(ctx);
        let (_, _, _, h) = self.rect();
        if self.content_h > h {
            let sb_ptr = &self.scroll_bar as *const ScrollBar as *mut ScrollBar as *mut (dyn Element + 'static);
            list.push(sb_ptr);
        }
        list
    }

    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>, ctx: &mut UiContext) {
        self.base.set_parent(parent, ctx);
    }

    fn parent(&self, ctx: &UiContext) -> Option<*mut (dyn Element + 'static)> {
        self.base.parent(ctx)
    }

    fn all_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.visible {
            return Vec::new();
        }
        let mut quads = Vec::new();
        let c = self.color();
        let bounds = self.rect();
        if c[3] > 0.0 {
            quads.push((bounds.0, bounds.1, bounds.2, bounds.3, c));
        }
        
        for q in self.base.all_quads(ctx) {
            if let Some(clipped) = clip_quad(q, bounds) {
                quads.push(clipped);
            }
        }

        if self.content_h > bounds.3 {
            quads.extend(self.scroll_bar.all_quads(ctx));
        }

        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        if !self.visible {
            return Vec::new();
        }
        let (_, py, _, ph) = self.rect();
        let mut result = Vec::new();
        for label in self.base.text_labels() {
            if label.y >= py && label.y + label.font_size <= py + ph {
                result.push(label);
            }
        }
        result
    }

    fn text_labels_with_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<[f32; 4]>)> {
        if !self.visible {
            return Vec::new();
        }
        let (px, py, pw, ph) = self.rect();
        let page_bounds = [px, py, px + pw, py + ph];

        let mut result = Vec::new();
        for (label, bounds) in self.base.text_labels_with_bounds(ctx) {
            let intersected_bounds = if let Some([l, t, r, b]) = bounds {
                let il = l.max(page_bounds[0]);
                let it = t.max(page_bounds[1]);
                let ir = r.min(page_bounds[2]);
                let ib = b.min(page_bounds[3]);
                if il < ir && it < ib {
                    Some([il, it, ir, ib])
                } else {
                    continue;
                }
            } else {
                if label.y + label.font_size < py || label.y > py + ph {
                    continue;
                }
                Some(page_bounds)
            };
            result.push((label, intersected_bounds));
        }
        result
    }

    fn text_labels_with_font_and_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        if !self.visible {
            return Vec::new();
        }
        let (px, py, pw, ph) = self.rect();
        let page_bounds = [px, py, px + pw, py + ph];

        let mut result = Vec::new();
        for (label, font, bounds) in self.base.text_labels_with_font_and_bounds(ctx) {
            let intersected_bounds = if let Some([l, t, r, b]) = bounds {
                let il = l.max(page_bounds[0]);
                let it = t.max(page_bounds[1]);
                let ir = r.min(page_bounds[2]);
                let ib = b.min(page_bounds[3]);
                if il < ir && it < ib {
                    Some([il, it, ir, ib])
                } else {
                    continue;
                }
            } else {
                if label.y + label.font_size < py || label.y > py + ph {
                    continue;
                }
                Some(page_bounds)
            };
            result.push((label, font, intersected_bounds));
        }
        result
    }

    fn get_text_items(&self) -> Vec<(&glyphon::Buffer, f32, f32, glyphon::Color)> {
        if !self.visible {
            return Vec::new();
        }
        self.base.get_text_items()
    }

    fn hit_test(&self, px: f32, py: f32, _ctx: &UiContext) -> bool {
        if !self.visible {
            return false;
        }
        let (rx, ry, rw, rh) = self.rect();
        let screen_y = py - self.scroll_y;
        screen_y >= ry && screen_y <= ry + rh && px >= rx && px <= rx + rw
    }

    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if !self.visible {
            return false;
        }
        if self.hit_test(px, py, ctx) {
            let scroll_speed = 24.0;
            let dy = match delta {
                MouseScrollDelta::LineDelta(_, y) => -y * scroll_speed,
                MouseScrollDelta::PixelDelta(pos) => -pos.y as f32,
            };
            let old_scroll = self.scroll_y;
            let (x, y, w, h) = self.rect();
            let max_scroll = (self.content_h - h).max(0.0);
            self.scroll_y = (self.scroll_y + dy).clamp(0.0, max_scroll);
            self.scroll_bar.scroll_y = self.scroll_y;
            if (self.scroll_y - old_scroll).abs() > 0.01 {
                self.set_rect(x, y, w, h);
                self.mark_dirty(ctx);
                return true;
            }
        }
        false
    }

    fn keyboard_input(&mut self, event: &KeyEvent, ctx: &mut UiContext) -> bool {
        if !self.visible {
            return false;
        }
        if event.state != ElementState::Pressed {
            return false;
        }
        let (x, y, w, h) = self.rect();
        let max_scroll = (self.content_h - h).max(0.0);
        if max_scroll <= 0.0 {
            return false;
        }

        let old_scroll = self.scroll_y;
        match &event.logical_key {
            Key::Named(NamedKey::ArrowDown) => {
                self.scroll_y = (self.scroll_y + 24.0).clamp(0.0, max_scroll);
            }
            Key::Named(NamedKey::ArrowUp) => {
                self.scroll_y = (self.scroll_y - 24.0).clamp(0.0, max_scroll);
            }
            Key::Named(NamedKey::PageDown) => {
                self.scroll_y = (self.scroll_y + h).clamp(0.0, max_scroll);
            }
            Key::Named(NamedKey::PageUp) => {
                self.scroll_y = (self.scroll_y - h).clamp(0.0, max_scroll);
            }
            Key::Named(NamedKey::Home) => {
                self.scroll_y = 0.0;
            }
            Key::Named(NamedKey::End) => {
                self.scroll_y = max_scroll;
            }
            _ => return false,
        }

        self.scroll_bar.scroll_y = self.scroll_y;
        if (self.scroll_y - old_scroll).abs() > 0.01 {
            self.set_rect(x, y, w, h);
            self.mark_dirty(ctx);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_bounds_scrolled() {
        let ctx = UiContext::new();
        let mut page = Page::new(0.0, 0.0, 800.0, 600.0);
        
        // 1. Unscrolled check
        assert_eq!(page.scroll_y, 0.0);
        
        // Inside page boundaries (unscrolled)
        let event_in = Event::MouseButton {
            button: MouseButton::Left,
            state: ElementState::Pressed,
            x: 100.0,
            y: 200.0,
            local_x: 100.0,
            local_y: 200.0,
        };
        assert!(!page.check_out_of_bounds(&event_in, &ctx));
        assert!(page.hit_test(100.0, 200.0, &ctx));

        // Outside page boundaries (unscrolled)
        let event_out = Event::MouseButton {
            button: MouseButton::Left,
            state: ElementState::Pressed,
            x: 100.0,
            y: 700.0,
            local_x: 100.0,
            local_y: 700.0,
        };
        assert!(page.check_out_of_bounds(&event_out, &ctx));
        assert!(!page.hit_test(100.0, 700.0, &ctx));

        // 2. Scrolled check (scrolled down by 300px)
        page.scroll_y = 300.0;

        // Pointer virtual y = 500.0 (which translates to screen y = 200.0, inside page height of 600)
        let event_scrolled_in = Event::MouseButton {
            button: MouseButton::Left,
            state: ElementState::Pressed,
            x: 100.0,
            y: 500.0,
            local_x: 100.0,
            local_y: 500.0,
        };
        assert!(!page.check_out_of_bounds(&event_scrolled_in, &ctx));
        assert!(page.hit_test(100.0, 500.0, &ctx));

        // Pointer virtual y = 1000.0 (which translates to screen y = 700.0, outside page height of 600)
        let event_scrolled_out = Event::MouseButton {
            button: MouseButton::Left,
            state: ElementState::Pressed,
            x: 100.0,
            y: 1000.0,
            local_x: 100.0,
            local_y: 1000.0,
        };
        assert!(page.check_out_of_bounds(&event_scrolled_out, &ctx));
        assert!(!page.hit_test(100.0, 1000.0, &ctx));
    }

    #[test]
    fn test_page_keyboard_input() {
        let mut page = Page::new(0.0, 0.0, 800.0, 600.0);
        page.content_h = 1000.0; // max_scroll = 400.0
        
        let mut ctx = UiContext::new();
        
        // 1. ArrowDown
        let event_down = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::ArrowDown),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
        };
        assert!(page.keyboard_input(&event_down, &mut ctx));
        assert_eq!(page.scroll_y, 24.0);

        // 2. PageDown
        let event_pgdown = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::PageDown),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
        };
        assert!(page.keyboard_input(&event_pgdown, &mut ctx));
        assert_eq!(page.scroll_y, 400.0);

        // 3. PageUp
        let event_pgup = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::PageUp),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
        };
        assert!(page.keyboard_input(&event_pgup, &mut ctx));
        assert_eq!(page.scroll_y, 0.0);

        // 4. End
        let event_end = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::End),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
        };
        assert!(page.keyboard_input(&event_end, &mut ctx));
        assert_eq!(page.scroll_y, 400.0);

        // 5. Home
        let event_home = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::Home),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
        };
        assert!(page.keyboard_input(&event_home, &mut ctx));
        assert_eq!(page.scroll_y, 0.0);
    }
}

