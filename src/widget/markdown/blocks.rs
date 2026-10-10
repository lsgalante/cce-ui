//! Laying out blocks: each kind's geometry — headings, paragraphs, the properties table, lists and
//! tasks, quotes and callouts, code blocks, tables — and the source line a block starts on.

use super::*;

impl<'a> Layouter<'a> {
    pub(super) fn blocks(&mut self, blocks: &[Block], x: f32, w: f32, mut y: f32, record: bool) -> f32 {
        for (i, b) in blocks.iter().enumerate() {
            if i > 0 {
                y += BLOCK_GAP;
            }
            if record {
                if let Some(line) = block_line(b) {
                    self.out.lines.push((line, y));
                }
            }
            y = self.block(b, x, w, y);
        }
        y
    }

    pub(super) fn block(&mut self, b: &Block, x: f32, w: f32, y: f32) -> f32 {
        let size = self.theme.size;
        match b {
            Block::Properties(props) => self.properties(props, x, w, y),
            Block::Heading { level, spans, .. } => {
                let scale = match level {
                    1 => 1.6,
                    2 => 1.4,
                    3 => 1.25,
                    4 => 1.1,
                    _ => 1.0,
                };
                let y = y + if *level <= 2 { size * 0.4 } else { 0.0 };
                let style = RunStyle { size: (size * scale).round(), bold: true, color: None };
                let end = self.inline(spans, x, w, y, style);
                if *level <= 2 {
                    let ry = end + 4.0;
                    self.out.draws.push(Draw::Quad { rect: Rect { x, y: ry, width: w, height: 1.0 }, color: RULE });
                    return ry + 1.0;
                }
                end
            }
            Block::Paragraph { spans, .. } => self.inline(spans, x, w, y, self.plain_style()),
            Block::Embed { target, size: want, .. } if (self.image)(target).is_some() => {
                let img = (self.image)(target).expect("checked by the guard");
                let (iw, ih) = img.fit(want.map(|s| s.width), want.and_then(|s| s.height), w);
                self.out.draws.push(Draw::Image { target: target.clone(), rect: Rect { x, y, width: iw, height: ih } });
                y + ih
            }
            Block::Embed { target, subpath, .. } => {
                // An embed that cannot be shown inline (a note, a missing
                // image) is a link to it, led by the `link` glyph in the
                // link colour (it was "↳ " until 2026-10-05).
                let shown = match subpath {
                    Some(s) => format!("{target}#{s}"),
                    None => target.clone(),
                };
                let size = self.theme.size;
                let side = (size * 0.85).round();
                let gap = (size * 0.4).round();
                self.out.draws.push(Draw::Icon {
                    name: "link",
                    rect: Rect { x, y: y + 0.5 * (size * 1.3 - side), width: side, height: side },
                    color: crate::color::text_link_color(),
                });
                let span = Span {
                    text: shown,
                    style: Style::default(),
                    link: Some(SpanLink::Note { target: target.clone(), subpath: subpath.clone() }),
                };
                self.inline(std::slice::from_ref(&span), x + side + gap, (w - side - gap).max(0.0), y, self.plain_style())
            }
            Block::List { start, items, .. } => self.list(*start, items, x, w, y),
            Block::Quote { callout, blocks, .. } => self.quote(callout.as_ref(), blocks, x, w, y),
            Block::Code { lang, text, .. } => self.code(lang.as_deref(), text, x, w, y),
            Block::Table { header, rows, .. } => self.table(header, rows, x, w, y),
            Block::Rule { .. } => {
                let ry = y + size * 0.5;
                self.out.draws.push(Draw::Quad { rect: Rect { x, y: ry, width: w, height: 1.0 }, color: RULE });
                ry + size * 0.5
            }
        }
    }

    pub(super) fn plain_style(&self) -> RunStyle {
        RunStyle { size: self.theme.size, bold: false, color: None }
    }

    pub(super) fn properties(&mut self, props: &[(String, String)], x: f32, w: f32, mut y: f32) -> f32 {
        let size = (self.theme.size * 0.9).round();
        let key_w = 140.0f32.min(w * 0.35);
        for (k, v) in props {
            let lh = self.theme.line_h(size);
            self.text(k, x, y + (lh - size) / 2.0, size, DIM, false, false);
            let span = Span { text: v.clone(), style: Style::default(), link: None };
            let style = RunStyle { size, bold: false, color: Some(FG) };
            y = self.inline(std::slice::from_ref(&span), x + key_w, w - key_w, y, style);
        }
        y += 6.0;
        self.out.draws.push(Draw::Quad { rect: Rect { x, y, width: w, height: 1.0 }, color: RULE });
        y + 1.0
    }

