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

/// A texture the stage made and owns: the 1x1 stand-in bound while the
/// scene has no image, and the headless tracer's image.
struct OwnedTexture {
    image: vk::Image,
    view: vk::ImageView,
    allocation: Option<Allocation>,
    size: (u32, u32),
}

impl OwnedTexture {
    /// Upload sRGB RGBA8 pixels as a one-level texture, with a blocking
    /// one-time submit. One level: the headless tracer renders a still, and
    /// its samples average what a mip chain would have.
    fn new(
        device: &ash::Device,
        allocator: &mut Allocator,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        pixels: &[u8],
        width: u32,
        height: u32,
    ) -> Self {
        assert_eq!(pixels.len(), (width * height * 4) as usize, "8888 size mismatch");
        let range = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .level_count(1)
            .layer_count(1);
        unsafe {
            let image = device
                .create_image(
                    &vk::ImageCreateInfo::default()
                        .image_type(vk::ImageType::TYPE_2D)
                        .format(vk::Format::R8G8B8A8_SRGB)
                        .extent(vk::Extent3D { width, height, depth: 1 })
                        .mip_levels(1)
                        .array_layers(1)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .tiling(vk::ImageTiling::OPTIMAL)
                        .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST)
                        .initial_layout(vk::ImageLayout::UNDEFINED),
                    None,
                )
                .expect("Failed to create RT texture");
            let requirements = device.get_image_memory_requirements(image);
            let allocation = allocator
                .allocate(&AllocationCreateDesc {
                    name: "rt-texture",
                    requirements,
                    location: MemoryLocation::GpuOnly,
                    linear: false,
                    allocation_scheme: AllocationScheme::GpuAllocatorManaged,
                })
                .expect("Failed to allocate RT texture memory");
            device
                .bind_image_memory(image, allocation.memory(), allocation.offset())
                .expect("Failed to bind RT texture memory");
            let mut staging = create_cpu_buffer(
                device,
                allocator,
                pixels.len() as vk::DeviceSize,
                vk::BufferUsageFlags::TRANSFER_SRC,
                "rt-texture-staging",
            );
            staging.allocation.as_mut().unwrap().mapped_slice_mut().unwrap()[..pixels.len()]
                .copy_from_slice(pixels);

            let cmd = device
                .allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(command_pool)
                        .level(vk::CommandBufferLevel::PRIMARY)
                        .command_buffer_count(1),
                )
                .expect("Failed to allocate RT texture command buffer")[0];
            device
                .begin_command_buffer(
                    cmd,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .unwrap();
            let barrier = |from_access, to_access, from_layout, to_layout, from_stage, to_stage| {
                device.cmd_pipeline_barrier(
                    cmd,
                    from_stage,
                    to_stage,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[vk::ImageMemoryBarrier::default()
                        .src_access_mask(from_access)
                        .dst_access_mask(to_access)
                        .old_layout(from_layout)
                        .new_layout(to_layout)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .image(image)
                        .subresource_range(range)],
                );
            };
            barrier(
                vk::AccessFlags::empty(),
                vk::AccessFlags::TRANSFER_WRITE,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
            );
            device.cmd_copy_buffer_to_image(
                cmd,
                staging.buffer,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[vk::BufferImageCopy::default()
                    .buffer_row_length(width)
                    .buffer_image_height(height)
                    .image_subresource(
                        vk::ImageSubresourceLayers::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .layer_count(1),
                    )
                    .image_extent(vk::Extent3D { width, height, depth: 1 })],
            );
            barrier(
                vk::AccessFlags::TRANSFER_WRITE,
                vk::AccessFlags::SHADER_READ,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
            );
            device.end_command_buffer(cmd).unwrap();
            let cmds = [cmd];
            device
                .queue_submit(
                    queue,
                    &[vk::SubmitInfo::default().command_buffers(&cmds)],
                    vk::Fence::null(),
                )
                .expect("RT texture upload submit failed");
            device.queue_wait_idle(queue).expect("RT texture upload wait failed");
            device.free_command_buffers(command_pool, &cmds);
            destroy_cpu_buffer(device, allocator, &mut staging);

            let view = device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(vk::Format::R8G8B8A8_SRGB)
                        .subresource_range(range),
                    None,
                )
                .expect("Failed to create RT texture view");
            OwnedTexture { image, view, allocation: Some(allocation), size: (width, height) }
        }
    }

    /// Caller must have the device idle.
    fn destroy(&mut self, device: &ash::Device, allocator: &mut Allocator) {
        unsafe {
            device.destroy_image_view(self.view, None);
            device.destroy_image(self.image, None);
        }
        if let Some(a) = self.allocation.take() {
            let _ = allocator.free(a);
        }
    }
}

/// The scene's image as the stage holds it.
struct StagedImage {
    /// The 2D pass's image of this id, or None for one the stage owns.
    shared: Option<u32>,
    owned: Option<OwnedTexture>,
    corners: [[f32; 3]; 4],
    opacity: f32,
}

struct DenoiserFrame {
    /// DENOISE_ITERATIONS dynamic-offset slices of [`DenoiseParams`].
    uniforms: AllocatedBuffer,
    /// src = ping, dst = pong.
    set_a: vk::DescriptorSet,
    /// src = pong, dst = ping.
    set_b: vk::DescriptorSet,
}

/// The à-trous denoise pipeline (rt_denoise.wgsl). Owned by [`RtStage`];
/// its buffers (features/ping/pong) live on the stage with the other
/// pane-sized targets.
struct Denoiser {
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    shader_module: vk::ShaderModule,
    uniform_stride: vk::DeviceSize,
    frames: Vec<DenoiserFrame>,
}

