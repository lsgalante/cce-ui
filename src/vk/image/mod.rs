//! User images in the 2D pass: upload RGBA pixels once, then draw them as
//! quads interleaved with the display list — 3D previews in graph nodes,
//! thumbnails in the files grid, any raster content in the UI.
//!
//! Upload is decoupled from the renderer because most apps never touch it
//! (the engine runner owns the frame): [`upload_rgba`] queues pixels from any
//! code and returns a stable id usable immediately in draws; the renderer
//! drains the queue at the next frame. [`free_image`] queues destruction the
//! same way.
//!
//! **Two shapes of caller.** Most upload an image once and draw it for the
//! rest of the process: a decoded PNG, a rasterized SVG, a thumbnail. One
//! uploads a *new* image every frame — cce-browser, whose whole page is a
//! readback of what the engine just painted. The one-shot path allocated a
//! fresh `VkImage` per upload and freed the old one behind a
//! `device_wait_idle`, which for a streaming caller meant an allocation, a
//! descriptor set and a full device stall per frame. [`update_pixels`] is the
//! streaming path: same id, same image, same descriptor, contents replaced in
//! place. [`recycle_buffer`] closes the loop on the CPU side by handing back
//! the pixel buffer the renderer has finished with, so a streaming caller
//! refills one buffer instead of allocating a frame-sized `Vec` per frame. Draw ordering comes from [`super::Frame2D::images`]: each
//! [`ImageQuad`] carries the vertex index it sorts before.
//!
//! **An image drawn much smaller than it is wants mipmaps**
//! ([`upload_rgba_mipmapped`]). The sampler filters between the four texels
//! nearest each pixel, which is every texel while the image is drawn near
//! its own size and one in twenty-five once it is drawn at a fifth of it: a
//! hairline is then on screen or not by where the sample happened to land,
//! and crawls as the image moves. It is asked for per image, because the
//! chain is a third more memory and is rebuilt on every update, and most
//! images — an icon, a thumbnail, a page read back at the size it is shown —
//! are drawn at their own size.
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the GPU image, the stage and its limits, the descriptor bindings, building and tearing down the stage |
//! | `pending` | draining the upload queue each frame, and freeing images |
//! | `upload` | staging, a one-shot upload and a batched run of them, recording the copy and the mip chain |
//! | `write` | replacing an image's pixels in place, whole or by regions |
//! | `draw` | the frame's quad vertices, an image's descriptor set and view, recording a quad |

mod draw;
mod pending;
mod upload;
mod write;
#[cfg(test)]
mod tests;

use upload::record_levels;

use std::collections::HashMap;

use ash::vk;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme, Allocator};
use gpu_allocator::MemoryLocation;

use super::renderer::{create_cpu_buffer, destroy_cpu_buffer, AllocatedBuffer};
use crate::draw::images::{image_table_built, retire_buffer, take_pending, Pending};
pub use crate::draw::images::{
    free_image, recycle_buffer, renderer_epoch, update_pixel_regions, update_pixels,
    upload_pixels, upload_rgba, upload_rgba_mipmapped, ImageQuad, PixelFormat, Region,
};

impl PixelFormat {
    fn vk(self) -> vk::Format {
        // SRGB, not UNORM — see the note in `upload`.
        match self {
            Self::Rgba => vk::Format::R8G8B8A8_SRGB,
            Self::Bgra => vk::Format::B8G8R8A8_SRGB,
        }
    }
}

/// How many levels a full mip chain of an image has: halved until the
/// longer side is one texel.
pub(crate) fn mip_level_count(width: u32, height: u32) -> u32 {
    32 - width.max(height).max(1).leading_zeros()
}

/// An image quad's vertex: the glyph shader's, shared with the text stage.
type ImageVertex = crate::draw::glyphs::GlyphVertex;

struct GpuImage {
    image: vk::Image,
    view: vk::ImageView,
    allocation: Option<Allocation>,
    descriptor_set: vk::DescriptorSet,
    /// What the image was created as, so an update can tell "same picture,
    /// new contents" from "different image under the same id".
    width: u32,
    height: u32,
    format: PixelFormat,
    /// Levels in the image: 1 for one uploaded plain, the full chain for a
    /// mipmapped one.
    mip_levels: u32,
}

