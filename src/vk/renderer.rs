//! The ash renderer. One graphics queue, a classic render pass, two frames in
//! flight, FIFO (vsync) presentation. Memory goes through gpu-allocator; the
//! descriptor set mirrors `shader.wgsl`'s @group(0): sampled backdrop texture
//! (binding 0), sampler (binding 1), WindowInfo uniform (binding 2). Binding 0
//! is the scene backdrop until the first blur-behind plate: there the UI pass
//! suspends, the frame-so-far is copied into the snapshot image, and the
//! snapshot descriptor set takes over — so blur plates blur everything painted
//! beneath them, not just the 3D scene.

use std::ffi::c_void;

use ash::vk;
use gpu_allocator::vulkan::{
    Allocation, AllocationCreateDesc, AllocationScheme, Allocator,
};
use gpu_allocator::MemoryLocation;

use crate::engine::Vertex;

use super::core::SurfaceLost;
use super::image::ImageStage;
pub use crate::draw::{Batch2D, Frame2D, PlatePush, MAX_PLATE_FEATURES};
pub(crate) use crate::draw::{batch_push_constants, PUSH_CONSTANT_FLOATS};
use super::rt::{RtCamera, RtEnvironment, RtImage, RtImageSource, RtMaterial, RtStage, RtTriangle};
use super::scene::{MeshId, SceneDraw, SceneImage, SceneStage, Vertex3D};
use super::text::{TextSpan, TextStage};

/// The block in bytes. **This is exactly `maxPushConstantsSize`'s
/// Vulkan-guaranteed minimum, so the budget is full** — every one of the 32
/// slots is written. That is why a new SDF mode reinterprets existing fields per
/// mode (5 reads `p_rect` as centre + radius, 6/7 as centre + radius + wedge
/// angle, 8 as centre + half-width with `p_radii.xy` a normal) instead of adding
/// one: there is nothing left to add.
///
/// A block over 128 bytes is not portable by construction — 128 is the floor
/// every conformant implementation must offer, and plenty of drivers offer no
/// more. So growing this means querying `limits.max_push_constants_size` at
/// device init and having a real fallback (a uniform buffer, or splitting the
/// block), not just raising the number. The assertion below is the tripwire: a
/// runtime check would be dead code today, because at exactly 128 it can never
/// fire on a conformant device.
pub(crate) const PUSH_CONSTANT_BYTES: u32 = (PUSH_CONSTANT_FLOATS * 4) as u32;

const _: () = assert!(
    PUSH_CONSTANT_BYTES <= 128,
    "the push-constant block has outgrown the 128-byte Vulkan-guaranteed minimum: \
     query limits.max_push_constants_size at device init and add a fallback path \
     before raising PUSH_CONSTANT_FLOATS"
);

/// How far a swapchain image's pixels are behind the latest frame. A frame
/// with [`Frame2D::damage`] repaints only what the image it acquired is
/// missing — the frame's own damage plus whatever frames that went to the
/// OTHER images changed in the meantime — instead of every pixel.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ImageAge {
    /// Never rendered, or a full-surface frame went by: repaint everything.
    Unknown,
    /// Holds the latest frame.
    Current,
    /// Holds the latest frame except inside this rect.
    Behind(vk::Rect2D),
}

fn rect_union(a: vk::Rect2D, b: vk::Rect2D) -> vk::Rect2D {
    if a.extent.width == 0 || a.extent.height == 0 {
        return b;
    }
    if b.extent.width == 0 || b.extent.height == 0 {
        return a;
    }
    let x0 = a.offset.x.min(b.offset.x);
    let y0 = a.offset.y.min(b.offset.y);
    let x1 = (a.offset.x + a.extent.width as i32).max(b.offset.x + b.extent.width as i32);
    let y1 = (a.offset.y + a.extent.height as i32).max(b.offset.y + b.extent.height as i32);
    vk::Rect2D {
        offset: vk::Offset2D { x: x0, y: y0 },
        extent: vk::Extent2D { width: (x1 - x0) as u32, height: (y1 - y0) as u32 },
    }
}

/// `a` cut down to `b`; zero-sized (a legal scissor that draws nothing) when
/// they do not meet.
fn rect_intersect(a: vk::Rect2D, b: vk::Rect2D) -> vk::Rect2D {
    let x0 = a.offset.x.max(b.offset.x);
    let y0 = a.offset.y.max(b.offset.y);
    let x1 = (a.offset.x + a.extent.width as i32).min(b.offset.x + b.extent.width as i32);
    let y1 = (a.offset.y + a.extent.height as i32).min(b.offset.y + b.extent.height as i32);
    vk::Rect2D {
        offset: vk::Offset2D { x: x0, y: y0 },
        extent: vk::Extent2D {
            width: (x1 - x0).max(0) as u32,
            height: (y1 - y0).max(0) as u32,
        },
    }
}

pub(crate) const FRAMES_IN_FLIGHT: usize = 2;
pub(crate) use crate::draw::PLATE_FEATURE_BYTES;
/// shader2d's WindowInfo uniform, in bytes (layout in `draw::window_info_data`).
pub(crate) const WINDOW_INFO_BYTES: vk::DeviceSize = crate::draw::WINDOW_INFO_BYTES as vk::DeviceSize;
pub(crate) use crate::draw::relief_px_at;

/// [`crate::draw::window_info_data`] for a Vulkan extent.
pub(crate) fn window_info_data(extent: vk::Extent2D, clip_corner_radius: f32, relief: (f32, f32)) -> [f32; crate::draw::WINDOW_INFO_BYTES / 4] {
    crate::draw::window_info_data(extent.width, extent.height, clip_corner_radius, relief)
}

pub(crate) struct AllocatedBuffer {
    pub(crate) buffer: vk::Buffer,
    pub(crate) allocation: Option<Allocation>,
    pub(crate) size: vk::DeviceSize,
}

impl AllocatedBuffer {
    pub(crate) fn null() -> Self {
        AllocatedBuffer { buffer: vk::Buffer::null(), allocation: None, size: 0 }
    }
}

/// Create a host-visible buffer bound to gpu-allocator memory.
pub(crate) fn create_cpu_buffer(
    device: &ash::Device,
    allocator: &mut Allocator,
    size: vk::DeviceSize,
    usage: vk::BufferUsageFlags,
    name: &str,
) -> AllocatedBuffer {
    unsafe {
        let buffer = device
            .create_buffer(
                &vk::BufferCreateInfo::default()
                    .size(size)
                    .usage(usage)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE),
                None,
            )
            .expect("Failed to create buffer");
        let requirements = device.get_buffer_memory_requirements(buffer);
        let allocation = allocator
            .allocate(&AllocationCreateDesc {
                name,
                requirements,
                location: MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .expect("Failed to allocate buffer memory");
        device
            .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
            .expect("Failed to bind buffer memory");
        AllocatedBuffer { buffer, allocation: Some(allocation), size }
    }
}

/// Destroy a buffer and return its memory to the allocator.
pub(crate) fn destroy_cpu_buffer(
    device: &ash::Device,
    allocator: &mut Allocator,
    buf: &mut AllocatedBuffer,
) {
    unsafe {
        self::destroy_buffer_handle(device, buf.buffer);
    }
    if let Some(allocation) = buf.allocation.take() {
        let _ = allocator.free(allocation);
    }
    buf.buffer = vk::Buffer::null();
    buf.size = 0;
}

unsafe fn destroy_buffer_handle(device: &ash::Device, buffer: vk::Buffer) {
    if buffer != vk::Buffer::null() {
        device.destroy_buffer(buffer, None);
    }
}

struct Frame {
    cmd: vk::CommandBuffer,
    image_available: vk::Semaphore,
    in_flight: vk::Fence,
    vertex: AllocatedBuffer,
    vertex_count: u32,
    overlay_start: u32,
    overlay_count: u32,
}

pub struct VkRenderer {
    surface: vk::SurfaceKHR,

    swapchain_loader: ash::khr::swapchain::Device,
    swapchain: vk::SwapchainKHR,
    surface_format: vk::SurfaceFormatKHR,
    extent: vk::Extent2D,
    swapchain_images: Vec<vk::Image>,
    swapchain_views: Vec<vk::ImageView>,
    framebuffers: Vec<vk::Framebuffer>,
    // One per swapchain image (not per frame in flight): present waits on the
    // semaphore tied to the image being presented.
    render_finished: Vec<vk::Semaphore>,

    render_pass: vk::RenderPass,
    /// UI pass over a backdrop copy: loadOp LOAD, initial layout TRANSFER_DST.
    /// Framebuffers are shared with `render_pass` (compatible attachments).
    render_pass_load: vk::RenderPass,
    /// LOAD from PRESENT_SRC: a partial frame repaints inside a swapchain
    /// image that still holds an earlier frame.
    render_pass_partial: vk::RenderPass,
    /// Per swapchain image, what it is missing; reset with the swapchain.
    image_ages: Vec<ImageAge>,
    descriptor_set_layout: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    shader_module: vk::ShaderModule,

    descriptor_pool: vk::DescriptorPool,
    descriptor_set: vk::DescriptorSet,
    /// Twin of `descriptor_set` with binding 0 pointing at `snapshot_image`
    /// instead of the scene backdrop; bound for every draw after the first
    /// mid-pass snapshot so blur plates sample the frame-so-far.
    descriptor_set_snapshot: vk::DescriptorSet,
    /// Mid-frame copy target for blur-behind plates: the swapchain content so
    /// far, sampled by the resumed pass's blur draws. Sized with the surface.
    snapshot_image: vk::Image,
    snapshot_view: vk::ImageView,
    snapshot_allocation: Option<Allocation>,
    backdrop_sampler: vk::Sampler,
    window_info: AllocatedBuffer,
    /// The bevel-profile generation `window_info` was last written with —
    /// `draw_frame_2d` rewrites the UBO when the layout global moves on.
    profile_gen: u64,
    /// The pinned relief heights (carve drop, roll rise) in physical px as
    /// last uploaded in WindowInfo — compared each frame, since editors set
    /// them straight into the style registry with no generation counter.
    relief_uploaded: (f32, f32),
    /// Same for the edge (roll) profile LUT.
    roll_profile_gen: u64,
    plate_features: AllocatedBuffer,

    frames: Vec<Frame>,
    frame_index: usize,
    text: TextStage,
    scene: SceneStage,
    image: ImageStage,
    /// Built lazily on the first `set_rt_scene`, so ordinary UI apps never
    /// compile the path-tracer pipeline.
    rt: Option<RtStage>,
    /// See [`VkRenderer::set_rt_background`].
    rt_background: Option<[f32; 3]>,
    /// See [`VkRenderer::set_rt_environment`].
    rt_environment: RtEnvironment,

    desired_extent: vk::Extent2D,
    corner_radius_px: f32,
    swapchain_dirty: bool,
    present_mode: vk::PresentModeKHR,
    present_debug_count: u64,
    /// Set when a surface call reports the surface lost (see [`SurfaceLost`]):
    /// the display connection is dead, so every later draw is skipped until
    /// a new surface is attached, rather than re-failing (and re-logging)
    /// each frame while the caller's event loop finds out for itself.
    surface_lost: bool,

    // Declared last: everything above must be destroyed before the device/
    // instance the core tears down in its own Drop.
    core: super::core::VkCore,
}

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

pub(crate) fn compile_wgsl(source: &str) -> Vec<u32> {
    let module = naga::front::wgsl::parse_str(source).expect("WGSL parse failed");
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::PUSH_CONSTANT,
    )
    .validate(&module)
    .expect("WGSL validation failed");
    let options = naga::back::spv::Options {
        lang_version: (1, 0),
        flags: naga::back::spv::WriterFlags::LABEL_VARYINGS,
        ..Default::default()
    };
    naga::back::spv::write_vec(&module, &info, &options, None).expect("SPIR-V write failed")
}

