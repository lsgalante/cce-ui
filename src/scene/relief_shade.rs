//! The relief shading model in Rust — the arithmetic `shader2d.wgsl` performs
//! per pixel, so code outside the GPU can PREDICT the pixels instead of
//! sketching them. Both branches: the free carves ([`carve_shade`]) and the
//! plate's own perimeter roll ([`plate_surface`]), which do not composite the
//! same way — a carve is a translucent overlay, a plate is a multiply on its
//! own fill plus an additive specular.
//!
//! This exists because `cce-relief` drew its cross-sections with a stand-in
//! (`lit = dot(normal, light) * 0.35`) that shares nothing with the shader but
//! a light azimuth. A section drawn that way shows the geometry honestly and
//! the shading not at all — it cannot tell you that a wall reads hot, which is
//! the single most common thing you go to the editor to judge.
//!
//! **Drift is the hazard**, since WGSL and Rust cannot share a function body.
//! Two defences: the light vector lives HERE and the finish in
//! [`crate::scene::material`], and the renderer reads both from there
//! (`window_runner`'s `plate_light`/`plate_mat`); and the constants below are
//! checked against the shader's own source text by a unit test. Anything that
//! is only a comment away from disagreeing is not shared.

/// The finish — how a surface answers light — moved to `scene::material` as
/// [`Finish`] (RFC material, § 11 (4)). `Material` survives here as an alias
/// through step 2 so out-of-crate readers build untouched.
pub use crate::scene::material::Finish;
pub use crate::scene::material::Finish as Material;

/// Ambient floor of the plate lighting model. Mirrors `PLATE_AMBIENT`.
pub const PLATE_AMBIENT: f32 = 0.55;
/// A carve's drop as a fraction of its wall width when the material pins no
/// height (`layout::bevel_height`). Mirrors `RECESS_DEPTH`.
pub const RECESS_DEPTH: f32 = 0.6;

/// Amplitude of the bright crest hugging a raised plate's silhouette.
/// Mirrors `PLATE_CREST`.
pub const PLATE_CREST: f32 = 0.25;
/// The roll's descent is truncated at this fraction of the quadrant, so the
/// profile ends on a bounded slope instead of plunging vertical at the
/// silhouette. Mirrors `ROLL_CUT`.
pub const ROLL_CUT: f32 = 0.8;

/// The free-carve modes, matching the shader's `MODE_*`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CarveMode {
    Recess,
    Boss,
    Ridge,
    Trough,
}

/// The DE's light as a unit vector in screen space (+z out of the screen), at
/// the fixed 45° elevation the renderer uses. The ONE definition, as with
/// [`Finish::from_style`].
pub fn light_vector() -> [f32; 3] {
    let az = crate::layout::light_source_position();
    let el = std::f32::consts::FRAC_PI_4;
    [az.cos() * el.cos(), -az.sin() * el.cos(), el.sin()]
}

/// Shading of the flat face — the denominator every carve is expressed
/// relative to, so an untouched surface composites to exactly nothing.
pub fn flat_shade(light: [f32; 3]) -> f32 {
    PLATE_AMBIENT + (1.0 - PLATE_AMBIENT) * light[2]
}

/// The analytic carve slope: smoothstep normally, smootherstep under a
/// continuous-curvature `corner_shape`. Mirrors `carve_slope`'s analytic
/// branch; a custom profile replaces it with the LUT, which callers model by
/// passing their own slope function to [`carve_shade`].
pub fn analytic_carve_slope(v: f32) -> f32 {
    if crate::layout::corner_shape() > 2.001 {
        let w = v * (1.0 - v);
        30.0 * w * w
    } else {
        6.0 * v * (1.0 - v)
    }
}

/// Specular term of a tilted surface under the DE light. Mirrors `roll_spec`.
pub fn roll_spec(sv: [f32; 2], light: [f32; 3], mat: &Finish) -> f32 {
    let m = (sv[0] * sv[0] + sv[1] * sv[1]).sqrt();
    if m < 1e-5 {
        return 0.0;
    }
    let hv = {
        let h = [light[0], light[1], light[2] + 1.0];
        let n = (h[0] * h[0] + h[1] * h[1] + h[2] * h[2]).sqrt().max(1e-6);
        [h[0] / n, h[1] / n, h[2] / n]
    };
    let facing = [sv[0] / m, sv[1] / m];
    let cos_t = 1.0 / (1.0 + m * m).sqrt();
    let sin_t = m * cos_t;
    let hxy = (hv[0] * hv[0] + hv[1] * hv[1]).sqrt();
    let prof = cos_t * hv[2] + sin_t * hxy;
    let az = ((facing[0] * hv[0] + facing[1] * hv[1]) / hxy.max(1e-4)).clamp(0.0, 1.0);
    mat.spec * (prof.powf(mat.shininess) - hv[2].powf(mat.shininess)).max(0.0) * az * az
}

