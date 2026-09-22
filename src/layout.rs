use crate::widget::WidgetHost;
use crate::context::UiContext;
use std::sync::RwLock;
use std::collections::HashMap;
use std::sync::OnceLock;

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

#[derive(Debug)]
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
            return;
        }
        #[cfg(not(test))]
        self.load_float(key, val);
    }

    pub fn set_len(&mut self, key: &str, len: crate::units::Len) {
        #[cfg(test)]
        {
            test_overlay::set_len(key, len);
            return;
        }
        #[cfg(not(test))]
        self.load_len(key, len);
    }

    pub fn set_string(&mut self, key: &str, val: String) {
        #[cfg(test)]
        {
            test_overlay::set_string(key, val);
            return;
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
                // The DE's relief material: canonical home style.surface.relief
                // (these shade every bevel/boss/recess in the toolkit — the
                // compositor never read them, so the old window_manager
                // spelling survives only as a compat alias).
                // `depth` is the light strength, not a length — `light` is
                // the honest spelling, `depth` the one every config has.
                "style.surface.relief.depth" | "style.surface.relief.light" | "window_manager.bevel_depth" => "bevel_depth",
                "style.surface.relief.width" | "window_manager.bevel_width" => "bevel_width",
                // The geometric heights, both lengths (unit-aware): a carve's
                // drop and the plate roll's rise. Unset = follow the width.
                "style.surface.relief.height" => "bevel_height",
                "style.surface.relief.edge_height" => "roll_height",
                // Ramp-spec strings for the custom wall/roll profiles
                // (written by cce-relief, installed by reload_config).
                "style.surface.relief.profile" => "bevel_profile_spec",
                "style.surface.relief.edge_profile" => "roll_profile_spec",
                // cce-relief's slider positions behind those specs
                // ("shoulder,base,bias" — only the editor reads these).
                "style.surface.relief.profile_knobs" => "bevel_profile_knobs",
                "style.surface.relief.edge_knobs" => "roll_profile_knobs",
                "style.container.section.depth" => "section_depth",
                "style.surface.param.backdrop_compression" => "param_compression",
                "style.surface.param.label_layout" => "param_label_layout",
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
                "style.surface.desktop.cell_fade_inset" => "desktop_cell_fade_inset",
                "style.surface.desktop.mode" => "desktop_mode",
                "style.surface.desktop.solid_color" => "desktop_solid_color",
                "style.surface.desktop.grid_cell_size" => "desktop_grid_scale",
                "style.surface.desktop.grid_cell_width" => "grid_cell_width",
                "style.surface.desktop.grid_cell_height" => "grid_cell_height",
                "style.surface.plate.padding" => "plate_padding",
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
                "style.surface.graph.cell_color" => "graph_cell_color",
                "style.surface.graph.gap_color" => "graph_gap_color",
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

/// Per-thread overrides for the style values tests pin, active only under
/// `cfg(test)`.
///
/// These values are process-global by design: the toolkit reads them from
/// config once and every widget sees the same style. That also makes them
/// shared mutable state BETWEEN tests, and `cargo test` runs tests on
/// parallel threads. `test_vstack_flow` pins a known font and margin so its
/// pixel assertions do not depend on the developer's config — and for as
/// long as it ran, every other test measuring text saw that font. That is
/// the whole "cce-ui parallel test flake": `a_members_wide_label_is_in_the_hull`
/// and two `layout::tests` siblings failing perhaps one run in three on a
/// clean tree, always green at `--test-threads=1`, and blamed on innocent
/// diffs for weeks.
///
/// A lock around the three known victims would have fixed those three. This
/// fixes the class: a `set_*` is invisible to tests running beside it, and a
/// future test that pins a style needs no lock and no discipline to remember.
/// Production is untouched — `cfg(test)` is set only while compiling this
/// crate's own unit tests, never for downstream crates.
#[cfg(test)]
mod test_style {
    use std::cell::RefCell;
    thread_local! {
        pub static CONTROL_LABEL_MARGIN: RefCell<Option<f32>> = const { RefCell::new(None) };
        pub static SECTION_PADDING: RefCell<Option<f32>> = const { RefCell::new(None) };
        pub static CONTROL_LABEL_FONT: RefCell<Option<String>> = const { RefCell::new(None) };
        pub static CONTROL_LABEL_FONT_DETACHED: RefCell<Option<String>> = const { RefCell::new(None) };
    }
}
/// The height every text-bearing control falls back to when its own
/// `style.control.<name>.height` is unset: button, toggle (and the checkbox
/// row), dropdown, textbox (and the keybind recorder), spinbox, font selector,
/// colour selector, button strip, breadcrumb. One number, so a form built from
/// defaults lines up; a per-control key is the deliberate exception.
pub const DEFAULT_CONTROL_HEIGHT: f32 = 24.0;

/// The one gap between controls — one control height — in BOTH axes: what every
/// layout strategy's `Default` puts between children's blocks (a detached label
/// and the control below it, see `WidgetHost::label_strip`) and around them, and
/// what the legacy row builders advance by. One number, one module, so the space
/// beside a control and the space below it read the same.
pub const CONTROL_GAP: f32 = DEFAULT_CONTROL_HEIGHT;

/// The one inset from a control's edge to its text: the field text of a TextBox,
/// Dropdown, Spinbox, FontSelector, KeybindRecorder or ColorSelector, a left-
/// justified Button or Toggle label, a Slider's readout. Fields in a column line
/// their text up because they all use this.
pub const CONTROL_TEXT_INSET: f32 = 8.0;

/// A carve that stays INSIDE its rect. A recess, boss or trough wall straddles the
/// rect edge it is given — half its depth outside — so a well carved at a widget's
/// rect edge painted past the widget's box, and the gap beside a well read up to
/// half a depth smaller than the gap beside a raised plate (whose roll is inside).
/// Every widget carves the rect this returns instead: inset by half the depth, the
/// radii reduced by the same so the OUTER silhouette keeps the configured radius.
/// The widget's footprint is then its rect, and the gap is the gap.
pub fn carve_inside(rect: crate::scene::layout::Rect, radii: crate::scene::paint::Radii, depth: f32) -> (crate::scene::layout::Rect, crate::scene::paint::Radii) {
    let g = depth * 0.5;
    let r = |r: f32| if r > 0.0 { (r - g).max(0.0) } else { 0.0 };
    (
        crate::scene::layout::Rect { x: rect.x + g, y: rect.y + g, width: (rect.width - depth).max(0.0), height: (rect.height - depth).max(0.0) },
        (r(radii.0), r(radii.1), r(radii.2), r(radii.3)),
    )
}

/// The one inset from a control's left edge to its detached label above it — the
/// x offset the adapter draws the label at, and the tab hugging that label in the
/// carve-out compositions (Slider, Dropdown, RangeSlider). Every labeled control
/// uses it, so a column of labels is one line.
pub const DETACHED_LABEL_INSET: f32 = 4.0;
/// The same for the track-shaped controls: slider, range slider, progress bar,
/// usage bar.
pub const DEFAULT_TRACK_HEIGHT: f32 = 16.0;

static SPINBOX_HEIGHT: RwLock<f32> = RwLock::new(DEFAULT_CONTROL_HEIGHT);
static SPINBOX_BUTTON_PADDING: RwLock<f32> = RwLock::new(0.0);
static COLOR_SELECTOR_HEIGHT: RwLock<f32> = RwLock::new(DEFAULT_CONTROL_HEIGHT);
static TEXTBOX_HEIGHT: RwLock<f32> = RwLock::new(DEFAULT_CONTROL_HEIGHT);
static FONT_SELECTOR_HEIGHT: RwLock<f32> = RwLock::new(DEFAULT_CONTROL_HEIGHT);
static SLIDER_HEIGHT: RwLock<f32> = RwLock::new(DEFAULT_TRACK_HEIGHT);
static PROGRESSBAR_HEIGHT: RwLock<f32> = RwLock::new(DEFAULT_TRACK_HEIGHT);
static RANGESLIDER_HEIGHT: RwLock<f32> = RwLock::new(DEFAULT_TRACK_HEIGHT);
static TOGGLE_HEIGHT: RwLock<f32> = RwLock::new(DEFAULT_CONTROL_HEIGHT);
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
static BUTTON_HEIGHT: RwLock<f32> = RwLock::new(DEFAULT_CONTROL_HEIGHT);
static RAMP_HEIGHT: RwLock<f32> = RwLock::new(32.0);
static BUTTON_STRIP_SPACING: RwLock<f32> = RwLock::new(8.0);
static SCROLLBAR_WIDTH: RwLock<f32> = RwLock::new(4.0);
static SCROLLBAR_INSET: RwLock<f32> = RwLock::new(16.0);
static COLUMN_GAP: RwLock<f32> = RwLock::new(16.0);
static CONTROL_PANEL_PADDING: RwLock<f32> = RwLock::new(16.0);
static CONTROL_PANEL_GAP: RwLock<f32> = RwLock::new(12.0);
static TREE_OPACITY: RwLock<f32> = RwLock::new(1.0);
static TREE_BLUR: RwLock<f32> = RwLock::new(0.0);

static PLATE_PADDING: RwLock<f32> = RwLock::new(20.0);
static DROPDOWN_HEIGHT: RwLock<f32> = RwLock::new(DEFAULT_CONTROL_HEIGHT);
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
                        registry.load_float(&key, f_val);
                    } else if let Some(len) = crate::units::Len::parse(val_str) {
                        // `(mm)2.0` arrived as the string `2mm`.
                        registry.load_len(&key, len);
                    } else {
                        registry.load_string(&key, val_str.to_string());
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
            if let Some(rest) = trimmed.strip_prefix("scrollbar_inset") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = SCROLLBAR_INSET.write() {
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
        // The configured relief profiles, applied last so every process (not
        // just the editor that wrote them) starts with the styled walls.
        apply_relief_profile_config();
    }
}

/// The untouched editor curve — the "analytic" sentinel in the config'd
/// profile specs (cce-designer's Edge Profile convention). For the wall curve
/// identity-smooth IS the analytic smoothstep, so skipping it changes
/// nothing; for the roll it would be a straight chamfer, not the analytic
/// superellipse quadrant, so it must read as "no custom profile".
pub const RELIEF_PROFILE_IDENTITY_SPEC: &str = "smooth;0.000:0.000,1.000:1.000";

/// Parse-and-install the relief profiles config carries as ramp specs
/// (`style.surface.relief.profile` / `edge_profile` → the style registry's
/// `bevel_profile_spec` / `roll_profile_spec`). Absent, identity, or
/// unparseable specs clear back to the analytic profiles.
fn apply_relief_profile_config() {
    let (wall, edge) = {
        let reg = get_style_registry().read().unwrap();
        (reg.get_string("bevel_profile_spec"), reg.get_string("roll_profile_spec"))
    };
    match parse_relief_profile_spec(wall.as_deref()) {
        Some((keys, smooth)) => set_bevel_profile_keys(&keys, smooth),
        None => clear_bevel_profile(),
    }
    match parse_relief_profile_spec(edge.as_deref()) {
        Some((keys, smooth)) => set_roll_profile_keys(&keys, smooth),
        None => clear_roll_profile(),
    }
}

/// A config'd profile spec → installable keys. `None` (falling back to the
/// analytic profile) for absent, identity-sentinel, or unparseable specs.
fn parse_relief_profile_spec(spec: Option<&str>) -> Option<(Vec<(f32, f32)>, bool)> {
    spec.filter(|s| *s != RELIEF_PROFILE_IDENTITY_SPEC).and_then(crate::widget::parse_ramp_spec)
}

/// Install (or clear back to analytic) the WALL profile from a ramp spec —
/// the entry point for a `(relief)` config value's profile
/// ([`crate::relief_spec::ReliefSpec`]): an app whose feature carries its
/// own material installs it process-wide here. Same identity/unparseable
/// filtering as the config path above.
pub fn install_wall_profile_spec(spec: Option<&str>) {
    match parse_relief_profile_spec(spec) {
        Some((keys, smooth)) => set_bevel_profile_keys(&keys, smooth),
        None => clear_bevel_profile(),
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
    #[cfg(test)]
    if let Some(v) = test_style::CONTROL_LABEL_MARGIN.with(|c| *c.borrow()) {
        return v;
    }
    *CONTROL_LABEL_MARGIN.read().unwrap()
}

pub(crate) fn control_label_strip() -> f32 {
    let (_, font_size) = control_label_font_detached_parsed();
    font_size + control_label_margin()
}

pub fn label_margin() -> f32 {
    control_label_margin()
}

pub fn set_control_label_margin(margin: f32) {
    #[cfg(test)]
    test_style::CONTROL_LABEL_MARGIN.with(|c| *c.borrow_mut() = Some(margin));
    #[cfg(not(test))]
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

/// The section carves' depth multiplier (`style.container.section.depth`,
/// default 1.0): scales the params pane's section-well wall — width and step
/// together — relative to the DE-wide relief material, so sections can read
/// deeper or shallower than the controls around them. Values past 1.0 let the
/// roll widen across the channel groove between the well wall and its packed
/// controls; tune to taste.
pub fn section_depth() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("section_depth").unwrap_or(1.0)
}

/// The parameter rows' backdrop compression
/// (`style.surface.param.backdrop_compression`, 0..1): when set, every
/// parameter row in a params pane is floored with the pane material frosted
/// at THIS compression before its controls paint, so each parameter sits on
/// a tablet that pulls the view toward the tint while the pane around it
/// stays at its own — the row-scale twin of the designer's node
/// compression. `None` (unset) draws no floor — the rows are bare, as they
/// always were.
pub fn param_compression() -> Option<f32> {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("param_compression").map(|v| v.clamp(0.0, 1.0))
}

/// Whether a params pane lays each row's label BESIDE its control
/// (`style.surface.param.label_layout = "inline"`, the default) or lets the
/// control carry it in the strip above itself (`"stacked"`, the layout every
/// row had before 2026-09-21). Inline, the pane owns the labels: it measures
/// a label column off the widest label, hands each control the rest of the
/// row, and the controls are built unlabelled — an unlabelled control takes
/// its whole rect (`WidgetHost::label_strip` is zero), so the row is one
/// control tall. Toggles and buttons carry their label as their own face and
/// are inline either way.
pub fn param_labels_inline() -> bool {
    lazy_init_style_registry();
    get_style_registry()
        .read()
        .unwrap()
        .get_string("param_label_layout")
        .map_or(true, |v| v.trim() != "stacked")
}

pub fn section_padding() -> f32 {
    #[cfg(test)]
    if let Some(v) = test_style::SECTION_PADDING.with(|c| *c.borrow()) {
        return v;
    }
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
    #[cfg(test)]
    test_style::SECTION_PADDING.with(|c| *c.borrow_mut() = Some(padding));
    #[cfg(not(test))]
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

/// The window silhouette's nominal corner radius: the SHARED config's
/// root plate corner_radius, never the per-app override. The compositor clips
/// every decorated window with this value (widened by
/// [`corner_span_factor`]), so any window-corner arc an app draws itself must
/// use it too — even when the app restyles its own plates through its
/// override file — or its corners detach from the silhouette (and from the
/// desktop grid's cells, which share the same knob).
pub fn window_corner_radius() -> f32 {
    crate::config::get_i64_shared("/style/surface/plate/root/corner_radius", 12) as f32
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
/// The window silhouette's EFFECTIVE corner radius: the shared nominal value
/// widened by the corner-span factor — exactly the arc the compositor clips
/// every decorated window with, and the radius a root plate's corners must
/// wear (RFC Phase 7b; `PlateSpec::radii_for` uses it for window-flagged
/// corners). Apps drawing root-surface geometry through non-Plate prims read
/// this scalar directly.
pub fn window_silhouette_radius() -> f32 {
    window_corner_radius() * corner_span_factor()
}

pub fn corner_span_factor() -> f32 {
    corner_span_factor_for(corner_shape())
}

/// The span factor for an explicit corner exponent — what a plate carrying
/// its own `shape` (see `scene::paint::Prim::Plate`) scales its radii by.
/// Same clamp as [`corner_shape`], so an override cannot reach an exponent
/// the shader would not accept.
pub fn corner_span_factor_for(n: f32) -> f32 {
    let n = n.clamp(2.0, 16.0);
    if n > 2.001 {
        (n - 1.0) * 2f32.powf(1.0 / n) / std::f32::consts::SQRT_2
    } else {
        1.0
    }
}

/// Whether the relief primitives (see `scene::paint::Prim`) render through
/// shader2d's per-pixel SDF-lit branch (the default) or the legacy banded vertex
/// shading. `bevel_shader 0` in config flips back to the old look for A/B
/// comparison — the key keeps the bevel name because it selects how the shared
/// lit EDGE is computed, not which shapes exist.
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

/// A carve's geometric drop when the material pins one
/// (`style.surface.relief.height`, a length — `(mm)0.3` resolves through
/// the display metric), in logical px. `None` = follow the wall width at the
/// analytic ratio ([`crate::scene::relief_shade::RECESS_DEPTH`]), the look
/// every config had before heights existed. A configured 0 reads as unset,
/// which is how an editor puts a material back on "follow".
pub fn bevel_height() -> Option<f32> {
    lazy_init_style_registry();
    get_style_registry()
        .read()
        .unwrap()
        .get_float("bevel_height")
        .filter(|h| h.is_finite() && *h > 0.0)
}

/// The plate roll's rise when pinned (`style.surface.relief.edge_height`, a
/// length), logical px. `None` = a quarter-round of radius `bevel_width`.
pub fn roll_height() -> Option<f32> {
    lazy_init_style_registry();
    get_style_registry()
        .read()
        .unwrap()
        .get_float("roll_height")
        .filter(|h| h.is_finite() && *h > 0.0)
}

/// The drop of a carve whose wall runs `wall` logical px: the pinned height
/// when there is one, else the analytic ratio of the wall — saturating at the
/// DE's roll width, so a wall wider than the plate's own perimeter roll
/// spreads the same step over a longer run (a softer transition) instead of
/// cutting proportionally deeper. The tessellator's CSG features and the
/// shader's free carves both derive from this rule.
pub fn carve_depth_px(wall: f32) -> f32 {
    match bevel_height() {
        Some(h) => h,
        None => crate::scene::relief_shade::RECESS_DEPTH * wall.min(bevel_width()),
    }
}

/// Drop over run for a wall of the DE roll width — what the shading twin
/// scales its slopes by.
pub fn carve_depth_ratio() -> f32 {
    let w = bevel_width().max(0.001);
    carve_depth_px(w) / w
}

/// Rise over run of the plate roll: 1 (the quarter-round) unless pinned.
pub fn roll_height_ratio() -> f32 {
    roll_height().map_or(1.0, |h| h / bevel_width().max(0.001))
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

/// Evaluate a ramp key list at `t` — THE ramp interpolation of the DE.
/// `cce_ui::widget::Ramp` draws it, `RampPreview` previews it, the relief
/// profile LUTs sample it, and cce-window-manager's camera speed ramp mirrors
/// it verbatim (that crate stays dependency-minimal), so a curve sculpted in
/// the widget is exactly the curve every consumer evaluates. Keys are
/// `(pos, value)` sorted by pos; outside the key range the end values hold.
///
/// `smooth` is the widget's curved line type: a **monotone cubic** through
/// the keys (Fritsch–Butland tangents, cubic Hermite segments) — C1, passes
/// through every key, never overshoots a key, and flattens only at the ends
/// and at genuine local extrema. It used to be a smoothstep blend PER
/// SEGMENT, which forces zero slope at every key: a curve with more than two
/// keys came out as a chain of little bumps, and a wall profile built from
/// it read as jagged and uneven where a smooth slope was drawn. A two-key
/// ramp is unchanged — zero tangents at both ends make the single Hermite
/// segment exactly the old smoothstep — so the identity sentinel and every
/// simple ease keep their look. `false` is straight segments.
pub fn sample_ramp_keys(keys: &[(f32, f32)], smooth: bool, t: f32) -> f32 {
    let Some(first) = keys.first() else { return 0.0 };
    let last = keys.last().unwrap();
    if t <= first.0 {
        return first.1;
    }
    if t >= last.0 {
        return last.1;
    }
    for i in 0..keys.len() - 1 {
        let ((x0, y0), (x1, y1)) = (keys[i], keys[i + 1]);
        if t < x0 || t > x1 {
            continue;
        }
        let h = x1 - x0;
        if h.abs() < 0.0001 {
            return y0;
        }
        let s = (t - x0) / h;
        if !smooth {
            return y0 + (y1 - y0) * s;
        }
        let (m0, m1) = (ramp_key_tangent(keys, i), ramp_key_tangent(keys, i + 1));
        let (s2, s3) = (s * s, s * s * s);
        let h00 = 2.0 * s3 - 3.0 * s2 + 1.0;
        let h10 = s3 - 2.0 * s2 + s;
        let h01 = -2.0 * s3 + 3.0 * s2;
        let h11 = s3 - s2;
        return h00 * y0 + h10 * h * m0 + h01 * y1 + h11 * h * m1;
    }
    first.1
}

/// The monotone cubic's tangent (dy/dpos) at key `i`: zero at either end and
/// at any local extremum (so the curve never overshoots a key), otherwise the
/// Fritsch–Butland weighted harmonic mean of the two neighbouring secants —
/// the shape-preserving choice, which keeps every segment monotone whenever
/// its keys are.
fn ramp_key_tangent(keys: &[(f32, f32)], i: usize) -> f32 {
    if i == 0 || i + 1 >= keys.len() {
        return 0.0;
    }
    let ((xp, yp), (x, y), (xn, yn)) = (keys[i - 1], keys[i], keys[i + 1]);
    let (h0, h1) = (x - xp, xn - x);
    if h0 <= 0.0001 || h1 <= 0.0001 {
        return 0.0;
    }
    let (d0, d1) = ((y - yp) / h0, (yn - y) / h1);
    if d0 * d1 <= 0.0 {
        return 0.0;
    }
    let (w0, w1) = (2.0 * h1 + h0, h1 + 2.0 * h0);
    (w0 + w1) / (w0 / d0 + w1 / d1)
}

#[cfg(test)]
mod ramp_sampling_tests {
    use super::sample_ramp_keys;

    #[test]
    fn two_key_smooth_is_exactly_smoothstep() {
        let keys = [(0.0, 0.0), (1.0, 1.0)];
        for i in 0..=20 {
            let t = i as f32 / 20.0;
            let ss = t * t * (3.0 - 2.0 * t);
            assert!((sample_ramp_keys(&keys, true, t) - ss).abs() < 1e-6, "t={t}");
        }
    }

    #[test]
    fn passes_through_every_key_and_holds_the_ends() {
        let keys = [(0.0, 0.0), (0.15, 0.45), (0.35, 0.7), (0.55, 0.78), (0.75, 0.85), (1.0, 1.0)];
        for &(p, v) in &keys {
            assert!((sample_ramp_keys(&keys, true, p) - v).abs() < 1e-6, "key {p}");
        }
        assert_eq!(sample_ramp_keys(&keys, true, -1.0), 0.0);
        assert_eq!(sample_ramp_keys(&keys, true, 2.0), 1.0);
    }

    #[test]
    fn monotone_keys_give_a_monotone_curve_without_wobble() {
        // The wall profile that came out as a chain of bumps under the old
        // per-segment smoothstep.
        let keys = [(0.0, 0.0), (0.15, 0.45), (0.35, 0.7), (0.55, 0.78), (0.75, 0.85), (1.0, 1.0)];
        let mut last = -1.0f32;
        let mut slopes = Vec::new();
        for i in 0..=400 {
            let t = i as f32 / 400.0;
            let v = sample_ramp_keys(&keys, true, t);
            assert!(v >= last - 1e-6, "not monotone at t={t}: {v} < {last}");
            slopes.push(v - last);
            last = v;
        }
        // No wobble: the slope at an INTERIOR key is a real slope, not the
        // zero the old blend pinned there (0.35: secants 1.25 and 0.4 either
        // side — the harmonic mean is well above half the smaller one).
        let dv = (sample_ramp_keys(&keys, true, 0.355) - sample_ramp_keys(&keys, true, 0.345)) / 0.01;
        assert!(dv > 0.3, "slope at key 0.35 is {dv}");
        // …and the slope never flips sign back and forth between keys: at
        // most one local slope maximum per segment would be a stretch to
        // assert, so pin the direct symptom — the curve stays inside the
        // key hull (no overshoot beyond the neighbouring key values).
        for i in 0..keys.len() - 1 {
            let (a, b) = (keys[i], keys[i + 1]);
            for j in 1..10 {
                let t = a.0 + (b.0 - a.0) * j as f32 / 10.0;
                let v = sample_ramp_keys(&keys, true, t);
                assert!(v >= a.1.min(b.1) - 1e-6 && v <= a.1.max(b.1) + 1e-6, "overshoot at t={t}: {v}");
            }
        }
    }

    #[test]
    fn a_peak_is_flat_at_the_peak_and_never_overshoots() {
        // The desktop overview speed ramp: rises then falls.
        let keys = [(0.0, 0.15), (0.4, 1.0), (1.0, 0.1)];
        assert!((sample_ramp_keys(&keys, true, 0.4) - 1.0).abs() < 1e-6);
        for i in 0..=100 {
            let v = sample_ramp_keys(&keys, true, i as f32 / 100.0);
            assert!(v <= 1.0 + 1e-6 && v >= 0.1 - 1e-6, "overshoot {v}");
        }
        let near = sample_ramp_keys(&keys, true, 0.39);
        assert!(near > 0.99, "flat at the extremum: {near}");
    }

    #[test]
    fn linear_is_untouched() {
        let keys = [(0.0, 0.0), (0.5, 1.0), (1.0, 0.0)];
        assert!((sample_ramp_keys(&keys, false, 0.25) - 0.5).abs() < 1e-6);
        assert!((sample_ramp_keys(&keys, false, 0.75) - 0.5).abs() < 1e-6);
    }
}

/// Install a custom bevel/carve profile from ramp keys (`(pos, value)`, both
/// 0..1, sorted by pos). Sampled into the slope LUT the shader's `carve_slope`
/// reads in place of its analytic smoothstep — every recess/boss/ridge wall in
/// this process restyles on the next frame. Empty or single-key lists clear
/// back to the analytic profile ([`clear_bevel_profile`]).
pub fn set_bevel_profile_keys(keys: &[(f32, f32)], smooth: bool) {
    let Some(lut) = ramp_profile_lut(keys, smooth) else {
        clear_bevel_profile();
        return;
    };
    *BEVEL_PROFILE.write().unwrap() = Some(lut);
    BEVEL_PROFILE_GEN.fetch_add(1, std::sync::atomic::Ordering::Release);
}

/// What a key list installs: its slope LUT, or `None` for a degenerate list
/// (fewer than two keys is no curve at all) — the caller clears back to the
/// analytic profile. The pure half of `set_*_profile_keys`.
fn ramp_profile_lut(keys: &[(f32, f32)], smooth: bool) -> Option<[f32; BEVEL_PROFILE_SAMPLES]> {
    if keys.len() < 2 {
        return None;
    }
    Some(ramp_slope_lut(keys, smooth))
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
    let Some(lut) = ramp_profile_lut(keys, smooth) else {
        clear_roll_profile();
        return;
    };
    *ROLL_PROFILE.write().unwrap() = Some(lut);
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
/// logical px (`style.surface.plate.root.padding` in config.kdl). DE-wide so
/// every app's content sits the same distance off the plate rim.
pub fn root_plate_padding() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("root_plate_padding").unwrap_or(16.0)
}

/// Gap between sibling objects on the window plate, in logical px
/// (`style.surface.plate.root.gap`) — pane splits, control rows. The companion to [`root_plate_padding`]:
/// rim distance vs object spacing.
pub fn root_plate_gap() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("root_plate_gap").unwrap_or(12.0)
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
/// TextBox/Slider wells, recessed MenuBar/StatusBar bands. Widgets read this
/// LIVE, at paint and layout, so [`set_control_relief`] restyles every control
/// in the process at once; the per-widget `with_raised` / `with_recessed` /
/// `with_recess` builders pin one widget either way. `0` reverts the whole DE
/// to the flat look.
pub fn control_relief() -> bool {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("control_relief").map(|v| v != 0.0).unwrap_or(true)
}

/// Switch the controls' relief styling at runtime — the gallery's Style
/// dropdown. Every widget without a per-widget override follows on its next
/// paint; the caller asks for a rebuild. Not persisted: a config reload puts
/// the configured value back.
pub fn set_control_relief(relief: bool) {
    lazy_init_style_registry();
    get_style_registry().write().unwrap().set_float("control_relief", if relief { 1.0 } else { 0.0 });
}

pub fn toggle_corner_radius() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("toggle_corner_radius").unwrap_or_else(control_corner_radius)
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
    get_style_registry().read().unwrap().get_float("slider_corner_radius").unwrap_or_else(control_corner_radius)
}

/// The slider band's knobs (`style.control.slider.*`): the flat band's thickness,
/// and the swell's half-span / peak height around the value position. The band —
/// a thin full-range band that swells at the value — is the one slider style.
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


pub fn plate_corner_radius() -> f32 {
    lazy_init_style_registry();
    let r = get_style_registry().read().unwrap();
    r.get_float("plate_corner_radius")
        .or_else(|| r.get_float("root_plate_corner_radius"))
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
    #[cfg(test)]
    if let Some(v) = test_style::CONTROL_LABEL_FONT.with(|c| c.borrow().clone()) {
        return v;
    }
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
    // The parse cache is process-wide, so under test it would hand back one
    // thread's pinned font to every other. Parsing is cheap; skip it there.
    #[cfg(not(test))]
    if let Ok(lock) = CONTROL_LABEL_FONT_CACHED.read() {
        if let Some(ref val) = *lock {
            return val.clone();
        }
    }
    let font_str = control_label_font();
    let parsed = parse_font_string(&font_str);
    let size = parsed.1.unwrap_or(12.0);
    let val = (parsed.0, size);
    #[cfg(not(test))]
    if let Ok(mut lock) = CONTROL_LABEL_FONT_CACHED.write() {
        *lock = Some(val.clone());
    }
    val
}

pub fn set_control_label_font(font: &str) {
    #[cfg(test)]
    test_style::CONTROL_LABEL_FONT.with(|c| *c.borrow_mut() = Some(font.to_string()));
    #[cfg(not(test))]
    if let Ok(mut lock) = CONTROL_LABEL_FONT.write() {
        *lock = font.to_string();
    }
    #[cfg(not(test))]
    if let Ok(mut lock) = CONTROL_LABEL_FONT_CACHED.write() {
        *lock = None;
    }
}

// Control Label Font Detached
pub fn control_label_font_detached() -> String {
    #[cfg(test)]
    if let Some(v) = test_style::CONTROL_LABEL_FONT_DETACHED.with(|c| c.borrow().clone()) {
        return v;
    }
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
    // The parse cache is process-wide, so under test it would hand back one
    // thread's pinned font to every other. Parsing is cheap; skip it there.
    #[cfg(not(test))]
    if let Ok(lock) = CONTROL_LABEL_FONT_DETACHED_CACHED.read() {
        if let Some(ref val) = *lock {
            return val.clone();
        }
    }
    let font_str = control_label_font_detached();
    let parsed = parse_font_string(&font_str);
    let size = parsed.1.unwrap_or(12.0);
    let val = (parsed.0, size);
    #[cfg(not(test))]
    if let Ok(mut lock) = CONTROL_LABEL_FONT_DETACHED_CACHED.write() {
        *lock = Some(val.clone());
    }
    val
}

pub fn set_control_label_font_detached(font: &str) {
    #[cfg(test)]
    test_style::CONTROL_LABEL_FONT_DETACHED.with(|c| *c.borrow_mut() = Some(font.to_string()));
    #[cfg(not(test))]
    if let Ok(mut lock) = CONTROL_LABEL_FONT_DETACHED.write() {
        *lock = font.to_string();
    }
    #[cfg(not(test))]
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
    // Unset: the swatch rounds like the text field beside it (the TextBox radius).
    get_style_registry().read().unwrap().get_float("color_selector_preview_corner_radius").unwrap_or_else(textbox_corner_radius)
}

pub fn set_color_selector_preview_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("color_selector_preview_corner_radius", radius);
    }
}

pub fn color_selector_corner_radius() -> f32 {
    lazy_init_style_registry();
    // Unset: the selector's frame rounds like the text field it stands in for.
    get_style_registry().read().unwrap().get_float("color_selector_corner_radius").unwrap_or_else(textbox_corner_radius)
}

pub fn set_color_selector_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("color_selector_corner_radius", radius);
    }
}

