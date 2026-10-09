//! Building the stage: the offscreen render pass, the descriptor and pipeline layouts, the fill,
//! see-through, wire, image and lit pipelines, and each frame in flight's buffers and descriptor set.

use super::*;

impl SceneStage {
    pub(crate) fn new(
        device: &ash::Device,
        allocator: &mut Allocator,
        format: vk::Format,
        extent: vk::Extent2D,
        frames_in_flight: usize,
        min_uniform_align: vk::DeviceSize,
        max_line_width: f32,
    ) -> Self {
        unsafe {
            // Offscreen pass: color -> TRANSFER_SRC (copied to the swapchain
            // right after), depth is transient.
            let attachments = [
                vk::AttachmentDescription::default()
                    .format(format)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .load_op(vk::AttachmentLoadOp::CLEAR)
                    .store_op(vk::AttachmentStoreOp::STORE)
                    .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                    .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                    .initial_layout(vk::ImageLayout::UNDEFINED)
                    .final_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL),
                vk::AttachmentDescription::default()
                    .format(vk::Format::D32_SFLOAT)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .load_op(vk::AttachmentLoadOp::CLEAR)
                    .store_op(vk::AttachmentStoreOp::DONT_CARE)
                    .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                    .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                    .initial_layout(vk::ImageLayout::UNDEFINED)
                    .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL),
            ];
            let color_refs = [vk::AttachmentReference::default()
                .attachment(0)
                .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
            let depth_ref = vk::AttachmentReference::default()
                .attachment(1)
                .layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);
            let subpasses = [vk::SubpassDescription::default()
                .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
                .color_attachments(&color_refs)
                .depth_stencil_attachment(&depth_ref)];
            let dependencies = [
                // Prior frame sampled the backdrop (blur plates) and used the depth
                // image; execution dependency before we overwrite from UNDEFINED.
                vk::SubpassDependency::default()
                    .src_subpass(vk::SUBPASS_EXTERNAL)
                    .dst_subpass(0)
                    .src_stage_mask(
                        vk::PipelineStageFlags::FRAGMENT_SHADER
                            | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                    )
                    .src_access_mask(vk::AccessFlags::empty())
                    .dst_stage_mask(
                        vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                            | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
                    )
                    .dst_access_mask(
                        vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                            | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                    ),
                // The copy to the swapchain reads the color attachment right after.
                vk::SubpassDependency::default()
                    .src_subpass(0)
                    .dst_subpass(vk::SUBPASS_EXTERNAL)
                    .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                    .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                    .dst_stage_mask(vk::PipelineStageFlags::TRANSFER)
                    .dst_access_mask(vk::AccessFlags::TRANSFER_READ),
            ];
            let render_pass = device
                .create_render_pass(
                    &vk::RenderPassCreateInfo::default()
                        .attachments(&attachments)
                        .subpasses(&subpasses)
                        .dependencies(&dependencies),
                    None,
                )
                .expect("Failed to create scene render pass");

