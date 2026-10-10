//! `Frost`: whether a surface shows what is behind it, and the recipe it shows it with —
//! compression, refraction and blur radius — packed into a plate's push block.

use super::*;

/// Whether and how a surface shows what is behind it.
///
/// An enum, not two floats and a bool: an opaque plate has no compression and
/// no refraction — not zero of each, none — and making the recipe unreachable
/// when the plate is not frosted is what keeps [`Material::fill`] to one
/// question.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Frost {
    /// No frost: the tint alone, composited at its ALPHA — a translucent
    /// tint is still translucent, and what shows through is sharp. This
    /// says nothing about coverage; it says the plate never samples its
    /// backdrop. (Named `Opaque` until 2026-09-28, which read as "solid"
    /// and was not.)
    Unfrosted,
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
    /// physical px). `Unfrosted` packs to zeros: nothing reads them, and a
    /// plate that was never frosted pushes the bytes it always did.
    pub fn pack(&self, scale: f32) -> [f32; 2] {
        match *self {
            Frost::Unfrosted => [0.0, 0.0],
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

    /// [`Frost::from_style`] when `on`, else [`Frost::Unfrosted`] — the shape of
    /// every `blur: bool` the toolkit carries today.
    pub fn from_flag(on: bool) -> Self {
        if on { Self::from_style() } else { Frost::Unfrosted }
    }

    pub fn is_frosted(&self) -> bool {
        matches!(self, Frost::Frosted { .. })
    }
}
