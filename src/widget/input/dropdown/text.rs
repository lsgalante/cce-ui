//! Measuring the trigger's text: monospace cell widths, shaped clusters, the displayed text
//! (without a mark) and its width.

use super::*;

/// Monospace detection — the paint pass lays characters out on a fixed cell in a monospace font
/// and on measured per-character advances otherwise, so the sizing pass must branch the same way.
pub(super) fn is_monospace_font(font_family: &str, font_size: f32) -> bool {
    let w_i10 = crate::widget::display::measure_text_width("iiiiiiiiii", font_family, font_size);
    let w_m10 = crate::widget::display::measure_text_width("mmmmmmmmmm", font_family, font_size);
    (w_i10 - w_m10).abs() < 5.0
}

/// The fixed per-character cell of a monospace font, taken as the slope between a 10- and a
/// 20-`m` run so any constant side bearing in the measurement cancels out.
pub(super) fn monospace_cell_width(font_family: &str, font_size: f32) -> f32 {
    let w_m10 = crate::widget::display::measure_text_width("mmmmmmmmmm", font_family, font_size);
    let w_m20 = crate::widget::display::measure_text_width("mmmmmmmmmmmmmmmmmmmm", font_family, font_size);
    ((w_m20 - w_m10) / 10.0).max(1.0)
}

/// The advance width `paint_text` will actually lay `text` out to.
///
/// This is the single source of truth shared by the sizing pass (`content_width` /
/// `display_width`) and the paint pass. It is deliberately NOT a plain `measure_text_width`: the
/// paint pass advances on the monospace cell or on the M-dummy trick, and either can exceed the
/// raw ink measure. Sizing a dropdown from the ink measure therefore left the trigger a few px too
/// narrow and tripped its own right-edge fade — badly under a monospace UI font like the default
/// Berkeley Mono. Keep this in step with `paint_text`.
///
/// Shaped first ([`shaped_clusters`]), which is exactly where the glyphs land; the measured
/// fallbacks below are for a process with no font system to shape through.
pub(super) fn text_advance(text: &str, font_family: &str, font_size: f32) -> f32 {
    let n = text.chars().count();
    if n == 0 {
        return 0.0;
    }
    if let Some(&(_, total)) = shaped_clusters(text, font_size).as_ref().and_then(|c| c.last()) {
        return total;
    }
    if is_monospace_font(font_family, font_size) {
        n as f32 * monospace_cell_width(font_family, font_size)
    } else {
        let w_dummy = crate::widget::display::measure_text_width("M", font_family, font_size);
        let measure_str = format!("{}M", text);
        (crate::widget::display::measure_text_width(&measure_str, font_family, font_size) - w_dummy).max(0.0)
    }
}

/// `text` shaped in the trigger's own font, as the renderer draws it: each cluster's start
/// `(byte, x)` in logical px, ending on `(text.len(), total advance)` — `None` when there is no
/// font system to shape through or the shape came back empty.
///
/// The trigger draws its text a cluster at a time (so it can fade them one by one), and these
/// are the offsets it places them at. Until 2026-09-30 they were measured instead — a prefix
/// through `measure_text_width`, with every character then pushed at least a pixel past the
/// previous one's MEASURED ink — and a lone narrow glyph measures wider than it advances, so
/// "Create" drew as "Cr eat e".
pub(super) fn shaped_clusters(text: &str, font_size: f32) -> Option<Vec<(usize, f32)>> {
    let font = crate::layout::control_label_font_detached();
    let mut fs = crate::geometry_font_system().lock().ok()?;
    let clusters =
        crate::backend::text::shaped_cluster_offsets(&mut fs, text, font_size, Some(&font));
    (clusters.len() > 1 && clusters.last().is_some_and(|&(_, total)| total > 0.0)).then_some(clusters)
}

/// [`shaped_clusters`]' fallback, a character a piece: on the monospace cell, or at a
/// measured prefix with each character kept a pixel clear of the last one's measured ink.
pub(super) fn measured_pieces(text: &str, font_family: &str, font_size: f32, total_advance: f32) -> Vec<(String, f32, f32)> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    if is_monospace_font(font_family, font_size) {
        let cell = monospace_cell_width(font_family, font_size);
        return chars.iter().enumerate().map(|(i, c)| (c.to_string(), i as f32 * cell, cell)).collect();
    }
    let w_dummy = crate::widget::display::measure_text_width("M", font_family, font_size);
    let mut offsets = Vec::with_capacity(n);
    let mut prefix = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if i > 0 {
            prefix.push(chars[i - 1]);
        }
        let measured = if i == 0 {
            0.0
        } else {
            let w = crate::widget::display::measure_text_width(&format!("{prefix}M"), font_family, font_size);
            (w - w_dummy).max(0.0)
        };
        offsets.push((c, measured));
    }
    let mut out = Vec::with_capacity(n);
    let mut prev_end = 0.0f32;
    for i in 0..n {
        let (c, mut offset) = offsets[i];
        if i > 0 {
            offset = offset.max(prev_end + 1.0);
        }
        let next = if i + 1 < n { offsets[i + 1].1 } else { total_advance };
        out.push((c.to_string(), offset, next - offset));
        prev_end = offset + crate::widget::display::measure_text_width(&c.to_string(), font_family, font_size);
    }
    out
}

impl Dropdown {

    pub fn content_width(&self) -> f32 {
        let font_setting = crate::layout::control_label_font_detached();
        let (font_family, font_size_opt) = crate::layout::parse_font_string(&font_setting);
        let font_size = font_size_opt.unwrap_or(12.0);
        let mut max_w = 0.0f32;
        let marks = self.mark_column();
        for opt in &self.options {
            let (_, label) = crate::widget::context_menu::split_mark(opt);
            let opt_w = marks + text_advance(label, &font_family, font_size) + Self::LABEL_INSET;
            if opt_w > max_w {
                max_w = opt_w;
            }
        }
        max_w
    }

    /// The text shown on the collapsed trigger — the fixed `custom_display_text` (menu-button
    /// mode) if set, otherwise the selected option. Mirrors the selection logic in `paint_text`.
    pub(super) fn display_text(&self) -> String {
        if let Some(ref custom_text) = self.custom_display_text {
            custom_text.clone()
        } else {
            let opt = self.options.get(self.selected).map(String::as_str).unwrap_or_default();
            crate::widget::context_menu::split_mark(opt).1.to_string()
        }
    }

    /// Trigger width sized to the collapsed display text rather than the widest option (via
    /// `content_width`). Used by menu-button dropdowns whose label is fixed, so "File"/"Edit" don't
    /// stretch to their longest menu entry. Shares `LABEL_INSET` so the label fits without fading.
    pub(super) fn display_width(&self) -> f32 {
        let font_setting = crate::layout::control_label_font_detached();
        let (font_family, font_size_opt) = crate::layout::parse_font_string(&font_setting);
        let font_size = font_size_opt.unwrap_or(12.0);
        text_advance(&self.display_text(), &font_family, font_size) + Self::LABEL_INSET
    }
}
