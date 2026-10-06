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

    /// Drain the global upload/free queue. Uploads are synchronous one-time
    /// submits (rare: images load once); frees wait for device idle.
    pub(crate) fn process_pending(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
    ) {
        if !self.shared_uploads {
            return;
        }
        let pending: Vec<Pending> = take_pending();
        if pending.is_empty() {
            self.staging_idle = self.staging_idle.saturating_add(1);
            if self.staging_idle > STAGING_IDLE_FRAMES {
                if let Some(mut idle) = self
                    .staging
                    .take_if(|b| b.size > STAGING_KEEP_BYTES)
                {
                    // Safe without a wait for the same reason `staging_for`
                    // needs none: every copy waits for itself before
                    // returning, so nothing is reading this buffer here.
                    destroy_cpu_buffer(device, allocator, &mut idle);
                }
            }
            return;
        }
        self.staging_idle = 0;
        for item in pending {
            match item {
                Pending::Upload { id, pixels, width, height, format, mips } => {
                    self.upload(
                        device, allocator, queue, command_pool, id, &pixels, width, height, format,
                        mips,
                    );
                    retire_buffer(pixels);
                }
                Pending::Update { id, pixels, width, height, format } => {
                    // Same picture, new contents: copy into the image that is
                    // already there. Anything else about it changing (a window
                    // resize) falls back to building a fresh one under the
                    // same id.
                    let reusable = self.images.get(&id).is_some_and(|gpu| {
                        gpu.width == width && gpu.height == height && gpu.format == format
                    });
                    if reusable {
                        self.write_into(device, allocator, queue, command_pool, id, &pixels);
                    } else {
                        // Mipmapped if what it replaces was: the id is the
                        // same picture at another size.
                        let mips = self.images.get(&id).is_some_and(|gpu| gpu.mip_levels > 1);
                        self.destroy_image(device, allocator, id);
                        self.upload(
                            device, allocator, queue, command_pool, id, &pixels, width, height,
                            format, mips,
                        );
                    }
                    retire_buffer(pixels);
                }
                Pending::UpdateRegions { id, pixels, width, height, format, regions } => {
                    // Only into the picture these regions were cut from. A
                    // fresh image here would be blank outside them, so a
                    // mismatch writes nothing; see `update_pixel_regions`.
                    let matches = self.images.get(&id).is_some_and(|gpu| {
                        gpu.width == width && gpu.height == height && gpu.format == format
                    });
                    if matches {
                        self.write_regions(
                            device, allocator, queue, command_pool, id, &pixels, &regions,
                        );
                    } else {
                        log::debug!("image {id}: region update for a {width}x{height} image it no longer matches, dropped");
                    }
                    retire_buffer(pixels);
                }
                Pending::Free { id } => self.destroy_image(device, allocator, id),
            }
        }
    }

    /// Tear one image down. Destroying something the GPU may still be reading
    /// needs the device idle, which is why this is not on the per-frame path
    /// any more: a streaming caller updates in place and never gets here until
    /// it is finished with the image for good.
    fn destroy_image(&mut self, device: &ash::Device, allocator: &mut Allocator, id: u32) {
        let Some(mut gpu) = self.images.remove(&id) else { return };
        unsafe {
            let _ = device.device_wait_idle();
            device.destroy_image_view(gpu.view, None);
            device.destroy_image(gpu.image, None);
            let _ = device.free_descriptor_sets(self.descriptor_pool, &[gpu.descriptor_set]);
        }
        if let Some(a) = gpu.allocation.take() {
            let _ = allocator.free(a);
        }
    }

    /// The shared staging buffer, grown if this upload needs more room.
    fn staging_for(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        bytes: usize,
    ) -> &mut AllocatedBuffer {
        let too_small = self
            .staging
            .as_ref()
            .is_none_or(|b| (b.size as usize) < bytes);
        if too_small {
            if let Some(mut old) = self.staging.take() {
                // No wait: every copy submitted from here is followed by
                // `queue_wait_idle` before its caller returns, so no GPU work
                // is referencing the old buffer by the time anything asks for
                // a bigger one.
                destroy_cpu_buffer(device, allocator, &mut old);
            }
            self.staging = Some(create_cpu_buffer(
                device,
                allocator,
                bytes as vk::DeviceSize,
                vk::BufferUsageFlags::TRANSFER_SRC,
                "image-staging",
            ));
        }
        self.staging.as_mut().expect("staging buffer")
    }

    #[allow(clippy::too_many_arguments)]
    /// Upload RGBA8 pixels into THIS table now, rather than queueing them for
    /// whichever renderer drains the shared queue next — for a renderer with
    /// images of its own (see [`Self::shared_uploads`]). The id comes from
    /// the process-wide counter, so it never collides with a queued one.
    pub(crate) fn upload_now(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        pixels: &[u8],
        width: u32,
        height: u32,
    ) -> u32 {
        let id = crate::draw::images::next_image_id();
        self.upload(device, allocator, queue, command_pool, id, pixels, width, height, PixelFormat::Rgba, false);
        id
    }

    fn upload(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        id: u32,
        pixels: &[u8],
        width: u32,
        height: u32,
        format: PixelFormat,
        mips: bool,
    ) {
        if self.images.len() as u32 >= MAX_IMAGES {
            log::error!("image registry full ({MAX_IMAGES}); dropping upload {id}");
            return;
        }
        let mip_levels =
            if mips && self.mips_supported { mip_level_count(width, height) } else { 1 };
        // Each level is blitted from the one above it, so a mipmapped image
        // is a transfer's source as well as its destination.
        let usage = if mip_levels > 1 {
            vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_DST
                | vk::ImageUsageFlags::TRANSFER_SRC
        } else {
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST
        };
        unsafe {
            let image = device
                .create_image(
                    &vk::ImageCreateInfo::default()
                        .image_type(vk::ImageType::TYPE_2D)
                        // SRGB, not UNORM: uploaded pixels are sRGB-encoded
                        // (rasterized SVGs, decoded PNGs, Servo page readback),
                        // and the swapchain is an sRGB format, so the hardware
                        // encodes shader output on write. Sampling as UNORM
                        // fed those bytes through as if linear and encoded
                        // them a second time, lightening every midtone —
                        // a page's #101010 measured (71,71,71) on screen.
                        // Decoding on sample makes the round trip exact.
                        //
                        // Which channel comes first is the caller's business
                        // (`PixelFormat`): the sampler reads either order at
                        // no cost, so a BGRA source never needs a CPU swizzle.
                        .format(format.vk())
                        .extent(vk::Extent3D { width, height, depth: 1 })
                        .mip_levels(mip_levels)
                        .array_layers(1)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .tiling(vk::ImageTiling::OPTIMAL)
                        .usage(usage)
                        .initial_layout(vk::ImageLayout::UNDEFINED),
                    None,
                )
                .expect("Failed to create user image");
            let requirements = device.get_image_memory_requirements(image);
            let allocation = allocator
                .allocate(&AllocationCreateDesc {
                    name: "user-image",
                    requirements,
                    location: MemoryLocation::GpuOnly,
                    linear: false,
                    allocation_scheme: AllocationScheme::GpuAllocatorManaged,
                })
                .expect("Failed to allocate user image memory");
            device
                .bind_image_memory(image, allocation.memory(), allocation.offset())
                .expect("Failed to bind user image memory");

            let staging_buffer = {
                let staging = self.staging_for(device, allocator, pixels.len());
                staging.allocation.as_mut().unwrap().mapped_slice_mut().unwrap()[..pixels.len()]
                    .copy_from_slice(pixels);
                staging.buffer
            };

            let range = vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .level_count(mip_levels)
                .layer_count(1);
            let cmd = device
                .allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(command_pool)
                        .level(vk::CommandBufferLevel::PRIMARY)
                        .command_buffer_count(1),
                )
                .expect("Failed to allocate upload command buffer")[0];
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
                    .subresource_range(range)],
            );
            device.cmd_copy_buffer_to_image(
                cmd,
                staging_buffer,
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
            record_levels(device, cmd, image, width, height, mip_levels);
            device.end_command_buffer(cmd).unwrap();
            let cmds = [cmd];
            device
                .queue_submit(queue, &[vk::SubmitInfo::default().command_buffers(&cmds)], vk::Fence::null())
                .expect("Image upload submit failed");
            device.queue_wait_idle(queue).expect("Image upload wait failed");
            device.free_command_buffers(command_pool, &cmds);

            let view = device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(format.vk())
                        .subresource_range(range),
                    None,
                )
                .expect("Failed to create user image view");

            let set_layouts = [self.descriptor_set_layout];
            let descriptor_set = device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(self.descriptor_pool)
                        .set_layouts(&set_layouts),
                )
                .expect("Failed to allocate image descriptor set")[0];
            let image_infos = [vk::DescriptorImageInfo::default()
                .image_view(view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let sampler_infos = [vk::DescriptorImageInfo::default().sampler(self.sampler)];
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
                ],
                &[],
            );

            self.images.insert(
                id,
                GpuImage {
                    image,
                    view,
                    allocation: Some(allocation),
                    descriptor_set,
                    width,
                    height,
                    format,
                    mip_levels,
                },
            );
        }
    }

    /// Replace an existing image's contents in place.
    ///
    /// The whole streaming path. Against `upload` it skips creating an image,
    /// allocating its memory, allocating and writing a descriptor set, and —
    /// the expensive one — destroying last frame's image, which needs the
    /// device idle and so waits for every frame still in flight.
    ///
    /// The copy is still its own submission followed by `queue_wait_idle`,
    /// and that wait is doing real work: the image is one the *previous*
    /// frame may still be sampling, and two submissions on one queue are not
    /// ordered against each other by anything weaker. Lifting it means giving
    /// each streaming image a second buffer to alternate between and
    /// recording the copy into the frame's own command buffer, which is the
    /// next step rather than this one.
    fn write_into(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        id: u32,
        pixels: &[u8],
    ) {
        let Some(&GpuImage { width, height, .. }) = self.images.get(&id) else {
            return;
        };
        self.write_regions(device, allocator, queue, command_pool, id, pixels, &[(0, 0, width, height)]);
    }

    /// [`Self::write_into`] for part of the image: each region's texels,
    /// packed one after another in `pixels`, copied into place in one
    /// submission. Everything outside the regions keeps what it held.
    #[allow(clippy::too_many_arguments)]
    fn write_regions(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        id: u32,
        pixels: &[u8],
        regions: &[Region],
    ) {
        let Some(&GpuImage { image, width, height, mip_levels, .. }) = self.images.get(&id) else {
            return;
        };
        let mut offset = 0u64;
        let copies: Vec<vk::BufferImageCopy> = regions
            .iter()
            .map(|&(x, y, w, h)| {
                let copy = vk::BufferImageCopy::default()
                    .buffer_offset(offset)
                    .buffer_row_length(w)
                    .buffer_image_height(h)
                    .image_subresource(
                        vk::ImageSubresourceLayers::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .layer_count(1),
                    )
                    .image_offset(vk::Offset3D { x: x as i32, y: y as i32, z: 0 })
                    .image_extent(vk::Extent3D { width: w, height: h, depth: 1 });
                offset += (w * h * 4) as u64;
                copy
            })
            .collect();
        unsafe {
            let staging_buffer = {
                let staging = self.staging_for(device, allocator, pixels.len());
                staging.allocation.as_mut().unwrap().mapped_slice_mut().unwrap()[..pixels.len()]
                    .copy_from_slice(pixels);
                staging.buffer
            };
            let range = vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .level_count(mip_levels)
                .layer_count(1);
            let cmd = device
                .allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(command_pool)
                        .level(vk::CommandBufferLevel::PRIMARY)
                        .command_buffer_count(1),
                )
                .expect("Failed to allocate update command buffer")[0];
            device
                .begin_command_buffer(
                    cmd,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .unwrap();
            // Unlike a fresh upload this image holds a picture already, and
            // it is in the layout the shader reads. Transitioning *from* that
            // layout (not UNDEFINED) keeps the contents, which a region
            // update depends on: everything outside its regions must survive.
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::SHADER_READ)
                    .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(image)
                    .subresource_range(range)],
            );
            device.cmd_copy_buffer_to_image(
                cmd,
                staging_buffer,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &copies,
            );
            record_levels(device, cmd, image, width, height, mip_levels);
            device.end_command_buffer(cmd).unwrap();
            let cmds = [cmd];
            device
                .queue_submit(
                    queue,
                    &[vk::SubmitInfo::default().command_buffers(&cmds)],
                    vk::Fence::null(),
                )
                .expect("Image update submit failed");
            device.queue_wait_idle(queue).expect("Image update wait failed");
            device.free_command_buffers(command_pool, &cmds);
        }
    }

    /// After the frame fence: build this frame's quad vertices (6 per image,
    /// in `images` order).
    pub(crate) fn write_frame_buffer(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        frame_index: usize,
        images: &[ImageQuad],
        extent: vk::Extent2D,
    ) {
        let verts = crate::draw::glyphs::image_quad_vertices(images, extent.width, extent.height);
        let bytes: &[u8] = bytemuck::cast_slice(&verts);
        let buf = &mut self.frame_buffers[frame_index];
        if bytes.len() as vk::DeviceSize > buf.size {
            let mut old = std::mem::replace(buf, AllocatedBuffer::null());
            destroy_cpu_buffer(device, allocator, &mut old);
            *buf = create_cpu_buffer(
                device,
                allocator,
                (bytes.len() as vk::DeviceSize).next_power_of_two(),
                vk::BufferUsageFlags::VERTEX_BUFFER,
                "image-quads",
            );
        }
        if !bytes.is_empty() {
            buf.allocation.as_mut().unwrap().mapped_slice_mut().unwrap()[..bytes.len()]
                .copy_from_slice(bytes);
        }
    }

    /// The descriptor set an uploaded image is drawn with, or None while its
    /// upload has not landed (or after it was freed).
    pub(crate) fn descriptor_set(&self, image_id: u32) -> Option<vk::DescriptorSet> {
        self.images.get(&image_id).map(|gpu| gpu.descriptor_set)
    }

    /// An uploaded image's view and size, for a pass that samples it under
    /// bindings of its own (the path tracer). None while its upload has not
    /// landed.
    pub(crate) fn view_and_size(&self, image_id: u32) -> Option<(vk::ImageView, u32, u32)> {
        self.images.get(&image_id).map(|gpu| (gpu.view, gpu.width, gpu.height))
    }

    /// Record one image quad (index `i` of this frame's list). The caller
    /// restores its own pipeline/scissor state afterwards. Returns false if the
    /// image hasn't finished uploading (draw skipped).
    pub(crate) fn record_quad(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame_index: usize,
        i: usize,
        image_id: u32,
    ) -> bool {
        let Some(gpu) = self.images.get(&image_id) else {
            return false;
        };
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                0,
                &[gpu.descriptor_set],
                &[],
            );
            device.cmd_bind_vertex_buffers(cmd, 0, &[self.frame_buffers[frame_index].buffer], &[0]);
            device.cmd_draw(cmd, 6, 1, (i * 6) as u32, 0);
        }
        true
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

