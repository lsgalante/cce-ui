//! Row types: what a row's type string (`slider:lo:hi:dec`, `float3:lo:hi:trackball`,
//! `textpick:a,b`, …) says, and parsing a row's value.

/// A text row, in either variant: plain (`"text"`), or with a completion
/// picker (`"textpick:a,b,c"` — the Houdini-style attribute/group chooser: a
/// TextBox plus a slim menu-button Dropdown at its right edge whose pick
/// fills the box; the host supplies the candidates in the type string).
pub(super) fn is_text_row(t: &str) -> bool {
    t == "text" || t.starts_with("textpick")
}

/// Display precision for a slider row from the type string's optional 4th
/// segment (`slider:min:max:decimals`); 2 when absent — the pane-wide
/// historical default.
pub(super) fn slider_decimals(ptype: &str) -> usize {
    ptype.split(':').nth(3).and_then(|s| s.parse().ok()).unwrap_or(2)
}

/// Whether a row of type `t` is a vector of sliders: `float2`, `float3` or
/// `float4`, each `floatN:lo:hi` — the one [`Float3`] group with that many
/// rows ([`Float3::set_components`]).
pub(super) fn is_vec_row(t: &str) -> bool {
    t.starts_with("float2") || t.starts_with("float3") || t.starts_with("float4")
}

/// A SEPARATOR row: a hairline between two runs of rows, a pixel tall with
/// the row gap either side, that nothing focuses, hovers or edits — what a
/// host puts between groups of parameters that are about different things.
/// Its key and value mean nothing; a host writing rows back skips it.
pub const SEPARATOR: &str = "separator";

/// Whether a slider or vector row's range is SOFT: a `soft` segment
/// anywhere after the range (`slider:lo:hi:dec:soft`,
/// `float3:lo:hi:trackball:soft`). A value typed past an end widens the
/// row's range rather than being clamped to it (`Slider::set_soft`); the
/// host is expected to choose the range around the value.
pub(super) fn is_soft_row(t: &str) -> bool {
    t.split(':').skip(3).any(|s| s == "soft")
}

/// How many rows a vector row has: the digit after `float`.
pub(super) fn vec_row_n(t: &str) -> usize {
    t.get(5..6).and_then(|d| d.parse().ok()).unwrap_or(3)
}

pub(super) fn parse_slider_range(ptype: &str) -> (f32, f32) {
    if ptype.starts_with("slider:") || (is_vec_row(ptype) && ptype.contains(':')) {
        let parts: Vec<&str> = ptype.split(':').collect();
        if parts.len() >= 3 {
            if let (Ok(min), Ok(max)) = (parts[1].parse::<f32>(), parts[2].parse::<f32>()) {
                return (min, max);
            }
        }
    }
    (0.0, 2.0)
}


pub(super) fn parse_hex_to_rgb(s: &str) -> Option<[u8; 3]> {
    crate::color::parse_hex_bytes(s).map(|[r, g, b, _]| [r, g, b])
}

pub(super) fn parse_hex_to_rgba(s: &str) -> Option<[u8; 4]> {
    crate::color::parse_hex_bytes(s)
}

pub(super) fn parse_spinbox_range(ptype: &str) -> (i32, i32, i32) {
    if ptype.starts_with("spinbox:") {
        let parts: Vec<&str> = ptype.split(':').collect();
        if parts.len() >= 4 {
            if let (Ok(min), Ok(max), Ok(step)) = (parts[1].parse::<i32>(), parts[2].parse::<i32>(), parts[3].parse::<i32>()) {
                return (min, max, step);
            }
        } else if parts.len() == 3 {
            if let (Ok(min), Ok(max)) = (parts[1].parse::<i32>(), parts[2].parse::<i32>()) {
                return (min, max, 1);
            }
        }
    }
    (0, 10000, 1)
}

/// A vector row's text as `n` normalized values: each component over the
/// row's range, mid-range where the text has none.
pub(super) fn parse_vec_value(val_str: &str, min: f32, max: f32, n: usize) -> Vec<f32> {
    let mut out = vec![0.5; n];
    let parts: Vec<&str> = val_str
        .split([':', ',', ' '])
        .filter(|s| !s.is_empty())
        .collect();
    for i in 0..n {
        if i < parts.len() {
            if let Ok(v) = parts[i].parse::<f32>() {
                let range = max - min;
                if range != 0.0 {
                    out[i] = ((v - min) / range).clamp(0.0, 1.0);
                } else {
                    out[i] = 0.0;
                }
            }
        }
    }
    out
}
