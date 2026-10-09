//! The relief edge as vertices — the legacy banded path (`style.surface.relief`
//! `shader=false`) and the solid borders: edge profiles and curvature, banded light and shadow,
//! plate faces.

use super::*;

pub fn push_plate_bevel_vertices(
    x: f32, y: f32, ww: f32, h: f32,
    r: f32,
    t: f32,
    sw: f32, sh: f32,
    base_color: [f32; 4],
    clip_circle: [f32; 3],
    out: &mut Vec<Vertex>,
) {
    push_bevel_edge_vertices(x, y, ww, h, r, t, sw, sh, base_color, clip_circle, 1.0, out);
}

/// The bevel edge shading, with the light direction selectable: `light_sign` is `1.0`
/// for a raised plate (edges facing `light_source_position` are lit) and `-1.0` for a
/// recess (those same edges fall into shadow instead, and the far edges catch the
/// light). Negating the whole light vector flips every edge and every corner segment
/// consistently, because both the flat-edge factors and the arc-normal dot product
/// below are linear in it.
pub fn push_bevel_edge_vertices(
    x: f32, y: f32, ww: f32, h: f32,
    r: f32,
    t: f32,
    sw: f32, sh: f32,
    base_color: [f32; 4],
    clip_circle: [f32; 3],
    light_sign: f32,
    out: &mut Vec<Vertex>,
) {
    push_bevel_edge_vertices_radii(
        x, y, ww, h, (r, r, r, r), t, sw, sh, base_color, clip_circle, light_sign, out,
    );
}

/// As [`push_bevel_edge_vertices`], but with a per-corner radius (TL, TR, BR, BL) so the
/// lip can follow a shape whose corners differ — a recess carved along the top of a
/// rounded plate needs the plate's radius on its top corners and square ones where it
/// meets the content below. A uniform radius there would either square off the plate's
/// arc (painting a notch outside it) or wrongly round the inner corners.
pub fn push_bevel_edge_vertices_radii(
    x: f32, y: f32, ww: f32, h: f32,
    radii: (f32, f32, f32, f32),
    t: f32,
    sw: f32, sh: f32,
    base_color: [f32; 4],
    clip_circle: [f32; 3],
    light_sign: f32,
    out: &mut Vec<Vertex>,
) {
    push_bevel_edge_vertices_banded(
        x, y, ww, h, radii, t, sw, sh, base_color, clip_circle, light_sign,
        default_bevel_bands(t), (true, true, true, true), EdgeKind::Rim, out,
    );
}

/// What kind of height change an edge represents. The two shade differently because they
/// are different shapes, and using one where the other belongs is what makes a bevel read
/// as a drawn line instead of a surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeKind {
    /// The surface *ends* here: a quarter-round rolling from face-on at the inner edge of
    /// the lip to fully in-plane at the outer boundary, where it drops away. The shading
    /// therefore peaks exactly at the boundary and dies inward. This is a plate's outer
    /// perimeter.
    Rim,
    /// The surface *continues* at a different height: one plateau steps down to another.
    /// A height field that falls monotonically across the transition has its normal tilted
    /// toward the low side the whole way, steepest in the middle and flat at both ends —
    /// so the shading is a bump straddling the boundary, not a band butted against it.
    /// Hanging the band on one side instead leaves the seam the eye reads as a drawn line.
    Step,
}

/// Shading across an edge at signed distance `d` from the boundary (positive = toward the
/// shape's interior), for a transition of width `t`. Returns the light term as a fraction
/// of full tilt.
#[inline]
pub(super) fn bevel_profile(kind: EdgeKind, d: f32, t: f32) -> f32 {
    if t <= 0.0 {
        return 0.0;
    }
    match kind {
        // Normal rotates from in-plane (d = 0) to face-on (d = t): sine of what tilt is
        // left. A linear ramp here reads as a flat 45° chamfer instead of a roll.
        EdgeKind::Rim => ((1.0 - (d / t).clamp(0.0, 1.0)) * std::f32::consts::FRAC_PI_2).sin(),
        // Symmetric bump over [-t/2, +t/2], zero at both ends so the transition blends into
        // both plateaus with no seam.
        EdgeKind::Step => {
            let s = (d / t + 0.5).clamp(0.0, 1.0);
            (s * std::f32::consts::PI).sin()
        }
    }
}

