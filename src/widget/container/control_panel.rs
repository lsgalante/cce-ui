use crate::colors;
use crate::widget::*;

pub struct ControlPanel {
    pub base: Widget,
    pub children: Vec<*mut (dyn Element + 'static)>,
    pub parent: Option<*mut (dyn Element + 'static)>,
}

impl ControlPanel {
    pub fn new() -> Self {
        Self {
            base: Widget::new(),
            children: Vec::new(),
            parent: None,
        }
    }

    pub fn add_child(&mut self, child: *mut (dyn Element + 'static)) {
        self.children.push(child);
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self
    }
}

impl Element for ControlPanel {
    crate::impl_widget_base!(ControlPanel);

    fn hit_test(&self, px: f32, py: f32, ctx: &UiContext) -> bool {
        if ctx.is_coordinate_covered(self as *const Self as *const () as usize, px, py) {
            return false;
        }

        // Check if cursor is inside child popovers first
        if let Some(pop_rect) = self.popover_rect() {
            if px >= pop_rect.0 && px <= pop_rect.0 + pop_rect.2 && py >= pop_rect.1 && py <= pop_rect.1 + pop_rect.3 {
                return true;
            }
        }

        // Otherwise check the panel itself
        let (x, y, w, h) = self.rect();
        if w <= 0.0 || h <= 0.0 {
            return false;
        }
        px >= x && px <= x + w && py >= y && py <= y + h
    }

    fn tick(&mut self, dt: f32, ctx: &mut UiContext) -> bool {
        let mut changed = false;
        unsafe {
            for child_ptr in &self.children {
                if (**child_ptr).tick(dt, ctx) {
                    changed = true;
                }
            }
        }
        changed
    }

    fn parent(&self, _ctx: &UiContext) -> Option<*mut (dyn Element + 'static)> { self.parent }
    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>, _ctx: &mut UiContext) {
        self.parent = parent;
    }

    fn children(&self, _ctx: &UiContext) -> Vec<*mut (dyn Element + 'static)> {
        self.children.clone()
    }

    fn color(&self) -> [f32; 4] {
        colors::page_low_color()
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.base.x = x;
        self.base.y = y;
        self.base.w = w;
        self.base.h = h;

        let self_ptr = self.as_ptr_mut();
        let mut dummy = crate::context::UiContext::new();
        let self_ptr_option = Some(self_ptr);

        let padding = 12.0;
        let mut curr_y = y + padding;
        let content_w = w - 2.0 * padding;

        unsafe {
            let mut create_btn: Option<*mut dyn Element> = None;
            let mut tile_btn: Option<*mut dyn Element> = None;
            let mut opacity_toggle: Option<*mut dyn Element> = None;
            let mut slider: Option<*mut dyn Element> = None;
            let mut slider_label: Option<*mut dyn Element> = None;
            let mut type_dd: Option<*mut dyn Element> = None;
            let mut shape_dd: Option<*mut dyn Element> = None;
            let mut border_toggle: Option<*mut dyn Element> = None;
            let mut width_spin: Option<*mut dyn Element> = None;
            let mut height_spin: Option<*mut dyn Element> = None;
            let mut backplate_toggle: Option<*mut dyn Element> = None;
            let mut menubar_toggle: Option<*mut dyn Element> = None;
            let mut statusbar_toggle: Option<*mut dyn Element> = None;
            let mut border_sec: Option<*mut dyn Element> = None;
            let mut bevel_toggle: Option<*mut dyn Element> = None;
            let mut border_width_spin: Option<*mut dyn Element> = None;
            let mut win_sec: Option<*mut dyn Element> = None;
            let mut bevel_shape_btn: Option<*mut dyn Element> = None;

            for &child_ptr in &self.children {
                let child = &mut *child_ptr;
                child.set_parent(self_ptr_option, &mut dummy);
                let label = child.base().and_then(|b| b.label.as_ref()).map(|s| s.as_str()).unwrap_or("");
                match label {
                    "Create Window" => create_btn = Some(child_ptr),
                    "Tile Windows" => tile_btn = Some(child_ptr),
                    "Opacity" => opacity_toggle = Some(child_ptr),
                    "Transparency Level" => slider_label = Some(child_ptr),
                    "Border" => border_sec = Some(child_ptr),
                    "Window Elements" => win_sec = Some(child_ptr),
                    "Window Type" => type_dd = Some(child_ptr),
                    "Window Shape" => shape_dd = Some(child_ptr),
                    "Enable" => border_toggle = Some(child_ptr),
                    "Width" => width_spin = Some(child_ptr),
                    "Height" => height_spin = Some(child_ptr),
                    "Backplate" => backplate_toggle = Some(child_ptr),
                    "MenuBar" => menubar_toggle = Some(child_ptr),
                    "StatusBar" => statusbar_toggle = Some(child_ptr),
                    "Bevel" => bevel_toggle = Some(child_ptr),
                    "Border Width" => border_width_spin = Some(child_ptr),
                    "Bevel Shape..." => bevel_shape_btn = Some(child_ptr),
                    _ => {
                        // Slider has no label string in widget base, let's identify it by type
                        if child.base().is_some() && child.base().unwrap().label.is_none() {
                            slider = Some(child_ptr);
                        }
                    }
                }
            }

            // --- SECTION 1: Window Actions ---
            if let (Some(c), Some(t)) = (create_btn, tile_btn) {
                let btn_w = (content_w - 10.0) / 2.0;
                (*c).set_rect(x + padding, curr_y, btn_w, 28.0);
                (*t).set_rect(x + padding + btn_w + 10.0, curr_y, btn_w, 28.0);
                curr_y += 28.0 + 10.0;
            }

            // --- SECTION 2: Dimensions ---
            if let (Some(w_sp), Some(h_sp)) = (width_spin, height_spin) {
                let spin_w = (content_w - 10.0) / 2.0;
                (*w_sp).set_rect(x + padding, curr_y, spin_w, 20.0);
                (*h_sp).set_rect(x + padding + spin_w + 10.0, curr_y, spin_w, 20.0);
                curr_y += 20.0 + 12.0;
            }

            // --- SECTION 3: Dropdowns ---
            if let Some(t_dd) = type_dd {
                (*t_dd).set_rect(x + padding, curr_y, content_w, 20.0);
                curr_y += 20.0 + 12.0;
            }
            if let Some(s_dd) = shape_dd {
                (*s_dd).set_rect(x + padding, curr_y, content_w, 20.0);
                curr_y += 20.0 + 12.0;
            }

            // --- SECTION 4: Opacity & Transparency ---
            if let Some(op_t) = opacity_toggle {
                (*op_t).set_rect(x + padding, curr_y, content_w, 20.0);
                curr_y += 20.0 + 10.0;
            }
            if let Some(sl_lbl) = slider_label {
                (*sl_lbl).set_rect(x + padding, curr_y, content_w, 12.0);
                curr_y += 12.0 + 2.0;
            }
            if let Some(sl) = slider {
                (*sl).set_rect(x + padding, curr_y, content_w, 20.0);
                curr_y += 20.0 + 12.0;
            }

            // --- SECTION 5: Window Elements ---
            if let Some(w_s) = win_sec {
                (*w_s).set_rect(x + padding, curr_y, content_w, 20.0);
                curr_y += 20.0 + 10.0;
            }
            let toggles = [backplate_toggle, menubar_toggle, statusbar_toggle];
            let active_toggles: Vec<*mut dyn Element> = toggles.iter().filter_map(|&t| t).collect();
            if !active_toggles.is_empty() {
                let t_w = (content_w - (active_toggles.len() as f32 - 1.0) * 8.0) / active_toggles.len() as f32;
                for (idx, &t) in active_toggles.iter().enumerate() {
                    (*t).set_rect(x + padding + idx as f32 * (t_w + 8.0), curr_y, t_w, 20.0);
                }
                curr_y += 20.0 + 12.0;
            }

            // --- SECTION 6: Border & Bevel ---
            if let Some(b_s) = border_sec {
                (*b_s).set_rect(x + padding, curr_y, content_w, 20.0);
                curr_y += 20.0 + 10.0;
            }
            if let (Some(b_tg), Some(bev_tg)) = (border_toggle, bevel_toggle) {
                let tg_w = (content_w - 10.0) / 2.0;
                (*b_tg).set_rect(x + padding, curr_y, tg_w, 20.0);
                (*bev_tg).set_rect(x + padding + tg_w + 10.0, curr_y, tg_w, 20.0);
                curr_y += 20.0 + 10.0;
            }
            if let Some(bw_sp) = border_width_spin {
                (*bw_sp).set_rect(x + padding, curr_y, content_w, 20.0);
                curr_y += 20.0 + 10.0;
            }
            if let Some(bs_btn) = bevel_shape_btn {
                (*bs_btn).set_rect(x + padding, curr_y, content_w, 28.0);
            }
        }
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        let (x, y, w, h) = self.rect();

        // 1. Draw main container background
        quads.push((x, y, w, h, self.color()));

        // 2. Draw border
        let border_color = colors::ramp_border_color();
        quads.push((x, y, w, 1.0, border_color));             // Top
        quads.push((x, y + h - 1.0, w, 1.0, border_color));     // Bottom
        quads.push((x, y, 1.0, h, border_color));             // Left
        quads.push((x + w - 1.0, y, 1.0, h, border_color));     // Right

        quads
    }

    fn popover_rect(&self) -> Option<(f32, f32, f32, f32)> {
        unsafe {
            for child_ptr in &self.children {
                if let Some(r) = (**child_ptr).popover_rect() {
                    return Some(r);
                }
            }
        }
        None
    }

    fn render_popover(&self, pc: &mut dyn crate::layout::RenderTarget) {
        unsafe {
            for child_ptr in &self.children {
                (**child_ptr).render_popover(pc);
            }
        }
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        unsafe {
            for child_ptr in &self.children {
                if (**child_ptr).mouse_input(button, state, px, py, ctx) {
                    return true;
                }
            }
        }
        false
    }

    fn cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        let mut changed = false;
        unsafe {
            for child_ptr in &self.children {
                if (**child_ptr).cursor_moved(px, py, ctx) {
                    changed = true;
                }
            }
        }
        changed
    }

    fn text_labels_with_font_and_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        let mut labels = Vec::new();
        unsafe {
            for child_ptr in &self.children {
                labels.extend((**child_ptr).text_labels_with_font_and_bounds(ctx));
            }
        }
        labels
    }

    fn get_text_items(&self) -> Vec<(&glyphon::Buffer, f32, f32, glyphon::Color)> {
        let mut items = Vec::new();
        unsafe {
            for child_ptr in &self.children {
                items.extend((**child_ptr).get_text_items());
            }
        }
        items
    }
}

impl Drop for ControlPanel {
    fn drop(&mut self) {
        clear_widget_references(self);
    }
}
