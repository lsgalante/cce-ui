//! The toolkit's colours: the built-in palette (linear constants), the colour and surface slots of
//! the style snapshot with their getters and setters, and loading them from the config.
//!
//! A slot is a `StyleCell` static beside the getter that reads it (the few with no getter of
//! their own stay here); `load` writes them from `config.kdl`. A setter writes through
//! `style_write`, which under `cfg(test)` lands in a per-thread overlay, so a test that pins a
//! colour pins it for its own thread only.
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the palette constants, the `style_slots!` block, the test overlay, `style_read` / `style_write` |
//! | `surfaces` | pages and layers, the pane and root plates, the well frame, menus and popovers, frost and finish, the theme |
//! | `controls` | buttons, text boxes, dropdowns, sliders, spinboxes, progress bars, control labels, the ramp |
//! | `lists` | lists, breadcrumbs, the tree list, scrollbars |
//! | `graph` | the node graph: nodes, wires, connectors, its grid |
//! | `text` | text colours outside a control: links, tags, the quote bar, the highlight and the code wash, and the colours derived from them |
//! | `load` | reading the config into the slots, retired surface keys, reloads |
//! | `math` | sRGB / linear, OKLab, the perceptual fade |
//! | `materials` | named materials and the rung bindings, and their config types (`MaterialDef`, `FrostDef`, `PlateRung`) |
//! | `chords` | the tree and list search chords |

mod load;
pub use load::*;
mod math;
pub use math::*;
mod chords;
pub use chords::*;
mod materials;
pub use materials::*;

mod controls;
pub use controls::*;
mod graph;
pub use graph::*;
mod lists;
pub use lists::*;
mod text;
pub use text::*;
mod surfaces;
pub use surfaces::*;

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

// The toggle's own palette (enabled/disabled/background) is RETIRED: a toggle
// inherits the plate it sits on and marks state with light and relief, so
// there is nothing left to tint. The `TOGGLE_*` consts above survive for the
// graph's node geometry switch, which is a different control.

