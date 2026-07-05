use crate::colors;
use crate::widget::*;
use crate::widget::container::scroll_box::ScrollBox;

pub struct ControlPanel {
    pub base: Widget,
    pub children: Vec<*mut (dyn Element + 'static)>,
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub scroll_box: ScrollBox,
    pub active_drag_widget: Option<*mut (dyn Element + 'static)>,
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
            active_drag_widget: None,
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
        let gap = crate::layout::column_gap();
        let mut col = ColumnLayout::new(x, y, w - 12.0, gap, padding); // 12px reserved for scrollbar track

        unsafe {
            let mut create_btn: Option<*mut dyn Element> = None;
            let mut tile_btn: Option<*mut dyn Element> = None;
            let mut opacity_toggle: Option<*mut dyn Element> = None;
            let mut enable_toggle: Option<*mut dyn Element> = None;
            let mut slider: Option<*mut dyn Element> = None;
            let mut slider_label: Option<*mut dyn Element> = None;
            let mut type_dd: Option<*mut dyn Element> = None;
            let mut shape_dd: Option<*mut dyn Element> = None;
            let mut border_style_dd: Option<*mut dyn Element> = None;
            let mut width_spin: Option<*mut dyn Element> = None;
            let mut height_spin: Option<*mut dyn Element> = None;
            let mut backplate_toggle: Option<*mut dyn Element> = None;
            let mut menubar_toggle: Option<*mut dyn Element> = None;
            let mut statusbar_toggle: Option<*mut dyn Element> = None;
            let mut border_sec: Option<*mut dyn Element> = None;
            let mut bevel_toggle: Option<*mut dyn Element> = None;
            let mut border_width_spin: Option<*mut dyn Element> = None;
            let mut bevel_depth_spin: Option<*mut dyn Element> = None;
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
                    "Enable" => enable_toggle = Some(child_ptr),
                    "Transparency Level" => slider_label = Some(child_ptr),
                    "Border" => border_sec = Some(child_ptr),
                    "Window Elements" => win_sec = Some(child_ptr),
                    "Window Type" => type_dd = Some(child_ptr),
                    "Window Shape" => shape_dd = Some(child_ptr),
                    "Border Style" => border_style_dd = Some(child_ptr),
                    "Width" => width_spin = Some(child_ptr),
                    "Height" => height_spin = Some(child_ptr),
                    "Backplate" => backplate_toggle = Some(child_ptr),
                    "MenuBar" => menubar_toggle = Some(child_ptr),
                    "StatusBar" => statusbar_toggle = Some(child_ptr),
                    "Bevel" => bevel_toggle = Some(child_ptr),
                    "Border Width" => border_width_spin = Some(child_ptr),
                    "Bevel Depth" => bevel_depth_spin = Some(child_ptr),
                    "Bevel Shape..." => bevel_shape_btn = Some(child_ptr),
                    _ => {
                        if child.base().is_some() && child.base().unwrap().label.is_none() {
                            slider = Some(child_ptr);
                        }
                    }
                }
            }

            if let (Some(c), Some(t)) = (create_btn, tile_btn) {
                col.add_row(&[c, t], 28.0, 12.0);
            } else {
                if let Some(c) = create_btn {
                    col.add_widget(&mut *c, 28.0);
                }
                if let Some(t) = tile_btn {
                    col.add_widget(&mut *t, 28.0);
                }
            }

            if let (Some(w_sp), Some(h_sp)) = (width_spin, height_spin) {
                col.add_row(&[w_sp, h_sp], 42.0, 12.0);
            } else {
                if let Some(w_sp) = width_spin {
                    col.add_widget(&mut *w_sp, 42.0);
                }
                if let Some(h_sp) = height_spin {
                    col.add_widget(&mut *h_sp, 42.0);
                }
            }

            if let Some(t_dd) = type_dd {
                col.add_widget(&mut *t_dd, 44.0);
            }
            if let Some(s_dd) = shape_dd {
                col.add_widget(&mut *s_dd, 44.0);
            }
            if let Some(op_t) = opacity_toggle {
                col.add_widget(&mut *op_t, 28.0);
            }
            if let Some(en_t) = enable_toggle {
                col.add_widget(&mut *en_t, 28.0);
            }
            if let Some(sl_lbl) = slider_label {
                col.add_widget(&mut *sl_lbl, 12.0);
            }
            if let Some(sl) = slider {
                col.add_widget(&mut *sl, 20.0);
            }

            if let Some(w_s) = win_sec {
                col.add_widget(&mut *w_s, 20.0);
            }
            let toggles = [backplate_toggle, menubar_toggle, statusbar_toggle];
            let active_toggles: Vec<*mut dyn Element> = toggles.iter().filter_map(|&t| t).collect();
            if !active_toggles.is_empty() {
                col.add_row(&active_toggles, 28.0, 10.0);
            }

            if let Some(b_s) = border_sec {
                col.add_widget(&mut *b_s, 20.0);
            }
            if let Some(bs_dd) = border_style_dd {
                col.add_widget(&mut *bs_dd, 44.0);
            }
            if let Some(bev_t) = bevel_toggle {
                col.add_widget(&mut *bev_t, 28.0);
            }

            if let (Some(bw_sp), Some(bd_sp)) = (border_width_spin, bevel_depth_spin) {
                col.add_row(&[bw_sp, bd_sp], 42.0, 12.0);
            } else {
                if let Some(bw_sp) = border_width_spin {
                    col.add_widget(&mut *bw_sp, 42.0);
                }
                if let Some(bd_sp) = bevel_depth_spin {
                    col.add_widget(&mut *bd_sp, 42.0);
                }
            }
            if let Some(bs_btn) = bevel_shape_btn {
                col.add_widget(&mut *bs_btn, 28.0);
            }

            let total_h = col.current_y();
            self.scroll_box.update_bounds(total_h, y, h);
        }
    }

    fn rounded_corners(&self) -> (bool, bool, bool, bool) {
        (true, true, true, true)
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
                let child = &**child_ptr;
                let solid_border_opt = if child.type_name() == "Toggle" { None } else { child.solid_border() };
                let (cx, cy, cw, ch) = child.rect();

                if let Some((b_color, _thickness)) = solid_border_opt {
                    let cy_shifted = cy - scroll_y;
                    let cy_top = cy_shifted;
                    let cy_bottom = cy_shifted + ch;
                    if cy_bottom > y_start && cy_top < y_end {
                        let visible_top = cy_top.max(y_start);
                        let visible_bottom = cy_bottom.min(y_end);
                        let visible_h = visible_bottom - visible_top;
                        if visible_h > 0.0 {
                            let radii_adjusted = if visible_top > cy_top || visible_bottom < cy_bottom {
                                0.0
                            } else {
                                child.corner_radius()
                            };
                            quads.push((
                                cx,
                                visible_top,
                                cw,
                                visible_h,
                                radii_adjusted,
                                b_color,
                                child.rounded_corners(),
                            ));
                        }
                    }
                }

                for (qx, qy, qw, qh, qr, qc, qcorners) in child.all_rounded_quads(ctx) {
                    let mut rx = qx;
                    let mut ry = qy;
                    let mut rw = qw;
                    let mut rh = qh;
                    let mut rqr = qr;

                    if let Some((_, thickness)) = solid_border_opt {
                        if (qx - cx).abs() < 0.1 && (qy - cy).abs() < 0.1 && (qw - cw).abs() < 0.1 && (qh - ch).abs() < 0.1 {
                            rx += thickness;
                            ry += thickness;
                            rw -= 2.0 * thickness;
                            rh -= 2.0 * thickness;
                            rqr = (qr - thickness).max(0.0);
                        }
                    }

                    let qy_shifted = ry - scroll_y;
                    let qy_top = qy_shifted;
                    let qy_bottom = qy_shifted + rh;
                    if qy_bottom > y_start && qy_top < y_end {
                        let visible_top = qy_top.max(y_start);
                        let visible_bottom = qy_bottom.min(y_end);
                        let visible_h = visible_bottom - visible_top;
                        if visible_h > 0.0 {
                            let radii_adjusted = if visible_top > qy_top || visible_bottom < qy_bottom {
                                0.0
                            } else {
                                rqr
                            };
                            quads.push((rx, visible_top, rw, visible_h, radii_adjusted, qc, qcorners));
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
        let (_x, y, _w, h) = self.rect();
        let scroll_y = self.scroll_box.scroll_y;
        let y_start = y;
        let y_end = y + h;
        let ctx_dummy = crate::context::UiContext::new();

        unsafe {
            for child_ptr in &self.children {
                let child = &**child_ptr;
                let (cx, cy, cw, ch) = child.rect();
                let has_rounded = child.rounded_corners() != (false, false, false, false);
                let has_bg = child.color()[3].abs() > 0.001;

                for (qx, qy, qw, qh, qc) in child.all_quads(&ctx_dummy) {
                    if has_rounded && has_bg && (qx - cx).abs() < 0.1 && (qy - cy).abs() < 0.1 && (qw - cw).abs() < 0.1 && (qh - ch).abs() < 0.1 {
                        continue;
                    }
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

    fn extra_arcs(&self) -> Vec<(f32, f32, f32, f32, f32, f32, [f32; 4])> {
        let mut arcs = Vec::new();
        let scroll_y = self.scroll_box.scroll_y;
        let y_start = self.base.y;
        let y_end = self.base.y + self.base.h;

        unsafe {
            for child_ptr in &self.children {
                let child = &**child_ptr;
                for (cx, cy, r, t, start, end, color) in child.extra_arcs() {
                    let cy_shifted = cy - scroll_y;
                    if cy_shifted + r > y_start && cy_shifted - r < y_end {
                        arcs.push((cx, cy_shifted, r, t, start, end, color));
                    }
                }
            }
        }
        arcs
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

    fn draggable(&self) -> bool {
        self.scroll_box.draggable() || self.active_drag_widget.is_some()
    }

    fn drag_begin(&mut self, px: f32, py: f32) {
        if self.scroll_box.hit_test_scrollbar(px, py) {
            self.scroll_box.drag_begin(px, py);
            return;
        }

        let scroll_y = self.scroll_box.scroll_y;
        let py_translated = py + scroll_y;
        if let Some(child_ptr) = self.active_drag_widget {
            unsafe {
                (*child_ptr).drag_begin(px, py_translated);
            }
        }
    }

    fn drag_update(&mut self, px: f32, py: f32) -> bool {
        if self.scroll_box.draggable() {
            return self.scroll_box.drag_update(px, py);
        }

        let scroll_y = self.scroll_box.scroll_y;
        let py_translated = py + scroll_y;
        if let Some(child_ptr) = self.active_drag_widget {
            unsafe {
                return (*child_ptr).drag_update(px, py_translated);
            }
        }
        false
    }

    fn drag_end(&mut self) {
        self.scroll_box.drag_end();
        if let Some(child_ptr) = self.active_drag_widget {
            unsafe {
                (*child_ptr).drag_end();
            }
            self.active_drag_widget = None;
        }
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if state == ElementState::Pressed {
            self.active_drag_widget = None;
        }

        if self.scroll_box.mouse_input(button, state, px, py, ctx) {
            return true;
        }

        let scroll_y = self.scroll_box.scroll_y;
        let py_translated = py + scroll_y;
        let (_x, y, _w, h) = self.rect();

        unsafe {
            for child_ptr in &self.children {
                let has_popover = (**child_ptr).popover_rect().is_some();
                // Only dispatch if the click Y is inside the viewport or the child has an active popover
                if has_popover || (py >= y && py <= y + h) {
                    if (**child_ptr).mouse_input(button, state, px, py_translated, ctx) {
                        if state == ElementState::Pressed && (**child_ptr).draggable() {
                            self.active_drag_widget = Some(*child_ptr);
                        }
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