/// Cached SPIR-V for the always-compiled UI shaders. The daemon-style
/// consumers (cce-cloud) build a renderer per popup; naga compilation is pure,
/// so compile each shader once per process.
pub(crate) fn shader2d_spirv() -> &'static [u32] {
    static SPIRV: std::sync::OnceLock<Vec<u32>> = std::sync::OnceLock::new();
    SPIRV.get_or_init(|| compile_wgsl(crate::draw::shaders::SHADER2D))
}

pub(crate) fn glyph_spirv() -> &'static [u32] {
    static SPIRV: std::sync::OnceLock<Vec<u32>> = std::sync::OnceLock::new();
    SPIRV.get_or_init(|| compile_wgsl(crate::draw::shaders::GLYPH))
}

pub(crate) fn scene3d_spirv() -> &'static [u32] {
    static SPIRV: std::sync::OnceLock<Vec<u32>> = std::sync::OnceLock::new();
    SPIRV.get_or_init(|| compile_wgsl(crate::draw::shaders::SCENE3D))
}

pub(crate) fn scene3d_image_spirv() -> &'static [u32] {
    static SPIRV: std::sync::OnceLock<Vec<u32>> = std::sync::OnceLock::new();
    SPIRV.get_or_init(|| compile_wgsl(crate::draw::shaders::SCENE3D_IMAGE))
}

/// Like [`compile_wgsl`], but with naga's RAY_QUERY capability and SPIR-V 1.4
/// (required by SPV_KHR_ray_query). Only used on devices where the ray-query
/// device stack was enabled — those are Vulkan 1.2+, which accepts 1.4.
pub(crate) fn compile_wgsl_ray_query(source: &str) -> Vec<u32> {
    let module = naga::front::wgsl::parse_str(source).expect("WGSL parse failed");
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::RAY_QUERY,
    )
    .validate(&module)
    .expect("WGSL validation failed");
    let options = naga::back::spv::Options {
        lang_version: (1, 4),
        flags: naga::back::spv::WriterFlags::LABEL_VARYINGS,
        ..Default::default()
    };
    naga::back::spv::write_vec(&module, &info, &options, None).expect("SPIR-V write failed")
}

const COLOR_RANGE: vk::ImageSubresourceRange = vk::ImageSubresourceRange {
    aspect_mask: vk::ImageAspectFlags::COLOR,
    base_mip_level: 0,
    level_count: 1,
    base_array_layer: 0,
    layer_count: 1,
};

/// One-time submit: clear a color image and leave it in SHADER_READ_ONLY, so a
/// freshly created backdrop is always legal to sample.
pub(crate) fn clear_image_to_shader_read(
    device: &ash::Device,
    queue: vk::Queue,
    command_pool: vk::CommandPool,
    image: vk::Image,
) {
    unsafe {
        let cmd = device
            .allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(command_pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )
            .expect("Failed to allocate init command buffer")[0];
        device
            .begin_command_buffer(
                cmd,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )
            .unwrap();
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(COLOR_RANGE)],
        );
        device.cmd_clear_color_image(
            cmd,
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &vk::ClearColorValue { float32: [0.0, 0.0, 0.0, 0.0] },
            &[COLOR_RANGE],
        );
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(COLOR_RANGE)],
        );
        device.end_command_buffer(cmd).unwrap();
        let cmds = [cmd];
        let submit = vk::SubmitInfo::default().command_buffers(&cmds);
        device
            .queue_submit(queue, &[submit], vk::Fence::null())
            .expect("Init submit failed");
        device.queue_wait_idle(queue).expect("Init wait failed");
        device.free_command_buffers(command_pool, &cmds);
    }
}

/// The wgpu-convention viewport: Y flipped via negative height (Vulkan >= 1.1).
pub(crate) fn flipped_viewport(extent: vk::Extent2D) -> vk::Viewport {
    vk::Viewport {
        x: 0.0,
        y: extent.height as f32,
        width: extent.width as f32,
        height: -(extent.height as f32),
        min_depth: 0.0,
        max_depth: 1.0,
    }
}


impl VkRenderer {
    /// A renderer presenting to `surface_ptr`, or [`SurfaceLost`] when the
    /// display connection under it is already dead — which is what a window
    /// requested as the compositor goes away gets. The caller should treat
    /// that as its connection ending (the runner does), not retry here.
    ///
    /// # Safety
    /// `display_ptr` and `surface_ptr` must be live `wl_display` / `wl_surface`
    /// pointers that outlive the renderer.
    pub unsafe fn try_new(
        display_ptr: *mut c_void,
        surface_ptr: *mut c_void,
        width: u32,
        height: u32,
        corner_radius_px: f32,
    ) -> Result<Self, SurfaceLost> {
        Self::try_new_for(
            super::core::SurfaceTarget::Wayland { display: display_ptr, surface: surface_ptr },
            width,
            height,
            corner_radius_px,
        )
    }

    /// A renderer presenting to any window [`SurfaceTarget`](super::core::SurfaceTarget)
    /// names — a Wayland surface, or on macOS a `CAMetalLayer` — on the same
    /// terms as [`try_new`](Self::try_new).
    ///
    /// # Safety
    /// The target's pointers must be live and outlive the renderer.
    pub unsafe fn try_new_for(
        target: super::core::SurfaceTarget,
        width: u32,
        height: u32,
        corner_radius_px: f32,
    ) -> Result<Self, SurfaceLost> {
        let t_new = std::time::Instant::now();
        let (mut core, surface) = super::core::VkCore::new_for_surface(target)?;
        log::debug!("[timing] VkCore::new_for_surface: {:?}", t_new.elapsed());
        let t_rest = std::time::Instant::now();
        // Locals over the core for the setup below (methods use self.core.*).
        let device = core.device.clone();
        let queue = core.queue;
        let command_pool = core.command_pool;
        let physical_device = core.physical_device;
        let min_uniform_align = core.min_uniform_align;
        let surface_loader = core.surface_loader.clone();

        // Surface format: prefer sRGB (wgpu's get_default_config sorts sRGB first,
        // so this matches the colors the app renders today). The first query
        // that talks to the compositor, so the one a dead connection fails.
        let formats = match surface_loader
            .get_physical_device_surface_formats(physical_device, surface)
        {
            Ok(formats) if !formats.is_empty() => formats,
            Ok(_) => panic!("surface offers no formats"),
            Err(result) => {
                surface_loader.destroy_surface(surface, None);
                return Err(SurfaceLost { call: "vkGetPhysicalDeviceSurfaceFormatsKHR", result });
            }
        };
        let allocator = core.allocator.as_mut().unwrap();
        let surface_format = formats
            .iter()
            .copied()
            .find(|f| {
                (f.format == vk::Format::B8G8R8A8_SRGB || f.format == vk::Format::R8G8B8A8_SRGB)
                    && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
            })
            .unwrap_or(formats[0]);

        // Render pass: one color attachment, clear -> present.
        let attachments = [vk::AttachmentDescription::default()
            .format(surface_format.format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)];
        let color_refs = [vk::AttachmentReference::default()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
        let subpasses = [vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_refs)];
        // One dependency shared VERBATIM by both UI pass variants: framebuffer
        // compatibility requires identical dependencies (only load/store ops and
        // image layouts may differ), so this unions the clear case (previous
        // frame's color output) with the load case (the backdrop copy's write).
        let dependencies = [vk::SubpassDependency::default()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags::TRANSFER,
            )
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(
                vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            )];
        let render_pass = device
            .create_render_pass(
                &vk::RenderPassCreateInfo::default()
                    .attachments(&attachments)
                    .subpasses(&subpasses)
                    .dependencies(&dependencies),
                None,
            )
            .expect("Failed to create render pass");

        // Variant used when a backdrop copy precedes the UI pass: keep the copied
        // pixels (LOAD) and take the image from the copy's TRANSFER_DST layout.
        let attachments_load = [vk::AttachmentDescription::default()
            .format(surface_format.format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)];
        let render_pass_load = device
            .create_render_pass(
                &vk::RenderPassCreateInfo::default()
                    .attachments(&attachments_load)
                    .subpasses(&subpasses)
                    .dependencies(&dependencies),
                None,
            )
            .expect("Failed to create load render pass");

        // Variant for a partial frame: keep what the image already shows and
        // repaint inside the damage only. A presented image comes back from
        // acquire in PRESENT_SRC with its contents intact.
        let attachments_partial = [vk::AttachmentDescription::default()
            .format(surface_format.format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)];
        let render_pass_partial = device
            .create_render_pass(
                &vk::RenderPassCreateInfo::default()
                    .attachments(&attachments_partial)
                    .subpasses(&subpasses)
                    .dependencies(&dependencies),
                None,
            )
            .expect("Failed to create partial render pass");

        let (descriptor_set_layout, pipeline_layout, shader_module, pipeline) =
            create_ui_pipeline(&device, render_pass);

        // Full-size backdrop + depth live in the scene stage: the 3D pass renders
        // into the backdrop, and the UI pass samples it for blur-behind plates.
        let initial_extent = vk::Extent2D { width: width.max(1), height: height.max(1) };
        let scene = SceneStage::new(
            &device,
            allocator,
            surface_format.format,
            initial_extent,
            FRAMES_IN_FLIGHT,
            min_uniform_align,
            core.max_line_width,
        );
        clear_image_to_shader_read(&device, queue, command_pool, scene.backdrop_image);

        // Matches the wgpu backdrop sampler: linear, clamp-to-edge.
        let backdrop_sampler = device
            .create_sampler(
                &vk::SamplerCreateInfo::default()
                    .mag_filter(vk::Filter::LINEAR)
                    .min_filter(vk::Filter::LINEAR)
                    .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                    .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                None,
            )
            .expect("Failed to create sampler");

