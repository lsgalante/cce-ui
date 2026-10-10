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
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the renderer's state, images, captures, building it (`new`), resizing, text, and `impl Stage3D` |
//! | `gpu` | WebGPU helpers: shader modules, blending, bind-group entries, samplers, textures and their writes |
//! | `frame` | a frame: draining queued images, the blur snapshot, the passes, `draw_frame_2d` |

mod frame;
mod gpu;

use gpu::*;
pub(super) use gpu::{alpha_blending, shader_module, texture, whole_view};

use std::collections::HashMap;

use super::rt::WebRt;
use super::scene::WebScene;
use crate::draw::rt::{PreparedRtScene, RtCamera, RtEnvironment};
use crate::draw::scene::{MeshId, SceneDraw, SceneImage, Stage3D, Vertex3D};

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
    GpuOrigin3dDict, GpuTexelCopyBufferLayout, GpuTexelCopyTextureInfo, GpuTexture, GpuTextureBindingLayout,
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
pub(super) struct Growable {
    pub(super) buffer: GpuBuffer,
    size: u32,
    usage: u32,
    label: &'static str,
}

impl Growable {
    pub(super) fn new(device: &GpuDevice, size: u32, usage: u32, label: &'static str) -> Result<Self, JsValue> {
        let size = size.max(256).next_power_of_two();
        let desc = GpuBufferDescriptor::new(size, usage | buffer_usage::COPY_DST);
        desc.set_label(label);
        Ok(Self { buffer: device.create_buffer(&desc)?, size, usage, label })
    }

    /// Make room for `needed` bytes; true when the buffer was replaced (any
    /// bind group naming it must be rebuilt).
    pub(super) fn ensure(&mut self, device: &GpuDevice, needed: u32) -> Result<bool, JsValue> {
        if needed <= self.size {
            return Ok(false);
        }
        self.buffer.destroy();
        *self = Self::new(device, needed, self.usage, self.label)?;
        Ok(true)
    }

    pub(super) fn write(&self, queue: &GpuQueue, bytes: &[u8]) -> Result<(), JsValue> {
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

/// A captured frame still on the GPU: [`read`](Self::read) waits for it.
pub struct PendingCapture {
    buffer: GpuBuffer,
    width: u32,
    height: u32,
    /// Bytes per row in the buffer: a copy's rows are 256-byte aligned.
    row: u32,
    /// The canvas is BGRA: swap to RGBA on the way out.
    bgra: bool,
}

impl PendingCapture {
    pub async fn read(self) -> Result<Capture, JsValue> {
        let Self { buffer, width, height, row, bgra } = self;
        buffer.map_async(map_mode::READ).await?;
        let mapped = js_sys::Uint8Array::new(&JsValue::from(buffer.get_mapped_range()?));
        let padded = mapped.to_vec();
        buffer.unmap();
        buffer.destroy();
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height as usize {
            rgba.extend_from_slice(&padded[y * row as usize..y * row as usize + width as usize * 4]);
        }
        if bgra {
            for px in rgba.chunks_exact_mut(4) {
                px.swap(0, 2);
            }
        }
        Ok(Capture { width, height, rgba })
    }
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
    /// The 3D scene pass, and `@group(0)` over its backdrop (rebuilt with
    /// the backdrop) — what the 2D pass binds while a scene is shown.
    scene: WebScene,
    scene_group_0: Option<GpuBindGroup>,
    /// The path tracer, made by the first `set_rt_scene`; the background and
    /// environment it is handed at each `stage_rt`, as on Vulkan.
    rt: Option<WebRt>,
    rt_background: Option<[f32; 3]>,
    rt_environment: RtEnvironment,
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

impl WebRenderer {
    /// Ask the browser for a WebGPU device and set `canvas` up to draw into.
    pub async fn new(canvas: HtmlCanvasElement) -> Result<Self, JsValue> {
        let (gpu, adapter, device) = super::request_device(&[]).await?;
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
        config.set_usage(texture_usage::RENDER_ATTACHMENT | texture_usage::COPY_SRC | texture_usage::COPY_DST);
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
        let scene = WebScene::new(&device, view_format, &glyph_layout)?;
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
            scene,
            scene_group_0: None,
            rt: None,
            rt_background: None,
            rt_environment: RtEnvironment::default(),
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
        match self.take_pending_capture() {
            Some(pending) => pending.read().await.map(Some),
            None => Ok(None),
        }
    }

    /// The captured frame's buffer, to be read without holding the renderer
    /// across the wait (the browser shell reads it after the turn that drew it).
    pub fn take_pending_capture(&mut self) -> Option<PendingCapture> {
        let (buffer, width, height, row) = self.capture.take()?;
        Some(PendingCapture { buffer, width, height, row, bgra: matches!(self.canvas_format, GpuTextureFormat::Bgra8unorm) })
    }
}

/// The 3D half of the renderer (see `draw::scene::Stage3D`): the scene pass
/// in `web::scene`.
impl Stage3D for WebRenderer {
    fn create_mesh(&mut self, verts: &[Vertex3D]) -> MeshId {
        self.scene.create_mesh(&self.device, &self.queue, verts)
    }
    fn update_mesh(&mut self, id: MeshId, verts: &[Vertex3D]) {
        self.scene.update_mesh(&self.device, &self.queue, id, verts)
    }
    fn stage_scene(&mut self, scissor: (u32, u32, u32, u32), draws: Vec<SceneDraw>) {
        self.scene.stage(scissor, draws)
    }
    fn stage_scene_images(&mut self, images: Vec<SceneImage>) {
        self.scene.stage_images(images)
    }
    fn set_scene_light(&mut self, toward: [f32; 3]) {
        let v = glam::Vec3::from_array(toward);
        if v.length_squared() > 1e-12 {
            self.scene.light = v.normalize().to_array();
        }
    }
    fn set_rt_scene_prepared(&mut self, scene: &PreparedRtScene) {
        if self.rt.is_none() {
            match WebRt::new(&self.device, self.view_format) {
                Ok(rt) => self.rt = Some(rt),
                Err(e) => {
                    web_sys::console::error_2(&"cce-ui: no path tracer:".into(), &e);
                    return;
                }
            }
        }
        let rt = self.rt.as_mut().unwrap();
        if let Err(e) = rt.set_scene(&self.device, &self.queue, scene) {
            web_sys::console::error_2(&"cce-ui: the traced scene was not uploaded:".into(), &e);
        }
    }
    fn set_rt_environment(&mut self, environment: RtEnvironment) {
        self.rt_environment = environment;
    }
    fn set_rt_background(&mut self, color: Option<[f32; 3]>) {
        self.rt_background = color;
    }
    fn stage_rt(&mut self, pane: (u32, u32, u32, u32), camera: RtCamera) {
        if let Some(rt) = self.rt.as_mut() {
            rt.set_background(self.rt_background);
            rt.set_environment(self.rt_environment);
            if let Err(e) = rt.stage(&self.device, pane, camera) {
                web_sys::console::error_2(&"cce-ui: the traced pane was not staged:".into(), &e);
            }
        }
    }
    fn rt_accumulating(&self) -> bool {
        self.rt.as_ref().is_some_and(|rt| rt.accumulating())
    }
}
