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

// --- GPU layouts (must match rt.wgsl) ---

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

// --- BVH construction (binned SAH) ---

const BVH_BINS: usize = 8;
const BVH_LEAF_MAX: u32 = 4;

#[derive(Clone, Copy)]
struct Aabb {
    min: [f32; 3],
    max: [f32; 3],
}

impl Aabb {
    const EMPTY: Aabb = Aabb { min: [f32::INFINITY; 3], max: [f32::NEG_INFINITY; 3] };

    fn grow(&mut self, p: [f32; 3]) {
        for a in 0..3 {
            self.min[a] = self.min[a].min(p[a]);
            self.max[a] = self.max[a].max(p[a]);
        }
    }

    fn grow_aabb(&mut self, other: &Aabb) {
        self.grow(other.min);
        self.grow(other.max);
    }

    fn half_area(&self) -> f32 {
        let dx = (self.max[0] - self.min[0]).max(0.0);
        let dy = (self.max[1] - self.min[1]).max(0.0);
        let dz = (self.max[2] - self.min[2]).max(0.0);
        dx * dy + dy * dz + dz * dx
    }
}

fn tri_aabb(t: &RtTriangle) -> Aabb {
    let mut b = Aabb::EMPTY;
    b.grow(t.p0);
    b.grow(t.p1);
    b.grow(t.p2);
    b
}

fn tri_centroid(t: &RtTriangle) -> [f32; 3] {
    let mut c = [0.0f32; 3];
    for a in 0..3 {
        c[a] = (t.p0[a] + t.p1[a] + t.p2[a]) / 3.0;
    }
    c
}

