//! Material — what a surface is made of (`docs/rfc-material.md`).
//!
//! A [`Material`] is a surface's **tint**, its **frost** (whether and how it
//! shows what is behind it) and its **finish** (how it answers the DE's
//! light), carried BY VALUE on the plate made of it. Step 1 of the RFC: the
//! type exists, the sentinel encoding lives in exactly one function
//! ([`Material::fill_tint`]), and the rung defaults resolve from the same style
//! getters the rungs read today — so nothing on screen moves. Step 2 threads
//! it through `PlateSpec`, `ControlPlate` and the prims.
//!
//! What is deliberately NOT here: the light (the scene's, `relief_shade::
//! light_vector`), the roll width (geometry, per prim as `depth`) and the
//! carve/roll profiles (per-window uniforms). See the RFC's non-goals.

/// The plastic finish: how a surface answers light. `[shading strength,
/// specular strength, shininess, curvature/AO strength]` as carried in
/// `PlatePush.material`, plus the two depth ratios the shader reads from
/// `WindowInfo`. Formerly `relief_shade::Material`; that module re-exports it
/// under the old name until step 2 (RFC § 11 (4)).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Finish {
    pub strength: f32,
    pub spec: f32,
    pub shininess: f32,
    pub curvature: f32,
    /// A carve's drop over its run (`layout::carve_depth_ratio`): the
    /// geometry the slopes are scaled by. `relief_shade::RECESS_DEPTH` unless
    /// a height is pinned. Not in `to_array` — the shader reads it from
    /// `WindowInfo`.
    pub carve_depth: f32,
    /// The plate roll's rise over its run (`layout::roll_height_ratio`):
    /// 1 for the quarter-round.
    pub roll_height: f32,
}

impl Finish {
    /// The DE's finish, strength tracking `bevel_depth` against the default,
    /// the other three from `color::finish_spec` / `finish_shininess` /
    /// `finish_curvature` (defaults: the literals the shader shipped with).
    /// This is the ONE definition — the renderer's push constants come from
    /// here too.
    pub fn from_style() -> Self {
        Self {
            strength: crate::layout::bevel_depth() / 0.15,
            spec: crate::color::finish_spec(),
            shininess: crate::color::finish_shininess(),
            curvature: crate::color::finish_curvature(),
            carve_depth: crate::layout::carve_depth_ratio(),
            roll_height: crate::layout::roll_height_ratio(),
        }
    }

    pub fn to_array(self) -> [f32; 4] {
        [self.strength, self.spec, self.shininess, self.curvature]
    }
}

/// Whether and how a surface shows what is behind it.
///
/// An enum, not two floats and a bool: an opaque plate has no compression and
/// no refraction — not zero of each, none — and making the recipe unreachable
/// when the plate is not frosted is what keeps [`Material::fill`] to one
/// question.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Frost {
    /// The tint alone, composited at its alpha. Not a sample of the backdrop.
    Opaque,
    /// Frosted glass: the backdrop blurred, luminance-compressed toward the
    /// tint's key, tinted at the tint's alpha; the rim refracts.
    Frosted {
        /// How hard the blurred backdrop's luminance is pulled toward the
        /// tint's key — the legibility control. 0..1.
        /// `style.surface.plate.backdrop_compression` today.
        compression: f32,
        /// How far the plate's roll bends what it samples — the objecthood
        /// control. 0..1. `style.surface.plate.refraction` today.
        refraction: f32,
        /// Blur radius (the kernel's sigma) in logical px. Carried from step 1
        /// so `Frost` changes shape once (RFC § 11 (2)); the renderer reads
        /// it from step 3. [`Frost::DEFAULT_RADIUS`] is today's literal
        /// kernel; 0 will be a CLEAR plate — one clean sample, tinted.
        radius: f32,
    },
}

impl Frost {
    /// The sigma of `resolve_blur`'s kernel as shipped: 7×7 taps at a 5.5 px
    /// stride, sigma 2 taps — ≈ 11 logical px at scale 1.
    pub const DEFAULT_RADIUS: f32 = 11.0;

    /// The DE's frost, from the plate-rung keys every frosted surface reads
    /// today (the window-wide recipe, until step 3 makes it per plate).
    pub fn from_style() -> Self {
        Frost::Frosted {
            compression: crate::color::plate_backdrop_compression(),
            refraction: crate::color::plate_refraction(),
            radius: Self::DEFAULT_RADIUS,
        }
    }

