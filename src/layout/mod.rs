
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
mod bridge;
pub use bridge::*;
mod section;
pub use section::*;
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

/// The untouched editor curve — the "analytic" sentinel in the config'd
/// profile specs (cce-designer's Edge Profile convention). For the wall curve
/// identity-smooth IS the analytic smoothstep, so skipping it changes
/// nothing; for the roll it would be a straight chamfer, not the analytic
/// superellipse quadrant, so it must read as "no custom profile".
pub const RELIEF_PROFILE_IDENTITY_SPEC: &str = "smooth;0.000:0.000,1.000:1.000";

/// Parse-and-install the relief profiles config carries as ramp specs
/// (`style.surface.relief.wall.profile` / `edge.profile` → the style
/// registry's `bevel_profile_spec` / `roll_profile_spec`). Absent, identity,
/// or unparseable specs clear back to the analytic profiles.
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

pub fn control_label_margin() -> f32 {
    registry_float("control_label_margin").unwrap_or(6.0)
}

pub(crate) fn control_label_strip() -> f32 {
    let (_, font_size) = control_label_font_detached_parsed();
    font_size + control_label_margin()
}

pub fn label_margin() -> f32 {
    control_label_margin()
}

pub fn set_control_label_margin(margin: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("control_label_margin", margin);
    }
}

pub fn set_label_margin(margin: f32) {
    set_control_label_margin(margin);
}

pub fn nested_section_label_alignment() -> u8 {
    registry_float("nested_section_label_alignment").map(|v| v as u8).unwrap_or(0)
}

pub fn set_nested_section_label_alignment(align: u8) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("nested_section_label_alignment", align as f32);
    }
}

pub fn nested_section_label_offset() -> f32 {
    registry_float("nested_section_label_offset").unwrap_or(0.0)
}

pub fn set_nested_section_label_offset(offset: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("nested_section_label_offset", offset);
    }
}

/// The pane rung's padding: from a pane plate's rim to its content, in
/// logical px (`style.surface.plate.padding`). The second rung of the
/// spacing ladder — [`root_plate_inset`] / [`root_plate_gap`] on the root
/// plate, this and [`plate_gap`] inside a pane plate, [`control_gap`]
/// between controls. Registry-backed (live-reloadable); the legacy flat
/// `plate_padding = N` line still loads as a fallback.
pub fn plate_padding() -> f32 {
    registry_float("plate_padding").unwrap_or(20.0)
}

pub fn set_plate_padding(padding: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("plate_padding", padding);
    }
}

/// Legacy: the page-level margin (`style.surface.page.margin`). Unset, it
/// IS the pane rung's [`plate_padding`] — a page is a pane — so an app
/// still reading it lands on the ladder. Set, it is honoured as before.
pub fn page_margin() -> f32 {
    registry_float("page_margin").unwrap_or_else(plate_padding)
}

pub fn set_page_margin(margin: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("page_margin", margin);
    }
}

pub fn grid_min_col_width() -> f32 {
    registry_float("grid_min_col_width").unwrap_or(260.0)
}

pub fn set_grid_min_col_width(width: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("grid_min_col_width", width);
    }
}

pub fn grid_gap() -> f32 {
    registry_float("grid_gap").unwrap_or(8.0)
}

pub fn set_grid_gap(gap: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("grid_gap", gap);
    }
}

/// Legacy: the inter-column gap (`style.layout.column.gap`). Unset, it is
/// the root plate's [`root_plate_gap`] — columns are siblings on the plate.
pub fn column_gap() -> f32 {
    registry_float("column_gap").unwrap_or_else(root_plate_gap)
}

pub fn set_column_gap(gap: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("column_gap", gap);
    }
}

/// Legacy: a control panel's padding (`style.control.control_panel.padding`).
/// Unset, it is the pane rung's [`plate_padding`] — a control panel is a pane.
pub fn control_panel_padding() -> f32 {
    registry_float("control_panel_padding").unwrap_or_else(plate_padding)
}

pub fn set_control_panel_padding(padding: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("control_panel_padding", padding);
    }
}

/// Legacy: a control panel's gap (`style.control.control_panel.gap`).
/// Unset, it is the pane rung's [`plate_gap`].
pub fn control_panel_gap() -> f32 {
    registry_float("control_panel_gap").unwrap_or_else(plate_gap)
}

pub fn set_control_panel_gap(gap: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("control_panel_gap", gap);
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
        .is_none_or(|v| v.trim() != "stacked")
}

pub fn section_padding() -> f32 {
    registry_float("section_padding").unwrap_or(8.0)
}

pub fn set_section_padding(padding: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("section_padding", padding);
    }
}

pub fn spinbox_height() -> f32 {
    registry_float("spinbox_height").unwrap_or(DEFAULT_CONTROL_HEIGHT)
}

pub fn set_spinbox_height(height: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("spinbox_height", height);
    }
}

