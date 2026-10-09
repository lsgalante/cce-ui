//! The UI pipeline: shader2d with its descriptor layout, push range, vertex layout and blend —
//! shared by the swapchain path and `vk::plate_probe`, so the two cannot draw differently.

use super::*;

/// Compile WGSL to SPIR-V. The Y-flip between wgpu NDC (Y-up) and Vulkan NDC
/// (Y-down) is handled with a negative-height viewport (like wgpu-hal), NOT in
/// the shader — flipping in the shader would reverse screen-space winding and
/// break the 3D pipeline's back-face culling.
/// The 2D UI pipeline over `render_pass`: shader2d's descriptor set layout,
/// its push-constant range, the vertex layout and the blend — what the
/// renderer draws every frame with. Shared with the offscreen test harness
/// (`vk::plate_probe`), so it draws with exactly the live pipeline.
pub(crate) unsafe fn create_ui_pipeline(
    device: &ash::Device,
    render_pass: vk::RenderPass,
) -> (vk::DescriptorSetLayout, vk::PipelineLayout, vk::ShaderModule, vk::Pipeline) {
    // Descriptor set layout mirroring shader.wgsl @group(0): naga maps WGSL
    // texture/sampler/uniform bindings 1:1 onto set 0 descriptor bindings.
    let bindings = [
        vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        vk::DescriptorSetLayoutBinding::default()
            .binding(1)
            .descriptor_type(vk::DescriptorType::SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        vk::DescriptorSetLayoutBinding::default()
            .binding(2)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        vk::DescriptorSetLayoutBinding::default()
            .binding(3)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    ];
    let descriptor_set_layout = device
        .create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
            None,
        )
        .expect("Failed to create descriptor set layout");

    let set_layouts = [descriptor_set_layout];
    // Push constants: the per-batch rounded-rect clip plus the SDF-lit
    // plate block (eight vec4s, matching shader2d's `RRectClip`), read by
    // shader2d's fragment stage. See `PUSH_CONSTANT_BYTES` — the block is
    // exactly the Vulkan-guaranteed minimum and completely full.
    let push_ranges = [vk::PushConstantRange::default()
        .stage_flags(vk::ShaderStageFlags::FRAGMENT)
        .offset(0)
        .size(PUSH_CONSTANT_BYTES)];
    let pipeline_layout = device
        .create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::default()
                .set_layouts(&set_layouts)
                .push_constant_ranges(&push_ranges),
            None,
        )
        .expect("Failed to create pipeline layout");

    // Pipeline from shader.wgsl (both entry points live in one SPIR-V module).
    let spirv = shader2d_spirv();
    let shader_module = device
        .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(spirv), None)
        .expect("Failed to create shader module");

    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(shader_module)
            .name(c"vs_main"),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(shader_module)
            .name(c"fs_main"),
    ];

    // Vertex layout = cce_ui::engine::Vertex: pos vec2f, color vec4f, clip vec3f.
    let vertex_bindings = [vk::VertexInputBindingDescription::default()
        .binding(0)
        .stride(std::mem::size_of::<Vertex>() as u32)
        .input_rate(vk::VertexInputRate::VERTEX)];
    let vertex_attributes = [
        vk::VertexInputAttributeDescription::default()
            .location(0)
            .binding(0)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(0),
        vk::VertexInputAttributeDescription::default()
            .location(1)
            .binding(0)
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .offset(8),
        vk::VertexInputAttributeDescription::default()
            .location(2)
            .binding(0)
            .format(vk::Format::R32G32B32_SFLOAT)
            .offset(24),
    ];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(&vertex_bindings)
        .vertex_attribute_descriptions(&vertex_attributes);

    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    // wgpu::BlendState::ALPHA_BLENDING.
    let blend_attachments = [vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .alpha_blend_op(vk::BlendOp::ADD)
        .color_write_mask(vk::ColorComponentFlags::RGBA)];
    let color_blend =
        vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend_attachments);
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    let pipeline = device
        .create_graphics_pipelines(
            vk::PipelineCache::null(),
            &[vk::GraphicsPipelineCreateInfo::default()
                .stages(&stages)
                .vertex_input_state(&vertex_input)
                .input_assembly_state(&input_assembly)
                .viewport_state(&viewport_state)
                .rasterization_state(&rasterization)
                .multisample_state(&multisample)
                .color_blend_state(&color_blend)
                .dynamic_state(&dynamic_state)
                .layout(pipeline_layout)
                .render_pass(render_pass)
                .subpass(0)],
            None,
        )
        .expect("Failed to create graphics pipeline")[0];
    (descriptor_set_layout, pipeline_layout, shader_module, pipeline)
}
