//! 3D scene stage: the ash port of the app's "3D canvas render pass". Draws
//! Vertex3D meshes (scene3d.wgsl: mvp transform, `SceneDraw::screen_space`
//! background quads, window-corner discard) into the full-size backdrop image with
//! a depth buffer, scissored to the viewport pane. The renderer then copies the
//! backdrop into the swapchain image and draws the UI pass over it — the same
//! image doubles as the blur-behind source for the 2D shader, replacing
//! milestone 1's 1x1 placeholder.
//!
//! Meshes are handle-based (`MeshId`); per-draw uniforms (mvp + window info) go
//! into one dynamic-offset uniform buffer per frame in flight, so a frame's
//! draws share a single descriptor set.
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the stage's state, staging a frame's draws, images and lit draws, teardown |
//! | `pipelines` | building the stage: render pass, layouts, pipelines, per-frame buffers and descriptors |
//! | `targets` | the backdrop and depth images: their extent, building them for a size, freeing them |
//! | `mesh` | meshes: creating them, replacing their vertices without waiting on the device, reclaiming spares |
//! | `record` | writing a frame's uniforms and recording the pass |

mod mesh;
mod pipelines;
mod record;
mod targets;

use mesh::Mesh;

use ash::vk;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme, Allocator};
use gpu_allocator::MemoryLocation;

use super::renderer::{create_cpu_buffer, destroy_cpu_buffer, AllocatedBuffer};

pub use crate::draw::scene::{MeshId, SceneDraw, SceneImage, Vertex3D};
use crate::draw::lit::{lit_uniforms, LitDraw, LitLight, LitMeshId, LitVertex, LIT_UNIFORM_SIZE};
use crate::draw::scene::{image_quads_3d, scene_uniforms, wire_base_bias, ImageVertex3D, SceneUniforms, DEFAULT_SCENE_LIGHT, UNIT_INSTANCE};

const UNIFORM_SIZE: vk::DeviceSize = std::mem::size_of::<SceneUniforms>() as vk::DeviceSize;

/// One uniform slot holds either a scene block or a lit one.
const SLOT_SIZE: vk::DeviceSize = if (LIT_UNIFORM_SIZE as vk::DeviceSize) > UNIFORM_SIZE {
    LIT_UNIFORM_SIZE as vk::DeviceSize
} else {
    UNIFORM_SIZE
};

struct StagedScene {
    scissor: (u32, u32, u32, u32),
    draws: Vec<SceneDraw>,
    images: Vec<SceneImage>,
    lit: Vec<LitDraw>,
}

struct SceneFrame {
    uniforms: AllocatedBuffer,
    /// Six vertices per staged `SceneImage`, in staged order.
    image_verts: AllocatedBuffer,
    descriptor_set: vk::DescriptorSet,
    draw_count: u32,
}

pub(crate) struct SceneStage {
    render_pass: vk::RenderPass,
    pipeline: vk::Pipeline,
    /// PolygonMode::LINE twin of `pipeline` — None when the device lacks
    /// fillModeNonSolid (wireframe draws then fall back to the fill pipeline).
    wireframe_pipeline: Option<vk::Pipeline>,
    /// `pipeline` with culling off and depth writes off — the
    /// `SceneDraw::see_through` fill.
    see_through_pipeline: vk::Pipeline,
    /// `wireframe_pipeline` WITH depth writes — the wires of a see-through
    /// fill (`SceneDraw::see_through` on a wireframe draw).
    wireframe_see_through_pipeline: vk::Pipeline,
    /// Device cap for `SceneDraw::line_width` (1.0 without wideLines).
    max_line_width: f32,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    shader_module: vk::ShaderModule,
    uniform_stride: vk::DeviceSize,
    /// The `SceneImage` pipeline: set 0 is the scene's uniforms, set 1 a
    /// user image's descriptor set (`image::image_set_bindings`).
    image_pipeline: vk::Pipeline,
    image_pipeline_layout: vk::PipelineLayout,
    image_set_layout: vk::DescriptorSetLayout,
    image_shader_module: vk::ShaderModule,
    /// The `LitDraw` pipeline (`draw::lit`): set 0 the scene's uniforms
    /// (a lit block in the slot), set 1 the base-colour image's set, as the
    /// image pipeline has it. No culling: the shader lights a back face by
    /// its flipped normal.
    lit_pipeline: vk::Pipeline,
    lit_shader_module: vk::ShaderModule,
    lit_meshes: Vec<Mesh>,
    pub(crate) lit_light: LitLight,
    /// A 1x1 white image the renderer uploads, bound for an untextured lit
    /// draw (and one whose texture is not resident): set 1 must be bound.
    pub(crate) lit_fallback_image: Option<u32>,

