//! `Finish`: the plastic finish — how a surface answers the DE's light.

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
