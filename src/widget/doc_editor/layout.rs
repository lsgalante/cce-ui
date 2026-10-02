//! One line of the document, styled ([`preview::Line`]), laid out at a
//! width: wrapped rows of runs, decorations, and the x of every byte the
//! runs show — what the caret is drawn at and what a click maps back to.
//!
//! Wrapping is decided on measured word widths (a word running across
//! styles wraps whole); then each run — consecutive bytes of one look on
//! one row — is shaped once and its char boundaries recorded, and runs are
//! placed at their shaped widths, so drawn text, caret and clicks agree.
//! Spaces belong to the run before them, so a caret between words maps
//! inside a run. A tab is shaped as one space.

use std::ops::Range;

use super::preview::{self, Kind, Look, Marker, PropShow, Target};
use crate::scene::layout::Rect;
use crate::scene::paint::TextAttrs;
use crate::widget::shaping::{Measure, ShapingMeasure};

/// Colours and fonts; colours are linear (what prims take).
#[derive(Clone, Debug, PartialEq)]
pub struct EditorTheme {
    pub body_font: String,
    pub mono_font: String,
    pub size: f32,
    /// Line height as a multiple of the font size.
    pub spacing: f32,
    pub fg: [f32; 4],
    pub dim: [f32; 4],
    pub link: [f32; 4],
    /// A note link with no target (the host says which, at paint).
    pub link_unresolved: [f32; 4],
    pub code_bg: [f32; 4],
    pub highlight_bg: [f32; 4],
    pub tag_bg: [f32; 4],
    /// A list property's item.
    pub pill_bg: [f32; 4],
    pub accent: [f32; 4],
    pub rule: [f32; 4],
    pub caret: [f32; 4],
    pub selection: [f32; 4],
}

impl EditorTheme {
    /// Obsidian-ish dark defaults at `size`, in the DE's sans and mono.
    pub fn new(size: f32) -> EditorTheme {
        let lin = crate::colors::to_linear;
        EditorTheme {
            body_font: "sans-serif".into(),
            mono_font: "monospace".into(),
            size,
            spacing: 1.55,
            fg: crate::colors::TEXT_FG,
            dim: crate::colors::TEXT_DIM,
            link: lin([0.66, 0.55, 0.98, 1.0]),
            link_unresolved: lin([0.50, 0.44, 0.70, 1.0]),
            code_bg: [1.0, 1.0, 1.0, 0.06],
            highlight_bg: lin([1.0, 0.82, 0.0, 0.40]),
            tag_bg: lin([0.66, 0.55, 0.98, 0.15]),
            pill_bg: [1.0, 1.0, 1.0, 0.09],
            accent: lin([0.66, 0.55, 0.98, 1.0]),
            rule: [1.0, 1.0, 1.0, 0.14],
            caret: lin([0.85, 0.85, 0.92, 1.0]),
            selection: lin([0.40, 0.45, 0.75, 0.45]),
        }
    }
}

/// Indent per list level, and the gutter a list marker sits in.
fn indent(th: &EditorTheme) -> f32 {
    (th.size * 1.6).round()
}
fn gutter(th: &EditorTheme) -> f32 {
    (th.size * 1.6).round()
}
const QUOTE_STEP: f32 = 18.0;
/// Space either side of a pill's text, inside its background.
pub const PILL_PAD: f32 = 6.0;
/// Space between two pills.
const PILL_GAP: f32 = 4.0;

/// The Properties table's key column, as the reading view sizes it.
pub fn prop_key_w(width: f32) -> f32 {
    140f32.min(width * 0.35).round()
}

#[derive(Clone, Debug)]
pub struct Run {
    pub row: usize,
    pub x: f32,
    pub w: f32,
    /// Source bytes of the line this run shows.
    pub src: Range<usize>,
    pub text: String,
    pub size: f32,
    pub font: String,
    pub attrs: TextAttrs,
    pub color: [f32; 4],
    pub bg: Option<[f32; 4]>,
    pub strike: bool,
    pub link: Option<usize>,
    pub look: Look,
    /// (line byte, x relative to `x`) at every char boundary of `src`.
    pub xs: Vec<(usize, f32)>,
}