        let window_info = create_cpu_buffer(
            &device,
            allocator,
            WINDOW_INFO_BYTES,
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            "window-info",
        );
        // Plate-carve features, one MAX_PLATE_FEATURES slot per frame in
        // flight so a write never races the previous frame's reads.
        let plate_features = create_cpu_buffer(
            &device,
            allocator,
            (FRAMES_IN_FLIGHT * MAX_PLATE_FEATURES * PLATE_FEATURE_BYTES) as vk::DeviceSize,
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            "plate-features",
        );

        // Two sets: the scene-backdrop set and its snapshot twin (binding 0
        // differs; 1-3 alias the same sampler/uniforms).
        let pool_sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(2),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLER)
                .descriptor_count(2),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(4),
        ];
        let descriptor_pool = device
            .create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(2)
                    .pool_sizes(&pool_sizes),
                None,
            )
            .expect("Failed to create descriptor pool");
        let both_layouts = [descriptor_set_layout, descriptor_set_layout];
        let sets = device
            .allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(descriptor_pool)
                    .set_layouts(&both_layouts),
            )
            .expect("Failed to allocate descriptor sets");
        let (descriptor_set, descriptor_set_snapshot) = (sets[0], sets[1]);

        let image_infos = [vk::DescriptorImageInfo::default()
            .image_view(scene.backdrop_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let sampler_infos = [vk::DescriptorImageInfo::default().sampler(backdrop_sampler)];
        let buffer_infos = [vk::DescriptorBufferInfo::default()
            .buffer(window_info.buffer)
            .offset(0)
            .range(WINDOW_INFO_BYTES)];
        let feature_infos = [vk::DescriptorBufferInfo::default()
            .buffer(plate_features.buffer)
            .offset(0)
            .range((FRAMES_IN_FLIGHT * MAX_PLATE_FEATURES * PLATE_FEATURE_BYTES) as vk::DeviceSize)];
        device.update_descriptor_sets(
            &[
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(&image_infos),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .image_info(&sampler_infos),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(2)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&buffer_infos),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(3)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&feature_infos),
                // Snapshot twin: bindings 1-3 alias the same objects; binding 0
                // is written by `sync_backdrop_targets` once the snapshot image
                // exists.
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set_snapshot)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .image_info(&sampler_infos),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set_snapshot)
                    .dst_binding(2)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&buffer_infos),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set_snapshot)
                    .dst_binding(3)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&feature_infos),
            ],
            &[],
        );

        // Per-frame command buffers, sync, and vertex buffers.
        let cmds = device
            .allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(command_pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(FRAMES_IN_FLIGHT as u32),
            )
            .expect("Failed to allocate command buffers");
        let frames = cmds
            .into_iter()
            .map(|cmd| Frame {
                cmd,
                image_available: device
                    .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                    .unwrap(),
                in_flight: device
                    .create_fence(
                        &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
                        None,
                    )
                    .unwrap(),
                vertex: create_cpu_buffer(
                    &device,
                    allocator,
                    64 * 1024,
                    vk::BufferUsageFlags::VERTEX_BUFFER,
                    "vertices",
                ),
                vertex_count: 0,
                overlay_start: 0,
                overlay_count: 0,
            })
            .collect();

        let text = TextStage::new(&device, allocator, render_pass, FRAMES_IN_FLIGHT);
        // Whether a mip chain can be built by blitting: both of the formats a
        // user image may be in have to be a blit's source and destination
        // and filter linearly. Asked here, where the instance is.
        let mips_supported = [vk::Format::R8G8B8A8_SRGB, vk::Format::B8G8R8A8_SRGB]
            .into_iter()
            .all(|format| {
                let needed = vk::FormatFeatureFlags::BLIT_SRC
                    | vk::FormatFeatureFlags::BLIT_DST
                    | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR;
                unsafe {
                    core.instance
                        .get_physical_device_format_properties(core.physical_device, format)
                }
                .optimal_tiling_features
                .contains(needed)
            });
        let image = ImageStage::new(
            &device,
            allocator,
            render_pass,
            FRAMES_IN_FLIGHT,
            mips_supported,
            core.max_anisotropy,
        );

        let swapchain_loader = ash::khr::swapchain::Device::new(&core.instance, &device);
        let mut renderer = Self {
            surface,
            swapchain_loader,
            swapchain: vk::SwapchainKHR::null(),
            swapchain_images: Vec::new(),
            surface_format,
            extent: vk::Extent2D { width: width.max(1), height: height.max(1) },
            swapchain_views: Vec::new(),
            framebuffers: Vec::new(),
            render_finished: Vec::new(),
            render_pass,
            render_pass_load,
            render_pass_partial,
            image_ages: Vec::new(),
            descriptor_set_layout,
            pipeline_layout,
            pipeline,
            shader_module,
            descriptor_pool,
            descriptor_set,
            descriptor_set_snapshot,
            snapshot_image: vk::Image::null(),
            snapshot_view: vk::ImageView::null(),
            snapshot_allocation: None,
            backdrop_sampler,
            window_info,
            profile_gen: 0,
            relief_uploaded: (0.0, 0.0),
            roll_profile_gen: 0,
            plate_features,
            frames,
            frame_index: 0,
            text,
            scene,
            image,
            rt: None,
            rt_background: None,
            rt_environment: RtEnvironment::default(),
            desired_extent: vk::Extent2D { width: width.max(1), height: height.max(1) },
            corner_radius_px,
            swapchain_dirty: false,
            present_mode: vk::PresentModeKHR::FIFO,
            present_debug_count: 0,
            surface_lost: false,
            core,
        };
        log::debug!("[timing] VkRenderer pipelines/stages: {:?}", t_rest.elapsed());
        let t_swap = std::time::Instant::now();
        // On failure `renderer` drops here, and its Drop tears down everything
        // built so far, surface included.
        renderer.create_swapchain()?;
        renderer.write_window_info();
        // The swapchain may have settled on a different extent than requested;
        // keep the backdrop targets in lockstep.
        renderer.sync_backdrop_targets();
        log::debug!("[timing] swapchain setup: {:?}", t_swap.elapsed());
        Ok(renderer)
    }

    /// The window-clip corner radius as the shaders consume it: the nominal
    /// radius widened by the curvature-match factor, so the clip cuts along
    /// the same curve as window-scale plate corners (`plate_push_raised` with
    /// `scale_corners`) and a clipped window reads the same as a plate-drawn
    /// one. Capped at half the smaller extent, like the plate path's cap.
    fn clip_corner_radius(&self) -> f32 {
        let cap = 0.5 * self.extent.width.min(self.extent.height) as f32;
        (self.corner_radius_px * crate::layout::corner_span_factor()).min(cap)
    }

    /// The pinned relief heights in physical px, 0 = follow the width.
    fn relief_px(&self) -> (f32, f32) {
        relief_px_at(crate::scale::scale_factor())
    }

    fn write_window_info(&mut self) {
        // [size/clip vec4][carve profile meta vec4][8 vec4 carve slopes]
        // [roll profile meta vec4][8 vec4 roll slopes][relief heights vec4]
        // — must stay in lockstep with shader2d's WindowInfo. (The frost
        // recipe is per plate, in its push block, since RFC material step 3.)
        let relief = self.relief_px();
        let data = window_info_data(self.extent, self.clip_corner_radius(), relief);
        self.relief_uploaded = relief;
        self.profile_gen = crate::layout::bevel_profile_generation();
        self.roll_profile_gen = crate::layout::roll_profile_generation();
        if let Some(allocation) = self.window_info.allocation.as_mut() {
            allocation.mapped_slice_mut().unwrap()[..WINDOW_INFO_BYTES as usize]
                .copy_from_slice(bytemuck::cast_slice(&data));
        }
    }

    fn destroy_swapchain_resources(&mut self) {
        unsafe {
            for fb in self.framebuffers.drain(..) {
                self.core.device.destroy_framebuffer(fb, None);
            }
            for view in self.swapchain_views.drain(..) {
                self.core.device.destroy_image_view(view, None);
            }
            self.swapchain_images.clear();
            for sem in self.render_finished.drain(..) {
                self.core.device.destroy_semaphore(sem, None);
            }
        }
    }

    /// Build the swapchain for the current surface. Only the calls that ask
    /// the surface can fail with [`SurfaceLost`]; everything after them is
    /// device work and still panics as the bug it would be. A failure leaves
    /// the previous swapchain (if any) in `self.swapchain` for Drop.
    fn create_swapchain(&mut self) -> Result<(), SurfaceLost> {
        unsafe {
            let caps = self.core
                .surface_loader
                .get_physical_device_surface_capabilities(self.core.physical_device, self.surface)
                .map_err(|result| SurfaceLost {
                    call: "vkGetPhysicalDeviceSurfaceCapabilitiesKHR",
                    result,
                })?;

            // Wayland reports "extent defined by the swapchain" (u32::MAX); use the
            // size the configure events gave us.
            let extent = if caps.current_extent.width != u32::MAX {
                caps.current_extent
            } else {
                vk::Extent2D {
                    width: self
                        .desired_extent
                        .width
                        .clamp(caps.min_image_extent.width, caps.max_image_extent.width.max(1)),
                    height: self
                        .desired_extent
                        .height
                        .clamp(caps.min_image_extent.height, caps.max_image_extent.height.max(1)),
                }
            };

            let mut image_count = caps.min_image_count + 1;
            if caps.max_image_count > 0 {
                image_count = image_count.min(caps.max_image_count);
            }

            // Prefer premultiplied (what the DE's other clients pick), else opaque,
            // else whatever the surface offers.
            let composite_alpha = [
                vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
                vk::CompositeAlphaFlagsKHR::OPAQUE,
                vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED,
                vk::CompositeAlphaFlagsKHR::INHERIT,
            ]
            .into_iter()
            .find(|&mode| caps.supported_composite_alpha.contains(mode))
            .unwrap_or(vk::CompositeAlphaFlagsKHR::OPAQUE);

            // MAILBOX when the driver offers it (Mesa Wayland always does):
            // FIFO's present throttle waits on the PREVIOUS present's frame
            // callback, and a surface the compositor never renders (off the
            // viewport) never gets one — the second-ever present then blocks
            // forever inside queue_present with the whole event loop behind
            // it. MAILBOX just replaces the queued buffer, so presenting to
            // an invisible surface is always safe. The demand-driven loop's
            // frame-callback gate keeps MAILBOX from free-running.
            let modes = self
                .core
                .surface_loader
                .get_physical_device_surface_present_modes(self.core.physical_device, self.surface)
                .unwrap_or_default();
            self.present_mode = if modes.contains(&vk::PresentModeKHR::MAILBOX) {
                vk::PresentModeKHR::MAILBOX
            } else {
                vk::PresentModeKHR::FIFO
            };

            let old_swapchain = self.swapchain;
            self.swapchain = self
                .swapchain_loader
                .create_swapchain(
                    &vk::SwapchainCreateInfoKHR::default()
                        .surface(self.surface)
                        .min_image_count(image_count)
                        .image_format(self.surface_format.format)
                        .image_color_space(self.surface_format.color_space)
                        .image_extent(extent)
                        .image_array_layers(1)
                        .image_usage(
                            vk::ImageUsageFlags::COLOR_ATTACHMENT
                                | vk::ImageUsageFlags::TRANSFER_DST
                                // Blur-behind plates copy the frame-so-far out
                                // of the swapchain into the snapshot image.
                                | vk::ImageUsageFlags::TRANSFER_SRC,
                        )
                        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                        .pre_transform(caps.current_transform)
                        .composite_alpha(composite_alpha)
                        .present_mode(self.present_mode)
                        .clipped(true)
                        .old_swapchain(old_swapchain),
                    None,
                )
                .map_err(|result| SurfaceLost { call: "vkCreateSwapchainKHR", result })?;
            if old_swapchain != vk::SwapchainKHR::null() {
                self.swapchain_loader.destroy_swapchain(old_swapchain, None);
            }
            self.extent = extent;
            if extent.width == self.desired_extent.width && extent.height == self.desired_extent.height {
                // Keep the two in step so a rebuild queued for a non-resize
                // reason (suboptimal/out-of-date) doesn't hand
                // `pending_extent` a stale or unclamped size.
                self.desired_extent = extent;
            } else {
                // The surface capabilities overrode the requested size (seen
                // on suspend/resume, when caps briefly lag the real surface
                // state). Presenting this swapchain would commit a buffer the
                // caller never approved — paired with the wrong buffer scale
                // that reads as a self-resize and half/double-sizes the
                // window. Keep the request, requeue the rebuild, and let
                // draw_frame skip the present until caps agree.
                log::warn!(
                    "swapchain extent {}x{} != requested {}x{}; skipping present until they agree",
                    extent.width, extent.height,
                    self.desired_extent.width, self.desired_extent.height,
                );
                self.swapchain_dirty = true;
            }

            let images = self
                .swapchain_loader
                .get_swapchain_images(self.swapchain)
                .map_err(|result| SurfaceLost { call: "vkGetSwapchainImagesKHR", result })?;
            self.swapchain_images = images.clone();
            self.image_ages = vec![ImageAge::Unknown; images.len()];
            let subresource_range = vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .base_mip_level(0)
                .level_count(1)
                .base_array_layer(0)
                .layer_count(1);
            for image in &images {
                let view = self.core
                    .device
                    .create_image_view(
                        &vk::ImageViewCreateInfo::default()
                            .image(*image)
                            .view_type(vk::ImageViewType::TYPE_2D)
                            .format(self.surface_format.format)
                            .subresource_range(subresource_range),
                        None,
                    )
                    .expect("Failed to create swapchain view");
                self.swapchain_views.push(view);
                let attachments = [view];
                let fb = self.core
                    .device
                    .create_framebuffer(
                        &vk::FramebufferCreateInfo::default()
                            .render_pass(self.render_pass)
                            .attachments(&attachments)
                            .width(extent.width)
                            .height(extent.height)
                            .layers(1),
                        None,
                    )
                    .expect("Failed to create framebuffer");
                self.framebuffers.push(fb);
                self.render_finished.push(
                    self.core.device
                        .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                        .unwrap(),
                );
            }
        }
        Ok(())
    }

    fn recreate_swapchain(&mut self) -> Result<(), SurfaceLost> {
        unsafe {
            let _ = self.core.device.device_wait_idle();
        }
        let before = self.extent;
        self.destroy_swapchain_resources();
        self.create_swapchain()?;
        if present_debug() {
            eprintln!(
                "[vk] swapchain rebuilt {}x{} -> {}x{} (backdrop valid: {})",
                before.width, before.height, self.extent.width, self.extent.height,
                self.scene.backdrop_valid,
            );
        }
        self.write_window_info();
        self.sync_backdrop_targets();
        Ok(())
    }

    /// Latch a lost surface: say so once, then skip draws until a new
    /// surface is attached.
    fn mark_surface_lost(&mut self, lost: SurfaceLost) {
        if !self.surface_lost {
            log::warn!("[vk] {lost}; skipping draws until the connection is replaced");
        }
        self.surface_lost = true;
    }

    /// Whether the surface has been reported lost (see [`SurfaceLost`]). A
    /// caller with its own event loop can end its session on this rather
    /// than wait for the connection error.
    pub fn surface_lost(&self) -> bool {
        self.surface_lost
    }

    /// Suspend the UI pass, copy the swapchain's frame-so-far into the blur
    /// snapshot image, and resume drawing — the mechanism behind blur-behind
    /// plates (`Batch2D::blur_behind`). Ending the pass leaves the swapchain in
    /// its PRESENT final layout; the copy walks it through TRANSFER_SRC and
    /// hands it back in TRANSFER_DST, which is exactly `render_pass_load`'s
    /// expected initial layout, so the resume reuses that pass (and the shared
    /// framebuffers). Dynamic viewport state dies with the pass and is restored;
    /// scissor/pipeline/descriptors are re-bound per draw by the batch loop.
    fn snapshot_frame_so_far(&self, cmd: vk::CommandBuffer, image_index: usize) {
        let device = &self.core.device;
        let swapchain_image = self.swapchain_images[image_index];
        unsafe {
            device.cmd_end_render_pass(cmd);
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[
                    vk::ImageMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                        .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                        .old_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                        .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .image(swapchain_image)
                        .subresource_range(COLOR_RANGE),
                    // Covers the previous frame's blur reads of the snapshot.
                    vk::ImageMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::SHADER_READ)
                        .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                        .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .image(self.snapshot_image)
                        .subresource_range(COLOR_RANGE),
                ],
            );
            let subresource = vk::ImageSubresourceLayers::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .layer_count(1);
            device.cmd_copy_image(
                cmd,
                swapchain_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                self.snapshot_image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[vk::ImageCopy::default()
                    .src_subresource(subresource)
                    .dst_subresource(subresource)
                    .extent(vk::Extent3D {
                        width: self.extent.width,
                        height: self.extent.height,
                        depth: 1,
                    })],
            );
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER | vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[
                    vk::ImageMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                        .dst_access_mask(vk::AccessFlags::SHADER_READ)
                        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .image(self.snapshot_image)
                        .subresource_range(COLOR_RANGE),
                    vk::ImageMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::TRANSFER_READ)
                        .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                        .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .image(swapchain_image)
                        .subresource_range(COLOR_RANGE),
                ],
            );
            device.cmd_begin_render_pass(
                cmd,
                &vk::RenderPassBeginInfo::default()
                    .render_pass(self.render_pass_load)
                    .framebuffer(self.framebuffers[image_index])
                    .render_area(vk::Rect2D {
                        offset: vk::Offset2D { x: 0, y: 0 },
                        extent: self.extent,
                    }),
                vk::SubpassContents::INLINE,
            );
            device.cmd_set_viewport(cmd, 0, &[flipped_viewport(self.extent)]);
        }
    }

    /// Recreate backdrop + depth at the surface size (device must be idle),
    /// re-point the UI descriptor at the new view, and make the fresh image
    /// legal to sample.
    ///
    /// A rebuild at the SAME size (a swapchain reported suboptimal or out of
    /// date, a corner-radius change) keeps the backdrop, and must not clear
    /// it: `backdrop_valid` stays true across it, so a cleared image is
    /// replayed under the UI as the scene, and an app that stages only when
    /// its scene changes never repairs it. Seen as a designer viewport that
    /// stayed black until the pointer moved, on a discrete GPU presenting to
    /// a compositor on the integrated one — its swapchain is rebuilt at the
    /// same size a few frames in (`CCE_PRESENT_DEBUG` logs every rebuild).
    fn sync_backdrop_targets(&mut self) {
        let recreated = self.scene.resize(
            &self.core.device,
            self.core.allocator.as_mut().unwrap(),
            self.extent,
        );
        if recreated {
            clear_image_to_shader_read(
                &self.core.device,
                self.core.queue,
                self.core.command_pool,
                self.scene.backdrop_image,
            );
        }
        // The blur snapshot target tracks the surface size alongside the
        // backdrop (same format so cmd_copy_image from the swapchain is legal).
        unsafe {
            let device = &self.core.device;
            if self.snapshot_view != vk::ImageView::null() {
                device.destroy_image_view(self.snapshot_view, None);
                device.destroy_image(self.snapshot_image, None);
                self.snapshot_view = vk::ImageView::null();
                self.snapshot_image = vk::Image::null();
            }
            if let Some(alloc) = self.snapshot_allocation.take() {
                let _ = self.core.allocator.as_mut().unwrap().free(alloc);
            }
            let device = &self.core.device;
            let snapshot_image = device
                .create_image(
                    &vk::ImageCreateInfo::default()
                        .image_type(vk::ImageType::TYPE_2D)
                        .format(self.surface_format.format)
                        .extent(vk::Extent3D {
                            width: self.extent.width,
                            height: self.extent.height,
                            depth: 1,
                        })
                        .mip_levels(1)
                        .array_layers(1)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .tiling(vk::ImageTiling::OPTIMAL)
                        .usage(
                            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
                        )
                        .initial_layout(vk::ImageLayout::UNDEFINED),
                    None,
                )
                .expect("Failed to create snapshot image");
            let requirements = device.get_image_memory_requirements(snapshot_image);
            let allocation = self
                .core
                .allocator
                .as_mut()
                .unwrap()
                .allocate(&AllocationCreateDesc {
                    name: "blur-snapshot",
                    requirements,
                    location: MemoryLocation::GpuOnly,
                    linear: false,
                    allocation_scheme: AllocationScheme::GpuAllocatorManaged,
                })
                .expect("Failed to allocate snapshot memory");
            self.core
                .device
                .bind_image_memory(snapshot_image, allocation.memory(), allocation.offset())
                .expect("Failed to bind snapshot memory");
            let snapshot_view = self
                .core
                .device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(snapshot_image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(self.surface_format.format)
                        .subresource_range(COLOR_RANGE),
                    None,
                )
                .expect("Failed to create snapshot view");
            self.snapshot_image = snapshot_image;
            self.snapshot_view = snapshot_view;
            self.snapshot_allocation = Some(allocation);
        }
        // A fresh snapshot must be legal to sample before its first copy.
        clear_image_to_shader_read(
            &self.core.device,
            self.core.queue,
            self.core.command_pool,
            self.snapshot_image,
        );
        let image_infos = [vk::DescriptorImageInfo::default()
            .image_view(self.scene.backdrop_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let snapshot_infos = [vk::DescriptorImageInfo::default()
            .image_view(self.snapshot_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        unsafe {
            self.core.device.update_descriptor_sets(
                &[
                    vk::WriteDescriptorSet::default()
                        .dst_set(self.descriptor_set)
                        .dst_binding(0)
                        .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                        .image_info(&image_infos),
                    vk::WriteDescriptorSet::default()
                        .dst_set(self.descriptor_set_snapshot)
                        .dst_binding(0)
                        .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                        .image_info(&snapshot_infos),
                ],
                &[],
            );
        }
    }

    /// Upload a 3D mesh (Vertex3D: position + color); the id is stable for the
    /// renderer's lifetime.
    pub fn create_mesh(&mut self, verts: &[Vertex3D]) -> MeshId {
        self.scene
            .create_mesh(&self.core.device, self.core.allocator.as_mut().unwrap(), verts)
    }

    /// Replace a mesh's vertices. Waits for the GPU to go idle first — geometry
    /// updates are rare (settings changes, graph rebuilds), matching the app.
    #[allow(dead_code)] // cutover API: the app's rebuild_scene_geometry path
    pub fn update_mesh(&mut self, id: MeshId, verts: &[Vertex3D]) {
        unsafe {
            let _ = self.core.device.device_wait_idle();
        }
        self.scene
            .update_mesh(&self.core.device, self.core.allocator.as_mut().unwrap(), id, verts);
    }

    /// Stage the 3D scene for the next `draw_frame`. Draws render into the
    /// backdrop image (scissored to the viewport pane, physical pixels), which
    /// is copied beneath the UI and doubles as the blur-behind source. Frames
    /// with no staged scene reuse the previous backdrop — the ash equivalent of
    /// the app's viewport-changed cache.
    pub fn stage_scene(&mut self, scissor: (u32, u32, u32, u32), draws: Vec<SceneDraw>) {
        self.scene.stage(scissor, draws);
    }

    /// Add textured quads to the scene staged by the last [`stage_scene`] —
    /// user images (ids from `upload_rgba`) standing in the 3D world, depth
    /// tested against the meshes. Call it AFTER `stage_scene`, which starts
    /// every staged scene with none; with no scene staged it does nothing.
    ///
    /// [`stage_scene`]: Self::stage_scene
    pub fn stage_scene_images(&mut self, images: Vec<SceneImage>) {
        self.scene.stage_images(images);
    }

    /// Replace the path tracer's scene (triangles in the space the camera's
    /// `inv_mvp` unprojects into). Builds the BVH on the CPU and uploads it;
    /// waits for the GPU to go idle first — scene replacement is rare
    /// (geometry rebuilds), matching `update_mesh`. The first call compiles
    /// the compute pipeline.
    pub fn set_rt_scene(&mut self, triangles: &[RtTriangle], materials: &[RtMaterial]) {
        self.set_rt_scene_with_image(triangles, materials, None);
    }

    /// [`set_rt_scene`](Self::set_rt_scene), with a user image standing in
    /// the scene: the picture the raster pass draws as a `SceneImage`,
    /// traced. The image's pixels changing is a change of scene like any
    /// other — set it again, which restarts the accumulation.
    pub fn set_rt_scene_with_image(
        &mut self,
        triangles: &[RtTriangle],
        materials: &[RtMaterial],
        image: Option<RtImage>,
    ) {
        unsafe {
            let _ = self.core.device.device_wait_idle();
        }
        let core = &mut self.core;
        let allocator = core.allocator.as_mut().unwrap();
        let rt = self.rt.get_or_insert_with(|| {
            RtStage::new(
                &core.device,
                allocator,
                FRAMES_IN_FLIGHT,
                core.accel_loader.as_ref(),
                core.as_scratch_align,
                core.min_uniform_align,
                core.queue,
                core.command_pool,
            )
        });
        rt.set_scene(
            &core.device,
            allocator,
            core.queue,
            core.command_pool,
            triangles,
            materials,
            image.map(|i| (RtImageSource::Shared(i.image), i.corners, i.opacity)),
        );
    }

    /// The direction TOWARD the 3D pass's light, in world space (any
    /// length; zero is ignored). The flat shading reads it, and a host that
    /// bakes smooth shading (`SceneDraw::prelit`) should light by the same
    /// one. Until a host sets it the light is the one this pass always had.
    /// A traced pane has its own, in [`RtEnvironment`]; a host that wants
    /// the two views to agree hands both the same direction.
    pub fn set_scene_light(&mut self, toward: [f32; 3]) {
        let v = glam::Vec3::from_array(toward);
        if v.length_squared() > 1e-12 {
            self.scene.light = v.normalize().to_array();
        }
    }

    /// The traced pane's sky and sun — see [`RtEnvironment`]. Kept here and
    /// handed over at each `stage_rt`, like the background; a change
    /// restarts the accumulation.
    pub fn set_rt_environment(&mut self, environment: RtEnvironment) {
        self.rt_environment = environment;
    }

    /// What a camera ray that meets nothing shows in the traced pane: a
    /// colour in LINEAR RGB (what the raster pass's vertex colours are), or
    /// None for the sky, which is what every miss showed until 2026-10-02.
    /// Only the camera ray: a bounce that leaves the scene still meets the
    /// sky, the tracer's one light, so a backdrop changes what is seen
    /// behind the scene and not how it is lit. Kept here and handed over at
    /// each `stage_rt`, so it may be set before the first scene; a change
    /// restarts the accumulation.
    pub fn set_rt_background(&mut self, color: Option<[f32; 3]>) {
        self.rt_background = color;
    }

    /// Stage one progressive path-tracing pass into the viewport pane
    /// (physical pixels) for the next `draw_frame`. Call it every frame while
    /// RT mode is on: each frame adds a sample; a camera/pane/scene change
    /// restarts the accumulation. No-op until `set_rt_scene` has run.
    pub fn stage_rt(&mut self, pane: (u32, u32, u32, u32), camera: RtCamera) {
        if let Some(rt) = self.rt.as_mut() {
            rt.set_background(self.rt_background);
            rt.set_environment(self.rt_environment);
            rt.stage(
                &self.core.device,
                self.core.allocator.as_mut().unwrap(),
                pane,
                camera,
            );
        }
    }

    /// True while more `stage_rt` + `draw_frame` rounds would still refine the
    /// image — the app's cue to keep requesting frames.
    pub fn rt_accumulating(&self) -> bool {
        self.rt.as_ref().is_some_and(|rt| rt.accumulating())
    }

    /// Whether presenting past an unacknowledged frame callback is safe.
    /// True under MAILBOX (the present replaces the queued buffer). Under
    /// FIFO the driver's present throttle waits on the previous present's
    /// frame event, so a forced present to a surface the compositor isn't
    /// rendering blocks forever — the caller must not force one.
    pub fn forced_present_safe(&self) -> bool {
        self.present_mode == vk::PresentModeKHR::MAILBOX
    }

    /// Let go of the window surface: wait idle, then destroy the swapchain
    /// and the `VkSurfaceKHR`, keeping the device, pipelines and atlases. The
    /// `wl_surface` under them may be destroyed after this returns, and must
    /// not be before — a swapchain presenting to a dead surface is undefined.
    /// Until [`attach_surface`](Self::attach_surface), `draw_frame_2d` draws
    /// nothing and returns false.
    pub fn detach_surface(&mut self) {
        unsafe {
            let _ = self.core.device.device_wait_idle();
            self.destroy_swapchain_resources();
            if self.swapchain != vk::SwapchainKHR::null() {
                self.swapchain_loader.destroy_swapchain(self.swapchain, None);
                self.swapchain = vk::SwapchainKHR::null();
            }
            if self.surface != vk::SurfaceKHR::null() {
                self.core.surface_loader.destroy_surface(self.surface, None);
                self.surface = vk::SurfaceKHR::null();
            }
        }
        self.extent = vk::Extent2D { width: 0, height: 0 };
        self.swapchain_dirty = true;
    }

    /// Present to a different `wl_surface` from now on, at `width` x
    /// `height` physical px — detaching from the current one first if it is
    /// still attached. What makes a popup surface cheap to re-open: a new
    /// renderer costs a device and every pipeline, this costs one swapchain.
    ///
    /// # Safety
    /// Same contract as [`VkRenderer::try_new`]: live `wl_display` / `wl_surface`
    /// pointers that outlive the attachment.
    pub unsafe fn attach_surface(
        &mut self,
        display_ptr: *mut c_void,
        surface_ptr: *mut c_void,
        width: u32,
        height: u32,
    ) -> Result<(), SurfaceLost> {
        self.attach_surface_to(
            super::core::SurfaceTarget::Wayland { display: display_ptr, surface: surface_ptr },
            width,
            height,
        )
    }

    /// [`attach_surface`](Self::attach_surface) for any window
    /// [`SurfaceTarget`](super::core::SurfaceTarget).
    ///
    /// # Safety
    /// The target's pointers must be live and outlive the attachment.
    pub unsafe fn attach_surface_to(
        &mut self,
        target: super::core::SurfaceTarget,
        width: u32,
        height: u32,
    ) -> Result<(), SurfaceLost> {
        if self.surface != vk::SurfaceKHR::null() {
            self.detach_surface();
        }
        self.surface = self.core.create_surface(target)?;
        self.surface_lost = false;
        self.resize(width, height);
        self.swapchain_dirty = true;
        Ok(())
    }

    /// Whether a surface is attached — false between
    /// [`detach_surface`](Self::detach_surface) and the next attach.
    pub fn has_surface(&self) -> bool {
        self.surface != vk::SurfaceKHR::null()
    }

    /// The extent the next `draw_frame` will render at: the pending size when a
    /// swapchain rebuild is queued, otherwise the live one.
    pub fn pending_extent(&self) -> vk::Extent2D {
        if self.swapchain_dirty { self.desired_extent } else { self.extent }
    }

    /// Request a new physical size (from xdg configure / scale changes). Applied
    /// lazily on the next `draw_frame`.
    pub fn resize(&mut self, width: u32, height: u32) {
        let extent = vk::Extent2D { width: width.max(1), height: height.max(1) };
        // Record the request unconditionally, not just when it differs from the
        // live extent: with a rebuild already queued (`swapchain_dirty`), a
        // request that returns to the live size must overwrite the queued one.
        // Otherwise `desired_extent` stays wedged at the intermediate size, the
        // caller's `pending_extent` gate never matches, and no frame presents
        // again — a resume scale bounce (2→1→2 before any draw) froze the
        // status-bar clock exactly this way.
        self.desired_extent = extent;
        if extent.width != self.extent.width || extent.height != self.extent.height {
            self.swapchain_dirty = true;
        }
    }

    // Used at cutover, when scale changes re-derive the radius; vk-smoke fixes it at init.
    // Nominal (circle-equivalent) radius in physical px — the curvature-match
    // widening for squircle corner shapes happens at consumption
    // (`clip_corner_radius`), so callers pass the configured radius as-is.
    #[allow(dead_code)]
    pub fn set_corner_radius(&mut self, radius_px: f32) {
        self.corner_radius_px = radius_px;
        // Written on the next swapchain rebuild or draw-idle moment; a mapped write
        // here would race in-flight frames, so route it through the dirty path.
        self.swapchain_dirty = true;
    }

    /// Stage text for the next `draw_frame`: shape-cache misses are rasterized
    /// into the glyph atlas and vertices are built against the current extent.
    /// Mirrors what was `glyphon::TextRenderer::prepare`.
    pub fn prepare_text(
        &mut self,
        font_system: &mut cosmic_text::FontSystem,
        swash_cache: &mut cosmic_text::SwashCache,
        spans: &[TextSpan<'_>],
    ) {
        // Against the extent this frame will actually be drawn at: `resize` is
        // lazy, so with a rebuild pending `self.extent` is still the previous
        // size and text would land in the wrong NDC (visibly mis-scaled and
        // offset while a window auto-sizes to its content).
        self.text.prepare(font_system, swash_cache, spans, self.pending_extent());
    }

    /// Render one frame of plain 2D geometry: a single unclipped batch, no
    /// overlay, transparent clear. See [`VkRenderer::draw_frame_2d`].
    pub fn draw_frame(&mut self, verts: &[Vertex]) -> bool {
        self.draw_frame_2d(Frame2D {
            verts,
            batches: &[],
            overlay_verts: &[],
            images: &[],
            plate_features: &[],
            clear_color: [0.0; 4],
            damage: None,
        })
    }

    /// Render one frame: the 2D geometry (optionally as scissored batches),
    /// then any text staged via `prepare_text`, then the overlay vertices on
    /// top. Returns false if the frame was skipped (swapchain rebuild); the
    /// caller just draws again next tick.
    pub fn draw_frame_2d(&mut self, frame2d: Frame2D<'_>) -> bool {
        if self.surface == vk::SurfaceKHR::null() || self.surface_lost {
            return false;
        }
        if self.swapchain_dirty {
            self.swapchain_dirty = false;
            if let Err(lost) = self.recreate_swapchain() {
                self.mark_surface_lost(lost);
                return false;
            }
            if self.swapchain_dirty {
                // The rebuild couldn't honor the requested extent (surface
                // caps disagree, e.g. mid suspend/resume) — presenting it
                // would commit a wrong-size buffer. Skip; the caller redraws.
                return false;
            }
        }

        unsafe {
            let frame_index = self.frame_index;
            let (in_flight, image_available) = {
                let f = &self.frames[frame_index];
                (f.in_flight, f.image_available)
            };
            self.core.device
                .wait_for_fences(&[in_flight], true, u64::MAX)
                .expect("Fence wait failed");

            if present_debug() {
                eprintln!("[vk] frame {} acquire...", self.present_debug_count);
            }
            let image_index = match self.swapchain_loader.acquire_next_image(
                self.swapchain,
                u64::MAX,
                image_available,
                vk::Fence::null(),
            ) {
                Ok((index, suboptimal)) => {
                    if suboptimal {
                        self.swapchain_dirty = true;
                    }
                    index
                }
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    self.swapchain_dirty = true;
                    return false;
                }
                Err(result @ vk::Result::ERROR_SURFACE_LOST_KHR) => {
                    self.mark_surface_lost(SurfaceLost { call: "vkAcquireNextImageKHR", result });
                    return false;
                }
                Err(e) => {
                    log::error!("acquire_next_image failed: {e:?}");
                    return false;
                }
            };

            self.core.device.reset_fences(&[in_flight]).unwrap();

            // Re-upload the bevel-profile LUT when it changed (a live ramp
            // edit). The other in-flight frame may still read the old bytes —
            // both are valid profiles, so the one-frame mix is benign.
            if self.profile_gen != crate::layout::bevel_profile_generation()
                || self.roll_profile_gen != crate::layout::roll_profile_generation()
                || self.relief_uploaded != self.relief_px()
            {
                self.write_window_info();
            }

            // Upload this frame's plate carves into its slot of the feature
            // UBO (the slot's previous user has fenced, so no race).
            if !frame2d.plate_features.is_empty() {
                let n = frame2d.plate_features.len().min(MAX_PLATE_FEATURES);
                let base = frame_index * MAX_PLATE_FEATURES * PLATE_FEATURE_BYTES;
                if let Some(allocation) = self.plate_features.allocation.as_mut() {
                    let bytes: &[u8] = bytemuck::cast_slice(&frame2d.plate_features[..n]);
                    allocation.mapped_slice_mut().unwrap()[base..base + bytes.len()]
                        .copy_from_slice(bytes);
                }
            }

            // Upload display-list + overlay vertices into this frame's buffer
            // (its fence has signaled, so the GPU is done with it; growing swaps
            // in a fresh buffer). Overlay verts sit after the main range.
            let vert_bytes: &[u8] = bytemuck::cast_slice(frame2d.verts);
            let overlay_bytes: &[u8] = bytemuck::cast_slice(frame2d.overlay_verts);
            let needed = (vert_bytes.len() + overlay_bytes.len()) as vk::DeviceSize;
            if needed > self.frames[frame_index].vertex.size {
                let mut old =
                    std::mem::replace(&mut self.frames[frame_index].vertex, AllocatedBuffer::null());
                let allocator = self.core.allocator.as_mut().unwrap();
                destroy_cpu_buffer(&self.core.device, allocator, &mut old);
                self.frames[frame_index].vertex = create_cpu_buffer(
                    &self.core.device,
                    allocator,
                    needed.next_power_of_two(),
                    vk::BufferUsageFlags::VERTEX_BUFFER,
                    "vertices",
                );
            }
            if needed > 0 {
                let mapped = self.frames[frame_index]
                    .vertex
                    .allocation
                    .as_mut()
                    .unwrap()
                    .mapped_slice_mut()
                    .unwrap();
                mapped[..vert_bytes.len()].copy_from_slice(vert_bytes);
                mapped[vert_bytes.len()..vert_bytes.len() + overlay_bytes.len()]
                    .copy_from_slice(overlay_bytes);
            }
            self.frames[frame_index].vertex_count = frame2d.verts.len() as u32;
            self.frames[frame_index].overlay_start = frame2d.verts.len() as u32;
            self.frames[frame_index].overlay_count = frame2d.overlay_verts.len() as u32;
            self.text.write_frame_buffers(
                &self.core.device,
                self.core.allocator.as_mut().unwrap(),
                frame_index,
            );
            self.image.process_pending(
                &self.core.device,
                self.core.allocator.as_mut().unwrap(),
                self.core.queue,
                self.core.command_pool,
            );
            self.image.write_frame_buffer(
                &self.core.device,
                self.core.allocator.as_mut().unwrap(),
                frame_index,
                frame2d.images,
                self.extent,
            );
            let clip_radius = self.clip_corner_radius();
            self.scene.write_frame_uniforms(
                &self.core.device,
                self.core.allocator.as_mut().unwrap(),
                frame_index,
                clip_radius,
            );
            if let Some(rt) = self.rt.as_mut() {
                let images = &self.image;
                rt.write_frame_uniforms(&self.core.device, frame_index, &|id| {
                    images.view_and_size(id)
                });
            }

            // Record.
            let cmd = self.frames[frame_index].cmd;
            self.core.device
                .begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())
                .unwrap();
            self.text.record_upload(&self.core.device, cmd, frame_index);

            // Offscreen 3D pass (only when a scene was staged); leaves the
            // backdrop in TRANSFER_SRC.
            let mut scene_recorded =
                self.scene.record(&self.core.device, cmd, frame_index, &self.image);

            // Path-tracer pass (only when staged via `stage_rt`): one
            // accumulation dispatch, blitted into the backdrop's pane region —
            // it fills the same slot as the raster scene pass and leaves the
            // backdrop in TRANSFER_SRC likewise.
            if let Some(rt) = self.rt.as_mut() {
                let rt_recorded = rt.record(
                    &self.core.device,
                    cmd,
                    frame_index,
                    self.scene.backdrop_image,
                    self.extent,
                    scene_recorded,
                );
                if rt_recorded {
                    self.scene.backdrop_valid = true;
                    scene_recorded = true;
                }
            }

            // With a valid backdrop, replay it under the UI: copy it into the
            // swapchain image and open the UI pass with LOAD instead of CLEAR.
            let use_backdrop = self.scene.backdrop_valid;
            if use_backdrop {
                if !scene_recorded {
                    // Reused backdrop is in SHADER_READ_ONLY from last frame.
                    self.core.device.cmd_pipeline_barrier(
                        cmd,
                        vk::PipelineStageFlags::FRAGMENT_SHADER,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[vk::ImageMemoryBarrier::default()
                            .src_access_mask(vk::AccessFlags::SHADER_READ)
                            .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                            .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                            .image(self.scene.backdrop_image)
                            .subresource_range(COLOR_RANGE)],
                    );
                }
                let swapchain_image = self.swapchain_images[image_index as usize];
                self.core.device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[vk::ImageMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::empty())
                        .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                        .old_layout(vk::ImageLayout::UNDEFINED)
                        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .image(swapchain_image)
                        .subresource_range(COLOR_RANGE)],
                );
                let subresource = vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .layer_count(1);
                self.core.device.cmd_copy_image(
                    cmd,
                    self.scene.backdrop_image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    swapchain_image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[vk::ImageCopy::default()
                        .src_subresource(subresource)
                        .dst_subresource(subresource)
                        .extent(vk::Extent3D {
                            width: self.extent.width,
                            height: self.extent.height,
                            depth: 1,
                        })],
                );
                // Backdrop back to sampleable for the UI pass's blur plates.
                self.core.device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[vk::ImageMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::TRANSFER_READ)
                        .dst_access_mask(vk::AccessFlags::SHADER_READ)
                        .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .image(self.scene.backdrop_image)
                        .subresource_range(COLOR_RANGE)],
                );
            }

            let frame = &self.frames[frame_index];
            let clear_values = [vk::ClearValue {
                color: vk::ClearColorValue { float32: frame2d.clear_color },
            }];
            let full_scissor = vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: self.extent,
            };
            // This frame's damage, and whether the acquired image can be
            // brought up to date by repainting part of it. Blur-behind and
            // backdrop frames copy whole images around and stay full.
            let damage = frame2d.damage.map(|(x, y, w, h)| {
                rect_intersect(
                    vk::Rect2D {
                        offset: vk::Offset2D { x: x as i32, y: y as i32 },
                        extent: vk::Extent2D { width: w, height: h },
                    },
                    full_scissor,
                )
            });
            let partial: Option<vk::Rect2D> = match (damage, self.image_ages[image_index as usize]) {
                _ if use_backdrop || frame2d.batches.iter().any(|b| b.blur_behind) => None,
                (Some(d), ImageAge::Current) => Some(d),
                (Some(d), ImageAge::Behind(missing)) => Some(rect_union(d, missing)),
                _ => None,
            };
            // Every scissor of the frame passes through this.
            let clip = |r: vk::Rect2D| match partial {
                Some(region) => rect_intersect(r, region),
                None => r,
            };
            let (ui_pass, ui_clear_values): (vk::RenderPass, &[vk::ClearValue]) = if use_backdrop {
                (self.render_pass_load, &[])
            } else if partial.is_some() {
                (self.render_pass_partial, &[])
            } else {
                (self.render_pass, &clear_values)
            };
            self.core.device.cmd_begin_render_pass(
                cmd,
                &vk::RenderPassBeginInfo::default()
                    .render_pass(ui_pass)
                    .framebuffer(self.framebuffers[image_index as usize])
                    .render_area(vk::Rect2D {
                        offset: vk::Offset2D { x: 0, y: 0 },
                        extent: self.extent,
                    })
                    .clear_values(ui_clear_values),
                vk::SubpassContents::INLINE,
            );
            self.core.device
                .cmd_set_viewport(cmd, 0, &[flipped_viewport(self.extent)]);
            self.core.device.cmd_set_scissor(cmd, 0, &[clip(full_scissor)]);
            if let Some(region) = partial {
                // The partial pass loads instead of clearing; clear what it
                // is about to repaint.
                if region.extent.width > 0 && region.extent.height > 0 {
                    self.core.device.cmd_clear_attachments(
                        cmd,
                        &[vk::ClearAttachment::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .color_attachment(0)
                            .clear_value(clear_values[0])],
                        &[vk::ClearRect::default().rect(region).layer_count(1)],
                    );
                }
            }
            // Display-list geometry interleaved with user images: each image
            // quad draws before the vertex its `z_before` names, so it sits
            // above earlier geometry and below later geometry.
            {
                let images = frame2d.images;
                let mut order: Vec<usize> = (0..images.len()).collect();
                order.sort_by_key(|&k| images[k].z_before);
                let mut img_i = 0usize;

                // Corner-shape exponent for the rounded-rect clip SDF, so clipped
                // edges cut along the same squircle family as the tessellated and
                // SDF-lit plate corners (a plate batch overwrites the slot with
                // its own — identical — per-plate value).
                let clip_shape = crate::layout::corner_shape();

                let default_batch = [Batch2D {
                    scissor: None,
                    clip_rrect: None,
                    start: 0,
                    end: frame.vertex_count,
                    plate: None,
                    blur_behind: false,
                }];
                let batches: &[Batch2D] =
                    if frame2d.batches.is_empty() { &default_batch } else { frame2d.batches };

                // Which @group(0) the vertex draws bind: the scene-backdrop set
                // until the first blur-behind snapshot, the snapshot set after —
                // so blur plates sample the frame-so-far, and later blur plates
                // sample refreshed copies that include earlier ones.
                let mut active_set = self.descriptor_set;
                // CONSECUTIVE blur plates share one snapshot: only a non-blur
                // draw invalidates it. A run of blur plates (the designer's
                // node bodies) costs one copy, not one per plate — they don't
                // see each other, which only matters where they overlap.
                let mut snapshot_fresh = false;

                for batch in batches {
                    // Images due at this batch's boundary draw first: they sit
                    // beneath the batch's geometry, and a blur snapshot taken
                    // for this batch must capture them (an image whose
                    // `z_before` equals the batch start would otherwise slip
                    // to after the snapshot and never be frosted).
                    while let Some(&k) = order.get(img_i) {
                        let q = &images[k];
                        if q.z_before > batch.start {
                            break;
                        }
                        img_i += 1;
                        let img_scissor = match q.clip {
                            Some((cx, cy, cw, ch)) => vk::Rect2D {
                                offset: vk::Offset2D { x: cx as i32, y: cy as i32 },
                                extent: vk::Extent2D {
                                    width: cw.min(self.extent.width.saturating_sub(cx)),
                                    height: ch.min(self.extent.height.saturating_sub(cy)),
                                },
                            },
                            None => full_scissor,
                        };
                        self.core.device.cmd_set_scissor(cmd, 0, &[clip(img_scissor)]);
                        self.image.record_quad(&self.core.device, cmd, frame_index, k, q.image);
                        snapshot_fresh = false;
                    }
                    if batch.blur_behind {
                        if !snapshot_fresh {
                            self.snapshot_frame_so_far(cmd, image_index as usize);
                            active_set = self.descriptor_set_snapshot;
                            snapshot_fresh = true;
                        }
                    } else if batch.start < batch.end {
                        snapshot_fresh = false;
                    }
                    // Resolve the batch scissor; a degenerate one skips the
                    // vertex draws (images still process on their own clips).
                    let batch_scissor: Option<vk::Rect2D> = match batch.scissor {
                        Some((bx, by, bw, bh)) => {
                            if bx >= self.extent.width || by >= self.extent.height {
                                None
                            } else {
                                let bw = bw.min(self.extent.width - bx);
                                let bh = bh.min(self.extent.height - by);
                                if bw == 0 || bh == 0 {
                                    None
                                } else {
                                    Some(vk::Rect2D {
                                        offset: vk::Offset2D { x: bx as i32, y: by as i32 },
                                        extent: vk::Extent2D { width: bw, height: bh },
                                    })
                                }
                            }
                        }
                        None => Some(full_scissor),
                    };

                    let mut cursor = batch.start;
                    while cursor < batch.end {
                        let next_z =
                            order.get(img_i).map(|&k| images[k].z_before).unwrap_or(u32::MAX);
                        if next_z <= cursor {
                            let k = order[img_i];
                            img_i += 1;
                            let q = &images[k];
                            let img_scissor = match q.clip {
                                Some((cx, cy, cw, ch)) => vk::Rect2D {
                                    offset: vk::Offset2D { x: cx as i32, y: cy as i32 },
                                    extent: vk::Extent2D {
                                        width: cw.min(self.extent.width.saturating_sub(cx)),
                                        height: ch.min(self.extent.height.saturating_sub(cy)),
                                    },
                                },
                                None => full_scissor,
                            };
                            self.core.device.cmd_set_scissor(cmd, 0, &[clip(img_scissor)]);
                            self.image.record_quad(&self.core.device, cmd, frame_index, k, q.image);
                            continue;
                        }
                        let upto = next_z.min(batch.end);
                        if let Some(scissor) = batch_scissor {
                            self.core.device
                                .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
                            self.core.device.cmd_bind_descriptor_sets(
                                cmd,
                                vk::PipelineBindPoint::GRAPHICS,
                                self.pipeline_layout,
                                0,
                                &[active_set],
                                &[],
                            );
                            self.core.device
                                .cmd_bind_vertex_buffers(cmd, 0, &[frame.vertex.buffer], &[0]);
                            self.core.device.cmd_set_scissor(cmd, 0, &[clip(scissor)]);
                            // Per-batch rounded-rect clip (fragments outside
                            // discard) + the SDF-lit plate block when this
                            // batch is a plate cover quad.
                            let pc = batch_push_constants(batch, clip_shape, frame_index * MAX_PLATE_FEATURES);
                            self.core.device.cmd_push_constants(
                                cmd,
                                self.pipeline_layout,
                                vk::ShaderStageFlags::FRAGMENT,
                                0,
                                bytemuck::cast_slice(&pc),
                            );
                            self.core.device.cmd_draw(cmd, upto - cursor, 1, cursor, 0);
                        }
                        cursor = upto;
                    }
                }
                // Images sorting after all geometry.
                while let Some(&k) = order.get(img_i) {
                    img_i += 1;
                    let q = &images[k];
                    let img_scissor = match q.clip {
                        Some((cx, cy, cw, ch)) => vk::Rect2D {
                            offset: vk::Offset2D { x: cx as i32, y: cy as i32 },
                            extent: vk::Extent2D {
                                width: cw.min(self.extent.width.saturating_sub(cx)),
                                height: ch.min(self.extent.height.saturating_sub(cy)),
                            },
                        },
                        None => full_scissor,
                    };
                    self.core.device.cmd_set_scissor(cmd, 0, &[clip(img_scissor)]);
                    self.image.record_quad(&self.core.device, cmd, frame_index, k, q.image);
                }
                // Restore for the text/overlay draws.
                self.core.device.cmd_set_scissor(cmd, 0, &[clip(full_scissor)]);
            }
            self.text.record_draw(&self.core.device, cmd, frame_index);
            if frame.overlay_count > 0 {
                // The text pass bound its own pipeline; rebind for the overlay.
                self.core.device
                    .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
                self.core.device.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.pipeline_layout,
                    0,
                    &[self.descriptor_set],
                    &[],
                );
                self.core.device
                    .cmd_bind_vertex_buffers(cmd, 0, &[frame.vertex.buffer], &[0]);
                // Push constants persist across binds — clear any batch's
                // rounded clip and plate mode.
                let pc = [0.0f32; PUSH_CONSTANT_FLOATS];
                self.core.device.cmd_push_constants(
                    cmd,
                    self.pipeline_layout,
                    vk::ShaderStageFlags::FRAGMENT,
                    0,
                    bytemuck::cast_slice(&pc),
                );
                self.core.device
                    .cmd_draw(cmd, frame.overlay_count, 1, frame.overlay_start, 0);
            }
            self.core.device.cmd_end_render_pass(cmd);
            self.core.device.end_command_buffer(cmd).unwrap();

            // Submit + present. The acquire semaphore gates the swapchain image's
            // first use: the backdrop copy (TRANSFER) or the UI pass (COLOR).
            let wait_semaphores = [image_available];
            let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags::TRANSFER];
            let cmds = [cmd];
            let signal_semaphores = [self.render_finished[image_index as usize]];
            let submit = vk::SubmitInfo::default()
                .wait_semaphores(&wait_semaphores)
                .wait_dst_stage_mask(&wait_stages)
                .command_buffers(&cmds)
                .signal_semaphores(&signal_semaphores);
            self.core.device
                .queue_submit(self.core.queue, &[submit], in_flight)
                .expect("Queue submit failed");

            // The image now holds this frame; every other image fell behind
            // by this frame's damage.
            for (k, age) in self.image_ages.iter_mut().enumerate() {
                *age = if k == image_index as usize {
                    ImageAge::Current
                } else {
                    match (damage, *age) {
                        (Some(d), ImageAge::Current) => ImageAge::Behind(d),
                        (Some(d), ImageAge::Behind(m)) => ImageAge::Behind(rect_union(d, m)),
                        _ => ImageAge::Unknown,
                    }
                };
            }

            let swapchains = [self.swapchain];
            let image_indices = [image_index];
            // What changed since the previous present — the frame's damage,
            // however much of the image had to be repainted to get there.
            // Only for a frame that was itself partial: the first frames of
            // a swapchain replace a buffer of another size or none at all.
            let present_rects = [vk::RectLayerKHR::default()
                .offset(damage.unwrap_or(full_scissor).offset)
                .extent(damage.unwrap_or(full_scissor).extent)
                .layer(0)];
            let present_region = [vk::PresentRegionKHR::default().rectangles(&present_rects)];
            let mut present_regions = vk::PresentRegionsKHR::default().regions(&present_region);
            let mut present = vk::PresentInfoKHR::default()
                .wait_semaphores(&signal_semaphores)
                .swapchains(&swapchains)
                .image_indices(&image_indices);
            if self.core.incremental_present && partial.is_some() {
                present = present.push_next(&mut present_regions);
            }
            if present_debug() {
                eprintln!("[vk] frame {} present img {}...", self.present_debug_count, image_index);
            }
            match self.swapchain_loader.queue_present(self.core.queue, &present) {
                Ok(suboptimal) => {
                    if suboptimal {
                        self.swapchain_dirty = true;
                    }
                }
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    self.swapchain_dirty = true;
                }
                Err(result @ vk::Result::ERROR_SURFACE_LOST_KHR) => {
                    self.mark_surface_lost(SurfaceLost { call: "vkQueuePresentKHR", result });
                }
                Err(e) => log::error!("queue_present failed: {e:?}"),
            }

            if present_debug() {
                eprintln!("[vk] frame {} presented", self.present_debug_count);
                self.present_debug_count += 1;
            }
            self.frame_index = (self.frame_index + 1) % FRAMES_IN_FLIGHT;
        }
        true
    }
}

