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

fn matches_query(key_path: &str, val: &serde_json::Value, annotation: Option<&str>, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let query_lower = query.to_lowercase();
    if key_path.to_lowercase().contains(&query_lower) {
        return true;
    }
    let val_str = match val {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    };
    if val_str.to_lowercase().contains(&query_lower) {
        return true;
    }
    if let Some(anno) = annotation {
        if anno.to_lowercase().contains(&query_lower) {
            return true;
        }
    }
    false
}

fn build_tree(
    flat_keys: &[(String, serde_json::Value)],
    annotations: &[Option<String>],
    collapsed_sections: &HashSet<String>,
    query: &str,
) -> Vec<TreeElement> {
    let mut items = Vec::new();
    let mut seen_prefixes = HashSet::new();

    let mut matching_indices = HashSet::new();
    for (idx, (key_path, val)) in flat_keys.iter().enumerate() {
        let annotation = annotations.get(idx).and_then(|opt| opt.as_deref());
        if query.is_empty() || matches_query(key_path, val, annotation, query) {
            matching_indices.insert(idx);
        }
    }

    for (original_idx, (key_path, val)) in flat_keys.iter().enumerate() {
        if !matching_indices.contains(&original_idx) {
            continue;
        }
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
    pub search_box: crate::widget::Adapted<TextBox>,
    pub add_key_btn: crate::widget::Adapted<Button>,
    pub add_key_popover_open: bool,
    pub add_key_popover_box: crate::widget::Adapted<TextBox>,
    pub new_key_path_request: Option<String>,
    pub flat_keys: Vec<(String, serde_json::Value)>,
    pub annotations: Vec<Option<String>>,
    pub collapsed_sections: HashSet<String>,
    pub items: Vec<TreeElement>,
    pub selected_key_idx: Option<usize>,
    pub hovered_row_idx: Option<usize>,
    pub item_height: f32,
    pub clicked_item: Option<TreeElement>,
    pub right_clicked_section: Option<String>,
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub children: Vec<*mut (dyn Element + 'static)>,
    pub last_scroll_y: f32,
    pub scrollbar_activity_timer: f32,
    pub deleted_key_path: Option<String>,
    pub edit_box: crate::widget::Adapted<TextBox>,
    pub editing_key_idx: Option<usize>,
    pub double_click_timer: Option<(std::time::Instant, usize)>,
    pub rename_request: Option<(String, String)>,
}

impl TreeList {
    pub fn new() -> Self {
        let mut scroll_box = ScrollBox::new();
        scroll_box.show_background = false;
        Self {
            base: Widget::new(),
            scroll_box,
            search_box: TextBox::new(String::new()).with_placeholder("Search...").with_update_on_type(true),
            add_key_btn: Button::new(0.0, 0.0, 80.0, 26.0).with_label("+ Add Key"),
            add_key_popover_open: false,
            add_key_popover_box: TextBox::new(String::new()).with_placeholder("new.key.path").with_multiline(false),
            new_key_path_request: None,
            flat_keys: Vec::new(),
            annotations: Vec::new(),
            collapsed_sections: HashSet::new(),
            items: Vec::new(),
            selected_key_idx: None,
            hovered_row_idx: None,
            item_height: 28.0,
            clicked_item: None,
            right_clicked_section: None,
            parent: None,
            children: Vec::new(),
            last_scroll_y: 0.0,
            scrollbar_activity_timer: 0.0,
            deleted_key_path: None,
            edit_box: TextBox::new(String::new()).with_multiline(false).with_draw_bg_border(true),
            editing_key_idx: None,
            double_click_timer: None,
            rename_request: None,
        }
    }

    pub fn take_new_key_path_request(&mut self) -> Option<String> {
        self.new_key_path_request.take()
    }

    pub fn popover_rect_geom(&self) -> (f32, f32, f32, f32) {
        let (bx, by, bw, bh) = self.add_key_btn.rect();
        let popover_w = 220.0;
        let popover_h = 36.0;
        let popover_x = bx + bw - popover_w;
        let popover_y = by + bh + 4.0;
        (popover_x, popover_y, popover_w, popover_h)
    }

    pub fn focus_search(&mut self, ctx: &mut UiContext) {
        ctx.set_focused(&mut self.search_box);
        self.search_box.focus();
    }

    pub fn set_flat_keys(&mut self, flat_keys: Vec<(String, serde_json::Value)>) {
        self.flat_keys = flat_keys;
        self.rebuild_tree();
    }

    pub fn rebuild_tree(&mut self) {
        let query = self.search_box.text.clone();
        self.items = build_tree(&self.flat_keys, &self.annotations, &self.collapsed_sections, &query);
        let content_h = self.items.len() as f32 * self.item_height;
        let (_, _, _, h) = self.rect();
        let search_margin_y = 6.0;
        let search_h = 26.0;
        let offset_y = search_h + 2.0 * search_margin_y;
        let header_h = 26.0;
        self.scroll_box.update_bounds(content_h, self.scroll_box.viewport_y, h - offset_y - header_h);
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

    pub fn take_deleted_key_path(&mut self) -> Option<String> {
        self.deleted_key_path.take()
    }

    pub fn take_rename_request(&mut self) -> Option<(String, String)> {
        self.rename_request.take()
    }

    pub fn check_scroll_activity(&mut self, ctx: &mut UiContext) {
        if (self.scroll_box.scroll_y - self.last_scroll_y).abs() > 0.01 {
            self.scrollbar_activity_timer = 1.0;
            ctx.register_tick_receiver(self.base.id());
            self.last_scroll_y = self.scroll_box.scroll_y;
            self.mark_dirty(ctx);
        }
    }

    pub fn scroll_to_selected_key(&mut self) {
        if let Some(selected_idx) = self.selected_key_idx {
            let visible_row_idx = self.items.iter().position(|item| {
                if let TreeElement::Leaf { original_idx, .. } = item {
                    *original_idx == selected_idx
                } else {
                    false
                }
            });
            if let Some(row_idx) = visible_row_idx {
                let row_top = row_idx as f32 * self.item_height;
                let row_bottom = row_top + self.item_height;
                let viewport_h = self.scroll_box.viewport_h;
                
                if row_top < self.scroll_box.scroll_y {
                    self.scroll_box.scroll_y = row_top;
                } else if row_bottom > self.scroll_box.scroll_y + viewport_h {
                    self.scroll_box.scroll_y = (row_bottom - viewport_h).max(0.0);
                }
                
                let max_scroll = (self.scroll_box.content_h - viewport_h).max(0.0);
                self.scroll_box.scroll_y = self.scroll_box.scroll_y.clamp(0.0, max_scroll);
            }
        }
    }

    pub fn select_and_show_key(&mut self, key_path: &str) -> bool {
        let found_idx = self.flat_keys.iter().position(|(k, _)| k == key_path);
        if let Some(idx) = found_idx {
            self.selected_key_idx = Some(idx);
            
            let parts: Vec<&str> = key_path.split('.').collect();
            let mut current = String::new();
            let mut expanded_any = false;
            for i in 0..parts.len() - 1 {
                if !current.is_empty() {
                    current.push('.');
                }
                current.push_str(parts[i]);
                if self.collapsed_sections.contains(&current) {
                    self.collapsed_sections.remove(&current);
                    expanded_any = true;
                }
            }
            if expanded_any {
                self.rebuild_tree();
            }
            
            self.scroll_to_selected_key();
            true
        } else {
            false
        }
    }
}

impl Element for TreeList {
    crate::impl_widget_base!(TreeList);

    fn blocks_backplate_drag(&self) -> bool {
        true
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::tree_font())
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.base.x = x;
        self.base.y = y;
        self.base.w = w;
        self.base.h = h;
        
        let search_margin_x = 8.0;
        let search_margin_y = 6.0;
        let search_h = 26.0;
        let offset_y = search_h + 2.0 * search_margin_y;
        
        let button_width = 80.0;
        let button_height = search_h;
        let button_x = x + w - search_margin_x - button_width;
        
        self.search_box.set_rect(x + search_margin_x, y + search_margin_y, w - 2.0 * search_margin_x - button_width - 6.0, search_h);
        self.add_key_btn.set_rect(button_x, y + search_margin_y, button_width, button_height);
        
        let (px, py, pw, ph) = self.popover_rect_geom();
        self.add_key_popover_box.set_rect(px + 8.0, py + 5.0, pw - 16.0, ph - 10.0);
        
        let header_h = 26.0;
        self.scroll_box.set_rect(x, y + offset_y + header_h, w, h - offset_y - header_h);
        
        let content_h = self.items.len() as f32 * self.item_height;
        self.scroll_box.update_bounds(content_h, y + offset_y + header_h, h - offset_y - header_h);
        self.last_scroll_y = self.scroll_box.scroll_y;
    }

    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn rounded_corners(&self) -> (bool, bool, bool, bool) {
        (true, true, true, true)
    }

    fn corner_radius(&self) -> f32 {
        crate::layout::tree_corner_radius()
    }

    fn solid_border(&self) -> Option<([f32; 4], f32)> {
        if self.scroll_box.show_border {
            Some((crate::color::tree_border_color(), 1.0))
        } else {
            None
        }
    }

    fn focus(&mut self) {
        focus::set_focused(self);
    }
    fn unfocus(&mut self) {
        self.add_key_popover_open = false;
        self.add_key_popover_box.unfocus();
    }

    fn prepare_text(&mut self, fs: &mut glyphon::FontSystem) {
        self.search_box.prepare_text(fs);
        self.add_key_btn.prepare_text(fs);
        if self.add_key_popover_open {
            self.add_key_popover_box.prepare_text(fs);
        }
        self.scroll_box.prepare_text(fs);
        if self.editing_key_idx.is_some() {
            self.edit_box.prepare_text(fs);
        }
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if self.editing_key_idx.is_some() {
            if button == MouseButton::Left && state == ElementState::Pressed {
                let (ex, ey, ew, eh) = self.edit_box.rect();
                if px >= ex && px <= ex + ew && py >= ey && py <= ey + eh {
                    if self.edit_box.mouse_input(button, state, px, py, ctx) {
                        return true;
                    }
                } else {
                    ctx.clear_focus();
                    return true;
                }
            }
            return false;
        }

        let mut changed = false;

        if self.add_key_popover_open {
            let (px_rect, py_rect, pw, ph) = self.popover_rect_geom();
            if button == MouseButton::Left && state == ElementState::Pressed {
                if px < px_rect || px > px_rect + pw || py < py_rect || py > py_rect + ph {
                    self.add_key_popover_open = false;
                    ctx.clear_focus();
                    changed = true;
                } else {
                    if self.add_key_popover_box.mouse_input(button, state, px, py, ctx) {
                        ctx.set_focused(&mut self.add_key_popover_box);
                        changed = true;
                    }
                }
            }
            let (px_rect, py_rect, pw, ph) = self.popover_rect_geom();
            if px >= px_rect && px <= px_rect + pw && py >= py_rect && py <= py_rect + ph {
                return true;
            }
        }

        if self.scroll_box.mouse_input(button, state, px, py, ctx) {
            changed = true;
        }
        if self.search_box.mouse_input(button, state, px, py, ctx) {
            ctx.set_focused(&mut self.search_box);
            changed = true;
        }
        if self.add_key_btn.mouse_input(button, state, px, py, ctx) {
            if self.add_key_btn.take_click() {
                self.add_key_popover_open = !self.add_key_popover_open;
                if self.add_key_popover_open {
                    self.add_key_popover_box.text.clear();
                    self.add_key_popover_box.edit_buffer.clear();
                    self.add_key_popover_box.cursor_idx = 0;
                    self.add_key_popover_box.select_anchor = None;
                    self.add_key_popover_box.all_selected = false;
                    self.add_key_popover_box.editing = true;
                    ctx.set_focused(&mut self.add_key_popover_box);
                    self.add_key_popover_box.focus();
                } else {
                    ctx.clear_focus();
                }
            }
            changed = true;
        }
        self.check_scroll_activity(ctx);

        let list_left = self.scroll_box.base.x;
        let list_width = self.scroll_box.base.w;
        let list_top = self.scroll_box.viewport_y;
        let list_bottom = self.scroll_box.viewport_y + self.scroll_box.viewport_h;

        if button == MouseButton::Left && state == ElementState::Pressed {
            let on_scrollbar = self.scroll_box.hit_test_scrollbar(px, py) || self.scroll_box.scrollbar_dragging;
            if !on_scrollbar && px >= list_left && px <= list_left + list_width && py >= list_top && py <= list_bottom {
                ctx.set_focused(self);
                self.focus();
                let relative_y = py - list_top + self.scroll_box.scroll_y;
                let row_idx = (relative_y / self.item_height) as usize;
                if row_idx < self.items.len() {
                    let item = self.items[row_idx].clone();
                    
                    let mut is_double = false;
                    let now = std::time::Instant::now();
                    if let Some((prev_time, prev_row)) = self.double_click_timer {
                        if prev_row == row_idx && now.duration_since(prev_time).as_millis() < 300 {
                            is_double = true;
                        }
                    }
                    self.double_click_timer = Some((now, row_idx));

                    if is_double {
                        let (_path_to_edit, relative_name) = match &item {
                            TreeElement::Section { path, name, .. } => {
                                if self.collapsed_sections.contains(path) {
                                    self.collapsed_sections.remove(path);
                                } else {
                                    self.collapsed_sections.insert(path.clone());
                                }
                                self.rebuild_tree();
                                (path.clone(), name.clone())
                            }
                            TreeElement::Leaf { path, name, .. } => (path.clone(), name.clone()),
                        };
                        self.editing_key_idx = Some(row_idx);
                        self.edit_box = TextBox::new(relative_name).with_multiline(false).with_draw_bg_border(true);
                        self.edit_box.editing = true;
                        self.edit_box.cursor_idx = self.edit_box.text.chars().count();
                        self.edit_box.select_anchor = Some(0);
                        
                        let self_ptr = self as *mut Self;
                        let self_id = self.base.id();
                        unsafe {
                            let eb_ptr = (*self_ptr).edit_box.as_ptr_mut();
                            let eb_id = (*self_ptr).edit_box.base().unwrap().id();
                            ctx.register_widget(eb_id, eb_ptr);
                            ctx.link_ids(self_id, eb_id);
                            (*eb_ptr).set_parent(Some(self_ptr), ctx);
                        }
                        
                        ctx.set_focused(&mut self.edit_box);
                        return true;
                    }

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

        if button == MouseButton::Right && state == ElementState::Pressed {
            if px >= list_left && px <= list_left + list_width && py >= list_top && py <= list_bottom {
                ctx.set_focused(self);
                self.focus();
                let relative_y = py - list_top + self.scroll_box.scroll_y;
                let row_idx = (relative_y / self.item_height) as usize;
                if row_idx < self.items.len() {
                    let item = self.items[row_idx].clone();
                    match item {
                        TreeElement::Section { ref path, collapsed, .. } => {
                            self.right_clicked_section = Some(path.clone());
                            
                            let mut options = vec![path.clone()];
                            if collapsed {
                                options.push("Expand".to_string());
                            } else {
                                options.push("Collapse".to_string());
                            }
                            options.push("Expand All".to_string());
                            options.push("Collapse All".to_string());
                            
                            let scroll_offset = crate::widget::hover_animation::get_scroll_offset();
                            ctx.show_context_menu(px, py - scroll_offset, options, 1, self.as_ptr_mut());
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
                            
                            let options = vec![
                                path.clone(),
                                "Copy Key".to_string(),
                                "Copy Value".to_string(),
                                "Delete".to_string(),
                            ];
                            let scroll_offset = crate::widget::hover_animation::get_scroll_offset();
                            ctx.show_context_menu(px, py - scroll_offset, options, 1, self.as_ptr_mut());
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
        if self.search_box.on_cursor_moved(px, py, ctx) {
            changed = true;
        }
        if self.add_key_btn.on_cursor_moved(px, py, ctx) {
            changed = true;
        }
        if self.add_key_popover_open {
            if self.add_key_popover_box.on_cursor_moved(px, py, ctx) {
                changed = true;
            }
        }

        let list_left = self.scroll_box.base.x;
        let list_width = self.scroll_box.base.w;
        let list_top = self.scroll_box.viewport_y;
        let list_bottom = self.scroll_box.viewport_y + self.scroll_box.viewport_h;

        let old_hovered = self.hovered_row_idx;
        self.hovered_row_idx = None;

        let on_scrollbar = self.scroll_box.hit_test_scrollbar(px, py) || self.scroll_box.scrollbar_dragging;
        if !on_scrollbar && px >= list_left && px <= list_left + list_width && py >= list_top && py <= list_bottom {
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
        let changed = self.scroll_box.mouse_wheel(delta, px, py, ctx);
        self.check_scroll_activity(ctx);
        changed
    }

    fn draggable(&self) -> bool {
        self.scroll_box.draggable()
    }

    fn drag_begin(&mut self, px: f32, py: f32) {
        self.scroll_box.drag_begin(px, py);
    }

    fn drag_update(&mut self, px: f32, py: f32) -> bool {
        self.scroll_box.drag_update(px, py)
    }

    fn drag_end(&mut self) {
        self.scroll_box.drag_end();
    }

    fn wants_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, dt: f32, ctx: &mut UiContext) -> bool {
        let mut changed = false;
        if self.search_box.tick(dt, ctx) {
            changed = true;
        }
        if self.add_key_btn.tick(dt, ctx) {
            changed = true;
        }
        if self.add_key_popover_open {
            if self.add_key_popover_box.tick(dt, ctx) {
                changed = true;
            }
            if !self.add_key_popover_box.editing {
                let path = self.add_key_popover_box.text.trim().to_string();
                if !path.is_empty() {
                    self.new_key_path_request = Some(path);
                }
                self.add_key_popover_open = false;
                ctx.clear_focus();
                changed = true;
            }
        }
        if self.search_box.take_change() {
            self.rebuild_tree();
            self.mark_dirty(ctx);
            changed = true;
        }
        
        if self.editing_key_idx.is_some() {
            if self.edit_box.tick(dt, ctx) {
                changed = true;
            }
            if let Some(row_idx) = self.editing_key_idx {
                if row_idx < self.items.len() {
                    let list_left = self.scroll_box.base.x;
                    let list_top = self.scroll_box.viewport_y;
                    let row_y = list_top + row_idx as f32 * self.item_height - self.scroll_box.scroll_y;
                    let box_x = list_left + 5.0;
                    let box_y = row_y + 2.0;
                    self.edit_box.set_rect(box_x, box_y, 170.0, 24.0);
                }
            }
            if !self.edit_box.editing {
                let row_idx = self.editing_key_idx.unwrap();
                if row_idx < self.items.len() {
                    let (old_path, relative_name) = match &self.items[row_idx] {
                        TreeElement::Section { path, name, .. } => (path.clone(), name.clone()),
                        TreeElement::Leaf { path, name, .. } => (path.clone(), name.clone()),
                    };
                    let new_name = self.edit_box.text.trim().to_string();
                    if !new_name.is_empty() && new_name != relative_name {
                        let new_path = if let Some(pos) = old_path.rfind('.') {
                            format!("{}.{}", &old_path[..pos], new_name)
                        } else {
                            new_name
                        };
                        self.rename_request = Some((old_path, new_path));
                    }
                }
                self.editing_key_idx = None;
                ctx.set_focused(self);
                self.mark_dirty(ctx);
                changed = true;
            }
        }

        if (self.scroll_box.scroll_y - self.last_scroll_y).abs() > 0.01 {
            self.last_scroll_y = self.scroll_box.scroll_y;
            self.scrollbar_activity_timer = 1.0;
            self.mark_dirty(ctx);
            changed = true;
        }
        if self.scrollbar_activity_timer > 0.0 {
            self.scrollbar_activity_timer = (self.scrollbar_activity_timer - dt).max(0.0);
            changed = true;
        }
        changed
    }

    fn keyboard_input(&mut self, event: &KeyEvent, ctx: &mut UiContext) -> bool {
        if self.editing_key_idx.is_some() {
            if self.edit_box.keyboard_input(event, ctx) {
                return true;
            }
        }
        if self.add_key_popover_open {
            if self.add_key_popover_box.keyboard_input(event, ctx) {
                return true;
            }
        }
        if self.search_box.keyboard_input(event, ctx) {
            return true;
        }
        false
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let (r1, r2, r3, r4) = self.rounded_corners();
        let has_rounded = r1 || r2 || r3 || r4;
        if has_rounded {
            return Vec::new();
        }

        let mut quads = self.scroll_box.extra_quads();

        let list_left = self.scroll_box.base.x;
        let list_width = self.scroll_box.base.w;

        let search_margin_y = 6.0;
        let search_h = 26.0;
        let offset_y = search_h + 2.0 * search_margin_y;

        // Draw Header background and border
        let header_h = 26.0;
        let header_bg_color = [0.12, 0.12, 0.16, 1.0]; // Dark header color
        let header_border_color = [0.18, 0.18, 0.22, 1.0];
        
        // Header background
        quads.push((list_left, self.base.y + offset_y, list_width, header_h, header_bg_color));
        
        // Separator line below header
        quads.push((list_left, self.base.y + offset_y + header_h - 1.0, list_width, 1.0, header_border_color));
        
        // Vertical separators inside header
        quads.push((list_left + 180.0, self.base.y + offset_y, 1.0, header_h, header_border_color));
        quads.push((list_left + 235.0, self.base.y + offset_y, 1.0, header_h, header_border_color));

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
                        crate::color::tree_section_bg_hover_color()
                    } else {
                        crate::color::tree_section_bg_color()
                    }
                }
                TreeElement::Leaf { original_idx, .. } => {
                    if Some(*original_idx) == self.selected_key_idx {
                        crate::color::tree_leaf_bg_selected_color()
                    } else if Some(i) == self.hovered_row_idx {
                        crate::color::tree_leaf_bg_hover_color()
                    } else if i % 2 == 0 {
                        crate::color::tree_leaf_bg_even_color()
                    } else {
                        crate::color::tree_leaf_bg_odd_color()
                    }
                }
            };
            
            quads.push((list_left + 1.0, draw_y, list_width - 2.0, draw_h, bg_color));
            
            // Draw column separator lines and color preview for Leaf rows
            if let TreeElement::Leaf { ref val, original_idx, .. } = item {
                let separator_color = crate::color::tree_separator_color();
                quads.push((list_left + 180.0, draw_y, 1.0, draw_h, separator_color));
                quads.push((list_left + 235.0, draw_y, 1.0, draw_h, separator_color));

                // Color preview in 3rd column next to value string (only when not selected)
                if Some(*original_idx) != self.selected_key_idx {
                    if let serde_json::Value::String(s) = val {
                        if s.starts_with('#') {
                            if let Some(rgba) = parse_hex_f32(s) {
                                let preview_x = list_left + 245.0;
                                let preview_y = row_y + 4.0;
                                let preview_bottom = (row_y + 20.0).min(list_bottom);
                                let preview_draw_y = preview_y.max(list_top);
                                let preview_draw_h = preview_bottom - preview_draw_y;
                                if preview_draw_h > 0.0 {
                                    // Checkerboard pattern
                                    let grid_size = 8.0;
                                    quads.push((preview_x, preview_draw_y, 16.0, preview_draw_h, [1.0, 1.0, 1.0, 1.0]));
                                    let cols = (16.0f32 / grid_size).ceil() as i32;
                                    let rows = (preview_draw_h as f32 / grid_size).ceil() as i32;
                                    for r in 0..rows {
                                        for c in 0..cols {
                                            if (r + c) % 2 == 1 {
                                                let qx = preview_x + c as f32 * grid_size;
                                                let qy = preview_draw_y + r as f32 * grid_size;
                                                let qw = grid_size.min(preview_x + 16.0 - qx);
                                                let qh = grid_size.min(preview_draw_y + preview_draw_h - qy);
                                                if qw > 0.0 && qh > 0.0 {
                                                    quads.push((qx, qy, qw, qh, [0.8, 0.8, 0.8, 1.0]));
                                                }
                                            }
                                        }
                                    }
                                    quads.push((preview_x, preview_draw_y, 16.0, preview_draw_h, rgba));
                                }
                            }
                        }
                    }
                }
            }

            if row_y + self.item_height <= list_bottom {
                quads.push((list_left + 1.0, row_y + self.item_height - 1.0, list_width - 2.0, 1.0, [0.13, 0.13, 0.17, 1.0]));
            }
        }

        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let f32_to_rgb = |c: [f32; 4]| -> [u8; 3] {
            [
                (crate::color::linear_to_srgb(c[0]) * 255.0).round() as u8,
                (crate::color::linear_to_srgb(c[1]) * 255.0).round() as u8,
                (crate::color::linear_to_srgb(c[2]) * 255.0).round() as u8,
            ]
        };

        let (_, tree_font_size) = crate::layout::tree_font_parsed();
        let header_font_size = (tree_font_size - 1.0).max(8.0);

        let mut labels = Vec::new();
        let list_left = self.scroll_box.base.x;
        let list_top = self.scroll_box.viewport_y;
        let list_bottom = self.scroll_box.viewport_y + self.scroll_box.viewport_h;

        let search_margin_y = 6.0;
        let search_h = 26.0;
        let offset_y = search_h + 2.0 * search_margin_y;

        labels.push(TextLabel {
            text: "Key".to_string(),
            x: list_left + 8.0,
            y: self.base.y + offset_y + 6.0,
            font_size: header_font_size,
            color: [200, 200, 210],
        });
        labels.push(TextLabel {
            text: "Type".to_string(),
            x: list_left + 180.0 + 8.0,
            y: self.base.y + offset_y + 6.0,
            font_size: header_font_size,
            color: [200, 200, 210],
        });
        labels.push(TextLabel {
            text: "Value".to_string(),
            x: list_left + 235.0 + 8.0,
            y: self.base.y + offset_y + 6.0,
            font_size: header_font_size,
            color: [200, 200, 210],
        });

        for (i, item) in self.items.iter().enumerate() {
            let row_y = list_top + i as f32 * self.item_height - self.scroll_box.scroll_y;
            if row_y + self.item_height < list_top || row_y > list_bottom {
                continue;
            }

            match item {
                TreeElement::Section { name, indent, collapsed, .. } => {
                    if self.editing_key_idx != Some(i) {
                        let display_text = format!("{} {}", if *collapsed { "▶" } else { "▼" }, name);
                        labels.push(TextLabel {
                            text: display_text,
                            x: list_left + 8.0 + *indent as f32 * 12.0,
                            y: row_y + 6.0,
                            font_size: tree_font_size,
                            color: f32_to_rgb(crate::color::tree_section_text_color()),
                        });
                    }
                }
                TreeElement::Leaf { name, indent, val, original_idx, .. } => {
                    let val_str = serde_json::to_string(val).unwrap_or_default();
                    let display_val = if val_str.len() > 18 {
                        format!("{}...", &val_str[..15])
                    } else {
                        val_str
                    };

                    let color = if Some(*original_idx) == self.selected_key_idx {
                        f32_to_rgb(crate::color::tree_leaf_text_selected_color())
                    } else {
                        f32_to_rgb(crate::color::tree_leaf_text_color())
                    };

                    if self.editing_key_idx != Some(i) {
                        labels.push(TextLabel {
                            text: name.clone(),
                            x: list_left + 8.0 + *indent as f32 * 12.0,
                            y: row_y + 6.0,
                            font_size: tree_font_size,
                            color,
                        });
                    }

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
                                let s_clean = s.trim_start_matches('#');
                                if s_clean.len() == 8 {
                                    Some("rgba")
                                } else {
                                    Some("rgb")
                                }
                            } else if name == "key" || name == "keybind" || name == "shortcut" || name == "open_search" || name == "close_search" || name == "delete" || name.ends_with("_key") || name.ends_with(".key") || name.ends_with(".keybind") || name.ends_with(".shortcut") || name.ends_with(".open_search") || name.ends_with(".close_search") || name.ends_with("_delete") || name.ends_with(".delete") {
                                Some("keybind")
                            } else if name == "font" || name.ends_with("_font") || name.ends_with(".font") {
                                Some("font")
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };
                    let mut display_ty = val_ty.map(|s| s.to_string());
                    if let Some(Some(ref anno)) = self.annotations.get(*original_idx) {
                        if anno.starts_with("menu:") {
                            display_ty = Some("menu".to_string());
                        } else if anno == "button" || anno.starts_with("button:") {
                            display_ty = Some("button".to_string());
                        } else {
                            display_ty = Some(anno.clone());
                        }
                    } else if display_ty.is_none() {
                        if let serde_json::Value::String(_) = val {
                            display_ty = Some("string".to_string());
                        }
                    }

                    if let Some(ty) = display_ty {
                        let ty_text = format!("({})", ty);
                        labels.push(TextLabel {
                            text: ty_text,
                            x: list_left + 190.0,
                            y: row_y + 6.0,
                            font_size: tree_font_size,
                            color: f32_to_rgb(crate::color::tree_type_text_color()),
                        });
                    }

                    if Some(*original_idx) != self.selected_key_idx {
                        let mut is_button = false;
                        if let Some(Some(ref anno)) = self.annotations.get(*original_idx) {
                            if anno == "button" || anno.starts_with("button:") {
                                is_button = true;
                            }
                        }

                        let is_color = if let serde_json::Value::String(s) = val {
                            s.starts_with('#')
                        } else {
                            false
                        };
                        
                        let label_x = if is_color {
                            list_left + 267.0
                        } else {
                            list_left + 245.0
                        };

                        if is_button {
                            labels.push(TextLabel {
                                text: display_val,
                                x: list_left + 245.0 + 8.0,
                                y: row_y + 6.0,
                                font_size: tree_font_size,
                                color: [240, 240, 245],
                            });
                        } else {
                            labels.push(TextLabel {
                                text: display_val,
                                x: label_x,
                                y: row_y + 6.0,
                                font_size: tree_font_size,
                                color: f32_to_rgb(crate::color::tree_value_text_color()),
                            });
                        }
                    }
                }
            }
        }
        labels
    }

    fn text_labels_with_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<[f32; 4]>)> {
        let (x, y, w, _h) = self.rect();
        
        let search_margin_y = 6.0;
        let search_h = 26.0;
        let offset_y = search_h + 2.0 * search_margin_y;
        let header_h = 26.0;

        let list_bounds = Some([self.scroll_box.base.x, self.scroll_box.viewport_y, self.scroll_box.base.x + self.scroll_box.base.w, self.scroll_box.viewport_y + self.scroll_box.viewport_h]);
        let header_bounds = Some([x, y + offset_y, x + w, y + offset_y + header_h]);
        
        let mut labels = self.text_labels().into_iter().enumerate().map(|(idx, l)| {
            let b = if idx < 3 {
                header_bounds
            } else {
                list_bounds
            };
            (l, b)
        }).collect::<Vec<_>>();

        labels.extend(self.search_box.text_labels_with_bounds(ctx));
        labels
    }

    fn text_labels_with_font_and_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        let font = self.widget_font();
        let (x, y, w, _h) = self.rect();
        
        let search_margin_y = 6.0;
        let search_h = 26.0;
        let offset_y = search_h + 2.0 * search_margin_y;
        let header_h = 26.0;

        let list_bounds = Some([self.scroll_box.base.x, self.scroll_box.viewport_y, self.scroll_box.base.x + self.scroll_box.base.w, self.scroll_box.viewport_y + self.scroll_box.viewport_h]);
        let header_bounds = Some([x, y + offset_y, x + w, y + offset_y + header_h]);
        
        let mut labels = self.text_labels().into_iter().enumerate().map(|(idx, l)| {
            let b = if idx < 3 {
                header_bounds
            } else {
                list_bounds
            };
            (l, font.clone(), b)
        }).collect::<Vec<_>>();

        labels.extend(self.search_box.text_labels_with_font_and_bounds(ctx));
        labels.extend(self.add_key_btn.text_labels_with_font_and_bounds(ctx));
        if self.add_key_popover_open {
            labels.extend(self.add_key_popover_box.text_labels_with_font_and_bounds(ctx));
        }
        labels
    }

    fn parent(&self, _ctx: &UiContext) -> Option<*mut (dyn Element + 'static)> {
        self.parent
    }

    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>, ctx: &mut UiContext) {
        self.parent = parent;
        if parent.is_some() {
            let self_ptr = self as *mut Self;
            let self_id = self.base.id();
            unsafe {
                let sb_ptr = (*self_ptr).search_box.as_ptr_mut();
                let sb_id = (*self_ptr).search_box.base().unwrap().id();
                ctx.register_widget(sb_id, sb_ptr);
                ctx.link_ids(self_id, sb_id);
                (*sb_ptr).set_parent(Some(self_ptr), ctx);

                let btn_ptr = (*self_ptr).add_key_btn.as_ptr_mut();
                let btn_id = (*self_ptr).add_key_btn.base().unwrap().id();
                ctx.register_widget(btn_id, btn_ptr);
                ctx.link_ids(self_id, btn_id);
                (*btn_ptr).set_parent(Some(self_ptr), ctx);

                let pop_ptr = (*self_ptr).add_key_popover_box.as_ptr_mut();
                let pop_id = (*self_ptr).add_key_popover_box.base().unwrap().id();
                ctx.register_widget(pop_id, pop_ptr);
                ctx.link_ids(self_id, pop_id);
                (*pop_ptr).set_parent(Some(self_ptr), ctx);
            }
        }
    }

    fn children(&self, _ctx: &UiContext) -> Vec<*mut (dyn Element + 'static)> {
        let mut list = self.children.clone();
        let self_ptr = self as *const Self as *mut Self;
        unsafe {
            list.push((*self_ptr).search_box.as_ptr_mut());
            list.push((*self_ptr).add_key_btn.as_ptr_mut());
            if (*self_ptr).add_key_popover_open {
                list.push((*self_ptr).add_key_popover_box.as_ptr_mut());
            }
            if (*self_ptr).editing_key_idx.is_some() {
                list.push((*self_ptr).edit_box.as_ptr_mut());
            }
        }
        list
    }

    // TreeList draws its rows, backgrounds and separators in the recursive all_rounded_quads below,
    // so the Phase 3 paint walk emits that directly instead of descending (see Element docs).
    fn renders_own_subtree(&self) -> bool {
        true
    }

    /// The whole subtree — geometry via the recursive `all_*` aggregates, text via the
    /// recursive bounded getter — emitted here so the paint walk's `renders_own_subtree`
    /// branch is just `paint_self` + no descent, with no trait-getter use left in the walk.
    fn paint_self(&self, ui: &UiContext, pc: &mut crate::scene::paint::PaintCtx) {
        use crate::scene::layout::Rect;
        for (x, y, w, h, r, c, corners) in self.all_rounded_quads(ui) {
            pc.rounded_rect(Rect { x, y, width: w, height: h }, r, corners, c);
        }
        for (x, y, w, h, c) in self.all_quads(ui) {
            pc.quad(Rect { x, y, width: w, height: h }, c);
        }
        for (cx, cy, r, t, s, e, c) in self.extra_arcs() {
            pc.arc(cx, cy, r, t, s, e, c);
        }
        for (cx, cy, r, c) in self.extra_circles() {
            pc.circle(cx, cy, r, c);
        }
        for (tl, font, bounds) in self.text_labels_with_font_and_bounds(ui) {
            pc.text_with(tl.text, tl.x, tl.y, tl.font_size, tl.color, font, bounds);
        }
    }

    fn all_rounded_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, f32, [f32; 4], (bool, bool, bool, bool))> {
        let mut quads = Vec::new();
        let (r1, r2, r3, r4) = self.rounded_corners();
        let has_rounded = r1 || r2 || r3 || r4;
        if !has_rounded {
            for &child_ptr in &self.children(ctx) {
                let widget = unsafe { &*child_ptr };
                quads.extend(widget.all_rounded_quads(ctx));
            }
            return quads;
        }

        let radius = self.corner_radius();
        let (x, y, w, h) = self.rect();
        
        let opacity = crate::layout::tree_opacity();
        let apply_opacity = |mut c: [f32; 4]| -> [f32; 4] {
            c[3] *= opacity;
            c
        };

        // 1. Draw container border and background
        if let Some((border_color, thickness)) = self.solid_border() {
            quads.push((x, y, w, h, radius, apply_opacity(border_color), (r1, r2, r3, r4)));
            quads.push((x + thickness, y + thickness, w - 2.0 * thickness, h - 2.0 * thickness, radius - thickness, apply_opacity(crate::color::tree_background_color()), (r1, r2, r3, r4)));
        } else {
            quads.push((x, y, w, h, radius, apply_opacity(crate::color::tree_background_color()), (r1, r2, r3, r4)));
        }

        let search_margin_y = 6.0;
        let search_h = 26.0;
        let offset_y = search_h + 2.0 * search_margin_y;

        // Draw Header background and border
        let header_h = 26.0;
        let header_bg_color = [0.12, 0.12, 0.16, 1.0]; // Dark header color
        let header_border_color = [0.18, 0.18, 0.22, 1.0];
        
        let list_left = self.scroll_box.base.x;
        let list_width = self.scroll_box.base.w;
        
        // Header background (not rounded anymore, since it is in the middle of TreeList, below search box)
        quads.push((list_left + 1.0, y + offset_y + 1.0, list_width - 2.0, header_h - 1.0, 0.0, apply_opacity(header_bg_color), (false, false, false, false)));
        
        // Separator line below header
        quads.push((list_left + 1.0, y + offset_y + header_h - 1.0, list_width - 2.0, 1.0, 0.0, apply_opacity(header_border_color), (false, false, false, false)));
        
        // Vertical separators inside header
        quads.push((list_left + 180.0, y + offset_y + 1.0, 1.0, header_h - 2.0, 0.0, apply_opacity(header_border_color), (false, false, false, false)));
        quads.push((list_left + 235.0, y + offset_y + 1.0, 1.0, header_h - 2.0, 0.0, apply_opacity(header_border_color), (false, false, false, false)));

        // Helper to collect scrollbar quads
        let get_scrollbar_quads = || {
            let mut sb_quads = Vec::new();
            let scroll_quads = self.scroll_box.extra_quads();
            if scroll_quads.len() > 1 {
                for q in &scroll_quads[1..] {
                    sb_quads.push((q.0, q.1, q.2, q.3, 0.0, q.4, (false, false, false, false)));
                }
            }
            sb_quads
        };

        let show_on_top = self.scrollbar_activity_timer > 0.0;

        // If NOT on top, draw scrollbar first (behind items)
        if !show_on_top {
            quads.extend(get_scrollbar_quads());
        }

        // 2. Draw items (row backgrounds, separator lines, color previews)
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
                        crate::color::tree_section_bg_hover_color()
                    } else {
                        crate::color::tree_section_bg_color()
                    }
                }
                TreeElement::Leaf { original_idx, .. } => {
                    if Some(*original_idx) == self.selected_key_idx {
                        crate::color::tree_leaf_bg_selected_color()
                    } else if Some(i) == self.hovered_row_idx {
                        crate::color::tree_leaf_bg_hover_color()
                    } else if i % 2 == 0 {
                        crate::color::tree_leaf_bg_even_color()
                    } else {
                        crate::color::tree_leaf_bg_odd_color()
                    }
                }
            };
            
            let row_r1 = false;
            let row_r2 = false;
            let mut row_r3 = false;
            let mut row_r4 = false;
            let row_radius = radius - 1.0;

            if draw_bottom >= list_bottom - radius {
                row_r3 = r3;
                row_r4 = r4;
            }

            quads.push((list_left + 1.0, draw_y, list_width - 2.0, draw_h, row_radius, apply_opacity(bg_color), (row_r1, row_r2, row_r3, row_r4)));
            
            if let TreeElement::Leaf { ref val, original_idx, .. } = item {
                let separator_color = crate::color::tree_separator_color();
                quads.push((list_left + 180.0, draw_y, 1.0, draw_h, 0.0, apply_opacity(separator_color), (false, false, false, false)));
                quads.push((list_left + 235.0, draw_y, 1.0, draw_h, 0.0, apply_opacity(separator_color), (false, false, false, false)));

                let mut is_button = false;
                if let Some(Some(ref anno)) = self.annotations.get(*original_idx) {
                    if anno == "button" || anno.starts_with("button:") {
                        is_button = true;
                    }
                }

                if Some(*original_idx) != self.selected_key_idx {
                    if is_button {
                        let btn_x = list_left + 245.0;
                        let btn_y = row_y + 1.0;
                        let btn_bottom = (row_y + 27.0).min(list_bottom);
                        let btn_draw_y = btn_y.max(list_top);
                        let btn_draw_h = btn_bottom - btn_draw_y;
                        if btn_draw_h > 0.0 {
                            let btn_bg = [0.10, 0.29, 0.33, 0.65]; // theme button color
                            quads.push((btn_x, btn_draw_y, 125.0, btn_draw_h, 4.0, apply_opacity(btn_bg), (true, true, true, true)));
                        }
                    } else if let serde_json::Value::String(s) = val {
                        if s.starts_with('#') {
                            if let Some(rgba) = parse_hex_f32(s) {
                                let preview_x = list_left + 245.0;
                                let preview_y = row_y + 4.0;
                                let preview_bottom = (row_y + 20.0).min(list_bottom);
                                let preview_draw_y = preview_y.max(list_top);
                                let preview_draw_h = preview_bottom - preview_draw_y;
                                if preview_draw_h > 0.0 {
                                    // Checkerboard pattern
                                    let grid_size = 8.0;
                                    quads.push((preview_x, preview_draw_y, 16.0, preview_draw_h, 0.0, [1.0, 1.0, 1.0, 1.0], (false, false, false, false)));
                                    let cols = (16.0f32 / grid_size).ceil() as i32;
                                    let rows = (preview_draw_h as f32 / grid_size).ceil() as i32;
                                    for r in 0..rows {
                                        for c in 0..cols {
                                            if (r + c) % 2 == 1 {
                                                let qx = preview_x + c as f32 * grid_size;
                                                let qy = preview_draw_y + r as f32 * grid_size;
                                                let qw = grid_size.min(preview_x + 16.0 - qx);
                                                let qh = grid_size.min(preview_draw_y + preview_draw_h - qy);
                                                if qw > 0.0 && qh > 0.0 {
                                                    quads.push((qx, qy, qw, qh, 0.0, [0.8, 0.8, 0.8, 1.0], (false, false, false, false)));
                                                }
                                            }
                                        }
                                    }
                                    quads.push((preview_x, preview_draw_y, 16.0, preview_draw_h, 0.0, rgba, (false, false, false, false)));
                                }
                            }
                        }
                    }
                }
            }

            if row_y + self.item_height <= list_bottom {
                let mut sep_r3 = false;
                let mut sep_r4 = false;
                if row_y + self.item_height >= list_bottom - radius {
                    sep_r3 = r3;
                    sep_r4 = r4;
                }
                quads.push((list_left + 1.0, row_y + self.item_height - 1.0, list_width - 2.0, 1.0, row_radius, apply_opacity([0.13, 0.13, 0.17, 1.0]), (false, false, sep_r3, sep_r4)));
            }
        }

        // If on top, draw scrollbar last
        if show_on_top {
            quads.extend(get_scrollbar_quads());
        }

        for &child_ptr in &self.children(ctx) {
            let widget = unsafe { &*child_ptr };
            quads.extend(widget.all_rounded_quads(ctx));
        }
        quads
    }

    fn add_child(&mut self, child: *mut (dyn Element + 'static), _ctx: &mut UiContext) {
        self.children.push(child);
    }

    fn clear_children(&mut self, _ctx: &mut UiContext) {
        self.children.clear();
    }

    fn copy_key(&self) {
        if let Some(idx) = self.selected_key_idx {
            if idx < self.flat_keys.len() {
                let key_path = &self.flat_keys[idx].0;
                clipboard::copy_to_clipboard(key_path);
            }
        }
    }

    fn copy_value(&self) {
        if let Some(idx) = self.selected_key_idx {
            if idx < self.flat_keys.len() {
                let val = &self.flat_keys[idx].1;
                let val_str = match val {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    serde_json::Value::Number(n) => n.to_string(),
                    other => serde_json::to_string(other).unwrap_or_default(),
                };
                clipboard::copy_to_clipboard(&val_str);
            }
        }
    }

    fn delete_key(&mut self) {
        if let Some(idx) = self.selected_key_idx {
            if idx < self.flat_keys.len() {
                let key_path = self.flat_keys[idx].0.clone();
                self.deleted_key_path = Some(key_path);
            }
        }
    }

    fn expand_node(&mut self) {
        if let Some(path) = self.right_clicked_section.clone() {
            self.collapsed_sections.remove(&path);
            self.rebuild_tree();
            self.clicked_item = Some(TreeElement::Section {
                path,
                name: String::new(),
                indent: 0,
                collapsed: false,
            });
        }
        self.right_clicked_section = None;
    }

    fn collapse_node(&mut self) {
        if let Some(path) = self.right_clicked_section.clone() {
            self.collapsed_sections.insert(path.clone());
            self.rebuild_tree();
            self.clicked_item = Some(TreeElement::Section {
                path,
                name: String::new(),
                indent: 0,
                collapsed: true,
            });
        }
        self.right_clicked_section = None;
    }

    fn expand_all_nodes(&mut self) {
        self.collapsed_sections.clear();
        self.rebuild_tree();
        if let Some(path) = self.right_clicked_section.clone() {
            self.clicked_item = Some(TreeElement::Section {
                path,
                name: String::new(),
                indent: 0,
                collapsed: false,
            });
        } else {
            self.clicked_item = Some(TreeElement::Section {
                path: String::new(),
                name: String::new(),
                indent: 0,
                collapsed: false,
            });
        }
        self.right_clicked_section = None;
    }

    fn collapse_all_nodes(&mut self) {
        self.collapsed_sections = self.get_all_section_paths();
        self.rebuild_tree();
        if let Some(path) = self.right_clicked_section.clone() {
            self.clicked_item = Some(TreeElement::Section {
                path,
                name: String::new(),
                indent: 0,
                collapsed: true,
            });
        } else {
            self.clicked_item = Some(TreeElement::Section {
                path: String::new(),
                name: String::new(),
                indent: 0,
                collapsed: true,
            });
        }
        self.right_clicked_section = None;
    }

    fn popover_rect(&self) -> Option<(f32, f32, f32, f32)> {
        if self.add_key_popover_open {
            Some(self.popover_rect_geom())
        } else {
            None
        }
    }

    fn render_popover(&self, pc: &mut dyn crate::layout::RenderTarget) {
        if !self.add_key_popover_open { return; }
        
        let (rx, ry, rw, rh) = self.popover_rect_geom();
        
        // 1. Soft layered drop shadows
        pc.rect([0.02, 0.02, 0.05, 0.15], rx + 1.0, ry + 1.0, rw, rh);
        pc.rect([0.02, 0.02, 0.05, 0.08], rx + 3.0, ry + 3.0, rw, rh);
        pc.rect([0.02, 0.02, 0.05, 0.04], rx + 5.0, ry + 5.0, rw, rh);

        let theme = crate::color::active_theme();

        // 2. High-contrast premium outer border
        pc.rect(theme.surface_border, rx, ry, rw, rh);
        
        // 3. Frosted glass background
        pc.rect(theme.surface_bg, rx + 1.0, ry + 1.0, rw - 2.0, rh - 2.0); // bg
    }
}

