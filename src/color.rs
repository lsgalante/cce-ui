pub const HEADER_BG: [f32; 4] = [0.08, 0.08, 0.12, 1.0];
pub const HEADER_ACCENT: [f32; 4] = [0.60, 0.40, 0.20, 1.0];
pub const SIDEBAR_BG: [f32; 4] = [0.10, 0.10, 0.13, 1.0];
pub const CONTENT_BG: [f32; 4] = [0.13, 0.13, 0.16, 0.2];
pub const PANEL_IDLE: [f32; 4] = [0.14, 0.70, 0.38, 1.0];
pub const PANEL_DRAG: [f32; 4] = [0.24, 0.85, 0.50, 1.0];
pub const NODE_IDLE: [f32; 4] = [0.10, 0.45, 0.70, 1.0];
pub const NODE_SELECTED: [f32; 4] = [0.20, 0.65, 0.90, 1.0];
pub const NODE_DRAG: [f32; 4] = [0.30, 0.80, 1.00, 1.0];
pub const BUTTON_IDLE: [f32; 4] = [0.20, 0.40, 0.65, 0.4];
pub const BUTTON_HOVER: [f32; 4] = [0.30, 0.52, 0.78, 0.6];
pub const BUTTON_PRESS: [f32; 4] = [0.12, 0.28, 0.50, 0.8];
pub const STATUS_BG: [f32; 4] = [0.06, 0.06, 0.10, 1.0];
pub const STATUS_ACCENT: [f32; 4] = [0.20, 0.20, 0.25, 1.0];
pub const RESET_BTN_IDLE: [f32; 4] = [0.55, 0.20, 0.20, 0.4];
pub const RESET_BTN_HOVER: [f32; 4] = [0.70, 0.30, 0.30, 0.6];
pub const RESET_BTN_PRESS: [f32; 4] = [0.40, 0.12, 0.12, 0.8];
pub const CHECKBOX_BG: [f32; 4] = [0.18, 0.18, 0.22, 1.0];
pub const CHECKBOX_CHECKED: [f32; 4] = [0.20, 0.50, 0.75, 1.0];
pub const CHECKBOX_HOVER: [f32; 4] = [0.25, 0.25, 0.30, 1.0];
pub const TOGGLE_OFF: [f32; 4] = [0.25, 0.25, 0.30, 1.0];
pub const TOGGLE_ON: [f32; 4] = [0.14, 0.70, 0.38, 1.0];
pub const TOGGLE_HOVER: [f32; 4] = [0.30, 0.30, 0.35, 1.0];
pub const SLIDER_TRACK: [f32; 4] = [0.18, 0.18, 0.22, 1.0];

use std::sync::RwLock;

static SLIDER_TRACK_COLOR: RwLock<[f32; 4]> = RwLock::new(SLIDER_TRACK);
static PAGE_LOW_COLOR: RwLock<[f32; 4]> = RwLock::new([0.0600316, 0.0600316, 0.080219, 1.0]);
static COLOR_BORDERS_COLOR: RwLock<[f32; 4]> = RwLock::new([0.2039, 0.2039, 0.2530, 1.0]);
static NODE_COLOR: RwLock<[f32; 4]> = RwLock::new(NODE_IDLE);
static SIDEBAR_BG_COLOR: RwLock<[f32; 4]> = RwLock::new(SIDEBAR_BG);
static HIGHLIGHT_PRIMARY_COLOR: RwLock<[f32; 4]> = RwLock::new(HIGHLIGHT_PRIMARY);
static MENUBAR_TAB_LABEL_COLOR: RwLock<[f32; 4]> = RwLock::new([0.90196, 0.90196, 0.94902, 1.0]); // sRGB [230, 230, 242] linear
static OPACITY: RwLock<Option<f32>> = RwLock::new(None);
static WINDOW_OPACITY: RwLock<Option<f32>> = RwLock::new(None);
static TOGGLE_ON_COLOR: RwLock<[f32; 4]> = RwLock::new(TOGGLE_ON);
static TOGGLE_OFF_COLOR: RwLock<[f32; 4]> = RwLock::new(TOGGLE_OFF);
static SCROLLINGLIST_BG_COLOR: RwLock<[f32; 4]> = RwLock::new([0.08, 0.08, 0.12, 0.3]);
static BREADCRUMB_BG_COLOR: RwLock<[f32; 4]> = RwLock::new([0.08, 0.08, 0.12, 1.0]);
static POPOVER_BG_COLOR: RwLock<[f32; 4]> = RwLock::new([0.08, 0.08, 0.12, 1.0]);
static PAGE_COLOR: RwLock<[f32; 4]> = RwLock::new([0.0, 0.0, 0.0, 0.0]);
static LAYER_COLOR: RwLock<[f32; 4]> = RwLock::new([0.0, 0.0, 0.0, 0.0]);



