//! The path tracer's scene and the work done on the CPU for it, with no GPU
//! in it: the schema an app fills ([`RtTriangle`], [`RtMaterial`],
//! [`RtImage`], [`RtCamera`], [`RtEnvironment`]), the binned-SAH BVH built
//! over it, the buffers and parameter blocks laid out as `rt_common.wgsl` /
//! `rt_bvh.wgsl` / `rt_denoise.wgsl` read them, and the tracer's constants.
//! The Vulkan stage (`vk::rt`, with a hardware ray-query tier besides) and
//! the WebGPU one (`web::rt`, the compute tier) both trace from these.
//!
//! The tracer is plain compute: a CPU-built BVH traversed per pixel,
//! progressive accumulation of one sample a frame, an à-trous denoise over
//! the running mean, blitted into the backdrop's pane region in place of the
//! raster scene. Scene schema is internal by design — importers (OBJ/glTF)
//! belong in a loader that converts *into* [`RtTriangle`]/[`RtMaterial`].
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the scene schema an app fills: triangles, materials, images, the camera, the environment |
//! | `bvh` | the binned-SAH BVH built over the triangles |
//! | `pack` | the GPU layouts, the tracer's constants, packing a scene into buffers (`PreparedRtScene`), the parameter blocks |

mod bvh;
mod pack;
#[cfg(test)]
mod tests;

pub(crate) use bvh::*;
pub use pack::*;

/// One triangle of an RT scene, in the same space as the camera's `inv_mvp`
/// (for the designer: mesh space, the space `Vertex3D` positions live in).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RtTriangle {
    pub p0: [f32; 3],
    pub p1: [f32; 3],
    pub p2: [f32; 3],
    /// Index into the material slice passed alongside.
    pub material: u32,
}

/// Lambertian surface + optional emission, linear color (matching the raster
/// path, whose vertex colors land in the sRGB attachment as linear values).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RtMaterial {
    pub albedo: [f32; 3],
    pub emission: [f32; 3],
}

/// An image standing in the traced scene: a quad that shows the image's
/// colour as it is — unlit, as the raster pass's `SceneImage` draws it — and
/// lets a ray through where the image is clear. A path ends on it, so to
/// the rest of the scene it is a light of its own colour. The tracer adds
/// the quad to the scene itself.
///
/// The image is one the 2D pass already holds (an id from `upload_rgba`),
/// so a picture shown in the raster viewport costs the tracer nothing more.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RtImage {
    pub image: u32,
    /// The quad's corners in the scene's space, in the image's own order:
    /// top-left, top-right, bottom-right, bottom-left.
    pub corners: [[f32; 3]; 4],
    /// Alpha multiplier over the image's own.
    pub opacity: f32,
}

/// [`RtImage`] for the headless tracer, which has no 2D pass to share an
/// image with and is handed the pixels: tightly packed sRGB RGBA8.
#[derive(Debug, Clone, Copy)]
pub struct RtImagePixels<'a> {
    pub pixels: &'a [u8],
    pub width: u32,
    pub height: u32,
    pub corners: [[f32; 3]; 4],
    pub opacity: f32,
}

/// The full camera: the inverse of the raster path's `proj * view * model`.
/// Rays are unprojected from NDC through it, so any matrix stack that renders
/// the raster viewport drives the tracer unchanged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RtCamera {
    pub inv_mvp: [[f32; 4]; 4],
}

/// The traced scene's light: a sky that grades from `sky_nadir` straight
/// down to `sky_zenith` straight up, and a sun — a bright lobe toward
/// `sun_direction` of `sun_color` radiance. It is the tracer's ONLY light
/// (nothing in a scene emits unless its material does). Colours are linear
/// RGB and may exceed 1. [`Default`] is the studio sky the tracer always
/// had.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RtEnvironment {
    /// Toward the sun, world space; any length (zero is straight up).
    pub sun_direction: [f32; 3],
    pub sun_color: [f32; 3],
    pub sky_zenith: [f32; 3],
    pub sky_nadir: [f32; 3],
}

impl Default for RtEnvironment {
    fn default() -> Self {
        Self {
            sun_direction: [0.45, 0.75, 0.35],
            sun_color: [8.0, 7.6, 6.8],
            sky_zenith: [0.72, 0.82, 0.98],
            sky_nadir: [0.32, 0.31, 0.35],
        }
    }
}

impl RtEnvironment {
    pub(crate) fn sun_unit(&self) -> [f32; 3] {
        let v = glam::Vec3::from_array(self.sun_direction);
        if v.length_squared() > 1e-12 { v.normalize().to_array() } else { [0.0, 1.0, 0.0] }
    }
}
