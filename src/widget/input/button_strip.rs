use crate::colors;
use crate::widget::*;
use crate::widget::input::get_font_db;

#[derive(Debug, Clone)]
pub struct ButtonStrip {
    pub base: Widget,
    pub buttons: Vec<String>,
    pub selected: Option<usize>,
    pub vertical: bool,
    pub just_clicked: Option<usize>,
    pub hovered_idx: Option<usize>,
    pub pressed_idx: Option<usize>,
    pub tab_text_quads: Vec<Vec<(f32, f32, f32, f32, [f32; 4])>>,
    pub tab_quads_cache: std::collections::HashMap<String, Vec<(f32, f32, f32, f32, [f32; 4])>>,
    pub last_padding: Option<f32>,
}

impl ButtonStrip {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            base: Widget::new_rect(x, y, w, h),
            buttons: Vec::new(),
            selected: None,
            vertical: false,
            just_clicked: None,
            hovered_idx: None,
            pressed_idx: None,
            tab_text_quads: Vec::new(),
            tab_quads_cache: std::collections::HashMap::new(),
            last_padding: None,
        }
    }

    pub fn with_buttons(mut self, buttons: Vec<String>) -> Self {
        self.buttons = buttons;
        self.generate_rotated_labels();
        self
    }

    pub fn with_selected(mut self, selected: Option<usize>) -> Self {
        self.selected = selected;
        self.generate_rotated_labels();
        self
    }

    pub fn with_vertical(mut self, vertical: bool) -> Self {
        self.vertical = vertical;
        self.generate_rotated_labels();
        self
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn set_selected(&mut self, selected: Option<usize>) {
        if self.selected != selected {
            self.selected = selected;
            self.generate_rotated_labels();
        }
    }

    pub fn take_click(&mut self) -> Option<usize> {
        self.just_clicked.take()
    }

    pub fn add_button(&mut self, label: &str) {
        self.buttons.push(label.to_string());
        self.generate_rotated_labels();
    }

    pub fn generate_rotated_labels(&mut self) {
        let current_padding = crate::layout::button_padding();
        self.last_padding = Some(current_padding);

        self.tab_text_quads.clear();
        if !self.vertical || self.buttons.is_empty() {
            return;
        }

        let active_color = colors::menubar_tab_label_color();
        let active_srgb = colors::to_srgb(active_color);
        let active_r = (active_srgb[0] * 255.0) as u8;
        let active_g = (active_srgb[1] * 255.0) as u8;
        let active_b = (active_srgb[2] * 255.0) as u8;
        let inactive_r = (active_r as f32 * 0.78) as u8;
        let inactive_g = (active_g as f32 * 0.78) as u8;
        let inactive_b = (active_b as f32 * 0.78) as u8;

        let (font_fam, font_size) = crate::layout::menubar_font_parsed();
        let scale = crate::scale::scale_factor().max(1.0);

        for (i, page_name) in self.buttons.iter().enumerate() {
            let color = if self.selected == Some(i) {
                [active_r, active_g, active_b]
            } else {
                [inactive_r, inactive_g, inactive_b]
            };
            let hex_color = format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2]);

            let trimmed = page_name.trim();
            let space_idx = trimmed.find(' ');
            let has_icon = space_idx.map(|idx| trimmed.split_at(idx).0.trim().chars().count() == 1).unwrap_or(false);
            let label_text = if has_icon {
                trimmed.split_at(space_idx.unwrap()).1.trim()
            } else {
                trimmed
            };

            let r = self.item_rect(i);
            let padding_y = crate::layout::button_padding();
            let y_offset = if has_icon { padding_y + 12.0 } else { 0.0 };
            let usable_h = (r.3 - y_offset).max(1.0);

            let vertical_buffer = 8.0;
            let w_px = (r.2 * scale) as u32;
            let h_px = ((usable_h + 2.0 * vertical_buffer) * scale) as u32;

            if w_px == 0 || h_px == 0 {
                self.tab_text_quads.push(Vec::new());
                continue;
            }

            let cache_key = format!("{}:{}:{}:{:?}:{}:{}", trimmed, w_px, h_px, color, font_fam, scale);
            if let Some(cached_quads) = self.tab_quads_cache.get(&cache_key) {
                self.tab_text_quads.push(cached_quads.clone());
                continue;
            }

            let svg_data = format!(
                r##"<svg width="{}" height="{}" viewBox="0 0 {} {}" xmlns="http://www.w3.org/2000/svg">
  <text x="{}" y="{}" font-family="{}" font-size="{}" fill="{}" text-anchor="middle" dominant-baseline="middle" transform="rotate(-90 {} {})">{}</text>
</svg>"##,
                w_px, h_px,
                r.2, usable_h + 2.0 * vertical_buffer,
                r.2 / 2.0, usable_h / 2.0 + vertical_buffer,
                font_fam,
                font_size,
                hex_color,
                r.2 / 2.0, usable_h / 2.0 + vertical_buffer,
                label_text
            );

            let opt = resvg::usvg::Options::default();
            let fontdb = get_font_db();
            
            let mut page_quads = Vec::new();
            if let Ok(tree) = resvg::usvg::Tree::from_data(svg_data.as_bytes(), &opt, fontdb) {
                if let Some(mut pixmap) = resvg::tiny_skia::Pixmap::new(w_px, h_px) {
                    resvg::render(&tree, resvg::tiny_skia::Transform::default(), &mut pixmap.as_mut());
                    let pixels = pixmap.data();
                    for row in 0..h_px {
                        for col in 0..w_px {
                            let idx = ((row * w_px + col) * 4) as usize;
                            if idx + 3 < pixels.len() {
                                let a = pixels[idx + 3] as f32 / 255.0;
                                if a > 0.0 {
                                    let r = ((pixels[idx] as f32 / 255.0) / a).min(1.0);
                                    let g = ((pixels[idx + 1] as f32 / 255.0) / a).min(1.0);
                                    let b = ((pixels[idx + 2] as f32 / 255.0) / a).min(1.0);
                                    page_quads.push((
                                        col as f32 / scale,
                                        row as f32 / scale,
                                        1.2 / scale,
                                        1.2 / scale,
                                        [r, g, b, a],
                                    ));
                                }
                            }
                        }
                    }
                }
            }
            self.tab_quads_cache.insert(cache_key, page_quads.clone());
            self.tab_text_quads.push(page_quads);
        }
    }

    pub fn item_rect(&self, idx: usize) -> (f32, f32, f32, f32) {
        if self.buttons.is_empty() || idx >= self.buttons.len() {
            return (0.0, 0.0, 0.0, 0.0);
        }
        let (x, y, w, h) = self.rect();

        let get_button_weight = |i: usize| -> f32 {
            let label = &self.buttons[i];
            let trimmed = label.trim();
            let font_size = crate::layout::menubar_font_parsed().1;
            let padding = crate::layout::button_padding();
            if self.vertical {
                let space_idx = trimmed.find(' ');
                let has_icon = space_idx.map(|idx| trimmed.split_at(idx).0.trim().chars().count() == 1).unwrap_or(false);
                let label_text = if has_icon {
                    trimmed.split_at(space_idx.unwrap()).1.trim()
                } else {
                    trimmed
                };
                let text_w = TextLabel::estimate_width(label_text, font_size);
                if has_icon {
                    (text_w + 12.0 + 3.0 * padding).max(1.0)
                } else {
                    (text_w + 2.0 * padding).max(1.0)
                }
            } else {
                let text_w = TextLabel::estimate_width(label, font_size);
                (text_w + 2.0 * padding).max(1.0)
            }
        };

        let mut weights = Vec::new();
        for i in 0..self.buttons.len() {
            weights.push(get_button_weight(i));
        }

        if self.vertical {
            let spacing = 8.0;
            let mut current_y = y;
            let mut btn_h = 0.0;
            for i in 0..=idx {
                btn_h = weights[i];
                if i < idx {
                    current_y += btn_h + spacing;
                }
            }
            (x, current_y, w, btn_h)
        } else {
            let mut current_x = x;
            let mut btn_w = 0.0;
            for i in 0..=idx {
                btn_w = weights[i];
                if i < idx {
                    current_x += btn_w;
                }
            }
            (current_x, y, btn_w, h)
        }
    }
}

