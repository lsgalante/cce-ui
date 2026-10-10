//! Shaped runs: clusters with their x extents, for caret and hit maths, and the bidi levels and
//! visual order of a paragraph and its runs.

use super::*;

/// Byte-offset → x mapping of single-line `text`, shaped exactly as the renderer draws it —
/// same buffer cache as the draw, so this is a lookup when the text is already on screen.
/// Returns ascending `(byte_idx, x)` pairs (one per cluster start, logical px, relative to
/// the text origin), terminated by `(text.len(), total_advance)`. A cluster's x is its
/// LEADING edge — its left in left-to-right text, its right in right-to-left — so the pairs
/// are ascending in bytes but not in x where the text turns; the closing pair is the width,
/// which is where the caret after the text stands only in left-to-right text. For carets and
/// selections in text of either direction use [`shaped_run`]. This is the correct
/// source for caret placement and click→cursor mapping in hand-rolled text fields:
/// `measure_text_width` reports SVG-rasterized inked extent through fontdb's family
/// resolution, which disagrees with cosmic-text's advance and can even resolve a
/// different face — a caret placed with it drifts off the drawn glyphs.
pub fn shaped_cluster_offsets(
    fs: &mut FontSystem,
    text: &str,
    size: f32,
    font: Option<&str>,
) -> Vec<(usize, f32)> {
    let scale = crate::scale::scale_factor();
    let buffer = shared_text_buffer_at(fs, text, size, font, crate::scene::paint::TextAttrs::default(), scale);
    let run = shaped_run(&buffer, text, scale);
    let mut out: Vec<(usize, f32)> = run.clusters.iter().map(|c| (c.start, c.leading())).collect();
    out.push((text.len(), run.width));
    out
}

/// Whether `text` is a right-to-left paragraph: its first strong character is right to left
/// (Hebrew, Arabic, …), as the Unicode bidirectional algorithm decides a paragraph's base
/// direction. Text with no strong character at all is left to right.
pub fn paragraph_rtl(text: &str) -> bool {
    matches!(unicode_bidi::get_base_direction(text), unicode_bidi::Direction::Rtl)
}

/// The visual order of runs of one line, given each run's text, in a paragraph whose base
/// direction is `rtl`: the bidirectional algorithm's reordering (rule L2) at the
/// granularity of runs. A run is at the paragraph's level when it has no strong character,
/// one level up when its first strong character goes against the paragraph, and the
/// sequences at each level from the highest down are reversed. So in a left-to-right line
/// two Hebrew runs side by side swap places, and in a right-to-left line every run is
/// placed from the right while English runs keep their order among themselves. Returns
/// indices into `runs`, left to right.
pub fn visual_run_order(runs: &[&str], rtl: bool) -> Vec<usize> {
    let base: u8 = if rtl { 1 } else { 0 };
    let levels: Vec<u8> = runs
        .iter()
        .map(|t| match unicode_bidi::get_base_direction(*t) {
            unicode_bidi::Direction::Rtl => if base % 2 == 1 { base } else { base + 1 },
            unicode_bidi::Direction::Ltr => if base.is_multiple_of(2) { base } else { base + 1 },
            unicode_bidi::Direction::Mixed => base,
        })
        .collect();
    visual_order(&levels)
}

/// The embedding level of every byte of `text` as one paragraph (`rtl` its base
/// direction), neutrals resolved from their neighbours — the bidirectional algorithm's
/// levels, rules W1–I2. Odd is right to left.
pub fn bidi_levels(text: &str, rtl: bool) -> Vec<u8> {
    let base = if rtl { unicode_bidi::Level::rtl() } else { unicode_bidi::Level::ltr() };
    unicode_bidi::BidiInfo::new(text, Some(base)).levels.iter().map(|l| l.number()).collect()
}

