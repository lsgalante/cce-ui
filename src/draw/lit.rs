//! The lit, textured mesh path of the 3D scene pass, beside the
//! `Vertex3D` one and drawn in the same pass.
//!
//! A [`Vertex3D`](super::scene::Vertex3D) is a position and a colour, shaded
//! flat or pre-lit by the host. A [`LitVertex`] carries what a model file
//! carries — a normal, a texture coordinate and a colour — and is lit in the
//! fragment shader (`scene3d_lit.wgsl`): a base colour (the material's,
//! times the vertex colour, times a texel of its base-colour image), a
//! metallic-roughness specular from GGX, a key and a fill light and a
//! sky/ground ambient, all view-dependent, so a highlight moves as the
//! camera does. It exists for files that bring their own normals and
//! textures (glTF, OBJ with an MTL `map_Kd`), which the flat path could
//! only show in a factor colour.
//!
//! It is ADDITIVE: [`Stage3D::lit`](super::scene::Stage3D::lit) answers
//! `None` unless a renderer implements it (the Vulkan one does; the WebGPU
//! one does not yet), so a host checks and keeps the flat path otherwise,
//! and nothing that does not ask for it changed.
//!
//! Textures are ordinary image ids (`upload_rgba`): RGBA8 sRGB, so a texel
//! reaches the shader linear, as an albedo must. They die with the renderer
//! like every image id; a host re-uploads them in `init_3d`. The image
//! sampler clamps, so the shader wraps texture coordinates itself (glTF
//! repeats by default), sampling with the unwrapped coordinates' gradients
//! so the wrap leaves no seam.

/// One corner of a lit triangle. `uv` is in image convention: (0, 0) the
/// top-left texel, as glTF has it (an OBJ `vt` needs its v flipped).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LitVertex {
    pub position: [f32; 3],
    /// Unit length; the shader flips it for a face seen from behind, so open
    /// and single-sided meshes light from both sides.
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    /// Linear RGB, multiplying the material's base colour: white for none.
    pub color: [f32; 3],
}

/// A lit mesh a renderer holds, as [`LitStage3D::create_lit_mesh`] named it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LitMeshId(pub(crate) usize);

/// How a lit surface answers the light (glTF's metallic-roughness model).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LitMaterial {
    /// Linear RGB.
    pub base_color: [f32; 3],
    /// An image id (`upload_rgba`) whose texels multiply `base_color`, or
    /// `None`. A texture whose upload has not landed draws as `None`.
    pub texture: Option<u32>,
    /// 0 a dielectric (plastic, paint, wood), 1 a metal.
    pub metallic: f32,
    /// 0 a mirror, 1 fully rough.
    pub roughness: f32,
}

impl Default for LitMaterial {
    fn default() -> Self {
        Self { base_color: [1.0; 3], texture: None, metallic: 0.0, roughness: 0.8 }
    }
}

/// The light every lit draw is shaded by: a key and a fill (directions
/// TOWARD them, world space, any length; colours linear and may exceed 1),
/// and an ambient that blends from `ground` (a surface facing down) to `sky`
/// (facing up), which also stands in for the environment a metal reflects.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LitLight {
    pub key_toward: [f32; 3],
    pub key_color: [f32; 3],
    pub fill_toward: [f32; 3],
    pub fill_color: [f32; 3],
    pub sky: [f32; 3],
    pub ground: [f32; 3],
}

impl Default for LitLight {
    fn default() -> Self {
        Self {
            key_toward: [-0.35, 0.75, 0.55],
            key_color: [0.95; 3],
            fill_toward: [0.75, 0.1, -0.1],
            fill_color: [0.25; 3],
            sky: [0.2; 3],
            ground: [0.06; 3],
        }
    }
}

/// One lit draw in the staged scene.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LitDraw {
    pub mesh: LitMeshId,
    pub mvp: [[f32; 4]; 4],
    /// The camera's position in the space the mesh is in, for the specular.
    pub eye: [f32; 3],
    pub material: LitMaterial,
    /// Whole-draw alpha multiplier (1 = opaque), as `SceneDraw::opacity`.
    pub opacity: f32,
    /// The width of a wire pass that will ride on this fill (0 = none), as
    /// `SceneDraw::wire_base_width`: the fill is pushed back so the wires
    /// win the depth test.
    pub wire_base_width: f32,
    /// Draw order: this draw renders before the `SceneDraw` at this index
    /// of the staged list, `u32::MAX` after them all — as
    /// `SceneImage::before`. A host drawing a background and a floor as
    /// scene draws and wires over the model puts the model between.
    pub before: u32,
}

