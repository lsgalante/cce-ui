//! Text on ash: cosmic-text shaping + swash rasterization into a self-managed
//! RGBA glyph atlas, drawn by the glyph.wgsl pipeline inside the renderer's
//! render pass. (cosmic-text used to be reached through glyphon's re-export;
//! the dependency is direct now that the wgpu path is gone, pinned to the same
//! version, so shaping behavior and fonts are unchanged.)
//!
//! `TextSpan` mirrors what was `glyphon::TextArea` (buffer + position + scale +
//! bounds + default color) — the shape the wgpu-era cutover was written against.
//!
//! Atlas strategy: shelf packing into a 1024² RGBA8 image with a CPU mirror.
//! When new glyphs land, the whole mirror is re-uploaded before the next render
//! pass (bounded 4 MiB, and only on glyph-miss frames); if the atlas fills, it is
//! cleared and repacked with just the current frame's glyphs. Mask glyphs are
//! stored white-with-alpha, color (emoji) glyphs as-is drawn with a white vertex
//! color — glyph.wgsl multiplies either by the vertex color.


use ash::vk;
use gpu_allocator::vulkan::{
    Allocation, AllocationCreateDesc, AllocationScheme, Allocator,
};
use gpu_allocator::MemoryLocation;


use cosmic_text::{FontSystem, SwashCache};

use super::renderer::{create_cpu_buffer, destroy_cpu_buffer, AllocatedBuffer};
pub use crate::draw::TextSpan;
use crate::draw::glyphs::{AtlasChange, GlyphAtlas, GlyphVertex, ATLAS_SIZE};

struct TextFrame {
    vertex: AllocatedBuffer,
    vertex_count: u32,
    /// Holds what this frame copies into the atlas image: one region's rows
    /// packed tight, or the whole atlas after a repack. Grown on demand —
    /// a full-size buffer per frame in flight was 8 MiB of mapped memory
    /// per window, held whether or not a glyph ever changed again.
    staging: AllocatedBuffer,
    /// What this frame's staging holds for the image and the atlas
    /// generation it brings the image to, recorded by `write_frame_buffers`
    /// and copied by `record_upload` (which is when the image counts as
    /// holding it).
    pending: Option<(AtlasChange, u64)>,
}

/// Staging a frame starts with: room for a run of new glyphs.
const STAGING_START: vk::DeviceSize = 64 * 1024;

pub(crate) struct TextStage {
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_set: vk::DescriptorSet,
    shader_module: vk::ShaderModule,
    sampler: vk::Sampler,

    atlas_image: vk::Image,
    atlas_view: vk::ImageView,
    atlas_allocation: Option<Allocation>,
    /// The atlas and glyph quads (CPU side, shared with every renderer).
    atlas: GlyphAtlas,
    atlas_initialized: bool,
    /// The atlas generation the GPU image holds (copied, or about to be by
    /// a recorded frame). One image serves every frame in flight, so this
    /// is one number, not one per frame. 1 is the cleared atlas, which the
    /// image is cleared to on first use rather than uploaded.
    image_generation: u64,
    frames: Vec<TextFrame>,
}