    /// [`Frost::from_style`] when `on`, else [`Frost::Opaque`] — the shape of
    /// every `blur: bool` the toolkit carries today.
    pub fn from_flag(on: bool) -> Self {
        if on { Self::from_style() } else { Frost::Opaque }
    }

    pub fn is_frosted(&self) -> bool {
        matches!(self, Frost::Frosted { .. })
    }
}

/// Which frost regime a plate is under: a root plate's frost is the
/// compositor's blur-behind (its fill stays positive-alpha whatever its
/// material says), a nested plate's is the in-app pass (the negative-alpha
/// sentinel). `PlateSpec::role` derives it from the window-corner flags; a
/// control plate is always nested.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlateRole {
    Root,
    Nested,
}

/// What a surface is made of. ~14 floats, copied freely.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    /// Linear RGBA. Alpha is opacity and is non-negative here — the
    /// blur-behind sentinel is an encoding detail of [`Material::fill`],
    /// never state.
    pub tint: [f32; 4],
    pub frost: Frost,
    pub finish: Finish,
}

impl Material {
    /// An opaque material of `tint` under the DE's finish.
    pub fn opaque(tint: [f32; 4]) -> Self {
        Self { tint, frost: Frost::Opaque, finish: Finish::from_style() }
    }

    pub fn with_tint(mut self, tint: [f32; 4]) -> Self {
        self.tint = tint;
        self
    }

    pub fn with_frost(mut self, frost: Frost) -> Self {
        self.frost = frost;
        self
    }

    pub fn with_finish(mut self, finish: Finish) -> Self {
        self.finish = finish;
        self
    }

    // ---- the rung defaults -------------------------------------------------

    /// The root rung: the window's background, `style.surface.plate.root.
    /// color` (`color::page_low_color`, whose alpha IS `root_plate_opacity`).
    /// `Frost::Opaque` on the client side by construction: a root plate's
    /// frost is the COMPOSITOR's (`plate.root.blur`, which the client never
    /// reads), and [`Material::fill`] under [`PlateRole::Root`] would ignore
    /// a `Frosted` here anyway.
    pub fn root() -> Self {
        Self::opaque(crate::color::page_low_color())
    }

    /// The pane rung: the params plate's tint (`style.surface.param.color`)
    /// at the global plate opacity, frosted when `style.surface.plate.blur`
    /// says so — exactly what `color::param_plate_fill` resolved before this
    /// type existed (it now resolves through here).
    pub fn pane() -> Self {
        let mut tint = crate::color::param_bg_color();
        tint[3] *= crate::layout::plate_opacity();
        Self { tint, frost: Frost::from_flag(crate::color::plate_blur()), finish: Finish::from_style() }
    }

    /// The control rung: the button fill, never frosted. Control faces under
    /// the relief stances are laid through a stroke the sentinel cannot reach
    /// (see `PlateStance::Flat`); frost at this rung is `Flat` only and takes
    /// the pane's material verbatim ([`Material::flat_control`]).
    pub fn control() -> Self {
        Self::opaque(crate::color::button_background_color())
    }

    /// The legacy bridge: a fill as the renderer consumed it before this
    /// type existed. A negative alpha is the frost sentinel — `Frosted` at
    /// the DE recipe, tint alpha `|a|`; otherwise `Opaque` with the colour
    /// as is. `from_fill(c).fill(Nested) == c` for every `c` a caller could
    /// hand the old API. For a call site that holds an encoded colour; a
    /// site that knows what it means says `Material::opaque` / `with_frost`.
    pub fn from_fill(encoded: [f32; 4]) -> Self {
        let a = encoded[3];
        let m = Self::opaque([encoded[0], encoded[1], encoded[2], a.abs()]);
        if a < 0.0 { m.with_frost(Frost::from_style()) } else { m }
    }

    /// This material as a plate in `role` carries it: a root plate's frost
    /// is the COMPOSITOR's, so under [`PlateRole::Root`] the client-side
    /// material is opaque — the prim a `PlateSpec` emits carries this, and
    /// the tessellator encodes every prim as nested.
    pub fn for_role(&self, role: PlateRole) -> Self {
        match role {
            PlateRole::Root => Material { frost: Frost::Opaque, ..*self },
            PlateRole::Nested => *self,
        }
    }

