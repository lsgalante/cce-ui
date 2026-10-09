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
//! | `mod.rs` | the tier, `RtStage` (construction, background and environment, staging, destroy), the tests |
//! | `scene` | a scene's upload, its image descriptors, the acceleration structures (ray-query tier) |
//! | `frame` | the per-frame targets, uniforms and `record` |
//! | `denoise` | the à-trous denoiser |
//! | `texture` | the tracer's own textures and staged images |
//! | `offscreen` | `RtOffscreen`: the tracer without a window, for tests and headless renders |

mod denoise;
mod frame;
mod offscreen;
mod scene;
mod texture;

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
    /// `accel_loader` present means the device has the ray-query stack; the
    /// stage then runs tier 2 unless `CCE_VK_RT=compute` forces the BVH tier.
    pub(crate) fn new(
        device: &ash::Device,
        allocator: &mut Allocator,
        frames_in_flight: usize,
        accel_loader: Option<&ash::khr::acceleration_structure::Device>,
        as_scratch_align: vk::DeviceSize,
        min_uniform_align: vk::DeviceSize,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
    ) -> Self {
        let denoise_on = !std::env::var("CCE_VK_RT_DENOISE")
            .is_ok_and(|v| v == "off" || v == "0" || v == "false");
        let tier = RtTier::choose(accel_loader.is_some());
        log::info!(
            "RT stage: {} tier",
            match tier {
                RtTier::Compute => "compute (BVH)",
                RtTier::RayQuery => "ray-query (hardware)",
            }
        );
        let binding1_type = match tier {
            RtTier::Compute => vk::DescriptorType::STORAGE_BUFFER,
            RtTier::RayQuery => vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
        };
        unsafe {
            let bindings = [
                vk::DescriptorSetLayoutBinding::default()
                    .binding(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(1)
                    .descriptor_type(binding1_type)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(2)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(3)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(4)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(5)
                    .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(6)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(7)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(8)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE),
            ];
            let descriptor_set_layout = device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
                .expect("Failed to create RT descriptor set layout");
            let set_layouts_one = [descriptor_set_layout];
            let pipeline_layout = device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts_one),
                    None,
                )
                .expect("Failed to create RT pipeline layout");

            let spirv = match tier {
                RtTier::Compute => compile_wgsl(&crate::draw::shaders::rt_bvh_source()),
                RtTier::RayQuery => compile_wgsl_ray_query(&format!("{}\n{}", crate::draw::shaders::RT_COMMON, crate::draw::shaders::RT_QUERY)),
            };
            let shader_module = device
                .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&spirv), None)
                .expect("Failed to create RT shader module");
            let pipeline = device
                .create_compute_pipelines(
                    vk::PipelineCache::null(),
                    &[vk::ComputePipelineCreateInfo::default()
                        .stage(
                            vk::PipelineShaderStageCreateInfo::default()
                                .stage(vk::ShaderStageFlags::COMPUTE)
                                .module(shader_module)
                                .name(c"cs_main"),
                        )
                        .layout(pipeline_layout)],
                    None,
                )
                .expect("Failed to create RT compute pipeline")[0];

            let n = frames_in_flight as u32;
            let mut pool_sizes = vec![
                vk::DescriptorPoolSize::default()
                    .ty(vk::DescriptorType::UNIFORM_BUFFER)
                    .descriptor_count(n),
                vk::DescriptorPoolSize::default()
                    .ty(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(5 * n),
                vk::DescriptorPoolSize::default()
                    .ty(vk::DescriptorType::STORAGE_IMAGE)
                    .descriptor_count(n),
                vk::DescriptorPoolSize::default()
                    .ty(vk::DescriptorType::SAMPLED_IMAGE)
                    .descriptor_count(n),
                vk::DescriptorPoolSize::default()
                    .ty(vk::DescriptorType::SAMPLER)
                    .descriptor_count(n),
            ];
            if tier == RtTier::RayQuery {
                pool_sizes.push(
                    vk::DescriptorPoolSize::default()
                        .ty(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                        .descriptor_count(n),
                );
            }
            let descriptor_pool = device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .max_sets(n)
                        .pool_sizes(&pool_sizes),
                    None,
                )
                .expect("Failed to create RT descriptor pool");
            let set_layouts: Vec<vk::DescriptorSetLayout> =
                vec![descriptor_set_layout; frames_in_flight];
            let sets = device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(descriptor_pool)
                        .set_layouts(&set_layouts),
                )
                .expect("Failed to allocate RT descriptor sets");
            let frames: Vec<RtFrame> = sets
                .into_iter()
                .map(|descriptor_set| {
                    let uniforms = create_cpu_buffer(
                        device,
                        allocator,
                        std::mem::size_of::<RtParams>() as vk::DeviceSize,
                        vk::BufferUsageFlags::UNIFORM_BUFFER,
                        "rt-uniforms",
                    );
                    let buffer_infos = [vk::DescriptorBufferInfo::default()
                        .buffer(uniforms.buffer)
                        .offset(0)
                        .range(std::mem::size_of::<RtParams>() as vk::DeviceSize)];
                    device.update_descriptor_sets(
                        &[vk::WriteDescriptorSet::default()
                            .dst_set(descriptor_set)
                            .dst_binding(0)
                            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                            .buffer_info(&buffer_infos)],
                        &[],
                    );
                    RtFrame { uniforms, descriptor_set }
                })
                .collect();

            let denoiser = denoise_on
                .then(|| Denoiser::new(device, allocator, frames_in_flight, min_uniform_align));

            let stand_in =
                OwnedTexture::new(device, allocator, queue, command_pool, &[255; 4], 1, 1);
            // Linear within a level and between levels; the shader names
            // the level, since a compute shader has no derivatives to
            // choose one by.
            let image_sampler = device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::LINEAR)
                        .min_filter(vk::Filter::LINEAR)
                        .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                        .min_lod(0.0)
                        .max_lod(vk::LOD_CLAMP_NONE)
                        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                    None,
                )
                .expect("Failed to create RT image sampler");
            for frame in &frames {
                Self::write_image_descriptor(
                    device,
                    frame.descriptor_set,
                    stand_in.view,
                    image_sampler,
                );
            }

            RtStage {
                tier,
                accel_loader: accel_loader.cloned(),
                as_scratch_align,
                accel: None,
                pipeline,
                pipeline_layout,
                descriptor_set_layout,
                descriptor_pool,
                shader_module,
                frames,
                nodes: AllocatedBuffer::null(),
                tris: AllocatedBuffer::null(),
                materials: AllocatedBuffer::null(),
                tri_count: 0,
                stand_in,
                image_sampler,
                image: None,
                accum: AllocatedBuffer::null(),
                features: AllocatedBuffer::null(),
                ping: AllocatedBuffer::null(),
                pong: AllocatedBuffer::null(),
                denoiser,
                output_image: vk::Image::null(),
                output_view: vk::ImageView::null(),
                output_allocation: None,
                output_size: (0, 0),
                output_initialized: false,
                pane: (0, 0, 0, 0),
                pane_moved: false,
                camera: None,
                sample_index: 0,
                spp: 1,
                staged: false,
                background: None,
                environment: RtEnvironment::default(),
            }
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rt_shaders_compile() {
        // naga parse + validate + SPIR-V write for both tiers; panics on failure.
        let tier1 = compile_wgsl(&crate::draw::shaders::rt_bvh_source());
        assert!(!tier1.is_empty());
        let tier2 = compile_wgsl_ray_query(&format!("{}\n{}", crate::draw::shaders::RT_COMMON, crate::draw::shaders::RT_QUERY));
        assert!(!tier2.is_empty());
        let denoise = compile_wgsl(crate::draw::shaders::RT_DENOISE);
        assert!(!denoise.is_empty());
    }

    /// An image in the traced scene, end to end: a quad half red and half
    /// clear, in front of a green wall. The red half shows red, the clear
    /// half shows the wall behind it, and beside the quad is the wall too.
    /// Run with: cargo test --lib vk::rt -- --ignored
    #[test]
    #[ignore = "requires a Vulkan device"]
    fn test_offscreen_renders_an_image() {
        let mut off = RtOffscreen::new();
        // 2x1: a red texel, a clear one.
        let pixels = [255u8, 0, 0, 255, 0, 0, 0, 0];
        let wall = |p0, p1, p2| RtTriangle { p0, p1, p2, material: 0 };
        off.set_scene_with_image(
            &[
                wall([-9.0, -9.0, -1.0], [9.0, -9.0, -1.0], [9.0, 9.0, -1.0]),
                wall([-9.0, -9.0, -1.0], [9.0, 9.0, -1.0], [-9.0, 9.0, -1.0]),
            ],
            &[RtMaterial { albedo: [0.1, 0.9, 0.1], emission: [0.0; 3] }],
            Some(RtImagePixels {
                pixels: &pixels,
                width: 2,
                height: 1,
                corners: [[-1.0, 0.5, 0.0], [1.0, 0.5, 0.0], [1.0, -0.5, 0.0], [-1.0, -0.5, 0.0]],
                opacity: 1.0,
            }),
        );
        let proj = glam::Mat4::perspective_rh(0.9, 1.0, 0.1, 100.0);
        let view = glam::Mat4::look_at_rh(
            glam::Vec3::new(0.0, 0.0, 3.0),
            glam::Vec3::ZERO,
            glam::Vec3::Y,
        );
        let camera = RtCamera { inv_mvp: (proj * view).inverse().to_cols_array_2d() };
        let (w, h) = (64u32, 64u32);
        let px = off.render(camera, w, h, 64);
        let at = |x: u32, y: u32| {
            let i = ((y * w + x) * 4) as usize;
            (px[i] as i32, px[i + 1] as i32, px[i + 2] as i32)
        };
        // The quad spans x in -1..1 of a view about 2.9 wide at z = 0: the
        // pane's columns 10 to 54, and rows 21 to 43.
        // Unlit: the texel's own colour, whatever the sky is doing.
        let (r, g, b) = at(16, 32);
        assert!(r >= 250 && g <= 5 && b <= 5, "the image's red half is not its red: {r} {g} {b}");
        let (r, g, _) = at(48, 32);
        assert!(g > r + 40, "the image's clear half hides the wall: r={r} g={g}");
        let (r, g, _) = at(32, 6);
        assert!(g > r + 40, "beside the image is not the wall: r={r} g={g}");

        // Without its image the scene is the wall alone.
        off.set_scene(
            &[wall([-9.0, -9.0, -1.0], [9.0, -9.0, -1.0], [9.0, 9.0, -1.0])],
            &[RtMaterial { albedo: [0.1, 0.9, 0.1], emission: [0.0; 3] }],
        );
        let px = off.render(camera, w, h, 16);
        let i = ((32 * w + 40) * 4) as usize;
        assert!(px[i + 1] > px[i], "the image outlived its scene");
    }

    /// End-to-end GPU test — needs a Vulkan device, so ignored by default.
    /// Run with: cargo test --lib vk::rt -- --ignored
    #[test]
    #[ignore = "requires a Vulkan device"]
    fn test_offscreen_render_smoke() {
        let mut off = RtOffscreen::new();
        // A red triangle filling the view center, camera looking down -Z.
        off.set_scene(
            &[RtTriangle {
                p0: [-1.0, -1.0, 0.0],
                p1: [1.0, -1.0, 0.0],
                p2: [0.0, 1.5, 0.0],
                material: 0,
            }],
            &[RtMaterial { albedo: [0.9, 0.1, 0.1], emission: [0.0; 3] }],
        );
        let proj = glam::Mat4::perspective_rh(0.9, 1.0, 0.1, 100.0);
        let view = glam::Mat4::look_at_rh(
            glam::Vec3::new(0.0, 0.0, 3.0),
            glam::Vec3::ZERO,
            glam::Vec3::Y,
        );
        let camera = RtCamera { inv_mvp: (proj * view).inverse().to_cols_array_2d() };
        let (w, h) = (64u32, 64u32);
        let px = off.render(camera, w, h, 16);
        assert_eq!(px.len(), (w * h * 4) as usize);
        // Center pixel hits the triangle: red-dominant. Corner pixel is sky:
        // blue >= red. Alpha opaque everywhere.
        let at = |x: u32, y: u32| {
            let i = ((y * w + x) * 4) as usize;
            (px[i], px[i + 1], px[i + 2], px[i + 3])
        };
        let (cr, _cg, cb, ca) = at(w / 2, h / 2);
        assert!(ca == 255, "alpha not opaque: {ca}");
        assert!(cr > cb, "center not red-dominant: r={cr} b={cb}");
        let (sr, _sg, sb, _sa) = at(1, 1);
        assert!(sb >= sr, "corner sky not blue-ish: r={sr} b={sb}");
    }

    /// A background colour is what a camera ray that meets nothing shows —
    /// every sample's, not only the one that writes the denoiser's features
    /// (a render takes several per dispatch) — while the sky still lights
    /// what is hit; and None is the sky again.
    #[test]
    #[ignore = "requires a Vulkan device"]
    fn test_offscreen_background_is_what_a_miss_shows() {
        let mut off = RtOffscreen::new();
        off.set_scene(
            &[RtTriangle { p0: [-1.0, -1.0, 0.0], p1: [1.0, -1.0, 0.0], p2: [0.0, 1.5, 0.0], material: 0 }],
            &[RtMaterial { albedo: [0.8, 0.8, 0.8], emission: [0.0; 3] }],
        );
        let proj = glam::Mat4::perspective_rh(0.9, 1.0, 0.1, 100.0);
        let view = glam::Mat4::look_at_rh(glam::Vec3::new(0.0, 0.0, 3.0), glam::Vec3::ZERO, glam::Vec3::Y);
        let camera = RtCamera { inv_mvp: (proj * view).inverse().to_cols_array_2d() };
        let (w, h) = (32u32, 32u32);
        let render = |off: &mut RtOffscreen| {
            let px = off.render(camera, w, h, 16);
            let at = |x: u32, y: u32| {
                let i = ((y * w + x) * 4) as usize;
                [px[i], px[i + 1], px[i + 2]]
            };
            (at(1, 1), at(w / 2, h / 2))
        };
        let (sky, lit) = render(&mut off);

        off.set_background(Some([0.0, 0.0, 0.0]));
        let (corner, centre) = render(&mut off);
        assert_eq!(corner, [0, 0, 0], "the corner is the backdrop, in every sample");
        assert!(centre.iter().all(|&c| c > 60), "the sky still lights the triangle: {centre:?}");
        assert!(centre.iter().zip(lit).all(|(&a, b)| a.abs_diff(b) < 24), "and lights it as before: {centre:?} against {lit:?}");

        off.set_background(None);
        assert_eq!(render(&mut off).0, sky, "None is the sky");
    }

    /// The environment is the scene's light: a black one leaves a grey
    /// triangle black, a sun in front of it lights it and the same sun
    /// behind it does not, and the sky's colours are what a miss shows.
    #[test]
    #[ignore = "requires a Vulkan device"]
    fn test_offscreen_environment_lights_the_scene() {
        let mut off = RtOffscreen::new();
        off.set_scene(
            &[RtTriangle { p0: [-1.0, -1.0, 0.0], p1: [1.0, -1.0, 0.0], p2: [0.0, 1.5, 0.0], material: 0 }],
            &[RtMaterial { albedo: [0.8, 0.8, 0.8], emission: [0.0; 3] }],
        );
        let proj = glam::Mat4::perspective_rh(0.9, 1.0, 0.1, 100.0);
        let view = glam::Mat4::look_at_rh(glam::Vec3::new(0.0, 0.0, 3.0), glam::Vec3::ZERO, glam::Vec3::Y);
        let camera = RtCamera { inv_mvp: (proj * view).inverse().to_cols_array_2d() };
        let (w, h) = (32u32, 32u32);
        let mut render = |env: RtEnvironment| {
            off.set_environment(env);
            let px = off.render(camera, w, h, 32);
            let at = |x: u32, y: u32| {
                let i = ((y * w + x) * 4) as usize;
                [px[i], px[i + 1], px[i + 2]]
            };
            (at(1, 1), at(w / 2, h / 2))
        };
        let dark = RtEnvironment { sun_direction: [0.0, 0.0, 1.0], sun_color: [0.0; 3], sky_zenith: [0.0; 3], sky_nadir: [0.0; 3] };
        assert_eq!(render(dark), ([0, 0, 0], [0, 0, 0]), "no light, nothing seen");
        let front = render(RtEnvironment { sun_color: [40.0; 3], ..dark }).1;
        let behind = render(RtEnvironment { sun_direction: [0.0, 0.0, -1.0], sun_color: [40.0; 3], ..dark }).1;
        assert!(front[0] > 100, "a sun in front lights the face: {front:?}");
        assert!(behind[0] < front[0] / 4, "a sun behind it does not: {behind:?} against {front:?}");
        let (corner, _) = render(RtEnvironment { sky_zenith: [0.0, 1.0, 0.0], sky_nadir: [0.0, 1.0, 0.0], ..dark });
        assert!(corner[1] > 200 && corner[0] < 10, "a miss shows the sky's colour: {corner:?}");
    }
}
