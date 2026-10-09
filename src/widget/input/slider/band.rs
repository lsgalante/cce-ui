//! The band: its height along the track (a flat band swelling to a bulge at each thumb, capsule
//! tips at the ends) and its shape painted from that profile. Public so app-owned scrubbers (the
//! designer's playbar) draw the same band.

use super::*;

/// The band's height at `x`: the flat band thickness (`style.control.slider.
/// band_thickness`), rising through a raised-cosine bell to the bulge height
/// around each of `centers` (`bulge_width` half-span, `bulge_height` peak), the
/// bell raised to a power so the flanks taper long and the crest stays plump —
/// mid-digestion, not a triangle; and, for a RangeSlider, one band thickness
/// more across `range` (between its two swells). Capsule tips: the profile
/// shrinks over a circular cap inside each track end — the band ends round, not
/// square-cut, and the well contour and wheel halo (both measured from here)
/// round with it. Public with `paint_band_shape` so app-owned scrubbers (the
/// designer's playbar) draw the same band.
pub fn band_profile(track_x: f32, track_w: f32, h: f32, x: f32, centers: &[f32], range: Option<(f32, f32)>) -> f32 {
    let band_t = crate::layout::slider_band_thickness().max(0.5);
    let bulge_h = crate::layout::slider_bulge_height().clamp(band_t, h);
    let bulge_w = crate::layout::slider_bulge_width().max(2.0);
    let mut bell = 0.0f32;
    for &vx in centers {
        let t = ((x - vx) / bulge_w).clamp(-1.0, 1.0);
        bell = bell.max(0.5 * (1.0 + (std::f32::consts::PI * t).cos()));
    }
    let base = if range.is_some_and(|(lo, hi)| x >= lo && x <= hi) { 2.0 * band_t } else { band_t };
    let h = base + (bulge_h - base) * bell.powf(1.35);
    let d = (x - track_x).min(track_x + track_w - x);
    let r = (h * 0.5).max(0.5);
    if d < r {
        let t = ((r - d.max(0.0)) / r).min(1.0);
        return h * (1.0 - t * t).max(0.0).sqrt();
    }
    h
}

/// The band, drawn from its height `profile` (`band_profile`): the well first —
/// the band appears INSET, a carve whose contour follows the drawn shape a small
/// gap outside it. The rect recess prims can't follow a bell, so the walls are
/// hand-shaded per column from the same profile the fill samples: a shadow band
/// hugging the top contour, a lit band along the bottom (the DE light sits
/// upper-left), stepped alphas like the legacy banded bevels, amplitude riding
/// `bevel_depth` like every other relief wall's. Then the band itself, one
/// column per pixel with a hair of overlap so AA seams can't open. The one
/// painter behind Slider, RangeSlider and Float3's rows.
pub fn paint_band_shape(ctx: &mut PaintCtx, track_x: f32, track_w: f32, cy: f32, color: [f32; 4], profile: &dyn Fn(f32) -> f32) {
    paint_band_shape_colored(ctx, track_x, track_w, cy, &|_| color, profile);
}

/// [`paint_band_shape`] with the band's colour sampled per column
/// (`color_at(x)`): a RangeSlider lights only the swell the keyboard is on.
pub fn paint_band_shape_colored(ctx: &mut PaintCtx, track_x: f32, track_w: f32, cy: f32, color_at: &dyn Fn(f32) -> [f32; 4], profile: &dyn Fn(f32) -> f32) {
    const WELL_GAP: f32 = 4.0;
    const WELL_WALL: f32 = 3.0;
    const WALL_STEPS: usize = 3;
    let strength = (crate::layout::bevel_depth() / 0.15).clamp(0.0, 2.0);
    let a_dark = 0.32 * strength;
    let a_light = 0.16 * strength;
    let wx0 = track_x - WELL_GAP;
    let wx1 = track_x + track_w + WELL_GAP;
    // 1px columns, EXACT widths: translucent shading quads must not overlap (a
    // seam double-blends into a visible tick) — unlike the opaque band columns
    // below, which overlap on purpose against AA gaps.
    let cols = (wx1 - wx0).ceil().max(1.0) as i32;
    let colw = (wx1 - wx0) / cols as f32;
    let sub = WELL_WALL / WALL_STEPS as f32;
    for i in 0..cols {
        let x = wx0 + i as f32 * colw;
        let xm = x + colw * 0.5;
        // Inside the track the contour rides the profile; past the tips it wraps
        // around them on a WELL_GAP circle — rounded well ends, not square-cut.
        let c = if xm < track_x {
            let e = track_x - xm;
            (WELL_GAP * WELL_GAP - e * e).max(0.0).sqrt()
        } else if xm > track_x + track_w {
            let e = xm - (track_x + track_w);
            (WELL_GAP * WELL_GAP - e * e).max(0.0).sqrt()
        } else {
            profile(xm) * 0.5 + WELL_GAP
        };
        for k in 0..WALL_STEPS {
            let fade = 1.0 - k as f32 / WALL_STEPS as f32;
            // Shadow INSIDE the well below the top contour; the lit lip OUTSIDE
            // below the bottom contour — the textbox-recess read.
            ctx.quad(Rect { x, y: cy - c + k as f32 * sub, width: colw, height: sub }, [0.0, 0.0, 0.0, a_dark * fade]);
            ctx.quad(Rect { x, y: cy + c + k as f32 * sub, width: colw, height: sub }, [1.0, 1.0, 1.0, a_light * fade]);
        }
    }
    let steps = (track_w.ceil() as i32).max(1);
    let step_w = track_w / steps as f32;
    for i in 0..steps {
        let x = track_x + i as f32 * step_w;
        let h = profile(x + step_w * 0.5);
        ctx.quad(Rect { x, y: cy - h * 0.5, width: step_w + 0.3, height: h }, color_at(x + step_w * 0.5));
    }
}
