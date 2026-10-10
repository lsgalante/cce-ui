//! `RtStage::new`: the tracer's tier (ray query or compute), its pipelines, descriptor layouts and
//! sets, the denoiser, and the frames in flight.

use super::*;

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
}