/// The signed shading value one carve contributes at `u` across its wall —
/// the shader's `v`, before the tint branch. Positive is a white screen over
/// what is beneath, negative a black multiply; magnitude is the alpha.
///
/// `facing` is the SDF gradient direction (unit, pointing OUT of the carve's
/// box). `slope_at` is the profile's slope function — pass
/// [`analytic_carve_slope`] for the default material, or the derivative of a
/// custom height curve to model an installed LUT.
///
/// `att` (the host-box roll fade) is left to the caller: it depends on where
/// the carve sits inside its host, not on the profile.
pub fn carve_shade(
    mode: CarveMode,
    u: f32,
    facing: [f32; 2],
    slope_at: &dyn Fn(f32) -> f32,
    light: [f32; 3],
    mat: &Finish,
) -> f32 {
    let u = u.clamp(0.0, 1.0);
    let (slope, curv) = match mode {
        // The straddling pair: ONE profile evaluation on the folded coordinate,
        // amplitude halved so the wall tilt matches a step's.
        CarveMode::Ridge | CarveMode::Trough => {
            let w = (2.0 * u).min(2.0 - 2.0 * u).clamp(0.0, 1.0);
            let up = if mode == CarveMode::Ridge { 1.0 } else { -1.0 };
            let rising = if u <= 0.5 { 1.0 } else { -1.0 } * up;
            (
                rising * 0.5 * mat.carve_depth * 2.0 * slope_at(w),
                -up * mat.curvature * (w * std::f32::consts::TAU).sin(),
            )
        }
        _ => {
            let dir = if mode == CarveMode::Boss { 1.0 } else { -1.0 };
            (
                dir * mat.carve_depth * slope_at(u),
                -dir * mat.curvature * (u * std::f32::consts::TAU).sin(),
            )
        }
    };
    let sv = [facing[0] * slope, facing[1] * slope];
    let n = {
        let len = (sv[0] * sv[0] + sv[1] * sv[1] + 1.0).sqrt();
        [sv[0] / len, sv[1] / len, 1.0 / len]
    };
    let ndl = (n[0] * light[0] + n[1] * light[1] + n[2] * light[2]).max(0.0);
    let diff = PLATE_AMBIENT + (1.0 - PLATE_AMBIENT) * ndl;
    let spec = roll_spec(sv, light, mat);
    (diff / flat_shade(light) - 1.0 + curv + spec) * mat.strength
}

/// The analytic roll slope at `f` (0 where the roll meets the face, 1 at the
/// silhouette). Mirrors `roll_slope`'s analytic branch: the one-exponent
/// generalisation of the circular quadrant, truncated at [`ROLL_CUT`].
pub fn analytic_roll_slope(f: f32) -> f32 {
    let shape = crate::layout::corner_shape();
    let fc = f * ROLL_CUT;
    if shape > 2.001 {
        let h = (1.0 - fc.powf(shape)).max(1e-4).powf(1.0 / shape);
        (fc / h).powf(shape - 1.0)
    } else {
        fc / (1.0 - fc * fc).max(1e-4).sqrt()
    }
}

/// How squarely a rim faces the light's azimuth, 0..1 — the weight on the
/// plate crest. Mirrors `crest_weight`. The crest used to be a flat
/// `PLATE_CREST` on every side, which out-measured the far edges' diffuse
/// fall-off everywhere along the roll, so a raised plate had a lit rim and no
/// shadowed one; weighted, the down-light edges keep only their diffuse
/// shading and read as the glint's dark counterpart. A light from straight
/// overhead has no near or far side and keeps the crest everywhere.
pub fn crest_weight(facing: [f32; 2], light: [f32; 3]) -> f32 {
    let m = (light[0] * light[0] + light[1] * light[1]).sqrt();
    if m < 1e-4 {
        return 1.0;
    }
    ((facing[0] * light[0] + facing[1] * light[1]) / m).max(0.0)
}

