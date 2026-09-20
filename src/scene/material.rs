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
        /// Blur radius — the kernel's sigma — in logical px.
        /// [`Frost::DEFAULT_RADIUS`] is the kernel every frosted plate had;
        /// 0 is a CLEAR plate: one clean sample, tinted.
        radius: f32,
    },
}

impl Frost {
    /// The kernel every frosted plate had, as a sigma in logical px.
    ///
    /// Before the recipe was per plate, `resolve_blur` sampled a 7×7 kernel
    /// at a fixed 5.5 PHYSICAL px stride (sigma two taps = 11 physical px)
    /// — half the blur on a scale-2 panel that it was on a scale-1 one, and
    /// the panel every frosted surface was tuned on is scale 2. A material
    /// cannot know the scale, so the default is stated in logical px at the
    /// value that reproduces the panel exactly: 5.5 logical = 11 physical at
    /// scale 2. A scale-1 display now gets the same logical blur instead of
    /// twice it.
    pub const DEFAULT_RADIUS: f32 = 5.5;

    /// Fixed-point width of `compression` and `refraction` inside one push
    /// float: `c·4095·4096 + r·4095` is an integer below 2²⁴, exact in f32.
    /// Mirrors the shader's `FROST_PACK_MAX` / `FROST_PACK_BASE`.
    pub const PACK_MAX: f32 = 4095.0;
    pub const PACK_BASE: f32 = 4096.0;

    /// The recipe as the plate branch reads it: `[p_host.z, p_host.w]` —
    /// compression and refraction packed in `z`, the blur radius in `w` as
    /// the kernel sigma in PHYSICAL px (the shader samples the backdrop in
    /// physical px). `Opaque` packs to zeros: nothing reads them, and a
    /// plate that was never frosted pushes the bytes it always did.
    pub fn pack(&self, scale: f32) -> [f32; 2] {
        match *self {
            Frost::Opaque => [0.0, 0.0],
            Frost::Frosted { compression, refraction, radius } => {
                let q = |v: f32| (v.clamp(0.0, 1.0) * Self::PACK_MAX).round();
                [q(compression) * Self::PACK_BASE + q(refraction), radius.max(0.0) * scale]
            }
        }
    }

    /// The Rust twin of the shader's unpack: `(compression, refraction)`
    /// from a packed `z`.
    pub fn unpack(z: f32) -> (f32, f32) {
        let hi = (z / Self::PACK_BASE).floor();
        (hi / Self::PACK_MAX, (z - hi * Self::PACK_BASE) / Self::PACK_MAX)
    }

    /// The DE's frost recipe: the pane rung's, when its bound material is
    /// frosted (`plate material="glass"`), else the default material's
    /// plate-rung keys (`style.surface.plate.backdrop_compression` /
    /// `refraction` / `radius`). What `from_fill` and `popover` frost with.
    pub fn from_style() -> Self {
        if let Some(f @ Frost::Frosted { .. }) = Material::bound(PlateRung::Pane).map(|m| m.frost) {
            return f;
        }
        Frost::Frosted {
            compression: crate::color::plate_backdrop_compression(),
            refraction: crate::color::plate_refraction(),
            radius: crate::color::plate_frost_radius(),
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

/// The three rungs of the plate ladder a material can be bound to in
/// config (`docs/rfc-material.md` § 5): `plate { root material="…" }`,
/// `plate material="…"` and `control material="…"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlateRung {
    Root,
    Pane,
    Control,
}

/// A named material as config spells it — every field optional, resolved
/// against the rung's legacy material by [`MaterialDef::resolve`]:
///
/// ```kdl
/// style { surface { material {
///     glass {
///         color (rgba)"#05050840"
///         frost backdrop_compression=(f64)0.6 refraction=(f64)0.3 radius=(f64)5.5
///         finish light=(f64)0.15 spec=(f64)0.4 shininess=(f64)24.0 curvature=(f64)0.2
///     }
/// } } }
/// ```
///
/// A node with no `frost` child is opaque — not "frosted at zero", none. A
/// missing `color` keeps the rung's tint; a missing `finish` key keeps the
/// DE's. `light` is the finish strength in the units `style.surface.relief.
/// depth` uses (0.15 = the default strength of 1).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaterialDef {
    pub tint: Option<[f32; 4]>,
    pub frost: Option<FrostDef>,
    pub light: Option<f32>,
    pub spec: Option<f32>,
    pub shininess: Option<f32>,
    pub curvature: Option<f32>,
}

/// The `frost` child of a material node: present means frosted, each knob
/// defaulting (0, 0, [`Frost::DEFAULT_RADIUS`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrostDef {
    pub compression: Option<f32>,
    pub refraction: Option<f32>,
    pub radius: Option<f32>,
}

