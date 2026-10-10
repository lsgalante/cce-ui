//! The tier-1 RT engine (RT-renderer phase 2): an internal triangle+material
//! scene, a CPU-built binned-SAH BVH uploaded as storage buffers, and the
//! `rt.wgsl` compute path tracer with progressive accumulation.
//!
//! The stage renders into its own pane-sized storage image and blits it into
//! the backdrop image's viewport-pane region — exactly the slot the raster
//! `SceneStage` fills — so the swapchain copy, blur plates, and the 2D UI pass
//! are untouched. Runs on plain Vulkan compute (no `VK_KHR_ray_*`), which is
//! the point: it works on the integrated GPU; a tier-2 ray-query backend can
//! later swap out just the traversal.
//!
//! Scene schema is internal by design — importers (OBJ/glTF) belong in a
//! future loader that converts *into* [`RtTriangle`]/[`RtMaterial`].
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the tier, `RtStage` (background and environment, staging, destroy) |
//! | `init` | `RtStage::new`: the tier's pipelines, descriptor layouts and sets, and the frames in flight |
//! | `tests` | the tracer's GPU tests (`#[ignore]`d; `CCE_VK_RT=compute cargo test --lib vk::rt -- --ignored`) |
//! | `scene` | a scene's upload, its image descriptors, the acceleration structures (ray-query tier) |
//! | `frame` | the per-frame targets, uniforms and `record` |
//! | `denoise` | the à-trous denoiser |
//! | `texture` | the tracer's own textures and staged images |
//! | `offscreen` | `RtOffscreen`: the tracer without a window, for tests and headless renders |

mod denoise;
mod frame;
mod init;
mod offscreen;
mod scene;
mod texture;
#[cfg(test)]
mod tests;

pub use offscreen::RtOffscreen;
use denoise::*;
use texture::*;

use ash::vk;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme, Allocator};
use gpu_allocator::MemoryLocation;

use super::renderer::{
    compile_wgsl, compile_wgsl_ray_query, create_cpu_buffer, destroy_cpu_buffer, AllocatedBuffer,
};
pub use crate::draw::rt::{PreparedRtScene, RtCamera, RtEnvironment, RtImage, RtImagePixels, RtMaterial, RtTriangle};
use crate::draw::rt::{
    denoise_params, pack_scene, rt_params, DenoiseParams, PackedScene, ParamImage, RtParams, DENOISE_ITERATIONS, MAX_SAMPLES,
    WORKGROUP,
};


/// Where the stage's image comes from.
pub(crate) enum RtImageSource<'a> {
    /// The 2D pass's image of this id, looked up each frame: its upload may
    /// not have landed when the scene is set, and it may be replaced after.
    Shared(u32),
    /// Pixels for the stage to upload and own.
    Pixels { pixels: &'a [u8], width: u32, height: u32 },
}

/// The two trace backends. They share every shader line except
/// `intersect_scene` (rt_bvh.wgsl vs rt_query.wgsl) and binding 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RtTier {
    /// CPU-built BVH traversed in compute — runs on any device.
    Compute,
    /// Driver acceleration structures + VK_KHR_ray_query — RT cores.
    RayQuery,
}

impl RtTier {
    /// The tier a stage on a device with (`has_ray_query`) or without the
    /// ray-query stack runs: tier 2 when it can, unless `CCE_VK_RT=compute`
    /// forces the BVH tier.
    fn choose(has_ray_query: bool) -> RtTier {
        let force_compute = std::env::var("CCE_VK_RT").is_ok_and(|v| v == "compute");
        if has_ray_query && !force_compute {
            RtTier::RayQuery
        } else {
            RtTier::Compute
        }
    }
}

/// Whether a stage made on a device with (`has_ray_query`) or without the
/// ray-query stack traverses a CPU-built BVH: what `Stage3D::rt_needs_bvh`
/// answers before the stage exists.
pub(crate) fn needs_bvh(has_ray_query: bool) -> bool {
    RtTier::choose(has_ray_query) == RtTier::Compute
}

/// Tier-2 GPU objects: one BLAS over the triangle buffer, a one-instance
/// TLAS over it. Rebuilt wholesale on every scene replacement.
struct Accel {
    blas: vk::AccelerationStructureKHR,
    blas_buffer: AllocatedBuffer,
    tlas: vk::AccelerationStructureKHR,
    tlas_buffer: AllocatedBuffer,
    instances: AllocatedBuffer,
}

struct RtFrame {
    uniforms: AllocatedBuffer,
    descriptor_set: vk::DescriptorSet,
}

