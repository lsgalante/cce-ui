//! Fonts: every configured font getter and setter (parsed fonts cached against their string),
//! and the preferred-font family resolution the cosmic-text path uses.

use super::*;

pub fn color_selector_font() -> String {
    registry_string("color_selector_font").filter(|f| !f.is_empty()).unwrap_or_else(|| "monospace".to_string())
}

pub fn set_color_selector_font(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("color_selector_font", font.to_string());
    }
}

pub fn menubar_font() -> String {
    registry_string("menubar_font").filter(|f| !f.is_empty()).unwrap_or_else(|| "Berkeley Mono".to_string())
}

pub fn menubar_font_parsed() -> (String, f32) {
    parsed_font(&MENUBAR_FONT_CACHED, menubar_font())
}

pub fn set_menubar_font(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("menubar_font", font.to_string());
    }
}

pub fn statusbar_font() -> String {
    registry_string("statusbar_font").filter(|f| !f.is_empty()).unwrap_or_else(|| "Berkeley Mono".to_string())
}

pub fn statusbar_font_parsed() -> (String, f32) {
    parsed_font(&STATUSBAR_FONT_CACHED, statusbar_font())
}

pub fn set_statusbar_font(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("statusbar_font", font.to_string());
    }
}

pub fn font_selector_font() -> String {
    registry_string("font_selector_font").filter(|f| !f.is_empty()).unwrap_or_else(|| "Berkeley Mono".to_string())
}

pub fn font_selector_font_parsed() -> (String, f32) {
    parsed_font(&FONT_SELECTOR_FONT_CACHED, font_selector_font())
}

pub fn set_font_selector_font(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("font_selector_font", font.to_string());
    }
}

// Button Strip Font
pub fn button_strip_font() -> String {
    registry_string("button_strip_font").filter(|f| !f.is_empty()).unwrap_or_else(|| "Berkeley Mono".to_string())
}

pub fn button_strip_font_parsed() -> (String, f32) {
    parsed_font(&BUTTON_STRIP_FONT_CACHED, button_strip_font())
}

pub fn set_button_strip_font(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("button_strip_font", font.to_string());
    }
}

// Control Label Font
pub fn control_label_font() -> String {
    registry_string("control_label_font").filter(|f| !f.is_empty()).unwrap_or_else(|| "Berkeley Mono".to_string())
}

pub fn control_label_font_parsed() -> (String, f32) {
    parsed_font(&CONTROL_LABEL_FONT_CACHED, control_label_font())
}

pub fn set_control_label_font(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("control_label_font", font.to_string());
    }
}

// Control Label Font Detached
pub fn control_label_font_detached() -> String {
    registry_string("control_label_font_detached").filter(|f| !f.is_empty()).unwrap_or_else(|| "Berkeley Mono".to_string())
}

pub fn control_label_font_detached_parsed() -> (String, f32) {
    parsed_font(&CONTROL_LABEL_FONT_DETACHED_CACHED, control_label_font_detached())
}

pub fn set_control_label_font_detached(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("control_label_font_detached", font.to_string());
    }
}

// List Font
pub fn list_font() -> String {
    registry_string("list_font").filter(|f| !f.is_empty()).unwrap_or_else(|| "Berkeley Mono".to_string())
}

pub fn list_font_parsed() -> (String, f32) {
    parsed_font(&LIST_FONT_CACHED, list_font())
}

pub fn set_list_font(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("list_font", font.to_string());
    }
}

// Tree Font
pub fn tree_font() -> String {
    registry_string("tree_font").filter(|f| !f.is_empty()).unwrap_or_else(|| "Berkeley Mono".to_string())
}

pub fn tree_font_parsed() -> (String, f32) {
    parsed_font(&TREE_FONT_CACHED, tree_font())
}

pub fn set_tree_font(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("tree_font", font.to_string());
    }
}

// Graph Font
pub fn graph_font() -> String {
    registry_string("graph_font").filter(|f| !f.is_empty()).unwrap_or_else(|| "Berkeley Mono".to_string())
}

pub fn graph_font_parsed() -> (String, f32) {
    parsed_font(&GRAPH_FONT_CACHED, graph_font())
}

pub fn set_graph_font(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("graph_font", font.to_string());
    }
}

// Graph Node Font
pub fn graph_node_font() -> String {
    registry_string("graph_node_font").filter(|f| !f.is_empty()).unwrap_or_else(|| "Berkeley Mono".to_string())
}

pub fn graph_node_font_parsed() -> (String, f32) {
    parsed_font(&GRAPH_NODE_FONT_CACHED, graph_node_font())
}

pub fn set_graph_node_font(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("graph_node_font", font.to_string());
    }
}

// List Justification
pub fn list_justification() -> u8 {
    registry_float("list_justification").map(|v| v as u8).unwrap_or(0)
}

pub fn set_list_justification(just: u8) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("list_justification", just as f32);
    }
}

pub fn section_label_font() -> String {
    registry_string("section_label_font").filter(|f| !f.is_empty()).unwrap_or_else(|| "Berkeley Mono".to_string())
}

pub fn set_section_label_font(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("section_label_font", font.to_string());
    }
}

pub fn nested_section_label_font() -> String {
    registry_string("nested_section_label_font").filter(|f| !f.is_empty()).unwrap_or_else(|| "Berkeley Mono".to_string())
}

pub fn set_nested_section_label_font(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("nested_section_label_font", font.to_string());
    }
}

pub fn breadcrumb_font() -> String {
    registry_string("breadcrumb_font").filter(|f| !f.is_empty()).unwrap_or_else(|| "Berkeley Mono".to_string())
}

pub fn set_breadcrumb_font(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("breadcrumb_font", font.to_string());
    }
}

pub fn button_font() -> String {
    registry_string("button_font").filter(|f| !f.is_empty()).unwrap_or_else(|| "Berkeley Mono".to_string())
}

pub fn set_button_font(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("button_font", font.to_string());
    }
}

/// The DE's font families, from the shared config's `fonts { }` block:
/// `(sans_serif, serif, monospace, terminal)`.
///
/// These used to live in `~/.config/fontconfig/fonts.conf`, read back out of
/// fontconfig's XML by alias. Three of the seven aliases it carried
/// (`window-borders`, `status-interface`, `fuzzel`) were cce inventions
/// squatting in fontconfig's family namespace, and by the end none of them was
/// read by anything: window borders lost their text when titlebars went away,
/// the status bar moved to `module { font }` / `/style/status/font` in KDL and
/// only ever consulted the alias as a last-resort fallback, and fuzzel was
/// replaced by cce-cloud. The settings app's Fonts page — which edited that
/// file, rewriting it wholesale and preserving only its `<dir>` lines — went
/// with them.
///
/// fonts.conf is still fontconfig's file and still governs GTK/Electron apps;
/// cce simply no longer reads or writes it. Per-app overrides work here because
/// this reads the merged config, the same way the status bar's font does.
pub fn read_preferred_fonts() -> (String, String, String, String) {
    let get = |key: &str, fallback: &str| {
        crate::config::get_string(&format!("/fonts/{key}"))
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| fallback.to_string())
    };
    (
        get("sans_serif", "Noto Sans"),
        get("serif", "Noto Serif"),
        get("monospace", "Noto Sans Mono"),
        get("terminal", "Noto Sans Mono"),
    )
}
