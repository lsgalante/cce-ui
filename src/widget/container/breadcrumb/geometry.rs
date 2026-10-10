//! The breadcrumb's geometry, from the paint rect: which segments show (eliding the leading ones
//! that do not fit), the segment under a point (following the seams' lean), the plate band, the
//! run box and the seams.

use super::*;

impl Breadcrumb {
    /// The segments to actually paint, each with its BUTTON BOX left edge/width and its
    /// *logical* index (position in [`virtual_segs`]; `None` marks the leading "…" ellipsis).
    /// This is the one source for hit-testing, the hover overlay, the plates, and the text
    /// run.
    ///
    /// Each segment is its own button: box width = the segment's measured ink width plus
    /// `SEG_PAD_X` each side, boxes separated by `SEG_GAP`. (The old abutting-text layout
    /// measured cumulative prefixes so glyph side bearings cancelled; per-box padding
    /// absorbs the bearings instead, so per-segment measurement is enough.)
    ///
    /// When the full run is wider than the container, leading segments are dropped and
    /// replaced with a "…" marker button, so the trailing (current) segments stay visible.
    /// The last segment is always kept.
    pub(super) fn visible_segs(&self, rect: Rect) -> Vec<VisibleSeg> {
        let all = self.virtual_segs();
        let avail = (rect.width - 2.0 * Self::SEG_INSET).max(0.0);

        let (font, size) = Self::font_and_size();
        // The "…" marker is drawn as the `more-horizontal` glyph, so its box
        // is the glyph's width, not the character's.
        let box_w = |s: &str| {
            let inner = if s == "…" { Self::marker_side(size) } else { crate::widget::display::measure_text_width(s, &font, size) };
            inner + 2.0 * SEG_PAD_X
        };

        // Lay a list of (text, logical index) out left-to-right from the widget's left edge,
        // one button box per segment.
        let place = |items: Vec<(String, Option<usize>)>| -> Vec<VisibleSeg> {
            let mut x = rect.x + Self::SEG_INSET;
            items
                .into_iter()
                .map(|(text, logical)| {
                    let w = box_w(&text);
                    let vs = VisibleSeg { text, x, w, logical };
                    x += w + SEG_GAP;
                    vs
                })
                .collect()
        };

        let full: f32 = all.iter().map(|s| box_w(s)).sum::<f32>()
            + SEG_GAP * all.len().saturating_sub(1) as f32;

        if full <= avail || all.len() <= 1 {
            return place(all.into_iter().enumerate().map(|(i, s)| (s, Some(i))).collect());
        }

        // Keep the last segment, then add trailing segments while the "…" marker button plus
        // the kept run still fits; finally prepend the marker and restore left-to-right order.
        let ell = "…".to_string();
        let mut used = box_w(&ell);
        let mut kept: Vec<(String, Option<usize>)> = Vec::new();
        for i in (0..all.len()).rev() {
            let w = SEG_GAP + box_w(&all[i]);
            if !kept.is_empty() && used + w > avail {
                break;
            }
            used += w;
            kept.push((all[i].clone(), Some(i)));
        }
        kept.push((ell, None));
        kept.reverse();
        place(kept)
    }

    /// The segment under `px` at height `py`. The interior boundaries LEAN
    /// (see [`SEG_SLANT`]), so the hit zones are parallelograms, not columns —
    /// testing x alone would put the top-left corner of a segment in its
    /// neighbor, exactly where the seam is drawn furthest from the nominal edge.
    ///
    /// The parallelograms are bounded vertically by the plate band. That bound
    /// is what makes this usable as [`Input::hit`]: `py` otherwise enters only
    /// through `lean`, which slants the seams without ever rejecting a point,
    /// so every segment would claim the full-height column beneath it.
    pub(super) fn seg_at(&self, rect: Rect, px: f32, py: f32) -> Option<usize> {
        let segs = self.visible_segs(rect);
        let (py0, ph) = Self::plate_band(rect);
        if py < py0 || py >= py0 + ph {
            return None;
        }
        let mid = py0 + ph * 0.5;
        // Only interior edges lean; the run's two outer ends stay upright.
        let lean = |i: usize| -> f32 {
            if i == 0 || i >= segs.len() { 0.0 } else { SEG_SLANT * (mid - py) }
        };
        for (i, s) in segs.iter().enumerate() {
            let left = s.x + lean(i);
            let right = s.x + s.w + lean(i + 1);
            if px >= left && px < right {
                return s.logical;
            }
        }
        None
    }

    /// The plate band: (y, height). Full control height since the dropdown
    /// restyle (SEG_INSET = 0) — the seams, hover tint and hit zones all
    /// span the plate.
    pub(super) fn plate_band(rect: Rect) -> (f32, f32) {
        (rect.y + Self::SEG_INSET, (rect.height - 2.0 * Self::SEG_INSET).max(0.0))
    }

    /// The ONE plate the whole segment run shares — (x, y, w, h), the full
    /// control height — or `None` when nothing is laid out.
    ///
    /// The run is a single plate, not a plate per segment: with the segments
    /// abutting, per-segment plates would put a boss wall falling and another
    /// rising within a pixel of each other at every boundary, which stacks two
    /// lighting evaluations and reads far hotter than one seam (the same reason
    /// `Prim::Ridge` exists). The divisions are engraved instead — see [`seams`].
    ///
    /// For flat-path hosts (cce-files) that mirror the relief app-side — the
    /// render_widget geometry path drops relief prims, same as the dropdown's.
    ///
    /// [`seams`]: Breadcrumb::seams
    pub fn run_box(&self, rect: Rect) -> Option<(f32, f32, f32, f32)> {
        let segs = self.visible_segs(rect);
        let first = segs.first()?;
        let last = segs.last()?;
        let (y, h) = Self::plate_band(rect);
        Some((first.x, y, last.x + last.w - first.x, h))
    }

    /// The seam between each pair of abutting segments, as (top, bottom) line
    /// endpoints. Each leans right at the top by `SEG_SLANT`, so it reads as a
    /// "/" between the two names. Only interior boundaries appear here — the
    /// run's outer ends are the plate's own upright edges.
    pub fn seams(&self, rect: Rect) -> Vec<((f32, f32), (f32, f32))> {
        let segs = self.visible_segs(rect);
        let (y, h) = Self::plate_band(rect);
        let run = SEG_SLANT * h * 0.5;
        segs.iter()
            .skip(1)
            .map(|s| ((s.x + run, y), (s.x - run, y + h)))
            .collect()
    }

    /// How far the run's rounded end stands in from its upright edge at height
    /// `yc`, for a band `y..y + h` with corner radius `r` — the wash's outer
    /// ends follow the plate's silhouette by this. The corner is the DE's
    /// family, `|x|^n + |y|^n = r^n` at exponent `n` (`layout::corner_shape`):
    /// the plate is drawn at the nominal radius in that family, so a circular
    /// arc here (as it was) cut inside a squircle plate's corners and left
    /// them unwashed — some 4 px at the top and bottom rows at n = 4.5.
    pub(super) fn end_inset(r: f32, n: f32, y: f32, h: f32, yc: f32) -> f32 {
        let dy = if yc < y + r {
            r - (yc - y)
        } else if yc > y + h - r {
            yc - (y + h - r)
        } else {
            return 0.0;
        };
        if r <= 0.0 {
            return 0.0;
        }
        let t = (dy / r).clamp(0.0, 1.0);
        r - r * (1.0 - t.powf(n)).max(0.0).powf(1.0 / n)
    }
}
