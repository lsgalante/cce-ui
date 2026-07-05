use crate::colors;
use crate::widget::*;
use crate::widget::display::TextLabel;

#[derive(Debug, Clone)]
pub struct Plate {
    pub base: Layer,
    pub dragging: bool,
    pub drag_ox: f32,
    pub drag_oy: f32,
    pub drag_start_x: f32,
    pub drag_start_y: f32,
    pub bounds: Option<(f32, f32, f32, f32)>,
    pub color: Option<[f32; 4]>,
    pub curved_circle: Option<(f32, f32, f32)>,
    pub network_opacity: f32,
    pub blur: bool,
    pub visible: bool,
    pub draggable: bool,
    pub solid_border: Option<([f32; 4], f32)>,
    pub selected: bool,
    pub padding: Option<f32>,
}

impl Plate {
    pub fn set_network_opacity(&mut self, opacity: f32) {
        self.network_opacity = opacity;
    }

    pub fn set_curved_circle(&mut self, circle: Option<(f32, f32, f32)>) {
        self.curved_circle = circle;
    }

    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            base: Layer::new(x, y, w, h),
            dragging: false,
            drag_ox: 0.0,
            drag_oy: 0.0,
            drag_start_x: 0.0,
            drag_start_y: 0.0,
            bounds: None,
            color: None,
            curved_circle: None,
            network_opacity: 1.0,
            blur: true,
            visible: true,
            draggable: true,
            solid_border: None,
            selected: false,
            padding: None,
        }
    }

    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = Some(color);
        self
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.base.base.label = Some(label.to_string());
        self
    }

    pub fn with_blur(mut self, blur: bool) -> Self {
        self.blur = blur;
        self
    }

    pub fn with_draggable(mut self, draggable: bool) -> Self {
        self.draggable = draggable;
        self
    }

    pub fn with_solid_border(mut self, color: [f32; 4], thickness: f32) -> Self {
        self.solid_border = Some((color, thickness));
        self
    }

    pub fn with_padding(mut self, padding: f32) -> Self {
        self.padding = Some(padding);
        self
    }

    pub fn set_bounds(&mut self, bx: f32, by: f32, bw: f32, bh: f32) {
        self.bounds = Some((bx, by, bw, bh));
    }
}

impl Element for Plate {
    fn base(&self) -> Option<&Widget> { Some(&self.base.base) }
    fn base_mut(&mut self) -> Option<&mut Widget> { Some(&mut self.base.base) }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn as_ptr(&self) -> *mut (dyn Element + 'static) {
        self as *const Self as *mut Self as *mut (dyn Element + 'static)
    }
    fn as_ptr_mut(&mut self) -> *mut (dyn Element + 'static) {
        self as *mut Self as *mut (dyn Element + 'static)
    }

    fn is_plate(&self) -> bool { true }
    fn rounded_corners(&self) -> (bool, bool, bool, bool) {
        let r = crate::layout::plate_corner_radius();
        if r > 0.0 {
            (true, true, true, true)
        } else {
            (false, false, false, false)
        }
    }
    fn corner_radius(&self) -> f32 {
        crate::layout::plate_corner_radius()
    }
    fn highlight_quad(&self, _ctx: &UiContext) -> Option<(f32, f32, f32, f32, [f32; 4])>{ None }


    fn solid_border(&self) -> Option<([f32; 4], f32)> {
        if self.selected {
            Some((colors::active_theme().primary_accent, 1.5))
        } else if let Some(border) = self.solid_border {
            Some(border)
        } else if let Some(bc) = colors::plate_border_color() {
            Some((bc, colors::plate_border_thickness()))
        } else {
            None
        }
    }

    fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }

    fn set_modifiers(&mut self, ctrl: bool, shift: bool, alt: bool) {
        for &child_ptr in &self.base.children {
            unsafe {
                (*child_ptr).set_modifiers(ctrl, shift, alt);
            }
        }
    }

    fn visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
        self.base.visible = visible;
    }

