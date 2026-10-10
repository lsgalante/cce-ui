//! A named material as config spells it (`style.surface.material`), resolved over a rung's
//! material, and the rungs (`PlateRung`) a material can be bound to.

use super::*;

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
            None => Frost::Unfrosted,
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
