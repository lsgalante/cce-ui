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
pub const TOGGLE_OFF: [f32; 4] = [0.25, 0.25, 0.30, 1.0];
pub const TOGGLE_ON: [f32; 4] = [0.14, 0.70, 0.38, 1.0];
pub const TOGGLE_HOVER: [f32; 4] = [0.30, 0.30, 0.35, 1.0];
pub const SLIDER_TRACK: [f32; 4] = [0.18, 0.18, 0.22, 1.0];

use std::sync::RwLock;

/// Per-thread overrides for runtime colour writes, under `cfg(test)` only.
///
/// Same defect as the font flake (cce-ui 1dc0ab1) and the style registry
/// beside it: these are process-wide `RwLock` statics, so a test that pins a
/// colour pins it for every test running alongside. It was live —
/// `test_graph_style_configuration` sets a dozen of them and restores none.
///
/// Keyed by the address of the static itself, so a slot needs no name
/// repeated in two places and cannot be typo'd into a silent miss. Config
/// loading is unaffected: `parse_and_set_colors` writes the statics directly
/// and never goes through these setters, so the loaded palette stays the
/// shared base every test thread reads.
#[cfg(test)]
mod test_overlay {
    use std::any::Any;
    use std::cell::RefCell;
    use std::collections::HashMap;
    thread_local! {
        static MAP: RefCell<HashMap<usize, Box<dyn Any>>> = RefCell::new(HashMap::new());
    }
    pub fn get<T: Clone + 'static>(key: usize) -> Option<T> {
        MAP.with(|m| m.borrow().get(&key).and_then(|b| b.downcast_ref::<T>()).cloned())
    }
    pub fn set<T: 'static>(key: usize, val: T) {
        MAP.with(|m| m.borrow_mut().insert(key, Box::new(val)));
    }
}

/// Read a style static, preferring this thread's test override.
#[inline]
fn style_read<T: Clone + 'static>(cell: &'static RwLock<T>) -> T {
    #[cfg(test)]
    if let Some(v) = test_overlay::get::<T>(cell as *const _ as usize) {
        return v;
    }
    cell.read().unwrap().clone()
}

/// Write a style static: per-thread under `cfg(test)`, process-wide otherwise.
#[inline]
fn style_write<T: Clone + 'static>(cell: &'static RwLock<T>, val: T) {
    #[cfg(test)]
    test_overlay::set(cell as *const _ as usize, val);
    #[cfg(not(test))]
    if let Ok(mut lock) = cell.write() {
        *lock = val;
    }
}


static PAGE_LOW_COLOR: RwLock<[f32; 4]> = RwLock::new([0.0600316, 0.0600316, 0.080219, 1.0]);
static COLOR_BORDERS_COLOR: RwLock<[f32; 4]> = RwLock::new([0.2039, 0.2039, 0.2530, 1.0]);
static NODE_COLOR: RwLock<[f32; 4]> = RwLock::new(NODE_IDLE);
static NODE_SELECTED_COLOR: RwLock<[f32; 4]> = RwLock::new(NODE_SELECTED);
static NODE_DRAG_COLOR: RwLock<[f32; 4]> = RwLock::new(NODE_DRAG);
static SIDEBAR_BG_COLOR: RwLock<[f32; 4]> = RwLock::new(SIDEBAR_BG);
static HIGHLIGHT_PRIMARY_COLOR: RwLock<[f32; 4]> = RwLock::new(HIGHLIGHT_PRIMARY);
static MENUBAR_TAB_LABEL_COLOR: RwLock<[f32; 4]> = RwLock::new([0.90196, 0.90196, 0.94902, 1.0]); // sRGB [230, 230, 242] linear
static CONTROL_LABEL_COLOR: RwLock<[f32; 4]> = RwLock::new([0.61206, 0.61206, 0.68666, 1.0]); // sRGB [204, 204, 212]
static CONTROL_LABEL_COLOR_DETACHED: RwLock<[f32; 4]> = RwLock::new([0.22416, 0.22416, 0.2526, 1.0]); // sRGB [131, 131, 138]
static CONTROL_LABEL_HOVER_COLOR: RwLock<Option<[f32; 4]>> = RwLock::new(None);
static CONTROL_LABEL_FOCUS_COLOR: RwLock<Option<[f32; 4]>> = RwLock::new(None);
static OPACITY: RwLock<Option<f32>> = RwLock::new(None);
static ROOT_PLATE_OPACITY: RwLock<Option<f32>> = RwLock::new(None);
static LIST_BG_COLOR: RwLock<[f32; 4]> = RwLock::new([0.08, 0.08, 0.12, 0.3]);
static LIST_ENTRY_BG_COLOR: RwLock<[f32; 4]> = RwLock::new([1.0, 1.0, 1.0, 0.04]);
static LIST_ENTRY_HIGHLIGHT_COLOR: RwLock<[f32; 4]> = RwLock::new([1.0, 1.0, 1.0, 0.8]);
static LIST_FONT_COLOR: RwLock<[f32; 4]> = RwLock::new([0.80, 0.80, 0.85, 1.0]);
static BREADCRUMB_BG_COLOR: RwLock<[f32; 4]> = RwLock::new([0.08, 0.08, 0.12, 1.0]);
static POPOVER_BG_COLOR: RwLock<[f32; 4]> = RwLock::new([0.08, 0.08, 0.12, 1.0]);
static PAGE_COLOR: RwLock<[f32; 4]> = RwLock::new([0.0, 0.0, 0.0, 0.0]);
static LAYER_COLOR: RwLock<[f32; 4]> = RwLock::new([0.0, 0.0, 0.0, 0.0]);
static ROOT_PLATE_CORNER_RADIUS: RwLock<f32> = RwLock::new(12.0);

// Transparent by default (alpha 0): a dropdown picks up the surface it sits
// on, and its closed-state chrome is the flush inset trough alone — the
// cce-files treatment, DE-wide. A configured `dropdown color=` opts a theme
// back into a filled face (the paint path judges the RAW alpha).
static DROPDOWN_BACKGROUND_COLOR: RwLock<[f32; 4]> = RwLock::new([0.0, 0.0, 0.0, 0.0]);

static TEXTBOX_PLACEHOLDER_TEXT_COLOR: RwLock<[u8; 3]> = RwLock::new([0x60, 0x60, 0x6a]);
static TEXTBOX_BACKGROUND_COLOR: RwLock<[f32; 4]> = RwLock::new([0.08, 0.08, 0.12, 1.0]);
static TEXTBOX_BACKGROUND_EDIT_COLOR: RwLock<[f32; 4]> = RwLock::new([0.06, 0.10, 0.18, 1.0]);

static ROOT_PLATE_MENUBAR_COLOR: RwLock<[f32; 4]> = RwLock::new([0.08, 0.08, 0.12, 1.0]);
static ROOT_PLATE_MENUBAR_TEXT_COLOR: RwLock<[f32; 4]> = RwLock::new([0.90196, 0.90196, 0.94902, 1.0]);
static ROOT_PLATE_MENUBAR_BLUR: RwLock<bool> = RwLock::new(false);
/// Tint strength of frosted menus/popovers over the blurred backdrop:
/// 1.0 is fully opaque (frost invisible), lower shows more content through.
static MENU_OPACITY: RwLock<f32> = RwLock::new(0.8);

static ROOT_PLATE_STATUSBAR_COLOR: RwLock<[f32; 4]> = RwLock::new([0.06, 0.06, 0.10, 1.0]);
static ROOT_PLATE_STATUSBAR_TEXT_COLOR: RwLock<[f32; 4]> = RwLock::new([0.6666, 0.6666, 0.7333, 1.0]);
static ROOT_PLATE_STATUSBAR_BLUR: RwLock<bool> = RwLock::new(false);
static BUTTON_BACKGROUND_COLOR: RwLock<[f32; 4]> = RwLock::new(BUTTON_IDLE);
static RAMP_BACKGROUND_COLOR: RwLock<[f32; 4]> = RwLock::new([0.08, 0.08, 0.12, 1.0]);
static RAMP_BORDER_COLOR: RwLock<[f32; 4]> = RwLock::new([0.18, 0.18, 0.24, 1.0]);
static CONTROL_PANEL_COLOR: RwLock<[f32; 4]> = RwLock::new([0.075, 0.082, 0.11, 1.0]); // default #13151cff
static CONTROL_PANEL_BORDER_COLOR: RwLock<[f32; 4]> = RwLock::new([0.161, 0.173, 0.216, 1.0]); // default #292c37ff

