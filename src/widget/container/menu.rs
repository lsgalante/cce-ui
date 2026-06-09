use crate::colors;
use crate::widget::*;
use crate::widget::display::{make_widget_text_buffer, TextLabel};

pub struct MenuBar {
    pub base: Plate,
    pub title: String,
    pub menus: Vec<Box<Menu>>,
    pub menu_items: Vec<String>,
    pub vertical_items: Vec<String>,
    pub menu_dropdowns: Vec<Vec<String>>,
    pub menu_dropdown_checked: Vec<Vec<Option<bool>>>,
    pub hovered_menu: Option<usize>,
    pub open_menu: Option<usize>,
    pub hovered_dropdown: Option<usize>,
    pub clicked_dropdown: Option<(usize, usize)>,
    pub was_open: Option<usize>,
    pub vertical: bool,
    pub focused: bool,
    pub z_level: i32,
    pub center_items: bool,
    pub title_pos: Option<(f32, f32)>,
    pub title_buf: Option<glyphon::Buffer>,
    pub curved_title_char_bufs: Vec<glyphon::Buffer>,
    pub font_family: String,
    pub label: Option<String>,
}

impl MenuBar {
    pub fn set_curved_circle(&mut self, circle: Option<(f32, f32, f32)>) {
        self.base.curved_circle = circle;
        if circle.is_none() {
            for menu in &mut self.menus {
                menu.curved_arc = None;
            }
        }
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
            menus: Vec::new(),
            menu_items: Vec::new(),
            vertical_items: Vec::new(),
            menu_dropdowns: Vec::new(),
            menu_dropdown_checked: Vec::new(),
            hovered_menu: None,
            open_menu: None,
            hovered_dropdown: None,
            clicked_dropdown: None,
            was_open: None,
            vertical: false,
            focused: false,
            z_level: 100,
            center_items: false,
            title_pos: None,
            title_buf: None,
            curved_title_char_bufs: Vec::new(),
            font_family: crate::layout::menubar_font(),
            label: None,
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

        let item_strs: Vec<String> = items.iter().map(|s| s.to_string()).collect();
        let mut menu = Menu::new(label, label, &item_strs);
        menu.vertical = self.vertical;
        self.menus.push(Box::new(menu));
        self
    }

    pub fn with_item_vh(mut self, horizontal_label: &str, vertical_label: &str, items: &[&str]) -> Self {
        self.menu_items.push(horizontal_label.to_string());
        self.vertical_items.push(vertical_label.to_string());
        self.menu_dropdowns.push(items.iter().map(|s| s.to_string()).collect());
        self.menu_dropdown_checked.push(vec![None; items.len()]);

        let item_strs: Vec<String> = items.iter().map(|s| s.to_string()).collect();
        let mut menu = Menu::new(horizontal_label, vertical_label, &item_strs);
        menu.vertical = self.vertical;
        self.menus.push(Box::new(menu));
        self
    }

    pub fn with_vertical(mut self, vertical: bool) -> Self {
        self.vertical = vertical;
        for menu in &mut self.menus {
            menu.vertical = vertical;
        }
        self
    }

    pub fn with_z_index(mut self, z: i32) -> Self {
        self.z_level = z;
        self
    }

    fn item_h_vertical(&self, idx: usize) -> f32 {
        let font_size = 12.0;
        let line_height = font_size * 1.2;
        let padding_y = 12.0;
        if let Some(menu) = self.menus.get(idx) {
            let label_len = menu.active_title().chars().count() as f32;
            label_len * line_height + padding_y
        } else {
            24.0
        }
    }

