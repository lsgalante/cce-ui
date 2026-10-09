//! A droplet spec resolved against its rect: the silhouette the lit drop and its scrim share.

use super::*;

/// A droplet spec resolved against a concrete rect: the push-constant fields
/// that define its SILHOUETTE, in logical px.
///
/// Shared by [`crate::scene::paint::Prim::Droplet`] and
/// [`crate::scene::paint::Prim::DropletScrim`] so the lit drop and the vignette
/// drawn inside it can never disagree about the shape — the whole reason the
/// scrim rides the droplet's shader path instead of approximating the outline
/// with a rounded rect.
pub(super) struct DropletGeom {
    pub(super) hx: f32,
    pub(super) hy: f32,
    pub(super) sag: f32,
    pub(super) br: f32,
    pub(super) bw: f32,
    pub(super) k: f32,
    pub(super) sr: f32,
    pub(super) ar: f32,
    pub(super) band: f32,
    pub(super) bow: f32,
    /// How far the contact shadow reaches below/beside the box (0 when the
    /// spec has no shadow). The lit drop's cover quad grows by this; a scrim
    /// never draws outside the silhouette and ignores it.
    pub(super) sh_reach: f32,
}

pub(super) fn droplet_geom(rect: &crate::scene::layout::Rect, spec: &crate::scene::paint::DropletSpec) -> DropletGeom {
    let hx = rect.width * 0.5;
    let hy = rect.height * 0.5;
    let sag = spec.sag.clamp(0.0, 0.9) * rect.height;
    // belly <= 0 disables the belly outright (the oval-dewdrop default) — the
    // shader skips the smin when the radius is 0.
    let (br, bw) = if spec.belly > 0.0 {
        let br = (spec.belly.min(1.0) * rect.height).min(hy).min(hx);
        (br, ((hx - br).max(0.0) * spec.belly_w.clamp(0.0, 1.0)).max(1.0))
    } else {
        (0.0, 0.0)
    };
    let k = (spec.blend.max(0.0) * rect.height).max(1.0);
    let sheet_hy = hy - sag * 0.5;
    // Bottom (sheet_r) and top (attach) corner radii: when the pair overfills
    // the sheet height, scale both down proportionally — 0.5 + 0.5 is the
    // fully continuous egg.
    let mut sr = (spec.sheet_r.clamp(0.0, 1.0) * rect.height).min(hx);
    let mut ar = (spec.attach.clamp(0.0, 1.0) * rect.height).min(hx);
    let sheet_h = (2.0 * sheet_hy).max(0.0);
    if sr + ar > sheet_h && sr + ar > 0.0 {
        let f = sheet_h / (sr + ar);
        sr *= f;
        ar *= f;
    }
    let band = (spec.band.max(0.05) * rect.height).max(1.0);
    // Bottom-bow edge rise; the shader derives the arc radius from it per drop
    // (R = hx^2/2*rise).
    let bow = (spec.bow.clamp(0.0, 0.5) * rect.height).min(hy * 0.9);
    let sh_reach = if spec.shadow > 0.0 { (0.18 * rect.height).max(2.0) } else { 0.0 };
    DropletGeom { hx, hy, sag, br, bw, k, sr, ar, band, bow, sh_reach }
}

