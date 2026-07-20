//! Narrow-trait `TextBox` (Phase 5q). The widest-surface leaf so far: real selection-aware
//! clipboard (the new `Input` cut/copy/paste/select-all/clear hooks — their defaults replicate
//! the whole-value `WidgetHost` defaults for everyone else), load-bearing glyph shaping through
//! `Paint::prepare_text` (cursor↔pixel mapping reads the measured advances), the row-hit
//! restoration (`Layout::hit_row_rect` — cce-files' save-name box relies on row hits), a
//! width/max-width clamp on both rect paths (`Layout::adjust_rect` + `adjust_row_rect`), the
//! ungated `Layout::rect_assigned` (scroll re-clamp on every `set_rect`, hidden or not), and
//! `Input::tracks_base_focus = false` (legacy `focus()` never set the base flag — the detached
//! label must not color as focused).
//!
//! Parity notes:
//! - The legacy render split is asymmetric and preserved faithfully: the non-rounded path
//!   (`extra_quads`) draws at the full base x/width with a disabled special-case; the rounded
//!   path (`all_rounded_quads`) insets by the side label and has NO disabled branch.
//! - Releases: legacy `mouse_input` hit-gated releases too (out-of-rect releases were dropped).
//!   The adapter delivers releases ungated, so the model re-checks containment itself against
//!   the plain rect (the row-substituted release geometry is approximated — flagged).
//! - Wheel scrolling is now hit-gated by the adapter (legacy hosts called `mouse_wheel`
//!   directly on the hovered widget, so the gate should be a no-op in practice — flagged).

use crate::widget::*;
use crate::scene::layout::{Rect, Size};
use crate::scene::paint::PaintCtx;
use crate::widget::model::{Adapted, EventCtx, Input, Layout, Paint};
use std::sync::OnceLock;

static FONT_DB: OnceLock<resvg::usvg::fontdb::Database> = OnceLock::new();

pub fn get_font_db() -> &'static resvg::usvg::fontdb::Database {
    FONT_DB.get_or_init(|| {
        let mut db = resvg::usvg::fontdb::Database::new();
        db.load_system_fonts();
        db.load_fonts_dir(crate::fonts_dir());
        db
    })
}

/// Side-layout label inset — the legacy `WidgetHost::label_x_offset` default for non-exempt
/// widgets (TextBox was never in the exempt list).
fn side_offset(label: &Option<String>) -> f32 {
    if crate::layout::control_label_layout() == "side" && label.is_some() {
        90.0
    } else {
        0.0
    }
}

#[derive(Debug, Clone)]
pub struct TextBox {
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
    /// Synced control label ([`Paint::sync_label`]) — drives the side/detached offsets.
    label: Option<String>,
    /// Own hover flag, maintained from `MouseEnter`/`MouseLeave` (adapter bookkeeping).
    hovered: bool,
    /// The laid-out base rect, cached from [`Layout::rect_assigned`] — the cursor/scroll math
    /// reads geometry between events, which the narrow traits don't otherwise carry.
    rect: Rect,
    /// Recessed style: a `Recess` overlay is carved over the box's own fill —
    /// an inset well, the input-direction counterpart of the raised controls.
    recessed: bool,
}

