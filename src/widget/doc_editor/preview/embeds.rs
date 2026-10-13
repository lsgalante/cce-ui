//! Embeds: a line that is nothing but one embed (`![[pic.png|300]]`, `![alt|300x200](pic.png)`),
//! and the embeds inside a line, each with the size it asks for, read as Obsidian reads it — by
//! `cce_core::markdown`'s rules, the reading view's.

use cce_core::markdown::{embed_size, is_url, split_target};

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
        if is_url(dest) || dest.contains(')') || alt.contains(']') {
            return None;
        }
        (cce_core::fmt::percent_decode(dest), alt)
    };
    let target = split_target(&target).0;
    if target.is_empty() {
        return None;
    }
    let size = embed_size(alias);
    Some((target.to_string(), size.map(|s| s.width), size.and_then(|s| s.height)))
}

/// Every embed in a line, in order — `![[…]]` and `![…](…)` anywhere in
/// it — read as [`standalone_embed`] reads one.
pub fn inline_embeds(text: &str) -> Vec<(String, Option<u32>, Option<u32>)> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(off) = text[i..].find("![") {
        let at = i + off;
        let rest = &text[at..];
        let end = if rest.starts_with("![[") {
            rest.find("]]").map(|e| e + 2)
        } else {
            rest.find("](").and_then(|m| rest[m..].find(')').map(|e| m + e + 1))
        };
        let Some(end) = end else { break };
        if let Some(found) = standalone_embed(&rest[..end]) {
            out.push(found);
        }
        i = at + end;
    }
    out
}

/// Whether an embed's whole alias is Obsidian's size form (`300`, `300x200`) rather than a
/// caption (or a caption and a size).
pub(super) fn embed_size_spec(alias: &str) -> bool {
    !alias.contains('|') && embed_size(alias).is_some()
}
