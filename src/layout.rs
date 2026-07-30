use crate::widget::WidgetHost;
use crate::context::UiContext;
use std::sync::RwLock;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug)]
pub struct StyleRegistry {
    pub floats: HashMap<String, f32>,
    pub strings: HashMap<String, String>,
}

impl StyleRegistry {
    pub fn new() -> Self {
        Self {
            floats: HashMap::new(),
            strings: HashMap::new(),
        }
    }

    pub fn get_float(&self, key: &str) -> Option<f32> {
        self.floats.get(key).copied()
    }

    pub fn get_string(&self, key: &str) -> Option<String> {
        self.strings.get(key).cloned()
    }

    pub fn set_float(&mut self, key: &str, val: f32) {
        self.floats.insert(key.to_string(), val);
    }

    pub fn set_string(&mut self, key: &str, val: String) {
        self.strings.insert(key.to_string(), val);
    }
}

pub static STYLE_REGISTRY: OnceLock<RwLock<StyleRegistry>> = OnceLock::new();

pub fn get_style_registry() -> &'static RwLock<StyleRegistry> {
    STYLE_REGISTRY.get_or_init(|| RwLock::new(StyleRegistry::new()))
}

pub fn lazy_init_style_registry() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        reload_config();
    });
}

fn flatten_json_to_flat_props(val: &serde_json::Value, prefix: &str, flat_props: &mut String) {
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
            let flat_key = match prefix {
                "style.list.font" | "style.data.list.font" => "list_font",
                "style.list.font_color" | "style.data.list.font_color" => "list_font_color",
                "style.control.breadcrumb.font" => "breadcrumb_font",
                "style.control.breadcrumb.corner_radius" => "breadcrumb_corner_radius",
                "style.control.button.padding" => "button_padding",
                "style.control.button.height" => "button_height",
                "style.control.button.corner_radius" => "button_corner_radius",
                "style.control.button.font" => "button_font",
                "style.list.corner_radius" | "style.data.list.corner_radius" => "list_corner_radius",
                "style.control.textbox.corner_radius" | "style.textbox.corner_radius" | "style.data.textbox.corner_radius" => "textbox_corner_radius",
                "style.control.dropdown.color" => "dropdown_color",
                "style.control.font_selector.font" => "font_selector_font",
                "style.section.font" => "section_label_font",
                "style.control.button_strip.font" => "button_strip_font",
                "style.control.button_strip.spacing" => "button_strip_spacing",
                "style.control.dropdown.height" => "dropdown_height",
                "style.control.dropdown.corner_radius" => "dropdown_corner_radius",
                "style.control.font_selector.height" => "font_selector_height",
                "style.control.font_selector.corner_radius" => "font_selector_corner_radius",
                "style.control.label.font" => "control_label_font",
                "style.control.label.font_detached" => "control_label_font_detached",
                "style.control.label.margin" => "control_label_margin",
                "style.control.label.layout" => "control_label_layout",
                "style.control.slider.height" => "slider_height",
                "style.control.slider.corner_radius" => "slider_corner_radius",
                "style.control.slider.style" => "slider_style",
                "style.control.slider.band_thickness" => "slider_band_thickness",
                "style.control.slider.bulge_width" => "slider_bulge_width",
                "style.control.slider.bulge_height" => "slider_bulge_height",
                "style.control.progressbar.height" => "progressbar_height",
                "style.control.rangeslider.height" => "rangeslider_height",
                "style.control.rangeslider.corner_radius" | "style.rangeslider.corner_radius" => "rangeslider_corner_radius",
                "style.control.scrollbar.width" => "scrollbar_width",
                "style.control.spinbox.height" => "spinbox_height",
                "style.control.spinbox.button_padding" => "spinbox_button_padding",
                "style.control.spinbox.corner_radius" => "spinbox_corner_radius",
                "style.control.textbox.height" | "style.textbox.height" | "style.data.textbox.height" => "textbox_height",
                "style.control.textbox.placeholder_text_color" | "style.textbox.placeholder_text_color" | "style.data.textbox.placeholder_text_color" => "textbox_placeholder_text_color",
                "style.control.textbox.background_color" | "style.textbox.background_color" | "style.data.textbox.background_color" => "textbox_background_color",
                "style.control.textbox.background_edit_color" | "style.textbox.background_edit_color" | "style.data.textbox.background_edit_color" => "textbox_background_edit_color",
                "style.control.textbox.multiline.line_wrap" | "style.textbox.multiline.line_wrap" | "style.data.textbox.multiline.line_wrap" => "textbox_line_wrap",
                "style.control.textbox.multiline.border_width" | "style.textbox.multiline.border_width" | "style.data.textbox.multiline.border_width" => "textbox_multiline_border_width",
                "style.control.toggle.style" => "toggle_style",
                "style.control.toggle.height" => "toggle_height",
                "style.control.toggle.border_width" => "toggle_border_width",
                "style.control.toggle.disabled_color" => "toggle_disabled_color",
                "style.control.toggle.border_color" => "toggle_border_color",
                "style.control.toggle.corner_radius" => "toggle_corner_radius",
                "window_manager.light_source_position" => "light_source_position",
                "window_manager.bevel_depth" => "bevel_depth",
                "window_manager.bevel_width" => "bevel_width",
                "window_manager.bevel_shader" => "bevel_shader",
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
                "style.surface.desktop.cell_corner_radius" => "desktop_cell_corner_radius",
                "style.surface.desktop.cell_fade_inset" => "desktop_cell_fade_inset",
                "style.surface.desktop.mode" => "desktop_mode",
                "style.surface.desktop.solid_color" => "desktop_solid_color",
                "style.surface.desktop.grid_cell_size" => "desktop_grid_scale",
                "style.surface.plate.padding" => "plate_padding",
                "style.surface.backplate.padding" => "backplate_padding",
                "style.surface.backplate.gap" => "backplate_gap",
                "style.surface.backplate.color" => "backplate_color",
                "style.surface.backplate.blur" => "backplate_blur",
                "style.surface.backplate.corner_radius" => "backplate_corner_radius",
                "style.surface.backplate.menubar.color" => "backplate_menubar_color",
                "style.surface.backplate.menubar.text_color" => "backplate_menubar_text_color",
                "style.surface.backplate.menubar.blur" => "backplate_menubar_blur",
                "style.surface.backplate.menubar.font" => "menubar_font",
                "style.surface.statusbar.color" => "backplate_statusbar_color",
                "style.surface.statusbar.text_color" => "backplate_statusbar_text_color",
                "style.surface.statusbar.blur" => "backplate_statusbar_blur",
                "style.surface.statusbar.font" => "statusbar_font",
                "style.surface.page.opacity" => "page_opacity",
                "style.surface.page.margin" => "page_margin",
                "style.surface.graph.cell_color" => "graph_cell_color",
                "style.surface.graph.gap_color" => "graph_gap_color",
                "style.surface.graph.opacity" => "graph_opacity",
                "style.surface.graph.node.opacity" => "graph_node_opacity",
                "style.surface.graph.spacing_x" => "graph_spacing_x",
                "style.surface.graph.spacing_y" => "graph_spacing_y",
                "style.surface.graph.gap_col_w" => "graph_gap_col_w",
                "style.surface.graph.gap_row_h" => "graph_gap_row_h",
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
                "style.surface.graph.node.wire_activation_radius" => "graph_wire_activation_radius",
                "style.surface.graph.node.connector_color" => "graph_connector_color",
                "style.surface.graph.node.connector_highlight_color" => "graph_connector_highlight_color",
                "style.surface.graph.node.connector_size" => "graph_connector_size",
                "style.surface.graph.node.connector_activation_radius" => "graph_connector_activation_radius",
                "style.surface.plate.color" => "plate_color",
                "style.surface.plate.border_color" => "plate_border_color",
                "style.surface.plate.border_thickness" => "plate_border_thickness",
                "style.surface.plate.blur" => "plate_blur",
                "input.touchpad.natural_scroll" => "touchpad_natural_scroll",
                
                other => {
                    if let Some(rest) = other.strip_prefix("layout.") {
                        rest
                    } else if let Some(rest) = other.strip_prefix("transparency.") {
                        rest
                    } else {
                        if let Some(idx) = other.find('.') {
                            &other[idx + 1..]
                        } else {
                            other
                        }
                    }
                }
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

fn read_config() -> Option<String> {
    let path = crate::config::get_config_path();
    if let Ok(content) = std::fs::read_to_string(&path) {
        let val = crate::config::parse_kdl_to_json(&content);
        let mut flat_props = String::new();
        flatten_json_to_flat_props(&val, "", &mut flat_props);
        return Some(flat_props);
    }
    None
}

pub fn read_config_value(target_key: &str) -> Option<String> {
    let path = crate::config::get_config_path();
    if let Ok(content) = std::fs::read_to_string(&path) {
        let val = crate::config::parse_kdl_to_json(&content);
        let mut flat_props = String::new();
        flatten_json_to_flat_props(&val, "", &mut flat_props);
        for line in flat_props.lines() {
            let trimmed = line.trim();
            if let Some(eq_idx) = trimmed.find('=') {
                let key = trimmed[..eq_idx].trim();
                if key == target_key {
                    let val_str = trimmed[eq_idx + 1..].trim().trim_matches('"').trim();
                    return Some(val_str.to_string());
                }
            }
        }
    }
    None
}

pub fn parse_font_string(s: &str) -> (String, Option<f32>) {
    let s = s.trim();
    if let Some(last_space_idx) = s.rfind(' ') {
        let (family, size_str) = s.split_at(last_space_idx);
        let size_str = size_str.trim();
        if let Ok(size) = size_str.parse::<f32>() {
            return (family.trim().to_string(), Some(size));
        }
    }
    (s.to_string(), None)
}

static SECTION_PADDING: RwLock<f32> = RwLock::new(8.0);
static SPINBOX_HEIGHT: RwLock<f32> = RwLock::new(26.0);
static SPINBOX_BUTTON_PADDING: RwLock<f32> = RwLock::new(0.0);
static COLOR_SELECTOR_HEIGHT: RwLock<f32> = RwLock::new(22.0);
static TEXTBOX_HEIGHT: RwLock<f32> = RwLock::new(44.0);
static FONT_SELECTOR_HEIGHT: RwLock<f32> = RwLock::new(44.0);
static SLIDER_HEIGHT: RwLock<f32> = RwLock::new(28.0);
static PROGRESSBAR_HEIGHT: RwLock<f32> = RwLock::new(24.0);
static RANGESLIDER_HEIGHT: RwLock<f32> = RwLock::new(28.0);
static TOGGLE_HEIGHT: RwLock<f32> = RwLock::new(44.0);
static COLOR_SELECTOR_FONT: RwLock<String> = RwLock::new(String::new());
static COLOR_SELECTOR_PREVIEW_CORNER_RADIUS: RwLock<f32> = RwLock::new(4.0);
static COLOR_SELECTOR_PREVIEW_MARGIN: RwLock<f32> = RwLock::new(0.0);
static COLOR_SELECTOR_CORNER_RADIUS: RwLock<f32> = RwLock::new(4.0);
static MENUBAR_FONT: RwLock<String> = RwLock::new(String::new());
static MENUBAR_FONT_CACHED: RwLock<Option<(String, f32)>> = RwLock::new(None);
static STATUSBAR_FONT: RwLock<String> = RwLock::new(String::new());
static STATUSBAR_FONT_CACHED: RwLock<Option<(String, f32)>> = RwLock::new(None);
static SECTION_LABEL_FONT: RwLock<String> = RwLock::new(String::new());
static NESTED_SECTION_LABEL_FONT: RwLock<String> = RwLock::new(String::new());
static BREADCRUMB_FONT: RwLock<String> = RwLock::new(String::new());
static BUTTON_FONT: RwLock<String> = RwLock::new(String::new());

static PAGINATOR_TAB_PADDING_X: RwLock<f32> = RwLock::new(10.0);
static BUTTON_PADDING: RwLock<f32> = RwLock::new(14.0);
static BUTTON_HEIGHT: RwLock<f32> = RwLock::new(40.0);
static RAMP_HEIGHT: RwLock<f32> = RwLock::new(32.0);
static BUTTON_STRIP_SPACING: RwLock<f32> = RwLock::new(8.0);
static SCROLLBAR_WIDTH: RwLock<f32> = RwLock::new(4.0);
static COLUMN_GAP: RwLock<f32> = RwLock::new(16.0);
static CONTROL_PANEL_PADDING: RwLock<f32> = RwLock::new(16.0);
static CONTROL_PANEL_GAP: RwLock<f32> = RwLock::new(12.0);
static TREE_OPACITY: RwLock<f32> = RwLock::new(1.0);
static TREE_BLUR: RwLock<f32> = RwLock::new(0.0);

static PLATE_PADDING: RwLock<f32> = RwLock::new(20.0);
static DROPDOWN_HEIGHT: RwLock<f32> = RwLock::new(44.0);
static NESTED_SECTION_LABEL_ALIGNMENT: RwLock<u8> = RwLock::new(0);
static TOUCHPAD_NATURAL_SCROLL: RwLock<bool> = RwLock::new(false);


static BUTTON_CORNER_RADIUS: RwLock<f32> = RwLock::new(4.0);
static SPINBOX_CORNER_RADIUS: RwLock<f32> = RwLock::new(4.0);
static TEXTBOX_CORNER_RADIUS: RwLock<f32> = RwLock::new(4.0);
static FONT_SELECTOR_CORNER_RADIUS: RwLock<f32> = RwLock::new(4.0);
static DROPDOWN_CORNER_RADIUS: RwLock<f32> = RwLock::new(4.0);
static TOGGLE_CORNER_RADIUS: RwLock<f32> = RwLock::new(4.0);
static SLIDER_CORNER_RADIUS: RwLock<f32> = RwLock::new(4.0);
static RANGESLIDER_CORNER_RADIUS: RwLock<f32> = RwLock::new(4.0);
static BREADCRUMB_CORNER_RADIUS: RwLock<f32> = RwLock::new(4.0);
static LIST_CORNER_RADIUS: RwLock<f32> = RwLock::new(4.0);
static TREE_CORNER_RADIUS: RwLock<f32> = RwLock::new(4.0);
static TOGGLE_BORDER_WIDTH: RwLock<f32> = RwLock::new(1.0);
static FONT_SELECTOR_FONT: RwLock<String> = RwLock::new(String::new());
static FONT_SELECTOR_FONT_CACHED: RwLock<Option<(String, f32)>> = RwLock::new(None);
static BUTTON_STRIP_FONT: RwLock<String> = RwLock::new(String::new());
static BUTTON_STRIP_FONT_CACHED: RwLock<Option<(String, f32)>> = RwLock::new(None);
static CONTROL_LABEL_FONT: RwLock<String> = RwLock::new(String::new());
static CONTROL_LABEL_FONT_CACHED: RwLock<Option<(String, f32)>> = RwLock::new(None);
static CONTROL_LABEL_FONT_DETACHED: RwLock<String> = RwLock::new(String::new());
static CONTROL_LABEL_FONT_DETACHED_CACHED: RwLock<Option<(String, f32)>> = RwLock::new(None);
static CONTROL_LABEL_MARGIN: RwLock<f32> = RwLock::new(6.0);
static CONTROL_LABEL_LAYOUT: RwLock<String> = RwLock::new(String::new());
static PLATE_CORNER_RADIUS: RwLock<f32> = RwLock::new(12.0);
static LIST_FONT: RwLock<String> = RwLock::new(String::new());
static LIST_FONT_CACHED: RwLock<Option<(String, f32)>> = RwLock::new(None);
static TREE_FONT: RwLock<String> = RwLock::new(String::new());
static TREE_FONT_CACHED: RwLock<Option<(String, f32)>> = RwLock::new(None);
static GRAPH_FONT: RwLock<String> = RwLock::new(String::new());
static GRAPH_FONT_CACHED: RwLock<Option<(String, f32)>> = RwLock::new(None);
static GRAPH_NODE_FONT: RwLock<String> = RwLock::new(String::new());
static GRAPH_NODE_FONT_CACHED: RwLock<Option<(String, f32)>> = RwLock::new(None);
static LIST_JUSTIFICATION: RwLock<u8> = RwLock::new(0);
static PLATE_OPACITY: RwLock<f32> = RwLock::new(1.0);
static PAGE_OPACITY: RwLock<f32> = RwLock::new(1.0);
static LAYER_OPACITY: RwLock<f32> = RwLock::new(1.0);
static TEXTBOX_LINE_WRAP: RwLock<bool> = RwLock::new(true);
static TEXTBOX_MULTILINE_BORDER_WIDTH: RwLock<f32> = RwLock::new(1.0);




/// Standard line height multiplier for text layout in cce-ui.
pub const TEXT_LINE_HEIGHT_MULTIPLIER: f32 = 1.0;

/// Standard line height based on font size.
pub fn line_height(font_size: f32) -> f32 {
    font_size * TEXT_LINE_HEIGHT_MULTIPLIER
}

/// Aligns a text label's top coordinate (`y`) so it is centered vertically
/// inside a container of height `container_h` starting at `y`.
pub fn center_text_y(y: f32, container_h: f32, font_size: f32) -> f32 {
    y + (container_h - line_height(font_size)) / 2.0
}

/// Standardized vertical text alignment calculation based on Spinbox widget alignment.
pub fn align_text_y(y: f32, height: f32, font_size: f32, top_offset: f32) -> f32 {
    y + top_offset + (height - top_offset - font_size) / 2.0
}

pub fn reload_config() {
    if let Some(content) = read_config() {
        let mut menubar_font_changed = false;
        let mut statusbar_font_changed = false;
        let mut font_selector_font_changed = false;
        let mut button_strip_font_changed = false;
        let mut label_font_changed = false;
        let mut label_font_detached_changed = false;
        let mut list_font_changed = false;
        let mut tree_font_changed = false;
        let mut graph_font_changed = false;
        let mut graph_node_font_changed = false;
        for line in content.lines() {
            let trimmed = line.trim();
            let mut key = String::new();
            let mut val_str = "";
            if let Some(eq_idx) = trimmed.find('=') {
                key = trimmed[..eq_idx].trim().to_string();
                val_str = trimmed[eq_idx + 1..].trim().trim_matches('"').trim();
                if let Ok(mut registry) = get_style_registry().write() {
                    if let Ok(f_val) = val_str.parse::<f32>() {
                        registry.set_float(&key, f_val);
                    } else {
                        registry.set_string(&key, val_str.to_string());
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("control_label_margin") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = CONTROL_LABEL_MARGIN.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("control_label_layout") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim().to_string();
                if let Ok(mut lock) = CONTROL_LABEL_LAYOUT.write() {
                    *lock = val_str;
                }
            }
            if let Some(rest) = trimmed.strip_prefix("nested_section_label_alignment") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<u8>() {
                    if let Ok(mut lock) = NESTED_SECTION_LABEL_ALIGNMENT.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("nested_section_label_offset") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = NESTED_SECTION_LABEL_OFFSET.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("plate_padding") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = PLATE_PADDING.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("page_margin") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = PAGE_MARGIN.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("grid_min_col_width") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = GRID_MIN_COL_WIDTH.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("grid_gap") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = GRID_GAP.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("column_gap") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = COLUMN_GAP.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("control_panel_padding") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = CONTROL_PANEL_PADDING.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("control_panel_gap") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = CONTROL_PANEL_GAP.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("section_padding") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = SECTION_PADDING.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("scrollbar_width") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = SCROLLBAR_WIDTH.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("tree_opacity") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = TREE_OPACITY.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("tree_blur") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = TREE_BLUR.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("spinbox_height") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = SPINBOX_HEIGHT.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("spinbox_button_padding") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = SPINBOX_BUTTON_PADDING.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("spinbox_corner_radius") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = SPINBOX_CORNER_RADIUS.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("textbox_corner_radius") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = TEXTBOX_CORNER_RADIUS.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("textbox_line_wrap") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                let wrap = val_str == "true" || val_str == "1" || val_str == "1.0";
                if let Ok(mut lock) = TEXTBOX_LINE_WRAP.write() {
                    *lock = wrap;
                }
            }
            if let Some(rest) = trimmed.strip_prefix("touchpad_natural_scroll") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                let enabled = val_str == "true" || val_str == "1" || val_str == "1.0";
                if let Ok(mut lock) = TOUCHPAD_NATURAL_SCROLL.write() {
                    *lock = enabled;
                }
            }
            if let Some(rest) = trimmed.strip_prefix("textbox_multiline_border_width") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = TEXTBOX_MULTILINE_BORDER_WIDTH.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("breadcrumb_corner_radius") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = BREADCRUMB_CORNER_RADIUS.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("list_corner_radius") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = LIST_CORNER_RADIUS.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("tree_corner_radius") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = TREE_CORNER_RADIUS.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("font_selector_corner_radius") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = FONT_SELECTOR_CORNER_RADIUS.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("dropdown_corner_radius") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = DROPDOWN_CORNER_RADIUS.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("toggle_corner_radius") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = TOGGLE_CORNER_RADIUS.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("plate_corner_radius") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = PLATE_CORNER_RADIUS.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("plate_opacity") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = PLATE_OPACITY.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("page_opacity") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = PAGE_OPACITY.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("layer_opacity") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = LAYER_OPACITY.write() {
                        *lock = val;
                    }
                }
            }


            if let Some(rest) = trimmed.strip_prefix("toggle_height") {

                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = TOGGLE_HEIGHT.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("color_selector_height") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = COLOR_SELECTOR_HEIGHT.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("font_selector_height") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = FONT_SELECTOR_HEIGHT.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("color_selector_font") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=');
                let rest = rest.trim();
                let val_str = if rest.starts_with('"') && rest.ends_with('"') && rest.len() >= 2 {
                    &rest[1..rest.len() - 1]
                } else {
                    rest
                };
                let font = val_str.trim().to_string();
                if let Ok(mut lock) = COLOR_SELECTOR_FONT.write() {
                    *lock = font;
                }
            }
            if let Some(rest) = trimmed.strip_prefix("menubar_font") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=');
                let rest = rest.trim();
                let val_str = if rest.starts_with('"') && rest.ends_with('"') && rest.len() >= 2 {
                    &rest[1..rest.len() - 1]
                } else {
                    rest
                };
                let font = val_str.trim().to_string();
                let mut changed = false;
                if let Ok(mut lock) = MENUBAR_FONT.write() {
                    if *lock != font {
                        *lock = font;
                        changed = true;
                    }
                }
                if changed {
                    menubar_font_changed = true;
                }
            }
            if let Some(rest) = trimmed.strip_prefix("statusbar_font") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=');
                let rest = rest.trim();
                let val_str = if rest.starts_with('"') && rest.ends_with('"') && rest.len() >= 2 {
                    &rest[1..rest.len() - 1]
                } else {
                    rest
                };
                let font = val_str.trim().to_string();
                let mut changed = false;
                if let Ok(mut lock) = STATUSBAR_FONT.write() {
                    if *lock != font {
                        *lock = font;
                        changed = true;
                    }
                }
                if changed {
                    statusbar_font_changed = true;
                }
            }
            if let Some(rest) = trimmed.strip_prefix("section_label_font") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=');
                let rest = mod_rest(rest);
                let font = rest.trim().to_string();
                if let Ok(mut lock) = SECTION_LABEL_FONT.write() {
                    *lock = font;
                }
            }
            if let Some(rest) = trimmed.strip_prefix("nested_section_label_font") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=');
                let rest = mod_rest(rest);
                let font = rest.trim().to_string();
                if let Ok(mut lock) = NESTED_SECTION_LABEL_FONT.write() {
                    *lock = font;
                }
            }
            if let Some(rest) = trimmed.strip_prefix("breadcrumb_font") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=');
                let rest = mod_rest(rest);
                let font = rest.trim().to_string();
                if let Ok(mut lock) = BREADCRUMB_FONT.write() {
                    *lock = font;
                }
            }
            if let Some(rest) = trimmed.strip_prefix("button_font") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=');
                let rest = mod_rest(rest);
                let font = rest.trim().to_string();
                if let Ok(mut lock) = BUTTON_FONT.write() {
                    *lock = font;
                }
            }
            if let Some(rest) = trimmed.strip_prefix("color_selector_preview_corner_radius") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = COLOR_SELECTOR_PREVIEW_CORNER_RADIUS.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("color_selector_corner_radius") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = COLOR_SELECTOR_CORNER_RADIUS.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("color_selector_preview_margin") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = COLOR_SELECTOR_PREVIEW_MARGIN.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("paginator_tab_padding_x") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = PAGINATOR_TAB_PADDING_X.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("button_padding") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = BUTTON_PADDING.write() {
                        *lock = val;
                    }
                }
            } else if let Some(rest) = trimmed.strip_prefix("paginator_tab_padding_y") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = BUTTON_PADDING.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("button_height") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = BUTTON_HEIGHT.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("button_strip_spacing") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = BUTTON_STRIP_SPACING.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("textbox_height") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = TEXTBOX_HEIGHT.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("dropdown_height") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = DROPDOWN_HEIGHT.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("slider_height") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = SLIDER_HEIGHT.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("rangeslider_height") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = RANGESLIDER_HEIGHT.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("button_corner_radius") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = BUTTON_CORNER_RADIUS.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("slider_corner_radius") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = SLIDER_CORNER_RADIUS.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("rangeslider_corner_radius") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = RANGESLIDER_CORNER_RADIUS.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("toggle_border_width") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = TOGGLE_BORDER_WIDTH.write() {
                        *lock = val;
                    }
                }
            }
            if key == "control_label_font_detached" {
                let font = val_str.to_string();
                let mut changed = false;
                if let Ok(mut lock) = CONTROL_LABEL_FONT_DETACHED.write() {
                    if *lock != font {
                        *lock = font;
                        changed = true;
                    }
                }
                if changed {
                    label_font_detached_changed = true;
                }
            }
            if key == "control_label_font" {
                let font = val_str.to_string();
                let mut changed = false;
                if let Ok(mut lock) = CONTROL_LABEL_FONT.write() {
                    if *lock != font {
                        *lock = font;
                        changed = true;
                    }
                }
                if changed {
                    label_font_changed = true;
                }
            }
            if key == "font_selector_font" {
                let font = val_str.to_string();
                let mut changed = false;
                if let Ok(mut lock) = FONT_SELECTOR_FONT.write() {
                    if *lock != font {
                        *lock = font;
                        changed = true;
                    }
                }
                if changed {
                    font_selector_font_changed = true;
                }
            }
            if key == "button_strip_font" {
                let font = val_str.to_string();
                let mut changed = false;
                if let Ok(mut lock) = BUTTON_STRIP_FONT.write() {
                    if *lock != font {
                        *lock = font;
                        changed = true;
                    }
                }
                if changed {
                    button_strip_font_changed = true;
                }
            }
            if key == "list_font" {
                let font = val_str.to_string();
                let mut changed = false;
                if let Ok(mut lock) = LIST_FONT.write() {
                    if *lock != font {
                        *lock = font;
                        changed = true;
                    }
                }
                if changed {
                    list_font_changed = true;
                }
            }
            if key == "tree_font" {
                let font = val_str.to_string();
                let mut changed = false;
                if let Ok(mut lock) = TREE_FONT.write() {
                    if *lock != font {
                        *lock = font;
                        changed = true;
                    }
                }
                if changed {
                    tree_font_changed = true;
                }
            }
            if key == "graph_font" {
                let font = val_str.to_string();
                let mut changed = false;
                if let Ok(mut lock) = GRAPH_FONT.write() {
                    if *lock != font {
                        *lock = font;
                        changed = true;
                    }
                }
                if changed {
                    graph_font_changed = true;
                }
            }
            if key == "graph_node_font" {
                let font = val_str.to_string();
                let mut changed = false;
                if let Ok(mut lock) = GRAPH_NODE_FONT.write() {
                    if *lock != font {
                        *lock = font;
                        changed = true;
                    }
                }
                if changed {
                    graph_node_font_changed = true;
                }
            }
            if let Some(rest) = trimmed.strip_prefix("list_justification") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<u8>() {
                    if let Ok(mut lock) = LIST_JUSTIFICATION.write() {
                        *lock = val;
                    }
                }
            }
        }
        if menubar_font_changed {
            if let Ok(mut lock) = MENUBAR_FONT_CACHED.write() {
                *lock = None;
            }
        }
        if statusbar_font_changed {
            if let Ok(mut lock) = STATUSBAR_FONT_CACHED.write() {
                *lock = None;
            }
        }
        if label_font_detached_changed {
            if let Ok(mut lock) = CONTROL_LABEL_FONT_DETACHED_CACHED.write() {
                *lock = None;
            }
        }
        if font_selector_font_changed {
            if let Ok(mut lock) = FONT_SELECTOR_FONT_CACHED.write() {
                *lock = None;
            }
        }
        if button_strip_font_changed {
            if let Ok(mut lock) = BUTTON_STRIP_FONT_CACHED.write() {
                *lock = None;
            }
        }
        if label_font_changed {
            if let Ok(mut lock) = CONTROL_LABEL_FONT_CACHED.write() {
                *lock = None;
            }
        }

        if list_font_changed {
            if let Ok(mut lock) = LIST_FONT_CACHED.write() {
                *lock = None;
            }
        }
        if tree_font_changed {
            if let Ok(mut lock) = TREE_FONT_CACHED.write() {
                *lock = None;
            }
        }
        if graph_font_changed {
            if let Ok(mut lock) = GRAPH_FONT_CACHED.write() {
                *lock = None;
            }
        }
        if graph_node_font_changed {
            if let Ok(mut lock) = GRAPH_NODE_FONT_CACHED.write() {
                *lock = None;
            }
        }
        if let Ok(raw_kdl) = std::fs::read_to_string(crate::config::get_config_path()) {
            crate::color::reload_colors(&raw_kdl);
        }
    }
}

