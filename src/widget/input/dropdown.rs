use crate::colors;
use crate::widget::*;

#[derive(Debug, Clone)]
pub struct Dropdown {
    base: Widget,
    pub options: Vec<String>,
    pub selected: usize,
    pub open: bool,
    pub(crate) hovered_item: Option<usize>,
    just_changed: bool,
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub children: Vec<*mut (dyn Element + 'static)>,
    pub font_family: String,
    pub custom_display_text: Option<String>,
}

impl Dropdown {
    pub fn new(options: Vec<String>, selected: usize) -> Self {
        Self {
            base: Widget::new(),
            options,
            selected,
            open: false,
            hovered_item: None,
            just_changed: false,
            parent: None,
            children: Vec::new(),
            font_family: "sans-serif".to_string(),
            custom_display_text: None,
        }
    }

    pub fn with_custom_display_text(mut self, text: &str) -> Self {
        self.custom_display_text = Some(text.to_string());
        self
    }

    pub fn with_font_family(mut self, font_family: &str) -> Self {
        self.font_family = font_family.to_string();
        self
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self
    }

    pub fn with_config(mut self, file: &str, key: &str) -> Self {
        self.base.config_file = Some(file.to_string());
        self.base.config_key = Some(key.to_string());
        self
    }

    pub fn set_label(&mut self, label: &str) {
        self.base.label = Some(label.to_string());
    }

    pub fn take_change(&mut self) -> bool {
        let changed = self.just_changed;
        self.just_changed = false;
        changed
    }

    pub fn popover_width(&self) -> f32 {
        let mut w = self.base.w;
        let font_setting = crate::layout::dropdown_font();
        let (font_family, font_size_opt) = crate::layout::parse_font_string(&font_setting);
        let font_size = font_size_opt.unwrap_or(12.0);
        for opt in &self.options {
            let opt_w = crate::widget::display::measure_text_width(opt, &font_family, font_size) + 32.0;
            if opt_w > w {
                w = opt_w;
            }
        }
        w
    }

    pub fn get_dy_dh(&self) -> (f32, f32) {
        let open_upward = self.base.y > 400.0;
        let dh = self.options.len() as f32 * 24.0;
        let dy = if open_upward {
            self.base.y - dh
        } else {
            self.base.y + self.base.h
        };
        (dy, dh)
    }

    pub fn render_popover(&self, pc: &mut dyn crate::layout::RenderTarget) {
        if !self.open { return; }
        
        let (dy, dh) = self.get_dy_dh();
        let pw = self.popover_width();
        
        // 1. Soft layered drop shadows
        pc.rect([0.02, 0.02, 0.05, 0.15], self.base.x + 1.0, dy + 1.0, pw, dh);
        pc.rect([0.02, 0.02, 0.05, 0.08], self.base.x + 3.0, dy + 3.0, pw, dh);
        pc.rect([0.02, 0.02, 0.05, 0.04], self.base.x + 5.0, dy + 5.0, pw, dh);

        let theme = colors::active_theme();

        // 2. High-contrast premium outer border
        pc.rect(theme.surface_border, self.base.x, dy, pw, dh);
        
        // 3. Frosted glass background
        pc.rect(theme.surface_bg, self.base.x + 1.0, dy + 1.0, pw - 2.0, dh - 2.0); // bg
        
        if let Some(h_idx) = self.hovered_item {
            let iy = dy + h_idx as f32 * 24.0;
            // 4. Vibrantly colored translucent selection highlight
            pc.rect(theme.primary_accent, self.base.x + 2.0, iy + 2.0, pw - 4.0, 20.0);
        }
        
        for (idx, opt) in self.options.iter().enumerate() {
            let iy = crate::layout::align_text_y(dy + idx as f32 * 24.0, 24.0, 12.0, 0.0);
            
            if opt == "-" {
                pc.rect(theme.surface_border, self.base.x + 8.0, dy + idx as f32 * 24.0 + 11.5, pw - 16.0, 1.0);
                continue;
            }

            let text_color = if self.hovered_item == Some(idx) {
                [0xff, 0xff, 0xff]
            } else if self.selected == idx {
                [0x3a, 0x9a, 0xff]
            } else {
                [0xcc, 0xcc, 0xd4]
            };
            
            let color_f32 = [
                text_color[0] as f32 / 255.0,
                text_color[1] as f32 / 255.0,
                text_color[2] as f32 / 255.0,
                1.0,
            ];
            
            let bounds = Some([self.base.x, dy, self.base.x + pw, dy + dh]);
            if let Some(ref font) = self.widget_font() {
                pc.text_with_font_and_bounds(
                    opt,
                    self.base.x + 8.0,
                    iy,
                    12.0,
                    color_f32,
                    font,
                    bounds,
                );
            } else {
                pc.text_with_bounds(
                    opt,
                    self.base.x + 8.0,
                    iy,
                    12.0,
                    color_f32,
                    bounds,
                );
            }
        }
    }
}

impl Default for Dropdown {
    fn default() -> Self {
        Self::new(Vec::new(), 0)
    }
}

impl Element for Dropdown {
    crate::impl_widget_base!(Dropdown);