#[derive(Clone, Debug)]
pub enum Deco {
    /// A filled rect (code backgrounds, rules, quote bars).
    Quad(Rect, [f32; 4]),
    Dot { cx: f32, cy: f32, r: f32, color: [f32; 4] },
    Check { cx: f32, cy: f32, r: f32, checked: bool },
    /// A list number, drawn in the gutter.
    Text { text: String, x: f32, y: f32, size: f32, color: [f32; 4], font: String },
    /// An embedded image, by its link text; the host's id is asked for at
    /// paint.
    Image { target: String, rect: Rect },
}

#[derive(Clone, Debug)]
pub struct LineLayout {
    pub height: f32,
    pub row_h: f32,
    pub rows: usize,
    pub runs: Vec<Run>,
    pub decos: Vec<Deco>,
    pub links: Vec<Target>,
    /// A task's checkbox hit rect, and the byte of its status char.
    pub task: Option<(Rect, usize)>,
    /// A boolean property's checkbox hit rect, its `true`/`false` bytes,
    /// and whether it is ticked.
    pub toggle: Option<(Rect, Range<usize>, bool)>,
    /// Where an empty line's (or a hidden prefix's) caret sits.
    pub content_x: f32,
    pub content_start: usize,
    pub text_size: f32,
}

fn size_for(kind: &Kind, base: f32) -> f32 {
    match kind {
        Kind::Heading(1) => (base * 1.6).round(),
        Kind::Heading(2) => (base * 1.4).round(),
        Kind::Heading(3) => (base * 1.25).round(),
        Kind::Heading(4) => (base * 1.1).round(),
        Kind::Code | Kind::Fence | Kind::Frontmatter | Kind::Table | Kind::Prop(_) => (base * 0.92).round(),
        _ => base,
    }
}

struct Piece {
    src: Range<usize>,
    seg: usize,
    row: usize,
}