impl Denoiser {
    fn new(
        device: &ash::Device,
        allocator: &mut Allocator,
        frames_in_flight: usize,
        min_uniform_align: vk::DeviceSize,
    ) -> Self {
        unsafe {
            let bindings = [
                vk::DescriptorSetLayoutBinding::default()
                    .binding(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
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
            ];
            let descriptor_set_layout = device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
                .expect("Failed to create denoise descriptor set layout");
            let set_layouts_one = [descriptor_set_layout];
            let pipeline_layout = device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts_one),
                    None,
                )
                .expect("Failed to create denoise pipeline layout");
            let spirv = compile_wgsl(crate::draw::shaders::RT_DENOISE);
            let shader_module = device
                .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&spirv), None)
                .expect("Failed to create denoise shader module");
            let pipeline = device
                .create_compute_pipelines(
                    vk::PipelineCache::null(),
                    &[vk::ComputePipelineCreateInfo::default()
                        .stage(
                            vk::PipelineShaderStageCreateInfo::default()
                                .stage(vk::ShaderStageFlags::COMPUTE)
                                .module(shader_module)
                                .name(c"cs_denoise"),
                        )
                        .layout(pipeline_layout)],
                    None,
                )
                .expect("Failed to create denoise pipeline")[0];

            let n = frames_in_flight as u32;
            let pool_sizes = [
                vk::DescriptorPoolSize::default()
                    .ty(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
                    .descriptor_count(2 * n),
                vk::DescriptorPoolSize::default()
                    .ty(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(8 * n),
                vk::DescriptorPoolSize::default()
                    .ty(vk::DescriptorType::STORAGE_IMAGE)
                    .descriptor_count(2 * n),
            ];
            let descriptor_pool = device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .max_sets(2 * n)
                        .pool_sizes(&pool_sizes),
                    None,
                )
                .expect("Failed to create denoise descriptor pool");
            let set_layouts: Vec<vk::DescriptorSetLayout> =
                vec![descriptor_set_layout; frames_in_flight * 2];
            let sets = device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(descriptor_pool)
                        .set_layouts(&set_layouts),
                )
                .expect("Failed to allocate denoise descriptor sets");

            let uniform_stride = (std::mem::size_of::<DenoiseParams>() as vk::DeviceSize)
                .next_multiple_of(min_uniform_align.max(1));
            let frames: Vec<DenoiserFrame> = (0..frames_in_flight)
                .map(|i| {
                    let uniforms = create_cpu_buffer(
                        device,
                        allocator,
                        uniform_stride * DENOISE_ITERATIONS as vk::DeviceSize,
                        vk::BufferUsageFlags::UNIFORM_BUFFER,
                        "rt-denoise-uniforms",
                    );
                    let (set_a, set_b) = (sets[2 * i], sets[2 * i + 1]);
                    for set in [set_a, set_b] {
                        let infos = [vk::DescriptorBufferInfo::default()
                            .buffer(uniforms.buffer)
                            .range(std::mem::size_of::<DenoiseParams>() as vk::DeviceSize)];
                        device.update_descriptor_sets(
                            &[vk::WriteDescriptorSet::default()
                                .dst_set(set)
                                .dst_binding(0)
                                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
                                .buffer_info(&infos)],
                            &[],
                        );
                    }
                    DenoiserFrame { uniforms, set_a, set_b }
                })
                .collect();

            Denoiser {
                pipeline,
                pipeline_layout,
                descriptor_set_layout,
                descriptor_pool,
                shader_module,
                uniform_stride,
                frames,
            }
        }
    }

    /// Re-point the per-target bindings after the pane-sized buffers are
    /// (re)created. Device is idle (target recreation contract).
    fn write_target_descriptors(
        &self,
        device: &ash::Device,
        accum: vk::Buffer,
        features: vk::Buffer,
        ping: vk::Buffer,
        pong: vk::Buffer,
        output_view: vk::ImageView,
    ) {
        for frame in &self.frames {
            for (set, src, dst) in
                [(frame.set_a, ping, pong), (frame.set_b, pong, ping)]
            {
                let buf_infos = [
                    vk::DescriptorBufferInfo::default().buffer(accum).range(vk::WHOLE_SIZE),
                    vk::DescriptorBufferInfo::default().buffer(features).range(vk::WHOLE_SIZE),
                    vk::DescriptorBufferInfo::default().buffer(src).range(vk::WHOLE_SIZE),
                    vk::DescriptorBufferInfo::default().buffer(dst).range(vk::WHOLE_SIZE),
                ];
                let image_infos = [vk::DescriptorImageInfo::default()
                    .image_view(output_view)
                    .image_layout(vk::ImageLayout::GENERAL)];
                let writes: Vec<vk::WriteDescriptorSet> = buf_infos
                    .iter()
                    .enumerate()
                    .map(|(i, info)| {
                        vk::WriteDescriptorSet::default()
                            .dst_set(set)
                            .dst_binding(1 + i as u32)
                            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                            .buffer_info(std::slice::from_ref(info))
                    })
                    .chain(std::iter::once(
                        vk::WriteDescriptorSet::default()
                            .dst_set(set)
                            .dst_binding(5)
                            .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                            .image_info(&image_infos),
                    ))
                    .collect();
                unsafe { device.update_descriptor_sets(&writes, &[]) };
            }
        }
    }

    /// After the frame fence: the per-iteration params. `n_after` is the
    /// sample count the accumulation will hold once this frame's dispatch
    /// lands — the color sigma tightens as it grows.
    fn write_frame_uniforms(&mut self, frame_index: usize, width: u32, height: u32, n_after: u32) {
        let frame = &mut self.frames[frame_index];
        let mapped = frame.uniforms.allocation.as_mut().unwrap().mapped_slice_mut().unwrap();
        for (i, params) in denoise_params(width, height, n_after).iter().enumerate() {
            let offset = self.uniform_stride as usize * i;
            mapped[offset..offset + std::mem::size_of::<DenoiseParams>()]
                .copy_from_slice(bytemuck::bytes_of(params));
        }
    }

    /// Record the à-trous iterations. The tracer's dispatch has already run
    /// in this command buffer; the last iteration rewrites `out_img` (still
    /// in GENERAL). Iteration parity: 0 → set_b (writes ping), 1 → set_a,
    /// 2 → set_b.
    fn record(&self, device: &ash::Device, cmd: vk::CommandBuffer, frame_index: usize, w: u32, h: u32) {
        let frame = &self.frames[frame_index];
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
            for i in 0..DENOISE_ITERATIONS {
                // Order this iteration's reads after the previous compute
                // writes (tracer or prior iteration).
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::DependencyFlags::empty(),
                    &[vk::MemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                        .dst_access_mask(
                            vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
                        )],
                    &[],
                    &[],
                );
                let set = if i % 2 == 0 { frame.set_b } else { frame.set_a };
                device.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::COMPUTE,
                    self.pipeline_layout,
                    0,
                    &[set],
                    &[(self.uniform_stride as u32) * i as u32],
                );
                device.cmd_dispatch(cmd, w.div_ceil(WORKGROUP), h.div_ceil(WORKGROUP), 1);
            }
        }
    }

    fn destroy(&mut self, device: &ash::Device, allocator: &mut Allocator) {
        unsafe {
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

    fn write_image_descriptor(
        device: &ash::Device,
        set: vk::DescriptorSet,
        view: vk::ImageView,
        sampler: vk::Sampler,
    ) {
        let image_infos = [vk::DescriptorImageInfo::default()
            .image_view(view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let sampler_infos = [vk::DescriptorImageInfo::default().sampler(sampler)];
        unsafe {
            device.update_descriptor_sets(
                &[
                    vk::WriteDescriptorSet::default()
                        .dst_set(set)
                        .dst_binding(7)
                        .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                        .image_info(&image_infos),
                    vk::WriteDescriptorSet::default()
                        .dst_set(set)
                        .dst_binding(8)
                        .descriptor_type(vk::DescriptorType::SAMPLER)
                        .image_info(&sampler_infos),
                ],
                &[],
            );
        }
    }

    /// Whether this stage traverses a CPU-built BVH (tier 1) rather than
    /// building driver acceleration structures (tier 2).
    pub(crate) fn needs_bvh(&self) -> bool {
        self.tier == RtTier::Compute
    }

    /// Replace the scene with one already packed (`draw::rt::pack_scene`,
    /// whose image quad must be the one `image` describes). Tier 1 uploads
    /// its BVH, building it here only if it was packed without one; tier 2
    /// builds driver acceleration structures on the given queue instead,
    /// ignoring any BVH. Caller must have the device idle.
    pub(crate) fn set_scene(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        scene: &PackedScene,
        image: Option<(RtImageSource, [[f32; 3]; 4], f32)>,
    ) {
        if let Some(mut old) = self.image.take().and_then(|i| i.owned) {
            old.destroy(device, allocator);
        }
        self.image = image.map(|(source, corners, opacity)| match source {
            RtImageSource::Shared(id) => {
                StagedImage { shared: Some(id), owned: None, corners, opacity }
            }
            RtImageSource::Pixels { pixels, width, height } => StagedImage {
                shared: None,
                owned: Some(OwnedTexture::new(
                    device,
                    allocator,
                    queue,
                    command_pool,
                    pixels,
                    width,
                    height,
                )),
                corners,
                opacity,
            },
        });
        let scene = match self.tier {
            RtTier::Compute => scene.with_bvh(),
            RtTier::RayQuery => std::borrow::Cow::Borrowed(scene),
        };
        let (gpu_tris, gpu_mats, nodes) = (&scene.tris, &scene.materials, &scene.nodes);

        self.destroy_accel(device, allocator);
        for buf in [&mut self.nodes, &mut self.tris, &mut self.materials] {
            destroy_cpu_buffer(device, allocator, buf);
        }
        let upload = |allocator: &mut Allocator,
                      bytes: &[u8],
                      usage: vk::BufferUsageFlags,
                      name: &str|
         -> AllocatedBuffer {
            let mut buf = create_cpu_buffer(
                device,
                allocator,
                (bytes.len() as vk::DeviceSize).max(64),
                usage,
                name,
            );
            if !bytes.is_empty() {
                buf.allocation.as_mut().unwrap().mapped_slice_mut().unwrap()[..bytes.len()]
                    .copy_from_slice(bytes);
            }
            buf
        };
        // Tier 2 reads the same triangle buffer as BLAS build input (the
        // shading data still comes through the storage binding).
        let tri_usage = match self.tier {
            RtTier::Compute => vk::BufferUsageFlags::STORAGE_BUFFER,
            RtTier::RayQuery => {
                vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                    | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
            }
        };
        self.tris = upload(allocator, bytemuck::cast_slice(gpu_tris), tri_usage, "rt-tris");
        self.materials = upload(
            allocator,
            bytemuck::cast_slice(gpu_mats),
            vk::BufferUsageFlags::STORAGE_BUFFER,
            "rt-materials",
        );
        self.tri_count = gpu_tris.len() as u32;
        self.sample_index = 0;

        // Binding 1 (per tier), then the shared 2/3.
        match self.tier {
            RtTier::Compute => {
                self.nodes = upload(
                    allocator,
                    bytemuck::cast_slice(nodes),
                    vk::BufferUsageFlags::STORAGE_BUFFER,
                    "rt-nodes",
                );
                for frame in &self.frames {
                    let infos = [vk::DescriptorBufferInfo::default()
                        .buffer(self.nodes.buffer)
                        .range(vk::WHOLE_SIZE)];
                    unsafe {
                        device.update_descriptor_sets(
                            &[vk::WriteDescriptorSet::default()
                                .dst_set(frame.descriptor_set)
                                .dst_binding(1)
                                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                                .buffer_info(&infos)],
                            &[],
                        );
                    }
                }
            }
            RtTier::RayQuery => {
                if self.tri_count > 0 {
                    self.build_accel(device, allocator, queue, command_pool);
                    let accel = self.accel.as_ref().unwrap();
                    let handles = [accel.tlas];
                    for frame in &self.frames {
                        let mut as_info =
                            vk::WriteDescriptorSetAccelerationStructureKHR::default()
                                .acceleration_structures(&handles);
                        let mut write = vk::WriteDescriptorSet::default()
                            .dst_set(frame.descriptor_set)
                            .dst_binding(1)
                            .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                            .push_next(&mut as_info);
                        write.descriptor_count = 1;
                        unsafe { device.update_descriptor_sets(&[write], &[]) };
                    }
                }
            }
        }

        for frame in &self.frames {
            let infos = [
                vk::DescriptorBufferInfo::default().buffer(self.tris.buffer).range(vk::WHOLE_SIZE),
                vk::DescriptorBufferInfo::default()
                    .buffer(self.materials.buffer)
                    .range(vk::WHOLE_SIZE),
            ];
            let writes: Vec<vk::WriteDescriptorSet> = infos
                .iter()
                .enumerate()
                .map(|(i, info)| {
                    vk::WriteDescriptorSet::default()
                        .dst_set(frame.descriptor_set)
                        .dst_binding(2 + i as u32)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(std::slice::from_ref(info))
                })
                .collect();
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }
    }

    /// Build the BLAS (over `self.tris`, opaque triangles) and a one-instance
    /// TLAS, on the given queue with a blocking one-time submit. Device is
    /// idle (set_scene contract), so replacing old structures is safe.
    fn build_accel(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
    ) {
        let loader = self.accel_loader.clone().expect("tier 2 without accel loader");
        let create_as_buffer = |allocator: &mut Allocator,
                                size: vk::DeviceSize,
                                usage: vk::BufferUsageFlags,
                                name: &str|
         -> AllocatedBuffer {
            unsafe {
                let buffer = device
                    .create_buffer(
                        &vk::BufferCreateInfo::default()
                            .size(size)
                            .usage(usage | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS)
                            .sharing_mode(vk::SharingMode::EXCLUSIVE),
                        None,
                    )
                    .expect("Failed to create AS buffer");
                let requirements = device.get_buffer_memory_requirements(buffer);
                let allocation = allocator
                    .allocate(&AllocationCreateDesc {
                        name,
                        requirements,
                        location: MemoryLocation::GpuOnly,
                        linear: true,
                        allocation_scheme: AllocationScheme::GpuAllocatorManaged,
                    })
                    .expect("Failed to allocate AS memory");
                device
                    .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
                    .expect("Failed to bind AS memory");
                AllocatedBuffer { buffer, allocation: Some(allocation), size }
            }
        };
        let addr_of = |buffer: vk::Buffer| unsafe {
            device.get_buffer_device_address(&vk::BufferDeviceAddressInfo::default().buffer(buffer))
        };

        unsafe {
            // --- BLAS over the triangle buffer (stride 16: p0/p1/p2 vec4s).
            let tri_addr = addr_of(self.tris.buffer);
            let blas_geometry = vk::AccelerationStructureGeometryKHR::default()
                .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
                .flags(vk::GeometryFlagsKHR::OPAQUE)
                .geometry(vk::AccelerationStructureGeometryDataKHR {
                    triangles: vk::AccelerationStructureGeometryTrianglesDataKHR::default()
                        .vertex_format(vk::Format::R32G32B32_SFLOAT)
                        .vertex_data(vk::DeviceOrHostAddressConstKHR { device_address: tri_addr })
                        .vertex_stride(16)
                        .max_vertex(self.tri_count * 3 - 1)
                        .index_type(vk::IndexType::NONE_KHR),
                });
            let blas_geometries = [blas_geometry];
            let mut blas_build = vk::AccelerationStructureBuildGeometryInfoKHR::default()
                .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
                .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
                .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
                .geometries(&blas_geometries);
            let blas_sizes = {
                let mut sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
                loader.get_acceleration_structure_build_sizes(
                    vk::AccelerationStructureBuildTypeKHR::DEVICE,
                    &blas_build,
                    &[self.tri_count],
                    &mut sizes,
                );
                sizes
            };
            let blas_buffer = create_as_buffer(
                allocator,
                blas_sizes.acceleration_structure_size,
                vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR,
                "rt-blas",
            );
            let blas = loader
                .create_acceleration_structure(
                    &vk::AccelerationStructureCreateInfoKHR::default()
                        .buffer(blas_buffer.buffer)
                        .size(blas_sizes.acceleration_structure_size)
                        .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL),
                    None,
                )
                .expect("Failed to create BLAS");

            // --- One-instance TLAS.
            let blas_addr = loader.get_acceleration_structure_device_address(
                &vk::AccelerationStructureDeviceAddressInfoKHR::default()
                    .acceleration_structure(blas),
            );
            let instance = vk::AccelerationStructureInstanceKHR {
                transform: vk::TransformMatrixKHR {
                    matrix: [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0],
                },
                instance_custom_index_and_mask: vk::Packed24_8::new(0, 0xff),
                instance_shader_binding_table_record_offset_and_flags: vk::Packed24_8::new(0, 0),
                acceleration_structure_reference: vk::AccelerationStructureReferenceKHR {
                    device_handle: blas_addr,
                },
            };
            let instance_bytes = std::slice::from_raw_parts(
                (&instance as *const vk::AccelerationStructureInstanceKHR).cast::<u8>(),
                std::mem::size_of::<vk::AccelerationStructureInstanceKHR>(),
            );
            let mut instances = create_cpu_buffer(
                device,
                allocator,
                instance_bytes.len() as vk::DeviceSize,
                vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                    | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
                "rt-tlas-instances",
            );
            instances.allocation.as_mut().unwrap().mapped_slice_mut().unwrap()
                [..instance_bytes.len()]
                .copy_from_slice(instance_bytes);

            let tlas_geometry = vk::AccelerationStructureGeometryKHR::default()
                .geometry_type(vk::GeometryTypeKHR::INSTANCES)
                .geometry(vk::AccelerationStructureGeometryDataKHR {
                    instances: vk::AccelerationStructureGeometryInstancesDataKHR::default()
                        .array_of_pointers(false)
                        .data(vk::DeviceOrHostAddressConstKHR {
                            device_address: addr_of(instances.buffer),
                        }),
                });
            let tlas_geometries = [tlas_geometry];
            let mut tlas_build = vk::AccelerationStructureBuildGeometryInfoKHR::default()
                .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL)
                .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
                .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
                .geometries(&tlas_geometries);
            let tlas_sizes = {
                let mut sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
                loader.get_acceleration_structure_build_sizes(
                    vk::AccelerationStructureBuildTypeKHR::DEVICE,
                    &tlas_build,
                    &[1],
                    &mut sizes,
                );
                sizes
            };
            let tlas_buffer = create_as_buffer(
                allocator,
                tlas_sizes.acceleration_structure_size,
                vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR,
                "rt-tlas",
            );
            let tlas = loader
                .create_acceleration_structure(
                    &vk::AccelerationStructureCreateInfoKHR::default()
                        .buffer(tlas_buffer.buffer)
                        .size(tlas_sizes.acceleration_structure_size)
                        .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL),
                    None,
                )
                .expect("Failed to create TLAS");

            // Shared scratch, aligned to the device's scratch requirement
            // (buffer device addresses only guarantee allocation alignment).
            let scratch_size =
                blas_sizes.build_scratch_size.max(tlas_sizes.build_scratch_size);
            let mut scratch = create_as_buffer(
                allocator,
                scratch_size + self.as_scratch_align,
                vk::BufferUsageFlags::STORAGE_BUFFER,
                "rt-as-scratch",
            );
            let scratch_addr =
                addr_of(scratch.buffer).next_multiple_of(self.as_scratch_align.max(1));

            blas_build = blas_build
                .dst_acceleration_structure(blas)
                .scratch_data(vk::DeviceOrHostAddressKHR { device_address: scratch_addr });
            tlas_build = tlas_build
                .dst_acceleration_structure(tlas)
                .scratch_data(vk::DeviceOrHostAddressKHR { device_address: scratch_addr });

            // One-time submit: BLAS build → barrier → TLAS build.
            let cmd = device
                .allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(command_pool)
                        .level(vk::CommandBufferLevel::PRIMARY)
                        .command_buffer_count(1),
                )
                .expect("Failed to allocate AS build command buffer")[0];
            device
                .begin_command_buffer(
                    cmd,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .unwrap();
            let blas_range = [vk::AccelerationStructureBuildRangeInfoKHR::default()
                .primitive_count(self.tri_count)];
            loader.cmd_build_acceleration_structures(cmd, &[blas_build], &[&blas_range]);
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                vk::DependencyFlags::empty(),
                &[vk::MemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR)
                    .dst_access_mask(
                        vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR
                            | vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR,
                    )],
                &[],
                &[],
            );
            let tlas_range =
                [vk::AccelerationStructureBuildRangeInfoKHR::default().primitive_count(1)];
            loader.cmd_build_acceleration_structures(cmd, &[tlas_build], &[&tlas_range]);
            device.end_command_buffer(cmd).unwrap();
            let cmds = [cmd];
            device
                .queue_submit(
                    queue,
                    &[vk::SubmitInfo::default().command_buffers(&cmds)],
                    vk::Fence::null(),
                )
                .expect("AS build submit failed");
            let _ = device.queue_wait_idle(queue);
            device.free_command_buffers(command_pool, &cmds);
            destroy_cpu_buffer(device, allocator, &mut scratch);

            self.accel = Some(Accel { blas, blas_buffer, tlas, tlas_buffer, instances });
        }
    }

    fn destroy_accel(&mut self, device: &ash::Device, allocator: &mut Allocator) {
        if let Some(mut accel) = self.accel.take() {
            let loader = self.accel_loader.as_ref().expect("accel without loader");
            unsafe {
                loader.destroy_acceleration_structure(accel.tlas, None);
                loader.destroy_acceleration_structure(accel.blas, None);
            }
            destroy_cpu_buffer(device, allocator, &mut accel.tlas_buffer);
            destroy_cpu_buffer(device, allocator, &mut accel.blas_buffer);
            destroy_cpu_buffer(device, allocator, &mut accel.instances);
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

    fn recreate_targets(&mut self, device: &ash::Device, allocator: &mut Allocator, w: u32, h: u32) {
        self.destroy_targets(device, allocator);
        self.output_size = (w, h);
        self.output_initialized = false;
        let gpu_buffer = |allocator: &mut Allocator,
                          bytes_per_px: vk::DeviceSize,
                          name: &'static str|
         -> AllocatedBuffer {
            let size = (w as vk::DeviceSize) * (h as vk::DeviceSize) * bytes_per_px;
            unsafe {
                let buffer = device
                    .create_buffer(
                        &vk::BufferCreateInfo::default()
                            .size(size)
                            .usage(vk::BufferUsageFlags::STORAGE_BUFFER)
                            .sharing_mode(vk::SharingMode::EXCLUSIVE),
                        None,
                    )
                    .expect("Failed to create RT target buffer");
                let requirements = device.get_buffer_memory_requirements(buffer);
                let allocation = allocator
                    .allocate(&AllocationCreateDesc {
                        name,
                        requirements,
                        location: MemoryLocation::GpuOnly,
                        linear: true,
                        allocation_scheme: AllocationScheme::GpuAllocatorManaged,
                    })
                    .expect("Failed to allocate RT target memory");
                device
                    .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
                    .expect("Failed to bind RT target memory");
                AllocatedBuffer { buffer, allocation: Some(allocation), size }
            }
        };
        self.accum = gpu_buffer(allocator, 16, "rt-accum");
        self.features = gpu_buffer(allocator, 32, "rt-features");
        if self.denoiser.is_some() {
            self.ping = gpu_buffer(allocator, 16, "rt-denoise-ping");
            self.pong = gpu_buffer(allocator, 16, "rt-denoise-pong");
        }
        unsafe {
            let image = device
                .create_image(
                    &vk::ImageCreateInfo::default()
                        .image_type(vk::ImageType::TYPE_2D)
                        .format(vk::Format::R8G8B8A8_UNORM)
                        .extent(vk::Extent3D { width: w, height: h, depth: 1 })
                        .mip_levels(1)
                        .array_layers(1)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .tiling(vk::ImageTiling::OPTIMAL)
                        .usage(vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_SRC)
                        .initial_layout(vk::ImageLayout::UNDEFINED),
                    None,
                )
                .expect("Failed to create RT output image");
            let requirements = device.get_image_memory_requirements(image);
            let allocation = allocator
                .allocate(&AllocationCreateDesc {
                    name: "rt-output",
                    requirements,
                    location: MemoryLocation::GpuOnly,
                    linear: false,
                    allocation_scheme: AllocationScheme::GpuAllocatorManaged,
                })
                .expect("Failed to allocate RT output memory");
            device
                .bind_image_memory(image, allocation.memory(), allocation.offset())
                .expect("Failed to bind RT output memory");
            let view = device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(vk::Format::R8G8B8A8_UNORM)
                        .subresource_range(
                            vk::ImageSubresourceRange::default()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .level_count(1)
                                .layer_count(1),
                        ),
                    None,
                )
                .expect("Failed to create RT output view");
            self.output_image = image;
            self.output_view = view;
            self.output_allocation = Some(allocation);

            for frame in &self.frames {
                let accum_infos = [vk::DescriptorBufferInfo::default()
                    .buffer(self.accum.buffer)
                    .range(vk::WHOLE_SIZE)];
                let feature_infos = [vk::DescriptorBufferInfo::default()
                    .buffer(self.features.buffer)
                    .range(vk::WHOLE_SIZE)];
                let image_infos = [vk::DescriptorImageInfo::default()
                    .image_view(view)
                    .image_layout(vk::ImageLayout::GENERAL)];
                device.update_descriptor_sets(
                    &[
                        vk::WriteDescriptorSet::default()
                            .dst_set(frame.descriptor_set)
                            .dst_binding(4)
                            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                            .buffer_info(&accum_infos),
                        vk::WriteDescriptorSet::default()
                            .dst_set(frame.descriptor_set)
                            .dst_binding(5)
                            .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                            .image_info(&image_infos),
                        vk::WriteDescriptorSet::default()
                            .dst_set(frame.descriptor_set)
                            .dst_binding(6)
                            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                            .buffer_info(&feature_infos),
                    ],
                    &[],
                );
            }
            if let Some(denoiser) = &self.denoiser {
                denoiser.write_target_descriptors(
                    device,
                    self.accum.buffer,
                    self.features.buffer,
                    self.ping.buffer,
                    self.pong.buffer,
                    view,
                );
            }
        }
    }

    fn destroy_targets(&mut self, device: &ash::Device, allocator: &mut Allocator) {
        unsafe {
            if self.output_view != vk::ImageView::null() {
                device.destroy_image_view(self.output_view, None);
                device.destroy_image(self.output_image, None);
                self.output_view = vk::ImageView::null();
                self.output_image = vk::Image::null();
            }
        }
        if let Some(a) = self.output_allocation.take() {
            let _ = allocator.free(a);
        }
        destroy_cpu_buffer(device, allocator, &mut self.accum);
        destroy_cpu_buffer(device, allocator, &mut self.features);
        destroy_cpu_buffer(device, allocator, &mut self.ping);
        destroy_cpu_buffer(device, allocator, &mut self.pong);
        self.output_size = (0, 0);
    }

    /// True while another dispatch would still refine the image.
    pub(crate) fn accumulating(&self) -> bool {
        self.tri_count > 0 && self.sample_index < MAX_SAMPLES
    }

    /// After the frame fence: write this frame's params, and point this
    /// frame's image bindings at the scene's image as it is NOW.
    ///
    /// Each frame, because a shared image is the 2D pass's to replace: its
    /// upload may land after the scene was set, and freeing it destroys the
    /// view. This frame's set is past its fence, so it is safe to rewrite,
    /// and it is rewritten before every use — a view that is gone is never
    /// one a dispatch reads. `shared` looks an id up: its view and size.
    pub(crate) fn write_frame_uniforms(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        shared: &dyn Fn(u32) -> Option<(vk::ImageView, u32, u32)>,
    ) {
        if !self.staged || self.tri_count == 0 {
            return;
        }
        let Some(camera) = self.camera else { return };
        let bound = self.image.as_ref().and_then(|image| {
            let (view, w, h) = match (&image.owned, image.shared) {
                (Some(owned), _) => (owned.view, owned.size.0, owned.size.1),
                (None, Some(id)) => shared(id)?,
                (None, None) => return None,
            };
            Some((view, ParamImage { width: w, height: h, corners: image.corners, opacity: image.opacity }))
        });
        // No image, or one not there yet: opacity 0 lets every ray through
        // the quad, and the stand-in is never sampled for it.
        let (view, param_image) = match bound {
            Some((view, p)) => (view, Some(p)),
            None => (self.stand_in.view, None),
        };
        Self::write_image_descriptor(
            device,
            self.frames[frame_index].descriptor_set,
            view,
            self.image_sampler,
        );
        let params = rt_params(
            camera,
            self.output_size,
            self.sample_index,
            self.spp,
            param_image,
            self.background,
            &self.environment,
        );
        let frame = &mut self.frames[frame_index];
        frame.uniforms.allocation.as_mut().unwrap().mapped_slice_mut().unwrap()
            [..std::mem::size_of::<RtParams>()]
            .copy_from_slice(bytemuck::bytes_of(&params));
        if let Some(denoiser) = &mut self.denoiser {
            denoiser.write_frame_uniforms(
                frame_index,
                self.output_size.0,
                self.output_size.1,
                self.sample_index + self.spp,
            );
        }
    }

    /// Record one accumulation dispatch + the blit into the backdrop's pane
    /// region. Returns false when there is nothing to do (not staged, empty
    /// scene, or converged) — the backdrop then simply keeps its content.
    /// On true, the backdrop ends in TRANSFER_SRC (like `SceneStage::record`).
    ///
    /// `backdrop_in_transfer_src` says the raster scene pass already ran this
    /// frame (backdrop in TRANSFER_SRC); otherwise it is in SHADER_READ_ONLY.
    pub(crate) fn record(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame_index: usize,
        backdrop_image: vk::Image,
        backdrop_extent: vk::Extent2D,
        backdrop_in_transfer_src: bool,
    ) -> bool {
        if !self.staged || self.tri_count == 0 || self.output_image == vk::Image::null() {
            self.staged = false;
            return false;
        }
        self.staged = false;
        if self.sample_index >= MAX_SAMPLES {
            return false;
        }
        let (w, h) = self.output_size;
        let color_range = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .level_count(1)
            .layer_count(1);
        unsafe {
            // Output image to GENERAL for the compute write; order this
            // dispatch's accum access after the previous frame's. The src
            // stage always includes COMPUTE_SHADER — the accum buffer
            // barrier's access flags must be legal for it even on the first
            // dispatch, when the image side is still UNDEFINED.
            let (old_layout, src_access, src_stage) = if self.output_initialized {
                (
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    vk::AccessFlags::TRANSFER_READ,
                    vk::PipelineStageFlags::TRANSFER | vk::PipelineStageFlags::COMPUTE_SHADER,
                )
            } else {
                (
                    vk::ImageLayout::UNDEFINED,
                    vk::AccessFlags::empty(),
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                )
            };
            device.cmd_pipeline_barrier(
                cmd,
                src_stage,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[vk::BufferMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .buffer(self.accum.buffer)
                    .size(vk::WHOLE_SIZE)],
                &[vk::ImageMemoryBarrier::default()
                    .src_access_mask(src_access)
                    .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
                    .old_layout(old_layout)
                    .new_layout(vk::ImageLayout::GENERAL)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(self.output_image)
                    .subresource_range(color_range)],
            );
            self.output_initialized = true;

            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.pipeline_layout,
                0,
                &[self.frames[frame_index].descriptor_set],
                &[],
            );
            device.cmd_dispatch(cmd, w.div_ceil(WORKGROUP), h.div_ceil(WORKGROUP), 1);

            // À-trous denoise passes; the last one rewrites out_img (still
            // GENERAL), so the transfer barrier below covers either writer.
            if let Some(denoiser) = &self.denoiser {
                denoiser.record(device, cmd, frame_index, w, h);
            }

            // Output to TRANSFER_SRC, backdrop to TRANSFER_DST for the blit.
            let (bd_old, bd_access, bd_stage) = if backdrop_in_transfer_src {
                (
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    vk::AccessFlags::TRANSFER_READ,
                    vk::PipelineStageFlags::TRANSFER,
                )
            } else {
                (
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    vk::AccessFlags::SHADER_READ,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                )
            };
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER | bd_stage,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[
                    vk::ImageMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                        .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                        .old_layout(vk::ImageLayout::GENERAL)
                        .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .image(self.output_image)
                        .subresource_range(color_range),
                    vk::ImageMemoryBarrier::default()
                        .src_access_mask(bd_access)
                        .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                        .old_layout(bd_old)
                        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .image(backdrop_image)
                        .subresource_range(color_range),
                ],
            );

            if self.pane_moved {
                self.pane_moved = false;
                device.cmd_clear_color_image(
                    cmd,
                    backdrop_image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &vk::ClearColorValue { float32: [0.0; 4] },
                    &[color_range],
                );
            }

            // Blit (not copy): converts UNORM → the backdrop's sRGB format.
            let (px, py, _, _) = self.pane;
            let dst_x0 = px.min(backdrop_extent.width);
            let dst_y0 = py.min(backdrop_extent.height);
            let bw = w.min(backdrop_extent.width - dst_x0);
            let bh = h.min(backdrop_extent.height - dst_y0);
            if bw > 0 && bh > 0 {
                let layers = vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .layer_count(1);
                device.cmd_blit_image(
                    cmd,
                    self.output_image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    backdrop_image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[vk::ImageBlit::default()
                        .src_subresource(layers)
                        .src_offsets([
                            vk::Offset3D { x: 0, y: 0, z: 0 },
                            vk::Offset3D { x: bw as i32, y: bh as i32, z: 1 },
                        ])
                        .dst_subresource(layers)
                        .dst_offsets([
                            vk::Offset3D { x: dst_x0 as i32, y: dst_y0 as i32, z: 0 },
                            vk::Offset3D {
                                x: (dst_x0 + bw) as i32,
                                y: (dst_y0 + bh) as i32,
                                z: 1,
                            },
                        ])],
                    vk::Filter::NEAREST,
                );
            }

            // Backdrop to TRANSFER_SRC: the swapchain copy path expects it
            // exactly as SceneStage::record leaves it.
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                    .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(backdrop_image)
                    .subresource_range(color_range)],
            );
        }
        self.sample_index += self.spp;
        true
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