impl TreeList {
    fn get_all_section_paths(&self) -> HashSet<String> {
        let mut sections = HashSet::new();
        for (key_path, _) in &self.flat_keys {
            let tokens = parse_path(key_path);
            let mut current_prefix = String::new();
            for i in 0..(tokens.len().saturating_sub(1)) {
                let token = &tokens[i];
                match token {
                    PathToken::Key(k) => {
                        if current_prefix.is_empty() {
                            current_prefix = k.clone();
                        } else {
                            current_prefix = format!("{}.{}", current_prefix, k);
                        }
                    }
                    PathToken::Index(idx) => {
                        let s = format!("[{}]", idx);
                        current_prefix = format!("{}{}", current_prefix, s);
                    }
                };
                sections.insert(current_prefix.clone());
            }
        }
        sections
    }
}

fn parse_hex_f32(s: &str) -> Option<[f32; 4]> {
    crate::color::parse_hex_rgba_linear(s)
}

unsafe impl Send for TreeList {}
unsafe impl Sync for TreeList {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::container::Backplate;
    use crate::context::UiContext;

    #[test]
    fn test_treelist_blocks_backplate_drag() {
        let mut ctx = UiContext::new();
        let mut win = Backplate::new(0.0, 0.0, 800.0, 600.0).with_movable(true);
        let mut tree_list = TreeList::new();
        tree_list.set_rect(10.0, 52.0, 380.0, 500.0);
        
        ctx.register_widget(win.base().unwrap().id(), win.as_ptr_mut());
        win.add_child(tree_list.as_ptr_mut(), &mut ctx);
        
        // Let's tick and clear dirty to build the spatial grid
        ctx.tick(0.016);
        ctx.clear_dirty();
        
        // Now, click at x=100, y=200, which is inside tree_list rect
        let is_movable = ctx.is_movable_backplate_at(100.0, 200.0);
        assert!(!is_movable, "Clicking the TreeList should block backplate drag!");
    }

