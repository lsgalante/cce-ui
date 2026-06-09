use crate::colors;
use crate::widget::*;
use crate::widget::display::{make_widget_text_buffer, TextLabel};

pub struct MenuBar {
    pub base: Plate,
    pub title: String,
    pub menus: ButtonStrip,
    pub menu_items: Vec<String>,
    pub vertical_items: Vec<String>,
    pub menu_dropdowns: Vec<Vec<String>>,
    pub menu_dropdown_checked: Vec<Vec<Option<bool>>>,
    pub vertical: bool,
    pub focused: bool,
    pub z_level: i32,
    pub center_items: bool,
    pub title_pos: Option<(f32, f32)>,
    pub title_buf: Option<glyphon::Buffer>,
    pub curved_title_char_bufs: Vec<glyphon::Buffer>,
    pub font_family: String,
    pub label: Option<String>,
    pub context_options: Vec<String>,
    pub context_selected: usize,
    pub context_dropdown_open: bool,
    pub context_just_changed: bool,
    pub context_hovered_item: Option<usize>,
    pub context_title_hovered: bool,
    pub context_item_bufs: Vec<glyphon::Buffer>,
}

impl MenuBar {
    pub fn set_curved_circle(&mut self, circle: Option<(f32, f32, f32)>) {
        self.base.curved_circle = circle;
    }

    pub fn set_network_opacity(&mut self, opacity: f32) {
        self.base.set_network_opacity(opacity);
    }

    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        let mut base_plate = Plate::new(x, y, w, h)
            .with_color(colors::PANEL_MENU_BG)
            .with_draggable(false);
        base_plate.blur = true;

        Self {
            base: base_plate,
            title: String::new(),
            menus: ButtonStrip::new(x, y, w, h),
            menu_items: Vec::new(),
            vertical_items: Vec::new(),
            menu_dropdowns: Vec::new(),
            menu_dropdown_checked: Vec::new(),
            vertical: false,
            focused: false,
            z_level: 100,
            center_items: false,
            title_pos: None,
            title_buf: None,
            curved_title_char_bufs: Vec::new(),
            font_family: crate::layout::menubar_font(),
            label: None,
            context_options: Vec::new(),
            context_selected: 0,
            context_dropdown_open: false,
            context_just_changed: false,
            context_hovered_item: None,
            context_title_hovered: false,
            context_item_bufs: Vec::new(),
        }
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.label = Some(label.to_string());
        self.base.base.label = Some(label.to_string());
        self
    }

    pub fn set_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
        self.base.base.label = Some(label.to_string());
    }

    pub fn with_context_options(mut self, options: Vec<String>, selected: usize) -> Self {
        self.context_options = options;
        self.context_selected = selected;
        self
    }

    pub fn set_context_selected(&mut self, selected: usize) {
        if self.context_selected != selected {
            self.context_selected = selected;
            self.context_item_bufs.clear();
        }
    }

    pub fn take_context_change(&mut self) -> Option<usize> {
        if self.context_just_changed {
            self.context_just_changed = false;
            Some(self.context_selected)
        } else {
            None
        }
    }

    pub fn title_rect(&self) -> (f32, f32, f32, f32) {
        if self.title.is_empty() {
            return (0.0, 0.0, 0.0, 0.0);
        }
        let font_setting = crate::layout::menubar_font();
        let (_, font_size_opt) = crate::layout::parse_font_string(&font_setting);
        let font_size = font_size_opt.unwrap_or(12.0);
        let char_w = 7.5 * (font_size / 12.0);
        let padding_x = crate::layout::paginator_tab_padding_x();

        let mut display_title = self.title.clone();
        if !self.context_options.is_empty() {
            display_title.push_str(" ▼");
        }

        if let Some((_ccx, _ccy, _ccr)) = self.base.curved_circle {
            if let Some((tx, ty)) = self.title_pos {
                let title_w = display_title.len() as f32 * char_w + 24.0;
                (tx, ty, title_w, self.base.base.h)
            } else {
                (self.base.base.x, self.base.base.y, display_title.len() as f32 * char_w + 24.0, self.base.base.h)
            }
        } else if self.vertical {
            let mut cy = 8.0;
            if let Some(ref label) = self.label {
                let line_height = font_size * 1.2;
                let label_h = label.chars().count() as f32 * line_height;
                cy += label_h + 8.0;
            }
            let line_height = font_size * 1.2;
            let mut display_title_vertical = self.title.clone();
            if !self.context_options.is_empty() {
                display_title_vertical.push_str("▼");
            }
            let title_h = display_title_vertical.chars().count() as f32 * line_height;
            (self.base.base.x, self.base.base.y + cy, self.base.base.w, title_h)
        } else {
            let mut start_x = 8.0;
            if self.center_items {
                let mut total_width = 8.0;
                total_width += display_title.len() as f32 * char_w + 24.0;
                for btn_label in &self.menus.buttons {
                    total_width += btn_label.len() as f32 * char_w + 2.0 * padding_x;
                }
                if self.base.base.w > total_width {
                    start_x = (self.base.base.w - total_width) / 2.0;
                }
            }
            let title_w = display_title.len() as f32 * char_w + 24.0;
            (self.base.base.x + start_x, self.base.base.y, title_w, self.base.base.h)
        }
    }

    pub fn context_popover_rect(&self) -> Option<(f32, f32, f32, f32)> {
        if self.context_options.is_empty() || !self.context_dropdown_open {
            return None;
        }
        let font_setting = crate::layout::menubar_font();
        let (_, font_size_opt) = crate::layout::parse_font_string(&font_setting);
        let font_size = font_size_opt.unwrap_or(12.0);
        let char_w = 7.5 * (font_size / 12.0);

        let max_len = self.context_options.iter().map(|s| s.len()).max().unwrap_or(0);
        let dw = (max_len as f32 * char_w + 40.0).max(140.0);
        let dh = self.context_options.len() as f32 * DROPDOWN_ITEM_H;

        let tr = self.title_rect();
        let dx = if self.vertical {
            tr.0 + tr.2
        } else {
            tr.0
        };
        let dy = if self.vertical {
            tr.1
        } else {
            tr.1 + tr.3
        };
        Some((dx, dy, dw, dh))
    }

    pub fn with_center_items(mut self, center: bool) -> Self {
        self.center_items = center;
        self
    }

    pub fn with_title(mut self, title: &str) -> Self {
        self.title = title.to_string();
        self
    }

    pub fn with_item(mut self, label: &str, items: &[&str]) -> Self {
        self.menu_items.push(label.to_string());
        self.vertical_items.push(label.to_string());
        self.menu_dropdowns.push(items.iter().map(|s| s.to_string()).collect());
        self.menu_dropdown_checked.push(vec![None; items.len()]);
        self.menus.add_button(label);
        self
    }

    pub fn with_item_vh(mut self, horizontal_label: &str, vertical_label: &str, items: &[&str]) -> Self {
        self.menu_items.push(horizontal_label.to_string());
        self.vertical_items.push(vertical_label.to_string());
        self.menu_dropdowns.push(items.iter().map(|s| s.to_string()).collect());
        self.menu_dropdown_checked.push(vec![None; items.len()]);
        let label = if self.vertical { vertical_label } else { horizontal_label };
        self.menus.add_button(label);
        self
    }

    fn update_menu_labels(&mut self) {
        let src = if self.vertical {
            &self.vertical_items
        } else {
            &self.menu_items
        };
        self.menus.buttons = src.clone();
    }

    pub fn with_vertical(mut self, vertical: bool) -> Self {
        self.vertical = vertical;
        self.menus.vertical = vertical;
        self.update_menu_labels();
        self
    }

    pub fn with_z_index(mut self, z: i32) -> Self {
        self.z_level = z;
        self
    }
}

