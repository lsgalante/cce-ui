pub mod color;
pub mod widget;
pub mod config;
pub mod input;
pub mod layout;
pub mod wayland;
pub mod protocol;
pub mod engine;
pub mod scale;
pub mod backend;
pub mod context;
pub mod scene;
pub mod process;
pub mod file_dialog;
pub mod ipc;
pub mod mcp;
pub mod vk;

pub mod colors {
    pub use crate::color::*;
}


pub static IS_VERTICAL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub static BAR_THICKNESS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(24);

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

/// Build a glyphon `FontSystem` loaded with the bundled CCE fonts (house style).
/// System fonts are loaded only if `$CCE_LOAD_SYSTEM_FONTS` is set. Configured
/// custom fonts are validated with a warning if missing.
pub fn create_font_system() -> glyphon::FontSystem {
    build_font_system(false)
}

/// Like [`create_font_system`] but always also loads installed system fonts, for
/// apps that must see every font on the system (e.g. the font picker) or want
/// them as fallbacks. Additive — bundled CCE fonts are still loaded.
pub fn create_font_system_with_system_fonts() -> glyphon::FontSystem {
    build_font_system(true)
}

/// Targeted script-fallback faces loaded alongside the bundled house fonts.
/// The bundled set covers Latin; anything else shaped to tofu unless
/// `$CCE_LOAD_SYSTEM_FONTS` pulled in the entire system set. Probing a short
/// list of well-known files keeps startup cheap while giving cosmic-text's
/// unix script fallback (family names "Noto Sans CJK *", "Noto Color Emoji")
/// real faces to land on. `$CCE_NO_FALLBACK_FONTS` opts out.
fn load_fallback_fonts(db: &mut glyphon::cosmic_text::fontdb::Database) {
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

fn build_font_system(load_system_fonts: bool) -> glyphon::FontSystem {
    let mut db = glyphon::cosmic_text::fontdb::Database::new();
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
    fn has_family(db: &glyphon::cosmic_text::fontdb::Database, fam: &str) -> bool {
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

    glyphon::FontSystem::new_with_locale_and_db("en-US".to_string(), db)
}


