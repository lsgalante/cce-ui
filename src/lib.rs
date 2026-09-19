pub mod color;
pub mod widget;
pub mod config;
pub mod input;
pub mod history;
pub mod layout;
pub mod relief_spec;
pub mod wayland;
pub mod protocol;
pub mod engine;
pub mod scale;
pub mod units;
pub mod backend;
pub mod context;
pub mod scene;
pub mod process;
pub mod file_dialog;
pub mod icon;
pub mod ipc;
pub mod mcp;
pub mod vk;

pub mod colors {
    pub use crate::color::*;
}

/// The text-shaping library, re-exported so clients need no text dependency of
/// their own: `cce_ui::cosmic_text::FontSystem` rather than a per-crate
/// `cosmic-text` (previously `glyphon`) entry in every client's Cargo.toml.
/// Re-exporting also keeps every client on the one version cce-ui shapes with —
/// a `FontSystem` handed across the boundary must be the same type.
pub use cosmic_text;


pub static IS_VERTICAL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub static BAR_THICKNESS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(24);

/// `CCE_SCROLL_DEBUG=1` traces the wheel pipeline to stderr: raw coalesced
/// axis input (runner), routing decisions (ParametersBg), slider gate/value
/// steps, and glide ticks. Diagnostic-only; checked once per process.
pub fn scroll_debug() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("CCE_SCROLL_DEBUG").is_some())
}

/// Directory bundled fonts are loaded from: `$CCE_FONTS_DIR`, else `~/Dropbox/Fonts`.
/// Resolving via `$HOME` keeps the existing location without a hardcoded username.
pub fn fonts_dir() -> String {
    std::env::var("CCE_FONTS_DIR").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{home}/Dropbox/Fonts")
    })
}

/// Directory the bundled cce-icons SVGs are loaded from: `$CCE_ICONS_DIR`, else
/// `~/projects/cce/cce-icons/svg`.
///
/// The workspace moved out of ~/Dropbox on 2026-08-27: 432k of its 444k files
/// were cargo build artifacts, and syncing them kept Dropbox re-hashing a tree
/// that regenerates itself. Set `$CCE_ICONS_DIR` if yours lives elsewhere —
/// this default is the only path in the toolkit that assumes a checkout
/// location.
pub fn icons_dir() -> String {
    std::env::var("CCE_ICONS_DIR").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{home}/projects/cce/cce-icons/svg")
    })
}

/// Rasterize a bundled cce-icons SVG (`<name>.svg` under [`icons_dir`]) at
/// `px` on its longer side and upload it as a renderer texture. Returns
/// `(image id, pixel w, pixel h)` for `PaintCtx::image` / `ImageView`; cached
/// per `(name, px)` so widget rebuilds reuse the one upload. `None` when the
/// icon is missing or unparsable (callers keep a text fallback).
///
/// **The cache is per renderer, not per process.** An image id names an entry
/// in one renderer's image table, and a renderer does not outlive its
/// session: `window_runner` repairs a lost Wayland transport by opening a new
/// session around the same `Application`, which rebuilds the renderer and its
/// image table. A draw for an id that table does not hold is skipped rather
/// than reported, so a cache that survived the rebuild left every bundled
/// glyph in the process silently undrawn — a status bar that reconnected kept
/// its numbers and lost its icons, and the same went for every
/// [`Button::new_icon`] face, treelist chevron and ramp delete button in the
/// DE. Keying the cache on [`vk::renderer_epoch`] makes the first lookup after
/// a rebuild a miss, which re-rasterizes and re-uploads into the live
/// renderer.
///
/// The id cache itself has to stay: this is called from widget rebuilds, so
/// uploading per call would burn through the renderer's 256-image budget in
/// seconds. Caching only the decode — what [`icon::upload_themed`] does — is
/// right for a caller that uploads rarely and owns what it gets back, and
/// wrong here.
///
/// [`Button::new_icon`]: widget::Button::new_icon
pub fn upload_icon(name: &str, px: u32) -> Option<(u32, u32, u32)> {
    use std::collections::HashMap;
    use std::sync::Mutex;
    /// The cached ids, and the renderer epoch they were uploaded to.
    static CACHE: Mutex<Option<(u32, HashMap<(String, u32), Option<(u32, u32, u32)>>)>> =
        Mutex::new(None);
    let epoch = crate::vk::renderer_epoch();
    let key = (name.to_string(), px);
    let mut guard = CACHE.lock().unwrap();
    let (cached_epoch, cache) = guard.get_or_insert_with(|| (epoch, HashMap::new()));
    if *cached_epoch != epoch {
        // The renderer these ids named is gone, and its image table went with
        // it — so this is a forget, not a teardown; there is nothing to free.
        cache.clear();
        *cached_epoch = epoch;
    }
    if let Some(hit) = cache.get(&key) {
        return *hit;
    }
    let loaded = (|| {
        let path = format!("{}/{name}.svg", icons_dir());
        let data = std::fs::read(&path).ok()?;
        let (rgba, w, h) = rasterize_svg(&data, px)?;
        Some((crate::vk::upload_rgba(rgba, w, h), w, h))
    })();
    cache.insert(key, loaded);
    loaded
}

