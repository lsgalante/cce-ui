//! Embeds: a line that is nothing but one embed (`![[pic.png|300]]`, `![alt|300x200](pic.png)`),
//! and the embeds inside a line, each with the size it asks for, read as Obsidian reads it.

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

/// Whether an embed alias is Obsidian's size form (`300`, `300x200`).
pub(super) fn embed_size_spec(alias: &str) -> bool {
    let num = |v: &str| !v.is_empty() && v.chars().all(|c| c.is_ascii_digit());
    match alias.trim().split_once('x') {
        Some((w, h)) => num(w.trim()) && num(h.trim()),
        None => num(alias.trim()),
    }
}
