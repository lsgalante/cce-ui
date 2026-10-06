//! The path tracer on WebGPU: the Vulkan `RtStage`'s compute tier, ported.
//! The same shaders (`draw::shaders::rt_bvh_source`, `RT_DENOISE`), the
//! same scene packing, BVH and parameter blocks (`draw::rt`), the same
//! rules for when the accumulation restarts — a camera, pane size, scene,
//! background or environment change — and the same frame: one sample a
//! frame added into the running mean, three à-trous denoise iterations over
//! it, the result put into the backdrop's pane region in place of the
//! raster scene, a pane that moved clearing the backdrop once first.
//!
//! What WebGPU makes different:
//!
//! - **The compute tier only.** The hardware ray-query tier needs
//!   `VK_KHR_ray_query`; WebGPU has no ray tracing. A BVH traversed in
//!   compute is the tier every Vulkan device without RT cores runs too.
//! - **The blit is a draw.** Vulkan blits the tracer's `rgba8unorm` image
//!   into the sRGB backdrop, converting as it copies; WebGPU copies only
//!   between formats that differ in sRGB-ness at most, so a small render
//!   pass loads each texel and writes it through the backdrop's sRGB view —
//!   the same conversion (the unorm value taken as linear and encoded).
//! - **No frames in flight to juggle.** One parameter buffer and one bind
//!   group per dispatch, written before the frame's single submission.

use wasm_bindgen::JsValue;
use web_sys::{
    gpu_buffer_usage as buffer_usage, gpu_shader_stage as shader_stage, gpu_texture_usage as texture_usage, GpuBindGroup,
    GpuBindGroupDescriptor, GpuBindGroupEntry, GpuBindGroupLayout, GpuBindGroupLayoutDescriptor, GpuBindGroupLayoutEntry,
    GpuBuffer, GpuBufferBinding, GpuBufferBindingLayout, GpuBufferBindingType, GpuColorTargetState, GpuCommandEncoder,
    GpuComputePassDescriptor, GpuComputePipeline, GpuComputePipelineDescriptor, GpuDevice, GpuFragmentState, GpuLoadOp,
    GpuPipelineLayoutDescriptor, GpuProgrammableStage, GpuQueue, GpuRenderPassColorAttachment, GpuRenderPassDescriptor,
    GpuRenderPipeline, GpuRenderPipelineDescriptor, GpuSampler, GpuSamplerBindingLayout, GpuSamplerBindingType,
    GpuStorageTextureAccess, GpuStorageTextureBindingLayout, GpuStoreOp, GpuTexture, GpuTextureBindingLayout,
    GpuTextureFormat, GpuTextureSampleType, GpuTextureView, GpuTextureViewDimension, GpuVertexState,
};

use super::renderer::{shader_module, texture, whole_view};
use crate::draw::rt::{
    denoise_params, pack_scene, rt_params, DenoiseParams, ParamImage, RtCamera, RtEnvironment, RtImage, RtMaterial,
    RtParams, RtTriangle, DENOISE_ITERATIONS, MAX_SAMPLES, WORKGROUP,
};
use crate::draw::shaders::{rt_bvh_source, RT_DENOISE};

/// Each denoise iteration's block sits at a multiple of this.
const STRIDE: u32 = 256;

/// Loads the tracer's image at the fragment's place in the pane and writes
/// it through the backdrop's sRGB view: Vulkan's UNORM-to-sRGB blit.
const BLIT: &str = "
struct Blit { origin: vec2<i32>, _pad: vec2<i32> }
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var<uniform> blit: Blit;
@vertex fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(src, vec2<i32>(pos.xy) - blit.origin, 0);
}";