// This module's part of the one style snapshot (`crate::style`): every slot below,
// with its default. Each slot's `static` handle stands where its lock did.
crate::style::style_slots! {
    NAMED_MATERIALS: Vec<(String, MaterialDef)> = Vec::new();
    MATERIAL_BINDINGS: [Option<String>; 3] = [None, None, None];
    PAGE_LOW_COLOR: [f32; 4] = [0.0600316, 0.0600316, 0.080219, 1.0];
    COLOR_BORDERS_COLOR: [f32; 4] = [0.2039, 0.2039, 0.2530, 1.0];
    NODE_COLOR: [f32; 4] = NODE_IDLE;
    NODE_SELECTED_COLOR: [f32; 4] = NODE_SELECTED;
    NODE_DRAG_COLOR: [f32; 4] = NODE_DRAG;
    SIDEBAR_BG_COLOR: [f32; 4] = SIDEBAR_BG;
    HIGHLIGHT_PRIMARY_COLOR: [f32; 4] = HIGHLIGHT_PRIMARY;
    MENUBAR_TAB_LABEL_COLOR: [f32; 4] = [0.90196, 0.90196, 0.94902, 1.0];
    CONTROL_LABEL_COLOR: [f32; 4] = [0.61206, 0.61206, 0.68666, 1.0];
    CONTROL_LABEL_COLOR_DETACHED: [f32; 4] = [0.22416, 0.22416, 0.2526, 1.0];
    CONTROL_LABEL_HOVER_COLOR: Option<[f32; 4]> = None;
    CONTROL_LABEL_FOCUS_COLOR: Option<[f32; 4]> = None;
    OPACITY: Option<f32> = None;
    ROOT_PLATE_OPACITY: Option<f32> = None;
    LIST_BG_COLOR: [f32; 4] = [0.08, 0.08, 0.12, 0.3];
    LIST_ENTRY_BG_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 0.04];
    LIST_ENTRY_HIGHLIGHT_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 0.8];
    LIST_FONT_COLOR: [f32; 4] = [0.80, 0.80, 0.85, 1.0];
    BREADCRUMB_BG_COLOR: [f32; 4] = [0.08, 0.08, 0.12, 1.0];
    POPOVER_BG_COLOR: [f32; 4] = [0.08, 0.08, 0.12, 1.0];
    PAGE_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 0.0];
    LAYER_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 0.0];
    ROOT_PLATE_CORNER_RADIUS: f32 = 12.0;
    DROPDOWN_BACKGROUND_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 0.0];
    TEXTBOX_PLACEHOLDER_TEXT_COLOR: [u8; 3] = [0x60, 0x60, 0x6a];
    TEXTBOX_BACKGROUND_COLOR: [f32; 4] = [0.08, 0.08, 0.12, 1.0];
    TEXTBOX_BACKGROUND_EDIT_COLOR: [f32; 4] = [0.06, 0.10, 0.18, 1.0];
    ROOT_PLATE_MENUBAR_COLOR: [f32; 4] = [0.08, 0.08, 0.12, 1.0];
    ROOT_PLATE_MENUBAR_TEXT_COLOR: [f32; 4] = [0.90196, 0.90196, 0.94902, 1.0];
    ROOT_PLATE_MENUBAR_BLUR: bool = false;
    MENU_OPACITY: f32 = 0.8;
    MENU_COMPRESSION: f32 = 0.6;
    MENU_COLOR: Option<[f32; 4]> = None;
    ROOT_PLATE_STATUSBAR_COLOR: [f32; 4] = [0.06, 0.06, 0.10, 1.0];
    ROOT_PLATE_STATUSBAR_TEXT_COLOR: [f32; 4] = [0.6666, 0.6666, 0.7333, 1.0];
    ROOT_PLATE_STATUSBAR_BLUR: bool = false;
    BUTTON_BACKGROUND_COLOR: [f32; 4] = BUTTON_IDLE;
    RAMP_BACKGROUND_COLOR: [f32; 4] = [0.08, 0.08, 0.12, 1.0];
    RAMP_BORDER_COLOR: [f32; 4] = [0.18, 0.18, 0.24, 1.0];
    CONTROL_PANEL_COLOR: [f32; 4] = [0.075, 0.082, 0.11, 1.0];
    CONTROL_PANEL_BORDER_COLOR: [f32; 4] = [0.161, 0.173, 0.216, 1.0];
    PROGRESS_BG_COLOR: [f32; 4] = PROGRESS_BG;
    PROGRESS_FILL_COLOR: [f32; 4] = PROGRESS_FILL;
    SPINBOX_DISPLAY_COLOR: [f32; 4] = SPINBOX_DISPLAY;
    SPINBOX_BUTTON_COLOR: [f32; 4] = SPINBOX_BUTTON;
    SPINBOX_BUTTON_HOVER_COLOR: [f32; 4] = SPINBOX_BUTTON_HOVER;
    SPINBOX_TEXT_COLOR: [f32; 4] = [0.8, 0.8, 0.83, 1.0];
    BUTTON_BORDER_COLOR: Option<[f32; 4]> = None;
    DROPDOWN_BORDER_COLOR: [f32; 4] = [0.18, 0.18, 0.24, 1.0];
    DROPDOWN_TEXT_COLOR: [f32; 4] = [0.72305, 0.72305, 0.76008, 1.0];
    SLIDER_THUMB_COLOR: [f32; 4] = SLIDER_THUMB;
    SLIDER_THUMB_DRAG_COLOR: [f32; 4] = SLIDER_THUMB_DRAG;
    RANGE_SLIDER_THUMB_COLOR: [f32; 4] = SLIDER_THUMB;
    RANGE_SLIDER_THUMB_DRAG_COLOR: [f32; 4] = SLIDER_THUMB_DRAG;
    TREE_BACKGROUND_COLOR: [f32; 4] = [0.08, 0.08, 0.12, 0.3];
    TREE_BORDER_COLOR: [f32; 4] = [0.18, 0.18, 0.24, 1.0];
    TREE_BORDER_HOVER_COLOR: [f32; 4] = [0.25, 0.25, 0.35, 1.0];
    TREE_BORDER_FOCUS_COLOR: [f32; 4] = [0.30, 0.50, 0.32, 1.0];
    TREE_OPEN_SEARCH_KEY: String = String::new();
    LIST_OPEN_SEARCH_KEY: String = String::new();
    LIST_CLOSE_SEARCH_KEY: String = String::new();
    TREE_SECTION_BG_COLOR: [f32; 4] = [0.07, 0.07, 0.09, 1.0];
    TREE_SECTION_BG_HOVER_COLOR: [f32; 4] = [0.10, 0.12, 0.18, 1.0];
    TREE_LEAF_BG_EVEN_COLOR: [f32; 4] = [0.09, 0.09, 0.11, 1.0];
    TREE_LEAF_BG_ODD_COLOR: [f32; 4] = [0.08, 0.08, 0.10, 1.0];
    TREE_LEAF_BG_HOVER_COLOR: [f32; 4] = [0.12, 0.12, 0.16, 1.0];
    TREE_LEAF_BG_SELECTED_COLOR: [f32; 4] = [0.15, 0.20, 0.30, 1.0];
    TREE_SECTION_TEXT_COLOR: [f32; 4] = [0.38, 0.69, 0.94, 1.0];
    TREE_LEAF_TEXT_COLOR: [f32; 4] = [0.80, 0.80, 0.83, 1.0];
    TREE_LEAF_TEXT_SELECTED_COLOR: [f32; 4] = [0.49, 1.0, 1.0, 1.0];
    TREE_TYPE_TEXT_COLOR: [f32; 4] = [0.78, 0.47, 0.87, 1.0];
    TREE_VALUE_TEXT_COLOR: [f32; 4] = [0.51, 0.51, 0.54, 1.0];
    TREE_SEPARATOR_COLOR: [f32; 4] = [0.15, 0.15, 0.19, 1.0];
    TEXT_LINK_COLOR: [f32; 4] = to_linear(text::DEFAULT_LINK_SRGB);
    TEXT_TAG_COLOR: [f32; 4] = to_linear(text::DEFAULT_TAG_SRGB);
    TEXT_QUOTE_BAR_COLOR: [f32; 4] = to_linear(text::DEFAULT_QUOTE_BAR_SRGB);
    TEXT_HIGHLIGHT_COLOR: [f32; 4] = to_linear(text::DEFAULT_HIGHLIGHT_SRGB);
    TEXT_CODE_BACKGROUND_COLOR: [f32; 4] = text::DEFAULT_CODE_BACKGROUND;
    SCROLLBAR_TRACK_COLOR: [f32; 4] = [0.15, 0.15, 0.20, 0.3];
    SCROLLBAR_THUMB_COLOR: [f32; 4] = [0.60, 0.60, 0.65, 0.4];
    GRAPH_GRID_COLOR: [f32; 3] = [0.07, 0.07, 0.09];
    GRAPH_OPACITY: f32 = 0.95;
    GRAPH_NODE_OPACITY: f32 = 1.0;
    GRAPH_NODE_COLOR: [f32; 4] = NODE_IDLE;
    GRAPH_NODE_SELECTED_COLOR: [f32; 4] = NODE_SELECTED;
    GRAPH_NODE_DRAG_COLOR: [f32; 4] = NODE_DRAG;
    GRAPH_WIRE_COLOR: [f32; 4] = [0.1, 0.8, 0.4, 1.0];
    GRAPH_WIRE_HIGHLIGHT_COLOR: [f32; 4] = [0.0, 1.0, 0.9, 1.0];
    GRAPH_CONNECTOR_COLOR: [f32; 4] = [0.1, 0.8, 0.4, 1.0];
    GRAPH_CONNECTOR_HIGHLIGHT_COLOR: [f32; 4] = [0.0, 1.0, 0.9, 1.0];
    PARAM_BG_COLOR: [f32; 4] = PARAM_BG;
    PANE_COLOR_WHOLE: bool = false;
    PLATE_COLOR: Option<[f32; 4]> = Some([0.15, 0.15, 0.2, 0.95]);
    PLATE_BORDER_COLOR: Option<[f32; 4]> = Some([0.3, 0.3, 0.4, 1.0]);
    PLATE_BORDER_THICKNESS: f32 = 1.0;
    PLATE_BACKDROP_COMPRESSION: f32 = 0.0;
    PLATE_REFRACTION: f32 = 0.0;
    PLATE_BLUR: bool = false;
    FINISH_SPEC: f32 = FINISH_SPEC_DEFAULT;
    FINISH_SHININESS: f32 = FINISH_SHININESS_DEFAULT;
    FINISH_CURVATURE: f32 = FINISH_CURVATURE_DEFAULT;
    PLATE_FROST_RADIUS: f32 = FROST_RADIUS_DEFAULT;
}