/// The plate's own surface colour across its perimeter roll — mode 1, which is
/// NOT the free-carve branch and does not composite like one.
///
/// A carve emits a translucent overlay; a plate emits its material directly, as
/// a MULTIPLY on the face colour plus an additive specular. Expressed relative
/// to the flat face (shade 1.0, specular 0.0) so the face keeps exactly the
/// app's chosen colour — which is why a plate can be tinted freely and a carve
/// cannot.
///
/// `f` is 0 where the roll meets the face and 1 at the silhouette. Returns
/// `None` past the silhouette, where the shader discards.
pub fn plate_surface(
    base: [f32; 3],
    f: f32,
    facing: [f32; 2],
    roll_slope_at: &dyn Fn(f32) -> f32,
    light: [f32; 3],
    mat: &Finish,
) -> Option<[f32; 3]> {
    if !(0.0..=1.0).contains(&f) {
        return None;
    }
    let slope = roll_slope_at(f) * mat.roll_height;
    let sv = [facing[0] * slope, facing[1] * slope];
    let n = {
        let len = (sv[0] * sv[0] + sv[1] * sv[1] + 1.0).sqrt();
        [sv[0] / len, sv[1] / len, 1.0 / len]
    };
    let ndl = (n[0] * light[0] + n[1] * light[1] + n[2] * light[2]).max(0.0);
    let diff = PLATE_AMBIENT + (1.0 - PLATE_AMBIENT) * ndl;
    // The crest: the ambient-catching convex rim that makes glass read as glass.
    let extra = PLATE_CREST * f * f * f * crest_weight(facing, light);
    let shade = 1.0 + (diff / flat_shade(light) - 1.0 + extra) * mat.strength;
    let spec = roll_spec(sv, light, mat) * mat.strength;
    Some([
        (base[0] * shade + spec).clamp(0.0, 1.0),
        (base[1] * shade + spec).clamp(0.0, 1.0),
        (base[2] * shade + spec).clamp(0.0, 1.0),
    ])
}