impl Element for ButtonStrip {
    crate::impl_widget_base!(ButtonStrip);

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if self.base.x != x || self.base.y != y || self.base.w != w || self.base.h != h {
            self.base.x = x;
            self.base.y = y;
            self.base.w = w;
            self.base.h = h;
            self.generate_rotated_labels();
        }
    }

    fn tick(&mut self, _dt: f32, _ctx: &mut UiContext) -> bool {
        let current_padding = crate::layout::button_padding();
        if self.last_padding != Some(current_padding) {
            self.generate_rotated_labels();
            return true;
        }
        false
    }

    fn highlight_quad(&self, _ctx: &UiContext) -> Option<(f32, f32, f32, f32, [f32; 4])> {
        None
    }

    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, _ctx: &mut UiContext) -> bool {
        if button != MouseButton::Left {
            return false;
        }
        let mut changed = false;
        match state {
            ElementState::Pressed => {
                for i in 0..self.buttons.len() {
                    let r = self.item_rect(i);
                    if px >= r.0 && px < r.0 + r.2 && py >= r.1 && py < r.1 + r.3 {
                        self.pressed_idx = Some(i);
                        changed = true;
                        break;
                    }
                }
            }
            ElementState::Released => {
                if let Some(pressed) = self.pressed_idx {
                    let r = self.item_rect(pressed);
                    if px >= r.0 && px < r.0 + r.2 && py >= r.1 && py < r.1 + r.3 {
                        if self.selected != Some(pressed) {
                            self.selected = Some(pressed);
                            self.just_clicked = Some(pressed);
                            self.generate_rotated_labels();
                        } else {
                            self.selected = None;
                            self.just_clicked = Some(pressed);
                            self.generate_rotated_labels();
                        }
                    }
                    changed = true;
                }
                self.pressed_idx = None;
            }
        }
        changed
    }

    fn cursor_moved(&mut self, px: f32, py: f32, _ctx: &mut UiContext) -> bool {
        let old_hovered = self.hovered_idx;
        self.hovered_idx = None;
        for i in 0..self.buttons.len() {
            let r = self.item_rect(i);
            if px >= r.0 && px < r.0 + r.2 && py >= r.1 && py < r.1 + r.3 {
                self.hovered_idx = Some(i);
                break;
            }
        }
        old_hovered != self.hovered_idx
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        for i in 0..self.buttons.len() {
            let r = self.item_rect(i);
            let mut bg_color = [0.0, 0.0, 0.0, 0.0];
            if Some(i) == self.selected {
                bg_color = colors::PANEL_MENU_FOCUSED;
            } else if Some(i) == self.pressed_idx {
                bg_color = colors::BUTTON_PRESS;
            } else if Some(i) == self.hovered_idx {
                bg_color = colors::PANEL_MENU_HOVER;
            }
            if bg_color != [0.0, 0.0, 0.0, 0.0] {
                quads.push((r.0, r.1, r.2, r.3, bg_color));
            }

            if self.vertical {
                if i < self.tab_text_quads.len() {
                    let min_y = self.base.y;
                    let max_y = self.base.y + self.base.h;
                    let page_name = &self.buttons[i];
                    let trimmed = page_name.trim();
                    let space_idx = trimmed.find(' ');
                    let has_icon = space_idx.map(|idx| trimmed.split_at(idx).0.trim().chars().count() == 1).unwrap_or(false);
                    let padding_y = crate::layout::button_padding();
                    let y_offset = if has_icon { padding_y + 12.0 } else { 0.0 };

                    for &(qx, qy, qw, qh, qc) in &self.tab_text_quads[i] {
                        let absolute_x = r.0 + qx;
                        let absolute_y = r.1 + y_offset + qy - 8.0;
                        
                        let ry1 = absolute_y.max(min_y);
                        let ry2 = (absolute_y + qh).min(max_y);
                        let rh = ry2 - ry1;
                        if rh > 0.0 {
                            quads.push((absolute_x, ry1, qw, rh, qc));
                        }
                    }
                }
            }
        }
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        let font_size = 12.0;
        for (i, btn_label) in self.buttons.iter().enumerate() {
            let r = self.item_rect(i);
            let color = if Some(i) == self.selected {
                [0xf0, 0xf0, 0xf5]
            } else {
                [0xa8, 0xa8, 0xb3]
            };
            if self.vertical {
                let trimmed = btn_label.trim();
                let space_idx = trimmed.find(' ');
                let has_icon = space_idx.map(|idx| trimmed.split_at(idx).0.trim().chars().count() == 1).unwrap_or(false);
                if has_icon {
                    let space_idx = space_idx.unwrap();
                    let (icon, _) = trimmed.split_at(space_idx);
                    let icon = icon.trim();
                    if !icon.is_empty() {
                        let icon_font_size = 14.0;
                        let est_icon_w = TextLabel::estimate_width(icon, icon_font_size);
                        let padding_y = crate::layout::button_padding();
                        let icon_y = r.1 + (padding_y - 2.0).max(0.0);
                        labels.push(TextLabel {
                            text: icon.to_string(),
                            x: r.0 + (r.2 - est_icon_w) / 2.0,
                            y: icon_y,
                            font_size: icon_font_size,
                            color,
                        });
                    }
                }
            } else {
                let est_w = TextLabel::estimate_width(btn_label, font_size);
                labels.push(TextLabel {
                    text: btn_label.clone(),
                    x: r.0 + (r.2 - est_w) / 2.0,
                    y: crate::layout::align_text_y(r.1, r.3, font_size, 0.0),
                    font_size,
                    color,
                });
            }
        }
        labels
    }

    fn keyboard_input(&mut self, event: &KeyEvent, _ctx: &mut UiContext) -> bool {
        if event.state != ElementState::Pressed { return false; }
        if self.buttons.is_empty() { return false; }

        let current = self.selected.unwrap_or(0);
        let next;

        match event.logical_key {
            Key::Named(NamedKey::ArrowLeft) | Key::Named(NamedKey::ArrowUp) => {
                if current > 0 {
                    next = current - 1;
                } else {
                    next = self.buttons.len() - 1;
                }
            }
            Key::Named(NamedKey::ArrowRight) | Key::Named(NamedKey::ArrowDown) => {
                if current + 1 < self.buttons.len() {
                    next = current + 1;
                } else {
                    next = 0;
                }
            }
            _ => return false,
        }

        if Some(next) != self.selected {
            self.selected = Some(next);
            self.just_clicked = Some(next);
            self.generate_rotated_labels();
            return true;
        }
        false
    }
}

impl Control for ButtonStrip {}
