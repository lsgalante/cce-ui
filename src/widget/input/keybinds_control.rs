use crate::colors;
use crate::widget::*;
use crate::widget::input::text_box::TextBox;
use crate::widget::input::button::Button;
use crate::widget::TextLabel;

#[derive(Clone, Debug)]
pub struct KeybindRow {
    pub key_input: TextBox,
    pub cmd_input: TextBox,
    pub remove_button: Button,
}

impl KeybindRow {
    pub fn new(binding: String, command: String) -> Self {
        let key_input = TextBox::new(binding).with_placeholder("e.g. super+shift+q");
        let cmd_input = TextBox::new(command).with_placeholder("e.g. close");
        let remove_button = Button::new(0.0, 0.0, 0.0, 0.0).with_label("Remove");
        Self {
            key_input,
            cmd_input,
            remove_button,
        }
    }
}

#[derive(Clone, Debug)]
pub struct KeybindsControl {
    pub base: Widget,
    pub rows: Vec<KeybindRow>,
    pub add_button: Button,
    pub just_changed: bool,
}

impl KeybindsControl {
    pub fn new() -> Self {
        let mut kc = Self {
            base: Widget::new(),
            rows: Vec::new(),
            add_button: Button::new(0.0, 0.0, 0.0, 0.0).with_label("Add Keybind +"),
            just_changed: false,
        };
        kc.load_from_config();
        kc
    }

    pub fn load_from_config(&mut self) {
        let content = std::fs::read_to_string("/home/lsgalante/.config/cce/config.json").unwrap_or_default();
        let val: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
        let mut loaded = Vec::new();
        if let Some(arr) = val.get("keybind").and_then(|k| k.as_array()) {
            for v in arr {
                let mods = v.get("mods").and_then(|m| m.as_str()).unwrap_or("").to_string();
                let key = v.get("key").and_then(|k| k.as_str()).unwrap_or("").to_string();
                let action = v.get("action").and_then(|a| a.as_str()).unwrap_or("").to_string();
                let command = v.get("command").and_then(|c| c.as_str()).unwrap_or("").to_string();
                
                let binding = if mods.is_empty() {
                    key
                } else {
                    format!("{}+{}", mods, key)
                };
                let cmd_val = if action == "spawn" {
                    command
                } else {
                    action
                };
                loaded.push((binding, cmd_val));
            }
        }
        self.rows = loaded.into_iter().map(|(binding, cmd_val)| {
            KeybindRow::new(binding, cmd_val)
        }).collect();
    }

    pub fn save_to_config(&self) {
        let path = "/home/lsgalante/.config/cce/config.json";
        let content = std::fs::read_to_string(path).unwrap_or_default();
        let mut val: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
        
        let mut keybinds = Vec::new();
        for row in &self.rows {
            let binding = row.key_input.get_value_string().unwrap_or_default();
            let cmd_val = row.cmd_input.get_value_string().unwrap_or_default();
            
            let (mods, key) = if let Some(last_plus) = binding.rfind('+') {
                (binding[..last_plus].to_string(), binding[last_plus + 1..].to_string())
            } else {
                ("".to_string(), binding)
            };

            let built_in_actions = [
                "expose", "close", "focus-next", "focus-prev", "mode-next",
                "mode-next-shared", "fullscreen", "exit", "reload",
                "view-1", "view-2", "view-3", "view-4",
                "set-tag-1", "set-tag-2", "set-tag-3", "set-tag-4"
            ];
            
            let mut obj = serde_json::Map::new();
            obj.insert("mods".to_string(), serde_json::Value::String(mods));
            obj.insert("key".to_string(), serde_json::Value::String(key));
            
            if built_in_actions.contains(&cmd_val.as_str()) {
                obj.insert("action".to_string(), serde_json::Value::String(cmd_val));
            } else {
                obj.insert("action".to_string(), serde_json::Value::String("spawn".to_string()));
                obj.insert("command".to_string(), serde_json::Value::String(cmd_val));
            }
            
            keybinds.push(serde_json::Value::Object(obj));
        }
        
        if let Some(obj) = val.as_object_mut() {
            obj.insert("keybind".to_string(), serde_json::Value::Array(keybinds));
        }
        
        if let Ok(updated_str) = serde_json::to_string_pretty(&val) {
            let _ = std::fs::write(path, updated_str);
        }
    }
}