impl TextStage {
    pub(crate) fn new(
        device: &ash::Device,
        allocator: &mut Allocator,
        render_pass: vk::RenderPass,
        frames_in_flight: usize,
    ) -> Self {
        unsafe {
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
            ];
            let descriptor_set_layout = device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
                .expect("Failed to create text descriptor set layout");
            let set_layouts = [descriptor_set_layout];
            let pipeline_layout = device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts),
                    None,
                )
                .expect("Failed to create text pipeline layout");

            let spirv = super::renderer::glyph_spirv();
            let shader_module = device
                .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(spirv), None)
                .expect("Failed to create glyph shader module");

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
                .stride(std::mem::size_of::<GlyphVertex>() as u32)
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
                .expect("Failed to create glyph pipeline")[0];

            let atlas_image = device
                .create_image(
                    &vk::ImageCreateInfo::default()
                        .image_type(vk::ImageType::TYPE_2D)
                        .format(vk::Format::R8G8B8A8_UNORM)
                        .extent(vk::Extent3D { width: ATLAS_SIZE, height: ATLAS_SIZE, depth: 1 })
                        .mip_levels(1)
                        .array_layers(1)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .tiling(vk::ImageTiling::OPTIMAL)
                        .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST)
                        .initial_layout(vk::ImageLayout::UNDEFINED),
                    None,
                )
                .expect("Failed to create atlas image");
            let requirements = device.get_image_memory_requirements(atlas_image);
            let atlas_allocation = allocator
                .allocate(&AllocationCreateDesc {
                    name: "glyph-atlas",
                    requirements,
                    location: MemoryLocation::GpuOnly,
                    linear: false,
                    allocation_scheme: AllocationScheme::GpuAllocatorManaged,
                })
                .expect("Failed to allocate atlas memory");
            device
                .bind_image_memory(atlas_image, atlas_allocation.memory(), atlas_allocation.offset())
                .expect("Failed to bind atlas memory");
            let atlas_view = device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(atlas_image)
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
                .expect("Failed to create atlas view");

            // Glyphs are sampled 1:1; NEAREST keeps them crisp.
            let sampler = device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::NEAREST)
                        .min_filter(vk::Filter::NEAREST)
                        .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                    None,
                )
                .expect("Failed to create atlas sampler");

            let pool_sizes = [
                vk::DescriptorPoolSize::default()
                    .ty(vk::DescriptorType::SAMPLED_IMAGE)
                    .descriptor_count(1),
                vk::DescriptorPoolSize::default()
                    .ty(vk::DescriptorType::SAMPLER)
                    .descriptor_count(1),
            ];
            let descriptor_pool = device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .max_sets(1)
                        .pool_sizes(&pool_sizes),
                    None,
                )
                .expect("Failed to create text descriptor pool");
            let descriptor_set = device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(descriptor_pool)
                        .set_layouts(&set_layouts),
                )
                .expect("Failed to allocate text descriptor set")[0];
            let image_infos = [vk::DescriptorImageInfo::default()
                .image_view(atlas_view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let sampler_infos = [vk::DescriptorImageInfo::default().sampler(sampler)];
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

            let frames = (0..frames_in_flight)
                .map(|_| TextFrame {
                    vertex: create_cpu_buffer(
                        device,
                        allocator,
                        64 * 1024,
                        vk::BufferUsageFlags::VERTEX_BUFFER,
                        "glyph-vertices",
                    ),
                    vertex_count: 0,
                    staging: create_cpu_buffer(
                        device,
                        allocator,
                        STAGING_START,
                        vk::BufferUsageFlags::TRANSFER_SRC,
                        "atlas-staging",
                    ),
                    pending: None,
                })
                .collect();

            TextStage {
                pipeline,
                pipeline_layout,
                descriptor_set_layout,
                descriptor_pool,
                descriptor_set,
                shader_module,
                sampler,
                atlas_image,
                atlas_view,
                atlas_allocation: Some(atlas_allocation),
                atlas: GlyphAtlas::new(),
                atlas_initialized: false,
                image_generation: 1,
                frames,
            }
        }
    }

    /// Build this frame's glyph quads against `extent` (see [`GlyphAtlas::prepare`]).
    pub(crate) fn prepare(
        &mut self,
        font_system: &mut FontSystem,
        swash_cache: &mut SwashCache,
        spans: &[TextSpan<'_>],
        extent: vk::Extent2D,
    ) {
        self.atlas.prepare(font_system, swash_cache, spans, extent.width, extent.height);
    }

    /// Called after this frame's fence has been waited: copy the current text
    /// vertices into the frame's buffer and refresh its staging copy if the
    /// atlas changed. `pending_vertices` is RETAINED — it is the staged text
    /// state, replaced only by the next `prepare` — so frames rendered without
    /// a re-prepare (progressive RT refinement, animation ticks) keep their
    /// text instead of alternating to an empty buffer.
    pub(crate) fn write_frame_buffers(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        frame_index: usize,
    ) {
        let frame = &mut self.frames[frame_index];

        let bytes: &[u8] = bytemuck::cast_slice(self.atlas.vertices());
        let needed = bytes.len() as vk::DeviceSize;
        if needed > frame.vertex.size {
            let mut old = std::mem::replace(&mut frame.vertex, AllocatedBuffer::null());
            destroy_cpu_buffer(device, allocator, &mut old);
            frame.vertex = create_cpu_buffer(
                device,
                allocator,
                needed.next_power_of_two(),
                vk::BufferUsageFlags::VERTEX_BUFFER,
                "glyph-vertices",
            );
        }
        if !bytes.is_empty() {
            frame.vertex.allocation.as_mut().unwrap().mapped_slice_mut().unwrap()
                [..bytes.len()]
                .copy_from_slice(bytes);
        }
        frame.vertex_count = self.atlas.vertices().len() as u32;

        // Stage only what changed since the image was last brought up to
        // date: the rows of one region, or everything after a repack. Until
        // 2026-10-06 each frame in flight re-staged and re-copied the whole
        // 4 MiB atlas for any new glyph.
        let change = self.atlas.changes_since(self.image_generation);
        let frame = &mut self.frames[frame_index];
        frame.pending = None;
        let (row_bytes, rows, x0, y0) = match change {
            AtlasChange::None => return,
            AtlasChange::Full => (ATLAS_SIZE * 4, ATLAS_SIZE, 0, 0),
            AtlasChange::Region { x, y, w, h } => (w * 4, h, x, y),
        };
        let needed = (row_bytes * rows) as vk::DeviceSize;
        if needed > frame.staging.size {
            let mut old = std::mem::replace(&mut frame.staging, AllocatedBuffer::null());
            destroy_cpu_buffer(device, allocator, &mut old);
            frame.staging = create_cpu_buffer(
                device,
                allocator,
                needed.next_power_of_two(),
                vk::BufferUsageFlags::TRANSFER_SRC,
                "atlas-staging",
            );
        }
        let pixels = self.atlas.pixels();
        let mapped = frame.staging.allocation.as_mut().unwrap().mapped_slice_mut().unwrap();
        let stride = (ATLAS_SIZE * 4) as usize;
        for r in 0..rows as usize {
            let src = (y0 as usize + r) * stride + x0 as usize * 4;
            let dst = r * row_bytes as usize;
            mapped[dst..dst + row_bytes as usize].copy_from_slice(&pixels[src..src + row_bytes as usize]);
        }
        frame.pending = Some((change, self.atlas.generation()));
    }

    /// Record the atlas upload (if this frame's staging is newer than the image).
    /// Must be called outside a render pass.
    pub(crate) fn record_upload(&mut self, device: &ash::Device, cmd: vk::CommandBuffer, frame_index: usize) {
        let frame = &mut self.frames[frame_index];
        let pending = frame.pending.take();
        if pending.is_none() && self.atlas_initialized {
            return;
        }
        if let Some((_, generation)) = pending {
            self.image_generation = generation;
        }
        let pending = pending.map(|(change, _)| change);

        let range = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .level_count(1)
            .layer_count(1);
        let (old_layout, src_stage, src_access) = if self.atlas_initialized {
            (
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::AccessFlags::SHADER_READ,
            )
        } else {
            (
                vk::ImageLayout::UNDEFINED,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::AccessFlags::empty(),
            )
        };
        self.atlas_initialized = true;

        unsafe {
            device.cmd_pipeline_barrier(
                cmd,
                src_stage,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[vk::ImageMemoryBarrier::default()
                    .src_access_mask(src_access)
                    .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .old_layout(old_layout)
                    .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(self.atlas_image)
                    .subresource_range(range)],
            );
            // A fresh image is cleared to the empty atlas rather than
            // uploaded from it (`image_generation` starts at the cleared one).
            if old_layout == vk::ImageLayout::UNDEFINED {
                device.cmd_clear_color_image(
                    cmd,
                    self.atlas_image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &vk::ClearColorValue { float32: [0.0; 4] },
                    &[range],
                );
            }
            let region = match pending {
                Some(AtlasChange::Full) => Some((0, 0, ATLAS_SIZE, ATLAS_SIZE)),
                Some(AtlasChange::Region { x, y, w, h }) => Some((x, y, w, h)),
                Some(AtlasChange::None) | None => None,
            };
            if let Some((x, y, w, h)) = region {
                device.cmd_copy_buffer_to_image(
                    cmd,
                    frame.staging.buffer,
                    self.atlas_image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[vk::BufferImageCopy::default()
                        .buffer_offset(0)
                        .buffer_row_length(w)
                        .buffer_image_height(h)
                        .image_subresource(
                            vk::ImageSubresourceLayers::default()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .layer_count(1),
                        )
                        .image_offset(vk::Offset3D { x: x as i32, y: y as i32, z: 0 })
                        .image_extent(vk::Extent3D { width: w, height: h, depth: 1 })],
                );
            }
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
                    .image(self.atlas_image)
                    .subresource_range(range)],
            );
        }
    }

    /// Record the glyph draw. Must be called inside the render pass, after the
    /// 2D quads (text goes on top). Viewport/scissor are inherited (dynamic,
    /// already set by the caller).
    pub(crate) fn record_draw(&self, device: &ash::Device, cmd: vk::CommandBuffer, frame_index: usize) {
        let frame = &self.frames[frame_index];
        if frame.vertex_count == 0 || !self.atlas_initialized {
            return;
        }
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                0,
                &[self.descriptor_set],
                &[],
            );
            device.cmd_bind_vertex_buffers(cmd, 0, &[frame.vertex.buffer], &[0]);
            device.cmd_draw(cmd, frame.vertex_count, 1, 0, 0);
        }
    }

    pub(crate) fn destroy(&mut self, device: &ash::Device, allocator: &mut Allocator) {
        unsafe {
            for frame in &mut self.frames {
                let mut vertex = std::mem::replace(&mut frame.vertex, AllocatedBuffer::null());
                destroy_cpu_buffer(device, allocator, &mut vertex);
                let mut staging = std::mem::replace(&mut frame.staging, AllocatedBuffer::null());
                destroy_cpu_buffer(device, allocator, &mut staging);
            }
            device.destroy_sampler(self.sampler, None);
            device.destroy_image_view(self.atlas_view, None);
            device.destroy_image(self.atlas_image, None);
            if let Some(allocation) = self.atlas_allocation.take() {
                let _ = allocator.free(allocation);
            }
            device.destroy_descriptor_pool(self.descriptor_pool, None);
            device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.pipeline_layout, None);
            device.destroy_shader_module(self.shader_module, None);
        }
    }
}
