//! The relief's geometry and light: light direction and strength, the bevel and roll widths and
//! heights, corner shape and the window's radii, carve depth, and the wall and edge profile tables.

use super::*;

/// The untouched editor curve — the "analytic" sentinel in the config'd
/// profile specs (cce-designer's Edge Profile convention). For the wall curve
/// identity-smooth IS the analytic smoothstep, so skipping it changes
/// nothing; for the roll it would be a straight chamfer, not the analytic
/// superellipse quadrant, so it must read as "no custom profile".
pub const RELIEF_PROFILE_IDENTITY_SPEC: &str = "smooth;0.000:0.000,1.000:1.000";

/// Parse-and-install the relief profiles config carries as ramp specs
/// (`style.surface.relief.wall.profile` / `edge.profile` → the style
/// registry's `bevel_profile_spec` / `roll_profile_spec`). Absent, identity,
/// or unparseable specs clear back to the analytic profiles.
pub(super) fn apply_relief_profile_config() {
    let (wall, edge) = {
        let reg = get_style_registry().read().unwrap();
        (reg.get_string("bevel_profile_spec"), reg.get_string("roll_profile_spec"))
    };
    match parse_relief_profile_spec(wall.as_deref()) {
        Some((keys, smooth)) => set_bevel_profile_keys(&keys, smooth),
        None => clear_bevel_profile(),
    }
    match parse_relief_profile_spec(edge.as_deref()) {
        Some((keys, smooth)) => set_roll_profile_keys(&keys, smooth),
        None => clear_roll_profile(),
    }
}

/// A config'd profile spec → installable keys. `None` (falling back to the
/// analytic profile) for absent, identity-sentinel, or unparseable specs.
pub(super) fn parse_relief_profile_spec(spec: Option<&str>) -> Option<(Vec<(f32, f32)>, bool)> {
    spec.filter(|s| *s != RELIEF_PROFILE_IDENTITY_SPEC).and_then(crate::widget::parse_ramp_spec)
}

/// Install (or clear back to analytic) the WALL profile from a ramp spec —
/// the entry point for a `(relief)` config value's profile
/// ([`crate::relief_spec::ReliefSpec`]): an app whose feature carries its
/// own material installs it process-wide here. Same identity/unparseable
/// filtering as the config path above.
pub fn install_wall_profile_spec(spec: Option<&str>) {
    match parse_relief_profile_spec(spec) {
        Some((keys, smooth)) => set_bevel_profile_keys(&keys, smooth),
        None => clear_bevel_profile(),
    }
}

pub fn light_source_position() -> f32 {
    let val = get_style_registry().read().unwrap().get_float("light_source_position").unwrap_or(2.3561945);
    if val > 2.0 * std::f32::consts::PI {
        val.to_radians()
    } else {
        val
    }
}

pub fn bevel_depth() -> f32 {
    get_style_registry().read().unwrap().get_float("bevel_depth").unwrap_or(0.15)
}

/// Corner shape exponent for SDF-lit plates: 2.0 (the default) is a circular
/// arc; higher values are superellipse "squircle" corners with continuous
/// curvature — ~4.5 is the Apple-like look. Clamped to [2, 16]: below 2 the
/// Lp construction degenerates toward a chamfer, above 16 it is visually a
/// square corner and the pow() terms start flirting with f32 range.
pub fn corner_shape() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("corner_shape").unwrap_or(2.0).clamp(2.0, 16.0)
}

/// The window silhouette's nominal corner radius: the SHARED config's
/// root plate corner_radius, never the per-app override. The compositor clips
/// every decorated window with this value (widened by
/// [`corner_span_factor`]), so any window-corner arc an app draws itself must
/// use it too — even when the app restyles its own plates through its
/// override file — or its corners detach from the silhouette (and from the
/// desktop grid's cells, which share the same knob).
pub fn window_corner_radius() -> f32 {
    crate::config::get_i64_shared("/style/surface/plate/root/corner_radius", 12) as f32
}