/// Build a BVH over `triangles`, reordering them so leaves reference
/// contiguous ranges. Returns the flat node array (empty input → empty vec).
pub(crate) fn build_bvh(triangles: &mut Vec<RtTriangle>) -> Vec<GpuBvhNode> {
    if triangles.is_empty() {
        return Vec::new();
    }
    let bounds: Vec<Aabb> = triangles.iter().map(tri_aabb).collect();
    let centroids: Vec<[f32; 3]> = triangles.iter().map(tri_centroid).collect();
    let mut order: Vec<u32> = (0..triangles.len() as u32).collect();

    fn range_bounds(order: &[u32], bounds: &[Aabb]) -> Aabb {
        let mut b = Aabb::EMPTY;
        for &i in order {
            b.grow_aabb(&bounds[i as usize]);
        }
        b
    }

    let mut nodes: Vec<GpuBvhNode> = Vec::with_capacity(triangles.len() * 2);
    let root_bounds = range_bounds(&order, &bounds);
    nodes.push(GpuBvhNode {
        min: root_bounds.min,
        left_first: 0,
        max: root_bounds.max,
        count: triangles.len() as u32,
    });

    // (node index, start, count) work list over `order`.
    let mut work = vec![(0usize, 0usize, triangles.len())];
    while let Some((node_idx, start, count)) = work.pop() {
        if (count as u32) <= BVH_LEAF_MAX {
            continue; // stays a leaf
        }
        let slice = &mut order[start..start + count];

        // Centroid bounds pick the split axis.
        let mut cb = Aabb::EMPTY;
        for &i in slice.iter() {
            cb.grow(centroids[i as usize]);
        }
        let mut axis = 0;
        let mut extent = 0.0f32;
        for a in 0..3 {
            let e = cb.max[a] - cb.min[a];
            if e > extent {
                extent = e;
                axis = a;
            }
        }

        let mut split_at = None;
        if extent > 1e-12 {
            // Binned SAH along `axis`.
            let scale = BVH_BINS as f32 / extent;
            let bin_of = |i: u32| -> usize {
                (((centroids[i as usize][axis] - cb.min[axis]) * scale) as usize)
                    .min(BVH_BINS - 1)
            };
            let mut bin_bounds = [Aabb::EMPTY; BVH_BINS];
            let mut bin_counts = [0usize; BVH_BINS];
            for &i in slice.iter() {
                let b = bin_of(i);
                bin_counts[b] += 1;
                bin_bounds[b].grow_aabb(&bounds[i as usize]);
            }
            // Cost of each of the BINS-1 split planes.
            let mut best_cost = f32::INFINITY;
            let mut best_plane = 0usize;
            for plane in 1..BVH_BINS {
                let (mut lb, mut rb) = (Aabb::EMPTY, Aabb::EMPTY);
                let (mut lc, mut rc) = (0usize, 0usize);
                for b in 0..plane {
                    lb.grow_aabb(&bin_bounds[b]);
                    lc += bin_counts[b];
                }
                for b in plane..BVH_BINS {
                    rb.grow_aabb(&bin_bounds[b]);
                    rc += bin_counts[b];
                }
                if lc == 0 || rc == 0 {
                    continue;
                }
                let cost = lb.half_area() * lc as f32 + rb.half_area() * rc as f32;
                if cost < best_cost {
                    best_cost = cost;
                    best_plane = plane;
                }
            }
            if best_plane > 0 {
                let mut mid = 0usize;
                for k in 0..count {
                    if bin_of(slice[k]) < best_plane {
                        slice.swap(k, mid);
                        mid += 1;
                    }
                }
                if mid > 0 && mid < count {
                    split_at = Some(mid);
                }
            }
        }
        // Degenerate centroids or a one-sided SAH result: median split keeps
        // the tree balanced instead of forcing a giant leaf.
        let mid = split_at.unwrap_or(count / 2);

        let left_bounds = range_bounds(&slice[..mid], &bounds);
        let right_bounds = range_bounds(&slice[mid..], &bounds);
        let left_idx = nodes.len();
        nodes.push(GpuBvhNode {
            min: left_bounds.min,
            left_first: (start) as u32,
            max: left_bounds.max,
            count: mid as u32,
        });
        nodes.push(GpuBvhNode {
            min: right_bounds.min,
            left_first: (start + mid) as u32,
            max: right_bounds.max,
            count: (count - mid) as u32,
        });
        nodes[node_idx].left_first = left_idx as u32;
        nodes[node_idx].count = 0;
        work.push((left_idx, start, mid));
        work.push((left_idx + 1, start + mid, count - mid));
    }

    // Apply the final order to the triangle array so leaf ranges are direct.
    let reordered: Vec<RtTriangle> =
        order.iter().map(|&i| triangles[i as usize]).collect();
    *triangles = reordered;
    nodes
}

// --- The Vulkan stage ---

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

