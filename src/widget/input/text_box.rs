use crate::widget::*;
use std::sync::OnceLock;

static FONT_DB: OnceLock<resvg::usvg::fontdb::Database> = OnceLock::new();

pub fn get_font_db() -> &'static resvg::usvg::fontdb::Database {
    FONT_DB.get_or_init(|| {
        let mut db = resvg::usvg::fontdb::Database::new();
        db.load_system_fonts();
        db.load_fonts_dir("/home/lsgalante/Dropbox/Fonts");
        db
    })
}

#[derive(Debug, Clone)]
pub struct TextBox {
    base: Widget,
    pub text: String,
    pub editing: bool,
    pub edit_buffer: String,
    pub(crate) just_changed: bool,
    pub disabled: bool,
    pub all_selected: bool,
    pub cursor_idx: usize,
    pub select_anchor: Option<usize>,
    pub dragging: bool,
    pub just_focused: bool,
    pub drag_start_idx: Option<usize>,
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub children: Vec<*mut (dyn Element + 'static)>,
    pub max_width: Option<f32>,
    pub width: Option<f32>,
    pub is_password: bool,
    pub multiline: bool,
    pub draw_bg_border: bool,
    pub text_color: Option<[u8; 3]>,
    pub font_size: f32,
    pub font_family: String,
    pub placeholder: Option<String>,
    pub editor_state: TextEditorState,
    pub scroll_y: f32,
    pub scroll_x: f32,
    default_font_size: f32,
    default_font_family: String,
    pub cursor_x_offset: f32,
    pub glyph_positions: Vec<f32>,
    pub total_text_width: f32,
    pub update_on_type: bool,
}

impl TextBox {
    pub fn new(text: String) -> Self {
        let (style_family, style_size) = crate::layout::control_label_font_detached_parsed();
        let editor_state = TextEditorState::new(text.clone());
        Self {
            base: Widget::new(),
            text,
            editing: false,
            edit_buffer: String::new(),
            just_changed: false,
            disabled: false,
            all_selected: false,
            cursor_idx: 0,
            select_anchor: None,
            dragging: false,
            just_focused: false,
            drag_start_idx: None,
            parent: None,
            children: Vec::new(),
            max_width: None,
            width: None,
            is_password: false,
            multiline: false,
            draw_bg_border: true,
            text_color: None,
            font_size: style_size,
            font_family: style_family.clone(),
            placeholder: None,
            editor_state,
            scroll_y: 0.0,
            scroll_x: 0.0,
            default_font_size: style_size,
            default_font_family: style_family,
            cursor_x_offset: 0.0,
            glyph_positions: Vec::new(),
            total_text_width: 0.0,
            update_on_type: false,
        }
    }

    pub fn with_update_on_type(mut self, update: bool) -> Self {
        self.update_on_type = update;
        self
    }

    pub fn with_multiline(mut self, multiline: bool) -> Self {
        self.multiline = multiline;
        self
    }

    fn map_x_to_idx(&self, click_x: f32) -> usize {
        let label_x = self.label_x_offset();
        let relative_x = click_x - (self.base.x + label_x + 8.0) + self.scroll_x;
        if self.glyph_positions.is_empty() {
            let char_width = self.char_width();
            return ((relative_x / char_width).round() as isize)
                .max(0)
                .min(self.edit_buffer.chars().count() as isize) as usize;
        }
        
        let mut closest_idx = 0;
        let mut min_diff = f32::MAX;
        for (i, &pos) in self.glyph_positions.iter().enumerate() {
            let diff = (pos - relative_x).abs();
            if diff < min_diff {
                min_diff = diff;
                closest_idx = i;
            }
        }
        closest_idx
    }

    pub fn with_draw_bg_border(mut self, draw: bool) -> Self {
        self.draw_bg_border = draw;
        self
    }

    pub fn with_text_color(mut self, color: Option<[u8; 3]>) -> Self {
        self.text_color = color;
        self
    }
    pub fn with_font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    pub fn with_font_family(mut self, family: String) -> Self {
        self.font_family = family;
        self
    }

    pub fn char_width(&self) -> f32 {
        crate::widget::display::measure_text_width("M", &self.font_family, self.font_size)
    }

    pub fn line_height(&self) -> f32 {
        self.font_size * 1.333
    }

    pub fn wrap_text(&self, max_chars_per_line: usize) -> (Vec<String>, Vec<(usize, usize)>) {
        let text_src = if self.editing { &self.edit_buffer } else { &self.text };
        let chars: Vec<char> = text_src.chars().collect();
        let mut lines = Vec::new();
        let mut current_line = Vec::new();
        let mut index_map = vec![(0, 0); chars.len() + 1];
        
        if !self.line_wrap_enabled() {
            let mut i = 0;
            while i < chars.len() {
                let ch = chars[i];
                if ch == '\n' {
                    index_map[i] = (lines.len(), current_line.len());
                    lines.push(current_line.iter().collect::<String>());
                    current_line.clear();
                } else {
                    current_line.push(ch);
                    index_map[i] = (lines.len(), current_line.len() - 1);
                }
                i += 1;
            }
            index_map[chars.len()] = (lines.len(), current_line.len());
            lines.push(current_line.iter().collect::<String>());
            return (lines, index_map);
        }
        
        let max_chars = max_chars_per_line.max(1);
        
        let mut i = 0;
        while i < chars.len() {
            let ch = chars[i];
            
            if ch == '\n' {
                index_map[i] = (lines.len(), current_line.len());
                lines.push(current_line.iter().collect::<String>());
                current_line.clear();
                i += 1;
                continue;
            }
            
            current_line.push(ch);
            index_map[i] = (lines.len(), current_line.len() - 1);
            
            if current_line.len() > max_chars {
                let mut space_idx = None;
                for (s_idx, &c) in current_line.iter().enumerate().rev() {
                    if c.is_whitespace() {
                        space_idx = Some(s_idx);
                        break;
                    }
                }
                
                if let Some(s_idx) = space_idx {
                    let line_to_push: Vec<char> = current_line[0..s_idx + 1].to_vec();
                    let remaining: Vec<char> = current_line[s_idx + 1..].to_vec();
                    
                    let line_idx = lines.len();
                    lines.push(line_to_push.iter().collect::<String>());
                    
                    current_line = remaining;
                    let start_orig = i - current_line.len() + 1;
                    for c_idx in 0..current_line.len() {
                        index_map[start_orig + c_idx] = (line_idx + 1, c_idx);
                    }
                } else {
                    let line_to_push: Vec<char> = current_line[0..max_chars].to_vec();
                    let remaining: Vec<char> = current_line[max_chars..].to_vec();
                    
                    let line_idx = lines.len();
                    lines.push(line_to_push.iter().collect::<String>());
                    
                    current_line = remaining;
                    let start_orig = i - current_line.len() + 1;
                    for c_idx in 0..current_line.len() {
                        index_map[start_orig + c_idx] = (line_idx + 1, c_idx);
                    }
                }
            }
            i += 1;
        }
        
        index_map[chars.len()] = (lines.len(), current_line.len());
        lines.push(current_line.iter().collect::<String>());
        
        (lines, index_map)
    }

