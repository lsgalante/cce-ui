//! Regression test: a missing bundled-fonts dir must not produce an empty font
//! database — cosmic-text panics with "no default font found" on the first
//! shaped glyph (this is what crash-looped the cce-display-manager greeter,
//! which runs as root with no $HOME/Dropbox/Fonts).

#[test]
fn empty_bundled_dir_falls_back_to_system_fonts() {
    std::env::set_var("CCE_FONTS_DIR", "/nonexistent-cce-fonts");
    std::env::remove_var("CCE_LOAD_SYSTEM_FONTS");
    let fs = cce_ui::create_font_system();
    assert!(
        fs.db().faces().next().is_some(),
        "font database is empty — system-font fallback did not kick in"
    );
}
