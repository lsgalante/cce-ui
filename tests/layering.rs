//! Which of cce-ui's top-level modules may name which: a ratchet on the
//! crate's layering.
//!
//! An audit on 2026-10-10 found 17 top-level modules in one import cycle —
//! `widget` reaching into `backend`, `scene` into `widget`, the renderer
//! into `engine` — so no part could be understood, tested or moved out
//! alone. This test fixes a direction, lowest layer first:
//!
//! 0. the base: crate-root helpers and the re-exported cce-core modules
//! 1. `style`, `color`, `scale`
//! 2. `draw`, `layout`, `icon`
//! 3. `scene`
//! 4. `context`, `widget`, `text`, `ime`, `text_input`, `window_state`, `a11y`
//! 5. the shells and the renderer: `backend`, `vk`, `wayland`, `web`, `mac`,
//!    `protocol`, `file_dialog`, `mcp`
//! 6. `engine`, the facade apps import
//!
//! A module may name its own layer and anything below it. Every `crate::X`
//! path in a module's non-test code is an edge; one that points UP fails
//! unless it is in [`ALLOWED_UPWARD`], the edges that existed when the test
//! was written. That list only shrinks: an allowance whose edge is gone
//! fails too, so the fix that removes an edge also deletes its line.
//!
//! An integration test, like `doc_claims.rs`, so its own scanning is not part
//! of what it scans. Needs nothing but std.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The layer of every top-level name a module can reach through `crate::`.
fn layer(name: &str) -> Option<u8> {
    Some(match name {
        // Crate-root helpers and macros (lib.rs), and cce-core's modules
        // re-exported at their old paths.
        "config" | "input" | "motion" | "units" | "ipc" | "locale" | "plan" | "process" | "fmt"
        | "desktop_entry" | "relief_spec" | "l10n" | "history" | "compute"
        | "create_font_system" | "create_font_system_with_system_fonts" | "geometry_font_system"
        | "icon_pixels" | "icon_source" | "icon_tint" | "icons_dir" | "rasterize_svg" | "upload_icon"
        | "upload_icon_tinted" | "page_fonts" | "scroll_debug" | "impl_widget_base" => 0,
        "style" | "color" | "colors" | "scale" => 1,
        // `icon` uploads what it resolves, through `draw`.
        "draw" | "layout" | "icon" => 2,
        "scene" => 3,
        // `text` shapes with `scene::paint` and widget types: it sits with them.
        "context" | "ime" | "text_input" | "window_state" | "a11y" | "widget" | "text" => 4,
        "backend" | "vk" | "wayland" | "web" | "mac" | "protocol" | "file_dialog" | "mcp" => 5,
        "engine" => 6,
        _ => return None,
    })
}

/// Upward edges that existed on 2026-10-10, `from -> to`. Delete a line
/// when its edge is gone (the test says which); never add one.
const ALLOWED_UPWARD: &[(&str, &str)] = &[
    ("color", "layout"),
    ("color", "scene"),
    ("layout", "context"),
    ("layout", "scene"),
    // Was layout -> backend: the same use of text shaping, renamed when
    // backend::text became the top-level text module.
    ("layout", "text"),
    ("layout", "widget"),
    ("scale", "window_state"),
    ("style", "layout"),
];

/// The code a module ships: everything before a trailing `#[cfg(test)] mod
/// tests`, with line comments dropped (doc links name modules too).
fn shipped(text: &str) -> String {
    let cut = text
        .match_indices("#[cfg(test)]")
        .find(|(i, _)| text[i + "#[cfg(test)]".len()..].trim_start().starts_with("mod tests"))
        .map_or(text.len(), |(i, _)| i);
    text[..cut].lines().map(|l| l.split("//").next().unwrap_or("")).collect::<Vec<_>>().join("\n")
}

fn rs_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            rs_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// Every `from -> to` edge between distinct top-level modules, with a file
/// that has it.
fn edges() -> BTreeMap<(String, String), String> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rs_files(&src, &mut files);
    let mut out = BTreeMap::new();
    for path in files {
        let rel = path.strip_prefix(&src).unwrap();
        let first = rel.components().next().unwrap().as_os_str().to_string_lossy().into_owned();
        let from = first.trim_end_matches(".rs").to_string();
        if matches!(from.as_str(), "lib" | "main" | "config_style_tests") {
            continue;
        }
        let code = shipped(&std::fs::read_to_string(&path).unwrap());
        for (i, _) in code.match_indices("crate::") {
            let to: String =
                code[i + 7..].chars().take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_').collect();
            if !to.is_empty() && to != from {
                out.entry((from.clone(), to)).or_insert_with(|| rel.display().to_string());
            }
        }
    }
    out
}

#[test]
fn modules_only_name_their_own_layer_or_lower() {
    let edges = edges();
    let allowed: BTreeSet<(String, String)> =
        ALLOWED_UPWARD.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    let mut problems = Vec::new();
    let mut seen_allowed = BTreeSet::new();
    for ((from, to), file) in &edges {
        let (Some(lf), Some(lt)) = (layer(from), layer(to)) else {
            problems.push(format!("{from} -> {to} ({file}): give the new module a layer in `layer()`"));
            continue;
        };
        if lt <= lf {
            continue;
        }
        let key = (from.clone(), to.clone());
        if allowed.contains(&key) {
            seen_allowed.insert(key);
        } else {
            problems.push(format!("{from} (layer {lf}) -> {to} (layer {lt}), first in {file}: an upward edge"));
        }
    }
    for (from, to) in allowed.difference(&seen_allowed) {
        problems.push(format!("{from} -> {to} no longer happens: delete it from ALLOWED_UPWARD"));
    }
    assert!(problems.is_empty(), "layering:\n  {}", problems.join("\n  "));
}
