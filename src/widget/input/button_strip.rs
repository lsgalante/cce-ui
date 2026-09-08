use crate::colors;
use crate::widget::*;
use crate::widget::input::get_font_db;

#[derive(Debug, Clone)]
pub struct ButtonStrip {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    pub buttons: Vec<String>,
    pub selected: Option<usize>,
    pub vertical: bool,
    pub just_clicked: Option<usize>,
    pub hovered_idx: Option<usize>,
    pub pressed_idx: Option<usize>,
    pub tab_text_quads: Vec<Vec<(f32, f32, f32, f32, [f32; 4])>>,
    pub tab_quads_cache: std::collections::HashMap<String, Vec<(f32, f32, f32, f32, [f32; 4])>>,
    pub last_padding: Option<f32>,
    pub last_font: Option<String>,
    pub last_scale: Option<f32>,
    pub inherit_menubar_font: bool,
    /// The detached control label, synced from the adapter (`Paint::sync_label`):
    /// the strip's geometry keeps clear of the label strip above it.
    label: Option<String>,
    /// Recessed style: the strip is ONE well carved into the plate below (the
    /// menu's recess around its run), the segments butting together on its
    /// floor; the selected segment is a plateau raised back out of it, hover
    /// and press a wash. Defaults to `control_relief()`; the flat style keeps
    /// the plain state quads.
    pub recessed: bool,
}

impl ButtonStrip {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            x,
            y,
            w,
            h,
            buttons: Vec::new(),
            selected: None,
            vertical: false,
            just_clicked: None,
            hovered_idx: None,
            pressed_idx: None,
            tab_text_quads: Vec::new(),
            tab_quads_cache: std::collections::HashMap::new(),
            last_padding: None,
            last_font: None,
            last_scale: None,
            inherit_menubar_font: false,
            label: None,
            recessed: crate::layout::control_relief(),
        }
    }

    /// Recessed style: see the `recessed` field.
    pub fn with_recessed(mut self, recessed: bool) -> Self {
        self.recessed = recessed;
        self
    }

    pub fn with_inherit_menubar_font(mut self, inherit: bool) -> Self {
        self.inherit_menubar_font = inherit;
        self.generate_rotated_labels();
        self
    }

    /// The laid-out content rect: the rect mirrored from the adapter by
    /// `Layout::rect_assigned` (or the constructor arguments until the first layout)
    /// less the detached label strip the adapter inflated it by, so the segments,
    /// the well and the hit-testing all sit below the label.
    fn rect(&self) -> (f32, f32, f32, f32) {
        let strip = crate::widget::input::slider::detached_strip(&self.label);
        (self.x, self.y + strip, self.w, (self.h - strip).max(0.0))
    }

    fn current_font(&self) -> String {
        if self.inherit_menubar_font {
            crate::layout::menubar_font()
        } else {
            crate::layout::button_strip_font()
        }
    }

    fn current_font_parsed(&self) -> (String, f32) {
        if self.inherit_menubar_font {
            crate::layout::menubar_font_parsed()
        } else {
            crate::layout::button_strip_font_parsed()
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
        let current_font = self.current_font();
        self.last_font = Some(current_font);

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

        let (font_fam, font_size) = self.current_font_parsed();
        let scale = crate::scale::scale_factor().max(1.0);
        self.last_scale = Some(scale);

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

            let vertical_buffer = 30.0;
            let w_px = (r.2 * scale) as u32;
            let h_px = ((usable_h + vertical_buffer) * scale) as u32;

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
                r.2, usable_h + vertical_buffer,
                r.2 / 2.0, usable_h / 2.0 + vertical_buffer / 2.0,
                font_fam,
                font_size,
                hex_color,
                r.2 / 2.0, usable_h / 2.0 + vertical_buffer / 2.0,
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
            let font_info = self.current_font_parsed();
            let font_fam = font_info.0;
            let font_size = font_info.1;
            let padding = crate::layout::button_padding();
            if self.vertical {
                let space_idx = trimmed.find(' ');
                let has_icon = space_idx.map(|idx| trimmed.split_at(idx).0.trim().chars().count() == 1).unwrap_or(false);
                let label_text = if has_icon {
                    trimmed.split_at(space_idx.unwrap()).1.trim()
                } else {
                    trimmed
                };
                let text_w = crate::widget::display::measure_text_width(label_text, &font_fam, font_size);
                if has_icon {
                    (text_w + 12.0 + 3.0 * padding).max(1.0)
                } else {
                    (text_w + 2.0 * padding).max(1.0)
                }
            } else {
                let text_w = crate::widget::display::measure_text_width(label, &font_fam, font_size);
                (text_w + 2.0 * padding).max(1.0)
            }
        };

        let mut weights = Vec::new();
        for i in 0..self.buttons.len() {
            weights.push(get_button_weight(i));
        }

        let spacing = crate::layout::button_strip_spacing();
        if self.vertical {
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
                    current_x += btn_w + spacing;
                }
            }
            (current_x, y, btn_w, h)
        }
    }
}