    // ---- derived materials -------------------------------------------------

    /// The well floor cut into this plate: the same material with the tint
    /// darkened by `WELL_FLOOR`'s strength (`WELL_FLOOR_LIFTED`'s when
    /// `lifted`, the hover cue). Frost and finish carried through — a well in
    /// glass is deeper glass (RFC § 11 (3)). Over an opaque, fully covering
    /// plate this is the darkening overlay `PaintCtx::well_floor` draws today,
    /// exactly; the emission switches to this in step 2.
    pub fn floor(&self, lifted: bool) -> Material {
        let overlay = if lifted { crate::color::WELL_FLOOR_LIFTED } else { crate::color::WELL_FLOOR };
        let keep = 1.0 - overlay[3];
        let t = self.tint;
        Material { tint: [t[0] * keep, t[1] * keep, t[2] * keep, t[3]], ..*self }
    }

    /// A `Flat`-stance control on this pane: the material verbatim. Exists so
    /// the call site says what it means.
    pub fn flat_control(&self) -> Material {
        *self
    }

    /// A control face from a configured fill, under the rule
    /// `ControlPlate::face_from_fill` states: an opaque one is the face
    /// (alpha forced to 1 — a translucent face would blend into the relief's
    /// shading and read as a second material); a transparent one is `None`,
    /// the surface below showing as the face (edges only).
    pub fn control_face(raw: [f32; 4]) -> Option<Material> {
        (raw[3] > 0.001).then(|| Self::opaque([raw[0], raw[1], raw[2], 1.0]))
    }

    // ---- the one encoding function ----------------------------------------

    /// The vertex colour the renderer consumes for a plate of this material
    /// in `role` — see [`Material::fill_tint`].
    pub fn fill(&self, role: PlateRole) -> [f32; 4] {
        Self::fill_tint(self.tint, self.frost, role)
    }