/// The control rung's corner radius (`style.control.corner_radius`): the
/// default every control-scale radius getter falls back to when the widget's
/// own `corner_radius` key is unset — buttons, dropdowns, font selectors,
/// sliders, spinboxes, text boxes, toggles, and the list and tree wells. The
/// per-widget keys are overrides on top of it. See "Plates, wells and seams"
/// in `CLAUDE.md`; the pane and root rungs are `plate_corner_radius` and
/// `crate::color::root_plate_corner_radius`.
pub fn control_corner_radius() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("control_corner_radius").unwrap_or(8.0)
}

pub fn set_control_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("control_corner_radius", radius);
    }
}

pub fn button_corner_radius() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("button_corner_radius").unwrap_or_else(control_corner_radius)
}

pub fn set_button_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("button_corner_radius", radius);
    }
}

pub fn spinbox_corner_radius() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("spinbox_corner_radius").unwrap_or_else(control_corner_radius)
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

/// How far a page-level scrollbar stands off its window/plate right edge — the
/// designer parameter-pane look (config `style.control.scrollbar.inset`).
/// Framed inner lists keep their own tight 4px hug; this is for bars floating
/// over a plate.
pub fn scrollbar_inset() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("scrollbar_inset") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = SCROLLBAR_INSET.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *SCROLLBAR_INSET.read().unwrap()
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
    get_style_registry().read().unwrap().get_float("textbox_corner_radius").unwrap_or_else(control_corner_radius)
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


