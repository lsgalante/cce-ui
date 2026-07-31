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

use super::image::{ImageQuad, ImageStage};
use super::rt::{RtCamera, RtMaterial, RtStage, RtTriangle};
use super::scene::{MeshId, SceneDraw, SceneStage, Vertex3D};
use super::text::{TextSpan, TextStage};

/// One scissored draw range of a 2D frame. `scissor` is (x, y, w, h) in
/// physical pixels; None draws with the full-surface scissor. `clip_rrect` is an
/// optional rounded-rect clip `[cx, cy, bx, by, r]` (center, SDF half-extents, corner
/// radius; physical px) applied via push constants — fragments outside it discard, so a
/// plate's children cut off at its rounded corners.
pub struct Batch2D {
    pub scissor: Option<(u32, u32, u32, u32)>,
    pub clip_rrect: Option<[f32; 5]>,
    pub start: u32,
    pub end: u32,
    /// When set, this batch is a single SDF-lit plate cover quad: the params go
    /// out as push constants and shader2d's plate branch lights it per pixel.
    pub plate: Option<PlatePush>,
    /// A blur-behind plate (negative-alpha color): the renderer suspends the UI
    /// pass, copies the swapchain-so-far into its snapshot image, and resumes —
    /// so the plate's blur samples everything painted beneath it (background,
    /// widgets, wires), not just the 3D scene backdrop.
    pub blur_behind: bool,
}

/// Push-constant block for one SDF-lit plate batch (physical px throughout).
/// Mirrors the `p_*` fields of shader2d's `RRectClip`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PlatePush {
    /// SDF box: center + half-extents. May extend past the cover quad — that is
    /// how a recess suppresses a wall.
    pub rect: [f32; 4],
    /// Per-corner radii [tl, tr, br, bl].
    pub radii: [f32; 4],
    /// xyz = unit vector toward the light (+z out of the screen), w = roll width px.
    pub light: [f32; 4],
    /// [shading strength, specular strength, shininess, curvature/AO strength].
    pub material: [f32; 4],
    /// Mode 1: `[feature offset, feature count, 0, 0]` into the frame's
    /// `plate_features` — the carves CSG'd out of this plate (the renderer adds
    /// the frame slot's base offset at record time). Mode 2: the host-plate box
    /// (center + half-extents) a free recess fades out against; far-away sides
    /// (±1e5) disable the fade.
    pub host: [f32; 4],
    /// RGB multiplies the roll's specular color (w unused). Neutral white
    /// normally; the focused-pane bevel carries the highlight color here.
    pub specular_tint: [f32; 4],
    /// 1.0 = raised lit plate, 2.0 = recess overlay.
    pub mode: f32,
    /// Corner shape exponent: 2.0 = circular arcs, > 2 = superellipse
    /// (continuous-curvature) corners — see shader2d's `plate_sdf_grad`.
    pub shape: f32,
}

/// A full 2D frame: the display-list vertices (optionally split into scissored
/// batches), overlay vertices drawn after text, and the clear color (linear;
/// only used on frames without a backdrop copy).
pub struct Frame2D<'a> {
    pub verts: &'a [Vertex],
    pub batches: &'a [Batch2D],
    pub overlay_verts: &'a [Vertex],
    /// User images drawn interleaved with `verts` by each quad's `z_before`.
    pub images: &'a [ImageQuad],
    /// Carves CSG'd into this frame's SDF-lit plates, 12 floats each (rect
    /// center+half-extents, per-corner radii, [width px, depth px, 0, 0]).
    /// Plate batches reference them by offset+count in `PlatePush::host`.
    pub plate_features: &'a [[f32; 12]],
    pub clear_color: [f32; 4],
}

