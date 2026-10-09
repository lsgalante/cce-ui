//! The inline code editor on a code row: its geometry, its state, and the clipboard,
//! selection and history actions it shares with its chords.

use super::*;

impl ParametersBg {
    /// Where a code row's text starts: past the gutter.
    pub(super) fn code_text_x(&self, r: (f32, f32, f32, f32)) -> f32 {
        r.0 + 12.0 + (Self::CODE_GUTTER_COLS + 1.0) * self.code_col_w()
    }

    /// Flag a line of the code row as an error site, or clear it. The host
    /// calls this when the value it evaluated failed with a line number —
    /// it is the one piece of feedback a script editor cannot do without.
    pub fn set_code_error_line(&mut self, line: Option<usize>) {
        self.code_error_line = line;
    }

    /// Whether a code row is being edited right now.
    pub fn code_editing(&self) -> bool {
        self.focused_param.is_some() && self.code_editor.is_some()
    }

    /// The clipboard, selection and history actions over the code editor:
    /// what the runner's undo / redo chords and the context menu's rows
    /// reach through [`Input::context_action`], and what the editor's own
    /// chords call. Returns whether the editor was open to act on.
    pub fn code_action(&mut self, action: crate::widget::ContextAction) -> bool {
        match self.code_editor.as_mut() {
            Some(editor) => {
                apply_code_action(editor, &mut self.code_history, action);
                true
            }
            None => false,
        }
    }

    /// Whether the focused code row holds edits not yet applied to its value.
    pub fn code_is_dirty(&self) -> bool {
        match (self.focused_param, &self.code_editor) {
            (Some(i), Some(editor)) => self.display_params.get(i).is_some_and(|p| p.1 != editor.buffer),
            _ => false,
        }
    }

    /// One code column's width — the shaped advance when recorded, else the
    /// legacy 7.2 estimate (only before the first `prepare_text`).
    pub(super) fn code_col_w(&self) -> f32 {
        if self.code_char_advance > 0.0 {
            self.code_char_advance
        } else {
            7.2
        }
    }

    /// A key on the focused code row. Edits go to the BUFFER, not the value: a host
    /// re-evaluates a script on every value change, and a half-typed line would fail on every
    /// keystroke. ctrl+enter applies, and so does leaving the row (Escape, a click elsewhere,
    /// `unfocus`). Typing runs coalesce into one undo step; anything structural (a newline, a
    /// paste, deleting a selection) starts a new one.
    pub(super) fn code_key(&mut self, idx: usize, event: &crate::widget::KeyEvent) -> bool {
        let Some(mut editor) = self.code_editor.take() else {
            return false;
        };
        let before = editor.clone();
        let k = code_editor_key(&mut editor, &mut self.code_history, event);
        if k.edited {
            let plain_char = matches!(&event.logical_key, Key::Character(c) if !event.ctrl && c.chars().count() == 1);
            if plain_char {
                self.code_history.record_grouped(before, 1);
            } else {
                self.code_history.record(before);
            }
        }
        if k.apply {
            self.display_params[idx].1 = editor.buffer.clone();
        }
        if k.unfocus {
            self.focused_param = None;
            self.code_editor = None;
            self.code_history.clear();
        } else {
            self.code_editor = Some(editor);
        }
        k.handled
    }

    /// A press in code row `i`'s box (`r` its row rect): place the caret there (shift extends
    /// the selection) and focus the row. A press in the box already being edited keeps its
    /// buffer and history; one in another row's box starts a fresh editor on that row's value.
    pub(super) fn place_code_caret(&mut self, i: usize, r: (f32, f32, f32, f32), px: f32, py: f32, shift: bool) {
        let mut editor = match (self.focused_param == Some(i), self.code_editor.take()) {
            (true, Some(e)) => e,
            (_, other) => {
                drop(other);
                self.code_history.clear();
                TextEditorState::new(self.display_params[i].1.clone())
            }
        };
        self.focused_param = Some(i);
        let click_x = px - self.code_text_x(r);
        let click_y = py - (r.1 + Self::CODE_TOP);
        let line = (click_y / Self::CODE_LINE_H).floor().max(0.0) as usize;
        let col = (click_x / self.code_col_w() + 0.5).floor().max(0.0) as usize;
        let at = map_2d_to_1d(&editor.buffer, line, col);
        if shift {
            if editor.select_anchor.is_none() {
                editor.select_anchor = Some(editor.cursor_idx);
            }
        } else {
            editor.clear_selection();
        }
        editor.cursor_idx = at;
        self.code_editor = Some(editor);
    }
}