pub fn toggle_height() -> f32 {
    registry_float("toggle_height").unwrap_or(DEFAULT_CONTROL_HEIGHT)
}

pub fn set_toggle_height(height: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("toggle_height", height);
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
/// shading. `style.surface.relief.shader=(bool)false` (or `0`) flips back to
/// the old look for A/B comparison — the registry key keeps the bevel name
/// because it selects how the shared lit EDGE is computed, not which shapes
/// exist. Until 2026-09-28 the config spelling was `window_manager.bevel_shader`
/// (retired, reported), and only a NUMBER worked: a `(bool)` flattens to the
/// string "false", which the float read never saw.
pub fn bevel_shader() -> bool {
    lazy_init_style_registry();
    let reg = get_style_registry().read().unwrap();
    shader_on(reg.get_float("bevel_shader"), reg.get_string("bevel_shader").as_deref())
}

/// The shader toggle's reading of its registry slot: a number is on unless
/// zero, a string is on unless it says `false` / `off` / `no`, and an unset
/// slot is on.
fn shader_on(float: Option<f32>, string: Option<&str>) -> bool {
    match (float, string) {
        (Some(v), _) => v != 0.0,
        (None, Some(s)) => !matches!(s.trim().to_ascii_lowercase().as_str(), "false" | "off" | "no" | "0"),
        (None, None) => true,
    }
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
/// (`style.surface.relief.wall.height`, a length — `(mm)0.3` resolves through
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

/// The plate roll's rise when pinned (`style.surface.relief.edge.height`, a
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
static BEVEL_PROFILE: crate::style::StyleCell<Option<[f32; BEVEL_PROFILE_SAMPLES]>> = crate::style::StyleCell::new(|s| &s.layout.BEVEL_PROFILE, |s| &mut s.layout.BEVEL_PROFILE);
/// Bumped on every profile change so renderers know to re-upload their LUT.
static BEVEL_PROFILE_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The custom EDGE (plate roll) profile — same slope-LUT encoding as
/// [`BEVEL_PROFILE`], but read by the shader's `roll_slope` for the perimeter
/// roll of widget-scale plates: `v` runs 0 at the face join → 1 at the
/// silhouette, and the curve is the roll's descent progress (0 = face height,
/// 1 = fully dropped), so the identity curve is a straight chamfer and `None`
/// is the analytic superellipse quadrant.
static ROLL_PROFILE: crate::style::StyleCell<Option<[f32; BEVEL_PROFILE_SAMPLES]>> = crate::style::StyleCell::new(|s| &s.layout.ROLL_PROFILE, |s| &mut s.layout.ROLL_PROFILE);
static ROLL_PROFILE_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub use cce_core::ramp::sample_ramp_keys;

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
            assert!((0.1 - 1e-6..=1.0 + 1e-6).contains(&v), "overshoot {v}");
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

/// Where content starts on the standard root plate, measured from the
/// WINDOW edge: the plate's rolled rim ([`bevel_width`]) plus one
/// [`root_plate_padding`]. The padding is a run of flat plate face, the
/// same run [`root_plate_gap`] leaves between two siblings; but the face
/// only begins where the roll ends, so a bare padding at a window edge
/// leaves most of it on the roll — measured at 4px of visible flat against
/// 12 between panes (cce-mail, 2026-09-19). This is the one number an app
/// on the standard plate insets by at its four edges; between siblings it
/// uses the gap, and everything inside a pane plate uses
/// [`plate_padding`]. An app whose base is NOT the rolled root plate (a
/// transparent surface, a bare fill) has no roll to clear and insets by
/// [`root_plate_padding`] alone.
pub fn root_plate_inset() -> f32 {
    bevel_width() + root_plate_padding()
}

/// One style-registry float, initialising the registry on first use — the
/// one read every rung getter goes through.
///
/// The guard is released before this returns, which is why a getter whose
/// unset slot falls back to ANOTHER getter must read through here:
/// `get_style_registry().read().unwrap().get_float(k).unwrap_or_else(g)`
/// holds its guard to the end of the statement, so `g`'s read nests inside
/// it, and std's `RwLock` queues a reader behind a waiting writer — a
/// `reload_config` arriving between the two reads parks both, and every
/// reader in the process behind them. That hung cce-designer's suite
/// (2026-09-25); `tests/style_registry_reentrancy.rs` is the check.
fn registry_float(slot: &str) -> Option<f32> {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float(slot)
}

/// [`registry_float`] for a switch: a number is on when it is not zero (`set_*` writes
/// 1 or 0), a word when it is `true`.
fn registry_bool(key: &str) -> Option<bool> {
    registry_float(key).map(|v| v != 0.0).or_else(|| registry_string(key).map(|v| v == "true"))
}

/// A configured font string parsed into (family, size; 12 when the string names none),
/// cached against the string it was parsed from — a reload, a setter or a test's
/// per-thread overlay that changes the string parses it again.
fn parsed_font(cache: &crate::style::StyleCell<Option<(String, (String, f32))>>, font: String) -> (String, f32) {
    if let Ok(c) = cache.read() {
        if let Some((src, val)) = &*c {
            if *src == font {
                return val.clone();
            }
        }
    }
    let (family, size) = parse_font_string(&font);
    let val = (family, size.unwrap_or(12.0));
    if let Ok(mut c) = cache.write() {
        *c = Some((font, val.clone()));
    }
    val
}

/// [`registry_float`] for a string key (a font), with the same guard discipline.
fn registry_string(key: &str) -> Option<String> {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_string(key)
}

/// Gap between siblings INSIDE a pane plate, in logical px
/// (`style.surface.plate.gap`) — the pane rung's twin of
/// [`root_plate_gap`]. Unset, it is the root gap: one number reads as one
/// rhythm across both rungs unless a config says otherwise.
pub fn plate_gap() -> f32 {
    registry_float("plate_gap").unwrap_or_else(root_plate_gap)
}

/// Gap between controls, in logical px (`style.control.gap`) — the control
/// rung of the ladder: what the layout strategies put between a form's
/// controls (and between a detached label's block and the next), in both
/// axes. Unset, it is [`CONTROL_GAP`], one control height, the value every
/// strategy's `Default` carried as a literal.
pub fn control_gap() -> f32 {
    registry_float("control_gap").unwrap_or(CONTROL_GAP)
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
    registry_float("toggle_corner_radius").unwrap_or_else(control_corner_radius)
}

pub fn set_toggle_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("toggle_corner_radius", radius);
    }
}