/// The curvature-matched corner-span factor for window-scale squircle corners.
/// A raw superellipse of exponent n at a circle's nominal radius turns tighter
/// at the diagonal than that circle — its radius of curvature there is
/// √2·r / (2^(1/n)·(n − 1)). Scaling the corner span by this factor makes the
/// diagonal curvature equal the configured radius, so the corner reads as the
/// same size as a circular one (the same reason Apple's continuous corners run
/// ~1.5·r along the edge). Exactly 1 at n = 2. Applied to window-scale corners
/// only — `Prim::Plate` and the renderer's window-corner clip — never to
/// widget-scale radii, which must match the nominal-radius squircles around them.
/// The window silhouette's EFFECTIVE corner radius: the shared nominal value
/// widened by the corner-span factor — exactly the arc the compositor clips
/// every decorated window with, and the radius a root plate's corners must
/// wear (RFC Phase 7b; `PlateSpec::radii_for` uses it for window-flagged
/// corners). Apps drawing root-surface geometry through non-Plate prims read
/// this scalar directly.
pub fn window_silhouette_radius() -> f32 {
    window_corner_radius() * corner_span_factor()
}

pub fn corner_span_factor() -> f32 {
    corner_span_factor_for(corner_shape())
}

/// The span factor for an explicit corner exponent — what a plate carrying
/// its own `shape` (see `scene::paint::Prim::Plate`) scales its radii by.
/// Same clamp as [`corner_shape`], so an override cannot reach an exponent
/// the shader would not accept.
pub fn corner_span_factor_for(n: f32) -> f32 {
    let n = n.clamp(2.0, 16.0);
    if n > 2.001 {
        (n - 1.0) * 2f32.powf(1.0 / n) / std::f32::consts::SQRT_2
    } else {
        1.0
    }
}

/// Whether the relief primitives (see `scene::paint::Prim`) render through
/// shader2d's per-pixel SDF-lit branch (the default) or the legacy banded vertex
/// shading. `style.surface.relief.shader=(bool)false` (or `0`) flips back to
/// the old look for A/B comparison — the registry key keeps the bevel name
/// because it selects how the shared lit EDGE is computed, not which shapes
/// exist. Until 2026-09-28 the config spelling was `window_manager.bevel_shader`
/// (retired, reported), and only a NUMBER worked: a `(bool)` flattens to the
/// string "false", which the float read never saw.
pub fn bevel_shader() -> bool {
    lazy_init_style_registry();
    let reg = get_style_registry().read().unwrap();
    shader_on(reg.get_float("bevel_shader"), reg.get_string("bevel_shader").as_deref())
}

/// The shader toggle's reading of its registry slot: a number is on unless
/// zero, a string is on unless it says `false` / `off` / `no`, and an unset
/// slot is on.
pub(super) fn shader_on(float: Option<f32>, string: Option<&str>) -> bool {
    match (float, string) {
        (Some(v), _) => v != 0.0,
        (None, Some(s)) => !matches!(s.trim().to_ascii_lowercase().as_str(), "false" | "off" | "no" | "0"),
        (None, None) => true,
    }
}

/// How wide a rolled edge is, in logical px — the distance over which a plate's perimeter
/// or a recess wall curves away from the flat surface. `bevel_depth` is the companion
/// knob: it sets how hard the light falls across that distance. Wide and shallow reads as
/// thick glass; narrow and deep reads as a stamped metal lip.
pub fn bevel_width() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("bevel_width").unwrap_or(9.3)
}

/// A carve's geometric drop when the material pins one
/// (`style.surface.relief.wall.height`, a length — `(mm)0.3` resolves through
/// the display metric), in logical px. `None` = follow the wall width at the
/// analytic ratio ([`crate::scene::relief_shade::RECESS_DEPTH`]), the look
/// every config had before heights existed. A configured 0 reads as unset,
/// which is how an editor puts a material back on "follow".
pub fn bevel_height() -> Option<f32> {
    lazy_init_style_registry();
    get_style_registry()
        .read()
        .unwrap()
        .get_float("bevel_height")
        .filter(|h| h.is_finite() && *h > 0.0)
}

