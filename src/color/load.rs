//! Loading the colour and surface config: reading `config.kdl` into the colour statics
//! (`parse_and_set_colors`), reporting retired surface keys, and the reload entry points.

use super::*;

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
    if let Some(c) = val.pointer("/style/surface/menu/compression").and_then(|v| v.as_f64()) {
        if let Ok(mut lock) = MENU_COMPRESSION.write() { *lock = (c as f32).clamp(0.0, 1.0); }
    }
    if let Some(c) = get_color("/style/surface/menu/color") {
        if let Ok(mut lock) = MENU_COLOR.write() { *lock = Some(c); }
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

    // The lattice's lines. The cells between them have no colour of their
    // own: a graph shows whatever it is painted on. `cell_color` and
    // `gap_color`, the cell model's pair, are retired (`retired_surface_keys`).
    if let Some(c) = get_color("/style/surface/graph/grid_color") {
        if let Ok(mut lock) = GRAPH_GRID_COLOR.write() { *lock = [c[0], c[1], c[2]]; }
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
    if let Some(c) = get_color("/style/surface/param/color") {
        if let Ok(mut lock) = PARAM_BG_COLOR.write() { *lock = c; }
        if let Ok(mut lock) = PANE_COLOR_WHOLE.write() { *lock = false; }
    }
    // The clear spelling of the same tint: the PANE plate's colour, whole —
    // its alpha is the tint strength and nothing multiplies it.
    if let Some(c) = get_color("/style/surface/plate/pane/color") {
        if let Ok(mut lock) = PARAM_BG_COLOR.write() { *lock = c; }
        if let Ok(mut lock) = PANE_COLOR_WHOLE.write() { *lock = true; }
    }
    // The default plate's frost is ONE block, the same shape a named
    // material's `frost` child has: `plate { frost radius=5.5 compression=0
    // refraction=0 }` is frosted with those knobs, a bare `frost` is frosted
    // at the defaults, `frost (bool)false` is not frosted. The four scattered
    // keys it replaced on 2026-09-28 (`blur` as the switch, `radius`,
    // `backdrop_compression`, `refraction`) were read as aliases for the rest
    // of that day and are retired: a config still carrying one is reported
    // (`retired_surface_keys`) and the key does nothing, because an alias that
    // keeps working is a second spelling forever. The block is absent → not
    // frosted, which is what `blur=true` alone now amounts to; the report
    // is what tells the user why their plates went sharp.
    let retired = retired_surface_keys(&val);
    if !retired.is_empty() {
        log::warn!(
            "retired style.surface keys in config: {} — the frost is `plate {{ frost radius= compression= refraction= }}` (a material's `frost` child spells `compression`), every roll's width is `relief width=`, the light strength is `relief light=` (a material's `finish light=`), and the relief's geometry is `relief {{ wall height= profile= ; edge height= profile= }}` (the window_manager bevel_* spellings are the same relief keys, and its bevel_shader is `relief shader=`); a graph's lines are `graph {{ grid_color }}` and its cells take no colour",
            retired.join(", ")
        );
    }
    match val.pointer("/style/surface/plate/frost") {
        Some(serde_json::Value::Object(fo)) => {
            let num = |k: &str| fo.get(k).and_then(|v| v.as_f64()).map(|v| v as f32);
            if let Ok(mut lock) = PLATE_BLUR.write() { *lock = true; }
            if let Some(c) = num("compression") {
                if let Ok(mut lock) = PLATE_BACKDROP_COMPRESSION.write() { *lock = c.clamp(0.0, 1.0); }
            }
            if let Some(r) = num("refraction") {
                if let Ok(mut lock) = PLATE_REFRACTION.write() { *lock = r.clamp(0.0, 1.0); }
            }
            if let Some(r) = num("radius") {
                if let Ok(mut lock) = PLATE_FROST_RADIUS.write() { *lock = r.max(0.0); }
            }
        }
        Some(serde_json::Value::Null) => {
            if let Ok(mut lock) = PLATE_BLUR.write() { *lock = true; }
        }
        Some(serde_json::Value::Bool(on)) => {
            if let Ok(mut lock) = PLATE_BLUR.write() { *lock = *on; }
        }
        _ => {}
    }

    // The DE finish beyond its strength (`relief.depth` / `light`, which
    // lives in the style registry): the three terms that were literals.
    let f = |k: &str| val.pointer(&format!("/style/surface/relief/{k}")).and_then(|v| v.as_f64()).map(|v| v as f32);
    if let Some(v) = f("spec") {
        if let Ok(mut lock) = FINISH_SPEC.write() { *lock = v.max(0.0); }
    }
    if let Some(v) = f("shininess") {
        if let Ok(mut lock) = FINISH_SHININESS.write() { *lock = v.max(1.0); }
    }
    if let Some(v) = f("curvature") {
        if let Ok(mut lock) = FINISH_CURVATURE.write() { *lock = v.max(0.0); }
    }

    // Named materials and the rung bindings (RFC material § 5), replaced
    // wholesale so a reload forgets what config no longer says.
    {
        use crate::scene::material::{FrostDef, MaterialDef};
        let num = |v: Option<&serde_json::Value>| v.and_then(|v| v.as_f64()).map(|v| v as f32);
        let mut map: Vec<(String, MaterialDef)> = Vec::new();
        if let Some(obj) = val.pointer("/style/surface/material").and_then(|v| v.as_object()) {
            for (name, node) in obj {
                let Some(node) = node.as_object() else { continue };
                let frost = node.get("frost").map(|fr| match fr.as_object() {
                    Some(fo) => FrostDef {
                        compression: num(fo.get("compression")),
                        refraction: num(fo.get("refraction")),
                        radius: num(fo.get("radius")),
                    },
                    // A bare `frost` node (no knobs) is frosted at the defaults.
                    None => FrostDef::default(),
                });
                let fin = node.get("finish").and_then(|v| v.as_object());
                let fk = |k: &str| fin.and_then(|fo| num(fo.get(k)));
                map.push((
                    name.clone(),
                    MaterialDef {
                        tint: node.get("color").and_then(|v| v.as_str()).and_then(parse_hex),
                        frost,
                        light: fk("light"),
                        spec: fk("spec"),
                        shininess: fk("shininess"),
                        curvature: fk("curvature"),
                    },
                ));
            }
        }
        if let Ok(mut lock) = NAMED_MATERIALS.write() { *lock = map; }
        let bind = |p: &str| val.pointer(p).and_then(|v| v.as_str()).map(|s| s.to_string()).filter(|s| !s.is_empty());
        let bindings = [
            bind("/style/surface/plate/root/material"),
            bind("/style/surface/plate/material"),
            bind("/style/control/material"),
        ];
        if let Ok(mut lock) = MATERIAL_BINDINGS.write() { *lock = bindings; }
    }
}

/// The retired `style.surface` spellings a parsed config still carries, as
/// the dotted paths a user would grep for: the four flat frost keys under
/// `plate` (`blur`, `radius`, `backdrop_compression`, `refraction`),
/// `backdrop_compression` inside any `frost` child (the plate's, or a named
/// material's), whose one name is `compression`; `plate.bevel_width`, the
/// pane roll's former width of its own (every roll is `relief.width`); and
/// `relief.depth` with `depth` inside any material's `finish` child, the
/// light strength's former name (`light`, since it is not a length); and
/// the relief's flat geometry keys `height` / `profile` (the wall's, now
/// `wall.height` / `wall.profile`) and `edge_height` / `edge_profile` (the
/// edge's, now `edge.height` / `edge.profile`); and `window_manager.bevel_depth`
/// / `bevel_width` / `bevel_shader`, the block the relief keys were born in
/// before `style.surface.relief` existed (the compositor never read them;
/// the shader toggle is `relief.shader` now). Each was an alias for part
/// of 2026-09-28 and is not read now. And the graph's `cell_color`,
/// `gap_color` and `uniform_background` (2026-09-29): a graph's cells have
/// no colour, so there is no fill to switch, and its lines are
/// `graph.grid_color`. Empty for a clean config.
pub fn retired_surface_keys(val: &serde_json::Value) -> Vec<String> {
    let mut found = Vec::new();
    for k in ["blur", "radius", "backdrop_compression", "refraction", "bevel_width"] {
        if val.pointer(&format!("/style/surface/plate/{k}")).is_some() {
            found.push(format!("style.surface.plate.{k}"));
        }
    }
    if val.pointer("/style/surface/plate/frost/backdrop_compression").is_some() {
        found.push("style.surface.plate.frost.backdrop_compression".to_string());
    }
    for k in ["depth", "height", "profile", "edge_height", "edge_profile"] {
        if val.pointer(&format!("/style/surface/relief/{k}")).is_some() {
            found.push(format!("style.surface.relief.{k}"));
        }
    }
    for k in ["bevel_depth", "bevel_width", "bevel_shader"] {
        if val.pointer(&format!("/window_manager/{k}")).is_some() {
            found.push(format!("window_manager.{k}"));
        }
    }
    if let Some(mats) = val.pointer("/style/surface/material").and_then(|v| v.as_object()) {
        for (name, node) in mats {
            if node.pointer("/frost/backdrop_compression").is_some() {
                found.push(format!("style.surface.material.{name}.frost.backdrop_compression"));
            }
            if node.pointer("/finish/depth").is_some() {
                found.push(format!("style.surface.material.{name}.finish.depth"));
            }
        }
    }
    for k in ["cell_color", "gap_color", "uniform_background"] {
        if val.pointer(&format!("/style/surface/graph/{k}")).is_some() {
            found.push(format!("style.surface.graph.{k}"));
        }
    }
    found
}

pub(super) fn load_colors_once() {
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