/// The light-independent curvature term at signed distance `d` — the second depth cue,
/// on top of the directional one. Curvature shading is what ambient light does: convex
/// surface catches it from everywhere (bright), concave is self-occluded (dark). Because
/// it does not rotate with the light, it survives exactly where the directional term
/// dies — walls parallel to the light vector — so no edge ever vanishes entirely.
///
/// `high_sign` is +1 when the rect interior is the HIGH side of the transition and -1
/// when it is the low side (a recess). Geometry, not lighting: it does not flip with
/// `light_sign`... except that for these 2.5D shapes the two are the same number, since
/// a raised shape is lit like a plateau and shaded like one.
#[inline]
pub(super) fn bevel_curvature(kind: EdgeKind, d: f32, t: f32, high_sign: f32) -> f32 {
    if t <= 0.0 {
        return 0.0;
    }
    match kind {
        // A rim is convex everywhere, tightest right at the silhouette: a bright crest
        // line hugging the boundary and dying fast inward. This is the line that makes
        // glass read as glass — the edge catches ambient light all the way around, even
        // (dimmer, via the gain asymmetry below) on the side facing away from the light.
        EdgeKind::Rim => {
            let u = (d / t).clamp(0.0, 1.0);
            let f = 1.0 - u;
            CREST_RATIO * f * f * f
        }
        // An S-curve step is convex on its high half (the shoulder) and concave on its
        // low half (the fillet, where the wall meets the floor): antisymmetric, zero at
        // the ends (no seam against either plateau) and at the midpoint.
        EdgeKind::Step => {
            let s = (d / t + 0.5).clamp(0.0, 1.0);
            let outer_is_high = -high_sign; // d < 0 is outside the rect
            // sin(2πs) is positive on the outer half — the shoulder when the outside is
            // the high side — and negative on the inner (fillet) half.
            AO_RATIO * outer_is_high * (s * std::f32::consts::TAU).sin()
        }
    }
}

/// Crest amplitude as a fraction of `bevel_depth` — how much brighter a rim's silhouette
/// line is than flat surface under even light. Must stay clearly below ~0.7 (the
/// projection of a 135° light onto an axis edge), or it cancels the directional shadow
/// on the dark side and the rim goes flat there instead of showing a faint bright line
/// over a shadowed roll.
pub(super) const CREST_RATIO: f32 = 0.4;

/// Shoulder/fillet amplitude as a fraction of `bevel_depth`.
pub(super) const AO_RATIO: f32 = 0.6;

/// Per-sign overlay gains. These are asymmetric the opposite way from intuition: on the
/// dark bases this DE runs, white-over blending (`b + a(1-b)`) moves the pixel far more
/// per unit alpha than black-over (`b(1-a)`) — a dark surface has little brightness for
/// black to take away. The old subtractive shading effectively crushed shadow sides to
/// black in linear space; the black overlay needs a high gain to keep shadows reading
/// at all, while white needs damping to keep highlights from blowing out.
pub(super) const LIGHT_GAIN: f32 = 0.7;

pub(super) const DARK_GAIN: f32 = 3.0;