    pub fn map_2d_to_1d(&self, index_map: &[(usize, usize)], target_line: usize, target_col: usize, max_line_idx: usize) -> usize {
        let line = target_line.min(max_line_idx);
        let mut best_idx = 0;
        let mut best_dist = usize::MAX;
        
        for (i, &(l, c)) in index_map.iter().enumerate() {
            if l == line {
                let dist = (c as isize - target_col as isize).abs() as usize;
                if dist < best_dist {
                    best_dist = dist;
                    best_idx = i;
                }
            }
        }
        best_idx
    }

    pub fn with_password(mut self, is_password: bool) -> Self {
        self.is_password = is_password;
        self
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self
    }

    pub fn with_config(mut self, file: &str, key: &str) -> Self {
        self.base.config_file = Some(file.to_string());
        self.base.config_key = Some(key.to_string());
        self
    }

    pub fn set_label(&mut self, label: &str) {
        self.base.label = Some(label.to_string());
    }

    pub fn with_placeholder(mut self, placeholder: &str) -> Self {
        self.placeholder = Some(placeholder.to_string());
        self
    }

    fn border_width(&self) -> f32 {
        if self.multiline {
            crate::layout::textbox_multiline_border_width()
        } else {
            1.0
        }
    }

    pub fn set_placeholder(&mut self, placeholder: &str) {
        self.placeholder = Some(placeholder.to_string());
    }

    pub fn take_change(&mut self) -> bool {
        if self.update_on_type {
            if self.text != self.edit_buffer {
                self.text = self.edit_buffer.clone();
                self.just_changed = true;
            }
        }
        let changed = self.just_changed;
        self.just_changed = false;
        changed
    }

    pub fn line_wrap_enabled(&self) -> bool {
        self.multiline && crate::layout::textbox_line_wrap()
    }

    pub fn with_max_width(mut self, max_w: Option<f32>) -> Self {
        self.max_width = max_w;
        self
    }

    pub fn set_max_width(&mut self, max_w: Option<f32>) {
        self.max_width = max_w;
    }

    pub fn with_width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }

    pub fn set_width(&mut self, w: f32) {
        self.width = Some(w);
    }

    pub fn sync_editor_state(&mut self) {
        self.editor_state = TextEditorState {
            buffer: self.edit_buffer.clone(),
            cursor_idx: self.cursor_idx,
            select_anchor: self.select_anchor,
            all_selected: self.all_selected,
        };
    }

    pub fn copy_selection(&self) {
        let state = TextEditorState {
            buffer: self.edit_buffer.clone(),
            cursor_idx: self.cursor_idx,
            select_anchor: self.select_anchor,
            all_selected: self.all_selected,
        };
        if let Some(text) = state.selected_text() {
            clipboard::copy_to_clipboard(&text);
        }
    }

    pub fn cut_selection(&mut self) -> bool {
        let mut state = TextEditorState {
            buffer: std::mem::take(&mut self.edit_buffer),
            cursor_idx: self.cursor_idx,
            select_anchor: self.select_anchor,
            all_selected: self.all_selected,
        };
        if let Some(text) = state.selected_text() {
            clipboard::copy_to_clipboard(&text);
            state.insert_text("");
            self.edit_buffer = state.buffer;
            self.cursor_idx = state.cursor_idx;
            self.select_anchor = state.select_anchor;
            self.all_selected = state.all_selected;
            self.sync_editor_state();
            true
        } else {
            self.edit_buffer = state.buffer;
            self.sync_editor_state();
            false
        }
    }

    pub fn paste_from_clipboard(&mut self) -> bool {
        if let Some(text) = clipboard::read_from_clipboard() {
            let mut state = TextEditorState {
                buffer: std::mem::take(&mut self.edit_buffer),
                cursor_idx: self.cursor_idx,
                select_anchor: self.select_anchor,
                all_selected: self.all_selected,
            };
            let mut cleaned = String::new();
            for ch in text.chars() {
                if !ch.is_control() && ch != '\n' && ch != '\r' {
                    cleaned.push(ch);
                }
            }
            state.insert_text(&cleaned);
            self.edit_buffer = state.buffer;
            self.cursor_idx = state.cursor_idx;
            self.select_anchor = state.select_anchor;
            self.all_selected = state.all_selected;
            self.sync_editor_state();
            true
        } else {
            false
        }
    }

    pub fn select_all(&mut self) {
        let mut state = TextEditorState {
            buffer: std::mem::take(&mut self.edit_buffer),
            cursor_idx: self.cursor_idx,
            select_anchor: self.select_anchor,
            all_selected: self.all_selected,
        };
        state.select_all();
        self.edit_buffer = state.buffer;
        self.cursor_idx = state.cursor_idx;
        self.select_anchor = state.select_anchor;
        self.all_selected = state.all_selected;
        self.sync_editor_state();
    }

    pub fn clamp_scroll(&mut self) {
        let char_width = self.char_width();
        let line_height = self.line_height();
        let max_chars = if self.line_wrap_enabled() {
            (((self.base.w - 16.0) / char_width).floor() as usize).max(1)
        } else {
            999999
        };
        let (lines, _) = if self.multiline {
            self.wrap_text(max_chars)
        } else {
            let buffer = if self.editing { &self.edit_buffer } else { &self.text };
            (vec![buffer.clone()], vec![(0, 0); buffer.chars().count() + 1])
        };

        if self.multiline {
            let content_h = lines.len() as f32 * line_height;
            let max_scroll = (content_h - (self.base.h - 16.0)).max(0.0);
            self.scroll_y = self.scroll_y.clamp(0.0, max_scroll);
        } else {
            self.scroll_y = 0.0;
        }

        if !self.line_wrap_enabled() {
            let max_line_len = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
            let content_w = max_line_len as f32 * char_width;
            let max_scroll_x = (content_w - (self.base.w - 16.0)).max(0.0);
            self.scroll_x = self.scroll_x.clamp(0.0, max_scroll_x);
        } else {
            self.scroll_x = 0.0;
        }
    }

    pub fn scroll_to_cursor(&mut self) {
        let char_width = self.char_width();
        let line_height = self.line_height();
        let max_chars = if self.line_wrap_enabled() {
            (((self.base.w - 16.0) / char_width).floor() as usize).max(1)
        } else {
            999999
        };
        let (_lines, index_map) = if self.multiline {
            self.wrap_text(max_chars)
        } else {
            let buffer = if self.editing { &self.edit_buffer } else { &self.text };
            let mut m = Vec::new();
            for i in 0..=buffer.chars().count() {
                m.push((0, i));
            }
            (vec![buffer.clone()], m)
        };
        if index_map.is_empty() { return; }
        
        let cursor_idx = self.cursor_idx.min(index_map.len() - 1);
        let (line_idx, col_idx) = index_map[cursor_idx];
        
        let top = self.base.label_offset();
        let viewport_w = self.base.w - 16.0;
        let viewport_h = self.base.h - top - 16.0;
        
        if self.multiline {
            let line_y = top + 8.0 + (line_idx as f32 * line_height);
            if line_y < self.scroll_y + 10.0 {
                self.scroll_y = (line_y - 20.0).max(0.0);
            } else if line_y + line_height > self.scroll_y + viewport_h - 10.0 {
                self.scroll_y = (line_y + line_height - viewport_h + 20.0).max(0.0);
            }
        }
        
        if !self.line_wrap_enabled() {
            let cursor_x = col_idx as f32 * char_width;
            if cursor_x < self.scroll_x + 10.0 {
                self.scroll_x = (cursor_x - 20.0).max(0.0);
            } else if cursor_x + char_width > self.scroll_x + viewport_w - 10.0 {
                self.scroll_x = (cursor_x + char_width - viewport_w + 20.0).max(0.0);
            }
        }
        self.clamp_scroll();
    }
}