/// The pane-sized targets: the running sum, the denoiser's features and
/// ping-pong, and the image the result is written to.
struct Targets {
    accum: GpuBuffer,
    features: GpuBuffer,
    ping: GpuBuffer,
    pong: GpuBuffer,
    output: GpuTexture,
    output_view: GpuTextureView,
    /// Denoise iterations 0 and 2 (src pong, dst ping), and 1 (src ping, dst pong).
    denoise_b: GpuBindGroup,
    denoise_a: GpuBindGroup,
    blit: GpuBindGroup,
    size: (u32, u32),
}

/// The scene the buffers hold, and the image standing in it.
struct Scene {
    nodes: GpuBuffer,
    tris: GpuBuffer,
    materials: GpuBuffer,
    tri_count: u32,
    image: Option<RtImage>,
}

pub(crate) struct WebRt {
    trace: GpuComputePipeline,
    trace_layout: GpuBindGroupLayout,
    denoise: GpuComputePipeline,
    denoise_layout: GpuBindGroupLayout,
    blit: GpuRenderPipeline,
    blit_layout: GpuBindGroupLayout,
    params: GpuBuffer,
    denoise_params: GpuBuffer,
    blit_params: GpuBuffer,
    stand_in: GpuTextureView,
    sampler: GpuSampler,
    scene: Option<Scene>,
    targets: Option<Targets>,

    pane: (u32, u32, u32, u32),
    pane_moved: bool,
    camera: Option<RtCamera>,
    sample_index: u32,
    spp: u32,
    staged: bool,
    background: Option<[f32; 3]>,
    environment: RtEnvironment,
}

fn buffer_entry(binding: u32, ty: GpuBufferBindingType, dynamic: bool) -> GpuBindGroupLayoutEntry {
    let entry = GpuBindGroupLayoutEntry::new(binding, shader_stage::COMPUTE);
    let layout = GpuBufferBindingLayout::new();
    layout.set_type(ty);
    layout.set_has_dynamic_offset(dynamic);
    entry.set_buffer(&layout);
    entry
}

fn storage_texture_entry(binding: u32) -> GpuBindGroupLayoutEntry {
    let entry = GpuBindGroupLayoutEntry::new(binding, shader_stage::COMPUTE);
    let layout = GpuStorageTextureBindingLayout::new(GpuTextureFormat::Rgba8unorm);
    layout.set_access(GpuStorageTextureAccess::WriteOnly);
    layout.set_view_dimension(GpuTextureViewDimension::N2d);
    entry.set_storage_texture(&layout);
    entry
}

fn texture_entry(binding: u32, visibility: u32, sample: GpuTextureSampleType) -> GpuBindGroupLayoutEntry {
    let entry = GpuBindGroupLayoutEntry::new(binding, visibility);
    let layout = GpuTextureBindingLayout::new();
    layout.set_sample_type(sample);
    layout.set_view_dimension(GpuTextureViewDimension::N2d);
    entry.set_texture(&layout);
    entry
}

fn whole(binding: u32, buffer: &GpuBuffer) -> GpuBindGroupEntry {
    GpuBindGroupEntry::new_with_gpu_buffer_binding(binding, &GpuBufferBinding::new(buffer))
}

fn sized(binding: u32, buffer: &GpuBuffer, size: u32) -> GpuBindGroupEntry {
    let b = GpuBufferBinding::new(buffer);
    b.set_size(size);
    GpuBindGroupEntry::new_with_gpu_buffer_binding(binding, &b)
}

fn buffer(device: &GpuDevice, size: u32, usage: u32, label: &str) -> Result<GpuBuffer, JsValue> {
    let desc = web_sys::GpuBufferDescriptor::new(size.max(16).next_multiple_of(4), usage);
    desc.set_label(label);
    device.create_buffer(&desc)
}

fn compute_pipeline(device: &GpuDevice, layout: &GpuBindGroupLayout, code: &str, entry: &str, label: &str) -> GpuComputePipeline {
    let pipeline_layout = device.create_pipeline_layout(&GpuPipelineLayoutDescriptor::new(&[js_sys::JsOption::wrap(layout.clone())]));
    let stage = GpuProgrammableStage::new(&shader_module(device, code, label));
    stage.set_entry_point(entry);
    let desc = GpuComputePipelineDescriptor::new(&pipeline_layout, &stage);
    desc.set_label(label);
    device.create_compute_pipeline(&desc)
}