/// What one key did to the code editor: whether it was the editor's at all, whether it
/// changed the buffer (for history), whether it applies the buffer to the row's value, and
/// whether it leaves the row.
pub(super) struct CodeKey {
    handled: bool,
    edited: bool,
    apply: bool,
    unfocus: bool,
}

/// One key press on a code editor: typing, deleting, moving and selecting (arrows, Home /
/// End, emacs chords), Tab / shift+Tab indenting, Enter carrying the line's indentation (a
/// level deeper after an opening bracket), and the clipboard and history chords — free of the
/// pane, so it is the editor's behaviour alone.
pub(super) fn code_editor_key(
    editor: &mut TextEditorState,
    history: &mut crate::history::History<TextEditorState>,
    event: &crate::widget::KeyEvent,
) -> CodeKey {
    let mut handled = true;
    let mut edited = false;
    let mut apply = false;
    let mut should_unfocus = false;
    let shift = event.shift;
    let move_vertical = |editor: &mut TextEditorState, delta: i32| {
        if shift && editor.select_anchor.is_none() {
            editor.select_anchor = Some(editor.cursor_idx);
        } else if !shift {
            editor.clear_selection();
        }
        let (line, col) = get_cursor_line_col(&editor.buffer, editor.cursor_idx);
        let total = editor.buffer.split('\n').count() as i32;
        let target = (line as i32 + delta).clamp(0, total - 1) as usize;
        editor.cursor_idx = map_2d_to_1d(&editor.buffer, target, col);
    };
    match &event.logical_key {
        Key::Named(NamedKey::Backspace) => {
            edited = editor.delete_backwards();
        }
        Key::Named(NamedKey::Delete) => {
            edited = editor.delete_forwards();
        }
        Key::Named(NamedKey::Enter) if event.ctrl => {
            apply = true;
        }
        Key::Named(NamedKey::Enter) => {
            // Auto-indent: the line's own leading whitespace,
            // one level deeper after an opening brace.
            let line_start = get_line_start(&editor.buffer, editor.cursor_idx);
            let line: String = editor.buffer.chars().skip(line_start).take(editor.cursor_idx - line_start).collect();
            let mut indent: String = line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
            if line.trim_end().ends_with('{') || line.trim_end().ends_with('(') || line.trim_end().ends_with('[') {
                indent.push_str(ParametersBg::CODE_INDENT);
            }
            editor.insert_text(&format!("\n{indent}"));
            edited = true;
        }
        Key::Named(NamedKey::Tab) if event.shift => {
            // Dedent the current line by one level, or what it has.
            let line_start = get_line_start(&editor.buffer, editor.cursor_idx);
            let leading = editor.buffer.chars().skip(line_start).take_while(|c| *c == ' ').count().min(ParametersBg::CODE_INDENT.len());
            if leading > 0 {
                let chars: Vec<char> = editor.buffer.chars().collect();
                editor.buffer = chars[..line_start].iter().chain(&chars[line_start + leading..]).collect();
                editor.cursor_idx = editor.cursor_idx.saturating_sub(leading).max(line_start);
                editor.clear_selection();
                edited = true;
            }
        }
        Key::Named(NamedKey::Tab) => {
            editor.insert_text(ParametersBg::CODE_INDENT);
            edited = true;
        }
        Key::Named(NamedKey::Escape) => {
            apply = true;
            should_unfocus = true;
        }
        Key::Named(NamedKey::ArrowLeft) => {
            editor.move_cursor_left(shift);
        }
        Key::Named(NamedKey::ArrowRight) => {
            editor.move_cursor_right(shift);
        }
        Key::Named(NamedKey::ArrowUp) => move_vertical(editor, -1),
        Key::Named(NamedKey::ArrowDown) => move_vertical(editor, 1),
        Key::Named(NamedKey::Home) => {
            if shift && editor.select_anchor.is_none() {
                editor.select_anchor = Some(editor.cursor_idx);
            } else if !shift {
                editor.clear_selection();
            }
            editor.cursor_idx = get_line_start(&editor.buffer, editor.cursor_idx);
        }
        Key::Named(NamedKey::End) => {
            if shift && editor.select_anchor.is_none() {
                editor.select_anchor = Some(editor.cursor_idx);
            } else if !shift {
                editor.clear_selection();
            }
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
                    "p" => move_vertical(editor, -1),
                    "n" => move_vertical(editor, 1),
                    "a" if event.shift => {
                        editor.select_all();
                    }
                    "a" => {
                        editor.clear_selection();
                        editor.cursor_idx = get_line_start(&editor.buffer, editor.cursor_idx);
                    }
                    "e" => {
                        editor.clear_selection();
                        editor.cursor_idx = get_line_end(&editor.buffer, editor.cursor_idx);
                    }
                    "d" => {
                        edited = editor.delete_forwards();
                    }
                    "h" => {
                        edited = editor.delete_backwards();
                    }
                    "k" => {
                        let current_idx = editor.cursor_idx;
                        let end_idx = get_line_end(&editor.buffer, current_idx);
                        let chars: Vec<char> = editor.buffer.chars().collect();
                        if current_idx < chars.len() {
                            let delete_end = if chars[current_idx] == '\n' { current_idx + 1 } else { end_idx };
                            editor.buffer = chars[..current_idx].iter().chain(&chars[delete_end..]).collect();
                            editor.clear_selection();
                            edited = true;
                        }
                    }
                    // The clipboard and history chords are the
                    // context actions, so the chord, the runner's
                    // routing and the menu row do one thing.
                    "c" | "x" | "v" | "z" => {
                        let action = match (s.to_lowercase().as_str(), event.shift) {
                            ("c", _) => crate::widget::ContextAction::Copy,
                            ("x", _) => crate::widget::ContextAction::Cut,
                            ("v", _) => crate::widget::ContextAction::Paste,
                            ("z", true) => crate::widget::ContextAction::Redo,
                            _ => crate::widget::ContextAction::Undo,
                        };
                        apply_code_action(editor, history, action);
                    }
                    _ => {
                        handled = false;
                    }
                }
            } else {
                editor.insert_text(s);
                edited = true;
            }
        }
        _ => {
            handled = false;
        }
    }
    CodeKey { handled, edited, apply, unfocus: should_unfocus }
}