pub fn toggle_border_width() -> f32 {
    registry_float("toggle_border_width").unwrap_or(1.0)
}

pub fn set_toggle_border_width(width: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("toggle_border_width", width);
    }
}

pub fn slider_corner_radius() -> f32 {
    lazy_init_style_registry();
    registry_float("slider_corner_radius").unwrap_or_else(control_corner_radius)
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
    registry_float("plate_opacity").unwrap_or(1.0)
}

pub fn set_plate_opacity(opacity: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("plate_opacity", opacity);
    }
}

pub fn page_opacity() -> f32 {
    registry_float("page_opacity").unwrap_or(1.0)
}

pub fn set_page_opacity(opacity: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("page_opacity", opacity);
    }
}

pub fn layer_opacity() -> f32 {
    registry_float("layer_opacity").unwrap_or(1.0)
}

pub fn set_layer_opacity(opacity: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("layer_opacity", opacity);
    }
}

pub fn color_selector_height() -> f32 {
    registry_float("color_selector_height").unwrap_or(DEFAULT_CONTROL_HEIGHT)
}

pub fn set_color_selector_height(height: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("color_selector_height", height);
    }
}

pub fn font_selector_height() -> f32 {
    registry_float("font_selector_height").unwrap_or(DEFAULT_CONTROL_HEIGHT)
}

pub fn set_font_selector_height(height: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("font_selector_height", height);
    }
}

pub fn color_selector_font() -> String {
    registry_string("color_selector_font").filter(|f| !f.is_empty()).unwrap_or_else(|| "monospace".to_string())
}

pub fn set_color_selector_font(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("color_selector_font", font.to_string());
    }
}

pub fn menubar_font() -> String {
    registry_string("menubar_font").filter(|f| !f.is_empty()).unwrap_or_else(|| "Berkeley Mono".to_string())
}

pub fn menubar_font_parsed() -> (String, f32) {
    parsed_font(&MENUBAR_FONT_CACHED, menubar_font())
}

pub fn set_menubar_font(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("menubar_font", font.to_string());
    }
}

pub fn statusbar_font() -> String {
    registry_string("statusbar_font").filter(|f| !f.is_empty()).unwrap_or_else(|| "Berkeley Mono".to_string())
}

pub fn statusbar_font_parsed() -> (String, f32) {
    parsed_font(&STATUSBAR_FONT_CACHED, statusbar_font())
}

pub fn set_statusbar_font(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("statusbar_font", font.to_string());
    }
}

pub fn font_selector_font() -> String {
    registry_string("font_selector_font").filter(|f| !f.is_empty()).unwrap_or_else(|| "Berkeley Mono".to_string())
}

pub fn font_selector_font_parsed() -> (String, f32) {
    parsed_font(&FONT_SELECTOR_FONT_CACHED, font_selector_font())
}

pub fn set_font_selector_font(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("font_selector_font", font.to_string());
    }
}

// Button Strip Font
pub fn button_strip_font() -> String {
    registry_string("button_strip_font").filter(|f| !f.is_empty()).unwrap_or_else(|| "Berkeley Mono".to_string())
}

pub fn button_strip_font_parsed() -> (String, f32) {
    parsed_font(&BUTTON_STRIP_FONT_CACHED, button_strip_font())
}

pub fn set_button_strip_font(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("button_strip_font", font.to_string());
    }
}