/// Finish an image whose level 0 has just been copied into and whose every
/// level is in TRANSFER_DST: build each further level from the one above
/// it, and leave them all in the layout the shader reads.
///
/// A blit, filtered linearly, halves a level into the next: each texel of
/// the smaller is the mean of the four it covers. The formats are sRGB, so
/// the mean is taken of the light and not of its encoding — a level of a
/// black and white check is the grey that looks half as bright, where a mean
/// of the bytes is darker than that.
///
/// With one level there is nothing to build and this is the transition the
/// plain upload always ended on.
unsafe fn record_levels(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    width: u32,
    height: u32,
    mip_levels: u32,
) {
    let level = |i: u32| {
        vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .base_mip_level(i)
            .level_count(1)
            .layer_count(1)
    };
    let barrier = |range, from_access, to_access, from_layout, to_layout, from_stage, to_stage| {
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
    let (mut w, mut h) = (width as i32, height as i32);
    for i in 1..mip_levels {
        let (next_w, next_h) = ((w / 2).max(1), (h / 2).max(1));
        barrier(
            level(i - 1),
            vk::AccessFlags::TRANSFER_WRITE,
            vk::AccessFlags::TRANSFER_READ,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::TRANSFER,
        );
        let layers = |i: u32| {
            vk::ImageSubresourceLayers::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .mip_level(i)
                .layer_count(1)
        };
        device.cmd_blit_image(
            cmd,
            image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[vk::ImageBlit::default()
                .src_subresource(layers(i - 1))
                .src_offsets([vk::Offset3D::default(), vk::Offset3D { x: w, y: h, z: 1 }])
                .dst_subresource(layers(i))
                .dst_offsets([
                    vk::Offset3D::default(),
                    vk::Offset3D { x: next_w, y: next_h, z: 1 },
                ])],
            vk::Filter::LINEAR,
        );
        barrier(
            level(i - 1),
            vk::AccessFlags::TRANSFER_READ,
            vk::AccessFlags::SHADER_READ,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
        );
        (w, h) = (next_w, next_h);
    }
    barrier(
        level(mip_levels - 1),
        vk::AccessFlags::TRANSFER_WRITE,
        vk::AccessFlags::SHADER_READ,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        vk::PipelineStageFlags::TRANSFER,
        vk::PipelineStageFlags::FRAGMENT_SHADER,
    );
}

#[cfg(test)]
mod tests {
    use super::mip_level_count;

    /// A full chain halves the longer side down to one texel.
    #[test]
    fn a_mip_chain_ends_at_one_texel() {
        assert_eq!(mip_level_count(1, 1), 1);
        assert_eq!(mip_level_count(2, 1), 2);
        assert_eq!(mip_level_count(256, 256), 9);
        assert_eq!(mip_level_count(257, 3), 9);
        assert_eq!(mip_level_count(2550, 3300), 12);
        assert_eq!(mip_level_count(0, 0), 1);
    }
}
