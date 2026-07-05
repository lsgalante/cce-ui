use crate::colors;
use crate::widget::*;
use crate::widget::container::scroll_box::ScrollBox;

pub struct ControlPanel {
    pub base: Widget,
    pub children: Vec<*mut (dyn Element + 'static)>,
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub scroll_box: ScrollBox,
}

impl ControlPanel {
    pub fn new() -> Self {
        let mut sb = ScrollBox::new();
        sb.show_border = false;
        sb.show_background = false;
        Self {
            base: Widget::new(),
            children: Vec::new(),
            parent: None,
            scroll_box: sb,
        }
    }

    pub fn add_child(&mut self, child: *mut (dyn Element + 'static)) {
        self.children.push(child);
        self.scroll_box.children.push(child);
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

        // Check if cursor is inside child popovers first (scrolled coordinates)
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
        let mut changed = self.scroll_box.tick(dt, ctx);
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

        self.scroll_box.set_rect(x, y, w, h);

        let self_ptr = self.as_ptr_mut();
        let mut dummy = crate::context::UiContext::new();
        let self_ptr_option = Some(self_ptr);

        let padding = 16.0;
        let mut curr_y = padding;
        let content_w = w - 2.0 * padding - 12.0; // 12px reserved for scrollbar track

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
                        if child.base().is_some() && child.base().unwrap().label.is_none() {
                            slider = Some(child_ptr);
                        }
                    }
                }
            }

            // --- SECTION 1: Window Actions ---
            if let (Some(c), Some(t)) = (create_btn, tile_btn) {
                let btn_w = (content_w - 12.0) / 2.0;
                (*c).set_rect(x + padding, y + curr_y, btn_w, 28.0);
                (*t).set_rect(x + padding + btn_w + 12.0, y + curr_y, btn_w, 28.0);
                curr_y += 28.0 + 24.0;
            }

            // --- SECTION 2: Dimensions ---
            if let (Some(w_sp), Some(h_sp)) = (width_spin, height_spin) {
                let spin_w = (content_w - 12.0) / 2.0;
                (*w_sp).set_rect(x + padding, y + curr_y, spin_w, 20.0);
                (*h_sp).set_rect(x + padding + spin_w + 12.0, y + curr_y, spin_w, 20.0);
                curr_y += 20.0 + 24.0;
            }

            // --- SECTION 3: Window Type & Shape ---
            let mut has_dd = false;
            if let Some(t_dd) = type_dd {
                (*t_dd).set_rect(x + padding, y + curr_y, content_w, 20.0);
                curr_y += 20.0 + 16.0;
                has_dd = true;
            }
            if let Some(s_dd) = shape_dd {
                (*s_dd).set_rect(x + padding, y + curr_y, content_w, 20.0);
                curr_y += 20.0 + 24.0;
                has_dd = true;
            }
            if !has_dd {
                // Keep spacing consistent
            }

            // --- SECTION 4: Opacity & Transparency ---
            if let Some(op_t) = opacity_toggle {
                (*op_t).set_rect(x + padding, y + curr_y, content_w, 20.0);
                curr_y += 20.0 + 16.0;
            }
            if let Some(sl_lbl) = slider_label {
                (*sl_lbl).set_rect(x + padding, y + curr_y, content_w, 12.0);
                curr_y += 12.0 + 4.0;
            }
            if let Some(sl) = slider {
                (*sl).set_rect(x + padding, y + curr_y, content_w, 20.0);
                curr_y += 20.0 + 24.0;
            }

            // --- SECTION 5: Window Elements ---
            if let Some(w_s) = win_sec {
                (*w_s).set_rect(x + padding, y + curr_y, content_w, 20.0);
                curr_y += 20.0 + 16.0;
            }
            let toggles = [backplate_toggle, menubar_toggle, statusbar_toggle];
            let active_toggles: Vec<*mut dyn Element> = toggles.iter().filter_map(|&t| t).collect();
            if !active_toggles.is_empty() {
                let t_w = (content_w - (active_toggles.len() as f32 - 1.0) * 10.0) / active_toggles.len() as f32;
                for (idx, &t) in active_toggles.iter().enumerate() {
                    (*t).set_rect(x + padding + idx as f32 * (t_w + 10.0), y + curr_y, t_w, 20.0);
                }
                curr_y += 20.0 + 24.0;
            }

