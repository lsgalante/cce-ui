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
    pub open_upward: Option<bool>,
    pub auto_width: bool,
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
            open_upward: None,
            auto_width: false,
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

    pub fn with_open_upward(mut self, open_upward: bool) -> Self {
        self.open_upward = Some(open_upward);
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

    pub fn with_auto_width(mut self, auto_width: bool) -> Self {
        self.auto_width = auto_width;
        self
    }

    pub fn content_width(&self) -> f32 {
        let font_setting = crate::layout::control_label_font_detached();
        let (font_family, font_size_opt) = crate::layout::parse_font_string(&font_setting);
        let font_size = font_size_opt.unwrap_or(12.0);
        let mut max_w = 0.0f32;
        for opt in &self.options {
            let opt_w = crate::widget::display::measure_text_width(opt, &font_family, font_size) + 32.0;
            if opt_w > max_w {
                max_w = opt_w;
            }
        }
        max_w
    }

    pub fn popover_width(&self) -> f32 {
        self.base.w.max(self.content_width())
    }

    pub fn get_popover_geom(&self) -> (f32, f32, f32, f32) {
        let rw = self.popover_width();
        let rh = self.options.len() as f32 * 24.0;
        
        let open_upward = self.open_upward.unwrap_or_else(|| self.base.y > 400.0);
        let label_x = self.label_x_offset();
        
        let mut rx = self.base.x + label_x;
        let mut ry = if open_upward {
            let label_offset = self.base.label_offset();
            self.base.y + label_offset - rh
        } else {
            self.base.y + self.base.h
        };
        
        let mut is_ramp = false;
        if let Some(parent_ptr) = self.parent {
            is_ramp = unsafe {
                (*parent_ptr).as_any().is::<crate::widget::Ramp>()
            };
        }
        
        if is_ramp {
            if let Some(parent_ptr) = self.parent {
                let (px, py, pw_parent, ph_parent) = unsafe { (*parent_ptr).rect() };
                if pw_parent > 0.0 && ph_parent > 0.0 {
                    let label_offset = self.base.label_offset();
                    let dy_down = self.base.y + self.base.h;
                    let dy_up = self.base.y + label_offset - rh;
                    
                    if self.open_upward.is_none() {
                        if dy_down + rh > py + ph_parent && dy_up >= py {
                            ry = dy_up;
                        } else if dy_up < py && dy_down + rh <= py + ph_parent {
                            ry = dy_down;
                        }
                    }
                    
                    // Clamp X to parent borders
                    if rx < px {
                        rx = px;
                    }
                    if rx + rw > px + pw_parent {
                        rx = px + pw_parent - rw;
                    }
                    
                    // Clamp Y to parent borders
                    if ry < py {
                        ry = py;
                    }
                    if ry + rh > py + ph_parent {
                        ry = py + ph_parent - rh;
                    }
                }
            }
        }
        
        (rx, ry, rw, rh)
    }

    pub fn render_popover(&self, pc: &mut dyn crate::layout::RenderTarget) {
        if !self.open { return; }
        
        let (rx, ry, rw, rh) = self.get_popover_geom();
        
        // 1. Soft layered drop shadows
        pc.rect([0.02, 0.02, 0.05, 0.15], rx + 1.0, ry + 1.0, rw, rh);
        pc.rect([0.02, 0.02, 0.05, 0.08], rx + 3.0, ry + 3.0, rw, rh);
        pc.rect([0.02, 0.02, 0.05, 0.04], rx + 5.0, ry + 5.0, rw, rh);

        let theme = colors::active_theme();

        // 2. High-contrast premium outer border
        pc.rect(theme.surface_border, rx, ry, rw, rh);
        
        // 3. Frosted glass background
        pc.rect(theme.surface_bg, rx + 1.0, ry + 1.0, rw - 2.0, rh - 2.0); // bg
        
        if let Some(h_idx) = self.hovered_item {
            let iy = ry + h_idx as f32 * 24.0;
            // 4. Vibrantly colored translucent selection highlight
            pc.rect(theme.primary_accent, rx + 2.0, iy + 2.0, rw - 4.0, 20.0);
        }
        
        for (idx, opt) in self.options.iter().enumerate() {
            let iy = crate::layout::align_text_y(ry + idx as f32 * 24.0, 24.0, 12.0, 0.0);
            
            if opt == "-" {
                pc.rect(theme.surface_border, rx + 8.0, ry + idx as f32 * 24.0 + 11.5, rw - 16.0, 1.0);
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
            
            let bounds = Some([rx, ry, rx + rw, ry + rh]);
            if let Some(ref font) = self.widget_font() {
                pc.text_with_font_and_bounds(
                    opt,
                    rx + 8.0,
                    iy,
                    12.0,
                    color_f32,
                    font,
                    bounds,
                );
            } else {
                pc.text_with_bounds(
                    opt,
                    rx + 8.0,
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

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.base.x = x;
        self.base.y = y;
        self.base.w = w;
        self.base.h = h + self.base.label_offset();
    }

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

    fn measure(&self, constraints: LayoutConstraints, _ctx: &UiContext) -> Size {
        let (_, _, w, h) = self.rect();
        let pref_w = if self.auto_width {
            self.content_width()
        } else {
            w
        };
        let pref_h = self.preferred_height().unwrap_or(h);
        
        let width = pref_w.clamp(constraints.min_width, constraints.max_width);
        let height = pref_h.clamp(constraints.min_height, constraints.max_height);
        
        Size { width, height }
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
            
            let mut bg_color = colors::dropdown_background_color();
            bg_color[3] = 1.0; // Force opaque background to prevent subpixel blending artifacts
            let border_color = if self.open {
                [0.30, 0.50, 0.32, 1.0]
            } else if self.base.hovered {
                let bc = colors::dropdown_border_color();
                [(bc[0] + 0.15).min(1.0), (bc[1] + 0.15).min(1.0), (bc[2] + 0.15).min(1.0), bc[3]]
            } else {
                colors::dropdown_border_color()
            };

            let label_x = self.label_x_offset();
            let x = self.base.x + label_x;
            let w = self.base.w - label_x;

            // Draw border and background (with potential parent-concentric corner adjustment)
            let inner_radius = (radius - 1.0).max(0.0);
            let mut adjusted = false;
            let mut outer_radii = [radius; 4];
            let mut inner_radii = [inner_radius; 4];

            let mut curr = self.parent(ctx);
            let mut backplate_ptr = None;
            while let Some(ptr) = curr {
                if unsafe { (*ptr).is_backplate() } {
                    backplate_ptr = Some(ptr);
                    break;
                }
                curr = unsafe { (*ptr).parent(ctx) };
            }

            if let Some(bp) = backplate_ptr {
                let (px, py, pw, ph) = unsafe { (*bp).rect() };
                let pr = unsafe { (*bp).corner_radius() };
                let (pr1, pr2, pr3, pr4) = unsafe { (*bp).rounded_corners() };

                let g_left = x - px;
                let g_top = (self.base.y + top) - py;
                let g_right = (px + pw) - (x + w);
                let g_bottom = (py + ph) - ((self.base.y + top) + visual_h);

                if pr1 && (g_left - g_top).abs() < 1.0 && g_left >= 0.0 {
                    outer_radii[0] = (pr - g_left).max(0.0);
                    inner_radii[0] = (outer_radii[0] - 1.0).max(0.0);
                    adjusted = true;
                }
                if pr2 && (g_right - g_top).abs() < 1.0 && g_right >= 0.0 {
                    outer_radii[1] = (pr - g_right).max(0.0);
                    inner_radii[1] = (outer_radii[1] - 1.0).max(0.0);
                    adjusted = true;
                }
                if pr3 && (g_right - g_bottom).abs() < 1.0 && g_right >= 0.0 {
                    outer_radii[2] = (pr - g_right).max(0.0);
                    inner_radii[2] = (outer_radii[2] - 1.0).max(0.0);
                    adjusted = true;
                }
                if pr4 && (g_left - g_bottom).abs() < 1.0 && g_left >= 0.0 {
                    outer_radii[3] = (pr - g_left).max(0.0);
                    inner_radii[3] = (outer_radii[3] - 1.0).max(0.0);
                    adjusted = true;
                }
            }

            if adjusted {
                let border_quads = crate::layout::partition_concentric_corners(
                    x, self.base.y + top, w, visual_h,
                    radius, outer_radii, border_color
                );
                quads.extend(border_quads);

                let bg_quads = crate::layout::partition_concentric_corners(
                    x + 1.0, self.base.y + top + 1.0, w - 2.0, visual_h - 2.0,
                    inner_radius, inner_radii, bg_color
                );
                quads.extend(bg_quads);
            } else {
                quads.push((x, self.base.y + top, w, visual_h, radius, border_color, (r1, r2, r3, r4)));
                quads.push((x + 1.0, self.base.y + top + 1.0, w - 2.0, visual_h - 2.0, inner_radius, bg_color, (r1, r2, r3, r4)));
            }
        }
        for &child_ptr in &self.children(ctx) {
            let widget = unsafe { &*child_ptr };
            quads.extend(widget.all_rounded_quads(ctx));
        }
        quads
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::control_label_font_detached())
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
            let (rx, ry, rw, rh) = self.get_popover_geom();
            let hit_trigger = px >= hx && px <= hx + hw && py >= y && py <= y + h;
            let hit_popover = px >= rx && px <= rx + rw && py >= ry && py <= ry + rh;
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
            let (rx, ry, rw, rh) = self.get_popover_geom();
            if px >= rx && px <= rx + rw && py >= ry && py <= ry + rh {
                let idx = ((py - ry) / 24.0) as usize;
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
        let (rx, ry, rw, rh) = self.get_popover_geom();

        let inside_trigger = px >= x && px <= x + w && py >= y && py <= y + h;
        let inside_popover = self.open && px >= rx && px <= rx + rw && py >= ry && py <= ry + rh;

        if inside_popover {
            let idx = ((py - ry) / 24.0) as usize;
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

        let mut bg_color = colors::dropdown_background_color();
        bg_color[3] = 1.0; // Force opaque background to prevent subpixel blending artifacts
        let border_color = if self.open {
            [0.30, 0.50, 0.32, 1.0]
        } else if self.base.hovered {
            let bc = colors::dropdown_border_color();
            [(bc[0] + 0.15).min(1.0), (bc[1] + 0.15).min(1.0), (bc[2] + 0.15).min(1.0), bc[3]]
        } else {
            colors::dropdown_border_color()
        };

        let label_x = self.label_x_offset();
        let x = self.base.x + label_x;
        let w = self.base.w - label_x;

        quads.push((x, self.base.y + top, w, visual_h, border_color));
        quads.push((x + 1.0, self.base.y + top + 1.0, w - 2.0, visual_h - 2.0, bg_color));

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

        let (font_family, font_size) = crate::layout::control_label_font_detached_parsed();
        let label_x = self.label_x_offset();
        let x = self.base.x + label_x;
        let w = self.base.w - label_x;
        let start_x = x + 8.0;
        let right_limit = x + w - 28.0; // 10px margin before the arrow
        let fade_start_x = (right_limit - 24.0).max(start_x); // Fade out over the last 24px
        let text_y = crate::layout::center_text_y(self.base.y + top, self.base.h - top, font_size);
        let tc = colors::dropdown_text_color();
        let default_color = [
            (colors::linear_to_srgb(tc[0]) * 255.0).round() as u8,
            (colors::linear_to_srgb(tc[1]) * 255.0).round() as u8,
            (colors::linear_to_srgb(tc[2]) * 255.0).round() as u8,
        ];
        let bg_color = colors::dropdown_background_color();
        let mut parent_color = colors::page_color();
        if let Some(parent_ptr) = self.parent {
            unsafe {
                parent_color = (*parent_ptr).color();
            }
        }
        let alpha = 1.0; // The dropdown background is drawn fully opaque
        let bg_rgb = [
            ((parent_color[0] * (1.0 - alpha) + bg_color[0] * alpha) * 255.0).round().clamp(0.0, 255.0) as u8,
            ((parent_color[1] * (1.0 - alpha) + bg_color[1] * alpha) * 255.0).round().clamp(0.0, 255.0) as u8,
            ((parent_color[2] * (1.0 - alpha) + bg_color[2] * alpha) * 255.0).round().clamp(0.0, 255.0) as u8,
        ];

        let w_dummy = crate::widget::display::measure_text_width("M", &font_family, font_size);
        let chars: Vec<char> = selected_text.chars().collect();
        let n = chars.len();

        let is_monospace = {
            let w_i10 = crate::widget::display::measure_text_width("iiiiiiiiii", &font_family, font_size);
            let w_m10 = crate::widget::display::measure_text_width("mmmmmmmmmm", &font_family, font_size);
            (w_i10 - w_m10).abs() < 5.0
        };

        let cell_width = if is_monospace {
            let w_m10 = crate::widget::display::measure_text_width("mmmmmmmmmm", &font_family, font_size);
            let w_m20 = crate::widget::display::measure_text_width("mmmmmmmmmmmmmmmmmmmm", &font_family, font_size);
            ((w_m20 - w_m10) / 10.0).max(1.0)
        } else {
            0.0
        };

        let mut char_offsets = Vec::with_capacity(n);
        if is_monospace {
            for i in 0..n {
                char_offsets.push(i as f32 * cell_width);
            }
        } else {
            if n > 0 {
                char_offsets.push(0.0f32);
            }
            let mut prefix = String::new();
            for i in 1..n {
                prefix.push(chars[i - 1]);
                let measure_str = format!("{}M", prefix);
                let w_prefix_dummy = crate::widget::display::measure_text_width(&measure_str, &font_family, font_size);
                let offset = (w_prefix_dummy - w_dummy).max(0.0);
                char_offsets.push(offset);
            }
        }

        let total_advance = if n > 0 {
            if is_monospace {
                n as f32 * cell_width
            } else {
                let measure_str = format!("{}M", selected_text);
                (crate::widget::display::measure_text_width(&measure_str, &font_family, font_size) - w_dummy).max(0.0)
            }
        } else {
            0.0
        };

        // Draw and fade every character individually
        let mut prev_char_end = 0.0;
        for i in 0..n {
            let mut offset = char_offsets[i];
            if !is_monospace {
                if i > 0 {
                    offset = offset.max(prev_char_end + 1.0);
                }
            }
            let next_offset = if i < n - 1 { char_offsets[i + 1] } else { total_advance };
            let c_w = if is_monospace { cell_width } else { next_offset - offset };
            let cur_x = start_x + offset;

            if cur_x >= right_limit {
                break;
            }

            let char_mid_x = cur_x + c_w / 2.0;
            let mut skip_char = false;
            let color = if char_mid_x > fade_start_x {
                let factor = ((char_mid_x - fade_start_x) / (right_limit - fade_start_x)).clamp(0.0, 1.0);
                if factor >= 0.9 {
                    skip_char = true;
                    default_color
                } else {
                    [
                        (default_color[0] as f32 + (bg_rgb[0] as f32 - default_color[0] as f32) * factor).round() as u8,
                        (default_color[1] as f32 + (bg_rgb[1] as f32 - default_color[1] as f32) * factor).round() as u8,
                        (default_color[2] as f32 + (bg_rgb[2] as f32 - default_color[2] as f32) * factor).round() as u8,
                    ]
                }
            } else {
                default_color
            };

            if !skip_char {
                labels.push(TextLabel {
                    text: chars[i].to_string(),
                    x: cur_x,
                    y: text_y,
                    font_size,
                    color,
                });
                let c_w_ink = if is_monospace {
                    cell_width
                } else {
                    crate::widget::display::measure_text_width(&chars[i].to_string(), &font_family, font_size)
                };
                prev_char_end = offset + c_w_ink;
            }
        }

        let label_x = self.label_x_offset();
        let x = self.base.x + label_x;
        let w = self.base.w - label_x;

        labels.push(TextLabel {
            text: "▼".to_string(),
            x: x + w - 18.0,
            y: crate::layout::center_text_y(self.base.y + top, self.base.h - top, 10.0),
            font_size: 10.0,
            color: [0x83, 0x83, 0x8a],
        });

        labels
    }

    fn value(&self) -> i32 { self.selected as i32 }
    fn take_click(&mut self) -> bool { self.take_change() }
    fn popover_rect(&self) -> Option<(f32, f32, f32, f32)> {
        if self.open {
            Some(self.get_popover_geom())
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

    #[test]
    fn test_dropdown_ramp_parent_constraints() {
        let mut ramp = crate::widget::Ramp::new();
        // Set the rect of parent Ramp
        ramp.set_rect(20.0, 20.0, 410.0, 260.0);
        
        let options = vec![
            "Option 1".to_string(),
            "Option 2".to_string(),
            "Option 3".to_string(),
            "Option 4".to_string(),
            "Option 5".to_string(),
            "Option 6".to_string(),
        ];
        let mut dd = Dropdown::new(options, 0).with_label("Preset");
        dd.set_rect(30.0, 125.0, 110.0, 20.0);
        
        // Link the dropdown parent pointer to the Ramp
        dd.parent = Some(&mut ramp as *mut crate::widget::Ramp as *mut (dyn crate::widget::Element + 'static));
        
        // Compute geometry
        let (rx, ry, rw, rh) = dd.get_popover_geom();
        
        // Validate coordinates stay inside the parent Ramp bounds: x in [20, 430], y in [20, 280]
        assert!(rx >= 20.0, "rx {} should be >= 20.0", rx);
        assert!(rx + rw <= 430.0, "rx + rw {} should be <= 430.0", rx + rw);
        assert!(ry >= 20.0, "ry {} should be >= 20.0", ry);
        assert!(ry + rh <= 280.0, "ry + rh {} should be <= 280.0", ry + rh);
    }

    #[test]
    fn test_dropdown_label_fade_out() {
        let options = vec!["This is a very long option name that will exceed the dropdown width".to_string()];
        let mut dd = Dropdown::new(options, 0);
        dd.set_rect(10.0, 10.0, 100.0, 24.0); // very narrow dropdown

        let labels = dd.text_labels();
        // Option 0: control_label (none)
        // Option 1: ▼ (arrow) at the end of labels
        // Remaining labels are individual characters of selected_text
        assert!(labels.len() > 2);
        
        // The last character label (excluding the arrow) should be faded (i.e. not the default color)
        let last_char_idx = labels.len() - 2;
        let first_char = &labels[0];
        let last_char = &labels[last_char_idx];
        
        let tc = colors::dropdown_text_color();
        let expected_color = [
            (colors::linear_to_srgb(tc[0]) * 255.0).round() as u8,
            (colors::linear_to_srgb(tc[1]) * 255.0).round() as u8,
            (colors::linear_to_srgb(tc[2]) * 255.0).round() as u8,
        ];
        assert_eq!(first_char.color, expected_color);
        assert_ne!(last_char.color, expected_color); // color has shifted towards background
    }

    #[test]
    fn test_dropdown_auto_width() {
        let dummy = crate::context::UiContext::new();
        let options = vec!["Short".to_string(), "A much longer option name".to_string()];
        let mut dd = Dropdown::new(options, 0).with_auto_width(true);
        dd.set_rect(10.0, 10.0, 50.0, 24.0);

        let size = dd.measure(LayoutConstraints::new(0.0, 500.0, 24.0, 24.0), &dummy);
        assert!(size.width > 50.0, "Measured auto-width {} should be greater than original width 50.0", size.width);
        
        let dd_no_auto = Dropdown::new(vec!["Short".to_string(), "A much longer option name".to_string()], 0);
        let size_no_auto = dd_no_auto.measure(LayoutConstraints::new(0.0, 500.0, 24.0, 24.0), &dummy);
        assert_eq!(size_no_auto.width, 0.0);
    }
}

