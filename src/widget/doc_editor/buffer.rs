//! The editor's text: a vector of lines (no `\n` stored), a caret and a
//! selection anchor, and edits that can be undone.
//!
//! Lines rather than a rope: every edit is O(length of the lines it
//! touches) plus a splice of line pointers, line access is O(1), and the
//! layout caches per line — so a change reports exactly which lines it
//! replaced ([`Change`]) and only those are shaped again. A 100k-line file
//! moves 2.4 MB of pointers on a line insert, well under a frame.
//!
//! Positions are (line, byte column) and always sit on a char boundary.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct Pos {
    pub line: usize,
    /// Byte offset into the line, on a char boundary.
    pub col: usize,
}

impl Pos {
    pub fn new(line: usize, col: usize) -> Pos {
        Pos { line, col }
    }
}

/// Lines `first .. first + removed` were replaced by `inserted` lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Change {
    pub first: usize,
    pub removed: usize,
    pub inserted: usize,
}

/// How an edit groups for undo: a run of typing (or of deleting) undoes
/// as one step; anything else is a step of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    Typing,
    Deleting,
    Other,
}

#[derive(Clone, Debug)]
struct Edit {
    start: Pos,
    removed: String,
    inserted: String,
    caret_before: (Pos, Option<Pos>),
    caret_after: (Pos, Option<Pos>),
    kind: EditKind,
}

/// Where `text` inserted at `start` ends.
fn end_of(start: Pos, text: &str) -> Pos {
    match text.rfind('\n') {
        Some(i) => Pos::new(start.line + text.matches('\n').count(), text.len() - i - 1),
        None => Pos::new(start.line, start.col + text.len()),
    }
}

pub struct Buffer {
    lines: Vec<String>,
    pub caret: Pos,
    /// The other end of the selection, when there is one.
    pub anchor: Option<Pos>,
    undo: Vec<Edit>,
    redo: Vec<Edit>,
    changes: Vec<Change>,
    /// Bumped on every change to the text.
    pub revision: u64,
}

impl Buffer {
    pub fn new(text: &str) -> Buffer {
        let mut b = Buffer { lines: Vec::new(), caret: Pos::default(), anchor: None, undo: Vec::new(), redo: Vec::new(), changes: Vec::new(), revision: 0 };
        b.set_text(text);
        b
    }