/// Rasterize SVG bytes at `px` on the longer side: straight (un-premultiplied)
/// RGBA8 pixels plus dimensions, ready for `vk::upload_rgba`. Text elements
/// resolve through the shared fontdb (a thread-safe static), so callers may
/// rasterize off the UI thread and upload later — cce-files' preview service
/// does. `None` when the data is unparsable.
pub fn rasterize_svg(data: &[u8], px: u32) -> Option<(Vec<u8>, u32, u32)> {
    let opt = resvg::usvg::Options::default();
    let fontdb = crate::widget::get_font_db();
    let tree = resvg::usvg::Tree::from_data(data, &opt, fontdb).ok()?;
    let size = tree.size();
    let (sw, sh) = (size.width().max(1.0), size.height().max(1.0));
    let scale = px as f32 / sw.max(sh);
    let w = (sw * scale).round().max(1.0) as u32;
    let h = (sh * scale).round().max(1.0) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    // tiny-skia pixels are premultiplied; the upload path takes straight RGBA.
    let mut rgba = pixmap.take();
    for p in rgba.chunks_exact_mut(4) {
        let a = p[3] as f32 / 255.0;
        if a > 0.0 {
            p[0] = ((p[0] as f32 / a).min(255.0)) as u8;
            p[1] = ((p[1] as f32 / a).min(255.0)) as u8;
            p[2] = ((p[2] as f32 / a).min(255.0)) as u8;
        }
    }
    Some((rgba, w, h))
}

/// Build a cosmic-text `FontSystem` loaded with the bundled CCE fonts (house style).
/// System fonts are loaded only if `$CCE_LOAD_SYSTEM_FONTS` is set. Configured
/// custom fonts are validated with a warning if missing.
pub fn create_font_system() -> cosmic_text::FontSystem {
    build_font_system(false)
}

/// A `FontSystem` the TOOLKIT owns, for widget GEOMETRY rather than drawing:
/// the shaping a widget's own selection, caret and click-to-index math needs on
/// a host that never hands one in.
///
/// Paint-walk apps shape through their own (`prepare_text`) and the display
/// list shapes through the runner's — but a flat-path host consumes
/// `all_quads`, so nothing ever shaped for the widgets it draws and `TextBox`
/// fell back to `measure_text_width("M")`: an SVG-rasterized INKED extent, not
/// an advance, which walks off the glyphs a few px per character.
///
/// Created on FIRST USE, so an app that shapes for itself never pays for it,
/// and from the same bundle [`create_font_system`] gives the renderer. The
/// shaped-buffer cache behind it is keyed by text/size/family and shared per
/// thread, so in practice this reads the very buffers the draw already built.
pub fn geometry_font_system() -> &'static std::sync::Mutex<cosmic_text::FontSystem> {
    static GEOMETRY_FONT_SYSTEM: std::sync::OnceLock<std::sync::Mutex<cosmic_text::FontSystem>> =
        std::sync::OnceLock::new();
    GEOMETRY_FONT_SYSTEM.get_or_init(|| std::sync::Mutex::new(create_font_system()))
}

/// Like [`create_font_system`] but always also loads installed system fonts, for
/// apps that must see every font on the system (e.g. the font picker) or want
/// them as fallbacks. Additive — bundled CCE fonts are still loaded.
pub fn create_font_system_with_system_fonts() -> cosmic_text::FontSystem {
    build_font_system(true)
}

