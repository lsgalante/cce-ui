use crate::widget::{
    Element, Widget, Checkbox, Button, Label, Spinbox, ColorSelector, TextLabel, Paginator,
    KeyEvent, MouseButton, ElementState, focus, Slider,
};
use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
pub struct JsonWidgetConfig {
    #[serde(rename = "type")]
    pub widget_type: String,
    pub text: String,
    pub id: Option<String>,
    pub checked: Option<bool>,
    pub value: Option<i32>,
    pub min: Option<i32>,
    pub max: Option<i32>,
    pub step: Option<i32>,
    pub decimals: Option<u32>,
    pub color: Option<[u8; 3]>,
    pub value_f32: Option<f32>,
    pub min_f32: Option<f32>,
    pub max_f32: Option<f32>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct JsonPageConfig {
    pub title: String,
    pub widgets: Vec<JsonWidgetConfig>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct JsonLayoutConfig {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub widgets: Option<Vec<JsonWidgetConfig>>,
    pub pages: Option<Vec<JsonPageConfig>>,
}

pub struct JsonWidget {
    pub id: String,
    pub widget_type: String,
    pub text: String,
    pub checkbox: Option<Checkbox>,
    pub button: Option<Button>,
    pub label: Option<Label>,
    pub spinbox: Option<Spinbox>,
    pub color_selector: Option<ColorSelector>,
    pub slider: Option<Slider>,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub label_text: Option<TextLabel>,
    pub page_idx: usize,
}

pub struct JsonLayoutWidget {
    base: Widget,
    pub widgets: Vec<JsonWidget>,
    pub paginator: Option<Paginator>,
    pub dragging_slider_idx: Option<usize>,
    pub page_scroll_y: Vec<f32>,
    pub page_total_heights: Vec<f32>,
}

impl JsonLayoutWidget {
    pub fn new(config: &JsonLayoutConfig) -> Self {
        let mut widgets = Vec::new();
        let mut page_titles = Vec::new();
        let mut paginator = None;

        if let Some(ref pages_conf) = config.pages {
            for (page_idx, page) in pages_conf.iter().enumerate() {
                page_titles.push(page.title.clone());
                for (idx, w_conf) in page.widgets.iter().enumerate() {
                    let id = w_conf.id.clone().unwrap_or_else(|| format!("widget_{}_{}", page_idx, idx));
                    let widget_type = w_conf.widget_type.clone();
                    let text = w_conf.text.clone();
                    let mut checkbox = None;
                    let mut button = None;
                    let mut label = None;
                    let mut spinbox = None;
                    let mut color_selector = None;
                    let mut slider = None;

                    match widget_type.as_str() {
                        "checkbox" => {
                            let mut cb = Checkbox::new();
                            if let Some(ch) = w_conf.checked {
                                cb.set_checked(ch);
                            }
                            checkbox = Some(cb);
                        }
                        "button" => {
                            button = Some(Button::new(0.0, 0.0, 0.0, 0.0).with_label(&text));
                        }
                        "label" => {
                            label = Some(Label::new(&text).with_font_size(13.0).with_color([0xcc, 0xcc, 0xd4]));
                        }
                        "spinbox" => {
                            let min_val = w_conf.min.unwrap_or(0);
                            let max_val = w_conf.max.unwrap_or(100);
                            let step_val = w_conf.step.unwrap_or(1);
                            let mut sb = Spinbox::new(w_conf.value.unwrap_or(0), min_val, max_val, step_val)
                                .with_label(&text);
                            if let Some(dec) = w_conf.decimals {
                                sb = sb.with_decimals(dec);
                            }
                            spinbox = Some(sb);
                        }
                        "color" => {
                            let col = w_conf.color.unwrap_or([255, 255, 255]);
                            let cs = ColorSelector::new(col).with_label(&text);
                            color_selector = Some(cs);
                        }
                        "slider" => {
                            let min_val = w_conf.min_f32.unwrap_or(0.0);
                            let max_val = w_conf.max_f32.unwrap_or(1.0);
                            let mut sl = Slider::new()
                                .with_range(min_val, max_val)
                                .with_label(&text)
                                .with_readout(true);
                            if let Some(val) = w_conf.value_f32 {
                                let pct = if max_val > min_val { (val - min_val) / (max_val - min_val) } else { 0.0 };
                                sl = sl.with_value(pct);
                            }
                            slider = Some(sl);
                        }
                        _ => {}
                    }

                    widgets.push(JsonWidget {
                        id,
                        widget_type,
                        text,
                        checkbox,
                        button,
                        label,
                        spinbox,
                        color_selector,
                        slider,
                        x: 0.0,
                        y: 0.0,
                        w: 0.0,
                        h: 0.0,
                        label_text: None,
                        page_idx,
                    });
                }
            }
            paginator = Some(Paginator::new(56.0, page_titles));
        } else if let Some(ref widgets_conf) = config.widgets {
            for (idx, w_conf) in widgets_conf.iter().enumerate() {
                let id = w_conf.id.clone().unwrap_or_else(|| format!("widget_{}", idx));
                let widget_type = w_conf.widget_type.clone();
                let text = w_conf.text.clone();
                let mut checkbox = None;
                let mut button = None;
                let mut label = None;
                let mut spinbox = None;
                let mut color_selector = None;
                let mut slider = None;

                match widget_type.as_str() {
                    "checkbox" => {
                        let mut cb = Checkbox::new();
                        if let Some(ch) = w_conf.checked {
                            cb.set_checked(ch);
                        }
                        checkbox = Some(cb);
                    }
                    "button" => {
                        button = Some(Button::new(0.0, 0.0, 0.0, 0.0).with_label(&text));
                    }
                    "label" => {
                        label = Some(Label::new(&text).with_font_size(13.0).with_color([0xcc, 0xcc, 0xd4]));
                    }
                    "spinbox" => {
                        let min_val = w_conf.min.unwrap_or(0);
                        let max_val = w_conf.max.unwrap_or(100);
                        let step_val = w_conf.step.unwrap_or(1);
                        let mut sb = Spinbox::new(w_conf.value.unwrap_or(0), min_val, max_val, step_val)
                            .with_label(&text);
                        if let Some(dec) = w_conf.decimals {
                            sb = sb.with_decimals(dec);
                        }
                        spinbox = Some(sb);
                    }
                    "color" => {
                        let col = w_conf.color.unwrap_or([255, 255, 255]);
                        let cs = ColorSelector::new(col).with_label(&text);
                        color_selector = Some(cs);
                    }
                    "slider" => {
                        let min_val = w_conf.min_f32.unwrap_or(0.0);
                        let max_val = w_conf.max_f32.unwrap_or(1.0);
                        let mut sl = Slider::new()
                            .with_range(min_val, max_val)
                            .with_label(&text)
                            .with_readout(true);
                        if let Some(val) = w_conf.value_f32 {
                            let pct = if max_val > min_val { (val - min_val) / (max_val - min_val) } else { 0.0 };
                            sl = sl.with_value(pct);
                        }
                        slider = Some(sl);
                    }
                    _ => {}
                }

                widgets.push(JsonWidget {
                    id,
                    widget_type,
                    text,
                    checkbox,
                    button,
                    label,
                    spinbox,
                    color_selector,
                    slider,
                    x: 0.0,
                    y: 0.0,
                    w: 0.0,
                    h: 0.0,
                    label_text: None,
                    page_idx: 0,
                });
            }
        }

        Self {
            base: Widget::new(),
            widgets,
            paginator,
            dragging_slider_idx: None,
            page_scroll_y: vec![0.0; 16],
            page_total_heights: vec![0.0; 16],
        }
    }

    pub fn layout_children(&mut self) {
        let (bx, by, bw, bh) = self.rect();
        if let Some(paginator) = &mut self.paginator {
            paginator.set_rect(bx, by, bw, bh);
        }

        let has_paginator = self.paginator.is_some();
        let pad_x = if has_paginator { 76.0 } else { 16.0 };
        let usable_w = if has_paginator { bw - pad_x - 16.0 } else { bw - 2.0 * 16.0 };
        
        let mut page_current_y = vec![16.0; 16]; // support up to 16 pages
        let spacing = 12.0;

        for w_state in &mut self.widgets {
            let p_idx = w_state.page_idx;
            if p_idx >= page_current_y.len() {
                continue;
            }
            let current_y = &mut page_current_y[p_idx];
            w_state.x = bx + pad_x;

            let mut top_room = 0.0;
            if let Some(sb) = &w_state.spinbox {
                top_room = crate::widget::label_offset(sb);
            } else if let Some(cs) = &w_state.color_selector {
                top_room = crate::widget::label_offset(cs);
            } else if let Some(cb) = &w_state.checkbox {
                top_room = crate::widget::label_offset(cb);
            } else if let Some(btn) = &w_state.button {
                top_room = crate::widget::label_offset(btn);
            } else if let Some(lbl) = &w_state.label {
                top_room = crate::widget::label_offset(lbl);
            } else if let Some(sl) = &w_state.slider {
                top_room = crate::widget::label_offset(sl);
            }

            let scroll_offset = self.page_scroll_y.get(p_idx).cloned().unwrap_or(0.0);
            w_state.y = by + *current_y - scroll_offset;
            w_state.w = usable_w;

            if let Some(cb) = &mut w_state.checkbox {
                cb.set_rect(w_state.x, w_state.y + 2.0, 18.0, 18.0);
                w_state.h = 22.0;
                w_state.label_text = Some(TextLabel {
                    text: w_state.text.clone(),
                    x: w_state.x + 28.0,
                    y: w_state.y + 2.0,
                    font_size: 13.0,
                    color: [0xcc, 0xcc, 0xd4],
                });
            } else if let Some(btn) = &mut w_state.button {
                let btn_h = 24.0 + top_room;
                btn.set_rect(w_state.x, w_state.y, usable_w, btn_h);
                w_state.h = btn_h;
            } else if let Some(lbl) = &mut w_state.label {
                let lbl_h = 18.0 + top_room;
                lbl.set_rect(w_state.x, w_state.y, usable_w, lbl_h);
                w_state.h = lbl_h;
            } else if let Some(sb) = &mut w_state.spinbox {
                let sb_h = 22.0 + top_room;
                sb.set_rect(w_state.x, w_state.y, usable_w, sb_h);
                w_state.h = sb_h;
            } else if let Some(cs) = &mut w_state.color_selector {
                let cs_h = 24.0 + top_room;
                cs.set_rect(w_state.x, w_state.y, usable_w, cs_h);
                w_state.h = cs_h;
            } else if let Some(sl) = &mut w_state.slider {
                let sl_h = 22.0 + top_room;
                sl.set_rect(w_state.x, w_state.y, usable_w, sl_h);
                w_state.h = sl_h;
            }

            *current_y += w_state.h + spacing;
        }

        // Store total height of each page (adding a little padding at the end)
        for (i, &height) in page_current_y.iter().enumerate() {
            if i < self.page_total_heights.len() {
                self.page_total_heights[i] = height + 4.0;
            }
        }
    }
}

impl Element for JsonLayoutWidget {
    fn base(&self) -> Option<&Widget> { Some(&self.base) }
    fn base_mut(&mut self) -> Option<&mut Widget> { Some(&mut self.base) }
    fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if let Some(b) = self.base_mut() {
            b.x = x;
            b.y = y;
            b.w = w;
            b.h = h;
        }
        self.layout_children();
    }

    fn tick(&mut self, dt: f32) -> bool {
        let mut changed = false;
        if let Some(paginator) = &mut self.paginator {
            if paginator.tick(dt) {
                changed = true;
            }
        }
        let active_page = self.paginator.as_ref().map(|p| p.selected_page()).unwrap_or(0);
        for w in &mut self.widgets {
            if w.page_idx != active_page {
                continue;
            }
            if let Some(sb) = &mut w.spinbox {
                if sb.tick(dt) {
                    changed = true;
                }
            } else if let Some(cs) = &mut w.color_selector {
                if cs.tick(dt) {
                    changed = true;
                }
            }
        }
        changed
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        if let Some(paginator) = &self.paginator {
            quads.extend(paginator.extra_quads());
            if let Some(hq) = paginator.highlight_quad() {
                quads.push(hq);
            }
        }

        let active_page = self.paginator.as_ref().map(|p| p.selected_page()).unwrap_or(0);
        let (bx, by, bw, bh) = self.rect();
        let has_paginator = self.paginator.is_some();
        let pad_x = if has_paginator { 76.0 } else { 16.0 };
        let min_x = bx + pad_x - 4.0;
        let max_x = bx + bw;
        let min_y = by;
        let max_y = by + bh;

        let push_clipped = |qx: f32, qy: f32, qw: f32, qh: f32, qc: [f32; 4], q: &mut Vec<(f32, f32, f32, f32, [f32; 4])>| {
            let rx1 = qx.max(min_x);
            let ry1 = qy.max(min_y);
            let rx2 = (qx + qw).min(max_x);
            let ry2 = (qy + qh).min(max_y);
            let rw = rx2 - rx1;
            let rh = ry2 - ry1;
            if rw > 0.0 && rh > 0.0 {
                q.push((rx1, ry1, rw, rh, qc));
            }
        };

        for w in &self.widgets {
            if w.page_idx != active_page {
                continue;
            }
            if let Some(cb) = &w.checkbox {
                push_clipped(cb.rect().0, cb.rect().1, cb.rect().2, cb.rect().3, cb.color(), &mut quads);
                for q in cb.extra_quads() {
                    push_clipped(q.0, q.1, q.2, q.3, q.4, &mut quads);
                }
                if let Some(hq) = cb.highlight_quad() {
                    push_clipped(hq.0, hq.1, hq.2, hq.3, hq.4, &mut quads);
                }
            } else if let Some(btn) = &w.button {
                push_clipped(btn.rect().0, btn.rect().1, btn.rect().2, btn.rect().3, btn.color(), &mut quads);
                for q in btn.extra_quads() {
                    push_clipped(q.0, q.1, q.2, q.3, q.4, &mut quads);
                }
                if let Some(hq) = btn.highlight_quad() {
                    push_clipped(hq.0, hq.1, hq.2, hq.3, hq.4, &mut quads);
                }
            } else if let Some(lbl) = &w.label {
                push_clipped(lbl.rect().0, lbl.rect().1, lbl.rect().2, lbl.rect().3, lbl.color(), &mut quads);
                for q in lbl.extra_quads() {
                    push_clipped(q.0, q.1, q.2, q.3, q.4, &mut quads);
                }
            } else if let Some(sb) = &w.spinbox {
                push_clipped(sb.rect().0, sb.rect().1, sb.rect().2, sb.rect().3, sb.color(), &mut quads);
                for q in sb.extra_quads() {
                    push_clipped(q.0, q.1, q.2, q.3, q.4, &mut quads);
                }
            } else if let Some(cs) = &w.color_selector {
                push_clipped(cs.rect().0, cs.rect().1, cs.rect().2, cs.rect().3, cs.color(), &mut quads);
                for q in cs.extra_quads() {
                    push_clipped(q.0, q.1, q.2, q.3, q.4, &mut quads);
                }
            } else if let Some(sl) = &w.slider {
                push_clipped(sl.rect().0, sl.rect().1, sl.rect().2, sl.rect().3, sl.color(), &mut quads);
                for q in sl.extra_quads() {
                    push_clipped(q.0, q.1, q.2, q.3, q.4, &mut quads);
                }
            }
        }
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        if let Some(paginator) = &self.paginator {
            labels.extend(paginator.text_labels());
        }

        let active_page = self.paginator.as_ref().map(|p| p.selected_page()).unwrap_or(0);

        for w in &self.widgets {
            if w.page_idx != active_page {
                continue;
            }
            if let Some(_cb) = &w.checkbox {
                if let Some(tl) = &w.label_text {
                    labels.push(tl.clone());
                }
            } else if let Some(btn) = &w.button {
                labels.extend(btn.text_labels());
            } else if let Some(lbl) = &w.label {
                labels.extend(lbl.text_labels());
            } else if let Some(sb) = &w.spinbox {
                labels.extend(sb.text_labels());
            } else if let Some(cs) = &w.color_selector {
                labels.extend(cs.text_labels());
            } else if let Some(sl) = &w.slider {
                labels.extend(sl.text_labels());
            }
        }
        labels
    }

    fn text_labels_with_bounds(&self) -> Vec<(TextLabel, Option<[f32; 4]>)> {
        let mut labels = Vec::new();
        let (bx, by, bw, bh) = self.rect();
        if let Some(paginator) = &self.paginator {
            for l in paginator.text_labels() {
                labels.push((l, None));
            }
        }

        let active_page = self.paginator.as_ref().map(|p| p.selected_page()).unwrap_or(0);
        let has_paginator = self.paginator.is_some();
        let pad_x = if has_paginator { 76.0 } else { 16.0 };
        let content_bounds = Some([bx + pad_x - 4.0, by, bx + bw, by + bh]);

        for w in &self.widgets {
            if w.page_idx != active_page {
                continue;
            }
            let w_labels = if let Some(_cb) = &w.checkbox {
                if let Some(tl) = &w.label_text {
                    vec![tl.clone()]
                } else {
                    Vec::new()
                }
            } else if let Some(btn) = &w.button {
                btn.text_labels()
            } else if let Some(lbl) = &w.label {
                lbl.text_labels()
            } else if let Some(sb) = &w.spinbox {
                sb.text_labels()
            } else if let Some(cs) = &w.color_selector {
                cs.text_labels()
            } else if let Some(sl) = &w.slider {
                sl.text_labels()
            } else {
                Vec::new()
            };
            for l in w_labels {
                labels.push((l, content_bounds));
            }
        }
        labels
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32) -> bool {
        let mut changed = false;
        if let Some(paginator) = &mut self.paginator {
            if paginator.on_cursor_moved(px, py) {
                changed = true;
            }
        }

        if let Some(idx) = self.dragging_slider_idx {
            if let Some(w) = self.widgets.get_mut(idx) {
                if let Some(ref mut sl) = w.slider {
                    if sl.drag_update(px, py) {
                        changed = true;
                    }
                }
            }
        }

        let active_page = self.paginator.as_ref().map(|p| p.selected_page()).unwrap_or(0);

        for (idx, w) in self.widgets.iter_mut().enumerate() {
            if w.page_idx != active_page {
                continue;
            }
            if let Some(cb) = &mut w.checkbox {
                let was = cb.hovered();
                let hit = px >= w.x && px <= w.x + w.w && py >= w.y && py <= w.y + w.h;
                cb.set_hovered(hit);
                if was != hit {
                    changed = true;
                }
            } else if let Some(btn) = &mut w.button {
                if btn.cursor_moved(px, py) {
                    changed = true;
                }
            } else if let Some(lbl) = &mut w.label {
                if lbl.cursor_moved(px, py) {
                    changed = true;
                }
            } else if let Some(sb) = &mut w.spinbox {
                if sb.cursor_moved(px, py) {
                    changed = true;
                }
            } else if let Some(cs) = &mut w.color_selector {
                if cs.cursor_moved(px, py) {
                    changed = true;
                }
            } else if let Some(sl) = &mut w.slider {
                if sl.on_cursor_moved(px, py) {
                    changed = true;
                }
            }
        }
        changed
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button != MouseButton::Left { return false; }
        let mut changed = false;

        if let Some(paginator) = &mut self.paginator {
            if paginator.mouse_input(button, state, px, py) {
                if paginator.take_click() {
                    focus::clear_focus();
                    self.layout_children();
                }
                changed = true;
                return true;
            }
        }

        if let Some(idx) = self.dragging_slider_idx {
            if state == ElementState::Released {
                if let Some(w) = self.widgets.get_mut(idx) {
                    if let Some(ref mut sl) = w.slider {
                        sl.mouse_input(button, state, px, py);
                        changed = true;
                    }
                }
                self.dragging_slider_idx = None;
            }
        }

        let active_page = self.paginator.as_ref().map(|p| p.selected_page()).unwrap_or(0);

        for (idx, w) in self.widgets.iter_mut().enumerate() {
            if w.page_idx != active_page {
                continue;
            }
            if let Some(cb) = &mut w.checkbox {
                let hit = px >= w.x && px <= w.x + w.w && py >= w.y && py <= w.y + w.h;
                if hit {
                    if state == ElementState::Pressed {
                        changed = true;
                    } else if state == ElementState::Released {
                        let new_checked = !cb.checked();
                        cb.set_checked(new_checked);
                        changed = true;
                    }
                }
            } else if let Some(btn) = &mut w.button {
                if btn.mouse_input(button, state, px, py) {
                    changed = true;
                }
            } else if let Some(sb) = &mut w.spinbox {
                if sb.mouse_input(button, state, px, py) {
                    changed = true;
                }
            } else if let Some(cs) = &mut w.color_selector {
                if cs.mouse_input(button, state, px, py) {
                    changed = true;
                }
            } else if let Some(sl) = &mut w.slider {
                let hit = px >= w.x && px <= w.x + w.w && py >= w.y && py <= w.y + w.h;
                if hit {
                    if state == ElementState::Pressed {
                        sl.mouse_input(button, state, px, py);
                        self.dragging_slider_idx = Some(idx);
                        changed = true;
                    }
                }
            }
        }

        if state == ElementState::Pressed && !changed {
            focus::clear_focus();
        }

        changed
    }

    fn keyboard_input(&mut self, event: &KeyEvent) -> bool {
        let active_page = self.paginator.as_ref().map(|p| p.selected_page()).unwrap_or(0);
        for w in &mut self.widgets {
            if w.page_idx != active_page {
                continue;
            }
            if let Some(sb) = &mut w.spinbox {
                if sb.keyboard_input(event) {
                    return true;
                }
            } else if let Some(cs) = &mut w.color_selector {
                if cs.keyboard_input(event) {
                    return true;
                }
            } else if let Some(sl) = &mut w.slider {
                if sl.keyboard_input(event) {
                    return true;
                }
            }
        }
        false
    }

    fn mouse_wheel(&mut self, delta: &crate::widget::MouseScrollDelta, px: f32, py: f32) -> bool {
        let (bx, by, bw, bh) = self.rect();
        if px >= bx && px <= bx + bw && py >= by && py <= by + bh {
            let active_page = self.paginator.as_ref().map(|p| p.selected_page()).unwrap_or(0);
            if active_page < self.page_total_heights.len() {
                let total_height = self.page_total_heights[active_page];
                let visible_h = bh;
                let max_scroll_y = (total_height - visible_h).max(0.0);
                if max_scroll_y > 0.0 {
                    let scroll_amount = match delta {
                        crate::widget::MouseScrollDelta::LineDelta(_x, y) => *y * 24.0,
                        crate::widget::MouseScrollDelta::PixelDelta(pos) => pos.y as f32,
                    };
                    let old_scroll = self.page_scroll_y[active_page];
                    self.page_scroll_y[active_page] = (old_scroll + scroll_amount).clamp(0.0, max_scroll_y);
                    if (self.page_scroll_y[active_page] - old_scroll).abs() > 0.01 {
                        self.layout_children();
                        return true;
                    }
                }
            }
        }
        false
    }
}