pub fn list_corner_radius() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("list_corner_radius").unwrap_or_else(control_corner_radius)
}

pub fn set_list_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("list_corner_radius", radius);
    }
}

pub fn tree_corner_radius() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("tree_corner_radius").unwrap_or_else(control_corner_radius)
}

pub fn set_tree_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("tree_corner_radius", radius);
    }
}

/// The graph grid's pitch along x: the distance from the centre of one
/// vertical grid line to the centre of the next. It is the grid's ONE size
/// per axis — nodes are centred on the lattice intersections. The node body
/// has a size of its own (`graph_node_width` / `graph_node_height`), so a
/// denser grid does not shrink the nodes. The pitch replaced a cell size
/// plus a gap (`spacing_*` was the cell, `gap_col_w` / `gap_row_h` the gap,
/// and a step was the two added up); the defaults are what those two used
/// to add up to, so a config that set neither draws the same lattice it did.
pub fn graph_spacing_x() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("graph_spacing_x").unwrap_or(187.5)
}

pub fn set_graph_spacing_x(spacing: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("graph_spacing_x", spacing);
    }
}

/// The graph grid's pitch along y — see [`graph_spacing_x`].
pub fn graph_spacing_y() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("graph_spacing_y").unwrap_or(112.5)
}

pub fn set_graph_spacing_y(spacing: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("graph_spacing_y", spacing);
    }
}

