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
