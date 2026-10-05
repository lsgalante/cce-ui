//! The 3D scene pass on WebGPU: the Vulkan `SceneStage`'s port, drawing the
//! same staged [`SceneDraw`]s and [`SceneImage`]s from the same shaders
//! (`draw::shaders::SCENE3D` / `SCENE3D_IMAGE`) and uniform blocks
//! (`draw::scene::scene_uniforms`) into a full-size backdrop texture with a
//! depth buffer, which the renderer copies beneath the UI pass and its blur
//! plates sample — the Vulkan renderer's arrangement, step for step.
//!
//! What WebGPU makes different:
//!
//! - **Depth bias is pipeline state**, where Vulkan sets it per draw. It
//!   takes one value here — a WebGPU line is one pixel wide (no wideLines),
//!   so a fill carrying wires is pushed back by `wire_base_bias(1.0)`, as a
//!   Vulkan device without wideLines pushes it — so the biased fill is a
//!   pipeline of its own: four fills (opaque or see-through, biased or not),
//!   two line pipelines (see-through wires write depth), the image pipeline.
//! - **No Y flip.** The shaders were written for WebGPU's conventions (the
//!   Vulkan renderer flips its viewport to match), so the viewport is the
//!   plain one and counter-clockwise is front, as there.
//! - **The backdrop is the canvas's sRGB view format**, copied into the
//!   canvas texture (formats that differ only in sRGB-ness copy).

use wasm_bindgen::JsValue;
use web_sys::{
    gpu_buffer_usage as buffer_usage, gpu_shader_stage as shader_stage, gpu_texture_usage as texture_usage, GpuBindGroup,
    GpuBindGroupDescriptor, GpuBindGroupEntry, GpuBindGroupLayout, GpuBindGroupLayoutDescriptor, GpuBindGroupLayoutEntry,
    GpuBufferBinding, GpuBufferBindingLayout, GpuBufferBindingType, GpuColorTargetState, GpuCommandEncoder, GpuCompareFunction,
    GpuCullMode, GpuDepthStencilState, GpuDevice, GpuFragmentState, GpuFrontFace, GpuLoadOp, GpuPipelineLayoutDescriptor,
    GpuPrimitiveState, GpuPrimitiveTopology, GpuQueue, GpuRenderPassColorAttachment, GpuRenderPassDepthStencilAttachment,
    GpuRenderPassDescriptor, GpuRenderPipeline, GpuRenderPipelineDescriptor, GpuStoreOp, GpuTexture, GpuTextureFormat,
    GpuTextureView, GpuVertexAttribute, GpuVertexBufferLayout, GpuVertexFormat, GpuVertexState,
};

use super::renderer::{alpha_blending, shader_module, texture, whole_view, Growable};
use crate::draw::scene::{
    image_quads_3d, scene_uniforms, wire_base_bias, ImageVertex3D, MeshId, SceneDraw, SceneImage, SceneUniforms, Vertex3D,
    DEFAULT_SCENE_LIGHT,
};
use crate::draw::shaders::{SCENE3D, SCENE3D_IMAGE};

/// One draw's uniform block sits at a multiple of this (the alignment every
/// WebGPU device takes for a dynamic offset).
const STRIDE: u32 = 256;
const UNIFORM_SIZE: u32 = std::mem::size_of::<SceneUniforms>() as u32;
const DEPTH: GpuTextureFormat = GpuTextureFormat::Depth32float;

struct Mesh {
    buffer: Growable,
    count: u32,
}

struct Staged {
    scissor: (u32, u32, u32, u32),
    draws: Vec<SceneDraw>,
    images: Vec<SceneImage>,
}

/// The backdrop and depth textures, at the canvas's size.
pub(crate) struct Target {
    pub backdrop: GpuTexture,
    pub view: GpuTextureView,
    depth_view: GpuTextureView,
    pub width: u32,
    pub height: u32,
}

pub(crate) struct WebScene {
    format: GpuTextureFormat,
    uniform_layout: GpuBindGroupLayout,
    /// [opaque, opaque biased, see-through, see-through biased].
    fills: [GpuRenderPipeline; 4],
    /// [no depth writes, depth writes (the wires of a see-through fill)].
    lines: [GpuRenderPipeline; 2],
    image: GpuRenderPipeline,
    uniforms: Growable,
    uniform_group: GpuBindGroup,
    image_verts: Growable,
    meshes: Vec<Mesh>,
    staged: Option<Staged>,
    pub(crate) light: [f32; 3],
    pub(crate) target: Option<Target>,
    /// The backdrop holds a rendered scene worth showing under the UI.
    pub(crate) backdrop_valid: bool,
}