impl Default for TextBox {
    fn default() -> Self {
        Self::new(String::new())
    }
}

impl Element for TextBox {
    crate::impl_widget_base!(TextBox);

    fn prepare_text(&mut self, fs: &mut glyphon::FontSystem) {
        let (style_family, style_size) = crate::layout::control_label_font_detached_parsed();
        if self.font_size == self.default_font_size {
            self.font_size = style_size;
        }
        self.default_font_size = style_size;

        if self.font_family == self.default_font_family {
            self.font_family = style_family.clone();
        }
        self.default_font_family = style_family;

        let text_src = if self.editing { &self.edit_buffer } else { &self.text };
        let display_text = if text_src.is_empty() && self.placeholder.is_some() {
            self.placeholder.as_ref().unwrap().as_str()
        } else {
            text_src.as_str()
        };

        let font_fam = if self.is_password {
            "monospace"
        } else {
            self.font_family.as_str()
        };

        let render_text = if self.is_password {
            "•".repeat(display_text.chars().count())
        } else {
            display_text.to_string()
        };

        let buffer = crate::widget::display::text_label::make_widget_text_buffer(fs, &render_text, self.font_size, font_fam);

        let char_count = render_text.chars().count();
        let mut x_offsets = vec![0.0; char_count + 1];
        let mut total_w: f32 = 0.0;
        let scale = crate::scale::scale_factor().max(1.0);

        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                let byte_offset = glyph.start;
                let c_idx = render_text[..byte_offset.min(render_text.len())].chars().count();
                if c_idx < x_offsets.len() {
                    x_offsets[c_idx] = glyph.x / scale;
                }
                total_w = total_w.max((glyph.x + glyph.w) / scale);
            }
        }

        let mut current_x = 0.0;
        for i in 0..x_offsets.len() {
            if x_offsets[i] == 0.0 && i > 0 {
                x_offsets[i] = current_x;
            } else {
                current_x = x_offsets[i];
            }
        }

        if !x_offsets.is_empty() {
            let last_idx = x_offsets.len() - 1;
            x_offsets[last_idx] = total_w;
        }

        self.glyph_positions = x_offsets;
        self.total_text_width = total_w;

        let cursor_pos = self.cursor_idx.min(self.glyph_positions.len() - 1);
        self.cursor_x_offset = self.glyph_positions.get(cursor_pos).copied().unwrap_or(0.0);

    }

    fn get_value_string(&self) -> Option<String> {
        Some(self.text.clone())
    }

    fn set_value_string(&mut self, val: &str) -> bool {
        let val_str = val.to_string();
        if self.text != val_str {
            self.text = val_str.clone();
            self.edit_buffer = val_str;
            self.just_changed = true;
            let len = self.edit_buffer.chars().count();
            self.cursor_idx = self.cursor_idx.min(len);
            if let Some(anchor) = self.select_anchor {
                self.select_anchor = Some(anchor.min(len));
            }
            if self.cursor_idx == 0 && self.select_anchor == Some(0) {
                self.all_selected = false;
            }
            self.sync_editor_state();
            self.clamp_scroll();
            true
        } else {
            false
        }
    }

    fn take_change(&mut self) -> bool {
        self.take_change()
    }

    fn cut_selection(&mut self) -> bool {
        let res = self.cut_selection();
        if res {
            self.just_changed = true;
        }
        res
    }

    fn copy_selection(&self) {
        self.copy_selection();
    }

    fn paste_from_clipboard(&mut self) -> bool {
        let res = self.paste_from_clipboard();
        if res {
            self.just_changed = true;
        }
        res
    }

    fn select_all(&mut self) {
        self.select_all();
    }

    fn clear_text(&mut self) {
        self.set_value_string("");
    }

    fn preferred_height(&self) -> Option<f32> {
        Some(crate::layout::textbox_height())
    }

    fn rounded_corners(&self) -> (bool, bool, bool, bool) {
        let r = crate::layout::textbox_corner_radius();
        if r > 0.0 {
            (true, true, true, true)
        } else {
            (false, false, false, false)
        }
    }

    fn corner_radius(&self) -> f32 {
        crate::layout::textbox_corner_radius()
    }

    fn rect(&self) -> (f32, f32, f32, f32) { (self.base.x, self.base.y, self.base.w, self.base.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let final_w = if let Some(explicit_w) = self.width {
            explicit_w
        } else if let Some(max_w) = self.max_width {
            w.min(max_w)
        } else {
            w
        };
        if let Some(b) = self.base_mut() {
            b.x = x;
            b.y = y;
            b.w = final_w;
            b.h = h;
        }
        self.clamp_scroll();
    }
    fn set_row_rect(&mut self, x: f32, w: f32) {
        let final_w = if let Some(explicit_w) = self.width {
            explicit_w
        } else if let Some(max_w) = self.max_width {
            w.min(max_w)
        } else {
            w
        };
        if let Some(b) = self.base_mut() {
            b.row_x = x;
            b.row_w = final_w;
        }
    }

    fn color(&self) -> [f32; 4] {
        [0.10, 0.10, 0.16, 1.0]
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if self.disabled {
            let was = self.base.hovered;
            self.base.hovered = false;
            return was;
        }
        let mut changed = false;
        if self.dragging && self.editing {
            let char_width = self.char_width();
            let drag_idx = if self.multiline {
                let line_height = self.line_height();
                let max_chars = if self.line_wrap_enabled() {
                    (((self.base.w - 16.0) / char_width).floor() as usize).max(1)
                } else {
                    999999
                };
                let (lines, index_map) = self.wrap_text(max_chars);
                let top = self.base.label_offset();
                let click_line = (((py - (self.base.y + top + 8.0) + self.scroll_y) / line_height).floor() as isize).max(0) as usize;
                let click_col = (((px - (self.base.x + 8.0) + self.scroll_x) / char_width).round() as isize).max(0) as usize;
                self.map_2d_to_1d(&index_map, click_line, click_col, lines.len() - 1)
            } else {
                self.map_x_to_idx(px)
            };
            if self.cursor_idx != drag_idx {
                self.cursor_idx = drag_idx;
                self.just_focused = false;
                let len = self.edit_buffer.chars().count();
                let start = self.select_anchor.unwrap_or(0).min(self.cursor_idx);
                let end = self.select_anchor.unwrap_or(0).max(self.cursor_idx);
                self.all_selected = start == 0 && end == len && len > 0;
                changed = true;
            }
        }
        let was = self.base.hovered;
        self.base.hovered = self.hit_test(px, py, ctx);
        if was != self.base.hovered {
            changed = true;
        }
        changed
    }

    fn draggable(&self) -> bool { !self.disabled }
    fn is_dragging(&self) -> bool { self.dragging }
    fn widget_font(&self) -> Option<String> { Some(crate::layout::control_label_font_detached()) }

    fn drag_begin(&mut self, _px: f32, _py: f32) {
        if self.disabled || !self.editing { return; }
        self.dragging = true;
    }

    fn drag_update(&mut self, px: f32, py: f32) -> bool {
        if self.disabled || !self.editing { return false; }
        let char_width = self.char_width();
        let top = self.base.label_offset();
        let drag_idx = if self.multiline {
            let line_height = self.line_height();
            let max_chars = if self.line_wrap_enabled() {
                (((self.base.w - 16.0) / char_width).floor() as usize).max(1)
            } else {
                999999
            };
            let (lines, index_map) = self.wrap_text(max_chars);
            let click_line = (((py - (self.base.y + top + 8.0) + self.scroll_y) / line_height).floor() as isize).max(0) as usize;
            let click_col = (((px - (self.base.x + 8.0) + self.scroll_x) / char_width).round() as isize).max(0) as usize;
            self.map_2d_to_1d(&index_map, click_line, click_col, lines.len() - 1)
        } else {
            self.map_x_to_idx(px)
        };
        if self.cursor_idx != drag_idx {
            self.cursor_idx = drag_idx;
            self.just_focused = false;
            let len = self.edit_buffer.chars().count();
            let start = self.select_anchor.unwrap_or(0).min(self.cursor_idx);
            let end = self.select_anchor.unwrap_or(0).max(self.cursor_idx);
            self.all_selected = start == 0 && end == len && len > 0;
            return true;
        }
        false
    }

    fn drag_end(&mut self) {
        self.dragging = false;
    }

    fn value(&self) -> i32 { 0 }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if self.disabled { return false; }
        if button == MouseButton::Right && state == ElementState::Pressed {
            if self.hit_test(px, py, ctx) {
                if !self.editing {
                    self.focus();
                }
                ctx.handle_right_click(self.as_ptr_mut(), px, py);
                return true;
            }
        }
        if button != MouseButton::Left { return false; }
        if !self.hit_test(px, py, ctx) { return false; }
        match state {
            ElementState::Pressed => {
                if !self.editing {
                    self.focus();
                } else {
                    let char_width = self.char_width();
                    let top = self.base.label_offset();
                    let label_x = self.label_x_offset();
                    let idx = if self.multiline {
                        let line_height = self.line_height();
                        let max_chars = if self.line_wrap_enabled() {
                            ((((self.base.w - label_x) - 16.0) / char_width).floor() as usize).max(1)
                        } else {
                            999999
                        };
                        let (lines, index_map) = self.wrap_text(max_chars);
                        let click_line = (((py - (self.base.y + top + 8.0) + self.scroll_y) / line_height).floor() as isize).max(0) as usize;
                        let click_col = (((px - (self.base.x + label_x + 8.0) + self.scroll_x) / char_width).round() as isize).max(0) as usize;
                        self.map_2d_to_1d(&index_map, click_line, click_col, lines.len() - 1)
                    } else {
                        self.map_x_to_idx(px)
                    };
                    self.cursor_idx = idx;
                    self.select_anchor = Some(idx);
                    self.all_selected = false;
                }
                true
            }
            ElementState::Released => {
                if self.dragging {
                    self.drag_end();
                }
                if self.select_anchor == Some(self.cursor_idx) {
                    self.select_anchor = None;
                }
                true
            }
        }
    }

    fn focus(&mut self) {
        if self.disabled { return; }
        self.editing = true;
        self.edit_buffer = self.text.clone();
        let len = self.edit_buffer.chars().count();
        self.cursor_idx = len;
        self.select_anchor = Some(0);
        self.all_selected = len > 0;
        self.just_focused = true;
        self.sync_editor_state();
        focus::set_focused(self);
    }

    fn unfocus(&mut self) {
        if self.editing {
            self.editing = false;
            if self.text != self.edit_buffer {
                self.text = self.edit_buffer.clone();
                self.just_changed = true;
            }
            self.select_anchor = None;
            self.all_selected = false;
            self.sync_editor_state();
        }
    }

    fn keyboard_input(&mut self, event: &KeyEvent, _ctx: &mut UiContext) -> bool {
        if !self.editing || self.disabled { return false; }
        if event.state != ElementState::Pressed { return false; }
        
        let control = event.ctrl;
        
        let mut state = TextEditorState {
            buffer: self.edit_buffer.clone(),
            cursor_idx: self.cursor_idx,
            select_anchor: self.select_anchor,
            all_selected: self.all_selected,
        };
        
        let handled = match &event.logical_key {
            Key::Named(NamedKey::Backspace) => {
                state.delete_backwards()
            }
            Key::Named(NamedKey::Delete) => {
                state.delete_forwards()
            }
            Key::Named(NamedKey::ArrowLeft) => {
                state.move_cursor_left(event.shift)
            }
            Key::Named(NamedKey::ArrowRight) => {
                state.move_cursor_right(event.shift)
            }
            Key::Named(NamedKey::ArrowUp) => {
                if state.all_selected {
                    state.clear_selection();
                } else if event.shift {
                    if state.select_anchor.is_none() {
                        state.select_anchor = Some(state.cursor_idx);
                    }
                } else {
                    state.clear_selection();
                }
                if self.multiline {
                    let char_width = self.char_width();
                    let max_chars = (((self.base.w - 16.0) / char_width).floor() as usize).max(1);
                    let (lines, index_map) = self.wrap_text(max_chars);
                    let (cursor_l, cursor_c) = index_map[state.cursor_idx.min(index_map.len() - 1)];
                    if cursor_l > 0 {
                        state.cursor_idx = self.map_2d_to_1d(&index_map, cursor_l - 1, cursor_c, lines.len() - 1);
                    } else {
                        state.cursor_idx = 0;
                    }
                } else {
                    state.cursor_idx = 0;
                }
                true
            }
            Key::Named(NamedKey::ArrowDown) => {
                if state.all_selected {
                    state.clear_selection();
                } else if event.shift {
                    if state.select_anchor.is_none() {
                        state.select_anchor = Some(state.cursor_idx);
                    }
                } else {
                    state.clear_selection();
                }
                if self.multiline {
                    let char_width = self.char_width();
                    let max_chars = (((self.base.w - 16.0) / char_width).floor() as usize).max(1);
                    let (lines, index_map) = self.wrap_text(max_chars);
                    let (cursor_l, cursor_c) = index_map[state.cursor_idx.min(index_map.len() - 1)];
                    if cursor_l < lines.len() - 1 {
                        state.cursor_idx = self.map_2d_to_1d(&index_map, cursor_l + 1, cursor_c, lines.len() - 1);
                    } else {
                        state.cursor_idx = state.buffer.chars().count();
                    }
                } else {
                    state.cursor_idx = state.buffer.chars().count();
                }
                true
            }
            Key::Named(NamedKey::Home) => {
                state.move_cursor_to_start(event.shift)
            }
            Key::Named(NamedKey::End) => {
                state.move_cursor_to_end(event.shift)
            }
            Key::Named(NamedKey::Enter) => {
                if self.multiline {
                    state.insert_text("\n");
                    true
                } else {
                    self.unfocus();
                    true
                }
            }
            Key::Named(NamedKey::Escape) => {
                self.editing = false;
                state.buffer = self.text.clone();
                state.clear_selection();
                true
            }
            Key::Character(ref ch_str) if control && (ch_str == "a" || ch_str == "A") => {
                state.select_all();
                true
            }
            Key::Character(ref ch_str) if control && (ch_str == "c" || ch_str == "C") => {
                if let Some(text) = state.selected_text() {
                    clipboard::copy_to_clipboard(&text);
                }
                true
            }
            Key::Character(ref ch_str) if control && (ch_str == "x" || ch_str == "X") => {
                if let Some(text) = state.selected_text() {
                    clipboard::copy_to_clipboard(&text);
                    state.insert_text("");
                }
                true
            }
            Key::Character(ref ch_str) if control && (ch_str == "v" || ch_str == "V") => {
                if let Some(pasted) = clipboard::read_from_clipboard() {
                    let mut cleaned = String::new();
                    for ch in pasted.chars() {
                        if !ch.is_control() && ch != '\n' && ch != '\r' {
                            cleaned.push(ch);
                        }
                    }
                    state.insert_text(&cleaned);
                } else {
                    state.clear_selection();
                }
                true
            }
            _ => {
                if let Some(text) = &event.text {
                    if !control {
                        state.insert_text(text);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
        };
        
        if self.editing {
            self.edit_buffer = state.buffer;
            self.cursor_idx = state.cursor_idx;
            self.select_anchor = state.select_anchor;
            self.all_selected = state.all_selected;
            self.sync_editor_state();
            self.scroll_to_cursor();
        }
        
        handled
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        let (r1, r2, r3, r4) = self.rounded_corners();
        let has_rounded = r1 || r2 || r3 || r4;
        if has_rounded {
            return quads;
        }
        
        let top = self.base.label_offset();
        let visual_h = self.base.h - top;
        let border_w = self.border_width();
        if self.disabled {
            if self.draw_bg_border {
                quads.push((self.base.x, self.base.y + top, self.base.w, visual_h, [0.12, 0.12, 0.16, 1.0]));
                quads.push((self.base.x + border_w, self.base.y + top + border_w, self.base.w - 2.0 * border_w, visual_h - 2.0 * border_w, [0.06, 0.06, 0.08, 1.0]));
            }
            return quads;
        }
        
        if self.draw_bg_border {
            let bg_color = if self.editing {
                crate::colors::textbox_background_edit_color()
            } else {
                crate::colors::textbox_background_color()
            };
            let border_color = if self.editing {
                [0.20, 0.50, 0.85, 1.0]
            } else if self.base.hovered {
                [0.25, 0.25, 0.35, 1.0]
            } else {
                [0.18, 0.18, 0.24, 1.0]
            };
            quads.push((self.base.x, self.base.y + top, self.base.w, visual_h, border_color));
            quads.push((self.base.x + border_w, self.base.y + top + border_w, self.base.w - 2.0 * border_w, visual_h - 2.0 * border_w, bg_color));
        }

        if self.editing || self.select_anchor.is_some() {
            let char_width = self.char_width();
            let line_height = self.line_height();
            
            let highlight_color = [0.20, 0.50, 0.85, 0.3];
            let cursor_color = if self.draw_bg_border {
                [0.80, 0.80, 0.85, 1.0]
            } else {
                [0.10, 0.10, 0.15, 1.0]
            };

            let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
            let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);

            if self.multiline {
                let max_chars = if self.line_wrap_enabled() {
                    (((self.base.w - 16.0) / char_width).floor() as usize).max(1)
                } else {
                    999999
                };
                let (_lines, index_map) = self.wrap_text(max_chars);

                let view_top = self.base.y + top;
                let view_bottom = self.base.y + self.base.h;

                if start != end {
                    let start_pos = index_map[start.min(index_map.len() - 1)];
                    let end_pos = index_map[end.min(index_map.len() - 1)];
                    
                    for line_idx in start_pos.0..=end_pos.0 {
                        let mut line_start_col = None;
                        let mut line_end_col = None;
                        for idx in start..end {
                            if idx < index_map.len() {
                                let (l, c) = index_map[idx];
                                if l == line_idx {
                                    if line_start_col.is_none() || c < line_start_col.unwrap() {
                                        line_start_col = Some(c);
                                    }
                                    if line_end_col.is_none() || c > line_end_col.unwrap() {
                                        line_end_col = Some(c);
                                    }
                                }
                            }
                        }
                        if let (Some(sc), Some(ec)) = (line_start_col, line_end_col) {
                            let highlight_x = self.base.x + 8.0 + (sc as f32 * char_width) - self.scroll_x;
                            let highlight_w = (ec - sc + 1) as f32 * char_width;
                            let highlight_y = self.base.y + top + 8.0 + (line_idx as f32 * line_height) - self.scroll_y;
                            let clipped_y = highlight_y.max(view_top);
                            let clipped_bottom = (highlight_y + line_height).min(view_bottom);
                            let h_left = highlight_x.max(self.base.x + 8.0);
                            let h_right = (highlight_x + highlight_w).min(self.base.x + self.base.w - 8.0);
                            if h_left < h_right && clipped_y < clipped_bottom {
                                quads.push((
                                    h_left,
                                    clipped_y,
                                    h_right - h_left,
                                    clipped_bottom - clipped_y,
                                    highlight_color,
                                ));
                            }
                        }
                    }
                }
                
                if self.editing {
                    let caret_h = self.font_size * 1.15;
                    let (cursor_l, cursor_c) = index_map[self.cursor_idx.min(index_map.len() - 1)];
                    let cursor_x = self.base.x + 8.0 + (cursor_c as f32 * char_width) - self.scroll_x;
                    let cursor_y = self.base.y + top + 8.0 + (cursor_l as f32 * line_height) + (line_height - caret_h) / 2.0 - self.scroll_y;
                    let clipped_y = cursor_y.max(view_top);
                    let clipped_bottom = (cursor_y + caret_h).min(view_bottom);
                    if cursor_x >= self.base.x + 8.0 && cursor_x <= self.base.x + self.base.w - 8.0 {
                        if clipped_y < clipped_bottom {
                            quads.push((cursor_x, clipped_y, 1.5, clipped_bottom - clipped_y, cursor_color));
                        }
                    }
                }
            } else {
                let caret_h = self.font_size * 1.15;
                if start != end {
                    let h_left_offset = self.glyph_positions.get(start).copied().unwrap_or_else(|| start as f32 * char_width);
                    let h_right_offset = self.glyph_positions.get(end).copied().unwrap_or_else(|| end as f32 * char_width);
                    let highlight_x = self.base.x + 8.0 + h_left_offset - self.scroll_x;
                    let h_left = highlight_x.max(self.base.x + 8.0);
                    let h_right = (self.base.x + 8.0 + h_right_offset - self.scroll_x).min(self.base.x + self.base.w - 8.0);
                    if h_left < h_right {
                        quads.push((
                            h_left,
                            crate::layout::align_text_y(self.base.y, self.base.h, self.font_size, top),
                            h_right - h_left,
                            crate::layout::line_height(self.font_size),
                            highlight_color,
                        ));
                    }
                }

                if self.editing {
                    let offset = if self.glyph_positions.is_empty() {
                        self.cursor_idx as f32 * char_width
                    } else {
                        self.cursor_x_offset
                    };
                    let cursor_x = self.base.x + 8.0 + offset - self.scroll_x;
                    if cursor_x >= self.base.x + 8.0 && cursor_x <= self.base.x + self.base.w - 8.0 {
                        let text_y = crate::layout::align_text_y(self.base.y, self.base.h, self.font_size, top);
                        let cursor_y = text_y + (self.font_size - caret_h) / 2.0;
                        quads.push((cursor_x, cursor_y, 1.5, caret_h, cursor_color));
                    }
                }
            }
        }

        quads
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

        let top = self.base.label_offset();
        let visual_h = self.base.h - top;
        let radius = self.corner_radius();
        
        let bg_color = if self.editing {
            crate::colors::textbox_background_edit_color()
        } else {
            crate::colors::textbox_background_color()
        };
        let border_color = if self.editing {
            [0.20, 0.50, 0.85, 1.0]
        } else if self.base.hovered {
            [0.25, 0.25, 0.35, 1.0]
        } else {
            [0.18, 0.18, 0.24, 1.0]
        };
        
        let label_x = self.label_x_offset();
        let x = self.base.x + label_x;
        let w = self.base.w - label_x;

        if self.draw_bg_border {
            let border_w = self.border_width();
            quads.push((x, self.base.y + top, w, visual_h, radius, border_color, (r1, r2, r3, r4)));
            quads.push((x + border_w, self.base.y + top + border_w, w - 2.0 * border_w, visual_h - 2.0 * border_w, (radius - border_w).max(0.0), bg_color, (r1, r2, r3, r4)));
        }

        if self.editing || self.select_anchor.is_some() {
            let char_width = self.char_width();
            let line_height = self.line_height();
            
            let highlight_color = [0.20, 0.50, 0.85, 0.3];
            let cursor_color = if self.draw_bg_border {
                [0.80, 0.80, 0.85, 1.0]
            } else {
                [0.10, 0.10, 0.15, 1.0]
            };

            let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
            let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);

            if self.multiline {
                let max_chars = if self.line_wrap_enabled() {
                    (((w - 16.0) / char_width).floor() as usize).max(1)
                } else {
                    999999
                };
                let (_lines, index_map) = self.wrap_text(max_chars);

                let view_top = self.base.y + top;
                let view_bottom = self.base.y + self.base.h;

                if start != end {
                    let start_pos = index_map[start.min(index_map.len() - 1)];
                    let end_pos = index_map[end.min(index_map.len() - 1)];
                    
                    for line_idx in start_pos.0..=end_pos.0 {
                        let mut line_start_col = None;
                        let mut line_end_col = None;
                        for idx in start..end {
                            if idx < index_map.len() {
                                let (l, c) = index_map[idx];
                                if l == line_idx {
                                    if line_start_col.is_none() || c < line_start_col.unwrap() {
                                        line_start_col = Some(c);
                                    }
                                    if line_end_col.is_none() || c > line_end_col.unwrap() {
                                        line_end_col = Some(c);
                                    }
                                }
                            }
                        }
                        if let (Some(sc), Some(ec)) = (line_start_col, line_end_col) {
                            let highlight_x = x + 8.0 + (sc as f32 * char_width) - self.scroll_x;
                            let highlight_w = (ec - sc + 1) as f32 * char_width;
                            let highlight_y = self.base.y + top + 8.0 + (line_idx as f32 * line_height) - self.scroll_y;
                            let clipped_y = highlight_y.max(view_top);
                            let clipped_bottom = (highlight_y + line_height).min(view_bottom);
                            let h_left = highlight_x.max(x + 8.0);
                            let h_right = (highlight_x + highlight_w).min(x + w - 8.0);
                            if h_left < h_right && clipped_y < clipped_bottom {
                                quads.push((h_left, clipped_y, h_right - h_left, clipped_bottom - clipped_y, 0.0, highlight_color, (false, false, false, false)));
                            }
                        }
                    }
                }
                
                if self.editing {
                    let caret_h = self.font_size * 1.15;
                    let (cursor_l, cursor_c) = index_map[self.cursor_idx.min(index_map.len() - 1)];
                    let cursor_x = x + 8.0 + (cursor_c as f32 * char_width) - self.scroll_x;
                    let cursor_y = self.base.y + top + 8.0 + (cursor_l as f32 * line_height) + (line_height - caret_h) / 2.0 - self.scroll_y;
                    let clipped_y = cursor_y.max(view_top);
                    let clipped_bottom = (cursor_y + caret_h).min(view_bottom);
                    if cursor_x >= x + 8.0 && cursor_x <= x + w - 8.0 {
                        if clipped_y < clipped_bottom {
                            quads.push((cursor_x, clipped_y, 1.5, clipped_bottom - clipped_y, 0.0, cursor_color, (false, false, false, false)));
                        }
                    }
                }
            } else {
                let caret_h = self.font_size * 1.15;
                if start != end {
                    let h_left_offset = self.glyph_positions.get(start).copied().unwrap_or_else(|| start as f32 * char_width);
                    let h_right_offset = self.glyph_positions.get(end).copied().unwrap_or_else(|| end as f32 * char_width);
                    let highlight_x = x + 8.0 + h_left_offset - self.scroll_x;
                    let h_left = highlight_x.max(x + 8.0);
                    let h_right = (x + 8.0 + h_right_offset - self.scroll_x).min(x + w - 8.0);
                    if h_left < h_right {
                        quads.push((
                            h_left,
                            crate::layout::align_text_y(self.base.y, self.base.h, self.font_size, top),
                            h_right - h_left,
                            crate::layout::line_height(self.font_size),
                            0.0,
                            highlight_color,
                            (false, false, false, false),
                        ));
                    }
                }

                if self.editing {
                    let offset = if self.glyph_positions.is_empty() {
                        self.cursor_idx as f32 * char_width
                    } else {
                        self.cursor_x_offset
                    };
                    let cursor_x = x + 8.0 + offset - self.scroll_x;
                    if cursor_x >= x + 8.0 && cursor_x <= x + w - 8.0 {
                        let text_y = crate::layout::align_text_y(self.base.y, self.base.h, self.font_size, top);
                        let cursor_y = text_y + (self.font_size - caret_h) / 2.0;
                        quads.push((cursor_x, cursor_y, 1.5, caret_h, 0.0, cursor_color, (false, false, false, false)));
                    }
                }
            }
        }
        
        for &child_ptr in &self.children(ctx) {
            let widget = unsafe { &*child_ptr };
            quads.extend(widget.all_rounded_quads(ctx));
        }
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        let top = self.base.label_offset();
        let _visual_h = self.base.h - top;
        if let Some(lbl) = self.control_label() {
            labels.push(lbl);
        }
        let mut val_text = if self.editing {
            self.edit_buffer.clone()
        } else {
            self.text.clone()
        };
        if self.is_password {
            val_text = "•".repeat(val_text.chars().count());
        }

        let is_placeholder = val_text.is_empty() && self.placeholder.is_some();
        let display_text = if is_placeholder {
            self.placeholder.as_ref().unwrap().clone()
        } else {
            val_text
        };

        let label_color = if is_placeholder {
            crate::colors::textbox_placeholder_text_color()
        } else if let Some(custom_color) = self.text_color {
            custom_color
        } else if self.disabled {
            [0x53, 0x53, 0x5a]
        } else if self.all_selected {
            [0xff, 0xff, 0xff]
        } else if self.editing {
            [0xee, 0xee, 0xf5]
        } else {
            [0xcc, 0xcc, 0xd4]
        };

        let label_x = self.label_x_offset();
        let x = self.base.x + label_x;
        let w = self.base.w - label_x;

        if self.multiline {
            let char_width = self.char_width();
            let line_height = self.line_height();
            let max_chars = if self.line_wrap_enabled() {
                (((w - 16.0) / char_width).floor() as usize).max(1)
            } else {
                999999
            };
            let (lines, _) = self.wrap_text(max_chars);
            let lines_to_draw = if is_placeholder {
                let placeholder_src = self.placeholder.as_ref().unwrap();
                let chars: Vec<char> = placeholder_src.chars().collect();
                let mut p_lines = Vec::new();
                let mut current_line = Vec::new();
                for ch in chars {
                    if ch == '\n' {
                        p_lines.push(current_line.iter().collect::<String>());
                        current_line.clear();
                    } else {
                        current_line.push(ch);
                        if self.line_wrap_enabled() && current_line.len() > max_chars {
                            p_lines.push(current_line.iter().collect::<String>());
                            current_line.clear();
                        }
                    }
                }
                p_lines.push(current_line.iter().collect::<String>());
                p_lines
            } else {
                lines
            };
            for (line_idx, line_text) in lines_to_draw.iter().enumerate() {
                labels.push(TextLabel {
                    text: line_text.clone(),
                    x: x + 8.0 - self.scroll_x,
                    y: self.base.y + top + 8.0 + (line_idx as f32 * line_height) + (line_height - self.font_size) / 2.0 - self.scroll_y,
                    font_size: self.font_size,
                    color: label_color,
                });
            }
        } else {
            labels.push(TextLabel {
                text: display_text,
                x: x + 8.0 - self.scroll_x,
                y: crate::layout::align_text_y(self.base.y, self.base.h, self.font_size, top),
                font_size: self.font_size,
                color: label_color,
            });
        }
        labels
    }

    fn text_labels_with_bounds(&self, _ctx: &UiContext) -> Vec<(TextLabel, Option<[f32; 4]>)> {
        let label_x = self.label_x_offset();
        let bounds = Some([self.base.x + label_x, self.base.y, self.base.x + self.base.w, self.base.y + self.base.h]);
        self.text_labels().into_iter().map(|l| (l, bounds)).collect()
    }

    fn text_labels_with_font_and_bounds(&self, _ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        let font = self.widget_font();
        let label_x = self.label_x_offset();
        let bounds = Some([self.base.x + label_x, self.base.y, self.base.x + self.base.w, self.base.y + self.base.h]);
        self.text_labels().into_iter().map(|l| (l, font.clone(), bounds)).collect()
    }

    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, _px: f32, _py: f32, ctx: &mut UiContext) -> bool {
        if self.disabled { return false; }
        let char_width = self.char_width();
        let line_height = self.line_height();
        
        let max_chars = if self.line_wrap_enabled() {
            (((self.base.w - 16.0) / char_width).floor() as usize).max(1)
        } else {
            999999
        };
        
        let (lines, _) = if self.multiline {
            self.wrap_text(max_chars)
        } else {
            let buffer = if self.editing { &self.edit_buffer } else { &self.text };
            (vec![buffer.clone()], vec![(0, 0); buffer.chars().count() + 1])
        };

        let mut changed = false;

        if self.multiline {
            let content_h = lines.len() as f32 * line_height;
            let max_scroll = (content_h - (self.base.h - 16.0)).max(0.0);
            let scroll_amt = match *delta {
                MouseScrollDelta::LineDelta(_, dy) => -dy * line_height * 2.0,
                MouseScrollDelta::PixelDelta(pos) => -pos.y as f32,
            };
            let old_scroll = self.scroll_y;
            self.scroll_y = (self.scroll_y + scroll_amt).clamp(0.0, max_scroll);
            if old_scroll != self.scroll_y {
                changed = true;
            }
        }

        if !self.line_wrap_enabled() {
            let max_line_len = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
            let content_w = max_line_len as f32 * char_width;
            let max_scroll_x = (content_w - (self.base.w - 16.0)).max(0.0);
            let scroll_amt_x = match *delta {
                MouseScrollDelta::LineDelta(dx, dy) => {
                    if !self.multiline {
                        let scroll_val = if dy != 0.0 { -dy } else { dx };
                        scroll_val * char_width * 3.0
                    } else {
                        dx * char_width * 3.0
                    }
                }
                MouseScrollDelta::PixelDelta(pos) => {
                    if !self.multiline {
                        let scroll_val = if pos.y != 0.0 { -pos.y as f32 } else { pos.x as f32 };
                        scroll_val
                    } else {
                        pos.x as f32
                    }
                }
            };
            let old_scroll_x = self.scroll_x;
            self.scroll_x = (self.scroll_x + scroll_amt_x).clamp(0.0, max_scroll_x);
            if old_scroll_x != self.scroll_x {
                changed = true;
            }
        }

        if changed {
            self.mark_dirty(ctx);
            true
        } else {
            false
        }
    }
}

