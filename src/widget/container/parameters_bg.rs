use crate::colors;
use crate::widget::*;
use crate::widget::input::{Slider, Spinbox, Button, Dropdown, TextBox, Checkbox, ColorSelector};
use crate::widget::display::{Float3, TextLabel};

pub struct ParametersBg {
    pub base: Widget,
    display_params: Vec<(String, String, String)>,
    dragging_param: Option<usize>,
    pub focused_param: Option<usize>,
    pub code_editor: Option<TextEditorState>,
    mouse_pos: Option<(f32, f32)>,
    pub sliders: Vec<Option<crate::widget::Adapted<Slider>>>,
    pub float3s: Vec<Option<Float3>>,
    pub spinboxes: Vec<Option<crate::widget::Adapted<Spinbox>>>,
    pub buttons: Vec<Option<crate::widget::Adapted<Button>>>,
    pub choices: Vec<Option<crate::widget::Adapted<Dropdown>>>,
    pub texts: Vec<Option<TextBox>>,
    pub checkboxes: Vec<Option<crate::widget::Adapted<Checkbox>>>,
    pub colors: Vec<Option<ColorSelector>>,
    visible: bool,
    pub children: Vec<*mut (dyn Element + 'static)>,
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub scroll_y: f32,
    pub content_h: f32,
    scrollbar_dragging: bool,
    drag_offset_y: f32,
}

impl ParametersBg {
    pub fn new() -> Self {
        Self {
            base: Widget::new(),
            display_params: Vec::new(),
            dragging_param: None,
            focused_param: None,
            code_editor: None,
            mouse_pos: None,
            sliders: Vec::new(),
            float3s: Vec::new(),
            spinboxes: Vec::new(),
            buttons: Vec::new(),
            choices: Vec::new(),
            texts: Vec::new(),
            checkboxes: Vec::new(),
            colors: Vec::new(),
            visible: true,
            children: Vec::new(),
            parent: None,
            scroll_y: 0.0,
            content_h: 0.0,
            scrollbar_dragging: false,
            drag_offset_y: 0.0,
        }
    }

    pub fn get_total_content_height(&self) -> f32 {
        let mut cur_y = 30.0;
        for (i, p) in self.display_params.iter().enumerate() {
            let h = if p.2 == "code" {
                let val_text = if self.focused_param == Some(i) {
                    if let Some(ref editor) = self.code_editor {
                        &editor.buffer
                    } else {
                        &p.1
                    }
                } else {
                    &p.1
                };
                let line_count = val_text.split('\n').count();
                let content_h = 22.0 + (line_count as f32 * 16.0) + 12.0;
                content_h.max(200.0)
            } else if p.2 == "section" {
                24.0
            } else if p.2.starts_with("float3") {
                108.0
            } else if p.2.starts_with("slider") {
                38.0
            } else if p.2 == "text" || p.2.starts_with("spinbox") || p.2.starts_with("choice") {
                42.0
            } else if p.2.starts_with("color") || p.2 == "rgb" || p.2 == "rgba" {
                40.0
            } else if p.2 == "button" || p.2 == "toggle" || p.2 == "checkbox" {
                24.0
            } else {
                20.0
            };
            cur_y += h + 8.0;
        }
        cur_y + 10.0 // Add padding at the bottom
    }

    pub fn get_param_rects(&self) -> Vec<(f32, f32, f32, f32)> {
        let mut rects = Vec::new();
        let mut cur_y = self.base.y + 30.0 - self.scroll_y;
        for (i, p) in self.display_params.iter().enumerate() {
            let h = if p.2 == "code" {
                let val_text = if self.focused_param == Some(i) {
                    if let Some(ref editor) = self.code_editor {
                        &editor.buffer
                    } else {
                        &p.1
                    }
                } else {
                    &p.1
                };
                let line_count = val_text.split('\n').count();
                let content_h = 22.0 + (line_count as f32 * 16.0) + 12.0;
                content_h.max(200.0)
            } else if p.2 == "section" {
                24.0
            } else if p.2.starts_with("float3") {
                108.0
            } else if p.2.starts_with("slider") {
                38.0
            } else if p.2 == "text" || p.2.starts_with("spinbox") || p.2.starts_with("choice") {
                42.0
            } else if p.2.starts_with("color") || p.2 == "rgb" || p.2 == "rgba" {
                40.0
            } else if p.2 == "button" || p.2 == "toggle" || p.2 == "checkbox" {
                24.0
            } else {
                20.0
            };
            rects.push((self.base.x + 8.0, cur_y, self.base.w - 16.0, h));
            cur_y += h + 8.0;
        }
        rects
    }

    pub fn hit_test_scrollbar(&self, px: f32, py: f32) -> bool {
        if self.content_h <= self.base.h {
            return false;
        }
        let sb_w = crate::layout::scrollbar_width();
        let sb_x = self.base.x + self.base.w - sb_w - 4.0;
        let sb_track_h = self.base.h - 8.0;
        let sb_track_y = self.base.y + 4.0;

        px >= sb_x - 4.0 && px <= sb_x + sb_w + 4.0
            && py >= sb_track_y && py <= sb_track_y + sb_track_h
    }

    fn update_slider_rects(&mut self) {
        let rects = self.get_param_rects();
        for (i, s_opt) in self.sliders.iter_mut().enumerate() {
            if let Some(s) = s_opt {
                let r = rects[i];
                s.set_rect(r.0, r.1, r.2, r.3);
            }
        }
        for (i, f_opt) in self.float3s.iter_mut().enumerate() {
            if let Some(f) = f_opt {
                let r = rects[i];
                f.set_rect(r.0, r.1, r.2, r.3);
            }
        }
        for (i, sb_opt) in self.spinboxes.iter_mut().enumerate() {
            if let Some(sb) = sb_opt {
                let r = rects[i];
                sb.set_rect(r.0, r.1, r.2, r.3);
            }
        }
        for (i, b_opt) in self.buttons.iter_mut().enumerate() {
            if let Some(b) = b_opt {
                let r = rects[i];
                b.set_rect(r.0, r.1, r.2, r.3);
            }
        }
        for (i, d_opt) in self.choices.iter_mut().enumerate() {
            if let Some(d) = d_opt {
                let r = rects[i];
                d.set_rect(r.0, r.1, r.2, r.3);
            }
        }
        for (i, tb_opt) in self.texts.iter_mut().enumerate() {
            if let Some(tb) = tb_opt {
                let r = rects[i];
                tb.set_rect(r.0, r.1, r.2, r.3);
            }
        }
        for (i, cb_opt) in self.checkboxes.iter_mut().enumerate() {
            if let Some(cb) = cb_opt {
                let r = rects[i];
                cb.set_rect(r.0, r.1, r.2, r.3);
            }
        }
        for (i, c_opt) in self.colors.iter_mut().enumerate() {
            if let Some(c) = c_opt {
                let r = rects[i];
                c.set_rect(r.0, r.1, r.2, r.3);
            }
        }
    }

