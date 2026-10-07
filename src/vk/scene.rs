//! 3D scene stage: the ash port of the app's "3D canvas render pass". Draws
//! Vertex3D meshes (shader_3d.wgsl: mvp transform, z=9.99 background-quad
//! special case, window-corner discard) into the full-size backdrop image with
//! a depth buffer, scissored to the viewport pane. The renderer then copies the
//! backdrop into the swapchain image and draws the UI pass over it — the same
//! image doubles as the blur-behind source for the 2D shader, replacing
//! milestone 1's 1x1 placeholder.
//!
//! Meshes are handle-based (`MeshId`); per-draw uniforms (mvp + window info) go
//! into one dynamic-offset uniform buffer per frame in flight, so a frame's
//! draws share a single descriptor set.

use ash::vk;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme, Allocator};
use gpu_allocator::MemoryLocation;

use super::renderer::{create_cpu_buffer, destroy_cpu_buffer, AllocatedBuffer};

pub use crate::draw::scene::{MeshId, SceneDraw, SceneImage, Vertex3D};
use crate::draw::scene::{image_quads_3d, scene_uniforms, wire_base_bias, ImageVertex3D, SceneUniforms, DEFAULT_SCENE_LIGHT, UNIT_INSTANCE};

const UNIFORM_SIZE: vk::DeviceSize = std::mem::size_of::<SceneUniforms>() as vk::DeviceSize;

struct Mesh {
    buffer: AllocatedBuffer,
    count: u32,
}

struct StagedScene {
    scissor: (u32, u32, u32, u32),
    draws: Vec<SceneDraw>,
    images: Vec<SceneImage>,
}

struct SceneFrame {
    uniforms: AllocatedBuffer,
    /// Six vertices per staged `SceneImage`, in staged order.
    image_verts: AllocatedBuffer,
    descriptor_set: vk::DescriptorSet,
    draw_count: u32,
}

pub(crate) struct SceneStage {
    render_pass: vk::RenderPass,
    pipeline: vk::Pipeline,
    /// PolygonMode::LINE twin of `pipeline` — None when the device lacks
    /// fillModeNonSolid (wireframe draws then fall back to the fill pipeline).
    wireframe_pipeline: Option<vk::Pipeline>,
    /// `pipeline` with culling off and depth writes off — the
    /// `SceneDraw::see_through` fill.
    see_through_pipeline: vk::Pipeline,
    /// `wireframe_pipeline` WITH depth writes — the wires of a see-through
    /// fill (`SceneDraw::see_through` on a wireframe draw).
    wireframe_see_through_pipeline: vk::Pipeline,
    /// Device cap for `SceneDraw::line_width` (1.0 without wideLines).
    max_line_width: f32,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    shader_module: vk::ShaderModule,
    uniform_stride: vk::DeviceSize,
    /// The `SceneImage` pipeline: set 0 is the scene's uniforms, set 1 a
    /// user image's descriptor set (`image::image_set_bindings`).
    image_pipeline: vk::Pipeline,
    image_pipeline_layout: vk::PipelineLayout,
    image_set_layout: vk::DescriptorSetLayout,
    image_shader_module: vk::ShaderModule,

    format: vk::Format,
    extent: vk::Extent2D,
    pub(crate) backdrop_image: vk::Image,
    pub(crate) backdrop_view: vk::ImageView,
    backdrop_allocation: Option<Allocation>,
    depth_image: vk::Image,
    depth_view: vk::ImageView,
    depth_allocation: Option<Allocation>,
    framebuffer: vk::Framebuffer,