    fn color(&self) -> [f32; 4] {
        let mut c = if let Some(c) = self.color {
            c
        } else if let Some(c) = colors::plate_color() {
            c
        } else if self.dragging {
            let base = colors::page_low_color();
            [
                (base[0] + 0.10).min(1.0),
                (base[1] + 0.15).min(1.0),
                (base[2] + 0.12).min(1.0),
                base[3],
            ]
        } else {
            colors::page_low_color()
        };
        c[3] *= crate::layout::plate_opacity();
        c[3] *= self.network_opacity;

        let use_blur = self.blur && colors::plate_blur();
        if use_blur {
            c[3] = -c[3].abs();
        }
        c
    }

    fn set_drag_bounds(&mut self, bx: f32, by: f32, bw: f32, bh: f32) {
        self.bounds = Some((bx, by, bw, bh));
    }

    fn hit_test(&self, px: f32, py: f32, ctx: &UiContext) -> bool {
        if ctx.is_coordinate_covered(self as *const Self as *const () as usize, px, py) {
            return false;
        }
        if let Some((cx, cy, r)) = self.curved_circle {
            let dx = px - cx;
            let dy = py - cy;
            return dx * dx + dy * dy <= r * r;
        }
        
        let (x, y, w, h) = self.rect();
        if px < x || px >= x + w || py < y || py >= y + h {
            return false;
        }
        
        let r = crate::layout::plate_corner_radius().min(w * 0.5).min(h * 0.5);

        if r <= 0.1 {
            return true;
        }
        
        // Check corners
        if px < x + r && py < y + r {
            let dx = px - (x + r);
            let dy = py - (y + r);
            return dx * dx + dy * dy <= r * r;
        }
        if px >= x + w - r && py < y + r {
            let dx = px - (x + w - r);
            let dy = py - (y + r);
            return dx * dx + dy * dy <= r * r;
        }
        if px >= x + w - r && py >= y + h - r {
            let dx = px - (x + w - r);
            let dy = py - (y + h - r);
            return dx * dx + dy * dy <= r * r;
        }
        if px < x + r && py >= y + h - r {
            let dx = px - (x + r);
            let dy = py - (y + h - r);
            return dx * dx + dy * dy <= r * r;
        }
        
        true
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let label_off = self.base.base.label_offset();
        let (clamped_x, clamped_y, clamped_w, clamped_h) = if let Some(parent_ptr) = self.base.parent {
            let (px, py, pw, ph) = unsafe { (*parent_ptr).rect() };
            let cx = x.clamp(px, px + pw.max(0.0));
            let cy = y.clamp(py, py + ph.max(0.0));
            let cw = w.min((px + pw.max(0.0) - cx).max(0.0));
            let ch = h.min((py + ph.max(0.0) - cy).max(0.0));
            (cx, cy, cw, ch)
        } else {
            (x, y, w, h)
        };

        self.base.base.x = clamped_x;
        self.base.base.y = clamped_y;
        self.base.base.w = clamped_w;
        self.base.base.h = clamped_h + label_off;

        if !self.base.visible {
            return;
        }

        let pad = self.padding.unwrap_or_else(|| crate::layout::plate_padding());
        let padding_x = pad;
        let padding_y = pad;
        let left_x = clamped_x + padding_x;
        let available_w = (clamped_w - 2.0 * padding_x).max(1.0);
        let start_y = clamped_y + label_off + padding_y;
        let available_h = (clamped_h - 2.0 * padding_y).max(1.0);

        let center_x = left_x + available_w / 2.0;
        let center_y = start_y + available_h / 2.0;
        let aspect_ratio = available_w / available_h;

        let mut active_widgets = Vec::new();
        for &w_ptr in &self.base.children {
            let w = unsafe { &*w_ptr };
            if !w.layout_ignore() {
                active_widgets.push(w_ptr);
            }
        }

        let mut total_diagonal = 0.0;
        let mut count = 0;
        for &w_ptr in &active_widgets {
            let w = unsafe { &*w_ptr };
            let (_, _, ww, wh) = w.rect();
            let use_w = if ww > 0.0 { ww.min(available_w) } else { available_w };
            let use_h = if wh > 0.0 { wh } else { 50.0 };
            total_diagonal += (use_w * use_w + use_h * use_h).sqrt();
            count += 1;
        }
        let avg_diagonal = if count > 0 { total_diagonal / count as f32 } else { 100.0 };
        let base_spacing = (avg_diagonal * 0.55).max(60.0);

        for (i, &w_ptr) in active_widgets.iter().enumerate() {
            let w = unsafe { &mut *w_ptr };
            let (_, _, ww, wh) = w.rect();
            let use_w = if ww > 0.0 { ww.min(available_w) } else { available_w };
            let use_h = if wh > 0.0 { wh } else { 50.0 };

            if i == 0 {
                let cx = (center_x - use_w / 2.0).clamp(left_x, (left_x + available_w - use_w).max(left_x));
                let cy = (center_y - use_h / 2.0).clamp(start_y, (start_y + available_h - use_h).max(start_y));
                let cw = use_w.min(clamped_x + clamped_w - padding_x - cx);
                let ch = use_h.min(clamped_y + label_off + clamped_h - padding_y - cy);
                w.set_rect(cx, cy, cw, ch);
            } else {
                let mut ring = 1;
                let mut ring_start = 1;
                let mut placed = false;
                while !placed {
                    let ring_capacity = ring * 6;
                    if i < ring_start + ring_capacity {
                        let pos_in_ring = i - ring_start;
                        let angle = (pos_in_ring as f32) * (2.0 * std::f32::consts::PI / ring_capacity as f32);
                        let radius = (ring as f32) * base_spacing;

                        let x_offset = radius * angle.cos() * aspect_ratio;
                        let y_offset = radius * angle.sin();

                        let raw_x = center_x + x_offset - use_w / 2.0;
                        let raw_y = center_y + y_offset - use_h / 2.0;

                        let cx = raw_x.clamp(left_x, (left_x + available_w - use_w).max(left_x));
                        let cy = raw_y.clamp(start_y, (start_y + available_h - use_h).max(start_y));
                        let cw = use_w.min(clamped_x + clamped_w - padding_x - cx);
                        let ch = use_h.min(clamped_y + label_off + clamped_h - padding_y - cy);

                        w.set_rect(cx, cy, cw, ch);
                        placed = true;
                    } else {
                        ring_start += ring_capacity;
                        ring += 1;
                    }
                }
            }
        }
    }