fn uniform_group(device: &GpuDevice, layout: &GpuBindGroupLayout, uniforms: &Growable) -> GpuBindGroup {
    let binding = GpuBufferBinding::new(&uniforms.buffer);
    binding.set_size(UNIFORM_SIZE);
    let entries = [GpuBindGroupEntry::new_with_gpu_buffer_binding(0, &binding)];
    device.create_bind_group(&GpuBindGroupDescriptor::new(&entries, layout))
}

impl WebScene {
    /// The pass's pipelines, drawing into `format` (the canvas's sRGB view
    /// format); `image_layout` is the renderer's (texture, sampler) layout,
    /// so a scene image draws with the group the 2D pass draws it with.
    pub(crate) fn new(device: &GpuDevice, format: GpuTextureFormat, image_layout: &GpuBindGroupLayout) -> Result<Self, JsValue> {
        let entry = GpuBindGroupLayoutEntry::new(0, shader_stage::VERTEX | shader_stage::FRAGMENT);
        let buffer = GpuBufferBindingLayout::new();
        buffer.set_type(GpuBufferBindingType::Uniform);
        buffer.set_has_dynamic_offset(true);
        buffer.set_min_binding_size(UNIFORM_SIZE);
        entry.set_buffer(&buffer);
        let uniform_layout = device.create_bind_group_layout(&GpuBindGroupLayoutDescriptor::new(&[entry]))?;
        let mesh_layout = device.create_pipeline_layout(&GpuPipelineLayoutDescriptor::new(&[js_sys::JsOption::wrap(uniform_layout.clone())]));
        let image_pipeline_layout = device.create_pipeline_layout(&GpuPipelineLayoutDescriptor::new(&[
            js_sys::JsOption::wrap(uniform_layout.clone()),
            js_sys::JsOption::wrap(image_layout.clone()),
        ]));

        let module = shader_module(device, SCENE3D, "scene3d");
        let mesh_attrs = [
            GpuVertexAttribute::new(GpuVertexFormat::Float32x3, 0, 0),
            GpuVertexAttribute::new(GpuVertexFormat::Float32x3, 12, 1),
        ];
        let mesh_buffers =
            [js_sys::JsOption::wrap(GpuVertexBufferLayout::new(std::mem::size_of::<Vertex3D>() as u32, &mesh_attrs))];
        let target = GpuColorTargetState::new(format);
        target.set_blend(&alpha_blending());
        let targets = [js_sys::JsOption::wrap(target)];

        // One pipeline of the pass: its topology, culling, depth writes and
        // compare, and the slope-scaled bias (constant, slope) it carries.
        let pipeline = |layout: &web_sys::GpuPipelineLayout,
                        module: &web_sys::GpuShaderModule,
                        buffers: &[js_sys::JsOption<GpuVertexBufferLayout>],
                        topology: GpuPrimitiveTopology,
                        cull: GpuCullMode,
                        write: bool,
                        compare: GpuCompareFunction,
                        bias: (f32, f32),
                        label: &str|
         -> Result<GpuRenderPipeline, JsValue> {
            let vertex = GpuVertexState::new(module);
            vertex.set_entry_point("vs_main");
            vertex.set_buffers(buffers);
            let fragment = GpuFragmentState::new(module, &targets);
            fragment.set_entry_point("fs_main");
            let primitive = GpuPrimitiveState::new();
            primitive.set_topology(topology);
            primitive.set_cull_mode(cull);
            primitive.set_front_face(GpuFrontFace::Ccw);
            let depth = GpuDepthStencilState::new(DEPTH);
            depth.set_depth_write_enabled(write);
            depth.set_depth_compare(compare);
            depth.set_depth_bias(bias.0 as i32);
            depth.set_depth_bias_slope_scale(bias.1);
            let desc = GpuRenderPipelineDescriptor::new(layout, &vertex);
            desc.set_fragment(&fragment);
            desc.set_primitive(&primitive);
            desc.set_depth_stencil(&depth);
            desc.set_label(label);
            device.create_render_pipeline(&desc)
        };
        use GpuCompareFunction::{Less, LessEqual};
        use GpuPrimitiveTopology::{LineList, TriangleList};
        let none = (0.0, 0.0);
        let biased = wire_base_bias(1.0);
        let fills = [
            pipeline(&mesh_layout, &module, &mesh_buffers, TriangleList, GpuCullMode::Back, true, Less, none, "scene-fill")?,
            pipeline(&mesh_layout, &module, &mesh_buffers, TriangleList, GpuCullMode::Back, true, Less, biased, "scene-fill-biased")?,
            pipeline(&mesh_layout, &module, &mesh_buffers, TriangleList, GpuCullMode::None, false, Less, none, "scene-see-through")?,
            pipeline(&mesh_layout, &module, &mesh_buffers, TriangleList, GpuCullMode::None, false, Less, biased, "scene-see-through-biased")?,
        ];
        let lines = [
            pipeline(&mesh_layout, &module, &mesh_buffers, LineList, GpuCullMode::None, false, LessEqual, none, "scene-wires")?,
            pipeline(&mesh_layout, &module, &mesh_buffers, LineList, GpuCullMode::None, true, LessEqual, none, "scene-wires-see-through")?,
        ];
        let image_module = shader_module(device, SCENE3D_IMAGE, "scene3d-image");
        let image_attrs = [
            GpuVertexAttribute::new(GpuVertexFormat::Float32x3, 0, 0),
            GpuVertexAttribute::new(GpuVertexFormat::Float32x2, 12, 1),
        ];
        let image_buffers =
            [js_sys::JsOption::wrap(GpuVertexBufferLayout::new(std::mem::size_of::<ImageVertex3D>() as u32, &image_attrs))];
        let image = pipeline(&image_pipeline_layout, &image_module, &image_buffers, TriangleList, GpuCullMode::None, true, Less, none, "scene-image")?;

        let uniforms = Growable::new(device, STRIDE * 16, buffer_usage::UNIFORM, "scene-uniforms")?;
        let uniform_group = uniform_group(device, &uniform_layout, &uniforms);
        let image_verts = Growable::new(device, 1024, buffer_usage::VERTEX, "scene-image-quads")?;
        let l = glam::Vec3::from_array(DEFAULT_SCENE_LIGHT).normalize().to_array();
        Ok(Self {
            format,
            uniform_layout,
            fills,
            lines,
            image,
            uniforms,
            uniform_group,
            image_verts,
            meshes: Vec::new(),
            staged: None,
            light: l,
            target: None,
            backdrop_valid: false,
        })
    }

