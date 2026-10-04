//! `WebRenderer`: the Vulkan renderer's 2D path, on WebGPU.
//!
//! It draws a [`Frame2D`] the way `vk::VkRenderer::draw_frame_2d` does, batch
//! for batch: the display-list geometry under each batch's scissor with its
//! parameter block (a dynamic-offset uniform here, push constants there),
//! user images interleaved by `z_before`, a blur-behind batch preceded by a
//! snapshot of the frame so far (the pass ends, the target is copied, the
//! pass resumes), then the text, then the overlay geometry. What differs is
//! only what WebGPU has instead:
//!
//! - **The canvas.** A WebGPU canvas takes no sRGB format, so it is
//!   configured with its preferred (unorm) format and an sRGB *view* format,
//!   and drawn through the sRGB view: the hardware encodes on write and blends
//!   in linear, as the Vulkan swapchain's `*_SRGB` format does.
//! - **The parameter block** is a uniform at `@group(1)`, one 256-byte slot
//!   per batch (`draw::shaders::WEBGPU_BLOCK_STRIDE`), plus a zero slot for
//!   the overlay — laid out by the same `draw::batch_push_constants`.
//! - **The backdrop.** With no 3D scene behind the UI, the Vulkan renderer's
//!   backdrop is a cleared image; here it is a 1x1 transparent texture (the
//!   shader samples it clamped, so every texel reads the same zero). The
//!   blur snapshot is a full-size texture of the canvas's sRGB view format.
//! - **No damage.** Every frame is drawn whole (`Frame2D::damage` is a
//!   promise the renderer may use, never a requirement).
//! - **Images have one mip level.** A mipmapped upload is uploaded plain, as
//!   the Vulkan renderer does on a device that cannot blit mips.

use std::collections::HashMap;

use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    gpu_buffer_usage as buffer_usage, gpu_map_mode as map_mode, gpu_shader_stage as shader_stage,
    gpu_texture_usage as texture_usage, GpuAddressMode, GpuBindGroup, GpuBindGroupDescriptor,
    GpuBindGroupEntry, GpuBindGroupLayout, GpuBindGroupLayoutDescriptor, GpuBindGroupLayoutEntry,
    GpuBlendComponent, GpuBlendFactor, GpuBlendOperation, GpuBlendState, GpuBuffer, GpuBufferBinding,
    GpuBufferBindingLayout, GpuBufferBindingType, GpuBufferDescriptor, GpuCanvasAlphaMode,
    GpuCanvasConfiguration, GpuCanvasContext, GpuColorTargetState, GpuCommandEncoder, GpuDevice,
    GpuExtent3dDict, GpuFilterMode, GpuFragmentState, GpuLoadOp, GpuMipmapFilterMode,
    GpuPipelineLayoutDescriptor, GpuPrimitiveState, GpuPrimitiveTopology, GpuQueue,
    GpuRenderPassColorAttachment, GpuRenderPassDescriptor, GpuRenderPassEncoder, GpuRenderPipeline,
    GpuRenderPipelineDescriptor, GpuSampler, GpuSamplerBindingLayout, GpuSamplerBindingType,
    GpuSamplerDescriptor, GpuShaderModuleDescriptor, GpuStoreOp, GpuTexelCopyBufferInfo,
    GpuTexelCopyBufferLayout, GpuTexelCopyTextureInfo, GpuTexture, GpuTextureBindingLayout,
    GpuTextureDescriptor, GpuTextureFormat, GpuTextureSampleType, GpuTextureView,
    GpuTextureViewDescriptor, GpuTextureViewDimension, GpuVertexAttribute, GpuVertexBufferLayout,
    GpuVertexFormat, GpuVertexState, HtmlCanvasElement,
};

use crate::backend::tessellate::Vertex;
use crate::draw::glyphs::{image_quad_vertices, GlyphAtlas, GlyphVertex, ATLAS_SIZE};
use crate::draw::images::{image_table_built, retire_buffer, take_pending, Pending, PixelFormat};
use crate::draw::shaders::{shader2d_for_webgpu, GLYPH, WEBGPU_BLOCK_STRIDE};
use crate::draw::{
    batch_push_constants, relief_px_at, window_info_data, Batch2D, Frame2D, TextSpan, MAX_PLATE_FEATURES,
    PLATE_FEATURE_BYTES, PUSH_CONSTANT_FLOATS, WINDOW_INFO_BYTES,
};

/// The shader's `PlateFeatures` array is 128 entries; the binding must cover
/// all of it even though a frame uses at most [`MAX_PLATE_FEATURES`].
const PLATE_FEATURES_BINDING_BYTES: usize = 128 * PLATE_FEATURE_BYTES;

/// One uploaded user image.
struct WebImage {
    texture: GpuTexture,
    group: GpuBindGroup,
    width: u32,
    height: u32,
    format: PixelFormat,
}

