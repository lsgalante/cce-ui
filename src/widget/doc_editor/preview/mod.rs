//! Live preview: how one line of Markdown shows. Pure: text in, styled
//! segments out.
//!
//! Every segment covers a byte range of the line and shows exactly those
//! bytes (a tab shows as one space), so caret and click positions map 1:1
//! between screen and source. Markup is never replaced, only hidden: on a
//! line the caret is NOT on, markers (`**`, `## `, `[[` and a link's target
//! part, a list's `- [ ] `…) have no segment and are drawn as nothing — or
//! as a decoration the line carries (a bullet, a checkbox, a quote bar, a
//! rule). On the caret's line ("active") the markers come back, dimmed,
//! as Obsidian's live preview does.
//!
//! Block context (fenced code, frontmatter) spans lines, so it is computed
//! for the whole document first ([`contexts`]); everything else is per line.
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | what a line is (`Context`, `Kind`, `Marker`, …), the styled `Line`, block contexts, `style_line` |
//! | `embeds` | a line's embeds: one standing alone, or several inside it, with their wanted sizes |
//! | `properties` | the frontmatter as a Properties table: each line's role and its styled row |
//! | `inline` | the inline-markup scanner: emphasis, code, links, tags, highlights, comments |

mod embeds;
mod inline;
mod properties;
#[cfg(test)]
mod tests;

pub use embeds::*;
use inline::*;
pub use properties::*;

use std::ops::Range;

/// What a line is inside, from the lines before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Context {
    Normal,
    /// Inside a fenced code block (not the fences themselves).
    Code,
    /// A ``` or ~~~ fence line, opening or closing.
    Fence,
    /// Inside the frontmatter, or one of its `---` lines.
    Frontmatter,
}

/// Block context per line: fences toggle code; a `---` first line opens
/// frontmatter, closed by the next `---`.
pub fn contexts(lines: &[String]) -> Vec<Context> {
    let mut out = Vec::with_capacity(lines.len());
    let mut fence: Option<(char, usize)> = None;
    let mut front = lines.first().is_some_and(|l| l.trim_end() == "---");
    for (i, line) in lines.iter().enumerate() {
        if front {
            out.push(Context::Frontmatter);
            if i > 0 && line.trim_end() == "---" {
                front = false;
            }
            continue;
        }
        let t = line.trim_start();
        let run = |c: char| t.chars().take_while(|&x| x == c).count();
        match fence {
            Some((c, n)) => {
                if run(c) >= n && t.trim_start_matches(c).trim().is_empty() {
                    fence = None;
                    out.push(Context::Fence);
                } else {
                    out.push(Context::Code);
                }
            }
            None => {
                let (bt, tl) = (run('`'), run('~'));
                if bt >= 3 {
                    fence = Some(('`', bt));
                    out.push(Context::Fence);
                } else if tl >= 3 {
                    fence = Some(('~', tl));
                    out.push(Context::Fence);
                } else {
                    out.push(Context::Normal);
                }
            }
        }
    }
    out
}

/// A line's block shape.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Plain,
    Heading(u8),
    /// `> ` nesting depth.
    Quote(u8),
    /// A list item: indentation level, and its marker.
    List { level: u8, marker: Marker },
    Rule,
    Code,
    Fence,
    Frontmatter,
    Table,
    /// A line of the frontmatter shown as the Properties table.
    Prop(PropShow),
}

/// How a frontmatter line draws in the Properties table.
#[derive(Clone, Debug, PartialEq)]
pub enum PropShow {
    /// The opening `---`: the table's "Properties" header.
    Header,
    /// The closing `---`: the rule under the table.
    Close,
    /// A row: the key in the key column (a list's key is drawn by its first
    /// item), the value as segments in the value column; a `true`/`false`
    /// value is a checkbox instead (its bytes, and whether it is ticked).
    Row { key: Option<String>, check: Option<(Range<usize>, bool)>, empty: bool },
    /// Blank, comment, and a list's own key line: no height.
    Hidden,
}