static PROGRESS_BG_COLOR: RwLock<[f32; 4]> = RwLock::new(PROGRESS_BG);
static PROGRESS_FILL_COLOR: RwLock<[f32; 4]> = RwLock::new(PROGRESS_FILL);
static SPINBOX_DISPLAY_COLOR: RwLock<[f32; 4]> = RwLock::new(SPINBOX_DISPLAY);
static SPINBOX_BUTTON_COLOR: RwLock<[f32; 4]> = RwLock::new(SPINBOX_BUTTON);
static SPINBOX_BUTTON_HOVER_COLOR: RwLock<[f32; 4]> = RwLock::new(SPINBOX_BUTTON_HOVER);
static SPINBOX_TEXT_COLOR: RwLock<[f32; 4]> = RwLock::new([0.8, 0.8, 0.83, 1.0]);

static BUTTON_BORDER_COLOR: RwLock<Option<[f32; 4]>> = RwLock::new(None);
static BUTTON_HOVER_COLOR: RwLock<Option<[f32; 4]>> = RwLock::new(None);

static DROPDOWN_BORDER_COLOR: RwLock<[f32; 4]> = RwLock::new([0.18, 0.18, 0.24, 1.0]);
static DROPDOWN_TEXT_COLOR: RwLock<[f32; 4]> = RwLock::new([0.72305, 0.72305, 0.76008, 1.0]);

static SLIDER_THUMB_COLOR: RwLock<[f32; 4]> = RwLock::new(SLIDER_THUMB);
static SLIDER_THUMB_DRAG_COLOR: RwLock<[f32; 4]> = RwLock::new(SLIDER_THUMB_DRAG);

static RANGE_SLIDER_THUMB_COLOR: RwLock<[f32; 4]> = RwLock::new(SLIDER_THUMB);
static RANGE_SLIDER_THUMB_DRAG_COLOR: RwLock<[f32; 4]> = RwLock::new(SLIDER_THUMB_DRAG);

static TREE_BACKGROUND_COLOR: RwLock<[f32; 4]> = RwLock::new([0.08, 0.08, 0.12, 0.3]);
static TREE_BORDER_COLOR: RwLock<[f32; 4]> = RwLock::new([0.18, 0.18, 0.24, 1.0]);
static TREE_BORDER_HOVER_COLOR: RwLock<[f32; 4]> = RwLock::new([0.25, 0.25, 0.35, 1.0]);
static TREE_BORDER_FOCUS_COLOR: RwLock<[f32; 4]> = RwLock::new([0.30, 0.50, 0.32, 1.0]);
static TREE_OPEN_SEARCH_KEY: RwLock<String> = RwLock::new(String::new());
static LIST_OPEN_SEARCH_KEY: RwLock<String> = RwLock::new(String::new());
static LIST_CLOSE_SEARCH_KEY: RwLock<String> = RwLock::new(String::new());

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
/// Opacity of the graph's NODE-domain content (node bodies, wires, connectors,
/// node text) — `style.surface.graph.node.opacity`, deliberately independent of
/// `GRAPH_OPACITY`, which fades only the pane surface (grid cells/gaps).
static GRAPH_NODE_OPACITY: RwLock<f32> = RwLock::new(1.0);

static GRAPH_NODE_COLOR: RwLock<[f32; 4]> = RwLock::new(NODE_IDLE);
static GRAPH_NODE_SELECTED_COLOR: RwLock<[f32; 4]> = RwLock::new(NODE_SELECTED);
static GRAPH_NODE_DRAG_COLOR: RwLock<[f32; 4]> = RwLock::new(NODE_DRAG);

static GRAPH_WIRE_COLOR: RwLock<[f32; 4]> = RwLock::new([0.1, 0.8, 0.4, 1.0]);
static GRAPH_WIRE_HIGHLIGHT_COLOR: RwLock<[f32; 4]> = RwLock::new([0.0, 1.0, 0.9, 1.0]);

static GRAPH_CONNECTOR_COLOR: RwLock<[f32; 4]> = RwLock::new([0.1, 0.8, 0.4, 1.0]);
static GRAPH_CONNECTOR_HIGHLIGHT_COLOR: RwLock<[f32; 4]> = RwLock::new([0.0, 1.0, 0.9, 1.0]);

pub fn button_background_color() -> [f32; 4] {
    style_read(&BUTTON_BACKGROUND_COLOR)
}

pub fn set_button_background_color(color: [f32; 4]) {
    style_write(&BUTTON_BACKGROUND_COLOR, color);
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
    style_read(&NODE_COLOR)
}

pub fn set_node_color(color: [f32; 4]) {
    style_write(&NODE_COLOR, color);
}

pub fn node_selected_color() -> [f32; 4] {
    load_colors_once();
    style_read(&NODE_SELECTED_COLOR)
}

pub fn set_node_selected_color(color: [f32; 4]) {
    style_write(&NODE_SELECTED_COLOR, color);
}

pub fn node_drag_color() -> [f32; 4] {
    load_colors_once();
    style_read(&NODE_DRAG_COLOR)
}