const MAX_IMAGES: u32 = 256;

/// Most staging one upload run holds; uploads queued back to back past it
/// go up as a further run. Keeps a burst of photographs from asking for a
/// staging buffer the size of all of them at once.
const RUN_STAGING_BYTES: usize = 64 << 20;

/// One image of an upload run (`ImageStage::upload_run`).
struct RunItem<'a> {
    id: u32,
    pixels: &'a [u8],
    width: u32,
    height: u32,
    format: PixelFormat,
    mips: bool,
}

/// Idle frames before the staging buffer is handed back — about two seconds
/// at 60 Hz. Long enough that a burst of uploads reuses one buffer, short
/// enough that a big one-shot upload does not hold its memory.
const STAGING_IDLE_FRAMES: u32 = 120;

/// Below this an idle staging buffer is simply kept; releasing and remaking a
/// small one costs more than it saves.
const STAGING_KEEP_BYTES: vk::DeviceSize = 1 << 20;

/// The bindings of a user image's descriptor set: the texture, then its
/// sampler. One definition, because the 3D scene pass binds these same sets
/// to a pipeline of its own (`SceneImage`), and a set is compatible with a
/// pipeline layout only where the two layouts are defined identically.
pub(crate) fn image_set_bindings() -> [vk::DescriptorSetLayoutBinding<'static>; 2] {
    [
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
    ]
}

pub(crate) struct ImageStage {
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    shader_module: vk::ShaderModule,
    sampler: vk::Sampler,
    /// Whether this device can build a mip chain by blitting.
    mips_supported: bool,
    images: HashMap<u32, GpuImage>,
    /// One host-visible staging buffer, grown to the largest upload and kept.
    /// Uploads are serialized against each other (each waits for its own copy
    /// before returning), so one buffer serves them all — and a streaming
    /// caller stops paying an allocation and a free per frame.
    ///
    /// Kept only while it is being used: a one-shot caller that uploads a
    /// 96 MB photograph should not leave 96 MB of host memory mapped for the
    /// life of the process, so an idle buffer is released (see
    /// `STAGING_IDLE_FRAMES`). A streaming caller touches it every frame and
    /// never reaches that.
    staging: Option<AllocatedBuffer>,
    /// Frames since the staging buffer was last used.
    staging_idle: u32,
    /// Per frame in flight: this frame's quad vertices (6 per ImageQuad).
    frame_buffers: Vec<AllocatedBuffer>,
    /// Whether this table takes the process-wide upload queue. True for a
    /// window's renderer; false for a renderer that keeps images of its own
    /// (the context menu's popup), which must not drain the queue — an
    /// upload meant for the window, queued between the window's frame and
    /// the popup's, would land in the popup's table and never be drawn.
    pub(crate) shared_uploads: bool,
}

impl ImageStage {
    pub(crate) fn new(
        device: &ash::Device,
        allocator: &mut Allocator,
        render_pass: vk::RenderPass,
        frames_in_flight: usize,
        mips_supported: bool,
        max_anisotropy: f32,
    ) -> Self {
        // One image table per renderer, so this is the renderer count — see
        // `renderer_epoch`, which is what tells a cache of ids that its
        // renderer is gone.
        image_table_built();
        unsafe {
            let bindings = image_set_bindings();
            let descriptor_set_layout = device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
                .expect("Failed to create image descriptor set layout");
            let set_layouts = [descriptor_set_layout];
            let pipeline_layout = device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts),
                    None,
                )
                .expect("Failed to create image pipeline layout");