pub fn node_color() -> [f32; 4] {
    *NODE_COLOR.read().unwrap()
}

pub fn set_node_color(color: [f32; 4]) {
    if let Ok(mut lock) = NODE_COLOR.write() {
        *lock = color;
    }
}

pub fn node_selected_color() -> [f32; 4] {
    let base = node_color();
    [
        (base[0] + 0.10).min(1.0),
        (base[1] + 0.20).min(1.0),
        (base[2] + 0.20).min(1.0),
        base[3]
    ]
}

pub fn node_drag_color() -> [f32; 4] {
    let base = node_color();
    [
        (base[0] + 0.20).min(1.0),
        (base[1] + 0.35).min(1.0),
        (base[2] + 0.30).min(1.0),
        base[3]
    ]
}

fn read_config() -> Option<String> {
    let paths = [
        "/home/lsgalante/.config/cce/config.json",
        "/home/lsgalante/.config/ccec/config.json",
    ];
    for path in &paths {
        if let Ok(content) = std::fs::read_to_string(path) {
            return Some(content);
        }
    }
    None
}

fn parse_and_set_colors(content: &str) {
    let val: serde_json::Value = match serde_json::from_str(content) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("JSON parse error: {}", e);
            return;
        }
    };

    if let Some(opacity) = val.pointer("/transparency/opacity").and_then(|v| v.as_f64()) {
        if let Ok(mut lock) = OPACITY.write() {
            *lock = Some(opacity as f32);
        }
    }

    if let Some(w_opacity) = val.pointer("/surfaces/window_opacity").and_then(|v| v.as_f64()) {
        if let Ok(mut lock) = WINDOW_OPACITY.write() {
            *lock = Some(w_opacity as f32);
        }
    }

    let parse_hex = |hex_str: &str| -> Option<[f32; 4]> {
        let hex = hex_str.trim_matches(|c| c == '"' || c == '\'' || c == ' ');
        let hex = hex.trim_start_matches('#');
        if hex.len() >= 6 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                u8::from_str_radix(&hex[0..2], 16),
                u8::from_str_radix(&hex[2..4], 16),
                u8::from_str_radix(&hex[4..6], 16),
            ) {
                let r_f = srgb_to_linear(r as f32 / 255.0);
                let g_f = srgb_to_linear(g as f32 / 255.0);
                let b_f = srgb_to_linear(b as f32 / 255.0);
                return Some([r_f, g_f, b_f, 1.0]);
            }
        }
        None
    };

    let get_color = |pointer: &str| -> Option<[f32; 4]> {
        val.pointer(pointer).and_then(|v| v.as_str()).and_then(parse_hex)
    };

    if let Some(c) = get_color("/surfaces/window_color").or_else(|| get_color("/layout/page_low_color")) {
        if let Ok(mut lock) = PAGE_LOW_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/layout/color_borders_color") {
        if let Ok(mut lock) = COLOR_BORDERS_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/layout/slider_track_color") {
        if let Ok(mut lock) = SLIDER_TRACK_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/layout/paginator_sidebar_color") {
        if let Ok(mut lock) = SIDEBAR_BG_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/layout/primary_highlight_color") {
        if let Ok(mut lock) = HIGHLIGHT_PRIMARY_COLOR.write() { *lock = [c[0], c[1], c[2], 0.12]; }
    }
    if let Some(c) = get_color("/layout/menubar_tab_label_color").or_else(|| get_color("/layout/paginator_tab_label_color")) {
        if let Ok(mut lock) = MENUBAR_TAB_LABEL_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/layout/toggle_enabled_color") {
        if let Ok(mut lock) = TOGGLE_ON_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/layout/toggle_disabled_color") {
        if let Ok(mut lock) = TOGGLE_OFF_COLOR.write() { *lock = c; }
    }
    let parsed_scrollinglist_bg = get_color("/layout/scrollinglist_bg_color");
    let parsed_breadcrumb_bg = get_color("/layout/breadcrumb_bg_color");
    let parsed_popover_bg = get_color("/layout/popover_bg_color");

    if let Some(c) = get_color("/layout/page_color") {
        if let Ok(mut lock) = PAGE_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/layout/layer_color") {
        if let Ok(mut lock) = LAYER_COLOR.write() { *lock = c; }
    }

    if let Some(c) = parsed_scrollinglist_bg {
        if let Ok(mut lock) = SCROLLINGLIST_BG_COLOR.write() {
            *lock = [c[0], c[1], c[2], 0.3];
        }
    }
    if let Some(c) = parsed_breadcrumb_bg {
        if let Ok(mut lock) = BREADCRUMB_BG_COLOR.write() {
            *lock = [c[0], c[1], c[2], 1.0];
        }
    } else if let Some(c) = parsed_scrollinglist_bg {
        if let Ok(mut lock) = BREADCRUMB_BG_COLOR.write() {
            *lock = [c[0], c[1], c[2], 1.0];
        }
    }
    if let Some(c) = parsed_popover_bg {
        if let Ok(mut lock) = POPOVER_BG_COLOR.write() {
            *lock = [c[0], c[1], c[2], 1.0];
        }
    } else if let Some(c) = parsed_scrollinglist_bg {
        if let Ok(mut lock) = POPOVER_BG_COLOR.write() {
            *lock = [c[0], c[1], c[2], 1.0];
        }
    }
}