/// One clipboard, selection or history action over a code editor and its
/// history — the body [`ParametersBg::code_action`] and the editor's own
/// chords share, free of `self` so a caller holding a row borrow can use it.
pub(super) fn apply_code_action(
    editor: &mut TextEditorState,
    history: &mut crate::history::History<TextEditorState>,
    action: crate::widget::ContextAction,
) {
    use crate::widget::ContextAction;
    let before = editor.clone();
    let mut edited = false;
    match action {
        ContextAction::Undo => {
            if let Some(prev) = history.undo(editor.clone()) {
                *editor = prev;
            }
        }
        ContextAction::Redo => {
            if let Some(next) = history.redo(editor.clone()) {
                *editor = next;
            }
        }
        ContextAction::SelectAll => editor.select_all(),
        ContextAction::Copy => {
            if let Some(text) = editor.selected_text() {
                crate::widget::clipboard::copy_to_clipboard(&text);
            }
        }
        ContextAction::Cut => {
            if let Some(text) = editor.selected_text() {
                crate::widget::clipboard::copy_to_clipboard(&text);
                edited = editor.delete_backwards();
            }
        }
        ContextAction::Paste => {
            if let Some(text) = crate::widget::clipboard::read_from_clipboard() {
                editor.insert_text(&text);
                edited = true;
            }
        }
        _ => {}
    }
    if edited {
        history.record(before);
    }
}

pub(super) fn get_cursor_line_col(buffer: &str, cursor_idx: usize) -> (usize, usize) {
    let mut cur_line = 0;
    let mut cur_col = 0;
    for (count, c) in buffer.chars().enumerate() {
        if count == cursor_idx {
            return (cur_line, cur_col);
        }
        if c == '\n' {
            cur_line += 1;
            cur_col = 0;
        } else {
            cur_col += 1;
        }
    }
    (cur_line, cur_col)
}

pub(super) fn map_2d_to_1d(buffer: &str, line: usize, col: usize) -> usize {
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

pub(super) fn get_line_start(buffer: &str, cursor_idx: usize) -> usize {
    let (line, _) = get_cursor_line_col(buffer, cursor_idx);
    map_2d_to_1d(buffer, line, 0)
}

pub(super) fn get_line_end(buffer: &str, cursor_idx: usize) -> usize {
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