impl Element for MenuBar {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn as_ptr(&self) -> *mut (dyn Element + 'static) {
        self as *const Self as *mut Self as *mut (dyn Element + 'static)
    }

    fn base(&self) -> Option<&Widget> { Some(&self.base.base) }
    fn base_mut(&mut self) -> Option<&mut Widget> { Some(&mut self.base.base) }

    fn label(&self) -> Option<String> {
        self.label.clone()
    }

    fn set_text(&mut self, text: &str) {
        self.label = Some(text.to_string());
        self.base.base.label = Some(text.to_string());
    }

    fn rect(&self) -> (f32, f32, f32, f32) {
        if !self.base.visible {
            return (0.0, 0.0, 0.0, 0.0);
        }
        if self.vertical {
            let mut h = 16.0;
            if let Some(ref label) = self.label {
                let font_size = 12.0;
                let line_height = font_size * 1.2;
                let label_h = label.chars().count() as f32 * line_height;
                h += label_h + 8.0;
            }
            if !self.title.is_empty() {
                let font_size = 12.0;
                let line_height = font_size * 1.2;
                let mut display_title = self.title.clone();
                if !self.context_options.is_empty() {
                    display_title.push_str("▼");
                }
                let title_h = display_title.chars().count() as f32 * line_height;
                h += title_h + 8.0;
            }
            let (_, _, _, menus_h) = self.menus.rect();
            (self.base.base.x, self.base.base.y, self.base.base.w, h + menus_h)
        } else {
            (self.base.base.x, self.base.base.y, self.base.base.w, self.base.base.h)
        }
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.base.set_rect(x, y, w, h);
        let parent_ptr = self as *mut MenuBar as *mut (dyn Element + 'static);

        let font_setting = crate::layout::menubar_font();
        let (_, font_size_opt) = crate::layout::parse_font_string(&font_setting);
        let font_size = font_size_opt.unwrap_or(12.0);
        let char_w = 7.5 * (font_size / 12.0);

        if self.vertical {
            let mut cy = 8.0;
            if let Some(ref label) = self.label {
                let font_size = 12.0;
                let line_height = font_size * 1.2;
                let label_h = label.chars().count() as f32 * line_height;
                cy += label_h + 8.0;
            }
            if !self.title.is_empty() {
                let font_size = 12.0;
                let line_height = font_size * 1.2;
                let mut display_title = self.title.clone();
                if !self.context_options.is_empty() {
                    display_title.push_str("▼");
                }
                let title_h = display_title.chars().count() as f32 * line_height;
                cy += title_h + 8.0;
            }
            self.menus.vertical = true;
            self.menus.set_rect(x, y + cy, w, (h - cy).max(0.0));
            let mut dummy = crate::context::UiContext::new();
            self.menus.set_parent(Some(parent_ptr), &mut dummy);
        } else {
            let padding_x = crate::layout::paginator_tab_padding_x();
            let mut cx = 8.0;
            if self.center_items {
                let mut total_width = 8.0;
                if !self.title.is_empty() {
                    let mut display_title = self.title.clone();
                    if !self.context_options.is_empty() {
                        display_title.push_str(" ▼");
                    }
                    total_width += display_title.len() as f32 * char_w + 24.0;
                }
                let mut btn_strip_w = 0.0;
                for btn_label in &self.menus.buttons {
                    btn_strip_w += btn_label.len() as f32 * char_w + 2.0 * padding_x;
                }
                total_width += btn_strip_w;
                if self.base.base.w > total_width {
                    cx = (self.base.base.w - total_width) / 2.0;
                }
            }
            if !self.title.is_empty() {
                let mut display_title = self.title.clone();
                if !self.context_options.is_empty() {
                    display_title.push_str(" ▼");
                }
                cx += display_title.len() as f32 * char_w + 24.0;
            }
            let mut btn_strip_w = 0.0;
            for btn_label in &self.menus.buttons {
                btn_strip_w += btn_label.len() as f32 * char_w + 2.0 * padding_x;
            }
            self.menus.vertical = false;
            self.menus.set_rect(x + cx, y, btn_strip_w, h);
            let mut dummy = crate::context::UiContext::new();
            self.menus.set_parent(Some(parent_ptr), &mut dummy);
        }
    }

    fn color(&self) -> [f32; 4] {
        if !self.base.visible {
            [0.0, 0.0, 0.0, 0.0]
        } else if self.focused {
            let mut c = colors::PANEL_MENU_FOCUSED;
            c[3] *= self.base.network_opacity;
            if self.base.blur {
                c[3] = -c[3].abs();
            }
            c
        } else {
            self.base.color()
        }
    }

    fn set_hovered(&mut self, v: bool) {
        self.base.base.hovered = v;
    }

    fn hovered(&self) -> bool {
        self.base.base.hovered
    }

    fn hit_test(&self, px: f32, py: f32, ctx: &UiContext) -> bool {
        if !self.base.visible {
            return false;
        }
        if ctx.is_coordinate_covered(self as *const Self as *const () as usize, px, py) {
            return false;
        }
        if let Some((dx, dy, dw, dh)) = self.context_popover_rect() {
            if px >= dx && px < dx + dw && py >= dy && py < dy + dh {
                return true;
            }
        }
        let (rx, ry, rw, rh) = self.rect();
        if px >= rx && px <= rx + rw && py >= ry && py <= ry + rh {
            return true;
        }
        if self.menus.hit_test(px, py, ctx) {
            return true;
        }
        false
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if !self.base.visible {
            return false;
        }
        let (rx, ry, rw, rh) = self.rect();
        self.set_rect(rx, ry, rw, rh);

        let mut changed = false;

        let old_title_hovered = self.context_title_hovered;
        self.context_title_hovered = false;
        if !self.context_options.is_empty() {
            let tr = self.title_rect();
            if px >= tr.0 && px <= tr.0 + tr.2 && py >= tr.1 && py <= tr.1 + tr.3 {
                self.context_title_hovered = true;
            }
        }
        if old_title_hovered != self.context_title_hovered {
            changed = true;
        }

        let old_hovered_item = self.context_hovered_item;
        self.context_hovered_item = None;
        if let Some((dx, dy, dw, dh)) = self.context_popover_rect() {
            if px >= dx && px < dx + dw && py >= dy && py < dy + dh {
                let di = ((py - dy) / DROPDOWN_ITEM_H) as usize;
                if di < self.context_options.len() {
                    self.context_hovered_item = Some(di);
                }
            }
        }
        if old_hovered_item != self.context_hovered_item {
            changed = true;
        }

        if self.menus.cursor_moved(px, py, ctx) {
            changed = true;
        }
        changed
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if !self.base.visible {
            return false;
        }
        if button != MouseButton::Left {
            return false;
        }

        let (rx, ry, rw, rh) = self.rect();
        self.set_rect(rx, ry, rw, rh);

        let mut changed = false;

        if let Some((dx, dy, dw, dh)) = self.context_popover_rect() {
            if px >= dx && px < dx + dw && py >= dy && py < dy + dh {
                if state == ElementState::Pressed {
                    let di = ((py - dy) / DROPDOWN_ITEM_H) as usize;
                    if di < self.context_options.len() {
                        self.context_selected = di;
                        self.context_just_changed = true;
                        self.context_dropdown_open = false;
                        self.unfocus();
                        return true;
                    }
                }
            }
        }

        if !self.context_options.is_empty() {
            let tr = self.title_rect();
            if px >= tr.0 && px <= tr.0 + tr.2 && py >= tr.1 && py <= tr.1 + tr.3 {
                if state == ElementState::Pressed {
                    if self.context_dropdown_open {
                        self.context_dropdown_open = false;
                        self.unfocus();
                    } else {
                        self.menus.unfocus();
                        self.context_dropdown_open = true;
                        self.focus();
                    }
                }
                return true;
            }
        }

        if self.context_dropdown_open && state == ElementState::Pressed {
            self.context_dropdown_open = false;
            self.unfocus();
            changed = true;
        }

        if self.menus.mouse_input(button, state, px, py, ctx) {
            changed = true;
        }
        changed
    }

    fn focus(&mut self) {
        if self.context_dropdown_open {
            self.focused = true;
            focus::set_focused(self);
        } else {
            self.focused = false;
            focus::clear_if_matches(self);
        }
    }

    fn unfocus(&mut self) {
        self.focused = false;
        self.context_dropdown_open = false;
        self.context_hovered_item = None;
        focus::clear_if_matches(self);
        self.menus.unfocus();
    }

    fn focused(&self, ctx: &UiContext) -> bool {
        self.focused || self.context_dropdown_open
    }

    fn popover_rect(&self) -> Option<(f32, f32, f32, f32)> {
        self.context_popover_rect()
    }

    fn keyboard_input(&mut self, event: &KeyEvent, ctx: &mut UiContext) -> bool {
        if event.state != ElementState::Pressed { return false; }
        if self.context_dropdown_open {
            match event.logical_key {
                Key::Named(NamedKey::ArrowDown) => {
                    let current = self.context_hovered_item.unwrap_or(self.context_selected);
                    if current + 1 < self.context_options.len() {
                        self.context_hovered_item = Some(current + 1);
                    } else {
                        self.context_hovered_item = Some(0);
                    }
                    return true;
                }
                Key::Named(NamedKey::ArrowUp) => {
                    let current = self.context_hovered_item.unwrap_or(self.context_selected);
                    if current > 0 {
                        self.context_hovered_item = Some(current - 1);
                    } else {
                        self.context_hovered_item = Some(self.context_options.len() - 1);
                    }
                    return true;
                }
                Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                    if let Some(idx) = self.context_hovered_item {
                        self.context_selected = idx;
                        self.context_just_changed = true;
                    }
                    self.context_dropdown_open = false;
                    self.unfocus();
                    return true;
                }
                Key::Named(NamedKey::Escape) => {
                    self.context_dropdown_open = false;
                    self.unfocus();
                    return true;
                }
                _ => {}
            }
        }
        self.menus.keyboard_input(event, ctx)
    }

    fn set_selected(&mut self, selected: bool) {
        self.focused = selected;
        if !selected {
            self.menus.set_selected(None);
        }
    }

    fn set_modifiers(&mut self, ctrl: bool, shift: bool, alt: bool) {
        self.menus.set_modifiers(ctrl, shift, alt);
    }

    fn menu_names(&self) -> Vec<String> {
        self.menus.buttons.clone()
    }

    fn menu_click(&mut self) -> Option<(usize, usize)> {
        if let Some(idx) = self.menus.take_click() {
            return Some((idx, 0));
        }
        None
    }

    fn set_item_checked(&mut self, menu_idx: usize, item_idx: usize, checked: bool) {
        if let Some(menu) = self.menu_dropdown_checked.get_mut(menu_idx) {
            if item_idx < menu.len() {
                menu[item_idx] = Some(checked);
            }
        }
    }

    fn set_menu_items(&mut self, menu_idx: usize, items: &[String]) {
        if menu_idx < self.menu_dropdowns.len() {
            self.menu_dropdowns[menu_idx] = items.to_vec();
            self.menu_dropdown_checked[menu_idx] = vec![Some(false); items.len()];
        }
    }

    fn is_menu_bar(&self) -> bool {
        self.base.visible
    }

    fn get_menu_items_at(&self, _px: f32, _py: f32) -> Option<(usize, String, Vec<String>, f32, f32, f32, f32)> {
        None
    }

    fn trigger_menu_click(&mut self, _menu_idx: usize, _item_idx: usize) {}

    fn is_menu_open(&self) -> bool {
        self.context_dropdown_open
    }

    fn all_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.base.visible {
            return Vec::new();
        }
        let mut quads = Vec::new();
        quads.extend(self.base.all_quads(ctx));
        quads.extend(self.menus.all_quads(ctx));
        quads
    }