fn mod_rest(rest: &str) -> &str {
    let rest = rest.trim();
    if rest.starts_with('"') && rest.ends_with('"') && rest.len() >= 2 {
        &rest[1..rest.len() - 1]
    } else {
        rest
    }
}

pub fn control_label_margin() -> f32 {
    *CONTROL_LABEL_MARGIN.read().unwrap()
}

pub fn control_label_layout() -> String {
    let lock = CONTROL_LABEL_LAYOUT.read().unwrap();
    if lock.is_empty() {
        "top".to_string()
    } else {
        lock.clone()
    }
}

pub fn label_margin() -> f32 {
    control_label_margin()
}

pub fn set_control_label_margin(margin: f32) {
    if let Ok(mut lock) = CONTROL_LABEL_MARGIN.write() {
        *lock = margin;
    }
}

pub fn set_label_margin(margin: f32) {
    set_control_label_margin(margin);
}

pub fn nested_section_label_alignment() -> u8 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("nested_section_label_alignment") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<u8>() {
                        if let Ok(mut lock) = NESTED_SECTION_LABEL_ALIGNMENT.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *NESTED_SECTION_LABEL_ALIGNMENT.read().unwrap()
}

pub fn set_nested_section_label_alignment(align: u8) {
    if let Ok(mut lock) = NESTED_SECTION_LABEL_ALIGNMENT.write() {
        *lock = align;
    }
}

static NESTED_SECTION_LABEL_OFFSET: RwLock<f32> = RwLock::new(0.0);

pub fn nested_section_label_offset() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("nested_section_label_offset") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = NESTED_SECTION_LABEL_OFFSET.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *NESTED_SECTION_LABEL_OFFSET.read().unwrap()
}

pub fn set_nested_section_label_offset(offset: f32) {
    if let Ok(mut lock) = NESTED_SECTION_LABEL_OFFSET.write() {
        *lock = offset;
    }
}


pub fn plate_padding() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("plate_padding") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = PLATE_PADDING.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *PLATE_PADDING.read().unwrap()
}

pub fn set_plate_padding(padding: f32) {
    if let Ok(mut lock) = PLATE_PADDING.write() {
        *lock = padding;
    }
}

static PAGE_MARGIN: RwLock<f32> = RwLock::new(20.0);

pub fn page_margin() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("page_margin") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = PAGE_MARGIN.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *PAGE_MARGIN.read().unwrap()
}

pub fn set_page_margin(margin: f32) {
    if let Ok(mut lock) = PAGE_MARGIN.write() {
        *lock = margin;
    }
}

static GRID_MIN_COL_WIDTH: RwLock<f32> = RwLock::new(260.0);

pub fn grid_min_col_width() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("grid_min_col_width") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = GRID_MIN_COL_WIDTH.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *GRID_MIN_COL_WIDTH.read().unwrap()
}

pub fn set_grid_min_col_width(width: f32) {
    if let Ok(mut lock) = GRID_MIN_COL_WIDTH.write() {
        *lock = width;
    }
}

static GRID_GAP: RwLock<f32> = RwLock::new(8.0);

pub fn grid_gap() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("grid_gap") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = GRID_GAP.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *GRID_GAP.read().unwrap()
}

pub fn set_grid_gap(gap: f32) {
    if let Ok(mut lock) = GRID_GAP.write() {
        *lock = gap;
    }
}

pub fn column_gap() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("column_gap") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = COLUMN_GAP.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *COLUMN_GAP.read().unwrap()
}

pub fn set_column_gap(gap: f32) {
    if let Ok(mut lock) = COLUMN_GAP.write() {
        *lock = gap;
    }
}

pub fn control_panel_padding() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("control_panel_padding") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = CONTROL_PANEL_PADDING.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *CONTROL_PANEL_PADDING.read().unwrap()
}

pub fn set_control_panel_padding(padding: f32) {
    if let Ok(mut lock) = CONTROL_PANEL_PADDING.write() {
        *lock = padding;
    }
}

pub fn control_panel_gap() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("control_panel_gap") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = CONTROL_PANEL_GAP.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *CONTROL_PANEL_GAP.read().unwrap()
}

pub fn set_control_panel_gap(gap: f32) {
    if let Ok(mut lock) = CONTROL_PANEL_GAP.write() {
        *lock = gap;
    }
}

pub fn section_padding() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("section_padding") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = SECTION_PADDING.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *SECTION_PADDING.read().unwrap()
}

pub fn set_section_padding(padding: f32) {
    if let Ok(mut lock) = SECTION_PADDING.write() {
        *lock = padding;
    }
}

pub fn spinbox_height() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("spinbox_height") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = SPINBOX_HEIGHT.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *SPINBOX_HEIGHT.read().unwrap()
}

pub fn set_spinbox_height(height: f32) {
    if let Ok(mut lock) = SPINBOX_HEIGHT.write() {
        *lock = height;
    }
}

pub fn toggle_height() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("toggle_height") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = TOGGLE_HEIGHT.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *TOGGLE_HEIGHT.read().unwrap()
}

pub fn set_toggle_height(height: f32) {
    if let Ok(mut lock) = TOGGLE_HEIGHT.write() {
        *lock = height;
    }
}

pub fn light_source_position() -> f32 {
    let val = get_style_registry().read().unwrap().get_float("light_source_position").unwrap_or(2.3561945);
    if val > 2.0 * std::f32::consts::PI {
        val.to_radians()
    } else {
        val
    }
}

pub fn bevel_depth() -> f32 {
    get_style_registry().read().unwrap().get_float("bevel_depth").unwrap_or(0.15)
}

/// Corner shape exponent for SDF-lit plates: 2.0 (the default) is a circular
/// arc; higher values are superellipse "squircle" corners with continuous
/// curvature — ~4.5 is the Apple-like look. Clamped to [2, 16]: below 2 the
/// Lp construction degenerates toward a chamfer, above 16 it is visually a
/// square corner and the pow() terms start flirting with f32 range.
pub fn corner_shape() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("corner_shape").unwrap_or(2.0).clamp(2.0, 16.0)
}

/// The curvature-matched corner-span factor for window-scale squircle corners.
/// A raw superellipse of exponent n at a circle's nominal radius turns tighter
/// at the diagonal than that circle — its radius of curvature there is
/// √2·r / (2^(1/n)·(n − 1)). Scaling the corner span by this factor makes the
/// diagonal curvature equal the configured radius, so the corner reads as the
/// same size as a circular one (the same reason Apple's continuous corners run
/// ~1.5·r along the edge). Exactly 1 at n = 2. Applied to window-scale corners
/// only — `Prim::Plate` and the renderer's window-corner clip — never to
/// widget-scale radii, which must match the nominal-radius squircles around them.
pub fn corner_span_factor() -> f32 {
    let n = corner_shape();
    if n > 2.001 {
        (n - 1.0) * 2f32.powf(1.0 / n) / std::f32::consts::SQRT_2
    } else {
        1.0
    }
}

/// Whether plates/bevels/recesses render through shader2d's per-pixel SDF-lit
/// plate branch (the default) or the legacy banded vertex shading. `bevel_shader 0`
/// in config flips back to the old look for A/B comparison.
pub fn bevel_shader() -> bool {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("bevel_shader").map(|v| v != 0.0).unwrap_or(true)
}

/// How wide a rolled edge is, in logical px — the distance over which a plate's perimeter
/// or a recess wall curves away from the flat surface. `bevel_depth` is the companion
/// knob: it sets how hard the light falls across that distance. Wide and shallow reads as
/// thick glass; narrow and deep reads as a stamped metal lip.
pub fn bevel_width() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("bevel_width").unwrap_or(9.3)
}

/// Sample count of the custom bevel profile LUT ([`set_bevel_profile_keys`]).
pub const BEVEL_PROFILE_SAMPLES: usize = 32;

/// The custom bevel/carve height profile, as the slope LUT the renderer uploads
/// to the 2D shader: slot `i` holds `h'` at `v = (i + 0.5) / N` of the wall's
/// height curve `h(v)` (`v` runs 0 at the surrounding plateau → 1 at the carve
/// floor / boss crest; `h` in units of the feature's depth, so a 0→1 curve is
/// the classic full-depth bevel and a curve ending back at its start height is
/// a pure decorative rim). `None` = the analytic smoothstep profile.
static BEVEL_PROFILE: std::sync::RwLock<Option<[f32; BEVEL_PROFILE_SAMPLES]>> =
    std::sync::RwLock::new(None);
/// Bumped on every profile change so renderers know to re-upload their LUT.
static BEVEL_PROFILE_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The custom EDGE (plate roll) profile — same slope-LUT encoding as
/// [`BEVEL_PROFILE`], but read by the shader's `roll_slope` for the perimeter
/// roll of widget-scale plates: `v` runs 0 at the face join → 1 at the
/// silhouette, and the curve is the roll's descent progress (0 = face height,
/// 1 = fully dropped), so the identity curve is a straight chamfer and `None`
/// is the analytic superellipse quadrant.
static ROLL_PROFILE: std::sync::RwLock<Option<[f32; BEVEL_PROFILE_SAMPLES]>> =
    std::sync::RwLock::new(None);
static ROLL_PROFILE_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Evaluate a ramp key list at `t` — the same piecewise interpolation
/// `cce_ui::widget::Ramp::get_interpolated_value` draws, so the bevel renders
/// exactly the curve the ramp widget shows (`smooth` = the widget's Bezier line
/// type: smoothstep blending between keys; else linear).
pub fn sample_ramp_keys(keys: &[(f32, f32)], smooth: bool, t: f32) -> f32 {
    let Some(first) = keys.first() else { return 0.0 };
    let last = keys.last().unwrap();
    if t <= first.0 {
        return first.1;
    }
    if t >= last.0 {
        return last.1;
    }
    for pair in keys.windows(2) {
        let (k1, k2) = (pair[0], pair[1]);
        if t >= k1.0 && t <= k2.0 {
            let range = k2.0 - k1.0;
            if range.abs() < 0.0001 {
                return k1.1;
            }
            let mut w = (t - k1.0) / range;
            if smooth {
                w = w * w * (3.0 - 2.0 * w);
            }
            return k1.1 * (1.0 - w) + k2.1 * w;
        }
    }
    first.1
}

/// Install a custom bevel/carve profile from ramp keys (`(pos, value)`, both
/// 0..1, sorted by pos). Sampled into the slope LUT the shader's `carve_slope`
/// reads in place of its analytic smoothstep — every recess/boss/ridge wall in
/// this process restyles on the next frame. Empty or single-key lists clear
/// back to the analytic profile ([`clear_bevel_profile`]).
pub fn set_bevel_profile_keys(keys: &[(f32, f32)], smooth: bool) {
    if keys.len() < 2 {
        clear_bevel_profile();
        return;
    }
    *BEVEL_PROFILE.write().unwrap() = Some(ramp_slope_lut(keys, smooth));
    BEVEL_PROFILE_GEN.fetch_add(1, std::sync::atomic::Ordering::Release);
}

/// A ramp key list sampled into the shader's slope LUT — slot `i` holds the
/// curve's slope at `v = (i + 0.5) / N`.
fn ramp_slope_lut(keys: &[(f32, f32)], smooth: bool) -> [f32; BEVEL_PROFILE_SAMPLES] {
    let n = BEVEL_PROFILE_SAMPLES;
    let mut slopes = [0.0f32; BEVEL_PROFILE_SAMPLES];
    for (i, slot) in slopes.iter_mut().enumerate() {
        let h0 = sample_ramp_keys(keys, smooth, i as f32 / n as f32);
        let h1 = sample_ramp_keys(keys, smooth, (i + 1) as f32 / n as f32);
        *slot = (h1 - h0) * n as f32;
    }
    slopes
}

/// Install a custom EDGE profile for the plate perimeter roll from ramp keys —
/// the [`set_bevel_profile_keys`] twin for [`ROLL_PROFILE`]. The curve is the
/// roll's descent progress from the face join (0) to the silhouette (1); the
/// shader's `roll_slope` samples it in place of the analytic superellipse
/// quadrant. Empty or single-key lists clear back to the analytic roll.
pub fn set_roll_profile_keys(keys: &[(f32, f32)], smooth: bool) {
    if keys.len() < 2 {
        clear_roll_profile();
        return;
    }
    *ROLL_PROFILE.write().unwrap() = Some(ramp_slope_lut(keys, smooth));
    ROLL_PROFILE_GEN.fetch_add(1, std::sync::atomic::Ordering::Release);
}