/// The plate roll's rise when pinned (`style.surface.relief.edge.height`, a
/// length), logical px. `None` = a quarter-round of radius `bevel_width`.
pub fn roll_height() -> Option<f32> {
    lazy_init_style_registry();
    get_style_registry()
        .read()
        .unwrap()
        .get_float("roll_height")
        .filter(|h| h.is_finite() && *h > 0.0)
}

/// The drop of a carve whose wall runs `wall` logical px: the pinned height
/// when there is one, else the analytic ratio of the wall — saturating at the
/// DE's roll width, so a wall wider than the plate's own perimeter roll
/// spreads the same step over a longer run (a softer transition) instead of
/// cutting proportionally deeper. The tessellator's CSG features and the
/// shader's free carves both derive from this rule.
pub fn carve_depth_px(wall: f32) -> f32 {
    match bevel_height() {
        Some(h) => h,
        None => crate::scene::relief_shade::RECESS_DEPTH * wall.min(bevel_width()),
    }
}

/// Drop over run for a wall of the DE roll width — what the shading twin
/// scales its slopes by.
pub fn carve_depth_ratio() -> f32 {
    let w = bevel_width().max(0.001);
    carve_depth_px(w) / w
}

/// Rise over run of the plate roll: 1 (the quarter-round) unless pinned.
pub fn roll_height_ratio() -> f32 {
    roll_height().map_or(1.0, |h| h / bevel_width().max(0.001))
}

/// Sample count of the custom bevel profile LUT ([`set_bevel_profile_keys`]).
pub const BEVEL_PROFILE_SAMPLES: usize = 32;

/// The custom bevel/carve height profile, as the slope LUT the renderer uploads
/// to the 2D shader: slot `i` holds `h'` at `v = (i + 0.5) / N` of the wall's
/// height curve `h(v)` (`v` runs 0 at the surrounding plateau → 1 at the carve
/// floor / boss crest; `h` in units of the feature's depth, so a 0→1 curve is
/// the classic full-depth bevel and a curve ending back at its start height is
/// a pure decorative rim). `None` = the analytic smoothstep profile.
pub(super) static BEVEL_PROFILE: crate::style::StyleCell<Option<[f32; BEVEL_PROFILE_SAMPLES]>> = crate::style::StyleCell::new(|s| &s.layout.BEVEL_PROFILE, |s| &mut s.layout.BEVEL_PROFILE);

/// Bumped on every profile change so renderers know to re-upload their LUT.
pub(super) static BEVEL_PROFILE_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The custom EDGE (plate roll) profile — same slope-LUT encoding as
/// [`BEVEL_PROFILE`], but read by the shader's `roll_slope` for the perimeter
/// roll of widget-scale plates: `v` runs 0 at the face join → 1 at the
/// silhouette, and the curve is the roll's descent progress (0 = face height,
/// 1 = fully dropped), so the identity curve is a straight chamfer and `None`
/// is the analytic superellipse quadrant.
pub(super) static ROLL_PROFILE: crate::style::StyleCell<Option<[f32; BEVEL_PROFILE_SAMPLES]>> = crate::style::StyleCell::new(|s| &s.layout.ROLL_PROFILE, |s| &mut s.layout.ROLL_PROFILE);

pub(super) static ROLL_PROFILE_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub use cce_core::ramp::sample_ramp_keys;

/// Install a custom bevel/carve profile from ramp keys (`(pos, value)`, both
/// 0..1, sorted by pos). Sampled into the slope LUT the shader's `carve_slope`
/// reads in place of its analytic smoothstep — every recess/boss/ridge wall in
/// this process restyles on the next frame. Empty or single-key lists clear
/// back to the analytic profile ([`clear_bevel_profile`]).
pub fn set_bevel_profile_keys(keys: &[(f32, f32)], smooth: bool) {
    let Some(lut) = ramp_profile_lut(keys, smooth) else {
        clear_bevel_profile();
        return;
    };
    *BEVEL_PROFILE.write().unwrap() = Some(lut);
    BEVEL_PROFILE_GEN.fetch_add(1, std::sync::atomic::Ordering::Release);
}