    fn extra_arcs(&self) -> Vec<(f32, f32, f32, f32, f32, f32, [f32; 4])> {
        if !self.base.visible {
            return Vec::new();
        }
        self.menus.extra_arcs()
    }

    fn prepare_text(&mut self, fs: &mut glyphon::FontSystem) {
        if !self.base.visible {
            return;
        }
        let current_font = crate::layout::menubar_font();
        if self.font_family != current_font {
            self.font_family = current_font;
            self.title_buf = None;
            self.curved_title_char_bufs.clear();
        }
        let (font_fam, font_size_opt) = crate::layout::parse_font_string(&self.font_family);
        let font_size = font_size_opt.unwrap_or(12.0);

        if !self.title.is_empty() {
            let mut display_title = self.title.clone();
            if !self.context_options.is_empty() {
                display_title.push_str(" ▼");
            }
            if let Some((_ccx, _ccy, _ccr)) = self.base.curved_circle {
                if self.curved_title_char_bufs.len() != display_title.chars().count() {
                    let font_fam_clone = font_fam.clone();
                    self.curved_title_char_bufs = display_title.chars()
                        .map(|c| make_widget_text_buffer(fs, &c.to_string(), font_size, &font_fam_clone))
                        .collect();
                }
                self.title_buf = None;
            } else if self.vertical {
                self.title_buf = None;
                self.curved_title_char_bufs.clear();
            } else {
                if self.title_buf.is_none() {
                    self.title_buf = Some(make_widget_text_buffer(fs, &display_title, font_size, &font_fam));
                }
                self.curved_title_char_bufs.clear();
            }
        } else {
            self.title_buf = None;
            self.curved_title_char_bufs.clear();
        }

        if self.context_dropdown_open {
            if self.context_item_bufs.len() != self.context_options.len() {
                let font_fam_clone = font_fam.clone();
                self.context_item_bufs = self.context_options.iter().enumerate().map(|(i, option)| {
                    let is_selected = self.context_selected == i;
                    let prefix = if is_selected { "✓ " } else { "  " };
                    let text = format!("{}{}", prefix, option);
                    make_widget_text_buffer(fs, &text, font_size, &font_fam_clone)
                }).collect();
            }
        } else {
            self.context_item_bufs.clear();
        }

        self.menus.prepare_text(fs);
    }