impl crate::widget::Layout for ButtonStrip {
    /// A horizontal strip is one button row tall; a vertical one is content-sized.
    fn intrinsic_size(&self) -> Option<crate::scene::layout::Size> {
        if self.vertical {
            None
        } else {
            Some(crate::scene::layout::Size::new(0.0, crate::layout::button_height()))
        }
    }

    // The legacy set_rect override: regenerate the rotated tab labels only when the rect
    // actually changed.
    fn rect_assigned(&mut self, rect: crate::scene::layout::Rect) {
        if self.x != rect.x || self.y != rect.y || self.w != rect.width || self.h != rect.height {
            self.x = rect.x;
            self.y = rect.y;
            self.w = rect.width;
            self.h = rect.height;
            self.generate_rotated_labels();
        }
    }
}

impl crate::widget::Paint for ButtonStrip {
    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn widget_font(&self) -> Option<String> {
        Some(self.current_font())
    }

    fn sync_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    fn paint(&self, _rect: crate::scene::layout::Rect, pc: &mut crate::scene::paint::PaintCtx) {
        use crate::scene::layout::Rect;
        // The strip's well: one recess around the whole run, rounded like the
        // buttons, before the segments so their fills sit on its floor. The
        // segment fills stay plain quads either way — they are what reaches the
        // flat hosts that read this widget through `extra_quads`.
        let (sx, sy, sw, sh) = self.rect();
        let radius = crate::layout::button_corner_radius();
        let depth = if self.recessed {
            let short = if self.vertical { sw } else { sh };
            let depth = crate::layout::bevel_width().min(short * 0.2);
            let (well, radii) = crate::layout::carve_inside(Rect { x: sx, y: sy, width: sw, height: sh }, (radius, radius, radius, radius), depth);
            pc.recess(well, radii, depth);
            depth
        } else {
            0.0
        };
        // Per-item state backgrounds, plus the rotated (SVG-rasterized) vertical
        // tab text clamped to the strip.
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
                pc.quad(Rect { x: r.0, y: r.1, width: r.2, height: r.3 }, bg_color);
            }
            if self.recessed && Some(i) == self.selected {
                // The selected segment: a plateau raised back out of the well,
                // inset by the wall's inner half-span so it stands on the floor.
                let inset = depth * 0.5;
                let plateau = Rect { x: r.0 + inset, y: r.1 + inset, width: (r.2 - 2.0 * inset).max(0.0), height: (r.3 - 2.0 * inset).max(0.0) };
                let pr = (radius - inset).max(0.0);
                let (plateau, radii) = crate::layout::carve_inside(plateau, (pr, pr, pr, pr), depth);
                pc.boss(plateau, radii, depth);
            }