/// Lay out `text` (one line) styled as `line`, wrapping at `width`.
/// `active` is whether the caret is on it (its markers show).
pub fn layout_line(text: &str, line: &preview::Line, active: bool, width: f32, th: &EditorTheme, m: &mut ShapingMeasure) -> LineLayout {
    let base = size_for(&line.kind, th.size);
    let row_h = (base * th.spacing).ceil();
    let heading = matches!(line.kind, Kind::Heading(_));
    let look_font = |look: &Look| -> (String, f32, TextAttrs) {
        let mono = look.mono || look.code;
        let size = if look.code && !look.mono { (base * 0.92).round() } else { base };
        let font = if mono { th.mono_font.clone() } else { th.body_font.clone() };
        (font, size, TextAttrs { italic: look.italic, weight: (look.bold || heading).then_some(700) })
    };

    // Where content starts, and the decorations of a hidden prefix.
    let mut decos: Vec<Deco> = Vec::new();
    let mut task = None;
    let mut content_x = 0.0f32;
    match &line.kind {
        Kind::List { level, marker } => {
            let x0 = *level as f32 * indent(th);
            // A task's gutter holds its raw `- [ ] ` too, so the text stays
            // put when the caret arrives and the marker shows.
            let g = match marker {
                Marker::Task(..) => gutter(th).max(m.width("- [ ] ", base, &th.body_font, TextAttrs::default()).ceil()),
                _ => gutter(th),
            };
            content_x = x0 + g;
            if !active {
                let cy = row_h / 2.0;
                let cx = x0 + g / 2.0;
                match marker {
                    Marker::Bullet => decos.push(Deco::Dot { cx, cy, r: (th.size * 0.17).max(2.0), color: th.dim }),
                    Marker::Number(n) => {
                        let w = m.width(n, base, &th.body_font, TextAttrs::default());
                        decos.push(Deco::Text {
                            text: n.clone(),
                            x: content_x - w - th.size * 0.35,
                            y: (row_h - base) / 2.0,
                            size: base,
                            color: th.dim,
                            font: th.body_font.clone(),
                        })
                    }
                    Marker::Task(c, at) => {
                        let r = (th.size * 0.42).round();
                        decos.push(Deco::Check { cx, cy, r, checked: *c != ' ' });
                        task = Some((Rect { x: cx - r - 3.0, y: cy - r - 3.0, width: 2.0 * r + 6.0, height: 2.0 * r + 6.0 }, *at));
                    }
                }
            } else if let Marker::Task(_, at) = marker {
                // The raw `- [ ] ` shows, but the box stays clickable.
                task = Some((Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 }, *at));
            }
        }
        Kind::Quote(d) => content_x = *d as f32 * QUOTE_STEP,
        Kind::Prop(PropShow::Row { .. }) => content_x = prop_key_w(width),
        _ => {}
    }

    // Pass 1: break into rows on measured widths.
    let segs = &line.segs;
    let looks: Vec<(String, f32, TextAttrs)> = segs.iter().map(|s| look_font(&s.look)).collect();
    let mut pieces: Vec<Piece> = Vec::new();
    // On the active line the hidden prefix shows; its markers end where
    // the content begins, so text does not jump when the caret arrives.
    let prefix_w: f32 = if active && line.content_start > 0 {
        segs.iter()
            .enumerate()
            .filter(|(_, s)| s.range.end <= line.content_start && s.look.marker)
            .map(|(i, s)| m.width(&text[s.range.clone()].replace('\t', " "), looks[i].1, &looks[i].0, looks[i].2))
            .sum()
    } else {
        0.0
    };
    let start_x = (content_x - prefix_w).max(0.0);
    let right = width.max(content_x + 40.0);
    let mut cx = start_x;
    let mut row = 0usize;
    let mut row_start = true;
    for (si, seg) in segs.iter().enumerate() {
        let (font, size, attrs) = &looks[si];
        let t = &text[seg.range.clone()];
        let mut i = 0;
        let bytes = t.as_bytes();
        while i < bytes.len() {
            let ws = bytes[i] == b' ' || bytes[i] == b'\t';
            let mut j = i;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') == ws {
                j += 1;
            }
            while j < t.len() && !t.is_char_boundary(j) {
                j += 1;
            }
            let src = seg.range.start + i..seg.range.start + j;
            if ws {
                // Spaces join the piece before them (or lead a row).
                if row_start && cx <= content_x && !active {
                    // Leading whitespace of a row shows as nothing.
                    pieces.push(Piece { src, seg: si, row });
                } else {
                    let w = m.width(&t[i..j].replace('\t', " "), *size, font, *attrs);
                    pieces.push(Piece { src, seg: si, row });
                    cx += w;
                }
            } else {
                let mut w = m.width(&t[i..j], *size, font, *attrs);
                if seg.look.pill && i == 0 {
                    w += 2.0 * PILL_PAD + PILL_GAP;
                }
                // A word glued to the previous piece (no space between,
                // other style) moves with it.
                let glued = pieces.last().is_some_and(|p| p.src.end == src.start && !text[p.src.clone()].ends_with([' ', '\t']));
                if cx + w > right && !row_start {
                    if glued {
                        // Carry the glued run down with this word.
                        let mut k = pieces.len();
                        while k > 0 && pieces[k - 1].row == row && !text[pieces[k - 1].src.clone()].ends_with([' ', '\t']) && (k == pieces.len() || pieces[k - 1].src.end == pieces[k].src.start) {
                            k -= 1;
                        }
                        if k > 0 && pieces[k - 1].row == row {
                            row += 1;
                            for p in &mut pieces[k..] {
                                p.row = row;
                            }
                            cx = content_x;
                            for p in &pieces[k..] {
                                let (f, s, a) = &looks[p.seg];
                                cx += m.width(&text[p.src.clone()], *s, f, *a);
                            }
                        }
                    } else {
                        row += 1;
                        cx = content_x;
                    }
                }
                pieces.push(Piece { src, seg: si, row });
                cx += w;
                row_start = false;
            }
            i = j;
        }
    }

    // Obsidian dims a finished task's text (the reading view does too).
    let done = matches!(&line.kind, Kind::List { marker: Marker::Task(c, _), .. } if *c != ' ');

    // Pass 2: merge pieces into runs, shape each, place left to right.
    let mut runs: Vec<Run> = Vec::new();
    let mut row_x: Vec<f32> = Vec::new();
    for p in pieces {
        let seg = &segs[p.seg];
        let joins = runs.last().is_some_and(|r: &Run| {
            r.row == p.row && r.src.end == p.src.start && r.look == seg.look && r.link == seg.link && !text[p.src.clone()].contains('\t') && !text[r.src.clone()].contains('\t')
        });
        if joins {
            let r = runs.last_mut().unwrap();
            r.src.end = p.src.end;
        } else {
            let (font, size, attrs) = looks[p.seg].clone();
            let color = if done && seg.link.is_none() { th.dim } else { color_for(&seg.look, th) };
            let bg = if seg.look.pill {
                Some(th.pill_bg)
            } else if seg.look.code && !seg.look.mono {
                Some(th.code_bg)
            } else if seg.look.highlight {
                Some(th.highlight_bg)
            } else if seg.look.tag {
                Some(th.tag_bg)
            } else {
                None
            };
            runs.push(Run {
                row: p.row,
                x: 0.0,
                w: 0.0,
                src: p.src,
                text: String::new(),
                size,
                font,
                attrs,
                color,
                bg,
                strike: seg.look.strike,
                link: seg.link,
                look: seg.look,
                xs: Vec::new(),
            });
        }
    }
    for r in &mut runs {
        while row_x.len() <= r.row {
            row_x.push(if row_x.is_empty() { start_x } else { content_x });
        }
        let shown = text[r.src.clone()].replace('\t', " ");
        let offs = m.offsets(&shown, r.size, &r.font, r.attrs);
        r.w = offs.last().map(|o| o.1).unwrap_or(0.0);
        r.xs = offs.into_iter().map(|(b, x)| (r.src.start + b, x)).collect();
        r.text = shown;
        let pad = if r.look.pill { PILL_PAD } else { 0.0 };
        r.x = row_x[r.row] + pad;
        row_x[r.row] += r.w + 2.0 * pad + if r.look.pill { PILL_GAP } else { 0.0 };
    }
    let rows = runs.last().map(|r| r.row + 1).unwrap_or(1);
    let mut height = rows as f32 * row_h;

    // The Properties table's own parts.
    let mut toggle = None;
    if let Kind::Prop(show) = &line.kind {
        let label_in = |text: &str, x: f32, color: [f32; 4]| Deco::Text {
            text: text.to_string(),
            x,
            y: (row_h - base) / 2.0,
            size: base,
            color,
            font: th.body_font.clone(),
        };
        let label = |text: &str, x: f32| label_in(text, x, th.dim);
        match show {
            PropShow::Header => decos.push(label("Properties", 0.0)),
            PropShow::Close => {
                height = (row_h * 0.75).round();
                decos.push(Deco::Quad(Rect { x: 0.0, y: (height / 2.0).round(), width, height: 1.0 }, th.rule));
            }
            PropShow::Hidden => height = 0.0,
            PropShow::Row { key, check, empty } => {
                if let Some(k) = key {
                    decos.push(label(&fit(k, content_x - 10.0, base, &th.body_font, m), 0.0));
                }
                if let Some((bytes, on)) = check {
                    let r = (th.size * 0.42).round();
                    let (cx, cy) = (content_x + r + 1.0, row_h / 2.0);
                    decos.push(Deco::Check { cx, cy, r, checked: *on });
                    let hit = Rect { x: cx - r - 3.0, y: cy - r - 3.0, width: 2.0 * r + 6.0, height: 2.0 * r + 6.0 };
                    toggle = Some((hit, bytes.clone(), *on));
                } else if *empty {
                    // Fainter than any value: text colour has no alpha.
                    let d = th.dim;
                    decos.push(label_in("Empty", content_x, [d[0] * 0.45, d[1] * 0.45, d[2] * 0.5, d[3]]));
                }
            }
        }
    }

    // Block decorations that span the line.
    match &line.kind {
        Kind::Quote(d) => {
            for k in 0..*d {
                decos.push(Deco::Quad(Rect { x: k as f32 * QUOTE_STEP + 2.0, y: 0.0, width: 3.0, height }, th.accent));
            }
        }
        Kind::Rule if !active => decos.push(Deco::Quad(Rect { x: 0.0, y: (height / 2.0).round(), width, height: 1.0 }, th.rule)),
        Kind::Code | Kind::Fence => decos.insert(0, Deco::Quad(Rect { x: -8.0, y: 0.0, width: width + 16.0, height }, th.code_bg)),
        _ => {}
    }
    LineLayout { height, row_h, rows, runs, decos, links: line.links.clone(), task, toggle, content_x, content_start: line.content_start, text_size: base }
}