    fn get_text_items(&self) -> Vec<(&glyphon::Buffer, f32, f32, glyphon::Color)> {
        if !self.base.visible {
            return Vec::new();
        }
        let mut items = Vec::new();
        let label_color = crate::colors::paginator_tab_label_color();
        let srgb = crate::colors::to_srgb(label_color);
        let color = glyphon::Color::rgb(
            (srgb[0] * 255.0) as u8,
            (srgb[1] * 255.0) as u8,
            (srgb[2] * 255.0) as u8,
        );

        let font_setting = crate::layout::menubar_font();
        let (_, font_size_opt) = crate::layout::parse_font_string(&font_setting);
        let font_size = font_size_opt.unwrap_or(12.0);
        let char_w = 7.5 * (font_size / 12.0);

        let padding_x = crate::layout::paginator_tab_padding_x();
        if let Some((ccx, ccy, ccr)) = self.base.curved_circle {
            let r_mid = ccr - self.base.base.h / 2.0;
            let mut total_width = 8.0;
            let mut display_title = self.title.clone();
            if !self.context_options.is_empty() {
                display_title.push_str(" ▼");
            }
            if !self.title.is_empty() {
                total_width += display_title.len() as f32 * char_w + 24.0;
            }
            for btn_label in &self.menus.buttons {
                total_width += btn_label.len() as f32 * char_w + 2.0 * padding_x;
            }
            let total_angular_width = total_width / r_mid;
            let start_angle = 1.5 * std::f32::consts::PI - total_angular_width / 2.0;
            let current_angle = start_angle;

            if !self.title.is_empty() {
                let title_w = display_title.len() as f32 * char_w + 24.0;
                let dtheta_title = title_w / r_mid;

                let char_widths: Vec<f32> = display_title.chars().map(|c| {
                    TextLabel::estimate_width(&c.to_string(), font_size)
                }).collect();
                let total_chars_width: f32 = char_widths.iter().sum();
                let mid_angle = (current_angle + current_angle + dtheta_title) / 2.0;
                let angular_width = total_chars_width / r_mid;
                let text_start_angle = mid_angle - angular_width / 2.0;
                let mut cur_char_angle = text_start_angle;

                for (char_idx, c_buf) in self.curved_title_char_bufs.iter().enumerate() {
                    if char_idx < char_widths.len() {
                        let cw = char_widths[char_idx];
                        let dtheta = cw / r_mid;
                        let char_center_angle = cur_char_angle + dtheta / 2.0;

                        let tx = ccx + r_mid * char_center_angle.cos() - cw / 2.0;
                        let ty = ccy + r_mid * char_center_angle.sin() - font_size / 2.0;

                        items.push((c_buf, tx, ty, color));
                        cur_char_angle += dtheta;
                    }
                }
            }
        } else if !self.vertical {
            let mut start_x = 8.0;
            let mut display_title = self.title.clone();
            if !self.context_options.is_empty() {
                display_title.push_str(" ▼");
            }
            if self.center_items {
                let mut total_width = 8.0;
                if !self.title.is_empty() {
                    total_width += display_title.len() as f32 * char_w + 24.0;
                }
                for btn_label in &self.menus.buttons {
                    total_width += btn_label.len() as f32 * char_w + 2.0 * padding_x;
                }
                if self.base.base.w > total_width {
                    start_x = (self.base.base.w - total_width) / 2.0;
                }
            }
            if let Some(ref title_buf) = self.title_buf {
                let text_y = self.base.base.y + (self.base.base.h - font_size) / 2.0;
                items.push((title_buf, self.base.base.x + start_x, text_y, color));
            }
        }

        if self.context_dropdown_open {
            if let Some((dx, dy, _, _)) = self.context_popover_rect() {
                for (i, item_buf) in self.context_item_bufs.iter().enumerate() {
                    items.push((
                        item_buf,
                        dx + 8.0,
                        dy + i as f32 * DROPDOWN_ITEM_H + 5.0,
                        color,
                    ));
                }
            }
        }

        items.extend(self.menus.get_text_items());
        items
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        if !self.base.visible {
            return Vec::new();
        }
        let label_color = crate::colors::paginator_tab_label_color();
        let srgb = crate::colors::to_srgb(label_color);
        let text_color = [
            (srgb[0] * 255.0) as u8,
            (srgb[1] * 255.0) as u8,
            (srgb[2] * 255.0) as u8,
        ];
        let padding_x = crate::layout::paginator_tab_padding_x();
        let mut labels = Vec::new();

        if let Some(ref label) = self.label {
            if self.vertical {
                let font_size = 12.0;
                let line_height = font_size * 1.2;
                let start_y = self.base.base.y + 8.0;
                for (i, c) in label.chars().enumerate() {
                    let char_str = c.to_string();
                    let char_w = TextLabel::estimate_width(&char_str, font_size);
                    let x_pos = self.base.base.x + (self.base.base.w - char_w) / 2.0;
                    let y_pos = start_y + i as f32 * line_height;
                    labels.push(TextLabel {
                        text: char_str,
                        x: x_pos,
                        y: y_pos,
                        font_size,
                        color: [0x83, 0x83, 0x8a],
                    });
                }
            }
        }

        let font_setting = crate::layout::menubar_font();
        let (_, font_size_opt) = crate::layout::parse_font_string(&font_setting);
        let font_size = font_size_opt.unwrap_or(12.0);
        let char_w = 7.5 * (font_size / 12.0);

        let mut display_title = self.title.clone();
        if !self.context_options.is_empty() {
            display_title.push_str(" ▼");
        }

        if let Some((ccx, ccy, ccr)) = self.base.curved_circle {
            let r_mid = ccr - self.base.base.h / 2.0;
            let mut total_width = 8.0;
            if !self.title.is_empty() {
                total_width += display_title.len() as f32 * char_w + 24.0;
            }
            for btn_label in &self.menus.buttons {
                total_width += btn_label.len() as f32 * char_w + 2.0 * padding_x;
            }
            let total_angular_width = total_width / r_mid;
            let start_angle = 1.5 * std::f32::consts::PI - total_angular_width / 2.0;
            let current_angle = start_angle;

            if !self.title.is_empty() {
                let title_w = display_title.len() as f32 * char_w + 24.0;
                let dtheta_title = title_w / r_mid;
                labels.extend(TextLabel::curved_layout(
                    &display_title,
                    ccx, ccy, r_mid,
                    current_angle, current_angle + dtheta_title,
                    font_size,
                    text_color,
                ));
            }
        } else if self.vertical {
            if !self.title.is_empty() {
                let mut start_y = self.base.base.y + 8.0;
                if let Some(ref label) = self.label {
                    let font_size = 12.0;
                    let line_height = font_size * 1.2;
                    let label_h = label.chars().count() as f32 * line_height;
                    start_y += label_h + 8.0;
                }
                let line_height = font_size * 1.2;
                let char_w = TextLabel::estimate_width("o", font_size);
                let x_pos = self.base.base.x + (self.base.base.w - char_w) / 2.0;
                let mut display_title_vertical = self.title.clone();
                if !self.context_options.is_empty() {
                    display_title_vertical.push_str("▼");
                }
                for (i, c) in display_title_vertical.chars().enumerate() {
                    let char_str = c.to_string();
                    let y_pos = start_y + i as f32 * line_height;
                    labels.push(TextLabel {
                        text: char_str,
                        x: x_pos,
                        y: y_pos,
                        font_size,
                        color: text_color,
                    });
                }
            }
        } else {
            let mut start_x = 8.0;
            if self.center_items {
                let mut total_width = 8.0;
                if !self.title.is_empty() {
                    total_width += display_title.len() as f32 * char_w + 24.0;
                }
                for btn_label in &self.menus.buttons {
                    total_width += btn_label.len() as f32 * char_w + 2.0 * padding_x;
                }
                if self.base.base.w > total_width {
                    start_x = (self.base.base.w - total_width) / 2.0;
                }
            }
            if !self.title.is_empty() {
                let text_y = self.base.base.y + (self.base.base.h - font_size) / 2.0;
                labels.push(TextLabel {
                    text: display_title,
                    x: self.base.base.x + start_x,
                    y: text_y,
                    font_size,
                    color: text_color,
                });
            }
        }

        if self.context_dropdown_open {
            if let Some((dx, dy, _, _)) = self.context_popover_rect() {
                for (i, option) in self.context_options.iter().enumerate() {
                    let is_selected = self.context_selected == i;
                    let prefix = if is_selected { "✓ " } else { "  " };
                    labels.push(TextLabel {
                        text: format!("{}{}", prefix, option),
                        x: dx + 8.0,
                        y: dy + i as f32 * DROPDOWN_ITEM_H + 5.0,
                        font_size: 12.0,
                        color: text_color,
                    });
                }
            }
        }

        labels.extend(self.menus.text_labels());
        labels
    }

