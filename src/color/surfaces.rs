//! The surface colours and their slots: pages and layers, the pane and root plates (with the
//! menubar and statusbar bands), the well frame, menus and popovers, the plate's frost and
//! finish, and the theme.

use super::*;

pub(super) static PAGE_LOW_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.PAGE_LOW_COLOR, |s| &mut s.color.PAGE_LOW_COLOR);

pub(super) static COLOR_BORDERS_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.COLOR_BORDERS_COLOR, |s| &mut s.color.COLOR_BORDERS_COLOR);

pub(super) static OPACITY: crate::style::StyleCell<Option<f32>> = crate::style::StyleCell::new(|s| &s.color.OPACITY, |s| &mut s.color.OPACITY);

pub(super) static ROOT_PLATE_OPACITY: crate::style::StyleCell<Option<f32>> = crate::style::StyleCell::new(|s| &s.color.ROOT_PLATE_OPACITY, |s| &mut s.color.ROOT_PLATE_OPACITY);

pub(super) static POPOVER_BG_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.POPOVER_BG_COLOR, |s| &mut s.color.POPOVER_BG_COLOR);

pub(super) static PAGE_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.PAGE_COLOR, |s| &mut s.color.PAGE_COLOR);

pub(super) static LAYER_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.LAYER_COLOR, |s| &mut s.color.LAYER_COLOR);

pub(super) static ROOT_PLATE_CORNER_RADIUS: crate::style::StyleCell<f32> = crate::style::StyleCell::new(|s| &s.color.ROOT_PLATE_CORNER_RADIUS, |s| &mut s.color.ROOT_PLATE_CORNER_RADIUS);

pub(super) static ROOT_PLATE_MENUBAR_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.ROOT_PLATE_MENUBAR_COLOR, |s| &mut s.color.ROOT_PLATE_MENUBAR_COLOR);

pub(super) static ROOT_PLATE_MENUBAR_TEXT_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.ROOT_PLATE_MENUBAR_TEXT_COLOR, |s| &mut s.color.ROOT_PLATE_MENUBAR_TEXT_COLOR);

pub(super) static ROOT_PLATE_MENUBAR_BLUR: crate::style::StyleCell<bool> = crate::style::StyleCell::new(|s| &s.color.ROOT_PLATE_MENUBAR_BLUR, |s| &mut s.color.ROOT_PLATE_MENUBAR_BLUR);

/// Tint strength of frosted menus/popovers over the blurred backdrop:
/// 1.0 is fully opaque (frost invisible), lower shows more content through.
pub(super) static MENU_OPACITY: crate::style::StyleCell<f32> = crate::style::StyleCell::new(|s| &s.color.MENU_OPACITY, |s| &mut s.color.MENU_OPACITY);

/// Backdrop compression of frosted menus/popovers (`style.surface.menu.
/// compression`, 0..1): how hard the blurred content beneath a menu is
/// pulled toward the menu's own key, so the menu holds its legibility over
/// whatever it opens above. Menu-scoped, overriding the DE recipe's
/// `plate.backdrop_compression` for popovers only; a menu is read while
/// something else is going on beneath it, which a pane is not.
pub(super) static MENU_COMPRESSION: crate::style::StyleCell<f32> = crate::style::StyleCell::new(|s| &s.color.MENU_COMPRESSION, |s| &mut s.color.MENU_COMPRESSION);

/// The colour a menu's face is tinted with (`style.surface.menu.color`),
/// when the config names one; `None` follows the root plate colour
/// (`page_low_color`), which is what every menu wore before the key
/// existed. The alpha is ignored — `menu.opacity` is the tint strength —
/// so a menu can be dark on a light window without the window's own
/// plate going dark with it.
pub(super) static MENU_COLOR: crate::style::StyleCell<Option<[f32; 4]>> = crate::style::StyleCell::new(|s| &s.color.MENU_COLOR, |s| &mut s.color.MENU_COLOR);

pub(super) static ROOT_PLATE_STATUSBAR_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.ROOT_PLATE_STATUSBAR_COLOR, |s| &mut s.color.ROOT_PLATE_STATUSBAR_COLOR);

pub(super) static ROOT_PLATE_STATUSBAR_TEXT_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.ROOT_PLATE_STATUSBAR_TEXT_COLOR, |s| &mut s.color.ROOT_PLATE_STATUSBAR_TEXT_COLOR);

pub(super) static ROOT_PLATE_STATUSBAR_BLUR: crate::style::StyleCell<bool> = crate::style::StyleCell::new(|s| &s.color.ROOT_PLATE_STATUSBAR_BLUR, |s| &mut s.color.ROOT_PLATE_STATUSBAR_BLUR);

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

/// The params pane's plate tint (linear rgba) — [`PARAM_BG`] made live:
/// `style.surface.param.color` in config overrides it, and apps can retint at
/// runtime (the designer's Style section "Plate Color"). Alpha doubles as the
/// frost strength under plate blur.
pub(super) static PARAM_BG_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.PARAM_BG_COLOR, |s| &mut s.color.PARAM_BG_COLOR);