fn gpu_triangle(t: &RtTriangle) -> GpuTriangle {
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

fn v4([x, y, z]: [f32; 3]) -> [f32; 4] {
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

#[cfg(test)]
mod tests {
    use super::*;

    // CPU mirror of the shader's traversal, for parity testing.
    fn intersect_tri_cpu(ro: [f32; 3], rd: [f32; 3], t: &RtTriangle, t_limit: f32) -> f32 {
        let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        let cross = |a: [f32; 3], b: [f32; 3]| {
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        };
        let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let e1 = sub(t.p1, t.p0);
        let e2 = sub(t.p2, t.p0);
        let h = cross(rd, e2);
        let a = dot(e1, h);
        if a.abs() < 1e-8 {
            return 1e30;
        }
        let f = 1.0 / a;
        let s = sub(ro, t.p0);
        let u = f * dot(s, h);
        if !(0.0..=1.0).contains(&u) {
            return 1e30;
        }
        let q = cross(s, e1);
        let v = f * dot(rd, q);
        if v < 0.0 || u + v > 1.0 {
            return 1e30;
        }
        let tt = f * dot(e2, q);
        if tt > 1e-4 && tt < t_limit {
            return tt;
        }
        1e30
    }

    fn traverse_bvh_cpu(
        nodes: &[GpuBvhNode],
        tris: &[RtTriangle],
        ro: [f32; 3],
        rd: [f32; 3],
    ) -> (f32, Option<usize>) {
        if nodes.is_empty() {
            return (1e30, None);
        }
        let inv = [1.0 / rd[0], 1.0 / rd[1], 1.0 / rd[2]];
        let hit_aabb = |min: [f32; 3], max: [f32; 3], t_limit: f32| -> bool {
            let mut tn = f32::NEG_INFINITY;
            let mut tf = f32::INFINITY;
            for a in 0..3 {
                let t1 = (min[a] - ro[a]) * inv[a];
                let t2 = (max[a] - ro[a]) * inv[a];
                tn = tn.max(t1.min(t2));
                tf = tf.min(t1.max(t2));
            }
            tf >= tn.max(0.0) && tn < t_limit
        };
        let mut best = 1e30f32;
        let mut best_tri = None;
        let mut stack = vec![0u32];
        while let Some(idx) = stack.pop() {
            let node = &nodes[idx as usize];
            if !hit_aabb(node.min, node.max, best) {
                continue;
            }
            if node.count > 0 {
                for i in node.left_first..node.left_first + node.count {
                    let t = intersect_tri_cpu(ro, rd, &tris[i as usize], best);
                    if t < best {
                        best = t;
                        best_tri = Some(i as usize);
                    }
                }
            } else {
                stack.push(node.left_first);
                stack.push(node.left_first + 1);
            }
        }
        (best, best_tri)
    }

    fn brute_force(tris: &[RtTriangle], ro: [f32; 3], rd: [f32; 3]) -> (f32, Option<usize>) {
        let mut best = 1e30f32;
        let mut best_tri = None;
        for (i, t) in tris.iter().enumerate() {
            let tt = intersect_tri_cpu(ro, rd, t, best);
            if tt < best {
                best = tt;
                best_tri = Some(i);
            }
        }
        (best, best_tri)
    }

    // Deterministic LCG so the test needs no rand dependency.
    struct Lcg(u64);
    impl Lcg {
        fn next_f32(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 33) as f32) / (u32::MAX >> 1) as f32
        }
        fn point(&mut self, scale: f32) -> [f32; 3] {
            [
                (self.next_f32() - 0.5) * scale,
                (self.next_f32() - 0.5) * scale,
                (self.next_f32() - 0.5) * scale,
            ]
        }
    }

    fn random_scene(n: usize, seed: u64) -> Vec<RtTriangle> {
        let mut rng = Lcg(seed);
        (0..n)
            .map(|i| {
                let c = rng.point(20.0);
                let jitter = |rng: &mut Lcg, c: [f32; 3]| {
                    let d = rng.point(2.0);
                    [c[0] + d[0], c[1] + d[1], c[2] + d[2]]
                };
                RtTriangle {
                    p0: jitter(&mut rng, c),
                    p1: jitter(&mut rng, c),
                    p2: jitter(&mut rng, c),
                    material: (i % 5) as u32,
                }
            })
            .collect()
    }

    #[test]
    fn test_bvh_matches_brute_force() {
        let mut tris = random_scene(500, 42);
        let nodes = build_bvh(&mut tris);
        assert!(!nodes.is_empty());
        let mut rng = Lcg(7);
        let mut hits = 0;
        for _ in 0..200 {
            let ro = rng.point(40.0);
            let target = rng.point(10.0);
            let d = [target[0] - ro[0], target[1] - ro[1], target[2] - ro[2]];
            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-6);
            let rd = [d[0] / len, d[1] / len, d[2] / len];
            let (t_bvh, tri_bvh) = traverse_bvh_cpu(&nodes, &tris, ro, rd);
            let (t_ref, tri_ref) = brute_force(&tris, ro, rd);
            assert_eq!(tri_bvh, tri_ref, "different triangle hit");
            assert!((t_bvh - t_ref).abs() < 1e-4, "t mismatch: {t_bvh} vs {t_ref}");
            if tri_bvh.is_some() {
                hits += 1;
            }
        }
        assert!(hits > 20, "test rays barely hit the scene ({hits}/200)");
    }

    #[test]
    fn test_bvh_leaf_ranges_cover_all_triangles() {
        let mut tris = random_scene(300, 9);
        let nodes = build_bvh(&mut tris);
        let mut seen = vec![false; tris.len()];
        for node in &nodes {
            if node.count > 0 {
                for i in node.left_first..node.left_first + node.count {
                    assert!(!seen[i as usize], "triangle {i} in two leaves");
                    seen[i as usize] = true;
                }
            }
        }
        assert!(seen.iter().all(|&s| s), "not every triangle is in a leaf");
    }

    #[test]
    fn test_bvh_degenerate_identical_centroids() {
        // All triangles share one centroid: SAH can't split, the median
        // fallback must still terminate and cover everything.
        let tri = RtTriangle {
            p0: [0.0, 0.0, 0.0],
            p1: [1.0, 0.0, 0.0],
            p2: [0.0, 1.0, 0.0],
            material: 0,
        };
        let mut tris = vec![tri; 100];
        let nodes = build_bvh(&mut tris);
        let covered: u32 = nodes.iter().filter(|n| n.count > 0).map(|n| n.count).sum();
        assert_eq!(covered, 100);
        let (t, hit) = traverse_bvh_cpu(&nodes, &tris, [0.2, 0.2, -5.0], [0.0, 0.0, 1.0]);
        assert!(hit.is_some());
        assert!((t - 5.0).abs() < 1e-3);
    }

    #[test]
    fn test_bvh_empty_and_single() {
        let mut empty: Vec<RtTriangle> = Vec::new();
        assert!(build_bvh(&mut empty).is_empty());

        let mut single = vec![RtTriangle {
            p0: [-1.0, -1.0, 0.0],
            p1: [1.0, -1.0, 0.0],
            p2: [0.0, 1.0, 0.0],
            material: 3,
        }];
        let nodes = build_bvh(&mut single);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].count, 1);
        let (t, hit) = traverse_bvh_cpu(&nodes, &single, [0.0, 0.0, -3.0], [0.0, 0.0, 1.0]);
        assert_eq!(hit, Some(0));
        assert!((t - 3.0).abs() < 1e-4);
    }

    #[test]
    fn test_prepared_scene_bvh_built_late_matches_built_early() {
        // A scene prepared for the ray-query tier and uploaded to the
        // compute one gets the BVH it would have been prepared with.
        let tris = random_scene(400, 3);
        let mats = [RtMaterial { albedo: [0.5; 3], emission: [0.0; 3] }; 5];
        let image = RtImage {
            image: 1,
            corners: [[-1.0, 1.0, 0.0], [1.0, 1.0, 0.0], [1.0, -1.0, 0.0], [-1.0, -1.0, 0.0]],
            opacity: 1.0,
        };
        let early = PreparedRtScene::new(tris.clone(), &mats, Some(image), true);
        let late = PreparedRtScene::new(tris, &mats, Some(image), false);
        assert!(early.has_bvh() && !late.has_bvh());
        assert!(late.packed.nodes.is_empty());
        assert_eq!(early.triangle_count(), 402, "the image's quad joins the scene");
        let built = late.packed.with_bvh();
        assert!(built.has_bvh);
        let bytes = |s: &PackedScene| {
            (bytemuck::cast_slice::<_, u8>(&s.tris).to_vec(), bytemuck::cast_slice::<_, u8>(&s.nodes).to_vec())
        };
        assert_eq!(bytes(&built), bytes(&early.packed));
        assert!(matches!(early.packed.with_bvh(), std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn test_prepared_scene_crosses_threads() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<PreparedRtScene>();
        let prepared = std::thread::spawn(|| PreparedRtScene::new(random_scene(50, 1), &[], None, true))
            .join()
            .unwrap();
        assert_eq!(prepared.triangle_count(), 50);
    }
}
