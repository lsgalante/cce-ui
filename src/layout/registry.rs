//! The style registry: the config flattened into one process-wide map of style keys, the
//! per-thread test overlay over it, and the font-string helpers its readers share. Every
//! getter in `layout` and `color` reads through it.

use super::*;
use std::collections::HashMap;

/// Per-thread overrides for runtime style writes, under `cfg(test)` only.
///
/// Every `graph_*`, corner-radius and control-height slot in this file is
/// backed by the one process-wide [`STYLE_REGISTRY`], so a test that pins any
/// of them pins it for every test running beside it. That is the same defect
/// as the font flake fixed in 1dc0ab1, and it was live:
/// `test_graph_style_configuration` sets ~20 style values and restores none,
/// while `dual_geometry_views_stay_consistent` bakes quads with
/// `graph_node_corner_radius` and then re-reads that getter to compare — so a
/// write landing between the two makes them disagree. Widening the write
/// window to 300ms reproduced it on demand.
///
/// Fixing it at the registry rather than per accessor covers every slot it
/// holds in one place, including ones no test pins yet.
/// The family fontconfig matches for `monospace` (`fc-match`), asked once.
pub fn get_system_monospace_font() -> &'static str {
    static MONOSPACE_FONT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    MONOSPACE_FONT.get_or_init(|| {
        if let Ok(output) = std::process::Command::new("fc-match")
            .args(["-f", "%{family}", "monospace"])
            .output()
        {
            let name = String::from_utf8_lossy(&output.stdout);
            let parsed = name.split(',').next().unwrap_or("monospace").trim();
            if !parsed.is_empty() {
                return parsed.to_string();
            }
        }
        "monospace".to_string()
    })
}

/// The family fontconfig matches for `sans-serif`, asked once.
pub fn get_system_sans_serif_font() -> &'static str {
    static SANS_SERIF_FONT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SANS_SERIF_FONT.get_or_init(|| {
        if let Ok(output) = std::process::Command::new("fc-match")
            .args(["-f", "%{family}", "sans-serif"])
            .output()
        {
            let name = String::from_utf8_lossy(&output.stdout);
            let parsed = name.split(',').next().unwrap_or("sans-serif").trim();
            if !parsed.is_empty() {
                return parsed.to_string();
            }
        }
        "sans-serif".to_string()
    })
}

#[cfg(test)]
mod test_overlay {
    use std::cell::RefCell;
    use std::collections::HashMap;
    thread_local! {
        static FLOATS: RefCell<HashMap<String, f32>> = RefCell::new(HashMap::new());
        static LENS: RefCell<HashMap<String, crate::units::Len>> = RefCell::new(HashMap::new());
        static STRINGS: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
    }
    pub fn set_float(k: &str, v: f32) {
        LENS.with(|m| m.borrow_mut().remove(k));
        FLOATS.with(|m| m.borrow_mut().insert(k.to_string(), v));
    }
    pub fn set_len(k: &str, v: crate::units::Len) {
        FLOATS.with(|m| m.borrow_mut().remove(k));
        LENS.with(|m| m.borrow_mut().insert(k.to_string(), v));
    }
    pub fn set_string(k: &str, v: String) {
        STRINGS.with(|m| m.borrow_mut().insert(k.to_string(), v));
    }
    pub fn get_float(k: &str) -> Option<f32> {
        if let Some(l) = LENS.with(|m| m.borrow().get(k).copied()) {
            return Some(l.to_px());
        }
        FLOATS.with(|m| m.borrow().get(k).copied())
    }
    pub fn get_len(k: &str) -> Option<crate::units::Len> {
        if let Some(l) = LENS.with(|m| m.borrow().get(k).copied()) {
            return Some(l);
        }
        FLOATS.with(|m| m.borrow().get(k).copied()).map(crate::units::Len::px)
    }
    pub fn get_string(k: &str) -> Option<String> {
        STRINGS.with(|m| m.borrow().get(k).cloned())
    }
}

#[derive(Debug, Clone, Default)]
pub struct StyleRegistry {
    pub floats: HashMap<String, f32>,
    pub strings: HashMap<String, String>,
    /// Slots whose config value carried a unit (`width=(mm)2.0`). Read
    /// through `get_float` like any other number, resolved against the
    /// process metric (`crate::units::metric`) at EVERY read, so a metric
    /// that arrives after config load — outputs come in after the first
    /// style read — or changes with the display is honoured live.
    pub lens: HashMap<String, crate::units::Len>,
}

impl StyleRegistry {
    pub fn new() -> Self {
        Self {
            floats: HashMap::new(),
            strings: HashMap::new(),
            lens: HashMap::new(),
        }
    }