/// Read a style slot.
#[inline]
fn style_read<T: Clone + 'static>(cell: &'static crate::style::StyleCell<T>) -> T {
    cell.read().unwrap().clone()
}

/// Write a style slot. Under `cfg(test)` the style is the test thread's own
/// (`crate::style`), so a test's write reaches no other test.
#[inline]
fn style_write<T: Clone + PartialEq + 'static>(cell: &'static crate::style::StyleCell<T>, val: T) {
    if let Ok(mut lock) = cell.write() {
        *lock = val;
    }
}

/// Change a style slot in place, under ONE write guard, so a change another thread
/// publishes meanwhile is merged with this one rather than overwritten (a `style_read` then a
/// `style_write` is two guards, and the write puts back what the read saw).
#[inline]
fn style_update<T: Clone + PartialEq + 'static>(cell: &'static crate::style::StyleCell<T>, f: impl FnOnce(&mut T)) {
    if let Ok(mut lock) = cell.write() {
        f(&mut lock);
    }
}

static SIDEBAR_BG_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.SIDEBAR_BG_COLOR, |s| &mut s.color.SIDEBAR_BG_COLOR);

static HIGHLIGHT_PRIMARY_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.HIGHLIGHT_PRIMARY_COLOR, |s| &mut s.color.HIGHLIGHT_PRIMARY_COLOR);