fn load_colors_once() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            parse_and_set_colors(&content);
        }
    });
}

pub fn reload_colors(content: &str) {
    if serde_json::from_str::<serde_json::Value>(content).is_ok() {
        parse_and_set_colors(content);
    } else if let Some(raw) = read_config() {
        parse_and_set_colors(&raw);
    }
}

pub fn page_low_color() -> [f32; 4] {
    load_colors_once();
    let mut color = *PAGE_LOW_COLOR.read().unwrap();
    if let Some(opacity) = read_window_opacity_if_configured() {
        color[3] = opacity;
    }
    color
}

pub fn set_page_low_color(color: [f32; 4]) {
    if let Ok(mut lock) = PAGE_LOW_COLOR.write() {
        *lock = color;
    }
}

pub fn page_color() -> [f32; 4] {
    load_colors_once();
    *PAGE_COLOR.read().unwrap()
}

pub fn set_page_color(color: [f32; 4]) {
    if let Ok(mut lock) = PAGE_COLOR.write() {
        *lock = color;
    }
}

pub fn layer_color() -> [f32; 4] {
    load_colors_once();
    *LAYER_COLOR.read().unwrap()
}

pub fn set_layer_color(color: [f32; 4]) {
    if let Ok(mut lock) = LAYER_COLOR.write() {
        *lock = color;
    }
}


pub fn color_borders_color() -> [f32; 4] {
    load_colors_once();
    *COLOR_BORDERS_COLOR.read().unwrap()
}