/// Drop the custom edge profile — plate rolls return to the analytic quadrant.
pub fn clear_roll_profile() {
    let mut guard = ROLL_PROFILE.write().unwrap();
    if guard.is_some() {
        *guard = None;
        ROLL_PROFILE_GEN.fetch_add(1, std::sync::atomic::Ordering::Release);
    }
}

/// The installed edge profile's slope LUT, if any — what the renderer uploads.
pub fn roll_profile_slopes() -> Option<[f32; BEVEL_PROFILE_SAMPLES]> {
    *ROLL_PROFILE.read().unwrap()
}

/// Change counter for [`roll_profile_slopes`].
pub fn roll_profile_generation() -> u64 {
    ROLL_PROFILE_GEN.load(std::sync::atomic::Ordering::Acquire)
}

/// Drop the custom bevel profile — walls return to the analytic smoothstep.
pub fn clear_bevel_profile() {
    let mut guard = BEVEL_PROFILE.write().unwrap();
    if guard.is_some() {
        *guard = None;
        BEVEL_PROFILE_GEN.fetch_add(1, std::sync::atomic::Ordering::Release);
    }
}

/// The installed profile's slope LUT, if any — what the renderer uploads.
pub fn bevel_profile_slopes() -> Option<[f32; BEVEL_PROFILE_SAMPLES]> {
    *BEVEL_PROFILE.read().unwrap()
}

/// Change counter for [`bevel_profile_slopes`] — a renderer re-uploads when it
/// differs from the generation it last wrote.
pub fn bevel_profile_generation() -> u64 {
    BEVEL_PROFILE_GEN.load(std::sync::atomic::Ordering::Acquire)
}

/// Padding between the window plate's edge and the objects sitting on it, in
/// logical px (`style.surface.backplate.padding` in config.kdl). DE-wide so
/// every app's content sits the same distance off the plate rim.
pub fn backplate_padding() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("backplate_padding").unwrap_or(16.0)
}

/// Gap between sibling objects on the window plate, in logical px
/// (`style.surface.backplate.gap` in config.kdl) — pane splits, control rows.
/// The companion to [`backplate_padding`]: rim distance vs object spacing.
pub fn backplate_gap() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("backplate_gap").unwrap_or(12.0)
}

/// Roll-off width for the wall where a bar (menubar / status bar / the demo's
/// header band) steps down into the window plate. Wider than the plate's own
/// perimeter roll on purpose: the carve depth saturates at `bevel_width` in the
/// tessellator, so the extra width flattens the wall's slope — a soft, gradual
/// transition into the bar — instead of cutting a proportionally deeper groove.
pub fn bar_wall_width() -> f32 {
    bevel_width() * 1.75
}

/// DE-wide default for the controls' relief styling (`window_manager.control_relief`
/// in config.kdl, default on): raised Button/Toggle/Dropdown plates, recessed
/// TextBox/Slider wells, recessed MenuBar/StatusBar bands. Widgets read this at
/// construction; the per-widget `with_raised` / `with_recessed` / `with_recess`
/// builders override it either way. `0` reverts the whole DE to the flat look.
pub fn control_relief() -> bool {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("control_relief").map(|v| v != 0.0).unwrap_or(true)
}

pub fn toggle_corner_radius() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("toggle_corner_radius").unwrap_or(4.0)
}

pub fn set_toggle_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("toggle_corner_radius", radius);
    }
}

pub fn toggle_border_width() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("toggle_border_width") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = TOGGLE_BORDER_WIDTH.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *TOGGLE_BORDER_WIDTH.read().unwrap()
}

pub fn set_toggle_border_width(width: f32) {
    if let Ok(mut lock) = TOGGLE_BORDER_WIDTH.write() {
        *lock = width;
    }
}

pub fn slider_corner_radius() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("slider_corner_radius").unwrap_or(4.0)
}

/// The toggle's render style: `style.control.toggle.style = "slide"` swaps the
/// rocker/gradient pill for a half-width button that slides between the left
/// (off) and right (on) ends of the widget (see `Toggle::paint`). Anything
/// else — or unset — keeps the default look, so apps opt in per-config.
pub fn toggle_slide() -> bool {
    lazy_init_style_registry();
    get_style_registry()
        .read()
        .unwrap()
        .get_string("toggle_style")
        .is_some_and(|s| s == "slide")
}

/// The slider's render style: `style.control.slider.style = "band"` swaps the
/// track/fill/thumb for a thin full-range band that inflates smoothly at the
/// value (see `Slider::paint`). Anything else — or unset — keeps the default
/// look, so apps opt in per-config.
pub fn slider_band() -> bool {
    lazy_init_style_registry();
    get_style_registry()
        .read()
        .unwrap()
        .get_string("slider_style")
        .is_some_and(|s| s == "band")
}

/// Band-style knobs (`style.control.slider.*`): the flat band's thickness, and
/// the bulge's half-span / peak height around the value position.
pub fn slider_band_thickness() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("slider_band_thickness").unwrap_or(2.0)
}

pub fn slider_bulge_width() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("slider_bulge_width").unwrap_or(26.0)
}

pub fn slider_bulge_height() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("slider_bulge_height").unwrap_or(14.0)
}

pub fn set_slider_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("slider_corner_radius", radius);
    }
}

pub fn rangeslider_corner_radius() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("rangeslider_corner_radius").unwrap_or(4.0)
}

pub fn set_rangeslider_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("rangeslider_corner_radius", radius);
    }
}

pub fn plate_corner_radius() -> f32 {
    lazy_init_style_registry();
    let r = get_style_registry().read().unwrap();
    r.get_float("plate_corner_radius")
        .or_else(|| r.get_float("backplate_corner_radius"))
        .unwrap_or(12.0)
}

pub fn set_plate_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("plate_corner_radius", radius);
    }
}

pub fn plate_opacity() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("plate_opacity") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = PLATE_OPACITY.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *PLATE_OPACITY.read().unwrap()
}

pub fn set_plate_opacity(opacity: f32) {
    if let Ok(mut lock) = PLATE_OPACITY.write() {
        *lock = opacity;
    }
}

pub fn page_opacity() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("page_opacity") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = PAGE_OPACITY.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *PAGE_OPACITY.read().unwrap()
}

pub fn set_page_opacity(opacity: f32) {
    if let Ok(mut lock) = PAGE_OPACITY.write() {
        *lock = opacity;
    }
}

pub fn layer_opacity() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("layer_opacity") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = LAYER_OPACITY.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *LAYER_OPACITY.read().unwrap()
}

pub fn set_layer_opacity(opacity: f32) {
    if let Ok(mut lock) = LAYER_OPACITY.write() {
        *lock = opacity;
    }
}




pub fn color_selector_height() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("color_selector_height") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = COLOR_SELECTOR_HEIGHT.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *COLOR_SELECTOR_HEIGHT.read().unwrap()
}

pub fn set_color_selector_height(height: f32) {
    if let Ok(mut lock) = COLOR_SELECTOR_HEIGHT.write() {
        *lock = height;
    }
}

pub fn font_selector_height() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("font_selector_height") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = FONT_SELECTOR_HEIGHT.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *FONT_SELECTOR_HEIGHT.read().unwrap()
}

pub fn set_font_selector_height(height: f32) {
    if let Ok(mut lock) = FONT_SELECTOR_HEIGHT.write() {
        *lock = height;
    }
}

pub fn color_selector_font() -> String {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let mut font = "monospace".to_string();
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("color_selector_font") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=');
                    let rest = rest.trim();
                    let val_str = if rest.starts_with('"') && rest.ends_with('"') && rest.len() >= 2 {
                        &rest[1..rest.len() - 1]
                    } else {
                        rest
                    };
                    font = val_str.trim().to_string();
                }
            }
        }
        if let Ok(mut lock) = COLOR_SELECTOR_FONT.write() {
            *lock = font;
        }
    });
    let lock = COLOR_SELECTOR_FONT.read().unwrap();
    if lock.is_empty() {
        "monospace".to_string()
    } else {
        lock.clone()
    }
}

pub fn set_color_selector_font(font: &str) {
    if let Ok(mut lock) = COLOR_SELECTOR_FONT.write() {
        *lock = font.to_string();
    }
}

pub fn menubar_font() -> String {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let font = read_config_value("menubar_font").unwrap_or_else(|| "Berkeley Mono".to_string());
        if let Ok(mut lock) = MENUBAR_FONT.write() {
            *lock = font;
        }
    });
    let lock = MENUBAR_FONT.read().unwrap();
    if lock.is_empty() {
        "Berkeley Mono".to_string()
    } else {
        lock.clone()
    }
}

pub fn menubar_font_parsed() -> (String, f32) {
    if let Ok(lock) = MENUBAR_FONT_CACHED.read() {
        if let Some(ref val) = *lock {
            return val.clone();
        }
    }
    let font_str = menubar_font();
    let parsed = parse_font_string(&font_str);
    let size = parsed.1.unwrap_or(12.0);
    let val = (parsed.0, size);
    if let Ok(mut lock) = MENUBAR_FONT_CACHED.write() {
        *lock = Some(val.clone());
    }
    val
}

pub fn set_menubar_font(font: &str) {
    if let Ok(mut lock) = MENUBAR_FONT.write() {
        *lock = font.to_string();
    }
    if let Ok(mut lock) = MENUBAR_FONT_CACHED.write() {
        *lock = None;
    }
}

pub fn statusbar_font() -> String {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let font = read_config_value("statusbar_font").unwrap_or_else(|| "Berkeley Mono".to_string());
        if let Ok(mut lock) = STATUSBAR_FONT.write() {
            *lock = font;
        }
    });
    let lock = STATUSBAR_FONT.read().unwrap();
    if lock.is_empty() {
        "Berkeley Mono".to_string()
    } else {
        lock.clone()
    }
}

pub fn statusbar_font_parsed() -> (String, f32) {
    if let Ok(lock) = STATUSBAR_FONT_CACHED.read() {
        if let Some(ref val) = *lock {
            return val.clone();
        }
    }
    let font_str = statusbar_font();
    let parsed = parse_font_string(&font_str);
    let size = parsed.1.unwrap_or(12.0);
    let val = (parsed.0, size);
    if let Ok(mut lock) = STATUSBAR_FONT_CACHED.write() {
        *lock = Some(val.clone());
    }
    val
}

pub fn set_statusbar_font(font: &str) {
    if let Ok(mut lock) = STATUSBAR_FONT.write() {
        *lock = font.to_string();
    }
    if let Ok(mut lock) = STATUSBAR_FONT_CACHED.write() {
        *lock = None;
    }
}



pub fn font_selector_font() -> String {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let font = read_config_value("font_selector_font").unwrap_or_else(|| "Berkeley Mono".to_string());
        if let Ok(mut lock) = FONT_SELECTOR_FONT.write() {
            *lock = font;
        }
    });
    let lock = FONT_SELECTOR_FONT.read().unwrap();
    if lock.is_empty() {
        "Berkeley Mono".to_string()
    } else {
        lock.clone()
    }
}

pub fn font_selector_font_parsed() -> (String, f32) {
    if let Ok(lock) = FONT_SELECTOR_FONT_CACHED.read() {
        if let Some(ref val) = *lock {
            return val.clone();
        }
    }
    let font_str = font_selector_font();
    let parsed = parse_font_string(&font_str);
    let size = parsed.1.unwrap_or(12.0);
    let val = (parsed.0, size);
    if let Ok(mut lock) = FONT_SELECTOR_FONT_CACHED.write() {
        *lock = Some(val.clone());
    }
    val
}

pub fn set_font_selector_font(font: &str) {
    if let Ok(mut lock) = FONT_SELECTOR_FONT.write() {
        *lock = font.to_string();
    }
    if let Ok(mut lock) = FONT_SELECTOR_FONT_CACHED.write() {
        *lock = None;
    }
}

// Button Strip Font
pub fn button_strip_font() -> String {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let font = read_config_value("button_strip_font").unwrap_or_else(|| "Berkeley Mono".to_string());
        if let Ok(mut lock) = BUTTON_STRIP_FONT.write() {
            *lock = font;
        }
    });
    let lock = BUTTON_STRIP_FONT.read().unwrap();
    if lock.is_empty() {
        "Berkeley Mono".to_string()
    } else {
        lock.clone()
    }
}

pub fn button_strip_font_parsed() -> (String, f32) {
    if let Ok(lock) = BUTTON_STRIP_FONT_CACHED.read() {
        if let Some(ref val) = *lock {
            return val.clone();
        }
    }
    let font_str = button_strip_font();
    let parsed = parse_font_string(&font_str);
    let size = parsed.1.unwrap_or(12.0);
    let val = (parsed.0, size);
    if let Ok(mut lock) = BUTTON_STRIP_FONT_CACHED.write() {
        *lock = Some(val.clone());
    }
    val
}

pub fn set_button_strip_font(font: &str) {
    if let Ok(mut lock) = BUTTON_STRIP_FONT.write() {
        *lock = font.to_string();
    }
    if let Ok(mut lock) = BUTTON_STRIP_FONT_CACHED.write() {
        *lock = None;
    }
}



// Control Label Font
pub fn control_label_font() -> String {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let font = read_config_value("control_label_font").unwrap_or_else(|| "Berkeley Mono".to_string());
        if let Ok(mut lock) = CONTROL_LABEL_FONT.write() {
            *lock = font;
        }
    });
    let lock = CONTROL_LABEL_FONT.read().unwrap();
    if lock.is_empty() {
        "Berkeley Mono".to_string()
    } else {
        lock.clone()
    }
}

pub fn control_label_font_parsed() -> (String, f32) {
    if let Ok(lock) = CONTROL_LABEL_FONT_CACHED.read() {
        if let Some(ref val) = *lock {
            return val.clone();
        }
    }
    let font_str = control_label_font();
    let parsed = parse_font_string(&font_str);
    let size = parsed.1.unwrap_or(12.0);
    let val = (parsed.0, size);
    if let Ok(mut lock) = CONTROL_LABEL_FONT_CACHED.write() {
        *lock = Some(val.clone());
    }
    val
}

pub fn set_control_label_font(font: &str) {
    if let Ok(mut lock) = CONTROL_LABEL_FONT.write() {
        *lock = font.to_string();
    }
    if let Ok(mut lock) = CONTROL_LABEL_FONT_CACHED.write() {
        *lock = None;
    }
}

// Control Label Font Detached
pub fn control_label_font_detached() -> String {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let font = read_config_value("control_label_font_detached").unwrap_or_else(|| "Berkeley Mono".to_string());
        if let Ok(mut lock) = CONTROL_LABEL_FONT_DETACHED.write() {
            *lock = font;
        }
    });
    let lock = CONTROL_LABEL_FONT_DETACHED.read().unwrap();
    if lock.is_empty() {
        "Berkeley Mono".to_string()
    } else {
        lock.clone()
    }
}

pub fn control_label_font_detached_parsed() -> (String, f32) {
    if let Ok(lock) = CONTROL_LABEL_FONT_DETACHED_CACHED.read() {
        if let Some(ref val) = *lock {
            return val.clone();
        }
    }
    let font_str = control_label_font_detached();
    let parsed = parse_font_string(&font_str);
    let size = parsed.1.unwrap_or(12.0);
    let val = (parsed.0, size);
    if let Ok(mut lock) = CONTROL_LABEL_FONT_DETACHED_CACHED.write() {
        *lock = Some(val.clone());
    }
    val
}

pub fn set_control_label_font_detached(font: &str) {
    if let Ok(mut lock) = CONTROL_LABEL_FONT_DETACHED.write() {
        *lock = font.to_string();
    }
    if let Ok(mut lock) = CONTROL_LABEL_FONT_DETACHED_CACHED.write() {
        *lock = None;
    }
}



// List Font
pub fn list_font() -> String {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let font = read_config_value("list_font").unwrap_or_else(|| "Berkeley Mono".to_string());
        if let Ok(mut lock) = LIST_FONT.write() {
            *lock = font;
        }
    });
    let lock = LIST_FONT.read().unwrap();
    if lock.is_empty() {
        "Berkeley Mono".to_string()
    } else {
        lock.clone()
    }
}

pub fn list_font_parsed() -> (String, f32) {
    if let Ok(lock) = LIST_FONT_CACHED.read() {
        if let Some(ref val) = *lock {
            return val.clone();
        }
    }
    let font_str = list_font();
    let parsed = parse_font_string(&font_str);
    let size = parsed.1.unwrap_or(12.0);
    let val = (parsed.0, size);
    if let Ok(mut lock) = LIST_FONT_CACHED.write() {
        *lock = Some(val.clone());
    }
    val
}

pub fn set_list_font(font: &str) {
    if let Ok(mut lock) = LIST_FONT.write() {
        *lock = font.to_string();
    }
    if let Ok(mut lock) = LIST_FONT_CACHED.write() {
        *lock = None;
    }
}

// Tree Font
pub fn tree_font() -> String {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let font = read_config_value("tree_font").unwrap_or_else(|| "Berkeley Mono".to_string());
        if let Ok(mut lock) = TREE_FONT.write() {
            *lock = font;
        }
    });
    let lock = TREE_FONT.read().unwrap();
    if lock.is_empty() {
        "Berkeley Mono".to_string()
    } else {
        lock.clone()
    }
}

pub fn tree_font_parsed() -> (String, f32) {
    if let Ok(lock) = TREE_FONT_CACHED.read() {
        if let Some(ref val) = *lock {
            return val.clone();
        }
    }
    let font_str = tree_font();
    let parsed = parse_font_string(&font_str);
    let size = parsed.1.unwrap_or(12.0);
    let val = (parsed.0, size);
    if let Ok(mut lock) = TREE_FONT_CACHED.write() {
        *lock = Some(val.clone());
    }
    val
}

pub fn set_tree_font(font: &str) {
    if let Ok(mut lock) = TREE_FONT.write() {
        *lock = font.to_string();
    }
    if let Ok(mut lock) = TREE_FONT_CACHED.write() {
        *lock = None;
    }
}

// Graph Font
pub fn graph_font() -> String {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let font = read_config_value("graph_font").unwrap_or_else(|| "Berkeley Mono".to_string());
        if let Ok(mut lock) = GRAPH_FONT.write() {
            *lock = font;
        }
    });
    let lock = GRAPH_FONT.read().unwrap();
    if lock.is_empty() {
        "Berkeley Mono".to_string()
    } else {
        lock.clone()
    }
}

pub fn graph_font_parsed() -> (String, f32) {
    if let Ok(lock) = GRAPH_FONT_CACHED.read() {
        if let Some(ref val) = *lock {
            return val.clone();
        }
    }
    let font_str = graph_font();
    let parsed = parse_font_string(&font_str);
    let size = parsed.1.unwrap_or(12.0);
    let val = (parsed.0, size);
    if let Ok(mut lock) = GRAPH_FONT_CACHED.write() {
        *lock = Some(val.clone());
    }
    val
}

pub fn set_graph_font(font: &str) {
    if let Ok(mut lock) = GRAPH_FONT.write() {
        *lock = font.to_string();
    }
    if let Ok(mut lock) = GRAPH_FONT_CACHED.write() {
        *lock = None;
    }
}

// Graph Node Font
pub fn graph_node_font() -> String {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let font = read_config_value("graph_node_font").unwrap_or_else(|| "Berkeley Mono".to_string());
        if let Ok(mut lock) = GRAPH_NODE_FONT.write() {
            *lock = font;
        }
    });
    let lock = GRAPH_NODE_FONT.read().unwrap();
    if lock.is_empty() {
        "Berkeley Mono".to_string()
    } else {
        lock.clone()
    }
}

pub fn graph_node_font_parsed() -> (String, f32) {
    if let Ok(lock) = GRAPH_NODE_FONT_CACHED.read() {
        if let Some(ref val) = *lock {
            return val.clone();
        }
    }
    let font_str = graph_node_font();
    let parsed = parse_font_string(&font_str);
    let size = parsed.1.unwrap_or(12.0);
    let val = (parsed.0, size);
    if let Ok(mut lock) = GRAPH_NODE_FONT_CACHED.write() {
        *lock = Some(val.clone());
    }
    val
}

pub fn set_graph_node_font(font: &str) {
    if let Ok(mut lock) = GRAPH_NODE_FONT.write() {
        *lock = font.to_string();
    }
    if let Ok(mut lock) = GRAPH_NODE_FONT_CACHED.write() {
        *lock = None;
    }
}

// List Justification
pub fn list_justification() -> u8 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("list_justification") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<u8>() {
                        if let Ok(mut lock) = LIST_JUSTIFICATION.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *LIST_JUSTIFICATION.read().unwrap()
}

pub fn set_list_justification(just: u8) {
    if let Ok(mut lock) = LIST_JUSTIFICATION.write() {
        *lock = just;
    }
}