            // --- SECTION 6: Border & Bevel ---
            if let Some(b_s) = border_sec {
                (*b_s).set_rect(x + padding, y + curr_y, content_w, 20.0);
                curr_y += 20.0 + 16.0;
            }
            if let (Some(b_tg), Some(bev_tg)) = (border_toggle, bevel_toggle) {
                let tg_w = (content_w - 12.0) / 2.0;
                (*b_tg).set_rect(x + padding, y + curr_y, tg_w, 20.0);
                (*bev_tg).set_rect(x + padding + tg_w + 12.0, y + curr_y, tg_w, 20.0);
                curr_y += 20.0 + 16.0;
            }
            if let Some(bw_sp) = border_width_spin {
                (*bw_sp).set_rect(x + padding, y + curr_y, content_w, 20.0);
                curr_y += 20.0 + 16.0;
            }
            if let Some(bs_btn) = bevel_shape_btn {
                (*bs_btn).set_rect(x + padding, y + curr_y, content_w, 28.0);
                curr_y += 28.0;
            }

            curr_y += padding;
            self.scroll_box.update_bounds(curr_y, y, h);
        }
    }

    fn all_rounded_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, f32, [f32; 4], (bool, bool, bool, bool))> {
        let mut quads = Vec::new();
        let (x, y, w, h) = self.rect();

        // 1. Background
        quads.push((x, y, w, h, 0.0, self.color(), (false, false, false, false)));

        // 2. Borders
        let border_color = colors::ramp_border_color();
        quads.push((x, y, w, 1.0, 0.0, border_color, (false, false, false, false)));
        quads.push((x, y + h - 1.0, w, 1.0, 0.0, border_color, (false, false, false, false)));
        quads.push((x, y, 1.0, h, 0.0, border_color, (false, false, false, false)));
        quads.push((x + w - 1.0, y, 1.0, h, 0.0, border_color, (false, false, false, false)));

        // 3. Child elements clipped to viewport bounds
        let scroll_y = self.scroll_box.scroll_y;
        let y_start = y;
        let y_end = y + h;

        unsafe {
            for child_ptr in &self.children {
                for (qx, qy, qw, qh, qr, qc, qcorners) in (**child_ptr).all_rounded_quads(ctx) {
                    let qy_shifted = qy - scroll_y;
                    let qy_top = qy_shifted;
                    let qy_bottom = qy_shifted + qh;
                    if qy_bottom > y_start && qy_top < y_end {
                        let visible_top = qy_top.max(y_start);
                        let visible_bottom = qy_bottom.min(y_end);
                        let visible_h = visible_bottom - visible_top;
                        if visible_h > 0.0 {
                            let radii_adjusted = if visible_top > qy_top || visible_bottom < qy_bottom {
                                0.0
                            } else {
                                qr
                            };
                            quads.push((qx, visible_top, qw, visible_h, radii_adjusted, qc, qcorners));
                        }
                    }
                }
            }
        }

        // 4. Scrollbar
        for (sx, sy, sw, sh, sc) in self.scroll_box.extra_quads() {
            quads.push((sx, sy, sw, sh, 0.0, sc, (false, false, false, false)));
        }

        quads
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        let (x, y, w, h) = self.rect();

        quads.push((x, y, w, h, self.color()));

        let border_color = colors::ramp_border_color();
        quads.push((x, y, w, 1.0, border_color));
        quads.push((x, y + h - 1.0, w, 1.0, border_color));
        quads.push((x, y, 1.0, h, border_color));
        quads.push((x + w - 1.0, y, 1.0, h, border_color));

        let scroll_y = self.scroll_box.scroll_y;
        let y_start = y;
        let y_end = y + h;
        let ctx_dummy = crate::context::UiContext::new();

        unsafe {
            for child_ptr in &self.children {
                for (qx, qy, qw, qh, qc) in (**child_ptr).all_quads(&ctx_dummy) {
                    let qy_shifted = qy - scroll_y;
                    let qy_top = qy_shifted;
                    let qy_bottom = qy_shifted + qh;
                    if qy_bottom > y_start && qy_top < y_end {
                        let visible_top = qy_top.max(y_start);
                        let visible_bottom = qy_bottom.min(y_end);
                        let visible_h = visible_bottom - visible_top;
                        if visible_h > 0.0 {
                            quads.push((qx, visible_top, qw, visible_h, qc));
                        }
                    }
                }
            }
        }

        quads.extend(self.scroll_box.extra_quads());
        quads
    }

    fn popover_rect(&self) -> Option<(f32, f32, f32, f32)> {
        let scroll_y = self.scroll_box.scroll_y;
        unsafe {
            for child_ptr in &self.children {
                let child = &mut **child_ptr;
                let old_y = child.base().map(|b| b.y).unwrap_or(0.0);
                if let Some(b) = child.base_mut() {
                    b.y = old_y - scroll_y;
                }
                let res = child.popover_rect();
                if let Some(b) = child.base_mut() {
                    b.y = old_y;
                }
                if res.is_some() {
                    return res;
                }
            }
        }
        None
    }

    fn render_popover(&self, pc: &mut dyn crate::layout::RenderTarget) {
        let scroll_y = self.scroll_box.scroll_y;
        unsafe {
            for child_ptr in &self.children {
                if (**child_ptr).popover_rect().is_some() {
                    let child = &mut **child_ptr;
                    let old_y = child.base().map(|b| b.y).unwrap_or(0.0);
                    if let Some(b) = child.base_mut() {
                        b.y = old_y - scroll_y;
                    }
                    child.render_popover(pc);
                    if let Some(b) = child.base_mut() {
                        b.y = old_y;
                    }
                }
            }
        }
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if self.scroll_box.mouse_input(button, state, px, py, ctx) {
            return true;
        }

        let scroll_y = self.scroll_box.scroll_y;
        let py_translated = py + scroll_y;
        let (_x, y, _w, h) = self.rect();

        unsafe {
            for child_ptr in &self.children {
                // Only dispatch if the click is physically inside the ControlPanel viewport
                if py >= y && py <= y + h {
                    if (**child_ptr).mouse_input(button, state, px, py_translated, ctx) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        let mut changed = self.scroll_box.cursor_moved(px, py, ctx);

        let scroll_y = self.scroll_box.scroll_y;
        let py_translated = py + scroll_y;
        unsafe {
            for child_ptr in &self.children {
                if (**child_ptr).cursor_moved(px, py_translated, ctx) {
                    changed = true;
                }
            }
        }
        changed
    }

    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if self.scroll_box.mouse_wheel(delta, px, py, ctx) {
            return true;
        }

        let scroll_y = self.scroll_box.scroll_y;
        let py_translated = py + scroll_y;
        unsafe {
            for child_ptr in &self.children {
                if (**child_ptr).mouse_wheel(delta, px, py_translated, ctx) {
                    return true;
                }
            }
        }
        false
    }

    fn text_labels_with_font_and_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        let mut labels = Vec::new();
        let scroll_y = self.scroll_box.scroll_y;
        let (x, y, w, h) = self.rect();
        unsafe {
            for child_ptr in &self.children {
                let mut child_labels = (**child_ptr).text_labels_with_font_and_bounds(ctx);
                for (label, _font, bounds) in &mut child_labels {
                    label.y -= scroll_y;
                    
                    let new_bounds = if let Some([l, t, r, b]) = *bounds {
                        let nl = l.max(x);
                        let nt = (t - scroll_y).max(y);
                        let nr = r.min(x + w);
                        let nb = (b - scroll_y).min(y + h);
                        Some([nl, nt, nr, nb])
                    } else {
                        Some([x, y, x + w, y + h])
                    };
                    *bounds = new_bounds;
                }
                labels.extend(child_labels);
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