    pub fn get_float(&self, key: &str) -> Option<f32> {
        #[cfg(test)]
        if let Some(v) = test_overlay::get_float(key) {
            return Some(v);
        }
        if let Some(len) = self.lens.get(key) {
            return Some(len.to_px());
        }
        self.floats.get(key).copied()
    }

    /// The slot as a length with its unit: the configured `Len` when one was
    /// given, else the plain number as logical px. For editors that show the
    /// unit the user chose rather than the resolved pixel count.
    pub fn get_len(&self, key: &str) -> Option<crate::units::Len> {
        #[cfg(test)]
        if let Some(v) = test_overlay::get_len(key) {
            return Some(v);
        }
        if let Some(len) = self.lens.get(key) {
            return Some(*len);
        }
        self.floats.get(key).map(|v| crate::units::Len::px(*v))
    }

    pub fn get_string(&self, key: &str) -> Option<String> {
        #[cfg(test)]
        if let Some(v) = test_overlay::get_string(key) {
            return Some(v);
        }
        self.strings.get(key).cloned()
    }

    /// A plain number wins over any earlier unit value for the slot — a
    /// runtime `set_float` is the newest opinion.
    pub fn set_float(&mut self, key: &str, val: f32) {
        #[cfg(test)]
        {
            test_overlay::set_float(key, val);
        }
        #[cfg(not(test))]
        self.load_float(key, val);
    }

    pub fn set_len(&mut self, key: &str, len: crate::units::Len) {
        #[cfg(test)]
        {
            test_overlay::set_len(key, len);
        }
        #[cfg(not(test))]
        self.load_len(key, len);
    }

    pub fn set_string(&mut self, key: &str, val: String) {
        #[cfg(test)]
        {
            test_overlay::set_string(key, val);
        }
        #[cfg(not(test))]
        self.load_string(key, val);
    }

    /// The CONFIG-LOAD writes, as opposed to the runtime `set_*` ones above.
    /// Kept apart because under `cfg(test)` a runtime set goes to a per-thread
    /// overlay while the loaded config must stay the shared base every test
    /// reads — routing config through `set_*` would put one thread's config
    /// in its overlay and leave every other thread with an empty registry.
    pub fn load_float(&mut self, key: &str, val: f32) {
        self.lens.remove(key);
        self.floats.insert(key.to_string(), val);
    }

    pub fn load_len(&mut self, key: &str, len: crate::units::Len) {
        self.floats.remove(key);
        self.lens.insert(key.to_string(), len);
    }

    pub fn load_string(&mut self, key: &str, val: String) {
        self.strings.insert(key.to_string(), val);
    }
}

/// The registry is a field of the one style snapshot (`crate::style`); this is its handle,
/// with the `RwLock` API it had.
pub static STYLE_REGISTRY: crate::style::StyleCell<StyleRegistry> =
    crate::style::StyleCell::new(|s| &s.registry, |s| &mut s.registry);

pub fn get_style_registry() -> &'static crate::style::StyleCell<StyleRegistry> {
    &STYLE_REGISTRY
}

pub fn lazy_init_style_registry() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        reload_config();
    });
}