/// `text` cut to `w` with an ellipsis.
fn fit(text: &str, w: f32, size: f32, font: &str, m: &mut ShapingMeasure) -> String {
    if m.width(text, size, font, TextAttrs::default()) <= w {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let t: String = chars.iter().collect::<String>() + "…";
        if m.width(&t, size, font, TextAttrs::default()) <= w {
            return t;
        }
    }
    "…".into()
}

fn color_for(look: &Look, th: &EditorTheme) -> [f32; 4] {
    if look.marker || look.comment || look.dim {
        th.dim
    } else if look.link || look.tag {
        th.link
    } else {
        th.fg
    }
}

/// Space above and below an embedded image.
pub const IMAGE_PAD: f32 = 4.0;

impl LineLayout {
    /// A line shown as nothing but its embedded image (live preview, the
    /// caret elsewhere): one row as tall as the image, so a click on it
    /// lands on the line and reveals the raw text.
    pub fn image(target: &str, w: f32, h: f32, text_size: f32) -> LineLayout {
        let height = h + 2.0 * IMAGE_PAD;
        LineLayout {
            height,
            row_h: height,
            rows: 1,
            runs: Vec::new(),
            decos: vec![Deco::Image { target: target.to_string(), rect: Rect { x: 0.0, y: IMAGE_PAD, width: w, height: h } }],
            links: Vec::new(),
            task: None,
            toggle: None,
            content_x: 0.0,
            content_start: 0,
            text_size,
        }
    }

