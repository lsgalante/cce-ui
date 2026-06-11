use crate::colors;
use crate::widget::*;
use crate::widget::input::{Slider, Spinbox};
use crate::widget::display::{Float3, TextLabel};

pub struct ParametersBg {
    pub base: Widget,
    display_params: Vec<(String, String, String)>,
    dragging_param: Option<usize>,
    pub focused_param: Option<usize>,
    mouse_pos: Option<(f32, f32)>,
    sliders: Vec<Option<Slider>>,
    float3s: Vec<Option<Float3>>,
    spinboxes: Vec<Option<Spinbox>>,
    visible: bool,
    pub children: Vec<*mut (dyn Element + 'static)>,
    pub parent: Option<*mut (dyn Element + 'static)>,
}

impl ParametersBg {
    pub fn new() -> Self {
        Self {
            base: Widget::new(),
            display_params: Vec::new(),
            dragging_param: None,
            focused_param: None,
            mouse_pos: None,
            sliders: Vec::new(),
            float3s: Vec::new(),
            spinboxes: Vec::new(),
            visible: true,
            children: Vec::new(),
            parent: None,
        }
    }

    pub fn get_param_rects(&self) -> Vec<(f32, f32, f32, f32)> {
        let mut rects = Vec::new();
        let mut cur_y = self.base.y + 30.0;
        for p in &self.display_params {
            let h = if p.2 == "code" {
                200.0
            } else if p.2 == "section" {
                24.0
            } else if p.2.starts_with("float3") {
                108.0
            } else if p.2 == "text" {
                24.0
            } else {
                20.0
            };
            rects.push((self.base.x + 8.0, cur_y, self.base.w - 16.0, h));
            cur_y += h + 8.0;
        }
        rects
    }

    fn update_slider_rects(&mut self) {
        let rects = self.get_param_rects();
        for (i, s_opt) in self.sliders.iter_mut().enumerate() {
            if let Some(s) = s_opt {
                let r = rects[i];
                let track_x = self.base.x + 100.0;
                let track_w = (self.base.w - 100.0 - 20.0).max(10.0);
                let track_y = r.1 + 4.0;
                let track_h = 12.0;
                s.set_rect(track_x, track_y, track_w, track_h);
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
                let box_x = self.base.x + 100.0;
                let box_w = (self.base.w - 100.0 - 16.0).max(10.0);
                sb.set_rect(box_x, r.1, box_w, r.3);
            }
        }
    }

    fn own_text_labels(&self) -> Vec<TextLabel> {
        let rects = self.get_param_rects();
        let mut labels = Vec::new();
        for (i, (name, value, ptype)) in self.display_params.iter().enumerate() {
            let r = rects[i];
            if ptype.starts_with("slider") {
                labels.push(TextLabel {
                    text: name.clone(),
                    x: self.base.x + 8.0,
                    y: r.1,
                    font_size: 12.0,
                    color: [0xaa, 0xaa, 0xbb],
                });
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
                    text: format!("{}:\n{}", name, value),
                    x: self.base.x + 12.0,
                    y: r.1,
                    font_size: 12.0,
                    color: [0xaa, 0xaa, 0xbb],
                });
            } else if ptype.starts_with("spinbox") {
                labels.push(TextLabel {
                    text: name.clone(),
                    x: self.base.x + 8.0,
                    y: r.1 + (r.3 - 12.0) / 2.0 - 2.0,
                    font_size: 12.0,
                    color: [0xaa, 0xaa, 0xbb],
                });
                if let Some(sb) = &self.spinboxes[i] {
                    labels.extend(sb.text_labels());
                }
            } else if ptype == "text" {
                labels.push(TextLabel {
                    text: name.clone(),
                    x: self.base.x + 8.0,
                    y: r.1 + (r.3 - 12.0) / 2.0 - 2.0,
                    font_size: 12.0,
                    color: [0xaa, 0xaa, 0xbb],
                });
                let val_text = if self.focused_param == Some(i) {
                    format!("{}|", value)
                } else {
                    value.clone()
                };
                labels.push(TextLabel {
                    text: val_text,
                    x: self.base.x + 106.0,
                    y: r.1 + (r.3 - 12.0) / 2.0 - 2.0,
                    font_size: 12.0,
                    color: [0xee, 0xee, 0xf0],
                });
            } else if ptype.starts_with("choice") {
                labels.push(TextLabel {
                    text: name.clone(),
                    x: self.base.x + 8.0,
                    y: r.1 + (r.3 - 12.0) / 2.0 - 2.0,
                    font_size: 12.0,
                    color: [0xaa, 0xaa, 0xbb],
                });
                labels.push(TextLabel {
                    text: value.clone(),
                    x: self.base.x + 106.0,
                    y: r.1 + (r.3 - 12.0) / 2.0 - 2.0,
                    font_size: 12.0,
                    color: [0xee, 0xee, 0xf0],
                });
            } else if ptype == "button" {
                labels.push(TextLabel {
                    text: name.clone(),
                    x: self.base.x + 8.0,
                    y: r.1 + (r.3 - 12.0) / 2.0 - 2.0,
                    font_size: 12.0,
                    color: [0xaa, 0xaa, 0xbb],
                });
                labels.push(TextLabel {
                    text: "Trigger".to_string(),
                    x: self.base.x + 106.0,
                    y: r.1 + (r.3 - 12.0) / 2.0 - 2.0,
                    font_size: 12.0,
                    color: [0xee, 0xee, 0xf0],
                });
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

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if let Some(b) = self.base_mut() {
            b.x = x;
            b.y = y;
            b.w = w;
            b.h = h;
        }
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
        px >= self.base.x && px <= self.base.x + self.base.w && py >= self.base.y && py <= self.base.y + self.base.h
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
        self.dragging_param.is_some() 
            || self.display_params.iter().any(|p| p.2.starts_with("slider") || p.2.starts_with("float3"))
    }

    fn is_dragging(&self) -> bool {
        self.dragging_param.is_some()
    }

    fn drag_begin(&mut self, px: f32, py: f32) {
        let rects = self.get_param_rects();
        for (i, p) in self.display_params.iter().enumerate() {
            if p.2.starts_with("slider") {
                let r = rects[i];
                let row_y = r.1;
                if py >= row_y - 2.0 && py <= row_y + 18.0 {
                    if let Some(s) = &mut self.sliders[i] {
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
            ctx.layout_tree.parents.remove(&id);
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

        for &widget_ptr in self.children.iter().rev() {
            let widget = unsafe { &mut *widget_ptr };
            if widget.popover_rect().is_some() {
                if widget.mouse_input(button, state, px, py, ctx) {
                    return true;
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
                        clicked_any_focusable = true;
                        break;
                    }
                } else if p.2 == "text" {
                    let box_x = self.base.x + 100.0;
                    let box_w = (self.base.w - 100.0 - 16.0).max(10.0);
                    let r = rects[i];
                    if px >= box_x && px <= box_x + box_w && py >= r.1 && py <= r.1 + r.3 {
                        self.focused_param = Some(i);
                        clicked_any_focusable = true;
                        break;
                    }
                } else if p.2.starts_with("choice") {
                    let box_x = self.base.x + 100.0;
                    let box_w = (self.base.w - 100.0 - 16.0).max(10.0);
                    let r = rects[i];
                    if px >= box_x && px <= box_x + box_w && py >= r.1 && py <= r.1 + r.3 {
                        if let Some(options_str) = p.2.strip_prefix("choice:") {
                            let options: Vec<&str> = options_str.split(',').collect();
                            if !options.is_empty() {
                                let cur_idx = options.iter().position(|&o| o == p.1).unwrap_or(0);
                                let next_idx = (cur_idx + 1) % options.len();
                                p.1 = options[next_idx].to_string();
                                return true;
                            }
                        }
                    }
                } else if p.2 == "button" {
                    let box_x = self.base.x + 100.0;
                    let box_w = (self.base.w - 100.0 - 16.0).max(10.0);
                    let r = rects[i];
                    if px >= box_x && px <= box_x + box_w && py >= r.1 && py <= r.1 + r.3 {
                        p.1 = "clicked".to_string();
                        return true;
                    }
                } else if p.2.starts_with("spinbox") {
                    if let Some(sb) = &mut self.spinboxes[i] {
                        if sb.mouse_input(button, state, px, py, ctx) {
                            p.1 = sb.value.to_string();
                            if sb.editing {
                                self.focused_param = Some(i);
                            } else {
                                self.unfocus();
                            }
                            return true;
                        }
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
                    match &event.logical_key {
                        Key::Named(NamedKey::Backspace) => {
                            if !p.1.is_empty() {
                                p.1.pop();
                                return true;
                            }
                        }
                        Key::Named(NamedKey::Enter) => {
                            p.1.push('\n');
                            return true;
                        }
                        Key::Named(NamedKey::Escape) => {
                            self.focused_param = None;
                            return true;
                        }
                        Key::Character(s) => {
                            p.1.push_str(s);
                            return true;
                        }
                        _ => {}
                    }
                } else if p.2 == "text" {
                    match &event.logical_key {
                        Key::Named(NamedKey::Backspace) => {
                            if !p.1.is_empty() {
                                p.1.pop();
                                return true;
                            }
                        }
                        Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Escape) => {
                            self.focused_param = None;
                            return true;
                        }
                        Key::Character(s) => {
                            p.1.push_str(s);
                            return true;
                        }
                        _ => {}
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
                if py >= row_y - 2.0 && py <= row_y + 18.0 && px >= self.base.x && px <= self.base.x + self.base.w {
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
        changed
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.visible {
            return Vec::new();
        }
        let mut quads = Vec::new();
        let rects = self.get_param_rects();

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
                quads.push((bx, by, bw, border_t, border_color));
                // Bottom border
                quads.push((bx, by + bh - border_t, bw, border_t, border_color));
                // Left border
                quads.push((bx, by, border_t, bh, border_color));
                // Right border
                quads.push((bx + bw - border_t, by, border_t, bh, border_color));
            }
        }

        for (i, p) in self.display_params.iter().enumerate() {
            let r = rects[i];
            if p.2.starts_with("slider") {
                if let Some(s) = &self.sliders[i] {
                    let (sx, sy, sw, sh) = s.rect();
                    quads.push((sx, sy, sw, sh, s.color()));
                    quads.extend(s.extra_quads());
                }
            } else if p.2 == "section" {
                // Section header line is handled by the border box top border now
            } else if p.2.starts_with("float3") {
                if let Some(f) = &self.float3s[i] {
                    quads.extend(f.extra_quads());
                }
            } else if p.2 == "code" {
                quads.push((r.0, r.1 + 18.0, r.2, r.3 - 18.0, [0.08, 0.08, 0.10, 1.0]));
                let border_color = if self.focused_param == Some(i) {
                    [0.25, 0.45, 0.85, 1.0]
                } else {
                    [0.20, 0.20, 0.25, 1.0]
                };
                let (bx, by, bw, bh) = (r.0, r.1 + 18.0, r.2, r.3 - 18.0);
                quads.push((bx, by, bw, 1.0, border_color));
                quads.push((bx, by + bh - 1.0, bw, 1.0, border_color));
                quads.push((bx, by, 1.0, bh, border_color));
                quads.push((bx + bw - 1.0, by, 1.0, bh, border_color));
            } else if p.2 == "text" {
                let box_x = self.base.x + 100.0;
                let box_w = (self.base.w - 100.0 - 16.0).max(10.0);
                quads.push((box_x, r.1, box_w, r.3, [0.08, 0.08, 0.10, 1.0]));
                let border_color = if self.focused_param == Some(i) {
                    [0.25, 0.45, 0.85, 1.0]
                } else {
                    [0.20, 0.20, 0.25, 1.0]
                };
                let (bx, by, bw, bh) = (box_x, r.1, box_w, r.3);
                let border_t = 1.0;
                quads.push((bx, by, bw, border_t, border_color));
                quads.push((bx, by + bh - border_t, bw, border_t, border_color));
                quads.push((bx, by, border_t, bh, border_color));
                quads.push((bx + bw - border_t, by, border_t, bh, border_color));
            } else if p.2.starts_with("choice") {
                let box_x = self.base.x + 100.0;
                let box_w = (self.base.w - 100.0 - 16.0).max(10.0);
                quads.push((box_x, r.1, box_w, r.3, [0.08, 0.08, 0.10, 1.0]));
                let border_color = [0.20, 0.20, 0.25, 1.0];
                let (bx, by, bw, bh) = (box_x, r.1, box_w, r.3);
                let border_t = 1.0;
                quads.push((bx, by, bw, border_t, border_color));
                quads.push((bx, by + bh - border_t, bw, border_t, border_color));
                quads.push((bx, by, border_t, bh, border_color));
                quads.push((bx + bw - border_t, by, border_t, bh, border_color));
            } else if p.2 == "button" {
                let box_x = self.base.x + 100.0;
                let box_w = (self.base.w - 100.0 - 16.0).max(10.0);
                quads.push((box_x, r.1, box_w, r.3, [0.15, 0.22, 0.38, 1.0]));
                let border_color = [0.25, 0.35, 0.58, 1.0];
                let (bx, by, bw, bh) = (box_x, r.1, box_w, r.3);
                let border_t = 1.0;
                quads.push((bx, by, bw, border_t, border_color));
                quads.push((bx, by + bh - border_t, bw, border_t, border_color));
                quads.push((bx, by, border_t, bh, border_color));
                quads.push((bx + bw - border_t, by, border_t, bh, border_color));
            } else if p.2.starts_with("spinbox") {
                if let Some(sb) = &self.spinboxes[i] {
                    quads.push((sb.base.x, sb.base.y, sb.base.w, sb.base.h, sb.color()));
                    quads.extend(sb.extra_quads());
                }
            }
        }

        for &child_ptr in &self.children {
            let child = unsafe { &*child_ptr };
            if child.visible() {
                quads.extend(collect_child_quads(child));
            }
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
        let mut labels = self.own_text_labels();
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
        let mut result = Vec::new();
        let font = self.widget_font();
        for l in self.own_text_labels() {
            result.push((l, font.clone(), None));
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
                    Some(Slider::new().with_value(t).with_range(min, max).with_readout(true))
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
                    Some(Spinbox::new(val, min, max, step))
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
                    }
                }
            }
        }
        self.update_slider_rects();
    }
}