impl TextBox {
    pub fn new(text: String) -> Adapted<TextBox> {
        let (style_family, style_size) = crate::layout::control_label_font_detached_parsed();
        let editor_state = TextEditorState::new(text.clone());
        Adapted::new(TextBox {
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
            label: None,
            hovered: false,
            rect: Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 },
            recessed: crate::layout::control_relief(),
        })
    }

    /// The detached-label strip height — a replica of `Widget::label_offset` over the synced
    /// label (zero in side layout or unlabeled).
    fn label_top(&self) -> f32 {
        if crate::layout::control_label_layout() == "side" {
            return 0.0;
        }
        if self.label.is_some() {
            let (_, font_size) = crate::layout::control_label_font_detached_parsed();
            font_size + crate::layout::control_label_margin()
        } else {
            0.0
        }
    }

    fn map_x_to_idx(&self, click_x: f32) -> usize {
        let label_x = side_offset(&self.label);
        let relative_x = click_x - (self.rect.x + label_x + 8.0) + self.scroll_x;
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

    pub fn set_max_width(&mut self, max_w: Option<f32>) {
        self.max_width = max_w;
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

    pub fn set_value(&mut self, val: &str) -> bool {
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

    pub fn clamp_scroll(&mut self) {
        let char_width = self.char_width();
        let line_height = self.line_height();
        let max_chars = if self.line_wrap_enabled() {
            (((self.rect.width - 16.0) / char_width).floor() as usize).max(1)
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
            let max_scroll = (content_h - (self.rect.height - 16.0)).max(0.0);
            self.scroll_y = self.scroll_y.clamp(0.0, max_scroll);
        } else {
            self.scroll_y = 0.0;
        }

        if !self.line_wrap_enabled() {
            let max_line_len = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
            let content_w = max_line_len as f32 * char_width;
            let max_scroll_x = (content_w - (self.rect.width - 16.0)).max(0.0);
            self.scroll_x = self.scroll_x.clamp(0.0, max_scroll_x);
        } else {
            self.scroll_x = 0.0;
        }
    }

    pub fn scroll_to_cursor(&mut self) {
        let char_width = self.char_width();
        let line_height = self.line_height();
        let max_chars = if self.line_wrap_enabled() {
            (((self.rect.width - 16.0) / char_width).floor() as usize).max(1)
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

        let top = self.label_top();
        let viewport_w = self.rect.width - 16.0;
        let viewport_h = self.rect.height - top - 16.0;

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

    /// The legacy `focus()` body minus the global-focus claim (the caller's, via
    /// `EventCtx::request_focus`).
    fn begin_editing(&mut self) {
        if self.disabled { return; }
        self.editing = true;
        self.edit_buffer = self.text.clone();
        let len = self.edit_buffer.chars().count();
        self.cursor_idx = len;
        self.select_anchor = Some(0);
        self.all_selected = len > 0;
        self.just_focused = true;
        self.sync_editor_state();
    }

    /// The legacy `unfocus()` body: leave edit mode and commit the buffer.
    fn commit_editing(&mut self) {
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

    /// Map a press/drag position to a buffer index — the shared body of the legacy
    /// `mouse_input` press arm and `drag_update`.
    fn position_to_idx(&self, px: f32, py: f32, with_label_x: bool) -> usize {
        let char_width = self.char_width();
        let top = self.label_top();
        let label_x = if with_label_x { side_offset(&self.label) } else { 0.0 };
        if self.multiline {
            let line_height = self.line_height();
            let max_chars = if self.line_wrap_enabled() {
                ((((self.rect.width - label_x) - 16.0) / char_width).floor() as usize).max(1)
            } else {
                999999
            };
            let (lines, index_map) = self.wrap_text(max_chars);
            let click_line = (((py - (self.rect.y + top + 8.0) + self.scroll_y) / line_height).floor() as isize).max(0) as usize;
            let click_col = (((px - (self.rect.x + label_x + 8.0) + self.scroll_x) / char_width).round() as isize).max(0) as usize;
            self.map_2d_to_1d(&index_map, click_line, click_col, lines.len() - 1)
        } else {
            self.map_x_to_idx(px)
        }
    }

    /// Extend the selection to a drag position — the shared body of the legacy
    /// `on_cursor_moved` drag arm and `drag_update` (which used no label inset).
    fn extend_selection_to(&mut self, px: f32, py: f32) -> bool {
        let drag_idx = self.position_to_idx(px, py, false);
        if self.cursor_idx != drag_idx {
            self.cursor_idx = drag_idx;
            self.just_focused = false;
            let len = self.edit_buffer.chars().count();
            let start = self.select_anchor.unwrap_or(0).min(self.cursor_idx);
            let end = self.select_anchor.unwrap_or(0).max(self.cursor_idx);
            self.all_selected = start == 0 && end == len && len > 0;
            true
        } else {
            false
        }
    }

    /// Port of the legacy `keyboard_input` body.
    fn handle_key(&mut self, event: &KeyEvent) -> bool {
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
                    let max_chars = (((self.rect.width - 16.0) / char_width).floor() as usize).max(1);
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
                    let max_chars = (((self.rect.width - 16.0) / char_width).floor() as usize).max(1);
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
                    self.commit_editing();
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

    /// Port of the legacy `mouse_wheel` body (scroll the multiline/no-wrap viewports).
    fn handle_wheel(&mut self, delta: &MouseScrollDelta) -> bool {
        if self.disabled { return false; }
        let char_width = self.char_width();
        let line_height = self.line_height();

        let max_chars = if self.line_wrap_enabled() {
            (((self.rect.width - 16.0) / char_width).floor() as usize).max(1)
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
            let max_scroll = (content_h - (self.rect.height - 16.0)).max(0.0);
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
            let max_scroll_x = (content_w - (self.rect.width - 16.0)).max(0.0);
            let natural = crate::layout::touchpad_natural_scroll();
            let scroll_amt_x = match *delta {
                MouseScrollDelta::LineDelta(dx, dy) => {
                    if !self.multiline {
                        let scroll_val = if dy != 0.0 { -dy } else { if natural { -dx } else { dx } };
                        scroll_val * char_width * 3.0
                    } else {
                        let scroll_val = if natural { -dx } else { dx };
                        scroll_val * char_width * 3.0
                    }
                }
                MouseScrollDelta::PixelDelta(pos) => {
                    if !self.multiline {
                        let scroll_val = if pos.y != 0.0 { -pos.y as f32 } else { if natural { -pos.x as f32 } else { pos.x as f32 } };
                        scroll_val
                    } else {
                        if natural { -pos.x as f32 } else { pos.x as f32 }
                    }
                }
            };
            let old_scroll_x = self.scroll_x;
            self.scroll_x = (self.scroll_x + scroll_amt_x).clamp(0.0, max_scroll_x);
            if old_scroll_x != self.scroll_x {
                changed = true;
            }
        }

        changed
    }

    /// Selection highlight + caret quads, shared by both render branches. `x`/`w` are the
    /// (possibly label-inset) horizontal span the branch draws in — the legacy paths differed
    /// (non-rounded used the full base span, rounded inset by the side label).
    fn selection_quads(&self, x: f32, w: f32, out: &mut Vec<(f32, f32, f32, f32, [f32; 4])>) {
        if !(self.editing || self.select_anchor.is_some()) {
            return;
        }
        let top = self.label_top();
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

            let view_top = self.rect.y + top;
            let view_bottom = self.rect.y + self.rect.height;

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
                        let highlight_y = self.rect.y + top + 8.0 + (line_idx as f32 * line_height) - self.scroll_y;
                        let clipped_y = highlight_y.max(view_top);
                        let clipped_bottom = (highlight_y + line_height).min(view_bottom);
                        let h_left = highlight_x.max(x + 8.0);
                        let h_right = (highlight_x + highlight_w).min(x + w - 8.0);
                        if h_left < h_right && clipped_y < clipped_bottom {
                            out.push((h_left, clipped_y, h_right - h_left, clipped_bottom - clipped_y, highlight_color));
                        }
                    }
                }
            }

            if self.editing {
                let caret_h = self.font_size * 1.15;
                let (cursor_l, cursor_c) = index_map[self.cursor_idx.min(index_map.len() - 1)];
                let cursor_x = x + 8.0 + (cursor_c as f32 * char_width) - self.scroll_x;
                let cursor_y = self.rect.y + top + 8.0 + (cursor_l as f32 * line_height) + (line_height - caret_h) / 2.0 - self.scroll_y;
                let clipped_y = cursor_y.max(view_top);
                let clipped_bottom = (cursor_y + caret_h).min(view_bottom);
                if cursor_x >= x + 8.0 && cursor_x <= x + w - 8.0 {
                    if clipped_y < clipped_bottom {
                        out.push((cursor_x, clipped_y, 1.5, clipped_bottom - clipped_y, cursor_color));
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
                    out.push((
                        h_left,
                        crate::layout::align_text_y(self.rect.y, self.rect.height, self.font_size, top),
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
                let cursor_x = x + 8.0 + offset - self.scroll_x;
                if cursor_x >= x + 8.0 && cursor_x <= x + w - 8.0 {
                    let text_y = crate::layout::align_text_y(self.rect.y, self.rect.height, self.font_size, top);
                    let cursor_y = text_y + (self.font_size - caret_h) / 2.0;
                    out.push((cursor_x, cursor_y, 1.5, caret_h, cursor_color));
                }
            }
        }
    }

    /// The value/placeholder text lines — the legacy `text_labels` body minus the control
    /// label (the adapter's base-label machinery draws that).
    fn value_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        let top = self.label_top();
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

        let label_x = side_offset(&self.label);
        let x = self.rect.x + label_x;
        let w = self.rect.width - label_x;

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
                    y: self.rect.y + top + 8.0 + (line_idx as f32 * line_height) + (line_height - self.font_size) / 2.0 - self.scroll_y,
                    font_size: self.font_size,
                    color: label_color,
                });
            }
        } else {
            labels.push(TextLabel {
                text: display_text,
                x: x + 8.0 - self.scroll_x,
                y: crate::layout::align_text_y(self.rect.y, self.rect.height, self.font_size, top),
                font_size: self.font_size,
                color: label_color,
            });
        }
        labels
    }
}

impl Adapted<TextBox> {
    /// Recessed style: see the `recessed` field.
    pub fn with_recessed(mut self, recessed: bool) -> Self {
        self.recessed = recessed;
        self
    }

    pub fn with_update_on_type(mut self, update: bool) -> Self {
        self.update_on_type = update;
        self
    }

    pub fn with_multiline(mut self, multiline: bool) -> Self {
        self.multiline = multiline;
        self
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

    pub fn with_password(mut self, is_password: bool) -> Self {
        self.is_password = is_password;
        self
    }

    pub fn with_placeholder(mut self, placeholder: &str) -> Self {
        self.placeholder = Some(placeholder.to_string());
        self
    }

    pub fn with_max_width(mut self, max_w: Option<f32>) -> Self {
        self.max_width = max_w;
        self
    }

    pub fn with_width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }
}

impl Layout for TextBox {
    fn inflates_label_rect(&self) -> bool {
        false
    }

    fn detached_label_inset(&self) -> f32 {
        4.0
    }

    fn intrinsic_size(&self) -> Option<Size> {
        Some(Size::new(0.0, crate::layout::textbox_height()))
    }

    fn hit_row_rect(&self) -> bool {
        true
    }

    /// The legacy `set_rect` width clamp: an explicit `width` wins, else cap at `max_width`.
    fn adjust_rect(&self, requested: Rect) -> Rect {
        let final_w = if let Some(explicit_w) = self.width {
            explicit_w
        } else if let Some(max_w) = self.max_width {
            requested.width.min(max_w)
        } else {
            requested.width
        };
        Rect { width: final_w, ..requested }
    }

    /// The legacy `set_row_rect` applied the same clamp to the row span.
    fn adjust_row_rect(&self, x: f32, w: f32) -> (f32, f32) {
        let final_w = if let Some(explicit_w) = self.width {
            explicit_w
        } else if let Some(max_w) = self.max_width {
            w.min(max_w)
        } else {
            w
        };
        (x, final_w)
    }

    fn rect_assigned(&mut self, rect: Rect) {
        self.rect = rect;
        self.clamp_scroll();
    }
}

impl Paint for TextBox {
    fn color(&self) -> [f32; 4] {
        [0.10, 0.10, 0.16, 1.0]
    }

    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        let r = crate::layout::textbox_corner_radius();
        if r > 0.0 {
            Some((r, (true, true, true, true)))
        } else {
            Some((r, (false, false, false, false)))
        }
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::control_label_font_detached())
    }

    /// Content text font for the paint walk: a TextBox whose `font_family`/`font_size` was
    /// deliberately customized (cce-text-editor's monospace editor) draws its value text in
    /// that family at the label's own size — a bare family name, so the control-font string's
    /// size suffix doesn't override `font_size`. Default boxes keep the `widget_font` string
    /// verbatim (the legacy convention, size suffix included).
    fn text_font(&self) -> Option<String> {
        if self.font_family != self.default_font_family || self.font_size != self.default_font_size {
            Some(self.font_family.clone())
        } else {
            self.widget_font()
        }
    }

    fn sync_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    /// Legacy TextBox kept the shared focus-highlight overlay (the focused editor's
    /// primary-tint wash — data-editor's teal editing surface).
    fn legacy_focus_highlight(&self) -> bool {
        true
    }

    fn text_bounds(&self, rect: Rect) -> Option<[f32; 4]> {
        // Legacy bounded-text getters clipped to the full base rect, inset on the left by the
        // side label.
        let top = self.label_top();
        let base_y = rect.y - top;
        let base_h = rect.height + top;
        let label_x = side_offset(&self.label);
        Some([rect.x + label_x, base_y, rect.x + rect.width, base_y + base_h])
    }

    /// The legacy `prepare_text`: sync font family/size with the live config defaults, then
    /// shape the display text and record per-glyph advances (`map_x_to_idx` reads them).
    fn prepare_text(&mut self, fs: &mut glyphon::FontSystem, _rect: Rect) {
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

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let top = self.label_top();
        let base_y = rect.y - top;
        let base_h = rect.height + top;
        let visual_h = rect.height;
        let radius = crate::layout::textbox_corner_radius();
        let border_w = self.border_width();

        // Keep the model's cached rect and the paint rect consistent: paint receives the
        // content rect derived from the same base the cache holds, so the bodies below read
        // `self.rect` (the legacy `self.base`) exactly as legacy did. `rect` is used only to
        // localize this frame's geometry.
        let _ = (base_y, base_h);

        if radius <= 0.0 {
            // Legacy `extra_quads`: full base span (no side-label inset), disabled
            // special-case with early return.
            let mut quads: Vec<(f32, f32, f32, f32, [f32; 4])> = Vec::new();
            if self.disabled {
                if self.draw_bg_border {
                    quads.push((self.rect.x, self.rect.y + top, self.rect.width, visual_h, [0.12, 0.12, 0.16, 1.0]));
                    quads.push((self.rect.x + border_w, self.rect.y + top + border_w, self.rect.width - 2.0 * border_w, visual_h - 2.0 * border_w, [0.06, 0.06, 0.08, 1.0]));
                }
            } else {
                if self.draw_bg_border {
                    let bg_color = if self.editing {
                        crate::colors::textbox_background_edit_color()
                    } else {
                        crate::colors::textbox_background_color()
                    };
                    let border_color = if self.editing {
                        [0.20, 0.50, 0.85, 1.0]
                    } else if self.hovered {
                        [0.25, 0.25, 0.35, 1.0]
                    } else {
                        [0.18, 0.18, 0.24, 1.0]
                    };
                    quads.push((self.rect.x, self.rect.y + top, self.rect.width, visual_h, border_color));
                    quads.push((self.rect.x + border_w, self.rect.y + top + border_w, self.rect.width - 2.0 * border_w, visual_h - 2.0 * border_w, bg_color));
                }
                self.selection_quads(self.rect.x, self.rect.width, &mut quads);
            }
            for (qx, qy, qw, qh, qc) in quads {
                ctx.quad(Rect { x: qx, y: qy, width: qw, height: qh }, qc);
            }
        } else {
            // Legacy `all_rounded_quads`: side-label inset, no disabled special-case.
            let label_x = side_offset(&self.label);
            let x = self.rect.x + label_x;
            let w = self.rect.width - label_x;

            let bg_color = if self.editing {
                crate::colors::textbox_background_edit_color()
            } else {
                crate::colors::textbox_background_color()
            };
            let border_color = if self.editing {
                [0.20, 0.50, 0.85, 1.0]
            } else if self.hovered {
                [0.25, 0.25, 0.35, 1.0]
            } else {
                [0.18, 0.18, 0.24, 1.0]
            };

            if self.draw_bg_border {
                let corners = (true, true, true, true);
                // Recessed + transparent fill: the carve alone defines the
                // well — the plate below is its floor, so the flat border and
                // bg rects are skipped entirely. An opaque fill (e.g. the edit
                // color while editing) draws as usual and gets carved.
                let bare = self.recessed && bg_color[3] <= 0.001;
                if !bare {
                    ctx.rounded_rect(Rect { x, y: self.rect.y + top, width: w, height: visual_h }, radius, corners, border_color);
                    ctx.rounded_rect(
                        Rect { x: x + border_w, y: self.rect.y + top + border_w, width: w - 2.0 * border_w, height: visual_h - 2.0 * border_w },
                        (radius - border_w).max(0.0),
                        corners,
                        bg_color,
                    );
                }
                if self.recessed {
                    let depth = crate::layout::bevel_width().min(visual_h * 0.2);
                    ctx.recess(
                        Rect { x, y: self.rect.y + top, width: w, height: visual_h },
                        (radius, radius, radius, radius),
                        depth,
                    );
                }
            }

            let mut quads: Vec<(f32, f32, f32, f32, [f32; 4])> = Vec::new();
            self.selection_quads(x, w, &mut quads);
            for (qx, qy, qw, qh, qc) in quads {
                ctx.quad(Rect { x: qx, y: qy, width: qw, height: qh }, qc);
            }
        }

        for tl in self.value_labels() {
            ctx.text(tl.text, tl.x, tl.y, tl.font_size, tl.color);
        }
    }
}