static MENUBAR_TAB_LABEL_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.MENUBAR_TAB_LABEL_COLOR, |s| &mut s.color.MENUBAR_TAB_LABEL_COLOR); // sRGB [230, 230, 242] linear

pub const SLIDER_THUMB: [f32; 4] = [0.60, 0.60, 0.65, 1.0];

pub const SLIDER_THUMB_DRAG: [f32; 4] = [0.80, 0.80, 0.85, 1.0];

pub const PROGRESS_BG: [f32; 4] = [0.18, 0.18, 0.22, 1.0];

pub const PROGRESS_FILL: [f32; 4] = [0.20, 0.50, 0.75, 1.0];

/// `#7dffff` in config, the DE's accent. Not white: a highlight is also a
/// specular tint (a selected graph node's bevel), and white is the tint a
/// plain bevel already wears, so a white default made selection invisible
/// on a machine with no config (until 2026-10-10).
pub const HIGHLIGHT_PRIMARY: [f32; 4] = [0.205, 1.0, 1.0, 0.12];

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

pub const TEXT_HEADER: [f32; 4] = [0.90, 0.90, 0.95, 1.0];

pub const TEXT_ACCENT: [f32; 4] = [0.56, 0.83, 0.56, 1.0];

/// The finish a config without `relief spec` / `shininess` / `curvature`
/// gets: the literals the shader shipped with.
pub(crate) const FINISH_SPEC_DEFAULT: f32 = 0.4;

pub(crate) const FINISH_SHININESS_DEFAULT: f32 = 24.0;

pub(crate) const FINISH_CURVATURE_DEFAULT: f32 = 0.2;

/// The frost kernel a config without `style.surface.plate.frost.radius` gets, as a sigma in
/// logical px: `scene::Frost::DEFAULT_RADIUS` is this, and says why it is 5.5. Here so the
/// slot's default names nothing above this module.
pub(crate) const FROST_RADIUS_DEFAULT: f32 = 5.5;

#[cfg(test)]
mod color_tests;
#[cfg(test)]
mod tests;
