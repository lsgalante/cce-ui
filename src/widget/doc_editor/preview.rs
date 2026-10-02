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

/// A line that is nothing but one embed — `![[pic.png|300]]` or
/// `![alt|300x200](pic%20one.png)` — as (link text, wanted width, wanted
/// height), the size read as Obsidian reads it. `None` for anything else,
/// URLs included.
pub fn standalone_embed(text: &str) -> Option<(String, Option<u32>, Option<u32>)> {
    let t = text.trim();
    let (target, alias) = if let Some(inner) = t.strip_prefix("![[").and_then(|r| r.strip_suffix("]]")) {
        if inner.contains("]]") || inner.contains("[[") {
            return None;
        }
        match inner.split_once('|') {
            Some((target, alias)) => (target.to_string(), alias),
            None => (inner.to_string(), ""),
        }
    } else {
        let rest = t.strip_prefix("![")?;
        let (alt, dest) = rest.split_once("](")?;
        let dest = dest.strip_suffix(')')?.trim();
        let dest = dest.strip_prefix('<').and_then(|d| d.strip_suffix('>')).unwrap_or(dest);
        if dest.contains("://") || dest.contains(')') || alt.contains(']') {
            return None;
        }
        (dest.replace("%20", " "), alt)
    };
    let target = target.split('#').next().unwrap_or("").trim().to_string();
    if target.is_empty() {
        return None;
    }
    let spec = alias.rsplit('|').next().unwrap_or(alias).trim();
    let num = |v: &str| v.trim().parse::<u32>().ok().filter(|n| *n > 0);
    let (w, h) = match spec.split_once('x') {
        Some((w, h)) => match (num(w), num(h)) {
            (Some(w), Some(h)) => (Some(w), Some(h)),
            _ => (None, None),
        },
        None => (num(spec), None),
    };
    Some((target, w, h))
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

/// A `key:` at the start of `line`: the key's byte range and where what
/// follows the colon starts.
fn yaml_key(line: &str) -> Option<(Range<usize>, usize)> {
    if line.starts_with([' ', '\t', '-', '#']) {
        return None;
    }
    let b = line.as_bytes();
    let colon = (0..b.len()).find(|&i| b[i] == b':' && (i + 1 == b.len() || b[i + 1] == b' ' || b[i + 1] == b'\t'))?;
    let key = line[..colon].trim_end();
    (!key.is_empty()).then_some((0..key.len(), colon + 1))
}

/// The range of `line[from..]` without surrounding whitespace.
fn trimmed(line: &str, from: usize) -> Range<usize> {
    let rest = &line[from..];
    let start = from + (rest.len() - rest.trim_start().len());
    start..from + rest.trim_end().len()
}

/// Roles for the frontmatter lines (see [`Prop`]), per line of the
/// document; `None` past the block.
pub fn properties(lines: &[String], ctx: &[Context]) -> Vec<Option<Prop>> {
    let mut out = vec![None; lines.len()];
    let Some((_, end)) = frontmatter_block(ctx, 0) else { return out };
    if end == 0 || lines[end].trim_end() != "---" {
        return out; // never closed: raw
    }
    out[0] = Some(Prop::Open);
    out[end] = Some(Prop::Close);
    let mut list_key: Option<usize> = None;
    let mut raw_block = false;
    for i in 1..end {
        let line = &lines[i];
        let t = line.trim_start();
        let indented = t.len() < line.len();
        let role = if t.is_empty() || t.starts_with('#') {
            Prop::Hidden
        } else if raw_block && indented {
            Prop::Raw
        } else if let Some((key, after)) = yaml_key(line) {
            raw_block = false;
            let value = trimmed(line, after);
            let v = &line[value.clone()];
            if v.starts_with(['|', '>']) {
                raw_block = true;
                Prop::Raw
            } else if v.is_empty() {
                // A list follows, or a nested map (raw), or nothing.
                let next = lines[i + 1..end].iter().find(|l| !l.trim().is_empty());
                match next.map(|l| l.trim_start()) {
                    Some(n) if n == "-" || n.starts_with("- ") => {
                        list_key = Some(i);
                        Prop::ListKey
                    }
                    Some(_) if next.is_some_and(|l| l.starts_with([' ', '\t'])) => {
                        raw_block = true;
                        Prop::Field { key, value }
                    }
                    _ => Prop::Field { key, value },
                }
            } else {
                Prop::Field { key, value }
            }
        } else if t == "-" || t.starts_with("- ") {
            let value = trimmed(line, line.len() - t.len() + 1);
            Prop::Item { value, key_line: list_key.take() }
        } else {
            Prop::Raw
        };
        out[i] = Some(role);
    }
    out
}

/// The key of a `key:` line, for a list item drawing its key.
pub fn prop_key(line: &str) -> Option<&str> {
    yaml_key(line).map(|(k, _)| &line[k])
}

/// A quoted YAML scalar's inside.
fn unquote(line: &str, r: Range<usize>) -> Range<usize> {
    let v = &line[r.clone()];
    if v.len() >= 2 && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\''))) {
        r.start + 1..r.end - 1
    } else {
        r
    }
}