impl WebRt {
    /// The tracer's pipelines; `backdrop_format` is what the blit writes.
    pub(crate) fn new(device: &GpuDevice, backdrop_format: GpuTextureFormat) -> Result<Self, JsValue> {
        use GpuBufferBindingType::{ReadOnlyStorage, Storage, Uniform};
        let sampler_entry = {
            let entry = GpuBindGroupLayoutEntry::new(8, shader_stage::COMPUTE);
            let layout = GpuSamplerBindingLayout::new();
            layout.set_type(GpuSamplerBindingType::Filtering);
            entry.set_sampler(&layout);
            entry
        };
        let trace_layout = device.create_bind_group_layout(&GpuBindGroupLayoutDescriptor::new(&[
            buffer_entry(0, Uniform, false),
            buffer_entry(1, ReadOnlyStorage, false),
            buffer_entry(2, ReadOnlyStorage, false),
            buffer_entry(3, ReadOnlyStorage, false),
            buffer_entry(4, Storage, false),
            storage_texture_entry(5),
            buffer_entry(6, Storage, false),
            texture_entry(7, shader_stage::COMPUTE, GpuTextureSampleType::Float),
            sampler_entry,
        ]))?;
        let trace = compute_pipeline(device, &trace_layout, &rt_bvh_source(), "cs_main", "rt-trace");
        let denoise_layout = device.create_bind_group_layout(&GpuBindGroupLayoutDescriptor::new(&[
            buffer_entry(0, Uniform, true),
            buffer_entry(1, ReadOnlyStorage, false),
            buffer_entry(2, ReadOnlyStorage, false),
            buffer_entry(3, ReadOnlyStorage, false),
            buffer_entry(4, Storage, false),
            storage_texture_entry(5),
        ]))?;
        let denoise = compute_pipeline(device, &denoise_layout, RT_DENOISE, "cs_denoise", "rt-denoise");

        let blit_layout = device.create_bind_group_layout(&GpuBindGroupLayoutDescriptor::new(&[
            texture_entry(0, shader_stage::FRAGMENT, GpuTextureSampleType::UnfilterableFloat),
            {
                let entry = GpuBindGroupLayoutEntry::new(1, shader_stage::FRAGMENT);
                let layout = GpuBufferBindingLayout::new();
                layout.set_type(GpuBufferBindingType::Uniform);
                entry.set_buffer(&layout);
                entry
            },
        ]))?;
        let blit_module = shader_module(device, BLIT, "rt-blit");
        let vertex = GpuVertexState::new(&blit_module);
        vertex.set_entry_point("vs_main");
        let targets = [js_sys::JsOption::wrap(GpuColorTargetState::new(backdrop_format))];
        let fragment = GpuFragmentState::new(&blit_module, &targets);
        fragment.set_entry_point("fs_main");
        let desc = GpuRenderPipelineDescriptor::new(
            &device.create_pipeline_layout(&GpuPipelineLayoutDescriptor::new(&[js_sys::JsOption::wrap(blit_layout.clone())])),
            &vertex,
        );
        desc.set_fragment(&fragment);
        desc.set_label("rt-blit");
        let blit = device.create_render_pipeline(&desc)?;

        let params = buffer(device, std::mem::size_of::<RtParams>() as u32, buffer_usage::UNIFORM | buffer_usage::COPY_DST, "rt-params")?;
        let denoise_params =
            buffer(device, STRIDE * DENOISE_ITERATIONS as u32, buffer_usage::UNIFORM | buffer_usage::COPY_DST, "rt-denoise-params")?;
        let blit_params = buffer(device, 16, buffer_usage::UNIFORM | buffer_usage::COPY_DST, "rt-blit-params")?;
        // The image binding's stand-in while the scene has none: one clear texel.
        let stand_in = texture(device, GpuTextureFormat::Rgba8unormSrgb, 1, 1, texture_usage::TEXTURE_BINDING | texture_usage::COPY_DST, "rt-stand-in")?;
        let sampler = {
            let desc = web_sys::GpuSamplerDescriptor::new();
            desc.set_mag_filter(web_sys::GpuFilterMode::Linear);
            desc.set_min_filter(web_sys::GpuFilterMode::Linear);
            desc.set_mipmap_filter(web_sys::GpuMipmapFilterMode::Linear);
            device.create_sampler_with_descriptor(&desc)
        };
        Ok(Self {
            trace,
            trace_layout,
            denoise,
            denoise_layout,
            blit,
            blit_layout,
            params,
            denoise_params,
            blit_params,
            stand_in: whole_view(&stand_in)?,
            sampler,
            scene: None,
            targets: None,
            pane: (0, 0, 0, 0),
            pane_moved: false,
            camera: None,
            sample_index: 0,
            spp: 1,
            staged: false,
            background: None,
            environment: RtEnvironment::default(),
        })
    }