// --- Headless offscreen rendering (thumbnails, previews) ---

/// One-shot path-traced rendering with no window anywhere: a headless
/// [`super::VkCore`] + an [`RtStage`] whose "backdrop" is a private sRGB
/// target image, read back to CPU pixels. This is the seam consumers like
/// the cce-files thumbnailer sit on.
///
/// Not `Send`-safe by design intent (owns a device); create it on the worker
/// thread that renders.
pub struct RtOffscreen {
    stage: RtStage,
    target_image: vk::Image,
    target_allocation: Option<Allocation>,
    readback: AllocatedBuffer,
    size: (u32, u32),
    cmd: vk::CommandBuffer,
    fence: vk::Fence,
    // Declared last: dropped after everything above is destroyed in Drop.
    core: super::VkCore,
}

impl RtOffscreen {
    /// Samples per submit: keeps each dispatch well under GPU watchdog
    /// timeouts even at large sizes; a render loops submits to reach the
    /// requested sample count.
    const CHUNK_SPP: u32 = 8;

    pub fn new() -> Self {
        let mut core = super::VkCore::new_headless();
        let device = core.device.clone();
        let accel_loader = core.accel_loader.clone();
        let as_scratch_align = core.as_scratch_align;
        let min_uniform_align = core.min_uniform_align;
        let (queue, command_pool) = (core.queue, core.command_pool);
        let allocator = core.allocator.as_mut().unwrap();
        let stage = RtStage::new(
            &device,
            allocator,
            1,
            accel_loader.as_ref(),
            as_scratch_align,
            min_uniform_align,
            queue,
            command_pool,
        );
        unsafe {
            let cmd = device
                .allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(core.command_pool)
                        .level(vk::CommandBufferLevel::PRIMARY)
                        .command_buffer_count(1),
                )
                .expect("Failed to allocate RT offscreen command buffer")[0];
            let fence = device
                .create_fence(&vk::FenceCreateInfo::default(), None)
                .expect("Failed to create RT offscreen fence");
            RtOffscreen {
                stage,
                target_image: vk::Image::null(),
                target_allocation: None,
                readback: AllocatedBuffer::null(),
                size: (0, 0),
                cmd,
                fence,
                core,
            }
        }
    }

    /// What a camera ray that meets nothing shows — see
    /// `VkRenderer::set_rt_background`.
    pub fn set_background(&mut self, background: Option<[f32; 3]>) {
        self.stage.set_background(background);
    }

    /// The sky and sun — see [`RtEnvironment`].
    pub fn set_environment(&mut self, environment: RtEnvironment) {
        self.stage.set_environment(environment);
    }

    /// Replace the scene (same schema as `VkRenderer::set_rt_scene`).
    pub fn set_scene(&mut self, triangles: &[RtTriangle], materials: &[RtMaterial]) {
        self.set_scene_with_image(triangles, materials, None);
    }

    /// [`set_scene`](Self::set_scene), with an image standing in the scene.
    pub fn set_scene_with_image(
        &mut self,
        triangles: &[RtTriangle],
        materials: &[RtMaterial],
        image: Option<RtImagePixels>,
    ) {
        let packed = pack_scene(
            triangles.to_vec(),
            materials,
            image.as_ref().map(|i| i.corners),
            self.stage.needs_bvh(),
        );
        unsafe {
            let _ = self.core.device.device_wait_idle();
        }
        let device = self.core.device.clone();
        let queue = self.core.queue;
        let command_pool = self.core.command_pool;
        self.stage.set_scene(
            &device,
            self.core.allocator.as_mut().unwrap(),
            queue,
            command_pool,
            &packed,
            image.map(|i| {
                (
                    RtImageSource::Pixels { pixels: i.pixels, width: i.width, height: i.height },
                    i.corners,
                    i.opacity,
                )
            }),
        );
    }

    /// Render `samples` paths per pixel and return tightly packed
    /// sRGB-encoded RGBA8 pixels (`width * height * 4` bytes). Blocks until
    /// the GPU finishes; meant for worker threads, not frame loops.
    pub fn render(
        &mut self,
        camera: RtCamera,
        width: u32,
        height: u32,
        samples: u32,
    ) -> Vec<u8> {
        let width = width.max(1);
        let height = height.max(1);
        let samples = samples.clamp(1, MAX_SAMPLES);
        let device = self.core.device.clone();
        self.ensure_target(width, height);

        // Fresh accumulation every render: thumbnails are one-shot.
        self.stage.sample_index = 0;
        let extent = vk::Extent2D { width, height };
        let mut done = 0u32;
        while done < samples {
            self.stage.spp = Self::CHUNK_SPP.min(samples - done);
            self.stage.stage(
                &device,
                self.core.allocator.as_mut().unwrap(),
                (0, 0, width, height),
                camera,
            );
            // stage() resets sample_index when the camera or size changed —
            // keep our resume point, not the reset, after the first chunk.
            self.stage.sample_index = done;
            self.stage.write_frame_uniforms(&device, 0, &|_| None);
            unsafe {
                device
                    .begin_command_buffer(self.cmd, &vk::CommandBufferBeginInfo::default())
                    .unwrap();
                let recorded =
                    self.stage.record(&device, self.cmd, 0, self.target_image, extent, true);
                device.end_command_buffer(self.cmd).unwrap();
                assert!(recorded, "RT offscreen: nothing recorded (empty scene?)");
                self.submit_and_wait();
            }
            done += Self::CHUNK_SPP.min(samples - done);
        }

        // Copy the sRGB target (left in TRANSFER_SRC by record) to the
        // readback buffer and map it.
        unsafe {
            device
                .begin_command_buffer(self.cmd, &vk::CommandBufferBeginInfo::default())
                .unwrap();
            device.cmd_copy_image_to_buffer(
                self.cmd,
                self.target_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                self.readback.buffer,
                &[vk::BufferImageCopy::default()
                    .image_subresource(
                        vk::ImageSubresourceLayers::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .layer_count(1),
                    )
                    .image_extent(vk::Extent3D { width, height, depth: 1 })],
            );
            device.cmd_pipeline_barrier(
                self.cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::HOST,
                vk::DependencyFlags::empty(),
                &[],
                &[vk::BufferMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::HOST_READ)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .buffer(self.readback.buffer)
                    .size(vk::WHOLE_SIZE)],
                &[],
            );
            device.end_command_buffer(self.cmd).unwrap();
            self.submit_and_wait();
        }
        let len = (width * height * 4) as usize;
        self.readback.allocation.as_ref().unwrap().mapped_slice().unwrap()[..len].to_vec()
    }

    unsafe fn submit_and_wait(&mut self) {
        let device = &self.core.device;
        let cmds = [self.cmd];
        device
            .queue_submit(
                self.core.queue,
                &[vk::SubmitInfo::default().command_buffers(&cmds)],
                self.fence,
            )
            .expect("RT offscreen submit failed");
        device
            .wait_for_fences(&[self.fence], true, u64::MAX)
            .expect("RT offscreen fence wait failed");
        device.reset_fences(&[self.fence]).unwrap();
    }

    fn ensure_target(&mut self, width: u32, height: u32) {
        if (width, height) == self.size {
            return;
        }
        let device = self.core.device.clone();
        unsafe {
            let _ = device.device_wait_idle();
        }
        self.destroy_target();
        let allocator = self.core.allocator.as_mut().unwrap();
        unsafe {
            let image = device
                .create_image(
                    &vk::ImageCreateInfo::default()
                        .image_type(vk::ImageType::TYPE_2D)
                        .format(vk::Format::R8G8B8A8_SRGB)
                        .extent(vk::Extent3D { width, height, depth: 1 })
                        .mip_levels(1)
                        .array_layers(1)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .tiling(vk::ImageTiling::OPTIMAL)
                        .usage(
                            vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::TRANSFER_SRC,
                        )
                        .initial_layout(vk::ImageLayout::UNDEFINED),
                    None,
                )
                .expect("Failed to create RT offscreen target");
            let requirements = device.get_image_memory_requirements(image);
            let allocation = allocator
                .allocate(&AllocationCreateDesc {
                    name: "rt-offscreen-target",
                    requirements,
                    location: MemoryLocation::GpuOnly,
                    linear: false,
                    allocation_scheme: AllocationScheme::GpuAllocatorManaged,
                })
                .expect("Failed to allocate RT offscreen target memory");
            device
                .bind_image_memory(image, allocation.memory(), allocation.offset())
                .expect("Failed to bind RT offscreen target memory");
            self.target_image = image;
            self.target_allocation = Some(allocation);

            self.readback = create_cpu_buffer(
                &device,
                allocator,
                (width as vk::DeviceSize) * (height as vk::DeviceSize) * 4,
                vk::BufferUsageFlags::TRANSFER_DST,
                "rt-readback",
            );

            // RtStage::record expects the blit destination in TRANSFER_SRC
            // (the steady state SceneStage leaves the backdrop in).
            device
                .begin_command_buffer(self.cmd, &vk::CommandBufferBeginInfo::default())
                .unwrap();
            device.cmd_pipeline_barrier(
                self.cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::empty())
                    .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                    .old_layout(vk::ImageLayout::UNDEFINED)
                    .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(image)
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .level_count(1)
                            .layer_count(1),
                    )],
            );
            device.end_command_buffer(self.cmd).unwrap();
            self.submit_and_wait();
        }
        self.size = (width, height);
    }

    fn destroy_target(&mut self) {
        unsafe {
            if self.target_image != vk::Image::null() {
                self.core.device.destroy_image(self.target_image, None);
                self.target_image = vk::Image::null();
            }
        }
        if let Some(a) = self.target_allocation.take() {
            let _ = self.core.allocator.as_mut().unwrap().free(a);
        }
        let device = self.core.device.clone();
        destroy_cpu_buffer(&device, self.core.allocator.as_mut().unwrap(), &mut self.readback);
        self.size = (0, 0);
    }
}

impl Default for RtOffscreen {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for RtOffscreen {
    fn drop(&mut self) {
        unsafe {
            let _ = self.core.device.device_wait_idle();
        }
        self.destroy_target();
        let device = self.core.device.clone();
        self.stage.destroy(&device, self.core.allocator.as_mut().unwrap());
        unsafe {
            self.core.device.destroy_fence(self.fence, None);
            // The command buffer dies with the pool in VkCore's Drop.
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