pub(crate) struct RtStage {
    tier: RtTier,
    accel_loader: Option<ash::khr::acceleration_structure::Device>,
    as_scratch_align: vk::DeviceSize,
    accel: Option<Accel>,

    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    shader_module: vk::ShaderModule,
    frames: Vec<RtFrame>,

    nodes: AllocatedBuffer,
    tris: AllocatedBuffer,
    materials: AllocatedBuffer,
    tri_count: u32,

    /// Bound at the image bindings while the scene has no image, or has one
    /// whose upload has not landed: a shader's bindings are never empty.
    stand_in: OwnedTexture,
    image_sampler: vk::Sampler,
    image: Option<StagedImage>,

    accum: AllocatedBuffer,
    /// Primary-hit features (2 vec4 per pixel) written by the tracer, read
    /// by the denoiser.
    features: AllocatedBuffer,
    /// À-trous ping-pong color buffers (1 vec4 per pixel each).
    ping: AllocatedBuffer,
    pong: AllocatedBuffer,
    denoiser: Option<Denoiser>,
    output_image: vk::Image,
    output_view: vk::ImageView,
    output_allocation: Option<Allocation>,
    output_size: (u32, u32),
    output_initialized: bool,

    pane: (u32, u32, u32, u32),
    pane_moved: bool,
    camera: Option<RtCamera>,
    sample_index: u32,
    /// Samples per dispatch: 1 interactive, higher for offscreen rendering.
    spp: u32,
    staged: bool,
    /// What a camera ray that meets nothing shows, linear RGB; None is the
    /// sky. See [`RtStage::set_background`].
    background: Option<[f32; 3]>,
    /// The sky and sun. See [`RtStage::set_environment`].
    environment: RtEnvironment,
}

impl RtStage {
    /// What a camera ray that meets nothing shows: a colour (linear RGB),
    /// or None for the sky. The sky stays the light either way — a bounce
    /// that leaves the scene still meets it — so this is the backdrop and
    /// not the lighting, as an app's raster background colour is. A change
    /// restarts the accumulation.
    pub(crate) fn set_background(&mut self, background: Option<[f32; 3]>) {
        if self.background != background {
            self.background = background;
            self.sample_index = 0;
        }
    }

    /// The sky and sun the scene is lit by. A change restarts the
    /// accumulation.
    pub(crate) fn set_environment(&mut self, environment: RtEnvironment) {
        if self.environment != environment {
            self.environment = environment;
            self.sample_index = 0;
        }
    }

    /// Stage an RT frame for the viewport pane (physical pixels). Recreates the
    /// pane-sized targets on size change (waits for device idle) and resets the
    /// accumulation when the camera, size, or scene changed.
    pub(crate) fn stage(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        pane: (u32, u32, u32, u32),
        camera: RtCamera,
    ) {
        let (_, _, w, h) = pane;
        if w == 0 || h == 0 {
            self.staged = false;
            return;
        }
        if (w, h) != self.output_size {
            unsafe {
                let _ = device.device_wait_idle();
            }
            self.recreate_targets(device, allocator, w, h);
            self.sample_index = 0;
        }
        if pane != self.pane && self.pane != (0, 0, 0, 0) {
            // Pane moved or shrank: stale RT pixels sit outside the new
            // region; clear the backdrop once before the next blit.
            self.pane_moved = true;
        }
        if self.camera != Some(camera) {
            self.camera = Some(camera);
            self.sample_index = 0;
        }
        self.pane = pane;
        self.staged = true;
    }

    /// True while another dispatch would still refine the image.
    pub(crate) fn accumulating(&self) -> bool {
        self.tri_count > 0 && self.sample_index < MAX_SAMPLES
    }

    pub(crate) fn destroy(&mut self, device: &ash::Device, allocator: &mut Allocator) {
        self.destroy_targets(device, allocator);
        self.destroy_accel(device, allocator);
        if let Some(mut denoiser) = self.denoiser.take() {
            denoiser.destroy(device, allocator);
        }
        for buf in [&mut self.nodes, &mut self.tris, &mut self.materials] {
            destroy_cpu_buffer(device, allocator, buf);
        }
        if let Some(mut owned) = self.image.take().and_then(|i| i.owned) {
            owned.destroy(device, allocator);
        }
        self.stand_in.destroy(device, allocator);
        unsafe {
            device.destroy_sampler(self.image_sampler, None);
            for frame in &mut self.frames {
                let mut uniforms = std::mem::replace(&mut frame.uniforms, AllocatedBuffer::null());
                destroy_cpu_buffer(device, allocator, &mut uniforms);
            }
            device.destroy_descriptor_pool(self.descriptor_pool, None);
            device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.pipeline_layout, None);
            device.destroy_shader_module(self.shader_module, None);
        }
    }
}