/// A grow-only GPU buffer: replaced by a larger one when a frame needs more.
struct Growable {
    buffer: GpuBuffer,
    size: u32,
    usage: u32,
    label: &'static str,
}

impl Growable {
    fn new(device: &GpuDevice, size: u32, usage: u32, label: &'static str) -> Result<Self, JsValue> {
        let size = size.max(256).next_power_of_two();
        let desc = GpuBufferDescriptor::new(size, usage | buffer_usage::COPY_DST);
        desc.set_label(label);
        Ok(Self { buffer: device.create_buffer(&desc)?, size, usage, label })
    }

    /// Make room for `needed` bytes; true when the buffer was replaced (any
    /// bind group naming it must be rebuilt).
    fn ensure(&mut self, device: &GpuDevice, needed: u32) -> Result<bool, JsValue> {
        if needed <= self.size {
            return Ok(false);
        }
        self.buffer.destroy();
        *self = Self::new(device, needed, self.usage, self.label)?;
        Ok(true)
    }

    fn write(&self, queue: &GpuQueue, bytes: &[u8]) -> Result<(), JsValue> {
        if !bytes.is_empty() {
            queue.write_buffer_with_u32_and_u8_slice(&self.buffer, 0, bytes)?;
        }
        Ok(())
    }
}

/// A frame read back from the canvas: `width` x `height` RGBA8, sRGB-encoded
/// — the bytes the Vulkan swapchain image would hold.
pub struct Capture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub struct WebRenderer {
    /// Held for the device's lifetime: the device is the adapter's, and a
    /// browser may tear the instance under it down once nothing holds it.
    _gpu: web_sys::Gpu,
    _adapter: web_sys::GpuAdapter,
    device: GpuDevice,
    queue: GpuQueue,
    canvas: HtmlCanvasElement,
    context: GpuCanvasContext,
    /// What the canvas is configured as, and the sRGB view it is drawn through.
    canvas_format: GpuTextureFormat,
    view_format: GpuTextureFormat,

    pipeline_2d: GpuRenderPipeline,
    layout_0: GpuBindGroupLayout,
    layout_1: GpuBindGroupLayout,
    window_info: GpuBuffer,
    plate_features: GpuBuffer,
    backdrop_sampler: GpuSampler,
    /// `@group(0)` over the empty backdrop, and over the blur snapshot.
    group_0: GpuBindGroup,
    snapshot: Option<(GpuTexture, GpuBindGroup, u32, u32)>,
    blocks: Growable,
    group_1: GpuBindGroup,
    vertices: Growable,

    glyph_pipeline: GpuRenderPipeline,
    glyph_layout: GpuBindGroupLayout,
    atlas: GlyphAtlas,
    atlas_texture: GpuTexture,
    atlas_group: GpuBindGroup,
    atlas_uploaded: u64,
    glyph_vertices: Growable,

    image_sampler: GpuSampler,
    images: HashMap<u32, WebImage>,
    image_vertices: Growable,

    /// Copy the next frame into this, for [`take_capture`](Self::take_capture).
    capture: Option<(GpuBuffer, u32, u32, u32)>,
    capture_requested: bool,
}

fn shader_module(device: &GpuDevice, code: &str, label: &str) -> web_sys::GpuShaderModule {
    let desc = GpuShaderModuleDescriptor::new(code);
    desc.set_label(label);
    device.create_shader_module(&desc)
}

/// `wgpu::BlendState::ALPHA_BLENDING`, as both Vulkan pipelines blend.
fn alpha_blending() -> GpuBlendState {
    let color = GpuBlendComponent::new();
    color.set_src_factor(GpuBlendFactor::SrcAlpha);
    color.set_dst_factor(GpuBlendFactor::OneMinusSrcAlpha);
    color.set_operation(GpuBlendOperation::Add);
    let alpha = GpuBlendComponent::new();
    alpha.set_src_factor(GpuBlendFactor::One);
    alpha.set_dst_factor(GpuBlendFactor::OneMinusSrcAlpha);
    alpha.set_operation(GpuBlendOperation::Add);
    GpuBlendState::new(&alpha, &color)
}

fn uniform_entry(binding: u32, min_size: u32, dynamic: bool) -> GpuBindGroupLayoutEntry {
    let entry = GpuBindGroupLayoutEntry::new(binding, shader_stage::FRAGMENT);
    let layout = GpuBufferBindingLayout::new();
    layout.set_type(GpuBufferBindingType::Uniform);
    layout.set_min_binding_size(min_size);
    layout.set_has_dynamic_offset(dynamic);
    entry.set_buffer(&layout);
    entry
}

fn texture_entry(binding: u32) -> GpuBindGroupLayoutEntry {
    let entry = GpuBindGroupLayoutEntry::new(binding, shader_stage::FRAGMENT);
    let layout = GpuTextureBindingLayout::new();
    layout.set_sample_type(GpuTextureSampleType::Float);
    layout.set_view_dimension(GpuTextureViewDimension::N2d);
    entry.set_texture(&layout);
    entry
}