const FRAMES_IN_FLIGHT: usize = 2;
/// Max plate-carve features per frame; the shader's UBO holds one slot of this
/// size per frame in flight.
pub const MAX_PLATE_FEATURES: usize = 64;
const PLATE_FEATURE_BYTES: usize = 48;
/// shader2d's WindowInfo UBO: [size/clip vec4][bevel-profile meta vec4]
/// [8 vec4 of profile slope samples].
// [size/clip vec4][carve profile meta + 8 vec4][roll profile meta + 8 vec4].
const WINDOW_INFO_BYTES: vk::DeviceSize = 304;

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

    desired_extent: vk::Extent2D,
    corner_radius_px: f32,
    swapchain_dirty: bool,

    // Declared last: everything above must be destroyed before the device/
    // instance the core tears down in its own Drop.
    core: super::core::VkCore,
}

/// Compile WGSL to SPIR-V. The Y-flip between wgpu NDC (Y-up) and Vulkan NDC
/// (Y-down) is handled with a negative-height viewport (like wgpu-hal), NOT in
/// the shader — flipping in the shader would reverse screen-space winding and
/// break the 3D pipeline's back-face culling.
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
    SPIRV.get_or_init(|| compile_wgsl(include_str!("shader2d.wgsl")))
}

pub(crate) fn glyph_spirv() -> &'static [u32] {
    static SPIRV: std::sync::OnceLock<Vec<u32>> = std::sync::OnceLock::new();
    SPIRV.get_or_init(|| compile_wgsl(include_str!("glyph.wgsl")))
}