    #[test]
    fn test_exact_app_layout_blocks_drag() {
        let mut ctx = UiContext::new();
        let mut root_window = Backplate::new(0.0, 0.0, 800.0, 600.0).with_movable(true);
        let mut tree_list = TreeList::new();
        
        // 1. Initial register (like in view)
        ctx.register_widget(root_window.base().unwrap().id(), root_window.as_ptr_mut());
        ctx.register_widget(tree_list.base().unwrap().id(), tree_list.as_ptr_mut());
        root_window.add_child(tree_list.as_ptr_mut(), &mut ctx);
        ctx.rebuild_spatial_grid();
        
        // 2. Set rect (like in view)
        root_window.set_rect(0.0, 0.0, 800.0, 600.0);
        let list_top = 52.0;
        let list_bottom = 600.0 - 180.0;
        let list_height = list_bottom - list_top;
        tree_list.set_rect(10.0, list_top, 380.0, list_height);
        
        ctx.rebuild_spatial_grid();
        
        // 3. Test click at logical x=100.0, y=200.0
        let is_movable = ctx.is_movable_backplate_at(100.0, 200.0);
        assert!(!is_movable, "Clicking TreeList under exact app layout should block backplate drag!");
    }

    #[test]
    fn test_treelist_separators() {
        let ctx = UiContext::new();
        let mut tree_list = TreeList::new();
        tree_list.set_rect(10.0, 52.0, 380.0, 500.0);
        tree_list.set_flat_keys(vec![
            ("style.data.tree.corner_radius".to_string(), serde_json::Value::Number(serde_json::Number::from(8)))
        ]);
        
        println!("Flat keys size: {}", tree_list.flat_keys.len());
        println!("Items count: {}", tree_list.items.len());
        for (i, item) in tree_list.items.iter().enumerate() {
            println!("Item {}: {:?}", i, item);
        }
        println!("scroll_box base.x: {}", tree_list.scroll_box.base.x);
        println!("scroll_box viewport_y: {}", tree_list.scroll_box.viewport_y);
        println!("scroll_box viewport_h: {}", tree_list.scroll_box.viewport_h);
        println!("scroll_box scroll_y: {}", tree_list.scroll_box.scroll_y);
        println!("item_height: {}", tree_list.item_height);
        
        let quads = tree_list.all_rounded_quads(&ctx);
        println!("Rounded quads count: {}", quads.len());
        for (i, q) in quads.iter().enumerate() {
            println!("Quad {}: {:?}", i, q);
        }
        assert!(quads.len() > 1, "Should have more than 1 quad!");
    }