pub fn section_label_font() -> String {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let mut font = "Berkeley Mono".to_string();
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("section_label_font") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=');
                    let rest = rest.trim();
                    let val_str = if rest.starts_with('"') && rest.ends_with('"') && rest.len() >= 2 {
                        &rest[1..rest.len() - 1]
                    } else {
                        rest
                    };
                    font = val_str.trim().to_string();
                }
            }
        }
        if let Ok(mut lock) = SECTION_LABEL_FONT.write() {
            *lock = font;
        }
    });
    let lock = SECTION_LABEL_FONT.read().unwrap();
    if lock.is_empty() {
        "Berkeley Mono".to_string()
    } else {
        lock.clone()
    }
}

pub fn set_section_label_font(font: &str) {
    if let Ok(mut lock) = SECTION_LABEL_FONT.write() {
        *lock = font.to_string();
    }
}

pub fn nested_section_label_font() -> String {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let mut font = "Berkeley Mono".to_string();
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("nested_section_label_font") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=');
                    let rest = rest.trim();
                    let val_str = if rest.starts_with('"') && rest.ends_with('"') && rest.len() >= 2 {
                        &rest[1..rest.len() - 1]
                    } else {
                        rest
                    };
                    font = val_str.trim().to_string();
                }
            }
        }
        if let Ok(mut lock) = NESTED_SECTION_LABEL_FONT.write() {
            *lock = font;
        }
    });
    let lock = NESTED_SECTION_LABEL_FONT.read().unwrap();
    if lock.is_empty() {
        "Berkeley Mono".to_string()
    } else {
        lock.clone()
    }
}

pub fn set_nested_section_label_font(font: &str) {
    if let Ok(mut lock) = NESTED_SECTION_LABEL_FONT.write() {
        *lock = font.to_string();
    }
}

pub fn breadcrumb_font() -> String {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let mut font = "Berkeley Mono".to_string();
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("breadcrumb_font") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=');
                    let rest = rest.trim();
                    let val_str = if rest.starts_with('"') && rest.ends_with('"') && rest.len() >= 2 {
                        &rest[1..rest.len() - 1]
                    } else {
                        rest
                    };
                    font = val_str.trim().to_string();
                }
            }
        }
        if let Ok(mut lock) = BREADCRUMB_FONT.write() {
            *lock = font;
        }
    });
    let lock = BREADCRUMB_FONT.read().unwrap();
    if lock.is_empty() {
        "Berkeley Mono".to_string()
    } else {
        lock.clone()
    }
}

pub fn set_breadcrumb_font(font: &str) {
    if let Ok(mut lock) = BREADCRUMB_FONT.write() {
        *lock = font.to_string();
    }
}

pub fn button_font() -> String {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let mut font = "Berkeley Mono".to_string();
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("button_font") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=');
                    let rest = rest.trim();
                    let val_str = if rest.starts_with('"') && rest.ends_with('"') && rest.len() >= 2 {
                        &rest[1..rest.len() - 1]
                    } else {
                        rest
                    };
                    font = val_str.trim().to_string();
                }
            }
        }
        if let Ok(mut lock) = BUTTON_FONT.write() {
            *lock = font;
        }
    });
    let lock = BUTTON_FONT.read().unwrap();
    if lock.is_empty() {
        "Berkeley Mono".to_string()
    } else {
        lock.clone()
    }
}

pub fn set_button_font(font: &str) {
    if let Ok(mut lock) = BUTTON_FONT.write() {
        *lock = font.to_string();
    }
}

pub fn color_selector_preview_corner_radius() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("color_selector_preview_corner_radius").unwrap_or(4.0)
}

pub fn set_color_selector_preview_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("color_selector_preview_corner_radius", radius);
    }
}

pub fn color_selector_corner_radius() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("color_selector_corner_radius").unwrap_or(4.0)
}

pub fn set_color_selector_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("color_selector_corner_radius", radius);
    }
}

pub fn button_corner_radius() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("button_corner_radius").unwrap_or(4.0)
}

pub fn set_button_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("button_corner_radius", radius);
    }
}

pub fn spinbox_corner_radius() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("spinbox_corner_radius").unwrap_or(4.0)
}

pub fn set_spinbox_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("spinbox_corner_radius", radius);
    }
}

pub fn spinbox_button_padding() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("spinbox_button_padding") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = SPINBOX_BUTTON_PADDING.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *SPINBOX_BUTTON_PADDING.read().unwrap()
}

pub fn scrollbar_width() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("scrollbar_width") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = SCROLLBAR_WIDTH.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *SCROLLBAR_WIDTH.read().unwrap()
}

pub fn set_scrollbar_width(width: f32) {
    if let Ok(mut lock) = SCROLLBAR_WIDTH.write() {
        *lock = width;
    }
}

pub fn tree_opacity() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("tree_opacity") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = TREE_OPACITY.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *TREE_OPACITY.read().unwrap()
}

pub fn set_tree_opacity(opacity: f32) {
    if let Ok(mut lock) = TREE_OPACITY.write() {
        *lock = opacity;
    }
}

pub fn tree_blur() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("tree_blur") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = TREE_BLUR.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *TREE_BLUR.read().unwrap()
}

pub fn set_tree_blur(blur: f32) {
    if let Ok(mut lock) = TREE_BLUR.write() {
        *lock = blur;
    }
}

pub fn set_spinbox_button_padding(padding: f32) {
    if let Ok(mut lock) = SPINBOX_BUTTON_PADDING.write() {
        *lock = padding;
    }
}

pub fn textbox_corner_radius() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("textbox_corner_radius").unwrap_or(4.0)
}

pub fn set_textbox_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("textbox_corner_radius", radius);
    }
}

pub fn textbox_line_wrap() -> bool {
    lazy_init_style_registry();
    *TEXTBOX_LINE_WRAP.read().unwrap()
}

pub fn set_textbox_line_wrap(wrap: bool) {
    lazy_init_style_registry();
    if let Ok(mut lock) = TEXTBOX_LINE_WRAP.write() {
        *lock = wrap;
    }
}

pub fn touchpad_natural_scroll() -> bool {
    lazy_init_style_registry();
    *TOUCHPAD_NATURAL_SCROLL.read().unwrap()
}

pub fn set_touchpad_natural_scroll(enabled: bool) {
    lazy_init_style_registry();
    if let Ok(mut lock) = TOUCHPAD_NATURAL_SCROLL.write() {
        *lock = enabled;
    }
}

pub fn textbox_multiline_border_width() -> f32 {
    lazy_init_style_registry();
    *TEXTBOX_MULTILINE_BORDER_WIDTH.read().unwrap()
}

pub fn set_textbox_multiline_border_width(width: f32) {
    lazy_init_style_registry();
    if let Ok(mut lock) = TEXTBOX_MULTILINE_BORDER_WIDTH.write() {
        *lock = width;
    }
}

pub fn breadcrumb_corner_radius() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("breadcrumb_corner_radius").unwrap_or(4.0)
}

pub fn set_breadcrumb_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("breadcrumb_corner_radius", radius);
    }
}

pub fn list_corner_radius() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("list_corner_radius").unwrap_or(4.0)
}

pub fn set_list_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("list_corner_radius", radius);
    }
}

pub fn tree_corner_radius() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("tree_corner_radius").unwrap_or(4.0)
}

pub fn set_tree_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("tree_corner_radius", radius);
    }
}

pub fn graph_spacing_x() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("graph_spacing_x").unwrap_or(150.0)
}

pub fn set_graph_spacing_x(spacing: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("graph_spacing_x", spacing);
    }
}

pub fn graph_spacing_y() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("graph_spacing_y").unwrap_or(75.0)
}

pub fn set_graph_spacing_y(spacing: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("graph_spacing_y", spacing);
    }
}

/// The gap between grid cells, the companion to `graph_spacing_*` (which is the cell
/// itself). One node slot to the next is the two added up. Defaults match `ContentBg`'s
/// own, i.e. a quarter of the cell.
pub fn graph_gap_col_w() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("graph_gap_col_w").unwrap_or(37.5)
}

pub fn set_graph_gap_col_w(gap: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("graph_gap_col_w", gap);
    }
}

pub fn graph_gap_row_h() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("graph_gap_row_h").unwrap_or(37.5)
}

pub fn set_graph_gap_row_h(gap: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("graph_gap_row_h", gap);
    }
}

pub fn graph_grid_snap() -> bool {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("graph_grid_snap").unwrap_or(0.0) != 0.0
}

pub fn set_graph_grid_snap(snap: bool) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("graph_grid_snap", if snap { 1.0 } else { 0.0 });
    }
}

pub fn graph_blur() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("graph_blur").unwrap_or(0.0)
}

pub fn set_graph_blur(blur: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("graph_blur", blur);
    }
}

pub fn graph_node_corner_radius() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("graph_node_corner_radius").unwrap_or(4.0)
}

pub fn set_graph_node_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("graph_node_corner_radius", radius);
    }
}

pub fn graph_node_delete() -> String {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_string("graph_node_delete").unwrap_or_else(|| "delete".to_string())
}

pub fn set_graph_node_delete(key: String) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_string("graph_node_delete", key);
    }
}

pub fn graph_wire_size() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("graph_wire_size").unwrap_or(6.0)
}

pub fn set_graph_wire_size(size: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("graph_wire_size", size);
    }
}

pub fn graph_wire_activation_radius() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("graph_wire_activation_radius").unwrap_or(9.0)
}

pub fn set_graph_wire_activation_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("graph_wire_activation_radius", radius);
    }
}

pub fn graph_connector_size() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("graph_connector_size").unwrap_or(8.0)
}

pub fn set_graph_connector_size(size: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("graph_connector_size", size);
    }
}

pub fn graph_connector_activation_radius() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("graph_connector_activation_radius").unwrap_or(12.0)
}

pub fn set_graph_connector_activation_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("graph_connector_activation_radius", radius);
    }
}

pub fn font_selector_corner_radius() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("font_selector_corner_radius").unwrap_or(4.0)
}

pub fn set_font_selector_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("font_selector_corner_radius", radius);
    }
}

pub fn dropdown_corner_radius() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("dropdown_corner_radius").unwrap_or(4.0)
}

pub fn set_dropdown_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("dropdown_corner_radius", radius);
    }
}

pub fn color_selector_preview_margin() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("color_selector_preview_margin") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = COLOR_SELECTOR_PREVIEW_MARGIN.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *COLOR_SELECTOR_PREVIEW_MARGIN.read().unwrap()
}

pub fn set_color_selector_preview_margin(margin: f32) {
    if let Ok(mut lock) = COLOR_SELECTOR_PREVIEW_MARGIN.write() {
        *lock = margin;
    }
}



pub fn paginator_tab_padding_x() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("paginator_tab_padding_x") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = PAGINATOR_TAB_PADDING_X.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *PAGINATOR_TAB_PADDING_X.read().unwrap()
}

pub fn set_paginator_tab_padding_x(padding: f32) {
    if let Ok(mut lock) = PAGINATOR_TAB_PADDING_X.write() {
        *lock = padding;
    }
}

pub fn button_padding() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            let mut found = false;
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("button_padding") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = BUTTON_PADDING.write() {
                            *lock = val;
                            found = true;
                        }
                    }
                }
            }
            if !found {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if let Some(rest) = trimmed.strip_prefix("paginator_tab_padding_y") {
                        let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                        let val_str = rest.trim_end_matches('"').trim();
                        if let Ok(val) = val_str.parse::<f32>() {
                            if let Ok(mut lock) = BUTTON_PADDING.write() {
                                *lock = val;
                            }
                        }
                    }
                }
            }
        }
    });
    *BUTTON_PADDING.read().unwrap()
}

pub fn set_button_padding(padding: f32) {
    if let Ok(mut lock) = BUTTON_PADDING.write() {
        *lock = padding;
    }
}

pub fn button_height() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("button_height") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = BUTTON_HEIGHT.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *BUTTON_HEIGHT.read().unwrap()
}

pub fn ramp_height() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("ramp_height") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = RAMP_HEIGHT.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *RAMP_HEIGHT.read().unwrap()
}

pub fn set_ramp_height(height: f32) {
    if let Ok(mut lock) = RAMP_HEIGHT.write() {
        *lock = height;
    }
}

pub fn set_button_height(height: f32) {
    if let Ok(mut lock) = BUTTON_HEIGHT.write() {
        *lock = height;
    }
}

pub fn button_strip_spacing() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("button_strip_spacing") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = BUTTON_STRIP_SPACING.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *BUTTON_STRIP_SPACING.read().unwrap()
}

pub fn set_button_strip_spacing(spacing: f32) {
    if let Ok(mut lock) = BUTTON_STRIP_SPACING.write() {
        *lock = spacing;
    }
}

pub fn paginator_tab_padding_y() -> f32 {
    button_padding()
}

pub fn set_paginator_tab_padding_y(padding: f32) {
    set_button_padding(padding);
}

pub fn textbox_height() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("textbox_height") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = TEXTBOX_HEIGHT.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *TEXTBOX_HEIGHT.read().unwrap()
}

pub fn set_textbox_height(height: f32) {
    if let Ok(mut lock) = TEXTBOX_HEIGHT.write() {
        *lock = height;
    }
}

pub fn dropdown_height() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("dropdown_height") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = DROPDOWN_HEIGHT.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *DROPDOWN_HEIGHT.read().unwrap()
}

pub fn set_dropdown_height(height: f32) {
    if let Ok(mut lock) = DROPDOWN_HEIGHT.write() {
        *lock = height;
    }
}

pub fn slider_height() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("slider_height") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = SLIDER_HEIGHT.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *SLIDER_HEIGHT.read().unwrap()
}

pub fn set_slider_height(height: f32) {
    if let Ok(mut lock) = SLIDER_HEIGHT.write() {
        *lock = height;
    }
}

pub fn progressbar_height() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("progressbar_height") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = PROGRESSBAR_HEIGHT.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *PROGRESSBAR_HEIGHT.read().unwrap()
}

pub fn set_progressbar_height(height: f32) {
    if let Ok(mut lock) = PROGRESSBAR_HEIGHT.write() {
        *lock = height;
    }
}

pub fn rangeslider_height() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("rangeslider_height") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = RANGESLIDER_HEIGHT.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *RANGESLIDER_HEIGHT.read().unwrap()
}

pub fn set_rangeslider_height(height: f32) {
    if let Ok(mut lock) = RANGESLIDER_HEIGHT.write() {
        *lock = height;
    }
}


pub trait RenderTarget {
    fn rect(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32);
    fn rect_with_radius(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32, _radius: f32) {
        self.rect(color, x, y, w, h);
    }
    fn rect_with_radius_corners(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32, radius: f32, _corners: (bool, bool, bool, bool)) {
        self.rect_with_radius(color, x, y, w, h, radius);
    }
    fn text(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4]);
    fn text_with_font(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], _font: &str) {
        self.text(content, x, y, size, color);
    }
    fn text_with_bounds(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], _bounds: Option<[f32; 4]>) {
        self.text(content, x, y, size, color);
    }
    fn text_with_font_and_bounds(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], font: &str, _bounds: Option<[f32; 4]>) {
        self.text_with_font(content, x, y, size, color, font);
    }
    fn push_clip_rect(&mut self, _x: f32, _y: f32, _w: f32, _h: f32) {}
    fn pop_clip_rect(&mut self) {}
    /// Whether this host renders sections as sunken wells (the designer idiom).
    /// `SectionContext` then lays the title out left-aligned over its tab box
    /// instead of centered on the top border.
    fn section_relief_style(&self) -> bool {
        false
    }
    /// The section frame hatch: `SectionContext::finish` offers the frame here
    /// before falling back to the legacy 1px outline. A relief-capable host
    /// returns true and carves the section into its plate instead (the
    /// designer sunken-well idiom); the tuple hosts keep the default.
    fn section_relief(&mut self, _frame: &SectionFrame) -> bool {
        false
    }
}

/// A section frame offered to [`RenderTarget::section_relief`]: the content
/// body box plus, under [`RenderTarget::section_relief_style`], the title tab
/// box the label was laid out in — the tab sits flush on the body's top edge
/// (the designer union-carve shape).
pub struct SectionFrame {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub tab: Option<(f32, f32, f32, f32)>,
    pub focused: bool,
    pub is_child: bool,
}

pub struct PopoverCollector {
    pub rects: Vec<([f32; 4], f32, f32, f32, f32)>,
    pub texts: Vec<(String, f32, f32, f32, [f32; 4], Option<String>, Option<[f32; 4]>)>,
}

impl PopoverCollector {
    pub fn new() -> Self {
        Self { rects: Vec::new(), texts: Vec::new() }
    }
}

impl RenderTarget for PopoverCollector {
    fn rect(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32) {
        self.rects.push((color, x, y, w, h));
    }

    fn text(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4]) {
        self.texts.push((content.to_string(), size, x, y, color, None, None));
    }

    fn text_with_font(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], font: &str) {
        self.texts.push((content.to_string(), size, x, y, color, Some(font.to_string()), None));
    }

    fn text_with_bounds(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], bounds: Option<[f32; 4]>) {
        self.texts.push((content.to_string(), size, x, y, color, None, bounds));
    }

    fn text_with_font_and_bounds(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], font: &str, bounds: Option<[f32; 4]>) {
        self.texts.push((content.to_string(), size, x, y, color, Some(font.to_string()), bounds));
    }
}


pub fn render_widget<T: WidgetHost + 'static>(pc: &mut dyn RenderTarget, w: &mut T, x: f32, y: f32, ww: f32, wh: f32, ctx: &mut UiContext) {
    let id = Some(w.base().id());
    if let Some(w_id) = id {
        ctx.register_widget(w_id, w as *mut T as *mut (dyn WidgetHost + 'static));
    }
    w.layout(crate::widget::Point { x, y }, crate::widget::LayoutConstraints::new(ww, ww, wh, wh), ctx);
    let (style_r, corners) = w.corner_style();
    let r = if corners != (false, false, false, false) { style_r } else { 0.0 };
    let (wx, mut wy, www, mut whh) = w.rect();
    let top_room = crate::widget::label_offset(w);
    wy += top_room;
    whh -= top_room;
 
    for (qx, qy, qw, qh, qc) in w.all_quads(ctx) {
        let extra_corners = (
            corners.0 && qx <= wx + 1.5 && qy <= wy + 1.5,
            corners.1 && qx + qw >= wx + www - 1.5 && qy <= wy + 1.5,
            corners.2 && qx + qw >= wx + www - 1.5 && qy + qh >= wy + whh - 1.5,
            corners.3 && qx <= wx + 1.5 && qy + qh >= wy + whh - 1.5,
        );

        let (resolved_r, resolved_corners) = if r <= 0.1 || corners == (false, false, false, false) || extra_corners == (false, false, false, false) {
            (0.0, (false, false, false, false))
        } else {
            (r, extra_corners)
        };

        // Check if this quad is the background quad for a widget with a solid border
        let mut border_drawn = false;
        let is_bg_quad = (qx - wx).abs() < 0.1 && (qy - wy).abs() < 0.1 && (qw - www).abs() < 0.1 && (qh - whh).abs() < 0.1;
        if is_bg_quad {
            if let Some((border_color, thickness)) = w.solid_border() {
                if thickness > 0.0 {
                    // Draw full size border quad
                    pc.rect_with_radius_corners(border_color, qx, qy, qw, qh, resolved_r, resolved_corners);
                    // Draw inset background quad on top
                    pc.rect_with_radius_corners(
                        qc,
                        qx + thickness,
                        qy + thickness,
                        (qw - 2.0 * thickness).max(0.0),
                        (qh - 2.0 * thickness).max(0.0),
                        (resolved_r - thickness).max(0.0),
                        resolved_corners,
                    );
                    border_drawn = true;
                }
            }
        }

        if !border_drawn {
            pc.rect_with_radius_corners(qc, qx, qy, qw, qh, resolved_r, resolved_corners);
        }
    }
    for (qx, qy, qw, qh, qr, qc, qcorners) in w.all_rounded_quads(ctx) {
        pc.rect_with_radius_corners(qc, qx, qy, qw, qh, qr, qcorners);
    }
    // Text via the paint walk: `paint_self` emits each widget's Text prims (content font +
    // scroll-ancestor clip) exactly as the live display-list render does. render_widget already
    // drew the geometry via `all_quads`/`all_rounded_quads` above, so we take only the Text prims
    // from the walk. This drops the legacy `widget_font` + `text_labels_with_font_and_bounds`
    // getters from render_widget — the prim already carries the per-widget font+bounds.
    let mut text_scratch = crate::scene::paint::PaintCtx::new();
    crate::scene::painter::paint_root_into(&*ctx, &*w, &mut text_scratch);
    for item in text_scratch.finish().items {
        if let crate::scene::paint::Prim::Text { text, x, y, font_size, color, font, bounds, .. } = item.prim {
            let color_f32 = [
                color[0] as f32 / 255.0,
                color[1] as f32 / 255.0,
                color[2] as f32 / 255.0,
                1.0,
            ];
            // Compose the walk's container clip with the prim's own bounds (the engine's dl-text
            // merge), so a clipping ancestor still bounds the text.
            let clip = item.clip.map(|c| [c.x, c.y, c.x + c.width, c.y + c.height]);
            let merged = match (clip, bounds) {
                (Some(a), Some(b)) => Some([a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])]),
                (Some(a), None) => Some(a),
                (None, b) => b,
            };
            match font {
                Some(ref f) => pc.text_with_font_and_bounds(&text, x, y, font_size, color_f32, f, merged),
                None => pc.text_with_bounds(&text, x, y, font_size, color_f32, merged),
            }
        }
    }
    if w.popover_rect().is_some() {
        ctx.register_popover(w);
    }
}

