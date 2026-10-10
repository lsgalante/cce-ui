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
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | what a laid-out line is (`Run`, `Deco`, `LineLayout`) and `layout_line`, which builds one |
//! | `theme` | the `EditorTheme`, the indents and the properties table's pill metrics |
//! | `geometry` | what a laid-out line answers: image layouts, the caret's x, selection rects, the column at an x, the link at a point |

mod geometry;
mod theme;
#[cfg(test)]
mod tests;

pub use geometry::*;
pub use theme::*;

use std::ops::Range;

use super::preview::{self, Kind, Look, Marker, PropShow, Target};
use crate::scene::layout::Rect;
use crate::scene::paint::TextAttrs;
use crate::widget::shaping::{Measure, ShapingMeasure};


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
    /// (line byte, x relative to `x`) at every char boundary of `src`: the caret's place
    /// before that char (a right-to-left letter's right edge), so not ascending in x where
    /// the text turns.
    pub xs: Vec<(usize, f32)>,
    /// The run's clusters (line bytes, x relative to `x`), what a selection covers.
    pub clusters: Vec<crate::widget::shaping::ShapedCluster>,
    /// Whether the run reads right to left (its own first strong character).
    pub rtl: bool,
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
        (font, size, TextAttrs { italic: look.italic, weight: (look.bold || heading).then_some(700), ..Default::default() })
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
                clusters: Vec::new(),
                rtl: false,
            });
        }
    }
    // Split every run where the bidirectional level changes, so each is one direction: a
    // run of plain text can hold an English word and a Hebrew one, which the reordering
    // below must be able to place apart. Levels are the whole line's, neutrals (spaces,
    // markup) resolved from their neighbours.
    let para_rtl = crate::backend::text::paragraph_rtl(text);
    let levels = crate::backend::text::bidi_levels(text, para_rtl);
    let level_at = |b: usize| levels.get(b).copied().unwrap_or(if para_rtl { 1 } else { 0 });
    let mut split: Vec<Run> = Vec::with_capacity(runs.len());
    for r in runs {
        let mut from = r.src.start;
        let mut cur = level_at(from);
        for (i, _) in text[r.src.clone()].char_indices() {
            let b = r.src.start + i;
            if level_at(b) != cur {
                split.push(Run { src: from..b, ..r.clone() });
                from = b;
                cur = level_at(b);
            }
        }
        split.push(Run { src: from..r.src.end, ..r });
    }
    let mut runs = split;
    for r in &mut runs {
        while row_x.len() <= r.row {
            row_x.push(if row_x.is_empty() { start_x } else { content_x });
        }
        let shown = text[r.src.clone()].replace('\t', " ");
        let shaped = m.shape(&shown, r.size, &r.font, r.attrs);
        r.w = shaped.width;
        r.rtl = shaped.rtl;
        r.xs = shaped.stops.iter().map(|&(b, x)| (r.src.start + b, x)).collect();
        r.clusters = shaped
            .clusters
            .iter()
            .map(|c| crate::widget::shaping::ShapedCluster { start: r.src.start + c.start, end: r.src.start + c.end, ..*c })
            .collect();
        r.text = shown;
        let pad = if r.look.pill { PILL_PAD } else { 0.0 };
        r.x = row_x[r.row] + pad;
        row_x[r.row] += r.w + 2.0 * pad + if r.look.pill { PILL_GAP } else { 0.0 };
    }
    // Visual order. Runs were placed left to right in logical order; a row whose runs are
    // not all of the paragraph's direction is re-placed in the order the bidirectional
    // algorithm draws them (`visual_run_order`), and a right-to-left paragraph's rows are
    // set against the right edge — a plain line or a heading's; a list, quote or table
    // keeps its markers at the left and only reorders.
    let align_right = para_rtl && matches!(line.kind, Kind::Plain | Kind::Heading(_));
    let rows_placed = runs.last().map_or(0, |r| r.row + 1);
    for row in 0..rows_placed {
        let idx: Vec<usize> = (0..runs.len()).filter(|&i| runs[i].row == row).collect();
        if idx.is_empty() {
            continue;
        }
        let order = {
            let run_levels: Vec<u8> = idx.iter().map(|&i| level_at(runs[i].src.start)).collect();
            crate::backend::text::visual_order(&run_levels)
        };
        if !align_right && order.iter().enumerate().all(|(k, &o)| k == o) {
            continue;
        }
        // A run's room: its pill padding either side, and the gap after a pill.
        let room = |r: &Run| {
            let pad = if r.look.pill { PILL_PAD } else { 0.0 };
            (pad, r.w + 2.0 * pad + if r.look.pill { PILL_GAP } else { 0.0 })
        };
        let start = runs[idx[0]].x - room(&runs[idx[0]]).0;
        let total: f32 = idx.iter().map(|&i| room(&runs[i]).1).sum();
        let mut x = if align_right { (width - total).max(start) } else { start };
        for &k in &order {
            let i = idx[k];
            let (pad, adv) = room(&runs[i]);
            runs[i].x = x + pad;
            x += adv;
        }
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
            PropShow::Header => decos.push(label(&crate::l10n::tr("doc-properties"), 0.0)),
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
                    decos.push(label_in(&crate::l10n::tr("doc-empty"), content_x, [d[0] * 0.45, d[1] * 0.45, d[2] * 0.5, d[3]]));
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
    } else if look.tag {
        th.tag
    } else if look.link {
        th.link
    } else {
        th.fg
    }
}