    /// Replace the scene: packed and its BVH built on the CPU, uploaded.
    pub(crate) fn set_scene(
        &mut self,
        device: &GpuDevice,
        queue: &GpuQueue,
        triangles: &[RtTriangle],
        materials: &[RtMaterial],
        image: Option<RtImage>,
    ) -> Result<(), JsValue> {
        let packed = pack_scene(triangles, materials, image.map(|i| i.corners), true);
        let upload = |bytes: &[u8], label: &str| -> Result<GpuBuffer, JsValue> {
            let b = buffer(device, bytes.len() as u32, buffer_usage::STORAGE | buffer_usage::COPY_DST, label)?;
            if !bytes.is_empty() {
                queue.write_buffer_with_u32_and_u8_slice(&b, 0, bytes)?;
            }
            Ok(b)
        };
        if let Some(old) = self.scene.take() {
            for b in [old.nodes, old.tris, old.materials] {
                b.destroy();
            }
        }
        self.scene = Some(Scene {
            nodes: upload(bytemuck::cast_slice(&packed.nodes), "rt-nodes")?,
            tris: upload(bytemuck::cast_slice(&packed.tris), "rt-tris")?,
            materials: upload(bytemuck::cast_slice(&packed.materials), "rt-materials")?,
            tri_count: packed.tris.len() as u32,
            image,
        });
        self.sample_index = 0;
        Ok(())
    }

    pub(crate) fn set_background(&mut self, background: Option<[f32; 3]>) {
        if self.background != background {
            self.background = background;
            self.sample_index = 0;
        }
    }

    pub(crate) fn set_environment(&mut self, environment: RtEnvironment) {
        if self.environment != environment {
            self.environment = environment;
            self.sample_index = 0;
        }
    }

    /// Stage a frame for `pane` (physical px): the targets follow its size,
    /// and a camera or size change restarts the accumulation; a pane that
    /// moved clears the backdrop once before the next blit.
    pub(crate) fn stage(&mut self, device: &GpuDevice, pane: (u32, u32, u32, u32), camera: RtCamera) -> Result<(), JsValue> {
        let (_, _, w, h) = pane;
        if w == 0 || h == 0 {
            self.staged = false;
            return Ok(());
        }
        if self.targets.as_ref().map(|t| t.size) != Some((w, h)) {
            self.recreate_targets(device, w, h)?;
            self.sample_index = 0;
        }
        if pane != self.pane && self.pane != (0, 0, 0, 0) {
            self.pane_moved = true;
        }
        if self.camera != Some(camera) {
            self.camera = Some(camera);
            self.sample_index = 0;
        }
        self.pane = pane;
        self.staged = true;
        Ok(())
    }

