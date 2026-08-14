//! User images in the 2D pass: upload RGBA pixels once, then draw them as
//! quads interleaved with the display list — 3D previews in graph nodes,
//! thumbnails in the files grid, any raster content in the UI.
//!
//! Upload is decoupled from the renderer because most apps never touch it
//! (the engine runner owns the frame): [`upload_rgba`] queues pixels from any
//! code and returns a stable id usable immediately in draws; the renderer
//! drains the queue at the next frame. [`free_image`] queues destruction the
//! same way. Draw ordering comes from [`super::Frame2D::images`]: each
//! [`ImageQuad`] carries the vertex index it sorts before.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use ash::vk;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme, Allocator};
use gpu_allocator::MemoryLocation;

use super::renderer::{create_cpu_buffer, destroy_cpu_buffer, AllocatedBuffer};

/// One image draw in a 2D frame.
pub struct ImageQuad {
    /// Id from [`upload_rgba`].
    pub image: u32,
    /// Destination rect (x, y, w, h) in physical pixels.
    pub rect: (f32, f32, f32, f32),
    pub alpha: f32,
    /// Draw order: this quad renders before the vertex at this index of
    /// `Frame2D::verts` (so vertices below it stay below, later ones cover it).
    /// Use `u32::MAX` to draw on top of all display-list geometry.
    pub z_before: u32,
    /// Optional scissor (x, y, w, h) in physical pixels.
    pub clip: Option<(u32, u32, u32, u32)>,
}

enum Pending {
    Upload { id: u32, pixels: Vec<u8>, width: u32, height: u32 },
    Free { id: u32 },
}

static PENDING: Mutex<Vec<Pending>> = Mutex::new(Vec::new());
static NEXT_ID: AtomicU32 = AtomicU32::new(1);

/// Queue an RGBA8 image for upload; the id is usable in [`ImageQuad`]s right
/// away (draws before the upload lands are skipped, not errors).
pub fn upload_rgba(pixels: Vec<u8>, width: u32, height: u32) -> u32 {
    assert_eq!(pixels.len(), (width * height * 4) as usize, "RGBA8 size mismatch");
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    PENDING.lock().unwrap().push(Pending::Upload { id, pixels, width, height });
    id
}

