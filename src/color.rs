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
static NODE_SELECTED_COLOR: RwLock<[f32; 4]> = RwLock::new(NODE_SELECTED);
static NODE_DRAG_COLOR: RwLock<[f32; 4]> = RwLock::new(NODE_DRAG);
static SIDEBAR_BG_COLOR: RwLock<[f32; 4]> = RwLock::new(SIDEBAR_BG);
static HIGHLIGHT_PRIMARY_COLOR: RwLock<[f32; 4]> = RwLock::new(HIGHLIGHT_PRIMARY);
static MENUBAR_TAB_LABEL_COLOR: RwLock<[f32; 4]> = RwLock::new([0.90196, 0.90196, 0.94902, 1.0]); // sRGB [230, 230, 242] linear
static OPACITY: RwLock<Option<f32>> = RwLock::new(None);
static BACKPLATE_OPACITY: RwLock<Option<f32>> = RwLock::new(None);
static TOGGLE_ON_COLOR: RwLock<[f32; 4]> = RwLock::new(TOGGLE_ON);
static TOGGLE_OFF_COLOR: RwLock<[f32; 4]> = RwLock::new(TOGGLE_OFF);
static TOGGLE_BG_COLOR: RwLock<[f32; 4]> = RwLock::new([0.18, 0.18, 0.22, 1.0]);
static LIST_BG_COLOR: RwLock<[f32; 4]> = RwLock::new([0.08, 0.08, 0.12, 0.3]);
static LIST_ENTRY_BG_COLOR: RwLock<[f32; 4]> = RwLock::new([1.0, 1.0, 1.0, 0.04]);
static LIST_ENTRY_HIGHLIGHT_COLOR: RwLock<[f32; 4]> = RwLock::new([1.0, 1.0, 1.0, 0.8]);
static LIST_FONT_COLOR: RwLock<[f32; 4]> = RwLock::new([0.80, 0.80, 0.85, 1.0]);
static BREADCRUMB_BG_COLOR: RwLock<[f32; 4]> = RwLock::new([0.08, 0.08, 0.12, 1.0]);
static POPOVER_BG_COLOR: RwLock<[f32; 4]> = RwLock::new([0.08, 0.08, 0.12, 1.0]);
static PAGE_COLOR: RwLock<[f32; 4]> = RwLock::new([0.0, 0.0, 0.0, 0.0]);
static LAYER_COLOR: RwLock<[f32; 4]> = RwLock::new([0.0, 0.0, 0.0, 0.0]);
static BACKPLATE_CORNER_RADIUS: RwLock<f32> = RwLock::new(12.0);

static DROPDOWN_BACKGROUND_COLOR: RwLock<[f32; 4]> = RwLock::new([0.08, 0.08, 0.12, 1.0]);

static TEXTBOX_PLACEHOLDER_TEXT_COLOR: RwLock<[u8; 3]> = RwLock::new([0x60, 0x60, 0x6a]);
static TEXTBOX_BACKGROUND_COLOR: RwLock<[f32; 4]> = RwLock::new([0.08, 0.08, 0.12, 1.0]);
static TEXTBOX_BACKGROUND_EDIT_COLOR: RwLock<[f32; 4]> = RwLock::new([0.06, 0.10, 0.18, 1.0]);

static BACKPLATE_MENUBAR_COLOR: RwLock<[f32; 4]> = RwLock::new([0.08, 0.08, 0.12, 1.0]);
static BACKPLATE_MENUBAR_TEXT_COLOR: RwLock<[f32; 4]> = RwLock::new([0.90196, 0.90196, 0.94902, 1.0]);
static BACKPLATE_MENUBAR_BLUR: RwLock<bool> = RwLock::new(false);

static BACKPLATE_STATUSBAR_COLOR: RwLock<[f32; 4]> = RwLock::new([0.06, 0.06, 0.10, 1.0]);
static BACKPLATE_STATUSBAR_TEXT_COLOR: RwLock<[f32; 4]> = RwLock::new([0.6666, 0.6666, 0.7333, 1.0]);
static BACKPLATE_STATUSBAR_BLUR: RwLock<bool> = RwLock::new(false);
static BUTTON_BACKGROUND_COLOR: RwLock<[f32; 4]> = RwLock::new(BUTTON_IDLE);

static TREE_BACKGROUND_COLOR: RwLock<[f32; 4]> = RwLock::new([0.08, 0.08, 0.12, 0.3]);
static TREE_BORDER_COLOR: RwLock<[f32; 4]> = RwLock::new([0.18, 0.18, 0.24, 1.0]);
static TREE_BORDER_HOVER_COLOR: RwLock<[f32; 4]> = RwLock::new([0.25, 0.25, 0.35, 1.0]);
static TREE_BORDER_FOCUS_COLOR: RwLock<[f32; 4]> = RwLock::new([0.30, 0.50, 0.32, 1.0]);
static TREE_OPEN_SEARCH_KEY: RwLock<String> = RwLock::new(String::new());

static TREE_SECTION_BG_COLOR: RwLock<[f32; 4]> = RwLock::new([0.07, 0.07, 0.09, 1.0]);
static TREE_SECTION_BG_HOVER_COLOR: RwLock<[f32; 4]> = RwLock::new([0.10, 0.12, 0.18, 1.0]);

static TREE_LEAF_BG_EVEN_COLOR: RwLock<[f32; 4]> = RwLock::new([0.09, 0.09, 0.11, 1.0]);
static TREE_LEAF_BG_ODD_COLOR: RwLock<[f32; 4]> = RwLock::new([0.08, 0.08, 0.10, 1.0]);
static TREE_LEAF_BG_HOVER_COLOR: RwLock<[f32; 4]> = RwLock::new([0.12, 0.12, 0.16, 1.0]);
static TREE_LEAF_BG_SELECTED_COLOR: RwLock<[f32; 4]> = RwLock::new([0.15, 0.20, 0.30, 1.0]);

