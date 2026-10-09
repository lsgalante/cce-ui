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