    fn item_y_vertical(&self, idx: usize) -> f32 {
        let mut y = 8.0;
        if let Some(ref label) = self.label {
            let font_size = 12.0;
            let line_height = font_size * 1.2;
            let label_h = label.chars().count() as f32 * line_height;
            y += label_h + 8.0;
        }
        if !self.title.is_empty() {
            let font_size = 12.0;
            let line_height = font_size * 1.2;
            let title_h = self.title.chars().count() as f32 * line_height;
            y += title_h + 8.0;
        }
        for i in 0..idx {
            y += self.item_h_vertical(i);
        }
        y
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
            let total_h = if self.menus.is_empty() {
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
                    let title_h = self.title.chars().count() as f32 * line_height;
                    h += title_h + 8.0;
                }
                h
            } else {
                let last_idx = self.menus.len() - 1;
                self.item_y_vertical(last_idx) + self.item_h_vertical(last_idx)
            };
            (self.base.base.x, self.base.base.y, self.base.base.w, total_h)
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
                let title_h = self.title.chars().count() as f32 * line_height;
                cy += title_h + 8.0;
            }
            let item_heights: Vec<f32> = (0..self.menus.len())
                .map(|idx| self.item_h_vertical(idx))
                .collect();
            let mut dummy = crate::context::UiContext::new();
            for (idx, menu) in self.menus.iter_mut().enumerate() {
                let item_h = item_heights[idx];
                menu.set_rect(x, y + cy, w, item_h);
                menu.set_parent(Some(parent_ptr), &mut dummy);
                cy += item_h;
            }
        } else {
            let padding_x = crate::layout::paginator_tab_padding_x();
            if let Some((ccx, ccy, ccr)) = self.base.curved_circle {
                let r_mid = ccr - h / 2.0;
                let mut total_width = 8.0;
                if !self.title.is_empty() {
                    total_width += self.title.len() as f32 * char_w + 24.0;
                }
                for menu in &self.menus {
                    total_width += menu.active_title().len() as f32 * char_w + 2.0 * padding_x;
                }
                
                let total_angular_width = total_width / r_mid;
                let start_angle = 1.5 * std::f32::consts::PI - total_angular_width / 2.0;
                let mut current_angle = start_angle;
                
                if !self.title.is_empty() {
                    let title_w = self.title.len() as f32 * char_w + 24.0;
                    let dtheta_title = title_w / r_mid;
                    let theta_title = current_angle + dtheta_title / 2.0;
                    
                    let tx = ccx + r_mid * theta_title.cos() - title_w / 2.0 + 8.0;
                    let ty = ccy + r_mid * theta_title.sin() - h / 2.0;
                    self.title_pos = Some((tx, ty));
                    current_angle += dtheta_title;
                } else {
                    self.title_pos = None;
                }
                
                let mut dummy = crate::context::UiContext::new();
                for menu in &mut self.menus {
                    let iw = menu.active_title().len() as f32 * char_w + 2.0 * padding_x;
                    let dtheta_menu = iw / r_mid;
                    let theta_menu = current_angle + dtheta_menu / 2.0;
                    
                    let mx = ccx + r_mid * theta_menu.cos() - iw / 2.0;
                    let my = ccy + r_mid * theta_menu.sin() - h / 2.0;
                    
                    menu.set_rect(mx, my, iw, h);
                    menu.set_parent(Some(parent_ptr), &mut dummy);
                    menu.curved_arc = Some((ccx, ccy, ccr, h, current_angle, current_angle + dtheta_menu));
                    current_angle += dtheta_menu;
                }
            } else {
                self.title_pos = None;
                let mut cx = 8.0;
                if self.center_items {
                    let mut total_width = 8.0;
                    if !self.title.is_empty() {
                        total_width += self.title.len() as f32 * char_w + 24.0;
                    }
                    for menu in &self.menus {
                        total_width += menu.active_title().len() as f32 * char_w + 2.0 * padding_x;
                    }
                    if self.base.base.w > total_width {
                        cx = (self.base.base.w - total_width) / 2.0;
                    }
                }
                if !self.title.is_empty() {
                    cx += self.title.len() as f32 * char_w + 24.0;
                }
                let mut dummy = crate::context::UiContext::new();
                for menu in &mut self.menus {
                    let iw = menu.active_title().len() as f32 * char_w + 2.0 * padding_x;
                    menu.set_rect(x + cx, y, iw, h);
                    menu.set_parent(Some(parent_ptr), &mut dummy);
                    cx += iw;
                }
            }
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
        let font_setting = crate::layout::menubar_font();
        let (_, font_size_opt) = crate::layout::parse_font_string(&font_setting);
        let font_size = font_size_opt.unwrap_or(12.0);
        let char_w = 7.5 * (font_size / 12.0);

        if let Some((ccx, ccy, ccr)) = self.base.curved_circle {
            let dx = px - ccx;
            let dy = py - ccy;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist >= ccr - self.base.base.h && dist <= ccr {
                let angle = dy.atan2(dx);
                let mut norm_angle = angle;
                if norm_angle < 0.0 {
                    norm_angle += 2.0 * std::f32::consts::PI;
                }
                
                let r_mid = ccr - self.base.base.h / 2.0;
                let mut total_width = 8.0;
                if !self.title.is_empty() {
                    total_width += self.title.len() as f32 * char_w + 24.0;
                }
                let padding_x = crate::layout::paginator_tab_padding_x();
                for menu in &self.menus {
                    total_width += menu.active_title().len() as f32 * char_w + 2.0 * padding_x;
                }
                let total_angular_width = total_width / r_mid;
                let start_angle = 1.5 * std::f32::consts::PI - total_angular_width / 2.0;
                let end_angle = 1.5 * std::f32::consts::PI + total_angular_width / 2.0;
                
                if norm_angle >= start_angle && norm_angle <= end_angle {
                    return true;
                }
            }
            for menu in &self.menus {
                if menu.hit_test(px, py, ctx) {
                    return true;
                }
            }
            return false;
        }
        let (rx, ry, rw, rh) = self.rect();
        if px >= rx && px <= rx + rw && py >= ry && py <= ry + rh {
            return true;
        }
        for menu in &self.menus {
            if menu.hit_test(px, py, ctx) {
                return true;
            }
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
        self.hovered_menu = None;
        for (idx, menu) in self.menus.iter_mut().enumerate() {
            if menu.cursor_moved(px, py, ctx) {
                changed = true;
            }
            if menu.hovered() {
                self.hovered_menu = Some(idx);
            }
        }
        changed
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if !self.base.visible {
            return false;
        }
        let (rx, ry, rw, rh) = self.rect();
        self.set_rect(rx, ry, rw, rh);

        let mut changed = false;
        for menu in &mut self.menus {
            let res = menu.mouse_input(button, state, px, py, ctx);
            if res {
                changed = true;
            }
        }
        if !self.is_menu_open() {
            self.unfocus();
        }
        changed
    }

    fn focus(&mut self) {
        if self.is_menu_open() {
            self.focused = true;
            for menu in &mut self.menus {
                if menu.is_menu_open() {
                    menu.focus();
                    return;
                }
            }
        } else {
            self.focused = false;
            focus::clear_if_matches(self);
            return;
        }
        self.focused = true;
        focus::set_focused(self);
    }

    fn unfocus(&mut self) {
        self.focused = false;
        focus::clear_if_matches(self);
        for menu in &mut self.menus {
            menu.unfocus();
        }
    }

    fn focused(&self, ctx: &UiContext) -> bool {
        self.focused || self.is_menu_open()
    }

    fn set_selected(&mut self, selected: bool) {
        self.focused = selected;
        if !selected {
            for menu in &mut self.menus {
                menu.set_selected(false);
            }
        }
    }

    fn set_modifiers(&mut self, ctrl: bool, shift: bool, alt: bool) {
        for menu in &mut self.menus {
            menu.set_modifiers(ctrl, shift, alt);
        }
    }

    fn menu_names(&self) -> Vec<String> {
        self.menu_items.clone()
    }

    fn menu_click(&mut self) -> Option<(usize, usize)> {
        for (idx, menu) in self.menus.iter_mut().enumerate() {
            if let Some((_, item_idx)) = menu.menu_click() {
                return Some((idx, item_idx));
            }
        }
        None
    }

    fn set_item_checked(&mut self, menu_idx: usize, item_idx: usize, checked: bool) {
        if let Some(menu) = self.menu_dropdown_checked.get_mut(menu_idx) {
            if item_idx < menu.len() {
                menu[item_idx] = Some(checked);
            }
        }
        if let Some(menu) = self.menus.get_mut(menu_idx) {
            menu.set_item_checked(0, item_idx, checked);
        }
    }

    fn set_menu_items(&mut self, menu_idx: usize, items: &[String]) {
        if menu_idx < self.menu_dropdowns.len() {
            self.menu_dropdowns[menu_idx] = items.to_vec();
            self.menu_dropdown_checked[menu_idx] = vec![Some(false); items.len()];
        }
        if let Some(menu) = self.menus.get_mut(menu_idx) {
            menu.items = items.to_vec();
            menu.item_checked = vec![Some(false); items.len()];
            menu.item_bufs.clear();
        }
    }

    fn is_menu_bar(&self) -> bool {
        self.base.visible
    }

    fn get_menu_items_at(&self, px: f32, py: f32) -> Option<(usize, String, Vec<String>, f32, f32, f32, f32)> {
        if !self.base.visible {
            return None;
        }
        let dummy = crate::context::UiContext::new();
        for (idx, menu) in self.menus.iter().enumerate() {
            if menu.hit_test(px, py, &dummy) {
                let mut formatted_items = Vec::new();
                for (i, item) in menu.items.iter().enumerate() {
                    let checked = menu.item_checked.get(i).and_then(|&v| v);
                    let prefix = match checked {
                        Some(true) => "✓ ",
                        Some(false) => "  ",
                        None => "",
                    };
                    formatted_items.push(format!("{}{}", prefix, item));
                }
                return Some((idx, menu.active_title().to_string(), formatted_items, menu.base.x, menu.base.y, menu.base.w, menu.base.h));
            }
        }
        None
    }

    fn trigger_menu_click(&mut self, menu_idx: usize, item_idx: usize) {
        if let Some(menu) = self.menus.get_mut(menu_idx) {
            menu.clicked_item = Some(item_idx);
        }
    }

    fn is_menu_open(&self) -> bool {
        self.base.visible && self.menus.iter().any(|m| m.is_menu_open())
    }

    fn all_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.base.visible {
            return Vec::new();
        }
        let mut quads = Vec::new();
        quads.extend(self.base.all_quads(ctx));
        for menu in &self.menus {
            quads.extend(menu.all_quads(ctx));
        }
        quads
    }

    fn extra_arcs(&self) -> Vec<(f32, f32, f32, f32, f32, f32, [f32; 4])> {
        if !self.base.visible {
            return Vec::new();
        }
        let mut arcs = Vec::new();
        for menu in &self.menus {
            arcs.extend(menu.extra_arcs());
        }
        arcs
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
            for menu in &mut self.menus {
                menu.title_buf = None;
                menu.curved_char_bufs.clear();
                menu.item_bufs.clear();
            }
        }
        let (font_fam, font_size_opt) = crate::layout::parse_font_string(&self.font_family);
        let font_size = font_size_opt.unwrap_or(12.0);

        if !self.title.is_empty() {
            if let Some((_ccx, _ccy, _ccr)) = self.base.curved_circle {
                if self.curved_title_char_bufs.len() != self.title.chars().count() {
                    let font_fam_clone = font_fam.clone();
                    self.curved_title_char_bufs = self.title.chars()
                        .map(|c| make_widget_text_buffer(fs, &c.to_string(), font_size, &font_fam_clone))
                        .collect();
                }
                self.title_buf = None;
            } else if self.vertical {
                self.title_buf = None;
                self.curved_title_char_bufs.clear();
            } else {
                if self.title_buf.is_none() {
                    self.title_buf = Some(make_widget_text_buffer(fs, &self.title, font_size, &font_fam));
                }
                self.curved_title_char_bufs.clear();
            }
        } else {
            self.title_buf = None;
            self.curved_title_char_bufs.clear();
        }
        for menu in &mut self.menus {
            menu.prepare_text(fs);
        }
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
            if !self.title.is_empty() {
                total_width += self.title.len() as f32 * char_w + 24.0;
            }
            for menu in &self.menus {
                total_width += menu.active_title().len() as f32 * char_w + 2.0 * padding_x;
            }
            let total_angular_width = total_width / r_mid;
            let start_angle = 1.5 * std::f32::consts::PI - total_angular_width / 2.0;
            let current_angle = start_angle;

            if !self.title.is_empty() {
                let title_w = self.title.len() as f32 * char_w + 24.0;
                let dtheta_title = title_w / r_mid;

                let char_widths: Vec<f32> = self.title.chars().map(|c| {
                    TextLabel::estimate_width(&c.to_string(), font_size)
                }).collect();
                let total_chars_width: f32 = char_widths.iter().sum();
                let mid_angle = (current_angle + current_angle + dtheta_title) / 2.0;
                let angular_width = total_chars_width / r_mid;
                let text_start_angle = mid_angle - angular_width / 2.0;
                let mut cur_char_angle = text_start_angle;

                for (char_idx, c_buf) in self.curved_title_char_bufs.iter().enumerate() {
                    let cw = char_widths[char_idx];
                    let dtheta = cw / r_mid;
                    let char_center_angle = cur_char_angle + dtheta / 2.0;

                    let tx = ccx + r_mid * char_center_angle.cos() - cw / 2.0;
                    let ty = ccy + r_mid * char_center_angle.sin() - font_size / 2.0;

                    items.push((c_buf, tx, ty, color));
                    cur_char_angle += dtheta;
                }
            }
        } else if !self.vertical {
            let mut start_x = 8.0;
            if self.center_items {
                let mut total_width = 8.0;
                if !self.title.is_empty() {
                    total_width += self.title.len() as f32 * char_w + 24.0;
                }
                for menu in &self.menus {
                    total_width += menu.active_title().len() as f32 * char_w + 2.0 * padding_x;
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

        for menu in &self.menus {
            items.extend(menu.get_text_items());
        }
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

        if let Some((ccx, ccy, ccr)) = self.base.curved_circle {
            let r_mid = ccr - self.base.base.h / 2.0;
            let mut total_width = 8.0;
            if !self.title.is_empty() {
                total_width += self.title.len() as f32 * char_w + 24.0;
            }
            for menu in &self.menus {
                total_width += menu.active_title().len() as f32 * char_w + 2.0 * padding_x;
            }
            let total_angular_width = total_width / r_mid;
            let start_angle = 1.5 * std::f32::consts::PI - total_angular_width / 2.0;
            let current_angle = start_angle;

            if !self.title.is_empty() {
                let title_w = self.title.len() as f32 * char_w + 24.0;
                let dtheta_title = title_w / r_mid;
                labels.extend(TextLabel::curved_layout(
                    &self.title,
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
                for (i, c) in self.title.chars().enumerate() {
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
                    total_width += self.title.len() as f32 * char_w + 24.0;
                }
                for menu in &self.menus {
                    total_width += menu.active_title().len() as f32 * char_w + 2.0 * padding_x;
                }
                if self.base.base.w > total_width {
                    start_x = (self.base.base.w - total_width) / 2.0;
                }
            }
            if !self.title.is_empty() {
                let text_y = self.base.base.y + (self.base.base.h - font_size) / 2.0;
                labels.push(TextLabel {
                    text: self.title.clone(),
                    x: self.base.base.x + start_x,
                    y: text_y,
                    font_size,
                    color: text_color,
                });
            }
        }
        for menu in &self.menus {
            labels.extend(menu.text_labels());
        }
        labels
    }

    fn set_visible(&mut self, visible: bool) {
        self.base.set_visible(visible);
        for menu in &mut self.menus {
            menu.set_visible(visible);
        }
    }

    fn visible(&self) -> bool {
        self.base.visible()
    }

    fn children(&self, _ctx: &UiContext) -> Vec<*mut (dyn Element + 'static)> {
        self.menus.iter().map(|m| {
            let ptr: *const dyn Element = &**m as &dyn Element;
            ptr as *mut (dyn Element + 'static)
        }).collect()
    }

    fn z_index(&self) -> i32 {
        self.z_level
    }

    fn set_center_items(&mut self, center: bool) {
        self.center_items = center;
    }

    fn menu_items_list(&self) -> Vec<Vec<String>> {
        self.menu_dropdowns.clone()
    }

    fn menu_checked_list(&self) -> Vec<Vec<Option<bool>>> {
        self.menu_dropdown_checked.clone()
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.base.visible {
            return Vec::new();
        }
        let mut quads = Vec::new();
        for menu in &self.menus {
            quads.extend(menu.extra_quads());
        }
        quads
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::menubar_font())
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