/// A shading value (already scaled by `bevel_depth`) as the two overlay passes: the lit
/// pass is translucent white, the shadow pass translucent black. Painting the
/// *modulation* instead of a resolved surface color is what lets relief primitives compose — a step
/// crossing a rim shades the rim's gradient instead of stamping a flat band over it, a
/// lip on a translucent plate no longer doubles its opacity, and a recess needs no
/// knowledge of the surface color it carves.
///
/// Why two passes with fixed RGB rather than one signed color: a primitive whose value
/// crosses zero inside a band would interpolate white→black through mid-gray at
/// non-negligible alpha — on a dark base a *brightening* artifact right where the
/// shading should vanish. With per-pass alphas clamped at the crossing, each pass fades
/// to zero there and the hue can never be wrong. Alphas also stay non-negative on every
/// vertex, which the renderer requires (negative alpha is the blur sentinel).
#[inline]
pub(super) fn overlay_light(v: f32) -> [f32; 4] {
    [1.0, 1.0, 1.0, (v.max(0.0) * LIGHT_GAIN).min(1.0)]
}

#[inline]
pub(super) fn overlay_dark(v: f32) -> [f32; 4] {
    [0.0, 0.0, 0.0, ((-v).max(0.0) * DARK_GAIN).min(1.0)]
}

/// The signed distance range an edge's shading occupies, relative to the boundary.
#[inline]
pub(super) fn bevel_span(kind: EdgeKind, t: f32) -> (f32, f32) {
    match kind {
        EdgeKind::Rim => (0.0, t),
        EdgeKind::Step => (-0.5 * t, 0.5 * t),
    }
}

/// How many gradient bands to slice a lip of thickness `t` into. Vertex colors interpolate
/// linearly, so each band is a chord of the shading curve; one band per ~1.25px keeps the
/// error under a shade step without emitting geometry finer than the display resolves.
/// The cap rose with the curvature term: a step now has two features across its width
/// (shoulder and fillet), so it needs double the samples a single bump did.
pub(super) fn default_bevel_bands(t: f32) -> usize {
    ((t / 1.25).ceil() as usize).clamp(1, 12)
}