pub fn set_color_borders_color(color: [f32; 4]) {
    if let Ok(mut lock) = COLOR_BORDERS_COLOR.write() {
        *lock = color;
    }
}

pub fn slider_track() -> [f32; 4] {
    load_colors_once();
    *SLIDER_TRACK_COLOR.read().unwrap()
}

pub fn set_slider_track(color: [f32; 4]) {
    if let Ok(mut lock) = SLIDER_TRACK_COLOR.write() {
        *lock = color;
    }
}

pub const SLIDER_THUMB: [f32; 4] = [0.60, 0.60, 0.65, 1.0];
pub const SLIDER_THUMB_DRAG: [f32; 4] = [0.80, 0.80, 0.85, 1.0];
pub const PROGRESS_BG: [f32; 4] = [0.18, 0.18, 0.22, 1.0];
pub const PROGRESS_FILL: [f32; 4] = [0.20, 0.50, 0.75, 1.0];

pub const HIGHLIGHT_PRIMARY: [f32; 4] = [1.0, 1.0, 1.0, 0.12];
pub const HIGHLIGHT_SECONDARY: [f32; 4] = [1.0, 1.0, 1.0, 0.06];

pub const SPINBOX_BG: [f32; 4] = [0.18, 0.18, 0.22, 1.0];
pub const SPINBOX_BUTTON: [f32; 4] = [0.25, 0.25, 0.32, 1.0];
pub const SPINBOX_BUTTON_HOVER: [f32; 4] = [0.35, 0.35, 0.42, 1.0];
pub const SPINBOX_DISPLAY: [f32; 4] = [0.12, 0.12, 0.16, 1.0];

pub const CANVAS_BG: [f32; 4] = [0.05, 0.05, 0.10, 1.0];
pub const VIEWPORT_BG: [f32; 4] = [0.0, 0.0, 0.0, 0.0];
pub const PARAM_BG: [f32; 4] = [0.10, 0.10, 0.14, 0.25];

pub const PANEL_MENU_BG: [f32; 4] = [0.08, 0.08, 0.12, 1.0];
pub const PANEL_MENU_HOVER: [f32; 4] = [0.18, 0.18, 0.25, 1.0];
pub const PANEL_MENU_FOCUSED: [f32; 4] = [0.08, 0.16, 0.28, 1.0];

pub const SPLITTER_IDLE: [f32; 4] = [0.20, 0.20, 0.27, 1.0];
pub const SPLITTER_HOVER: [f32; 4] = [0.40, 0.40, 0.50, 1.0];
pub const SPLITTER_DRAG: [f32; 4] = [0.50, 0.50, 0.60, 1.0];

pub const TEXT_FG: [f32; 4] = [0.80, 0.80, 0.85, 1.0];
pub const TEXT_DIM: [f32; 4] = [0.53, 0.53, 0.60, 1.0];
pub const TEXT_HEADER: [f32; 4] = [0.90, 0.90, 0.95, 1.0];
pub const TEXT_ACCENT: [f32; 4] = [0.56, 0.83, 0.56, 1.0];

pub fn srgb_to_linear(c: f32) -> f32 {
    c.powf(2.2)
}

pub fn linear_to_srgb(c: f32) -> f32 {
    c.powf(1.0 / 2.2)
}

pub fn to_linear(color: [f32; 4]) -> [f32; 4] {
    [
        srgb_to_linear(color[0]),
        srgb_to_linear(color[1]),
        srgb_to_linear(color[2]),
        color[3],
    ]
}

pub fn to_srgb(color: [f32; 4]) -> [f32; 4] {
    [
        linear_to_srgb(color[0]),
        linear_to_srgb(color[1]),
        linear_to_srgb(color[2]),
        color[3],
    ]
}