// Control Label Font
pub fn control_label_font() -> String {
    registry_string("control_label_font").filter(|f| !f.is_empty()).unwrap_or_else(|| "Berkeley Mono".to_string())
}

pub fn control_label_font_parsed() -> (String, f32) {
    parsed_font(&CONTROL_LABEL_FONT_CACHED, control_label_font())
}

pub fn set_control_label_font(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("control_label_font", font.to_string());
    }
}

// Control Label Font Detached
pub fn control_label_font_detached() -> String {
    registry_string("control_label_font_detached").filter(|f| !f.is_empty()).unwrap_or_else(|| "Berkeley Mono".to_string())
}

pub fn control_label_font_detached_parsed() -> (String, f32) {
    parsed_font(&CONTROL_LABEL_FONT_DETACHED_CACHED, control_label_font_detached())
}

pub fn set_control_label_font_detached(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("control_label_font_detached", font.to_string());
    }
}

// List Font
pub fn list_font() -> String {
    registry_string("list_font").filter(|f| !f.is_empty()).unwrap_or_else(|| "Berkeley Mono".to_string())
}

pub fn list_font_parsed() -> (String, f32) {
    parsed_font(&LIST_FONT_CACHED, list_font())
}

pub fn set_list_font(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("list_font", font.to_string());
    }
}

// Tree Font
pub fn tree_font() -> String {
    registry_string("tree_font").filter(|f| !f.is_empty()).unwrap_or_else(|| "Berkeley Mono".to_string())
}

pub fn tree_font_parsed() -> (String, f32) {
    parsed_font(&TREE_FONT_CACHED, tree_font())
}

pub fn set_tree_font(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("tree_font", font.to_string());
    }
}

// Graph Font
pub fn graph_font() -> String {
    registry_string("graph_font").filter(|f| !f.is_empty()).unwrap_or_else(|| "Berkeley Mono".to_string())
}

pub fn graph_font_parsed() -> (String, f32) {
    parsed_font(&GRAPH_FONT_CACHED, graph_font())
}

pub fn set_graph_font(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("graph_font", font.to_string());
    }
}

// Graph Node Font
pub fn graph_node_font() -> String {
    registry_string("graph_node_font").filter(|f| !f.is_empty()).unwrap_or_else(|| "Berkeley Mono".to_string())
}

pub fn graph_node_font_parsed() -> (String, f32) {
    parsed_font(&GRAPH_NODE_FONT_CACHED, graph_node_font())
}

pub fn set_graph_node_font(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("graph_node_font", font.to_string());
    }
}

// List Justification
pub fn list_justification() -> u8 {
    registry_float("list_justification").map(|v| v as u8).unwrap_or(0)
}

pub fn set_list_justification(just: u8) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("list_justification", just as f32);
    }
}

pub fn section_label_font() -> String {
    registry_string("section_label_font").filter(|f| !f.is_empty()).unwrap_or_else(|| "Berkeley Mono".to_string())
}

pub fn set_section_label_font(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("section_label_font", font.to_string());
    }
}

pub fn nested_section_label_font() -> String {
    registry_string("nested_section_label_font").filter(|f| !f.is_empty()).unwrap_or_else(|| "Berkeley Mono".to_string())
}

pub fn set_nested_section_label_font(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("nested_section_label_font", font.to_string());
    }
}

pub fn breadcrumb_font() -> String {
    registry_string("breadcrumb_font").filter(|f| !f.is_empty()).unwrap_or_else(|| "Berkeley Mono".to_string())
}

pub fn set_breadcrumb_font(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("breadcrumb_font", font.to_string());
    }
}

pub fn button_font() -> String {
    registry_string("button_font").filter(|f| !f.is_empty()).unwrap_or_else(|| "Berkeley Mono".to_string())
}

pub fn set_button_font(font: &str) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_string("button_font", font.to_string());
    }
}

pub fn color_selector_preview_corner_radius() -> f32 {
    lazy_init_style_registry();
    // Unset: the swatch rounds like the text field beside it (the TextBox radius).
    registry_float("color_selector_preview_corner_radius").unwrap_or_else(textbox_corner_radius)
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
    registry_float("color_selector_corner_radius").unwrap_or_else(textbox_corner_radius)
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
    registry_float("button_corner_radius").unwrap_or_else(control_corner_radius)
}

pub fn set_button_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("button_corner_radius", radius);
    }
}

pub fn spinbox_corner_radius() -> f32 {
    lazy_init_style_registry();
    registry_float("spinbox_corner_radius").unwrap_or_else(control_corner_radius)
}

pub fn set_spinbox_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("spinbox_corner_radius", radius);
    }
}

pub fn spinbox_button_padding() -> f32 {
    registry_float("spinbox_button_padding").unwrap_or(0.0)
}

pub fn scrollbar_width() -> f32 {
    registry_float("scrollbar_width").unwrap_or(4.0)
}