    fn get_value_string(&self) -> Option<String> {
        self.options.get(self.selected).cloned()
    }

    fn set_value_string(&mut self, val: &str) -> bool {
        let val_trimmed = val.trim();
        for (idx, opt) in self.options.iter().enumerate() {
            if opt.eq_ignore_ascii_case(val_trimmed) {
                if self.selected != idx {
                    self.selected = idx;
                    self.just_changed = true;
                    return true;
                }
                return false;
            }
        }
        if let Ok(idx) = val_trimmed.parse::<usize>() {
            if idx < self.options.len() {
                if self.selected != idx {
                    self.selected = idx;
                    self.just_changed = true;
                    return true;
                }
                return false;
            }
        }
        false
    }

    fn take_change(&mut self) -> bool {
        self.take_change()
    }

    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn preferred_height(&self) -> Option<f32> {
        Some(crate::layout::dropdown_height())
    }

    fn rounded_corners(&self) -> (bool, bool, bool, bool) {
        let r = crate::layout::dropdown_corner_radius();
        if r > 0.0 {
            (true, true, true, true)
        } else {
            (false, false, false, false)
        }
    }

    fn corner_radius(&self) -> f32 {
        crate::layout::dropdown_corner_radius()
    }

    fn all_rounded_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, f32, [f32; 4], (bool, bool, bool, bool))> {
        let mut quads = Vec::new();
        let (r1, r2, r3, r4) = self.rounded_corners();
        if r1 || r2 || r3 || r4 {
            let top = self.base.label_offset();
            let visual_h = self.base.h - top;
            let radius = self.corner_radius();
            
            let bg_color = colors::dropdown_background_color();
            let border_color = if self.open {
                [0.30, 0.50, 0.32, 1.0]
            } else if self.base.hovered {
                [0.25, 0.25, 0.35, 1.0]
            } else {
                [0.18, 0.18, 0.24, 1.0]
            };

            // Draw border
            quads.push((self.base.x, self.base.y + top, self.base.w, visual_h, radius, border_color, (r1, r2, r3, r4)));
            // Draw background (slightly inset to show border)
            let inner_radius = (radius - 1.0).max(0.0);
            quads.push((self.base.x + 1.0, self.base.y + top + 1.0, self.base.w - 2.0, visual_h - 2.0, inner_radius, bg_color, (r1, r2, r3, r4)));
        }
        for &child_ptr in &self.children(ctx) {
            let widget = unsafe { &*child_ptr };
            quads.extend(widget.all_rounded_quads(ctx));
        }
        quads
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::dropdown_font())
    }

    fn hit_test(&self, px: f32, py: f32, ctx: &UiContext) -> bool {
        if ctx.is_coordinate_covered(self as *const Self as *const () as usize, px, py) {
            return false;
        }
        let (x, y, w, h) = self.rect();
        if w <= 0.0 || h <= 0.0 {
            return false;
        }
        let hx = if self.base.row_w > 0.0 { self.base.row_x } else { x };
        let hw = if self.base.row_w > 0.0 { self.base.row_w } else { w };
        if self.open {
            let (dy, dh) = self.get_dy_dh();
            let pw = self.popover_width();
            let hit_trigger = px >= hx && px <= hx + hw && py >= y && py <= y + h;
            let hit_popover = px >= x && px <= x + pw && py >= dy && py <= dy + dh;
            hit_trigger || hit_popover
        } else {
            px >= hx && px <= hx + hw && py >= y && py <= y + h
        }
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        let was_hovered = self.base.hovered;
        let was_hovered_item = self.hovered_item;
        
        self.base.hovered = self.hit_test(px, py, ctx);
        self.hovered_item = None;

        if self.open {
            let (x, _, _, _) = self.rect();
            let (dy, dh) = self.get_dy_dh();
            let pw = self.popover_width();
            if px >= x && px <= x + pw && py >= dy && py <= dy + dh {
                let idx = ((py - dy) / 24.0) as usize;
                if idx < self.options.len() {
                    if self.options[idx] != "-" {
                        self.hovered_item = Some(idx);
                    }
                }
            }
        }

        self.base.hovered != was_hovered || self.hovered_item != was_hovered_item
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if button == MouseButton::Right && state == ElementState::Pressed {
            if self.hit_test(px, py, ctx) {
                ctx.handle_right_click(self.as_ptr_mut(), px, py);
                return true;
            }
        }
        if button != MouseButton::Left || state != ElementState::Pressed { return false; }

        let (x, y, w, h) = self.rect();
        let (dy, dh) = self.get_dy_dh();
        let pw = self.popover_width();

        let inside_trigger = px >= x && px <= x + w && py >= y && py <= y + h;
        let inside_popover = self.open && px >= x && px <= x + pw && py >= dy && py <= dy + dh;

        if inside_popover {
            let idx = ((py - dy) / 24.0) as usize;
            if idx < self.options.len() {
                if self.options[idx] == "-" {
                    return true;
                }
                if self.selected != idx || self.custom_display_text.is_some() {
                    self.selected = idx;
                    self.just_changed = true;
                }
            }
            self.open = false;
            return true;
        }

        if inside_trigger {
            self.open = !self.open;
            if self.open {
                self.focus();
            } else {
                self.unfocus();
            }
            return true;
        }

        if self.open {
            self.open = false;
            return true;
        }

        false
    }

    fn focus(&mut self) {
        focus::set_focused(self);
    }

    fn unfocus(&mut self) {
        self.open = false;
    }

    fn keyboard_input(&mut self, event: &KeyEvent, _ctx: &mut UiContext) -> bool {
        if event.state != ElementState::Pressed { return false; }
        if !self.open {
            if let Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) = event.logical_key {
                self.open = true;
                let mut start_idx = self.selected;
                if start_idx < self.options.len() && self.options[start_idx] == "-" {
                    for i in 0..self.options.len() {
                        if self.options[i] != "-" {
                            start_idx = i;
                            break;
                        }
                    }
                }
                self.hovered_item = Some(start_idx);
                return true;
            }
            return false;
        }
        
        match event.logical_key {
            Key::Named(NamedKey::ArrowDown) => {
                let current = self.hovered_item.unwrap_or(self.selected);
                let mut next = (current + 1) % self.options.len();
                for _ in 0..self.options.len() {
                    if self.options[next] != "-" {
                        self.hovered_item = Some(next);
                        break;
                    }
                    next = (next + 1) % self.options.len();
                }
                true
            }
            Key::Named(NamedKey::ArrowUp) => {
                let current = self.hovered_item.unwrap_or(self.selected);
                let mut prev = if current == 0 { self.options.len() - 1 } else { current - 1 };
                for _ in 0..self.options.len() {
                    if self.options[prev] != "-" {
                        self.hovered_item = Some(prev);
                        break;
                    }
                    prev = if prev == 0 { self.options.len() - 1 } else { prev - 1 };
                }
                true
            }
            Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                if let Some(idx) = self.hovered_item {
                    if idx < self.options.len() && self.options[idx] != "-" {
                        if self.selected != idx || self.custom_display_text.is_some() {
                            self.selected = idx;
                            self.just_changed = true;
                        }
                        self.open = false;
                    }
                }
                true
            }
            Key::Named(NamedKey::Escape) => {
                self.open = false;
                true
            }
            _ => false
        }
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let (r1, r2, r3, r4) = self.rounded_corners();
        if r1 || r2 || r3 || r4 {
            return Vec::new();
        }

        let mut quads = Vec::new();
        let top = self.base.label_offset();
        let visual_h = self.base.h - top;

        let bg_color = colors::dropdown_background_color();
        let border_color = if self.open {
            [0.30, 0.50, 0.32, 1.0]
        } else if self.base.hovered {
            [0.25, 0.25, 0.35, 1.0]
        } else {
            [0.18, 0.18, 0.24, 1.0]
        };

        quads.push((self.base.x, self.base.y + top, self.base.w, visual_h, border_color));
        quads.push((self.base.x + 1.0, self.base.y + top + 1.0, self.base.w - 2.0, visual_h - 2.0, bg_color));

        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        let top = self.base.label_offset();
        let _visual_h = self.base.h - top;
        
        if let Some(lbl) = self.control_label() {
            labels.push(lbl);
        }

        let selected_text = if let Some(ref custom_text) = self.custom_display_text {
            custom_text.clone()
        } else {
            self.options.get(self.selected).cloned().unwrap_or_default()
        };
        labels.push(TextLabel {
            text: selected_text,
            x: self.base.x + 8.0,
            y: crate::layout::align_text_y(self.base.y, self.base.h, 12.0, top),
            font_size: 12.0,
            color: [0xdd, 0xdd, 0xe2],
        });

        labels.push(TextLabel {
            text: "▼".to_string(),
            x: self.base.x + self.base.w - 18.0,
            y: crate::layout::align_text_y(self.base.y, self.base.h, 10.0, top),
            font_size: 10.0,
            color: [0x83, 0x83, 0x8a],
        });

        labels
    }

    fn value(&self) -> i32 { self.selected as i32 }
    fn take_click(&mut self) -> bool { self.take_change() }
    fn popover_rect(&self) -> Option<(f32, f32, f32, f32)> {
        if self.open {
            let (dy, dh) = self.get_dy_dh();
            Some((self.base.x, dy, self.popover_width(), dh))
        } else {
            None
        }
    }
    fn render_popover(&self, pc: &mut dyn crate::layout::RenderTarget) {
        Dropdown::render_popover(self, pc);
    }

    fn z_index(&self) -> i32 {
        if self.open { 100 } else { 0 }
    }

    fn layout_ignore(&self) -> bool {
        true
    }
}