    pub(super) fn list(&mut self, start: Option<u64>, items: &[ListItem], x: f32, w: f32, mut y: f32) -> f32 {
        let size = self.theme.size;
        let lh = self.theme.line_h(size);
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                y += 2.0;
            }
            // Items are where a search hit or a task usually points.
            if self.out.lines.last().is_none_or(|(l, _)| *l < item.line) {
                self.out.lines.push((item.line, y));
            }
            let body_x = x + INDENT;
            let mid = y + lh / 2.0;
            match (item.task, start) {
                (Some(status), _) => {
                    let r = (size * 0.45).round();
                    let cx = x + INDENT - r - 6.0;
                    self.out.draws.push(Draw::Check { cx, cy: mid, r, checked: status != ' ' });
                    self.out.hits.push((
                        Rect { x: cx - r - 3.0, y: mid - r - 3.0, width: 2.0 * r + 6.0, height: 2.0 * r + 6.0 },
                        Hit::Task { line: item.line, status },
                    ));
                }
                (None, Some(n)) => {
                    let label = format!("{}.", n + i as u64);
                    let lw = self.m.width(&label, size, &self.theme.body_font, TextAttrs::default());
                    self.text(&label, body_x - 6.0 - lw, y + (lh - size) / 2.0, size, DIM, false, false);
                }
                (None, None) => {
                    let r = (size * 0.16).max(2.0);
                    self.out.draws.push(Draw::Dot { cx: body_x - 12.0, cy: mid, r, color: DIM });
                }
            }
            let done = item.task.is_some_and(|s| s == 'x' || s == 'X');
            let before = self.out.draws.len();
            y = self.item_blocks(&item.blocks, body_x, w - INDENT, y);
            if done {
                // Obsidian dims a finished task's text rather than striking it.
                for d in &mut self.out.draws[before..] {
                    if let Draw::Text { color, .. } = d {
                        *color = DIM;
                    }
                }
            }
        }
        y
    }

    /// An item's blocks, without the block gap before a nested list.
    pub(super) fn item_blocks(&mut self, blocks: &[Block], x: f32, w: f32, mut y: f32) -> f32 {
        let lh = self.theme.line_h(self.theme.size);
        if blocks.is_empty() {
            return y + lh;
        }
        for (i, b) in blocks.iter().enumerate() {
            if i > 0 && !matches!(b, Block::List { .. }) {
                y += BLOCK_GAP / 2.0;
            }
            y = self.block(b, x, w, y);
        }
        y
    }

    pub(super) fn quote(&mut self, callout: Option<&Callout>, blocks: &[Block], x: f32, w: f32, y: f32) -> f32 {
        let size = self.theme.size;
        let start = self.out.draws.len();
        let pad = if callout.is_some() { 10.0 } else { 0.0 };
        let mut cy = y + pad;
        let body_x = x + if callout.is_some() { 14.0 } else { INDENT * 0.6 };
        let body_w = w - (body_x - x) - pad;
        if let Some(c) = callout {
            let style = RunStyle { size, bold: true, color: Some(lin(callout_color(&c.kind))) };
            let title: Vec<Span> = if c.title.is_empty() {
                vec![Span { text: title_case(&c.kind), style: Style::default(), link: None }]
            } else {
                c.title.clone()
            };
            cy = self.inline(&title, body_x, body_w, cy, style);
            if !blocks.is_empty() && c.folded != Some(true) {
                cy += BLOCK_GAP / 2.0;
            }
        }
        if callout.is_none_or(|c| c.folded != Some(true)) {
            cy = self.blocks(blocks, body_x, body_w, cy, false);
        }
        let end = cy + pad;
        let rect = Rect { x, y, width: w, height: end - y };
        // Backgrounds go under the text laid out above them.
        match callout {
            Some(c) => {
                let mut bg = lin(callout_color(&c.kind));
                bg[3] = 0.10;
                self.out.draws.insert(start, Draw::Round { rect, radius: 6.0, color: bg });
            }
            None => {
                self.out.draws.insert(
                    start,
                    Draw::Quad { rect: Rect { x, y, width: 2.0, height: end - y }, color: crate::color::text_quote_bar_color() },
                );
            }
        }
        end
    }

    pub(super) fn code(&mut self, _lang: Option<&str>, text: &str, x: f32, w: f32, y: f32) -> f32 {
        let size = (self.theme.size * 0.9).round();
        let lh = self.theme.line_h(size);
        let n = text.split('\n').count().max(1);
        let h = n as f32 * lh + 2.0 * CODE_PAD;
        self.out.draws.push(Draw::Round { rect: Rect { x, y, width: w, height: h }, radius: 6.0, color: crate::color::text_code_background_color() });
        // Code does not wrap; overlong lines are cut at the block's edge
        // by the paint clip, as a scrolled source view would show them.
        for (i, line) in text.split('\n').enumerate() {
            if line.is_empty() {
                continue;
            }
            let ty = y + CODE_PAD + i as f32 * lh + (lh - size) / 2.0;
            let font = self.theme.mono_font.clone();
            self.out.draws.push(Draw::Text {
                text: line.replace('\t', "    "),
                x: x + CODE_PAD,
                y: ty,
                size,
                color: FG,
                font,
                attrs: TextAttrs::default(),
            });
        }
        y + h
    }

    pub(super) fn table(&mut self, header: &[Vec<Span>], rows: &[Vec<Vec<Span>>], x: f32, w: f32, mut y: f32) -> f32 {
        let cols = header.len().max(rows.iter().map(Vec::len).max().unwrap_or(0)).max(1);
        let col_w = w / cols as f32;
        let pad = 6.0;
        let top = y;
        let all = std::iter::once((header, true)).chain(rows.iter().map(|r| (r.as_slice(), false)));
        for (ri, (row, is_header)) in all.enumerate() {
            let style = RunStyle { size: self.theme.size, bold: is_header, color: None };
            let mut bottom = y;
            for (ci, cell) in row.iter().enumerate().take(cols) {
                let cx = x + ci as f32 * col_w + pad;
                bottom = bottom.max(self.inline(cell, cx, col_w - 2.0 * pad, y + pad, style));
            }
            y = bottom.max(y + self.theme.line_h(self.theme.size)) + pad;
            if ri == 0 || ri < rows.len() {
                self.out.draws.push(Draw::Quad { rect: Rect { x, y, width: w, height: 1.0 }, color: RULE });
            }
        }
        for c in 1..cols {
            let lx = x + c as f32 * col_w;
            self.out.draws.push(Draw::Quad { rect: Rect { x: lx, y: top, width: 1.0, height: y - top }, color: RULE });
        }
        y
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn text(&mut self, text: &str, x: f32, y: f32, size: f32, color: [f32; 4], bold: bool, italic: bool) {
        let attrs = TextAttrs { italic, weight: bold.then_some(700), ..Default::default() };
        let font = self.theme.body_font.clone();
        self.out.draws.push(Draw::Text { text: text.to_string(), x, y, size, color, font, attrs });
    }
}

pub(super) fn block_line(b: &Block) -> Option<usize> {
    match b {
        Block::Properties(_) => Some(0),
        Block::Heading { line, .. }
        | Block::Paragraph { line, .. }
        | Block::Embed { line, .. }
        | Block::List { line, .. }
        | Block::Quote { line, .. }
        | Block::Code { line, .. }
        | Block::Table { line, .. }
        | Block::Rule { line } => Some(*line),
    }
}

pub(super) fn callout_color(kind: &str) -> [f32; 4] {
    match kind {
        "warning" | "caution" | "attention" => [0.93, 0.60, 0.25, 1.0],
        "danger" | "error" | "bug" | "failure" | "fail" | "missing" => [0.93, 0.35, 0.35, 1.0],
        "tip" | "hint" | "important" | "success" | "check" | "done" => [0.30, 0.78, 0.60, 1.0],
        "question" | "help" | "faq" => [0.93, 0.75, 0.25, 1.0],
        "quote" | "cite" => [0.60, 0.60, 0.65, 1.0],
        "example" => [0.62, 0.55, 0.98, 1.0],
        _ => [0.35, 0.62, 0.95, 1.0],
    }
}

pub(super) fn title_case(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}