/// Queue an image's GPU resources for destruction.
pub fn free_image(id: u32) {
    PENDING.lock().unwrap().push(Pending::Free { id });
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
/// Must match `GlyphVertex` field-for-field: both pipelines are fed by the same
/// glyph shader, whose vertex entry point declares locations 0..=4. Omitting
/// `clip_extents` here left location 4 with no `VkVertexInputAttributeDescription`,
/// which the validation layer flags (VUID-VkGraphicsPipelineCreateInfo-Input-07904)
/// and which reads undefined data without `vertexAttributeRobustness`.
struct ImageVertex {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
    clip_circle: [f32; 3],
    clip_extents: [f32; 2],
}

// The pipeline below hardcodes one offset per shader location. Pin the struct to
// them so adding, reordering, or resizing a field fails the build instead of
// silently feeding the shader mis-aligned attributes — the drift that left
// location 4 undescribed. `text.rs` pins `GlyphVertex` to the same layout.
const _: () = {
    assert!(std::mem::size_of::<ImageVertex>() == 52);
    assert!(std::mem::offset_of!(ImageVertex, position) == 0);
    assert!(std::mem::offset_of!(ImageVertex, uv) == 8);
    assert!(std::mem::offset_of!(ImageVertex, color) == 16);
    assert!(std::mem::offset_of!(ImageVertex, clip_circle) == 32);
    assert!(std::mem::offset_of!(ImageVertex, clip_extents) == 44);
};

struct GpuImage {
    image: vk::Image,
    view: vk::ImageView,
    allocation: Option<Allocation>,
    descriptor_set: vk::DescriptorSet,
}

const MAX_IMAGES: u32 = 256;

pub(crate) struct ImageStage {
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    shader_module: vk::ShaderModule,
    sampler: vk::Sampler,
    images: HashMap<u32, GpuImage>,
    /// Per frame in flight: this frame's quad vertices (6 per ImageQuad).
    frame_buffers: Vec<AllocatedBuffer>,
}

impl ImageStage {
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

            // Linear filtering: thumbnails scale smoothly.
            let sampler = device
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
                images: HashMap::new(),
                frame_buffers,
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
        let pending: Vec<Pending> = std::mem::take(&mut *PENDING.lock().unwrap());
        for item in pending {
            match item {
                Pending::Upload { id, pixels, width, height } => {
                    self.upload(device, allocator, queue, command_pool, id, &pixels, width, height);
                }
                Pending::Free { id } => {
                    if let Some(mut gpu) = self.images.remove(&id) {
                        unsafe {
                            let _ = device.device_wait_idle();
                            device.destroy_image_view(gpu.view, None);
                            device.destroy_image(gpu.image, None);
                            let _ = device.free_descriptor_sets(
                                self.descriptor_pool,
                                &[gpu.descriptor_set],
                            );
                        }
                        if let Some(a) = gpu.allocation.take() {
                            let _ = allocator.free(a);
                        }
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
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
    ) {
        if self.images.len() as u32 >= MAX_IMAGES {
            log::error!("image registry full ({MAX_IMAGES}); dropping upload {id}");
            return;
        }
        unsafe {
            let image = device
                .create_image(
                    &vk::ImageCreateInfo::default()
                        .image_type(vk::ImageType::TYPE_2D)
                        .format(vk::Format::R8G8B8A8_UNORM)
                        .extent(vk::Extent3D { width, height, depth: 1 })
                        .mip_levels(1)
                        .array_layers(1)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .tiling(vk::ImageTiling::OPTIMAL)
                        .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST)
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

            let mut staging = create_cpu_buffer(
                device,
                allocator,
                pixels.len() as vk::DeviceSize,
                vk::BufferUsageFlags::TRANSFER_SRC,
                "image-staging",
            );
            staging.allocation.as_mut().unwrap().mapped_slice_mut().unwrap()[..pixels.len()]
                .copy_from_slice(pixels);

            let range = vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .level_count(1)
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
                staging.buffer,
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
                    .subresource_range(range)],
            );
            device.end_command_buffer(cmd).unwrap();
            let cmds = [cmd];
            device
                .queue_submit(queue, &[vk::SubmitInfo::default().command_buffers(&cmds)], vk::Fence::null())
                .expect("Image upload submit failed");
            device.queue_wait_idle(queue).expect("Image upload wait failed");
            device.free_command_buffers(command_pool, &cmds);
            destroy_cpu_buffer(device, allocator, &mut staging);

            let view = device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(vk::Format::R8G8B8A8_UNORM)
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

            self.images.insert(id, GpuImage { image, view, allocation: Some(allocation), descriptor_set });
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
        let sw = extent.width as f32;
        let sh = extent.height as f32;
        let mut verts: Vec<ImageVertex> = Vec::with_capacity(images.len() * 6);
        for q in images {
            let (x, y, w, h) = q.rect;
            let ndc = |px: f32, py: f32| [(px / sw) * 2.0 - 1.0, 1.0 - (py / sh) * 2.0];
            let color = [1.0, 1.0, 1.0, q.alpha];
            let clip_circle = [0.0; 3];
            // Zero extents = the shader's plain-circle clip degenerate case. Inert
            // while clip_circle.z is 0 (the clip branch never runs), but it must be
            // a defined value, not whatever the missing attribute used to read.
            let clip_extents = [0.0; 2];
            let tl = ImageVertex { position: ndc(x, y), uv: [0.0, 0.0], color, clip_circle, clip_extents };
            let tr = ImageVertex { position: ndc(x + w, y), uv: [1.0, 0.0], color, clip_circle, clip_extents };
            let bl = ImageVertex { position: ndc(x, y + h), uv: [0.0, 1.0], color, clip_circle, clip_extents };
            let br = ImageVertex { position: ndc(x + w, y + h), uv: [1.0, 1.0], color, clip_circle, clip_extents };
            verts.extend([tl, tr, bl, tr, br, bl]);
        }
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
            device.destroy_sampler(self.sampler, None);
            device.destroy_descriptor_pool(self.descriptor_pool, None);
            device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.pipeline_layout, None);
            device.destroy_shader_module(self.shader_module, None);
        }
    }
}