/// Whether the pane tint came in as `style.surface.plate.pane.color` — the
/// spelling whose alpha IS the tint strength — rather than the legacy
/// `style.surface.param.color`, which the top-level `plate_opacity` line
/// still multiplies (`Material::pane_legacy`). Two spellings, one tint.
pub(super) static PANE_COLOR_WHOLE: crate::style::StyleCell<bool> = crate::style::StyleCell::new(|s| &s.color.PANE_COLOR_WHOLE, |s| &mut s.color.PANE_COLOR_WHOLE);

pub fn param_bg_color() -> [f32; 4] {
    load_colors_once();
    style_read(&PARAM_BG_COLOR)
}

/// Whether the pane tint was spelled `style.surface.plate.pane.color`, in
/// which case its alpha is the whole tint strength and `plate_opacity` does
/// not multiply it. See `PANE_COLOR_WHOLE`.
pub fn pane_color_is_whole() -> bool {
    load_colors_once();
    style_read(&PANE_COLOR_WHOLE)
}

pub fn set_param_bg_color(color: [f32; 4]) {
    style_write(&PARAM_BG_COLOR, color);
}

/// The params plate's final fill as the renderer consumes it: the pane rung's
/// material ([`crate::scene::Material::pane`] — the tint at the global plate
/// opacity, frosted under plate blur) encoded for a nested plate. The single
/// source both `ParametersBg`'s own plate and any surface that wants to match
/// it (the designer's node bodies) draw from, so they track a live retint /
/// opacity / blur toggle together.
pub fn param_plate_fill() -> [f32; 4] {
    use crate::scene::material::{Material, PlateRole};
    Material::pane().fill(PlateRole::Nested)
}

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

/// Backdrop compression of frosted menus/popovers
/// (`/style/surface/menu/compression`, default 0.6) — see `MENU_COMPRESSION`.
pub fn menu_compression() -> f32 {
    load_colors_once();
    style_read(&MENU_COMPRESSION)
}

pub fn set_menu_compression(c: f32) {
    style_write(&MENU_COMPRESSION, c.clamp(0.0, 1.0));
}

/// The colour a menu's face is tinted with: `style.surface.menu.color` when
/// configured, else the root plate colour (`page_low_color`) — the face
/// every menu wore before the key existed, so an unconfigured menu looks as
/// it always did. Its alpha is not the tint strength; `menu_opacity` is
/// (`Material::popover` reads both).
pub fn menu_color() -> [f32; 4] {
    load_colors_once();
    style_read(&MENU_COLOR).unwrap_or_else(page_low_color)
}

pub fn set_menu_color(c: Option<[f32; 4]>) {
    style_write(&MENU_COLOR, c);
}