pub fn render_popovers(pc: &mut dyn RenderTarget, ctx: &UiContext) {
    for &pop_id in &ctx.active_popovers {
        if let Some(ptr) = ctx.tree.get_ptr(pop_id) {
            unsafe {
                (*ptr).render_popover(pc);
            }
        }
    }
}

pub fn partition_concentric_corners(
    x: f32, y: f32, w: f32, h: f32,
    _r_std: f32,
    r_adjust: [f32; 4],
    color: [f32; 4],
) -> Vec<(f32, f32, f32, f32, f32, [f32; 4], (bool, bool, bool, bool))> {
    let mut quads = Vec::new();

    let r0 = r_adjust[0];
    let r1 = r_adjust[1];
    let r2 = r_adjust[2];
    let r3 = r_adjust[3];

    let max_left = r0.max(r3);
    let max_right = r1.max(r2);

    let mut push_valid_quad = |qx: f32, qy: f32, qw: f32, qh: f32, qr: f32, qcorners: (bool, bool, bool, bool)| {
        if qw > 0.001 && qh > 0.001 {
            quads.push((qx, qy, qw, qh, qr, color, qcorners));
        }
    };

    // 1. Center vertical block
    push_valid_quad(x + max_left, y, w - max_left - max_right, h, 0.0, (false, false, false, false));

    // 2. Left block
    push_valid_quad(x, y + r0, max_left, h - r0 - r3, 0.0, (false, false, false, false));

    // 3. Top-left transition
    push_valid_quad(x + r0, y, max_left - r0, r0, 0.0, (false, false, false, false));

    // 4. Bottom-left transition
    push_valid_quad(x + r3, y + h - r3, max_left - r3, r3, 0.0, (false, false, false, false));

    // 5. Right block
    push_valid_quad(x + w - max_right, y + r1, max_right, h - r1 - r2, 0.0, (false, false, false, false));

    // 6. Top-right transition
    push_valid_quad(x + w - max_right, y, max_right - r1, r1, 0.0, (false, false, false, false));

    // 7. Bottom-right transition
    push_valid_quad(x + w - max_right, y + h - r2, max_right - r2, r2, 0.0, (false, false, false, false));

    // 8. Corner 0 (top-left)
    push_valid_quad(x, y, r0, r0, r0, (true, false, false, false));

    // 9. Corner 1 (top-right)
    push_valid_quad(x + w - r1, y, r1, r1, r1, (false, true, false, false));

    // 10. Corner 2 (bottom-right)
    push_valid_quad(x + w - r2, y + h - r2, r2, r2, r2, (false, false, true, false));

    // 11. Corner 3 (bottom-left)
    push_valid_quad(x, y + h - r3, r3, r3, r3, (false, false, false, true));

    quads
}

pub struct UiFrame;

impl UiFrame {
    pub fn start(scroll_offset: f32) -> Self {
        crate::widget::hover_animation::reset_frame_registration();
        crate::widget::hover_animation::set_scroll_offset(scroll_offset);
        Self
    }

    pub fn finish(self, pc: &mut dyn RenderTarget) {
        crate::widget::hover_animation::post_render_check();
        if let Some((qx, qy, qw, qh, qc)) = crate::widget::hover_animation::get_quad() {
            pc.rect(qc, qx, qy, qw, qh);
        }
        // render_popovers(pc);
    }
}

pub struct Column {
    ox: f32,
    oy: f32,
    cx: f32,
    pub y: f32,
    pub cw: f32,
}

impl Column {
    pub fn new(ox: f32, oy: f32, cx: f32, cy: f32, cw: f32) -> Self {
        Self { ox, oy, cx, y: cy, cw }
    }

    pub fn ax(&self, x_off: f32) -> f32 {
        self.ox + self.cx + x_off
    }

    pub fn ay(&self) -> f32 {
        self.oy + self.y
    }

    pub fn rect(&mut self, pc: &mut dyn RenderTarget, color: [f32; 4], x_off: f32, w: f32, h: f32) {
        pc.rect(color, self.ax(x_off), self.ay(), w, h);
        self.y += h;
    }

    pub fn advance(&mut self, dy: f32) {
        self.y += dy;
    }

    pub fn spacing(&mut self, dy: f32) {
        self.y += dy;
    }

    pub fn separator(&mut self, pc: &mut dyn RenderTarget) {
        let x = self.ax(8.0);
        let y = self.ay();
        pc.rect([0.18, 0.18, 0.27, 1.0], x, y, self.cw - 16.0, 1.0);
        self.y += 8.0;
    }

    pub fn header(&mut self, pc: &mut dyn RenderTarget, text: &str, x_off: f32) {
        let x = self.ax(x_off);
        let y = self.ay();
        pc.text(text, x, y, 14.0, [0.83, 0.83, 0.83, 1.0]);
        self.y += 22.0;
    }

    pub fn text(&mut self, pc: &mut dyn RenderTarget, text: &str, x_off: f32, y_off: f32, font_size: f32, color: [f32; 4]) {
        let x = self.ax(x_off);
        let y = self.ay() + y_off;
        pc.text(text, x, y, font_size, color);
    }

    pub fn widget<T: WidgetHost + 'static>(&mut self, pc: &mut dyn RenderTarget, w: &mut T, x_off: f32, ww: f32, mut wh: f32, ctx: &mut UiContext) {
        if let Some(pref) = w.preferred_height() {
            wh = pref;
        }
        let top_room = crate::widget::label_offset(w);
        let total_h = wh + top_room;
        let x = self.ax(x_off);
        let y = self.ay();
        w.set_row_rect(self.ox + self.cx + 8.0, self.cw - 16.0);
        let clamped_w = ww.min((self.cw - x_off).max(0.0));
        render_widget(pc, w, x, y, clamped_w, total_h, ctx);
        self.y += total_h;
    }

    pub fn row<F: FnOnce(&mut Row)>(&mut self, pc: &mut dyn RenderTarget, h: f32, f: F) {
        let row_y = self.ay();
        let mut row = Row {
            pc: &mut *pc,
            base_x: self.ox + self.cx,
            y: row_y,
            cursor_x: 0.0,
            spacing: 8.0,
        };
        f(&mut row);
        self.y = self.y + h;
    }
}

pub struct Row<'a> {
    pc: &'a mut dyn RenderTarget,
    base_x: f32,
    y: f32,
    pub cursor_x: f32,
    pub spacing: f32,
}

impl<'a> Row<'a> {
    pub fn set_spacing(&mut self, spacing: f32) {
        self.spacing = spacing;
    }

    pub fn gap(&mut self, width: f32) {
        self.cursor_x += width;
    }

    pub fn text(&mut self, text: &str, y_off: f32, font_size: f32, color: [f32; 4], width: f32) {
        self.pc
            .text(text, self.base_x + self.cursor_x, self.y + y_off, font_size, color);
        self.cursor_x += width + self.spacing;
    }

    pub fn widget<T: WidgetHost + 'static>(&mut self, w: &mut T, ww: f32, mut wh: f32, ctx: &mut UiContext) {
        if let Some(pref) = w.preferred_height() {
            wh = pref;
        }
        render_widget(self.pc, w, self.base_x + self.cursor_x, self.y, ww, wh, ctx);
        self.cursor_x += ww + self.spacing;
    }
}

fn estimate_label_width_helper(label: &str, font_size: f32, font_fam: &str) -> f32 {
    let fam_lower = font_fam.to_lowercase();
    let is_mono = fam_lower.contains("mono") || fam_lower.contains("courier") || fam_lower == "monospace";
    if is_mono {
        label.chars().count() as f32 * font_size * 0.60
    } else {
        let mut width = 0.0;
        for c in label.chars() {
            let factor = match c {
                'i' | 'l' | 'I' | ' ' | '.' | ',' | '!' | ';' | ':' | '\'' | '"' | '(' | ')' | '[' | ']' | '-' => 0.30,
                'f' | 'j' | 't' => 0.35,
                'r' | 's' | 'c' | 'z' => 0.50,
                'a' | 'b' | 'd' | 'e' | 'g' | 'h' | 'k' | 'n' | 'o' | 'p' | 'q' | 'u' | 'v' | 'x' | 'y' => 0.60,
                'm' | 'w' | 'M' | 'W' | '&' | '@' | 'O' | 'Q' | 'G' => 0.85,
                'A' | 'B' | 'C' | 'D' | 'H' | 'N' | 'U' | 'V' | 'X' | 'Y' => 0.75,
                'E' | 'F' | 'K' | 'L' | 'P' | 'R' | 'S' | 'T' | 'Z' | 'J' => 0.68,
                '0'..='9' => 0.60,
                _ => 0.60,
            };
            width += factor * font_size;
        }
        width
    }
}

pub struct Section {
    pub left: f32,
    pub top: f32,
    pub content_y: f32,
    pub cw: f32,
    pub label_width: f32,
    pub is_child: bool,
    pub grid: Grid,
    pub last_col: usize,
}

impl Section {
    pub const DEFAULT_MARGIN_X: f32 = 12.0;
    pub const DEFAULT_ROW_GAP: f32 = 8.0;

    fn estimate_label_width(label: &str, font_size: f32, font_fam: &str) -> f32 {
        estimate_label_width_helper(label, font_size, font_fam)
    }

    pub fn padding(&self) -> f32 {
        section_padding()
    }

    pub fn new(pc: &mut dyn RenderTarget, left: f32, top: f32, cw: f32, label: &str) -> Self {
        Self::new_opt(pc, left, top, cw, label, false)
    }

    pub fn new_opt(pc: &mut dyn RenderTarget, left: f32, top: f32, cw: f32, label: &str, is_child: bool) -> Self {
        let font_setting = if is_child {
            nested_section_label_font()
        } else {
            section_label_font()
        };
        let (font_fam, font_size_opt) = parse_font_string(&font_setting);
        let font_size = font_size_opt.unwrap_or(if is_child { 12.0 } else { 14.0 });
        let font_color = if is_child { [0.53, 0.53, 0.60, 1.0] } else { [0.83, 0.83, 0.83, 1.0] };
        let label_width = Self::estimate_label_width(label, font_size, &font_fam);
        let label_x = if is_child {
            let base_x = match nested_section_label_alignment() {
                0 => left + 12.0,
                1 => left + (cw - label_width) / 2.0,
                2 => left + cw - 12.0 - label_width,
                _ => left + 12.0,
            };
            base_x + nested_section_label_offset()
        } else {
            left + (cw - label_width) / 2.0
        };
        pc.text_with_font(label, label_x, top, font_size, font_color, &font_fam);

        let pad = section_padding();
        let margin_x = 2.0 * pad + 12.0;
        let usable_w = (cw - 2.0 * margin_x).max(1.0);
        let min_col_width = 130.0;
        let gap = 8.0;
        let max_cols = if is_child {
            1
        } else {
            ((usable_w + gap) / (min_col_width + gap)).floor().max(1.0).min(2.0) as usize
        };
        let content_start_y = top + pad + 19.0;
        let grid = Grid::new(left + margin_x, content_start_y, usable_w, min_col_width, gap, max_cols);

        Self { left, top, content_y: content_start_y, cw, label_width, is_child, grid, last_col: usize::MAX }
    }

    pub fn ax(&self, x_off: f32) -> f32 {
        let shift = if x_off >= 12.0 { self.padding() } else { 0.0 };
        self.left + x_off + shift
    }

    pub fn ay(&self) -> f32 { self.content_y }

    pub fn spacing(&mut self, dy: f32) {
        if self.grid.col_heights.len() >= 2 {
            if self.last_col == usize::MAX {
                for h in &mut self.grid.col_heights {
                    *h += dy;
                }
            } else if self.last_col < self.grid.col_heights.len() {
                self.grid.col_heights[self.last_col] += dy;
            }
            self.content_y = self.grid.max_height();
        } else {
            self.content_y += dy;
            for h in &mut self.grid.col_heights {
                *h += dy;
            }
        }
    }

    pub fn text(&mut self, pc: &mut dyn RenderTarget, text: &str, x_off: f32, y_off: f32, font_size: f32, color: [f32; 4]) {
        pc.text(text, self.ax(x_off), self.ay() + y_off, font_size, color);
    }

    pub fn widget<T: WidgetHost + 'static>(&mut self, pc: &mut dyn RenderTarget, w: &mut T, _x_off: f32, _ww: f32, mut wh: f32, ctx: &mut UiContext) {
        if let Some(pref) = w.preferred_height() {
            wh = pref;
        }
        let pad = self.padding();
        let top_room = crate::widget::label_offset(w);
        let total_h = wh + top_room;

        let name = w.type_name();
        let span_full = name == "Trackpad"
            || name == "Canvas"
            || name == "UsageBar"
            || name == "ProgressBar"
            || name == "ButtonStrip"
            || name == "Spreadsheet"
            || name == "Graph";

        if span_full {
            let margin_x = 2.0 * pad + 12.0;
            let x = self.left + margin_x;
            let clamped_w = (self.cw - 2.0 * margin_x).max(0.0);
            let max_h = self.grid.max_height().max(self.content_y);
            let y = max_h;

            w.set_row_rect(self.left + pad, self.cw - 2.0 * pad);
            render_widget(pc, w, x, y, clamped_w, total_h, ctx);

            let new_bottom = y + total_h;
            self.content_y = new_bottom;
            for h in &mut self.grid.col_heights {
                *h = new_bottom;
            }
        } else {
            let max_h = self.grid.max_height();
            if self.content_y > max_h {
                for h in &mut self.grid.col_heights {
                    *h = self.content_y;
                }
            }

            let col = self.grid.next_column();
            self.last_col = col;
            let x = self.grid.col_lefts[col];
            let y = self.grid.col_heights[col];

            w.set_row_rect(x, self.grid.col_width);
            render_widget(pc, w, x, y, self.grid.col_width, total_h, ctx);
            self.grid.col_heights[col] += total_h;
            self.content_y = self.grid.max_height();
        }
    }

    pub fn widget_full<T: WidgetHost + 'static>(&mut self, pc: &mut dyn RenderTarget, w: &mut T, wh: f32, ctx: &mut UiContext) {
        let x_off = 12.0;
        let ww = self.cw - 2.0 * (self.padding() + x_off);
        self.widget(pc, w, x_off, ww, wh, ctx);
    }

    pub fn separator(&mut self, pc: &mut dyn RenderTarget) {
        let pad = self.padding();
        let x = self.ax(pad);
        let max_h = self.grid.max_height().max(self.content_y);
        let y = max_h;
        pc.rect([0.18, 0.18, 0.27, 1.0], x, y, self.cw - 2.0 * pad, 1.0);
        self.content_y = max_h + 8.0;
        for h in &mut self.grid.col_heights {
            *h = self.content_y;
        }
    }

    pub fn rect(&mut self, pc: &mut dyn RenderTarget, color: [f32; 4], x_off: f32, w: f32, h: f32) {
        let max_h = self.grid.max_height().max(self.content_y);
        pc.rect(color, self.ax(x_off), max_h, w, h);
        self.content_y = max_h + h;
        for col_h in &mut self.grid.col_heights {
            *col_h = self.content_y;
        }
    }

    pub fn row_layout(&self, count: usize, gap: f32) -> Vec<(f32, f32)> {
        let margin_x = 2.0 * self.padding() + 12.0;
        let usable_w = self.cw - 2.0 * margin_x;
        if count == 0 {
            return Vec::new();
        }
        let total_gap = gap * (count - 1) as f32;
        let col_w = (usable_w - total_gap).max(0.0) / count as f32;

        let mut cols = Vec::with_capacity(count);
        for i in 0..count {
            let x = self.left + margin_x + i as f32 * (col_w + gap);
            cols.push((x, col_w));
        }
        cols
    }

    pub fn row<F>(&mut self, count: usize, gap: f32, h: f32, mut f: F)
    where
        F: FnMut(usize, f32, f32),
    {
        let max_h = self.grid.max_height().max(self.content_y);
        for col_h in &mut self.grid.col_heights {
            *col_h = max_h;
        }
        self.content_y = max_h;

        let cols = self.row_layout(count, gap);
        for (i, &(x, w)) in cols.iter().enumerate() {
            f(i, x, w);
        }
        self.content_y += h;

        for col_h in &mut self.grid.col_heights {
            *col_h = self.content_y;
        }
    }

    pub fn finish(&mut self, pc: &mut dyn RenderTarget) -> f32 {
        self.finish_focused(pc, false)
    }

    pub fn finish_focused(&mut self, pc: &mut dyn RenderTarget, focused: bool) -> f32 {
        let border: [f32; 4] = if self.is_child {
            if focused {
                [0.22, 0.38, 0.24, 1.0]
            } else {
                [0.18, 0.18, 0.25, 1.0]
            }
        } else {
            if focused {
                [0.30, 0.50, 0.32, 1.0]
            } else {
                [0.25, 0.25, 0.35, 1.0]
            }
        };
        let pad = self.padding();
        let x = self.left + pad;
        let y = self.top + 7.0;
        let w = self.cw - 2.0 * pad;
        let h = self.content_y - y;
        
        let left_edge = x;
        let right_edge = x + w;
        if self.label_width > 0.0 {
            let label_x = if self.is_child {
                let base_x = match nested_section_label_alignment() {
                    0 => self.left + 12.0,
                    1 => self.left + (self.cw - self.label_width) / 2.0,
                    2 => self.left + self.cw - 12.0 - self.label_width,
                    _ => self.left + 12.0,
                };
                base_x + nested_section_label_offset()
            } else {
                self.left + (self.cw - self.label_width) / 2.0
            };
            let gap_margin = 6.0;
            let gap_start = label_x - gap_margin;
            let gap_end = label_x + self.label_width + gap_margin;
            if gap_start > left_edge {
                pc.rect(border, left_edge, y, gap_start - left_edge, 1.0);
            }
            if right_edge > gap_end {
                pc.rect(border, gap_end, y, right_edge - gap_end, 1.0);
            }
        } else {
            pc.rect(border, left_edge, y, w, 1.0);
        }

        let extra_bottom = pad + 12.0;
        pc.rect(border, x, y + h + extra_bottom, w, 1.0);
        pc.rect(border, x, y, 1.0, h + extra_bottom);
        pc.rect(border, x + w - 1.0, y, 1.0, h + extra_bottom);
        self.content_y + extra_bottom + 8.0
    }

    pub fn vstack<'a>(&'a mut self, pc: &'a mut dyn RenderTarget, spacing: f32) -> SectionVStack<'a> {
        SectionVStack {
            section: self,
            pc,
            spacing,
        }
    }
}

pub struct SectionVStack<'a> {
    section: &'a mut Section,
    pc: &'a mut dyn RenderTarget,
    spacing: f32,
}

impl<'a> SectionVStack<'a> {
    pub fn add_widget<T: WidgetHost + 'static>(&mut self, w: &mut T, ww: f32, wh: f32, ctx: &mut UiContext) {
        self.section.widget(self.pc, w, Section::DEFAULT_MARGIN_X, ww, wh, ctx);
        self.section.spacing(self.spacing);
    }

    pub fn add_row<F>(&mut self, count: usize, gap: f32, h: f32, f: F)
    where
        F: FnMut(usize, f32, f32),
    {
        self.section.row(count, gap, h, f);
        self.section.spacing(self.spacing);
    }
}


pub struct SplitterLayout {
    pub splitter1_x: f32,
    pub splitter2_x: f32,
    pub splitter_width: f32,
    pub min_column_width: f32,
}

impl SplitterLayout {
    pub fn new(width: f32, splitter_width: f32, min_column_width: f32) -> Self {
        let s1 = (width - 2.0 * splitter_width) / 3.0;
        let s2 = s1 + splitter_width + (width - 2.0 * splitter_width) / 3.0;
        Self {
            splitter1_x: s1,
            splitter2_x: s2,
            splitter_width,
            min_column_width,
        }
    }

    pub fn clamp(&mut self, total_width: f32, detached_circular_network: bool) {
        if detached_circular_network {
            let min_s2 = self.min_column_width;
            let max_s2 = (total_width - self.min_column_width).max(min_s2);
            self.splitter2_x = self.splitter2_x.clamp(min_s2, max_s2);
        } else {
            let min_s1 = self.min_column_width;
            let max_s1 = (self.splitter2_x - self.splitter_width - self.min_column_width).max(min_s1);
            self.splitter1_x = self.splitter1_x.clamp(min_s1, max_s1);
            let min_s2 = self.splitter1_x + self.splitter_width + self.min_column_width;
            let max_s2 = (total_width - self.min_column_width).max(min_s2);
            self.splitter2_x = self.splitter2_x.clamp(min_s2, max_s2);
        }
    }

    pub fn scale(&mut self, factor: f32) {
        self.splitter1_x *= factor;
        self.splitter2_x *= factor;
    }