    /// The raw line with its image shown below it (the caret on it).
    pub fn with_image_below(mut self, target: &str, w: f32, h: f32) -> LineLayout {
        let y = self.height + IMAGE_PAD;
        self.decos.push(Deco::Image { target: target.to_string(), rect: Rect { x: 0.0, y, width: w, height: h } });
        self.height = y + h + IMAGE_PAD;
        self
    }

    /// Where the caret before byte `col` is drawn: (x, row).
    pub fn caret_xy(&self, col: usize) -> (f32, usize) {
        let mut best: Option<(f32, usize)> = None;
        for r in &self.runs {
            if col < r.src.start {
                break;
            }
            if col <= r.src.end {
                let x = r.xs.iter().rev().find(|(b, _)| *b <= col).map(|(_, x)| *x).unwrap_or(0.0);
                best = Some((r.x + x, r.row));
                if col < r.src.end {
                    break;
                }
            } else {
                best = Some((r.x + r.w, r.row));
            }
        }
        best.unwrap_or_else(|| match self.runs.first() {
            Some(r) => (r.x, r.row),
            None => (self.content_x, 0),
        })
    }

    /// The byte nearest a point (x, row) of the line.
    pub fn col_at(&self, x: f32, row: usize) -> usize {
        let in_row: Vec<&Run> = self.runs.iter().filter(|r| r.row == row).collect();
        if in_row.is_empty() {
            return match self.runs.iter().rev().find(|r| r.row < row) {
                Some(r) => r.src.end,
                None => self.content_start,
            };
        }
        let mut best = (f32::INFINITY, in_row[0].src.start);
        for r in in_row {
            for (b, bx) in &r.xs {
                let d = (r.x + bx - x).abs();
                if d < best.0 {
                    best = (d, *b);
                }
            }
        }
        best.1
    }