impl Tess {
    /// A droplet or its scrim: lit by its SDF on the shader path, its flat outline on the banded one.
    pub(super) fn droplet(&mut self, it: &mut Item) -> bool {
        match &it.item.prim {
            Prim::DropletScrim { rect, material, spec, feather } if self.shader_plates => {
                let color = material.fill(PlateRole::Nested);
                let mat = material.finish.to_array();
                // Shader mode 12: the droplet's own SDF, filled flat and
                // feathered inward. No contact shadow, so unlike the lit drop
                // the cover quad is exactly the box — a scrim never draws
                // outside the silhouette.
                let g = droplet_geom(rect, spec);
                self.verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, self.sw, self.sh, color));
                it.plate = Some(crate::draw::PlatePush {
                    rect: [
                        (rect.x + rect.width * 0.5) * self.scale,
                        (rect.y + rect.height * 0.5) * self.scale,
                        g.hx * self.scale,
                        g.hy * self.scale,
                    ],
                    radii: [g.sag * self.scale, g.br * self.scale, g.bw * self.scale, g.k * self.scale],
                    // p_light.w carries the FEATHER here; mode 12 returns
                    // before the shading band it otherwise holds is read.
                    light: [self.light[0], self.light[1], self.light[2], feather.max(0.001) * self.scale],
                    material: [mat[0], 0.0, 0.0, 0.0],
                    host: [g.sr * self.scale, 0.0, 0.0, g.ar * self.scale],
                    specular_tint: [0.0, 0.0, 0.0, g.bow * self.scale],
                    mode: 12.0,
                    shape: spec.curve.clamp(2.0, 6.0),
                });
            }
            Prim::Droplet { rect, material, spec } if self.shader_plates => {
                let color = material.fill(PlateRole::Nested);
                let mat = material.finish.to_array();
                // A water droplet lit by shader mode 10: one cover quad; the
                // shader owns silhouette (sheet ∪smin belly), dome shading,
                // fresnel rim and thin-edge clarity. The spec's height
                // fractions resolve against the concrete rect here, clamped so
                // small or narrow boxes stay well-formed (a belly wider than
                // the box would turn the SDF interior inside out).
                // The cover quad grows sideways and BELOW the box by the
                // contact shadow's reach — shadow fragments live outside the
                // silhouette, so they need covered pixels to shade.
                let g = droplet_geom(rect, spec);
                let (hx, hy, sag, br, bw, k, sr, ar, band, bow, sh_reach) =
                    (g.hx, g.hy, g.sag, g.br, g.bw, g.k, g.sr, g.ar, g.band, g.bow, g.sh_reach);
                self.verts.extend(quad_vertices(
                    rect.x - sh_reach,
                    rect.y,
                    rect.width + 2.0 * sh_reach,
                    rect.height + sh_reach,
                    self.sw, self.sh, color,
                ));
                it.plate = Some(crate::draw::PlatePush {
                    rect: [
                        (rect.x + rect.width * 0.5) * self.scale,
                        (rect.y + rect.height * 0.5) * self.scale,
                        hx * self.scale,
                        hy * self.scale,
                    ],
                    radii: [sag * self.scale, br * self.scale, bw * self.scale, k * self.scale],
                    light: [self.light[0], self.light[1], self.light[2], band * self.scale],
                    // Slots y/z/w feed roll_spec and the rim term directly:
                    // a droplet's material carries its own gleam/shine/rim
                    // there (`DropletSpec::finish`; a drop is wetter than the
                    // DE's plates), so this is the material's finish like any
                    // plate's.
                    material: mat,
                    host: [sr * self.scale, spec.clarity.clamp(0.0, 1.0), spec.dome, ar * self.scale],
                    // Droplet glints are always white, so the tint RGB slots
                    // carry droplet params instead: x = core density,
                    // y = contact-shadow reach px, z = shadow strength.
                    specular_tint: [
                        spec.core.clamp(0.0, 2.0),
                        sh_reach * self.scale,
                        spec.shadow.clamp(0.0, 1.0),
                        bow * self.scale,
                    ],
                    mode: 10.0,
                    shape: spec.curve.clamp(2.0, 6.0),
                });
            }
            Prim::DropletScrim { rect, material, spec, .. } => {
                let color = material.fill(PlateRole::Nested);
                // Legacy banded path: no SDF to feather against, so the scrim
                // degrades to the same flat outline the drop itself does —
                // hard-edged, but present. A prim with no arm here VANISHES.
                let cap = (rect.height * 0.5).min(rect.width * 0.5);
                let sr = (spec.sheet_r.clamp(0.0, 1.0) * rect.height).min(cap);
                let ar = (spec.attach.clamp(0.0, 1.0) * rect.height).min(cap);
                let radii = crate::widget::CornerRadii::new(ar, ar, sr, sr);
                push_rounded_rect_vertices_corners(rect.x, rect.y, rect.width, rect.height, radii, self.sw, self.sh, color, it.circle, None, &mut self.verts);
            }
            Prim::Droplet { rect, material, spec } => {
                let color = material.fill(PlateRole::Nested);
                // Legacy banded path: the flat drop outline — attach-tapered
                // top, round bottom. Degrades the material but keeps the
                // silhouette (a prim with no arm here VANISHES, it doesn't
                // degrade — see Ridge/Groove above).
                let cap = (rect.height * 0.5).min(rect.width * 0.5);
                let sr = (spec.sheet_r.clamp(0.0, 1.0) * rect.height).min(cap);
                let ar = (spec.attach.clamp(0.0, 1.0) * rect.height).min(cap);
                let radii = crate::widget::CornerRadii::new(ar, ar, sr, sr);
                push_rounded_rect_vertices_corners(rect.x, rect.y, rect.width, rect.height, radii, self.sw, self.sh, color, it.circle, None, &mut self.verts);
            }
            _ => unreachable!("droplet: not a droplet prim"),
        }
        true
    }
}