impl Drop for TextBox {
    fn drop(&mut self) {
        clear_widget_references(self);
    }
}

unsafe impl Send for TextBox {}
unsafe impl Sync for TextBox {}

impl Control for TextBox {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_textbox_selection_highlight() {
        let mut dummy = crate::context::UiContext::new();
        let mut tb = TextBox::new("Initial Text".to_string());
        tb.set_rect(10.0, 10.0, 200.0, 30.0);

        // 1. Initial state
        assert!(!tb.editing);
        assert!(!tb.all_selected);

        // 2. Click focuses and triggers highlighting
        let clicked = tb.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 20.0, &mut dummy);
        assert!(clicked);
        assert!(tb.editing);
        assert!(tb.all_selected);
        assert_eq!(tb.edit_buffer, "Initial Text");

        // 3. Typing a key replaces all text
        let key_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Character("A".to_string()),
            text: Some("A".to_string()),
            repeat: false,
            ctrl: false,
            shift: false,
        };
        let handled = tb.keyboard_input(&key_ev, &mut dummy);
        assert!(handled);
        assert!(!tb.all_selected);
        assert_eq!(tb.edit_buffer, "A");

        // 4. Pressing Enter commits change
        let enter_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::Enter),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
        };
        let handled_enter = tb.keyboard_input(&enter_ev, &mut dummy);
        assert!(handled_enter);
        assert!(!tb.editing);
        assert_eq!(tb.text, "A");
        assert!(tb.take_change());
    }

    #[test]
    fn test_textbox_drag_and_modifier_selection() {
        let mut dummy = crate::context::UiContext::new();
        let mut tb = TextBox::new("Hello World".to_string());
        tb.set_rect(10.0, 10.0, 200.0, 30.0);

        // 1. Initial click focuses and selects all
        let pressed = tb.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 20.0, &mut dummy);
        assert!(pressed);
        let released = tb.mouse_input(MouseButton::Left, ElementState::Released, 50.0, 20.0, &mut dummy);
        assert!(released);
        assert!(tb.editing);
        assert!(tb.all_selected);
        assert_eq!(tb.cursor_idx, 11);
        assert_eq!(tb.select_anchor, Some(0));

        // 2. Click inside placed caret at index 5
        let click_x5 = 10.0 + 8.0 + 5.0 * tb.char_width();
        let pressed_inside = tb.mouse_input(MouseButton::Left, ElementState::Pressed, click_x5, 20.0, &mut dummy);
        assert!(pressed_inside);
        assert_eq!(tb.cursor_idx, 5);
        assert_eq!(tb.select_anchor, Some(5));
        assert!(!tb.all_selected);

        // 3. Drag to index 11
        let drag_x11 = 10.0 + 8.0 + 11.0 * tb.char_width();
        tb.drag_begin(click_x5, 20.0);
        let updated = tb.drag_update(drag_x11, 20.0);
        assert!(updated);
        assert_eq!(tb.cursor_idx, 11);
        assert_eq!(tb.select_anchor, Some(5));
        tb.drag_end();

        // 4. Keyboard ArrowLeft with Shift shrinks selection from 11 to 10
        let left_shift_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::ArrowLeft),
            text: None,
            repeat: false,
            ctrl: false,
            shift: true,
        };
        let handled = tb.keyboard_input(&left_shift_ev, &mut dummy);
        assert!(handled);
        assert_eq!(tb.cursor_idx, 10);
        assert_eq!(tb.select_anchor, Some(5));

        // 5. Keyboard ArrowLeft without Shift collapses selection to start (index 5)
        let left_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::ArrowLeft),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
        };
        let handled = tb.keyboard_input(&left_ev, &mut dummy);
        assert!(handled);
        assert_eq!(tb.cursor_idx, 5);
        assert_eq!(tb.select_anchor, None);

        // 6. Keyboard Shift+Up highlights to beginning (cursor 0, anchor 5)
        let up_shift_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::ArrowUp),
            text: None,
            repeat: false,
            ctrl: false,
            shift: true,
        };
        let handled = tb.keyboard_input(&up_shift_ev, &mut dummy);
        assert!(handled);
        assert_eq!(tb.cursor_idx, 0);
        assert_eq!(tb.select_anchor, Some(5));

        // 7. Typing a key replaces selected range "Hello" with "Rust"
        let rust_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Character("Rust".to_string()),
            text: Some("Rust".to_string()),
            repeat: false,
            ctrl: false,
            shift: false,
        };
        let handled = tb.keyboard_input(&rust_ev, &mut dummy);
        assert!(handled);
        assert_eq!(tb.edit_buffer, "Rust World");
        assert_eq!(tb.cursor_idx, 4);
        assert_eq!(tb.select_anchor, None);
    }

    #[test]
    fn test_textbox_right_click_context_menu() {
        let mut dummy = crate::context::UiContext::new();
        let mut tb = TextBox::new("Context Menu Text".to_string());
        tb.set_rect(10.0, 10.0, 200.0, 30.0);

        // Hide context menu initially
        dummy.hide_context_menu();
        assert!(!dummy.is_context_menu_visible());

        // Right click on textbox
        let clicked = tb.mouse_input(MouseButton::Right, ElementState::Pressed, 50.0, 20.0, &mut dummy);
        assert!(clicked);
        assert!(dummy.is_context_menu_visible());
        assert!(tb.editing);
     }

    #[test]
    fn test_textbox_multiline_selection_highlight() {
        let dummy = crate::context::UiContext::new();
        let mut tb = TextBox::new("Line 1\nLine 2\nLine 3".to_string()).with_multiline(true);
        tb.set_rect(10.0, 10.0, 200.0, 100.0);
        tb.select_anchor = Some(7); // starts at "Line 2"
        tb.cursor_idx = 13;        // ends at end of "Line 2"
        
        let has_rounded = tb.rounded_corners() != (false, false, false, false);
        let has_highlight = if has_rounded {
            let rounded = tb.all_rounded_quads(&dummy);
            println!("Rounded quads: {:?}", rounded);
            rounded.iter().any(|q| q.5 == [0.20, 0.50, 0.85, 0.3])
        } else {
            let extra = tb.extra_quads();
            println!("Extra quads: {:?}", extra);
            extra.iter().any(|q| q.4 == [0.20, 0.50, 0.85, 0.3])
        };
        assert!(has_highlight, "Should have a highlight quad!");
    }

    #[test]
    fn test_textbox_line_wrap_disabled_horizontal_scrolling() {
        let _dummy = crate::context::UiContext::new();
        crate::layout::set_textbox_line_wrap(false);
        
        let mut tb = TextBox::new("Very long text that should not wrap and instead scroll horizontally".to_string());
        tb.set_rect(10.0, 10.0, 100.0, 30.0);
        
        assert_eq!(tb.scroll_x, 0.0);
        
        tb.focus();
        tb.cursor_idx = tb.edit_buffer.chars().count();
        tb.scroll_to_cursor();
        
        assert!(tb.scroll_x > 0.0, "scroll_x should be scrolled horizontally to keep the cursor visible");
        
        crate::layout::set_textbox_line_wrap(true);
    }

    #[test]
    fn test_search_textbox_cear_option() {
        let mut dummy = crate::context::UiContext::new();
        let mut tb = TextBox::new("Some Search query".to_string()).with_placeholder("Search...");
        tb.set_rect(10.0, 10.0, 200.0, 30.0);

        // Right click on textbox
        let clicked = tb.mouse_input(MouseButton::Right, ElementState::Pressed, 50.0, 20.0, &mut dummy);
        assert!(clicked);
        assert!(dummy.is_context_menu_visible());

        // Verify "Cear" option is in options
        let opts = crate::widget::context_menu::options();
        assert!(opts.contains(&"Cear".to_string()));

        // Simulate choosing the "Cear" option
        tb.clear_text();
        assert_eq!(tb.text, "");
        assert_eq!(tb.edit_buffer, "");
    }

    #[test]
    fn test_multiline_textbox_border_width() {
        let _dummy = crate::context::UiContext::new();
        crate::layout::set_textbox_multiline_border_width(1.0);
        let tb_single = TextBox::new("Singleline".to_string()).with_multiline(false);
        let tb_multi = TextBox::new("Multiline".to_string()).with_multiline(true);

        // Default border width
        assert_eq!(tb_single.border_width(), 1.0);
        assert_eq!(tb_multi.border_width(), 1.0);

        // Configure custom border width
        crate::layout::set_textbox_multiline_border_width(4.5);
        assert_eq!(tb_single.border_width(), 1.0);
        assert_eq!(tb_multi.border_width(), 4.5);

        // Reset to default
        crate::layout::set_textbox_multiline_border_width(1.0);
    }
}