    fn parent(&self, _ctx: &UiContext) -> Option<*mut (dyn Element + 'static)> {
        self.base.parent
    }

    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>, ctx: &mut UiContext) {
        self.base.parent = parent;
        let id = self.base.base.id();
        if let Some(p_ptr) = parent {
            if let Some(p_base) = unsafe { (*p_ptr).base() } {
                let p_id = p_base.id();
                ctx.register_widget(p_id, p_ptr);
                ctx.register_widget(id, self as *mut Self as *mut (dyn Element + 'static));
                ctx.link_ids(p_id, id);
            }
        } else {
            ctx.layout_tree.parents.remove(&id);
        }
    }

    fn children(&self, _ctx: &UiContext) -> Vec<*mut (dyn Element + 'static)> {
        self.base.children.clone()
    }

    fn add_child(&mut self, child: *mut (dyn Element + 'static), ctx: &mut UiContext) {
        self.base.children.push(child);
        let id = self.base.base.id();
        let self_ptr = self.as_ptr();
        if let Some(c_base) = unsafe { (*child).base() } {
            let c_id = c_base.id();
            ctx.register_widget(id, self_ptr);
            ctx.register_widget(c_id, child);
            ctx.link_ids(id, c_id);
        }
        unsafe {
            (*child).set_parent(Some(self_ptr), ctx);
        }
    }

    fn clear_children(&mut self, ctx: &mut UiContext) {
        self.base.children.clear();
        let id = self.base.base.id();
        ctx.clear_children_ids(id);
    }

    fn all_rounded_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, f32, [f32; 4], (bool, bool, bool, bool))> {
        if !self.visible {
            return Vec::new();
        }
        let mut quads = Vec::new();
        let (r1, r2, r3, r4) = self.rounded_corners();
        if r1 || r2 || r3 || r4 {
            let (x, y, w, h) = self.rect();
            let label_off = self.base.base.label_offset();
            let radius = self.corner_radius();
            let c = self.color();
            if c[3].abs() > 0.001 {
                quads.push((x, y + label_off, w, h - label_off, radius, c, (r1, r2, r3, r4)));
            }
        }
        for &child_ptr in &self.base.children {
            let widget = unsafe { &*child_ptr };
            quads.extend(widget.all_rounded_quads(ctx));
        }
        quads
    }

    fn all_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.visible {
            return Vec::new();
        }
        let mut quads = Vec::new();
        let (px, py, pw, ph) = self.rect();
        let has_rounded = self.rounded_corners() != (false, false, false, false);
        if !has_rounded {
            let label_off = self.base.base.label_offset();
            quads.push((px, py + label_off, pw, ph - label_off, self.color()));
        }

        for &child_ptr in &self.base.children {
            let widget = unsafe { &*child_ptr };
            let (wx, wy, ww, wh) = widget.rect();
            let has_rounded = widget.rounded_corners() != (false, false, false, false);
            for (qx, qy, qw, qh, qc) in widget.all_quads(ctx) {
                if has_rounded && (qx - wx).abs() < 0.1 && (qy - wy).abs() < 0.1 && (qw - ww).abs() < 0.1 && (qh - wh).abs() < 0.1 {
                    continue;
                }
                quads.push((qx, qy, qw, qh, qc));
            }
        }
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        if !self.visible {
            return Vec::new();
        }
        let mut labels = Vec::new();
        if let Some(ref label) = self.base.base.label {
            let (_, font_size) = crate::layout::control_label_font_parsed();
            labels.push(TextLabel {
                text: label.clone(),
                x: self.base.base.x,
                y: self.base.base.y,
                font_size,
                color: colors::control_label_color_u8(),
            });
        }
        for &child_ptr in &self.base.children {
            let widget = unsafe { &*child_ptr };
            labels.extend(widget.text_labels());
        }
        labels
    }

    fn text_labels_with_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<[f32; 4]>)> {
        if !self.visible {
            return Vec::new();
        }
        let mut result = Vec::new();
        if let Some(ref label) = self.base.base.label {
            let (_, font_size) = crate::layout::control_label_font_parsed();
            result.push((
                TextLabel {
                    text: label.clone(),
                    x: self.base.base.x,
                    y: self.base.base.y,
                    font_size,
                    color: colors::control_label_color_u8(),
                },
                None,
            ));
        }
        for &child_ptr in &self.base.children {
            let widget = unsafe { &*child_ptr };
            result.extend(widget.text_labels_with_bounds(ctx));
        }
        result
    }

    fn text_labels_with_font_and_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        if !self.visible {
            return Vec::new();
        }
        let mut result = Vec::new();
        if let Some(ref label) = self.base.base.label {
            let (_, font_size) = crate::layout::control_label_font_parsed();
            result.push((
                TextLabel {
                    text: label.clone(),
                    x: self.base.base.x,
                    y: self.base.base.y,
                    font_size,
                    color: colors::control_label_color_u8(),
                },
                None,
                None,
            ));
        }
        for &child_ptr in &self.base.children {
            let widget = unsafe { &*child_ptr };
            result.extend(widget.text_labels_with_font_and_bounds(ctx));
        }
        result
    }

    fn get_text_items(&self) -> Vec<(&glyphon::Buffer, f32, f32, glyphon::Color)> {
        if !self.visible {
            return Vec::new();
        }
        let mut items = Vec::new();
        for &child_ptr in &self.base.children {
            let widget = unsafe { &*child_ptr };
            items.extend(widget.get_text_items());
        }
        items
    }

    fn prepare_text(&mut self, fs: &mut glyphon::FontSystem) {
        if !self.visible {
            return;
        }
        for &child_ptr in &self.base.children {
            unsafe {
                (*child_ptr).prepare_text(fs);
            }
        }
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if !self.visible {
            return false;
        }
        let mut changed = false;
        for &widget_ptr in &self.base.children {
            let widget = unsafe { &mut *widget_ptr };
            if widget.is_dragging() {
                if widget.drag_update(px, py) {
                    changed = true;
                }
            } else if widget.cursor_moved(px, py, ctx) {
                changed = true;
            }
        }
        changed
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if !self.visible {
            return false;
        }
        for &widget_ptr in self.base.children.iter().rev() {
            let widget = unsafe { &mut *widget_ptr };
            if widget.popover_rect().is_some() {
                if widget.mouse_input(button, state, px, py, ctx) {
                    return true;
                }
            }
        }
        for &widget_ptr in self.base.children.iter().rev() {
            let widget = unsafe { &mut *widget_ptr };
            if widget.mouse_input(button, state, px, py, ctx) {
                return true;
            }
            if state == ElementState::Pressed && !widget.hit_test(px, py, ctx) {
                widget.unfocus();
            }
        }

        if !self.draggable { return false; }
        if button != MouseButton::Left { return false; }
        match state {
            ElementState::Pressed => {
                if self.hit_test(px, py, ctx) {
                    self.drag_begin(px, py);
                    return true;
                }
            }
            ElementState::Released => {
                if self.dragging { self.drag_end(); return true; }
            }
        }
        false
    }

    fn keyboard_input(&mut self, event: &KeyEvent, ctx: &mut UiContext) -> bool {
        if !self.visible {
            return false;
        }
        for &widget_ptr in &self.base.children {
            let widget = unsafe { &mut *widget_ptr };
            if widget.keyboard_input(event, ctx) {
                return true;
            }
        }
        false
    }

    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if !self.visible {
            return false;
        }
        for &widget_ptr in self.base.children.iter().rev() {
            let widget = unsafe { &mut *widget_ptr };
            if widget.mouse_wheel(delta, px, py, ctx) {
                return true;
            }
        }
        false
    }

    fn popover_rect(&self) -> Option<(f32, f32, f32, f32)> {
        if !self.visible {
            return None;
        }
        for &widget_ptr in self.base.children.iter().rev() {
            let widget = unsafe { &*widget_ptr };
            if let Some(r) = widget.popover_rect() {
                return Some(r);
            }
        }
        None
    }

    fn render_popover(&self, pc: &mut dyn crate::layout::RenderTarget) {
        if !self.visible {
            return;
        }
        for &widget_ptr in self.base.children.iter().rev() {
            let widget = unsafe { &*widget_ptr };
            widget.render_popover(pc);
        }
    }

    fn tick(&mut self, dt: f32, ctx: &mut UiContext) -> bool {
        if !self.visible {
            return false;
        }
        let mut changed = false;
        for &widget_ptr in &self.base.children {
            let widget = unsafe { &mut *widget_ptr };
            if widget.tick(dt, ctx) {
                changed = true;
            }
        }
        changed
    }

    fn drag_update(&mut self, px: f32, py: f32) -> bool {
        let nx = px - self.drag_ox;
        let ny = py - self.drag_oy;
        let (nx, ny) = if let Some((bx, by, bw, bh)) = self.bounds {
            (nx.clamp(bx, bx + bw - self.base.base.w), ny.clamp(by, by + bh - self.base.base.h))
        } else {
            (nx, ny)
        };
        if (nx - self.base.base.x).abs() > 0.01 || (ny - self.base.base.y).abs() > 0.01 {
            let dx = nx - self.base.base.x;
            let dy = ny - self.base.base.y;
            self.base.base.x = nx;
            self.base.base.y = ny;
            
            for &child_ptr in &self.base.children {
                unsafe {
                    let (cx, cy, cw, ch) = (*child_ptr).rect();
                    (*child_ptr).set_rect(cx + dx, cy + dy, cw, ch);
                }
            }
            return true;
        }
        false
    }

    fn drag_begin(&mut self, px: f32, py: f32) {
        self.dragging = true;
        self.drag_ox = px - self.base.base.x;
        self.drag_oy = py - self.base.base.y;
        self.drag_start_x = self.base.base.x;
        self.drag_start_y = self.base.base.y;
    }

    fn drag_end(&mut self) { self.dragging = false; }
}

unsafe impl Send for Plate {}
unsafe impl Sync for Plate {}
