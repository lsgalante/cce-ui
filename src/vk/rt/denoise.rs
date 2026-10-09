//! The à-trous denoiser run over the accumulated image.

use super::*;

pub(super) struct DenoiserFrame {
    /// DENOISE_ITERATIONS dynamic-offset slices of [`DenoiseParams`].
    pub(super) uniforms: AllocatedBuffer,
    /// src = ping, dst = pong.
    pub(super) set_a: vk::DescriptorSet,
    /// src = pong, dst = ping.
    pub(super) set_b: vk::DescriptorSet,
}

/// The à-trous denoise pipeline (rt_denoise.wgsl). Owned by [`RtStage`];
/// its buffers (features/ping/pong) live on the stage with the other
/// pane-sized targets.
pub(super) struct Denoiser {
    pub(super) pipeline: vk::Pipeline,
    pub(super) pipeline_layout: vk::PipelineLayout,
    pub(super) descriptor_set_layout: vk::DescriptorSetLayout,
    pub(super) descriptor_pool: vk::DescriptorPool,
    pub(super) shader_module: vk::ShaderModule,
    pub(super) uniform_stride: vk::DeviceSize,
    pub(super) frames: Vec<DenoiserFrame>,
}

impl Denoiser {
    pub(super) fn new(
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
    pub(super) fn write_target_descriptors(
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
    pub(super) fn write_frame_uniforms(&mut self, frame_index: usize, width: u32, height: u32, n_after: u32) {
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
    pub(super) fn record(&self, device: &ash::Device, cmd: vk::CommandBuffer, frame_index: usize, w: u32, h: u32) {
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

    pub(super) fn destroy(&mut self, device: &ash::Device, allocator: &mut Allocator) {
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