static TREE_SECTION_TEXT_COLOR: RwLock<[f32; 4]> = RwLock::new([0.38, 0.69, 0.94, 1.0]);
static TREE_LEAF_TEXT_COLOR: RwLock<[f32; 4]> = RwLock::new([0.80, 0.80, 0.83, 1.0]);
static TREE_LEAF_TEXT_SELECTED_COLOR: RwLock<[f32; 4]> = RwLock::new([0.49, 1.0, 1.0, 1.0]);

static TREE_TYPE_TEXT_COLOR: RwLock<[f32; 4]> = RwLock::new([0.78, 0.47, 0.87, 1.0]);
static TREE_VALUE_TEXT_COLOR: RwLock<[f32; 4]> = RwLock::new([0.51, 0.51, 0.54, 1.0]);
static TREE_SEPARATOR_COLOR: RwLock<[f32; 4]> = RwLock::new([0.15, 0.15, 0.19, 1.0]);

static SCROLLBAR_TRACK_COLOR: RwLock<[f32; 4]> = RwLock::new([0.15, 0.15, 0.20, 0.3]);
static SCROLLBAR_THUMB_COLOR: RwLock<[f32; 4]> = RwLock::new([0.60, 0.60, 0.65, 0.4]);

static GRAPH_CELL_COLOR: RwLock<[f32; 3]> = RwLock::new([0.13, 0.13, 0.16]);
static GRAPH_GAP_COLOR: RwLock<[f32; 3]> = RwLock::new([0.07, 0.07, 0.09]);
static GRAPH_OPACITY: RwLock<f32> = RwLock::new(0.95);

static GRAPH_NODE_COLOR: RwLock<[f32; 4]> = RwLock::new(NODE_IDLE);
static GRAPH_NODE_SELECTED_COLOR: RwLock<[f32; 4]> = RwLock::new(NODE_SELECTED);
static GRAPH_NODE_DRAG_COLOR: RwLock<[f32; 4]> = RwLock::new(NODE_DRAG);

static GRAPH_WIRE_COLOR: RwLock<[f32; 4]> = RwLock::new([0.1, 0.8, 0.4, 1.0]);
static GRAPH_WIRE_HIGHLIGHT_COLOR: RwLock<[f32; 4]> = RwLock::new([0.0, 1.0, 0.9, 1.0]);

static GRAPH_CONNECTOR_COLOR: RwLock<[f32; 4]> = RwLock::new([0.1, 0.8, 0.4, 1.0]);
static GRAPH_CONNECTOR_HIGHLIGHT_COLOR: RwLock<[f32; 4]> = RwLock::new([0.0, 1.0, 0.9, 1.0]);

pub fn button_background_color() -> [f32; 4] {
    *BUTTON_BACKGROUND_COLOR.read().unwrap()
}

pub fn set_button_background_color(color: [f32; 4]) {
    if let Ok(mut lock) = BUTTON_BACKGROUND_COLOR.write() {
        *lock = color;
    }
}

pub fn button_hover_color() -> [f32; 4] {
    let base = button_background_color();
    let mut oklab = linear_srgb_to_oklab([base[0], base[1], base[2]]);
    oklab[0] = (oklab[0] + 0.05).min(1.0); // increase lightness slightly
    let rgb = oklab_to_linear_srgb(oklab);
    [
        rgb[0].clamp(0.0, 1.0),
        rgb[1].clamp(0.0, 1.0),
        rgb[2].clamp(0.0, 1.0),
        (base[3] + 0.20).min(1.0),
    ]
}

pub fn button_press_color() -> [f32; 4] {
    let base = button_background_color();
    let mut oklab = linear_srgb_to_oklab([base[0], base[1], base[2]]);
    oklab[0] = (oklab[0] - 0.07).max(0.0); // decrease lightness
    let rgb = oklab_to_linear_srgb(oklab);
    [
        rgb[0].clamp(0.0, 1.0),
        rgb[1].clamp(0.0, 1.0),
        rgb[2].clamp(0.0, 1.0),
        (base[3] + 0.40).min(1.0),
    ]
}

pub fn node_color() -> [f32; 4] {
    load_colors_once();
    *NODE_COLOR.read().unwrap()
}

pub fn set_node_color(color: [f32; 4]) {
    if let Ok(mut lock) = NODE_COLOR.write() {
        *lock = color;
    }
}

pub fn node_selected_color() -> [f32; 4] {
    load_colors_once();
    *NODE_SELECTED_COLOR.read().unwrap()
}

pub fn set_node_selected_color(color: [f32; 4]) {
    if let Ok(mut lock) = NODE_SELECTED_COLOR.write() {
        *lock = color;
    }
}

pub fn node_drag_color() -> [f32; 4] {
    load_colors_once();
    *NODE_DRAG_COLOR.read().unwrap()
}

pub fn set_node_drag_color(color: [f32; 4]) {
    if let Ok(mut lock) = NODE_DRAG_COLOR.write() {
        *lock = color;
    }
}

fn read_config() -> Option<String> {
    let path = crate::config::get_config_path();
    if let Ok(content) = std::fs::read_to_string(&path) {
        return Some(content);
    }
    None
}

