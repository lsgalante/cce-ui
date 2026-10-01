//! `DocEditor`: a document editor with Markdown live preview — cce-notes'
//! editing mode (Obsidian-on-cce milestone 6), and a fast plain editor with
//! preview off.
//!
//! - [`buffer`]: the text as lines, caret and selection, undoable edits.
//! - [`preview`]: how a line shows — markup hidden except on the caret's
//!   line, where it shows dimmed (Obsidian's live preview).
//! - [`layout`]: one styled line wrapped into runs, with the x of every
//!   byte, so drawing, caret and clicks agree.
//!
//! **Incremental.** A line is laid out (shaped) only when it is drawn and
//! has changed: an edit invalidates the lines it replaced
//! ([`buffer::Change`]), a moved caret the lines it left and entered (their
//! markers show or hide), a fence the block contexts after it. Lines never
//! drawn keep an estimated height, so opening a long file shapes one
//! screen of it.
//!
//! **Driven by its host, not a registered widget.** The app forwards keys,
//! pointer events and the wheel, and paints it into its display list
//! ([`DocEditor::paint`]); answers come back as [`Response`] — a link to
//! follow is the host's to resolve. The caret does not blink: a blinking
//! caret is a frame every half second for as long as the window is open.

pub mod buffer;
pub mod layout;
pub mod preview;

use std::time::{Duration, Instant};

pub use buffer::{Buffer, EditKind, Pos};
pub use layout::EditorTheme;
pub use preview::Target;

use crate::scene::layout::Rect;
use crate::scene::paint::{Cap, PaintCtx};
use crate::widget::shaping::ShapingMeasure;
use crate::widget::{Bounds, Key, KeyEvent, MouseScrollDelta, NamedKey, ScrollMotion};
use layout::{Deco, LineLayout};
use preview::{Context, Kind, Marker};

/// What a key or click did, for the host.
#[derive(Clone, Debug, PartialEq)]
pub enum Response {
    None,
    /// The caret or selection moved; the text did not change.
    Moved,
    /// The text changed.
    Changed,
    /// A link was clicked (rendered) or Ctrl+clicked (raw).
    Follow(Target),
}

const DOUBLE_CLICK: Duration = Duration::from_millis(400);

pub struct DocEditor {
    pub buf: Buffer,
    /// Live preview: markup hidden except on the caret's lines. Off: every
    /// line shows as written (with its syntax still dimmed).
    pub preview: bool,
    pub theme: EditorTheme,
    /// The widest the text column grows (Obsidian's readable line length);
    /// 0 lets it fill the rect.
    pub max_width: f32,
    pub pad: f32,
    measure: ShapingMeasure,
    ctx: Vec<Context>,
    layouts: Vec<Option<LineLayout>>,
    heights: Vec<f32>,
    tops: Vec<f32>,
    tops_dirty: bool,
    /// Lines laid out as active (raw), so a caret move relays out the lines
    /// it leaves and the ones it enters.
    shown_active: (usize, usize),
    width: f32,
    pub scroll: f32,
    motion: ScrollMotion,
    /// The x a vertical move keeps to.
    want_x: Option<f32>,
    dragging: bool,
    clicks: Option<(Instant, Pos, u8)>,
    /// Where the text column was last painted: its origin and the rect.
    origin: (f32, f32),
    viewport: Rect,
    follow_caret: bool,
}