    pub fn left_col(&self) -> (f32, f32) { // (x, width)
        (0.0, self.splitter1_x)
    }

    pub fn center_col(&self) -> (f32, f32) { // (x, width)
        let x = self.splitter1_x + self.splitter_width;
        (x, self.splitter2_x - x)
    }

    pub fn right_col(&self, total_width: f32) -> (f32, f32) { // (x, width)
        let x = self.splitter2_x + self.splitter_width;
        (x, (total_width - x).max(0.0))
    }
}

#[derive(Debug, Clone)]
pub struct Grid {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub col_width: f32,
    pub gap: f32,
    pub col_heights: Vec<f32>,
    pub col_lefts: Vec<f32>,
}

impl Grid {
    pub fn new(left: f32, top: f32, width: f32, min_col_width: f32, gap: f32, count: usize) -> Self {
        let total_gap = gap * (count - 1) as f32;
        let col_width = if count > 0 {
            (width - total_gap).max(0.0) / count as f32
        } else {
            min_col_width
        };
        let left_offset = 0.0;

        let mut col_lefts = Vec::with_capacity(count);
        let col_heights = vec![top; count];
        for i in 0..count {
            col_lefts.push(left + left_offset + i as f32 * (col_width + gap));
        }

        Self {
            left,
            top,
            width,
            col_width,
            gap,
            col_heights,
            col_lefts,
        }
    }

    pub fn next_column(&self) -> usize {
        let mut min_idx = 0;
        let mut min_h = self.col_heights[0];
        for i in 1..self.col_heights.len() {
            if self.col_heights[i] < min_h {
                min_h = self.col_heights[i];
                min_idx = i;
            }
        }
        min_idx
    }

    pub fn max_height(&self) -> f32 {
        let mut max_h = self.col_heights[0];
        for i in 1..self.col_heights.len() {
            if self.col_heights[i] > max_h {
                max_h = self.col_heights[i];
            }
        }
        max_h
    }
}

pub struct CircularPaneLayout {
    pub x: f32,
    pub y: f32,
    pub r: f32,
}

impl CircularPaneLayout {
    pub fn new(x: f32, y: f32, r: f32) -> Self {
        Self { x, y, r }
    }

    pub fn hit_test_content(&self, cx: f32, cy: f32, menubar_h: f32, breadcrumb_h: f32) -> bool {
        let dx = cx - self.x;
        let dy = cy - self.y;
        let dist_sq = dx * dx + dy * dy;
        dist_sq <= self.r * self.r && cy >= self.y - self.r + 45.0 + menubar_h + breadcrumb_h
    }

    pub fn hit_test_menubar(&self, cx: f32, cy: f32, menubar_h: f32) -> bool {
        let dx = cx - self.x;
        let dy = cy - self.y;
        let dist = (dx * dx + dy * dy).sqrt();
        dist >= self.r - menubar_h && dist <= self.r && cy < self.y
    }

    pub fn hit_test_breadcrumb(&self, cx: f32, cy: f32, menubar_h: f32, breadcrumb_h: f32) -> bool {
        let dx = cx - self.x;
        let dy = cy - self.y;
        let dist_sq = dx * dx + dy * dy;
        dist_sq <= self.r * self.r && cy >= self.y - self.r + menubar_h && cy < self.y - self.r + 45.0 + menubar_h + breadcrumb_h
    }

    pub fn hit_test_border(&self, cx: f32, cy: f32, border_thickness: f32) -> bool {
        let dx = cx - self.x;
        let dy = cy - self.y;
        let dist = (dx * dx + dy * dy).sqrt();
        dist >= self.r - border_thickness && dist <= self.r
    }
}

#[derive(Debug, Clone)]
pub struct Radial {
    pub center_x: f32,
    pub center_y: f32,
    pub aspect_ratio: f32,
    pub base_spacing: f32,
}

impl Radial {
    pub fn new(center_x: f32, center_y: f32, aspect_ratio: f32, base_spacing: f32) -> Self {
        Self { center_x, center_y, aspect_ratio, base_spacing }
    }

    pub fn widget_rect(&self, idx: usize, ww: f32, wh: f32) -> (f32, f32, f32, f32) {
        if idx == 0 {
            (self.center_x - ww / 2.0, self.center_y - wh / 2.0, ww, wh)
        } else {
            let mut ring = 1;
            let mut ring_start = 1;
            loop {
                let ring_capacity = ring * 6;
                if idx < ring_start + ring_capacity {
                    let pos_in_ring = idx - ring_start;
                    let angle = (pos_in_ring as f32) * (2.0 * std::f32::consts::PI / ring_capacity as f32);
                    let radius = (ring as f32) * self.base_spacing;

                    let x_offset = radius * angle.cos() * self.aspect_ratio;
                    let y_offset = radius * angle.sin();

                    return (
                        self.center_x + x_offset - ww / 2.0,
                        self.center_y + y_offset - wh / 2.0,
                        ww,
                        wh,
                    );
                }
                ring_start += ring_capacity;
                ring += 1;
            }
        }
    }

}



pub trait LayoutStrategy: std::fmt::Debug {
    fn init(&mut self, left: f32, top: f32, width: f32, height: f32);
    fn allocate(&mut self, ww: f32, wh: f32) -> (f32, f32, f32, f32);
    fn set_section_count(&mut self, _count: usize) {}
    fn get_column_width(&self) -> Option<f32> { None }
    fn get_gap(&self) -> f32 { 20.0 }