impl Input for TextBox {
    fn tracks_base_focus(&self) -> bool {
        false
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button: MouseButton::Right, state: ElementState::Pressed, x: px, y: py, .. } => {
                if self.disabled { return false; }
                // Adapter hit-gates presses; legacy focused an un-editing box before opening
                // the menu (work-before-menu, so `opens_context_menu` can't express it).
                if !self.editing {
                    self.begin_editing();
                    ectx.request_focus();
                }
                ectx.open_context_menu(*px, *py);
                true
            }
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, x: px, y: py, .. } => {
                if self.disabled { return false; }
                if !self.editing {
                    self.begin_editing();
                    ectx.request_focus();
                } else {
                    let idx = self.position_to_idx(*px, *py, true);
                    self.cursor_idx = idx;
                    self.select_anchor = Some(idx);
                    self.all_selected = false;
                }
                true
            }
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Released, x: px, y: py, .. } => {
                if self.disabled { return false; }
                // Legacy gated releases on the hit test; the adapter delivers them ungated, so
                // re-check containment (plain rect + side inset — row spans approximated).
                let label_x = side_offset(&self.label);
                let top = self.label_top();
                let (bx, by, bw, bh) = (self.rect.x + label_x, self.rect.y, self.rect.width - label_x, self.rect.height);
                let _ = top;
                if !(*px >= bx && *px <= bx + bw && *py >= by && *py <= by + bh) {
                    return false;
                }
                if self.dragging {
                    self.dragging = false;
                }
                if self.select_anchor == Some(self.cursor_idx) {
                    self.select_anchor = None;
                }
                true
            }
            Event::PointerMove { x: px, y: py, .. } => {
                // The drag-selection half of the legacy `on_cursor_moved`; hover bookkeeping
                // is the adapter's (Enter/Leave below).
                if self.disabled {
                    return false;
                }
                if self.dragging && self.editing {
                    return self.extend_selection_to(*px, *py);
                }
                false
            }
            Event::MouseEnter => {
                self.hovered = !self.disabled;
                true
            }
            Event::MouseLeave => {
                self.hovered = false;
                true
            }
            Event::MouseWheel { delta, .. } => self.handle_wheel(delta),
            Event::KeyInput(key_event) => self.handle_key(key_event),
            Event::FocusIn => {
                // The legacy `focus()`: enter editing and claim the global slot (unless
                // disabled — legacy early-returned before `set_focused`).
                if !self.disabled {
                    self.begin_editing();
                    ectx.request_focus();
                }
                false
            }
            Event::FocusOut => {
                self.commit_editing();
                false
            }
            _ => false,
        }
    }

    fn take_change(&mut self) -> bool {
        self.take_change()
    }

    fn value_string(&self) -> Option<String> {
        Some(self.text.clone())
    }

    fn set_value_string(&mut self, val: &str) -> bool {
        self.set_value(val)
    }

    fn context_action(&mut self, action: crate::widget::ContextAction) -> bool {
        use crate::widget::ContextAction as CA;
        match action {
            CA::Cut => {
                let res = self.cut_selection();
                if res {
                    self.just_changed = true;
                }
                res
            }
            CA::Copy => {
                self.copy_selection();
                true
            }
            CA::Paste => {
                let res = self.paste_from_clipboard();
                if res {
                    self.just_changed = true;
                }
                res
            }
            CA::SelectAll => {
                self.select_all();
                true
            }
            CA::ClearText => {
                self.set_value("");
                true
            }
            _ => false,
        }
    }

    fn draggable(&self, _rect: Rect) -> bool {
        !self.disabled
    }

    fn is_dragging(&self) -> bool {
        self.dragging
    }

    fn drag_begin(&mut self, _px: f32, _py: f32, _rect: Rect) {
        if self.disabled || !self.editing { return; }
        self.dragging = true;
    }

    fn drag_update(&mut self, px: f32, py: f32, _rect: Rect) -> bool {
        if self.disabled || !self.editing { return false; }
        self.extend_selection_to(px, py)
    }

    fn drag_end(&mut self) {
        self.dragging = false;
    }
}

/// Legacy `Default` (an empty box) — settings' accounts page derives `Default` over fields of
/// this type.
impl Default for Adapted<TextBox> {
    fn default() -> Self {
        TextBox::new(String::new())
    }
}

unsafe impl Send for TextBox {}
unsafe impl Sync for TextBox {}


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

        let has_rounded = WidgetHost::corner_style(&tb).1 != (false, false, false, false);
        let has_highlight = if has_rounded {
            let rounded = tb.all_rounded_quads(&dummy);
            println!("Rounded quads: {:?}", rounded);
            let quads = tb.all_quads(&dummy);
            rounded.iter().any(|q| q.5 == [0.20, 0.50, 0.85, 0.3])
                || quads.iter().any(|q| q.4 == [0.20, 0.50, 0.85, 0.3])
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
        WidgetHost::context_action(&mut tb, crate::widget::ContextAction::ClearText);
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