/// Left-to-right order of items at `levels` (rule L2): from the highest level down to the
/// lowest odd one, every maximal run of items at that level or above is reversed.
pub fn visual_order(levels: &[u8]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..levels.len()).collect();
    let max = levels.iter().copied().max().unwrap_or(0);
    for level in (1..=max).rev() {
        let mut i = 0;
        while i < order.len() {
            if levels[order[i]] >= level {
                let start = i;
                while i < order.len() && levels[order[i]] >= level {
                    i += 1;
                }
                order[start..i].reverse();
            } else {
                i += 1;
            }
        }
    }
    order
}

/// One cluster of a [`ShapedRun`]: the bytes `start..end` drawn as one unit (a glyph, a
/// ligature, a base with its marks), spanning `x0..x1` (logical px, left to right whatever
/// the direction), and whether it reads right to left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapedCluster {
    pub start: usize,
    pub end: usize,
    pub x0: f32,
    pub x1: f32,
    pub rtl: bool,
}

impl ShapedCluster {
    /// Where the caret before this cluster stands: its left in left-to-right text, its
    /// right in right-to-left.
    pub fn leading(&self) -> f32 {
        if self.rtl { self.x1 } else { self.x0 }
    }

    /// Where the caret after it stands.
    pub fn trailing(&self) -> f32 {
        if self.rtl { self.x0 } else { self.x1 }
    }
}

/// A single line of text as shaped, in the terms an editor needs whatever its direction:
/// where the caret stands at every char boundary, the clusters in logical order with their
/// boxes, the width, and the paragraph's base direction (cosmic-text takes it from the first
/// strong character, as the Unicode bidirectional algorithm does).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShapedRun {
    /// `(byte, x)` for every char boundary, ascending in bytes, ending at `text.len()`: the
    /// caret before the char at `byte` stands at its cluster's leading edge (a boundary
    /// inside a cluster takes the cluster's), and the caret after the text at the last
    /// char's trailing edge — the RIGHT end of left-to-right text, the LEFT end of
    /// right-to-left. In mixed text x is not monotonic.
    pub stops: Vec<(usize, f32)>,
    /// The clusters, in logical order (by `start`).
    pub clusters: Vec<ShapedCluster>,
    /// The glyphs' extent, trailing spaces included.
    pub width: f32,
    /// The paragraph's base direction.
    pub rtl: bool,
}

impl ShapedRun {
    /// The char boundary nearest x: a click's caret.
    pub fn index_at(&self, x: f32) -> usize {
        self.stops
            .iter()
            .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))
            .map_or(0, |s| s.0)
    }

    /// The caret x before byte `at` (the nearest boundary at or before it).
    pub fn x_of(&self, at: usize) -> f32 {
        self.stops.iter().rev().find(|s| s.0 <= at).map_or(0.0, |s| s.1)
    }

    /// The x spans the bytes `a..b` cover, left to right and merged where they touch: one
    /// span in text of one direction, more where a selection crosses a change of direction
    /// (the logical range is then visually apart).
    pub fn spans(&self, a: usize, b: usize) -> Vec<(f32, f32)> {
        let mut boxes: Vec<(f32, f32)> =
            self.clusters.iter().filter(|c| c.start < b && c.end > a).map(|c| (c.x0, c.x1)).collect();
        boxes.sort_by(|p, q| p.0.total_cmp(&q.0));
        let mut out: Vec<(f32, f32)> = Vec::new();
        for (x0, x1) in boxes {
            match out.last_mut() {
                Some(last) if x0 <= last.1 + 0.5 => last.1 = last.1.max(x1),
                _ => out.push((x0, x1)),
            }
        }
        out
    }
}