/// Targeted script-fallback faces loaded alongside the bundled house fonts.
/// The bundled set covers Latin; anything else shaped to tofu unless
/// `$CCE_LOAD_SYSTEM_FONTS` pulled in the entire system set. Probing a short
/// list of well-known files keeps startup cheap while giving cosmic-text's
/// unix script fallback (family names "Noto Sans CJK *", "Noto Color Emoji")
/// real faces to land on. `$CCE_NO_FALLBACK_FONTS` opts out.
fn load_fallback_fonts(db: &mut cosmic_text::fontdb::Database) {
    if std::env::var("CCE_NO_FALLBACK_FONTS").is_ok() {
        return;
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let candidates = [
        // CJK (Arch noto-fonts-cjk; Debian/Fedora paths for good measure)
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc".to_string(),
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc".to_string(),
        "/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc".to_string(),
        // emoji (Arch noto-fonts-emoji; other distros; per-user install)
        "/usr/share/fonts/noto/NotoColorEmoji.ttf".to_string(),
        "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf".to_string(),
        "/usr/share/fonts/google-noto-emoji-color-fonts/NotoColorEmoji.ttf".to_string(),
        format!("{home}/.local/share/fonts/NotoColorEmoji.ttf"),
    ];
    for path in &candidates {
        if std::path::Path::new(path).exists() {
            let _ = db.load_font_file(path);
        }
    }
}

fn build_font_system(load_system_fonts: bool) -> cosmic_text::FontSystem {
    let mut db = cosmic_text::fontdb::Database::new();
    db.load_fonts_dir(fonts_dir());
    load_fallback_fonts(&mut db);
    if load_system_fonts || std::env::var("CCE_LOAD_SYSTEM_FONTS").is_ok() {
        db.load_system_fonts();
    }
    // An empty database guarantees a panic on the first shaped glyph
    // (cosmic-text: "no default font found"), so if the bundled dir yielded
    // nothing (missing $HOME/Dropbox/Fonts — e.g. the greeter running as
    // root), fall back to system fonts rather than crash.
    if db.faces().next().is_none() {
        db.load_system_fonts();
    }

    // Pin the generic families to faces that actually exist. fontdb's defaults
    // name Windows faces ("Arial"/"Times New Roman"), so Family::SansSerif /
    // Monospace never resolved here and every glyph of generic-family text
    // dropped into the per-glyph fallback chain — where Noto Color Emoji sits
    // high (cosmic-text common_fallback) and hijacked spaces and digits with
    // emoji metrics. Berkeley Mono is the house mono; Noto Sans CJK SC (the
    // targeted fallback face above) doubles as a full Latin sans.
    fn has_family(db: &cosmic_text::fontdb::Database, fam: &str) -> bool {
        db.faces()
            .any(|f| f.families.iter().any(|(n, _)| n == fam))
    }
    if has_family(&db, "Berkeley Mono") {
        db.set_monospace_family("Berkeley Mono");
    }
    if has_family(&db, "Noto Sans CJK SC") {
        db.set_sans_serif_family("Noto Sans CJK SC");
    }

    // Validate configured custom fonts
    let font_getters = vec![
        ("list_font", crate::layout::list_font_parsed().0),
        ("menubar_font", crate::layout::menubar_font_parsed().0),
        ("statusbar_font", crate::layout::statusbar_font_parsed().0),
        ("font_selector_font", crate::layout::font_selector_font_parsed().0),
        ("button_strip_font", crate::layout::button_strip_font_parsed().0),
        ("control_label_font", crate::layout::control_label_font_parsed().0),
        ("control_label_font_detached", crate::layout::control_label_font_detached_parsed().0),
        ("tree_font", crate::layout::tree_font_parsed().0),
        ("graph_font", crate::layout::graph_font_parsed().0),
        ("graph_node_font", crate::layout::graph_node_font_parsed().0),
    ];

    for (name, family) in font_getters {
        if !family.is_empty() && family != "Berkeley Mono" && family != "sans-serif" {
            let mut found = false;
            for face in db.faces() {
                for (fam, _) in &face.families {
                    if fam.to_lowercase() == family.to_lowercase() {
                        found = true;
                        break;
                    }
                }
                if found {
                    break;
                }
            }
            if !found {
                eprintln!(
                    "WARNING: Configured font family '{}' for property '{}' was not found in the fonts database. Falling back to default font.",
                    family, name
                );
            }
        }
    }

    cosmic_text::FontSystem::new_with_locale_and_db("en-US".to_string(), db)
}



/// CLAUDE.md's `(~Nk lines)` figures, checked against the files they describe.
///
/// Those numbers exist to set expectations before opening a file — "this is
/// the big one" — and they drift in total silence, because a stale number
/// reads exactly like a fresh one. A workspace sweep on 2026-09-19 found
/// EVERY size figure in every CLAUDE.md stale, all of them undercounts, the
/// worst by 49% (cce-designer's app.rs, written ~5.4k at 8046 lines).
///
/// Tolerance is 10%: loose enough that ordinary work does not trip it, tight
/// enough that a file cannot quietly double. When it fails, write the number
/// it reports — that is the whole fix.
///
/// Deliberately MIRRORED into each crate that carries such a figure rather
/// than shared from a helper: every crate here is its own git repository and
/// must build standalone, and this needs nothing but `std`. Same call
/// `ramp.rs` makes about its cce-ui parser.
#[cfg(test)]
mod doc_size_claims {
    use std::path::{Path, PathBuf};

    /// One `(~Nk lines)` claim: the name as CLAUDE.md spells it, the figure,
    /// and whether the claim also calls it the largest file.
    fn claims(doc: &str) -> Vec<(String, f64, bool)> {
        const TAIL: &str = " lines)";
        let mut out = Vec::new();
        let mut i = 0;
        while let Some(p) = doc[i..].find(TAIL) {
            let end = i + p;
            i = end + TAIL.len();
            let Some(open) = doc[..end].rfind('(') else { continue };
            let inner = &doc[open + 1..end];
            // "~8k", or "largest file, ~6.9k" — take the last word.
            let largest = inner.contains("largest file");
            let word = inner.rsplit([' ', ',']).next().unwrap_or("").trim();
            let digits = word.trim_start_matches('~');
            let value = match digits.strip_suffix('k') {
                Some(k) => k.parse::<f64>().ok().map(|v| v * 1000.0),
                None => digits.parse::<f64>().ok(),
            };
            // The backticked name immediately before the parenthetical.
            let before = &doc[..open];
            let name = before.rfind('`').and_then(|e| {
                before[..e].rfind('`').map(|s| before[s + 1..e].to_string())
            });
            if let (Some(v), Some(n)) = (value, name) {
                if v > 0.0 {
                    out.push((n, v, largest));
                }
            }
        }
        out
    }

    /// Every `.rs` file under `src/`, as (path, line count).
    fn sources(root: &Path) -> Vec<(PathBuf, usize)> {
        fn walk(dir: &Path, out: &mut Vec<(PathBuf, usize)>) {
            let Ok(entries) = std::fs::read_dir(dir) else { return };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().and_then(|x| x.to_str()) == Some("rs") {
                    if let Ok(s) = std::fs::read_to_string(&p) {
                        out.push((p, s.lines().count()));
                    }
                }
            }
        }
        let mut v = Vec::new();
        walk(&root.join("src"), &mut v);
        v
    }

    #[test]
    fn test_claude_md_line_counts_match_the_source() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let doc = std::fs::read_to_string(root.join("CLAUDE.md"))
            .expect("CLAUDE.md is missing next to Cargo.toml");
        let files = sources(root);
        let claims = claims(&doc);
        assert!(
            !claims.is_empty(),
            "no `(~N lines)` figure found in CLAUDE.md — either the syntax changed \
             and this scan needs updating, or the figures were removed and so \
             should this test"
        );

        let mut bad: Vec<String> = Vec::new();
        for (name, claimed, largest) in &claims {
            // A path relative to the crate root, else a unique basename.
            let hits: Vec<&(PathBuf, usize)> = if root.join(name).is_file() {
                files.iter().filter(|(p, _)| *p == root.join(name)).collect()
            } else {
                files
                    .iter()
                    .filter(|(p, _)| p.file_name().and_then(|x| x.to_str()) == Some(name.as_str()))
                    .collect()
            };
            let [(path, actual)] = hits[..] else {
                bad.push(format!("`{name}`: names {} files under src/, cannot check", hits.len()));
                continue;
            };
            let actual = *actual as f64;
            let drift = (actual - claimed) / claimed;
            if drift.abs() > 0.10 {
                bad.push(format!(
                    "`{name}` is documented as ~{} lines but has {} ({:+.0}%) — write ~{}",
                    round_k(*claimed),
                    actual as usize,
                    drift * 100.0,
                    round_k(actual)
                ));
            }
            if *largest {
                if let Some((big, n)) = files.iter().max_by_key(|(_, n)| *n) {
                    if big != path {
                        bad.push(format!(
                            "`{name}` is called the largest file, but {} has {n} lines",
                            big.strip_prefix(root).unwrap_or(big).display()
                        ));
                    }
                }
            }
        }
        assert!(bad.is_empty(), "CLAUDE.md size claims are stale:\n  {}", bad.join("\n  "));
    }

    /// "~8k" for 8046, "~3.1k" for 3134, "~950" for 950 — the spelling the
    /// docs already use, so the failure message can be pasted straight in.
    fn round_k(n: f64) -> String {
        if n < 1000.0 {
            return format!("{}", n.round() as usize);
        }
        let k = n / 1000.0;
        if (k - k.round()).abs() < 0.05 {
            format!("{}k", k.round() as usize)
        } else {
            format!("{k:.1}k")
        }
    }
}
