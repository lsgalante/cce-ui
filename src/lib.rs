pub mod color;
pub mod widget;
pub mod config;
pub mod layout;
pub mod wayland;
pub mod protocol;
pub mod engine;
pub mod scale;
pub mod backend;
pub mod context;
pub mod process;
pub mod file_dialog;

pub mod colors {
    pub use crate::color::*;
}

pub const SHADER: &str = include_str!("shader.wgsl");

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

pub fn create_font_system() -> glyphon::FontSystem {
    let mut db = glyphon::cosmic_text::fontdb::Database::new();
    db.load_fonts_dir(fonts_dir());
    if std::env::var("CCE_LOAD_SYSTEM_FONTS").is_ok() {
        db.load_system_fonts();
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
        if !family.is_empty() && family != "Outfit" && family != "sans-serif" {
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