    #[test]
    fn test_keybind_label() {
        let mut tree_list = TreeList::new();
        tree_list.set_rect(10.0, 52.0, 380.0, 500.0);
        tree_list.annotations = vec![Some("menu:flat,adaptive".to_string())];
        tree_list.set_flat_keys(vec![
            ("input.accel_profile".to_string(), serde_json::Value::String("flat".to_string()))
        ]);
        
        let labels = tree_list.text_labels();
        for label in &labels {
            println!("TEST LABEL: {:?}", label);
        }
        
        let has_menu_label = labels.iter().any(|l| l.text == "(menu)");
        assert!(has_menu_label, "Should have (menu) label!");
    }

    #[test]
    fn test_treelist_headers() {
        let tree_list = TreeList::new();
        let labels = tree_list.text_labels();
        assert!(labels.iter().any(|l| l.text == "Key"), "Should have Key header!");
        assert!(labels.iter().any(|l| l.text == "Type"), "Should have Type header!");
        assert!(labels.iter().any(|l| l.text == "Value"), "Should have Value header!");
    }

    #[test]
    fn test_treelist_search_filtering() {
        let mut tree_list = TreeList::new();
        tree_list.set_rect(10.0, 52.0, 380.0, 500.0);
        tree_list.set_flat_keys(vec![
            ("style.data.tree.corner_radius".to_string(), serde_json::Value::Number(serde_json::Number::from(8))),
            ("style.data.tree.border_color".to_string(), serde_json::Value::String("#ff0000".to_string())),
            ("input.accel_profile".to_string(), serde_json::Value::String("flat".to_string())),
        ]);
        
        // Match none
        tree_list.search_box.text = "nonexistent".to_string();
        tree_list.rebuild_tree();
        assert!(tree_list.items.is_empty(), "Tree should be empty for nonexistent search query!");

        // Match partially on key path
        tree_list.search_box.text = "corner".to_string();
        tree_list.rebuild_tree();
        assert!(!tree_list.items.is_empty(), "Tree should have items matching 'corner'!");
        let has_corner = tree_list.items.iter().any(|item| match item {
            TreeElement::Leaf { name, .. } => name == "corner_radius",
            _ => false,
        });
        assert!(has_corner, "Tree should contain 'corner_radius' item!");
        let has_accel = tree_list.items.iter().any(|item| match item {
            TreeElement::Leaf { name, .. } => name == "accel_profile",
            _ => false,
        });
        assert!(!has_accel, "Tree should not contain 'accel_profile' item!");

        // Match on value
        tree_list.search_box.text = "flat".to_string();
        tree_list.rebuild_tree();
        let has_accel = tree_list.items.iter().any(|item| match item {
            TreeElement::Leaf { name, .. } => name == "accel_profile",
            _ => false,
        });
        assert!(has_accel, "Tree should contain 'accel_profile' when matching on value 'flat'!");

        // Collapse matching section when filtered
        tree_list.search_box.text = "corner".to_string();
        tree_list.collapsed_sections.insert("style.data.tree".to_string());
        tree_list.rebuild_tree();
        let has_corner = tree_list.items.iter().any(|item| match item {
            TreeElement::Leaf { name, .. } => name == "corner_radius",
            _ => false,
        });
        assert!(!has_corner, "Tree should NOT contain 'corner_radius' item when its parent section 'style.data.tree' is collapsed!");
        
        let has_collapsed_section = tree_list.items.iter().any(|item| match item {
            TreeElement::Section { path, collapsed, .. } => path == "style.data.tree" && *collapsed,
            _ => false,
        });
        assert!(has_collapsed_section, "Tree should contain 'style.data.tree' collapsed section!");
    }

