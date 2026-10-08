// Modules under `cfg(not(target_arch = "wasm32"))` are the native renderer
// and services (Vulkan, the compositor IPC, file dialogs); those under
// `cfg(not(any(target_arch = "wasm32", target_os = "macos")))` are the
// Wayland shell's, which macOS replaces with `mac` (AppKit). Everything else
// builds for the browser too: `scripts/check-wasm` is the check, and
// `scripts/check-mac` the macOS one.
pub mod color;
pub mod compute;
pub mod widget;
pub use cce_core::config;
pub use cce_core::input;
pub mod history;
pub mod ime;
pub mod layout;
pub use cce_core::relief_spec;
#[cfg(not(any(target_arch = "wasm32", target_os = "macos")))]
pub mod wayland;
#[cfg(not(any(target_arch = "wasm32", target_os = "macos")))]
pub mod protocol;
pub mod engine;
pub mod scale;
pub use cce_core::units;
pub mod backend;
pub mod context;
pub mod draw;
pub mod scene;
#[cfg(not(target_arch = "wasm32"))]
pub mod file_dialog;
pub mod icon;
#[cfg(not(target_arch = "wasm32"))]
pub use cce_core::ipc;
#[cfg(not(any(target_arch = "wasm32", target_os = "macos")))]
pub mod mcp;
pub use cce_core::motion;
pub mod text_input;
#[cfg(not(target_arch = "wasm32"))]
pub mod vk;
#[cfg(target_arch = "wasm32")]
pub mod web;
#[cfg(target_os = "macos")]
pub mod mac;

#[cfg(test)]
mod config_style_tests;

pub mod colors {
    pub use crate::color::*;
}

/// The text-shaping library, re-exported so clients need no text dependency of
/// their own: `cce_ui::cosmic_text::FontSystem` rather than a per-crate
/// `cosmic-text` (previously `glyphon`) entry in every client's Cargo.toml.
/// Re-exporting also keeps every client on the one version cce-ui shapes with —
/// a `FontSystem` handed across the boundary must be the same type.
pub use cosmic_text;



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
    upload_icon_as(name, px, None)
}

/// [`upload_icon`] in a colour: the glyph's pixels multiplied by `rgb`
/// (raw sRGB, as a [`TextLabel`](widget::display::TextLabel)'s colour is,
/// so a glyph tinted with a label's colour matches the label). The artwork
/// is white, so multiplying IS tinting, and what a glyph shades darker (a
/// knocked-out mark) stays proportionally darker. Cached per
/// `(name, px, rgb)` like [`upload_icon`], and re-uploaded after a renderer
/// rebuild the same way.
///
/// A `PaintCtx::image` carries alpha and no colour, which is why a colour
/// has to be baked into the texture; [`icon_tint`] is the colour a
/// toolkit `[f32; 4]` becomes. The `weather-*` glyphs carry colours of
/// their own and are drawn with [`upload_icon`].
pub fn upload_icon_tinted(name: &str, px: u32, rgb: [u8; 3]) -> Option<(u32, u32, u32)> {
    upload_icon_as(name, px, Some(rgb))
}

/// The `[u8; 3]` tint a toolkit colour becomes — the conversion a
/// `TextLabel` applies to its own colour, so glyph and text match.
pub fn icon_tint(color: [f32; 4]) -> [u8; 3] {
    [
        (color[0] * 255.0).round().clamp(0.0, 255.0) as u8,
        (color[1] * 255.0).round().clamp(0.0, 255.0) as u8,
        (color[2] * 255.0).round().clamp(0.0, 255.0) as u8,
    ]
}

/// A bundled glyph rasterized at `px`, tinted when `tint` is given: the
/// pixels [`upload_icon`] and [`upload_icon_tinted`] upload, for a caller
/// that uploads them itself (a renderer with images of its own).
pub fn icon_pixels(name: &str, px: u32, tint: Option<[u8; 3]>) -> Option<(Vec<u8>, u32, u32)> {
    let path = format!("{}/{name}.svg", icons_dir());
    let data = std::fs::read(&path).ok()?;
    let (mut rgba, w, h) = rasterize_svg(&data, px)?;
    if let Some(rgb) = tint {
        for p in rgba.chunks_exact_mut(4) {
            for (c, &t) in p[..3].iter_mut().zip(rgb.iter()) {
                *c = ((*c as u16 * t as u16 + 127) / 255) as u8;
            }
        }
    }
    Some((rgba, w, h))
}

/// What a bundled glyph's image id holds: `(name, px, tint)`.
type IconSource = (String, u32, Option<[u8; 3]>);

/// Every id [`upload_icon`] and [`upload_icon_tinted`] have handed out, and
/// the glyph it holds — see [`icon_source`].
static ICON_SOURCES: std::sync::Mutex<Option<std::collections::HashMap<u32, IconSource>>> =
    std::sync::Mutex::new(None);