pub(crate) fn scene3d_spirv() -> &'static [u32] {
    static SPIRV: std::sync::OnceLock<Vec<u32>> = std::sync::OnceLock::new();
    SPIRV.get_or_init(|| compile_wgsl(include_str!("scene3d.wgsl")))
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
    /// # Safety
    /// `display_ptr` and `surface_ptr` must be live `wl_display` / `wl_surface`
    /// pointers that outlive the renderer (same contract as `WgpuAdapter::new`).
    pub unsafe fn new(
        display_ptr: *mut c_void,
        surface_ptr: *mut c_void,
        width: u32,
        height: u32,
        corner_radius_px: f32,
    ) -> Self {
        let t_new = std::time::Instant::now();
        let (mut core, surface) =
            super::core::VkCore::new_for_wayland_surface(display_ptr, surface_ptr);
        log::debug!("[timing] VkCore::new_for_wayland_surface: {:?}", t_new.elapsed());
        let t_rest = std::time::Instant::now();
        // Locals over the core for the setup below (methods use self.core.*).
        let device = core.device.clone();
        let queue = core.queue;
        let command_pool = core.command_pool;
        let physical_device = core.physical_device;
        let min_uniform_align = core.min_uniform_align;
        let surface_loader = core.surface_loader.clone();
        let allocator = core.allocator.as_mut().unwrap();

        // Surface format: prefer sRGB (wgpu's get_default_config sorts sRGB first,
        // so this matches the colors the app renders today).
        let formats = surface_loader
            .get_physical_device_surface_formats(physical_device, surface)
            .expect("No surface formats");
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
        // shader2d's fragment stage. 128 bytes — exactly the Vulkan-guaranteed
        // minimum budget.
        let push_ranges = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(128)];
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
            core.wireframe_supported,
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
        let image = ImageStage::new(&device, allocator, render_pass, FRAMES_IN_FLIGHT);

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
            roll_profile_gen: 0,
            plate_features,
            frames,
            frame_index: 0,
            text,
            scene,
            image,
            rt: None,
            desired_extent: vk::Extent2D { width: width.max(1), height: height.max(1) },
            corner_radius_px,
            swapchain_dirty: false,
            core,
        };
        log::debug!("[timing] VkRenderer pipelines/stages: {:?}", t_rest.elapsed());
        let t_swap = std::time::Instant::now();
        renderer.create_swapchain();
        renderer.write_window_info();
        // The swapchain may have settled on a different extent than requested;
        // keep the backdrop targets in lockstep.
        renderer.sync_backdrop_targets();
        log::debug!("[timing] swapchain setup: {:?}", t_swap.elapsed());
        renderer
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

    fn write_window_info(&mut self) {
        // [size/clip vec4][carve profile meta vec4][8 vec4 carve slopes]
        // [roll profile meta vec4][8 vec4 roll slopes] — must stay in
        // lockstep with shader2d's WindowInfo.
        let mut data = [0.0f32; WINDOW_INFO_BYTES as usize / 4];
        data[0] = self.extent.width as f32;
        data[1] = self.extent.height as f32;
        data[2] = self.clip_corner_radius();
        data[3] = crate::layout::corner_shape();
        if let Some(slopes) = crate::layout::bevel_profile_slopes() {
            data[4] = 1.0;
            data[5] = crate::layout::BEVEL_PROFILE_SAMPLES as f32;
            data[8..8 + slopes.len()].copy_from_slice(&slopes);
        }
        if let Some(slopes) = crate::layout::roll_profile_slopes() {
            data[40] = 1.0;
            data[41] = crate::layout::BEVEL_PROFILE_SAMPLES as f32;
            data[44..44 + slopes.len()].copy_from_slice(&slopes);
        }
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

    fn create_swapchain(&mut self) {
        unsafe {
            let caps = self.core
                .surface_loader
                .get_physical_device_surface_capabilities(self.core.physical_device, self.surface)
                .expect("Failed to query surface capabilities");

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
                        .present_mode(vk::PresentModeKHR::FIFO)
                        .clipped(true)
                        .old_swapchain(old_swapchain),
                    None,
                )
                .expect("Failed to create swapchain");
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
                .expect("Failed to get swapchain images");
            self.swapchain_images = images.clone();
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
    }

    fn recreate_swapchain(&mut self) {
        unsafe {
            let _ = self.core.device.device_wait_idle();
        }
        self.destroy_swapchain_resources();
        self.create_swapchain();
        self.write_window_info();
        self.sync_backdrop_targets();
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
    fn sync_backdrop_targets(&mut self) {
        self.scene.resize(
            &self.core.device,
            self.core.allocator.as_mut().unwrap(),
            self.extent,
        );
        clear_image_to_shader_read(
            &self.core.device,
            self.core.queue,
            self.core.command_pool,
            self.scene.backdrop_image,
        );
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

    /// Replace the path tracer's scene (triangles in the space the camera's
    /// `inv_mvp` unprojects into). Builds the BVH on the CPU and uploads it;
    /// waits for the GPU to go idle first — scene replacement is rare
    /// (geometry rebuilds), matching `update_mesh`. The first call compiles
    /// the compute pipeline.
    pub fn set_rt_scene(&mut self, triangles: &[RtTriangle], materials: &[RtMaterial]) {
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
            )
        });
        rt.set_scene(
            &core.device,
            allocator,
            core.queue,
            core.command_pool,
            triangles,
            materials,
        );
    }

    /// Stage one progressive path-tracing pass into the viewport pane
    /// (physical pixels) for the next `draw_frame`. Call it every frame while
    /// RT mode is on: each frame adds a sample; a camera/pane/scene change
    /// restarts the accumulation. No-op until `set_rt_scene` has run.
    pub fn stage_rt(&mut self, pane: (u32, u32, u32, u32), camera: RtCamera) {
        if let Some(rt) = self.rt.as_mut() {
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

    /// The extent the next `draw_frame` will render at: the pending size when a
    /// swapchain rebuild is queued, otherwise the live one.
    pub fn pending_extent(&self) -> vk::Extent2D {
        if self.swapchain_dirty { self.desired_extent } else { self.extent }
    }

    /// Request a new physical size (from xdg configure / scale changes). Applied
    /// lazily on the next `draw_frame`.
    pub fn resize(&mut self, width: u32, height: u32) {
        let extent = vk::Extent2D { width: width.max(1), height: height.max(1) };
        if extent.width != self.extent.width || extent.height != self.extent.height {
            self.desired_extent = extent;
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
    /// Mirrors `glyphon::TextRenderer::prepare`.
    pub fn prepare_text(
        &mut self,
        font_system: &mut glyphon::FontSystem,
        swash_cache: &mut glyphon::SwashCache,
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
        })
    }

    /// Render one frame: the 2D geometry (optionally as scissored batches),
    /// then any text staged via `prepare_text`, then the overlay vertices on
    /// top. Returns false if the frame was skipped (swapchain rebuild); the
    /// caller just draws again next tick.
    pub fn draw_frame_2d(&mut self, frame2d: Frame2D<'_>) -> bool {
        if self.swapchain_dirty {
            self.swapchain_dirty = false;
            self.recreate_swapchain();
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
                rt.write_frame_uniforms(frame_index);
            }

            // Record.
            let cmd = self.frames[frame_index].cmd;
            self.core.device
                .begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())
                .unwrap();
            self.text.record_upload(&self.core.device, cmd, frame_index);

            // Offscreen 3D pass (only when a scene was staged); leaves the
            // backdrop in TRANSFER_SRC.
            let mut scene_recorded = self.scene.record(&self.core.device, cmd, frame_index);

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
            let (ui_pass, ui_clear_values): (vk::RenderPass, &[vk::ClearValue]) = if use_backdrop {
                (self.render_pass_load, &[])
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
            self.core.device.cmd_set_scissor(
                cmd,
                0,
                &[vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: self.extent,
                }],
            );
            let full_scissor = vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: self.extent,
            };
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
                            self.core.device.cmd_set_scissor(cmd, 0, &[img_scissor]);
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
                            self.core.device.cmd_set_scissor(cmd, 0, &[scissor]);
                            // Per-batch rounded-rect clip (fragments outside
                            // discard) + the SDF-lit plate block when this
                            // batch is a plate cover quad.
                            let rr = batch.clip_rrect.unwrap_or([0.0; 5]);
                            let enabled = if batch.clip_rrect.is_some() { 1.0f32 } else { 0.0 };
                            let mut pc = [0.0f32; 32];
                            pc[..5].copy_from_slice(&rr);
                            pc[5] = enabled;
                            pc[7] = clip_shape;
                            if let Some(p) = &batch.plate {
                                pc[6] = p.mode;
                                pc[7] = p.shape;
                                pc[8..12].copy_from_slice(&p.rect);
                                pc[12..16].copy_from_slice(&p.radii);
                                pc[16..20].copy_from_slice(&p.light);
                                pc[20..24].copy_from_slice(&p.material);
                                pc[24..28].copy_from_slice(&p.host);
                                pc[28..32].copy_from_slice(&p.specular_tint);
                                if p.mode == 1.0 {
                                    // Rebase the feature offset onto this
                                    // frame's UBO slot.
                                    pc[24] += (frame_index * MAX_PLATE_FEATURES) as f32;
                                }
                            }
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
                    self.core.device.cmd_set_scissor(cmd, 0, &[img_scissor]);
                    self.image.record_quad(&self.core.device, cmd, frame_index, k, q.image);
                }
                // Restore for the text/overlay draws.
                self.core.device.cmd_set_scissor(cmd, 0, &[full_scissor]);
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
                let pc = [0.0f32; 32];
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

            let swapchains = [self.swapchain];
            let image_indices = [image_index];
            let present = vk::PresentInfoKHR::default()
                .wait_semaphores(&signal_semaphores)
                .swapchains(&swapchains)
                .image_indices(&image_indices);
            match self.swapchain_loader.queue_present(self.core.queue, &present) {
                Ok(suboptimal) => {
                    if suboptimal {
                        self.swapchain_dirty = true;
                    }
                }
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    self.swapchain_dirty = true;
                }
                Err(e) => log::error!("queue_present failed: {e:?}"),
            }

            self.frame_index = (self.frame_index + 1) % FRAMES_IN_FLIGHT;
        }
        true
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

    #[test]
    fn scene3d_compiles() {
        assert!(!super::scene3d_spirv().is_empty());
    }
}
