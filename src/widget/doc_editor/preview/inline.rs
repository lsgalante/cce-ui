//! The inline-markup scanner: one pass over a line's text marking each byte's style (bold, italic,
//! code, strike, highlight, link, tag, comment, marker), and the pair-finding it rests on.

use super::*;

/// Mark the inline syntax of `text[from..]` in `flags`.
pub(super) fn inline(text: &str, from: usize, flags: &mut [u16], link_of: &mut [Option<usize>], links: &mut Vec<Target>) {
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
        // An embed's alias that is only a size (`![[pic.png|300]]`) is not
        // a caption: show the file name and hide the `|300`, as for links.
        let size_alias = start < o && alias_at.is_some_and(|a| embed_size_spec(&text[a..c]));
        let shown = match alias_at {
            Some(a) if size_alias => o + 2..a - 1,
            Some(a) => a..c,
            None => o + 2..c,
        };
        mark(flags, start..shown.start, MARKER);
        mark(flags, shown.clone(), LINK);
        mark(flags, shown.end..c + 2, MARKER);
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
pub(super) fn find_free(text: &str, from: usize, pat: &str, taken: &[bool]) -> Option<usize> {
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
pub(super) fn find_run(b: &[u8], from: usize, c: u8, len: usize) -> Option<usize> {
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
