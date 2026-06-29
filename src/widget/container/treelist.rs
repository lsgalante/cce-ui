use crate::widget::*;
use crate::widget::container::scroll_box::ScrollBox;
use crate::widget::display::TextLabel;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub enum TreeElement {
    Section {
        path: String,
        name: String,
        indent: usize,
        collapsed: bool,
    },
    Leaf {
        path: String,
        name: String,
        indent: usize,
        val: serde_json::Value,
        original_idx: usize,
    }
}

#[derive(Debug, Clone)]
enum PathToken {
    Key(String),
    Index(usize),
}

fn parse_path(path: &str) -> Vec<PathToken> {
    let mut tokens = Vec::new();
    for part in path.split('.') {
        if part.is_empty() { continue; }
        if let Some(bracket_idx) = part.find('[') {
            let name = &part[..bracket_idx];
            if !name.is_empty() {
                tokens.push(PathToken::Key(name.to_string()));
            }
            let mut rest = &part[bracket_idx..];
            while let Some(start) = rest.find('[') {
                if let Some(end) = rest.find(']') {
                    let idx_str = &rest[start + 1..end];
                    if let Ok(idx) = idx_str.parse::<usize>() {
                        tokens.push(PathToken::Index(idx));
                    }
                    rest = &rest[end + 1..];
                } else {
                    break;
                }
            }
        } else {
            tokens.push(PathToken::Key(part.to_string()));
        }
    }
    tokens
}

fn build_tree(flat_keys: &[(String, serde_json::Value)], collapsed_sections: &HashSet<String>) -> Vec<TreeElement> {
    let mut items = Vec::new();
    let mut seen_prefixes = HashSet::new();

    for (original_idx, (key_path, val)) in flat_keys.iter().enumerate() {
        let tokens = parse_path(key_path);
        let mut current_prefix = String::new();
        let mut is_hidden = false;
        
        for i in 0..tokens.len() {
            let token = &tokens[i];
            let part_name = match token {
                PathToken::Key(k) => {
                    if current_prefix.is_empty() {
                        current_prefix = k.clone();
                    } else {
                        current_prefix = format!("{}.{}", current_prefix, k);
                    }
                    k.clone()
                }
                PathToken::Index(idx) => {
                    let s = format!("[{}]", idx);
                    current_prefix = format!("{}{}", current_prefix, s);
                    s
                }
            };

            let is_last = i == tokens.len() - 1;
            
            if is_hidden {
                continue;
            }

            if is_last {
                items.push(TreeElement::Leaf {
                    path: key_path.clone(),
                    name: part_name,
                    indent: i,
                    val: val.clone(),
                    original_idx,
                });
            } else {
                if !seen_prefixes.contains(&current_prefix) {
                    seen_prefixes.insert(current_prefix.clone());
                    let collapsed = collapsed_sections.contains(&current_prefix);
                    items.push(TreeElement::Section {
                        path: current_prefix.clone(),
                        name: part_name,
                        indent: i,
                        collapsed,
                    });
                }
                if collapsed_sections.contains(&current_prefix) {
                    is_hidden = true;
                }
            }
        }
    }
    items
}

#[derive(Debug, Clone)]
pub struct TreeList {
    pub base: Widget,
    pub scroll_box: ScrollBox,
    pub flat_keys: Vec<(String, serde_json::Value)>,
    pub collapsed_sections: HashSet<String>,
    pub items: Vec<TreeElement>,
    pub selected_key_idx: Option<usize>,
    pub hovered_row_idx: Option<usize>,
    pub item_height: f32,
    pub clicked_item: Option<TreeElement>,
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub children: Vec<*mut (dyn Element + 'static)>,
}

impl TreeList {
    pub fn new() -> Self {
        Self {
            base: Widget::new(),
            scroll_box: ScrollBox::new(),
            flat_keys: Vec::new(),
            collapsed_sections: HashSet::new(),
            items: Vec::new(),
            selected_key_idx: None,
            hovered_row_idx: None,
            item_height: 28.0,
            clicked_item: None,
            parent: None,
            children: Vec::new(),
        }
    }

    pub fn set_flat_keys(&mut self, flat_keys: Vec<(String, serde_json::Value)>) {
        self.flat_keys = flat_keys;
        self.rebuild_tree();
    }

    pub fn rebuild_tree(&mut self) {
        self.items = build_tree(&self.flat_keys, &self.collapsed_sections);
        let content_h = self.items.len() as f32 * self.item_height;
        let (_, _, _, h) = self.rect();
        self.scroll_box.update_bounds(content_h, self.scroll_box.viewport_y, h);
    }