pub(super) fn flatten_json_to_flat_props(val: &serde_json::Value, prefix: &str, flat_props: &mut String) {
    match val {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                let next_prefix = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{}.{}", prefix, k)
                };
                flatten_json_to_flat_props(v, &next_prefix, flat_props);
            }
        }
        _ => {
            // A retired `window_manager` spelling of a relief key emits NO
            // line: the fall-through below strips a path's first segment, so
            // `window_manager.bevel_depth` would otherwise land on the live
            // `bevel_depth` registry key by accident, alias or not. The
            // `style.surface.*` retirements need no guard — stripped, they
            // become `surface.relief.depth` and the like, which nothing reads.
            if matches!(prefix, "window_manager.bevel_depth" | "window_manager.bevel_width" | "window_manager.bevel_shader") {
                return;
            }
            let flat_key = match prefix {
                "style.list.font" | "style.data.list.font" => "list_font",
                "style.list.font_color" | "style.data.list.font_color" => "list_font_color",
                "style.control.breadcrumb.font" => "breadcrumb_font",

                // The control rung's default radius: every control-scale
                // `corner_radius` below falls back to it when its own key is unset.
                "style.control.corner_radius" => "control_corner_radius",
                "style.control.button.padding" => "button_padding",
                "style.control.button.height" => "button_height",
                "style.control.button.corner_radius" => "button_corner_radius",
                "style.control.button.font" => "button_font",
                "style.list.corner_radius" | "style.data.list.corner_radius" => "list_corner_radius",
                "style.control.textbox.corner_radius" | "style.textbox.corner_radius" | "style.data.textbox.corner_radius" => "textbox_corner_radius",
                "style.control.dropdown.color" => "dropdown_color",
                "style.control.font_selector.font" => "font_selector_font",
                "style.container.section.font" | "style.section.font" => "section_label_font",
                "style.container.section.padding" | "style.section.padding" => "section_padding",
                "style.control.button_strip.font" => "button_strip_font",
                "style.control.button_strip.spacing" => "button_strip_spacing",
                "style.control.dropdown.height" => "dropdown_height",
                "style.control.dropdown.corner_radius" => "dropdown_corner_radius",
                "style.control.font_selector.height" => "font_selector_height",
                "style.control.font_selector.corner_radius" => "font_selector_corner_radius",
                "style.control.label.font" => "control_label_font",
                "style.control.label.font_detached" => "control_label_font_detached",
                "style.control.label.margin" => "control_label_margin",
                "style.control.slider.height" => "slider_height",
                "style.control.slider.corner_radius" => "slider_corner_radius",
                "style.control.slider.band_thickness" => "slider_band_thickness",
                "style.control.slider.bulge_width" => "slider_bulge_width",
                "style.control.slider.bulge_height" => "slider_bulge_height",
                "style.control.progressbar.height" => "progressbar_height",
                "style.control.rangeslider.height" => "rangeslider_height",
                "style.control.scrollbar.width" => "scrollbar_width",
                "style.control.scrollbar.inset" => "scrollbar_inset",
                "style.control.spinbox.height" => "spinbox_height",
                "style.control.spinbox.button_padding" => "spinbox_button_padding",
                "style.control.spinbox.corner_radius" => "spinbox_corner_radius",
                "style.control.textbox.height" | "style.textbox.height" | "style.data.textbox.height" => "textbox_height",
                "style.control.textbox.placeholder_text_color" | "style.textbox.placeholder_text_color" | "style.data.textbox.placeholder_text_color" => "textbox_placeholder_text_color",
                "style.control.textbox.background_color" | "style.textbox.background_color" | "style.data.textbox.background_color" => "textbox_background_color",
                "style.control.textbox.background_edit_color" | "style.textbox.background_edit_color" | "style.data.textbox.background_edit_color" => "textbox_background_edit_color",
                "style.control.textbox.multiline.line_wrap" | "style.textbox.multiline.line_wrap" | "style.data.textbox.multiline.line_wrap" => "textbox_line_wrap",
                "style.control.textbox.multiline.border_width" | "style.textbox.multiline.border_width" | "style.data.textbox.multiline.border_width" => "textbox_multiline_border_width",
                "style.control.toggle.height" => "toggle_height",
                "style.control.toggle.border_width" => "toggle_border_width",
                "style.control.toggle.disabled_color" => "toggle_disabled_color",
                "style.control.toggle.border_color" => "toggle_border_color",
                "style.control.toggle.corner_radius" => "toggle_corner_radius",
                "window_manager.light_source_position" => "light_source_position",
                // The DE's relief material, home style.surface.relief: these
                // shade every bevel/boss/recess in the toolkit. `light` is
                // the spelling (since 2026-09-28): it is the light strength,
                // not a length. Two older spellings are RETIRED — not read,
                // reported by path (`color::retired_surface_keys`), removed
                // by cce-relief's Save: `relief.depth`, what every config
                // said until that day, and `window_manager.bevel_depth` /
                // `bevel_width`, the block these keys were born in before
                // they had a home of their own (the compositor never read
                // them; the spelling survived as a compat alias until the
                // evening of 2026-09-28).
                "style.surface.relief.light" => "bevel_depth",
                "style.surface.relief.width" => "bevel_width",
                // The two SHAPES of the relief, each a node under it
                // (2026-09-28): `wall` is a carve's wall — a recess, boss,
                // ridge or trough cut into a surface — and `edge` is the
                // plate's perimeter roll. Each carries `height` (a length:
                // the carve's drop, the roll's rise; unset = follow the
                // width) and `profile` (the ramp spec of its curve, written
                // by cce-relief, installed by reload_config). The registry
                // keys keep their old names. The pre-rename flat spellings
                // (`height` / `profile` were the wall's, `edge_height` /
                // `edge_profile` the edge's) were aliases for the rest of
                // that day and are RETIRED: not read, reported by path
                // (`color::retired_surface_keys`), removed by cce-relief's
                // Save, which also reads them once as a seed. cce-relief's
                // slider knobs are NOT a style key any more
                // (`relief.wall.knobs` / `edge.knobs`, before that
                // `profile_knobs` / `edge_knobs`): editor state, kept in
                // that app's own state.kdl, and read off a config only by
                // the editor itself, as a one-time seed.
                "style.surface.relief.wall.height" => "bevel_height",
                "style.surface.relief.edge.height" => "roll_height",
                "style.surface.relief.wall.profile" => "bevel_profile_spec",
                "style.surface.relief.edge.profile" => "roll_profile_spec",
                "style.container.section.depth" => "section_depth",
                "style.surface.param.backdrop_compression" => "param_compression",
                "style.surface.param.label_layout" => "param_label_layout",
                // The relief's shader toggle lives with the relief
                // (`relief shader=(bool)false` is the legacy banded look);
                // `window_manager.bevel_shader`, its old home, is retired
                // and reported like the block's other bevel keys.
                "style.surface.relief.shader" => "bevel_shader",
                "window_manager.control_relief" => "control_relief",
                "window_manager.corner_shape" => "corner_shape",
                "style.control.ramp.height" => "ramp_height",
                "style.layout.column.gap" => "column_gap",
                "style.control.control_panel.padding" => "control_panel_padding",
                "style.control.control_panel.gap" => "control_panel_gap",
                "style.status.normal_color" => "status_normal_color",
                "style.status.background_color" => "status_background_color",
                "style.status.background_blur" => "status_background_blur",
                "style.highlight.primary" => "primary_highlight_color",
                "style.window.page_opacity" => "page_opacity",
                "style.window.page_margin" => "page_margin",
                "style.window.plate_padding" => "plate_padding",
                "style.window.transition_duration" => "transition_duration",
                "style.overlay.behavior" => "overlay_behavior",
                "style.overlay.width" => "overlay_width",
                "style.overlay.position" => "overlay_position",
                "style.overlay.border_gap" => "overlay_border_gap",
                "style.editor.last_page" | "style.data.editor.last_page" => "last_page",
                "style.data.tree.corner_radius" => "tree_corner_radius",
                "style.data.tree.opacity" => "tree_opacity",
                "style.data.tree.blur" => "tree_blur",
                "style.data.tree.font" => "tree_font",
                "style.surface.desktop.gap_color" => "desktop_gap_color",
                "style.surface.desktop.cell_color" => "desktop_cell_color",
                "style.surface.desktop.gap_width" => "desktop_gap_width",
                "style.surface.desktop.cell_fade_inset" => "desktop_cell_fade_inset",
                "style.surface.desktop.mode" => "desktop_mode",
                "style.surface.desktop.solid_color" => "desktop_solid_color",
                "style.surface.desktop.grid_cell_size" => "desktop_grid_scale",
                "style.surface.desktop.grid_cell_width" => "grid_cell_width",
                "style.surface.desktop.grid_cell_height" => "grid_cell_height",
                "style.surface.plate.padding" => "plate_padding",
                "style.surface.plate.gap" => "plate_gap",
                "style.control.gap" => "control_gap",
                "style.control.list_gap" => "list_gap",
                // The context menu's own radius (`menu_corner_radius`): a
                // popover's corner is control-scale, not pane-scale.
                "style.surface.menu.corner_radius" => "menu_corner_radius",
                // `style.surface.plate.root.*` is the one spelling of the
                // root-plate style (RFC Phase 7a). The slot names keep the
                // historical `root_plate_` prefix; the legacy `root plate.*`
                // config read-alias was removed 2026-09-06.
                "style.surface.plate.root.padding" => "root_plate_padding",
                "style.surface.plate.root.gap" => "root_plate_gap",
                "style.surface.plate.root.color" => "root_plate_color",
                "style.surface.plate.root.blur" => "root_plate_blur",
                "style.surface.plate.root.corner_radius" => "root_plate_corner_radius",
                "style.surface.plate.root.menubar.color" => "root_plate_menubar_color",
                "style.surface.plate.root.menubar.text_color" => "root_plate_menubar_text_color",
                "style.surface.plate.root.menubar.blur" => "root_plate_menubar_blur",
                "style.surface.plate.root.menubar.font" => "menubar_font",
                "style.surface.statusbar.color" => "root_plate_statusbar_color",
                "style.surface.statusbar.text_color" => "root_plate_statusbar_text_color",
                "style.surface.statusbar.blur" => "root_plate_statusbar_blur",
                "style.surface.statusbar.font" => "statusbar_font",
                "style.surface.page.opacity" => "page_opacity",
                "style.surface.page.margin" => "page_margin",
                "style.surface.graph.grid_color" => "graph_grid_color",
                "style.surface.graph.opacity" => "graph_opacity",
                "style.surface.graph.node.opacity" => "graph_node_opacity",
                "style.surface.graph.spacing_x" => "graph_spacing_x",
                "style.surface.graph.spacing_y" => "graph_spacing_y",
                "style.surface.graph.line_width" => "graph_line_width",
                "style.surface.graph.node.width" => "graph_node_width",
                "style.surface.graph.node.height" => "graph_node_height",
                "style.surface.graph.grid_snap" => "graph_grid_snap",
                "style.surface.graph.blur" => "graph_blur",
                "style.surface.graph.font" => "graph_font",
                "style.surface.graph.node.color" => "graph_node_color",
                "style.surface.graph.node.font" => "graph_node_font",
                "style.surface.graph.node.delete" => "graph_node_delete",
                "style.surface.graph.node.selected_color" => "graph_node_selected_color",
                "style.surface.graph.node.drag_color" => "graph_node_drag_color",
                "style.surface.graph.node.corner_radius" => "graph_node_corner_radius",
                "style.surface.graph.node.wire_color" => "graph_wire_color",
                "style.surface.graph.node.wire_highlight_color" => "graph_wire_highlight_color",
                "style.surface.graph.node.wire_size" => "graph_wire_size",
                "style.surface.graph.node.wire_style" => "graph_wire_style",
                "style.surface.graph.node.wire_activation_radius" => "graph_wire_activation_radius",
                "style.surface.graph.node.connector_color" => "graph_connector_color",
                "style.surface.graph.node.connector_highlight_color" => "graph_connector_highlight_color",
                "style.surface.graph.node.connector_size" => "graph_connector_size",
                "style.surface.graph.node.connector_activation_radius" => "graph_connector_activation_radius",
                "style.surface.plate.color" => "plate_color",
                "style.surface.plate.border_color" => "plate_border_color",
                "style.surface.plate.border_thickness" => "plate_border_thickness",
                "input.touchpad.natural_scroll" => "touchpad_natural_scroll",
                
                // Anything else keeps its whole path. A key the toolkit reads is mapped above
                // (or is a flat top-level key, which has no path to strip); the rest belong
                // to other programs. Until 2026-10-08 an unmapped path was cut down — the
                // `layout.` and `transparency.` blocks (the compositor's own) to their bare
                // keys, anything else past its first segment — so the compositor's
                // `layout { grid_gap 18 }`, its window-tiling gap, arrived as the toolkit's
                // `grid_gap`, and any block's key could stand in for a toolkit one.
                other => other,
            };
            
            if let Some(s) = val.as_str() {
                flat_props.push_str(&format!("{} = \"{}\"\n", flat_key, s));
            } else if let Some(b) = val.as_bool() {
                flat_props.push_str(&format!("{} = {}\n", flat_key, b));
            } else if let Some(n) = val.as_f64() {
                flat_props.push_str(&format!("{} = {}\n", flat_key, n));
            } else if let Some(n) = val.as_i64() {
                flat_props.push_str(&format!("{} = {}\n", flat_key, n));
            }
        }
    }
}