pub fn sidebar_bg_color() -> [f32; 4] {
    load_colors_once();
    let mut color = *SIDEBAR_BG_COLOR.read().unwrap();
    if let Some(opacity) = read_opacity_if_configured() {
        color[3] = opacity;
    }
    color
}

pub fn set_sidebar_bg_color(color: [f32; 4]) {
    if let Ok(mut lock) = SIDEBAR_BG_COLOR.write() {
        *lock = color;
    }
}

pub fn highlight_primary_color() -> [f32; 4] {
    load_colors_once();
    *HIGHLIGHT_PRIMARY_COLOR.read().unwrap()
}

pub fn set_highlight_primary_color(color: [f32; 4]) {
    if let Ok(mut lock) = HIGHLIGHT_PRIMARY_COLOR.write() {
        *lock = [color[0], color[1], color[2], 0.12];
    }
}

pub fn menubar_tab_label_color() -> [f32; 4] {
    load_colors_once();
    *MENUBAR_TAB_LABEL_COLOR.read().unwrap()
}

pub fn set_menubar_tab_label_color(color: [f32; 4]) {
    if let Ok(mut lock) = MENUBAR_TAB_LABEL_COLOR.write() {
        *lock = color;
    }
}

pub fn read_opacity_if_configured() -> Option<f32> {
    load_colors_once();
    *OPACITY.read().unwrap()
}

pub fn read_window_opacity_if_configured() -> Option<f32> {
    load_colors_once();
    *WINDOW_OPACITY.read().unwrap()
}

pub fn toggle_on_color() -> [f32; 4] {
    load_colors_once();
    *TOGGLE_ON_COLOR.read().unwrap()
}

pub fn set_toggle_on_color(color: [f32; 4]) {
    if let Ok(mut lock) = TOGGLE_ON_COLOR.write() {
        *lock = color;
    }
}

pub fn toggle_off_color() -> [f32; 4] {
    load_colors_once();
    *TOGGLE_OFF_COLOR.read().unwrap()
}

pub fn set_toggle_off_color(color: [f32; 4]) {
    if let Ok(mut lock) = TOGGLE_OFF_COLOR.write() {
        *lock = color;
    }
}

pub fn scrollinglist_bg_color() -> [f32; 4] {
    load_colors_once();
    *SCROLLINGLIST_BG_COLOR.read().unwrap()
}

pub fn set_scrollinglist_bg_color(color: [f32; 4]) {
    if let Ok(mut lock) = SCROLLINGLIST_BG_COLOR.write() {
        *lock = [color[0], color[1], color[2], 0.3];
    }
}

pub fn breadcrumb_bg_color() -> [f32; 4] {
    load_colors_once();
    *BREADCRUMB_BG_COLOR.read().unwrap()
}

pub fn set_breadcrumb_bg_color(color: [f32; 4]) {
    if let Ok(mut lock) = BREADCRUMB_BG_COLOR.write() {
        *lock = [color[0], color[1], color[2], 1.0];
    }
}

pub fn popover_bg_color() -> [f32; 4] {
    load_colors_once();
    *POPOVER_BG_COLOR.read().unwrap()
}

pub fn set_popover_bg_color(color: [f32; 4]) {
    if let Ok(mut lock) = POPOVER_BG_COLOR.write() {
        *lock = [color[0], color[1], color[2], 1.0];
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub surface_bg: [f32; 4],
    pub surface_border: [f32; 4],
    pub primary_accent: [f32; 4],
    pub press_overlay: [f32; 4],
    pub hover_overlay: [f32; 4],
}

pub fn active_theme() -> Theme {
    Theme {
        surface_bg: popover_bg_color(),
        surface_border: [0.25, 0.25, 0.35, 0.8],
        primary_accent: [0.20, 0.50, 0.75, 1.0],
        press_overlay: [1.0, 1.0, 1.0, 0.15],
        hover_overlay: [1.0, 1.0, 1.0, 0.08],
    }
}