/// `CCE_PRESENT_DEBUG=1` traces every acquire/present to stderr — the
/// diagnostic for present-pipeline stalls (a present that logs `acquire...`
/// or `present img N...` with no matching completion line is blocked inside
/// the driver; see the off-viewport freeze notes on the present-mode choice
/// in `create_swapchain`).
fn present_debug() -> bool {
    static FLAG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FLAG.get_or_init(|| std::env::var_os("CCE_PRESENT_DEBUG").is_some())
}

/// The 3D half of the renderer, as an app stages it through
/// `Application::init_3d` / `stage_3d` — each method is the inherent one.
impl crate::draw::scene::Stage3D for VkRenderer {
    fn create_mesh(&mut self, verts: &[Vertex3D]) -> MeshId {
        VkRenderer::create_mesh(self, verts)
    }
    fn update_mesh(&mut self, id: MeshId, verts: &[Vertex3D]) {
        VkRenderer::update_mesh(self, id, verts)
    }
    fn stage_scene(&mut self, scissor: (u32, u32, u32, u32), draws: Vec<SceneDraw>) {
        VkRenderer::stage_scene(self, scissor, draws)
    }
    fn stage_scene_images(&mut self, images: Vec<SceneImage>) {
        VkRenderer::stage_scene_images(self, images)
    }
    fn set_scene_light(&mut self, toward: [f32; 3]) {
        VkRenderer::set_scene_light(self, toward)
    }
    fn set_rt_scene_with_image(&mut self, triangles: &[RtTriangle], materials: &[RtMaterial], image: Option<RtImage>) {
        VkRenderer::set_rt_scene_with_image(self, triangles, materials, image)
    }
    fn set_rt_environment(&mut self, environment: RtEnvironment) {
        VkRenderer::set_rt_environment(self, environment)
    }
    fn set_rt_background(&mut self, color: Option<[f32; 3]>) {
        VkRenderer::set_rt_background(self, color)
    }
    fn stage_rt(&mut self, pane: (u32, u32, u32, u32), camera: RtCamera) {
        VkRenderer::stage_rt(self, pane, camera)
    }
    fn rt_accumulating(&self) -> bool {
        VkRenderer::rt_accumulating(self)
    }
}