fn sampler_entry(binding: u32) -> GpuBindGroupLayoutEntry {
    let entry = GpuBindGroupLayoutEntry::new(binding, shader_stage::FRAGMENT);
    let layout = GpuSamplerBindingLayout::new();
    layout.set_type(GpuSamplerBindingType::Filtering);
    entry.set_sampler(&layout);
    entry
}

fn sampler(device: &GpuDevice, filter: GpuFilterMode, mipmap: GpuMipmapFilterMode) -> GpuSampler {
    let desc = GpuSamplerDescriptor::new();
    desc.set_address_mode_u(GpuAddressMode::ClampToEdge);
    desc.set_address_mode_v(GpuAddressMode::ClampToEdge);
    desc.set_address_mode_w(GpuAddressMode::ClampToEdge);
    desc.set_mag_filter(filter);
    desc.set_min_filter(filter);
    desc.set_mipmap_filter(mipmap);
    device.create_sampler_with_descriptor(&desc)
}

fn texture(device: &GpuDevice, format: GpuTextureFormat, w: u32, h: u32, usage: u32, label: &str) -> Result<GpuTexture, JsValue> {
    let size = [js_sys::Number::from(w.max(1)), js_sys::Number::from(h.max(1))];
    let desc = GpuTextureDescriptor::new(format, &size, usage);
    desc.set_label(label);
    device.create_texture(&desc)
}

fn whole_view(texture: &GpuTexture) -> Result<GpuTextureView, JsValue> {
    texture.create_view()
}

fn extent(w: u32, h: u32) -> GpuExtent3dDict {
    let e = GpuExtent3dDict::new(w);
    e.set_height(h);
    e
}

/// Write `pixels` (`w` x `h`, 4 bytes a texel, rows packed) into `texture`.
fn write_texture(queue: &GpuQueue, texture: &GpuTexture, pixels: &[u8], w: u32, h: u32) -> Result<(), JsValue> {
    let layout = GpuTexelCopyBufferLayout::new();
    layout.set_bytes_per_row(w * 4);
    layout.set_rows_per_image(h);
    queue.write_texture_with_u8_slice_and_gpu_extent_3d_dict(
        &GpuTexelCopyTextureInfo::new(texture),
        pixels,
        &layout,
        &extent(w, h),
    )
}

/// A two-entry (texture, sampler) bind group for the glyph shader.
fn texture_group(device: &GpuDevice, layout: &GpuBindGroupLayout, view: &GpuTextureView, sampler: &GpuSampler) -> GpuBindGroup {
    let entries = [GpuBindGroupEntry::new_with_gpu_texture_view(0, view), GpuBindGroupEntry::new(1, sampler)];
    device.create_bind_group(&GpuBindGroupDescriptor::new(&entries, layout))
}

fn uniform_binding(buffer: &GpuBuffer, size: u32) -> GpuBufferBinding {
    let binding = GpuBufferBinding::new(buffer);
    binding.set_size(size);
    binding
}

