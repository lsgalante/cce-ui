//! The frontmatter as a Properties table: each line's role (a key, a list item, the fences), and
//! its row styled — keys in a column, values inline-styled, list values as pills, booleans as
//! checkboxes.

use super::*;

/// A `key:` at the start of `line`: the key's byte range and where what
/// follows the colon starts.
pub(super) fn yaml_key(line: &str) -> Option<(Range<usize>, usize)> {
    if line.starts_with([' ', '\t', '-', '#']) {
        return None;
    }
    let b = line.as_bytes();
    let colon = (0..b.len()).find(|&i| b[i] == b':' && (i + 1 == b.len() || b[i + 1] == b' ' || b[i + 1] == b'\t'))?;
    let key = line[..colon].trim_end();
    (!key.is_empty()).then_some((0..key.len(), colon + 1))
}

/// The range of `line[from..]` without surrounding whitespace.
pub(super) fn trimmed(line: &str, from: usize) -> Range<usize> {
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
pub(super) fn unquote(line: &str, r: Range<usize>) -> Range<usize> {
    let v = &line[r.clone()];
    if v.len() >= 2 && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\''))) {
        r.start + 1..r.end - 1
    } else {
        r
    }
}

/// The items of an inline `[a, "b", [[c]]]` list: split on commas outside
/// quotes and `[[…]]`, trimmed and unquoted.
pub(super) fn flow_items(line: &str, r: Range<usize>) -> Vec<Range<usize>> {
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
pub(super) fn value_segs(line: &str, r: Range<usize>, pill: bool, segs: &mut Vec<Seg>, links: &mut Vec<Target>) {
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
