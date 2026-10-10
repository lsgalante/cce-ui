//! Measuring text the way the renderer will draw it: widths of runs, and
//! the x of every character boundary in a run (a caret's or a click's
//! position). Shared by `widget::markdown` and `widget::doc_editor`.

use std::collections::HashMap;

use crate::scene::paint::TextAttrs;

pub use crate::text::{ShapedCluster, ShapedRun};

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
        let buf = crate::text::shared_text_buffer_at(&mut self.fs, text, size, Some(font), attrs, scale);
        // The glyphs' extent, trailing spaces included — the same measure
        // `offsets` ends on. (A layout run's `line_w` leaves trailing
        // whitespace out, so a width taken from it disagreed with where
        // the next run was placed by a space.)
        let w = crate::text::normalized_glyph_starts(&buf, text)
            .into_iter()
            .map(|(_, x, w)| x + w)
            .fold(0.0, f32::max)
            / scale;
        self.cache.insert(key, w);
        w
    }
}

impl ShapingMeasure {
    /// `text` shaped as one run, in an editor's terms: the caret's x at every char
    /// boundary, the clusters with their boxes, the width and the base direction — right
    /// for text of either direction (see [`ShapedRun`]).
    pub fn shape(&mut self, text: &str, size: f32, font: &str, attrs: TextAttrs) -> ShapedRun {
        let scale = crate::scale::scale_factor().max(0.01);
        let buf = crate::text::shared_text_buffer_at(&mut self.fs, text, size, Some(font), attrs, scale);
        crate::text::shaped_run(&buf, text, scale)
    }

    /// The x (logical px) of every char boundary in `text` shaped as one
    /// run: `(byte offset, x)`, from `(0, _)` to `(text.len(), _)` — the
    /// caret's place before each char, [`ShapedRun::stops`]. In left-to-right
    /// text they run from 0 to the width; in right-to-left text they fall. A
    /// boundary inside a cluster (a ligature, a combining mark) takes the
    /// cluster's place. The width is [`Measure::width`].
    pub fn offsets(&mut self, text: &str, size: f32, font: &str, attrs: TextAttrs) -> Vec<(usize, f32)> {
        self.shape(text, size, font, attrs).stops
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Carets in right-to-left text stand at each letter's right edge and fall across the
    /// run; the caret after it is its left end; a click finds the boundary nearest it; and a
    /// selection crossing a change of direction is two spans. The bidi levels come from the
    /// text, not the font, so this holds whatever face draws the letters.
    #[test]
    fn carets_follow_the_text_in_either_direction() {
        let mut m = ShapingMeasure::new(false);
        let font = crate::layout::control_label_font();
        let attrs = TextAttrs::default();

        let ltr = m.shape("abc", 14.0, &font, attrs);
        assert!(!ltr.rtl);
        let xs: Vec<f32> = ltr.stops.iter().map(|s| s.1).collect();
        assert!(xs.windows(2).all(|w| w[1] > w[0]), "left to right, carets rise: {xs:?}");
        assert!((xs[3] - ltr.width).abs() < 0.01, "the caret after it is its right end");

        let heb = "שלום"; // four letters, two bytes each
        let rtl = m.shape(heb, 14.0, &font, attrs);
        assert!(rtl.rtl, "the first strong letter is Hebrew: the paragraph is right to left");
        let xs: Vec<f32> = rtl.stops.iter().map(|s| s.1).collect();
        assert_eq!(rtl.stops.iter().map(|s| s.0).collect::<Vec<_>>(), [0, 2, 4, 6, 8]);
        assert!(xs.windows(2).all(|w| w[1] < w[0]), "right to left, carets fall: {xs:?}");
        assert!((xs[0] - rtl.width).abs() < 0.01 && xs[4].abs() < 0.01, "from the right end to the left: {xs:?}");
        assert_eq!(rtl.index_at(rtl.width + 5.0), 0, "a click past the right end is the start");
        assert_eq!(rtl.index_at(-5.0), heb.len(), "past the left end, the end");

        // "ab" then a Hebrew word: the logical range a..ש (bytes 1..4) is the "b" on the left
        // and the Hebrew word's FIRST letter at its far right — two spans.
        let mixed = m.shape("ab שלום", 14.0, &font, attrs);
        assert!(!mixed.rtl, "the first strong letter is Latin");
        let spans = mixed.spans(1, 5);
        assert_eq!(spans.len(), 2, "visually apart: {spans:?} in {:?}", mixed.clusters);
        assert!(spans[0].1 < spans[1].0);
    }
}