fn parse_and_set_colors(content: &str) {
    let val = crate::config::parse_kdl_to_json(content);

    let parse_hex = |hex_str: &str| -> Option<[f32; 4]> {
        let hex = hex_str.trim_matches(|c| c == '"' || c == '\'' || c == ' ');
        let hex = hex.trim_start_matches('#');
        if hex.len() >= 8 {
            if let (Ok(r), Ok(g), Ok(b), Ok(a)) = (
                u8::from_str_radix(&hex[0..2], 16),
                u8::from_str_radix(&hex[2..4], 16),
                u8::from_str_radix(&hex[4..6], 16),
                u8::from_str_radix(&hex[6..8], 16),
            ) {
                let r_f = srgb_to_linear(r as f32 / 255.0);
                let g_f = srgb_to_linear(g as f32 / 255.0);
                let b_f = srgb_to_linear(b as f32 / 255.0);
                let a_f = a as f32 / 255.0;
                return Some([r_f, g_f, b_f, a_f]);
            }
        } else if hex.len() >= 6 {
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

    let get_color_u8 = |pointer: &str| -> Option<[u8; 3]> {
        val.pointer(pointer).and_then(|v| v.as_str()).and_then(|hex_str| {
            let hex = hex_str.trim_matches(|c| c == '"' || c == '\'' || c == ' ');
            let hex = hex.trim_start_matches('#');
            if hex.len() >= 6 {
                if let (Ok(r), Ok(g), Ok(b)) = (
                    u8::from_str_radix(&hex[0..2], 16),
                    u8::from_str_radix(&hex[2..4], 16),
                    u8::from_str_radix(&hex[4..6], 16),
                ) {
                    return Some([r, g, b]);
                }
            }
            None
        })
    };

    if let Some(opacity) = val.pointer("/layout/menubar_opacity").and_then(|v| v.as_f64()) {
        if let Ok(mut lock) = OPACITY.write() {
            *lock = Some(opacity as f32);
        }
    }

    if let Some(radius) = val.pointer("/style/surface/backplate/corner_radius").and_then(|v| v.as_f64()) {
        if let Ok(mut lock) = BACKPLATE_CORNER_RADIUS.write() {
            *lock = radius as f32;
        }
     }

    if let Some(c) = get_color("/style/surface/backplate/color") {
        if let Ok(mut lock) = PAGE_LOW_COLOR.write() { *lock = c; }
        if let Ok(mut lock) = BACKPLATE_OPACITY.write() { *lock = Some(c[3]); }
    }

    if let Some(c) = get_color("/style/surface/backplate/menubar/color") {
        if let Ok(mut lock) = BACKPLATE_MENUBAR_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/surface/backplate/menubar/text_color") {
        if let Ok(mut lock) = BACKPLATE_MENUBAR_TEXT_COLOR.write() { *lock = c; }
    }
    if let Some(blur) = val.pointer("/style/surface/backplate/menubar/blur").and_then(|v| v.as_bool()) {
        if let Ok(mut lock) = BACKPLATE_MENUBAR_BLUR.write() { *lock = blur; }
    } else if let Some(blur_val) = val.pointer("/style/surface/backplate/menubar/blur").and_then(|v| v.as_f64()) {
        if let Ok(mut lock) = BACKPLATE_MENUBAR_BLUR.write() { *lock = blur_val > 0.001; }
    }

    if let Some(c) = get_color("/style/surface/backplate/statusbar/color") {
        if let Ok(mut lock) = BACKPLATE_STATUSBAR_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/surface/backplate/statusbar/text_color") {
        if let Ok(mut lock) = BACKPLATE_STATUSBAR_TEXT_COLOR.write() { *lock = c; }
    }
    if let Some(blur) = val.pointer("/style/surface/backplate/statusbar/blur").and_then(|v| v.as_bool()) {
        if let Ok(mut lock) = BACKPLATE_STATUSBAR_BLUR.write() { *lock = blur; }
    } else if let Some(blur_val) = val.pointer("/style/surface/backplate/statusbar/blur").and_then(|v| v.as_f64()) {
        if let Ok(mut lock) = BACKPLATE_STATUSBAR_BLUR.write() { *lock = blur_val > 0.001; }
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
    if let Some(c) = get_color("/style/highlight/primary") {
        if let Ok(mut lock) = HIGHLIGHT_PRIMARY_COLOR.write() { *lock = [c[0], c[1], c[2], 0.12]; }
    }
    if let Some(c) = get_color("/style/control/button/background").or_else(|| get_color("/layout/button_background_color")) {
        if let Ok(mut lock) = BUTTON_BACKGROUND_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/control/dropdown/color") {
        if let Ok(mut lock) = DROPDOWN_BACKGROUND_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color_u8("/style/textbox/placeholder_text_color")
        .or_else(|| get_color_u8("/style/data/textbox/placeholder_text_color")) {
        if let Ok(mut lock) = TEXTBOX_PLACEHOLDER_TEXT_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/textbox/background_color")
        .or_else(|| get_color("/style/data/textbox/background_color")) {
        if let Ok(mut lock) = TEXTBOX_BACKGROUND_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/textbox/background_edit_color")
        .or_else(|| get_color("/style/data/textbox/background_edit_color")) {
        if let Ok(mut lock) = TEXTBOX_BACKGROUND_EDIT_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/layout/menubar_tab_label_color").or_else(|| get_color("/layout/paginator_tab_label_color")) {
        if let Ok(mut lock) = MENUBAR_TAB_LABEL_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/layout/toggle_enabled_color") {
        if let Ok(mut lock) = TOGGLE_ON_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/control/toggle/disabled_color") {
        if let Ok(mut lock) = TOGGLE_OFF_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/layout/toggle_bg_color") {
        if let Ok(mut lock) = TOGGLE_BG_COLOR.write() { *lock = c; }
    }
    let parsed_list_bg = get_color("/layout/list_bg_color");
    let parsed_breadcrumb_bg = get_color("/layout/breadcrumb_bg_color");
    let parsed_popover_bg = get_color("/layout/popover_bg_color");

    if let Some(c) = get_color("/layout/page_color") {
        if let Ok(mut lock) = PAGE_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/layout/layer_color") {
        if let Ok(mut lock) = LAYER_COLOR.write() { *lock = c; }
    }

    if let Some(c) = parsed_list_bg {
        if let Ok(mut lock) = LIST_BG_COLOR.write() {
            *lock = [c[0], c[1], c[2], 0.3];
        }
    }
    if let Some(c) = get_color("/layout/list_entry_bg_color") {
        if let Ok(mut lock) = LIST_ENTRY_BG_COLOR.write() {
            *lock = c;
        }
    }
    if let Some(c) = get_color("/layout/list_entry_highlight_color") {
        if let Ok(mut lock) = LIST_ENTRY_HIGHLIGHT_COLOR.write() {
            *lock = c;
        }
    }
    if let Some(c) = get_color("/style/data/list/font_color").or_else(|| get_color("/layout/list_font_color")) {
        if let Ok(mut lock) = LIST_FONT_COLOR.write() {
            *lock = c;
        }
    }
    if let Some(c) = parsed_breadcrumb_bg {
        if let Ok(mut lock) = BREADCRUMB_BG_COLOR.write() {
            *lock = [c[0], c[1], c[2], 1.0];
        }
    } else if let Some(c) = parsed_list_bg {
        if let Ok(mut lock) = BREADCRUMB_BG_COLOR.write() {
            *lock = [c[0], c[1], c[2], 1.0];
        }
    }
    if let Some(c) = parsed_popover_bg {
        if let Ok(mut lock) = POPOVER_BG_COLOR.write() {
            *lock = [c[0], c[1], c[2], 1.0];
        }
    } else if let Some(c) = parsed_list_bg {
        if let Ok(mut lock) = POPOVER_BG_COLOR.write() {
            *lock = [c[0], c[1], c[2], 1.0];
        }
    }

    if let Some(c) = get_color("/style/surface/graph/cell_color") {
        if let Ok(mut lock) = GRAPH_CELL_COLOR.write() { *lock = [c[0], c[1], c[2]]; }
    }
    if let Some(c) = get_color("/style/surface/graph/gap_color") {
        if let Ok(mut lock) = GRAPH_GAP_COLOR.write() { *lock = [c[0], c[1], c[2]]; }
    }
    if let Some(c) = get_color("/style/surface/graph/node/color") {
        if let Ok(mut lock) = GRAPH_NODE_COLOR.write() { *lock = c; }
        if let Ok(mut lock) = NODE_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/surface/graph/node/selected_color") {
        if let Ok(mut lock) = GRAPH_NODE_SELECTED_COLOR.write() { *lock = c; }
        if let Ok(mut lock) = NODE_SELECTED_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/surface/graph/node/drag_color") {
        if let Ok(mut lock) = GRAPH_NODE_DRAG_COLOR.write() { *lock = c; }
        if let Ok(mut lock) = NODE_DRAG_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/surface/graph/node/wire_color") {
        if let Ok(mut lock) = GRAPH_WIRE_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/surface/graph/node/wire_highlight_color") {
        if let Ok(mut lock) = GRAPH_WIRE_HIGHLIGHT_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/surface/graph/node/connector_color") {
        if let Ok(mut lock) = GRAPH_CONNECTOR_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/surface/graph/node/connector_highlight_color") {
        if let Ok(mut lock) = GRAPH_CONNECTOR_HIGHLIGHT_COLOR.write() { *lock = c; }
    }
    if let Some(opacity) = val.pointer("/style/surface/graph/opacity").and_then(|v| v.as_f64()) {
        if let Ok(mut lock) = GRAPH_OPACITY.write() { *lock = opacity as f32; }
    }

    if let Some(c) = get_color("/style/data/tree/background_color") {
        if let Ok(mut lock) = TREE_BACKGROUND_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/data/tree/border_color") {
        if let Ok(mut lock) = TREE_BORDER_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/data/tree/border_hover_color") {
        if let Ok(mut lock) = TREE_BORDER_HOVER_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/data/tree/border_focus_color") {
        if let Ok(mut lock) = TREE_BORDER_FOCUS_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/data/tree/section_bg_color") {
        if let Ok(mut lock) = TREE_SECTION_BG_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/data/tree/section_bg_hover_color") {
        if let Ok(mut lock) = TREE_SECTION_BG_HOVER_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/data/tree/leaf_bg_even_color") {
        if let Ok(mut lock) = TREE_LEAF_BG_EVEN_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/data/tree/leaf_bg_odd_color") {
        if let Ok(mut lock) = TREE_LEAF_BG_ODD_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/data/tree/leaf_bg_hover_color") {
        if let Ok(mut lock) = TREE_LEAF_BG_HOVER_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/data/tree/leaf_bg_selected_color") {
        if let Ok(mut lock) = TREE_LEAF_BG_SELECTED_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/data/tree/section_text_color") {
        if let Ok(mut lock) = TREE_SECTION_TEXT_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/data/tree/leaf_text_color") {
        if let Ok(mut lock) = TREE_LEAF_TEXT_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/data/tree/leaf_text_selected_color") {
        if let Ok(mut lock) = TREE_LEAF_TEXT_SELECTED_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/data/tree/type_text_color") {
        if let Ok(mut lock) = TREE_TYPE_TEXT_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/data/tree/value_text_color") {
        if let Ok(mut lock) = TREE_VALUE_TEXT_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/data/tree/separator_color") {
        if let Ok(mut lock) = TREE_SEPARATOR_COLOR.write() { *lock = c; }
    }
    if let Some(k) = val.pointer("/style/data/tree/open_search").and_then(|v| v.as_str()) {
        if let Ok(mut lock) = TREE_OPEN_SEARCH_KEY.write() { *lock = k.to_string(); }
    }
    if let Some(c) = get_color("/style/control/scrollbar/track_color") {
        if let Ok(mut lock) = SCROLLBAR_TRACK_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/control/scrollbar/thumb_color") {
        if let Ok(mut lock) = SCROLLBAR_THUMB_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/surface/plate/color") {
        if let Ok(mut lock) = PLATE_COLOR.write() { *lock = Some(c); }
    }
    if let Some(c) = get_color("/style/surface/plate/border_color") {
        if let Ok(mut lock) = PLATE_BORDER_COLOR.write() { *lock = Some(c); }
    }
    if let Some(t) = val.pointer("/style/surface/plate/border_thickness").and_then(|v| v.as_f64()) {
        if let Ok(mut lock) = PLATE_BORDER_THICKNESS.write() { *lock = t as f32; }
    }
    if let Some(blur) = val.pointer("/style/surface/plate/blur").and_then(|v| v.as_bool()) {
        if let Ok(mut lock) = PLATE_BLUR.write() { *lock = blur; }
    } else if let Some(blur_val) = val.pointer("/style/surface/plate/blur").and_then(|v| v.as_f64()) {
        if let Ok(mut lock) = PLATE_BLUR.write() { *lock = blur_val > 0.001; }
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
    parse_and_set_colors(content);
}

pub fn dropdown_background_color() -> [f32; 4] {
    load_colors_once();
    *DROPDOWN_BACKGROUND_COLOR.read().unwrap()
}

pub fn set_dropdown_background_color(color: [f32; 4]) {
    if let Ok(mut lock) = DROPDOWN_BACKGROUND_COLOR.write() {
        *lock = color;
    }
}

pub fn textbox_placeholder_text_color() -> [u8; 3] {
    load_colors_once();
    *TEXTBOX_PLACEHOLDER_TEXT_COLOR.read().unwrap()
}

pub fn set_textbox_placeholder_text_color(color: [u8; 3]) {
    if let Ok(mut lock) = TEXTBOX_PLACEHOLDER_TEXT_COLOR.write() {
        *lock = color;
    }
}

pub fn textbox_background_color() -> [f32; 4] {
    load_colors_once();
    *TEXTBOX_BACKGROUND_COLOR.read().unwrap()
}

pub fn set_textbox_background_color(color: [f32; 4]) {
    if let Ok(mut lock) = TEXTBOX_BACKGROUND_COLOR.write() {
        *lock = color;
    }
}

pub fn textbox_background_edit_color() -> [f32; 4] {
    load_colors_once();
    *TEXTBOX_BACKGROUND_EDIT_COLOR.read().unwrap()
}

pub fn set_textbox_background_edit_color(color: [f32; 4]) {
    if let Ok(mut lock) = TEXTBOX_BACKGROUND_EDIT_COLOR.write() {
        *lock = color;
    }
}

pub fn graph_wire_color() -> [f32; 4] {
    load_colors_once();
    *GRAPH_WIRE_COLOR.read().unwrap()
}

pub fn set_graph_wire_color(color: [f32; 4]) {
    if let Ok(mut lock) = GRAPH_WIRE_COLOR.write() {
        *lock = color;
    }
}

pub fn graph_wire_highlight_color() -> [f32; 4] {
    load_colors_once();
    *GRAPH_WIRE_HIGHLIGHT_COLOR.read().unwrap()
}

pub fn set_graph_wire_highlight_color(color: [f32; 4]) {
    if let Ok(mut lock) = GRAPH_WIRE_HIGHLIGHT_COLOR.write() {
        *lock = color;
    }
}

pub fn graph_connector_color() -> [f32; 4] {
    load_colors_once();
    *GRAPH_CONNECTOR_COLOR.read().unwrap()
}

pub fn set_graph_connector_color(color: [f32; 4]) {
    if let Ok(mut lock) = GRAPH_CONNECTOR_COLOR.write() {
        *lock = color;
    }
}

pub fn graph_connector_highlight_color() -> [f32; 4] {
    load_colors_once();
    *GRAPH_CONNECTOR_HIGHLIGHT_COLOR.read().unwrap()
}

pub fn set_graph_connector_highlight_color(color: [f32; 4]) {
    if let Ok(mut lock) = GRAPH_CONNECTOR_HIGHLIGHT_COLOR.write() {
        *lock = color;
    }
}

pub fn graph_node_color() -> [f32; 4] {
    load_colors_once();
    *GRAPH_NODE_COLOR.read().unwrap()
}

pub fn set_graph_node_color(color: [f32; 4]) {
    if let Ok(mut lock) = GRAPH_NODE_COLOR.write() {
        *lock = color;
    }
}

pub fn graph_node_selected_color() -> [f32; 4] {
    load_colors_once();
    *GRAPH_NODE_SELECTED_COLOR.read().unwrap()
}

pub fn set_graph_node_selected_color(color: [f32; 4]) {
    if let Ok(mut lock) = GRAPH_NODE_SELECTED_COLOR.write() {
        *lock = color;
    }
}

pub fn graph_node_drag_color() -> [f32; 4] {
    load_colors_once();
    *GRAPH_NODE_DRAG_COLOR.read().unwrap()
}

pub fn set_graph_node_drag_color(color: [f32; 4]) {
    if let Ok(mut lock) = GRAPH_NODE_DRAG_COLOR.write() {
        *lock = color;
    }
}

pub fn graph_cell_color() -> [f32; 3] {
    load_colors_once();
    *GRAPH_CELL_COLOR.read().unwrap()
}

pub fn set_graph_cell_color(color: [f32; 3]) {
    if let Ok(mut lock) = GRAPH_CELL_COLOR.write() {
        *lock = color;
    }
}

pub fn graph_gap_color() -> [f32; 3] {
    load_colors_once();
    *GRAPH_GAP_COLOR.read().unwrap()
}

pub fn set_graph_gap_color(color: [f32; 3]) {
    if let Ok(mut lock) = GRAPH_GAP_COLOR.write() {
        *lock = color;
    }
}

pub fn graph_opacity() -> f32 {
    load_colors_once();
    *GRAPH_OPACITY.read().unwrap()
}

pub fn set_graph_opacity(opacity: f32) {
    if let Ok(mut lock) = GRAPH_OPACITY.write() {
        *lock = opacity;
    }
}

pub fn page_low_color() -> [f32; 4] {
    load_colors_once();
    let mut color = *PAGE_LOW_COLOR.read().unwrap();
    if let Some(opacity) = read_backplate_opacity_if_configured() {
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
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
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

pub fn to_linear_rgb(color: [f32; 3]) -> [f32; 3] {
    [
        srgb_to_linear(color[0]),
        srgb_to_linear(color[1]),
        srgb_to_linear(color[2]),
    ]
}

pub fn to_srgb_rgb(color: [f32; 3]) -> [f32; 3] {
    [
        linear_to_srgb(color[0]),
        linear_to_srgb(color[1]),
        linear_to_srgb(color[2]),
    ]
}

pub fn linear_srgb_to_oklab(rgb: [f32; 3]) -> [f32; 3] {
    let l = 0.4122214708 * rgb[0] + 0.5363325363 * rgb[1] + 0.0514459929 * rgb[2];
    let m = 0.2119034982 * rgb[0] + 0.6806995451 * rgb[1] + 0.1073969566 * rgb[2];
    let s = 0.0883024619 * rgb[0] + 0.2817188376 * rgb[1] + 0.6299787005 * rgb[2];

    let l_ = l.max(0.0).powf(1.0 / 3.0);
    let m_ = m.max(0.0).powf(1.0 / 3.0);
    let s_ = s.max(0.0).powf(1.0 / 3.0);

    [
        0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_,
        1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_,
        0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_,
    ]
}

pub fn oklab_to_linear_srgb(lab: [f32; 3]) -> [f32; 3] {
    let l_ = lab[0] + 0.3963377774 * lab[1] + 0.2158037573 * lab[2];
    let m_ = lab[0] - 0.1055613458 * lab[1] - 0.0638541728 * lab[2];
    let s_ = lab[0] - 0.0894841775 * lab[1] - 1.2914855480 * lab[2];

    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;

    [
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
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

pub fn read_backplate_opacity_if_configured() -> Option<f32> {
    load_colors_once();
    *BACKPLATE_OPACITY.read().unwrap()
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

pub fn toggle_bg_color() -> [f32; 4] {
    load_colors_once();
    *TOGGLE_BG_COLOR.read().unwrap()
}

pub fn set_toggle_bg_color(color: [f32; 4]) {
    if let Ok(mut lock) = TOGGLE_BG_COLOR.write() {
        *lock = color;
    }
}

pub fn list_bg_color() -> [f32; 4] {
    load_colors_once();
    *LIST_BG_COLOR.read().unwrap()
}

pub fn set_list_bg_color(color: [f32; 4]) {
    if let Ok(mut lock) = LIST_BG_COLOR.write() {
        *lock = [color[0], color[1], color[2], 0.3];
    }
}

pub fn list_entry_bg_color() -> [f32; 4] {
    load_colors_once();
    *LIST_ENTRY_BG_COLOR.read().unwrap()
}

pub fn set_list_entry_bg_color(color: [f32; 4]) {
    if let Ok(mut lock) = LIST_ENTRY_BG_COLOR.write() {
        *lock = color;
    }
}

pub fn list_entry_highlight_color() -> [f32; 4] {
    load_colors_once();
    *LIST_ENTRY_HIGHLIGHT_COLOR.read().unwrap()
}

pub fn set_list_entry_highlight_color(color: [f32; 4]) {
    if let Ok(mut lock) = LIST_ENTRY_HIGHLIGHT_COLOR.write() {
        *lock = color;
    }
}

pub fn list_font_color() -> [f32; 4] {
    load_colors_once();
    *LIST_FONT_COLOR.read().unwrap()
}

pub fn set_list_font_color(color: [f32; 4]) {
    if let Ok(mut lock) = LIST_FONT_COLOR.write() {
        *lock = color;
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

pub fn backplate_corner_radius() -> f32 {
    load_colors_once();
    *BACKPLATE_CORNER_RADIUS.read().unwrap()
}

pub fn set_backplate_corner_radius(radius: f32) {
    if let Ok(mut lock) = BACKPLATE_CORNER_RADIUS.write() {
        *lock = radius;
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

pub fn active_window_mode() -> String {
    let app_id = crate::scale::app_id();
    if app_id.is_empty() {
        return "floating".to_string();
    }

    let config_path = crate::config::get_config_path();
    let content = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(_) => return "floating".to_string(),
    };
    let val = crate::config::parse_kdl_to_json(&content);

    if let Some(mode_rules) = val.get("mode_rule").and_then(|r| r.as_array()) {
        for rule in mode_rules {
            if rule.get("app_id").and_then(|id| id.as_str()) == Some(&app_id) {
                if let Some(mode) = rule.get("mode").and_then(|m| m.as_str()) {
                    return mode.to_string();
                }
            }
        }
    }

    let mut default_tile_mode = "cascade".to_string();
    if let Some(tag_layouts) = val.get("tag_layout").and_then(|l| l.as_array()) {
        for tl in tag_layouts {
            if tl.get("tag").and_then(|t| t.as_u64()) == Some(1) {
                if let Some(m) = tl.get("mode").and_then(|m| m.as_str()) {
                    default_tile_mode = m.to_string();
                }
            }
        }
    }

    if crate::scale::is_fullscreen() {
        return "fullscreen".to_string();
    }
    if crate::scale::is_maximized() {
        return default_tile_mode;
    }

    "floating".to_string()
}

pub fn active_backplate_opacity() -> f32 {
    page_low_color()[3]
}

pub fn tree_background_color() -> [f32; 4] { *TREE_BACKGROUND_COLOR.read().unwrap() }
pub fn tree_border_color() -> [f32; 4] { *TREE_BORDER_COLOR.read().unwrap() }
pub fn tree_border_hover_color() -> [f32; 4] { *TREE_BORDER_HOVER_COLOR.read().unwrap() }
pub fn tree_border_focus_color() -> [f32; 4] { *TREE_BORDER_FOCUS_COLOR.read().unwrap() }

pub fn tree_section_bg_color() -> [f32; 4] { *TREE_SECTION_BG_COLOR.read().unwrap() }
pub fn tree_section_bg_hover_color() -> [f32; 4] { *TREE_SECTION_BG_HOVER_COLOR.read().unwrap() }

pub fn tree_leaf_bg_even_color() -> [f32; 4] { *TREE_LEAF_BG_EVEN_COLOR.read().unwrap() }
pub fn tree_leaf_bg_odd_color() -> [f32; 4] { *TREE_LEAF_BG_ODD_COLOR.read().unwrap() }
pub fn tree_leaf_bg_hover_color() -> [f32; 4] { *TREE_LEAF_BG_HOVER_COLOR.read().unwrap() }
pub fn tree_leaf_bg_selected_color() -> [f32; 4] { *TREE_LEAF_BG_SELECTED_COLOR.read().unwrap() }

pub fn tree_section_text_color() -> [f32; 4] { *TREE_SECTION_TEXT_COLOR.read().unwrap() }
pub fn tree_leaf_text_color() -> [f32; 4] { *TREE_LEAF_TEXT_COLOR.read().unwrap() }
pub fn tree_leaf_text_selected_color() -> [f32; 4] { *TREE_LEAF_TEXT_SELECTED_COLOR.read().unwrap() }

pub fn tree_type_text_color() -> [f32; 4] { *TREE_TYPE_TEXT_COLOR.read().unwrap() }
pub fn tree_value_text_color() -> [f32; 4] { *TREE_VALUE_TEXT_COLOR.read().unwrap() }
pub fn tree_separator_color() -> [f32; 4] { *TREE_SEPARATOR_COLOR.read().unwrap() }

pub fn set_tree_background_color(c: [f32; 4]) { if let Ok(mut lock) = TREE_BACKGROUND_COLOR.write() { *lock = c; } }
pub fn set_tree_border_color(c: [f32; 4]) { if let Ok(mut lock) = TREE_BORDER_COLOR.write() { *lock = c; } }
pub fn set_tree_border_hover_color(c: [f32; 4]) { if let Ok(mut lock) = TREE_BORDER_HOVER_COLOR.write() { *lock = c; } }
pub fn set_tree_border_focus_color(c: [f32; 4]) { if let Ok(mut lock) = TREE_BORDER_FOCUS_COLOR.write() { *lock = c; } }

pub fn set_tree_section_bg_color(c: [f32; 4]) { if let Ok(mut lock) = TREE_SECTION_BG_COLOR.write() { *lock = c; } }
pub fn set_tree_section_bg_hover_color(c: [f32; 4]) { if let Ok(mut lock) = TREE_SECTION_BG_HOVER_COLOR.write() { *lock = c; } }

pub fn set_tree_leaf_bg_even_color(c: [f32; 4]) { if let Ok(mut lock) = TREE_LEAF_BG_EVEN_COLOR.write() { *lock = c; } }
pub fn set_tree_leaf_bg_odd_color(c: [f32; 4]) { if let Ok(mut lock) = TREE_LEAF_BG_ODD_COLOR.write() { *lock = c; } }
pub fn set_tree_leaf_bg_hover_color(c: [f32; 4]) { if let Ok(mut lock) = TREE_LEAF_BG_HOVER_COLOR.write() { *lock = c; } }
pub fn set_tree_leaf_bg_selected_color(c: [f32; 4]) { if let Ok(mut lock) = TREE_LEAF_BG_SELECTED_COLOR.write() { *lock = c; } }

pub fn set_tree_section_text_color(c: [f32; 4]) { if let Ok(mut lock) = TREE_SECTION_TEXT_COLOR.write() { *lock = c; } }
pub fn set_tree_leaf_text_color(c: [f32; 4]) { if let Ok(mut lock) = TREE_LEAF_TEXT_COLOR.write() { *lock = c; } }
pub fn set_tree_leaf_text_selected_color(c: [f32; 4]) { if let Ok(mut lock) = TREE_LEAF_TEXT_SELECTED_COLOR.write() { *lock = c; } }

pub fn set_tree_type_text_color(c: [f32; 4]) { if let Ok(mut lock) = TREE_TYPE_TEXT_COLOR.write() { *lock = c; } }
pub fn set_tree_value_text_color(c: [f32; 4]) { if let Ok(mut lock) = TREE_VALUE_TEXT_COLOR.write() { *lock = c; } }
pub fn set_tree_separator_color(c: [f32; 4]) { if let Ok(mut lock) = TREE_SEPARATOR_COLOR.write() { *lock = c; } }

pub fn scrollbar_track_color() -> [f32; 4] { *SCROLLBAR_TRACK_COLOR.read().unwrap() }
pub fn set_scrollbar_track_color(c: [f32; 4]) { if let Ok(mut lock) = SCROLLBAR_TRACK_COLOR.write() { *lock = c; } }
pub fn scrollbar_thumb_color() -> [f32; 4] { *SCROLLBAR_THUMB_COLOR.read().unwrap() }
pub fn set_scrollbar_thumb_color(c: [f32; 4]) { if let Ok(mut lock) = SCROLLBAR_THUMB_COLOR.write() { *lock = c; } }

pub fn tree_open_search_key() -> String {
    load_colors_once();
    let val = TREE_OPEN_SEARCH_KEY.read().unwrap().clone();
    if val.is_empty() {
        "ctrl+f".to_string()
    } else {
        val
    }
}
pub fn set_tree_open_search_key(k: String) {
    if let Ok(mut lock) = TREE_OPEN_SEARCH_KEY.write() {
        *lock = k;
    }
}

pub fn backplate_menubar_color() -> [f32; 4] {
    load_colors_once();
    *BACKPLATE_MENUBAR_COLOR.read().unwrap()
}
pub fn set_backplate_menubar_color(c: [f32; 4]) {
    if let Ok(mut lock) = BACKPLATE_MENUBAR_COLOR.write() { *lock = c; }
}

pub fn backplate_menubar_text_color() -> [f32; 4] {
    load_colors_once();
    *BACKPLATE_MENUBAR_TEXT_COLOR.read().unwrap()
}
pub fn set_backplate_menubar_text_color(c: [f32; 4]) {
    if let Ok(mut lock) = BACKPLATE_MENUBAR_TEXT_COLOR.write() { *lock = c; }
}

pub fn backplate_menubar_blur() -> bool {
    load_colors_once();
    *BACKPLATE_MENUBAR_BLUR.read().unwrap()
}
pub fn set_backplate_menubar_blur(b: bool) {
    if let Ok(mut lock) = BACKPLATE_MENUBAR_BLUR.write() { *lock = b; }
}

pub fn backplate_statusbar_color() -> [f32; 4] {
    load_colors_once();
    *BACKPLATE_STATUSBAR_COLOR.read().unwrap()
}
pub fn set_backplate_statusbar_color(c: [f32; 4]) {
    if let Ok(mut lock) = BACKPLATE_STATUSBAR_COLOR.write() { *lock = c; }
}

pub fn backplate_statusbar_text_color() -> [f32; 4] {
    load_colors_once();
    *BACKPLATE_STATUSBAR_TEXT_COLOR.read().unwrap()
}
pub fn set_backplate_statusbar_text_color(c: [f32; 4]) {
    if let Ok(mut lock) = BACKPLATE_STATUSBAR_TEXT_COLOR.write() { *lock = c; }
}

pub fn backplate_statusbar_blur() -> bool {
    load_colors_once();
    *BACKPLATE_STATUSBAR_BLUR.read().unwrap()
}
pub fn set_backplate_statusbar_blur(b: bool) {
    if let Ok(mut lock) = BACKPLATE_STATUSBAR_BLUR.write() { *lock = b; }
}

static PLATE_COLOR: RwLock<Option<[f32; 4]>> = RwLock::new(Some([0.15, 0.15, 0.2, 0.95]));
static PLATE_BORDER_COLOR: RwLock<Option<[f32; 4]>> = RwLock::new(Some([0.3, 0.3, 0.4, 1.0]));
static PLATE_BORDER_THICKNESS: RwLock<f32> = RwLock::new(1.0);

pub fn plate_color() -> Option<[f32; 4]> {
    load_colors_once();
    if let Ok(lock) = PLATE_COLOR.read() {
        return *lock;
    }
    None
}

pub fn set_plate_color(c: Option<[f32; 4]>) {
    if let Ok(mut lock) = PLATE_COLOR.write() {
        *lock = c;
    }
}

pub fn plate_border_color() -> Option<[f32; 4]> {
    load_colors_once();
    if let Ok(lock) = PLATE_BORDER_COLOR.read() {
        return *lock;
    }
    None
}

pub fn set_plate_border_color(c: Option<[f32; 4]>) {
    if let Ok(mut lock) = PLATE_BORDER_COLOR.write() {
        *lock = c;
    }
}

pub fn plate_border_thickness() -> f32 {
    load_colors_once();
    if let Ok(lock) = PLATE_BORDER_THICKNESS.read() {
        return *lock;
    }
    1.0
}

pub fn set_plate_border_thickness(t: f32) {
    if let Ok(mut lock) = PLATE_BORDER_THICKNESS.write() {
        *lock = t;
    }
}

static PLATE_BLUR: RwLock<bool> = RwLock::new(false);

pub fn plate_blur() -> bool {
    load_colors_once();
    if let Ok(lock) = PLATE_BLUR.read() {
        return *lock;
    }
    false
}

pub fn set_plate_blur(b: bool) {
    if let Ok(mut lock) = PLATE_BLUR.write() {
        *lock = b;
    }
}

#[cfg(test)]
mod color_tests {
    use super::*;

    #[test]
    fn test_print_active_config() {
        let path = crate::config::get_config_path();
        println!("ACTIVE CONFIG PATH: {:?}", path);
        if let Ok(content) = std::fs::read_to_string(&path) {
            println!("FILE READ OK! Length: {}", content.len());
            let val = crate::config::parse_kdl_to_json(&content);
            println!("PARSED JSON POINTER: {:?}", val.pointer("/style/control/dropdown/color"));
        } else {
            println!("FILE READ FAILED!");
        }
        println!("DROPDOWN COLOR GETTER: {:?}", dropdown_background_color());
        println!("LIST FONT COLOR GETTER: {:?}", list_font_color());
        println!("PLATE COLOR: {:?}", plate_color());
        println!("PLATE BORDER COLOR: {:?}", plate_border_color());
        println!("PLATE BORDER THICKNESS: {:?}", plate_border_thickness());
        println!("PLATE BLUR: {:?}", plate_blur());
    }
}