impl WebRenderer {
    /// Ask the browser for a WebGPU device and set `canvas` up to draw into.
    pub async fn new(canvas: HtmlCanvasElement) -> Result<Self, JsValue> {
        let window = web_sys::window().ok_or("no window")?;
        let gpu = window.navigator().gpu();
        let adapter = gpu
            .request_adapter()
            .await?
            .into_option()
            .ok_or("this browser offers no WebGPU adapter")?;
        let device: GpuDevice = adapter.request_device().await?;
        // Report what the device rejects and why it was lost, on the console:
        // a WebGPU validation error is otherwise silent.
        js_sys::Function::new_with_args(
            "d",
            "d.onuncapturederror = (e) => console.error('cce-ui WebGPU:', e.error.message); \
             d.lost.then((i) => console.error('cce-ui WebGPU device lost:', i.reason, i.message));",
        )
        .call1(&JsValue::NULL, &device)?;
        let queue = device.queue();
        let context: GpuCanvasContext = canvas
            .get_context("webgpu")?
            .ok_or("the canvas has no webgpu context")?
            .dyn_into()?;

        let canvas_format = gpu.get_preferred_canvas_format();
        let view_format = match canvas_format {
            GpuTextureFormat::Bgra8unorm => GpuTextureFormat::Bgra8unormSrgb,
            GpuTextureFormat::Rgba8unorm => GpuTextureFormat::Rgba8unormSrgb,
            other => other,
        };
        let config = GpuCanvasConfiguration::new(&device, canvas_format);
        config.set_usage(texture_usage::RENDER_ATTACHMENT | texture_usage::COPY_SRC);
        config.set_view_formats(&[js_sys::JsString::from(JsValue::from(view_format))]);
        config.set_alpha_mode(GpuCanvasAlphaMode::Premultiplied);
        context.configure(&config)?;

        // The 2D pipeline: shader2d with its parameter block as a uniform.
        let layout_0 = device.create_bind_group_layout(&GpuBindGroupLayoutDescriptor::new(&[
            texture_entry(0),
            sampler_entry(1),
            uniform_entry(2, WINDOW_INFO_BYTES as u32, false),
            uniform_entry(3, PLATE_FEATURES_BINDING_BYTES as u32, false),
        ]))?;
        let layout_1 = device.create_bind_group_layout(&GpuBindGroupLayoutDescriptor::new(&[uniform_entry(
            0,
            (PUSH_CONSTANT_FLOATS * 4) as u32,
            true,
        )]))?;
        let layout_2d = device.create_pipeline_layout(&GpuPipelineLayoutDescriptor::new(&[
            js_sys::JsOption::wrap(layout_0.clone()),
            js_sys::JsOption::wrap(layout_1.clone()),
        ]));
        let module_2d = shader_module(&device, &shader2d_for_webgpu(), "shader2d");
        let attrs_2d = [
            GpuVertexAttribute::new(GpuVertexFormat::Float32x2, 0, 0),
            GpuVertexAttribute::new(GpuVertexFormat::Float32x4, 8, 1),
            GpuVertexAttribute::new(GpuVertexFormat::Float32x3, 24, 2),
        ];
        let vertex_2d = GpuVertexState::new(&module_2d);
        vertex_2d.set_entry_point("vs_main");
        vertex_2d.set_buffers(&[js_sys::JsOption::wrap(GpuVertexBufferLayout::new(
            std::mem::size_of::<Vertex>() as u32,
            &attrs_2d,
        ))]);
        let target = GpuColorTargetState::new(view_format);
        target.set_blend(&alpha_blending());
        let targets = [js_sys::JsOption::wrap(target)];
        let fragment_2d = GpuFragmentState::new(&module_2d, &targets);
        fragment_2d.set_entry_point("fs_main");
        let primitive = GpuPrimitiveState::new();
        primitive.set_topology(GpuPrimitiveTopology::TriangleList);
        let desc_2d = GpuRenderPipelineDescriptor::new(&layout_2d, &vertex_2d);
        desc_2d.set_fragment(&fragment_2d);
        desc_2d.set_primitive(&primitive);
        desc_2d.set_label("2d");
        let pipeline_2d = device.create_render_pipeline(&desc_2d)?;

        // Text and images: glyph.wgsl, one (texture, sampler) group each.
        let glyph_layout = device.create_bind_group_layout(&GpuBindGroupLayoutDescriptor::new(&[
            texture_entry(0),
            sampler_entry(1),
        ]))?;
        let layout_glyph =
            device.create_pipeline_layout(&GpuPipelineLayoutDescriptor::new(&[js_sys::JsOption::wrap(glyph_layout.clone())]));
        let module_glyph = shader_module(&device, GLYPH, "glyph");
        let attrs_glyph = [
            GpuVertexAttribute::new(GpuVertexFormat::Float32x2, 0, 0),
            GpuVertexAttribute::new(GpuVertexFormat::Float32x2, 8, 1),
            GpuVertexAttribute::new(GpuVertexFormat::Float32x4, 16, 2),
            GpuVertexAttribute::new(GpuVertexFormat::Float32x3, 32, 3),
            GpuVertexAttribute::new(GpuVertexFormat::Float32x2, 44, 4),
        ];
        let vertex_glyph = GpuVertexState::new(&module_glyph);
        vertex_glyph.set_entry_point("vs_main");
        vertex_glyph.set_buffers(&[js_sys::JsOption::wrap(GpuVertexBufferLayout::new(
            std::mem::size_of::<GlyphVertex>() as u32,
            &attrs_glyph,
        ))]);
        let target = GpuColorTargetState::new(view_format);
        target.set_blend(&alpha_blending());
        let targets = [js_sys::JsOption::wrap(target)];
        let fragment_glyph = GpuFragmentState::new(&module_glyph, &targets);
        fragment_glyph.set_entry_point("fs_main");
        let desc_glyph = GpuRenderPipelineDescriptor::new(&layout_glyph, &vertex_glyph);
        desc_glyph.set_fragment(&fragment_glyph);
        desc_glyph.set_primitive(&primitive);
        desc_glyph.set_label("glyph");
        let glyph_pipeline = device.create_render_pipeline(&desc_glyph)?;

        let uniform_desc = |size: usize, label: &str| {
            let d = GpuBufferDescriptor::new(size as u32, buffer_usage::UNIFORM | buffer_usage::COPY_DST);
            d.set_label(label);
            d
        };
        let window_info = device.create_buffer(&uniform_desc(WINDOW_INFO_BYTES, "window-info"))?;
        let plate_features = device.create_buffer(&uniform_desc(PLATE_FEATURES_BINDING_BYTES, "plate-features"))?;

        // Linear, clamp-to-edge, as the Vulkan backdrop sampler.
        let backdrop_sampler = sampler(&device, GpuFilterMode::Linear, GpuMipmapFilterMode::Nearest);
        let empty = texture(&device, view_format, 1, 1, texture_usage::TEXTURE_BINDING | texture_usage::COPY_DST, "backdrop")?;
        write_texture(&queue, &empty, &[0, 0, 0, 0], 1, 1)?;
        let group_0 = Self::group_0(&device, &layout_0, &whole_view(&empty)?, &backdrop_sampler, &window_info, &plate_features);

        let blocks = Growable::new(&device, (2 * WEBGPU_BLOCK_STRIDE) as u32, buffer_usage::UNIFORM, "batch-blocks")?;
        let group_1 = Self::group_1(&device, &layout_1, &blocks.buffer);
        let vertices = Growable::new(&device, 64 * 1024, buffer_usage::VERTEX, "vertices")?;

        // The atlas: RGBA8 unorm, sampled nearest (as the Vulkan atlas).
        let atlas_texture = texture(
            &device,
            GpuTextureFormat::Rgba8unorm,
            ATLAS_SIZE,
            ATLAS_SIZE,
            texture_usage::TEXTURE_BINDING | texture_usage::COPY_DST,
            "glyph-atlas",
        )?;
        let atlas_sampler = sampler(&device, GpuFilterMode::Nearest, GpuMipmapFilterMode::Nearest);
        let atlas_group = texture_group(&device, &glyph_layout, &whole_view(&atlas_texture)?, &atlas_sampler);
        let glyph_vertices = Growable::new(&device, 64 * 1024, buffer_usage::VERTEX, "glyph-vertices")?;

        // Images: sRGB, linear (as the Vulkan image stage).
        let image_sampler = sampler(&device, GpuFilterMode::Linear, GpuMipmapFilterMode::Linear);
        let image_vertices = Growable::new(&device, 4 * 1024, buffer_usage::VERTEX, "image-quads")?;
        // This renderer's image table: the ids queued before it existed are
        // drained into it, as a Vulkan renderer's are.
        image_table_built();

        Ok(Self {
            _gpu: gpu,
            _adapter: adapter,
            device,
            queue,
            canvas,
            context,
            canvas_format,
            view_format,
            pipeline_2d,
            layout_0,
            layout_1,
            window_info,
            plate_features,
            backdrop_sampler,
            group_0,
            snapshot: None,
            blocks,
            group_1,
            vertices,
            glyph_pipeline,
            glyph_layout,
            atlas: GlyphAtlas::new(),
            atlas_texture,
            atlas_group,
            atlas_uploaded: 0,
            glyph_vertices,
            image_sampler,
            images: HashMap::new(),
            image_vertices,
            capture: None,
            capture_requested: false,
        })
    }