impl Drop for VkRenderer {
    fn drop(&mut self) {
        unsafe {
            let _ = self.core.device.device_wait_idle();

            let mut frames = std::mem::take(&mut self.frames);
            for frame in &mut frames {
                self.core.device.destroy_semaphore(frame.image_available, None);
                self.core.device.destroy_fence(frame.in_flight, None);
                let mut vertex = std::mem::replace(&mut frame.vertex, AllocatedBuffer::null());
                if let Some(allocator) = self.core.allocator.as_mut() {
                    destroy_cpu_buffer(&self.core.device, allocator, &mut vertex);
                }
            }

            self.destroy_swapchain_resources();
            if self.swapchain != vk::SwapchainKHR::null() {
                self.swapchain_loader.destroy_swapchain(self.swapchain, None);
            }

            if let Some(allocator) = self.core.allocator.as_mut() {
                self.text.destroy(&self.core.device, allocator);
            }

            self.core.device.destroy_sampler(self.backdrop_sampler, None);
            if self.snapshot_view != vk::ImageView::null() {
                self.core.device.destroy_image_view(self.snapshot_view, None);
                self.core.device.destroy_image(self.snapshot_image, None);
            }
            if let Some(alloc) = self.snapshot_allocation.take() {
                if let Some(allocator) = self.core.allocator.as_mut() {
                    let _ = allocator.free(alloc);
                }
            }
            if let Some(allocator) = self.core.allocator.as_mut() {
                self.scene.destroy(&self.core.device, allocator);
                self.image.destroy(&self.core.device, allocator);
                if let Some(mut rt) = self.rt.take() {
                    rt.destroy(&self.core.device, allocator);
                }
            }
            let mut window_info = std::mem::replace(&mut self.window_info, AllocatedBuffer::null());
            let mut plate_features =
                std::mem::replace(&mut self.plate_features, AllocatedBuffer::null());
            if let Some(allocator) = self.core.allocator.as_mut() {
                destroy_cpu_buffer(&self.core.device, allocator, &mut window_info);
                destroy_cpu_buffer(&self.core.device, allocator, &mut plate_features);
            }

            self.core.device.destroy_descriptor_pool(self.descriptor_pool, None);
            self.core.device
                .destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            self.core.device.destroy_pipeline(self.pipeline, None);
            self.core.device.destroy_pipeline_layout(self.pipeline_layout, None);
            self.core.device.destroy_shader_module(self.shader_module, None);
            self.core.device.destroy_render_pass(self.render_pass, None);
            self.core.device.destroy_render_pass(self.render_pass_load, None);
            self.core.device.destroy_render_pass(self.render_pass_partial, None);
            self.core.surface_loader.destroy_surface(self.surface, None);
            // The rest (allocator, command pool, device, instance) is the
            // core's Drop, which runs after this body.
        }
    }
}