/// Composite one carve's shading over what is already there, the way the
/// renderer's alpha blend does.
///
/// This asymmetry is load-bearing and is why a wall's bright side always
/// out-measures its dark side: brightening screens toward WHITE, darkening
/// multiplies toward BLACK, so on a mid-grey surface the same |v| moves the
/// pixel about twice as far up as down.
pub fn composite(base: [f32; 3], v: f32) -> [f32; 3] {
    let a = v.abs().min(1.0);
    let target = if v >= 0.0 { 1.0f32 } else { 0.0f32 };
    [
        base[0] * (1.0 - a) + target * a,
        base[1] * (1.0 - a) + target * a,
        base[2] * (1.0 - a) + target * a,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shader's own source, so the constants below are checked against the
    /// thing they mirror rather than against a comment.
    const WGSL: &str = include_str!("../vk/shader2d.wgsl");

    fn wgsl_const(name: &str) -> f32 {
        let needle = format!("const {name}: f32 = ");
        let rest = WGSL
            .split(&needle)
            .nth(1)
            .unwrap_or_else(|| panic!("{name} not found in shader2d.wgsl"));
        let lit: String = rest.chars().take_while(|c| *c != ';').collect();
        lit.trim().parse().expect("numeric literal")
    }

    #[test]
    fn constants_match_the_shader() {
        assert_eq!(wgsl_const("PLATE_AMBIENT"), PLATE_AMBIENT);
        assert_eq!(wgsl_const("RECESS_DEPTH"), RECESS_DEPTH);
        assert_eq!(wgsl_const("PLATE_CREST"), PLATE_CREST);
        assert_eq!(wgsl_const("ROLL_CUT"), ROLL_CUT);
    }

    /// The crest is light-facing: at the silhouette the edge toward the light
    /// shades brighter than the face and the edge away from it darker. Before
    /// the weight, the flat crest left the far edge at or above the face.
    #[test]
    fn far_edge_shades_darker_than_the_face() {
        let light = light_vector();
        let mat = Finish::from_style();
        let base = [0.5f32; 3];
        let lxy = [light[0], light[1]];
        let m = (lxy[0] * lxy[0] + lxy[1] * lxy[1]).sqrt();
        let near = [lxy[0] / m, lxy[1] / m];
        let far = [-near[0], -near[1]];
        let lit = plate_surface(base, 1.0, near, &analytic_roll_slope, light, &mat).unwrap();
        let dark = plate_surface(base, 1.0, far, &analytic_roll_slope, light, &mat).unwrap();
        assert!(lit[0] > base[0] + 0.02, "near edge {} vs face {}", lit[0], base[0]);
        assert!(dark[0] < base[0] - 0.02, "far edge {} vs face {}", dark[0], base[0]);
    }

    /// The plate's face must come through as exactly the app's colour, or a
    /// plate silently recolours whatever it is filled with.
    #[test]
    fn plate_face_is_untouched() {
        let light = light_vector();
        let mat = Finish::from_style();
        let base = [0.3f32, 0.4, 0.5];
        let out = plate_surface(base, 0.0, [-1.0, 0.0], &analytic_roll_slope, light, &mat).unwrap();
        for i in 0..3 {
            assert!((out[i] - base[i]).abs() < 1e-4, "face channel {i}: {} vs {}", out[i], base[i]);
        }
    }

    /// A flat surface must composite to nothing, or the cover quad tints
    /// everything it covers — the property the whole "relative to the flat
    /// face" formulation exists to guarantee.
    #[test]
    fn flat_ground_shades_to_zero() {
        let light = light_vector();
        let mat = Finish::from_style();
        for mode in [CarveMode::Recess, CarveMode::Boss, CarveMode::Ridge, CarveMode::Trough] {
            for u in [0.0f32, 1.0] {
                let v = carve_shade(mode, u, [-1.0, 0.0], &analytic_carve_slope, light, &mat);
                assert!(v.abs() < 1e-4, "{mode:?} at u={u} shaded {v}, expected 0");
            }
        }
    }

    /// A pinned height is geometry: a deeper carve tilts its wall more and
    /// shades harder, with nothing else changed.
    #[test]
    fn deeper_carve_shades_harder() {
        let light = light_vector();
        let shallow = Finish { carve_depth: 0.3, ..Finish::from_style() };
        let deep = Finish { carve_depth: 1.2, ..Finish::from_style() };
        let at = |m: &Finish| carve_shade(CarveMode::Recess, 0.5, [-1.0, 0.0], &analytic_carve_slope, light, m).abs();
        assert!(at(&deep) > at(&shallow) * 1.5, "deep {} vs shallow {}", at(&deep), at(&shallow));
        // And the flat plateaus still composite to nothing.
        assert!(carve_shade(CarveMode::Recess, 0.0, [-1.0, 0.0], &analytic_carve_slope, light, &deep).abs() < 1e-4);
    }

    /// Ridge and trough are the same wall with the height sign flipped, so
    /// their shading is opposite WHERE THE WALL IS STEEP.
    ///
    /// Not everywhere, which is worth stating because it is the first thing you
    /// would assume: the response has an EVEN component. Tilting a surface
    /// either way shortens the face-on `n.z` term and adds a non-negative
    /// specular, so near the plateau lips — where the directional part is
    /// nearly nothing — a ridge and a trough both darken slightly. That is the
    /// faint lip lobe visible on both, not an asymmetry bug.
    #[test]
    fn ridge_and_trough_oppose_where_the_wall_is_steep() {
        let light = light_vector();
        let mat = Finish::from_style();
        let sample = |m: CarveMode, u: f32| {
            carve_shade(m, u, [-1.0, 0.0], &analytic_carve_slope, light, &mat)
        };
        // Steepest point of the folded profile: w = 1 at u = 0.5 is the crest
        // (zero slope), so the extremes sit either side of it.
        let steep = (0..=100)
            .map(|i| i as f32 / 100.0)
            .max_by(|a, b| sample(CarveMode::Ridge, *a).abs().total_cmp(&sample(CarveMode::Ridge, *b).abs()))
            .unwrap();
        let (r, t) = (sample(CarveMode::Ridge, steep), sample(CarveMode::Trough, steep));
        assert!(r.abs() > 0.05, "ridge shading {r} at u={steep} is too faint to test");
        assert!(r * t < 0.0, "ridge {r} and trough {t} agree in sign at u={steep}");
    }
}