/// The thickness of a CENTRED scrollbar — one that rides the centre line of
/// what it scrolls, over the content and behind the host's plate until a
/// scroll raises it (the params pane, the spreadsheet, a sink-behind
/// `ScrollRegion`, the designer's dialog). [`scrollbar_width`] widened by
/// 1.6: over rows rather than in a lane of its own, the stock width reads
/// too slim.
pub fn centred_scrollbar_width() -> f32 {
    scrollbar_width() * 1.6
}

pub fn set_scrollbar_width(width: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("scrollbar_width", width);
    }
}

/// How far a page-level scrollbar stands off its window/plate right edge — the
/// designer parameter-pane look (config `style.control.scrollbar.inset`).
/// Framed inner lists keep their own tight 4px hug; this is for bars floating
/// over a plate.
pub fn scrollbar_inset() -> f32 {
    registry_float("scrollbar_inset").unwrap_or(16.0)
}

pub fn tree_opacity() -> f32 {
    registry_float("tree_opacity").unwrap_or(1.0)
}

pub fn set_tree_opacity(opacity: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("tree_opacity", opacity);
    }
}

pub fn tree_blur() -> f32 {
    registry_float("tree_blur").unwrap_or(0.0)
}

pub fn set_tree_blur(blur: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("tree_blur", blur);
    }
}

pub fn set_spinbox_button_padding(padding: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("spinbox_button_padding", padding);
    }
}

pub fn textbox_corner_radius() -> f32 {
    lazy_init_style_registry();
    registry_float("textbox_corner_radius").unwrap_or_else(control_corner_radius)
}

pub fn set_textbox_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("textbox_corner_radius", radius);
    }
}

pub fn textbox_line_wrap() -> bool {
    registry_bool("textbox_line_wrap").unwrap_or(true)
}

pub fn set_textbox_line_wrap(wrap: bool) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("textbox_line_wrap", if wrap { 1.0 } else { 0.0 });
    }
}

pub fn touchpad_natural_scroll() -> bool {
    registry_bool("touchpad_natural_scroll").unwrap_or(false)
}

pub fn set_touchpad_natural_scroll(enabled: bool) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("touchpad_natural_scroll", if enabled { 1.0 } else { 0.0 });
    }
}

pub fn textbox_multiline_border_width() -> f32 {
    registry_float("textbox_multiline_border_width").unwrap_or(1.0)
}

pub fn set_textbox_multiline_border_width(width: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("textbox_multiline_border_width", width);
    }
}

pub fn list_corner_radius() -> f32 {
    lazy_init_style_registry();
    registry_float("list_corner_radius").unwrap_or_else(control_corner_radius)
}

pub fn set_list_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("list_corner_radius", radius);
    }
}

pub fn tree_corner_radius() -> f32 {
    lazy_init_style_registry();
    registry_float("tree_corner_radius").unwrap_or_else(control_corner_radius)
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

/// `style.surface.graph.node.wire_style`: how a wire runs, by
/// `WireStyle::name` — `orthogonal`, `rounded`, `bezier` or `straight`.
/// `None` when the config does not say.
pub fn graph_wire_style() -> Option<String> {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_string("graph_wire_style")
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
    registry_float("font_selector_corner_radius").unwrap_or_else(control_corner_radius)
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
    registry_float("menu_corner_radius").unwrap_or_else(control_corner_radius)
}

pub fn dropdown_corner_radius() -> f32 {
    lazy_init_style_registry();
    registry_float("dropdown_corner_radius").unwrap_or_else(control_corner_radius)
}

pub fn set_dropdown_corner_radius(radius: f32) {
    lazy_init_style_registry();
    if let Ok(mut registry) = get_style_registry().write() {
        registry.set_float("dropdown_corner_radius", radius);
    }
}

pub fn color_selector_preview_margin() -> f32 {
    registry_float("color_selector_preview_margin").unwrap_or(0.0)
}

pub fn set_color_selector_preview_margin(margin: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("color_selector_preview_margin", margin);
    }
}

pub fn paginator_tab_padding_x() -> f32 {
    registry_float("paginator_tab_padding_x").unwrap_or(10.0)
}

pub fn set_paginator_tab_padding_x(padding: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("paginator_tab_padding_x", padding);
    }
}

pub fn button_padding() -> f32 {
    registry_float("button_padding")
        .or_else(|| registry_float("paginator_tab_padding_y"))
        .unwrap_or(14.0)
}

pub fn set_button_padding(padding: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("button_padding", padding);
    }
}

pub fn button_height() -> f32 {
    registry_float("button_height").unwrap_or(DEFAULT_CONTROL_HEIGHT)
}

pub fn ramp_height() -> f32 {
    registry_float("ramp_height").unwrap_or(32.0)
}

pub fn set_ramp_height(height: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("ramp_height", height);
    }
}

pub fn set_button_height(height: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("button_height", height);
    }
}

pub fn button_strip_spacing() -> f32 {
    registry_float("button_strip_spacing").unwrap_or(8.0)
}