fn link_child(parent_ptr: *mut (dyn Element + 'static), parent_id: WidgetId, child: &mut dyn Element, ctx: &mut UiContext) {
    let c_ptr = child.as_ptr();
    if let Some(c_base) = child.base() {
        let c_id = c_base.id();
        ctx.register_widget(parent_id, parent_ptr);
        ctx.register_widget(c_id, c_ptr);
        ctx.layout_tree.parents.insert(c_id, parent_id);
        let children = ctx.layout_tree.children.entry(parent_id).or_default();
        if !children.contains(&c_id) {
            children.push(c_id);
        }
    }
    child.set_parent(Some(parent_ptr), ctx);
}

impl Element for KeybindsControl {
    crate::impl_widget_base!(KeybindsControl);

    fn preferred_height(&self) -> Option<f32> {
        let pad_y = 8.0;
        let gap_between_rows = 8.0;
        let row_h = crate::layout::textbox_height();
        let add_btn_h = 30.0;

        let mut total_h = pad_y * 2.0;
        if !self.rows.is_empty() {
            total_h += self.rows.len() as f32 * row_h + (self.rows.len() - 1) as f32 * gap_between_rows;
            total_h += gap_between_rows;
        }
        total_h += add_btn_h;
        Some(total_h)
    }

    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn layout(&mut self, origin: Point, constraints: LayoutConstraints, ctx: &mut UiContext) {
        let size = self.measure(constraints, ctx);
        self.set_rect(origin.x, origin.y, size.width, size.height);

        let pad_x = 8.0;
        let pad_y = 8.0;
        let gap_between_rows = 8.0;

        let row_h = crate::layout::textbox_height();

        let usable_w = (size.width - 2.0 * pad_x).max(1.0);
        let gap_x = 6.0;
        let row_usable_w = usable_w - 2.0 * gap_x;

        let key_w = row_usable_w * 0.36;
        let cmd_w = row_usable_w * 0.49;
        let remove_w = row_usable_w * 0.15;

        let mut curr_y = origin.y + pad_y;
        let self_ptr = self.as_ptr();
        let self_id = self.base.id();

        for row in &mut self.rows {
            let key_x = origin.x + pad_x;
            row.key_input.layout(Point { x: key_x, y: curr_y }, LayoutConstraints::new(key_w, key_w, row_h, row_h), ctx);

            let cmd_x = key_x + key_w + gap_x;
            row.cmd_input.layout(Point { x: cmd_x, y: curr_y }, LayoutConstraints::new(cmd_w, cmd_w, row_h, row_h), ctx);

            let remove_x = cmd_x + cmd_w + gap_x;
            row.remove_button.layout(Point { x: remove_x, y: curr_y }, LayoutConstraints::new(remove_w, remove_w, row_h, row_h), ctx);

            link_child(self_ptr, self_id, &mut row.key_input, ctx);
            link_child(self_ptr, self_id, &mut row.cmd_input, ctx);
            link_child(self_ptr, self_id, &mut row.remove_button, ctx);

            curr_y += row_h + gap_between_rows;
        }

        let add_btn_w = 120.0f32.min(usable_w);
        let add_btn_h = 30.0;
        let add_x = origin.x + pad_x;
        self.add_button.layout(Point { x: add_x, y: curr_y }, LayoutConstraints::new(add_btn_w, add_btn_w, add_btn_h, add_btn_h), ctx);
        link_child(self_ptr, self_id, &mut self.add_button, ctx);
    }

    fn all_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = self.extra_quads();
        if let Some(hq) = self.highlight_quad(ctx) {
            if hq.4 != colors::HIGHLIGHT_SECONDARY {
                quads.push(hq);
            }
        }

        for row in &self.rows {
            quads.extend(row.key_input.all_quads(ctx));
            quads.extend(row.cmd_input.all_quads(ctx));
            quads.extend(row.remove_button.all_quads(ctx));
        }

        quads.extend(self.add_button.all_quads(ctx));
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        Vec::new()
    }

    fn text_labels_with_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<[f32; 4]>)> {
        let mut labels = Vec::new();
        for row in &self.rows {
            labels.extend(row.key_input.text_labels_with_bounds(ctx));
            labels.extend(row.cmd_input.text_labels_with_bounds(ctx));
            labels.extend(row.remove_button.text_labels_with_bounds(ctx));
        }
        labels.extend(self.add_button.text_labels_with_bounds(ctx));
        labels
    }

    fn text_labels_with_font_and_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        let mut labels = Vec::new();
        let font = self.widget_font();
        for l in self.text_labels() {
            labels.push((l, font.clone(), None));
        }

        for row in &self.rows {
            labels.extend(row.key_input.text_labels_with_font_and_bounds(ctx));
            labels.extend(row.cmd_input.text_labels_with_font_and_bounds(ctx));
            labels.extend(row.remove_button.text_labels_with_font_and_bounds(ctx));
        }

        labels.extend(self.add_button.text_labels_with_font_and_bounds(ctx));
        labels
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut UiContext) -> bool {
        let mut handled = false;

        if self.add_button.handle_event(event, ctx) {
            handled = true;
        }

        let mut row_to_remove = None;
        for (i, row) in self.rows.iter_mut().enumerate() {
            if row.key_input.handle_event(event, ctx) {
                handled = true;
            }
            if row.cmd_input.handle_event(event, ctx) {
                handled = true;
            }
            if row.remove_button.handle_event(event, ctx) {
                handled = true;
            }
            if row.remove_button.take_click() {
                row_to_remove = Some(i);
                handled = true;
            }
        }

        if let Some(idx) = row_to_remove {
            self.rows.remove(idx);
            self.save_to_config();
            self.just_changed = true;
            handled = true;
        }

        if self.add_button.take_click() {
            let new_row = KeybindRow::new("".to_string(), "".to_string());
            self.rows.push(new_row);
            self.save_to_config();
            self.just_changed = true;
            handled = true;
        }

        let mut key_changed = false;
        let mut cmd_changed = false;

        for row in &mut self.rows {
            if row.key_input.take_change() {
                key_changed = true;
            }
            if row.cmd_input.take_change() {
                cmd_changed = true;
            }
        }

        if key_changed || cmd_changed {
            self.save_to_config();
            self.just_changed = true;
        }

        if !handled {
            match event {
                Event::PointerMove { x, y, .. } => {
                    let is_hit = self.hit_test(*x, *y, ctx);
                    let was = self.hovered();
                    self.set_hovered(is_hit);
                    if was != is_hit {
                        handled = true;
                    }
                }
                _ => {}
            }
        }

        handled
    }

    fn take_change(&mut self) -> bool {
        let ret = self.just_changed;
        self.just_changed = false;
        ret
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        self.handle_event(&Event::MouseButton { button, state, x: px, y: py, local_x: px, local_y: py }, ctx)
    }

    fn keyboard_input(&mut self, event: &KeyEvent, ctx: &mut UiContext) -> bool {
        self.handle_event(&Event::KeyInput(event.clone()), ctx)
    }

    fn cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        self.handle_event(&Event::PointerMove { x: px, y: py, local_x: px, local_y: py }, ctx)
    }

    fn unfocus(&mut self) {
        for row in &mut self.rows {
            row.key_input.unfocus();
            row.cmd_input.unfocus();
        }
    }

    fn hit_test(&self, px: f32, py: f32, _ctx: &UiContext) -> bool {
        let (x, y, w, h) = self.rect();
        if w <= 0.0 || h <= 0.0 {
            return false;
        }
        px >= x && px <= x + w && py >= y && py <= y + h
    }
}

impl Control for KeybindsControl {}