    /// THE place a negative alpha is written. For `tint` under `frost` in
    /// `role`:
    /// - [`PlateRole::Root`] → alpha positive whatever `frost` says. A root
    ///   plate's frost is the compositor's blur-behind, never the in-app pass.
    /// - nested + [`Frost::Frosted`] → the in-app frost pass's negative-alpha
    ///   sentinel, `-|alpha|`.
    /// - nested + [`Frost::Opaque`] → the tint as is.
    ///
    /// Static so a caller holding a tint and a flag (today's `PlateSpec`)
    /// encodes through the same rule without resolving a finish it does not
    /// need.
    pub fn fill_tint(tint: [f32; 4], frost: Frost, role: PlateRole) -> [f32; 4] {
        let mut c = tint;
        match role {
            PlateRole::Root => c[3] = c[3].abs(),
            PlateRole::Nested if frost.is_frosted() => c[3] = -c[3].abs(),
            PlateRole::Nested => {}
        }
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frosted() -> Frost {
        Frost::Frosted { compression: 0.6, refraction: 0.3, radius: Frost::DEFAULT_RADIUS }
    }

    /// The encoding rule, stated once: root stays positive whatever the
    /// frost, nested frost is the sentinel, nested opaque passes through.
    #[test]
    fn fill_encodes_by_role() {
        let tint = [0.1, 0.2, 0.3, 0.8];
        assert_eq!(Material::fill_tint(tint, frosted(), PlateRole::Root)[3], 0.8, "root frost is the compositor's");
        assert_eq!(Material::fill_tint(tint, Frost::Opaque, PlateRole::Root)[3], 0.8);
        assert_eq!(Material::fill_tint(tint, frosted(), PlateRole::Nested)[3], -0.8, "nested frost = sentinel");
        assert_eq!(Material::fill_tint(tint, Frost::Opaque, PlateRole::Nested), tint, "no frost, no encoding");
        // A caller that hands a negative alpha in is normalised, not doubled.
        assert_eq!(Material::fill_tint([0.0, 0.0, 0.0, -0.5], frosted(), PlateRole::Nested)[3], -0.5);
        assert_eq!(Material::fill_tint([0.0, 0.0, 0.0, -0.5], Frost::Opaque, PlateRole::Root)[3], 0.5);
        let m = Material::opaque(tint).with_frost(frosted());
        assert_eq!(m.fill(PlateRole::Nested), Material::fill_tint(tint, frosted(), PlateRole::Nested));
    }

    /// The pane rung is `param_plate_fill`'s old arithmetic exactly: the
    /// tint at plate opacity, negated under plate blur.
    #[test]
    fn pane_resolves_like_param_plate_fill_did() {
        let old = |blur: bool| {
            let mut c = crate::color::param_bg_color();
            c[3] *= crate::layout::plate_opacity();
            if blur {
                c[3] = -c[3].abs();
            }
            c
        };
        for blur in [false, true] {
            crate::color::set_plate_blur(blur);
            assert_eq!(Material::pane().fill(PlateRole::Nested), old(blur), "blur={blur}");
            assert_eq!(crate::color::param_plate_fill(), old(blur), "blur={blur}");
            assert_eq!(Material::pane().frost.is_frosted(), blur);
        }
        crate::color::set_plate_blur(false);
    }

    /// The finish defaults are the literals the shader shipped with, and the
    /// push-constant layout is unchanged.
    #[test]
    fn finish_defaults_and_layout() {
        let f = Finish::from_style();
        assert_eq!(f.spec, 0.4);
        assert_eq!(f.shininess, 24.0);
        assert_eq!(f.curvature, 0.2);
        assert_eq!(f.to_array(), [f.strength, f.spec, f.shininess, f.curvature]);
        crate::color::set_finish_spec(0.9);
        assert_eq!(Finish::from_style().spec, 0.9);
        crate::color::set_finish_spec(0.4);
    }

    /// The frost recipe reads the two plate-rung keys and carries the default
    /// kernel; the flag form is today's `blur: bool`.
    #[test]
    fn frost_from_style_and_flag() {
        crate::color::set_plate_backdrop_compression(0.6);
        crate::color::set_plate_refraction(0.3);
        assert_eq!(Frost::from_style(), frosted());
        assert_eq!(Frost::from_flag(false), Frost::Opaque);
        assert!(Frost::from_flag(true).is_frosted());
        crate::color::set_plate_backdrop_compression(0.0);
        crate::color::set_plate_refraction(0.0);
    }

    /// A floor darkens the tint by the overlay's strength and keeps
    /// everything else: alpha, frost, finish.
    #[test]
    fn floor_darkens_and_carries_the_frost() {
        let m = Material::opaque([0.5, 0.5, 0.5, 0.7]).with_frost(frosted());
        let f = m.floor(false);
        let keep = 1.0 - crate::color::WELL_FLOOR[3];
        assert!((f.tint[0] - 0.5 * keep).abs() < 1e-6);
        assert_eq!(f.tint[3], 0.7);
        assert_eq!(f.frost, m.frost);
        assert_eq!(f.finish, m.finish);
        assert!(m.floor(true).tint[0] > f.tint[0], "lifted rises toward the plate");
        assert_eq!(m.flat_control(), m);
    }

    /// The legacy bridge round-trips every fill the old API accepted, and
    /// the role resolution agrees with the encoding function.
    #[test]
    fn from_fill_round_trips_and_for_role_matches_fill() {
        for c in [[0.1, 0.2, 0.3, 0.8], [0.1, 0.2, 0.3, -0.8], [0.0; 4], [0.5, 0.5, 0.5, 1.0]] {
            let m = Material::from_fill(c);
            assert_eq!(m.fill(PlateRole::Nested), c, "{c:?}");
            assert_eq!(m.frost.is_frosted(), c[3] < 0.0);
            assert!(m.tint[3] >= 0.0, "tint alpha is never negative");
            for role in [PlateRole::Root, PlateRole::Nested] {
                assert_eq!(m.for_role(role).fill(PlateRole::Nested), m.fill(role), "{c:?} {role:?}");
            }
        }
        assert_eq!(Material::from_fill([0.0, 0.0, 0.0, -0.5]).frost, Frost::from_style());
    }

    /// The control-face rule: opaque or nothing.
    #[test]
    fn control_face_is_opaque_or_none() {
        let face = Material::control_face([0.2, 0.3, 0.4, 0.5]).expect("a fill is a face");
        assert_eq!(face.tint, [0.2, 0.3, 0.4, 1.0]);
        assert_eq!(face.frost, Frost::Opaque);
        assert!(Material::control_face([0.2, 0.3, 0.4, 0.0]).is_none());
        assert_eq!(Material::control().frost, Frost::Opaque);
        assert_eq!(Material::root().frost, Frost::Opaque);
    }
}