/// As [`push_bevel_edge_vertices_radii`], with the band count forced and the walls
/// selectable — for callers that want a coarser or finer roll-off than thickness alone
/// implies, or that are shading a step rather than a closed shape.
///
/// `edges` is (top, right, bottom, left). Suppressing a wall matters for a region that
/// runs flush to the surface's own edge: a full-width menubar sunk into the top of a plate
/// is a *plateau one step down*, not a trough, so its only real wall is the one facing the
/// content. Drawing the other three would carve a lip along the plate's outer edge, where
/// the plate's own roll already lives, and the two would fight.
pub fn push_bevel_edge_vertices_banded(
    x: f32, y: f32, ww: f32, h: f32,
    radii: (f32, f32, f32, f32),
    t: f32,
    sw: f32, sh: f32,
    base_color: [f32; 4],
    clip_circle: [f32; 3],
    light_sign: f32,
    bands: usize,
    edges: (bool, bool, bool, bool),
    kind: EdgeKind,
    out: &mut Vec<Vertex>,
) {
    // Floored for the same reason as `plate_push_raised`'s cap: a negative
    // extent must degrade to no ring, not panic in `clamp`.
    let cap = (ww.min(h) * 0.5).max(0.0);
    let (tl, tr, br, bl) = (
        radii.0.clamp(0.0, cap),
        radii.1.clamp(0.0, cap),
        radii.2.clamp(0.0, cap),
        radii.3.clamp(0.0, cap),
    );
    let t = t.clamp(0.0, cap);
    if t <= 0.0 {
        return;
    }
    let bands = bands.max(1);

    let rad = crate::layout::light_source_position();
    let lx = rad.cos() * light_sign;
    let ly = -rad.sin() * light_sign;
    let depth = crate::layout::bevel_depth();

    // `base_color` is no longer painted: shading is an overlay (see `overlay_color`), so
    // the surface below shows through with its own gradients and translucency intact.
    let _ = base_color;
    // Shading (directional + curvature, scaled by bevel_depth) at signed distance `d`,
    // for an edge whose outward flat normal is `dir`. A `Step` band runs negative — it
    // straddles the boundary into the plateau outside the rect, which is exactly what
    // removes the seam.
    let value = |dot: f32, d: f32| {
        depth * (bevel_profile(kind, d, t) * dot + bevel_curvature(kind, d, t, light_sign))
    };
    // The (up to two) overlay color pairs for a band running from value `v0` to `v1`:
    // one white pair and/or one black pair, each pass fading to zero alpha wherever the
    // value has the other sign. Both fire only when the band straddles the terminator.
    let passes = |v0: f32, v1: f32| -> [Option<([f32; 4], [f32; 4])>; 2] {
        [
            (v0 > 0.0 || v1 > 0.0).then(|| (overlay_light(v0), overlay_light(v1))),
            (v0 < 0.0 || v1 < 0.0).then(|| (overlay_dark(v0), overlay_dark(v1))),
        ]
    };
    let (span_lo, span_hi) = bevel_span(kind, t);

    // Each flat edge spans between its two adjoining corner radii, not a single uniform
    // inset — that is what lets the corners differ. At a square corner there is no arc to
    // cover the t×t patch where two edges meet, so the horizontal edges claim it (they run
    // the full span) and the vertical ones inset by `t`; overlapping them instead would
    // double-blend that patch, which shows as a dark notch on a translucent surface.
    let (left_top, left_bot) = (if tl > 0.0 { tl } else { t }, if bl > 0.0 { bl } else { t });
    let (right_top, right_bot) = (if tr > 0.0 { tr } else { t }, if br > 0.0 { br } else { t });
    let top_w = ww - tl - tr;
    let bottom_w = ww - bl - br;
    let left_h = h - left_top - left_bot;
    let right_h = h - right_top - right_bot;

    for k in 0..bands {
        let d0 = span_lo + (span_hi - span_lo) * (k as f32 / bands as f32);
        let d1 = span_lo + (span_hi - span_lo) * ((k + 1) as f32 / bands as f32);
        let bw = d1 - d0;

        // Top: outward normal (0,-1); the gradient runs downward, into the surface.
        if top_w > 0.0 && edges.0 {
            let (v0, v1) = (value(-ly, d0), value(-ly, d1));
            for (c0, c1) in passes(v0, v1).into_iter().flatten() {
                out.extend_from_slice(&quad_vertices_shaded(
                    x + tl, y + d0, top_w, bw, sw, sh, c0, c0, c1, c1, clip_circle,
                ));
            }
        }
        // Bottom: outward normal (0,1); gradient runs upward.
        if bottom_w > 0.0 && edges.2 {
            let (v0, v1) = (value(ly, d0), value(ly, d1));
            for (c0, c1) in passes(v0, v1).into_iter().flatten() {
                out.extend_from_slice(&quad_vertices_shaded(
                    x + bl, y + h - d1, bottom_w, bw, sw, sh, c1, c1, c0, c0, clip_circle,
                ));
            }
        }
        // Left: outward normal (-1,0); gradient runs rightward.
        if left_h > 0.0 && edges.3 {
            let (v0, v1) = (value(-lx, d0), value(-lx, d1));
            for (c0, c1) in passes(v0, v1).into_iter().flatten() {
                out.extend_from_slice(&quad_vertices_shaded(
                    x + d0, y + left_top, bw, left_h, sw, sh, c0, c1, c1, c0, clip_circle,
                ));
            }
        }
        // Right: outward normal (1,0); gradient runs leftward.
        if right_h > 0.0 && edges.1 {
            let (v0, v1) = (value(lx, d0), value(lx, d1));
            for (c0, c1) in passes(v0, v1).into_iter().flatten() {
                out.extend_from_slice(&quad_vertices_shaded(
                    x + ww - d1, y + right_top, bw, right_h, sw, sh, c1, c0, c0, c1, clip_circle,
                ));
            }
        }
    }

    // A corner arc belongs to both of its adjoining walls, so it is drawn only when both
    // are — otherwise a suppressed wall would still get a quarter of a lip.
    let corners = [
        (x + tl, y + tl, tl, std::f32::consts::PI, 1.5 * std::f32::consts::PI, edges.0 && edges.3), // Top-Left
        (x + ww - tr, y + tr, tr, 1.5 * std::f32::consts::PI, 2.0 * std::f32::consts::PI, edges.0 && edges.1), // Top-Right
        (x + ww - br, y + h - br, br, 0.0, 0.5 * std::f32::consts::PI, edges.2 && edges.1), // Bottom-Right
        (x + bl, y + h - bl, bl, 0.5 * std::f32::consts::PI, std::f32::consts::PI, edges.2 && edges.3), // Bottom-Left
    ];

    for &(cx, cy, r, start_angle, end_angle, enabled) in &corners {
        // A square corner has no arc to sweep — the flat edges already met there.
        if r <= 0.0 || !enabled {
            continue;
        }
        // The corner is a quarter of a torus: shading varies along the sweep (the normal
        // swings through 90° of the light) *and* across the lip (the roll-off). Both come
        // out of the vertex colors, so one quad per (segment × band) cell is enough — no
        // faceting, unlike the 16 flat wedges this replaced.
        let segments = ((r * 0.75) as usize).clamp(8, 48);
        let ct = t.min(r);
        for j in 0..segments {
            let theta0 = start_angle + (j as f32) * (end_angle - start_angle) / (segments as f32);
            let theta1 = start_angle + ((j + 1) as f32) * (end_angle - start_angle) / (segments as f32);
            let (cos0, sin0) = (theta0.cos(), theta0.sin());
            let (cos1, sin1) = (theta1.cos(), theta1.sin());
            for k in 0..bands {
                let d0 = span_lo + (span_hi - span_lo) * (k as f32 / bands as f32);
                let d1 = span_lo + (span_hi - span_lo) * ((k + 1) as f32 / bands as f32);
                // Inward along the corner's radius is the same signed distance as inward
                // from a flat edge, so the arc scales the span the same way.
                let (r0, r1) = (r - ct * (d0 / t), r - ct * (d1 / t));
                let p = |rho: f32, c: f32, s: f32| -> [f32; 2] {
                    [
                        ((cx + rho * c) / sw) * 2.0 - 1.0,
                        1.0 - ((cy + rho * s) / sh) * 2.0,
                    ]
                };
                // Outer/inner × the two sweep ends; each vertex gets its own value, and
                // the cell is drawn once per overlay pass that has any coverage.
                let vals = [
                    value(cos0 * lx + sin0 * ly, d0),
                    value(cos1 * lx + sin1 * ly, d0),
                    value(cos1 * lx + sin1 * ly, d1),
                    value(cos0 * lx + sin0 * ly, d1),
                ];
                let geo = [
                    p(r0, cos0, sin0),
                    p(r0, cos1, sin1),
                    p(r1, cos1, sin1),
                    p(r1, cos0, sin0),
                ];
                let mut cells: [Option<fn(f32) -> [f32; 4]>; 2] = [None, None];
                if vals.iter().any(|&v| v > 0.0) {
                    cells[0] = Some(overlay_light);
                }
                if vals.iter().any(|&v| v < 0.0) {
                    cells[1] = Some(overlay_dark);
                }
                for f in cells.into_iter().flatten() {
                    let c: Vec<Vertex> = (0..4)
                        .map(|i| Vertex { position: geo[i], color: f(vals[i]), clip_circle })
                        .collect();
                    out.extend_from_slice(&[c[0], c[1], c[2], c[0], c[2], c[3]]);
                }
            }
        }
    }
}

