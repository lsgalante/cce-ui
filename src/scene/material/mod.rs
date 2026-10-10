//! Material — what a surface is made of (`docs/rfc-material.md`).
//!
//! A [`Material`] is a surface's **tint**, its **frost** (whether and how it shows what is behind
//! it) and its **finish** (how it answers the DE's light), carried BY VALUE on the plate made of
//! it — `PlateSpec`, `ControlPlate` and the plate prims each carry one. The blur-behind sentinel
//! encoding lives in exactly one function ([`Material::fill_tint`]); the rung defaults
//! ([`Material::root`], [`Material::pane`], [`Material::control`]) resolve from the style; and a
//! config can name materials and bind them to rungs ([`MaterialDef`]).
//!
//! What is deliberately NOT here: the light (the scene's, `relief_shade::light_vector`), the roll
//! width (geometry, per prim as `depth`) and the carve/roll profiles (per-window uniforms). See
//! the RFC's non-goals.
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | `Material` and `PlateRole`: construction, the rung defaults, fills and the sentinel |
//! | `finish` | `Finish`, how a surface answers light |
//! | `frost` | `Frost`, whether and how a surface shows what is behind it, and its packing |
//! | `def` | a named material as config spells it (`MaterialDef`, `FrostDef`) and the rungs it binds to |

mod def;
mod finish;
mod frost;
#[cfg(test)]
mod tests;

pub use def::*;
pub use finish::*;
pub use frost::*;

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
        Self { tint, frost: Frost::Unfrosted, finish: Finish::from_style() }
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
    /// `Frost::Unfrosted` on the client side by construction: a root plate's
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
        // `style.surface.plate.pane.color` is the whole tint, alpha
        // included; the legacy `param.color` is still multiplied by the
        // top-level `plate_opacity` line, as it always was.
        if !crate::color::pane_color_is_whole() {
            tint[3] *= crate::layout::plate_opacity();
        }
        let frost = if crate::color::plate_blur() {
            Frost::Frosted {
                compression: crate::color::plate_backdrop_compression(),
                refraction: crate::color::plate_refraction(),
                radius: crate::color::plate_frost_radius(),
            }
        } else {
            Frost::Unfrosted
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
    /// the DE recipe, tint alpha `|a|`; otherwise `Unfrosted` with the colour
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
            PlateRole::Root => Material { frost: Frost::Unfrosted, ..*self },
            PlateRole::Nested => *self,
        }
    }

    /// The popover material: a menu, a context menu, a dropdown's open
    /// surface. `base`'s colour at `style.surface.menu.opacity`
    /// (`color::menu_opacity`) — not the colour's own alpha, since a page
    /// colour is typically opaque and would resolve the frost to a solid
    /// tint — and frosted at the DE recipe, so a menu shows the content
    /// beneath it blurred and tinted rather than covering it, with the
    /// recipe's compression replaced by the menu's own
    /// (`style.surface.menu.compression`, `color::menu_compression`): a menu
    /// is read over whatever it opened above, so it holds its key harder
    /// than a pane does.
    pub fn popover(base: [f32; 4]) -> Self {
        let mut frost = Frost::from_style();
        if let Frost::Frosted { compression, .. } = &mut frost {
            *compression = crate::color::menu_compression();
        }
        Self::opaque([base[0], base[1], base[2], crate::color::menu_opacity()]).with_frost(frost)
    }

    /// THE menu material: [`Material::popover`] of `color::menu_color` —
    /// `style.surface.menu.color`, else the root plate colour. What a
    /// context menu's plate is made of, and what any other surface that
    /// should read as one (a command palette, a modal list) asks for, so
    /// the `style.surface.menu` block is the one place both are configured.
    pub fn menu() -> Self {
        Self::popover(crate::color::menu_color())
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
    /// - nested + [`Frost::Unfrosted`] → the tint as is.
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