/// What a key list installs: its slope LUT, or `None` for a degenerate list
/// (fewer than two keys is no curve at all) — the caller clears back to the
/// analytic profile. The pure half of `set_*_profile_keys`.
pub(super) fn ramp_profile_lut(keys: &[(f32, f32)], smooth: bool) -> Option<[f32; BEVEL_PROFILE_SAMPLES]> {
    if keys.len() < 2 {
        return None;
    }
    Some(ramp_slope_lut(keys, smooth))
}

/// A ramp key list sampled into the shader's slope LUT — slot `i` holds the
/// curve's slope at `v = (i + 0.5) / N`.
pub(super) fn ramp_slope_lut(keys: &[(f32, f32)], smooth: bool) -> [f32; BEVEL_PROFILE_SAMPLES] {
    let n = BEVEL_PROFILE_SAMPLES;
    let mut slopes = [0.0f32; BEVEL_PROFILE_SAMPLES];
    for (i, slot) in slopes.iter_mut().enumerate() {
        let h0 = sample_ramp_keys(keys, smooth, i as f32 / n as f32);
        let h1 = sample_ramp_keys(keys, smooth, (i + 1) as f32 / n as f32);
        *slot = (h1 - h0) * n as f32;
    }
    slopes
}

/// Install a custom EDGE profile for the plate perimeter roll from ramp keys —
/// the [`set_bevel_profile_keys`] twin for `ROLL_PROFILE`. The curve is the
/// roll's descent progress from the face join (0) to the silhouette (1); the
/// shader's `roll_slope` samples it in place of the analytic superellipse
/// quadrant. Empty or single-key lists clear back to the analytic roll.
pub fn set_roll_profile_keys(keys: &[(f32, f32)], smooth: bool) {
    let Some(lut) = ramp_profile_lut(keys, smooth) else {
        clear_roll_profile();
        return;
    };
    *ROLL_PROFILE.write().unwrap() = Some(lut);
    ROLL_PROFILE_GEN.fetch_add(1, std::sync::atomic::Ordering::Release);
}

/// Drop the custom edge profile — plate rolls return to the analytic quadrant.
pub fn clear_roll_profile() {
    let mut guard = ROLL_PROFILE.write().unwrap();
    if guard.is_some() {
        *guard = None;
        ROLL_PROFILE_GEN.fetch_add(1, std::sync::atomic::Ordering::Release);
    }
}

/// The installed edge profile's slope LUT, if any — what the renderer uploads.
pub fn roll_profile_slopes() -> Option<[f32; BEVEL_PROFILE_SAMPLES]> {
    *ROLL_PROFILE.read().unwrap()
}

/// Change counter for [`roll_profile_slopes`].
pub fn roll_profile_generation() -> u64 {
    ROLL_PROFILE_GEN.load(std::sync::atomic::Ordering::Acquire)
}

/// Drop the custom bevel profile — walls return to the analytic smoothstep.
pub fn clear_bevel_profile() {
    let mut guard = BEVEL_PROFILE.write().unwrap();
    if guard.is_some() {
        *guard = None;
        BEVEL_PROFILE_GEN.fetch_add(1, std::sync::atomic::Ordering::Release);
    }
}

/// The installed profile's slope LUT, if any — what the renderer uploads.
pub fn bevel_profile_slopes() -> Option<[f32; BEVEL_PROFILE_SAMPLES]> {
    *BEVEL_PROFILE.read().unwrap()
}

/// Change counter for [`bevel_profile_slopes`] — a renderer re-uploads when it
/// differs from the generation it last wrote.
pub fn bevel_profile_generation() -> u64 {
    BEVEL_PROFILE_GEN.load(std::sync::atomic::Ordering::Acquire)
}