/// The drawn width of a graph grid line, in logical px. The pitch is
/// measured centre to centre, so this changes how heavy the lattice looks
/// and nothing about where anything sits. Not scaled by zoom — a lattice is
/// a reference, not a thing in the scene.
pub fn graph_line_width() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("graph_line_width").unwrap_or(1.0)
}

pub fn set_graph_line_width(width: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("graph_line_width", width);
    }
}

/// The node body's width at 100% zoom (`style.surface.graph.node.width`),
/// independent of the grid pitch: a node is a thing of its own size sitting
/// on a crossing, and the grid is a reference under it. The default is the
/// cell the old grid gave a node.
pub fn graph_node_width() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("graph_node_width").unwrap_or(150.0)
}

pub fn set_graph_node_width(width: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("graph_node_width", width);
    }
}

/// The node body's height at 100% zoom — see [`graph_node_width`].
pub fn graph_node_height() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("graph_node_height").unwrap_or(75.0)
}

pub fn set_graph_node_height(height: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("graph_node_height", height);
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
    get_style_registry().read().unwrap().get_float("font_selector_corner_radius").unwrap_or_else(control_corner_radius)
}

pub fn set_font_selector_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("font_selector_corner_radius", radius);
    }
}

/// The context menu's corner radius (`style.surface.menu.corner_radius`),
/// falling back to the control rung's. It used to take the pane radius
/// (`plate_corner_radius`, 12 in the shipped config), which is a corner
/// too wide for a surface whose labels sit 8px in from its edge: the first
/// row's text ran off the plate through the arc. A menu is a popover, and
/// its corner belongs to the control scale, like the dropdown's.
pub fn menu_corner_radius() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("menu_corner_radius").unwrap_or_else(control_corner_radius)
}

pub fn dropdown_corner_radius() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("dropdown_corner_radius").unwrap_or_else(control_corner_radius)
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



/// One step carve a widget's `paint` draws, handed to a flat-path host through
/// [`RenderTarget::relief_carve`] so it can re-emit it as a real prim.
///
/// Geometry always comes from the WIDGET (`TextBox::well`, `Toggle::well` /
/// `Toggle::slide_plate`), never re-derived here — a second copy of that math
/// in the bridge is exactly how the flat host's carve and the drawn one drift
/// apart.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReliefCarve {
    pub kind: CarveKind,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Per-corner radii, clockwise from top-left.
    pub radii: (f32, f32, f32, f32),
    /// Full width of the step's transition band.
    pub depth: f32,
    /// Which walls the carve has (top, right, bottom, left). A suppressed wall
    /// means the step runs flush to its neighbour there — a Spinbox's field
    /// running into its button column, where the two meet in ONE step rather
    /// than two facing walls.
    pub edges: (bool, bool, bool, bool),
}

/// Which way a [`ReliefCarve`] steps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CarveKind {
    /// Interior one step DOWN ([`crate::scene::paint::PaintCtx::recess_edges`]).
    /// `tint` lights the rim in the focus accent (`recess_tinted`).
    Recess { tint: Option<[f32; 3]> },
    /// Interior one step UP ([`crate::scene::paint::PaintCtx::boss_edges`]).
    Boss { tint: Option<[f32; 3]> },
    /// A FLUSH inset ([`crate::scene::paint::PaintCtx::trough_edges`]): the
    /// interior stays level with the surface and a valley seam runs the
    /// boundary — the closed dropdown's chrome, for a control that is part
    /// of the plate rather than a step up or down from it.
    Trough,
}

