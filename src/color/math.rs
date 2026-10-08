//! Colour arithmetic: sRGB/linear conversion (the hex parsers come from `cce_core::color`),
//! OKLab, and the perceptual fade.

use super::*;

pub use cce_core::color::{linear_to_srgb, parse_hex_bytes, parse_hex_rgb, parse_hex_rgba, parse_hex_rgba_linear, srgb_to_linear};

pub fn to_linear(color: [f32; 4]) -> [f32; 4] {
    [
        srgb_to_linear(color[0]),
        srgb_to_linear(color[1]),
        srgb_to_linear(color[2]),
        color[3],
    ]
}

pub fn to_srgb(color: [f32; 4]) -> [f32; 4] {
    [
        linear_to_srgb(color[0]),
        linear_to_srgb(color[1]),
        linear_to_srgb(color[2]),
        color[3],
    ]
}

pub fn to_linear_rgb(color: [f32; 3]) -> [f32; 3] {
    [
        srgb_to_linear(color[0]),
        srgb_to_linear(color[1]),
        srgb_to_linear(color[2]),
    ]
}

pub fn to_srgb_rgb(color: [f32; 3]) -> [f32; 3] {
    [
        linear_to_srgb(color[0]),
        linear_to_srgb(color[1]),
        linear_to_srgb(color[2]),
    ]
}

pub fn linear_srgb_to_oklab(rgb: [f32; 3]) -> [f32; 3] {
    let l = 0.4122214708 * rgb[0] + 0.5363325363 * rgb[1] + 0.0514459929 * rgb[2];
    let m = 0.2119034982 * rgb[0] + 0.6806995451 * rgb[1] + 0.1073969566 * rgb[2];
    let s = 0.0883024619 * rgb[0] + 0.2817188376 * rgb[1] + 0.6299787005 * rgb[2];

    let l_ = l.max(0.0).powf(1.0 / 3.0);
    let m_ = m.max(0.0).powf(1.0 / 3.0);
    let s_ = s.max(0.0).powf(1.0 / 3.0);

    [
        0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_,
        1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_,
        0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_,
    ]
}

pub fn oklab_to_linear_srgb(lab: [f32; 3]) -> [f32; 3] {
    let l_ = lab[0] + 0.3963377774 * lab[1] + 0.2158037573 * lab[2];
    let m_ = lab[0] - 0.1055613458 * lab[1] - 0.0638541728 * lab[2];
    let s_ = lab[0] - 0.0894841775 * lab[1] - 1.2914855480 * lab[2];

    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;

    [
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
    ]
}

/// The alpha that makes an overlay's fade-out read as an EVEN fade to the eye.
///
/// `t` runs 0 (overlay at `max_alpha`) to 1 (fully faded); `overlay` and `backdrop` are
/// linear-light RGB, the backdrop being whatever the fade composites onto.
///
/// A linear alpha ramp is not a linear fade: the surface is an sRGB attachment, so the GPU
/// blends in LINEAR light, and perceived lightness goes as roughly the cube root of that. A
/// straight alpha ramp therefore hangs bright through the middle and then dives near the
/// end. This walks perceived lightness linearly instead and solves back for the alpha that
/// lands on it — exactly, since the composite is linear in alpha:
///
/// ```text
/// Y(a) = a·Y_overlay + (1 - a)·Y_backdrop        (blending, in linear light)
/// L    ≈ cbrt(Y)                                 (perception — OKLab's L, and CIE L* to within a hair)
/// ```
///
/// Solved on luminance rather than per channel because a single alpha can only satisfy one
/// axis, and lightness is the one the eye reads a fade by. With no lightness contrast to
/// shape (a fade between equally-bright colors) it falls back to the linear ramp.
pub fn perceptual_fade_alpha(t: f32, overlay: [f32; 3], backdrop: [f32; 3], max_alpha: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let luminance = |c: [f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    let y_over = luminance(overlay);
    let y_back = luminance(backdrop);
    let contrast = y_over - y_back;
    if contrast.abs() < 1e-4 {
        return max_alpha * (1.0 - t);
    }
    // Walk L from the fully-applied composite down to the bare backdrop.
    let l = |y: f32| y.max(0.0).cbrt();
    let y_full = max_alpha * y_over + (1.0 - max_alpha) * y_back;
    let l_t = l(y_full) + (l(y_back) - l(y_full)) * t;
    let y_t = l_t * l_t * l_t;
    ((y_t - y_back) / contrast).clamp(0.0, max_alpha)
}

pub fn sidebar_bg_color() -> [f32; 4] {
    load_colors_once();
    let mut color = *SIDEBAR_BG_COLOR.read().unwrap();
    if let Some(opacity) = read_opacity_if_configured() {
        color[3] = opacity;
    }
    color
}

pub fn set_sidebar_bg_color(color: [f32; 4]) {
    style_write(&SIDEBAR_BG_COLOR, color);
}

pub fn highlight_primary_color() -> [f32; 4] {
    load_colors_once();
    style_read(&HIGHLIGHT_PRIMARY_COLOR)
}

pub fn set_highlight_primary_color(color: [f32; 4]) {
    if let Ok(mut lock) = HIGHLIGHT_PRIMARY_COLOR.write() {
        *lock = [color[0], color[1], color[2], 0.12];
    }
}

pub fn menubar_tab_label_color() -> [f32; 4] {
    load_colors_once();
    style_read(&MENUBAR_TAB_LABEL_COLOR)
}

pub fn set_menubar_tab_label_color(color: [f32; 4]) {
    style_write(&MENUBAR_TAB_LABEL_COLOR, color);
}