    pub fn get_row_rect(&self, original_idx: usize) -> Option<(f32, f32, f32, f32)> {
        let list_top = self.scroll_box.viewport_y;
        let list_bottom = self.scroll_box.viewport_y + self.scroll_box.viewport_h;
        
        let row_idx = self.items.iter().position(|item| {
            match item {
                TreeElement::Leaf { original_idx: idx, .. } => *idx == original_idx,
                _ => false,
            }
        })?;

        let row_y = list_top + row_idx as f32 * self.item_height - self.scroll_box.scroll_y;
        if row_y >= list_top && row_y + self.item_height <= list_bottom {
            Some((self.scroll_box.base.x, row_y, self.scroll_box.base.w, self.item_height))
        } else {
            None
        }
    }

    pub fn take_clicked_item(&mut self) -> Option<TreeElement> {
        self.clicked_item.take()
    }
}

impl Element for TreeList {
    crate::impl_widget_base!(TreeList);

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.base.x = x;
        self.base.y = y;
        self.base.w = w;
        self.base.h = h;
        self.scroll_box.set_rect(x, y, w, h);
        
        let content_h = self.items.len() as f32 * self.item_height;
        self.scroll_box.update_bounds(content_h, y, h);
    }

    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn focus(&mut self) {
        focus::set_focused(self);
    }

    fn unfocus(&mut self) {}

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        let mut changed = self.scroll_box.mouse_input(button, state, px, py, ctx);

        let list_left = self.scroll_box.base.x;
        let list_width = self.scroll_box.base.w;
        let list_top = self.scroll_box.viewport_y;
        let list_bottom = self.scroll_box.viewport_y + self.scroll_box.viewport_h;

        if button == MouseButton::Left && state == ElementState::Pressed {
            if px >= list_left && px <= list_left + list_width && py >= list_top && py <= list_bottom {
                self.focus();
                let relative_y = py - list_top + self.scroll_box.scroll_y;
                let row_idx = (relative_y / self.item_height) as usize;
                if row_idx < self.items.len() {
                    let item = self.items[row_idx].clone();
                    match item {
                        TreeElement::Section { ref path, .. } => {
                            if self.collapsed_sections.contains(path) {
                                self.collapsed_sections.remove(path);
                            } else {
                                self.collapsed_sections.insert(path.clone());
                            }
                            self.rebuild_tree();
                            self.clicked_item = Some(TreeElement::Section {
                                path: path.clone(),
                                name: String::new(),
                                indent: 0,
                                collapsed: self.collapsed_sections.contains(path),
                            });
                            changed = true;
                        }
                        TreeElement::Leaf { original_idx, ref path, ref name, indent, ref val } => {
                            self.selected_key_idx = Some(original_idx);
                            self.clicked_item = Some(TreeElement::Leaf {
                                path: path.clone(),
                                name: name.clone(),
                                indent,
                                val: val.clone(),
                                original_idx,
                            });
                            changed = true;
                        }
                    }
                }
            }
        }
        changed
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        let mut changed = self.scroll_box.on_cursor_moved(px, py, ctx);

        let list_left = self.scroll_box.base.x;
        let list_width = self.scroll_box.base.w;
        let list_top = self.scroll_box.viewport_y;
        let list_bottom = self.scroll_box.viewport_y + self.scroll_box.viewport_h;

        let old_hovered = self.hovered_row_idx;
        self.hovered_row_idx = None;

        if px >= list_left && px <= list_left + list_width && py >= list_top && py <= list_bottom {
            let relative_y = py - list_top + self.scroll_box.scroll_y;
            let row_idx = (relative_y / self.item_height) as usize;
            if row_idx < self.items.len() {
                self.hovered_row_idx = Some(row_idx);
            }
        }