/// How strong the face gradient is, as a fraction of `bevel_depth` at the corner nearest
/// the light. Deliberately well below the edge amplitude: the face is a plane, not a
/// roll — it only *leans* toward the light.
pub(super) const FACE_RATIO: f32 = 0.35;

/// The face lighting of a plate: a single diagonal luminance gradient across the whole
/// surface, brightest at the corner facing `light_source_position` and darkest at the
/// opposite one. This is the difference between an object and a sticker: a real surface
/// under directional light is never uniform, and a perfectly flat fill makes the eye
/// read the (much smaller) edge shading as frame decoration rather than shape.
///
/// Emitted as the same two-pass white/black overlays as the relief primitives (see
/// [`overlay_light`]/[`overlay_dark`]): fixed RGB per pass, per-corner alphas clamped at
/// the terminator, bilinear across the quad. The quad is square — its corners poke past
/// a rounded plate's arcs — but the compositor clips the window surface to the same
/// radius, so the overhang never reaches the screen.
pub fn push_plate_face_vertices(
    x: f32, y: f32, ww: f32, h: f32,
    sw: f32, sh: f32,
    clip_circle: [f32; 3],
    out: &mut Vec<Vertex>,
) {
    let rad = crate::layout::light_source_position();
    let (lx, ly) = (rad.cos(), -rad.sin());
    let amp = crate::layout::bevel_depth() * FACE_RATIO;
    // Corner value = how much its outward diagonal faces the light.
    let inv = std::f32::consts::FRAC_1_SQRT_2;
    let v_tl = amp * inv * (-lx - ly);
    let v_tr = amp * inv * (lx - ly);
    let v_br = amp * inv * (lx + ly);
    let v_bl = amp * inv * (-lx + ly);
    let vs = [v_tl, v_tr, v_br, v_bl];
    if vs.iter().any(|&v| v > 0.0) {
        out.extend_from_slice(&quad_vertices_shaded(
            x, y, ww, h, sw, sh,
            overlay_light(v_tl), overlay_light(v_tr), overlay_light(v_br), overlay_light(v_bl),
            clip_circle,
        ));
    }
    if vs.iter().any(|&v| v < 0.0) {
        out.extend_from_slice(&quad_vertices_shaded(
            x, y, ww, h, sw, sh,
            overlay_dark(v_tl), overlay_dark(v_tr), overlay_dark(v_br), overlay_dark(v_bl),
            clip_circle,
        ));
    }
}