    fn group_0(
        device: &GpuDevice,
        layout: &GpuBindGroupLayout,
        backdrop: &GpuTextureView,
        sampler: &GpuSampler,
        window_info: &GpuBuffer,
        plate_features: &GpuBuffer,
    ) -> GpuBindGroup {
        let entries = [
            GpuBindGroupEntry::new_with_gpu_texture_view(0, backdrop),
            GpuBindGroupEntry::new(1, sampler),
            GpuBindGroupEntry::new_with_gpu_buffer_binding(2, &uniform_binding(window_info, WINDOW_INFO_BYTES as u32)),
            GpuBindGroupEntry::new_with_gpu_buffer_binding(
                3,
                &uniform_binding(plate_features, PLATE_FEATURES_BINDING_BYTES as u32),
            ),
        ];
        device.create_bind_group(&GpuBindGroupDescriptor::new(&entries, layout))
    }

    fn group_1(device: &GpuDevice, layout: &GpuBindGroupLayout, blocks: &GpuBuffer) -> GpuBindGroup {
        let entries = [GpuBindGroupEntry::new_with_gpu_buffer_binding(
            0,
            &uniform_binding(blocks, (PUSH_CONSTANT_FLOATS * 4) as u32),
        )];
        device.create_bind_group(&GpuBindGroupDescriptor::new(&entries, layout))
    }

    /// The drawing buffer's size in physical px.
    pub fn size(&self) -> (u32, u32) {
        (self.canvas.width(), self.canvas.height())
    }