#[cfg(test)]
mod ramp_sampling_tests {
    use super::sample_ramp_keys;

    #[test]
    fn two_key_smooth_is_exactly_smoothstep() {
        let keys = [(0.0, 0.0), (1.0, 1.0)];
        for i in 0..=20 {
            let t = i as f32 / 20.0;
            let ss = t * t * (3.0 - 2.0 * t);
            assert!((sample_ramp_keys(&keys, true, t) - ss).abs() < 1e-6, "t={t}");
        }
    }

    #[test]
    fn passes_through_every_key_and_holds_the_ends() {
        let keys = [(0.0, 0.0), (0.15, 0.45), (0.35, 0.7), (0.55, 0.78), (0.75, 0.85), (1.0, 1.0)];
        for &(p, v) in &keys {
            assert!((sample_ramp_keys(&keys, true, p) - v).abs() < 1e-6, "key {p}");
        }
        assert_eq!(sample_ramp_keys(&keys, true, -1.0), 0.0);
        assert_eq!(sample_ramp_keys(&keys, true, 2.0), 1.0);
    }

    #[test]
    fn monotone_keys_give_a_monotone_curve_without_wobble() {
        // The wall profile that came out as a chain of bumps under the old
        // per-segment smoothstep.
        let keys = [(0.0, 0.0), (0.15, 0.45), (0.35, 0.7), (0.55, 0.78), (0.75, 0.85), (1.0, 1.0)];
        let mut last = -1.0f32;
        let mut slopes = Vec::new();
        for i in 0..=400 {
            let t = i as f32 / 400.0;
            let v = sample_ramp_keys(&keys, true, t);
            assert!(v >= last - 1e-6, "not monotone at t={t}: {v} < {last}");
            slopes.push(v - last);
            last = v;
        }
        // No wobble: the slope at an INTERIOR key is a real slope, not the
        // zero the old blend pinned there (0.35: secants 1.25 and 0.4 either
        // side — the harmonic mean is well above half the smaller one).
        let dv = (sample_ramp_keys(&keys, true, 0.355) - sample_ramp_keys(&keys, true, 0.345)) / 0.01;
        assert!(dv > 0.3, "slope at key 0.35 is {dv}");
        // …and the slope never flips sign back and forth between keys: at
        // most one local slope maximum per segment would be a stretch to
        // assert, so pin the direct symptom — the curve stays inside the
        // key hull (no overshoot beyond the neighbouring key values).
        for i in 0..keys.len() - 1 {
            let (a, b) = (keys[i], keys[i + 1]);
            for j in 1..10 {
                let t = a.0 + (b.0 - a.0) * j as f32 / 10.0;
                let v = sample_ramp_keys(&keys, true, t);
                assert!(v >= a.1.min(b.1) - 1e-6 && v <= a.1.max(b.1) + 1e-6, "overshoot at t={t}: {v}");
            }
        }
    }

    #[test]
    fn a_peak_is_flat_at_the_peak_and_never_overshoots() {
        // The desktop overview speed ramp: rises then falls.
        let keys = [(0.0, 0.15), (0.4, 1.0), (1.0, 0.1)];
        assert!((sample_ramp_keys(&keys, true, 0.4) - 1.0).abs() < 1e-6);
        for i in 0..=100 {
            let v = sample_ramp_keys(&keys, true, i as f32 / 100.0);
            assert!((0.1 - 1e-6..=1.0 + 1e-6).contains(&v), "overshoot {v}");
        }
        let near = sample_ramp_keys(&keys, true, 0.39);
        assert!(near > 0.99, "flat at the extremum: {near}");
    }

    #[test]
    fn linear_is_untouched() {
        let keys = [(0.0, 0.0), (0.5, 1.0), (1.0, 0.0)];
        assert!((sample_ramp_keys(&keys, false, 0.25) - 0.5).abs() < 1e-6);
        assert!((sample_ramp_keys(&keys, false, 0.75) - 0.5).abs() < 1e-6);
    }
}
