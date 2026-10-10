//! The layout and style getters: every style key the toolkit reads, with its default, its
//! setter, and the code filed beside them. Re-exported whole, so `crate::layout::…` paths do not
//! depend on which file a getter lives in.
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the style slots, the shared constants, text-line metrics, `reload_config` |
//! | `registry` | the style registry (config flattened to one map) and its typed reads |
//! | `relief` | the light, the bevel and roll geometry, corner shape and window radii, carve depth, the relief profiles |
//! | `spacing` | the spacing ladder (root, pane, control and list rungs) and label margins |
//! | `fonts` | every font getter and setter, and the preferred-font resolution |
//! | `controls` | control heights, corner radii, opacities, scrollbar sizes |
//! | `graph` | the graph widget's settings |
//! | `bridge` | the flat-host render bridge |
//! | `section`, `form` | settings-page sections and what goes inside them |


// This module's part of the one style snapshot (`crate::style`): every slot below,
// with its default. Each slot's `static` handle stands where its lock did.
crate::style::style_slots! {
    MENUBAR_FONT_CACHED: Option<(String, (String, f32))> = None;
    STATUSBAR_FONT_CACHED: Option<(String, (String, f32))> = None;
    FONT_SELECTOR_FONT_CACHED: Option<(String, (String, f32))> = None;
    BUTTON_STRIP_FONT_CACHED: Option<(String, (String, f32))> = None;
    CONTROL_LABEL_FONT_CACHED: Option<(String, (String, f32))> = None;
    CONTROL_LABEL_FONT_DETACHED_CACHED: Option<(String, (String, f32))> = None;
    LIST_FONT_CACHED: Option<(String, (String, f32))> = None;
    TREE_FONT_CACHED: Option<(String, (String, f32))> = None;
    GRAPH_FONT_CACHED: Option<(String, (String, f32))> = None;
    GRAPH_NODE_FONT_CACHED: Option<(String, (String, f32))> = None;
    BEVEL_PROFILE: Option<[f32; BEVEL_PROFILE_SAMPLES]> = None;
    ROLL_PROFILE: Option<[f32; BEVEL_PROFILE_SAMPLES]> = None;
}

mod registry;
pub use registry::*;
mod controls;
pub use controls::*;
mod fonts;
pub use fonts::*;
mod graph;
pub use graph::*;
mod relief;
pub use relief::*;
mod spacing;
pub use spacing::*;
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

static MENUBAR_FONT_CACHED: crate::style::StyleCell<Option<(String, (String, f32))>> = crate::style::StyleCell::new(|s| &s.layout.MENUBAR_FONT_CACHED, |s| &mut s.layout.MENUBAR_FONT_CACHED);
static STATUSBAR_FONT_CACHED: crate::style::StyleCell<Option<(String, (String, f32))>> = crate::style::StyleCell::new(|s| &s.layout.STATUSBAR_FONT_CACHED, |s| &mut s.layout.STATUSBAR_FONT_CACHED);

static FONT_SELECTOR_FONT_CACHED: crate::style::StyleCell<Option<(String, (String, f32))>> = crate::style::StyleCell::new(|s| &s.layout.FONT_SELECTOR_FONT_CACHED, |s| &mut s.layout.FONT_SELECTOR_FONT_CACHED);
static BUTTON_STRIP_FONT_CACHED: crate::style::StyleCell<Option<(String, (String, f32))>> = crate::style::StyleCell::new(|s| &s.layout.BUTTON_STRIP_FONT_CACHED, |s| &mut s.layout.BUTTON_STRIP_FONT_CACHED);
static CONTROL_LABEL_FONT_CACHED: crate::style::StyleCell<Option<(String, (String, f32))>> = crate::style::StyleCell::new(|s| &s.layout.CONTROL_LABEL_FONT_CACHED, |s| &mut s.layout.CONTROL_LABEL_FONT_CACHED);
static CONTROL_LABEL_FONT_DETACHED_CACHED: crate::style::StyleCell<Option<(String, (String, f32))>> = crate::style::StyleCell::new(|s| &s.layout.CONTROL_LABEL_FONT_DETACHED_CACHED, |s| &mut s.layout.CONTROL_LABEL_FONT_DETACHED_CACHED);
static LIST_FONT_CACHED: crate::style::StyleCell<Option<(String, (String, f32))>> = crate::style::StyleCell::new(|s| &s.layout.LIST_FONT_CACHED, |s| &mut s.layout.LIST_FONT_CACHED);
static TREE_FONT_CACHED: crate::style::StyleCell<Option<(String, (String, f32))>> = crate::style::StyleCell::new(|s| &s.layout.TREE_FONT_CACHED, |s| &mut s.layout.TREE_FONT_CACHED);
static GRAPH_FONT_CACHED: crate::style::StyleCell<Option<(String, (String, f32))>> = crate::style::StyleCell::new(|s| &s.layout.GRAPH_FONT_CACHED, |s| &mut s.layout.GRAPH_FONT_CACHED);
static GRAPH_NODE_FONT_CACHED: crate::style::StyleCell<Option<(String, (String, f32))>> = crate::style::StyleCell::new(|s| &s.layout.GRAPH_NODE_FONT_CACHED, |s| &mut s.layout.GRAPH_NODE_FONT_CACHED);

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


/// Load the style config as one change of style (`crate::style::batch`): the registry and
/// every slot are published together, and the hundreds of per-key writes cost one copy.
pub fn reload_config() {
    crate::style::batch(reload_config_in_batch);
}

fn reload_config_in_batch() {
    if let Some(content) = read_config() {
        // Every flattened key goes into the registry, which is the one copy every getter
        // reads (since 2026-10-08; until then about fifty keys were also scanned off the
        // same lines into slots of their own, and read from there).
        if let Ok(mut registry) = get_style_registry().write() {
            for line in content.lines() {
                let trimmed = line.trim();
                let Some(eq_idx) = trimmed.find('=') else { continue };
                let key = trimmed[..eq_idx].trim();
                let val_str = trimmed[eq_idx + 1..].trim().trim_matches('"').trim();
                if let Ok(f_val) = val_str.parse::<f32>() {
                    registry.load_float(key, f_val);
                } else if let Some(len) = crate::units::Len::parse(val_str) {
                    // `(mm)2.0` arrived as the string `2mm`.
                    registry.load_len(key, len);
                } else {
                    registry.load_string(key, val_str.to_string());
                }
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

#[cfg(test)]
mod tests;