    pub(crate) fn create_mesh(&mut self, device: &GpuDevice, queue: &GpuQueue, verts: &[Vertex3D]) -> MeshId {
        let bytes: &[u8] = bytemuck::cast_slice(verts);
        let buffer = Growable::new(device, (bytes.len() as u32).max(64), buffer_usage::VERTEX, "mesh")
            .expect("a vertex buffer the device can make");
        let _ = buffer.write(queue, bytes);
        self.meshes.push(Mesh { buffer, count: verts.len() as u32 });
        MeshId(self.meshes.len() - 1)
    }

    pub(crate) fn update_mesh(&mut self, device: &GpuDevice, queue: &GpuQueue, id: MeshId, verts: &[Vertex3D]) {
        let Some(mesh) = self.meshes.get_mut(id.0) else { return };
        let bytes: &[u8] = bytemuck::cast_slice(verts);
        if mesh.buffer.ensure(device, bytes.len() as u32).is_err() {
            return;
        }
        let _ = mesh.buffer.write(queue, bytes);
        mesh.count = verts.len() as u32;
    }

    pub(crate) fn stage(&mut self, scissor: (u32, u32, u32, u32), draws: Vec<SceneDraw>) {
        self.staged = Some(Staged { scissor, draws, images: Vec::new() });
    }

    pub(crate) fn has_staged(&self) -> bool {
        self.staged.is_some()
    }

    pub(crate) fn stage_images(&mut self, images: Vec<SceneImage>) {
        if let Some(staged) = &mut self.staged {
            staged.images = images;
        }
    }

    /// Size the backdrop and depth to the canvas. A new backdrop holds no
    /// scene: `backdrop_valid` falls until a scene is drawn into it.
    pub(crate) fn fit(&mut self, device: &GpuDevice, w: u32, h: u32) -> Result<(), JsValue> {
        if self.target.as_ref().is_some_and(|t| (t.width, t.height) == (w, h)) {
            return Ok(());
        }
        let backdrop = texture(
            device,
            self.format,
            w,
            h,
            texture_usage::RENDER_ATTACHMENT | texture_usage::TEXTURE_BINDING | texture_usage::COPY_SRC,
            "scene-backdrop",
        )?;
        let depth = texture(device, DEPTH, w, h, texture_usage::RENDER_ATTACHMENT, "scene-depth")?;
        if let Some(old) = self.target.take() {
            old.backdrop.destroy();
        }
        self.target = Some(Target { view: whole_view(&backdrop)?, backdrop, depth_view: whole_view(&depth)?, width: w, height: h });
        self.backdrop_valid = false;
        Ok(())
    }

