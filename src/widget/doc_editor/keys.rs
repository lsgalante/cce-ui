//! The keymap: one key at a time, with Enter's list continuation, Tab's indenting and Ctrl+B /
//! Ctrl+I wrapping the selection; edits from the host; undo and redo.

use super::*;

impl DocEditor {
    /// A key press. Undo/redo chords are the host's to route (the runner
    /// sends them to `Application::undo`); call [`DocEditor::undo`] there.
    pub fn key(&mut self, ev: &KeyEvent) -> Response {
        self.sync();
        // While an input method composes, its keys are its own: a shell does
        // not deliver them, and one that does is not editing this text.
        self.sync_ime();
        if self.composition.is_some() {
            return Response::None;
        }
        let rev = self.buf.revision;
        let caret = (self.buf.caret, self.buf.anchor);
        let (ctrl, shift) = (ev.ctrl, ev.shift);
        let mut keep_x = false;
        match &ev.logical_key {
            Key::Named(NamedKey::ArrowLeft) => {
                let to = match (self.buf.selection(), shift, ctrl) {
                    (Some((a, _)), false, false) => a,
                    (_, _, true) => self.buf.word_left(self.buf.caret),
                    _ => self.buf.prev(self.buf.caret),
                };
                self.buf.set_caret(to, shift);
            }
            Key::Named(NamedKey::ArrowRight) => {
                let to = match (self.buf.selection(), shift, ctrl) {
                    (Some((_, b)), false, false) => b,
                    (_, _, true) => self.buf.word_right(self.buf.caret),
                    _ => self.buf.next(self.buf.caret),
                };
                self.buf.set_caret(to, shift);
            }
            Key::Named(NamedKey::ArrowUp) => {
                self.vertical(-1, shift);
                keep_x = true;
            }
            Key::Named(NamedKey::ArrowDown) => {
                self.vertical(1, shift);
                keep_x = true;
            }
            Key::Named(NamedKey::PageUp) | Key::Named(NamedKey::PageDown) => {
                let row = (self.theme.size * self.theme.spacing).max(1.0);
                let rows = ((self.viewport.height - 2.0 * self.pad) / row).floor().max(1.0) as i32;
                let dir = if matches!(ev.logical_key, Key::Named(NamedKey::PageUp)) { -1 } else { 1 };
                self.vertical(dir * rows, shift);
                keep_x = true;
            }
            Key::Named(NamedKey::Home) => {
                let to = if ctrl {
                    Pos::default()
                } else {
                    // Smart home: the content start first, then column 0.
                    let p = self.buf.caret;
                    self.ensure(p.line);
                    let start = self.content_start(p.line);
                    Pos::new(p.line, if p.col > start { start } else { 0 })
                };
                self.buf.set_caret(to, shift);
            }
            Key::Named(NamedKey::End) => {
                let to = if ctrl { self.buf.end() } else { Pos::new(self.buf.caret.line, self.buf.line(self.buf.caret.line).len()) };
                self.buf.set_caret(to, shift);
            }
            Key::Named(NamedKey::Backspace) => self.buf.backspace(ctrl),
            Key::Named(NamedKey::Delete) => self.buf.delete_forward(ctrl),
            Key::Named(NamedKey::Enter) => self.enter(),
            Key::Named(NamedKey::Tab) => self.tab(shift),
            Key::Named(NamedKey::Escape) => {
                if self.buf.anchor.is_some() {
                    self.buf.anchor = None;
                } else {
                    return Response::None;
                }
            }
            Key::Character(c) if ctrl => match c.to_lowercase().as_str() {
                "a" => self.buf.select_all(),
                "c" => {
                    if let Some(t) = self.buf.selected_text() {
                        crate::widget::clipboard::copy_to_clipboard(&t);
                    }
                }
                "x" => {
                    if let Some(t) = self.buf.selected_text() {
                        crate::widget::clipboard::copy_to_clipboard(&t);
                        self.buf.delete_selection();
                    }
                }
                "v" => {
                    if let Some(t) = crate::widget::clipboard::read_from_clipboard() {
                        self.buf.insert(&t.replace("\r\n", "\n"), EditKind::Other);
                    }
                }
                "b" => self.wrap("**"),
                "i" => self.wrap("*"),
                _ => return Response::None,
            },
            _ => {
                let Some(text) = ev.text.as_deref() else { return Response::None };
                if ctrl || ev.alt || text.is_empty() || text.chars().any(|c| c.is_control()) {
                    return Response::None;
                }
                let kind = if text.chars().all(char::is_whitespace) { EditKind::Other } else { EditKind::Typing };
                self.buf.insert(text, kind);
            }
        }
        if !keep_x {
            self.want_x = None;
        }
        self.after_edit(rev, caret)
    }

    pub(super) fn after_edit(&mut self, rev: u64, caret: (Pos, Option<Pos>)) -> Response {
        if self.buf.revision != rev {
            self.sync();
            self.follow_caret = true;
            Response::Changed
        } else if (self.buf.caret, self.buf.anchor) != caret {
            self.sync();
            self.follow_caret = true;
            Response::Moved
        } else {
            Response::None
        }
    }