            let bindings = [vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)];
            let descriptor_set_layout = device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
                .expect("Failed to create scene descriptor set layout");
            let set_layouts_one = [descriptor_set_layout];
            let pipeline_layout = device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts_one),
                    None,
                )
                .expect("Failed to create scene pipeline layout");

            let spirv = crate::vk::renderer::scene3d_spirv();
            let shader_module = device
                .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(spirv), None)
                .expect("Failed to create 3D shader module");
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
            // Binding 0 the mesh's vertices, binding 1 the instances it is
            // drawn for (`SceneDraw::instances`), both `Vertex3D`s: the
            // instance's position is the offset, its colour the multiplier.
            let vertex_bindings = [
                vk::VertexInputBindingDescription::default()
                    .binding(0)
                    .stride(std::mem::size_of::<Vertex3D>() as u32)
                    .input_rate(vk::VertexInputRate::VERTEX),
                vk::VertexInputBindingDescription::default()
                    .binding(1)
                    .stride(std::mem::size_of::<Vertex3D>() as u32)
                    .input_rate(vk::VertexInputRate::INSTANCE),
            ];
            let vertex_attributes = [
                vk::VertexInputAttributeDescription::default()
                    .location(0)
                    .binding(0)
                    .format(vk::Format::R32G32B32_SFLOAT)
                    .offset(0),
                vk::VertexInputAttributeDescription::default()
                    .location(1)
                    .binding(0)
                    .format(vk::Format::R32G32B32_SFLOAT)
                    .offset(12),
                vk::VertexInputAttributeDescription::default()
                    .location(2)
                    .binding(1)
                    .format(vk::Format::R32G32B32_SFLOAT)
                    .offset(0),
                vk::VertexInputAttributeDescription::default()
                    .location(3)
                    .binding(1)
                    .format(vk::Format::R32G32B32_SFLOAT)
                    .offset(12),
            ];
            let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
                .vertex_binding_descriptions(&vertex_bindings)
                .vertex_attribute_descriptions(&vertex_attributes);
            let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
                .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
            let viewport_state = vk::PipelineViewportStateCreateInfo::default()
                .viewport_count(1)
                .scissor_count(1);
            // wgpu pipeline_3d: CCW front, back-face culling. Winding survives
            // because the renderer flips Y via negative viewport height (like
            // wgpu-hal), not in the shader. Depth bias is enabled but DYNAMIC
            // (zero for ordinary fills): fills carrying a wire overlay are
            // pushed back per `SceneDraw::wire_base_width`.
            let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
                .polygon_mode(vk::PolygonMode::FILL)
                .cull_mode(vk::CullModeFlags::BACK)
                .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
                .depth_bias_enable(true)
                .line_width(1.0);
            let multisample = vk::PipelineMultisampleStateCreateInfo::default()
                .rasterization_samples(vk::SampleCountFlags::TYPE_1);
            let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
                .depth_test_enable(true)
                .depth_write_enable(true)
                .depth_compare_op(vk::CompareOp::LESS);
            let blend_attachments = [vk::PipelineColorBlendAttachmentState::default()
                .blend_enable(true)
                .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
                .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(vk::BlendFactor::ONE)
                .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                .alpha_blend_op(vk::BlendOp::ADD)
                .color_write_mask(vk::ColorComponentFlags::RGBA)];
            let color_blend = vk::PipelineColorBlendStateCreateInfo::default()
                .attachments(&blend_attachments);
            let dynamic_states = [
                vk::DynamicState::VIEWPORT,
                vk::DynamicState::SCISSOR,
                vk::DynamicState::DEPTH_BIAS,
            ];
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
                        .depth_stencil_state(&depth_stencil)
                        .color_blend_state(&color_blend)
                        .dynamic_state(&dynamic_state)
                        .layout(pipeline_layout)
                        .render_pass(render_pass)
                        .subpass(0)],
                    None,
                )
                .expect("Failed to create 3D pipeline")[0];

            // The see-through twin: the fill pipeline with no culling (a
            // translucent closed mesh shows its far wall) and no depth
            // WRITES (its near layers must not hide its far ones, nor the
            // wires riding it). The depth test stays: an opaque thing drawn
            // earlier still occludes it.
            let see_through_pipeline = {
                let rasterization_st = vk::PipelineRasterizationStateCreateInfo::default()
                    .polygon_mode(vk::PolygonMode::FILL)
                    .cull_mode(vk::CullModeFlags::NONE)
                    .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
                    .depth_bias_enable(true)
                    .line_width(1.0);
                let depth_stencil_st = vk::PipelineDepthStencilStateCreateInfo::default()
                    .depth_test_enable(true)
                    .depth_write_enable(false)
                    .depth_compare_op(vk::CompareOp::LESS);
                device
                    .create_graphics_pipelines(
                        vk::PipelineCache::null(),
                        &[vk::GraphicsPipelineCreateInfo::default()
                            .stages(&stages)
                            .vertex_input_state(&vertex_input)
                            .input_assembly_state(&input_assembly)
                            .viewport_state(&viewport_state)
                            .rasterization_state(&rasterization_st)
                            .multisample_state(&multisample)
                            .depth_stencil_state(&depth_stencil_st)
                            .color_blend_state(&color_blend)
                            .dynamic_state(&dynamic_state)
                            .layout(pipeline_layout)
                            .render_pass(render_pass)
                            .subpass(0)],
                        None,
                    )
                    .expect("Failed to create 3D see-through pipeline")[0]
            };

            // The wireframe twin draws LINE_LIST edge meshes, NOT the fill
            // mesh through PolygonMode::LINE. Polygon-mode lines proved
            // driver-broken twice on Mesa ANV with the negative-height
            // viewport: triangle winding is evaluated without the
            // framebuffer Y-mirror (CCW-front selected the FAR facet set —
            // wireframe spheres drew only the far hemisphere's interior,
            // pole-fan forensics), and vertex-attribute sourcing fetches
            // from the wrong vertices (wires aligned to the mesh but carried
            // colors from a rotated region — the sphere's symmetry masked
            // the misplacement geometrically). Real line primitives take the
            // ordinary, well-tested raster path: no facet culling exists, so
            // hidden-wire removal is the DEPTH test against the fill, which
            // `SceneDraw::wire_base_width` pushes back.
            // The compare is LESS_OR_EQUAL with writes off, and the wires
            // carry NO bias — the tiebreak lives on the FILL side
            // (`SceneDraw::wire_base_width` pushes the fill back by its own
            // slope-scaled polygon offset). Biasing the line cannot work for
            // wide wires: a w-px line's fragments sample the fill's plane up
            // to (w/2 + 0.5) px off the true edge, but the hardware scales a
            // line's slope bias by its ALONG-AXIS depth slope — near zero
            // for contour-following wires — while strong constant terms
            // punch FAR-side wires through the near fill (the old -4/-1 did
            // exactly that; with the culled-facet flip above those far wires
            // were the only wires, and the overlay's whole lattice was the
            // back side showing through, which read as the mesh
            // counter-rotating during orbits).
            // `depth_write` builds the see-through twin (`SceneDraw::
            // see_through` on a WIRE draw): identical but for the writes.
            let make_lines_pipeline = |depth_write: bool| {
                let depth_stencil_lines = vk::PipelineDepthStencilStateCreateInfo::default()
                    .depth_test_enable(true)
                    .depth_write_enable(depth_write)
                    .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL);
                let input_assembly_lines = vk::PipelineInputAssemblyStateCreateInfo::default()
                    .topology(vk::PrimitiveTopology::LINE_LIST);
                let rasterization_lines = vk::PipelineRasterizationStateCreateInfo::default()
                    .polygon_mode(vk::PolygonMode::FILL)
                    .cull_mode(vk::CullModeFlags::NONE)
                    .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
                    .depth_bias_enable(true)
                    .line_width(1.0);
                // Line width is dynamic (SceneDraw::line_width); depth bias
                // is dynamic on both pipelines and set to zero for wires —
                // see the comment above.
                let dynamic_states_lines = [
                    vk::DynamicState::VIEWPORT,
                    vk::DynamicState::SCISSOR,
                    vk::DynamicState::LINE_WIDTH,
                    vk::DynamicState::DEPTH_BIAS,
                ];
                let dynamic_state_lines = vk::PipelineDynamicStateCreateInfo::default()
                    .dynamic_states(&dynamic_states_lines);
                device
                    .create_graphics_pipelines(
                        vk::PipelineCache::null(),
                        &[vk::GraphicsPipelineCreateInfo::default()
                            .stages(&stages)
                            .vertex_input_state(&vertex_input)
                            .input_assembly_state(&input_assembly_lines)
                            .viewport_state(&viewport_state)
                            .rasterization_state(&rasterization_lines)
                            .multisample_state(&multisample)
                            .depth_stencil_state(&depth_stencil_lines)
                            .color_blend_state(&color_blend)
                            .dynamic_state(&dynamic_state_lines)
                            .layout(pipeline_layout)
                            .render_pass(render_pass)
                            .subpass(0)],
                        None,
                    )
                    .expect("Failed to create 3D wireframe pipeline")[0]
            };
            let wireframe_pipeline = Some(make_lines_pipeline(false));
            let wireframe_see_through_pipeline = make_lines_pipeline(true);

            // The image pipeline: the fill's pass, blend and depth state,
            // with no culling (a picture is seen from behind, mirrored) and
            // a vertex that carries a uv where a mesh's carries a colour.
            let image_bindings = crate::vk::image::image_set_bindings();
            let image_set_layout = device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&image_bindings),
                    None,
                )
                .expect("Failed to create scene image set layout");
            let image_set_layouts = [descriptor_set_layout, image_set_layout];
            let image_pipeline_layout = device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default().set_layouts(&image_set_layouts),
                    None,
                )
                .expect("Failed to create scene image pipeline layout");
            let image_shader_module = device
                .create_shader_module(
                    &vk::ShaderModuleCreateInfo::default()
                        .code(crate::vk::renderer::scene3d_image_spirv()),
                    None,
                )
                .expect("Failed to create 3D image shader module");
            let image_pipeline = {
                let stages = [
                    vk::PipelineShaderStageCreateInfo::default()
                        .stage(vk::ShaderStageFlags::VERTEX)
                        .module(image_shader_module)
                        .name(c"vs_main"),
                    vk::PipelineShaderStageCreateInfo::default()
                        .stage(vk::ShaderStageFlags::FRAGMENT)
                        .module(image_shader_module)
                        .name(c"fs_main"),
                ];
                let vertex_bindings = [vk::VertexInputBindingDescription::default()
                    .binding(0)
                    .stride(std::mem::size_of::<ImageVertex3D>() as u32)
                    .input_rate(vk::VertexInputRate::VERTEX)];
                let vertex_attributes = [
                    vk::VertexInputAttributeDescription::default()
                        .location(0)
                        .binding(0)
                        .format(vk::Format::R32G32B32_SFLOAT)
                        .offset(0),
                    vk::VertexInputAttributeDescription::default()
                        .location(1)
                        .binding(0)
                        .format(vk::Format::R32G32_SFLOAT)
                        .offset(12),
                ];
                let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
                    .vertex_binding_descriptions(&vertex_bindings)
                    .vertex_attribute_descriptions(&vertex_attributes);
                let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
                    .polygon_mode(vk::PolygonMode::FILL)
                    .cull_mode(vk::CullModeFlags::NONE)
                    .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
                    .line_width(1.0);
                let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
                let dynamic_state = vk::PipelineDynamicStateCreateInfo::default()
                    .dynamic_states(&dynamic_states);
                device
                    .create_graphics_pipelines(
                        vk::PipelineCache::null(),
                        &[vk::GraphicsPipelineCreateInfo::default()
                            .stages(&stages)
                            .vertex_input_state(&vertex_input)
                            .input_assembly_state(&input_assembly)
                            .viewport_state(&viewport_state)
                            .rasterization_state(&rasterization)
                            .multisample_state(&multisample)
                            .depth_stencil_state(&depth_stencil)
                            .color_blend_state(&color_blend)
                            .dynamic_state(&dynamic_state)
                            .layout(image_pipeline_layout)
                            .render_pass(render_pass)
                            .subpass(0)],
                        None,
                    )
                    .expect("Failed to create 3D image pipeline")[0]
            };

            // The lit pipeline: the fill's pass, blend, depth and dynamic
            // depth-bias state, the image pipeline's layout, no culling, and
            // a `LitVertex`.
            let lit_shader_module = device
                .create_shader_module(
                    &vk::ShaderModuleCreateInfo::default().code(crate::vk::renderer::scene3d_lit_spirv()),
                    None,
                )
                .expect("Failed to create 3D lit shader module");
            let lit_pipeline = {
                let stages = [
                    vk::PipelineShaderStageCreateInfo::default()
                        .stage(vk::ShaderStageFlags::VERTEX)
                        .module(lit_shader_module)
                        .name(c"vs_main"),
                    vk::PipelineShaderStageCreateInfo::default()
                        .stage(vk::ShaderStageFlags::FRAGMENT)
                        .module(lit_shader_module)
                        .name(c"fs_main"),
                ];
                let vertex_bindings = [vk::VertexInputBindingDescription::default()
                    .binding(0)
                    .stride(std::mem::size_of::<LitVertex>() as u32)
                    .input_rate(vk::VertexInputRate::VERTEX)];
                let attribute = |location: u32, format: vk::Format, offset: u32| {
                    vk::VertexInputAttributeDescription::default()
                        .location(location)
                        .binding(0)
                        .format(format)
                        .offset(offset)
                };
                let vertex_attributes = [
                    attribute(0, vk::Format::R32G32B32_SFLOAT, 0),
                    attribute(1, vk::Format::R32G32B32_SFLOAT, 12),
                    attribute(2, vk::Format::R32G32_SFLOAT, 24),
                    attribute(3, vk::Format::R32G32B32_SFLOAT, 32),
                ];
                let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
                    .vertex_binding_descriptions(&vertex_bindings)
                    .vertex_attribute_descriptions(&vertex_attributes);
                let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
                    .polygon_mode(vk::PolygonMode::FILL)
                    .cull_mode(vk::CullModeFlags::NONE)
                    .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
                    .depth_bias_enable(true)
                    .line_width(1.0);
                device
                    .create_graphics_pipelines(
                        vk::PipelineCache::null(),
                        &[vk::GraphicsPipelineCreateInfo::default()
                            .stages(&stages)
                            .vertex_input_state(&vertex_input)
                            .input_assembly_state(&input_assembly)
                            .viewport_state(&viewport_state)
                            .rasterization_state(&rasterization)
                            .multisample_state(&multisample)
                            .depth_stencil_state(&depth_stencil)
                            .color_blend_state(&color_blend)
                            .dynamic_state(&dynamic_state)
                            .layout(image_pipeline_layout)
                            .render_pass(render_pass)
                            .subpass(0)],
                        None,
                    )
                    .expect("Failed to create 3D lit pipeline")[0]
            };

            let uniform_stride = SLOT_SIZE.next_multiple_of(min_uniform_align.max(1));

            let pool_sizes = [vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
                .descriptor_count(frames_in_flight as u32)];
            let descriptor_pool = device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .max_sets(frames_in_flight as u32)
                        .pool_sizes(&pool_sizes),
                    None,
                )
                .expect("Failed to create scene descriptor pool");
            let set_layouts: Vec<vk::DescriptorSetLayout> =
                vec![descriptor_set_layout; frames_in_flight];
            let sets = device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(descriptor_pool)
                        .set_layouts(&set_layouts),
                )
                .expect("Failed to allocate scene descriptor sets");
            let frames: Vec<SceneFrame> = sets
                .into_iter()
                .map(|descriptor_set| {
                    let uniforms = create_cpu_buffer(
                        device,
                        allocator,
                        uniform_stride * 16,
                        vk::BufferUsageFlags::UNIFORM_BUFFER,
                        "scene-uniforms",
                    );
                    let image_verts = create_cpu_buffer(
                        device,
                        allocator,
                        1024,
                        vk::BufferUsageFlags::VERTEX_BUFFER,
                        "scene-image-quads",
                    );
                    SceneFrame { uniforms, image_verts, descriptor_set, draw_count: 0 }
                })
                .collect();
            for frame in &frames {
                Self::write_descriptor(device, frame);
            }
            let mut unit_instance = create_cpu_buffer(
                device,
                allocator,
                64,
                vk::BufferUsageFlags::VERTEX_BUFFER,
                "scene-unit-instance",
            );
            let unit: &[u8] = bytemuck::bytes_of(&UNIT_INSTANCE);
            unit_instance.allocation.as_mut().unwrap().mapped_slice_mut().unwrap()[..unit.len()].copy_from_slice(unit);

            let mut stage = SceneStage {
                render_pass,
                pipeline,
                wireframe_pipeline,
                see_through_pipeline,
                wireframe_see_through_pipeline,
                max_line_width,
                pipeline_layout,
                descriptor_set_layout,
                descriptor_pool,
                shader_module,
                uniform_stride,
                image_pipeline,
                image_pipeline_layout,
                image_set_layout,
                image_shader_module,
                lit_pipeline,
                lit_shader_module,
                lit_meshes: Vec::new(),
                lit_light: LitLight::default(),
                lit_fallback_image: None,
                format,
                extent: vk::Extent2D { width: 0, height: 0 },
                backdrop_image: vk::Image::null(),
                backdrop_view: vk::ImageView::null(),
                backdrop_allocation: None,
                depth_image: vk::Image::null(),
                depth_view: vk::ImageView::null(),
                depth_allocation: None,
                framebuffer: vk::Framebuffer::null(),
                meshes: Vec::new(),
                submitted: 0,
                complete_before: 0,
                unit_instance,
                frames,
                staged: None,
                backdrop_valid: false,
                wanted: false,
                light: glam::Vec3::from_array(DEFAULT_SCENE_LIGHT).normalize().to_array(),
            };
            let extent = stage.target_extent(extent);
            stage.resize(device, allocator, extent);
            stage
        }
    }

    pub(super) fn write_descriptor(device: &ash::Device, frame: &SceneFrame) {
        let buffer_infos = [vk::DescriptorBufferInfo::default()
            .buffer(frame.uniforms.buffer)
            .offset(0)
            .range(UNIFORM_SIZE)];
        unsafe {
            device.update_descriptor_sets(
                &[vk::WriteDescriptorSet::default()
                    .dst_set(frame.descriptor_set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
                    .buffer_info(&buffer_infos)],
                &[],
            );
        }
    }
}