    /// The link under a point of the line, by index into `links`.
    pub fn link_at(&self, x: f32, y: f32) -> Option<usize> {
        let row = (y / self.row_h).floor().max(0.0) as usize;
        self.runs.iter().find(|r| r.row == row && r.link.is_some() && x >= r.x && x <= r.x + r.w).and_then(|r| r.link)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::doc_editor::preview::{style_line, Context};

    fn lay(text: &str, active: bool, width: f32) -> LineLayout {
        let th = EditorTheme::new(14.0);
        let mut m = ShapingMeasure::new(false);
        let line = style_line(text, Context::Normal, active);
        layout_line(text, &line, active, width, &th, &mut m)
    }

    #[test]
    fn every_shown_byte_has_an_x_and_clicks_come_back() {
        let t = "plain **bold** and [[Link|shown]] end";
        for active in [false, true] {
            let l = lay(t, active, 2000.0);
            assert_eq!(l.rows, 1);
            let mut last = -1.0;
            for r in &l.runs {
                for (b, x) in &r.xs {
                    assert!(t.is_char_boundary(*b));
                    let ax = r.x + x;
                    assert!(ax >= last - 0.01, "x runs backwards at {b}: {ax} < {last}");
                    last = ax;
                }
            }
            // Every visible boundary maps to a caret x that maps back to it.
            for r in &l.runs {
                for (b, _) in &r.xs {
                    let (x, row) = l.caret_xy(*b);
                    let back = l.col_at(x, row);
                    let (x2, _) = l.caret_xy(back);
                    assert!((x - x2).abs() < 0.5, "{b} -> {x} -> {back}");
                }
            }
        }
    }

    #[test]
    fn long_lines_wrap_with_a_hanging_list_indent() {
        let t = "- one two three four five six seven eight nine ten eleven twelve";
        let l = lay(t, false, 160.0);
        assert!(l.rows > 2, "rows {}", l.rows);
        let first_x: Vec<f32> = (0..l.rows).map(|row| l.runs.iter().find(|r| r.row == row).unwrap().x).collect();
        assert!(first_x.iter().all(|x| (*x - l.content_x).abs() < 0.5), "{first_x:?} vs {}", l.content_x);
        assert!(matches!(l.decos[0], Deco::Dot { .. }));
    }

    #[test]
    fn the_active_lines_marker_ends_where_the_content_starts() {
        let t = "- [ ] task text";
        let off = lay(t, false, 1000.0);
        let on = lay(t, true, 1000.0);
        let text_x = |l: &LineLayout| l.caret_xy(6).0;
        assert!((text_x(&off) - text_x(&on)).abs() < 1.5, "{} vs {}", text_x(&off), text_x(&on));
        assert!(off.task.is_some() && matches!(off.decos[0], Deco::Check { checked: false, .. }));
        // Hidden prefix: a click left of the text lands at its start.
        assert_eq!(off.col_at(0.0, 0), 6);
    }

    #[test]
    fn empty_and_link_lines() {
        let l = lay("", false, 500.0);
        assert_eq!((l.rows, l.caret_xy(0)), (1, (0.0, 0)));
        let t = "see [[Target]]";
        let l = lay(t, false, 500.0);
        let r = l.runs.iter().find(|r| r.link.is_some()).unwrap();
        assert_eq!(l.link_at(r.x + 2.0, 2.0), Some(0));
        assert_eq!(l.link_at(1.0, 2.0), None);
    }
}