impl MaterialDef {
    /// The material this definition names, over `base` — the rung's legacy
    /// material, which supplies everything the node leaves unsaid.
    pub fn resolve(&self, base: Material) -> Material {
        let frost = match self.frost {
            Some(f) => Frost::Frosted {
                compression: f.compression.unwrap_or(0.0).clamp(0.0, 1.0),
                refraction: f.refraction.unwrap_or(0.0).clamp(0.0, 1.0),
                radius: f.radius.unwrap_or(Frost::DEFAULT_RADIUS).max(0.0),
            },
            None => Frost::Opaque,
        };
        let mut finish = base.finish;
        if let Some(l) = self.light {
            finish.strength = l / 0.15;
        }
        if let Some(v) = self.spec {
            finish.spec = v.max(0.0);
        }
        if let Some(v) = self.shininess {
            finish.shininess = v.max(1.0);
        }
        if let Some(v) = self.curvature {
            finish.curvature = v.max(0.0);
        }
        Material { tint: self.tint.unwrap_or(base.tint), frost, finish }
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

    /// The root rung: `plate { root material="…" }` when bound, else the
    /// window's background, `style.surface.plate.root.color`
    /// (`color::page_low_color`, whose alpha IS `root_plate_opacity`).
    /// `Frost::Opaque` on the client side by construction: a root plate's
    /// frost is the COMPOSITOR's (`plate.root.blur`, which the client never
    /// reads), and [`Material::fill`] under [`PlateRole::Root`] would ignore
    /// a `Frosted` here anyway.
    pub fn root() -> Self {
        Self::bound(PlateRung::Root).unwrap_or_else(Self::root_legacy)
    }

    /// The pane rung: `plate material="…"` when bound, else the params
    /// plate's tint (`style.surface.param.color`) at the global plate
    /// opacity, frosted when `style.surface.plate.blur` says so — exactly
    /// what `color::param_plate_fill` resolved before this type existed (it
    /// now resolves through here).
    pub fn pane() -> Self {
        Self::bound(PlateRung::Pane).unwrap_or_else(Self::pane_legacy)
    }

    /// The control rung: `control material="…"` when bound, else the button
    /// fill, never frosted. Control faces under the relief stances are laid
    /// through a stroke the sentinel cannot reach (see `PlateStance::Flat`);
    /// frost at this rung is `Flat` only and takes the pane's material
    /// verbatim ([`Material::flat_control`]).
    pub fn control() -> Self {
        Self::bound(PlateRung::Control).unwrap_or_else(Self::control_legacy)
    }

    /// The rung's material from the legacy keys alone — what every config
    /// without a `material=` binding resolves to, and the base a bound
    /// material's unset fields fall back to.
    pub fn legacy(rung: PlateRung) -> Self {
        match rung {
            PlateRung::Root => Self::root_legacy(),
            PlateRung::Pane => Self::pane_legacy(),
            PlateRung::Control => Self::control_legacy(),
        }
    }

    fn root_legacy() -> Self {
        Self::opaque(crate::color::page_low_color())
    }

    fn pane_legacy() -> Self {
        let mut tint = crate::color::param_bg_color();
        tint[3] *= crate::layout::plate_opacity();
        let frost = if crate::color::plate_blur() {
            Frost::Frosted {
                compression: crate::color::plate_backdrop_compression(),
                refraction: crate::color::plate_refraction(),
                radius: crate::color::plate_frost_radius(),
            }
        } else {
            Frost::Opaque
        };
        Self { tint, frost, finish: Finish::from_style() }
    }

    fn control_legacy() -> Self {
        Self::opaque(crate::color::button_background_color())
    }

    /// The material `rung` is bound to in config, resolved over the rung's
    /// legacy material — `None` when the rung is unbound. A binding to a
    /// name no `material` node defines is reported once and treated as
    /// unbound, so a typo degrades to today's look rather than to nothing.
    pub fn bound(rung: PlateRung) -> Option<Self> {
        let name = crate::color::material_binding(rung)?;
        match crate::color::named_material(&name) {
            Some(def) => Some(def.resolve(Self::legacy(rung))),
            None => {
                log::warn!("{rung:?} plate rung is bound to material \"{name}\", which no material node defines");
                None
            }
        }
    }

    /// A named material from config, resolved over the pane rung's legacy
    /// material — for an app that wants a material by name for its own
    /// surfaces. `None` when no node defines it.
    pub fn named(name: &str) -> Option<Self> {
        crate::color::named_material(name).map(|def| def.resolve(Self::pane_legacy()))
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

    /// The legacy bridge for a FACE slot: a transparent fill is no face at
    /// all (`None` — the surface below shows through), anything else is
    /// [`Material::from_fill`] of it. The `|alpha| > 0.001` test every face
    /// slot applied, stated once.
    pub fn face(encoded: [f32; 4]) -> Option<Self> {
        (encoded[3].abs() > 0.001).then(|| Self::from_fill(encoded))
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

    /// The popover material: a menu, a context menu, a dropdown's open
    /// surface. `base`'s colour at `style.surface.menu.opacity`
    /// (`color::menu_opacity`) — not the colour's own alpha, since a page
    /// colour is typically opaque and would resolve the frost to a solid
    /// tint — and frosted at the DE recipe, so a menu shows the content
    /// beneath it blurred and tinted rather than covering it.
    pub fn popover(base: [f32; 4]) -> Self {
        Self::opaque([base[0], base[1], base[2], crate::color::menu_opacity()]).with_frost(Frost::from_style())
    }

    // ---- derived materials -------------------------------------------------

    /// The well floor cut into this plate: the same material with the tint
    /// darkened by `WELL_FLOOR`'s strength (`WELL_FLOOR_LIFTED`'s when
    /// `lifted`, the hover cue). Frost and finish carried through — a well in
    /// glass is deeper glass (RFC § 11 (3)). `PaintCtx::well_floor` draws
    /// this for a FROSTED host; an opaque host's floor stays the darkening
    /// overlay, which is this exactly at plate alpha 1 and the honest
    /// darkening at any other alpha (a darkened fill at the plate's own
    /// alpha would barely darken a translucent plate).
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
    /// the control rung states: an opaque one is the face
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
        let _lock = crate::color::test_color_state_lock();
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
        let _lock = crate::color::test_color_state_lock();
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
        assert!(Material::face([0.3, 0.3, 0.3, 0.0]).is_none(), "transparent = no face");
        assert!(Material::face([0.3, 0.3, 0.3, -0.5]).is_some_and(|m| m.frost.is_frosted()));
        assert_eq!(Material::face([0.3, 0.3, 0.3, 0.7]).map(|m| m.tint), Some([0.3, 0.3, 0.3, 0.7]));
    }

    /// The pack is exact on its own grid, monotone, and never mixes the two
    /// halves; the shader's literals are the ones the Rust twin uses.
    #[test]
    fn frost_pack_round_trips() {
        for i in [0u32, 1, 2, 613, 614, 2047, 2048, 4094, 4095] {
            for j in [0u32, 1, 819, 4095] {
                let (c, r) = (i as f32 / Frost::PACK_MAX, j as f32 / Frost::PACK_MAX);
                let f = Frost::Frosted { compression: c, refraction: r, radius: 5.5 };
                let [z, w] = f.pack(2.0);
                let (c2, r2) = Frost::unpack(z);
                assert!((c2 - c).abs() < 1e-6 && (r2 - r).abs() < 1e-6, "{i},{j}: {c},{r} -> {c2},{r2}");
                assert_eq!(w, 11.0);
                assert!(z < (1u32 << 24) as f32, "packed value must stay an exact f32 integer");
            }
        }
        // 0.6 / 0.3 (the designer's recipe) survive to better than a 1/255 step.
        let (c, r) = Frost::unpack(Frost::Frosted { compression: 0.6, refraction: 0.3, radius: 0.0 }.pack(1.0)[0]);
        assert!((c - 0.6).abs() < 1.0 / 510.0 && (r - 0.3).abs() < 1.0 / 510.0);
        assert_eq!(Frost::Opaque.pack(2.0), [0.0, 0.0]);
        assert_eq!(Frost::Frosted { compression: 0.0, refraction: 0.0, radius: 0.0 }.pack(2.0), [0.0, 0.0]);

        let wgsl = include_str!("../vk/shader2d.wgsl");
        let lit = |name: &str| -> f32 {
            let rest = wgsl.split(&format!("const {name}: f32 = ")).nth(1).unwrap_or_else(|| panic!("{name} missing"));
            rest.split(';').next().unwrap().trim().parse().unwrap()
        };
        assert_eq!(lit("FROST_PACK_MAX"), Frost::PACK_MAX);
        assert_eq!(lit("FROST_PACK_BASE"), Frost::PACK_BASE);
        // The droplet's and the raw-vertex fallback's stride is the panel's
        // default kernel in physical px: DEFAULT_RADIUS × scale 2 / 2.
        assert_eq!(lit("LEGACY_STRIDE"), Frost::DEFAULT_RADIUS * 2.0 / 2.0);
    }

    const DESIGNER_LEGACY: &str = r##"
        style {
            surface {
                param color=(rgba)"#05050840"
                plate backdrop_compression=(f64)0.6 refraction=(f64)0.3 bevel_width=(f64)12.0 blur=(bool)true {
                    root corner_radius=(i64)24
                }
                relief depth=(f64)0.08
            }
        }
    "##;

    const DESIGNER_NAMED: &str = r##"
        style {
            surface {
                material {
                    glass {
                        color (rgba)"#05050840"
                        frost backdrop_compression=(f64)0.6 refraction=(f64)0.3
                    }
                }
                plate material="glass" bevel_width=(f64)12.0 {
                    root corner_radius=(i64)24
                }
                relief depth=(f64)0.08
            }
        }
    "##;

    /// The designer's frosted pane spelled with the legacy keys and as a
    /// named material bound to the pane rung resolve to the SAME material —
    /// the step-4 exit test: same Material, same bytes (steps 2–3).
    #[test]
    fn named_material_round_trips_the_legacy_spelling() {
        let _lock = crate::color::test_color_state_lock();
        crate::layout::lazy_init_style_registry();
        let _ = crate::color::plate_blur(); // fire the once-per-process load BEFORE the reload
        crate::layout::set_plate_opacity(1.0);
        crate::color::reload_colors(DESIGNER_LEGACY);
        assert_eq!(crate::color::material_binding(PlateRung::Pane), None);
        let legacy = Material::pane();
        assert!(legacy.frost.is_frosted());
        assert!((legacy.tint[3] - 0x40 as f32 / 255.0).abs() < 1e-6, "{:?}", legacy.tint);

        crate::color::reload_colors(DESIGNER_NAMED);
        assert_eq!(crate::color::material_binding(PlateRung::Pane).as_deref(), Some("glass"));
        let named = Material::pane();
        assert_eq!(named.tint, legacy.tint);
        assert_eq!(named.finish, legacy.finish);
        match (named.frost, legacy.frost) {
            (Frost::Frosted { compression: c1, refraction: r1, radius: d1 }, Frost::Frosted { compression: c2, refraction: r2, radius: d2 }) => {
                assert!((c1 - c2).abs() < 1e-6 && (r1 - r2).abs() < 1e-6 && d1 == d2, "{:?} vs {:?}", named.frost, legacy.frost);
            }
            other => panic!("{other:?}"),
        }
        // The DE recipe follows the bound pane.
        assert_eq!(Frost::from_style(), named.frost);
        assert_eq!(Material::named("glass"), Some(named));
        assert_eq!(Material::named("nope"), None);
        // The other rungs are unbound and unchanged.
        assert_eq!(Material::root(), Material::legacy(PlateRung::Root));
        assert_eq!(Material::control(), Material::legacy(PlateRung::Control));
        assert_eq!(crate::color::material_names(), vec!["glass".to_string()]);
        // Bindings and nodes are replaced wholesale by every load, so an
        // empty document unbinds every rung for the tests that follow.
        crate::color::reload_colors("");
        assert_eq!(crate::color::material_binding(PlateRung::Pane), None);
    }

    /// A binding wins over the legacy keys, a node without `frost` is opaque
    /// whatever `plate blur` says, unset fields fall back to the rung, and a
    /// binding to an undefined name degrades to the legacy material.
    #[test]
    fn binding_semantics() {
        let _lock = crate::color::test_color_state_lock();
        crate::layout::lazy_init_style_registry();
        let _ = crate::color::plate_blur(); // fire the once-per-process load BEFORE the reload
        crate::color::reload_colors(r##"
            style {
                surface {
                    material {
                        plastic {
                            finish spec=(f64)0.25 shininess=(f64)12.0
                        }
                        matte {
                            color (rgba)"#20202080"
                            finish light=(f64)0.3
                        }
                    }
                    plate blur=(bool)true material="plastic"
                    relief depth=(f64)0.15 spec=(f64)0.5 shininess=(f64)20.0 curvature=(f64)0.1
                }
                control material="ghost"
            }
        "##);
        let pane = Material::pane();
        assert_eq!(pane.frost, Frost::Opaque, "no frost child = opaque, blur flag or not");
        assert_eq!(pane.tint, Material::legacy(PlateRung::Pane).tint, "no color = the rung's tint");
        assert_eq!(pane.finish.spec, 0.25);
        assert_eq!(pane.finish.shininess, 12.0);
        assert_eq!(pane.finish.curvature, 0.1, "unset finish keys take the DE's (relief curvature)");
        assert_eq!(Finish::from_style().spec, 0.5, "style.surface.relief.spec is the DE finish");
        assert_eq!(Material::named("matte").map(|m| m.finish.strength), Some(0.3 / 0.15));
        assert_eq!(Material::control(), Material::legacy(PlateRung::Control), "unknown name = unbound");
        assert!(Frost::from_style().is_frosted(), "an opaque bound pane leaves the DE recipe to the keys");
        crate::color::set_finish_spec(0.4);
        crate::color::set_finish_shininess(24.0);
        crate::color::set_finish_curvature(0.2);
        crate::color::reload_colors("");
    }

    /// A popover is the base colour at menu opacity, frosted — the bytes the
    /// three menu sites used to write by negating an alpha.
    #[test]
    fn popover_is_the_menu_recipe() {
        let _lock = crate::color::test_color_state_lock();
        let m = Material::popover([0.1, 0.2, 0.3, 1.0]);
        let mut old = [0.1, 0.2, 0.3, 1.0];
        old[3] = -crate::color::menu_opacity();
        assert_eq!(m.fill(PlateRole::Nested), old);
        assert!(m.frost.is_frosted());
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