    fn own_text_labels(&self) -> Vec<TextLabel> {
        let rects = self.get_param_rects();
        let mut labels = Vec::new();
        for (i, (name, value, ptype)) in self.display_params.iter().enumerate() {
            let r = rects[i];
            if ptype.starts_with("slider") {
                if let Some(s) = &self.sliders[i] {
                    labels.extend(s.text_labels());
                }
            } else if ptype.starts_with("float3") {
                if let Some(f) = &self.float3s[i] {
                    labels.extend(f.text_labels());
                }
            } else if ptype == "section" {
                labels.push(TextLabel {
                    text: name.clone(),
                    x: self.base.x + 12.0,
                    y: r.1 + 2.0,
                    font_size: 13.0,
                    color: [0xee, 0xee, 0xf0],
                });
            } else if ptype == "code" {
                labels.push(TextLabel {
                    text: format!("{}:", name),
                    x: self.base.x + 12.0,
                    y: r.1,
                    font_size: 12.0,
                    color: [0xaa, 0xaa, 0xbb],
                });
                let val_text = if self.focused_param == Some(i) {
                    if let Some(ref editor) = self.code_editor {
                        editor.buffer.clone()
                    } else {
                        value.clone()
                    }
                } else {
                    value.clone()
                };
                labels.push(TextLabel {
                    text: val_text,
                    x: r.0 + 12.0,
                    y: r.1 + 22.0,
                    font_size: 12.0,
                    color: [0xee, 0xee, 0xf0],
                });
            } else if ptype.starts_with("spinbox") {
                if let Some(sb) = &self.spinboxes[i] {
                    labels.extend(sb.text_labels());
                }
            } else if ptype == "text" {
                if let Some(tb) = &self.texts[i] {
                    labels.extend(tb.text_labels());
                }
            } else if ptype.starts_with("choice") {
                if let Some(d) = &self.choices[i] {
                    labels.extend(d.text_labels());
                }
            } else if ptype == "button" {
                if let Some(b) = &self.buttons[i] {
                    labels.extend(b.text_labels());
                }
            } else if ptype == "toggle" || ptype == "checkbox" {
                if let Some(cb) = &self.checkboxes[i] {
                    labels.extend(cb.text_labels());
                }
            } else if ptype.starts_with("color") || ptype == "rgb" || ptype == "rgba" {
                if let Some(c) = &self.colors[i] {
                    labels.extend(c.text_labels());
                }
            } else {
                labels.push(TextLabel {
                    text: format!("{}: {}", name, value),
                    x: self.base.x + 8.0,
                    y: r.1,
                    font_size: 12.0,
                    color: [0xaa, 0xaa, 0xbb],
                });
            }
        }
        labels
    }
}

fn parse_slider_range(ptype: &str) -> (f32, f32) {
    if ptype.starts_with("slider:") || ptype.starts_with("float3:") {
        let parts: Vec<&str> = ptype.split(':').collect();
        if parts.len() >= 3 {
            if let (Ok(min), Ok(max)) = (parts[1].parse::<f32>(), parts[2].parse::<f32>()) {
                return (min, max);
            }
        }
    }
    (0.0, 2.0)
}

fn parse_hex_to_rgb(s: &str) -> Option<[u8; 3]> {
    crate::color::parse_hex_bytes(s).map(|[r, g, b, _]| [r, g, b])
}

fn parse_spinbox_range(ptype: &str) -> (i32, i32, i32) {
    if ptype.starts_with("spinbox:") {
        let parts: Vec<&str> = ptype.split(':').collect();
        if parts.len() >= 4 {
            if let (Ok(min), Ok(max), Ok(step)) = (parts[1].parse::<i32>(), parts[2].parse::<i32>(), parts[3].parse::<i32>()) {
                return (min, max, step);
            }
        } else if parts.len() == 3 {
            if let (Ok(min), Ok(max)) = (parts[1].parse::<i32>(), parts[2].parse::<i32>()) {
                return (min, max, 1);
            }
        }
    }
    (0, 10000, 1)
}

fn parse_float3_value(val_str: &str, min: f32, max: f32) -> [f32; 3] {
    let mut out = [0.5, 0.5, 0.5];
    let parts: Vec<&str> = val_str
        .split(|c| c == ':' || c == ',' || c == ' ')
        .filter(|s| !s.is_empty())
        .collect();
    for i in 0..3 {
        if i < parts.len() {
            if let Ok(v) = parts[i].parse::<f32>() {
                let range = max - min;
                if range != 0.0 {
                    out[i] = ((v - min) / range).clamp(0.0, 1.0);
                } else {
                    out[i] = 0.0;
                }
            }
        }
    }
    out
}

impl Element for ParametersBg {
    crate::impl_widget_base!(ParametersBg);
    fn is_scrollable(&self) -> bool { true }
    fn blocks_backplate_drag(&self) -> bool { true }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::control_label_font())
    }

    fn rounded_corners(&self) -> (bool, bool, bool, bool) { (true, true, true, true) }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if let Some(b) = self.base_mut() {
            b.x = x;
            b.y = y;
            b.w = w;
            b.h = h;
        }
        self.content_h = self.get_total_content_height();
        let max_scroll = (self.content_h - h).max(0.0);
        self.scroll_y = self.scroll_y.clamp(0.0, max_scroll);
        self.update_slider_rects();

        // Layout child widgets vertically
        if !self.visible {
            return;
        }
        let padding_x = 8.0;
        let padding_y = 10.0;
        let left_x = x + padding_x;
        let available_w = (w - 2.0 * padding_x).max(1.0);
        let mut current_y = y + padding_y;
        let spacing = 8.0;

        for &child_ptr in &self.children {
            let child = unsafe { &mut *child_ptr };
            let (_, _, _, ch) = child.rect();
            let use_h = if ch > 0.0 { ch } else { 42.0 };
            child.set_rect(left_x, current_y, available_w, use_h);
            current_y += use_h + spacing;
        }
    }

    fn color(&self) -> [f32; 4] {
        if !self.visible {
            return [0.0, 0.0, 0.0, 0.0];
        }
        colors::PARAM_BG
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn visible(&self) -> bool {
        self.visible
    }

    fn hit_test(&self, px: f32, py: f32, ctx: &UiContext) -> bool {
        if !self.visible {
            return false;
        }
        if ctx.is_coordinate_covered(self as *const Self as *const () as usize, px, py) {
            return false;
        }
        let (x, y, w, h) = self.rect();
        let hit_base = px >= x && px <= x + w && py >= y && py <= y + h;
        if hit_base {
            return true;
        }
        if let Some((pop_x, pop_y, pop_w, pop_h)) = self.popover_rect() {
            if px >= pop_x && px <= pop_x + pop_w && py >= pop_y && py <= pop_y + pop_h {
                return true;
            }
        }
        false
    }

    fn as_param_controller(&self) -> Option<&dyn ParamController> { Some(self) }
    fn as_param_controller_mut(&mut self) -> Option<&mut dyn ParamController> { Some(self) }

    fn unfocus(&mut self) {
        if let Some(idx) = self.focused_param {
            if idx < self.display_params.len() {
                let p = &mut self.display_params[idx];
                if p.2.starts_with("spinbox") {
                    if let Some(sb) = &mut self.spinboxes[idx] {
                        sb.unfocus();
                        p.1 = sb.value.to_string();
                    }
                } else if p.2.starts_with("slider") {
                    if let Some(s) = &mut self.sliders[idx] {
                        s.unfocus();
                        let (min, max) = parse_slider_range(&p.2);
                        let new_val = min + s.value * (max - min);
                        p.1 = format!("{:.2}", new_val);
                    }
                } else if p.2.starts_with("float3") {
                    if let Some(f) = &mut self.float3s[idx] {
                        f.unfocus();
                        let (min, max) = parse_slider_range(&p.2);
                        let val0 = min + f.values[0] * (max - min);
                        let val1 = min + f.values[1] * (max - min);
                        let val2 = min + f.values[2] * (max - min);
                        p.1 = format!("{:.2}:{:.2}:{:.2}", val0, val1, val2);
                    }
                } else if p.2 == "text" {
                    if let Some(tb) = &mut self.texts[idx] {
                        tb.unfocus();
                        p.1 = tb.text.clone();
                    }
                } else if p.2.starts_with("choice") {
                    if let Some(d) = &mut self.choices[idx] {
                        d.unfocus();
                        if let Some(val) = d.get_value_string() {
                            p.1 = val;
                        }
                    }
                } else if p.2.starts_with("color") || p.2 == "rgb" || p.2 == "rgba" {
                    if let Some(c) = &mut self.colors[idx] {
                        c.unfocus();
                        if let Some(val) = c.get_value_string() {
                            p.1 = val;
                        }
                    }
                } else if p.2 == "code" {
                    if let Some(ref editor) = self.code_editor {
                        p.1 = editor.buffer.clone();
                    }
                    self.code_editor = None;
                }
            }
        }
        self.focused_param = None;

        for &child_ptr in &self.children {
            let child = unsafe { &mut *child_ptr };
            child.unfocus();
        }
    }

    fn draggable(&self) -> bool {
        self.scrollbar_dragging
            || self.dragging_param.is_some() 
            || self.display_params.iter().any(|p| p.2.starts_with("slider") || p.2.starts_with("float3"))
    }

    fn is_dragging(&self) -> bool {
        self.scrollbar_dragging || self.dragging_param.is_some()
    }

    fn drag_begin(&mut self, px: f32, py: f32) {
        if self.scrollbar_dragging {
            return;
        }
        let rects = self.get_param_rects();
        for (i, p) in self.display_params.iter().enumerate() {
            if p.2.starts_with("slider") {
                let r = rects[i];
                if let Some(s) = &mut self.sliders[i] {
                    let top = crate::widget::label_offset(s);
                    if py >= r.1 + top && py <= r.1 + r.3 {
                        s.drag_begin(px, py);
                        self.dragging_param = Some(i);
                        break;
                    }
                }
            } else if p.2.starts_with("float3") {
                let r = rects[i];
                if py >= r.1 && py <= r.1 + r.3 {
                    if let Some(f) = &mut self.float3s[i] {
                        let mut dummy = crate::context::UiContext::new();
                        if f.mouse_input(MouseButton::Left, ElementState::Pressed, px, py, &mut dummy) {
                            self.dragging_param = Some(i);
                            break;
                        }
                    }
                }
            }
        }
    }

    fn drag_update(&mut self, px: f32, py: f32) -> bool {
        if self.scrollbar_dragging {
            let sb_track_h = self.base.h - 8.0;
            let sb_track_y = self.base.y + 4.0;
            let visible_ratio = self.base.h / self.content_h;
            let thumb_h = if sb_track_h <= 20.0 {
                sb_track_h
            } else {
                (sb_track_h * visible_ratio).clamp(20.0, sb_track_h)
            };
            let max_scroll = (self.content_h - self.base.h).max(0.0);
            
            let target_thumb_y = py - self.drag_offset_y;
            let new_scroll_ratio = if sb_track_h - thumb_h > 0.0 {
                ((target_thumb_y - sb_track_y) / (sb_track_h - thumb_h)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            
            let old_scroll = self.scroll_y;
            self.scroll_y = new_scroll_ratio * max_scroll;
            if (self.scroll_y - old_scroll).abs() > 0.01 {
                self.update_slider_rects();
                return true;
            }
            return false;
        }

        if let Some(i) = self.dragging_param {
            if let Some(s) = &mut self.sliders[i] {
                if s.drag_update(px, py) {
                    let (min, max) = parse_slider_range(&self.display_params[i].2);
                    let new_val = min + s.value * (max - min);
                    let old_val = &self.display_params[i].1;
                    let new_val_str = format!("{:.2}", new_val);
                    if *old_val != new_val_str {
                        self.display_params[i].1 = new_val_str;
                        return true;
                    }
                }
            } else if let Some(f) = &mut self.float3s[i] {
                if f.drag_update(px, py) {
                    let (min, max) = parse_slider_range(&self.display_params[i].2);
                    let val0 = min + f.values[0] * (max - min);
                    let val1 = min + f.values[1] * (max - min);
                    let val2 = min + f.values[2] * (max - min);
                    let new_val_str = format!("{:.2}:{:.2}:{:.2}", val0, val1, val2);
                    let old_val = &self.display_params[i].1;
                    if *old_val != new_val_str {
                        self.display_params[i].1 = new_val_str;
                        return true;
                    }
                }
            }
        }
        false
    }

    fn drag_end(&mut self) {
        if self.scrollbar_dragging {
            self.scrollbar_dragging = false;
            return;
        }
        if let Some(i) = self.dragging_param.take() {
            if let Some(s) = &mut self.sliders[i] {
                s.drag_end();
            } else if let Some(f) = &mut self.float3s[i] {
                f.drag_end();
            }
        }
    }

    fn parent(&self, _ctx: &UiContext) -> Option<*mut (dyn Element + 'static)> {
        self.parent
    }

    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>, ctx: &mut UiContext) {
        self.parent = parent;
        let id = self.base.id();
        if let Some(p_ptr) = parent {
            if let Some(p_base) = unsafe { (*p_ptr).base() } {
                let p_id = p_base.id();
                ctx.register_widget(p_id, p_ptr);
                ctx.register_widget(id, self as *mut Self as *mut (dyn Element + 'static));
                ctx.link_ids(p_id, id);
            }
        } else {
            ctx.tree.set_parent(id, None);
        }
    }

    fn children(&self, _ctx: &UiContext) -> Vec<*mut (dyn Element + 'static)> {
        self.children.clone()
    }

    fn add_child(&mut self, child: *mut (dyn Element + 'static), ctx: &mut UiContext) {
        self.children.push(child);
        let id = self.base.id();
        if let Some(c_base) = unsafe { (*child).base() } {
            let c_id = c_base.id();
            let self_ptr = self.as_ptr();
            ctx.register_widget(id, self_ptr);
            ctx.register_widget(c_id, child);
            ctx.link_ids(id, c_id);
        }
    }

    fn clear_children(&mut self, ctx: &mut UiContext) {
        self.children.clear();
        let id = self.base.id();
        ctx.clear_children_ids(id);
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        self.mouse_pos = Some((px, py));
        let was = self.base.hovered;
        let is_hit = self.hit_test(px, py, ctx);
        self.base.hovered = is_hit;
        let mut changed = was != is_hit;

        if self.scrollbar_dragging {
            let sb_track_h = self.base.h - 8.0;
            let sb_track_y = self.base.y + 4.0;
            let visible_ratio = self.base.h / self.content_h;
            let thumb_h = if sb_track_h <= 20.0 {
                sb_track_h
            } else {
                (sb_track_h * visible_ratio).clamp(20.0, sb_track_h)
            };
            let max_scroll = (self.content_h - self.base.h).max(0.0);
            
            let target_thumb_y = py - self.drag_offset_y;
            let new_scroll_ratio = if sb_track_h - thumb_h > 0.0 {
                ((target_thumb_y - sb_track_y) / (sb_track_h - thumb_h)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            
            let old_scroll = self.scroll_y;
            self.scroll_y = new_scroll_ratio * max_scroll;
            if (self.scroll_y - old_scroll).abs() > 0.01 {
                self.update_slider_rects();
                changed = true;
            }
        }

        for sb_opt in &mut self.spinboxes {
            if let Some(sb) = sb_opt {
                if sb.on_cursor_moved(px, py, ctx) {
                    changed = true;
                }
            }
        }
        for f_opt in &mut self.float3s {
            if let Some(f) = f_opt {
                if f.on_cursor_moved(px, py, ctx) {
                    changed = true;
                }
            }
        }
        for b_opt in &mut self.buttons {
            if let Some(b) = b_opt {
                if b.on_cursor_moved(px, py, ctx) {
                    changed = true;
                }
            }
        }
        for d_opt in &mut self.choices {
            if let Some(d) = d_opt {
                if d.on_cursor_moved(px, py, ctx) {
                    changed = true;
                }
            }
        }
        for tb_opt in &mut self.texts {
            if let Some(tb) = tb_opt {
                if tb.on_cursor_moved(px, py, ctx) {
                    changed = true;
                }
            }
        }
        for cb_opt in &mut self.checkboxes {
            if let Some(cb) = cb_opt {
                if cb.on_cursor_moved(px, py, ctx) {
                    changed = true;
                }
            }
        }
        for c_opt in &mut self.colors {
            if let Some(c) = c_opt {
                if c.on_cursor_moved(px, py, ctx) {
                    changed = true;
                }
            }
        }

        for &widget_ptr in &self.children {
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

        if button == MouseButton::Left {
            if state == ElementState::Pressed {
                if self.hit_test_scrollbar(px, py) {
                    self.focus();
                    self.scrollbar_dragging = true;
                    
                    let sb_track_h = self.base.h - 8.0;
                    let sb_track_y = self.base.y + 4.0;
                    let visible_ratio = self.base.h / self.content_h;
                    let thumb_h = if sb_track_h <= 20.0 {
                        sb_track_h
                    } else {
                        (sb_track_h * visible_ratio).clamp(20.0, sb_track_h)
                    };
                    let max_scroll = (self.content_h - self.base.h).max(0.0);
                    let scroll_ratio = if max_scroll > 0.0 { self.scroll_y / max_scroll } else { 0.0 };
                    let thumb_y = sb_track_y + scroll_ratio * (sb_track_h - thumb_h);
                    
                    let click_offset = py - thumb_y;
                    if click_offset >= 0.0 && click_offset <= thumb_h {
                        self.drag_offset_y = click_offset;
                    } else {
                        // Clicked outside the thumb: jump thumb center to py
                        self.drag_offset_y = thumb_h / 2.0;
                        let target_thumb_y = py - self.drag_offset_y;
                        let new_scroll_ratio = if sb_track_h - thumb_h > 0.0 {
                            ((target_thumb_y - sb_track_y) / (sb_track_h - thumb_h)).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        self.scroll_y = new_scroll_ratio * max_scroll;
                        self.update_slider_rects();
                    }
                    return true;
                }
            } else if state == ElementState::Released {
                if self.scrollbar_dragging {
                    self.scrollbar_dragging = false;
                    return true;
                }
            }
        }

        // 1. Check open dropdown popovers first (since they are drawn on top)
        for (i, d_opt) in self.choices.iter_mut().enumerate() {
            if let Some(d) = d_opt {
                if d.popover_rect().is_some() {
                    if d.mouse_input(button, state, px, py, ctx) {
                        if d.take_change() {
                            if let Some(val) = d.get_value_string() {
                                self.display_params[i].1 = val;
                            }
                        }
                        return true;
                    }
                }
            }
        }

        for &widget_ptr in self.children.iter().rev() {
            let widget = unsafe { &mut *widget_ptr };
            if widget.popover_rect().is_some() {
                if widget.mouse_input(button, state, px, py, ctx) {
                    return true;
                }
            }
        }

        // 2. Propagate to our widgets
        for (i, p) in self.display_params.iter_mut().enumerate() {
            if p.2.starts_with("choice") {
                if let Some(d) = &mut self.choices[i] {
                    if d.mouse_input(button, state, px, py, ctx) {
                        if d.take_change() {
                            if let Some(val) = d.get_value_string() {
                                p.1 = val;
                            }
                        }
                        return true;
                    }
                }
            } else if p.2 == "button" {
                if let Some(b) = &mut self.buttons[i] {
                    if b.mouse_input(button, state, px, py, ctx) {
                        if b.take_click() {
                            p.1 = "clicked".to_string();
                        }
                        return true;
                    }
                }
            } else if p.2 == "text" {
                if let Some(tb) = &mut self.texts[i] {
                    if tb.mouse_input(button, state, px, py, ctx) {
                        if tb.editing {
                            self.focused_param = Some(i);
                        } else {
                            if self.focused_param == Some(i) {
                                self.focused_param = None;
                            }
                        }
                        if tb.take_change() {
                            if let Some(val) = tb.get_value_string() {
                                p.1 = val;
                            }
                        }
                        return true;
                    }
                }
            } else if p.2.starts_with("spinbox") {
                if let Some(sb) = &mut self.spinboxes[i] {
                    if sb.mouse_input(button, state, px, py, ctx) {
                        p.1 = sb.value.to_string();
                        if sb.editing {
                            self.focused_param = Some(i);
                        } else {
                            if self.focused_param == Some(i) {
                                self.focused_param = None;
                            }
                        }
                        return true;
                    }
                }
            } else if p.2 == "toggle" || p.2 == "checkbox" {
                if let Some(cb) = &mut self.checkboxes[i] {
                    if cb.mouse_input(button, state, px, py, ctx) {
                        if cb.take_change() {
                            if let Some(val) = cb.get_value_string() {
                                p.1 = val;
                            }
                        }
                        return true;
                    }
                }
            } else if p.2.starts_with("color") || p.2 == "rgb" || p.2 == "rgba" {
                if let Some(c) = &mut self.colors[i] {
                    if c.mouse_input(button, state, px, py, ctx) {
                        if let Some(val) = c.get_value_string() {
                            p.1 = val;
                        }
                        if c.editing {
                            self.focused_param = Some(i);
                        } else {
                            if self.focused_param == Some(i) {
                                self.focused_param = None;
                            }
                        }
                        return true;
                    }
                }
            }
        }

        for &widget_ptr in self.children.iter().rev() {
            let widget = unsafe { &mut *widget_ptr };
            if widget.mouse_input(button, state, px, py, ctx) {
                return true;
            }
            if state == ElementState::Pressed && !widget.hit_test(px, py, ctx) {
                widget.unfocus();
            }
        }

        if button == MouseButton::Left && state == ElementState::Pressed {
            let rects = self.get_param_rects();
            let mut clicked_any_focusable = false;
            for (i, p) in self.display_params.iter_mut().enumerate() {
                if p.2 == "code" {
                    let r = rects[i];
                    if px >= r.0 && px <= r.0 + r.2 && py >= r.1 + 18.0 && py <= r.1 + r.3 {
                        self.focused_param = Some(i);
                        let mut editor = TextEditorState::new(p.1.clone());
                        let click_x = px - (r.0 + 12.0);
                        let click_y = py - (r.1 + 22.0);
                        let line = (click_y / 16.0).floor().max(0.0) as usize;
                        let col = (click_x / 7.2 + 0.5).floor().max(0.0) as usize;
                        editor.cursor_idx = map_2d_to_1d(&editor.buffer, line, col);
                        self.code_editor = Some(editor);
                        clicked_any_focusable = true;
                        break;
                    }
                } else if p.2.starts_with("slider") {
                    let r = rects[i];
                    if py >= r.1 && py <= r.1 + r.3 {
                        if let Some(s) = &mut self.sliders[i] {
                            if s.mouse_input(button, state, px, py, ctx) {
                                if s.editing {
                                    self.focused_param = Some(i);
                                    clicked_any_focusable = true;
                                }
                                break;
                            }
                        }
                    }
                } else if p.2.starts_with("float3") {
                    let r = rects[i];
                    if py >= r.1 && py <= r.1 + r.3 {
                        if let Some(f) = &mut self.float3s[i] {
                            if f.mouse_input(button, state, px, py, ctx) {
                                if f.editing_idx.is_some() {
                                    self.focused_param = Some(i);
                                    clicked_any_focusable = true;
                                }
                                break;
                            }
                        }
                    }
                }
            }
            if !clicked_any_focusable {
                self.unfocus();
            }
            return true;
        }
        false
    }

    fn keyboard_input(&mut self, event: &KeyEvent, ctx: &mut UiContext) -> bool {
        if !self.visible {
            return false;
        }
        for &widget_ptr in &self.children {
            let widget = unsafe { &mut *widget_ptr };
            if widget.keyboard_input(event, ctx) {
                return true;
            }
        }

        if let Some(idx) = self.focused_param {
            if event.state == ElementState::Pressed {
                let p = &mut self.display_params[idx];
                if p.2 == "code" {
                    if let Some(mut editor) = self.code_editor.take() {
                        let mut changed = false;
                        let mut handled = true;
                        let mut should_unfocus = false;
                        match &event.logical_key {
                            Key::Named(NamedKey::Backspace) => {
                                changed = editor.delete_backwards();
                            }
                            Key::Named(NamedKey::Delete) => {
                                changed = editor.delete_forwards();
                            }
                            Key::Named(NamedKey::Enter) => {
                                editor.insert_text("\n");
                                changed = true;
                            }
                            Key::Named(NamedKey::Escape) => {
                                should_unfocus = true;
                            }
                            Key::Named(NamedKey::ArrowLeft) => {
                                editor.move_cursor_left(false);
                            }
                            Key::Named(NamedKey::ArrowRight) => {
                                editor.move_cursor_right(false);
                            }
                            Key::Named(NamedKey::ArrowUp) => {
                                let (line, col) = get_cursor_line_col(&editor.buffer, editor.cursor_idx);
                                if line > 0 {
                                    editor.cursor_idx = map_2d_to_1d(&editor.buffer, line - 1, col);
                                }
                            }
                            Key::Named(NamedKey::ArrowDown) => {
                                let (line, col) = get_cursor_line_col(&editor.buffer, editor.cursor_idx);
                                let total_lines = editor.buffer.split('\n').count();
                                if line + 1 < total_lines {
                                    editor.cursor_idx = map_2d_to_1d(&editor.buffer, line + 1, col);
                                }
                            }
                            Key::Named(NamedKey::Home) => {
                                editor.cursor_idx = get_line_start(&editor.buffer, editor.cursor_idx);
                            }
                            Key::Named(NamedKey::End) => {
                                editor.cursor_idx = get_line_end(&editor.buffer, editor.cursor_idx);
                            }
                            Key::Character(s) => {
                                if event.ctrl {
                                    match s.to_lowercase().as_str() {
                                        "f" => {
                                            editor.move_cursor_right(false);
                                        }
                                        "b" => {
                                            editor.move_cursor_left(false);
                                        }
                                        "p" => {
                                            let (line, col) = get_cursor_line_col(&editor.buffer, editor.cursor_idx);
                                            if line > 0 {
                                                editor.cursor_idx = map_2d_to_1d(&editor.buffer, line - 1, col);
                                            }
                                        }
                                        "n" => {
                                            let (line, col) = get_cursor_line_col(&editor.buffer, editor.cursor_idx);
                                            let total_lines = editor.buffer.split('\n').count();
                                            if line + 1 < total_lines {
                                                editor.cursor_idx = map_2d_to_1d(&editor.buffer, line + 1, col);
                                            }
                                        }
                                        "a" => {
                                            editor.cursor_idx = get_line_start(&editor.buffer, editor.cursor_idx);
                                        }
                                        "e" => {
                                            editor.cursor_idx = get_line_end(&editor.buffer, editor.cursor_idx);
                                        }
                                        "d" => {
                                            changed = editor.delete_forwards();
                                        }
                                        "h" => {
                                            changed = editor.delete_backwards();
                                        }
                                        "k" => {
                                            let current_idx = editor.cursor_idx;
                                            let end_idx = get_line_end(&editor.buffer, current_idx);
                                            let chars: Vec<char> = editor.buffer.chars().collect();
                                            if chars.is_empty() {
                                                // do nothing
                                            } else if current_idx < chars.len() {
                                                let delete_end = if chars[current_idx] == '\n' {
                                                    current_idx + 1
                                                } else {
                                                    end_idx
                                                };
                                                let mut new_buf = String::new();
                                                for i in 0..current_idx {
                                                    new_buf.push(chars[i]);
                                                }
                                                for i in delete_end..chars.len() {
                                                    new_buf.push(chars[i]);
                                                }
                                                editor.buffer = new_buf;
                                                changed = true;
                                            }
                                        }
                                        _ => {
                                            handled = false;
                                        }
                                    }
                                } else {
                                    editor.insert_text(s);
                                    changed = true;
                                }
                            }
                            _ => {
                                handled = false;
                            }
                        }
                        if changed {
                            p.1 = editor.buffer.clone();
                        }
                        if should_unfocus {
                            p.1 = editor.buffer;
                            self.focused_param = None;
                            self.code_editor = None;
                        } else {
                            self.code_editor = Some(editor);
                        }
                        if handled {
                            return true;
                        }
                    }
                } else if p.2 == "text" {
                    if let Some(tb) = &mut self.texts[idx] {
                        if tb.keyboard_input(event, ctx) {
                            if !tb.editing {
                                p.1 = tb.text.clone();
                                self.focused_param = None;
                            } else {
                                p.1 = tb.edit_buffer.clone();
                            }
                            return true;
                        }
                    }
                } else if p.2.starts_with("choice") {
                    if let Some(d) = &mut self.choices[idx] {
                        if d.keyboard_input(event, ctx) {
                            if !d.open {
                                if let Some(val) = d.get_value_string() {
                                    p.1 = val;
                                }
                                self.focused_param = None;
                            }
                            return true;
                        }
                    }
                } else if p.2.starts_with("spinbox") {
                    if let Some(sb) = &mut self.spinboxes[idx] {
                        if sb.keyboard_input(event, ctx) {
                            if !sb.editing {
                                p.1 = sb.value.to_string();
                                self.focused_param = None;
                            } else {
                                p.1 = sb.edit_buffer.clone();
                            }
                            return true;
                        }
                    }
                } else if p.2.starts_with("slider") {
                    if let Some(s) = &mut self.sliders[idx] {
                        if s.keyboard_input(event, ctx) {
                            let (min, max) = parse_slider_range(&p.2);
                            let new_val = min + s.value * (max - min);
                            p.1 = format!("{:.2}", new_val);
                            if !s.editing {
                                self.focused_param = None;
                            }
                            return true;
                        }
                    }
                } else if p.2.starts_with("color") || p.2 == "rgb" || p.2 == "rgba" {
                    if let Some(c) = &mut self.colors[idx] {
                        if c.keyboard_input(event, ctx) {
                            if let Some(val) = c.get_value_string() {
                                p.1 = val;
                            }
                            if !c.editing {
                                self.focused_param = None;
                            }
                            return true;
                        }
                    }
                } else if p.2.starts_with("float3") {
                    if let Some(f) = &mut self.float3s[idx] {
                        if f.keyboard_input(event, ctx) {
                            let (min, max) = parse_slider_range(&p.2);
                            let val0 = min + f.values[0] * (max - min);
                            let val1 = min + f.values[1] * (max - min);
                            let val2 = min + f.values[2] * (max - min);
                            p.1 = format!("{:.2}:{:.2}:{:.2}", val0, val1, val2);
                            if f.editing_idx.is_none() {
                                self.focused_param = None;
                            }
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if !self.visible {
            return false;
        }
        for &widget_ptr in self.children.iter().rev() {
            let widget = unsafe { &mut *widget_ptr };
            if widget.mouse_wheel(delta, px, py, ctx) {
                return true;
            }
        }

        let mut changed = false;
        let rects = self.get_param_rects();
        for (i, p) in self.display_params.iter_mut().enumerate() {
            if p.2.starts_with("slider") {
                let r = rects[i];
                let row_y = r.1;
                if py >= row_y - 2.0 && py <= row_y + r.3 && px >= self.base.x && px <= self.base.x + self.base.w {
                    if let Some(s) = &mut self.sliders[i] {
                        let was_scroll = s.scroll_enabled;
                        s.set_scroll(true);
                        if s.mouse_wheel(delta, px, py, ctx) {
                            let (min, max) = parse_slider_range(&p.2);
                            let new_val = min + s.value * (max - min);
                            let old_val = &p.1;
                            let new_val_str = format!("{:.2}", new_val);
                            if *old_val != new_val_str {
                                p.1 = new_val_str;
                                changed = true;
                            }
                        }
                        s.set_scroll(was_scroll);
                    }
                }
            } else if p.2.starts_with("float3") {
                let r = rects[i];
                let row_y = r.1;
                if py >= row_y && py <= row_y + r.3 && px >= self.base.x && px <= self.base.x + self.base.w {
                    if let Some(f) = &mut self.float3s[i] {
                        let rects_inner = f.get_row_rects();
                        for j in 0..3 {
                            let r_inner = rects_inner[j];
                            if py >= r_inner.1 && py <= r_inner.1 + r_inner.3 {
                                let scroll_amount = match delta {
                                    MouseScrollDelta::LineDelta(_x, y) => *y,
                                    MouseScrollDelta::PixelDelta(pos) => (pos.y as f32) / 120.0,
                                };
                                let step = 0.02;
                                let new_val = (f.values[j] - scroll_amount * step).clamp(0.0, 1.0);
                                if (new_val - f.values[j]).abs() > 0.0001 {
                                    f.values[j] = new_val;
                                    if f.editing_idx == Some(j) {
                                        let scaled_val = f.mins[j] + f.values[j] * (f.maxs[j] - f.mins[j]);
                                        f.edit_buffer = format!("{:.2}", scaled_val);
                                    }
                                    let (min, max) = parse_slider_range(&p.2);
                                    let val0 = min + f.values[0] * (max - min);
                                    let val1 = min + f.values[1] * (max - min);
                                    let val2 = min + f.values[2] * (max - min);
                                    let new_val_str = format!("{:.2}:{:.2}:{:.2}", val0, val1, val2);
                                    if p.1 != new_val_str {
                                        p.1 = new_val_str;
                                        changed = true;
                                    }
                                }
                            }
                        }
                    }
                }
            } else if p.2.starts_with("spinbox") {
                let r = rects[i];
                let row_y = r.1;
                if py >= row_y && py <= row_y + r.3 && px >= self.base.x && px <= self.base.x + self.base.w {
                    if let Some(sb) = &mut self.spinboxes[i] {
                        let scroll_amount = match delta {
                            MouseScrollDelta::LineDelta(_x, y) => *y as i32,
                            MouseScrollDelta::PixelDelta(pos) => {
                                let dy = pos.y;
                                if dy > 0.0 { 1 } else if dy < 0.0 { -1 } else { 0 }
                            }
                        };
                        let new_val = (sb.value + scroll_amount * sb.step).clamp(sb.min, sb.max);
                        if sb.value != new_val {
                            sb.value = new_val;
                            p.1 = new_val.to_string();
                            changed = true;
                        }
                    }
                }
            }
        }

        if !changed && self.hit_test(px, py, ctx) {
            let scroll_speed = 24.0;
            let dy = match delta {
                MouseScrollDelta::LineDelta(_, y) => -y * scroll_speed,
                MouseScrollDelta::PixelDelta(pos) => -pos.y as f32,
            };
            let old_scroll = self.scroll_y;
            let max_scroll = (self.content_h - self.base.h).max(0.0);
            self.scroll_y = (self.scroll_y + dy).clamp(0.0, max_scroll);
            if (self.scroll_y - old_scroll).abs() > 0.01 {
                self.update_slider_rects();
                changed = true;
            }
        }

        changed
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.visible {
            return Vec::new();
        }
        let mut quads = Vec::new();
        let rects = self.get_param_rects();
        let view_min = self.base.y + 4.0;
        let view_max = self.base.y + self.base.h - 4.0;

        let clip_quad = |q: (f32, f32, f32, f32, [f32; 4])| -> Option<(f32, f32, f32, f32, [f32; 4])> {
            let (qx, qy, qw, qh, qc) = q;
            let y1 = qy.max(view_min);
            let y2 = (qy + qh).min(view_max);
            if y1 < y2 {
                Some((qx, y1, qw, y2 - y1, qc))
            } else {
                None
            }
        };

        let mut param_quads = Vec::new();

        // Find sections and their ranges
        let mut sections = Vec::new();
        let mut current_section: Option<(usize, usize)> = None;
        let mut in_section = false;
        for (i, p) in self.display_params.iter().enumerate() {
            if p.2 == "section" {
                if let Some((start, end)) = current_section {
                    sections.push((start, end));
                }
                current_section = None;
                in_section = true;
            } else {
                if in_section {
                    if let Some((_, ref mut end)) = current_section {
                        *end = i;
                    } else {
                        current_section = Some((i, i));
                    }
                }
            }
        }
        if let Some((start, end)) = current_section {
            sections.push((start, end));
        }

        // Draw section border boxes
        for (start, end) in sections {
            if start <= end && start < rects.len() && end < rects.len() {
                let r_start = rects[start];
                let r_end = rects[end];
                let bx = self.base.x + 4.0;
                let bw = self.base.w - 8.0;
                let by = r_start.1 - 4.0;
                let bh = (r_end.1 + r_end.3 + 4.0) - by;
                
                let border_color = [0.18, 0.18, 0.27, 1.0];
                let border_t = 1.0;
                
                // Top border
                param_quads.push((bx, by, bw, border_t, border_color));
                // Bottom border
                param_quads.push((bx, by + bh - border_t, bw, border_t, border_color));
                // Left border
                param_quads.push((bx, by, border_t, bh, border_color));
                // Right border
                param_quads.push((bx + bw - border_t, by, border_t, bh, border_color));
            }
        }

        for (i, p) in self.display_params.iter().enumerate() {
            let r = rects[i];
            if p.2.starts_with("slider") {
                if let Some(s) = &self.sliders[i] {
                    let (sx, sy, sw, sh) = s.rect();
                    param_quads.push((sx, sy, sw, sh, s.color()));
                    param_quads.extend(s.extra_quads());
                }
            } else if p.2 == "section" {
                // Section header line is handled by the border box top border now
            } else if p.2.starts_with("float3") {
                if let Some(f) = &self.float3s[i] {
                    param_quads.extend(f.extra_quads());
                }
            } else if p.2 == "code" {
                param_quads.push((r.0, r.1 + 18.0, r.2, r.3 - 18.0, [0.08, 0.08, 0.10, 1.0]));
                let border_color = if self.focused_param == Some(i) {
                    [0.25, 0.45, 0.85, 1.0]
                } else {
                    [0.20, 0.20, 0.25, 1.0]
                };
                let (bx, by, bw, bh) = (r.0, r.1 + 18.0, r.2, r.3 - 18.0);
                param_quads.push((bx, by, bw, 1.0, border_color));
                param_quads.push((bx, by + bh - 1.0, bw, 1.0, border_color));
                param_quads.push((bx, by, 1.0, bh, border_color));
                param_quads.push((bx + bw - 1.0, by, 1.0, bh, border_color));
                if self.focused_param == Some(i) {
                    if let Some(ref editor) = self.code_editor {
                        let (cursor_l, cursor_c) = get_cursor_line_col(&editor.buffer, editor.cursor_idx);
                        let cursor_x = r.0 + 12.0 + (cursor_c as f32 * 7.2);
                        let cursor_y = r.1 + 22.0 + (cursor_l as f32 * 16.0) + (16.0 - 13.0) / 2.0;
                        if cursor_y >= r.1 + 18.0 && cursor_y + 13.0 <= r.1 + r.3 {
                            param_quads.push((cursor_x, cursor_y, 1.5, 13.0, [0.80, 0.80, 0.85, 1.0]));
                        }
                    }
                }
            } else if p.2 == "text" {
                if let Some(tb) = &self.texts[i] {
                    param_quads.extend(tb.extra_quads());
                }
            } else if p.2.starts_with("choice") {
                if let Some(d) = &self.choices[i] {
                    param_quads.extend(d.extra_quads());
                }
            } else if p.2 == "button" {
                if let Some(b) = &self.buttons[i] {
                    param_quads.extend(b.extra_quads());
                }
            } else if p.2.starts_with("spinbox") {
                if let Some(sb) = &self.spinboxes[i] {
                    { let (bx, by, bw, bh) = sb.rect(); param_quads.push((bx, by, bw, bh, sb.color())); }
                    param_quads.extend(sb.extra_quads());
                }
            } else if p.2 == "toggle" || p.2 == "checkbox" {
                if let Some(cb) = &self.checkboxes[i] {
                    param_quads.extend(cb.extra_quads());
                }
            } else if p.2.starts_with("color") || p.2 == "rgb" || p.2 == "rgba" {
                if let Some(c) = &self.colors[i] {
                    param_quads.extend(c.extra_quads());
                }
            }
        }

        for &child_ptr in &self.children {
            let child = unsafe { &*child_ptr };
            if child.visible() {
                param_quads.extend(collect_child_quads(child));
            }
        }

        // Clip all parameter quads vertically
        for q in param_quads {
            if let Some(clipped) = clip_quad(q) {
                quads.push(clipped);
            }
        }

        // Draw Scrollbar (unclipped) if content_h > base.h
        if self.content_h > self.base.h {
            let sb_w = crate::layout::scrollbar_width();
            let sb_x = self.base.x + self.base.w - sb_w - 4.0;
            let sb_track_h = self.base.h - 8.0;
            let sb_track_y = self.base.y + 4.0;

            // Track
            quads.push((sb_x, sb_track_y, sb_w, sb_track_h, crate::color::scrollbar_track_color()));

            // Thumb
            let visible_ratio = self.base.h / self.content_h;
            let thumb_h = if sb_track_h <= 20.0 {
                sb_track_h
            } else {
                (sb_track_h * visible_ratio).clamp(20.0, sb_track_h)
            };
            let max_scroll = self.content_h - self.base.h;
            let scroll_ratio = if max_scroll > 0.0 { self.scroll_y / max_scroll } else { 0.0 };
            let thumb_y = sb_track_y + scroll_ratio * (sb_track_h - thumb_h);
            quads.push((sb_x, thumb_y, sb_w, thumb_h, crate::color::scrollbar_thumb_color()));
        }

        quads
    }

    fn all_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.visible {
            return Vec::new();
        }
        let mut quads = self.extra_quads();
        if let Some(hq) = self.highlight_quad(ctx) {
            if hq.4 != colors::HIGHLIGHT_SECONDARY {
                quads.push(hq);
            }
        }
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        if !self.visible {
            return Vec::new();
        }
        let view_min = self.base.y + 4.0;
        let view_max = self.base.y + self.base.h - 4.0;
        let mut labels = Vec::new();
        for l in self.own_text_labels() {
            if l.y >= view_min - 20.0 && l.y <= view_max + 20.0 {
                labels.push(l);
            }
        }
        for &child_ptr in &self.children {
            let widget = unsafe { &*child_ptr };
            labels.extend(widget.text_labels());
        }
        labels
    }

    fn text_labels_with_font_and_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        if !self.visible {
            return Vec::new();
        }
        let view_min = self.base.y + 4.0;
        let view_max = self.base.y + self.base.h - 4.0;
        let mut result = Vec::new();
        let font = self.widget_font();
        let rects = self.get_param_rects();
        for l in self.own_text_labels() {
            if l.y < view_min - 20.0 || l.y > view_max + 20.0 {
                continue;
            }
            let mut bounds = Some([self.base.x + 4.0, view_min, self.base.x + self.base.w - 4.0, view_max]);
            let mut label_font = font.clone();
            for (i, p) in self.display_params.iter().enumerate() {
                if p.2 == "code" {
                    let r = rects[i];
                    if l.y >= r.1 + 18.0 && l.y <= r.1 + r.3 {
                        let code_min = (r.1 + 19.0).max(view_min);
                        let code_max = (r.1 + r.3 - 1.0).min(view_max);
                        if code_min < code_max {
                            bounds = Some([r.0 + 1.0, code_min, r.0 + r.2 - 1.0, code_max]);
                        } else {
                            bounds = Some([0.0, 0.0, 0.0, 0.0]); // hidden
                        }
                        label_font = Some("monospace".to_string());
                        break;
                    }
                }
            }
            result.push((l, label_font, bounds));
        }
        for &child_ptr in &self.children {
            let widget = unsafe { &*child_ptr };
            result.extend(widget.text_labels_with_font_and_bounds(ctx));
        }
        result
    }

    fn popover_rect(&self) -> Option<(f32, f32, f32, f32)> {
        if !self.visible {
            return None;
        }
        for d_opt in &self.choices {
            if let Some(d) = d_opt {
                if let Some(r) = d.popover_rect() {
                    return Some(r);
                }
            }
        }
        for &widget_ptr in self.children.iter().rev() {
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
        for d_opt in &self.choices {
            if let Some(d) = d_opt {
                d.render_popover(pc);
            }
        }
        for &widget_ptr in self.children.iter().rev() {
            let widget = unsafe { &*widget_ptr };
            widget.render_popover(pc);
        }
    }

    fn tick(&mut self, dt: f32, ctx: &mut UiContext) -> bool {
        if !self.visible {
            return false;
        }
        let mut changed = false;
        for &widget_ptr in &self.children {
            let widget = unsafe { &mut *widget_ptr };
            if widget.tick(dt, ctx) {
                changed = true;
            }
        }
        for cb_opt in &mut self.checkboxes {
            if let Some(cb) = cb_opt {
                if cb.tick(dt, ctx) {
                    changed = true;
                }
            }
        }
        changed
    }
}

fn collect_child_quads(widget: &dyn Element) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
    let mut quads = Vec::new();
    let c = widget.color();
    let (wx, wy, ww, wh) = widget.rect();
    let has_bg = c[3].abs() > 0.001;
    let extra = widget.extra_quads();
    let already_has_bg = extra.iter().any(|q| {
        (q.0 - wx).abs() < 0.1 && (q.1 - wy).abs() < 0.1 && (q.2 - ww).abs() < 0.1 && (q.3 - wh).abs() < 0.1
    });
    if has_bg && !already_has_bg {
        quads.push((wx, wy, ww, wh, c));
    }
    quads.extend(extra);
    let dummy_ctx = UiContext::new();
    for &child_ptr in &widget.children(&dummy_ctx) {
        let child = unsafe { &*child_ptr };
        if child.visible() {
            quads.extend(collect_child_quads(child));
        }
    }
    quads
}

impl ParamController for ParametersBg {
    fn node_params(&self) -> Vec<(String, String, String)> {
        self.display_params.clone()
    }

    fn set_display_params(&mut self, params: &[(String, String, String)]) {
        let mut layout_changed = self.display_params.len() != params.len();
        if !layout_changed {
            for (p_old, p_new) in self.display_params.iter().zip(params.iter()) {
                if p_old.0 != p_new.0 || p_old.2 != p_new.2 {
                    layout_changed = true;
                    break;
                }
            }
        }

        if layout_changed {
            self.scroll_y = 0.0;
            self.display_params = params.to_vec();
            self.focused_param = None;
            self.sliders = self.display_params.iter().map(|p| {
                if p.2.starts_with("slider") {
                    let val = p.1.parse::<f32>().unwrap_or(0.0);
                    let (min, max) = parse_slider_range(&p.2);
                    let t = if max - min != 0.0 {
                        ((val - min) / (max - min)).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    Some(Slider::new().with_value(t).with_range(min, max).with_readout(true).with_label(&p.0))
                } else {
                    None
                }
            }).collect();
            self.float3s = self.display_params.iter().map(|p| {
                if p.2.starts_with("float3") {
                    let (min, max) = parse_slider_range(&p.2);
                    let vals = parse_float3_value(&p.1, min, max);
                    Some(Float3::new().with_values(vals).with_range(min, max).with_label(&p.0))
                } else {
                    None
                }
            }).collect();
            self.spinboxes = self.display_params.iter().map(|p| {
                if p.2.starts_with("spinbox") {
                    let (min, max, step) = parse_spinbox_range(&p.2);
                    let val = p.1.parse::<i32>().unwrap_or(min);
                    Some(Spinbox::new(val, min, max, step).with_label(&p.0))
                } else {
                    None
                }
            }).collect();
            self.buttons = self.display_params.iter().map(|p| {
                if p.2 == "button" {
                    Some(Button::new(0.0, 0.0, 0.0, 0.0).with_label(&p.0))
                } else {
                    None
                }
            }).collect();
            self.choices = self.display_params.iter().map(|p| {
                if p.2.starts_with("choice:") {
                    let options_str = p.2.strip_prefix("choice:").unwrap_or("");
                    let options: Vec<String> = options_str.split(',').map(|s| s.to_string()).collect();
                    let selected = options.iter().position(|o| o == &p.1).unwrap_or(0);
                    Some(Dropdown::new(options, selected).with_label(&p.0))
                } else {
                    None
                }
            }).collect();
            self.texts = self.display_params.iter().map(|p| {
                if p.2 == "text" {
                    Some(TextBox::new(p.1.clone()).with_label(&p.0))
                } else {
                    None
                }
            }).collect();
            self.checkboxes = self.display_params.iter().map(|p| {
                if p.2 == "toggle" || p.2 == "checkbox" {
                    let checked = p.1.trim().to_lowercase() == "true";
                    let mut cb = Checkbox::new().with_label(&p.0);
                    cb.set_checked(checked);
                    Some(cb)
                } else {
                    None
                }
            }).collect();
            self.colors = self.display_params.iter().map(|p| {
                if p.2.starts_with("color") || p.2 == "rgb" || p.2 == "rgba" {
                    let col = parse_hex_to_rgb(&p.1).unwrap_or([255, 255, 255]);
                    Some(ColorSelector::new(col).with_label(&p.0))
                } else {
                    None
                }
            }).collect();
        } else {
            for (i, p_new) in params.iter().enumerate() {
                if Some(i) != self.focused_param && Some(i) != self.dragging_param {
                    self.display_params[i].1 = p_new.1.clone();
                    if let Some(ref mut s) = self.sliders[i] {
                        let val = p_new.1.parse::<f32>().unwrap_or(0.0);
                        let (min, max) = parse_slider_range(&p_new.2);
                        let t = if max - min != 0.0 {
                            ((val - min) / (max - min)).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        s.set_value(t);
                    } else if let Some(ref mut f) = self.float3s[i] {
                        let (min, max) = parse_slider_range(&p_new.2);
                        let vals = parse_float3_value(&p_new.1, min, max);
                        f.set_values(vals);
                    } else if let Some(ref mut sb) = self.spinboxes[i] {
                        if !sb.editing {
                            let (min, _max, _step) = parse_spinbox_range(&p_new.2);
                            let val = p_new.1.parse::<i32>().unwrap_or(min);
                            sb.value = val;
                        }
                    } else if let Some(ref mut d) = self.choices[i] {
                        if !d.open {
                            if let Some(options_str) = p_new.2.strip_prefix("choice:") {
                                let options: Vec<String> = options_str.split(',').map(|s| s.to_string()).collect();
                                if d.options != options {
                                    d.options = options.clone();
                                }
                                if let Some(idx) = options.iter().position(|o| o == &p_new.1) {
                                    d.selected = idx;
                                }
                            }
                        }
                    } else if let Some(ref mut tb) = self.texts[i] {
                        if !tb.editing {
                            tb.set_value_string(&p_new.1);
                        }
                    } else if let Some(ref mut cb) = self.checkboxes[i] {
                        let checked = p_new.1.trim().to_lowercase() == "true";
                        cb.set_checked(checked);
                    } else if let Some(ref mut c) = self.colors[i] {
                        if !c.editing {
                            c.set_value_string(&p_new.1);
                        }
                    }
                }
            }
        }
        self.content_h = self.get_total_content_height();
        let max_scroll = (self.content_h - self.base.h).max(0.0);
        self.scroll_y = self.scroll_y.clamp(0.0, max_scroll);
        self.update_slider_rects();
    }
}

fn get_cursor_line_col(buffer: &str, cursor_idx: usize) -> (usize, usize) {
    let mut cur_line = 0;
    let mut cur_col = 0;
    let mut count = 0;
    for c in buffer.chars() {
        if count == cursor_idx {
            return (cur_line, cur_col);
        }
        if c == '\n' {
            cur_line += 1;
            cur_col = 0;
        } else {
            cur_col += 1;
        }
        count += 1;
    }
    (cur_line, cur_col)
}

fn map_2d_to_1d(buffer: &str, line: usize, col: usize) -> usize {
    let mut target_line = line;
    let lines: Vec<Vec<char>> = buffer.split('\n').map(|l| l.chars().collect()).collect();
    if lines.is_empty() {
        return 0;
    }
    if target_line >= lines.len() {
        target_line = lines.len() - 1;
    }
    let mut target_col = col;
    if target_col > lines[target_line].len() {
        target_col = lines[target_line].len();
    }
    let mut index = 0;
    for i in 0..target_line {
        index += lines[i].len() + 1; // +1 for the '\n'
    }
    index += target_col;
    index
}

fn get_line_start(buffer: &str, cursor_idx: usize) -> usize {
    let (line, _) = get_cursor_line_col(buffer, cursor_idx);
    map_2d_to_1d(buffer, line, 0)
}

fn get_line_end(buffer: &str, cursor_idx: usize) -> usize {
    let (line, _) = get_cursor_line_col(buffer, cursor_idx);
    let lines: Vec<Vec<char>> = buffer.split('\n').map(|l| l.chars().collect()).collect();
    if lines.is_empty() {
        return 0;
    }
    let line_len = if line < lines.len() {
        lines[line].len()
    } else {
        lines[lines.len() - 1].len()
    };
    map_2d_to_1d(buffer, line, line_len)
}