// There is no alias-precedence pass any more. For the rest of 2026-09-28
// `prefer_relief_spellings` ran here, dropping a relief key's legacy
// spelling whenever its current one was present (`height` under
// `wall.height`, `depth` under `light`), because two spellings of one
// registry key would otherwise be decided by the order the JSON handed
// the lines out. Every legacy spelling of the relief is retired now — not
// read at all, reported by `color::retired_surface_keys` — so there is
// nothing left to prefer, and a config is what it says.

pub(super) fn read_config() -> Option<String> {
    let path = crate::config::get_config_path();
    if let Ok(content) = std::fs::read_to_string(&path) {
        let val = crate::config::parse_kdl_to_json(&content);
        let mut flat_props = String::new();
        flatten_json_to_flat_props(&val, "", &mut flat_props);
        return Some(flat_props);
    }
    None
}

pub fn parse_font_string(s: &str) -> (String, Option<f32>) {
    let (family, size) = split_font_string(s);
    (family.to_string(), size)
}

/// [`parse_font_string`] borrowing the family from `s` instead of copying it.
pub fn split_font_string(s: &str) -> (&str, Option<f32>) {
    let s = s.trim();
    if let Some(last_space_idx) = s.rfind(' ') {
        let (family, size_str) = s.split_at(last_space_idx);
        let size_str = size_str.trim();
        if let Ok(size) = size_str.parse::<f32>() {
            return (family.trim(), Some(size));
        }
    }
    (s, None)
}
