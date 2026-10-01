//! Measuring text the way the renderer will draw it: widths of runs, and
//! the x of every character boundary in a run (a caret's or a click's
//! position). Shared by `widget::markdown` and `widget::doc_editor`.

use std::collections::HashMap;

use crate::scene::paint::TextAttrs;

/// Text widths in logical px, for one run in one style.
pub trait Measure {
    fn width(&mut self, text: &str, size: f32, font: &str, attrs: TextAttrs) -> f32;
}

/// [`Measure`] through the renderer's own shaping entry
/// (`get_text_buffer_attrs`), so laid-out widths are the drawn widths. It
/// owns its FontSystem: create it with the same font set the app's renderer
/// loads — `system_fonts` mirroring the app's `load_system_fonts` — or
/// face ids and widths will not match. Widths are logical px at the
/// toolkit's current scale factor, cached per run.
pub struct ShapingMeasure {
    fs: cosmic_text::FontSystem,
    cache: HashMap<(String, u32, u32, String, TextAttrs), f32>,
}

impl ShapingMeasure {
    pub fn new(system_fonts: bool) -> ShapingMeasure {
        let fs = if system_fonts { crate::create_font_system_with_system_fonts() } else { crate::create_font_system() };
        ShapingMeasure { fs, cache: HashMap::new() }
    }
}

impl Measure for ShapingMeasure {
    fn width(&mut self, text: &str, size: f32, font: &str, attrs: TextAttrs) -> f32 {
        let scale = crate::scale::scale_factor().max(0.01);
        let key = (text.to_string(), (size * 100.0) as u32, (scale * 1000.0) as u32, font.to_string(), attrs);
        if let Some(w) = self.cache.get(&key) {
            return *w;
        }
        let buf = crate::backend::window_runner::get_text_buffer_attrs(&mut self.fs, text, size, Some(font), attrs);
        // The glyphs' extent, trailing spaces included — the same measure
        // `offsets` ends on. (A layout run's `line_w` leaves trailing
        // whitespace out, so a width taken from it disagreed with where
        // the next run was placed by a space.)
        let w = crate::backend::window_runner::normalized_glyph_starts(&buf, text)
            .into_iter()
            .map(|(_, x, w)| x + w)
            .fold(0.0, f32::max)
            / scale;
        self.cache.insert(key, w);
        w
    }
}

impl ShapingMeasure {
    /// The x (logical px) of every char boundary in `text` shaped as one
    /// run: `(byte offset, x)`, starting at `(0, 0.0)` and ending at
    /// `(text.len(), width)`. A boundary inside a cluster (a ligature, a
    /// combining mark) takes the cluster's start.
    pub fn offsets(&mut self, text: &str, size: f32, font: &str, attrs: TextAttrs) -> Vec<(usize, f32)> {
        let scale = crate::scale::scale_factor().max(0.01);
        let buf = crate::backend::window_runner::get_text_buffer_attrs(&mut self.fs, text, size, Some(font), attrs);
        let glyphs = crate::backend::window_runner::normalized_glyph_starts(&buf, text);
        let mut starts: Vec<(usize, f32)> = Vec::with_capacity(glyphs.len() + 1);
        let mut width = 0.0f32;
        for (start, x, w) in glyphs {
            if starts.last().map_or(true, |&(b, _)| b != start) {
                starts.push((start, x / scale));
            }
            width = width.max((x + w) / scale);
        }
        starts.sort_by_key(|&(b, _)| b);
        let mut out = Vec::with_capacity(text.len() + 1);
        let mut gi = 0;
        let mut last_x = 0.0;
        for (b, _) in text.char_indices() {
            while gi < starts.len() && starts[gi].0 <= b {
                last_x = starts[gi].1;
                gi += 1;
            }
            out.push((b, last_x));
        }
        out.push((text.len(), width));
        out
    }
}