    fn set_visible(&mut self, visible: bool) {
        self.base.set_visible(visible);
        self.menus.set_visible(visible);
    }

    fn visible(&self) -> bool {
        self.base.visible()
    }

    fn children(&self, _ctx: &UiContext) -> Vec<*mut (dyn Element + 'static)> {
        let ptr: *const dyn Element = &self.menus as &dyn Element;
        vec![ptr as *mut (dyn Element + 'static)]
    }

    fn z_index(&self) -> i32 {
        self.z_level
    }

    fn set_center_items(&mut self, center: bool) {
        self.center_items = center;
    }

    fn menu_items_list(&self) -> Vec<Vec<String>> {
        self.menus.buttons.iter().map(|_| Vec::new()).collect()
    }

    fn menu_checked_list(&self) -> Vec<Vec<Option<bool>>> {
        self.menus.buttons.iter().map(|_| Vec::new()).collect()
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.base.visible {
            return Vec::new();
        }
        let mut quads = Vec::new();

        // 1. Highlight the title on hover or open
        if !self.context_options.is_empty() {
            let tr = self.title_rect();
            if self.context_dropdown_open {
                quads.push((tr.0, tr.1, tr.2, tr.3, colors::highlight_primary_color()));
            } else if self.context_title_hovered {
                quads.push((tr.0, tr.1, tr.2, tr.3, colors::HIGHLIGHT_SECONDARY));
            }
        }

        // 2. Draw the context popover background and hovered item highlight
        if self.context_dropdown_open {
            if let Some((dx, dy, dw, dh)) = self.context_popover_rect() {
                quads.push((dx, dy, dw, dh, colors::PANEL_MENU_BG));
                if let Some(di) = self.context_hovered_item {
                    quads.push((dx, dy + di as f32 * DROPDOWN_ITEM_H, dw, DROPDOWN_ITEM_H, colors::PANEL_MENU_HOVER));
                }
            }
        }

        quads.extend(self.menus.extra_quads());
        quads
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::menubar_font())
    }

    fn take_context_change(&mut self) -> Option<usize> {
        self.take_context_change()
    }

    fn set_context_selected(&mut self, selected: usize) {
        self.set_context_selected(selected);
    }
}impl Drop for MenuBar {
    fn drop(&mut self) {
        focus::clear_if_matches(self);
    }
}

#[derive(Debug, Clone)]
pub struct Menu {
    pub base: Widget,
    pub title: String,
    pub vertical_title: String,
    pub items: Vec<String>,
    pub item_checked: Vec<Option<bool>>,
    pub open: bool,
    pub vertical: bool,
    hovered_item: Option<usize>,
    clicked_item: Option<usize>,
    was_open: Option<usize>,
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub children: Vec<*mut (dyn Element + 'static)>,
    pub curved_arc: Option<(f32, f32, f32, f32, f32, f32)>,
    pub title_buf: Option<glyphon::Buffer>,
    pub item_bufs: Vec<glyphon::Buffer>,
    pub check_buf: Option<glyphon::Buffer>,
    pub curved_char_bufs: Vec<glyphon::Buffer>,
    pub font_family: String,
}

impl Menu {
    pub fn new(title: &str, vertical_title: &str, items: &[String]) -> Self {
        Self {
            base: Widget::new(),
            title: title.to_string(),
            vertical_title: vertical_title.to_string(),
            items: items.to_vec(),
            item_checked: vec![None; items.len()],
            open: false,
            vertical: false,
            hovered_item: None,
            clicked_item: None,
            was_open: None,
            parent: None,
            children: Vec::new(),
            curved_arc: None,
            title_buf: None,
            item_bufs: Vec::new(),
            check_buf: None,
            curved_char_bufs: Vec::new(),
            font_family: String::new(),
        }
    }

