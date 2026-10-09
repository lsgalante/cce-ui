//! Control metrics: heights, corner radii (falling back to the control rung), opacities,
//! scrollbar sizes, and the per-widget switches.

use super::*;

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
