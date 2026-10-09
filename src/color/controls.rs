//! The control colours and their slots: buttons, text boxes, dropdowns, sliders, spinboxes,
//! progress bars, the control panel, control labels, and the ramp.

use super::*;

pub(super) static CONTROL_LABEL_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.CONTROL_LABEL_COLOR, |s| &mut s.color.CONTROL_LABEL_COLOR); // sRGB [204, 204, 212]

pub(super) static CONTROL_LABEL_COLOR_DETACHED: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.CONTROL_LABEL_COLOR_DETACHED, |s| &mut s.color.CONTROL_LABEL_COLOR_DETACHED); // sRGB [131, 131, 138]

pub(super) static CONTROL_LABEL_HOVER_COLOR: crate::style::StyleCell<Option<[f32; 4]>> = crate::style::StyleCell::new(|s| &s.color.CONTROL_LABEL_HOVER_COLOR, |s| &mut s.color.CONTROL_LABEL_HOVER_COLOR);

pub(super) static CONTROL_LABEL_FOCUS_COLOR: crate::style::StyleCell<Option<[f32; 4]>> = crate::style::StyleCell::new(|s| &s.color.CONTROL_LABEL_FOCUS_COLOR, |s| &mut s.color.CONTROL_LABEL_FOCUS_COLOR);

// Transparent by default (alpha 0): a dropdown picks up the surface it sits
// on, and its closed-state chrome is the flush inset trough alone — the
// cce-files treatment, DE-wide. A configured `dropdown color=` opts a theme
// back into a filled face (the paint path judges the RAW alpha).
pub(super) static DROPDOWN_BACKGROUND_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.DROPDOWN_BACKGROUND_COLOR, |s| &mut s.color.DROPDOWN_BACKGROUND_COLOR);

pub(super) static TEXTBOX_PLACEHOLDER_TEXT_COLOR: crate::style::StyleCell<[u8; 3]> = crate::style::StyleCell::new(|s| &s.color.TEXTBOX_PLACEHOLDER_TEXT_COLOR, |s| &mut s.color.TEXTBOX_PLACEHOLDER_TEXT_COLOR);

pub(super) static TEXTBOX_BACKGROUND_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.TEXTBOX_BACKGROUND_COLOR, |s| &mut s.color.TEXTBOX_BACKGROUND_COLOR);

pub(super) static TEXTBOX_BACKGROUND_EDIT_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.TEXTBOX_BACKGROUND_EDIT_COLOR, |s| &mut s.color.TEXTBOX_BACKGROUND_EDIT_COLOR);

pub(super) static BUTTON_BACKGROUND_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.BUTTON_BACKGROUND_COLOR, |s| &mut s.color.BUTTON_BACKGROUND_COLOR);

pub(super) static RAMP_BACKGROUND_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.RAMP_BACKGROUND_COLOR, |s| &mut s.color.RAMP_BACKGROUND_COLOR);

pub(super) static RAMP_BORDER_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.RAMP_BORDER_COLOR, |s| &mut s.color.RAMP_BORDER_COLOR);

pub(super) static CONTROL_PANEL_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.CONTROL_PANEL_COLOR, |s| &mut s.color.CONTROL_PANEL_COLOR); // default #13151cff

pub(super) static CONTROL_PANEL_BORDER_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.CONTROL_PANEL_BORDER_COLOR, |s| &mut s.color.CONTROL_PANEL_BORDER_COLOR); // default #292c37ff

pub(super) static PROGRESS_BG_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.PROGRESS_BG_COLOR, |s| &mut s.color.PROGRESS_BG_COLOR);

pub(super) static PROGRESS_FILL_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.PROGRESS_FILL_COLOR, |s| &mut s.color.PROGRESS_FILL_COLOR);

pub(super) static SPINBOX_DISPLAY_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.SPINBOX_DISPLAY_COLOR, |s| &mut s.color.SPINBOX_DISPLAY_COLOR);

pub(super) static SPINBOX_BUTTON_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.SPINBOX_BUTTON_COLOR, |s| &mut s.color.SPINBOX_BUTTON_COLOR);

pub(super) static SPINBOX_BUTTON_HOVER_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.SPINBOX_BUTTON_HOVER_COLOR, |s| &mut s.color.SPINBOX_BUTTON_HOVER_COLOR);

pub(super) static SPINBOX_TEXT_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.SPINBOX_TEXT_COLOR, |s| &mut s.color.SPINBOX_TEXT_COLOR);

pub(super) static BUTTON_BORDER_COLOR: crate::style::StyleCell<Option<[f32; 4]>> = crate::style::StyleCell::new(|s| &s.color.BUTTON_BORDER_COLOR, |s| &mut s.color.BUTTON_BORDER_COLOR);

pub(super) static DROPDOWN_BORDER_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.DROPDOWN_BORDER_COLOR, |s| &mut s.color.DROPDOWN_BORDER_COLOR);

pub(super) static DROPDOWN_TEXT_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.DROPDOWN_TEXT_COLOR, |s| &mut s.color.DROPDOWN_TEXT_COLOR);

pub(super) static SLIDER_THUMB_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.SLIDER_THUMB_COLOR, |s| &mut s.color.SLIDER_THUMB_COLOR);

pub(super) static SLIDER_THUMB_DRAG_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.SLIDER_THUMB_DRAG_COLOR, |s| &mut s.color.SLIDER_THUMB_DRAG_COLOR);

pub(super) static RANGE_SLIDER_THUMB_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.RANGE_SLIDER_THUMB_COLOR, |s| &mut s.color.RANGE_SLIDER_THUMB_COLOR);

pub(super) static RANGE_SLIDER_THUMB_DRAG_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.RANGE_SLIDER_THUMB_DRAG_COLOR, |s| &mut s.color.RANGE_SLIDER_THUMB_DRAG_COLOR);

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
        control_label_focus_color().unwrap_or_else(control_label_color)
    } else if hovered {
        control_label_hover_color().unwrap_or_else(control_label_color)
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
        control_label_focus_color().unwrap_or_else(control_label_color_detached)
    } else if hovered {
        control_label_hover_color().unwrap_or_else(control_label_color_detached)
    } else {
        control_label_color_detached()
    };
    [
        (linear_to_srgb(c[0]) * 255.0).round() as u8,
        (linear_to_srgb(c[1]) * 255.0).round() as u8,
        (linear_to_srgb(c[2]) * 255.0).round() as u8,
    ]
}