    pub fn active_title(&self) -> &str {
        if self.vertical {
            &self.vertical_title
        } else {
            &self.title
        }
    }

    fn dropdown_rect(&self) -> (f32, f32, f32, f32) {
        let dh = self.items.len() as f32 * DROPDOWN_ITEM_H;
        let mut max_len = 0;
        for item in &self.items {
            max_len = max_len.max(item.len());
        }
        let font_setting = crate::layout::menubar_font();
        let (_, font_size_opt) = crate::layout::parse_font_string(&font_setting);
        let font_size = font_size_opt.unwrap_or(12.0);
        let char_w = 7.5 * (font_size / 12.0);

        let dw = (max_len as f32 * char_w + 40.0).max(120.0);
        let dx = if self.vertical {
            self.base.x + self.base.w
        } else {
            self.base.x
        };
        let dy = if self.vertical {
            self.base.y
        } else {
            self.base.y + self.base.h
        };
        (dx, dy, dw, dh)
    }
}
impl Element for Menu {
    crate::impl_widget_base!(Menu);

    fn label(&self) -> Option<String> {
        Some(self.active_title().to_string())
    }

    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn is_active(&self) -> bool {
        self.base.focused || self.open
    }

    fn hit_test(&self, px: f32, py: f32, ctx: &UiContext) -> bool {
        if ctx.is_coordinate_covered(self as *const Self as *const () as usize, px, py) {
            return false;
        }
        if let Some((cx, cy, r, thickness, start_angle, end_angle)) = self.curved_arc {
            let dx = px - cx;
            let dy = py - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist >= r - thickness && dist <= r {
                let angle = dy.atan2(dx);
                let mut norm_angle = angle;
                if norm_angle < 0.0 {
                    norm_angle += 2.0 * std::f32::consts::PI;
                }
                if norm_angle >= start_angle && norm_angle <= end_angle {
                    return true;
                }
            }
            if self.open {
                let (dx, dy, dw, dh) = self.dropdown_rect();
                if dh > 0.0 && px >= dx && px < dx + dw && py >= dy && py < dy + dh {
                    return true;
                }
            }
            return false;
        }
        let (rx, ry, rw, rh) = self.rect();
        if px >= rx && px <= rx + rw && py >= ry && py <= ry + rh {
            return true;
        }
        if self.open {
            let (dx, dy, dw, dh) = self.dropdown_rect();
            if dh > 0.0 && px >= dx && px < dx + dw && py >= dy && py < dy + dh {
                return true;
            }
        }
        false
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        self.was_open = None;
        let was_hovering = self.base.hovered;
        self.base.hovered = self.hit_test(px, py, ctx);
        let old_item = self.hovered_item;
        self.hovered_item = None;

        if self.open {
            let (dx, dy, dw, dh) = self.dropdown_rect();
            if px >= dx && px < dx + dw && py >= dy && py < dy + dh {
                let di = ((py - dy) / DROPDOWN_ITEM_H) as usize;
                if di < self.items.len() {
                    self.hovered_item = Some(di);
                }
            }
        }

        was_hovering != self.base.hovered || old_item != self.hovered_item
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if button != MouseButton::Left || state != ElementState::Pressed {
            return false;
        }
        if !self.hit_test(px, py, ctx) {
            return false;
        }

        // Check dropdown click if open
        if self.open {
            let (dx, dy, dw, dh) = self.dropdown_rect();
            if dh > 0.0 && px >= dx && px < dx + dw && py >= dy && py < dy + dh {
                let di = ((py - dy) / DROPDOWN_ITEM_H) as usize;
                if di < self.items.len() {
                    self.clicked_item = Some(di);
                    self.open = false;
                    return true;
                }
            }
        }

        // Since we passed hit_test and didn't click dropdown, it's a click on the header title
        if self.was_open == Some(0) || self.open {
            self.open = false;
            self.was_open = None;
        } else {
            self.open = true;
            self.was_open = None;
            focus::set_focused(self);
        }
        true
    }