impl Drop for Dropdown {
    fn drop(&mut self) {
        clear_widget_references(self);
    }
}


unsafe impl Send for Dropdown {}
unsafe impl Sync for Dropdown {}

impl Control for Dropdown {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dropdown_widget_interaction() {
        let mut dummy = crate::context::UiContext::new();
        let options = vec!["Option A".to_string(), "Option B".to_string(), "Option C".to_string()];
        let mut dd = Dropdown::new(options, 0);
        dd.set_rect(10.0, 10.0, 100.0, 24.0);

        // 1. Initial State
        assert!(!dd.open);
        assert_eq!(dd.selected, 0);

        // 2. Click trigger area opens dropdown
        let input_changed = dd.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 20.0, &mut dummy);
        assert!(input_changed);
        assert!(dd.open);

        // 3. Hovering options inside popover
        // Popover starts at y = 10 + 24 = 34. Options are of height 24 each.
        // Hover option B at y = 34 + 24 + 12 = 70.0
        let move_changed = dd.on_cursor_moved(50.0, 70.0, &mut dummy);
        assert!(move_changed);
        assert_eq!(dd.hovered_item, Some(1));

        // 4. Click option B selects it and closes dropdown
        let select_changed = dd.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 70.0, &mut dummy);
        assert!(select_changed);
        assert!(!dd.open);
        assert_eq!(dd.selected, 1);
        assert!(dd.take_change());
    }

    #[test]
    fn test_dropdown_context_menu_with_config() {
        let mut dummy = crate::context::UiContext::new();
        let options = vec!["Option A".to_string(), "Option B".to_string()];
        let mut dd = Dropdown::new(options, 0).with_config("path/to/config.json", "some_key");
        dd.set_rect(10.0, 10.0, 100.0, 24.0);

        assert!(!crate::widget::context_menu::is_visible());

        // Right click dropdown
        let handled = dd.mouse_input(MouseButton::Right, ElementState::Pressed, 50.0, 20.0, &mut dummy);
        assert!(handled);

        assert!(crate::widget::context_menu::is_visible());
        let menu_options = crate::widget::context_menu::options();
        assert!(menu_options.len() >= 3);
        assert_eq!(menu_options[1], "File: path/to/config.json");
        assert_eq!(menu_options[2], "Key: some_key");

        crate::widget::context_menu::hide();
        assert!(!crate::widget::context_menu::is_visible());
    }

    #[test]
    fn test_dropdown_separators() {
        let mut dummy = crate::context::UiContext::new();
        let options = vec![
            "Option A".to_string(),
            "-".to_string(),
            "Option B".to_string(),
        ];
        let mut dd = Dropdown::new(options, 0);
        dd.set_rect(10.0, 10.0, 100.0, 24.0);

        // Open dropdown
        dd.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 20.0, &mut dummy);
        assert!(dd.open);

        // Hover over separator at index 1 at y = 34 + 24 + 12 = 70.0
        dd.on_cursor_moved(50.0, 70.0, &mut dummy);
        assert_eq!(dd.hovered_item, None); // Separator should not be hovered

        // Click separator at index 1
        let clicked = dd.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 70.0, &mut dummy);
        assert!(clicked);
        assert!(dd.open); // Dropdown should remain open
        assert_eq!(dd.selected, 0); // Selection should not change

        // Hover over Option B at index 2 at y = 34 + 48 + 12 = 94.0
        dd.on_cursor_moved(50.0, 94.0, &mut dummy);
        assert_eq!(dd.hovered_item, Some(2));

        // Keyboard arrow up from index 2 should skip separator (index 1) and go to index 0
        let key_up = crate::widget::KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::ArrowUp),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
        };
        dd.keyboard_input(&key_up, &mut dummy);
        assert_eq!(dd.hovered_item, Some(0));
    }
}