#[cfg(test)]
mod tests {
    /// The WGSL shaders compile at process start, so a syntax or validation
    /// error is a runtime panic in every client — catch it headlessly here.
    #[test]
    fn shader2d_compiles() {
        assert!(!super::shader2d_spirv().is_empty());
    }

    /// The WebGPU variants validate with no capabilities at all — WebGPU has
    /// no push constants — and the 2D one carries its block as the uniform
    /// the web renderer binds. naga does not see everything a browser's
    /// compiler rejects (its derivative-uniformity analysis does not follow
    /// calls), so the browser probe is the last word; this catches a
    /// substitution that silently stopped applying.
    #[test]
    fn the_webgpu_shaders_validate_without_push_constants() {
        let web2d = crate::draw::shaders::shader2d_for_webgpu();
        assert!(!web2d.contains("var<push_constant>"));
        assert!(web2d.contains("@group(1) @binding(0) var<uniform> rrect_clip: RRectClip;"));
        for (name, src) in [("shader2d (web)", web2d.as_str()), ("glyph", crate::draw::shaders::GLYPH)] {
            let module = naga::front::wgsl::parse_str(src).unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(src)));
            naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty())
                .validate(&module)
                .unwrap_or_else(|e| panic!("{name} does not validate for WebGPU: {e:?}"));
        }
        // And the block's size is what the renderers lay out.
        let module = naga::front::wgsl::parse_str(&web2d).unwrap();
        let block = module.types.iter().find(|(_, t)| t.name.as_deref() == Some("RRectClip")).expect("RRectClip").1;
        let naga::TypeInner::Struct { span, .. } = block.inner else { panic!("RRectClip is a struct") };
        assert_eq!(span as usize, crate::draw::PUSH_CONSTANT_FLOATS * 4);
        assert!(span as usize <= crate::draw::shaders::WEBGPU_BLOCK_STRIDE);
    }

    #[test]
    fn scene3d_compiles() {
        assert!(!super::scene3d_spirv().is_empty());
    }

    #[test]
    fn scene3d_image_compiles() {
        assert!(!super::scene3d_image_spirv().is_empty());
    }

    /// `WINDOW_INFO_BYTES` sizes the uniform buffer AND its descriptor range,
    /// and `write_window_info` addresses it by float index — all three have to
    /// agree with shader2d's `WindowInfo` struct, and nothing but a comment
    /// said so. A field appended to the WGSL without growing the const writes
    /// the new value past the end of the buffer, which is a validation error
    /// on a good day and a garbage uniform on a bad one.
    ///
    /// Reads the struct out of the shader source rather than duplicating its
    /// shape here, so it measures the thing it is guarding.
    #[test]
    fn window_info_layout_matches_the_uniform_size() {
        let src = crate::draw::shaders::SHADER2D;
        let body = src
            .split_once("struct WindowInfo {")
            .expect("WindowInfo moved; this test scans for it")
            .1
            .split_once("\n}")
            .expect("unterminated WindowInfo")
            .0;

        let mut floats = 0usize;
        for line in body.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            let ty = line.split_once(':').expect("field: type").1.trim().trim_end_matches(',');
            floats += match ty {
                "f32" => 1,
                "vec2<f32>" | "vec2f" => 2,
                "vec4<f32>" | "vec4f" => 4,
                // std140-ish: an array of vec4 is its element count x 4.
                t if t.starts_with("array<vec4f,") => {
                    let n: usize = t
                        .trim_start_matches("array<vec4f,")
                        .trim_end_matches('>')
                        .trim()
                        .parse()
                        .expect("array length");
                    n * 4
                }
                other => panic!("WindowInfo field type {other} is not in this test's size table"),
            };
        }

        assert_eq!(
            floats * 4,
            super::WINDOW_INFO_BYTES as usize,
            "WindowInfo is {floats} floats ({} bytes); WINDOW_INFO_BYTES says {}",
            floats * 4,
            super::WINDOW_INFO_BYTES,
        );
    }
}