    #[test]
    fn test_treelist_double_click_rename() {
        let mut ctx = UiContext::new();
        let mut tree_list = TreeList::new();
        tree_list.set_rect(0.0, 0.0, 380.0, 500.0);
        tree_list.set_flat_keys(vec![
            ("style.control.dropdown.color".to_string(), serde_json::Value::String("#ff00ff".to_string()))
        ]);

        // 1. Test renaming a section (row 0)
        let list_top = tree_list.scroll_box.viewport_y;
        let py0 = list_top + 10.0;
        tree_list.mouse_input(MouseButton::Left, ElementState::Pressed, 10.0, py0, &mut ctx);
        std::thread::sleep(std::time::Duration::from_millis(10));
        tree_list.mouse_input(MouseButton::Left, ElementState::Pressed, 10.0, py0, &mut ctx);

        assert!(tree_list.editing_key_idx.is_some());
        assert_eq!(tree_list.edit_box.text, "style"); // Pre-populated with relative name!

        tree_list.edit_box.text = "theme".to_string();
        tree_list.edit_box.edit_buffer = "theme".to_string();
        tree_list.edit_box.editing = false;
        tree_list.tick(0.016, &mut ctx);

        let req = tree_list.take_rename_request();
        assert_eq!(req, Some(("style".to_string(), "theme".to_string())));

        // 2. Test renaming a leaf (row 3)
        tree_list.rebuild_tree();
        let py3 = list_top + 3.0 * tree_list.item_height + 10.0; // Click row 3 (Leaf "color")
        tree_list.mouse_input(MouseButton::Left, ElementState::Pressed, 10.0, py3, &mut ctx);
        std::thread::sleep(std::time::Duration::from_millis(10));
        tree_list.mouse_input(MouseButton::Left, ElementState::Pressed, 10.0, py3, &mut ctx);

        assert!(tree_list.editing_key_idx.is_some());
        assert_eq!(tree_list.edit_box.text, "color"); // Pre-populated with relative name "color"!

        tree_list.edit_box.text = "bg_color".to_string();
        tree_list.edit_box.edit_buffer = "bg_color".to_string();
        tree_list.edit_box.editing = false;
        tree_list.tick(0.016, &mut ctx);

        let req = tree_list.take_rename_request();
        assert_eq!(req, Some(("style.control.dropdown.color".to_string(), "style.control.dropdown.bg_color".to_string())));
    }
}