    fn layout(&self, x: f32, y: f32, w: f32, h: f32, children: &[*mut (dyn crate::widget::WidgetHost + 'static)], ctx: &mut crate::context::UiContext) -> f32;
    fn measure(&self, constraints: crate::widget::LayoutConstraints, children: &[*mut (dyn crate::widget::WidgetHost + 'static)], ctx: &crate::context::UiContext) -> crate::widget::Size;
    fn box_clone(&self) -> Box<dyn LayoutStrategy>;
}

impl Clone for Box<dyn LayoutStrategy> {
    fn clone(&self) -> Self {
        self.box_clone()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexDirection {
    Row,
    Column,
}

#[derive(Debug, Clone)]
pub struct FlexLayout {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    direction: FlexDirection,
    spacing: f32,
    current_x: f32,
    current_y: f32,
}

impl FlexLayout {
    pub fn new(direction: FlexDirection, spacing: f32) -> Self {
        Self {
            left: 0.0,
            top: 0.0,
            width: 0.0,
            height: 0.0,
            direction,
            spacing,
            current_x: 0.0,
            current_y: 0.0,
        }
    }
}

impl LayoutStrategy for FlexLayout {
    fn init(&mut self, left: f32, top: f32, width: f32, height: f32) {
        self.left = left;
        self.top = top;
        self.width = width;
        self.height = height;
        self.current_x = left;
        self.current_y = top;
    }

    fn allocate(&mut self, ww: f32, wh: f32) -> (f32, f32, f32, f32) {
        match self.direction {
            FlexDirection::Row => {
                let rx = self.current_x;
                let ry = self.current_y;
                self.current_x += ww + self.spacing;
                (rx, ry, ww, wh)
            }
            FlexDirection::Column => {
                let rx = self.current_x;
                let ry = self.current_y;
                self.current_y += wh + self.spacing;
                (rx, ry, ww, wh)
            }
        }
    }

    fn get_gap(&self) -> f32 {
        self.spacing
    }

    fn layout(&self, x: f32, y: f32, w: f32, h: f32, children: &[*mut (dyn crate::widget::WidgetHost + 'static)], _ctx: &mut crate::context::UiContext) -> f32 {
        let mut cur_x = x;
        let mut cur_y = y;
        match self.direction {
            FlexDirection::Row => {
                for &child_ptr in children {
                    unsafe {
                        let child = &mut *child_ptr;
                        let child_w = child.rect().2;
                        let child_h = child.preferred_height().unwrap_or(child.rect().3);
                        let use_h = if child_h > 0.0 { child_h } else { h };
                        child.set_rect(cur_x, cur_y, child_w, use_h);
                        cur_x += child_w + self.spacing;
                    }
                }
                (cur_x - x).max(0.0)
            }
            FlexDirection::Column => {
                for &child_ptr in children {
                    unsafe {
                        let child = &mut *child_ptr;
                        let child_h = child.preferred_height().unwrap_or(child.rect().3);
                        let use_h = if child_h > 0.0 { child_h } else { 44.0 };
                        child.set_rect(x, cur_y, w, use_h);
                        cur_y += use_h + self.spacing;
                    }
                }
                (cur_y - y).max(0.0)
            }
        }
    }

    fn measure(&self, constraints: crate::widget::LayoutConstraints, children: &[*mut (dyn crate::widget::WidgetHost + 'static)], ctx: &crate::context::UiContext) -> crate::widget::Size {
        match self.direction {
            FlexDirection::Row => {
                let mut total_w = 0.0f32;
                let mut max_h = 0.0f32;
                for (i, &child_ptr) in children.iter().enumerate() {
                    unsafe {
                        let size = (*child_ptr).measure(constraints, ctx);
                        total_w += size.width;
                        max_h = max_h.max(size.height);
                        if i > 0 {
                            total_w += self.spacing;
                        }
                    }
                }
                crate::widget::Size {
                    width: total_w.clamp(constraints.min_width, constraints.max_width),
                    height: max_h.clamp(constraints.min_height, constraints.max_height),
                }
            }
            FlexDirection::Column => {
                let mut total_h = 0.0f32;
                let mut max_w = 0.0f32;
                for (i, &child_ptr) in children.iter().enumerate() {
                    unsafe {
                        let size = (*child_ptr).measure(constraints, ctx);
                        total_h += size.height;
                        max_w = max_w.max(size.width);
                        if i > 0 {
                            total_h += self.spacing;
                        }
                    }
                }
                crate::widget::Size {
                    width: max_w.clamp(constraints.min_width, constraints.max_width),
                    height: total_h.clamp(constraints.min_height, constraints.max_height),
                }
            }
        }
    }

    fn box_clone(&self) -> Box<dyn LayoutStrategy> {
        Box::new(self.clone())
    }
}

#[derive(Debug, Clone)]
pub struct ColumnLayout {
    left: f32,
    top: f32,
    width: f32,
    current_y: f32,
    gap: f32,
}

impl ColumnLayout {
    pub fn new(gap: f32) -> Self {
        Self {
            left: 0.0,
            top: 0.0,
            width: 0.0,
            current_y: 0.0,
            gap,
        }
    }
}

impl LayoutStrategy for ColumnLayout {
    fn init(&mut self, left: f32, top: f32, width: f32, _height: f32) {
        self.left = left;
        self.top = top;
        self.width = width;
        self.current_y = top;
    }

    fn allocate(&mut self, ww: f32, wh: f32) -> (f32, f32, f32, f32) {
        let x = self.left;
        let y = self.current_y;
        self.current_y += wh + self.gap;
        (x, y, ww, wh)
    }

    fn get_column_width(&self) -> Option<f32> {
        Some(self.width)
    }

    fn get_gap(&self) -> f32 {
        self.gap
    }

    fn layout(&self, x: f32, y: f32, w: f32, _h: f32, children: &[*mut (dyn crate::widget::WidgetHost + 'static)], _ctx: &mut crate::context::UiContext) -> f32 {
        let mut cur_y = y;
        for &child_ptr in children {
            unsafe {
                let child = &mut *child_ptr;
                let child_h = child.preferred_height().unwrap_or(child.rect().3);
                let use_h = if child_h > 0.0 { child_h } else { 44.0 };
                child.set_rect(x, cur_y, w, use_h);
                cur_y += use_h + self.gap;
            }
        }
        (cur_y - y).max(0.0)
    }

    fn measure(&self, constraints: crate::widget::LayoutConstraints, children: &[*mut (dyn crate::widget::WidgetHost + 'static)], ctx: &crate::context::UiContext) -> crate::widget::Size {
        let mut total_h = 0.0f32;
        let mut max_w = 0.0f32;
        for (i, &child_ptr) in children.iter().enumerate() {
            unsafe {
                let size = (*child_ptr).measure(constraints, ctx);
                total_h += size.height;
                max_w = max_w.max(size.width);
                if i > 0 {
                    total_h += self.gap;
                }
            }
        }
        crate::widget::Size {
            width: max_w.clamp(constraints.min_width, constraints.max_width),
            height: total_h.clamp(constraints.min_height, constraints.max_height),
        }
    }

    fn box_clone(&self) -> Box<dyn LayoutStrategy> {
        Box::new(self.clone())
    }
}

#[derive(Debug, Clone)]
pub struct AdaptiveGrid {
    grid: Option<Grid>,
    #[allow(dead_code)]
    min_col_width: f32,
    #[allow(dead_code)]
    gap: f32,
    num_sections: Option<usize>,
}

impl AdaptiveGrid {
    pub fn new(min_col_width: f32, gap: f32) -> Self {
        Self {
            grid: None,
            min_col_width,
            gap,
            num_sections: None,
        }
    }
}

impl LayoutStrategy for AdaptiveGrid {
    fn init(&mut self, left: f32, top: f32, width: f32, _height: f32) {
        let min_col_width = crate::layout::grid_min_col_width();
        let gap = crate::layout::grid_gap();
        let max_cols = ((width + gap) / (min_col_width + gap)).floor().max(1.0) as usize;
        let count = if let Some(n) = self.num_sections {
            n.min(max_cols).max(1)
        } else {
            max_cols
        };
        self.grid = Some(Grid::new(left, top, width, min_col_width, gap, count));
    }

    fn allocate(&mut self, ww: f32, wh: f32) -> (f32, f32, f32, f32) {
        if let Some(ref mut grid) = self.grid {
            let num_cols = grid.col_heights.len();
            let num_cols_spanned = (((ww + grid.gap) / (grid.col_width + grid.gap)).round() as usize)
                .min(num_cols)
                .max(1);

            if num_cols_spanned >= num_cols {
                let y = grid.max_height();
                let x = grid.left;
                let allocated_w = grid.width;
                for col_h in &mut grid.col_heights {
                    *col_h = y + wh + grid.gap;
                }
                (x, y, allocated_w, wh)
            } else if num_cols_spanned == 1 {
                let col = grid.next_column();
                let x = grid.col_lefts[col];
                let y = grid.col_heights[col];
                grid.col_heights[col] += wh + grid.gap;
                (x, y, grid.col_width, wh)
            } else {
                let n = num_cols_spanned;
                let mut best_start_col = 0;
                let mut min_max_h = f32::MAX;
                for c in 0..=(num_cols - n) {
                    let mut max_h = 0.0f32;
                    for i in 0..n {
                        if grid.col_heights[c + i] > max_h {
                            max_h = grid.col_heights[c + i];
                        }
                    }
                    if max_h < min_max_h {
                        min_max_h = max_h;
                        best_start_col = c;
                    }
                }
                let x = grid.col_lefts[best_start_col];
                let y = min_max_h;
                let allocated_w = n as f32 * grid.col_width + (n - 1) as f32 * grid.gap;
                for i in 0..n {
                    grid.col_heights[best_start_col + i] = y + wh + grid.gap;
                }
                (x, y, allocated_w, wh)
            }
        } else {
            (0.0, 0.0, 0.0, wh)
        }
    }

    fn set_section_count(&mut self, count: usize) {
        self.num_sections = Some(count);
    }

    fn get_column_width(&self) -> Option<f32> {
        self.grid.as_ref().map(|g| g.col_width)
    }

    fn get_gap(&self) -> f32 {
        crate::layout::grid_gap()
    }

    fn layout(&self, x: f32, y: f32, w: f32, _h: f32, children: &[*mut (dyn crate::widget::WidgetHost + 'static)], _ctx: &mut crate::context::UiContext) -> f32 {
        let usable_w = w.max(1.0);
        let min_col_width = crate::layout::grid_min_col_width();
        let gap = crate::layout::grid_gap();
        let cols = (((usable_w + gap) / (min_col_width + gap)).floor().max(1.0)) as usize;
        let count = if let Some(n) = self.num_sections {
            n.min(cols).max(1)
        } else {
            cols
        };

        let total_gap = gap * (count - 1) as f32;
        let available_w = (w - total_gap).max(1.0);
        let col_w = available_w / count as f32;
        
        let mut col_heights = vec![y; count];

        for &child_ptr in children {
            unsafe {
                let child = &mut *child_ptr;
                let ch = child.preferred_height().unwrap_or(child.rect().3);
                let use_h = if ch > 0.0 { ch } else { 44.0 };
                
                let mut min_col = 0;
                let mut min_h = col_heights[0];
                for i in 1..count {
                    if col_heights[i] < min_h {
                        min_h = col_heights[i];
                        min_col = i;
                    }
                }
                
                let cx = x + min_col as f32 * (col_w + gap);
                let cy = col_heights[min_col];
                child.set_rect(cx, cy, col_w, use_h);
                col_heights[min_col] += use_h + gap;
            }
        }
        
        let max_h = col_heights.iter().cloned().fold(0.0f32, |a, b| a.max(b));
        (max_h - y).max(0.0)
    }

    fn measure(&self, constraints: crate::widget::LayoutConstraints, children: &[*mut (dyn crate::widget::WidgetHost + 'static)], ctx: &crate::context::UiContext) -> crate::widget::Size {
        let usable_w = constraints.max_width.max(1.0);
        let min_col_width = crate::layout::grid_min_col_width();
        let gap = crate::layout::grid_gap();
        let cols = (((usable_w + gap) / (min_col_width + gap)).floor().max(1.0)) as usize;
        let count = if let Some(n) = self.num_sections {
            n.min(cols).max(1)
        } else {
            cols
        };

        let mut col_heights = vec![0.0f32; count];
        let total_gap = gap * (count - 1) as f32;
        let available_w = (constraints.max_width - total_gap).max(1.0);
        let col_w = available_w / count as f32;
        
        let child_constraints = crate::widget::LayoutConstraints::new(col_w, col_w, constraints.min_height, constraints.max_height);

        for &child_ptr in children {
            unsafe {
                let size = (*child_ptr).measure(child_constraints, ctx);
                let mut min_col = 0;
                let mut min_h = col_heights[0];
                for i in 1..count {
                    if col_heights[i] < min_h {
                        min_h = col_heights[i];
                        min_col = i;
                    }
                }
                col_heights[min_col] += size.height + gap;
            }
        }
        
        let max_h = col_heights.iter().cloned().fold(0.0f32, |a, b| a.max(b));
        crate::widget::Size {
            width: constraints.max_width,
            height: max_h.clamp(constraints.min_height, constraints.max_height),
        }
    }

    fn box_clone(&self) -> Box<dyn LayoutStrategy> {
        Box::new(self.clone())
    }
}

#[derive(Debug, Clone)]
pub struct RadialLayout {
    radial: Option<Radial>,
    aspect_ratio: f32,
    base_spacing: f32,
    idx: usize,
}

impl RadialLayout {
    pub fn new(aspect_ratio: f32, base_spacing: f32) -> Self {
        Self {
            radial: None,
            aspect_ratio,
            base_spacing,
            idx: 0,
        }
    }
}

impl LayoutStrategy for RadialLayout {
    fn init(&mut self, left: f32, top: f32, width: f32, height: f32) {
        let cx = left + width / 2.0;
        let cy = top + height / 2.0;
        let aspect = if self.aspect_ratio > 0.0 {
            self.aspect_ratio
        } else {
            let screen_aspect = (width / height.max(1.0)).max(0.1);
            1.0 + (screen_aspect - 1.0) * 0.4
        };
        self.radial = Some(Radial::new(cx, cy, aspect, self.base_spacing));
        self.idx = 0;
    }

    fn allocate(&mut self, ww: f32, wh: f32) -> (f32, f32, f32, f32) {
        if let Some(ref radial) = self.radial {
            let rect = radial.widget_rect(self.idx, ww, wh);
            self.idx += 1;
            rect
        } else {
            (0.0, 0.0, ww, wh)
        }
    }

    fn layout(&self, x: f32, y: f32, w: f32, h: f32, children: &[*mut (dyn crate::widget::WidgetHost + 'static)], _ctx: &mut crate::context::UiContext) -> f32 {
        let cx = x + w / 2.0;
        let cy = y + h / 2.0;
        let aspect = if self.aspect_ratio > 0.0 {
            self.aspect_ratio
        } else {
            let screen_aspect = (w / h.max(1.0)).max(0.1);
            1.0 + (screen_aspect - 1.0) * 0.4
        };
        let radial = Radial::new(cx, cy, aspect, self.base_spacing);
        for (idx, &child_ptr) in children.iter().enumerate() {
            unsafe {
                let child = &mut *child_ptr;
                let cw = child.rect().2;
                let ch = child.preferred_height().unwrap_or(child.rect().3);
                let use_h = if ch > 0.0 { ch } else { 44.0 };
                let (rx, ry, rw, rh) = radial.widget_rect(idx, cw, use_h);
                child.set_rect(rx, ry, rw, rh);
            }
        }
        h
    }

    fn measure(&self, constraints: crate::widget::LayoutConstraints, children: &[*mut (dyn crate::widget::WidgetHost + 'static)], ctx: &crate::context::UiContext) -> crate::widget::Size {
        let cx = constraints.max_width / 2.0;
        let cy = constraints.max_height / 2.0;
        let aspect = if self.aspect_ratio > 0.0 {
            self.aspect_ratio
        } else {
            let screen_aspect = (constraints.max_width / constraints.max_height.max(1.0)).max(0.1);
            1.0 + (screen_aspect - 1.0) * 0.4
        };
        let radial = Radial::new(cx, cy, aspect, self.base_spacing);
        let mut max_w = 0.0f32;
        let mut max_h = 0.0f32;
        for (idx, &child_ptr) in children.iter().enumerate() {
            unsafe {
                let size = (*child_ptr).measure(constraints, ctx);
                let (rx, ry, rw, rh) = radial.widget_rect(idx, size.width, size.height);
                max_w = max_w.max(rx + rw);
                max_h = max_h.max(ry + rh);
            }
        }
        crate::widget::Size {
            width: max_w.clamp(constraints.min_width, constraints.max_width),
            height: max_h.clamp(constraints.min_height, constraints.max_height),
        }
    }

    fn box_clone(&self) -> Box<dyn LayoutStrategy> {
        Box::new(self.clone())
    }
}

pub struct PageLayoutBuilder<'a, P> {
    pub strategy: &'a mut dyn LayoutStrategy,
    pub cx: f32,
    pub cy: f32,
    pub cw: f32,
    pub ch: f32,
    pub section_width: f32,
    pub idx: usize,
    _phantom: std::marker::PhantomData<P>,
}

impl<'a, P: RenderTarget + Default> PageLayoutBuilder<'a, P> {
    pub fn new(
        strategy: &'a mut dyn LayoutStrategy,
        cx: f32,
        cy: f32,
        cw: f32,
        ch: f32,
        section_width: f32,
    ) -> Self {
        strategy.init(cx, cy, cw, ch);
        Self {
            strategy,
            cx,
            cy,
            cw,
            ch,
            section_width,
            idx: 0,
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn with_section_count(self, count: usize) -> Self {
        self.strategy.set_section_count(count);
        self.strategy.init(self.cx, self.cy, self.cw, self.ch);
        self
    }

    pub fn add_section<F>(&mut self, final_pc: &mut P, label: &str, focused: bool, mut render_fn: F)
    where
        F: FnMut(&mut SectionContext<'_, P>),
    {
        let width = self.strategy.get_column_width().unwrap_or(self.section_width);
        let mut dummy = P::default();
        let mut dummy_ctx = SectionContext::new(&mut dummy, 0.0, 0.0, width, label, focused, false);
        render_fn(&mut dummy_ctx);
        let wh = dummy_ctx.finish();
        let (rx, ry, rw, _) = self.strategy.allocate(width, wh);
        let mut real_ctx = SectionContext::new(final_pc, rx, ry, rw, label, focused, false);
        render_fn(&mut real_ctx);
        real_ctx.finish();
        self.idx += 1;
    }

    pub fn add_section_with_width<F>(&mut self, final_pc: &mut P, width: f32, label: &str, focused: bool, mut render_fn: F)
    where
        F: FnMut(&mut SectionContext<'_, P>),
    {
        let mut dummy = P::default();
        let mut dummy_ctx = SectionContext::new(&mut dummy, 0.0, 0.0, width, label, focused, false);
        render_fn(&mut dummy_ctx);
        let wh = dummy_ctx.finish();
        let (rx, ry, rw, _) = self.strategy.allocate(width, wh);
        let mut real_ctx = SectionContext::new(final_pc, rx, ry, rw, label, focused, false);
        render_fn(&mut real_ctx);
        real_ctx.finish();
        self.idx += 1;
    }

    pub fn add_section_spanned<F>(&mut self, final_pc: &mut P, label: &str, span: usize, focused: bool, render_fn: F)
    where
        F: FnMut(&mut SectionContext<'_, P>),
    {
        let col_width = self.strategy.get_column_width().unwrap_or(self.section_width);
        let gap = self.strategy.get_gap();
        let width = span as f32 * col_width + (span - 1) as f32 * gap;
        self.add_section_with_width(final_pc, width, label, focused, render_fn);
    }
}

pub struct SectionContext<'a, P> {
    pub pc: &'a mut P,
    pub left: f32,
    pub top: f32,
    pub content_y: f32,
    pub cw: f32,
    pub label_width: f32,
    pub label_x: f32,
    /// The title tab box (x, y, w, h) when the host's relief styling laid the
    /// label out left-aligned — `finish` offers it with the section carve.
    pub relief_tab: Option<(f32, f32, f32, f32)>,
    pub focused: bool,
    pub is_child: bool,
    pub grid: Grid,
    pub last_col: usize,
    pub content_start_y: f32,
    pub row_gap: f32,
}

impl<'a, P: RenderTarget> SectionContext<'a, P> {
    pub const DEFAULT_MARGIN_X: f32 = 12.0;
    pub const DEFAULT_ROW_GAP: f32 = 8.0;

    fn estimate_label_width(label: &str, font_size: f32, font_fam: &str) -> f32 {
        estimate_label_width_helper(label, font_size, font_fam)
    }

    pub fn padding(&self) -> f32 {
        section_padding()
    }

    pub fn new(pc: &'a mut P, left: f32, top: f32, cw: f32, label: &str, focused: bool, is_child: bool) -> Self {
        let font_setting = if is_child {
            nested_section_label_font()
        } else {
            section_label_font()
        };
        let (font_fam, font_size_opt) = parse_font_string(&font_setting);
        let font_size = font_size_opt.unwrap_or(if is_child { 12.0 } else { 14.0 });
        let font_color = if is_child { [0.53, 0.53, 0.60, 1.0] } else { [0.83, 0.83, 0.83, 1.0] };
        let label_width = Self::estimate_label_width(label, font_size, &font_fam);
        let relief_style = pc.section_relief_style();
        let label_x = if is_child {
            let base_x = match nested_section_label_alignment() {
                0 => left + 12.0,
                1 => left + (cw - label_width) / 2.0,
                2 => left + cw - 12.0 - label_width,
                _ => left + 12.0,
            };
            base_x + nested_section_label_offset()
        } else if relief_style {
            // Sunken style: the title sits in a tab flush with the well's left
            // edge (the designer look), not centered on the border.
            left + 12.0
        } else {
            left + (cw - label_width) / 2.0
        };
        // Under relief styling the well fills the whole allocated rect (so the
        // page's gaps and margins are the visual gaps) — the tab tops the
        // allocation and the label centers inside it.
        let label_y = if relief_style { top + 4.0 } else { top };
        pc.text_with_font(label, label_x, label_y, font_size, font_color, &font_fam);

        // The tab wraps the label, flush on the well's top-left corner; clamped
        // so off-default child alignments can't push it outside the well.
        let relief_tab = if relief_style && label_width > 0.0 {
            let tab_x = (label_x - 12.0).max(left);
            Some((tab_x, top, label_width + 24.0, font_size + 10.0))
        } else {
            None
        };

        let pad = section_padding();
        let margin_x = 2.0 * pad + 12.0;
        let usable_w = (cw - 2.0 * margin_x).max(1.0);
        let min_col_width = 130.0;
        let gap = 8.0;
        let max_cols = if is_child {
            1
        } else {
            ((usable_w + gap) / (min_col_width + gap)).floor().max(1.0).min(2.0) as usize
        };
        let content_start_y = top + pad + 19.0;
        let grid = Grid::new(left + margin_x, content_start_y, usable_w, min_col_width, gap, max_cols);

        Self {
            pc,
            left,
            top,
            content_y: content_start_y,
            cw,
            label_width,
            label_x,
            relief_tab,
            focused,
            is_child,
            grid,
            last_col: usize::MAX,
            content_start_y,
            row_gap: Self::DEFAULT_ROW_GAP,
        }
    }

    pub fn with_row_gap(mut self, gap: f32) -> Self {
        self.row_gap = gap;
        self
    }

    pub fn set_row_gap(&mut self, gap: f32) {
        self.row_gap = gap;
    }

    pub fn ax(&self, x_off: f32) -> f32 {
        let shift = if x_off >= 12.0 { self.padding() } else { 0.0 };
        self.left + x_off + shift
    }

    pub fn ay(&self) -> f32 {
        self.content_y
    }

    pub fn spacing(&mut self, dy: f32) {
        if self.grid.col_heights.len() >= 2 {
            if self.last_col == usize::MAX {
                for h in &mut self.grid.col_heights {
                    *h += dy;
                }
            } else if self.last_col < self.grid.col_heights.len() {
                self.grid.col_heights[self.last_col] += dy;
            }
            self.content_y = self.grid.max_height();
        } else {
            self.content_y += dy;
            for h in &mut self.grid.col_heights {
                *h += dy;
            }
        }
    }

    pub fn text(&mut self, text: &str, x_off: f32, y_off: f32, font_size: f32, color: [f32; 4]) {
        let mut y = self.content_y + y_off;
        if y > self.content_start_y {
            y += self.row_gap;
        }
        self.pc.text(text, self.ax(x_off), y, font_size, color);
        let new_bottom = y + font_size + 4.0;
        self.content_y = new_bottom;
        for h in &mut self.grid.col_heights {
            *h = new_bottom;
        }
    }

    pub fn widget<T: WidgetHost + 'static>(&mut self, w: &mut T, _x_off: f32, _ww: f32, mut wh: f32, ctx: &mut UiContext) {
        if let Some(pref) = w.preferred_height() {
            wh = pref;
        }
        let pad = self.padding();
        let top_room = crate::widget::label_offset(w);
        let total_h = wh + top_room;

        let name = w.type_name();
        let span_full = name == "Trackpad"
            || name == "Canvas"
            || name == "UsageBar"
            || name == "ProgressBar"
            || name == "ButtonStrip"
            || name == "Spreadsheet"
            || name == "Graph";

        if span_full {
            let margin_x = 2.0 * pad + 12.0;
            let x = self.left + margin_x;
            let clamped_w = (self.cw - 2.0 * margin_x).max(0.0);
            let mut max_h = self.grid.max_height().max(self.content_y);
            if max_h > self.content_start_y {
                max_h += self.row_gap;
            }
            let y = max_h;

            w.set_row_rect(self.left + pad, self.cw - 2.0 * pad);
            render_widget(self.pc, w, x, y, clamped_w, total_h, ctx);

            let new_bottom = y + total_h;
            self.content_y = new_bottom;
            for h in &mut self.grid.col_heights {
                *h = new_bottom;
            }
        } else {
            let max_h = self.grid.max_height();
            if self.content_y > max_h {
                for h in &mut self.grid.col_heights {
                    *h = self.content_y;
                }
            }

            let col = self.grid.next_column();
            self.last_col = col;
            let x = self.grid.col_lefts[col];
            let mut y = self.grid.col_heights[col];
            if y > self.content_start_y {
                y += self.row_gap;
            }

            let aligned_x = x;
            let aligned_w = self.grid.col_width;

            w.set_row_rect(aligned_x, aligned_w);
            render_widget(self.pc, w, aligned_x, y, aligned_w, total_h, ctx);
            self.grid.col_heights[col] = y + total_h;
            self.content_y = self.grid.max_height();
        }
    }

    pub fn widget_full<T: WidgetHost + 'static>(&mut self, w: &mut T, wh: f32, ctx: &mut UiContext) {
        let x_off = 12.0;
        let ww = self.cw - 2.0 * (self.padding() + x_off); // cw - 40.0
        self.widget(w, x_off, ww, wh, ctx);
    }

    pub fn separator(&mut self) {
        let pad = self.padding();
        let x = self.ax(pad);
        let max_h = self.grid.max_height().max(self.content_y);
        let y = max_h;
        self.pc.rect([0.18, 0.18, 0.27, 1.0], x, y, self.cw - 2.0 * pad, 1.0);
        self.content_y = max_h + 8.0;
        for h in &mut self.grid.col_heights {
            *h = self.content_y;
        }
    }

    pub fn rect(&mut self, color: [f32; 4], x_off: f32, w: f32, h: f32) {
        let max_h = self.grid.max_height().max(self.content_y);
        self.pc.rect(color, self.ax(x_off), max_h, w, h);
        self.content_y = max_h + h;
        for col_h in &mut self.grid.col_heights {
            *col_h = self.content_y;
        }
    }

    pub fn row_layout(&self, count: usize, gap: f32) -> Vec<(f32, f32)> {
        let margin_x = 2.0 * self.padding() + 12.0;
        let usable_w = self.cw - 2.0 * margin_x;
        if count == 0 {
            return Vec::new();
        }
        let total_gap = gap * (count - 1) as f32;
        let col_w = (usable_w - total_gap).max(0.0) / count as f32;

        let mut cols = Vec::with_capacity(count);
        for i in 0..count {
            let x = self.left + margin_x + i as f32 * (col_w + gap);
            cols.push((x, col_w));
        }
        cols
    }

    pub fn row<F>(&mut self, count: usize, gap: f32, h: f32, mut f: F)
    where
        F: FnMut(usize, f32, f32),
    {
        let max_h = self.grid.max_height().max(self.content_y);
        for col_h in &mut self.grid.col_heights {
            *col_h = max_h;
        }
        self.content_y = max_h;

        let cols = self.row_layout(count, gap);
        for (i, &(x, w)) in cols.iter().enumerate() {
            f(i, x, w);
        }
        self.content_y += h;

        for col_h in &mut self.grid.col_heights {
            *col_h = self.content_y;
        }
    }

    pub fn vstack(&mut self, spacing: f32) -> VStack<'_, 'a, P> {
        VStack {
            context: self,
            spacing,
        }
    }

    pub fn add_section<F>(&mut self, label: &str, focused: bool, mut render_fn: F)
    where
        F: FnMut(&mut SectionContext<'_, P>),
    {
        let pad = self.padding();
        let (left, top, cw, is_side_by_side) = if self.grid.col_heights.len() >= 2 {
            let col = self.grid.next_column();
            self.last_col = col;
            let x = self.grid.col_lefts[col];
            let mut y = self.grid.col_heights[col];
            if y > self.content_start_y {
                y += self.row_gap;
            }
            (x, y, self.grid.col_width, true)
        } else {
            let left = self.ax(0.0) + pad;
            let mut max_h = self.grid.max_height().max(self.content_y);
            if max_h > self.content_start_y {
                max_h += self.row_gap;
            }
            let top = max_h;
            let cw = self.cw - 2.0 * pad;
            (left, top, cw, false)
        };

        let mut sub_ctx = SectionContext::new(self.pc, left, top, cw, label, focused, true);
        render_fn(&mut sub_ctx);
        let new_bottom = sub_ctx.finish();

        if is_side_by_side {
            let col = self.last_col;
            self.grid.col_heights[col] = new_bottom;
            self.content_y = self.grid.max_height();
        } else {
            self.content_y = new_bottom;
            for h in &mut self.grid.col_heights {
                *h = new_bottom;
            }
        }
    }

    pub fn finish(self) -> f32 {
        let border: [f32; 4] = if self.is_child {
            if self.focused {
                [0.22, 0.38, 0.24, 1.0]
            } else {
                [0.18, 0.18, 0.25, 1.0]
            }
        } else {
            if self.focused {
                [0.30, 0.50, 0.32, 1.0] // Focused green
            } else {
                [0.25, 0.25, 0.35, 1.0] // Default gray
            }
        };
        let pad = self.padding();
        let x = self.left + pad;
        let y = self.top + 7.0;
        let w = self.cw - 2.0 * pad;
        let h = self.content_y - y;
        let extra_bottom = pad + 12.0;
        let bottom = y + h + extra_bottom;

        if let Some(tab) = self.relief_tab {
            // Sunken style: the well spans the full allocated rect — body top
            // edge at the tab's bottom (the tab is flush ON the body, the
            // designer union shape), walls on the allocation's edges, and no
            // trailing slack so the layout gap IS the visual gap.
            let body_y = tab.1 + tab.3;
            let frame = SectionFrame {
                x: self.left,
                y: body_y,
                w: self.cw,
                h: bottom - body_y,
                tab: Some(tab),
                focused: self.focused,
                is_child: self.is_child,
            };
            if self.pc.section_relief(&frame) {
                return self.content_y + extra_bottom;
            }
        }

        let left_edge = x;
        let right_edge = x + w;
        if self.label_width > 0.0 {
            let label_x = self.label_x;
            let gap_margin = 6.0;
            let gap_start = label_x - gap_margin;
            let gap_end = label_x + self.label_width + gap_margin;
            if gap_start > left_edge {
                self.pc.rect(border, left_edge, y, gap_start - left_edge, 1.0);
            }
            if right_edge > gap_end {
                self.pc.rect(border, gap_end, y, right_edge - gap_end, 1.0);
            }
        } else {
            self.pc.rect(border, left_edge, y, w, 1.0);
        }

        self.pc.rect(border, x, y + h + extra_bottom, w, 1.0);
        self.pc.rect(border, x, y, 1.0, h + extra_bottom);
        self.pc.rect(border, x + w - 1.0, y, 1.0, h + extra_bottom);
        self.content_y + extra_bottom + 8.0
    }
}


pub struct VStack<'b, 'a, P> {
    pub context: &'b mut SectionContext<'a, P>,
    pub spacing: f32,
}

impl<'b, 'a, P: RenderTarget> VStack<'b, 'a, P> {
    pub fn add_widget<T: WidgetHost + 'static>(&mut self, w: &mut T, _ww: f32, wh: f32, ctx: &mut UiContext) {
        let pad = self.context.padding();
        let margin_x = 2.0 * pad + 12.0;
        let x = self.context.left + margin_x;
        let clamped_w = (self.context.cw - 2.0 * margin_x).max(0.0);

        let mut max_h = self.context.grid.max_height().max(self.context.content_y);
        if max_h > self.context.content_start_y {
            max_h += self.context.row_gap;
        }
        let y = max_h;

        let pref_h = w.preferred_height().unwrap_or(wh);
        let top_room = crate::widget::label_offset(w);
        let total_h = pref_h + top_room;

        w.set_row_rect(self.context.left + pad, self.context.cw - 2.0 * pad);
        render_widget(self.context.pc, w, x, y, clamped_w, total_h, ctx);

        let new_bottom = y + total_h;
        self.context.content_y = new_bottom;
        for h in &mut self.context.grid.col_heights {
            *h = new_bottom;
        }
        self.context.spacing(self.spacing);
    }

    pub fn add_row<F>(&mut self, count: usize, gap: f32, h: f32, mut f: F)
    where
        F: FnMut(&mut SectionContext<'a, P>, usize, f32, f32),
    {
        let max_h = self.context.grid.max_height().max(self.context.content_y);
        for col_h in &mut self.context.grid.col_heights {
            *col_h = max_h;
        }
        self.context.content_y = max_h;

        let cols = self.context.row_layout(count, gap);
        for (i, &(x, w)) in cols.iter().enumerate() {
            self.context.content_y = max_h;
            f(self.context, i, x, w);
        }

        let new_bottom = max_h + h;
        self.context.content_y = new_bottom;
        for col_h in &mut self.context.grid.col_heights {
            *col_h = new_bottom;
        }
        self.context.spacing(self.spacing);
    }
}

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

pub fn parse_font_for_alias(content: &str, alias: &str) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    for i in 0..lines.len() {
        let line = lines[i].trim();
        if line.contains("<test") && line.contains("name=\"family\"") && line.contains(&format!("<string>{}</string>", alias)) {
            for j in (i + 1)..(i + 6).min(lines.len()) {
                let next_line = lines[j].trim();
                if next_line.contains("<edit") {
                    for k in (j + 1)..(j + 6).min(lines.len()) {
                        let str_line = lines[k].trim();
                        if str_line.contains("<string>") && str_line.contains("</string>") {
                            if let Some(start) = str_line.find("<string>") {
                                if let Some(end) = str_line.find("</string>") {
                                    let font = &str_line[start + 8..end];
                                    return Some(font.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

pub fn read_preferred_fonts() -> (String, String, String, String, String, String, String) {
    let home = std::env::var("HOME").unwrap_or_default();
    let path = std::path::Path::new(&home).join(".config/fontconfig/fonts.conf");
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    
    let sans = parse_font_for_alias(&content, "sans-serif").unwrap_or_else(|| "Noto Sans".to_string());
    let serif = parse_font_for_alias(&content, "serif").unwrap_or_else(|| "Noto Serif".to_string());
    let mono = parse_font_for_alias(&content, "monospace").unwrap_or_else(|| "Noto Sans Mono".to_string());
    let borders = parse_font_for_alias(&content, "window-borders").unwrap_or_else(|| "Noto Sans".to_string());
    let status = parse_font_for_alias(&content, "status-interface").unwrap_or_else(|| "Noto Sans".to_string());
    let fuzzel_font = parse_font_for_alias(&content, "fuzzel").unwrap_or_else(|| "Noto Sans".to_string());
    let term = parse_font_for_alias(&content, "terminal").unwrap_or_else(|| "Noto Sans Mono".to_string());
    
    (sans, serif, mono, borders, status, fuzzel_font, term)
}

impl crate::widget::ContainerLayout for FlexLayout {
    fn box_clone_container(&self) -> Box<dyn crate::widget::ContainerLayout> {
        Box::new(self.clone())
    }
}

impl crate::widget::ContainerLayout for ColumnLayout {
    fn box_clone_container(&self) -> Box<dyn crate::widget::ContainerLayout> {
        Box::new(self.clone())
    }
}

impl crate::widget::ContainerLayout for AdaptiveGrid {
    fn box_clone_container(&self) -> Box<dyn crate::widget::ContainerLayout> {
        Box::new(self.clone())
    }
}

impl crate::widget::ContainerLayout for RadialLayout {
    fn box_clone_container(&self) -> Box<dyn crate::widget::ContainerLayout> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockRenderTarget {
        rects: Vec<([f32; 4], f32, f32, f32, f32)>,
    }

    impl RenderTarget for MockRenderTarget {
        fn rect(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32) {
            self.rects.push((color, x, y, w, h));
        }
        fn text(&mut self, _content: &str, _x: f32, _y: f32, _size: f32, _color: [f32; 4]) {}
    }

    struct MockWidget {
        base: crate::widget::Widget,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    }

    impl WidgetHost for MockWidget {
        crate::impl_widget_base!(MockWidget);
        fn rect(&self) -> (f32, f32, f32, f32) {
            (self.x, self.y, self.w, self.h)
        }
        fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
            self.x = x;
            self.y = y;
            self.w = w;
            self.h = h;
        }
        fn color(&self) -> [f32; 4] {
            [0.0, 0.0, 0.0, 0.0]
        }
    }

    struct MockWidgetWithLabel {
        base: crate::widget::Widget,
    }

    impl WidgetHost for MockWidgetWithLabel {
        crate::impl_widget_base!(MockWidgetWithLabel);
        fn rect(&self) -> (f32, f32, f32, f32) {
            let offset = crate::widget::label_offset(self);
            (self.base.x, self.base.y - offset, self.base.w, self.base.h + offset)
        }
        fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
            let offset = crate::widget::label_offset(self);
            self.base.x = x;
            self.base.y = y + offset;
            self.base.w = w;
            self.base.h = (h - offset).max(0.0);
        }
        fn color(&self) -> [f32; 4] {
            [0.0, 0.0, 0.0, 0.0]
        }
    }

    #[test]
    fn test_vstack_flow() {
        let orig_margin = label_margin();
        set_label_margin(6.0);
        let orig_font = control_label_font();
        set_control_label_font("Berkeley Mono 12");
        let orig_font_detached = control_label_font_detached();
        set_control_label_font_detached("Berkeley Mono 12");
        let _ = section_padding();
        set_section_padding(8.0);
        let mut mock_pc = MockRenderTarget { rects: Vec::new() };
        let mut sec = Section::new(&mut mock_pc, 10.0, 20.0, 200.0, "Test Section");
        
        let start_y = sec.ay();
        let mut stack = sec.vstack(&mut mock_pc, 10.0);

        let mut dummy = crate::context::UiContext::new();
        let mut w1 = MockWidget { base: crate::widget::Widget::new(), x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
        stack.add_widget(&mut w1, 50.0, 30.0, &mut dummy);

        // Standard margin should be applied
        assert_eq!(w1.x, 38.0);
        assert_eq!(w1.y, start_y);

        let mut w2 = MockWidget { base: crate::widget::Widget::new(), x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
        stack.add_widget(&mut w2, 60.0, 40.0, &mut dummy);

        // Second widget should start after first widget height + vstack spacing
        assert_eq!(w2.y, start_y + 30.0 + 10.0);

        let mut base = crate::widget::Widget::new();
        base.label = Some("Test Label".to_string());
        let mut w3 = MockWidgetWithLabel { base };
        stack.add_widget(&mut w3, 70.0, 50.0, &mut dummy);

        let offset = w3.base.label_offset();
        
        // Third widget has label, so its y should be shifted by offset
        assert_eq!(w3.base.y, start_y + 30.0 + 10.0 + 40.0 + 10.0 + offset);
        set_label_margin(orig_margin);
        set_control_label_font(&orig_font);
        set_control_label_font_detached(&orig_font_detached);
    }

    #[test]
    fn test_grid_layout() {
        // Test single column layout (width = 200, min_col_width = 300)
        let grid1 = Grid::new(10.0, 20.0, 200.0, 300.0, 10.0, 1);
        assert_eq!(grid1.col_heights.len(), 1);
        assert_eq!(grid1.col_lefts[0], 10.0);
        assert_eq!(grid1.col_width, 200.0);

        // Test multi column layout (width = 700, min_col_width = 300, gap = 20)
        // count = floor((700 + 20) / (300 + 20)) = floor(720 / 320) = 2.
        // total_gap = 20 * 1 = 20.
        // col_width = (700 - 20) / 2 = 340.
        let mut grid2 = Grid::new(5.0, 15.0, 700.0, 300.0, 20.0, 2);
        assert_eq!(grid2.col_heights.len(), 2);
        assert_eq!(grid2.col_lefts[0], 5.0);
        assert_eq!(grid2.col_lefts[1], 365.0);
        assert_eq!(grid2.col_width, 340.0);

        assert_eq!(grid2.next_column(), 0);
        grid2.col_heights[0] += 50.0; // Column 0 height becomes 65.0
        assert_eq!(grid2.next_column(), 1);
        grid2.col_heights[1] += 30.0; // Column 1 height becomes 45.0
        assert_eq!(grid2.next_column(), 1);
        grid2.col_heights[1] += 30.0; // Column 1 height becomes 75.0
        assert_eq!(grid2.next_column(), 0);
        
        assert_eq!(grid2.max_height(), 75.0);
    }

    #[test]
    fn test_subsection() {
        let mut pc = PopoverCollector::new();
        let mut subsec = Section::new_opt(&mut pc, 10.0, 20.0, 300.0, "Test Subsec", true);
        assert_eq!(subsec.left, 10.0);
        assert_eq!(subsec.top, 20.0);
        assert_eq!(subsec.cw, 300.0);
        assert!(subsec.is_child);
        
        let mut dummy = crate::context::UiContext::new();
        let mut w = MockWidget { base: crate::widget::Widget::new(), x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
        subsec.widget(&mut pc, &mut w, 12.0, 100.0, 40.0, &mut dummy);
        
        let bottom = subsec.finish(&mut pc);
        assert!(bottom > 20.0);
    }

    #[test]
    fn test_radial_layout() {
        let radial = Radial::new(100.0, 100.0, 1.5, 50.0);
        
        // Check first widget is centered at (100.0, 100.0)
        let rect0 = radial.widget_rect(0, 40.0, 30.0);
        assert_eq!(rect0, (100.0 - 20.0, 100.0 - 15.0, 40.0, 30.0));
        
        // Check Ring 1 (idx = 1) vs Ring 2 (idx = 7)
        let rect1 = radial.widget_rect(1, 40.0, 30.0);
        let rect7 = radial.widget_rect(7, 40.0, 30.0);
        
        let c1_x = rect1.0 + rect1.2 / 2.0;
        let c1_y = rect1.1 + rect1.3 / 2.0;
        let c7_x = rect7.0 + rect7.2 / 2.0;
        let c7_y = rect7.1 + rect7.3 / 2.0;
        
        let d1 = ((c1_x - 100.0).powi(2) + (c1_y - 100.0).powi(2)).sqrt();
        let d7 = ((c7_x - 100.0).powi(2) + (c7_y - 100.0).powi(2)).sqrt();
        
        // Ring 2 should be further out than Ring 1
        assert!(d7 > d1);
        assert!(d1 > 0.0);
    }

    #[test]
    fn test_nested_section_label_alignment() {
        let _ = nested_section_label_alignment();
        set_nested_section_label_alignment(1);
        assert_eq!(nested_section_label_alignment(), 1);
        set_nested_section_label_alignment(2);
        assert_eq!(nested_section_label_alignment(), 2);
        set_nested_section_label_alignment(0);
        assert_eq!(nested_section_label_alignment(), 0);

        let _ = nested_section_label_offset();
        set_nested_section_label_offset(15.0);
        assert_eq!(nested_section_label_offset(), 15.0);
        set_nested_section_label_offset(0.0);
        assert_eq!(nested_section_label_offset(), 0.0);
    }

    #[test]
    fn test_dropdown_height() {
        let _ = dropdown_height();
        set_dropdown_height(48.0);
        assert_eq!(dropdown_height(), 48.0);
        set_dropdown_height(44.0);
        assert_eq!(dropdown_height(), 44.0);
    }

    #[test]
    fn test_column_gap() {
        println!("LIST FONT: {:?}", list_font());
        let _ = column_gap();
        set_column_gap(24.0);
        assert_eq!(column_gap(), 24.0);
        set_column_gap(16.0);
        assert_eq!(column_gap(), 16.0);
    }

    #[test]
    fn test_print_fonts() {
        let db = crate::widget::get_font_db();
        for face in db.faces() {
            println!("FAMILY: {:?}", face.families);
        }
    }

    #[test]
    fn test_section_context_grid() {
        let mut mock_pc = MockRenderTarget { rects: Vec::new() };
        let mut ctx = SectionContext::new(&mut mock_pc, 10.0, 20.0, 500.0, "Test Section", false, false);
        assert_eq!(ctx.left, 10.0);
        assert_eq!(ctx.top, 20.0);
        assert_eq!(ctx.cw, 500.0);

        let mut ui_ctx = crate::context::UiContext::new();
        let mut w1 = MockWidget { base: crate::widget::Widget::new(), x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
        ctx.widget(&mut w1, 12.0, 100.0, 40.0, &mut ui_ctx);

        let mut w2 = MockWidget { base: crate::widget::Widget::new(), x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
        ctx.widget(&mut w2, 12.0, 100.0, 30.0, &mut ui_ctx);

        // Since cw=500, we should have multiple columns!
        // The first widget goes into column 0, second into column 1.
        assert_ne!(w1.x, w2.x);
        
        let bottom = ctx.finish();
        assert!(bottom > 20.0);
    }

    #[test]
    fn test_child_section_single_column_controls() {
        let mut mock_pc = MockRenderTarget { rects: Vec::new() };
        let mut ctx = SectionContext::new(&mut mock_pc, 10.0, 20.0, 500.0, "Test Child Section", false, true);
        assert_eq!(ctx.grid.col_heights.len(), 1);
        
        let mut ui_ctx = crate::context::UiContext::new();
        let mut w1 = MockWidget { base: crate::widget::Widget::new(), x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
        ctx.widget(&mut w1, 12.0, 100.0, 40.0, &mut ui_ctx);

        let mut w2 = MockWidget { base: crate::widget::Widget::new(), x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
        ctx.widget(&mut w2, 12.0, 100.0, 30.0, &mut ui_ctx);

        // Since it's a child section, we should have a single column only, so w1.x == w2.x.
        assert_eq!(w1.x, w2.x);
    }

    #[test]
    fn test_parent_section_side_by_side_child_sections() {
        let mut mock_pc = MockRenderTarget { rects: Vec::new() };
        let mut parent_ctx = SectionContext::new(&mut mock_pc, 10.0, 20.0, 500.0, "Parent Section", false, false);
        assert_eq!(parent_ctx.grid.col_heights.len(), 2);

        let mut sub_left_1 = 0.0;
        let mut sub_left_2 = 0.0;

        parent_ctx.add_section("Child Section 1", false, |subsec1| {
            sub_left_1 = subsec1.left;
        });

        parent_ctx.add_section("Child Section 2", false, |subsec2| {
            sub_left_2 = subsec2.left;
        });

        // The two child sections should be rendered side-by-side in different columns, so sub_left_1 != sub_left_2.
        assert_ne!(sub_left_1, sub_left_2);
    }

    #[test]
    fn test_section_context_spacing_preserves_columns() {
        let mut mock_pc = MockRenderTarget { rects: Vec::new() };
        let mut ctx = SectionContext::new(&mut mock_pc, 10.0, 20.0, 500.0, "Test Section", false, false);
        assert_eq!(ctx.grid.col_heights.len(), 2);

        let mut ui_ctx = crate::context::UiContext::new();
        let mut w1 = MockWidget { base: crate::widget::Widget::new(), x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
        ctx.widget(&mut w1, 12.0, 100.0, 40.0, &mut ui_ctx); // placed in col 0

        let height_col_0_before = ctx.grid.col_heights[0];
        let height_col_1_before = ctx.grid.col_heights[1];
        assert_ne!(height_col_0_before, height_col_1_before);

        ctx.spacing(12.0);

        let height_col_0_after = ctx.grid.col_heights[0];
        let height_col_1_after = ctx.grid.col_heights[1];
        assert_eq!(height_col_0_after, height_col_0_before + 12.0);
        assert_eq!(height_col_1_after, height_col_1_before);
        assert_ne!(height_col_0_after, height_col_1_after);
    }

    #[test]
    fn test_spinbox_button_padding_config() {
        let padding = spinbox_button_padding();
        println!("Parsed spinbox button padding: {}", padding);
        assert!(padding >= 0.0);
    }

    #[test]
    fn test_graph_style_configuration() {
        // Trigger load_colors_once first so it doesn't overwrite values later
        let _ = crate::color::page_low_color();

        set_graph_spacing_x(200.0);
        set_graph_spacing_y(100.0);
        set_graph_grid_snap(true);
        set_graph_blur(0.8);
        set_graph_node_corner_radius(8.0);
        set_graph_wire_size(10.0);
        set_graph_wire_activation_radius(15.0);
        set_graph_connector_size(12.0);
        set_graph_connector_activation_radius(18.0);

        assert_eq!(graph_spacing_x(), 200.0);
        assert_eq!(graph_spacing_y(), 100.0);
        assert_eq!(graph_grid_snap(), true);
        assert_eq!(graph_blur(), 0.8);
        assert_eq!(graph_node_corner_radius(), 8.0);
        assert_eq!(graph_wire_size(), 10.0);
        assert_eq!(graph_wire_activation_radius(), 15.0);
        assert_eq!(graph_connector_size(), 12.0);
        assert_eq!(graph_connector_activation_radius(), 18.0);

        crate::color::set_graph_cell_color([0.1, 0.2, 0.3]);
        crate::color::set_graph_gap_color([0.4, 0.5, 0.6]);
        crate::color::set_graph_node_color([0.7, 0.8, 0.9, 1.0]);
        crate::color::set_graph_node_selected_color([0.9, 0.8, 0.7, 1.0]);
        crate::color::set_graph_node_drag_color([0.5, 0.5, 0.5, 1.0]);
        crate::color::set_node_color([0.7, 0.8, 0.9, 1.0]);
        crate::color::set_node_selected_color([0.9, 0.8, 0.7, 1.0]);
        crate::color::set_node_drag_color([0.5, 0.5, 0.5, 1.0]);
        crate::color::set_graph_wire_color([0.1, 0.1, 0.1, 1.0]);
        crate::color::set_graph_wire_highlight_color([0.2, 0.2, 0.2, 1.0]);
        crate::color::set_graph_connector_color([0.3, 0.3, 0.3, 1.0]);
        crate::color::set_graph_connector_highlight_color([0.4, 0.4, 0.4, 1.0]);

        let cell_color = crate::color::graph_cell_color();
        assert_eq!(cell_color, [0.1, 0.2, 0.3]);
        let gap_color = crate::color::graph_gap_color();
        assert_eq!(gap_color, [0.4, 0.5, 0.6]);
        assert_eq!(crate::color::graph_node_color(), [0.7, 0.8, 0.9, 1.0]);
        assert_eq!(crate::color::graph_node_selected_color(), [0.9, 0.8, 0.7, 1.0]);
        assert_eq!(crate::color::graph_node_drag_color(), [0.5, 0.5, 0.5, 1.0]);
        assert_eq!(crate::color::node_color(), [0.7, 0.8, 0.9, 1.0]);
        assert_eq!(crate::color::node_selected_color(), [0.9, 0.8, 0.7, 1.0]);
        assert_eq!(crate::color::node_drag_color(), [0.5, 0.5, 0.5, 1.0]);
        assert_eq!(crate::color::graph_wire_color(), [0.1, 0.1, 0.1, 1.0]);
        assert_eq!(crate::color::graph_wire_highlight_color(), [0.2, 0.2, 0.2, 1.0]);
        assert_eq!(crate::color::graph_connector_color(), [0.3, 0.3, 0.3, 1.0]);
        assert_eq!(crate::color::graph_connector_highlight_color(), [0.4, 0.4, 0.4, 1.0]);
    }

    #[test]
    fn test_mosaic_layout() {
        use crate::widget::MosaicLayout;
        use crate::layout::LayoutStrategy;
        
        let layout = MosaicLayout {
            gap: 10.0,
            padding_x: 5.0,
            padding_y: 5.0,
        };

        let mut dummy = crate::context::UiContext::new();
        let mut w1 = MockWidget { base: crate::widget::Widget::new(), x: 0.0, y: 0.0, w: 100.0, h: 50.0 };
        let mut w2 = MockWidget { base: crate::widget::Widget::new(), x: 0.0, y: 0.0, w: 100.0, h: 80.0 };
        let mut w3 = MockWidget { base: crate::widget::Widget::new(), x: 0.0, y: 0.0, w: 80.0, h: 40.0 };
        
        let children = vec![
            &mut w1 as *mut MockWidget as *mut (dyn WidgetHost + 'static),
            &mut w2 as *mut MockWidget as *mut (dyn WidgetHost + 'static),
            &mut w3 as *mut MockWidget as *mut (dyn WidgetHost + 'static),
        ];

        let _ = layout.layout(10.0, 20.0, 250.0, 500.0, &children, &mut dummy);

        assert_eq!(w1.x, 15.0);
        assert_eq!(w1.y, 25.0);
        assert_eq!(w2.x, 15.0);
        assert_eq!(w2.y, 85.0);
        assert_eq!(w3.x, 125.0);
        assert_eq!(w3.y, 25.0);
    }

    #[test]
    fn test_reverse_mosaic_layout() {
        use crate::widget::ReverseMosaicLayout;
        use crate::layout::LayoutStrategy;
        
        let layout = ReverseMosaicLayout {
            gap: 10.0,
            padding_x: 5.0,
            padding_y: 5.0,
        };

        let mut dummy = crate::context::UiContext::new();
        let mut w1 = MockWidget { base: crate::widget::Widget::new(), x: 0.0, y: 0.0, w: 100.0, h: 50.0 };
        let mut w2 = MockWidget { base: crate::widget::Widget::new(), x: 0.0, y: 0.0, w: 100.0, h: 80.0 };
        let mut w3 = MockWidget { base: crate::widget::Widget::new(), x: 0.0, y: 0.0, w: 80.0, h: 40.0 };
        
        let children = vec![
            &mut w1 as *mut MockWidget as *mut (dyn WidgetHost + 'static),
            &mut w2 as *mut MockWidget as *mut (dyn WidgetHost + 'static),
            &mut w3 as *mut MockWidget as *mut (dyn WidgetHost + 'static),
        ];

        let _ = layout.layout(10.0, 20.0, 250.0, 300.0, &children, &mut dummy);

        assert_eq!(w1.x, 15.0);
        assert_eq!(w1.y, 25.0);
        assert!((w1.w - 126.315).abs() < 0.01);
        assert!((w1.h - 103.57).abs() < 0.01);

        assert!((w3.x - 153.947).abs() < 0.01);
        assert_eq!(w3.y, 25.0);
        assert!((w3.w - 101.05).abs() < 0.01);
        assert!((w3.h - 82.857).abs() < 0.01);
    }

    #[test]
    fn bevel_profile_lut_integrates_to_the_curves_net_rise() {
        // The identity 0→1 curve: slopes sum/N to its net rise of 1 (the same
        // total step the analytic smoothstep carries).
        crate::layout::set_bevel_profile_keys(&[(0.0, 0.0), (1.0, 1.0)], true);
        let slopes = crate::layout::bevel_profile_slopes().expect("profile installed");
        let n = crate::layout::BEVEL_PROFILE_SAMPLES as f32;
        let rise: f32 = slopes.iter().map(|s| s / n).sum();
        assert!((rise - 1.0).abs() < 0.001, "net rise {rise}");

        // A rim curve that returns to its start height nets zero.
        crate::layout::set_bevel_profile_keys(
            &[(0.0, 0.5), (0.2, 1.0), (0.8, 1.0), (1.0, 0.5)],
            false,
        );
        let slopes = crate::layout::bevel_profile_slopes().unwrap();
        let rise: f32 = slopes.iter().map(|s| s / n).sum();
        assert!(rise.abs() < 0.001, "net rise {rise}");

        // Degenerate key lists clear back to the analytic profile; the
        // generation moves on every change so renderers re-upload.
        let gen = crate::layout::bevel_profile_generation();
        crate::layout::set_bevel_profile_keys(&[(0.0, 1.0)], false);
        assert!(crate::layout::bevel_profile_slopes().is_none());
        assert!(crate::layout::bevel_profile_generation() > gen);
    }
}