impl DocEditor {
    /// An editor over `text`. `system_fonts` must match the app's
    /// `Application::load_system_fonts`, so measured widths are drawn ones.
    pub fn new(text: &str, theme: EditorTheme, system_fonts: bool) -> DocEditor {
        let mut e = DocEditor {
            buf: Buffer::new(text),
            preview: true,
            theme,
            max_width: 0.0,
            pad: 24.0,
            measure: ShapingMeasure::new(system_fonts),
            ctx: Vec::new(),
            layouts: Vec::new(),
            heights: Vec::new(),
            tops: Vec::new(),
            tops_dirty: true,
            shown_active: (usize::MAX, usize::MAX),
            width: 0.0,
            scroll: 0.0,
            motion: ScrollMotion::new(),
            want_x: None,
            dragging: false,
            clicks: None,
            origin: (0.0, 0.0),
            viewport: Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 },
            follow_caret: false,
        };
        e.sync();
        e
    }

    pub fn set_text(&mut self, text: &str) {
        self.buf.set_text(text);
        self.scroll = 0.0;
        self.motion = ScrollMotion::new();
        self.want_x = None;
        self.sync();
    }

    pub fn text(&self) -> String {
        self.buf.text()
    }

    /// Drop every layout (fonts or the theme changed).
    pub fn invalidate(&mut self) {
        for l in &mut self.layouts {
            *l = None;
        }
        self.tops_dirty = true;
    }

    pub fn set_preview(&mut self, on: bool) {
        if self.preview != on {
            self.preview = on;
            self.invalidate();
        }
    }

    // ---- incremental bookkeeping ----------------------------------------

    /// The lines shown raw: the caret's (or the selection's), widened to
    /// a fenced code block the caret is in, so its fences show.
    fn active_range(&self) -> (usize, usize) {
        let (mut a, mut b) = match self.buf.selection() {
            Some((a, b)) => (a.line, b.line),
            None => (self.buf.caret.line, self.buf.caret.line),
        };
        if let Some((x, _)) = preview::fenced_block(&self.ctx, a) {
            a = x;
        }
        if let Some((_, y)) = preview::fenced_block(&self.ctx, b) {
            b = y;
        }
        (a, b)
    }

    fn is_active(&self, i: usize, act: (usize, usize)) -> bool {
        !self.preview || (i >= act.0 && i <= act.1)
    }

    fn estimate(&self, i: usize) -> f32 {
        let row = (self.theme.size * self.theme.spacing).ceil();
        let chars = self.buf.line(i).chars().count() as f32;
        let per_row = (self.width / (self.theme.size * 0.5)).max(10.0);
        row * (chars / per_row).ceil().max(1.0)
    }

    /// Bring layouts, contexts and heights in line with the buffer.
    fn sync(&mut self) {
        for c in self.buf.take_changes() {
            if c.removed == usize::MAX {
                self.layouts = vec![None; self.buf.line_count()];
                self.heights = vec![0.0; self.buf.line_count()];
                self.ctx.clear();
                continue;
            }
            let end = (c.first + c.removed).min(self.layouts.len());
            self.layouts.splice(c.first..end, std::iter::repeat_with(|| None).take(c.inserted));
            self.heights.splice(c.first..end, std::iter::repeat_n(0.0, c.inserted));
            self.tops_dirty = true;
        }
        let n = self.buf.line_count();
        self.layouts.resize_with(n, || None);
        self.heights.resize(n, 0.0);
        let ctx = preview::contexts(self.buf.lines());
        if ctx.len() == self.ctx.len() {
            for i in 0..n {
                if ctx[i] != self.ctx[i] {
                    self.layouts[i] = None;
                }
            }
        } else {
            for l in &mut self.layouts {
                *l = None;
            }
        }
        self.ctx = ctx;
        let act = self.active_range();
        if act != self.shown_active {
            let (a, b) = self.shown_active;
            for i in [a, b, act.0, act.1] {
                if i < n {
                    self.layouts[i] = None;
                }
            }
            // A selection's middle lines too.
            for i in act.0.min(n)..=act.1.min(n.saturating_sub(1)) {
                self.layouts[i] = None;
            }
            if a != usize::MAX {
                for i in a.min(n)..=b.min(n.saturating_sub(1)) {
                    self.layouts[i] = None;
                }
            }
            self.shown_active = act;
        }
        for i in 0..n {
            let h = match &self.layouts[i] {
                Some(l) => l.height,
                None if self.heights[i] > 0.0 => self.heights[i],
                None => self.estimate(i),
            };
            if (h - self.heights[i]).abs() > 0.01 {
                self.heights[i] = h;
                self.tops_dirty = true;
            }
        }
        self.retop();
    }

    fn retop(&mut self) {
        if !self.tops_dirty && self.tops.len() == self.heights.len() + 1 {
            return;
        }
        self.tops.clear();
        self.tops.reserve(self.heights.len() + 1);
        let mut y = 0.0;
        for h in &self.heights {
            self.tops.push(y);
            y += h;
        }
        self.tops.push(y);
        self.tops_dirty = false;
    }

    fn ensure(&mut self, i: usize) {
        if i >= self.layouts.len() || self.layouts[i].is_some() {
            return;
        }
        let act = self.shown_active;
        let active = self.is_active(i, act);
        let text = self.buf.line(i);
        let line = preview::style_line(text, self.ctx[i], active);
        let l = layout::layout_line(text, &line, active, self.width, &self.theme, &mut self.measure);
        if (l.height - self.heights[i]).abs() > 0.01 {
            self.heights[i] = l.height;
            self.tops_dirty = true;
        }
        self.layouts[i] = Some(l);
    }

    fn line_at_y(&self, y: f32) -> usize {
        let n = self.heights.len();
        match self.tops[..n].binary_search_by(|t| t.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Less)) {
            Ok(i) => i,
            Err(0) => 0,
            Err(i) => (i - 1).min(n - 1),
        }
    }

    pub fn content_height(&self) -> f32 {
        self.tops.last().copied().unwrap_or(0.0) + 2.0 * self.pad
    }

    fn max_scroll(&self) -> f32 {
        (self.content_height() - self.viewport.height).max(0.0)
    }

    // ---- caret geometry -----------------------------------------------------

    /// The caret's (x, y) in content space and its row height.
    fn caret_point(&mut self, p: Pos) -> (f32, f32, f32) {
        self.ensure(p.line);
        self.retop();
        let l = self.layouts[p.line].as_ref().expect("laid out");
        let (x, row) = l.caret_xy(p.col);
        (x, self.tops[p.line] + row as f32 * l.row_h, l.row_h)
    }

    /// The caret's rect on screen as last painted — for a popup at it.
    pub fn caret_rect(&mut self) -> Rect {
        let (x, y, h) = self.caret_point(self.buf.caret);
        Rect { x: self.origin.0 + x, y: self.origin.1 + y - self.scroll, width: 2.0, height: h }
    }

    /// The position under a screen point.
    fn pos_at(&mut self, sx: f32, sy: f32) -> Pos {
        let (x, y) = (sx - self.origin.0, sy - self.origin.1 + self.scroll);
        if y < 0.0 {
            return Pos::default();
        }
        let i = self.line_at_y(y);
        self.ensure(i);
        self.retop();
        let l = self.layouts[i].as_ref().unwrap();
        let row = (((y - self.tops[i]) / l.row_h).floor().max(0.0) as usize).min(l.rows - 1);
        Pos::new(i, l.col_at(x, row))
    }

    fn scroll_to_caret(&mut self) {
        let (_, y, h) = self.caret_point(self.buf.caret);
        let view = self.viewport.height - 2.0 * self.pad;
        if view <= 0.0 {
            return;
        }
        if y < self.scroll {
            self.scroll = y;
        } else if y + h > self.scroll + view {
            self.scroll = y + h - view;
        }
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());
        self.motion.y.jump_to(self.scroll);
    }

    /// Move the caret one visual row (or `rows`) up or down, keeping `want_x`.
    fn vertical(&mut self, rows: i32, select: bool) {
        let p = self.buf.caret;
        self.ensure(p.line);
        let (cx, row) = self.layouts[p.line].as_ref().unwrap().caret_xy(p.col);
        let x = *self.want_x.get_or_insert(cx);
        let (mut line, mut row) = (p.line as i64, row as i64);
        let mut left = rows.unsigned_abs();
        while left > 0 {
            row += rows.signum() as i64;
            let rows_here = self.layouts[line as usize].as_ref().map_or(1, |l| l.rows) as i64;
            if row < 0 {
                if line == 0 {
                    row = 0;
                    break;
                }
                line -= 1;
                self.ensure(line as usize);
                row = self.layouts[line as usize].as_ref().unwrap().rows as i64 - 1;
            } else if row >= rows_here {
                if line as usize + 1 >= self.buf.line_count() {
                    row = rows_here - 1;
                    break;
                }
                line += 1;
                self.ensure(line as usize);
                row = 0;
            }
            left -= 1;
        }
        let l = self.layouts[line as usize].as_ref().unwrap();
        let col = l.col_at(x, row as usize);
        let keep = self.want_x;
        self.buf.set_caret(Pos::new(line as usize, col), select);
        self.want_x = keep;
    }

    // ---- input --------------------------------------------------------------

    /// A key press. Undo/redo chords are the host's to route (the runner
    /// sends them to `Application::undo`); call [`DocEditor::undo`] there.
    pub fn key(&mut self, ev: &KeyEvent) -> Response {
        self.sync();
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

    fn after_edit(&mut self, rev: u64, caret: (Pos, Option<Pos>)) -> Response {
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

    fn content_start(&self, line: usize) -> usize {
        preview::style_line(self.buf.line(line), self.ctx.get(line).copied().unwrap_or(Context::Normal), true).content_start
    }

    /// Enter continues a list item (`- `, `1. ` counting on, `- [ ] `),
    /// and on an empty item ends the list instead, as Obsidian does.
    fn enter(&mut self) {
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
    fn tab(&mut self, outdent: bool) {
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
    fn wrap(&mut self, mark: &str) {
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

    /// A left press at a screen point. A rendered link follows; a task's
    /// box toggles; Ctrl+click follows a raw link; otherwise the caret
    /// moves (Shift extends; a double press selects a word, a triple the
    /// line).
    pub fn press(&mut self, sx: f32, sy: f32, shift: bool, ctrl: bool) -> Response {
        self.sync();
        let rev = self.buf.revision;
        let before = (self.buf.caret, self.buf.anchor);
        let (x, y) = (sx - self.origin.0, sy - self.origin.1 + self.scroll);
        if y >= 0.0 && !shift {
            let i = self.line_at_y(y);
            self.ensure(i);
            self.retop();
            let active = self.is_active(i, self.shown_active);
            let l = self.layouts[i].as_ref().unwrap();
            let ly = y - self.tops[i];
            if let Some((r, at)) = l.task {
                if r.width > 0.0 && x >= r.x && x <= r.x + r.width && ly >= r.y && ly <= r.y + r.height {
                    let status = self.buf.line(i)[at..].chars().next().unwrap_or(' ');
                    let next = if status == ' ' { "x" } else { " " };
                    let end = at + status.len_utf8();
                    let keep = (self.buf.caret, self.buf.anchor);
                    self.buf.replace(Pos::new(i, at), Pos::new(i, end), next, EditKind::Other);
                    (self.buf.caret, self.buf.anchor) = keep;
                    return self.after_edit(rev, before);
                }
            }
            if !active || ctrl {
                if let Some(k) = l.link_at(x, ly) {
                    return Response::Follow(l.links[k].clone());
                }
            }
        }
        let p = self.pos_at(sx, sy);
        let now = Instant::now();
        let count = match self.clicks {
            Some((t, at, n)) if now.duration_since(t) < DOUBLE_CLICK && at == p => n % 3 + 1,
            _ => 1,
        };
        self.clicks = Some((now, p, count));
        match count {
            2 => {
                let (a, b) = self.buf.word_at(p);
                self.buf.set_caret(a, false);
                self.buf.set_caret(b, true);
            }
            3 => {
                self.buf.set_caret(Pos::new(p.line, 0), false);
                self.buf.set_caret(Pos::new(p.line, self.buf.line(p.line).len()), true);
            }
            _ => self.buf.set_caret(p, shift),
        }
        self.dragging = true;
        self.want_x = None;
        self.after_edit(rev, before)
    }

    /// Pointer motion; extends the selection while a press is held.
    pub fn drag(&mut self, sx: f32, sy: f32) -> Response {
        if !self.dragging {
            return Response::None;
        }
        let before = (self.buf.caret, self.buf.anchor);
        let p = self.pos_at(sx, sy);
        if p != self.buf.caret {
            self.buf.set_caret(p, true);
        }
        let r = self.after_edit(self.buf.revision, before);
        // Selecting by drag does not drag the view to the caret: only an
        // edge does, and the wheel.
        self.follow_caret = sy < self.viewport.y || sy > self.viewport.y + self.viewport.height;
        r
    }

    pub fn release(&mut self) {
        self.dragging = false;
    }

    /// Whether a press is being dragged (the host keeps routing motion).
    pub fn dragging(&self) -> bool {
        self.dragging
    }

    /// A link under a screen point, for a hover cursor.
    pub fn link_at(&mut self, sx: f32, sy: f32) -> bool {
        let (x, y) = (sx - self.origin.0, sy - self.origin.1 + self.scroll);
        if y < 0.0 || self.layouts.is_empty() {
            return false;
        }
        let i = self.line_at_y(y);
        if self.is_active(i, self.shown_active) {
            return false;
        }
        self.ensure(i);
        self.retop();
        let l = self.layouts[i].as_ref().unwrap();
        l.link_at(x, y - self.tops[i]).is_some() || l.task.is_some_and(|(r, _)| r.width > 0.0 && x >= r.x && x <= r.x + r.width && y - self.tops[i] <= r.y + r.height)
    }

    pub fn wheel(&mut self, delta: &MouseScrollDelta) -> bool {
        let line = (self.theme.size * self.theme.spacing).max(1.0);
        self.motion.reconcile(0.0, self.scroll);
        let moved = self.motion.apply(delta, (line, line * 2.0), Bounds::max(0.0), Bounds::max(self.max_scroll()));
        self.scroll = self.motion.y.pos();
        moved
    }

    /// Advance a wheel glide; true while it moves.
    pub fn tick(&mut self, dt: f32) -> bool {
        if !self.motion.is_animating() {
            return false;
        }
        self.motion.tick(dt, Bounds::max(0.0), Bounds::max(self.max_scroll()));
        self.scroll = self.motion.y.pos();
        true
    }

    /// Bring a line to the top of the view (search hits, outline clicks).
    pub fn reveal_line(&mut self, line: usize) {
        self.sync();
        let line = line.min(self.buf.line_count() - 1);
        self.retop();
        self.scroll = self.tops[line].clamp(0.0, self.max_scroll());
        self.motion.y.jump_to(self.scroll);
        self.buf.set_caret(Pos::new(line, 0), false);
        self.sync();
    }

    // ---- painting -------------------------------------------------------------

    /// Paint into `rect`: lay out what shows, then selection, text, caret.
    /// `focused` draws the caret.
    pub fn paint(&mut self, pc: &mut PaintCtx, rect: Rect, focused: bool) {
        self.prepare(rect);
        self.paint_prepared(pc, focused);
    }

    /// Lay out what shows in `rect` and settle the scroll, without
    /// painting: after it, [`DocEditor::caret_rect`] is where the caret
    /// will be drawn (a popup placed before the paint needs that).
    pub fn prepare(&mut self, rect: Rect) {
        let mut width = (rect.width - 2.0 * self.pad).max(40.0);
        if self.max_width > 0.0 {
            width = width.min(self.max_width);
        }
        if (width - self.width).abs() > 0.5 {
            self.width = width;
            self.invalidate();
            for i in 0..self.heights.len() {
                self.heights[i] = 0.0;
            }
        }
        self.viewport = rect;
        self.origin = (rect.x + (rect.width - width) / 2.0, rect.y + self.pad);
        self.sync();
        if std::mem::take(&mut self.follow_caret) {
            self.scroll_to_caret();
        }
        // Lay out what shows (heights settle), then paint it.
        let view_h = rect.height;
        for _ in 0..2 {
            let mut i = self.line_at_y(self.scroll - self.pad);
            while i < self.buf.line_count() && self.tops[i] < self.scroll + view_h {
                self.ensure(i);
                i += 1;
            }
            self.retop();
        }
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());
    }

    /// Paint what the last [`DocEditor::prepare`] laid out.
    pub fn paint_prepared(&mut self, pc: &mut PaintCtx, focused: bool) {
        self.paint_prepared_with(pc, focused, &|_| true);
    }

    /// [`DocEditor::paint_prepared`], with `resolved` saying whether a
    /// link has a target: one that does not draws faded, as in Obsidian.
    /// Asked at paint, so a vault change needs no relayout.
    pub fn paint_prepared_with(&mut self, pc: &mut PaintCtx, focused: bool, resolved: &dyn Fn(&Target) -> bool) {
        let rect = self.viewport;
        let view_h = rect.height;
        let (ox, oy) = (self.origin.0, self.origin.1 - self.scroll);
        let sel = self.buf.selection();
        let caret = self.buf.caret;
        let th = self.theme.clone();
        let first = self.line_at_y(self.scroll - self.pad);
        pc.clip(rect, |pc| {
            let mut i = first;
            while i < self.buf.line_count() && self.tops[i] < self.scroll + view_h {
                let Some(l) = self.layouts[i].as_ref() else {
                    i += 1;
                    continue;
                };
                let top = oy + self.tops[i];
                // Selection, behind everything on the line.
                if let Some((a, b)) = sel {
                    if i >= a.line && i <= b.line {
                        let from = if i == a.line { l.caret_xy(a.col) } else { (l.runs.first().map_or(l.content_x, |r| r.x), 0) };
                        let to = if i == b.line { l.caret_xy(b.col) } else { (l.runs.iter().map(|r| r.x + r.w).fold(l.content_x, f32::max) + 6.0, l.rows - 1) };
                        for row in from.1..=to.1 {
                            let x0 = if row == from.1 { from.0 } else { l.runs.iter().filter(|r| r.row == row).map(|r| r.x).fold(f32::INFINITY, f32::min).min(l.content_x) };
                            let x1 = if row == to.1 { to.0 } else { l.runs.iter().filter(|r| r.row == row).map(|r| r.x + r.w).fold(x0, f32::max) };
                            if x1 > x0 {
                                pc.quad(Rect { x: ox + x0, y: top + row as f32 * l.row_h, width: x1 - x0, height: l.row_h }, th.selection);
                            }
                        }
                    }
                }
                for d in &l.decos {
                    match d {
                        Deco::Quad(r, c) => pc.quad(Rect { x: ox + r.x, y: top + r.y, width: r.width, height: r.height }, *c),
                        Deco::Dot { cx, cy, r, color } => pc.circle(ox + cx, top + cy, *r, *color),
                        Deco::Check { cx, cy, r, checked } => crate::widget::Checkbox::paint_round_mark(pc, ox + cx, top + cy, *r, *checked),
                        Deco::Text { text, x, y, size, color, font } => {
                            pc.text_with(text.clone(), ox + x, top + y, *size, srgb_u8(*color), Some(font.clone()), None)
                        }
                    }
                }
                for r in &l.runs {
                    let row_y = top + r.row as f32 * l.row_h;
                    if let Some(bg) = r.bg {
                        pc.rounded_rect(Rect { x: ox + r.x - 2.0, y: row_y + 2.0, width: r.w + 4.0, height: l.row_h - 4.0 }, 3.0, (true, true, true, true), bg);
                    }
                    let ty = row_y + (l.row_h - r.size) / 2.0;
                    let color = match r.link {
                        Some(k) if !resolved(&l.links[k]) => th.link_unresolved,
                        _ => r.color,
                    };
                    pc.text_attrs(r.text.clone(), ox + r.x, ty, r.size, srgb_u8(color), Some(r.font.clone()), None, r.attrs);
                    if r.strike {
                        let sy = ty + r.size * 0.55;
                        pc.vector(ox + r.x, sy, ox + r.x + r.w, sy, 1.0, r.color, Cap::Flat);
                    }
                }
                if focused && i == caret.line {
                    let (x, row) = l.caret_xy(caret.col);
                    let h = l.row_h * 0.8;
                    let y = top + row as f32 * l.row_h + (l.row_h - h) / 2.0;
                    pc.quad(Rect { x: ox + x - 0.5, y, width: 2.0, height: h }, th.caret);
                }
                i += 1;
            }
        });
    }
}

fn srgb_u8(linear: [f32; 4]) -> [u8; 3] {
    let s = crate::colors::to_srgb(linear);
    [(s[0] * 255.0) as u8, (s[1] * 255.0) as u8, (s[2] * 255.0) as u8]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::ElementState;

    fn key(k: Key, text: Option<&str>, ctrl: bool, shift: bool) -> KeyEvent {
        KeyEvent { state: ElementState::Pressed, logical_key: k, text: text.map(String::from), repeat: false, ctrl, shift, alt: false }
    }

    fn typed(c: &str) -> KeyEvent {
        key(Key::Character(c.into()), Some(c), false, false)
    }

    fn named(n: NamedKey) -> KeyEvent {
        key(Key::Named(n), None, false, false)
    }

    fn editor(text: &str) -> DocEditor {
        let mut e = DocEditor::new(text, EditorTheme::new(14.0), false);
        let mut pc = PaintCtx::new();
        e.paint(&mut pc, Rect { x: 0.0, y: 0.0, width: 600.0, height: 400.0 }, true);
        e
    }

    #[test]
    fn typing_enter_and_list_continuation() {
        let mut e = editor("");
        for c in ["-", " ", "[", " ", "]", " ", "a"] {
            e.key(&typed(c));
        }
        assert_eq!(e.key(&named(NamedKey::Enter)), Response::Changed);
        e.key(&typed("b"));
        assert_eq!(e.text(), "- [ ] a\n- [ ] b");
        e.key(&named(NamedKey::Enter));
        // Enter on an empty item ends the list.
        e.key(&named(NamedKey::Enter));
        assert_eq!(e.text(), "- [ ] a\n- [ ] b\n");
        let mut e = editor("3. three");
        e.buf.caret = Pos::new(0, 8);
        e.key(&named(NamedKey::Enter));
        assert_eq!(e.text(), "3. three\n4. ");
    }

    #[test]
    fn tab_indents_list_items_and_undo_works() {
        let mut e = editor("- a\n- b");
        e.buf.caret = Pos::new(1, 3);
        e.key(&named(NamedKey::Tab));
        assert_eq!(e.text(), "- a\n\t- b");
        assert_eq!(e.buf.caret, Pos::new(1, 4));
        e.key(&key(Key::Named(NamedKey::Tab), None, false, true));
        assert_eq!(e.text(), "- a\n- b");
        assert!(e.undo());
        assert_eq!(e.text(), "- a\n\t- b");
    }

    #[test]
    fn arrows_words_home_end_and_vertical() {
        let mut e = editor("- [ ] first item\nsecond");
        e.buf.caret = Pos::new(0, 10);
        e.key(&named(NamedKey::Home));
        assert_eq!(e.buf.caret, Pos::new(0, 6), "smart home: the content start");
        e.key(&named(NamedKey::Home));
        assert_eq!(e.buf.caret, Pos::new(0, 0));
        e.key(&named(NamedKey::End));
        assert_eq!(e.buf.caret, Pos::new(0, 16));
        e.key(&named(NamedKey::ArrowDown));
        assert_eq!(e.buf.caret.line, 1);
        e.key(&key(Key::Named(NamedKey::ArrowLeft), None, true, true));
        assert_eq!(e.buf.selected_text().as_deref(), Some("second"));
    }

    #[test]
    fn clicks_place_the_caret_and_toggle_tasks() {
        let mut e = editor("- [ ] todo\nplain line");
        // Line 0 is not active (the caret is on it? it starts at 0,0 — move it).
        e.buf.caret = Pos::new(1, 0);
        let mut pc = PaintCtx::new();
        e.paint(&mut pc, Rect { x: 0.0, y: 0.0, width: 600.0, height: 400.0 }, true);
        let l = e.layouts[0].as_ref().unwrap();
        let (r, _) = l.task.unwrap();
        let (sx, sy) = (e.origin.0 + r.x + r.width / 2.0, e.origin.1 + r.y + r.height / 2.0);
        assert_eq!(e.press(sx, sy, false, false), Response::Changed);
        assert_eq!(e.text(), "- [x] todo\nplain line");
        assert_eq!(e.buf.caret, Pos::new(1, 0), "ticking does not move the caret");
        // A click at the far left of the second line puts the caret there.
        let y1 = e.origin.1 + e.tops[1] + 3.0;
        e.release();
        e.press(e.origin.0 + 1.0, y1, false, false);
        assert_eq!(e.buf.caret, Pos::new(1, 0));
    }

    #[test]
    fn rendered_links_follow() {
        let mut e = editor("see [[Target|it]] now\nsecond");
        e.buf.caret = Pos::new(1, 0);
        let mut pc = PaintCtx::new();
        e.paint(&mut pc, Rect { x: 0.0, y: 0.0, width: 600.0, height: 400.0 }, true);
        let l = e.layouts[0].as_ref().unwrap();
        let run = l.runs.iter().find(|r| r.link.is_some()).unwrap();
        let (sx, sy) = (e.origin.0 + run.x + 2.0, e.origin.1 + 4.0);
        assert_eq!(e.press(sx, sy, false, false), Response::Follow(Target::Note { target: "Target".into(), subpath: None }));
    }

    #[test]
    fn a_long_document_shapes_only_what_shows() {
        let text: String = (0..5000).map(|i| format!("line {i} with **some** text\n")).collect();
        let e = editor(&text);
        let laid = e.layouts.iter().filter(|l| l.is_some()).count();
        assert!(laid < 60, "{laid} lines shaped");
        assert!(e.content_height() > 5000.0 * 14.0);
    }
}