/// Shape-derived positions of single-line `text` from its `buffer` (see [`ShapedRun`]),
/// in logical px at `scale`.
pub fn shaped_run(buffer: &Buffer, text: &str, scale: f32) -> ShapedRun {
    let scale = scale.max(0.01);
    let mut rtl = false;
    let mut glyphs: Vec<ShapedCluster> = Vec::new();
    let mut first = true;
    for run in buffer.layout_runs() {
        if first {
            rtl = run.rtl;
            first = false;
        }
        for g in run.glyphs {
            glyphs.push(ShapedCluster { start: g.start, end: g.end, x0: g.x / scale, x1: (g.x + g.w) / scale, rtl: g.level.is_rtl() });
        }
    }
    // The Basic shaping path's span-relative starts (see `normalized_glyph_starts`): ASCII
    // only, one glyph per char in logical order.
    if text.is_ascii() && glyphs.windows(2).any(|w| w[1].start < w[0].start) {
        for (g, (i, _)) in glyphs.iter_mut().zip(text.char_indices()) {
            g.start = i;
            g.end = i + 1;
        }
    }
    // One cluster per byte range: a base and its marks are several glyphs of one cluster.
    glyphs.sort_by(|a, b| a.start.cmp(&b.start).then(a.x0.total_cmp(&b.x0)));
    let mut clusters: Vec<ShapedCluster> = Vec::new();
    for g in glyphs {
        match clusters.last_mut() {
            Some(c) if c.start == g.start && c.end == g.end => {
                c.x0 = c.x0.min(g.x0);
                c.x1 = c.x1.max(g.x1);
            }
            _ => clusters.push(g),
        }
    }
    let width = clusters.iter().map(|c| c.x1).fold(0.0, f32::max);
    let mut stops: Vec<(usize, f32)> = Vec::with_capacity(text.len() + 1);
    let mut ci = 0;
    let mut last_x = if rtl { width } else { 0.0 };
    for (b, _) in text.char_indices() {
        while ci < clusters.len() && clusters[ci].end <= b {
            ci += 1;
        }
        if let Some(c) = clusters.get(ci).filter(|c| c.start <= b) {
            last_x = c.leading();
        }
        stops.push((b, last_x));
    }
    let end_x = match clusters.last() {
        Some(c) => c.trailing(),
        None => 0.0,
    };
    stops.push((text.len(), end_x));
    ShapedRun { stops, clusters, width, rtl }
}

/// Every glyph of `buffer`'s layout runs as `(start_byte, x, w)` (physical px),
/// with `start` normalized to be text-relative.
///
/// Exists because cosmic-text 0.12's `Shaping::Basic` path (`shape_skip`) emits
/// `LayoutGlyph::start` relative to the shape SPAN — it resets to 0 at every
/// word — while the Advanced path emits line-relative starts. `shaping_for`
/// picks Basic exactly for ASCII text in a monospace family (the DE's default
/// control font), so any multi-word value hit the bug: offsets keyed by those
/// starts collide on the low columns and the caret/selection walk off the
/// glyphs. That path shapes strictly one glyph per char in logical order, and
/// only ASCII text — so in ASCII text a reset means it, and byte starts are
/// rebuilt by walking the text's chars. Anywhere else glyph starts fall
/// because the text turns right to left (glyphs are in visual order), and are
/// kept: until 2026-10-08 any fall was read as the reset, which scrambled
/// every right-to-left run. `text` must be the single line the buffer was
/// shaped from.
pub(crate) fn normalized_glyph_starts(buffer: &Buffer, text: &str) -> Vec<(usize, f32, f32)> {
    let mut glyphs: Vec<(usize, f32, f32)> = Vec::new();
    let mut monotonic = true;
    let mut prev = 0usize;
    for run in buffer.layout_runs() {
        for g in run.glyphs {
            if g.start < prev {
                monotonic = false;
            }
            prev = g.start;
            glyphs.push((g.start, g.x, g.w));
        }
    }
    if !monotonic && text.is_ascii() {
        let mut starts = text.char_indices().map(|(i, _)| i);
        for g in glyphs.iter_mut() {
            g.0 = starts.next().unwrap_or(text.len());
        }
    }
    glyphs
}