    /// Replace everything: caret to the start, history and changes cleared
    /// (the layout is rebuilt whole).
    pub fn set_text(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n");
        self.lines = text.split('\n').map(String::from).collect();
        self.caret = Pos::default();
        self.anchor = None;
        self.undo.clear();
        self.redo.clear();
        self.changes.clear();
        self.changes.push(Change { first: 0, removed: usize::MAX, inserted: self.lines.len() });
        self.revision += 1;
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn line(&self, i: usize) -> &str {
        self.lines.get(i).map(String::as_str).unwrap_or("")
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// What changed since the last call, in order. A `removed` of
    /// `usize::MAX` means everything.
    pub fn take_changes(&mut self) -> Vec<Change> {
        std::mem::take(&mut self.changes)
    }

    pub fn clamp(&self, p: Pos) -> Pos {
        let line = p.line.min(self.lines.len().saturating_sub(1));
        let s = self.line(line);
        let mut col = p.col.min(s.len());
        while !s.is_char_boundary(col) {
            col -= 1;
        }
        Pos::new(line, col)
    }

    pub fn end(&self) -> Pos {
        let last = self.lines.len() - 1;
        Pos::new(last, self.lines[last].len())
    }

    /// The selection, ordered, when it is not empty.
    pub fn selection(&self) -> Option<(Pos, Pos)> {
        let a = self.anchor?;
        if a == self.caret {
            return None;
        }
        Some((a.min(self.caret), a.max(self.caret)))
    }

    pub fn text_range(&self, a: Pos, b: Pos) -> String {
        let (a, b) = (a.min(b), a.max(b));
        if a.line == b.line {
            return self.line(a.line)[a.col..b.col].to_string();
        }
        let mut out = self.line(a.line)[a.col..].to_string();
        for l in a.line + 1..b.line {
            out.push('\n');
            out.push_str(self.line(l));
        }
        out.push('\n');
        out.push_str(&self.line(b.line)[..b.col]);
        out
    }

    pub fn selected_text(&self) -> Option<String> {
        self.selection().map(|(a, b)| self.text_range(a, b))
    }

    /// Replace `a..b` with `text`, no history. Returns where the inserted
    /// text ends.
    fn raw_replace(&mut self, a: Pos, b: Pos, text: &str) -> Pos {
        let (a, b) = (a.min(b), a.max(b));
        let head = self.lines[a.line][..a.col].to_string();
        let tail = self.lines[b.line][b.col..].to_string();
        let mut new: Vec<String> = text.split('\n').map(String::from).collect();
        let n = new.len();
        let end = if n == 1 { Pos::new(a.line, a.col + new[0].len()) } else { Pos::new(a.line + n - 1, new[n - 1].len()) };
        new[0].insert_str(0, &head);
        new[n - 1].push_str(&tail);
        self.lines.splice(a.line..=b.line, new);
        self.changes.push(Change { first: a.line, removed: b.line - a.line + 1, inserted: n });
        self.revision += 1;
        end
    }

    /// Replace `a..b` with `text` as one undoable edit; the caret lands at
    /// the end of the inserted text, and the selection goes.
    pub fn replace(&mut self, a: Pos, b: Pos, text: &str, kind: EditKind) {
        let (a, b) = (self.clamp(a.min(b)), self.clamp(a.max(b)));
        let removed = self.text_range(a, b);
        if removed.is_empty() && text.is_empty() {
            return;
        }
        let before = (self.caret, self.anchor);
        let end = self.raw_replace(a, b, text);
        self.caret = end;
        self.anchor = None;
        self.redo.clear();
        let edit = Edit { start: a, removed, inserted: text.to_string(), caret_before: before, caret_after: (end, None), kind };
        // A run of typing (or deleting) is one undo step.
        if let Some(last) = self.undo.last_mut() {
            let joins = match kind {
                EditKind::Typing => {
                    last.kind == EditKind::Typing
                        && edit.removed.is_empty()
                        && !text.contains('\n')
                        && end_of(last.start, &last.inserted) == a
                }
                EditKind::Deleting => last.kind == EditKind::Deleting && edit.inserted.is_empty() && (last.start == b || last.start == a),
                EditKind::Other => false,
            };
            if joins {
                match kind {
                    EditKind::Typing => last.inserted.push_str(text),
                    _ if last.start == b => {
                        // Backspacing: the new text removed sits before.
                        last.removed.insert_str(0, &edit.removed);
                        last.start = a;
                    }
                    _ => last.removed.push_str(&edit.removed),
                }
                last.caret_after = edit.caret_after;
                return;
            }
        }
        self.undo.push(edit);
    }

    /// Type `text` over the selection, or at the caret.
    pub fn insert(&mut self, text: &str, kind: EditKind) {
        let (a, b) = self.selection().unwrap_or((self.caret, self.caret));
        self.replace(a, b, text, kind);
    }

    pub fn delete_selection(&mut self) -> bool {
        match self.selection() {
            Some((a, b)) => {
                self.replace(a, b, "", EditKind::Other);
                true
            }
            None => false,
        }
    }

    pub fn backspace(&mut self, word: bool) {
        if self.delete_selection() {
            return;
        }
        let to = if word { self.word_left(self.caret) } else { self.prev(self.caret) };
        if to != self.caret {
            self.replace(to, self.caret, "", EditKind::Deleting);
        }
    }

    pub fn delete_forward(&mut self, word: bool) {
        if self.delete_selection() {
            return;
        }
        let to = if word { self.word_right(self.caret) } else { self.next(self.caret) };
        if to != self.caret {
            let at = self.caret;
            self.replace(at, to, "", EditKind::Deleting);
            self.caret = at;
        }
    }

    pub fn undo(&mut self) -> bool {
        let Some(e) = self.undo.pop() else { return false };
        let end = end_of(e.start, &e.inserted);
        self.raw_replace(e.start, end, &e.removed);
        (self.caret, self.anchor) = e.caret_before;
        self.redo.push(e);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(e) = self.redo.pop() else { return false };
        let end = end_of(e.start, &e.removed);
        self.raw_replace(e.start, end, &e.inserted);
        (self.caret, self.anchor) = e.caret_after;
        self.undo.push(e);
        true
    }

    /// Break the current typing run, so the next keystroke starts a new
    /// undo step (a caret move between them, a pause).
    pub fn seal(&mut self) {
        if let Some(last) = self.undo.last_mut() {
            last.kind = EditKind::Other;
        }
    }

    /// Move the caret, extending the selection when `select`.
    pub fn set_caret(&mut self, p: Pos, select: bool) {
        let p = self.clamp(p);
        if select {
            if self.anchor.is_none() {
                self.anchor = Some(self.caret);
            }
        } else {
            self.anchor = None;
        }
        self.caret = p;
        self.seal();
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(Pos::default());
        self.caret = self.end();
    }

    // ---- boundaries ----------------------------------------------------

    pub fn prev(&self, p: Pos) -> Pos {
        if p.col == 0 {
            return if p.line == 0 { p } else { Pos::new(p.line - 1, self.line(p.line - 1).len()) };
        }
        let s = self.line(p.line);
        let col = s[..p.col].char_indices().next_back().map(|(i, _)| i).unwrap_or(0);
        Pos::new(p.line, col)
    }

    pub fn next(&self, p: Pos) -> Pos {
        let s = self.line(p.line);
        if p.col >= s.len() {
            return if p.line + 1 >= self.lines.len() { p } else { Pos::new(p.line + 1, 0) };
        }
        let c = s[p.col..].chars().next().map(char::len_utf8).unwrap_or(1);
        Pos::new(p.line, p.col + c)
    }

    /// The start of the word before `p` (skipping spaces first), or the
    /// end of the previous line at a line start.
    pub fn word_left(&self, p: Pos) -> Pos {
        if p.col == 0 {
            return self.prev(p);
        }
        let s = &self.line(p.line)[..p.col];
        let chars: Vec<(usize, char)> = s.char_indices().collect();
        let mut i = chars.len();
        while i > 0 && chars[i - 1].1.is_whitespace() {
            i -= 1;
        }
        let word = |c: char| c.is_alphanumeric() || c == '_';
        if i > 0 {
            let in_word = word(chars[i - 1].1);
            while i > 0 && !chars[i - 1].1.is_whitespace() && word(chars[i - 1].1) == in_word {
                i -= 1;
            }
        }
        Pos::new(p.line, chars.get(i).map(|(b, _)| *b).unwrap_or(0))
    }

    pub fn word_right(&self, p: Pos) -> Pos {
        let s = self.line(p.line);
        if p.col >= s.len() {
            return self.next(p);
        }
        let rest: Vec<(usize, char)> = s[p.col..].char_indices().collect();
        let mut i = 0;
        while i < rest.len() && rest[i].1.is_whitespace() {
            i += 1;
        }
        let word = |c: char| c.is_alphanumeric() || c == '_';
        if i < rest.len() {
            let in_word = word(rest[i].1);
            while i < rest.len() && !rest[i].1.is_whitespace() && word(rest[i].1) == in_word {
                i += 1;
            }
        }
        Pos::new(p.line, p.col + rest.get(i).map(|(b, _)| *b).unwrap_or(s.len() - p.col))
    }

    /// The word around `p` (a double-click's selection).
    pub fn word_at(&self, p: Pos) -> (Pos, Pos) {
        let s = self.line(p.line);
        let word = |c: char| c.is_alphanumeric() || c == '_';
        let mut a = p.col;
        while a > 0 {
            let prev = s[..a].chars().next_back().unwrap();
            if !word(prev) {
                break;
            }
            a -= prev.len_utf8();
        }
        let mut b = p.col;
        while let Some(c) = s[b..].chars().next() {
            if !word(c) {
                break;
            }
            b += c.len_utf8();
        }
        (Pos::new(p.line, a), Pos::new(p.line, b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_newlines_and_changes() {
        let mut b = Buffer::new("ab\ncd");
        b.take_changes();
        b.caret = Pos::new(0, 1);
        b.insert("X\nY", EditKind::Other);
        assert_eq!(b.text(), "aX\nYb\ncd");
        assert_eq!(b.caret, Pos::new(1, 1));
        assert_eq!(b.take_changes(), [Change { first: 0, removed: 1, inserted: 2 }]);
        b.set_caret(Pos::new(0, 1), false);
        b.set_caret(Pos::new(2, 1), true);
        assert_eq!(b.selected_text().as_deref(), Some("X\nYb\nc"));
        b.insert("", EditKind::Other);
        assert_eq!(b.text(), "ad");
    }

    #[test]
    fn typing_undoes_as_one_step_and_redoes() {
        let mut b = Buffer::new("");
        for c in ["h", "e", "y"] {
            b.insert(c, EditKind::Typing);
        }
        b.insert("\n", EditKind::Other);
        b.insert("x", EditKind::Typing);
        assert_eq!(b.text(), "hey\nx");
        assert!(b.undo());
        assert_eq!(b.text(), "hey\n");
        assert!(b.undo());
        assert!(b.undo());
        assert_eq!(b.text(), "");
        assert!(!b.undo());
        assert!(b.redo());
        assert_eq!(b.text(), "hey");
        assert_eq!(b.caret, Pos::new(0, 3));
    }

    #[test]
    fn backspace_runs_join_and_restore() {
        let mut b = Buffer::new("héllo wörld");
        b.caret = b.end();
        for _ in 0..3 {
            b.backspace(false);
        }
        assert_eq!(b.text(), "héllo wö");
        b.backspace(true);
        assert_eq!(b.text(), "héllo ");
        assert!(b.undo());
        assert_eq!(b.text(), "héllo wörld");
        // At a line start, backspace joins lines.
        let mut b = Buffer::new("a\nb");
        b.caret = Pos::new(1, 0);
        b.backspace(false);
        assert_eq!(b.text(), "ab");
        assert_eq!(b.caret, Pos::new(0, 1));
    }

    #[test]
    fn words_and_boundaries() {
        let b = Buffer::new("foo bar_baz  qux.");
        assert_eq!(b.word_right(Pos::new(0, 0)), Pos::new(0, 3));
        assert_eq!(b.word_right(Pos::new(0, 3)), Pos::new(0, 11));
        assert_eq!(b.word_left(Pos::new(0, 13)), Pos::new(0, 4));
        assert_eq!(b.word_at(Pos::new(0, 6)), (Pos::new(0, 4), Pos::new(0, 11)));
        let b = Buffer::new("é");
        assert_eq!(b.next(Pos::new(0, 0)), Pos::new(0, 2));
        assert_eq!(b.clamp(Pos::new(0, 1)), Pos::new(0, 0));
    }

    #[test]
    fn delete_forward_keeps_the_caret() {
        let mut b = Buffer::new("abc");
        b.caret = Pos::new(0, 1);
        b.delete_forward(false);
        b.delete_forward(false);
        assert_eq!(b.text(), "a");
        assert_eq!(b.caret, Pos::new(0, 1));
        assert!(b.undo());
        assert_eq!(b.text(), "abc");
    }
}