    fn focus(&mut self) {
        self.open = true;
        self.base.focused = true;
        focus::set_focused(self);
    }

    fn unfocus(&mut self) {
        if self.open {
            self.was_open = Some(0);
        }
        self.open = false;
        self.base.focused = false;
        focus::clear_if_matches(self);
        self.hovered_item = None;
    }

    fn set_selected(&mut self, selected: bool) {
        self.base.focused = selected;
        if !selected {
            self.open = false;
            self.was_open = None;
            self.hovered_item = None;
            focus::clear_if_matches(self);
        }
    }

    fn menu_click(&mut self) -> Option<(usize, usize)> {
        self.clicked_item.take().map(|i| (0, i))
    }

    fn set_item_checked(&mut self, _menu_idx: usize, item_idx: usize, checked: bool) {
        if item_idx < self.item_checked.len() {
            self.item_checked[item_idx] = Some(checked);
            self.item_bufs.clear();
        }
    }

    fn is_menu_open(&self) -> bool {
        self.open
    }

    fn highlight_quad(&self, _ctx: &UiContext) -> Option<(f32, f32, f32, f32, [f32; 4])>{
        None
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        if self.curved_arc.is_none() {
            if self.base.focused || self.open {
                quads.push((self.base.x, self.base.y, self.base.w, self.base.h, colors::highlight_primary_color()));
            } else if self.base.hovered {
                quads.push((self.base.x, self.base.y, self.base.w, self.base.h, colors::HIGHLIGHT_SECONDARY));
            }
        }
        if self.open {
            let (dx, dy, dw, dh) = self.dropdown_rect();
            if dh > 0.0 {
                quads.push((dx, dy, dw, dh, colors::PANEL_MENU_BG));
                if let Some(di) = self.hovered_item {
                    quads.push((dx, dy + di as f32 * DROPDOWN_ITEM_H, dw, DROPDOWN_ITEM_H, colors::PANEL_MENU_HOVER));
                }
            }
        }
        quads
    }

    fn extra_arcs(&self) -> Vec<(f32, f32, f32, f32, f32, f32, [f32; 4])> {
        let mut arcs = Vec::new();
        if let Some((cx, cy, r, thickness, start_angle, end_angle)) = self.curved_arc {
            if self.base.hovered && !self.open {
                arcs.push((cx, cy, r, thickness, start_angle, end_angle, colors::PANEL_MENU_HOVER));
            }
        }
        arcs
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let label_color = crate::colors::paginator_tab_label_color();
        let srgb = crate::colors::to_srgb(label_color);
        let text_color = [
            (srgb[0] * 255.0) as u8,
            (srgb[1] * 255.0) as u8,
            (srgb[2] * 255.0) as u8,
        ];
        let mut labels = Vec::new();
        if let Some((cx, cy, r, thickness, start_angle, end_angle)) = self.curved_arc {
            let r_mid = r - thickness / 2.0;
            labels.extend(TextLabel::curved_layout(
                &self.active_title(),
                cx, cy, r_mid,
                start_angle, end_angle,
                12.0,
                text_color,
            ));
        } else if self.vertical {
            let font_size = 12.0;
            let line_height = font_size * 1.2;
            let title = self.active_title();
            let label_len = title.chars().count() as f32;
            let total_h = label_len * line_height;
            let start_y = self.base.y + (self.base.h - total_h) / 2.0;
            let char_w = TextLabel::estimate_width("o", font_size);
            let x_pos = self.base.x + (self.base.w - char_w) / 2.0;
            for (i, c) in title.chars().enumerate() {
                let char_str = c.to_string();
                let y_pos = start_y + i as f32 * line_height;
                labels.push(TextLabel {
                    text: char_str,
                    x: x_pos,
                    y: y_pos,
                    font_size,
                    color: text_color,
                });
            }
        } else {
            labels.push(TextLabel {
                text: self.active_title().to_string(),
                x: self.base.x + crate::layout::paginator_tab_padding_x(),
                y: self.base.y + 7.0,
                font_size: 12.0,
                color: text_color,
            });
        }
        if self.open {
            let (dx, dy, _, _) = self.dropdown_rect();
            for (i, item) in self.items.iter().enumerate() {
                let checked = self.item_checked.get(i).and_then(|&v| v);
                let prefix = match checked {
                    Some(true) => "\u{2713} ",
                    Some(false) => "  ",
                    None => "",
                };
                labels.push(TextLabel {
                    text: format!("{}{}", prefix, item),
                    x: dx + 8.0,
                    y: dy + i as f32 * DROPDOWN_ITEM_H + 5.0,
                    font_size: 12.0,
                    color: text_color,
                });
            }
        }
        labels
    }