/// The glyph an image id from [`upload_icon`] / [`upload_icon_tinted`]
/// holds, or `None` for any other image. What lets a renderer that keeps
/// images of its own (the context menu's popup) draw the same glyph from a
/// display list painted with the window's ids.
pub fn icon_source(id: u32) -> Option<IconSource> {
    ICON_SOURCES.lock().unwrap().as_ref()?.get(&id).cloned()
}

fn upload_icon_as(name: &str, px: u32, tint: Option<[u8; 3]>) -> Option<(u32, u32, u32)> {
    use std::collections::HashMap;
    use std::sync::Mutex;
    /// The cached ids, and the renderer epoch they were uploaded to.
    static CACHE: Mutex<Option<(u32, HashMap<IconSource, Option<(u32, u32, u32)>>)>> =
        Mutex::new(None);
    let epoch = crate::draw::renderer_epoch();
    let key = (name.to_string(), px, tint);
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
    let loaded = icon_pixels(name, px, tint).map(|(rgba, w, h)| {
        let id = crate::draw::upload_rgba(rgba, w, h);
        ICON_SOURCES.lock().unwrap().get_or_insert_with(HashMap::new).insert(id, key.clone());
        (id, w, h)
    });
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
#[cfg(not(target_arch = "wasm32"))]
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

/// Pin the generic families to faces that actually exist. fontdb's defaults
/// name Windows faces ("Arial"/"Times New Roman"), so Family::SansSerif /
/// Monospace never resolved here and every glyph of generic-family text
/// dropped into the per-glyph fallback chain — where Noto Color Emoji sits
/// high (cosmic-text common_fallback) and hijacked spaces and digits with
/// emoji metrics. Berkeley Mono is the house mono; Noto Sans CJK SC (the
/// targeted fallback face) doubles as a full Latin sans. Shared by every
/// shell's font system, the browser's included, so a font set resolves the
/// same families wherever it is loaded.
fn pin_generic_families(db: &mut cosmic_text::fontdb::Database) {
    fn has_family(db: &cosmic_text::fontdb::Database, fam: &str) -> bool {
        db.faces()
            .any(|f| f.families.iter().any(|(n, _)| n == fam))
    }
    if has_family(db, "Berkeley Mono") {
        db.set_monospace_family("Berkeley Mono");
    }
    if has_family(db, "Noto Sans CJK SC") {
        db.set_sans_serif_family("Noto Sans CJK SC");
    }
}

fn build_font_system(load_system_fonts: bool) -> cosmic_text::FontSystem {
    let mut db = cosmic_text::fontdb::Database::new();
    #[cfg(not(target_arch = "wasm32"))]
    {
        db.load_fonts_dir(fonts_dir());
        load_fallback_fonts(&mut db);
        // On macOS the system set is always loaded: it is a curated one,
        // and what cosmic-text's fallback list there names ("Apple Color
        // Emoji", "PingFang SC", …) — where on Linux the probe above finds
        // the Noto faces in its place.
        if load_system_fonts || cfg!(target_os = "macos") || std::env::var("CCE_LOAD_SYSTEM_FONTS").is_ok() {
            db.load_system_fonts();
        }
        // An empty database guarantees a panic on the first shaped glyph
        // (cosmic-text: "no default font found"), so if the bundled dir yielded
        // nothing (missing $HOME/Dropbox/Fonts — e.g. the greeter running as
        // root), fall back to system fonts rather than crash.
        if db.faces().next().is_none() {
            db.load_system_fonts();
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = load_system_fonts;
        page_fonts::load_into(&mut db);
    }

    pin_generic_families(&mut db);

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

    #[cfg(target_arch = "wasm32")]
    page_fonts::stand_in_for_missing(&mut db);

    cosmic_text::FontSystem::new_with_locale_and_db("en-US".to_string(), db)
}

/// The fonts a page hands the browser shell (`web::run`). A page has no font
/// directory and no fontconfig, so on wasm every font database the toolkit
/// builds — the shell's, its own for widget geometry
/// ([`geometry_font_system`]) and the text-measurement one
/// (`widget::input::get_font_db`) — is loaded from these, one shared copy of
/// each file, with the generic families the page named.
#[cfg(target_arch = "wasm32")]
pub(crate) mod page_fonts {
    use std::sync::{Arc, Mutex};

    use cosmic_text::fontdb::{Database, Language};

    type Font = Arc<dyn AsRef<[u8]> + Send + Sync>;

    struct Provided {
        files: Vec<Font>,
        serif: Option<String>,
        sans_serif: Option<String>,
        monospace: Option<String>,
    }

    static PROVIDED: Mutex<Provided> =
        Mutex::new(Provided { files: Vec::new(), serif: None, sans_serif: None, monospace: None });

    /// What every font database built from now on loads: `files` (each a
    /// font file's bytes), in order, and the families the generic ones name —
    /// a fontconfig's answer, which on Linux `load_system_fonts` reads.
    pub(crate) fn provide(files: Vec<Vec<u8>>, serif: Option<String>, sans_serif: Option<String>, monospace: Option<String>) {
        let mut p = PROVIDED.lock().unwrap();
        p.files.extend(files.into_iter().map(|f| Arc::new(f) as Font));
        p.serif = serif.or(p.serif.take());
        p.sans_serif = sans_serif.or(p.sans_serif.take());
        p.monospace = monospace.or(p.monospace.take());
    }

    /// Load the page's fonts into `db`, as `load_system_fonts` loads the
    /// system's: the files, then the generic families.
    pub(crate) fn load_into(db: &mut Database) {
        let p = PROVIDED.lock().unwrap();
        for font in &p.files {
            db.load_font_source(cosmic_text::fontdb::Source::Binary(font.clone()));
        }
        if let Some(f) = &p.serif {
            db.set_serif_family(f.clone());
        }
        if let Some(f) = &p.sans_serif {
            db.set_sans_serif_family(f.clone());
        }
        if let Some(f) = &p.monospace {
            db.set_monospace_family(f.clone());
        }
    }

    fn has(db: &Database, family: &str) -> bool {
        db.faces().any(|f| f.families.iter().any(|(n, _)| n.eq_ignore_ascii_case(family)))
    }

    /// cosmic-text falls back from a family the database lacks through a
    /// list of well-known families per OS — on Linux "Noto Sans", then
    /// "DejaVu Sans", "FreeSans", … (its `fallback/unix.rs`) — and has no
    /// list at all on wasm, where such text lands on whichever face happens
    /// to come first (Noto Color Emoji, in a set loaded as Linux loads it).
    /// So here the families the toolkit itself names — the configured
    /// fonts, and the house default "Berkeley Mono" — are given, when absent,
    /// the faces of the first family of that Linux list the page provided,
    /// and the generic sans and serif families likewise unless the page named
    /// them; "monospace" (the name `fc-match` would have resolved) is given
    /// the first monospaced one.
    /// Text then resolves to the face it would on a Linux box with the same
    /// fonts. A family an app names itself must be among the page's fonts.
    pub(crate) fn stand_in_for_missing(db: &mut Database) {
        const SANS: [&str; 6] = ["Noto Sans", "DejaVu Sans", "FreeSans", "Noto Sans Mono", "DejaVu Sans Mono", "FreeMono"];
        const MONO: [&str; 4] = ["Noto Sans Mono", "DejaVu Sans Mono", "FreeMono", "Liberation Mono"];
        let first = |db: &Database, list: &[&str]| list.iter().find(|f| has(db, f)).map(|f| f.to_string());
        let alias = |db: &mut Database, name: &str, to: &str| {
            if name.is_empty() || has(db, name) {
                return;
            }
            let faces: Vec<_> =
                db.faces().filter(|f| f.families.iter().any(|(n, _)| n == to)).cloned().collect();
            for mut face in faces {
                face.families = vec![(name.to_string(), Language::English_UnitedStates)];
                db.push_face_info(face);
            }
        };
        if let Some(sans) = first(db, &SANS) {
            let mut names: Vec<String> = vec!["Berkeley Mono".into()];
            names.extend(
                [
                    crate::layout::list_font_parsed().0,
                    crate::layout::menubar_font_parsed().0,
                    crate::layout::statusbar_font_parsed().0,
                    crate::layout::font_selector_font_parsed().0,
                    crate::layout::button_strip_font_parsed().0,
                    crate::layout::control_label_font_parsed().0,
                    crate::layout::control_label_font_detached_parsed().0,
                    crate::layout::tree_font_parsed().0,
                    crate::layout::graph_font_parsed().0,
                    crate::layout::graph_node_font_parsed().0,
                ]
                .into_iter()
                .filter(|f| !matches!(f.as_str(), "sans-serif" | "serif" | "monospace")),
            );
            for name in &names {
                alias(db, name, &sans);
            }
            for generic in [cosmic_text::Family::SansSerif, cosmic_text::Family::Serif] {
                if !has(db, db.family_name(&generic)) {
                    match generic {
                        cosmic_text::Family::SansSerif => db.set_sans_serif_family(sans.clone()),
                        _ => db.set_serif_family(sans.clone()),
                    }
                }
            }
        }
        if let Some(mono) = first(db, &MONO) {
            alias(db, "monospace", &mono);
        }
    }
}