/// What a renderer offers for lit meshes, reached through
/// [`Stage3D::lit`](super::scene::Stage3D::lit).
pub trait LitStage3D {
    fn create_lit_mesh(&mut self, verts: &[LitVertex]) -> LitMeshId;
    /// Replace a lit mesh's vertices (a new model in the same slot).
    fn update_lit_mesh(&mut self, id: LitMeshId, verts: &[LitVertex]);
    /// The light every lit draw is shaded by, from now on.
    fn set_lit_light(&mut self, light: LitLight);
    /// This frame's lit draws, after [`Stage3D::stage_scene`](super::scene::Stage3D::stage_scene)
    /// (nothing is drawn when no scene is staged).
    fn stage_lit(&mut self, draws: Vec<LitDraw>);
}

/// `scene3d_lit.wgsl`'s uniform block. Its head is the scene block's (mvp,
/// window size, corner radius and shape), so the corner cut is shared.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct LitUniforms {
    mvp: [[f32; 4]; 4],
    window_size: [f32; 2],
    window_radius: f32,
    corner_shape: f32,
    /// xyz the eye, w unused.
    eye: [f32; 4],
    /// rgb the base colour, a the draw's opacity.
    base: [f32; 4],
    /// metallic, roughness, 1 when textured, unused.
    surface: [f32; 4],
    key_toward: [f32; 4],
    key_color: [f32; 4],
    fill_toward: [f32; 4],
    fill_color: [f32; 4],
    sky: [f32; 4],
    ground: [f32; 4],
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))] // the Vulkan renderer's; WebGPU has no lit pass yet
pub(crate) const LIT_UNIFORM_SIZE: usize = std::mem::size_of::<LitUniforms>();

#[cfg_attr(target_arch = "wasm32", allow(dead_code))] // the Vulkan renderer's; WebGPU has no lit pass yet
fn unit4(v: [f32; 3]) -> [f32; 4] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l > 0.0 {
        [v[0] / l, v[1] / l, v[2] / l, 0.0]
    } else {
        [0.0, 1.0, 0.0, 0.0]
    }
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))] // the Vulkan renderer's; WebGPU has no lit pass yet
fn pad(v: [f32; 3]) -> [f32; 4] {
    [v[0], v[1], v[2], 0.0]
}

/// The staged lit draws' uniform blocks, in order. A texture not resident
/// in the renderer is bound as its white fallback, so "textured" with no
/// texture yet draws the plain base colour.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))] // the Vulkan renderer's; WebGPU has no lit pass yet
pub(crate) fn lit_uniforms(
    draws: &[LitDraw],
    light: &LitLight,
    window_size: [f32; 2],
    corner_radius_px: f32,
) -> Vec<LitUniforms> {
    let corner_shape = crate::layout::corner_shape();
    draws
        .iter()
        .map(|d| {
            let m = &d.material;
            LitUniforms {
                mvp: d.mvp,
                window_size,
                window_radius: corner_radius_px,
                corner_shape,
                eye: pad(d.eye),
                base: [m.base_color[0], m.base_color[1], m.base_color[2], d.opacity],
                surface: [
                    m.metallic.clamp(0.0, 1.0),
                    m.roughness.clamp(0.04, 1.0),
                    if m.texture.is_some() { 1.0 } else { 0.0 },
                    0.0,
                ],
                key_toward: unit4(light.key_toward),
                key_color: pad(light.key_color),
                fill_toward: unit4(light.fill_toward),
                fill_color: pad(light.fill_color),
                sky: pad(light.sky),
                ground: pad(light.ground),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_uniform_block_is_what_the_shader_reads() {
        // mvp, then window size/radius/shape, then nine vec4s.
        assert_eq!(LIT_UNIFORM_SIZE, 64 + 16 + 9 * 16);
        assert_eq!(std::mem::size_of::<LitVertex>(), 11 * 4);
    }

    #[test]
    fn a_textured_draw_says_so_and_lights_are_normalized() {
        let draw = LitDraw {
            mesh: LitMeshId(0),
            mvp: [[0.0; 4]; 4],
            eye: [0.0, 0.0, 5.0],
            material: LitMaterial { texture: Some(7), ..LitMaterial::default() },
            opacity: 1.0,
            wire_base_width: 0.0,
            before: u32::MAX,
        };
        let light = LitLight { key_toward: [0.0, 3.0, 0.0], ..LitLight::default() };
        let plain = LitDraw { material: LitMaterial::default(), ..draw };
        let blocks = lit_uniforms(&[draw, plain], &light, [100.0, 100.0], 0.0);
        assert_eq!(blocks[0].surface[2], 1.0);
        assert_eq!(blocks[1].surface[2], 0.0);
        assert_eq!(blocks[0].key_toward, [0.0, 1.0, 0.0, 0.0]);
    }
}
