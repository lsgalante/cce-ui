#[derive(Debug, Clone, Default)]
pub struct TextEditorState {
    pub buffer: String,
    pub cursor_idx: usize,
    pub select_anchor: Option<usize>,
    pub all_selected: bool,
}

impl TextEditorState {
    pub fn new(text: String) -> Self {
        let len = text.chars().count();
        Self {
            buffer: text,
            cursor_idx: len,
            select_anchor: None,
            all_selected: false,
        }
    }

    pub fn select_all(&mut self) {
        let len = self.buffer.chars().count();
        self.select_anchor = Some(0);
        self.cursor_idx = len;
        self.all_selected = len > 0;
    }

    pub fn clear_selection(&mut self) {
        self.select_anchor = None;
        self.all_selected = false;
    }

    pub fn selected_range(&self) -> Option<(usize, usize)> {
        let anchor = self.select_anchor?;
        if anchor == self.cursor_idx {
            None
        } else {
            Some((anchor.min(self.cursor_idx), anchor.max(self.cursor_idx)))
        }
    }

    pub fn selected_text(&self) -> Option<String> {
        let (start, end) = self.selected_range()?;
        let chars: Vec<char> = self.buffer.chars().collect();
        Some(chars[start..end].iter().collect())
    }

    pub fn insert_text(&mut self, text: &str) {
        if self.all_selected {
            self.buffer.clear();
            self.cursor_idx = 0;
            self.clear_selection();
        }

        let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
        let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);
        let chars: Vec<char> = self.buffer.chars().collect();
        let mut new_buf = String::new();
        for i in 0..start {
            new_buf.push(chars[i]);
        }
        let inserted_count = text.chars().count();
        new_buf.push_str(text);
        for i in end..chars.len() {
            new_buf.push(chars[i]);
        }
        self.buffer = new_buf;
        self.cursor_idx = start + inserted_count;
        self.clear_selection();
    }

    pub fn delete_backwards(&mut self) -> bool {
        if self.all_selected {
            self.buffer.clear();
            self.cursor_idx = 0;
            self.clear_selection();
            return true;
        }
        let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
        let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);
        if start != end {
            let chars: Vec<char> = self.buffer.chars().collect();
            let mut new_buf = String::new();
            for i in 0..start {
                new_buf.push(chars[i]);
            }
            for i in end..chars.len() {
                new_buf.push(chars[i]);
            }
            self.buffer = new_buf;
            self.cursor_idx = start;
            self.clear_selection();
            return true;
        }
        if self.cursor_idx > 0 {
            let mut chars: Vec<char> = self.buffer.chars().collect();
            chars.remove(self.cursor_idx - 1);
            self.buffer = chars.into_iter().collect();
            self.cursor_idx -= 1;
            self.clear_selection();
            return true;
        }
        false
    }

    pub fn delete_forwards(&mut self) -> bool {
        if self.all_selected {
            self.buffer.clear();
            self.cursor_idx = 0;
            self.clear_selection();
            return true;
        }
        let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
        let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);
        if start != end {
            let chars: Vec<char> = self.buffer.chars().collect();
            let mut new_buf = String::new();
            for i in 0..start {
                new_buf.push(chars[i]);
            }
            for i in end..chars.len() {
                new_buf.push(chars[i]);
            }
            self.buffer = new_buf;
            self.cursor_idx = start;
            self.clear_selection();
            return true;
        }
        let char_count = self.buffer.chars().count();
        if self.cursor_idx < char_count {
            let mut chars: Vec<char> = self.buffer.chars().collect();
            chars.remove(self.cursor_idx);
            self.buffer = chars.into_iter().collect();
            self.clear_selection();
            return true;
        }
        false
    }

    pub fn move_cursor_left(&mut self, select: bool) -> bool {
        if select {
            if self.select_anchor.is_none() {
                self.select_anchor = Some(self.cursor_idx);
            }
            if self.cursor_idx > 0 {
                self.cursor_idx -= 1;
                true
            } else {
                false
            }
        } else {
            if let Some(anchor) = self.select_anchor {
                self.cursor_idx = anchor.min(self.cursor_idx);
                self.clear_selection();
                true
            } else if self.cursor_idx > 0 {
                self.cursor_idx -= 1;
                true
            } else {
                false
            }
        }
    }

    pub fn move_cursor_right(&mut self, select: bool) -> bool {
        let char_count = self.buffer.chars().count();
        if select {
            if self.select_anchor.is_none() {
                self.select_anchor = Some(self.cursor_idx);
            }
            if self.cursor_idx < char_count {
                self.cursor_idx += 1;
                true
            } else {
                false
            }
        } else {
            if let Some(anchor) = self.select_anchor {
                self.cursor_idx = anchor.max(self.cursor_idx);
                self.clear_selection();
                true
            } else if self.cursor_idx < char_count {
                self.cursor_idx += 1;
                true
            } else {
                false
            }
        }
    }

    pub fn move_cursor_to_start(&mut self, select: bool) -> bool {
        if select {
            if self.select_anchor.is_none() {
                self.select_anchor = Some(self.cursor_idx);
            }
        } else {
            self.clear_selection();
        }
        let changed = self.cursor_idx != 0;
        self.cursor_idx = 0;
        changed
    }

    pub fn move_cursor_to_end(&mut self, select: bool) -> bool {
        let char_count = self.buffer.chars().count();
        if select {
            if self.select_anchor.is_none() {
                self.select_anchor = Some(self.cursor_idx);
            }
        } else {
            self.clear_selection();
        }
        let changed = self.cursor_idx != char_count;
        self.cursor_idx = char_count;
        changed
    }
}
