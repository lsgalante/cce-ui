pub const HEADER_BG: [f32; 4] = [0.08, 0.08, 0.12, 1.0];
pub const HEADER_ACCENT: [f32; 4] = [0.60, 0.40, 0.20, 1.0];
pub const SIDEBAR_BG: [f32; 4] = [0.10, 0.10, 0.13, 1.0];
pub const CONTENT_BG: [f32; 4] = [0.13, 0.13, 0.16, 0.2];
pub const PANEL_IDLE: [f32; 4] = [0.14, 0.70, 0.38, 1.0];
pub const PANEL_DRAG: [f32; 4] = [0.24, 0.85, 0.50, 1.0];
pub const NODE_IDLE: [f32; 4] = [0.10, 0.45, 0.70, 1.0];
pub const NODE_SELECTED: [f32; 4] = [0.20, 0.65, 0.90, 1.0];
pub const NODE_DRAG: [f32; 4] = [0.30, 0.80, 1.00, 1.0];
pub const BUTTON_IDLE: [f32; 4] = [0.20, 0.40, 0.65, 1.0];
pub const BUTTON_HOVER: [f32; 4] = [0.30, 0.52, 0.78, 1.0];
pub const BUTTON_PRESS: [f32; 4] = [0.12, 0.28, 0.50, 1.0];
pub const STATUS_BG: [f32; 4] = [0.06, 0.06, 0.10, 1.0];
pub const STATUS_ACCENT: [f32; 4] = [0.20, 0.20, 0.25, 1.0];
pub const RESET_BTN_IDLE: [f32; 4] = [0.55, 0.20, 0.20, 1.0];
pub const RESET_BTN_HOVER: [f32; 4] = [0.70, 0.30, 0.30, 1.0];
pub const RESET_BTN_PRESS: [f32; 4] = [0.40, 0.12, 0.12, 1.0];
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

pub fn page_low_color() -> [f32; 4] {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Ok(content) = std::fs::read_to_string("/home/lsgalante/.config/ccec/config.toml") {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("page_low_color") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let hex = rest.trim_end_matches('"').trim().trim_start_matches('#');
                    if hex.len() >= 6 {
                        if let (Ok(r), Ok(g), Ok(b)) = (
                            u8::from_str_radix(&hex[0..2], 16),
                            u8::from_str_radix(&hex[2..4], 16),
                            u8::from_str_radix(&hex[4..6], 16),
                        ) {
                            let r_f = srgb_to_linear(r as f32 / 255.0);
                            let g_f = srgb_to_linear(g as f32 / 255.0);
                            let b_f = srgb_to_linear(b as f32 / 255.0);
                            if let Ok(mut lock) = PAGE_LOW_COLOR.write() {
                                *lock = [r_f, g_f, b_f, 1.0];
                            }
                        }
                    }
                }
            }
        }
    });
    *PAGE_LOW_COLOR.read().unwrap()
}

pub fn set_page_low_color(color: [f32; 4]) {
    if let Ok(mut lock) = PAGE_LOW_COLOR.write() {
        *lock = color;
    }
}

pub fn color_borders_color() -> [f32; 4] {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Ok(content) = std::fs::read_to_string("/home/lsgalante/.config/ccec/config.toml") {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("color_borders_color") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let hex = rest.trim_end_matches('"').trim().trim_start_matches('#');
                    if hex.len() >= 6 {
                        if let (Ok(r), Ok(g), Ok(b)) = (
                            u8::from_str_radix(&hex[0..2], 16),
                            u8::from_str_radix(&hex[2..4], 16),
                            u8::from_str_radix(&hex[4..6], 16),
                        ) {
                            let r_f = srgb_to_linear(r as f32 / 255.0);
                            let g_f = srgb_to_linear(g as f32 / 255.0);
                            let b_f = srgb_to_linear(b as f32 / 255.0);
                            if let Ok(mut lock) = COLOR_BORDERS_COLOR.write() {
                                *lock = [r_f, g_f, b_f, 1.0];
                            }
                        }
                    }
                }
            }
        }
    });
    *COLOR_BORDERS_COLOR.read().unwrap()
}

pub fn set_color_borders_color(color: [f32; 4]) {
    if let Ok(mut lock) = COLOR_BORDERS_COLOR.write() {
        *lock = color;
    }
}

pub fn slider_track() -> [f32; 4] {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Ok(content) = std::fs::read_to_string("/home/lsgalante/.config/ccec/config.toml") {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("slider_track_color") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let hex = rest.trim_end_matches('"').trim().trim_start_matches('#');
                    if hex.len() >= 6 {
                        if let (Ok(r), Ok(g), Ok(b)) = (
                            u8::from_str_radix(&hex[0..2], 16),
                            u8::from_str_radix(&hex[2..4], 16),
                            u8::from_str_radix(&hex[4..6], 16),
                        ) {
                            let r_f = srgb_to_linear(r as f32 / 255.0);
                            let g_f = srgb_to_linear(g as f32 / 255.0);
                            let b_f = srgb_to_linear(b as f32 / 255.0);
                            if let Ok(mut lock) = SLIDER_TRACK_COLOR.write() {
                                *lock = [r_f, g_f, b_f, 1.0];
                            }
                        }
                    }
                }
            }
        }
    });
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