    /// Record the staged scene into the backdrop (after [`fit`](Self::fit)),
    /// consuming it. False when nothing was staged.
    pub(crate) fn record(
        &mut self,
        device: &GpuDevice,
        queue: &GpuQueue,
        encoder: &GpuCommandEncoder,
        image_group: &dyn Fn(u32) -> Option<GpuBindGroup>,
    ) -> Result<bool, JsValue> {
        let Some(staged) = self.staged.take() else { return Ok(false) };
        let Some(target) = &self.target else { return Ok(false) };
        let (w, h) = (target.width, target.height);

        let blocks = scene_uniforms(&staged.draws, &staged.images, [w as f32, h as f32], 0.0, self.light);
        let mut bytes = vec![0u8; blocks.len().max(1) * STRIDE as usize];
        for (i, b) in blocks.iter().enumerate() {
            let at = i * STRIDE as usize;
            bytes[at..at + UNIFORM_SIZE as usize].copy_from_slice(bytemuck::bytes_of(b));
        }
        if self.uniforms.ensure(device, bytes.len() as u32)? {
            self.uniform_group = uniform_group(device, &self.uniform_layout, &self.uniforms);
        }
        self.uniforms.write(queue, &bytes)?;
        let quads = image_quads_3d(&staged.images);
        let quad_bytes: &[u8] = bytemuck::cast_slice(&quads);
        self.image_verts.ensure(device, quad_bytes.len() as u32)?;
        self.image_verts.write(queue, quad_bytes)?;

        let color = GpuRenderPassColorAttachment::new_with_gpu_texture_view(GpuLoadOp::Clear, GpuStoreOp::Store, &target.view);
        color.set_clear_value(&[0.0, 0.0, 0.0, 0.0].map(js_sys::Number::from));
        let depth = GpuRenderPassDepthStencilAttachment::new_with_gpu_texture_view(&target.depth_view);
        depth.set_depth_load_op(GpuLoadOp::Clear);
        depth.set_depth_clear_value(1.0);
        depth.set_depth_store_op(GpuStoreOp::Discard);
        let desc = GpuRenderPassDescriptor::new(&[js_sys::JsOption::wrap(color)]);
        desc.set_depth_stencil_attachment(&depth);
        let pass = encoder.begin_render_pass(&desc)?;
        let (sx, sy, sw, sh) = staged.scissor;
        let (sx, sy) = (sx.min(w), sy.min(h));
        pass.set_scissor_rect(sx, sy, sw.min(w - sx), sh.min(h - sy));

        let offset = |slot: usize| [(slot as u32) * STRIDE];
        // The images due before mesh draw `at` (past the last draw, every
        // one left), in staged order.
        let draw_images = |at: usize| -> Result<(), JsValue> {
            for (j, image) in staged.images.iter().enumerate() {
                if (image.before as usize).min(staged.draws.len()) != at {
                    continue;
                }
                let Some(group) = image_group(image.image) else { continue };
                pass.set_pipeline(&self.image);
                pass.set_bind_group_with_u32_slice_and_u32_and_dynamic_offsets_data_length(
                    0,
                    Some(&self.uniform_group),
                    &offset(staged.draws.len() + j),
                    0,
                    1,
                )?;
                pass.set_bind_group(1, Some(&group));
                pass.set_vertex_buffer_with_u32(0, Some(&self.image_verts.buffer), 0);
                pass.draw_with_instance_count_and_first_vertex(6, 1, (j * 6) as u32);
            }
            Ok(())
        };
        for (i, draw) in staged.draws.iter().enumerate() {
            draw_images(i)?;
            let Some(mesh) = self.meshes.get(draw.mesh.0) else { continue };
            if mesh.count == 0 {
                continue;
            }
            let pipeline = if draw.wireframe {
                &self.lines[draw.see_through as usize]
            } else {
                &self.fills[2 * draw.see_through as usize + (draw.wire_base_width > 0.0) as usize]
            };
            pass.set_pipeline(pipeline);
            pass.set_bind_group_with_u32_slice_and_u32_and_dynamic_offsets_data_length(0, Some(&self.uniform_group), &offset(i), 0, 1)?;
            pass.set_vertex_buffer_with_u32(0, Some(&mesh.buffer.buffer), 0);
            pass.draw(mesh.count);
        }
        draw_images(staged.draws.len())?;
        pass.end();
        self.backdrop_valid = true;
        Ok(true)
    }
}