/// The role of each line of a closed leading frontmatter block, `None`
/// outside one (or when it never closes, which shows raw).
#[derive(Clone, Debug, PartialEq)]
pub enum Prop {
    Open,
    Close,
    /// `key: value` on one line (`value` may be empty).
    Field { key: Range<usize>, value: Range<usize> },
    /// `key:` with list items under it; its first item draws the key.
    ListKey,
    /// `- item` under a key; the first carries the key line.
    Item { value: Range<usize>, key_line: Option<usize> },
    Hidden,
    /// What the table cannot show (nested maps, block scalars): raw.
    Raw,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Marker {
    Bullet,
    /// The number as written, with its `.` or `)`.
    Number(String),
    /// A task's status char, and the byte of that char in the line.
    Task(char, usize),
}

/// A segment's look. `marker` is markup shown on the active line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Look {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strike: bool,
    pub highlight: bool,
    pub link: bool,
    pub tag: bool,
    pub marker: bool,
    pub comment: bool,
    /// Code blocks, fences, frontmatter and tables: monospace.
    pub mono: bool,
    /// The dim text of a done task or a fence line.
    pub dim: bool,
    /// An item of a list property, drawn as a pill.
    pub pill: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// `[[target#sub|…]]` or a relative `[…](path)`.
    Note { target: String, subpath: Option<String> },
    Url(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Seg {
    pub range: Range<usize>,
    pub look: Look,
    /// The link this segment shows, by index into [`Line::links`].
    pub link: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub kind: Kind,
    pub segs: Vec<Seg>,
    pub links: Vec<Target>,
    /// Where content starts after the hidden prefix (`## `, `> `, a list
    /// marker), for a click left of the text and an empty item's caret.
    pub content_start: usize,
}

const BOLD: u16 = 1;

const ITALIC: u16 = 2;

const CODE: u16 = 4;

const STRIKE: u16 = 8;

const HIGHLIGHT: u16 = 16;

const LINK: u16 = 32;

const TAG: u16 = 64;

const MARKER: u16 = 128;

const COMMENT: u16 = 256;

const DIM: u16 = 512;

/// The fenced code block holding `line` — its opening and closing fence
/// lines (the last line when it never closes) — or `None` outside one.
pub fn fenced_block(ctx: &[Context], line: usize) -> Option<(usize, usize)> {
    if !matches!(ctx.get(line), Some(Context::Code | Context::Fence)) {
        return None;
    }
    let mut open: Option<usize> = None;
    for (i, c) in ctx.iter().enumerate() {
        match (c, open) {
            (Context::Fence, None) => open = Some(i),
            (Context::Fence, Some(a)) => {
                if line >= a && line <= i {
                    return Some((a, i));
                }
                open = None;
            }
            _ => {}
        }
        if i > line && open.is_none() {
            break;
        }
    }
    open.map(|a| (a, ctx.len() - 1))
}

/// The leading frontmatter block (open and close lines) when `line` is
/// in it.
pub fn frontmatter_block(ctx: &[Context], line: usize) -> Option<(usize, usize)> {
    if ctx.get(line) != Some(&Context::Frontmatter) {
        return None;
    }
    let end = ctx.iter().take_while(|c| **c == Context::Frontmatter).count();
    Some((0, end - 1))
}

/// Style one line. `active` shows its markers; `ctx` is its block context.
pub fn style_line(text: &str, ctx: Context, active: bool) -> Line {
    let n = text.len();
    let whole = |look: Look| Line { kind: Kind::Plain, segs: vec![Seg { range: 0..n, look, link: None }], links: Vec::new(), content_start: 0 };
    match ctx {
        Context::Code => return Line { kind: Kind::Code, ..whole(Look { mono: true, ..Look::default() }) },
        // A fence hides while the caret is outside its block (the editor
        // makes the whole block active), as Obsidian's live preview does.
        Context::Fence if !active => return Line { kind: Kind::Fence, segs: Vec::new(), links: Vec::new(), content_start: 0 },
        Context::Fence => return Line { kind: Kind::Fence, ..whole(Look { mono: true, dim: true, ..Look::default() }) },
        Context::Frontmatter => return Line { kind: Kind::Frontmatter, ..whole(Look { mono: true, dim: true, ..Look::default() }) },
        Context::Normal => {}
    }
    let mut flags = vec![0u16; n];
    let mut link_of: Vec<Option<usize>> = vec![None; n];
    let mut links: Vec<Target> = Vec::new();
    let mut kind = Kind::Plain;
    let mut prefix = 0usize;

    let trimmed = text.trim_start();
    let indent_bytes = n - trimmed.len();
    // A thematic break: three or more of - * _ and nothing else.
    let compact: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.len() >= 3 && (compact.chars().all(|c| c == '-') || compact.chars().all(|c| c == '*') || compact.chars().all(|c| c == '_')) && indent_bytes < 4 {
        let look = Look { marker: true, ..Look::default() };
        return Line { kind: Kind::Rule, segs: if active { vec![Seg { range: 0..n, look, link: None }] } else { Vec::new() }, links, content_start: n };
    }
    if trimmed.starts_with('|') {
        return Line { kind: Kind::Table, ..whole(Look { mono: true, ..Look::default() }) };
    }
    // Headings.
    let hashes = trimmed.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&hashes) && indent_bytes < 4 && (trimmed.len() == hashes || trimmed[hashes..].starts_with(' ')) {
        kind = Kind::Heading(hashes as u8);
        prefix = (indent_bytes + hashes + 1).min(n);
    } else {
        // Quotes, possibly nested, then a list marker inside them.
        let mut at = 0usize;
        let mut depth = 0u8;
        loop {
            let rest = &text[at..];
            let sp = rest.len() - rest.trim_start().len();
            if rest[sp..].starts_with('>') {
                depth += 1;
                at += sp + 1;
                if text[at..].starts_with(' ') {
                    at += 1;
                }
            } else {
                break;
            }
        }
        if depth > 0 {
            kind = Kind::Quote(depth);
            prefix = at;
        }
        if let Some((level, marker, end)) = list_marker(&text[at..]) {
            let marker = match marker {
                Marker::Task(c, b) => Marker::Task(c, b + at),
                m => m,
            };
            if depth == 0 {
                kind = Kind::List { level, marker };
            }
            prefix = at + end;
        }
    }
    for f in flags.iter_mut().take(prefix) {
        *f |= MARKER;
    }
    let done = matches!(kind, Kind::List { marker: Marker::Task(c, _), .. } if c != ' ');
    inline(text, prefix, &mut flags, &mut link_of, &mut links);
    if done {
        for f in flags.iter_mut().skip(prefix) {
            *f |= DIM;
        }
    }
    // Merge bytes into segments at char boundaries.
    let mut segs: Vec<Seg> = Vec::new();
    for (i, _) in text.char_indices() {
        let f = flags[i];
        let hidden = !active && f & MARKER != 0;
        if hidden {
            continue;
        }
        let end = i + text[i..].chars().next().map(char::len_utf8).unwrap_or(1);
        let look = Look {
            bold: f & BOLD != 0,
            italic: f & ITALIC != 0,
            code: f & CODE != 0,
            strike: f & STRIKE != 0,
            highlight: f & HIGHLIGHT != 0,
            link: f & LINK != 0,
            tag: f & TAG != 0,
            marker: f & MARKER != 0,
            comment: f & COMMENT != 0,
            mono: false,
            dim: f & DIM != 0,
            pill: false,
        };
        let link = link_of[i];
        match segs.last_mut() {
            Some(s) if s.range.end == i && s.look == look && s.link == link => s.range.end = end,
            _ => segs.push(Seg { range: i..end, look, link }),
        }
    }
    Line { kind, segs, links, content_start: prefix }
}

/// A list marker at the start of `s` (after indentation): its level, the
/// marker, and the byte where the content starts.
fn list_marker(s: &str) -> Option<(u8, Marker, usize)> {
    let body = s.trim_start_matches([' ', '\t']);
    let ind = &s[..s.len() - body.len()];
    // A tab is one level (Obsidian indents with tabs); two spaces are one.
    let cols: usize = ind.chars().map(|c| if c == '\t' { 2 } else { 1 }).sum();
    let level = (cols / 2).min(12) as u8;
    let (marker, mlen) = if body.starts_with("- ") || body.starts_with("* ") || body.starts_with("+ ") {
        (Marker::Bullet, 2)
    } else if matches!(body, "-" | "*" | "+") {
        (Marker::Bullet, 1)
    } else {
        let digits = body.chars().take_while(char::is_ascii_digit).count();
        if digits == 0 || digits > 9 {
            return None;
        }
        let after = &body[digits..];
        if after.starts_with(". ") || after.starts_with(") ") {
            (Marker::Number(body[..digits + 1].to_string()), digits + 2)
        } else {
            return None;
        }
    };
    let mut end = ind.len() + mlen;
    let rest = &s[end..];
    let mut chars = rest.chars();
    if let (Some('['), Some(c), Some(']')) = (chars.next(), chars.next(), chars.next()) {
        let after = &rest[2 + c.len_utf8()..];
        if after.is_empty() || after.starts_with(' ') {
            let status_at = end + 1;
            end += 2 + c.len_utf8() + usize::from(after.starts_with(' '));
            return Some((level, Marker::Task(c, status_at), end));
        }
    }
    Some((level, marker, end))
}