impl ReliefCarve {
    /// This carve shifted vertically — the page-scroll adjustment a host
    /// applies when it re-emits collected carves.
    pub fn shifted_y(self, dy: f32) -> Self {
        Self { y: self.y + dy, ..self }
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
    /// A flush inset control plate ([`PaintCtx::inset_plate`]) — the raised
    /// control surface (groove ring down, beveled lip back up). Lets a popover
    /// draw the ACTUAL widget surface expanded (the Dropdown's grown trigger).
    /// Hosts without relief prims degrade to a flat rounded fill.
    fn inset_plate(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32, radius: f32, _depth: f32) {
        self.rect_with_radius(color, x, y, w, h, radius);
    }
    /// [`inset_plate`](Self::inset_plate) with the rim lit — the focused
    /// control plate's ring (`ControlPlate::with_tint`). Hosts without relief
    /// prims draw the plain plate.
    fn inset_plate_tinted(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32, radius: f32, depth: f32, _tint: [f32; 3]) {
        self.inset_plate(color, x, y, w, h, radius, depth);
    }
    /// One step carve from a widget's `paint` ([`ReliefCarve`]) — offered here
    /// for the same reason as `inset_plate`: the legacy `all_quads` stream
    /// carries no relief prims, so a flat-path host never sees them.
    ///
    /// The default is deliberately a NO-OP, not a fill. These controls have
    /// transparent faces by design (the host surface IS the well floor / the
    /// plate's face), so the carve is their entire decoration — a host that
    /// can't carve has nothing truthful to draw, and a solid box here would
    /// paint every text field a flat slab it never had.
    fn relief_carve(&mut self, _carve: &ReliefCarve) {}
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
    // The flat-host contract, the same block `set_rect` takes: `(x, y)` is the top of
    // the detached label and `wh` the block height, label strip included. `layout`
    // takes the CONTENT origin and height, so step down by the strip.
    let strip = w.label_strip();
    let content_h = (wh - strip).max(0.0);
    w.layout(crate::widget::Point { x, y: y + strip }, crate::widget::LayoutConstraints::new(ww, ww, content_h, content_h), ctx);

    // Shape, which on this path nobody else does. A flat host consumes
    // `all_quads`, so `prepare_text` — where a TextBox records the per-glyph x
    // offsets its selection highlight, caret and click->index mapping all read
    // — was never called for the widgets it draws. Those three then fell back
    // to `measure_text_width("M")`, an SVG-rasterized INKED extent rather than
    // an advance, so the highlight under-ran the glyphs by a few px per
    // character (a full glyph by the end of "example.com"). Hosts that shape
    // for themselves (cce-files, the TreeList) just re-read the shared buffer
    // cache here.
    if let Ok(mut fs) = crate::geometry_font_system().lock() {
        w.prepare_text(&mut fs);
    }
    let (style_r, corners) = w.corner_style();
    let r = if corners != (false, false, false, false) { style_r } else { 0.0 };
    let (wx, mut wy, www, mut whh) = w.rect();
    let top_room = w.label_strip();
    wy += top_room;
    whh -= top_room;

    // ONE ordered replay of the paint walk — the same walk the live display-
    // list render runs (`scene::painter`), every prim in the order the widget
    // painted it, each mapped onto the flat host's RenderTarget surface.
    //
    // This used to be three passes over three typed views of the same paint:
    // the relief prims (downcast per widget type and re-derived from the
    // widget's accessors — the Dropdown's inset plate, the TextBox's well, the
    // Button's face, the Toggle's faces and steps), then every plain quad
    // (`all_quads`), then every rounded quad (`all_rounded_quads`). Splitting
    // one paint into typed streams loses the order between them, and the
    // order is the picture: a TextBox draws its rounded background, carves
    // its well, THEN lays the selection highlight and caret on top — the
    // three-pass replay put the well under the highlight and the background
    // over both. A Dropdown's hovered row is drawn after its menu plate; a
    // host that replays plates after rects buries the highlight. The prim
    // walk keeps the widget's order, covers every widget instead of the four
    // that had a special case, and carries the relief prims' own per-corner
    // radii and depth (the re-derivations rounded those off).
    //
    // Mapping onto the tuple surface: Quad and RoundedRect are the two
    // native fills (the root's plain background keeps its solid-border
    // expansion and window-corner resolution); a zero-stroke Border is an
    // inset plate's FACE and is held until the Trough that follows it, so the
    // pair reaches the host as ONE `inset_plate` call (a relief host carves
    // it for real; the default degrades to the flat fill); Recess/Boss go to
    // `relief_carve`; a Bevel degrades to its fill — what a flat host can
    // draw of a raised plate. Ridges, circles, arcs, vectors and images have
    // no flat-surface counterpart and are skipped, as they always were.
    let solid_border = w.solid_border();
    let mut text_scratch = crate::scene::paint::PaintCtx::new();
    crate::scene::painter::paint_root_into(&*ctx, &*w, &mut text_scratch);

    // A zero-stroke Border waiting for its Trough: (rect, radii, fill).
    let mut pending_face: Option<(crate::scene::layout::Rect, crate::scene::paint::Radii, [f32; 4])> = None;
    fn same_rect(a: crate::scene::layout::Rect, b: crate::scene::layout::Rect) -> bool {
        (a.x - b.x).abs() < 0.1 && (a.y - b.y).abs() < 0.1 && (a.width - b.width).abs() < 0.1 && (a.height - b.height).abs() < 0.1
    }
    fn emit_rounded(pc: &mut dyn RenderTarget, rect: crate::scene::layout::Rect, radii: crate::scene::paint::Radii, color: [f32; 4]) {
        let (r1, r2, r3, r4) = radii;
        let radius = r1.max(r2).max(r3).max(r4);
        let mask = (r1 > 0.0, r2 > 0.0, r3 > 0.0, r4 > 0.0);
        pc.rect_with_radius_corners(color, rect.x, rect.y, rect.width, rect.height, radius, mask);
    }

    for item in text_scratch.finish().items {
        use crate::scene::paint::Prim;
        let trough_for_face = match (&item.prim, &pending_face) {
            (Prim::Trough { rect, .. }, Some((face_rect, _, _))) => same_rect(*rect, *face_rect),
            _ => false,
        };
        if !trough_for_face {
            if let Some((frect, fradii, fill)) = pending_face.take() {
                emit_rounded(pc, frect, fradii, fill);
            }
        }
        match item.prim {
            Prim::Quad { rect, color: qc } => {
                let (qx, qy, qw, qh) = (rect.x, rect.y, rect.width, rect.height);
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

                // The widget's own background quad with a solid border: full-size
                // border quad, then the inset background over it.
                let mut border_drawn = false;
                let is_bg_quad = (qx - wx).abs() < 0.1 && (qy - wy).abs() < 0.1 && (qw - www).abs() < 0.1 && (qh - whh).abs() < 0.1;
                if is_bg_quad {
                    if let Some((border_color, thickness)) = solid_border {
                        if thickness > 0.0 {
                            pc.rect_with_radius_corners(border_color, qx, qy, qw, qh, resolved_r, resolved_corners);
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
            Prim::RoundedRect { rect, radius, corners: qcorners, color: qc } => {
                pc.rect_with_radius_corners(qc, rect.x, rect.y, rect.width, rect.height, radius, qcorners);
            }
            Prim::Border { rect, radii, fill, border, thickness } => {
                if thickness > 0.0 && border[3].abs() > 0.001 {
                    emit_rounded(pc, rect, radii, border);
                    if fill[3].abs() > 0.001 {
                        let inner = crate::scene::layout::Rect {
                            x: rect.x + thickness,
                            y: rect.y + thickness,
                            width: (rect.width - 2.0 * thickness).max(0.0),
                            height: (rect.height - 2.0 * thickness).max(0.0),
                        };
                        let (r1, r2, r3, r4) = radii;
                        let shrink = |v: f32| if v > 0.0 { (v - thickness).max(0.0) } else { 0.0 };
                        emit_rounded(pc, inner, (shrink(r1), shrink(r2), shrink(r3), shrink(r4)), fill);
                    }
                } else if fill[3].abs() > 0.001 {
                    // abs(): a negative alpha is the frost sentinel, a real face.
                    pending_face = Some((rect, radii, fill));
                }
            }
            Prim::Trough { rect, radii, depth, tint, .. } => {
                let face = pending_face.take().map(|(_, _, fill)| fill).unwrap_or([0.0; 4]);
                let (r1, r2, r3, r4) = radii;
                let r = r1.max(r2).max(r3).max(r4);
                match tint {
                    Some(t) => pc.inset_plate_tinted(face, rect.x, rect.y, rect.width, rect.height, r, depth, t),
                    None => pc.inset_plate(face, rect.x, rect.y, rect.width, rect.height, r, depth),
                }
            }
            Prim::Bevel { rect, radii, material, .. } => {
                let color = material.fill(crate::scene::material::PlateRole::Nested);
                if color[3].abs() > 0.001 {
                    emit_rounded(pc, rect, radii, color);
                }
            }
            Prim::Recess { rect, radii, depth, edges, tint } => {
                pc.relief_carve(&ReliefCarve {
                    kind: CarveKind::Recess { tint },
                    x: rect.x,
                    y: rect.y,
                    w: rect.width,
                    h: rect.height,
                    radii,
                    depth,
                    edges,
                });
            }
            Prim::Boss { rect, radii, depth, edges, tint } => {
                pc.relief_carve(&ReliefCarve {
                    kind: CarveKind::Boss { tint },
                    x: rect.x,
                    y: rect.y,
                    w: rect.width,
                    h: rect.height,
                    radii,
                    depth,
                    edges,
                });
            }
            // Text via the paint walk: each widget's Text prims (content font + scroll-ancestor
            // clip) exactly as the live display-list render does; the prim already carries the
            // per-widget font + bounds.
            Prim::Text { text, x, y, font_size, color, font, bounds, .. } => {
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
            _ => {}
        }
    }
    if let Some((frect, fradii, fill)) = pending_face.take() {
        emit_rounded(pc, frect, fradii, fill);
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
    // Registry sweep for open popovers the app never registered (popover
    // registration is optional and spotty) — the same fallback the coverage
    // check and the engine's outside-press close use.
    for (id, ptr) in ctx.tree.iter_registered() {
        if ctx.active_popovers.contains(&id) {
            continue;
        }
        unsafe {
            if let Some(w) = ptr.as_ref() {
                if w.visible() && w.popover_rect().is_some() {
                    w.render_popover(pc);
                }
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
        let top_room = w.label_strip();
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
            spacing: CONTROL_GAP,
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

    /// Horizontal inset of content from this section's left edge — the same
    /// one `row_layout`, the column `Grid` and `widget` use, so everything in
    /// a section lines up. See `SectionContext::content_margin`.
    pub fn content_margin(&self) -> f32 {
        2.0 * self.padding() + 12.0
    }

    pub fn content_left(&self) -> f32 {
        self.left + self.content_margin()
    }

    pub fn content_width(&self) -> f32 {
        (self.cw - 2.0 * self.content_margin()).max(0.0)
    }

    /// See `SectionContext::ax` — same mapping, same reason it is no longer
    /// stepped at `x_off == 12.0`.
    pub fn ax(&self, x_off: f32) -> f32 {
        self.left + 2.0 * self.padding() + x_off
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
        let x = self.ax(x_off);
        let y = self.ay() + y_off;
        // Bounded to the content box, like `SectionContext::text` — text is
        // the only thing a section draws that is not already sized to fit it.
        let right = self.content_left() + self.content_width();
        let bounds = if right > x {
            Some([x, y - font_size, right, y + 2.0 * font_size])
        } else {
            None
        };
        pc.text_with_bounds(text, x, y, font_size, color, bounds);
    }

    pub fn widget<T: WidgetHost + 'static>(&mut self, pc: &mut dyn RenderTarget, w: &mut T, _x_off: f32, _ww: f32, mut wh: f32, ctx: &mut UiContext) {
        if let Some(pref) = w.preferred_height() {
            wh = pref;
        }
        let pad = self.padding();
        let top_room = w.label_strip();
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
        let margin_x = self.content_margin();
        let usable_w = self.content_width();
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

    fn layout(&self, x: f32, y: f32, w: f32, h: f32, children: &[*mut (dyn crate::widget::WidgetHost + 'static)], ctx: &mut crate::context::UiContext) -> f32 {
        let (cur_x, cur_y) = (x, y);
        // Blocks: the label row (`label_lead`) above every child's content, the
        // gap between blocks, so a row's controls are level and a carve-out tab
        // sits a full gap from its neighbour.
        let lead = crate::widget::container::container_layout::label_lead(children);
        match self.direction {
            FlexDirection::Row => {
                let mut cur_x = cur_x;
                for &child_ptr in children {
                    unsafe {
                        let child = &mut *child_ptr;
                        let child_w = child.rect().2;
                        let child_h = crate::widget::container::container_layout::content_height(child);
                        let use_h = if child_h > 0.0 { child_h } else { h };
                        child.layout(
                            crate::widget::Point { x: cur_x, y: cur_y + lead },
                            crate::widget::LayoutConstraints::new(child_w, child_w, use_h, use_h),
                            ctx,
                        );
                        cur_x += child_w + self.spacing;
                    }
                }
                (cur_x - x).max(0.0)
            }
            FlexDirection::Column => {
                let mut cur_y = cur_y;
                for &child_ptr in children {
                    unsafe {
                        let child = &mut *child_ptr;
                        let child_h = crate::widget::container::container_layout::content_height(child);
                        let use_h = if child_h > 0.0 { child_h } else { 44.0 };
                        child.layout(
                            crate::widget::Point { x, y: cur_y + lead },
                            crate::widget::LayoutConstraints::new(w, w, use_h, use_h),
                            ctx,
                        );
                        cur_y += lead + use_h + self.spacing;
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

/// Vertical slack for a section's content clip. The clip exists to stop
/// content escaping its section SIDEWAYS, which is the axis a section's width
/// actually fixes; a section's height is only known once its content has been
/// placed, so bounding that axis too would risk cutting content off rather
/// than keeping it in. Deliberately far larger than any section.
const SECTION_CLIP_SLACK: f32 = 100_000.0;

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
        let (clip_x, clip_w) = (real_ctx.content_left(), real_ctx.content_width());
        real_ctx.pc.push_clip_rect(clip_x, ry - SECTION_CLIP_SLACK, clip_w, 2.0 * SECTION_CLIP_SLACK);
        render_fn(&mut real_ctx);
        real_ctx.pc.pop_clip_rect();
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
        let (clip_x, clip_w) = (real_ctx.content_left(), real_ctx.content_width());
        real_ctx.pc.push_clip_rect(clip_x, ry - SECTION_CLIP_SLACK, clip_w, 2.0 * SECTION_CLIP_SLACK);
        render_fn(&mut real_ctx);
        real_ctx.pc.pop_clip_rect();
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
    /// Whether the host renders sections as sunken wells (`section_relief_style`).
    pub relief_style: bool,
    /// The title tab box (x, y, w, h) when the host's relief styling laid the
    /// label out left-aligned — `finish` offers it with the section carve.
    /// None under relief styling means a label-less section: the well carves
    /// tabless, flush with the allocation top.
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
            relief_style,
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

    /// The section's content-box top edge: the body well's top (the tab's
    /// bottom) under relief styling, the outline's border line otherwise. Lets
    /// a page place content at an exact inset from the well's walls.
    pub fn well_top(&self) -> f32 {
        if self.relief_style {
            self.relief_tab.map(|t| t.1 + t.3).unwrap_or(self.top)
        } else {
            self.top + 7.0
        }
    }

    pub fn set_row_gap(&mut self, gap: f32) {
        self.row_gap = gap;
    }

    /// Horizontal inset of section CONTENT from the section's left edge.
    ///
    /// One number, used by every content placer in here — `row_layout`, the
    /// column `Grid`, `widget`, `VStack` and `ax` (so `text`) — because they
    /// share a section and have to line up inside it. The section's border is
    /// drawn at `left + padding()` (see `finish`), so content clears the
    /// border by `padding() + 12`.
    pub fn content_margin(&self) -> f32 {
        2.0 * self.padding() + 12.0
    }

    /// Left edge of the content box: where a row, a widget or a `text(_, 12.0,
    /// ..)` starts.
    pub fn content_left(&self) -> f32 {
        self.left + self.content_margin()
    }

    /// Width of the content box — the section's width less the inset on both
    /// sides. Nothing a section draws should extend past `content_left() +
    /// content_width()`.
    pub fn content_width(&self) -> f32 {
        (self.cw - 2.0 * self.content_margin()).max(0.0)
    }

    /// `x_off` px into the content box's coordinate space, where 12.0 is the
    /// content's own left edge — the offset 46 of the ~55 call sites in the
    /// tree already pass, and the one that lines text up with the buttons and
    /// widgets beside it.
    ///
    /// This used to add `padding()` only when `x_off >= 12.0`, which made the
    /// mapping DISCONTINUOUS: asking for 11 instead of 12 moved the text 5px
    /// LEFT rather than 1px, and silently dropped it out of alignment with
    /// every row in the same section. `cce-mail` and two others sit on the
    /// wrong side of that cliff today.
    pub fn ax(&self, x_off: f32) -> f32 {
        self.left + 2.0 * self.padding() + x_off
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
        let x = self.ax(x_off);
        // Bound it to the content box. A section's text was drawn unbounded,
        // so a string wider than its section simply kept going — over the
        // border, over whatever sat to the right, and off the window (the
        // settings app's GPU names did all three). Rows and widgets have
        // always been sized to the section; text was the one thing that could
        // leave it. The vertical band is generous on purpose: it is the
        // horizontal overrun that has to be cut, and a tight band would
        // shave descenders.
        let right = self.content_left() + self.content_width();
        let bounds = if right > x {
            Some([x, y - font_size, right, y + 2.0 * font_size])
        } else {
            None
        };
        self.pc.text_with_bounds(text, x, y, font_size, color, bounds);
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
        let top_room = w.label_strip();
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
        let margin_x = self.content_margin();
        let usable_w = self.content_width();
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

    /// A row whose columns are sized to what goes IN them: each gets the width
    /// it asked for in `needs`, and whatever is left over is shared equally.
    ///
    /// `row_layout` splits a row evenly and knows nothing about content, so it
    /// hands "Reboot" and "Hibernate" the same width — one floats in slack
    /// while the other is cut off, which is what a row of mismatched labels
    /// looks like. Sharing the SLACK equally rather than sizing proportionally
    /// is deliberate: proportional widths would make a two-character label a
    /// sliver, where what is wanted is "everyone fits, then everyone gets the
    /// same bonus".
    ///
    /// When the needs do not fit, every column is scaled by the same factor, so
    /// the row still cannot overflow its section and the shortfall is shared
    /// rather than landing entirely on the last column.
    pub fn row_layout_for(&self, needs: &[f32], gap: f32) -> Vec<(f32, f32)> {
        let count = needs.len();
        if count == 0 {
            return Vec::new();
        }
        let total_gap = gap * (count - 1) as f32;
        let room = (self.content_width() - total_gap).max(0.0);
        let total_need: f32 = needs.iter().map(|n| n.max(0.0)).sum();

        let widths: Vec<f32> = if total_need <= room {
            let extra = (room - total_need) / count as f32;
            needs.iter().map(|n| n.max(0.0) + extra).collect()
        } else if total_need > 0.0 {
            let scale = room / total_need;
            needs.iter().map(|n| n.max(0.0) * scale).collect()
        } else {
            vec![room / count as f32; count]
        };

        let mut cols = Vec::with_capacity(count);
        let mut x = self.content_left();
        for w in widths {
            cols.push((x, w));
            x += w + gap;
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

        if self.relief_style {
            // Sunken style: the well spans the full allocated rect — body top
            // edge at the tab's bottom (the tab is flush ON the body, the
            // designer union shape; label-less sections carve tabless from the
            // allocation top), walls on the allocation's edges, and no
            // trailing slack so the layout gap IS the visual gap.
            let body_y = self.relief_tab.map(|t| t.1 + t.3).unwrap_or(self.top);
            let frame = SectionFrame {
                x: self.left,
                y: body_y,
                w: self.cw,
                h: bottom - body_y,
                tab: self.relief_tab,
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
        let top_room = w.label_strip();
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

    /// [`add_row`](Self::add_row) with per-column widths from `needs` — see
    /// [`SectionContext::row_layout_for`]. For a row of buttons, `needs` is
    /// each label's measured width plus the plate's own inset.
    pub fn add_row_for<F>(&mut self, needs: &[f32], gap: f32, h: f32, mut f: F)
    where
        F: FnMut(&mut SectionContext<'a, P>, usize, f32, f32),
    {
        let max_h = self.context.grid.max_height().max(self.context.content_y);
        for col_h in &mut self.context.grid.col_heights {
            *col_h = max_h;
        }
        self.context.content_y = max_h;

        let cols = self.context.row_layout_for(needs, gap);
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
    #[test]
    fn unit_slots_resolve_through_the_metric_at_read_time() {
        use crate::units::{Len, Metric, MetricSource};
        let mut reg = super::StyleRegistry::new();
        reg.set_len("probe_width", Len::mm(2.0));
        // `get_float` resolves against the PROCESS metric at read time — the
        // same number `Len::to_px` gives — so it tracks whatever the metric
        // is now, not what it was at load. (The metric itself is left alone:
        // it is process-global and the suite runs in parallel.)
        let live = reg.get_float("probe_width").unwrap();
        assert!((live - Len::mm(2.0).to_px()).abs() < 1e-4, "{live}");
        // Two metrics give two answers for the one stored length.
        let assumed = Metric::assumed(1.0);
        let panel = Metric::from_sizes(2.0, (1920.0, 1200.0), (344.0, 215.0), MetricSource::Measured).unwrap();
        let (a, b) = (Len::mm(2.0).resolve(&assumed), Len::mm(2.0).resolve(&panel));
        assert!((a - 2.0 * 96.0 / 25.4).abs() < 1e-3, "{a}");
        assert!((b - 2.0 * panel.px_per_mm).abs() < 1e-3, "{b}");
        assert_ne!(a, b);
        assert_eq!(reg.get_len("probe_width"), Some(Len::mm(2.0)));
        // A plain number written later wins, and reads back as px.
        reg.set_float("probe_width", 7.0);
        assert_eq!(reg.get_float("probe_width"), Some(7.0));
        assert_eq!(reg.get_len("probe_width"), Some(Len::px(7.0)));
    }

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

    #[derive(Default)]
    struct ProbeTarget {
        rects: Vec<([f32; 4], f32, f32, f32, f32)>,
        bounded: Vec<(String, f32, f32, Option<[f32; 4]>)>,
    }

    impl RenderTarget for ProbeTarget {
        fn rect(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32) {
            self.rects.push((color, x, y, w, h));
        }
        fn text(&mut self, content: &str, x: f32, y: f32, _size: f32, _color: [f32; 4]) {
            self.bounded.push((content.to_string(), x, y, None));
        }
        fn text_with_bounds(&mut self, content: &str, x: f32, y: f32, _size: f32, _color: [f32; 4], bounds: Option<[f32; 4]>) {
            self.bounded.push((content.to_string(), x, y, bounds));
        }
    }

    /// Everything a section places horizontally — text, button rows, widgets,
    /// the column grid — must share ONE inset, or content does not line up
    /// with the content beside it. Text used to sit `padding()` to the left of
    /// every row in the same section.
    #[test]
    fn section_text_and_rows_share_one_inset() {
        let mut pc = ProbeTarget::default();
        let (left, cw) = (100.0f32, 320.0f32);
        let mut sec: SectionContext<'_, ProbeTarget> =
            SectionContext::new(&mut pc, left, 50.0, cw, "Probe", false, false);

        let cols = sec.row_layout(2, 8.0);
        assert_eq!(sec.ax(12.0), cols[0].0, "text at the conventional 12.0 must start where a row starts");
        assert_eq!(sec.ax(12.0), sec.content_left());

        // Symmetric: the right-hand inset from the section border matches the
        // left-hand one.
        let pad = sec.padding();
        let (border_l, border_r) = (left + pad, left + cw - pad);
        let row_right = cols[1].0 + cols[1].1;
        assert_eq!(cols[0].0 - border_l, border_r - row_right);
    }

    /// Even division gives a long label and a short one the same box, so one
    /// is clipped while the other floats in slack — the row of Suspend /
    /// Hibernate / Reboot / Power Off that prompted this. `row_layout_for`
    /// gives each column what it needs and shares the leftover equally.
    #[test]
    fn a_row_sized_for_content_fits_every_column() {
        let mut pc = ProbeTarget::default();
        let sec: SectionContext<'_, ProbeTarget> =
            SectionContext::new(&mut pc, 100.0, 50.0, 400.0, "Probe", false, false);
        let needs = [80.0f32, 30.0, 50.0, 40.0];
        let cols = sec.row_layout_for(&needs, 8.0);

        for (i, &(_, w)) in cols.iter().enumerate() {
            assert!(w >= needs[i], "column {i} got {w}, less than the {} it needs", needs[i]);
        }
        // The slack is shared equally, so every column overshoots by the same
        // amount — not proportionally, which would starve the short ones.
        let slack: Vec<f32> = cols.iter().zip(needs.iter()).map(|(&(_, w), n)| w - n).collect();
        for s in &slack {
            assert!((s - slack[0]).abs() < 1.0e-3, "slack shared unevenly: {slack:?}");
        }
        // And the row still ends inside the section.
        let (lx, lw) = *cols.last().unwrap();
        assert!(lx + lw <= sec.content_left() + sec.content_width() + 1.0e-3);
    }

    /// When the labels genuinely do not fit, everyone shrinks by the same
    /// factor rather than the last column absorbing the whole shortfall.
    #[test]
    fn an_overfull_row_shrinks_every_column_together() {
        let mut pc = ProbeTarget::default();
        let sec: SectionContext<'_, ProbeTarget> =
            SectionContext::new(&mut pc, 100.0, 50.0, 200.0, "Probe", false, false);
        let needs = [300.0f32, 150.0];
        let cols = sec.row_layout_for(&needs, 8.0);
        let ratio0 = cols[0].1 / needs[0];
        let ratio1 = cols[1].1 / needs[1];
        assert!((ratio0 - ratio1).abs() < 1.0e-3, "shrink was not shared: {ratio0} vs {ratio1}");
        let (lx, lw) = cols[1];
        assert!(lx + lw <= sec.content_left() + sec.content_width() + 1.0e-3, "overfull row escaped the section");
    }

    /// `ax` used to add `padding()` only for `x_off >= 12.0`, so asking for one
    /// pixel less moved content a whole `padding()` the other way. Callers do
    /// pass 11.0 and 13.0 in this tree, and the step put them in different
    /// coordinate spaces from each other.
    #[test]
    fn section_ax_is_continuous() {
        let mut pc = ProbeTarget::default();
        let mut sec: SectionContext<'_, ProbeTarget> =
            SectionContext::new(&mut pc, 100.0, 50.0, 320.0, "Probe", false, false);
        for off in [0.0f32, 1.0, 11.0, 11.999, 12.0, 13.0, 24.0] {
            assert!(
                (sec.ax(off) - sec.ax(0.0) - off).abs() < 1.0e-3,
                "ax must be a plain translation; it stepped at {off}"
            );
        }
    }

    /// A string wider than its section used to be drawn unbounded, running over
    /// the border and out of the window. Every section text now carries bounds
    /// no wider than the content box.
    #[test]
    fn section_text_is_bounded_to_the_content_box() {
        let mut pc = ProbeTarget::default();
        let (left, cw) = (100.0f32, 320.0f32);
        {
            let mut sec: SectionContext<'_, ProbeTarget> =
                SectionContext::new(&mut pc, left, 50.0, cw, "Probe", false, false);
            let right = sec.content_left() + sec.content_width();
            sec.text(
                "NVIDIA Corporation AD104M [GeForce RTX 4080 Max-Q / Mobile] and then some",
                12.0, 0.0, 12.0, [1.0; 4],
            );
            assert!(right < left + cw, "content box must sit inside the section");
        }
        // Pick our string out by content: the section's own label is drawn
        // through this target too, and it is not content.
        let (_, _, _, bounds) = pc
            .bounded
            .iter()
            .find(|(t, ..)| t.starts_with("NVIDIA"))
            .cloned()
            .expect("the section text must reach the render target");
        let b = bounds.expect("section text must be bounded");
        // Recompute the expectation from the same inputs the section used.
        let mut pc2 = ProbeTarget::default();
        let probe: SectionContext<'_, ProbeTarget> =
            SectionContext::new(&mut pc2, left, 50.0, cw, "Probe", false, false);
        assert_eq!(b[2], probe.content_left() + probe.content_width());
        assert!(b[2] <= left + cw - probe.padding(), "bound must not exceed the section border");
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
            let offset = self.label_strip();
            (self.base.x, self.base.y - offset, self.base.w, self.base.h + offset)
        }
        fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
            let offset = self.label_strip();
            self.base.x = x;
            self.base.y = y + offset;
            self.base.w = w;
            self.base.h = (h - offset).max(0.0);
        }
        fn color(&self) -> [f32; 4] {
            [0.0, 0.0, 0.0, 0.0]
        }
    }

    /// The vstack flow is checked against the LIVE style — the label margin, the
    /// detached-label font and the section padding are process-global and the suite
    /// runs in parallel, so pinning them here would be a window every other test
    /// could see (a label measured in one font by its own call and in another by the
    /// group hull's is exactly the flake this cost us). Every expectation below is
    /// derived from the getters instead, so the flow holds under any config.
    #[test]
    fn test_vstack_flow() {
        // `Section` seats its first column one `margin_x` in from its left edge.
        let first_col_x = 10.0 + 2.0 * section_padding() + Section::DEFAULT_MARGIN_X;
        let mut mock_pc = MockRenderTarget { rects: Vec::new() };
        let mut sec = Section::new(&mut mock_pc, 10.0, 20.0, 200.0, "Test Section");
        
        let start_y = sec.ay();
        let mut stack = sec.vstack(&mut mock_pc, 10.0);

        let mut dummy = crate::context::UiContext::new();
        let mut w1 = MockWidget { base: crate::widget::Widget::new(), x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
        stack.add_widget(&mut w1, 50.0, 30.0, &mut dummy);

        // Standard margin should be applied
        assert_eq!(w1.x, first_col_x);
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
        assert!(offset > 0.0, "a labelled widget has a strip");
        assert_eq!(w3.base.y, start_y + 30.0 + 10.0 + 40.0 + 10.0 + offset);
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

    // The `Once`-initialised style getters below are checked against the
    // statics their config pass fills, under whatever the live config says.
    // None of them WRITES: these are process globals and the suite runs in
    // parallel, so a set/restore window is visible to every other test — and
    // a "restore" that writes a hardcoded literal (as these did) clobbers a
    // non-default config permanently. What a getter here can actually get
    // wrong is which static it reads, and that is what these pin.

    #[test]
    fn test_nested_section_label_alignment() {
        let align = nested_section_label_alignment();
        assert_eq!(align, *super::NESTED_SECTION_LABEL_ALIGNMENT.read().unwrap());

        let offset = nested_section_label_offset();
        assert_eq!(offset, *super::NESTED_SECTION_LABEL_OFFSET.read().unwrap());
        assert!(offset.is_finite(), "label offset {offset}");
    }

    #[test]
    fn test_dropdown_height() {
        let h = dropdown_height();
        assert_eq!(h, *super::DROPDOWN_HEIGHT.read().unwrap());
        assert!(h.is_finite() && h > 0.0, "dropdown height {h}");
    }

    #[test]
    fn test_column_gap() {
        let gap = column_gap();
        assert_eq!(gap, *super::COLUMN_GAP.read().unwrap());
        assert!(gap.is_finite() && gap >= 0.0, "column gap {gap}");
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

    /// The control rung's key flattens to `control_corner_radius`, beside a
    /// widget's own override — the config shape `control corner_radius=8 { button corner_radius=10 }`.
    #[test]
    fn control_rung_radius_flattens_beside_widget_overrides() {
        let val: serde_json::Value = serde_json::json!({
            "style": { "control": { "corner_radius": 8, "button": { "corner_radius": 10 } } }
        });
        let mut flat = String::new();
        flatten_json_to_flat_props(&val, "", &mut flat);
        assert!(flat.lines().any(|l| l.starts_with("control_corner_radius")), "{flat}");
        assert!(flat.lines().any(|l| l.starts_with("button_corner_radius")), "{flat}");
    }

    /// A registry-backed style pinned by one test is invisible to a test
    /// beside it.
    ///
    /// The graph-style test below used to pin ~20 of these and restore none
    /// (it now reads the live registry instead: see
    /// `graph_style_getters_resolve_their_own_registry_keys`). Before the
    /// per-thread overlay that reached every test running alongside —
    /// provably: `dual_geometry_views_stay_consistent` bakes
    /// quads with `graph_node_corner_radius`, then re-reads the getter to
    /// compare, and a write landing between the two made them disagree.
    #[test]
    fn a_pinned_style_is_private_to_its_thread() {
        let base = graph_node_corner_radius();
        set_graph_node_corner_radius(base + 17.0);
        assert_eq!(graph_node_corner_radius(), base + 17.0, "the pinning thread sees its own value");

        let elsewhere = std::thread::spawn(graph_node_corner_radius).join().unwrap();
        assert_eq!(elsewhere, base, "a thread beside it must still see the shared base");
    }

    /// Every graph style getter resolves the registry key it is named for,
    /// falling back to its own documented default — checked against the live
    /// registry as it stands. Nothing is written: the style registry and the
    /// colour statics are process-global and the suite runs in parallel, so a
    /// set/assert here would be visible to every other test (this one used to
    /// set all twenty-one and restore none).
    #[test]
    fn graph_style_getters_resolve_their_own_registry_keys() {
        fn stored(key: &str) -> Option<f32> {
            crate::layout::lazy_init_style_registry();
            let reg = crate::layout::get_style_registry().read().unwrap();
            reg.get_float(key)
        }

        assert_eq!(graph_spacing_x(), stored("graph_spacing_x").unwrap_or(187.5));
        assert_eq!(graph_spacing_y(), stored("graph_spacing_y").unwrap_or(112.5));
        assert_eq!(graph_line_width(), stored("graph_line_width").unwrap_or(1.0));
        assert_eq!(graph_node_width(), stored("graph_node_width").unwrap_or(150.0));
        assert_eq!(graph_node_height(), stored("graph_node_height").unwrap_or(75.0));
        assert_eq!(graph_grid_snap(), stored("graph_grid_snap").unwrap_or(0.0) != 0.0);
        assert_eq!(graph_blur(), stored("graph_blur").unwrap_or(0.0));
        assert_eq!(graph_node_corner_radius(), stored("graph_node_corner_radius").unwrap_or(4.0));
        assert_eq!(graph_wire_size(), stored("graph_wire_size").unwrap_or(6.0));
        assert_eq!(
            graph_wire_activation_radius(),
            stored("graph_wire_activation_radius").unwrap_or(9.0)
        );
        assert_eq!(graph_connector_size(), stored("graph_connector_size").unwrap_or(8.0));
        assert_eq!(
            graph_connector_activation_radius(),
            stored("graph_connector_activation_radius").unwrap_or(12.0)
        );

        // The graph colours live behind statics in `color`, out of this
        // module's reach; what is checkable without writing them is that each
        // resolves to a real, in-gamut colour rather than an unparsed or
        // uninitialised one.
        let rgb: [(&str, [f32; 3]); 2] = [
            ("graph_cell", crate::color::graph_cell_color()),
            ("graph_gap", crate::color::graph_gap_color()),
        ];
        for (name, c) in rgb {
            assert!(c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)), "{name}: {c:?}");
        }
        let rgba: [(&str, [f32; 4]); 10] = [
            ("graph_node", crate::color::graph_node_color()),
            ("graph_node_selected", crate::color::graph_node_selected_color()),
            ("graph_node_drag", crate::color::graph_node_drag_color()),
            ("node", crate::color::node_color()),
            ("node_selected", crate::color::node_selected_color()),
            ("node_drag", crate::color::node_drag_color()),
            ("graph_wire", crate::color::graph_wire_color()),
            ("graph_wire_highlight", crate::color::graph_wire_highlight_color()),
            ("graph_connector", crate::color::graph_connector_color()),
            ("graph_connector_highlight", crate::color::graph_connector_highlight_color()),
        ];
        for (name, c) in rgba {
            assert!(c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)), "{name}: {c:?}");
        }
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

    /// The LUT is checked on `ramp_profile_lut`, the pure half of
    /// `set_bevel_profile_keys` / `set_roll_profile_keys` — installing it
    /// would restyle every wall in the process, and the suite runs in
    /// parallel.
    #[test]
    fn bevel_profile_lut_integrates_to_the_curves_net_rise() {
        let n = crate::layout::BEVEL_PROFILE_SAMPLES as f32;
        let net_rise = |slopes: [f32; crate::layout::BEVEL_PROFILE_SAMPLES]| -> f32 {
            slopes.iter().map(|s| s / n).sum()
        };

        // The identity 0→1 curve: slopes sum/N to its net rise of 1 (the same
        // total step the analytic smoothstep carries).
        let slopes = super::ramp_profile_lut(&[(0.0, 0.0), (1.0, 1.0)], true).expect("a curve");
        let rise = net_rise(slopes);
        assert!((rise - 1.0).abs() < 0.001, "net rise {rise}");

        // A rim curve that returns to its start height nets zero.
        let slopes =
            super::ramp_profile_lut(&[(0.0, 0.5), (0.2, 1.0), (0.8, 1.0), (1.0, 0.5)], false)
                .expect("a curve");
        let rise = net_rise(slopes);
        assert!(rise.abs() < 0.001, "net rise {rise}");

        // Degenerate key lists are no curve at all — the installers take that
        // `None` as "clear back to the analytic profile".
        assert!(super::ramp_profile_lut(&[(0.0, 1.0)], false).is_none());
        assert!(super::ramp_profile_lut(&[], true).is_none());
    }

    #[test]
    fn relief_profile_specs_parse_with_identity_sentinel() {
        // Absent and identity-smooth mean "analytic" — nothing to install.
        assert!(crate::layout::parse_relief_profile_spec(None).is_none());
        assert!(crate::layout::parse_relief_profile_spec(Some(
            crate::layout::RELIEF_PROFILE_IDENTITY_SPEC
        ))
        .is_none());
        // Garbage falls back to analytic instead of poisoning the walls.
        assert!(crate::layout::parse_relief_profile_spec(Some("not a spec")).is_none());
        // A real curve installs: keys and line type round-trip.
        let (keys, smooth) = crate::layout::parse_relief_profile_spec(Some(
            "linear;0.000:0.200,0.500:1.000,1.000:0.800",
        ))
        .expect("custom spec parses");
        assert!(!smooth);
        assert_eq!(keys.len(), 3);
        assert!((keys[1].0 - 0.5).abs() < 0.001 && (keys[1].1 - 1.0).abs() < 0.001);
        // Identity under a LINEAR line type is a real profile (a straight
        // chamfer), not the sentinel — only the smooth spelling is analytic.
        assert!(crate::layout::parse_relief_profile_spec(Some(
            "linear;0.000:0.000,1.000:1.000"
        ))
        .is_some());
    }
}