    format: vk::Format,
    extent: vk::Extent2D,
    pub(crate) backdrop_image: vk::Image,
    pub(crate) backdrop_view: vk::ImageView,
    backdrop_allocation: Option<Allocation>,
    depth_image: vk::Image,
    depth_view: vk::ImageView,
    depth_allocation: Option<Allocation>,
    framebuffer: vk::Framebuffer,

    meshes: Vec<Mesh>,
    /// Frames submitted so far, counted by the renderer as it submits them.
    pub(crate) submitted: u64,
    /// Every frame numbered below this one has finished on the GPU, as the
    /// renderer learns by waiting on a frame slot's fence
    /// ([`SceneStage::frame_waited`]).
    complete_before: u64,
    /// The one instance a draw without instances is drawn with
    /// ([`UNIT_INSTANCE`]), bound in binding 1 in place of an instance mesh.
    unit_instance: AllocatedBuffer,
    frames: Vec<SceneFrame>,
    staged: Option<StagedScene>,
    /// True once the backdrop holds rendered content worth copying to screen.
    pub(crate) backdrop_valid: bool,
    /// Whether the app has ever staged a scene (or a traced one). Until it
    /// has, the backdrop and depth targets are 1×1 — see
    /// [`Scene::target_extent`].
    pub(crate) wanted: bool,
    /// Toward the flat shading's light, unit length — see
    /// `VkRenderer::set_scene_light`.
    pub(crate) light: [f32; 3],
}

impl SceneStage {
    pub(crate) fn stage(&mut self, scissor: (u32, u32, u32, u32), draws: Vec<SceneDraw>) {
        self.staged = Some(StagedScene { scissor, draws, images: Vec::new(), lit: Vec::new() });
    }

    /// The staged scene's lit draws; nothing when no scene is staged.
    pub(crate) fn stage_lit(&mut self, draws: Vec<LitDraw>) {
        if let Some(staged) = &mut self.staged {
            staged.lit = draws;
        }
    }

    /// The staged scene's images; nothing when no scene is staged.
    pub(crate) fn stage_images(&mut self, images: Vec<SceneImage>) {
        if let Some(staged) = &mut self.staged {
            staged.images = images;
        }
    }

    pub(crate) fn destroy(&mut self, device: &ash::Device, allocator: &mut Allocator) {
        self.destroy_targets(device, allocator);
        unsafe {
            for frame in &mut self.frames {
                let mut uniforms = std::mem::replace(&mut frame.uniforms, AllocatedBuffer::null());
                destroy_cpu_buffer(device, allocator, &mut uniforms);
                let mut quads = std::mem::replace(&mut frame.image_verts, AllocatedBuffer::null());
                destroy_cpu_buffer(device, allocator, &mut quads);
            }
            for mesh in self.meshes.iter_mut().chain(self.lit_meshes.iter_mut()) {
                mesh.destroy(device, allocator);
            }
            let mut unit = std::mem::replace(&mut self.unit_instance, AllocatedBuffer::null());
            destroy_cpu_buffer(device, allocator, &mut unit);
            device.destroy_descriptor_pool(self.descriptor_pool, None);
            device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            if let Some(p) = self.wireframe_pipeline.take() {
                device.destroy_pipeline(p, None);
            }
            device.destroy_pipeline(self.lit_pipeline, None);
            device.destroy_shader_module(self.lit_shader_module, None);
            device.destroy_pipeline(self.image_pipeline, None);
            device.destroy_pipeline_layout(self.image_pipeline_layout, None);
            device.destroy_descriptor_set_layout(self.image_set_layout, None);
            device.destroy_shader_module(self.image_shader_module, None);
            device.destroy_pipeline(self.see_through_pipeline, None);
            device.destroy_pipeline(self.wireframe_see_through_pipeline, None);
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.pipeline_layout, None);
            device.destroy_shader_module(self.shader_module, None);
            device.destroy_render_pass(self.render_pass, None);
        }
    }
}
