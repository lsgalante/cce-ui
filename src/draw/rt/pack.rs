//! What the tracer's shaders read, laid out as `rt_common.wgsl` / `rt_bvh.wgsl` /
//! `rt_denoise.wgsl` declare it: the GPU structs, the tracer's constants, a scene packed into its
//! buffers (`PackedScene`, `PreparedRtScene` — the CPU half, built off the UI thread), and the
//! per-frame parameter blocks for the trace and the denoise.

use super::*;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuTriangle {
    pub(crate) p0: [f32; 4], // w = material index (bitcast)
    pub(crate) p1: [f32; 4],
    pub(crate) p2: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuMaterial {
    pub(crate) albedo: [f32; 4],
    pub(crate) emission: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuBvhNode {
    pub(crate) min: [f32; 3],
    /// Leaf (`count > 0`): first triangle. Internal: left child; right = +1.
    pub(crate) left_first: u32,
    pub(crate) max: [f32; 3],
    pub(crate) count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct RtParams {
    pub(crate) inv_mvp: [[f32; 4]; 4],
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) sample_index: u32,
    pub(crate) max_bounces: u32,
    pub(crate) spp: u32,
    pub(crate) _pad: [u32; 3],
    pub(crate) img_origin: [f32; 4],
    pub(crate) img_u: [f32; 4],
    pub(crate) img_v: [f32; 4],
    pub(crate) background: [f32; 4],
    // The environment, each xyz in a vec4 (w unused): toward the sun
    // (unit), the sun's radiance, the sky overhead, the sky below.
    pub(crate) sun_dir: [f32; 4],
    pub(crate) sun_color: [f32; 4],
    pub(crate) sky_zenith: [f32; 4],
    pub(crate) sky_nadir: [f32; 4],
}

pub(crate) const MAX_SAMPLES: u32 = 1024;

pub(crate) const MAX_BOUNCES: u32 = 4;

pub(crate) const WORKGROUP: u32 = 8;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct DenoiseParams {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) step: u32,
    pub(crate) first: u32,
    pub(crate) last: u32,
    pub(crate) inv_sqrt_n: f32,
    pub(crate) _pad: [u32; 2],
}

pub(crate) const DENOISE_ITERATIONS: usize = 3; // à-trous steps 1, 2, 4

/// A scene as the tracer's buffers hold it.
#[derive(Clone)]
pub(crate) struct PackedScene {
    pub tris: Vec<GpuTriangle>,
    pub materials: Vec<GpuMaterial>,
    /// Empty unless `with_bvh` was asked for (the ray-query tier builds
    /// its own structure).
    pub nodes: Vec<GpuBvhNode>,
    /// The BVH was built (`nodes` may still be empty: an empty scene).
    pub has_bvh: bool,
}

impl PackedScene {
    /// This scene with its BVH: itself when it has one, else one built
    /// now over a copy of the triangles — the compute tier handed a scene
    /// prepared for the ray-query one.
    pub(crate) fn with_bvh(&self) -> std::borrow::Cow<'_, PackedScene> {
        if self.has_bvh {
            return std::borrow::Cow::Borrowed(self);
        }
        let xyz = |p: [f32; 4]| [p[0], p[1], p[2]];
        let mut tris: Vec<RtTriangle> = self
            .tris
            .iter()
            .map(|t| RtTriangle { p0: xyz(t.p0), p1: xyz(t.p1), p2: xyz(t.p2), material: t.p0[3].to_bits() })
            .collect();
        let nodes = build_bvh(&mut tris);
        std::borrow::Cow::Owned(PackedScene {
            tris: tris.iter().map(gpu_triangle).collect(),
            materials: self.materials.clone(),
            nodes,
            has_bvh: true,
        })
    }
}

pub(super) fn gpu_triangle(t: &RtTriangle) -> GpuTriangle {
    GpuTriangle {
        p0: [t.p0[0], t.p0[1], t.p0[2], f32::from_bits(t.material)],
        p1: [t.p1[0], t.p1[1], t.p1[2], 0.0],
        p2: [t.p2[0], t.p2[1], t.p2[2], 0.0],
    }
}

/// Lay a scene out for the tracer. An image joins it as a quad of two
/// triangles (corners in its own order, top-left first) under a material
/// of its own, marked textured (`albedo.w`), past the scene's materials —
/// or past the one stood in for a scene that names none — so every tier
/// meets it as it meets any triangle, and a scene that is an image alone is
/// not an empty one. `with_bvh` builds the BVH, reordering the triangles.
pub(crate) fn pack_scene(
    mut tris: Vec<RtTriangle>,
    materials: &[RtMaterial],
    image_corners: Option<[[f32; 3]; 4]>,
    with_bvh: bool,
) -> PackedScene {
    let image_material = materials.len().max(1) as u32;
    if let Some([tl, tr, br, bl]) = image_corners {
        tris.push(RtTriangle { p0: tl, p1: bl, p2: tr, material: image_material });
        tris.push(RtTriangle { p0: tr, p1: bl, p2: br, material: image_material });
    }
    let nodes = if with_bvh { build_bvh(&mut tris) } else { Vec::new() };
    let gpu_tris = tris.iter().map(gpu_triangle).collect();
    drop(tris);
    let mut gpu_mats: Vec<GpuMaterial> = if materials.is_empty() {
        vec![GpuMaterial { albedo: [0.8, 0.8, 0.8, 0.0], emission: [0.0; 4] }]
    } else {
        materials
            .iter()
            .map(|m| GpuMaterial {
                albedo: [m.albedo[0], m.albedo[1], m.albedo[2], 0.0],
                emission: [m.emission[0], m.emission[1], m.emission[2], 0.0],
            })
            .collect()
    };
    if image_corners.is_some() {
        // albedo.w marks it textured: the shader takes the colour from the image.
        gpu_mats.push(GpuMaterial { albedo: [1.0, 1.0, 1.0, 1.0], emission: [0.0; 4] });
    }
    PackedScene { tris: gpu_tris, materials: gpu_mats, nodes, has_bvh: with_bvh }
}