    /// Size the drawing buffer (physical px). The page sizes the element.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.canvas.set_width(width.max(1));
        self.canvas.set_height(height.max(1));
    }

    /// Shape this frame's text into glyph quads (see [`GlyphAtlas::prepare`]),
    /// against the drawing buffer's current size.
    pub fn prepare_text(&mut self, fs: &mut cosmic_text::FontSystem, swash: &mut cosmic_text::SwashCache, spans: &[TextSpan<'_>]) {
        let (w, h) = self.size();
        self.atlas.prepare(fs, swash, spans, w, h);
    }

    /// Copy the next frame drawn into a buffer [`take_capture`](Self::take_capture) reads.
    pub fn capture_next_frame(&mut self) {
        self.capture_requested = true;
    }

    /// The captured frame, once the GPU has finished it.
    pub async fn take_capture(&mut self) -> Result<Option<Capture>, JsValue> {
        let Some((buffer, width, height, row)) = self.capture.take() else { return Ok(None) };
        buffer.map_async(map_mode::READ).await?;
        let mapped = js_sys::Uint8Array::new(&JsValue::from(buffer.get_mapped_range()?));
        let padded = mapped.to_vec();
        buffer.unmap();
        buffer.destroy();
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height as usize {
            rgba.extend_from_slice(&padded[y * row as usize..y * row as usize + width as usize * 4]);
        }
        if matches!(self.canvas_format, GpuTextureFormat::Bgra8unorm) {
            for px in rgba.chunks_exact_mut(4) {
                px.swap(0, 2);
            }
        }
        Ok(Some(Capture { width, height, rgba }))
    }

    /// Apply the image queue: uploads, in-place updates, frees.
    fn process_images(&mut self) -> Result<(), JsValue> {
        for pending in take_pending() {
            match pending {
                Pending::Upload { id, pixels, width, height, format, mips: _ }
                | Pending::Update { id, pixels, width, height, format } => {
                    let same = self
                        .images
                        .get(&id)
                        .is_some_and(|img| img.width == width && img.height == height && img.format == format);
                    if !same {
                        if let Some(old) = self.images.remove(&id) {
                            old.texture.destroy();
                        }
                        let tex_format = match format {
                            PixelFormat::Rgba => GpuTextureFormat::Rgba8unormSrgb,
                            PixelFormat::Bgra => GpuTextureFormat::Bgra8unormSrgb,
                        };
                        let texture = texture(
                            &self.device,
                            tex_format,
                            width,
                            height,
                            texture_usage::TEXTURE_BINDING | texture_usage::COPY_DST,
                            "image",
                        )?;
                        let group = texture_group(&self.device, &self.glyph_layout, &whole_view(&texture)?, &self.image_sampler);
                        self.images.insert(id, WebImage { texture, group, width, height, format });
                    }
                    let img = &self.images[&id];
                    write_texture(&self.queue, &img.texture, &pixels, width, height)?;
                    retire_buffer(pixels);
                }
                Pending::Free { id } => {
                    if let Some(old) = self.images.remove(&id) {
                        old.texture.destroy();
                    }
                }
            }
        }
        Ok(())
    }

    /// The blur snapshot texture and its `@group(0)`, sized to the target.
    fn snapshot_group(&mut self, w: u32, h: u32) -> Result<(GpuTexture, GpuBindGroup), JsValue> {
        if let Some((tex, group, sw, sh)) = &self.snapshot {
            if *sw == w && *sh == h {
                return Ok((tex.clone(), group.clone()));
            }
            tex.destroy();
        }
        let tex = texture(&self.device, self.view_format, w, h, texture_usage::TEXTURE_BINDING | texture_usage::COPY_DST, "blur-snapshot")?;
        let group = Self::group_0(&self.device, &self.layout_0, &whole_view(&tex)?, &self.backdrop_sampler, &self.window_info, &self.plate_features);
        self.snapshot = Some((tex.clone(), group.clone(), w, h));
        Ok((tex, group))
    }

    fn begin_pass(encoder: &GpuCommandEncoder, view: &GpuTextureView, clear: Option<[f32; 4]>) -> Result<GpuRenderPassEncoder, JsValue> {
        let attachment = match clear {
            Some(c) => {
                let a = GpuRenderPassColorAttachment::new_with_gpu_texture_view(GpuLoadOp::Clear, GpuStoreOp::Store, view);
                a.set_clear_value(&[
                    js_sys::Number::from(c[0] as f64),
                    js_sys::Number::from(c[1] as f64),
                    js_sys::Number::from(c[2] as f64),
                    js_sys::Number::from(c[3] as f64),
                ]);
                a
            }
            None => GpuRenderPassColorAttachment::new_with_gpu_texture_view(GpuLoadOp::Load, GpuStoreOp::Store, view),
        };
        encoder.begin_render_pass(&GpuRenderPassDescriptor::new(&[js_sys::JsOption::wrap(attachment)]))
    }

    /// Draw one frame into the canvas. The browser presents it when the task
    /// that called this returns.
    pub fn draw_frame_2d(&mut self, frame: Frame2D<'_>) -> Result<(), JsValue> {
        self.process_images()?;
        let target = self.context.get_current_texture()?;
        let (w, h) = (target.width(), target.height());
        let view_desc = GpuTextureViewDescriptor::new();
        view_desc.set_format(self.view_format);
        let view = target.create_view_with_descriptor(&view_desc)?;

        // The frame's uniforms and vertices.
        let info = window_info_data(w, h, 0.0, relief_px_at(crate::scale::scale_factor()));
        self.queue.write_buffer_with_u32_and_u8_slice(&self.window_info, 0, bytemuck::cast_slice(&info))?;
        if !frame.plate_features.is_empty() {
            let n = frame.plate_features.len().min(MAX_PLATE_FEATURES);
            self.queue.write_buffer_with_u32_and_u8_slice(
                &self.plate_features,
                0,
                bytemuck::cast_slice(&frame.plate_features[..n]),
            )?;
        }
        let vert_bytes: &[u8] = bytemuck::cast_slice(frame.verts);
        let overlay_bytes: &[u8] = bytemuck::cast_slice(frame.overlay_verts);
        let mut all = Vec::with_capacity(vert_bytes.len() + overlay_bytes.len());
        all.extend_from_slice(vert_bytes);
        all.extend_from_slice(overlay_bytes);
        self.vertices.ensure(&self.device, all.len() as u32)?;
        self.vertices.write(&self.queue, &all)?;
        let vertex_count = frame.verts.len() as u32;
        let overlay_count = frame.overlay_verts.len() as u32;

        let default_batch = [Batch2D { scissor: None, clip_rrect: None, start: 0, end: vertex_count, plate: None, blur_behind: false }];
        let batches: &[Batch2D] = if frame.batches.is_empty() { &default_batch } else { frame.batches };
        // Corner-shape exponent for the rounded-rect clip SDF (see the Vulkan renderer).
        let clip_shape = crate::layout::corner_shape();
        // One block per batch, then the overlay's zero block.
        let block_floats = WEBGPU_BLOCK_STRIDE / 4;
        let mut blocks = vec![0.0f32; (batches.len() + 1) * block_floats];
        for (i, batch) in batches.iter().enumerate() {
            blocks[i * block_floats..i * block_floats + PUSH_CONSTANT_FLOATS]
                .copy_from_slice(&batch_push_constants(batch, clip_shape, 0));
        }
        let overlay_block = batches.len() as u32;
        if self.blocks.ensure(&self.device, (blocks.len() * 4) as u32)? {
            self.group_1 = Self::group_1(&self.device, &self.layout_1, &self.blocks.buffer);
        }
        self.blocks.write(&self.queue, bytemuck::cast_slice(&blocks))?;

        if self.atlas_uploaded != self.atlas.generation() {
            write_texture(&self.queue, &self.atlas_texture, self.atlas.pixels(), ATLAS_SIZE, ATLAS_SIZE)?;
            self.atlas_uploaded = self.atlas.generation();
        }
        let glyph_bytes: &[u8] = bytemuck::cast_slice(self.atlas.vertices());
        self.glyph_vertices.ensure(&self.device, glyph_bytes.len() as u32)?;
        self.glyph_vertices.write(&self.queue, glyph_bytes)?;
        let image_verts = image_quad_vertices(frame.images, w, h);
        let image_bytes: &[u8] = bytemuck::cast_slice(&image_verts);
        self.image_vertices.ensure(&self.device, image_bytes.len() as u32)?;
        self.image_vertices.write(&self.queue, image_bytes)?;

        // The blur snapshot, made (or resized) before recording when any
        // batch needs one.
        let snapshot = if batches.iter().any(|b| b.blur_behind) { Some(self.snapshot_group(w, h)?) } else { None };

        // Record.
        let encoder = self.device.create_command_encoder();
        let mut pass = Self::begin_pass(&encoder, &view, Some(frame.clear_color))?;
        let clamp_scissor = |pass: &GpuRenderPassEncoder, (x, y, sw, sh): (u32, u32, u32, u32)| {
            let x = x.min(w);
            let y = y.min(h);
            pass.set_scissor_rect(x, y, sw.min(w - x), sh.min(h - y));
        };
        let images = frame.images;
        let mut order: Vec<usize> = (0..images.len()).collect();
        order.sort_by_key(|&k| images[k].z_before);
        let mut img_i = 0usize;
        let draw_image = |pass: &GpuRenderPassEncoder, k: usize| {
            let q = &images[k];
            let Some(img) = self.images.get(&q.image) else { return }; // not landed / freed: skipped, as on Vulkan
            clamp_scissor(pass, q.clip.unwrap_or((0, 0, w, h)));
            pass.set_pipeline(&self.glyph_pipeline);
            pass.set_bind_group(0, Some(&img.group));
            pass.set_vertex_buffer_with_u32(0, Some(&self.image_vertices.buffer), 0);
            pass.draw_with_instance_count_and_first_vertex(6, 1, (k * 6) as u32);
        };

        // The `@group(0)` vertex draws bind: the empty backdrop until the
        // first blur snapshot, the snapshot after. Consecutive blur plates
        // share one snapshot; only a non-blur draw invalidates it.
        let mut active_group_0 = self.group_0.clone();
        let mut snapshot_fresh = false;

        for (bi, batch) in batches.iter().enumerate() {
            // Images due at this batch's boundary draw first (beneath its
            // geometry, and inside a snapshot taken for it).
            while let Some(&k) = order.get(img_i) {
                if images[k].z_before > batch.start {
                    break;
                }
                img_i += 1;
                draw_image(&pass, k);
                snapshot_fresh = false;
            }
            if batch.blur_behind {
                if !snapshot_fresh {
                    let (snap, snap_group) = snapshot.as_ref().expect("made above for a blur batch");
                    pass.end();
                    encoder.copy_texture_to_texture_with_gpu_extent_3d_dict(
                        &GpuTexelCopyTextureInfo::new(&target),
                        &GpuTexelCopyTextureInfo::new(snap),
                        &extent(w, h),
                    )?;
                    pass = Self::begin_pass(&encoder, &view, None)?;
                    active_group_0 = snap_group.clone();
                    snapshot_fresh = true;
                }
            } else if batch.start < batch.end {
                snapshot_fresh = false;
            }
            // A degenerate scissor skips the geometry (images keep their own clips).
            let scissor = match batch.scissor {
                Some((bx, by, bw, bh)) => {
                    if bx >= w || by >= h || bw.min(w - bx) == 0 || bh.min(h - by) == 0 {
                        None
                    } else {
                        Some((bx, by, bw.min(w - bx), bh.min(h - by)))
                    }
                }
                None => Some((0, 0, w, h)),
            };
            let mut cursor = batch.start;
            while cursor < batch.end {
                let next_z = order.get(img_i).map(|&k| images[k].z_before).unwrap_or(u32::MAX);
                if next_z <= cursor {
                    let k = order[img_i];
                    img_i += 1;
                    draw_image(&pass, k);
                    continue;
                }
                let upto = next_z.min(batch.end);
                if let Some(s) = scissor {
                    pass.set_pipeline(&self.pipeline_2d);
                    pass.set_bind_group(0, Some(&active_group_0));
                    pass.set_bind_group_with_u32_slice_and_u32_and_dynamic_offsets_data_length(
                        1,
                        Some(&self.group_1),
                        &[(bi * WEBGPU_BLOCK_STRIDE) as u32],
                        0,
                        1,
                    )?;
                    pass.set_vertex_buffer_with_u32(0, Some(&self.vertices.buffer), 0);
                    clamp_scissor(&pass, s);
                    pass.draw_with_instance_count_and_first_vertex(upto - cursor, 1, cursor);
                }
                cursor = upto;
            }
        }
        // Images sorting after all geometry.
        while let Some(&k) = order.get(img_i) {
            img_i += 1;
            draw_image(&pass, k);
        }
        pass.set_scissor_rect(0, 0, w, h);

        // Text on top of the geometry.
        let glyph_count = self.atlas.vertices().len() as u32;
        if glyph_count > 0 {
            pass.set_pipeline(&self.glyph_pipeline);
            pass.set_bind_group(0, Some(&self.atlas_group));
            pass.set_vertex_buffer_with_u32(0, Some(&self.glyph_vertices.buffer), 0);
            pass.draw_with_instance_count_and_first_vertex(glyph_count, 1, 0);
        }
        // Overlays last, with the default backdrop and a zero parameter block.
        if overlay_count > 0 {
            pass.set_pipeline(&self.pipeline_2d);
            pass.set_bind_group(0, Some(&self.group_0));
            pass.set_bind_group_with_u32_slice_and_u32_and_dynamic_offsets_data_length(
                1,
                Some(&self.group_1),
                &[overlay_block * WEBGPU_BLOCK_STRIDE as u32],
                0,
                1,
            )?;
            pass.set_vertex_buffer_with_u32(0, Some(&self.vertices.buffer), 0);
            pass.draw_with_instance_count_and_first_vertex(overlay_count, 1, vertex_count);
        }
        pass.end();

        if std::mem::take(&mut self.capture_requested) {
            let row = (w * 4).div_ceil(256) * 256;
            let desc = GpuBufferDescriptor::new(row * h, buffer_usage::COPY_DST | buffer_usage::MAP_READ);
            desc.set_label("capture");
            let buffer = self.device.create_buffer(&desc)?;
            let dst = GpuTexelCopyBufferInfo::new(&buffer);
            dst.set_bytes_per_row(row);
            dst.set_rows_per_image(h);
            encoder.copy_texture_to_buffer_with_gpu_extent_3d_dict(&GpuTexelCopyTextureInfo::new(&target), &dst, &extent(w, h))?;
            self.capture = Some((buffer, w, h, row));
        }

        self.queue.submit(&[encoder.finish()]);
        Ok(())
    }
}