        if old_hovered != self.hovered_row_idx {
            changed = true;
        }
        changed
    }

    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        self.scroll_box.mouse_wheel(delta, px, py, ctx)
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = self.scroll_box.extra_quads();
        
        if focus::is_focused(self) {
            let focus_color = [0.30, 0.50, 0.32, 1.0];
            for i in 1..=4 {
                if i < quads.len() {
                    quads[i].4 = focus_color;
                }
            }
        }

        let list_left = self.scroll_box.base.x;
        let list_width = self.scroll_box.base.w;
        let list_top = self.scroll_box.viewport_y;
        let list_bottom = self.scroll_box.viewport_y + self.scroll_box.viewport_h;

        for (i, item) in self.items.iter().enumerate() {
            let row_y = list_top + i as f32 * self.item_height - self.scroll_box.scroll_y;
            if row_y + self.item_height < list_top || row_y > list_bottom {
                continue;
            }
            
            let draw_y = row_y.max(list_top);
            let draw_bottom = (row_y + self.item_height).min(list_bottom);
            let draw_h = draw_bottom - draw_y;
            if draw_h <= 0.0 { continue; }
            
            let bg_color = match item {
                TreeElement::Section { .. } => {
                    if Some(i) == self.hovered_row_idx {
                        [0.10, 0.12, 0.18, 1.0]
                    } else {
                        [0.07, 0.07, 0.09, 1.0]
                    }
                }
                TreeElement::Leaf { original_idx, .. } => {
                    if Some(*original_idx) == self.selected_key_idx {
                        [0.15, 0.20, 0.30, 1.0]
                    } else if Some(i) == self.hovered_row_idx {
                        [0.12, 0.12, 0.16, 1.0]
                    } else if i % 2 == 0 {
                        [0.09, 0.09, 0.11, 1.0]
                    } else {
                        [0.08, 0.08, 0.10, 1.0]
                    }
                }
            };
            
            quads.push((list_left + 1.0, draw_y, list_width - 9.0, draw_h, bg_color));
            
            if row_y + self.item_height <= list_bottom {
                quads.push((list_left + 1.0, row_y + self.item_height - 1.0, list_width - 9.0, 1.0, [0.13, 0.13, 0.17, 1.0]));
            }
        }

        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        let list_left = self.scroll_box.base.x;
        let list_top = self.scroll_box.viewport_y;
        let list_bottom = self.scroll_box.viewport_y + self.scroll_box.viewport_h;

        for (i, item) in self.items.iter().enumerate() {
            let row_y = list_top + i as f32 * self.item_height - self.scroll_box.scroll_y;
            if row_y + self.item_height < list_top || row_y > list_bottom {
                continue;
            }

            match item {
                TreeElement::Section { name, indent, collapsed, .. } => {
                    let display_text = format!("{} {}", if *collapsed { "▶" } else { "▼" }, name);
                    labels.push(TextLabel {
                        text: display_text,
                        x: list_left + 8.0 + *indent as f32 * 12.0,
                        y: row_y + 6.0,
                        font_size: 12.0,
                        color: [0x61, 0xaf, 0xef],
                    });
                }
                TreeElement::Leaf { name, indent, val, original_idx, .. } => {
                    let val_str = serde_json::to_string(val).unwrap_or_default();
                    let display_val = if val_str.len() > 18 {
                        format!("{}...", &val_str[..15])
                    } else {
                        val_str
                    };

                    let color = if Some(*original_idx) == self.selected_key_idx {
                        [0x7d, 0xff, 0xff]
                    } else {
                        [0xcc, 0xcc, 0xd4]
                    };

                    labels.push(TextLabel {
                        text: name.clone(),
                        x: list_left + 8.0 + *indent as f32 * 12.0,
                        y: row_y + 6.0,
                        font_size: 12.0,
                        color,
                    });

                    let val_ty = match val {
                        serde_json::Value::Bool(_) => Some("bool"),
                        serde_json::Value::Number(num) => {
                            if num.is_f64() {
                                Some("f64")
                            } else {
                                Some("i64")
                            }
                        }
                        serde_json::Value::String(s) => {
                            if s.starts_with('#') {
                                Some("color")
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };
                    if let Some(ty) = val_ty {
                        let ty_text = format!("({})", ty);
                        labels.push(TextLabel {
                            text: ty_text,
                            x: list_left + 190.0,
                            y: row_y + 6.0,
                            font_size: 12.0,
                            color: [0xc6, 0x78, 0xdd],
                        });
                    }

                    if Some(*original_idx) != self.selected_key_idx {
                        labels.push(TextLabel {
                            text: display_val,
                            x: list_left + 245.0,
                            y: row_y + 6.0,
                            font_size: 12.0,
                            color: [0x83, 0x83, 0x8a],
                        });
                    }
                }
            }
        }
        labels
    }

    fn text_labels_with_bounds(&self, _ctx: &UiContext) -> Vec<(TextLabel, Option<[f32; 4]>)> {
        let bounds = Some([self.scroll_box.base.x, self.scroll_box.viewport_y, self.scroll_box.base.x + self.scroll_box.base.w, self.scroll_box.viewport_y + self.scroll_box.viewport_h]);
        self.text_labels().into_iter().map(|l| (l, bounds)).collect()
    }

    fn text_labels_with_font_and_bounds(&self, _ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        let font = self.widget_font();
        let bounds = Some([self.scroll_box.base.x, self.scroll_box.viewport_y, self.scroll_box.base.x + self.scroll_box.base.w, self.scroll_box.viewport_y + self.scroll_box.viewport_h]);
        self.text_labels().into_iter().map(|l| (l, font.clone(), bounds)).collect()
    }

    fn parent(&self, _ctx: &UiContext) -> Option<*mut (dyn Element + 'static)> {
        self.parent
    }

    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>, _ctx: &mut UiContext) {
        self.parent = parent;
    }

    fn children(&self, _ctx: &UiContext) -> Vec<*mut (dyn Element + 'static)> {
        self.children.clone()
    }

    fn add_child(&mut self, child: *mut (dyn Element + 'static), _ctx: &mut UiContext) {
        self.children.push(child);
    }

    fn clear_children(&mut self, _ctx: &mut UiContext) {
        self.children.clear();
    }
}

unsafe impl Send for TreeList {}
unsafe impl Sync for TreeList {}