/// A traced scene with the CPU's share of the work already done: the
/// triangles packed into the tracer's buffer layouts and, for the compute
/// tier, the BVH built over them. Building one is plain CPU work with no
/// renderer in it — seconds for millions of triangles — so an app builds it
/// on a worker thread and hands it to
/// [`Stage3D::set_rt_scene_prepared`](crate::draw::scene::Stage3D::set_rt_scene_prepared)
/// on the UI thread, which only uploads it. `set_rt_scene` does both in
/// one call, on whatever thread calls it.
///
/// It is not consumed by the upload: keep it to upload again into a
/// renderer rebuilt after a reconnect.
#[derive(Clone)]
pub struct PreparedRtScene {
    pub(crate) packed: PackedScene,
    pub(crate) image: Option<RtImage>,
}

impl PreparedRtScene {
    /// Prepare `triangles` (taken, so millions of them are not copied) and
    /// `materials`, with `image` standing in the scene as
    /// `set_rt_scene_with_image` takes it. `with_bvh` builds the BVH: pass
    /// the renderer's [`Stage3D::rt_needs_bvh`](crate::draw::scene::Stage3D::rt_needs_bvh),
    /// asked on the UI thread before the work is sent off. A scene prepared
    /// without one still traces on a renderer that needs it — the upload
    /// builds it then, on the UI thread, as `set_rt_scene` would.
    pub fn new(
        triangles: Vec<RtTriangle>,
        materials: &[RtMaterial],
        image: Option<RtImage>,
        with_bvh: bool,
    ) -> Self {
        PreparedRtScene { packed: pack_scene(triangles, materials, image.map(|i| i.corners), with_bvh), image }
    }

    /// Triangles the tracer holds, an image's quad included.
    pub fn triangle_count(&self) -> usize {
        self.packed.tris.len()
    }

    /// Whether the BVH was built (`with_bvh`).
    pub fn has_bvh(&self) -> bool {
        self.packed.has_bvh
    }
}

/// A summary: the buffers are megabytes.
impl std::fmt::Debug for PreparedRtScene {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedRtScene")
            .field("triangles", &self.packed.tris.len())
            .field("materials", &self.packed.materials.len())
            .field("bvh_nodes", &self.packed.has_bvh.then_some(self.packed.nodes.len()))
            .field("image", &self.image)
            .finish()
    }
}

/// The image a frame traces, as its parameter block describes it: the
/// texture's size and the quad's corners and opacity.
pub(crate) struct ParamImage {
    pub width: u32,
    pub height: u32,
    pub corners: [[f32; 3]; 4],
    pub opacity: f32,
}

pub(super) fn v4([x, y, z]: [f32; 3]) -> [f32; 4] {
    [x, y, z, 0.0]
}

/// One dispatch's parameter block. With no image (or one not uploaded
/// yet) the quad's opacity is 0, which lets every ray through it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn rt_params(
    camera: RtCamera,
    size: (u32, u32),
    sample_index: u32,
    spp: u32,
    image: Option<ParamImage>,
    background: Option<[f32; 3]>,
    environment: &RtEnvironment,
) -> RtParams {
    let (img_origin, img_u, img_v) = match image {
        Some(ParamImage { width, height, corners: [tl, tr, _, bl], opacity }) => (
            [tl[0], tl[1], tl[2], opacity.clamp(0.0, 1.0)],
            [tr[0] - tl[0], tr[1] - tl[1], tr[2] - tl[2], width as f32],
            [bl[0] - tl[0], bl[1] - tl[1], bl[2] - tl[2], height as f32],
        ),
        None => ([0.0; 4], [1.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 1.0]),
    };
    RtParams {
        inv_mvp: camera.inv_mvp,
        width: size.0,
        height: size.1,
        sample_index,
        max_bounces: MAX_BOUNCES,
        spp,
        _pad: [0; 3],
        img_origin,
        img_u,
        img_v,
        background: match background {
            Some([r, g, b]) => [r, g, b, 1.0],
            None => [0.0; 4],
        },
        sun_dir: v4(environment.sun_unit()),
        sun_color: v4(environment.sun_color),
        sky_zenith: v4(environment.sky_zenith),
        sky_nadir: v4(environment.sky_nadir),
    }
}

/// The denoiser's blocks, one per à-trous iteration (steps 1, 2, 4), for an
/// accumulation that will hold `n_after` samples once this frame's dispatch
/// lands — the colour sigma tightens as it grows.
pub(crate) fn denoise_params(width: u32, height: u32, n_after: u32) -> [DenoiseParams; DENOISE_ITERATIONS] {
    let inv_sqrt_n = 1.0 / (n_after.max(1) as f32).sqrt();
    std::array::from_fn(|i| DenoiseParams {
        width,
        height,
        step: 1 << i,
        first: (i == 0) as u32,
        last: (i == DENOISE_ITERATIONS - 1) as u32,
        inv_sqrt_n,
        _pad: [0; 2],
    })
}