            // Same shader as glyphs: sampled texel * vertex color (+ circle clip).
            let spirv = super::renderer::glyph_spirv();
            let shader_module = device
                .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(spirv), None)
                .expect("Failed to create image shader module");
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
            let vertex_bindings = [vk::VertexInputBindingDescription::default()
                .binding(0)
                .stride(std::mem::size_of::<ImageVertex>() as u32)
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
                    .format(vk::Format::R32G32_SFLOAT)
                    .offset(8),
                vk::VertexInputAttributeDescription::default()
                    .location(2)
                    .binding(0)
                    .format(vk::Format::R32G32B32A32_SFLOAT)
                    .offset(16),
                vk::VertexInputAttributeDescription::default()
                    .location(3)
                    .binding(0)
                    .format(vk::Format::R32G32B32_SFLOAT)
                    .offset(32),
                // Location 4 is declared by the shared glyph shader; without this
                // entry the pipeline is invalid and the attribute reads undefined.
                vk::VertexInputAttributeDescription::default()
                    .location(4)
                    .binding(0)
                    .format(vk::Format::R32G32_SFLOAT)
                    .offset(44),
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
                .expect("Failed to create image pipeline")[0];

            // Linear filtering: thumbnails scale smoothly. Between mip
            // levels too, and across every level an image has — which for
            // an image uploaded plain is the one, so nothing changes for
            // it. Anisotropy is for an image seen at a slant, whose long
            // axis would otherwise be blurred to the level its short one
            // asks for; an axis-aligned quad in the 2D pass has no slant and
            // takes one sample as before.
            let anisotropy = max_anisotropy.min(8.0);
            let sampler = device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::LINEAR)
                        .min_filter(vk::Filter::LINEAR)
                        .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                        .min_lod(0.0)
                        .max_lod(vk::LOD_CLAMP_NONE)
                        .anisotropy_enable(anisotropy > 1.0)
                        .max_anisotropy(anisotropy.max(1.0))
                        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                    None,
                )
                .expect("Failed to create image sampler");

            let pool_sizes = [
                vk::DescriptorPoolSize::default()
                    .ty(vk::DescriptorType::SAMPLED_IMAGE)
                    .descriptor_count(MAX_IMAGES),
                vk::DescriptorPoolSize::default()
                    .ty(vk::DescriptorType::SAMPLER)
                    .descriptor_count(MAX_IMAGES),
            ];
            let descriptor_pool = device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .flags(vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET)
                        .max_sets(MAX_IMAGES)
                        .pool_sizes(&pool_sizes),
                    None,
                )
                .expect("Failed to create image descriptor pool");

            let frame_buffers = (0..frames_in_flight)
                .map(|_| {
                    create_cpu_buffer(
                        device,
                        allocator,
                        16 * 1024,
                        vk::BufferUsageFlags::VERTEX_BUFFER,
                        "image-quads",
                    )
                })
                .collect();

            ImageStage {
                pipeline,
                pipeline_layout,
                descriptor_set_layout,
                descriptor_pool,
                shader_module,
                sampler,
                mips_supported,
                images: HashMap::new(),
                staging: None,
                staging_idle: 0,
                frame_buffers,
                shared_uploads: true,
            }
        }
    }

    pub(crate) fn destroy(&mut self, device: &ash::Device, allocator: &mut Allocator) {
        unsafe {
            for (_, mut gpu) in self.images.drain() {
                device.destroy_image_view(gpu.view, None);
                device.destroy_image(gpu.image, None);
                if let Some(a) = gpu.allocation.take() {
                    let _ = allocator.free(a);
                }
            }
            for buf in &mut self.frame_buffers {
                let mut b = std::mem::replace(buf, AllocatedBuffer::null());
                destroy_cpu_buffer(device, allocator, &mut b);
            }
            // The upload staging buffer, kept between uploads. Missed here
            // until 2026-10-05: gpu-allocator reported it leaked whenever a
            // renderer that had uploaded an image was dropped (a reconnect,
            // or a layer app hiding its surface).
            if let Some(mut staging) = self.staging.take() {
                destroy_cpu_buffer(device, allocator, &mut staging);
            }
            device.destroy_sampler(self.sampler, None);
            device.destroy_descriptor_pool(self.descriptor_pool, None);
            device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.pipeline_layout, None);
            device.destroy_shader_module(self.shader_module, None);
        }
    }
}