pub fn read_root_plate_opacity_if_configured() -> Option<f32> {
    load_colors_once();
    style_read(&ROOT_PLATE_OPACITY)
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

pub(super) static PLATE_COLOR: crate::style::StyleCell<Option<[f32; 4]>> = crate::style::StyleCell::new(|s| &s.color.PLATE_COLOR, |s| &mut s.color.PLATE_COLOR);

pub(super) static PLATE_BORDER_COLOR: crate::style::StyleCell<Option<[f32; 4]>> = crate::style::StyleCell::new(|s| &s.color.PLATE_BORDER_COLOR, |s| &mut s.color.PLATE_BORDER_COLOR);

pub(super) static PLATE_BORDER_THICKNESS: crate::style::StyleCell<f32> = crate::style::StyleCell::new(|s| &s.color.PLATE_BORDER_THICKNESS, |s| &mut s.color.PLATE_BORDER_THICKNESS);

pub fn plate_color() -> Option<[f32; 4]> {
    load_colors_once();
    style_read(&PLATE_COLOR)
}

pub fn set_plate_color(c: Option<[f32; 4]>) {
    style_write(&PLATE_COLOR, c);
}

pub fn plate_border_color() -> Option<[f32; 4]> {
    load_colors_once();
    style_read(&PLATE_BORDER_COLOR)
}

pub fn set_plate_border_color(c: Option<[f32; 4]>) {
    style_write(&PLATE_BORDER_COLOR, c);
}

pub fn plate_border_thickness() -> f32 {
    load_colors_once();
    style_read(&PLATE_BORDER_THICKNESS)
}

pub fn set_plate_border_thickness(t: f32) {
    style_write(&PLATE_BORDER_THICKNESS, t);
}

/// Roll width of a beveled pane plate (the control_relief replacement for the
/// flat border line), in logical px.
///
/// **This is `style.surface.relief.width`** — the one roll width, the same
/// number the root plate rolls over, every control wall runs, and
/// `relief.edge.height` is a rise against. Until 2026-09-28 it was a second
/// width of its own (`style.surface.plate.bevel_width`, default 6 against the
/// relief's 9.3), so a `PlateSpec` pane plate and an `append_widget_plate`
/// pane plate rolled over different widths in one window, the designer's
/// panes could not be made to match its own window lip by editing one key,
/// and the edge height was expressed against a width the panes did not use.
/// The old key was an explicit override for the rest of that day and is
/// RETIRED: a config still carrying it is reported by path
/// (`retired_surface_keys`) and the key is not read. The getter survives
/// as the name the plate painters call, so a caller need not know which
/// registry key a roll is.
pub fn plate_bevel_width() -> f32 {
    crate::layout::bevel_width()
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
pub(super) static PLATE_BACKDROP_COMPRESSION: crate::style::StyleCell<f32> = crate::style::StyleCell::new(|s| &s.color.PLATE_BACKDROP_COMPRESSION, |s| &mut s.color.PLATE_BACKDROP_COMPRESSION);

pub fn plate_backdrop_compression() -> f32 {
    load_colors_once();
    style_read(&PLATE_BACKDROP_COMPRESSION)
}

pub fn set_plate_backdrop_compression(c: f32) {
    style_write(&PLATE_BACKDROP_COMPRESSION, c.clamp(0.0, 1.0));
}

/// How far a frosted plate's roll bends what it samples, and how much clearer
/// its rim is than its frosted body: 0 = a flat window, 1 = full.
///
/// This buys no legibility and is not meant to — see
/// [`plate_backdrop_compression`] for that. What it buys is the plate reading
/// as an OBJECT: a curved edge displaces the view through it, so the
/// silhouette stops being where the haze ends and becomes where a slab with a
/// thickness begins. The two are complementary, and on a dark desktop
/// especially: compression flattens the body toward the tint, which leaves the
/// rim as the only place the material can still say what it is.
///
/// Default 0.0 — no existing config changes appearance.
pub(super) static PLATE_REFRACTION: crate::style::StyleCell<f32> = crate::style::StyleCell::new(|s| &s.color.PLATE_REFRACTION, |s| &mut s.color.PLATE_REFRACTION);

pub fn plate_refraction() -> f32 {
    load_colors_once();
    style_read(&PLATE_REFRACTION)
}

pub fn set_plate_refraction(r: f32) {
    style_write(&PLATE_REFRACTION, r.clamp(0.0, 1.0));
}

/// Whether the default plate material is frosted at all — the switch on
/// `Frost::from_style`. Spelled `style.surface.plate.frost` (a block, the
/// knobs inside it; see the loader). The older `style.surface.plate.blur`
/// bool is retired and reported, not read.
pub(super) static PLATE_BLUR: crate::style::StyleCell<bool> = crate::style::StyleCell::new(|s| &s.color.PLATE_BLUR, |s| &mut s.color.PLATE_BLUR);

/// The finish's three fixed terms — specular strength, shininess exponent,
/// curvature/AO strength — as `scene::material::Finish::from_style` reads
/// them. Until 2026-09-20 these were literals in the finish constructor
/// (0.4 / 24 / 0.2); the getters exist so the DE's plastic can be edited and
/// so a named material (RFC material, step 4) has somewhere to land. No config
/// path yet: the defaults ARE the shipped look.
pub(super) static FINISH_SPEC: crate::style::StyleCell<f32> = crate::style::StyleCell::new(|s| &s.color.FINISH_SPEC, |s| &mut s.color.FINISH_SPEC);

pub(super) static FINISH_SHININESS: crate::style::StyleCell<f32> = crate::style::StyleCell::new(|s| &s.color.FINISH_SHININESS, |s| &mut s.color.FINISH_SHININESS);

pub(super) static FINISH_CURVATURE: crate::style::StyleCell<f32> = crate::style::StyleCell::new(|s| &s.color.FINISH_CURVATURE, |s| &mut s.color.FINISH_CURVATURE);

/// The default material's blur radius (`style.surface.plate.frost.radius`,
/// the kernel sigma in logical px) — [`crate::scene::Frost::DEFAULT_RADIUS`]
/// unless config says otherwise.
pub(super) static PLATE_FROST_RADIUS: crate::style::StyleCell<f32> = crate::style::StyleCell::new(|s| &s.color.PLATE_FROST_RADIUS, |s| &mut s.color.PLATE_FROST_RADIUS);

pub fn plate_frost_radius() -> f32 {
    load_colors_once();
    style_read(&PLATE_FROST_RADIUS)
}

pub fn set_plate_frost_radius(r: f32) {
    style_write(&PLATE_FROST_RADIUS, r.max(0.0));
}

pub fn finish_spec() -> f32 {
    style_read(&FINISH_SPEC)
}

pub fn set_finish_spec(v: f32) {
    style_write(&FINISH_SPEC, v.max(0.0));
}

pub fn finish_shininess() -> f32 {
    style_read(&FINISH_SHININESS)
}

pub fn set_finish_shininess(v: f32) {
    style_write(&FINISH_SHININESS, v.max(1.0));
}

pub fn finish_curvature() -> f32 {
    style_read(&FINISH_CURVATURE)
}

pub fn set_finish_curvature(v: f32) {
    style_write(&FINISH_CURVATURE, v.max(0.0));
}

pub fn plate_blur() -> bool {
    load_colors_once();
    style_read(&PLATE_BLUR)
}

pub fn set_plate_blur(b: bool) {
    style_write(&PLATE_BLUR, b);
}
