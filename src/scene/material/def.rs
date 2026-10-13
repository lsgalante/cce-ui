//! A named material as config spells it (`style.surface.material`), resolved over a rung's
//! material, and the rungs (`PlateRung`) a material can be bound to. The types are defined in
//! `color::materials`; `MaterialDef::resolve` is here, where `Material` is.

use super::*;

// The config types live in `color` (`color::materials`), beside the slots
// that hold them, so the colour layer names nothing above it; this module
// adds what needs a `Material` to say, and keeps their paths.
pub use crate::color::{FrostDef, MaterialDef, PlateRung};

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