pub fn set_button_strip_spacing(spacing: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("button_strip_spacing", spacing);
    }
}

pub fn paginator_tab_padding_y() -> f32 {
    button_padding()
}

pub fn set_paginator_tab_padding_y(padding: f32) {
    set_button_padding(padding);
}

pub fn textbox_height() -> f32 {
    registry_float("textbox_height").unwrap_or(DEFAULT_CONTROL_HEIGHT)
}

pub fn set_textbox_height(height: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("textbox_height", height);
    }
}

pub fn dropdown_height() -> f32 {
    registry_float("dropdown_height").unwrap_or(DEFAULT_CONTROL_HEIGHT)
}

pub fn set_dropdown_height(height: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("dropdown_height", height);
    }
}

pub fn slider_height() -> f32 {
    registry_float("slider_height").unwrap_or(DEFAULT_TRACK_HEIGHT)
}

pub fn set_slider_height(height: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("slider_height", height);
    }
}

pub fn progressbar_height() -> f32 {
    registry_float("progressbar_height").unwrap_or(DEFAULT_TRACK_HEIGHT)
}

pub fn set_progressbar_height(height: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("progressbar_height", height);
    }
}

pub fn rangeslider_height() -> f32 {
    registry_float("rangeslider_height").unwrap_or(DEFAULT_TRACK_HEIGHT)
}

pub fn set_rangeslider_height(height: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("rangeslider_height", height);
    }
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

#[cfg(test)]
mod tests {
    use crate::widget::WidgetHost;

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

    /// A graph's wires and ports reach a flat host: `render_widget` hands
    /// its strokes to `RenderTarget::line` (and arcs and discs to `arc` and
    /// `circle`), where it used to drop them, so cce-files' Graph page drew
    /// nodes and no wires.
    #[test]
    fn a_graphs_wires_reach_a_flat_host() {
        #[derive(Default)]
        struct Strokes {
            lines: usize,
            circles: usize,
        }
        impl RenderTarget for Strokes {
            fn rect(&mut self, _: [f32; 4], _: f32, _: f32, _: f32, _: f32) {}
            fn text(&mut self, _: &str, _: f32, _: f32, _: f32, _: [f32; 4]) {}
            fn line(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: [f32; 4], _: crate::scene::paint::Cap) {
                self.lines += 1;
            }
            fn arc(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32, _: [f32; 4]) {
                self.lines += 1;
            }
            fn circle(&mut self, _: f32, _: f32, _: f32, _: [f32; 4]) {
                self.circles += 1;
            }
        }
        use crate::widget::GraphController;
        let node = |name: &str, pos: (f32, f32), input: Option<&str>| crate::widget::GraphNode {
            id: name.to_string(),
            name: name.to_string(),
            position: pos,
            parameters: input.map(|i| vec![("input".to_string(), i.to_string(), "string".to_string())]).unwrap_or_default(),
            geom_visible: true,
            node_type: String::new(),
            inputs: 1,
            outputs: 1,
        };
        let mut graph = crate::widget::Graph::new();
        graph.set_grid_origin(100.0, 100.0);
        graph.set_nodes(&[node("a", (0.0, 0.0), None), node("b", (0.0, 1.0), Some("a"))]);
        let mut pc = Strokes::default();
        let mut ctx = crate::context::UiContext::new();
        render_widget(&mut pc, &mut graph, 0.0, 0.0, 600.0, 600.0, &mut ctx);
        assert!(pc.lines > 0, "the wire a -> b reached the host");
        assert!(pc.circles > 0, "the ports reached the host");
    }

    /// A widget's glyph reaches a flat host: `render_widget` hands the
    /// dropdown's arrow to the host's `RenderTarget::icon` by name. It used
    /// to drop every image, so after the toolkit's symbols became glyphs a
    /// flat host showed a dropdown with no arrow. Skipped where the icon set
    /// is not checked out (no glyph uploads, so there is nothing to name).
    #[test]
    fn a_widget_glyph_reaches_a_flat_host() {
        if !std::path::Path::new(&crate::icons_dir()).join("chevron-down.svg").is_file() {
            eprintln!("skipped: no icon set");
            return;
        }
        #[derive(Default)]
        struct Icons(Vec<String>);
        impl RenderTarget for Icons {
            fn rect(&mut self, _: [f32; 4], _: f32, _: f32, _: f32, _: f32) {}
            fn text(&mut self, _: &str, _: f32, _: f32, _: f32, _: [f32; 4]) {}
            fn icon(&mut self, name: &str, _: crate::scene::layout::Rect, _: [f32; 4]) {
                self.0.push(name.to_string());
            }
        }
        let mut pc = Icons::default();
        let mut ctx = crate::context::UiContext::new();
        let mut dd = crate::widget::Dropdown::new(vec!["One".to_string(), "Two".to_string()], 0);
        render_widget(&mut pc, &mut dd, 10.0, 10.0, 160.0, 24.0, &mut ctx);
        assert!(pc.0.iter().any(|n| n == "chevron-down"), "the arrow reached the host: {:?}", pc.0);
    }