    pub(crate) fn staged(&self) -> bool {
        self.staged
    }

    /// True while another dispatch would still refine the image.
    pub(crate) fn accumulating(&self) -> bool {
        self.scene.as_ref().is_some_and(|s| s.tri_count > 0) && self.sample_index < MAX_SAMPLES
    }

    fn recreate_targets(&mut self, device: &GpuDevice, w: u32, h: u32) -> Result<(), JsValue> {
        if let Some(old) = self.targets.take() {
            for b in [old.accum, old.features, old.ping, old.pong] {
                b.destroy();
            }
            old.output.destroy();
        }
        let px = w * h;
        let storage = buffer_usage::STORAGE;
        let accum = buffer(device, px * 16, storage, "rt-accum")?;
        let features = buffer(device, px * 32, storage, "rt-features")?;
        let ping = buffer(device, px * 16, storage, "rt-denoise-ping")?;
        let pong = buffer(device, px * 16, storage, "rt-denoise-pong")?;
        let output = texture(
            device,
            GpuTextureFormat::Rgba8unorm,
            w,
            h,
            texture_usage::STORAGE_BINDING | texture_usage::TEXTURE_BINDING,
            "rt-output",
        )?;
        let output_view = whole_view(&output)?;
        let denoise_group = |src: &GpuBuffer, dst: &GpuBuffer| {
            let entries = [
                sized(0, &self.denoise_params, std::mem::size_of::<DenoiseParams>() as u32),
                whole(1, &accum),
                whole(2, &features),
                whole(3, src),
                whole(4, dst),
                GpuBindGroupEntry::new_with_gpu_texture_view(5, &output_view),
            ];
            device.create_bind_group(&GpuBindGroupDescriptor::new(&entries, &self.denoise_layout))
        };
        let denoise_b = denoise_group(&pong, &ping);
        let denoise_a = denoise_group(&ping, &pong);
        let blit = device.create_bind_group(&GpuBindGroupDescriptor::new(
            &[GpuBindGroupEntry::new_with_gpu_texture_view(0, &output_view), whole(1, &self.blit_params)],
            &self.blit_layout,
        ));
        self.targets = Some(Targets { accum, features, ping, pong, output, output_view, denoise_b, denoise_a, blit, size: (w, h) });
        Ok(())
    }

