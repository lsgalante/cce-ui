//! The 2D, glyph and 3D scene shaders, shared by every renderer: one WGSL
//! source each.
//!
//! The Vulkan renderer compiles them to SPIR-V through naga. WebGPU takes WGSL
//! as it is, with one difference: it has no push constants, so the per-batch
//! parameter block (`RRectClip`, [`super::PUSH_CONSTANT_FLOATS`] floats laid
//! out by [`super::batch_push_constants`]) is a uniform there, bound at
//! `@group(1) @binding(0)` with a dynamic offset per batch
//! ([`shader2d_for_webgpu`]).

/// The 2D pipeline's shader: geometry, the SDF-lit plates, blur-behind.
pub const SHADER2D: &str = include_str!("shader2d.wgsl");

/// Text and image quads.
pub const GLYPH: &str = include_str!("glyph.wgsl");

/// The 3D scene pass's meshes ([`super::scene`]).
pub const SCENE3D: &str = include_str!("scene3d.wgsl");

/// The 3D scene pass's images: textured quads under the meshes' uniforms.
pub const SCENE3D_IMAGE: &str = include_str!("scene3d_image.wgsl");

/// The lit, textured mesh path of the scene pass (`draw::lit`).
pub const SCENE3D_LIT: &str = include_str!("scene3d_lit.wgsl");

/// The path tracer ([`super::rt`]): its shared core, then one trace tier
/// after it — the compute BVH traversal, or (Vulkan only) hardware ray
/// queries — and the à-trous denoiser that runs over its output.
pub const RT_COMMON: &str = include_str!("rt_common.wgsl");
pub const RT_BVH: &str = include_str!("rt_bvh.wgsl");
pub const RT_QUERY: &str = include_str!("rt_query.wgsl");
pub const RT_DENOISE: &str = include_str!("rt_denoise.wgsl");

/// The compute tier's tracer: [`RT_COMMON`] with [`RT_BVH`] after it.
pub fn rt_bvh_source() -> String {
    format!("{RT_COMMON}\n{RT_BVH}")
}

/// The one line that differs between the Vulkan and the WebGPU 2D shader.
const PUSH_BLOCK: &str = "var<push_constant> rrect_clip: RRectClip;";
const UNIFORM_BLOCK: &str = "@group(1) @binding(0) var<uniform> rrect_clip: RRectClip;";

/// Where a batch's block sits in WebGPU's uniform buffer: each batch's
/// [`super::PUSH_CONSTANT_FLOATS`] floats at a multiple of this, the
/// `minUniformBufferOffsetAlignment` every WebGPU device supports.
pub const WEBGPU_BLOCK_STRIDE: usize = 256;

/// [`SHADER2D`] for WebGPU: the push-constant block declared as a uniform.
pub fn shader2d_for_webgpu() -> String {
    assert_eq!(SHADER2D.matches(PUSH_BLOCK).count(), 1, "shader2d.wgsl must declare its push block exactly once");
    SHADER2D.replace(PUSH_BLOCK, UNIFORM_BLOCK)
}