pub fn set_node_drag_color(color: [f32; 4]) {
    style_write(&NODE_DRAG_COLOR, color);
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

    // `/style/surface/plate/root/...` is the one spelling of the root-plate
    // style (RFC Phase 7a; the legacy `root plate` read-alias was removed
    // 2026-09-06 once every live config had migrated).
    if let Some(radius) = val.pointer("/style/surface/plate/root/corner_radius")
        .and_then(|v| v.as_f64())
    {
        if let Ok(mut lock) = ROOT_PLATE_CORNER_RADIUS.write() {
            *lock = radius as f32;
        }
     }

    if let Some(c) = get_color("/style/surface/plate/root/color") {
        if let Ok(mut lock) = PAGE_LOW_COLOR.write() { *lock = c; }
        if let Ok(mut lock) = ROOT_PLATE_OPACITY.write() { *lock = Some(c[3]); }
    }

    if let Some(c) = get_color("/style/surface/plate/root/menubar/color") {
        if let Ok(mut lock) = ROOT_PLATE_MENUBAR_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/surface/plate/root/menubar/text_color") {
        if let Ok(mut lock) = ROOT_PLATE_MENUBAR_TEXT_COLOR.write() { *lock = c; }
    }
    if let Some(o) = val.pointer("/style/surface/menu/opacity").and_then(|v| v.as_f64()) {
        if let Ok(mut lock) = MENU_OPACITY.write() { *lock = (o as f32).clamp(0.0, 1.0); }
    }

    let menubar_blur_ptr = val.pointer("/style/surface/plate/root/menubar/blur");
    if let Some(blur) = menubar_blur_ptr.and_then(|v| v.as_bool()) {
        if let Ok(mut lock) = ROOT_PLATE_MENUBAR_BLUR.write() { *lock = blur; }
    } else if let Some(blur_val) = menubar_blur_ptr.and_then(|v| v.as_f64()) {
        if let Ok(mut lock) = ROOT_PLATE_MENUBAR_BLUR.write() { *lock = blur_val > 0.001; }
    }

    if let Some(c) = get_color("/style/surface/statusbar/color") {
        if let Ok(mut lock) = ROOT_PLATE_STATUSBAR_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/surface/statusbar/text_color") {
        if let Ok(mut lock) = ROOT_PLATE_STATUSBAR_TEXT_COLOR.write() { *lock = c; }
    }
    if let Some(blur) = val.pointer("/style/surface/statusbar/blur").and_then(|v| v.as_bool()) {
        if let Ok(mut lock) = ROOT_PLATE_STATUSBAR_BLUR.write() { *lock = blur; }
    } else if let Some(blur_val) = val.pointer("/style/surface/statusbar/blur").and_then(|v| v.as_f64()) {
        if let Ok(mut lock) = ROOT_PLATE_STATUSBAR_BLUR.write() { *lock = blur_val > 0.001; }
    }
    if let Some(c) = get_color("/layout/color_borders_color") {
        if let Ok(mut lock) = COLOR_BORDERS_COLOR.write() { *lock = c; }
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
    if let Some(c) = get_color_u8("/style/control/textbox/placeholder_text_color")
        .or_else(|| get_color_u8("/style/textbox/placeholder_text_color"))
        .or_else(|| get_color_u8("/style/data/textbox/placeholder_text_color")) {
        if let Ok(mut lock) = TEXTBOX_PLACEHOLDER_TEXT_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/control/textbox/background_color")
        .or_else(|| get_color("/style/textbox/background_color"))
        .or_else(|| get_color("/style/data/textbox/background_color")) {
        if let Ok(mut lock) = TEXTBOX_BACKGROUND_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/control/textbox/background_edit_color")
        .or_else(|| get_color("/style/textbox/background_edit_color"))
        .or_else(|| get_color("/style/data/textbox/background_edit_color")) {
        if let Ok(mut lock) = TEXTBOX_BACKGROUND_EDIT_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/layout/menubar_tab_label_color").or_else(|| get_color("/layout/paginator_tab_label_color")) {
        if let Ok(mut lock) = MENUBAR_TAB_LABEL_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/control/label/color") {
        if let Ok(mut lock) = CONTROL_LABEL_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/control/label/color_detached") {
        if let Ok(mut lock) = CONTROL_LABEL_COLOR_DETACHED.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/control/label/hover_color") {
        if let Ok(mut lock) = CONTROL_LABEL_HOVER_COLOR.write() { *lock = Some(c); }
    }
    if let Some(c) = get_color("/style/control/label/focus_color") {
        if let Ok(mut lock) = CONTROL_LABEL_FOCUS_COLOR.write() { *lock = Some(c); }
    }
    // The toggle state colors (enabled/disabled/gradient/background) are not
    // read: the control inherits its plate — see the note by the retired
    // getters below.
    if let Some(c) = get_color("/style/control/ramp/background") {
        if let Ok(mut lock) = RAMP_BACKGROUND_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/control/ramp/border_color") {
        if let Ok(mut lock) = RAMP_BORDER_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/control/control_panel/color") {
        if let Ok(mut lock) = CONTROL_PANEL_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/control/control_panel/border_color") {
        if let Ok(mut lock) = CONTROL_PANEL_BORDER_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/control/spinbox/background_color") {
        if let Ok(mut lock) = SPINBOX_DISPLAY_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/control/spinbox/button_color") {
        if let Ok(mut lock) = SPINBOX_BUTTON_COLOR.write() { *lock = c; }
        if let Ok(mut lock_hover) = SPINBOX_BUTTON_HOVER_COLOR.write() {
            *lock_hover = [
                (c[0] + 0.1).min(1.0),
                (c[1] + 0.1).min(1.0),
                (c[2] + 0.1).min(1.0),
                c[3]
            ];
        }
    }
    if let Some(c) = get_color("/style/control/spinbox/button_hover_color") {
        if let Ok(mut lock) = SPINBOX_BUTTON_HOVER_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/control/spinbox/text_color") {
        if let Ok(mut lock) = SPINBOX_TEXT_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/control/button/border_color").or_else(|| get_color("/style/button/border_color")) {
        if let Ok(mut lock) = BUTTON_BORDER_COLOR.write() { *lock = Some(c); }
    }
    if let Some(c) = get_color("/style/control/button/hover_color").or_else(|| get_color("/style/button/hover_color")) {
        if let Ok(mut lock) = BUTTON_HOVER_COLOR.write() { *lock = Some(c); }
    }
    if let Some(c) = get_color("/style/control/dropdown/border_color").or_else(|| get_color("/style/dropdown/border_color")) {
        if let Ok(mut lock) = DROPDOWN_BORDER_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/control/dropdown/text_color").or_else(|| get_color("/style/dropdown/text_color")) {
        if let Ok(mut lock) = DROPDOWN_TEXT_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/control/slider/thumb_color").or_else(|| get_color("/style/slider/thumb_color")) {
        if let Ok(mut lock) = SLIDER_THUMB_COLOR.write() { *lock = c; }
        if let Ok(mut lock_drag) = SLIDER_THUMB_DRAG_COLOR.write() {
            *lock_drag = [
                (c[0] + 0.15).min(1.0),
                (c[1] + 0.15).min(1.0),
                (c[2] + 0.15).min(1.0),
                c[3]
            ];
        }
    }
    if let Some(c) = get_color("/style/control/rangeslider/thumb_color").or_else(|| get_color("/style/rangeslider/thumb_color")) {
        if let Ok(mut lock) = RANGE_SLIDER_THUMB_COLOR.write() { *lock = c; }
        if let Ok(mut lock_drag) = RANGE_SLIDER_THUMB_DRAG_COLOR.write() {
            *lock_drag = [
                (c[0] + 0.15).min(1.0),
                (c[1] + 0.15).min(1.0),
                (c[2] + 0.15).min(1.0),
                c[3]
            ];
        }
    }
    if let Some(c) = get_color("/style/control/progressbar/background") {
        if let Ok(mut lock) = PROGRESS_BG_COLOR.write() { *lock = c; }
    }
    if let Some(c) = get_color("/style/control/progressbar/fill") {
        if let Ok(mut lock) = PROGRESS_FILL_COLOR.write() { *lock = c; }
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
    if let Some(opacity) = val.pointer("/style/surface/graph/node/opacity").and_then(|v| v.as_f64()) {
        if let Ok(mut lock) = GRAPH_NODE_OPACITY.write() { *lock = opacity as f32; }
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
    if let Some(k) = val.pointer("/style/data/list/open_search").and_then(|v| v.as_str()) {
        if let Ok(mut lock) = LIST_OPEN_SEARCH_KEY.write() { *lock = k.to_string(); }
    }
    if let Some(k) = val.pointer("/style/data/list/close_search").and_then(|v| v.as_str()) {
        if let Ok(mut lock) = LIST_CLOSE_SEARCH_KEY.write() { *lock = k.to_string(); }
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
    if let Some(t) = val.pointer("/style/surface/plate/bevel_width").and_then(|v| v.as_f64()) {
        if let Ok(mut lock) = PLATE_BEVEL_WIDTH.write() { *lock = t as f32; }
    }
    if let Some(c) = get_color("/style/surface/param/color") {
        if let Ok(mut lock) = PARAM_BG_COLOR.write() { *lock = c; }
    }
    if let Some(c) = val
        .pointer("/style/surface/plate/backdrop_compression")
        .and_then(|v| v.as_f64())
    {
        if let Ok(mut lock) = PLATE_BACKDROP_COMPRESSION.write() { *lock = (c as f32).clamp(0.0, 1.0); }
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

/// Test-only: the color state is process-global, so any test that calls
/// [`reload_colors`] and then asserts getter values races every other such
/// test on the parallel harness. Each of those tests must hold this lock
/// across its reload + asserts, and should fire the once-per-process config
/// loads ([`load_colors_once`] via any getter, and
/// `layout::lazy_init_style_registry`) inside the lock BEFORE its reload, so
/// neither can rewrite the state from the live config file mid-assert.
#[cfg(test)]
pub(crate) fn test_color_state_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub fn dropdown_background_color() -> [f32; 4] {
    load_colors_once();
    style_read(&DROPDOWN_BACKGROUND_COLOR)
}

pub fn set_dropdown_background_color(color: [f32; 4]) {
    style_write(&DROPDOWN_BACKGROUND_COLOR, color);
}

pub fn textbox_placeholder_text_color() -> [u8; 3] {
    load_colors_once();
    style_read(&TEXTBOX_PLACEHOLDER_TEXT_COLOR)
}

pub fn set_textbox_placeholder_text_color(color: [u8; 3]) {
    style_write(&TEXTBOX_PLACEHOLDER_TEXT_COLOR, color);
}

pub fn textbox_background_color() -> [f32; 4] {
    load_colors_once();
    style_read(&TEXTBOX_BACKGROUND_COLOR)
}

pub fn set_textbox_background_color(color: [f32; 4]) {
    style_write(&TEXTBOX_BACKGROUND_COLOR, color);
}

pub fn textbox_background_edit_color() -> [f32; 4] {
    load_colors_once();
    style_read(&TEXTBOX_BACKGROUND_EDIT_COLOR)
}

pub fn set_textbox_background_edit_color(color: [f32; 4]) {
    style_write(&TEXTBOX_BACKGROUND_EDIT_COLOR, color);
}

pub fn graph_wire_color() -> [f32; 4] {
    load_colors_once();
    style_read(&GRAPH_WIRE_COLOR)
}

pub fn set_graph_wire_color(color: [f32; 4]) {
    style_write(&GRAPH_WIRE_COLOR, color);
}

pub fn graph_wire_highlight_color() -> [f32; 4] {
    load_colors_once();
    style_read(&GRAPH_WIRE_HIGHLIGHT_COLOR)
}

pub fn set_graph_wire_highlight_color(color: [f32; 4]) {
    style_write(&GRAPH_WIRE_HIGHLIGHT_COLOR, color);
}

pub fn graph_connector_color() -> [f32; 4] {
    load_colors_once();
    style_read(&GRAPH_CONNECTOR_COLOR)
}

pub fn set_graph_connector_color(color: [f32; 4]) {
    style_write(&GRAPH_CONNECTOR_COLOR, color);
}

pub fn graph_connector_highlight_color() -> [f32; 4] {
    load_colors_once();
    style_read(&GRAPH_CONNECTOR_HIGHLIGHT_COLOR)
}

pub fn set_graph_connector_highlight_color(color: [f32; 4]) {
    style_write(&GRAPH_CONNECTOR_HIGHLIGHT_COLOR, color);
}

pub fn graph_node_color() -> [f32; 4] {
    load_colors_once();
    style_read(&GRAPH_NODE_COLOR)
}

pub fn set_graph_node_color(color: [f32; 4]) {
    style_write(&GRAPH_NODE_COLOR, color);
}

pub fn graph_node_selected_color() -> [f32; 4] {
    load_colors_once();
    style_read(&GRAPH_NODE_SELECTED_COLOR)
}

pub fn set_graph_node_selected_color(color: [f32; 4]) {
    style_write(&GRAPH_NODE_SELECTED_COLOR, color);
}

pub fn graph_node_drag_color() -> [f32; 4] {
    load_colors_once();
    style_read(&GRAPH_NODE_DRAG_COLOR)
}

pub fn set_graph_node_drag_color(color: [f32; 4]) {
    style_write(&GRAPH_NODE_DRAG_COLOR, color);
}

pub fn graph_cell_color() -> [f32; 3] {
    load_colors_once();
    style_read(&GRAPH_CELL_COLOR)
}

pub fn set_graph_cell_color(color: [f32; 3]) {
    style_write(&GRAPH_CELL_COLOR, color);
}

pub fn graph_gap_color() -> [f32; 3] {
    load_colors_once();
    style_read(&GRAPH_GAP_COLOR)
}

pub fn set_graph_gap_color(color: [f32; 3]) {
    style_write(&GRAPH_GAP_COLOR, color);
}

pub fn graph_opacity() -> f32 {
    load_colors_once();
    style_read(&GRAPH_OPACITY)
}

pub fn graph_node_opacity() -> f32 {
    load_colors_once();
    style_read(&GRAPH_NODE_OPACITY)
}

pub fn set_graph_node_opacity(opacity: f32) {
    style_write(&GRAPH_NODE_OPACITY, opacity);
}

pub fn set_graph_opacity(opacity: f32) {
    style_write(&GRAPH_OPACITY, opacity);
}

pub fn page_low_color() -> [f32; 4] {
    load_colors_once();
    let mut color = *PAGE_LOW_COLOR.read().unwrap();
    if let Some(opacity) = read_root_plate_opacity_if_configured() {
        color[3] = opacity;
    }
    color
}

pub fn set_page_low_color(color: [f32; 4]) {
    style_write(&PAGE_LOW_COLOR, color);
}

pub fn page_color() -> [f32; 4] {
    load_colors_once();
    style_read(&PAGE_COLOR)
}

pub fn set_page_color(color: [f32; 4]) {
    style_write(&PAGE_COLOR, color);
}

pub fn layer_color() -> [f32; 4] {
    load_colors_once();
    style_read(&LAYER_COLOR)
}

pub fn set_layer_color(color: [f32; 4]) {
    style_write(&LAYER_COLOR, color);
}


pub fn color_borders_color() -> [f32; 4] {
    load_colors_once();
    style_read(&COLOR_BORDERS_COLOR)
}

pub fn set_color_borders_color(color: [f32; 4]) {
    style_write(&COLOR_BORDERS_COLOR, color);
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

/// The params pane's plate tint (linear rgba) — [`PARAM_BG`] made live:
/// `style.surface.param.color` in config overrides it, and apps can retint at
/// runtime (the designer's Style section "Plate Color"). Alpha doubles as the
/// frost strength under plate blur.
static PARAM_BG_COLOR: RwLock<[f32; 4]> = RwLock::new(PARAM_BG);

pub fn param_bg_color() -> [f32; 4] {
    load_colors_once();
    style_read(&PARAM_BG_COLOR)
}

pub fn set_param_bg_color(color: [f32; 4]) {
    style_write(&PARAM_BG_COLOR, color);
}

/// The params plate's final fill as the renderer consumes it: the tint scaled
/// by the global plate opacity, alpha negated as the blur-behind marker when
/// plate blur is on. The single source both `ParametersBg`'s own plate and any
/// surface that wants to match it (the designer's node bodies) draw from, so
/// they track a live retint / opacity / blur toggle together.
pub fn param_plate_fill() -> [f32; 4] {
    let mut c = param_bg_color();
    c[3] *= crate::layout::plate_opacity();
    if plate_blur() {
        c[3] = -c[3].abs();
    }
    c
}

pub const PANEL_MENU_BG: [f32; 4] = [0.08, 0.08, 0.12, 1.0];
pub const PANEL_MENU_HOVER: [f32; 4] = [0.18, 0.18, 0.25, 1.0];
pub const PANEL_MENU_FOCUSED: [f32; 4] = [0.08, 0.16, 0.28, 1.0];

pub const SPLITTER_IDLE: [f32; 4] = [0.20, 0.20, 0.27, 1.0];
pub const SPLITTER_HOVER: [f32; 4] = [0.40, 0.40, 0.50, 1.0];
pub const SPLITTER_DRAG: [f32; 4] = [0.50, 0.50, 0.60, 1.0];

pub const TEXT_FG: [f32; 4] = [0.80, 0.80, 0.85, 1.0];
pub const TEXT_DIM: [f32; 4] = [0.53, 0.53, 0.60, 1.0];

/// A canvas well's floor: the plate darkened, not a fill of its own, so every
/// opening you look into — Trackpad, Slider2D, the bevel and ramp previews —
/// is cut from the one material ([`crate::scene::paint::PaintCtx::well_floor`]).
pub const WELL_FLOOR: [f32; 4] = [0.0, 0.0, 0.0, 0.18];
/// [`WELL_FLOOR`] risen toward the plate: a clickable canvas's hover cue.
pub const WELL_FLOOR_LIFTED: [f32; 4] = [0.0, 0.0, 0.0, 0.10];
/// The hairline a well is framed with when relief is off, idle; see
/// [`well_frame_color`].
pub const WELL_FRAME: [f32; 4] = [0.18, 0.18, 0.24, 1.0];
/// [`WELL_FRAME`] under the pointer.
pub const WELL_FRAME_HOVER: [f32; 4] = [0.25, 0.25, 0.35, 1.0];

/// The frame a flat well is drawn in — the one hairline every well (text,
/// keybind, colour and font fields, the canvases) wears when relief is off:
/// neutral greys idle and hovered, the highlight accent while the well is
/// active (editing, recording, pressed) — the colour the relief wells light
/// their rims with, so the two styles share one focus cue. A flat well has no
/// floor of its own any more than a relief one: the plate is the floor.
pub fn well_frame_color(hovered: bool, active: bool) -> [f32; 4] {
    if active {
        let c = highlight_primary_color();
        [c[0], c[1], c[2], 1.0]
    } else if hovered {
        WELL_FRAME_HOVER
    } else {
        WELL_FRAME
    }
}
pub const TEXT_HEADER: [f32; 4] = [0.90, 0.90, 0.95, 1.0];
pub const TEXT_ACCENT: [f32; 4] = [0.56, 0.83, 0.56, 1.0];

pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Parse a hex color string into raw RGBA bytes.
///
/// Accepts `#RRGGBB` or `#RRGGBBAA`, tolerating surrounding quotes/whitespace and
/// an optional leading `#`. 6-digit input yields alpha `255`. Returns `None` for
/// any shorter/invalid input. This is the primitive the `[f32;_]` parsers build on.
pub fn parse_hex_bytes(s: &str) -> Option<[u8; 4]> {
    let hex = s
        .trim_matches(|c| c == '"' || c == '\'' || c == ' ')
        .trim_start_matches('#');
    let b = |i: usize| u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok();
    if hex.len() >= 8 {
        Some([b(0)?, b(1)?, b(2)?, b(3)?])
    } else if hex.len() >= 6 {
        Some([b(0)?, b(1)?, b(2)?, 255])
    } else {
        None
    }
}

/// Parse a hex color string into raw sRGB RGBA in `[0,1]` (no gamma conversion).
/// See [`parse_hex_bytes`] for the accepted formats. Apply [`srgb_to_linear`] to
/// the RGB channels yourself if your render target expects linear color.
pub fn parse_hex_rgba(s: &str) -> Option<[f32; 4]> {
    parse_hex_bytes(s).map(|[r, g, b, a]| {
        [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0]
    })
}

/// Like [`parse_hex_rgba`] but drops alpha, returning raw sRGB RGB in `[0,1]`.
pub fn parse_hex_rgb(s: &str) -> Option<[f32; 3]> {
    parse_hex_rgba(s).map(|[r, g, b, _]| [r, g, b])
}

/// Like [`parse_hex_rgba`] but converts RGB from sRGB to linear (alpha kept as-is).
/// Use when your render target samples colors in linear space.
pub fn parse_hex_rgba_linear(s: &str) -> Option<[f32; 4]> {
    parse_hex_rgba(s).map(|[r, g, b, a]| {
        [srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b), a]
    })
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

/// The alpha that makes an overlay's fade-out read as an EVEN fade to the eye.
///
/// `t` runs 0 (overlay at `max_alpha`) to 1 (fully faded); `overlay` and `backdrop` are
/// linear-light RGB, the backdrop being whatever the fade composites onto.
///
/// A linear alpha ramp is not a linear fade: the surface is an sRGB attachment, so the GPU
/// blends in LINEAR light, and perceived lightness goes as roughly the cube root of that. A
/// straight alpha ramp therefore hangs bright through the middle and then dives near the
/// end. This walks perceived lightness linearly instead and solves back for the alpha that
/// lands on it — exactly, since the composite is linear in alpha:
///
/// ```text
/// Y(a) = a·Y_overlay + (1 - a)·Y_backdrop        (blending, in linear light)
/// L    ≈ cbrt(Y)                                 (perception — OKLab's L, and CIE L* to within a hair)
/// ```
///
/// Solved on luminance rather than per channel because a single alpha can only satisfy one
/// axis, and lightness is the one the eye reads a fade by. With no lightness contrast to
/// shape (a fade between equally-bright colors) it falls back to the linear ramp.
pub fn perceptual_fade_alpha(t: f32, overlay: [f32; 3], backdrop: [f32; 3], max_alpha: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let luminance = |c: [f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    let y_over = luminance(overlay);
    let y_back = luminance(backdrop);
    let contrast = y_over - y_back;
    if contrast.abs() < 1e-4 {
        return max_alpha * (1.0 - t);
    }
    // Walk L from the fully-applied composite down to the bare backdrop.
    let l = |y: f32| y.max(0.0).cbrt();
    let y_full = max_alpha * y_over + (1.0 - max_alpha) * y_back;
    let l_t = l(y_full) + (l(y_back) - l(y_full)) * t;
    let y_t = l_t * l_t * l_t;
    ((y_t - y_back) / contrast).clamp(0.0, max_alpha)
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
    style_write(&SIDEBAR_BG_COLOR, color);
}

pub fn highlight_primary_color() -> [f32; 4] {
    load_colors_once();
    style_read(&HIGHLIGHT_PRIMARY_COLOR)
}

pub fn set_highlight_primary_color(color: [f32; 4]) {
    if let Ok(mut lock) = HIGHLIGHT_PRIMARY_COLOR.write() {
        *lock = [color[0], color[1], color[2], 0.12];
    }
}

pub fn menubar_tab_label_color() -> [f32; 4] {
    load_colors_once();
    style_read(&MENUBAR_TAB_LABEL_COLOR)
}

pub fn set_menubar_tab_label_color(color: [f32; 4]) {
    style_write(&MENUBAR_TAB_LABEL_COLOR, color);
}

pub fn read_opacity_if_configured() -> Option<f32> {
    load_colors_once();
    style_read(&OPACITY)
}

/// Tint strength for frosted menus/popovers (`/style/surface/menu/opacity`,
/// default 0.8): the |alpha| of the blur-behind sentinel their plates carry.
pub fn menu_opacity() -> f32 {
    load_colors_once();
    style_read(&MENU_OPACITY)
}

pub fn read_root_plate_opacity_if_configured() -> Option<f32> {
    load_colors_once();
    style_read(&ROOT_PLATE_OPACITY)
}

// The toggle's own palette (enabled/disabled/background) is RETIRED: a toggle
// inherits the plate it sits on and marks state with light and relief, so
// there is nothing left to tint. The `TOGGLE_*` consts above survive for the
// graph's node geometry switch, which is a different control.

pub fn list_bg_color() -> [f32; 4] {
    load_colors_once();
    style_read(&LIST_BG_COLOR)
}

pub fn set_list_bg_color(color: [f32; 4]) {
    if let Ok(mut lock) = LIST_BG_COLOR.write() {
        *lock = [color[0], color[1], color[2], 0.3];
    }
}

pub fn list_entry_bg_color() -> [f32; 4] {
    load_colors_once();
    style_read(&LIST_ENTRY_BG_COLOR)
}

pub fn set_list_entry_bg_color(color: [f32; 4]) {
    style_write(&LIST_ENTRY_BG_COLOR, color);
}

pub fn list_entry_highlight_color() -> [f32; 4] {
    load_colors_once();
    style_read(&LIST_ENTRY_HIGHLIGHT_COLOR)
}

pub fn set_list_entry_highlight_color(color: [f32; 4]) {
    style_write(&LIST_ENTRY_HIGHLIGHT_COLOR, color);
}

pub fn list_font_color() -> [f32; 4] {
    load_colors_once();
    style_read(&LIST_FONT_COLOR)
}

pub fn set_list_font_color(color: [f32; 4]) {
    style_write(&LIST_FONT_COLOR, color);
}

pub fn breadcrumb_bg_color() -> [f32; 4] {
    load_colors_once();
    style_read(&BREADCRUMB_BG_COLOR)
}

pub fn set_breadcrumb_bg_color(color: [f32; 4]) {
    if let Ok(mut lock) = BREADCRUMB_BG_COLOR.write() {
        *lock = [color[0], color[1], color[2], 1.0];
    }
}

pub fn popover_bg_color() -> [f32; 4] {
    load_colors_once();
    style_read(&POPOVER_BG_COLOR)
}

pub fn set_popover_bg_color(color: [f32; 4]) {
    if let Ok(mut lock) = POPOVER_BG_COLOR.write() {
        *lock = [color[0], color[1], color[2], 1.0];
    }
}

/// Corner radius of the root plate (`style.surface.plate.root.corner_radius`).
/// The window silhouette value — the compositor clips windows from the SHARED
/// copy of this.
pub fn root_plate_corner_radius() -> f32 {
    load_colors_once();
    style_read(&ROOT_PLATE_CORNER_RADIUS)
}

pub fn ramp_background_color() -> [f32; 4] {
    load_colors_once();
    style_read(&RAMP_BACKGROUND_COLOR)
}

pub fn ramp_border_color() -> [f32; 4] {
    load_colors_once();
    style_read(&RAMP_BORDER_COLOR)
}

pub fn control_panel_color() -> [f32; 4] {
    load_colors_once();
    style_read(&CONTROL_PANEL_COLOR)
}

pub fn set_control_panel_color(color: [f32; 4]) {
    style_write(&CONTROL_PANEL_COLOR, color);
}

pub fn control_panel_border_color() -> [f32; 4] {
    load_colors_once();
    style_read(&CONTROL_PANEL_BORDER_COLOR)
}

pub fn set_control_panel_border_color(color: [f32; 4]) {
    style_write(&CONTROL_PANEL_BORDER_COLOR, color);
}

pub fn progress_bg() -> [f32; 4] {
    load_colors_once();
    style_read(&PROGRESS_BG_COLOR)
}

pub fn set_progress_bg(color: [f32; 4]) {
    style_write(&PROGRESS_BG_COLOR, color);
}

pub fn progress_fill() -> [f32; 4] {
    load_colors_once();
    style_read(&PROGRESS_FILL_COLOR)
}

pub fn set_progress_fill(color: [f32; 4]) {
    style_write(&PROGRESS_FILL_COLOR, color);
}

pub fn button_border_color() -> Option<[f32; 4]> {
    load_colors_once();
    style_read(&BUTTON_BORDER_COLOR)
}

pub fn set_button_border_color(color: [f32; 4]) {
    if let Ok(mut lock) = BUTTON_BORDER_COLOR.write() {
        *lock = Some(color);
    }
}

pub fn dropdown_border_color() -> [f32; 4] {
    load_colors_once();
    style_read(&DROPDOWN_BORDER_COLOR)
}

pub fn set_dropdown_border_color(color: [f32; 4]) {
    style_write(&DROPDOWN_BORDER_COLOR, color);
}

pub fn dropdown_text_color() -> [f32; 4] {
    load_colors_once();
    style_read(&DROPDOWN_TEXT_COLOR)
}

pub fn set_dropdown_text_color(color: [f32; 4]) {
    style_write(&DROPDOWN_TEXT_COLOR, color);
}


pub fn slider_thumb() -> [f32; 4] {
    load_colors_once();
    style_read(&SLIDER_THUMB_COLOR)
}

pub fn set_slider_thumb(color: [f32; 4]) {
    style_write(&SLIDER_THUMB_COLOR, color);
}

pub fn slider_thumb_drag() -> [f32; 4] {
    load_colors_once();
    style_read(&SLIDER_THUMB_DRAG_COLOR)
}

pub fn set_slider_thumb_drag(color: [f32; 4]) {
    style_write(&SLIDER_THUMB_DRAG_COLOR, color);
}

pub fn rangeslider_thumb() -> [f32; 4] {
    load_colors_once();
    style_read(&RANGE_SLIDER_THUMB_COLOR)
}

pub fn set_rangeslider_thumb(color: [f32; 4]) {
    style_write(&RANGE_SLIDER_THUMB_COLOR, color);
}

pub fn rangeslider_thumb_drag() -> [f32; 4] {
    load_colors_once();
    style_read(&RANGE_SLIDER_THUMB_DRAG_COLOR)
}

pub fn set_rangeslider_thumb_drag(color: [f32; 4]) {
    style_write(&RANGE_SLIDER_THUMB_DRAG_COLOR, color);
}

pub fn spinbox_display() -> [f32; 4] {
    load_colors_once();
    style_read(&SPINBOX_DISPLAY_COLOR)
}

pub fn set_spinbox_display(color: [f32; 4]) {
    style_write(&SPINBOX_DISPLAY_COLOR, color);
}

pub fn spinbox_button() -> [f32; 4] {
    load_colors_once();
    style_read(&SPINBOX_BUTTON_COLOR)
}

pub fn set_spinbox_button(color: [f32; 4]) {
    style_write(&SPINBOX_BUTTON_COLOR, color);
}

pub fn spinbox_button_hover() -> [f32; 4] {
    load_colors_once();
    style_read(&SPINBOX_BUTTON_HOVER_COLOR)
}

pub fn set_spinbox_button_hover(color: [f32; 4]) {
    style_write(&SPINBOX_BUTTON_HOVER_COLOR, color);
}

pub fn spinbox_text_color() -> [f32; 4] {
    load_colors_once();
    style_read(&SPINBOX_TEXT_COLOR)
}

pub fn set_spinbox_text_color(color: [f32; 4]) {
    style_write(&SPINBOX_TEXT_COLOR, color);
}

pub fn set_root_plate_corner_radius(radius: f32) {
    style_write(&ROOT_PLATE_CORNER_RADIUS, radius);
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

/// Opacity of the root plate's fill (the alpha of the root-plate color).
pub fn root_plate_opacity() -> f32 {
    page_low_color()[3]
}

pub fn tree_background_color() -> [f32; 4] {
    style_read(&TREE_BACKGROUND_COLOR)
}
pub fn tree_border_color() -> [f32; 4] {
    style_read(&TREE_BORDER_COLOR)
}
pub fn tree_border_hover_color() -> [f32; 4] {
    style_read(&TREE_BORDER_HOVER_COLOR)
}
pub fn tree_border_focus_color() -> [f32; 4] {
    style_read(&TREE_BORDER_FOCUS_COLOR)
}

pub fn tree_section_bg_color() -> [f32; 4] {
    style_read(&TREE_SECTION_BG_COLOR)
}
pub fn tree_section_bg_hover_color() -> [f32; 4] {
    style_read(&TREE_SECTION_BG_HOVER_COLOR)
}

pub fn tree_leaf_bg_even_color() -> [f32; 4] {
    style_read(&TREE_LEAF_BG_EVEN_COLOR)
}
pub fn tree_leaf_bg_odd_color() -> [f32; 4] {
    style_read(&TREE_LEAF_BG_ODD_COLOR)
}
pub fn tree_leaf_bg_hover_color() -> [f32; 4] {
    style_read(&TREE_LEAF_BG_HOVER_COLOR)
}
pub fn tree_leaf_bg_selected_color() -> [f32; 4] {
    style_read(&TREE_LEAF_BG_SELECTED_COLOR)
}

pub fn tree_section_text_color() -> [f32; 4] {
    style_read(&TREE_SECTION_TEXT_COLOR)
}
pub fn tree_leaf_text_color() -> [f32; 4] {
    style_read(&TREE_LEAF_TEXT_COLOR)
}
pub fn tree_leaf_text_selected_color() -> [f32; 4] {
    style_read(&TREE_LEAF_TEXT_SELECTED_COLOR)
}

pub fn tree_type_text_color() -> [f32; 4] {
    style_read(&TREE_TYPE_TEXT_COLOR)
}
pub fn tree_value_text_color() -> [f32; 4] {
    style_read(&TREE_VALUE_TEXT_COLOR)
}
pub fn tree_separator_color() -> [f32; 4] {
    style_read(&TREE_SEPARATOR_COLOR)
}

pub fn set_tree_background_color(c: [f32; 4]) {
    style_write(&TREE_BACKGROUND_COLOR, c);
}
pub fn set_tree_border_color(c: [f32; 4]) {
    style_write(&TREE_BORDER_COLOR, c);
}
pub fn set_tree_border_hover_color(c: [f32; 4]) {
    style_write(&TREE_BORDER_HOVER_COLOR, c);
}
pub fn set_tree_border_focus_color(c: [f32; 4]) {
    style_write(&TREE_BORDER_FOCUS_COLOR, c);
}

pub fn set_tree_section_bg_color(c: [f32; 4]) {
    style_write(&TREE_SECTION_BG_COLOR, c);
}
pub fn set_tree_section_bg_hover_color(c: [f32; 4]) {
    style_write(&TREE_SECTION_BG_HOVER_COLOR, c);
}

pub fn set_tree_leaf_bg_even_color(c: [f32; 4]) {
    style_write(&TREE_LEAF_BG_EVEN_COLOR, c);
}
pub fn set_tree_leaf_bg_odd_color(c: [f32; 4]) {
    style_write(&TREE_LEAF_BG_ODD_COLOR, c);
}
pub fn set_tree_leaf_bg_hover_color(c: [f32; 4]) {
    style_write(&TREE_LEAF_BG_HOVER_COLOR, c);
}
pub fn set_tree_leaf_bg_selected_color(c: [f32; 4]) {
    style_write(&TREE_LEAF_BG_SELECTED_COLOR, c);
}

pub fn set_tree_section_text_color(c: [f32; 4]) {
    style_write(&TREE_SECTION_TEXT_COLOR, c);
}
pub fn set_tree_leaf_text_color(c: [f32; 4]) {
    style_write(&TREE_LEAF_TEXT_COLOR, c);
}
pub fn set_tree_leaf_text_selected_color(c: [f32; 4]) {
    style_write(&TREE_LEAF_TEXT_SELECTED_COLOR, c);
}

pub fn set_tree_type_text_color(c: [f32; 4]) {
    style_write(&TREE_TYPE_TEXT_COLOR, c);
}
pub fn set_tree_value_text_color(c: [f32; 4]) {
    style_write(&TREE_VALUE_TEXT_COLOR, c);
}
pub fn set_tree_separator_color(c: [f32; 4]) {
    style_write(&TREE_SEPARATOR_COLOR, c);
}

pub fn scrollbar_track_color() -> [f32; 4] {
    style_read(&SCROLLBAR_TRACK_COLOR)
}
pub fn set_scrollbar_track_color(c: [f32; 4]) {
    style_write(&SCROLLBAR_TRACK_COLOR, c);
}
pub fn scrollbar_thumb_color() -> [f32; 4] {
    style_read(&SCROLLBAR_THUMB_COLOR)
}
pub fn set_scrollbar_thumb_color(c: [f32; 4]) {
    style_write(&SCROLLBAR_THUMB_COLOR, c);
}

pub fn tree_open_search_key() -> String {
    load_colors_once();
    let legacy = TREE_OPEN_SEARCH_KEY.read().unwrap().clone();
    crate::input::widget_chord("open_search", &legacy, "ctrl+f")
}
pub fn set_tree_open_search_key(k: String) {
    style_write(&TREE_OPEN_SEARCH_KEY, k);
}

pub fn list_open_search_key() -> String {
    load_colors_once();
    let legacy = LIST_OPEN_SEARCH_KEY.read().unwrap().clone();
    crate::input::widget_chord("open_search", &legacy, "ctrl+f")
}
pub fn set_list_open_search_key(k: String) {
    style_write(&LIST_OPEN_SEARCH_KEY, k);
}

pub fn list_close_search_key() -> String {
    load_colors_once();
    let legacy = LIST_CLOSE_SEARCH_KEY.read().unwrap().clone();
    crate::input::widget_chord("close_search", &legacy, "escape")
}
pub fn set_list_close_search_key(k: String) {
    style_write(&LIST_CLOSE_SEARCH_KEY, k);
}

pub fn root_plate_menubar_color() -> [f32; 4] {
    load_colors_once();
    style_read(&ROOT_PLATE_MENUBAR_COLOR)
}

pub fn set_root_plate_menubar_color(c: [f32; 4]) {
    style_write(&ROOT_PLATE_MENUBAR_COLOR, c);
}

pub fn root_plate_menubar_text_color() -> [f32; 4] {
    load_colors_once();
    style_read(&ROOT_PLATE_MENUBAR_TEXT_COLOR)
}

pub fn set_root_plate_menubar_text_color(c: [f32; 4]) {
    style_write(&ROOT_PLATE_MENUBAR_TEXT_COLOR, c);
}

pub fn root_plate_menubar_blur() -> bool {
    load_colors_once();
    style_read(&ROOT_PLATE_MENUBAR_BLUR)
}

pub fn set_root_plate_menubar_blur(b: bool) {
    style_write(&ROOT_PLATE_MENUBAR_BLUR, b);
}

pub fn root_plate_statusbar_color() -> [f32; 4] {
    load_colors_once();
    style_read(&ROOT_PLATE_STATUSBAR_COLOR)
}

pub fn set_root_plate_statusbar_color(c: [f32; 4]) {
    style_write(&ROOT_PLATE_STATUSBAR_COLOR, c);
}

pub fn root_plate_statusbar_text_color() -> [f32; 4] {
    load_colors_once();
    style_read(&ROOT_PLATE_STATUSBAR_TEXT_COLOR)
}

pub fn set_root_plate_statusbar_text_color(c: [f32; 4]) {
    style_write(&ROOT_PLATE_STATUSBAR_TEXT_COLOR, c);
}

pub fn root_plate_statusbar_blur() -> bool {
    load_colors_once();
    style_read(&ROOT_PLATE_STATUSBAR_BLUR)
}

pub fn set_root_plate_statusbar_blur(b: bool) {
    style_write(&ROOT_PLATE_STATUSBAR_BLUR, b);
}

static PLATE_COLOR: RwLock<Option<[f32; 4]>> = RwLock::new(Some([0.15, 0.15, 0.2, 0.95]));
static PLATE_BORDER_COLOR: RwLock<Option<[f32; 4]>> = RwLock::new(Some([0.3, 0.3, 0.4, 1.0]));
static PLATE_BORDER_THICKNESS: RwLock<f32> = RwLock::new(1.0);
/// Roll width of the beveled plate border (the control_relief replacement for
/// the flat border line) — `style.surface.plate.bevel_width`.
static PLATE_BEVEL_WIDTH: RwLock<f32> = RwLock::new(6.0);

pub fn plate_color() -> Option<[f32; 4]> {
    load_colors_once();
    if let Ok(lock) = PLATE_COLOR.read() {
        return *lock;
    }
    None
}

pub fn set_plate_color(c: Option<[f32; 4]>) {
    style_write(&PLATE_COLOR, c);
}

pub fn plate_border_color() -> Option<[f32; 4]> {
    load_colors_once();
    if let Ok(lock) = PLATE_BORDER_COLOR.read() {
        return *lock;
    }
    None
}

pub fn set_plate_border_color(c: Option<[f32; 4]>) {
    style_write(&PLATE_BORDER_COLOR, c);
}

pub fn plate_border_thickness() -> f32 {
    load_colors_once();
    if let Ok(lock) = PLATE_BORDER_THICKNESS.read() {
        return *lock;
    }
    1.0
}

pub fn set_plate_border_thickness(t: f32) {
    style_write(&PLATE_BORDER_THICKNESS, t);
}

pub fn plate_bevel_width() -> f32 {
    load_colors_once();
    if let Ok(lock) = PLATE_BEVEL_WIDTH.read() {
        return *lock;
    }
    6.0
}

pub fn set_plate_bevel_width(t: f32) {
    style_write(&PLATE_BEVEL_WIDTH, t);
}

/// How hard a frosted plate pulls its backdrop's LUMINANCE toward its own key
/// before tinting: 0 = the backdrop passes through untouched, 1 = flat.
///
/// This is the plate's legibility control, and it is NOT opacity. Blur
/// destroys a backdrop's spatial detail but preserves its mean luminance, and
/// text contrast is a mean-luminance property — so a frosted plate over
/// something bright washes out however hard it is blurred, which is the whole
/// of the "liquid glass" legibility problem. Compression remaps the backdrop's
/// luminance toward the plate's own, symmetrically: a bright backdrop comes
/// down and a DARK one comes up, so the plate stops swinging through the ink's
/// luminance while its hue, chroma and movement still read.
///
/// Default 0.0 — the behavior every existing config already has.
static PLATE_BACKDROP_COMPRESSION: RwLock<f32> = RwLock::new(0.0);

pub fn plate_backdrop_compression() -> f32 {
    load_colors_once();
    style_read(&PLATE_BACKDROP_COMPRESSION)
}

pub fn set_plate_backdrop_compression(c: f32) {
    style_write(&PLATE_BACKDROP_COMPRESSION, c.clamp(0.0, 1.0));
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
    style_write(&PLATE_BLUR, b);
}

pub fn control_label_color() -> [f32; 4] {
    style_read(&CONTROL_LABEL_COLOR)
}

pub fn control_label_color_u8() -> [u8; 3] {
    let c = control_label_color();
    [
        (linear_to_srgb(c[0]) * 255.0).round() as u8,
        (linear_to_srgb(c[1]) * 255.0).round() as u8,
        (linear_to_srgb(c[2]) * 255.0).round() as u8,
    ]
}

pub fn set_control_label_color(color: [f32; 4]) {
    style_write(&CONTROL_LABEL_COLOR, color);
}

pub fn control_label_hover_color() -> Option<[f32; 4]> {
    style_read(&CONTROL_LABEL_HOVER_COLOR)
}

pub fn control_label_focus_color() -> Option<[f32; 4]> {
    style_read(&CONTROL_LABEL_FOCUS_COLOR)
}

pub fn control_label_color_for_state(hovered: bool, focused: bool) -> [u8; 3] {
    let c = if focused {
        control_label_focus_color().unwrap_or_else(|| control_label_color())
    } else if hovered {
        control_label_hover_color().unwrap_or_else(|| control_label_color())
    } else {
        control_label_color()
    };
    [
        (linear_to_srgb(c[0]) * 255.0).round() as u8,
        (linear_to_srgb(c[1]) * 255.0).round() as u8,
        (linear_to_srgb(c[2]) * 255.0).round() as u8,
    ]
}

pub fn control_label_color_detached() -> [f32; 4] {
    style_read(&CONTROL_LABEL_COLOR_DETACHED)
}

pub fn control_label_color_detached_u8() -> [u8; 3] {
    let c = control_label_color_detached();
    [
        (linear_to_srgb(c[0]) * 255.0).round() as u8,
        (linear_to_srgb(c[1]) * 255.0).round() as u8,
        (linear_to_srgb(c[2]) * 255.0).round() as u8,
    ]
}

pub fn set_control_label_color_detached(color: [f32; 4]) {
    style_write(&CONTROL_LABEL_COLOR_DETACHED, color);
}

pub fn control_label_color_detached_for_state(hovered: bool, focused: bool) -> [u8; 3] {
    let c = if focused {
        control_label_focus_color().unwrap_or_else(|| control_label_color_detached())
    } else if hovered {
        control_label_hover_color().unwrap_or_else(|| control_label_color_detached())
    } else {
        control_label_color_detached()
    };
    [
        (linear_to_srgb(c[0]) * 255.0).round() as u8,
        (linear_to_srgb(c[1]) * 255.0).round() as u8,
        (linear_to_srgb(c[2]) * 255.0).round() as u8,
    ]
}

#[cfg(test)]
mod color_tests {
    use super::*;

    #[test]
    fn test_parse_hex_rgba() {
        // 6-digit → alpha 1.0, raw sRGB
        assert_eq!(parse_hex_rgba("#ff0000"), Some([1.0, 0.0, 0.0, 1.0]));
        // 8-digit → explicit alpha
        assert_eq!(parse_hex_rgba("#00ff0080"), Some([0.0, 1.0, 0.0, 128.0 / 255.0]));
        // leading '#' optional; quotes/whitespace tolerated
        assert_eq!(parse_hex_rgba("\" ffffff \""), Some([1.0, 1.0, 1.0, 1.0]));
        assert_eq!(parse_hex_rgba("000000"), Some([0.0, 0.0, 0.0, 1.0]));
        // invalid
        assert_eq!(parse_hex_rgba("#fff"), None);
        assert_eq!(parse_hex_rgba("nothex"), None);
        assert_eq!(parse_hex_rgba(""), None);
        // rgb drops alpha; linear applies gamma to rgb only
        assert_eq!(parse_hex_rgb("#ff0000"), Some([1.0, 0.0, 0.0]));
        assert_eq!(parse_hex_rgba_linear("#000000ff"), Some([0.0, 0.0, 0.0, 1.0]));
        // byte primitive
        assert_eq!(parse_hex_bytes("#010203"), Some([1, 2, 3, 255]));
        assert_eq!(parse_hex_bytes("#01020304"), Some([1, 2, 3, 4]));
        assert_eq!(parse_hex_bytes("#fff"), None);
    }

    #[test]
    fn perceptual_fade_alpha_walks_lightness_not_luminance() {
        let overlay = [0.62, 0.70, 0.95];
        let backdrop = [0.028, 0.028, 0.041];
        let a = |t: f32| perceptual_fade_alpha(t, overlay, backdrop, 1.0);

        // Endpoints are the plain ones, and the ramp only ever falls.
        assert!((a(0.0) - 1.0).abs() < 1e-4);
        assert!(a(1.0).abs() < 1e-4);
        for i in 1..=20 {
            assert!(a(i as f32 / 20.0) <= a((i - 1) as f32 / 20.0));
        }

        // The correction runs BELOW the straight ramp — that ramp's excess brightness
        // through the middle is the bow the eye reads as non-linear.
        assert!(a(0.5) < 0.5 - 0.1, "midpoint {} should sit well under 0.5", a(0.5));

        // What it buys: composited lightness lands on a straight line. cbrt(luminance) is
        // the same proxy the implementation uses, checked here end to end through blending.
        let lum = |c: [f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        let l_at = |t: f32| {
            let a = a(t);
            let y: f32 = a * lum(overlay) + (1.0 - a) * lum(backdrop);
            y.cbrt()
        };
        let (top, bottom) = (l_at(0.0), l_at(1.0));
        for i in 0..=10 {
            let t = i as f32 / 10.0;
            let ideal = top + (bottom - top) * t;
            assert!((l_at(t) - ideal).abs() < 1e-3, "t={t}: {} vs {ideal}", l_at(t));
        }

        // No lightness contrast to shape: falls back to the straight ramp.
        let flat = perceptual_fade_alpha(0.5, overlay, overlay, 1.0);
        assert!((flat - 0.5).abs() < 1e-4);
    }

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


#[cfg(test)]
mod tests {
    use super::*;

    /// The plate's backdrop compression defaults OFF and clamps.
    ///
    /// Off is load-bearing: it is the behaviour every config already in the
    /// wild has, and a toolkit-wide default that changed how every frosted
    /// surface in the DE looks would arrive unannounced in eighteen apps.
    /// Opting in is a per-app `style.surface.plate.backdrop_compression`.
    #[test]
    fn backdrop_compression_defaults_off_and_clamps() {
        assert_eq!(plate_backdrop_compression(), 0.0, "off unless a config asks");

        set_plate_backdrop_compression(0.85);
        assert_eq!(plate_backdrop_compression(), 0.85);

        // The shader clamps too, but a nonsense config value should not be
        // able to reach it and make the plate flat or inverted.
        set_plate_backdrop_compression(4.0);
        assert_eq!(plate_backdrop_compression(), 1.0);
        set_plate_backdrop_compression(-1.0);
        assert_eq!(plate_backdrop_compression(), 0.0);
    }

    /// A colour pinned by one test is invisible to a test beside it.
    ///
    /// The invariant that ends the parallel-flake class here. These statics
    /// are process-wide, and `test_graph_style_configuration` pins a dozen of
    /// them without restoring any; before `style_write` became per-thread
    /// under `cfg(test)`, every test running alongside it saw those values.
    #[test]
    fn a_pinned_colour_is_private_to_its_thread() {
        let base = node_color();
        let pinned = [0.123, 0.456, 0.789, 1.0];
        assert_ne!(base, pinned, "pick a value the config cannot already hold");

        set_node_color(pinned);
        assert_eq!(node_color(), pinned, "the pinning thread sees its own value");

        let elsewhere = std::thread::spawn(node_color).join().unwrap();
        assert_eq!(elsewhere, base, "a thread beside it must still see the shared base");
    }
}