/// The items of an inline `[a, "b", [[c]]]` list: split on commas outside
/// quotes and `[[…]]`, trimmed and unquoted.
fn flow_items(line: &str, r: Range<usize>) -> Vec<Range<usize>> {
    let inner = r.start + 1..r.end - 1;
    let b = line.as_bytes();
    let (mut out, mut start, mut quote, mut depth) = (Vec::new(), inner.start, None::<u8>, 0i32);
    let mut i = inner.start;
    while i <= inner.end {
        let c = if i < inner.end { b[i] } else { b',' };
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (Some(_), _) => {}
            (None, b'"' | b'\'') => quote = Some(c),
            (None, b'[') => depth += 1,
            (None, b']') => depth -= 1,
            (None, b',') if depth <= 0 => {
                let item = unquote(line, trimmed(&line[..i], start));
                if !item.is_empty() {
                    out.push(item);
                }
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    out
}

/// Inline-style `line[r]` as part of a property value, segments offset into
/// the line; `pill` marks them as list pills.
fn value_segs(line: &str, r: Range<usize>, pill: bool, segs: &mut Vec<Seg>, links: &mut Vec<Target>) {
    let sub = &line[r.clone()];
    let styled = style_line(sub, Context::Normal, false);
    let base = links.len();
    if styled.kind != Kind::Plain || styled.content_start != 0 {
        segs.push(Seg { range: r, look: Look { pill, ..Look::default() }, link: None });
        return;
    }
    for s in styled.segs {
        let look = Look { pill, ..s.look };
        segs.push(Seg { range: s.range.start + r.start..s.range.end + r.start, look, link: s.link.map(|k| k + base) });
    }
    links.extend(styled.links);
}

/// Style a frontmatter line as its row of the Properties table. `key` is
/// the key an [`Prop::Item`] draws (its key line's), if it is the first.
pub fn style_property(line: &str, prop: &Prop, key: Option<&str>) -> Line {
    let mut segs = Vec::new();
    let mut links = Vec::new();
    let row = |key: Option<String>, check, empty| Kind::Prop(PropShow::Row { key, check, empty });
    let (kind, content_start) = match prop {
        Prop::Open => (Kind::Prop(PropShow::Header), 0),
        Prop::Close => (Kind::Prop(PropShow::Close), 0),
        Prop::Hidden | Prop::ListKey => (Kind::Prop(PropShow::Hidden), 0),
        Prop::Raw => return style_line(line, Context::Frontmatter, false),
        Prop::Field { key: k, value } => {
            let v = &line[value.clone()];
            let key = Some(line[k.clone()].to_string());
            if v == "true" || v == "false" {
                (row(key, Some((value.clone(), v == "true")), false), value.start)
            } else if v.starts_with('[') && v.ends_with(']') && !v.starts_with("[[") {
                let items = flow_items(line, value.clone());
                // The separators hide; the layout pads pills apart.
                for it in &items {
                    value_segs(line, it.clone(), true, &mut segs, &mut links);
                }
                (row(key, None, items.is_empty()), value.start)
            } else {
                if !value.is_empty() {
                    value_segs(line, unquote(line, value.clone()), false, &mut segs, &mut links);
                }
                (row(key, None, value.is_empty()), value.start)
            }
        }
        Prop::Item { value, .. } => {
            let key = key.map(str::to_string);
            let it = unquote(line, value.clone());
            if !it.is_empty() {
                value_segs(line, it, true, &mut segs, &mut links);
            }
            (row(key, None, value.is_empty()), value.start)
        }
    };
    Line { kind, segs, links, content_start }
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

/// Mark the inline syntax of `text[from..]` in `flags`.
fn inline(text: &str, from: usize, flags: &mut [u16], link_of: &mut [Option<usize>], links: &mut Vec<Target>) {
    let b = text.as_bytes();
    let n = b.len();
    let mut taken = vec![false; n];
    let mark = |flags: &mut [u16], r: Range<usize>, f: u16| {
        for x in &mut flags[r] {
            *x |= f;
        }
    };
    // Code spans first: nothing inside them is syntax.
    let mut i = from;
    while i < n {
        if b[i] == b'`' {
            let run = b[i..].iter().take_while(|&&c| c == b'`').count();
            let close = find_run(b, i + run, b'`', run);
            if let Some(c) = close {
                mark(flags, i..i + run, MARKER | CODE);
                mark(flags, i + run..c, CODE);
                mark(flags, c..c + run, MARKER | CODE);
                for t in &mut taken[i..c + run] {
                    *t = true;
                }
                i = c + run;
                continue;
            }
            i += run;
            continue;
        }
        i += 1;
    }
    // Comments.
    let mut i = from;
    while let Some(o) = find_free(text, i, "%%", &taken) {
        let Some(c) = find_free(text, o + 2, "%%", &taken) else { break };
        mark(flags, o..c + 2, COMMENT);
        mark(flags, o..o + 2, MARKER);
        mark(flags, c..c + 2, MARKER);
        for t in &mut taken[o..c + 2] {
            *t = true;
        }
        i = c + 2;
    }
    // Wikilinks and embeds: [[target#sub|alias]].
    let mut i = from;
    while let Some(o) = find_free(text, i, "[[", &taken) {
        let Some(c) = find_free(text, o + 2, "]]", &taken) else { break };
        let inner = &text[o + 2..c];
        let start = if o > 0 && b[o - 1] == b'!' && !taken[o - 1] { o - 1 } else { o };
        let (target, alias_at) = match inner.find('|') {
            Some(p) => (&inner[..p], Some(o + 2 + p + 1)),
            None => (inner, None),
        };
        let (t, sub) = match target.split_once('#') {
            Some((t, s)) => (t.to_string(), Some(s.to_string())),
            None => (target.to_string(), None),
        };
        let idx = links.len();
        links.push(Target::Note { target: t.trim().to_string(), subpath: sub.filter(|s| !s.is_empty()) });
        let shown = alias_at.unwrap_or(o + 2)..c;
        mark(flags, start..shown.start, MARKER);
        mark(flags, shown.clone(), LINK);
        mark(flags, c..c + 2, MARKER);
        for x in &mut link_of[shown] {
            *x = Some(idx);
        }
        for t in &mut taken[start..c + 2] {
            *t = true;
        }
        i = c + 2;
    }
    // Markdown links: [text](dest).
    let mut i = from;
    while let Some(o) = find_free(text, i, "[", &taken) {
        let Some(c) = find_free(text, o + 1, "](", &taken) else { break };
        let Some(e) = find_free(text, c + 2, ")", &taken) else { break };
        let dest = text[c + 2..e].trim();
        let start = if o > 0 && b[o - 1] == b'!' && !taken[o - 1] { o - 1 } else { o };
        let idx = links.len();
        let decoded = dest.replace("%20", " ");
        links.push(if dest.contains("://") || dest.starts_with("mailto:") {
            Target::Url(dest.to_string())
        } else {
            let (t, s) = match decoded.split_once('#') {
                Some((t, s)) => (t.to_string(), Some(s.to_string())),
                None => (decoded.clone(), None),
            };
            Target::Note { target: t, subpath: s }
        });
        mark(flags, start..o + 1, MARKER);
        mark(flags, o + 1..c, LINK);
        mark(flags, c..e + 1, MARKER);
        for x in &mut link_of[o + 1..c] {
            *x = Some(idx);
        }
        for t in &mut taken[start..e + 1] {
            *t = true;
        }
        i = e + 1;
    }
    // Emphasis: paired delimiters, longest first.
    for (delim, flag) in [("**", BOLD), ("__", BOLD), ("~~", STRIKE), ("==", HIGHLIGHT), ("*", ITALIC), ("_", ITALIC)] {
        let mut i = from;
        while let Some(o) = find_free(text, i, delim, &taken) {
            let inner = o + delim.len();
            // Opening: not followed by a space; `_` must not sit mid-word.
            let next = text[inner..].chars().next();
            let prev = text[..o].chars().next_back();
            let mid_word = delim.starts_with('_') && prev.is_some_and(char::is_alphanumeric);
            if next.is_none_or(char::is_whitespace) || mid_word {
                i = inner;
                continue;
            }
            let mut search = inner;
            let close = loop {
                let Some(c) = find_free(text, search, delim, &taken) else { break None };
                let before = text[..c].chars().next_back();
                let after = text[c + delim.len()..].chars().next();
                let bad_word = delim.starts_with('_') && after.is_some_and(char::is_alphanumeric);
                if c > inner && before.is_some_and(|ch| !ch.is_whitespace()) && !bad_word {
                    break Some(c);
                }
                search = c + delim.len();
            };
            let Some(c) = close else {
                i = inner;
                continue;
            };
            mark(flags, o..inner, MARKER);
            mark(flags, inner..c, flag);
            mark(flags, c..c + delim.len(), MARKER);
            for t in &mut taken[o..inner] {
                *t = true;
            }
            for t in &mut taken[c..c + delim.len()] {
                *t = true;
            }
            i = c + delim.len();
        }
    }
    // Tags: #name at a word start, not all digits.
    let mut i = from;
    while i < n {
        if b[i] == b'#' && !taken[i] && (i == 0 || text[..i].chars().next_back().is_some_and(char::is_whitespace)) {
            let rest = &text[i + 1..];
            let len: usize = rest.chars().take_while(|&c| c.is_alphanumeric() || "_-/".contains(c)).map(char::len_utf8).sum();
            if len > 0 && !rest[..len].chars().all(|c| c.is_ascii_digit() || c == '/') {
                mark(flags, i..i + 1 + len, TAG);
                i += 1 + len;
                continue;
            }
        }
        i += 1;
    }
}

/// The next `pat` at or after `from` that starts on a byte nothing else
/// has taken.
fn find_free(text: &str, from: usize, pat: &str, taken: &[bool]) -> Option<usize> {
    let mut at = from;
    while at <= text.len() {
        let p = at + text.get(at..)?.find(pat)?;
        if !taken[p..p + pat.len()].iter().any(|&t| t) {
            return Some(p);
        }
        at = p + 1;
        while at < text.len() && !text.is_char_boundary(at) {
            at += 1;
        }
    }
    None
}

/// The next run of exactly `len` `c` bytes at or after `from`.
fn find_run(b: &[u8], from: usize, c: u8, len: usize) -> Option<usize> {
    let mut i = from;
    while i < b.len() {
        if b[i] == c {
            let run = b[i..].iter().take_while(|&&x| x == c).count();
            if run == len {
                return Some(i);
            }
            i += run;
        } else {
            i += 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standalone_embeds() {
        assert_eq!(standalone_embed("![[a.png]]"), Some(("a.png".into(), None, None)));
        assert_eq!(standalone_embed("  ![[Pics/a.png|300]] "), Some(("Pics/a.png".into(), Some(300), None)));
        assert_eq!(standalone_embed("![[a.png|300x200]]"), Some(("a.png".into(), Some(300), Some(200))));
        assert_eq!(standalone_embed("![cap|120](my%20pic.png)"), Some(("my pic.png".into(), Some(120), None)));
        assert_eq!(standalone_embed("![](https://x.y/a.png)"), None);
        assert_eq!(standalone_embed("see ![[a.png]]"), None);
        assert_eq!(standalone_embed("![[a.png]] and ![[b.png]]"), None);
    }

    /// The shown text of each segment, `[marker]` for dimmed markup.
    fn shown(text: &str, line: &Line) -> Vec<String> {
        line.segs
            .iter()
            .map(|s| {
                let t = &text[s.range.clone()];
                if s.look.marker { format!("[{t}]") } else { t.to_string() }
            })
            .collect()
    }

    #[test]
    fn inactive_lines_hide_markup() {
        let t = "## A **bold** and *it* with `code`";
        let l = style_line(t, Context::Normal, false);
        assert_eq!(l.kind, Kind::Heading(2));
        assert_eq!(shown(t, &l), ["A ", "bold", " and ", "it", " with ", "code"]);
        assert!(l.segs[1].look.bold && l.segs[3].look.italic && l.segs[5].look.code);
        assert_eq!(l.content_start, 3);
    }

    #[test]
    fn the_active_line_shows_markup_dimmed() {
        let t = "## A **bold**";
        let l = style_line(t, Context::Normal, true);
        assert_eq!(shown(t, &l), ["[## ]", "A ", "[**]", "bold", "[**]"]);
        assert!(l.segs[2].look.marker && l.segs[3].look.bold && !l.segs[3].look.marker);
        // Every byte is covered, in order: 1:1 with the source.
        let mut at = 0;
        for s in &l.segs {
            assert_eq!(s.range.start, at);
            at = s.range.end;
        }
        assert_eq!(at, t.len());
    }

    #[test]
    fn links_show_their_alias_and_carry_a_target() {
        let t = "see [[Note#Part|the part]] and [web](https://a.b) or [[Plain]]";
        let l = style_line(t, Context::Normal, false);
        assert_eq!(shown(t, &l), ["see ", "the part", " and ", "web", " or ", "Plain"]);
        assert_eq!(l.links[0], Target::Note { target: "Note".into(), subpath: Some("Part".into()) });
        assert_eq!(l.links[1], Target::Note { target: "Plain".into(), subpath: None });
        assert_eq!(l.links[2], Target::Url("https://a.b".into()));
        assert_eq!(l.segs[1].link, Some(0));
        assert_eq!(l.segs[3].link, Some(2));
    }

    #[test]
    fn frontmatter_lines_get_property_roles() {
        let src = "---\ntitle: \"Hello\"\ntags: [a, \"b c\"]\ndone: true\n\naliases:\n  - one\n  - \"[[Two]]\"\nmeta:\n  nested: x\nempty:\n---\nbody";
        let lines: Vec<String> = src.split('\n').map(String::from).collect();
        let ctx = contexts(&lines);
        let p = properties(&lines, &ctx);
        assert_eq!(p[0], Some(Prop::Open));
        assert_eq!(p[11], Some(Prop::Close));
        assert_eq!(p[12], None);
        assert!(matches!(p[1], Some(Prop::Field { .. })));
        assert_eq!(p[4], Some(Prop::Hidden));
        assert_eq!(p[5], Some(Prop::ListKey));
        assert!(matches!(p[6], Some(Prop::Item { key_line: Some(5), .. })));
        assert!(matches!(p[7], Some(Prop::Item { key_line: None, .. })));
        assert_eq!(p[9], Some(Prop::Raw), "a nested map shows raw");
        assert_eq!(prop_key(&lines[5]), Some("aliases"));

        let shown = |i: usize, key: Option<&str>| {
            let l = style_property(&lines[i], p[i].as_ref().unwrap(), key);
            (l.kind.clone(), l.segs.iter().map(|s| lines[i][s.range.clone()].to_string()).collect::<Vec<_>>(), l)
        };
        let (kind, segs, _) = shown(1, None);
        assert_eq!(kind, Kind::Prop(PropShow::Row { key: Some("title".into()), check: None, empty: false }));
        assert_eq!(segs, ["Hello"], "quotes hide");
        let (_, segs, l) = shown(2, None);
        assert_eq!(segs, ["a", "b c"]);
        assert!(l.segs.iter().all(|s| s.look.pill));
        let (kind, segs, _) = shown(3, None);
        assert!(segs.is_empty());
        assert_eq!(kind, Kind::Prop(PropShow::Row { key: Some("done".into()), check: Some((6..10, true)), empty: false }));
        let (kind, segs, _) = shown(6, Some("aliases"));
        assert_eq!(segs, ["one"]);
        assert!(matches!(kind, Kind::Prop(PropShow::Row { key: Some(k), .. }) if k == "aliases"));
        let (_, segs, l) = shown(7, None);
        assert_eq!(segs, ["Two"], "a link item shows its name");
        assert_eq!(l.links, [Target::Note { target: "Two".into(), subpath: None }]);
        let (kind, _, _) = shown(10, None);
        assert_eq!(kind, Kind::Prop(PropShow::Row { key: Some("empty".into()), check: None, empty: true }));

        // Never closed: no table.
        let open: Vec<String> = ["---", "a: b"].iter().map(|s| s.to_string()).collect();
        assert!(properties(&open, &contexts(&open)).iter().all(Option::is_none));
    }

    #[test]
    fn fenced_blocks_are_found_and_fences_hide() {
        let lines: Vec<String> = ["a", "```", "x", "```", "```", "y"].iter().map(|s| s.to_string()).collect();
        let ctx = contexts(&lines);
        assert_eq!(fenced_block(&ctx, 0), None);
        assert_eq!(fenced_block(&ctx, 2), Some((1, 3)));
        assert_eq!(fenced_block(&ctx, 3), Some((1, 3)));
        assert_eq!(fenced_block(&ctx, 4), Some((4, 5)));
        assert!(style_line("```rust", Context::Fence, false).segs.is_empty());
        assert_eq!(style_line("```rust", Context::Fence, true).segs.len(), 1);
    }

    #[test]
    fn lists_tasks_quotes_and_rules() {
        let t = "\t- [x] done *thing*";
        let l = style_line(t, Context::Normal, false);
        assert_eq!(l.kind, Kind::List { level: 1, marker: Marker::Task('x', 4) });
        assert_eq!(shown(t, &l), ["done ", "thing"]);
        assert!(l.segs[0].look.dim);
        let l = style_line("1. first", Context::Normal, false);
        assert_eq!(l.kind, Kind::List { level: 0, marker: Marker::Number("1.".into()) });
        let t = "> > quoted";
        let l = style_line(t, Context::Normal, false);
        assert_eq!((l.kind.clone(), shown(t, &l)), (Kind::Quote(2), vec!["quoted".to_string()]));
        assert_eq!(style_line("---", Context::Normal, false).kind, Kind::Rule);
        assert!(style_line("---", Context::Normal, false).segs.is_empty());
        assert_eq!(style_line("- ", Context::Normal, false).kind, Kind::List { level: 0, marker: Marker::Bullet });
    }

    #[test]
    fn emphasis_needs_real_pairs() {
        let t = "a * b * c snake_case_word 2*3*4 ==hi== ~~no~~";
        let l = style_line(t, Context::Normal, false);
        let styled: Vec<(String, bool, bool, bool)> = l
            .segs
            .iter()
            .map(|s| (t[s.range.clone()].to_string(), s.look.italic, s.look.highlight, s.look.strike))
            .collect();
        assert!(styled.iter().any(|(s, _, h, _)| s == "hi" && *h));
        assert!(styled.iter().any(|(s, _, _, k)| s == "no" && *k));
        assert!(styled.iter().all(|(s, i, _, _)| !(s.contains("snake") && *i)), "{styled:?}");
        assert!(!styled.iter().any(|(s, i, _, _)| s.contains(" b ") && *i), "spaced * is not emphasis");
    }

    #[test]
    fn tags_and_code_protect() {
        let t = "#tag and `#not **x**` #2026 #a/b";
        let l = style_line(t, Context::Normal, false);
        let tags: Vec<&str> = l.segs.iter().filter(|s| s.look.tag).map(|s| &t[s.range.clone()]).collect();
        assert_eq!(tags, ["#tag", "#a/b"]);
        assert!(l.segs.iter().any(|s| s.look.code && &t[s.range.clone()] == "#not **x**"));
    }

    #[test]
    fn block_contexts() {
        let lines: Vec<String> = ["---", "a: 1", "---", "text", "```rust", "let x;", "```", "after"].iter().map(|s| s.to_string()).collect();
        assert_eq!(
            contexts(&lines),
            [
                Context::Frontmatter,
                Context::Frontmatter,
                Context::Frontmatter,
                Context::Normal,
                Context::Fence,
                Context::Code,
                Context::Fence,
                Context::Normal
            ]
        );
        assert_eq!(style_line("let x;", Context::Code, false).segs[0].look.mono, true);
    }
}