    meshes: Vec<Mesh>,
    /// The one instance a draw without instances is drawn with
    /// ([`UNIT_INSTANCE`]), bound in binding 1 in place of an instance mesh.
    unit_instance: AllocatedBuffer,
    frames: Vec<SceneFrame>,
    staged: Option<StagedScene>,
    /// True once the backdrop holds rendered content worth copying to screen.
    pub(crate) backdrop_valid: bool,
    /// Whether the app has ever staged a scene (or a traced one). Until it
    /// has, the backdrop and depth targets are 1×1 — see
    /// [`Scene::target_extent`].
    pub(crate) wanted: bool,
    /// Toward the flat shading's light, unit length — see
    /// `VkRenderer::set_scene_light`.
    pub(crate) light: [f32; 3],
}

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

            let spirv = super::renderer::scene3d_spirv();
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
            let image_bindings = super::image::image_set_bindings();
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
                        .code(super::renderer::scene3d_image_spirv()),
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

            let uniform_stride = UNIFORM_SIZE.next_multiple_of(min_uniform_align.max(1));

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

    fn write_descriptor(device: &ash::Device, frame: &SceneFrame) {
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

    /// The size the backdrop and depth targets are made at for a surface of
    /// `surface`: the surface's, once a scene has been staged; 1×1 before.
    ///
    /// Most windows never draw a 3D scene, and full-surface targets cost
    /// ~33 MiB a window at 2560×1600 (D32 depth plus the RGBA backdrop). A
    /// 2D frame still samples the backdrop — the frosted root plate reads it,
    /// zeroed — and a zeroed 1×1 image reads the same under the UI's
    /// clamp-to-edge sampler at normalized coordinates. The first
    /// `stage_scene` / `stage_rt` grows them (`VkRenderer::want_scene_targets`).
    pub(crate) fn target_extent(&self, surface: vk::Extent2D) -> vk::Extent2D {
        if self.wanted {
            surface
        } else {
            vk::Extent2D { width: 1, height: 1 }
        }
    }

    fn destroy_targets(&mut self, device: &ash::Device, allocator: &mut Allocator) {
        unsafe {
            if self.framebuffer != vk::Framebuffer::null() {
                device.destroy_framebuffer(self.framebuffer, None);
                self.framebuffer = vk::Framebuffer::null();
            }
            if self.backdrop_view != vk::ImageView::null() {
                device.destroy_image_view(self.backdrop_view, None);
                device.destroy_image(self.backdrop_image, None);
                self.backdrop_view = vk::ImageView::null();
                self.backdrop_image = vk::Image::null();
            }
            if self.depth_view != vk::ImageView::null() {
                device.destroy_image_view(self.depth_view, None);
                device.destroy_image(self.depth_image, None);
                self.depth_view = vk::ImageView::null();
                self.depth_image = vk::Image::null();
            }
        }
        if let Some(a) = self.backdrop_allocation.take() {
            let _ = allocator.free(a);
        }
        if let Some(a) = self.depth_allocation.take() {
            let _ = allocator.free(a);
        }
    }

    /// (Re)create the backdrop + depth targets at `extent`. Caller must have the
    /// device idle (the renderer's swapchain-rebuild path guarantees it) and must
    /// re-point the UI descriptor at the new `backdrop_view` and re-init its layout.
    ///
    /// Returns whether the targets were recreated. At an unchanged extent they
    /// are kept, CONTENTS included: the backdrop still holds the last staged
    /// scene and `backdrop_valid` still says so, and an app that stages only
    /// when its scene changes will not stage again to repair it.
    pub(crate) fn resize(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        extent: vk::Extent2D,
    ) -> bool {
        if extent == self.extent && self.framebuffer != vk::Framebuffer::null() {
            return false;
        }
        self.destroy_targets(device, allocator);
        self.extent = extent;
        self.backdrop_valid = false;
        unsafe {
            let backdrop_image = device
                .create_image(
                    &vk::ImageCreateInfo::default()
                        .image_type(vk::ImageType::TYPE_2D)
                        .format(self.format)
                        .extent(vk::Extent3D {
                            width: extent.width,
                            height: extent.height,
                            depth: 1,
                        })
                        .mip_levels(1)
                        .array_layers(1)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .tiling(vk::ImageTiling::OPTIMAL)
                        .usage(
                            vk::ImageUsageFlags::COLOR_ATTACHMENT
                                | vk::ImageUsageFlags::SAMPLED
                                | vk::ImageUsageFlags::TRANSFER_SRC
                                | vk::ImageUsageFlags::TRANSFER_DST,
                        )
                        .initial_layout(vk::ImageLayout::UNDEFINED),
                    None,
                )
                .expect("Failed to create backdrop image");
            let requirements = device.get_image_memory_requirements(backdrop_image);
            let allocation = allocator
                .allocate(&AllocationCreateDesc {
                    name: "backdrop",
                    requirements,
                    location: MemoryLocation::GpuOnly,
                    linear: false,
                    allocation_scheme: AllocationScheme::GpuAllocatorManaged,
                })
                .expect("Failed to allocate backdrop memory");
            device
                .bind_image_memory(backdrop_image, allocation.memory(), allocation.offset())
                .expect("Failed to bind backdrop memory");
            let backdrop_view = device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(backdrop_image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(self.format)
                        .subresource_range(
                            vk::ImageSubresourceRange::default()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .level_count(1)
                                .layer_count(1),
                        ),
                    None,
                )
                .expect("Failed to create backdrop view");
            self.backdrop_image = backdrop_image;
            self.backdrop_view = backdrop_view;
            self.backdrop_allocation = Some(allocation);

            let depth_image = device
                .create_image(
                    &vk::ImageCreateInfo::default()
                        .image_type(vk::ImageType::TYPE_2D)
                        .format(vk::Format::D32_SFLOAT)
                        .extent(vk::Extent3D {
                            width: extent.width,
                            height: extent.height,
                            depth: 1,
                        })
                        .mip_levels(1)
                        .array_layers(1)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .tiling(vk::ImageTiling::OPTIMAL)
                        .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
                        .initial_layout(vk::ImageLayout::UNDEFINED),
                    None,
                )
                .expect("Failed to create depth image");
            let requirements = device.get_image_memory_requirements(depth_image);
            let allocation = allocator
                .allocate(&AllocationCreateDesc {
                    name: "depth",
                    requirements,
                    location: MemoryLocation::GpuOnly,
                    linear: false,
                    allocation_scheme: AllocationScheme::GpuAllocatorManaged,
                })
                .expect("Failed to allocate depth memory");
            device
                .bind_image_memory(depth_image, allocation.memory(), allocation.offset())
                .expect("Failed to bind depth memory");
            let depth_view = device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(depth_image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(vk::Format::D32_SFLOAT)
                        .subresource_range(
                            vk::ImageSubresourceRange::default()
                                .aspect_mask(vk::ImageAspectFlags::DEPTH)
                                .level_count(1)
                                .layer_count(1),
                        ),
                    None,
                )
                .expect("Failed to create depth view");
            self.depth_image = depth_image;
            self.depth_view = depth_view;
            self.depth_allocation = Some(allocation);

            let attachments = [self.backdrop_view, self.depth_view];
            self.framebuffer = device
                .create_framebuffer(
                    &vk::FramebufferCreateInfo::default()
                        .render_pass(self.render_pass)
                        .attachments(&attachments)
                        .width(extent.width)
                        .height(extent.height)
                        .layers(1),
                    None,
                )
                .expect("Failed to create scene framebuffer");
        }
        true
    }

    pub(crate) fn create_mesh(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        verts: &[Vertex3D],
    ) -> MeshId {
        let bytes: &[u8] = bytemuck::cast_slice(verts);
        let mut buffer = create_cpu_buffer(
            device,
            allocator,
            (bytes.len() as vk::DeviceSize).max(64),
            vk::BufferUsageFlags::VERTEX_BUFFER,
            "mesh",
        );
        if !bytes.is_empty() {
            buffer.allocation.as_mut().unwrap().mapped_slice_mut().unwrap()[..bytes.len()]
                .copy_from_slice(bytes);
        }
        self.meshes.push(Mesh { buffer, count: verts.len() as u32 });
        MeshId(self.meshes.len() - 1)
    }

    /// Replace a mesh's vertices. Caller must have the device idle: meshes may be
    /// referenced by in-flight frames (geometry updates are rare — settings
    /// changes and graph rebuilds — so a wait is acceptable here).
    #[allow(dead_code)] // cutover API: the app's rebuild_scene_geometry path
    pub(crate) fn update_mesh(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        id: MeshId,
        verts: &[Vertex3D],
    ) {
        let mesh = &mut self.meshes[id.0];
        let bytes: &[u8] = bytemuck::cast_slice(verts);
        let needed = bytes.len() as vk::DeviceSize;
        if needed > mesh.buffer.size {
            let mut old = std::mem::replace(&mut mesh.buffer, AllocatedBuffer::null());
            destroy_cpu_buffer(device, allocator, &mut old);
            mesh.buffer = create_cpu_buffer(
                device,
                allocator,
                needed.next_power_of_two(),
                vk::BufferUsageFlags::VERTEX_BUFFER,
                "mesh",
            );
        }
        if !bytes.is_empty() {
            mesh.buffer.allocation.as_mut().unwrap().mapped_slice_mut().unwrap()[..bytes.len()]
                .copy_from_slice(bytes);
        }
        mesh.count = verts.len() as u32;
    }

    pub(crate) fn stage(&mut self, scissor: (u32, u32, u32, u32), draws: Vec<SceneDraw>) {
        self.staged = Some(StagedScene { scissor, draws, images: Vec::new() });
    }

    /// The staged scene's images; nothing when no scene is staged.
    pub(crate) fn stage_images(&mut self, images: Vec<SceneImage>) {
        if let Some(staged) = &mut self.staged {
            staged.images = images;
        }
    }

    /// After the frame fence: write this frame's per-draw uniforms (mvp + the
    /// window-corner info shader_3d shares with the 2D shader).
    pub(crate) fn write_frame_uniforms(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        frame_index: usize,
        corner_radius_px: f32,
    ) {
        let Some(staged) = &self.staged else {
            self.frames[frame_index].draw_count = 0;
            return;
        };
        let frame = &mut self.frames[frame_index];
        // One slot per mesh draw, then one per image.
        let slots = staged.draws.len() + staged.images.len();
        let needed = self.uniform_stride * slots.max(1) as vk::DeviceSize;
        if needed > frame.uniforms.size {
            let mut old = std::mem::replace(&mut frame.uniforms, AllocatedBuffer::null());
            destroy_cpu_buffer(device, allocator, &mut old);
            frame.uniforms = create_cpu_buffer(
                device,
                allocator,
                needed.next_power_of_two(),
                vk::BufferUsageFlags::UNIFORM_BUFFER,
                "scene-uniforms",
            );
            Self::write_descriptor(device, frame);
        }
        let window_size = [self.extent.width as f32, self.extent.height as f32];
        let mapped = frame.uniforms.allocation.as_mut().unwrap().mapped_slice_mut().unwrap();
        let blocks = scene_uniforms(&staged.draws, &staged.images, window_size, corner_radius_px, self.light);
        for (i, uniforms) in blocks.iter().enumerate() {
            let offset = (self.uniform_stride as usize) * i;
            mapped[offset..offset + UNIFORM_SIZE as usize].copy_from_slice(bytemuck::bytes_of(uniforms));
        }
        frame.draw_count = staged.draws.len() as u32;

        let verts = image_quads_3d(&staged.images);
        let bytes: &[u8] = bytemuck::cast_slice(&verts);
        if bytes.len() as vk::DeviceSize > frame.image_verts.size {
            let mut old = std::mem::replace(&mut frame.image_verts, AllocatedBuffer::null());
            destroy_cpu_buffer(device, allocator, &mut old);
            frame.image_verts = create_cpu_buffer(
                device,
                allocator,
                (bytes.len() as vk::DeviceSize).next_power_of_two(),
                vk::BufferUsageFlags::VERTEX_BUFFER,
                "scene-image-quads",
            );
        }
        if !bytes.is_empty() {
            frame.image_verts.allocation.as_mut().unwrap().mapped_slice_mut().unwrap()
                [..bytes.len()]
                .copy_from_slice(bytes);
        }
    }

    /// Record the offscreen scene pass. Consumes the staged scene; afterwards the
    /// backdrop is in TRANSFER_SRC layout, ready for the swapchain copy. Returns
    /// false if nothing was staged.
    pub(crate) fn record(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame_index: usize,
        images: &super::image::ImageStage,
    ) -> bool {
        let Some(staged) = self.staged.take() else {
            return false;
        };
        let frame = &self.frames[frame_index];
        unsafe {
            let clear_values = [
                vk::ClearValue { color: vk::ClearColorValue { float32: [0.0, 0.0, 0.0, 0.0] } },
                vk::ClearValue {
                    depth_stencil: vk::ClearDepthStencilValue { depth: 1.0, stencil: 0 },
                },
            ];
            device.cmd_begin_render_pass(
                cmd,
                &vk::RenderPassBeginInfo::default()
                    .render_pass(self.render_pass)
                    .framebuffer(self.framebuffer)
                    .render_area(vk::Rect2D {
                        offset: vk::Offset2D { x: 0, y: 0 },
                        extent: self.extent,
                    })
                    .clear_values(&clear_values),
                vk::SubpassContents::INLINE,
            );
            // Negative-height viewport: wgpu's Y-up NDC without touching winding.
            device.cmd_set_viewport(
                cmd,
                0,
                &[vk::Viewport {
                    x: 0.0,
                    y: self.extent.height as f32,
                    width: self.extent.width as f32,
                    height: -(self.extent.height as f32),
                    min_depth: 0.0,
                    max_depth: 1.0,
                }],
            );
            let (sx, sy, sw, sh) = staged.scissor;
            let sx = sx.min(self.extent.width);
            let sy = sy.min(self.extent.height);
            device.cmd_set_scissor(
                cmd,
                0,
                &[vk::Rect2D {
                    offset: vk::Offset2D { x: sx as i32, y: sy as i32 },
                    extent: vk::Extent2D {
                        width: sw.min(self.extent.width - sx),
                        height: sh.min(self.extent.height - sy),
                    },
                }],
            );
            let mut bound = self.pipeline;
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, bound);
            // The images due before mesh draw `at` (or, past the last draw,
            // every one left), in staged order.
            let draw_images = |at: usize, bound: &mut vk::Pipeline| {
                for (j, image) in staged.images.iter().enumerate() {
                    let due = (image.before as usize).min(staged.draws.len());
                    if due != at {
                        continue;
                    }
                    let Some(set) = images.descriptor_set(image.image) else { continue };
                    if *bound != self.image_pipeline {
                        device.cmd_bind_pipeline(
                            cmd,
                            vk::PipelineBindPoint::GRAPHICS,
                            self.image_pipeline,
                        );
                        *bound = self.image_pipeline;
                    }
                    device.cmd_bind_descriptor_sets(
                        cmd,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.image_pipeline_layout,
                        0,
                        &[frame.descriptor_set],
                        &[(self.uniform_stride as u32) * (staged.draws.len() + j) as u32],
                    );
                    device.cmd_bind_descriptor_sets(
                        cmd,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.image_pipeline_layout,
                        1,
                        &[set],
                        &[],
                    );
                    device.cmd_bind_vertex_buffers(cmd, 0, &[frame.image_verts.buffer], &[0]);
                    device.cmd_draw(cmd, 6, 1, (j * 6) as u32, 0);
                }
            };
            for (i, draw) in staged.draws.iter().enumerate() {
                draw_images(i, &mut bound);
                let mesh = &self.meshes[draw.mesh.0];
                if mesh.count == 0 {
                    continue;
                }
                // What it is drawn for: an instance mesh, or the one unit
                // instance that leaves it as it is.
                let (instance_buffer, instance_count) = match draw.instances {
                    Some(id) => {
                        let instances = &self.meshes[id.0];
                        (instances.buffer.buffer, instances.count)
                    }
                    None => (self.unit_instance.buffer, 1),
                };
                if instance_count == 0 {
                    continue;
                }
                let wanted = if draw.wireframe && draw.see_through {
                    self.wireframe_see_through_pipeline
                } else if draw.wireframe {
                    self.wireframe_pipeline.unwrap_or(self.pipeline)
                } else if draw.see_through {
                    self.see_through_pipeline
                } else {
                    self.pipeline
                };
                if wanted != bound {
                    device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, wanted);
                    bound = wanted;
                }
                if draw.wireframe && (draw.see_through || self.wireframe_pipeline.is_some()) {
                    device.cmd_set_line_width(cmd, draw.line_width.clamp(1.0, self.max_line_width));
                    device.cmd_set_depth_bias(cmd, 0.0, 0.0, 0.0);
                } else if draw.wire_base_width > 0.0 {
                    // Push this fill behind its coming wire overlay (see
                    // `wire_base_bias`).
                    let w = draw.wire_base_width.clamp(1.0, self.max_line_width);
                    let (constant, slope) = wire_base_bias(w);
                    device.cmd_set_depth_bias(cmd, constant, 0.0, slope);
                } else {
                    device.cmd_set_depth_bias(cmd, 0.0, 0.0, 0.0);
                }
                device.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.pipeline_layout,
                    0,
                    &[frame.descriptor_set],
                    &[(self.uniform_stride as u32) * i as u32],
                );
                device.cmd_bind_vertex_buffers(cmd, 0, &[mesh.buffer.buffer, instance_buffer], &[0, 0]);
                device.cmd_draw(cmd, mesh.count, instance_count, 0, 0);
            }
            draw_images(staged.draws.len(), &mut bound);
            device.cmd_end_render_pass(cmd);
        }
        self.backdrop_valid = true;
        true
    }

    pub(crate) fn destroy(&mut self, device: &ash::Device, allocator: &mut Allocator) {
        self.destroy_targets(device, allocator);
        unsafe {
            for frame in &mut self.frames {
                let mut uniforms = std::mem::replace(&mut frame.uniforms, AllocatedBuffer::null());
                destroy_cpu_buffer(device, allocator, &mut uniforms);
                let mut quads = std::mem::replace(&mut frame.image_verts, AllocatedBuffer::null());
                destroy_cpu_buffer(device, allocator, &mut quads);
            }
            for mesh in &mut self.meshes {
                let mut buffer = std::mem::replace(&mut mesh.buffer, AllocatedBuffer::null());
                destroy_cpu_buffer(device, allocator, &mut buffer);
            }
            let mut unit = std::mem::replace(&mut self.unit_instance, AllocatedBuffer::null());
            destroy_cpu_buffer(device, allocator, &mut unit);
            device.destroy_descriptor_pool(self.descriptor_pool, None);
            device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            if let Some(p) = self.wireframe_pipeline.take() {
                device.destroy_pipeline(p, None);
            }
            device.destroy_pipeline(self.image_pipeline, None);
            device.destroy_pipeline_layout(self.image_pipeline_layout, None);
            device.destroy_descriptor_set_layout(self.image_set_layout, None);
            device.destroy_shader_module(self.image_shader_module, None);
            device.destroy_pipeline(self.see_through_pipeline, None);
            device.destroy_pipeline(self.wireframe_see_through_pipeline, None);
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.pipeline_layout, None);
            device.destroy_shader_module(self.shader_module, None);
            device.destroy_render_pass(self.render_pass, None);
        }
    }
}