    /// Everything a section places horizontally — text, button rows, widgets,
    /// the column grid — must share ONE inset, or content does not line up
    /// with the content beside it. Text used to sit `padding()` to the left of
    /// every row in the same section.
    #[test]
    fn section_text_and_rows_share_one_inset() {
        let mut pc = ProbeTarget::default();
        let (left, cw) = (100.0f32, 320.0f32);
        let sec: SectionContext<'_, ProbeTarget> =
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
        let sec: SectionContext<'_, ProbeTarget> =
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
        // A section's VStack seats every widget one content margin in from its left edge.
        let mut mock_pc = MockRenderTarget { rects: Vec::new() };
        let mut sec = SectionContext::new(&mut mock_pc, 10.0, 20.0, 200.0, "Test Section", false, false);
        let first_col_x = sec.content_left();
        let gap = sec.row_gap;
        let start_y = sec.ay();
        let mut stack = sec.vstack(10.0);

        let mut dummy = crate::context::UiContext::new();
        let mut w1 = MockWidget { base: crate::widget::Widget::new(), x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
        stack.add_widget(&mut w1, 50.0, 30.0, &mut dummy);

        assert_eq!(w1.x, first_col_x);
        assert_eq!(w1.y, start_y);

        let mut w2 = MockWidget { base: crate::widget::Widget::new(), x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
        stack.add_widget(&mut w2, 60.0, 40.0, &mut dummy);

        // The next starts below the first, the stack's spacing and the row gap on.
        assert_eq!(w2.y, start_y + 30.0 + 10.0 + gap);

        let mut base = crate::widget::Widget::new();
        base.label = Some("Test Label".to_string());
        let mut w3 = MockWidgetWithLabel { base };
        stack.add_widget(&mut w3, 70.0, 50.0, &mut dummy);

        let offset = w3.base.label_offset();
        // A labelled widget's control sits below its label strip.
        assert!(offset > 0.0, "a labelled widget has a strip");
        assert_eq!(w3.base.y, start_y + 30.0 + 10.0 + gap + 40.0 + 10.0 + gap + offset);
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
        let mut subsec = SectionContext::new(&mut pc, 10.0, 20.0, 300.0, "Test Subsec", false, true);
        assert_eq!(subsec.left, 10.0);
        assert_eq!(subsec.top, 20.0);
        assert_eq!(subsec.cw, 300.0);
        assert!(subsec.is_child);

        let mut dummy = crate::context::UiContext::new();
        let mut w = MockWidget { base: crate::widget::Widget::new(), x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
        subsec.widget(&mut w, 12.0, 100.0, 40.0, &mut dummy);

        let bottom = subsec.finish();
        assert!(bottom > 20.0);
    }

    /// These keys have ONE home, the style registry (since 2026-10-08): the setter writes
    /// it, the getter reads it, an unset key is its default, and a length in other units
    /// resolves through the metric — which the slot each used to have, filled by a scan
    /// of `key = number` lines, ignored. Writes go to this thread's test overlay, so no
    /// other test sees them.
    #[test]
    fn a_style_key_lives_in_the_registry_alone() {
        set_button_height(33.0);
        assert_eq!(button_height(), 33.0);
        set_nested_section_label_alignment(2);
        assert_eq!(nested_section_label_alignment(), 2);
        set_dropdown_height(31.0);
        assert_eq!(dropdown_height(), 31.0);

        set_section_label_font("Circe Slab A 12");
        assert_eq!(section_label_font(), "Circe Slab A 12");
        set_section_label_font("");
        assert_eq!(section_label_font(), "Berkeley Mono", "an empty font falls back");

        // `button_padding` reads `paginator_tab_padding_y` when it has none of its own.
        get_style_registry().write().unwrap().set_float("paginator_tab_padding_y", 5.0);
        assert_eq!(button_padding(), 5.0);
        set_button_padding(9.0);
        assert_eq!(button_padding(), 9.0, "its own wins");

        let len = crate::units::Len::mm(5.0);
        get_style_registry().write().unwrap().set_len("textbox_height", len);
        assert_eq!(textbox_height(), len.to_px(), "a length in mm is honoured");
    }

    #[test]
    fn test_column_gap() {
        // A legacy key: set (by config or setter) it is honoured; unset it
        // lands on the ladder — the root plate's gap.
        let gap = column_gap();
        match registry_float("column_gap") {
            Some(v) => assert_eq!(gap, v),
            None => assert_eq!(gap, root_plate_gap()),
        }
        assert!(gap.is_finite() && gap >= 0.0, "column gap {gap}");
    }

    #[test]
    fn spacing_ladder_falls_back_rung_by_rung() {
        // Every rung getter is finite and non-negative, and the inset is the
        // roll plus the padding, whatever the config says.
        for v in [root_plate_padding(), root_plate_gap(), root_plate_inset(), plate_padding(), plate_gap(), control_gap()] {
            assert!(v.is_finite() && v >= 0.0, "{v}");
        }
        assert_eq!(root_plate_inset(), bevel_width() + root_plate_padding());
        if registry_float("plate_gap").is_none() {
            assert_eq!(plate_gap(), root_plate_gap());
        }
        if registry_float("control_gap").is_none() {
            assert_eq!(control_gap(), CONTROL_GAP);
        }
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

    /// The relief's two shapes are nodes — `relief { wall … ; edge … }` —
    /// and each of their keys flattens to the registry key the flat legacy
    /// spelling always did, so nothing downstream of the registry moved.
    #[test]
    fn relief_wall_and_edge_nodes_flatten_to_the_legacy_registry_keys() {
        let val: serde_json::Value = serde_json::json!({
            "style": { "surface": { "relief": {
                "width": 9.3,
                "wall": { "height": "0.3mm", "profile": "smooth;0:0,1:1" },
                "edge": { "height": 4.0, "profile": "smooth;0:0,1:0.9" }
            } } }
        });
        let mut flat = String::new();
        flatten_json_to_flat_props(&val, "", &mut flat);
        // A flat line is `key = value`, strings quoted — what reload_config parses.
        let value = |k: &str| {
            flat.lines()
                .find(|l| l.starts_with(&format!("{k} = ")))
                .map(|l| l[k.len() + 3..].trim_matches('"').to_string())
        };
        assert_eq!(value("bevel_height").as_deref(), Some("0.3mm"), "{flat}");
        assert_eq!(value("roll_height").as_deref(), Some("4"), "{flat}");
        assert_eq!(value("bevel_profile_spec").as_deref(), Some("smooth;0:0,1:1"), "{flat}");
        assert_eq!(value("roll_profile_spec").as_deref(), Some("smooth;0:0,1:0.9"), "{flat}");
        // The retired spellings — the flat geometry keys, `depth`, and the
        // knob keys that are editor state — reach NO registry key: a config
        // that says only these draws the defaults, and the load-time report
        // (`color::retired_surface_keys`) is what says why.
        let old: serde_json::Value = serde_json::json!({
            "style": { "surface": { "relief": {
                "depth": 0.3,
                "height": 2.0, "edge_height": 3.0, "profile": "a", "edge_profile": "b",
                "wall": { "knobs": "1,1,1" }, "edge_knobs": "2,2,2"
            } } },
            "window_manager": { "bevel_depth": 0.3, "bevel_width": 5.0, "bevel_shader": 0 }
        });
        let mut flat = String::new();
        flatten_json_to_flat_props(&old, "", &mut flat);
        for k in ["bevel_depth", "bevel_width", "bevel_shader", "bevel_height", "roll_height", "bevel_profile_spec", "roll_profile_spec", "profile_knobs"] {
            assert!(!flat.lines().any(|l| l.starts_with(&format!("{k} = "))), "{k} landed from a retired spelling: {flat}");
        }
        // And a file carrying BOTH spellings is what its current one says,
        // with no precedence pass in between — the retired one is not read.
        let both: serde_json::Value = serde_json::json!({
            "style": { "surface": { "relief": {
                "light": 0.15, "depth": 0.9,
                "height": 2.0, "wall": { "height": 5.0 },
                "edge_profile": "old", "edge": { "profile": "new" }
            } } }
        });
        let mut flat = String::new();
        flatten_json_to_flat_props(&both, "", &mut flat);
        assert_eq!(flat.lines().filter(|l| l.starts_with("bevel_height = ")).count(), 1, "{flat}");
        assert!(flat.contains("bevel_height = 5") && flat.contains("bevel_depth = 0.15"), "{flat}");
        assert!(flat.contains("roll_profile_spec = \"new\""), "{flat}");
    }

    /// The relief's shader toggle is `style.surface.relief.shader`, and a
    /// `(bool)` works there: it flattens to the string "false", which the
    /// getter reads (a number was the only thing the old float read saw).
    #[test]
    fn the_shader_toggle_lives_with_the_relief_and_takes_a_bool() {
        let val: serde_json::Value = serde_json::json!({
            "style": { "surface": { "relief": { "shader": false } } }
        });
        let mut flat = String::new();
        flatten_json_to_flat_props(&val, "", &mut flat);
        assert!(flat.lines().any(|l| l == "bevel_shader = false"), "{flat}");
        assert!(shader_on(None, None), "unset: the SDF branch");
        assert!(shader_on(Some(1.0), None) && !shader_on(Some(0.0), Some("true")), "a number decides when present");
        assert!(!shader_on(None, Some("false")) && !shader_on(None, Some("off")) && shader_on(None, Some("true")));
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
        let rgb: [(&str, [f32; 3]); 1] = [("graph_grid", crate::color::graph_grid_color())];
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
        use crate::widget::ContainerLayout;
        
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
        use crate::widget::ContainerLayout;
        
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