pub fn push_plate_solid_border_vertices(
    x: f32, y: f32, ww: f32, h: f32,
    radii: crate::widget::CornerRadii,
    t: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
    clip_circle: [f32; 3],
    out: &mut Vec<Vertex>,
) {
    let mut r_tl = radii.top_left.max(0.0);
    let mut r_tr = radii.top_right.max(0.0);
    let mut r_br = radii.bottom_right.max(0.0);
    let mut r_bl = radii.bottom_left.max(0.0);

    // Simple scale clamping
    let sum_top = r_tl + r_tr;
    if sum_top > ww {
        let f = ww / sum_top;
        r_tl *= f;
        r_tr *= f;
    }
    let sum_bottom = r_bl + r_br;
    if sum_bottom > ww {
        let f = ww / sum_bottom;
        r_bl *= f;
        r_br *= f;
    }
    let sum_left = r_tl + r_bl;
    if sum_left > h {
        let f = h / sum_left;
        r_tl *= f;
        r_bl *= f;
    }
    let sum_right = r_tr + r_br;
    if sum_right > h {
        let f = h / sum_right;
        r_tr *= f;
        r_br *= f;
    }

    out.extend_from_slice(&quad_vertices_with_clip(x + r_tl, y, ww - r_tl - r_tr, t, sw, sh, color, clip_circle));
    out.extend_from_slice(&quad_vertices_with_clip(x, y + r_tl, t, h - r_tl - r_bl, sw, sh, color, clip_circle));
    out.extend_from_slice(&quad_vertices_with_clip(x + r_bl, y + h - t, ww - r_bl - r_br, t, sw, sh, color, clip_circle));
    out.extend_from_slice(&quad_vertices_with_clip(x + ww - t, y + r_tr, t, h - r_tr - r_br, sw, sh, color, clip_circle));

    let segments = 16;
    let corner_e = 2.0 / crate::layout::corner_shape();

    // Corner strokes as annulus strips between the outer superellipse (radius
    // r) and its inner scaled copy (r - t): at 1px thickness the scaled inner
    // curve is indistinguishable from the true parallel curve, and at
    // corner_shape 2 this is exactly the circular arc annulus. NOT
    // push_arc_background_vertices — that stays circular for genuine arcs.
    //
    // Both edges of the annulus are FEATHERED, like the fill fan's corners
    // (push_rounded_rect_vertices_corners) and push_feathered_line_vertices:
    // each fades over `f` either side of its true curve, so the 50%-coverage
    // lines stay on the exact silhouette and the stroke reads at the same
    // weight, but the arc anti-aliases instead of rasterizing a staircase
    // beside the SDF-smooth face it outlines. The straight edges stay crisp
    // quads (a pixel-snapped hairline would only blur). A corner too tight to
    // fit the inner fade keeps the hard annulus.
    let f = 0.5f32.min(t * 0.25);
    let fade = [color[0], color[1], color[2], 0.0];
    let corner = |cx: f32, cy: f32, r: f32, start: f32, end: f32, out: &mut Vec<Vertex>| {
        let r_in = (r - t).max(0.0);
        // (inner radius, outer radius, inner alpha colour, outer alpha colour)
        let bands: &[(f32, f32, [f32; 4], [f32; 4])] = if r_in - f > 0.0 {
            &[
                (r_in - f, r_in + f, fade, color),
                (r_in + f, r - f, color, color),
                (r - f, r + f, color, fade),
            ]
        } else {
            &[(r_in, r, color, color)]
        };
        let ndc = |px: f32, py: f32| [(px / sw) * 2.0 - 1.0, 1.0 - (py / sh) * 2.0];
        for i in 0..segments {
            let t1 = start + (i as f32) * (end - start) / segments as f32;
            let t2 = start + ((i + 1) as f32) * (end - start) / segments as f32;
            let (c1, s1) = superellipse_pt(t1, corner_e);
            let (c2, s2) = superellipse_pt(t2, corner_e);
            for &(ra, rb, ca, cb) in bands {
                if rb - ra <= 0.0 {
                    continue;
                }
                let o1 = ndc(cx + rb * c1, cy + rb * s1);
                let o2 = ndc(cx + rb * c2, cy + rb * s2);
                let i1 = ndc(cx + ra * c1, cy + ra * s1);
                let i2 = ndc(cx + ra * c2, cy + ra * s2);
                out.push(Vertex { position: o1, color: cb, clip_circle });
                out.push(Vertex { position: o2, color: cb, clip_circle });
                out.push(Vertex { position: i1, color: ca, clip_circle });
                out.push(Vertex { position: o2, color: cb, clip_circle });
                out.push(Vertex { position: i2, color: ca, clip_circle });
                out.push(Vertex { position: i1, color: ca, clip_circle });
            }
        }
    };

    if r_tl > 0.1 {
        corner(x + r_tl, y + r_tl, r_tl, std::f32::consts::PI, 1.5 * std::f32::consts::PI, out);
    }
    if r_tr > 0.1 {
        corner(x + ww - r_tr, y + r_tr, r_tr, 1.5 * std::f32::consts::PI, 2.0 * std::f32::consts::PI, out);
    }
    if r_br > 0.1 {
        corner(x + ww - r_br, y + h - r_br, r_br, 0.0, 0.5 * std::f32::consts::PI, out);
    }
    if r_bl > 0.1 {
        corner(x + r_bl, y + h - r_bl, r_bl, 0.5 * std::f32::consts::PI, std::f32::consts::PI, out);
    }
}
