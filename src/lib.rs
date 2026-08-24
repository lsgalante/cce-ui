pub mod color;
pub mod widget;
pub mod config;
pub mod input;
pub mod layout;
pub mod relief_spec;
pub mod wayland;
pub mod protocol;
pub mod engine;
pub mod scale;
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
/// `~/Dropbox/cce/cce-icons/svg`.
pub fn icons_dir() -> String {
    std::env::var("CCE_ICONS_DIR").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{home}/Dropbox/cce/cce-icons/svg")
    })
}

/// Rasterize a bundled cce-icons SVG (`<name>.svg` under [`icons_dir`]) at
/// `px` on its longer side and upload it as a renderer texture. Returns
/// `(image id, pixel w, pixel h)` for `PaintCtx::image` / `ImageView`; cached
/// per `(name, px)` so widget rebuilds reuse the one upload. `None` when the
/// icon is missing or unparsable (callers keep a text fallback).
pub fn upload_icon(name: &str, px: u32) -> Option<(u32, u32, u32)> {
    use std::collections::HashMap;
    use std::sync::Mutex;
    static CACHE: Mutex<Option<HashMap<(String, u32), Option<(u32, u32, u32)>>>> =
        Mutex::new(None);
    let key = (name.to_string(), px);
    let mut guard = CACHE.lock().unwrap();
    let cache = guard.get_or_insert_with(HashMap::new);
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


