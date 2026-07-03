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

pub fn create_font_system() -> glyphon::FontSystem {
    let mut db = glyphon::cosmic_text::fontdb::Database::new();
    db.load_fonts_dir("/home/lsgalante/Dropbox/Fonts");
    if std::env::var("CCE_LOAD_SYSTEM_FONTS").is_ok() {
        db.load_system_fonts();
    }
    glyphon::FontSystem::new_with_locale_and_db("en-US".to_string(), db)
}