    fn set_visible(&mut self, visible: bool) {
        self.base.hovered = false;
        if !visible {
            self.open = false;
            self.was_open = None;
            self.hovered_item = None;
            focus::clear_if_matches(self);
        }
    }

    fn parent(&self, ctx: &UiContext) -> Option<*mut (dyn Element + 'static)> {
        self.parent
    }

    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>, ctx: &mut UiContext) {
        self.parent = parent;
    }

    fn z_index(&self) -> i32 {
        100
    }

    fn menu_items(&self) -> Vec<String> {
        self.items.clone()
    }

    fn menu_item_checked(&self) -> Vec<Option<bool>> {
        self.item_checked.clone()
    }

    fn is_vertical(&self) -> bool {
        self.vertical
    }

    fn focused(&self, ctx: &UiContext) -> bool {
        self.base.focused
    }

    fn prepare_text(&mut self, fs: &mut glyphon::FontSystem) {
        let font_setting = crate::layout::menubar_font();
        if self.font_family != font_setting {
            self.font_family = font_setting.clone();
            self.title_buf = None;
            self.curved_char_bufs.clear();
            self.item_bufs.clear();
        }
        let title_text = self.active_title();
        let (font_fam, font_size_opt) = crate::layout::parse_font_string(&font_setting);
        let font_size = font_size_opt.unwrap_or(12.0);

        if let Some((_cx, _cy, _r, _thickness, _start_angle, _end_angle)) = self.curved_arc {
            if self.curved_char_bufs.len() != title_text.chars().count() {
                let font_fam_clone = font_fam.clone();
                self.curved_char_bufs = title_text.chars()
                    .map(|c| make_widget_text_buffer(fs, &c.to_string(), font_size, &font_fam_clone))
                    .collect();
            }
            self.title_buf = None;
        } else if self.vertical {
            self.title_buf = None;
            self.curved_char_bufs.clear();
        } else {
            if self.title_buf.is_none() {
                self.title_buf = Some(make_widget_text_buffer(fs, title_text, font_size, &font_fam));
            }
            self.curved_char_bufs.clear();
        }

        if self.open {
            if self.item_bufs.len() != self.items.len() {
                let font_fam_clone = font_fam.clone();
                self.item_bufs = self.items.iter().enumerate().map(|(i, item)| {
                    let checked = self.item_checked.get(i).and_then(|&v| v);
                    let prefix = match checked {
                        Some(true) => "\u{2713} ",
                        Some(false) => "  ",
                        None => "",
                    };
                    let text = format!("{}{}", prefix, item);
                    make_widget_text_buffer(fs, &text, font_size, &font_fam_clone)
                }).collect();
            }
        } else {
            self.item_bufs.clear();
        }
    }

    fn get_text_items(&self) -> Vec<(&glyphon::Buffer, f32, f32, glyphon::Color)> {
        let mut items = Vec::new();
        let label_color = crate::colors::paginator_tab_label_color();
        let srgb = crate::colors::to_srgb(label_color);
        let color = glyphon::Color::rgb(
            (srgb[0] * 255.0) as u8,
            (srgb[1] * 255.0) as u8,
            (srgb[2] * 255.0) as u8,
        );

        let font_setting = crate::layout::menubar_font();
        let (_, font_size_opt) = crate::layout::parse_font_string(&font_setting);
        let font_size = font_size_opt.unwrap_or(12.0);

        if let Some((cx, cy, r, thickness, start_angle, end_angle)) = self.curved_arc {
            let r_mid = r - thickness / 2.0;
            let active_title = self.active_title();
            
            let char_widths: Vec<f32> = active_title.chars().map(|c| {
                TextLabel::estimate_width(&c.to_string(), font_size)
            }).collect();
            let total_chars_width: f32 = char_widths.iter().sum();
            
            let angular_width = total_chars_width / r_mid;
            let text_start_angle = (start_angle + end_angle) / 2.0 - angular_width / 2.0;
            let mut cur_char_angle = text_start_angle;
            
            for (char_idx, c_buf) in self.curved_char_bufs.iter().enumerate() {
                if char_idx < char_widths.len() {
                    let cw = char_widths[char_idx];
                    let dtheta = cw / r_mid;
                    let char_center_angle = cur_char_angle + dtheta / 2.0;
                    
                    let tx = cx + r_mid * char_center_angle.cos() - cw / 2.0;
                    let ty = cy + r_mid * char_center_angle.sin() - font_size / 2.0;
                    
                    items.push((c_buf, tx, ty, color));
                    cur_char_angle += dtheta;
                }
            }
        } else if !self.vertical {
            if let Some(ref title_buf) = self.title_buf {
                let text_y = self.base.y + (self.base.h - font_size) / 2.0;
                items.push((title_buf, self.base.x + crate::layout::paginator_tab_padding_x(), text_y, color));
            }
        }

        if self.open {
            let (dx, dy, _, _) = self.dropdown_rect();
            for (i, item_buf) in self.item_bufs.iter().enumerate() {
                items.push((
                    item_buf,
                    dx + 8.0,
                    dy + i as f32 * DROPDOWN_ITEM_H + 5.0,
                    color,
                ));
            }
        }

        items
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::menubar_font())
    }
}

unsafe impl Send for Menu {}
unsafe impl Sync for Menu {}

impl Drop for Menu {
    fn drop(&mut self) {
        focus::clear_if_matches(self);
    }
}