            if self.vertical {
                if i < self.tab_text_quads.len() {
                    let min_y = self.y;
                    let max_y = self.y + self.h;
                    let page_name = &self.buttons[i];
                    let trimmed = page_name.trim();
                    let space_idx = trimmed.find(' ');
                    let has_icon = space_idx.map(|idx| trimmed.split_at(idx).0.trim().chars().count() == 1).unwrap_or(false);
                    let padding_y = crate::layout::button_padding();
                    let y_offset = if has_icon { padding_y + 12.0 } else { 0.0 };

                    for &(qx, qy, qw, qh, qc) in &self.tab_text_quads[i] {
                        let absolute_x = r.0 + qx;
                        let absolute_y = r.1 + y_offset + qy - 15.0;

                        let ry1 = absolute_y.max(min_y);
                        let ry2 = (absolute_y + qh).min(max_y);
                        let rh = ry2 - ry1;
                        if rh > 0.0 {
                            pc.quad(Rect { x: absolute_x, y: ry1, width: qw, height: rh }, qc);
                        }
                    }
                }
            }
        }

        // Own labels (horizontal button text / vertical icon glyphs).
        for tl in self.own_labels() {
            pc.text(tl.text, tl.x, tl.y, tl.font_size, tl.color);
        }
    }
}

impl crate::widget::Input for ButtonStrip {
    // The legacy dispatch reached mouse_input ungated (the hosts call it directly, and a
    // press on another tab must land while a dropdown popover covers the strip); the item
    // scan below is the real gate.
    fn gates_presses(&self) -> bool {
        false
    }

    fn wants_tick(&self) -> bool {
        true
    }

    // Config watch: regenerate the rotated labels when padding/font/scale change.
    fn tick(&mut self, _dt: f32, _rect: crate::scene::layout::Rect) -> bool {
        let mut changed = false;
        let current_padding = crate::layout::button_padding();
        let current_font = self.current_font();
        let current_scale = crate::scale::scale_factor().max(1.0);
        if self.last_padding != Some(current_padding)
            || self.last_font.as_ref() != Some(&current_font)
            || self.last_scale != Some(current_scale)
        {
            self.generate_rotated_labels();
            changed = true;
        }
        changed
    }

    fn on_event(&mut self, event: &Event, _ectx: &mut crate::widget::EventCtx) -> bool {
        match event {
            Event::MouseButton { button, state, x: px, y: py, .. } => {
                if *button != MouseButton::Left {
                    return false;
                }
                let mut changed = false;
                match state {
                    ElementState::Pressed => {
                        for i in 0..self.buttons.len() {
                            let r = self.item_rect(i);
                            if *px >= r.0 && *px < r.0 + r.2 && *py >= r.1 && *py < r.1 + r.3 {
                                self.pressed_idx = Some(i);
                                changed = true;
                                break;
                            }
                        }
                    }
                    ElementState::Released => {
                        if let Some(pressed) = self.pressed_idx {
                            let r = self.item_rect(pressed);
                            if *px >= r.0 && *px < r.0 + r.2 && *py >= r.1 && *py < r.1 + r.3 {
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
            Event::PointerMove { x: px, y: py, .. } => {
                let old_hovered = self.hovered_idx;
                self.hovered_idx = None;
                for i in 0..self.buttons.len() {
                    let r = self.item_rect(i);
                    if *px >= r.0 && *px < r.0 + r.2 && *py >= r.1 && *py < r.1 + r.3 {
                        self.hovered_idx = Some(i);
                        break;
                    }
                }
                old_hovered != self.hovered_idx
            }
            Event::KeyInput(event) => {
                if event.state != ElementState::Pressed {
                    return false;
                }
                if self.buttons.is_empty() {
                    return false;
                }

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
            _ => false,
        }
    }
}

impl ButtonStrip {
    pub(crate) fn own_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        let font_info = self.current_font_parsed();
        let font_fam = font_info.0;
        let font_size = font_info.1;
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
                        let est_icon_w = crate::widget::display::measure_text_width(icon, &font_fam, icon_font_size);
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
                let est_w = crate::widget::display::measure_text_width(btn_label, &font_fam, font_size);
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
}