    /// Record one accumulation dispatch, the denoise and the blit into
    /// `backdrop` (the scene's, `backdrop_size` px). False when there is
    /// nothing to do (not staged, an empty scene, converged): the backdrop
    /// then keeps what it has. `image_view` looks the scene's image up in
    /// the 2D pass's table: its view and size, if it has landed.
    pub(crate) fn record(
        &mut self,
        device: &GpuDevice,
        queue: &GpuQueue,
        encoder: &GpuCommandEncoder,
        backdrop: &GpuTextureView,
        backdrop_size: (u32, u32),
        image_view: &dyn Fn(u32) -> Option<(GpuTextureView, u32, u32)>,
    ) -> Result<bool, JsValue> {
        let ready = self.staged && self.scene.as_ref().is_some_and(|s| s.tri_count > 0) && self.targets.is_some();
        self.staged = false;
        if !ready || self.sample_index >= MAX_SAMPLES {
            return Ok(false);
        }
        let (Some(scene), Some(t), Some(camera)) = (&self.scene, &self.targets, self.camera) else { return Ok(false) };
        let (w, h) = t.size;

        // The parameter blocks: the scene's image as it is NOW (its upload
        // may land after the scene was set), or none, which lets rays through.
        let bound = scene.image.and_then(|i| image_view(i.image).map(|(view, iw, ih)| (view, i, iw, ih)));
        let (view, param_image) = match bound {
            Some((view, i, iw, ih)) => (view, Some(ParamImage { width: iw, height: ih, corners: i.corners, opacity: i.opacity })),
            None => (self.stand_in.clone(), None),
        };
        let params = rt_params(camera, (w, h), self.sample_index, self.spp, param_image, self.background, &self.environment);
        queue.write_buffer_with_u32_and_u8_slice(&self.params, 0, bytemuck::bytes_of(&params))?;
        let mut blocks = vec![0u8; STRIDE as usize * DENOISE_ITERATIONS];
        for (i, p) in denoise_params(w, h, self.sample_index + self.spp).iter().enumerate() {
            let at = i * STRIDE as usize;
            blocks[at..at + std::mem::size_of::<DenoiseParams>()].copy_from_slice(bytemuck::bytes_of(p));
        }
        queue.write_buffer_with_u32_and_u8_slice(&self.denoise_params, 0, &blocks)?;
        let (px, py, _, _) = self.pane;
        let dst_x = px.min(backdrop_size.0);
        let dst_y = py.min(backdrop_size.1);
        let (bw, bh) = (w.min(backdrop_size.0 - dst_x), h.min(backdrop_size.1 - dst_y));
        queue.write_buffer_with_u32_and_u8_slice(&self.blit_params, 0, bytemuck::cast_slice(&[dst_x as i32, dst_y as i32, 0, 0]))?;

        let trace_group = device.create_bind_group(&GpuBindGroupDescriptor::new(
            &[
                whole(0, &self.params),
                whole(1, &scene.nodes),
                whole(2, &scene.tris),
                whole(3, &scene.materials),
                whole(4, &t.accum),
                GpuBindGroupEntry::new_with_gpu_texture_view(5, &t.output_view),
                whole(6, &t.features),
                GpuBindGroupEntry::new_with_gpu_texture_view(7, &view),
                GpuBindGroupEntry::new(8, &self.sampler),
            ],
            &self.trace_layout,
        ));
        let groups = (w.div_ceil(WORKGROUP), h.div_ceil(WORKGROUP));
        let pass = encoder.begin_compute_pass_with_descriptor(&GpuComputePassDescriptor::new());
        pass.set_pipeline(&self.trace);
        pass.set_bind_group(0, Some(&trace_group));
        pass.dispatch_workgroups_with_workgroup_count_y(groups.0, groups.1);
        // À-trous iterations: 0 and 2 write ping, 1 writes pong; the last
        // rewrites the output image.
        pass.set_pipeline(&self.denoise);
        for i in 0..DENOISE_ITERATIONS {
            let group = if i % 2 == 0 { &t.denoise_b } else { &t.denoise_a };
            pass.set_bind_group_with_u32_slice_and_u32_and_dynamic_offsets_data_length(0, Some(group), &[i as u32 * STRIDE], 0, 1)?;
            pass.dispatch_workgroups_with_workgroup_count_y(groups.0, groups.1);
        }
        pass.end();

        // Into the backdrop's pane region (clearing the whole backdrop first
        // when the pane moved: stale pixels sit outside the new region).
        let load = if std::mem::take(&mut self.pane_moved) { GpuLoadOp::Clear } else { GpuLoadOp::Load };
        let color = GpuRenderPassColorAttachment::new_with_gpu_texture_view(load, GpuStoreOp::Store, backdrop);
        color.set_clear_value(&[0.0, 0.0, 0.0, 0.0].map(js_sys::Number::from));
        let pass = encoder.begin_render_pass(&GpuRenderPassDescriptor::new(&[js_sys::JsOption::wrap(color)]))?;
        if bw > 0 && bh > 0 {
            pass.set_viewport(dst_x as f32, dst_y as f32, bw as f32, bh as f32, 0.0, 1.0);
            pass.set_scissor_rect(dst_x, dst_y, bw, bh);
            pass.set_pipeline(&self.blit);
            pass.set_bind_group(0, Some(&t.blit));
            pass.draw(3);
        }
        pass.end();
        self.sample_index += self.spp;
        Ok(true)
    }
}
