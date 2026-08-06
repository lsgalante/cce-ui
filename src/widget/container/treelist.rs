use crate::widget::*;
use crate::widget::container::scroll_box::ScrollBox;
use crate::widget::display::TextLabel;
use crate::widget::model::{Adapted, EventCtx, Input, Layout, Paint};
use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
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
    pub last_scroll_y: f32,
    pub scrollbar_activity_timer: f32,
    /// Lights the recess rim (`paint` has no UiContext, so this mirrors focus).
    /// Driven by FocusIn/FocusOut by default; an app whose inline editors float
    /// over the tree (cce-data-editor) overrides it per input event with its own
    /// focus-within computation — those editors take ctx focus away from the
    /// tree while still being, visually, part of the tree pane.
    pub focused: bool,
    pub deleted_key_path: Option<String>,
    pub edit_box: crate::widget::Adapted<TextBox>,
    pub editing_key_idx: Option<usize>,
    pub double_click_timer: Option<(std::time::Instant, usize)>,
    pub rename_request: Option<(String, String)>,
}

impl TreeList {
    pub fn new() -> Adapted<TreeList> {
        let mut scroll_box = ScrollBox::new();
        scroll_box.show_background = false;
        Adapted::new(TreeList {
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
            last_scroll_y: 0.0,
            scrollbar_activity_timer: 0.0,
            focused: false,
            deleted_key_path: None,
            edit_box: TextBox::new(String::new()).with_multiline(false).with_draw_bg_border(true),
            editing_key_idx: None,
            double_click_timer: None,
            rename_request: None,
        })
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
        let h = self.base.h;
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

impl TreeList {
    fn tree_radius(&self) -> f32 {
        crate::layout::tree_corner_radius()
    }

    fn tree_corners(&self) -> (bool, bool, bool, bool) {
        (true, true, true, true)
    }

    fn tree_border(&self) -> Option<([f32; 4], f32)> {
        if self.scroll_box.show_border {
            Some((crate::color::tree_border_color(), 1.0))
        } else {
            None
        }
    }

    fn mouse_body(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ui: &mut UiContext, host: Option<*mut (dyn WidgetHost + 'static)>, host_id: WidgetId) -> bool {
        let _ = host_id;
        if self.editing_key_idx.is_some() {
            if button == MouseButton::Left && state == ElementState::Pressed {
                let (ex, ey, ew, eh) = self.edit_box.rect();
                if px >= ex && px <= ex + ew && py >= ey && py <= ey + eh {
                    if self.edit_box.mouse_input(button, state, px, py, ui) {
                        return true;
                    }
                } else {
                    ui.clear_focus();
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
                    ui.clear_focus();
                    changed = true;
                } else {
                    if self.add_key_popover_box.mouse_input(button, state, px, py, ui) {
                        ui.set_focused(&mut self.add_key_popover_box);
                        changed = true;
                    }
                }
            }
            let (px_rect, py_rect, pw, ph) = self.popover_rect_geom();
            if px >= px_rect && px <= px_rect + pw && py >= py_rect && py <= py_rect + ph {
                return true;
            }
        }

        if self.scroll_box.mouse_input(button, state, px, py, ui) {
            changed = true;
        }
        if self.search_box.mouse_input(button, state, px, py, ui) {
            ui.set_focused(&mut self.search_box);
            changed = true;
        }
        if self.add_key_btn.mouse_input(button, state, px, py, ui) {
            if self.add_key_btn.take_click() {
                self.add_key_popover_open = !self.add_key_popover_open;
                if self.add_key_popover_open {
                    self.add_key_popover_box.text.clear();
                    self.add_key_popover_box.edit_buffer.clear();
                    self.add_key_popover_box.cursor_idx = 0;
                    self.add_key_popover_box.select_anchor = None;
                    self.add_key_popover_box.all_selected = false;
                    self.add_key_popover_box.editing = true;
                    ui.set_focused(&mut self.add_key_popover_box);
                    self.add_key_popover_box.focus();
                } else {
                    ui.clear_focus();
                }
            }
            changed = true;
        }
        
        let list_left = self.scroll_box.base.x;
        let list_width = self.scroll_box.base.w;
        let list_top = self.scroll_box.viewport_y;
        let list_bottom = self.scroll_box.viewport_y + self.scroll_box.viewport_h;

        // The tree receives every press UNGATED (see gates_presses) for its
        // dismiss/commit semantics, which skips the router's popover-coverage
        // check — honor it here for row interactions, or a click on a menu
        // floating over the tree (the File dropdown) also selects the row
        // beneath it. The dismiss paths above deliberately stay: a covered
        // press IS an outside press for the rename editor and add-key popover.
        let covered = ui.is_coordinate_covered(host_id, px, py);

        if button == MouseButton::Left && state == ElementState::Pressed && !covered {
            let on_scrollbar = self.scroll_box.hit_test_scrollbar(px, py) || self.scroll_box.scrollbar_dragging;
            if !on_scrollbar && px >= list_left && px <= list_left + list_width && py >= list_top && py <= list_bottom {
                if let Some(h) = host { ui.set_focused_ptr(h); }
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
                        
                        let eb_ptr = self.edit_box.as_ptr_mut();
                        let eb_id = self.edit_box.base().id();
                        ui.register_widget(eb_id, eb_ptr);
                        ui.link_ids(host_id, eb_id);
                        
                        ui.set_focused(&mut self.edit_box);
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

        if button == MouseButton::Right && state == ElementState::Pressed && !covered {
            if px >= list_left && px <= list_left + list_width && py >= list_top && py <= list_bottom {
                if let Some(h) = host { ui.set_focused_ptr(h); }
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
                            if let Some(h) = host { ui.show_context_menu(px, py - scroll_offset, options, 1, h); }
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
                            if let Some(h) = host { ui.show_context_menu(px, py - scroll_offset, options, 1, h); }
                            changed = true;
                        }
                    }
                }
            }
        }
        changed
    
    }

    fn move_body(&mut self, px: f32, py: f32, ui: &mut UiContext) -> bool {
        let mut changed = self.scroll_box.on_cursor_moved(px, py, ui);
        if self.search_box.on_cursor_moved(px, py, ui) {
            changed = true;
        }
        if self.add_key_btn.on_cursor_moved(px, py, ui) {
            changed = true;
        }
        if self.add_key_popover_open {
            if self.add_key_popover_box.on_cursor_moved(px, py, ui) {
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

    fn key_body(&mut self, event: &KeyEvent, ui: &mut UiContext) -> bool {
        if self.editing_key_idx.is_some() {
            if self.edit_box.keyboard_input(event, ui) {
                return true;
            }
        }
        if self.add_key_popover_open {
            if self.add_key_popover_box.keyboard_input(event, ui) {
                return true;
            }
        }
        if self.search_box.keyboard_input(event, ui) {
            return true;
        }
        false
    
    }
}

impl Layout for TreeList {
    /// The legacy `set_rect` body: cache the rect on the internal base and arrange the
    /// field widgets (search box, add-key button, popover box, scroll box).
    fn rect_assigned(&mut self, rect: Rect) {
        let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);
        self.base.x = x;
        self.base.y = y;
        self.base.w = w;
        self.base.h = h;
        
        let search_margin_x = 8.0;
        let search_margin_y = 6.0;
        let search_h = 26.0;
        let offset_y = search_h + 2.0 * search_margin_y;
        
        let (btn_family, btn_size) = crate::layout::parse_font_string(&crate::layout::button_font());
        let label_w = crate::widget::display::measure_text_width(
            self.add_key_btn.base().label.as_deref().unwrap_or(""),
            &btn_family,
            btn_size.unwrap_or(12.0),
        );
        let button_width = label_w + 2.0 * crate::layout::button_padding();
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

    /// Keep the field widgets registered/linked under the adapter every tick (the legacy
    /// `set_parent` side effect; also heals the inline rename editor's registry entry).
    fn register_embedded_children(&mut self, host_id: WidgetId, ctx: &mut UiContext) {
        // Registered but deliberately NOT tree-linked (6bd): the tree is a SELF-ROUTING
        // composite — mouse_body/move_body/key_body forward to every field widget
        // internally, so the router's children-first descent double-delivered AND starved
        // the tree-level logic (the recorded 6as latents: the hit add-key button consumed
        // the press before mouse_body's take_click toggle ran, so the popover never
        // opened, and the wheel died the same way). Registration alone keeps the ids
        // resolvable for focus, coverage, and the spatial grid.
        let _ = host_id;
        let sb_ptr = self.search_box.as_ptr_mut();
        let sb_id = self.search_box.base().id();
        ctx.register_widget(sb_id, sb_ptr);

        let btn_ptr = self.add_key_btn.as_ptr_mut();
        let btn_id = self.add_key_btn.base().id();
        ctx.register_widget(btn_id, btn_ptr);

        let pop_ptr = self.add_key_popover_box.as_ptr_mut();
        let pop_id = self.add_key_popover_box.base().id();
        ctx.register_widget(pop_id, pop_ptr);

        if self.editing_key_idx.is_some() {
            let eb_ptr = self.edit_box.as_ptr_mut();
            let eb_id = self.edit_box.base().id();
            ctx.register_widget(eb_id, eb_ptr);
        }
    }
}

impl Paint for TreeList {
    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::tree_font())
    }

    // The field widgets are registered but not tree-linked (self-routing, see
    // register_embedded_children); their pixels come from `paint`'s child pass — the
    // walk must not descend either.
    fn paints_own_subtree(&self) -> bool {
        true
    }

    fn prepare_text(&mut self, fs: &mut glyphon::FontSystem, _rect: Rect) {
        self.search_box.prepare_text(fs);
        self.add_key_btn.prepare_text(fs);
        if self.add_key_popover_open {
            self.add_key_popover_box.prepare_text(fs);
        }
        if self.editing_key_idx.is_some() {
            self.edit_box.prepare_text(fs);
        }
    
    }

    /// The whole tree — container border/background, search/header chrome, virtualized
    /// rows (backgrounds, separators, color previews, button pills), the scrollbar, the
    /// row/header labels, and the field children (search box, add-key button, the add-key
    /// popover box while open, the inline rename editor while editing). Ported verbatim
    /// from the legacy `all_rounded_quads` rounded branch + `subtree_fonted_labels`;
    /// children paint through their own adapters (dummy ctx — none of their paint reads it).
    fn paint(&self, rect: Rect, pc: &mut PaintCtx) {
        let (r1, r2, r3, r4) = self.tree_corners();
        let mut quads: Vec<(f32, f32, f32, f32, f32, [f32; 4], (bool, bool, bool, bool))> = Vec::new();
        let radius = self.tree_radius();
        let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);
        
        let opacity = crate::layout::tree_opacity();
        let apply_opacity = |mut c: [f32; 4]| -> [f32; 4] {
            c[3] *= opacity;
            c
        };

        // 1. Draw container border and background
        if let Some((border_color, thickness)) = self.tree_border() {
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
        for (qx, qy, qw, qh, qr, qc, qcorners) in quads {
            if qr > 0.1 {
                pc.rounded_rect(Rect { x: qx, y: qy, width: qw, height: qh }, qr, qcorners, qc);
            } else {
                pc.quad(Rect { x: qx, y: qy, width: qw, height: qh }, qc);
            }
        }

        // Recessed well like a text box or list: the tree floor sits below the
        // pane surface, its wall carved over the bg and row quads above. Focus
        // lights the rim in the highlight accent instead of washing the tree
        // (the retired legacy_focus_highlight overlay).
        if crate::layout::control_relief() {
            let depth = crate::layout::bevel_width().min(h * 0.2);
            let well = Rect { x, y, width: w, height: h };
            let radii = (radius, radius, radius, radius);
            if self.focused {
                let hc = crate::color::highlight_primary_color();
                pc.recess_tinted(well, radii, depth, [hc[0], hc[1], hc[2]]);
            } else {
                pc.recess(well, radii, depth);
            }
        }

        // Row/header labels with the legacy header/list viewport bounds.
        let font = Some(crate::layout::tree_font());
        let (x, y, w, _h) = (rect.x, rect.y, rect.width, rect.height);
        let search_margin_y = 6.0;
        let search_h = 26.0;
        let offset_y = search_h + 2.0 * search_margin_y;
        let header_h = 26.0;
        let list_bounds = Some([self.scroll_box.base.x, self.scroll_box.viewport_y, self.scroll_box.base.x + self.scroll_box.base.w, self.scroll_box.viewport_y + self.scroll_box.viewport_h]);
        let header_bounds = Some([x, y + offset_y, x + w, y + offset_y + header_h]);
        for (idx, (l, col_max_x)) in self.own_labels().into_iter().enumerate() {
            let mut b = if idx < 3 { header_bounds } else { list_bounds };
            if let (Some(bb), Some(mx)) = (b.as_mut(), col_max_x) {
                bb[2] = bb[2].min(mx);
            }
            pc.text_with(l.text, l.x, l.y, l.font_size, l.color, font.clone(), b);
        }

        // Section chevrons: image icons in the slot own_labels leaves open,
        // clipped to the list viewport like the row text.
        {
            let (_, tree_font_size) = crate::layout::tree_font_parsed();
            let list_left = self.scroll_box.base.x;
            let list_top = self.scroll_box.viewport_y;
            let list_bottom = list_top + self.scroll_box.viewport_h;
            let viewport = Rect {
                x: list_left,
                y: list_top,
                width: self.scroll_box.base.w,
                height: self.scroll_box.viewport_h,
            };
            pc.clip(viewport, |pc| {
                for (i, item) in self.items.iter().enumerate() {
                    let row_y = list_top + i as f32 * self.item_height - self.scroll_box.scroll_y;
                    if row_y + self.item_height < list_top || row_y > list_bottom {
                        continue;
                    }
                    if let TreeElement::Section { indent, collapsed, .. } = item {
                        if self.editing_key_idx == Some(i) {
                            continue;
                        }
                        if let Some((id, _, _)) = Self::chevron_icon(*collapsed) {
                            let s = tree_font_size;
                            pc.image(id, Rect {
                                x: list_left + 8.0 + *indent as f32 * 12.0,
                                y: row_y + 7.0,
                                width: s,
                                height: s,
                            }, 1.0);
                        }
                    }
                }
            });
        }

        // Field children, in the legacy children() order.
        let dummy = UiContext::new();
        self.search_box.paint_self(&dummy, pc);
        self.add_key_btn.paint_self(&dummy, pc);
        if self.add_key_popover_open {
            self.add_key_popover_box.paint_self(&dummy, pc);
        }
        if self.editing_key_idx.is_some() {
            self.edit_box.paint_self(&dummy, pc);
        }
    }

    fn popover(&self, _rect: Rect) -> Option<(f32, f32, f32, f32)> {
        if self.add_key_popover_open {
            Some(self.popover_rect_geom())
        } else {
            None
        }
    
    }

    fn draw_popover(&self, _rect: Rect, pc: &mut dyn crate::layout::RenderTarget) {
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

impl Input for TreeList {
    fn blocks_backplate_drag(&self) -> bool {
        true
    }

    /// The tree must see every press: outside presses dismiss the add-key popover and
    /// commit/cancel the inline rename editor (the legacy ungated `mouse_input` contract).
    fn gates_presses(&self) -> bool {
        false
    }

    fn wants_tick(&self) -> bool {
        true
    }

    fn draggable(&self, _rect: Rect) -> bool {
        self.scroll_box.draggable()
    }

    fn is_dragging(&self) -> bool {
        self.scroll_box.is_dragging()
    }

    fn drag_begin(&mut self, px: f32, py: f32, _rect: Rect) {
        self.scroll_box.drag_begin(px, py);
    }

    fn drag_update(&mut self, px: f32, py: f32, _rect: Rect) -> bool {
        self.scroll_box.drag_update(px, py)
    }

    fn drag_end(&mut self) {
        self.scroll_box.drag_end();
    }

    /// The legacy `tick` body: advances the field widgets, drains the add-key popover and
    /// search box, positions/commits the inline rename editor (re-targeting focus to the
    /// adapter on commit), and runs the scrollbar activity fade.
    fn tick_ctx(&mut self, dt: f32, ectx: &mut EventCtx) -> bool {
        let host = ectx.host_ptr();
        let Some(ui) = ectx.ui.as_deref_mut() else {
            return false;
        };
        let mut changed = false;
        if self.search_box.tick(dt, ui) {
            changed = true;
        }
        if self.add_key_btn.tick(dt, ui) {
            changed = true;
        }
        if self.add_key_popover_open {
            if self.add_key_popover_box.tick(dt, ui) {
                changed = true;
            }
            if !self.add_key_popover_box.editing {
                let path = self.add_key_popover_box.text.trim().to_string();
                if !path.is_empty() {
                    self.new_key_path_request = Some(path);
                }
                self.add_key_popover_open = false;
                ui.clear_focus();
                changed = true;
            }
        }
        if self.search_box.take_change() {
            self.rebuild_tree();
            changed = true;
        }
        
        if self.editing_key_idx.is_some() {
            if self.edit_box.tick(dt, ui) {
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
                if let Some(h) = host { ui.set_focused_ptr(h); }
                changed = true;
            }
        }

        if (self.scroll_box.scroll_y - self.last_scroll_y).abs() > 0.01 {
            self.last_scroll_y = self.scroll_box.scroll_y;
            self.scrollbar_activity_timer = 1.0;
            changed = true;
        }
        if self.scrollbar_activity_timer > 0.0 {
            self.scrollbar_activity_timer = (self.scrollbar_activity_timer - dt).max(0.0);
            changed = true;
        }
        changed
    
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        let host = ectx.host_ptr();
        let host_id = ectx.id;
        match event {
            Event::MouseButton { button, state, x, y, .. } => {
                let (button, state, px, py) = (*button, *state, *x, *y);
                let Some(ui) = ectx.ui.as_deref_mut() else { return false; };
                self.mouse_body(button, state, px, py, ui, host, host_id)
            }
            Event::PointerMove { x, y, .. } => {
                let (px, py) = (*x, *y);
                let Some(ui) = ectx.ui.as_deref_mut() else { return false; };
                self.move_body(px, py, ui)
            }
            Event::MouseWheel { delta, x, y, .. } => {
                let (delta, px, py) = (delta.clone(), *x, *y);
                let Some(ui) = ectx.ui.as_deref_mut() else { return false; };
                self.scroll_box.mouse_wheel(&delta, px, py, ui)
            }
            Event::KeyInput(ev) => {
                let ev = ev.clone();
                let Some(ui) = ectx.ui.as_deref_mut() else { return false; };
                self.key_body(&ev, ui)
            }
            Event::FocusIn => {
                self.focused = true;
                false
            }
            Event::FocusOut => {
                self.focused = false;
                self.add_key_popover_open = false;
                self.add_key_popover_box.unfocus();
                false
            }
            _ => false,
        }
    }

    fn context_action(&mut self, action: crate::widget::ContextAction) -> bool {
        use crate::widget::ContextAction as CA;
        match action {
            CA::CopyKey => self.copy_key(),
            CA::CopyValue => self.copy_value(),
            CA::DeleteKey => self.delete_key(),
            CA::ExpandNode => self.expand_node(),
            CA::CollapseNode => self.collapse_node(),
            CA::ExpandAll => self.expand_all_nodes(),
            CA::CollapseAll => self.collapse_all_nodes(),
            _ => return false,
        }
        true
    }
}

impl TreeList {
    // The tree context-menu actions, dispatched by the global context menu through
    // `Input::context_action`.
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
    use crate::context::UiContext;

    #[test]
    fn test_treelist_blocks_window_drag() {
        // Backplate is DELETED: dissolved windows ask `drag_allowed_at` instead — same
        // walk, minus the registered-movable-Backplate requirement.
        let mut ctx = UiContext::new();
        let mut tree_list = TreeList::new();
        tree_list.set_rect(10.0, 52.0, 380.0, 500.0);

        ctx.register_widget(tree_list.base().id(), tree_list.as_ptr_mut());
        ctx.tick(0.016);
        ctx.clear_dirty();

        assert!(!ctx.drag_allowed_at(100.0, 200.0), "clicking the TreeList must block the window drag");
        assert!(ctx.drag_allowed_at(600.0, 300.0), "empty surface stays draggable");
    }

    #[test]
    fn test_exact_app_layout_blocks_drag() {
        // The data-editor shape: a parentless tree registered directly (dissolved root).
        let mut ctx = UiContext::new();
        let mut tree_list = TreeList::new();

        ctx.register_widget(tree_list.base().id(), tree_list.as_ptr_mut());
        ctx.rebuild_spatial_grid();

        let list_top = 52.0;
        let list_bottom = 600.0 - 180.0;
        tree_list.set_rect(10.0, list_top, 380.0, list_bottom - list_top);
        ctx.rebuild_spatial_grid();

        assert!(!ctx.drag_allowed_at(100.0, 200.0), "clicking the TreeList under the app layout must block the drag");
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
        
        let labels = tree_list.own_labels();
        for label in &labels {
            println!("TEST LABEL: {:?}", label);
        }
        
        let has_menu_label = labels.iter().any(|(l, _)| l.text == "(menu)");
        assert!(has_menu_label, "Should have (menu) label!");
    }

    #[test]
    fn test_treelist_headers() {
        let tree_list = TreeList::new();
        let labels = tree_list.own_labels();
        assert!(labels.iter().any(|(l, _)| l.text == "Key"), "Should have Key header!");
        assert!(labels.iter().any(|(l, _)| l.text == "Type"), "Should have Type header!");
        assert!(labels.iter().any(|(l, _)| l.text == "Value"), "Should have Value header!");
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

impl TreeList {
    /// Row/header labels plus each label's column clip: the x where its
    /// column ends (None = only the shared list/header bounds apply). Key and
    /// Type cells clip at their separators so text can't bleed into the next
    /// column; sections span the whole row.
    /// The section-row chevron (cce-icons), cached per size by `upload_icon`;
    /// `None` when the icon set is missing (rows fall back to text triangles).
    fn chevron_icon(collapsed: bool) -> Option<(u32, u32, u32)> {
        crate::upload_icon(if collapsed { "chevron-right" } else { "chevron-down" }, 32)
    }

    pub(crate) fn own_labels(&self) -> Vec<(TextLabel, Option<f32>)> {
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

        labels.push((TextLabel {
            text: "Key".to_string(),
            x: list_left + 8.0,
            y: self.base.y + offset_y + 6.0,
            font_size: header_font_size,
            color: [200, 200, 210],
        }, None));
        labels.push((TextLabel {
            text: "Type".to_string(),
            x: list_left + 180.0 + 8.0,
            y: self.base.y + offset_y + 6.0,
            font_size: header_font_size,
            color: [200, 200, 210],
        }, None));
        labels.push((TextLabel {
            text: "Value".to_string(),
            x: list_left + 235.0 + 8.0,
            y: self.base.y + offset_y + 6.0,
            font_size: header_font_size,
            color: [200, 200, 210],
        }, None));

        for (i, item) in self.items.iter().enumerate() {
            let row_y = list_top + i as f32 * self.item_height - self.scroll_box.scroll_y;
            if row_y + self.item_height < list_top || row_y > list_bottom {
                continue;
            }

            match item {
                TreeElement::Section { name, indent, collapsed, .. } => {
                    if self.editing_key_idx != Some(i) {
                        let sx = list_left + 8.0 + *indent as f32 * 12.0;
                        // Chevron icons (cce-icons) replace the text triangles
                        // when available — paint() draws the image in the slot
                        // this leaves open. Text triangles are the fallback.
                        let (text, tx) = if Self::chevron_icon(*collapsed).is_some() {
                            (name.clone(), sx + tree_font_size + 6.0)
                        } else {
                            (format!("{} {}", if *collapsed { "▶" } else { "▼" }, name), sx)
                        };
                        labels.push((TextLabel {
                            text,
                            x: tx,
                            y: row_y + 6.0,
                            font_size: tree_font_size,
                            color: f32_to_rgb(crate::color::tree_section_text_color()),
                        }, None));
                    }
                }
                TreeElement::Leaf { name, indent, val, original_idx, .. } => {
                    let val_str = serde_json::to_string(val).unwrap_or_default();
                    // Generous shaping cap only — the column bounds clip the
                    // visible text at the list edge. (char-based: the old
                    // byte slice could panic on multibyte text.)
                    let display_val = if val_str.chars().count() > 120 {
                        let cut: String = val_str.chars().take(117).collect();
                        format!("{}...", cut)
                    } else {
                        val_str
                    };

                    let color = if Some(*original_idx) == self.selected_key_idx {
                        f32_to_rgb(crate::color::tree_leaf_text_selected_color())
                    } else {
                        f32_to_rgb(crate::color::tree_leaf_text_color())
                    };

                    if self.editing_key_idx != Some(i) {
                        labels.push((TextLabel {
                            text: name.clone(),
                            x: list_left + 8.0 + *indent as f32 * 12.0,
                            y: row_y + 6.0,
                            font_size: tree_font_size,
                            color,
                        }, Some(list_left + 178.0)));
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
                        labels.push((TextLabel {
                            text: ty_text,
                            x: list_left + 190.0,
                            y: row_y + 6.0,
                            font_size: tree_font_size,
                            color: f32_to_rgb(crate::color::tree_type_text_color()),
                        }, Some(list_left + 233.0)));
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
                            labels.push((TextLabel {
                                text: display_val,
                                x: list_left + 245.0 + 8.0,
                                y: row_y + 6.0,
                                font_size: tree_font_size,
                                color: [240, 240, 245],
                            }, None));
                        } else {
                            labels.push((TextLabel {
                                text: display_val,
                                x: label_x,
                                y: row_y + 6.0,
                                font_size: tree_font_size,
                                color: f32_to_rgb(crate::color::tree_value_text_color()),
                            }, None));
                        }
                    }
                }
            }
        }
        labels
    }

}