    pub(super) fn content_start(&self, line: usize) -> usize {
        preview::style_line(self.buf.line(line), self.ctx.get(line).copied().unwrap_or(Context::Normal), true).content_start
    }

    /// Enter continues a list item (`- `, `1. ` counting on, `- [ ] `),
    /// and on an empty item ends the list instead, as Obsidian does.
    pub(super) fn enter(&mut self) {
        let p = self.buf.caret;
        let text = self.buf.line(p.line).to_string();
        let ctx = self.ctx.get(p.line).copied().unwrap_or(Context::Normal);
        let line = preview::style_line(&text, ctx, true);
        if self.buf.selection().is_none() {
            if let Kind::List { marker, .. } = &line.kind {
                let body = &text[line.content_start..];
                if body.trim().is_empty() && p.col >= line.content_start {
                    // An empty item: drop its marker, leave the list.
                    self.buf.replace(Pos::new(p.line, 0), Pos::new(p.line, text.len()), "", EditKind::Other);
                    return;
                }
                let indent: String = text.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
                let next = match marker {
                    Marker::Bullet => format!("{indent}{} ", text.trim_start().chars().next().unwrap_or('-')),
                    Marker::Number(n) => {
                        let digits: String = n.chars().filter(char::is_ascii_digit).collect();
                        let delim = n.chars().last().unwrap_or('.');
                        format!("{indent}{}{delim} ", digits.parse::<u64>().unwrap_or(0) + 1)
                    }
                    Marker::Task(..) => {
                        let bullet = text.trim_start().chars().next().unwrap_or('-');
                        format!("{indent}{bullet} [ ] ")
                    }
                };
                if p.col >= line.content_start {
                    self.buf.insert(&format!("\n{next}"), EditKind::Other);
                    return;
                }
            }
        }
        self.buf.insert("\n", EditKind::Other);
    }

    /// Tab indents a list item (or the selected lines) by a tab; Shift+Tab
    /// takes one level off. Anywhere else Tab inserts a tab.
    pub(super) fn tab(&mut self, outdent: bool) {
        let (a, b) = self.buf.selection().unwrap_or((self.buf.caret, self.buf.caret));
        let is_list = |s: &Self, i: usize| matches!(preview::style_line(s.buf.line(i), s.ctx.get(i).copied().unwrap_or(Context::Normal), true).kind, Kind::List { .. });
        if a.line == b.line && !is_list(self, a.line) && !outdent {
            self.buf.insert("\t", EditKind::Other);
            return;
        }
        let (caret, anchor) = (self.buf.caret, self.buf.anchor);
        let mut caret_delta = 0isize;
        let mut anchor_delta = 0isize;
        for i in a.line..=b.line {
            let text = self.buf.line(i).to_string();
            let d: isize = if outdent {
                let n = if text.starts_with('\t') {
                    1
                } else {
                    text.chars().take(4).take_while(|c| *c == ' ').count()
                };
                if n == 0 {
                    continue;
                }
                self.buf.replace(Pos::new(i, 0), Pos::new(i, n), "", EditKind::Other);
                -(n as isize)
            } else {
                self.buf.replace(Pos::new(i, 0), Pos::new(i, 0), "\t", EditKind::Other);
                1
            };
            if i == caret.line {
                caret_delta = d;
            }
            if anchor.is_some_and(|x| x.line == i) {
                anchor_delta = d;
            }
        }
        let fix = |p: Pos, d: isize| Pos::new(p.line, (p.col as isize + d).max(0) as usize);
        self.buf.caret = self.buf.clamp(fix(caret, caret_delta));
        self.buf.anchor = anchor.map(|x| self.buf.clamp(fix(x, anchor_delta)));
    }

    /// Wrap the selection in `mark` (Ctrl+B, Ctrl+I), or insert a pair
    /// with the caret between.
    pub(super) fn wrap(&mut self, mark: &str) {
        match self.buf.selection() {
            Some((a, b)) if a.line == b.line => {
                let inner = self.buf.text_range(a, b);
                self.buf.replace(a, b, &format!("{mark}{inner}{mark}"), EditKind::Other);
                self.buf.anchor = Some(Pos::new(a.line, a.col + mark.len()));
                self.buf.caret = Pos::new(a.line, a.col + mark.len() + inner.len());
            }
            Some(_) => {}
            None => {
                let at = self.buf.caret;
                self.buf.insert(&format!("{mark}{mark}"), EditKind::Other);
                self.buf.caret = Pos::new(at.line, at.col + mark.len());
            }
        }
    }

    /// Replace `a..b` with `text` as one undo step and put the caret at
    /// `caret` (a host's splice: a completion, a template).
    pub fn edit(&mut self, a: Pos, b: Pos, text: &str, caret: Pos) {
        self.buf.replace(a, b, text, EditKind::Other);
        self.buf.anchor = None;
        self.buf.caret = self.buf.clamp(caret);
        self.want_x = None;
        self.sync();
        self.follow_caret = true;
    }

    pub fn undo(&mut self) -> bool {
        let done = self.buf.undo();
        if done {
            self.sync();
            self.follow_caret = true;
        }
        done
    }

    pub fn redo(&mut self) -> bool {
        let done = self.buf.redo();
        if done {
            self.sync();
            self.follow_caret = true;
        }
        done
    }
}
