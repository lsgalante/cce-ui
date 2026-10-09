//! The ash renderer. One graphics queue, a classic render pass, two frames in
//! flight, FIFO (vsync) presentation. Memory goes through gpu-allocator; the
//! descriptor set mirrors `shader.wgsl`'s @group(0): sampled backdrop texture
//! (binding 0), sampler (binding 1), WindowInfo uniform (binding 2). Binding 0
//! is the scene backdrop until the first blur-behind plate: there the UI pass
//! suspends, the frame-so-far is copied into the snapshot image, and the
//! snapshot descriptor set takes over — so blur plates blur everything painted
//! beneath them, not just the 3D scene.
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | [`VkRenderer`] and its per-frame `Frame`, the push-constant and window-info sizes, `Drop` |
//! | `init` | `try_new` / `try_new_for`: instance, device, queues, pipelines, the stages |
//! | `surface` | the swapchain and its surface: create, recreate, detach and attach, resize, surface loss, the window-info block |
//! | `frame` | a frame: `draw_frame_2d` (the UI pass, the blur snapshots, the image and text passes, damage), uploads, text |
//! | `snapshot` | the blur-behind snapshot of the frame so far and its mip chain; the backdrop and snapshot targets |
//! | `stage3d` | meshes, lit meshes and the path tracer's scene, and the `Stage3D` / `LitStage3D` impls |
//! | `pipeline` | `create_ui_pipeline`, the one 2D pipeline (shared with `vk::plate_probe`) |
//! | `shaders` | the precompiled SPIR-V and run-time WGSL compilation |
//! | `damage` | the partial-frame rect arithmetic and the first-frost exemption |
//! | `images` | image subresource ranges, the snapshot's mip levels, image clears, the flipped viewport |
//! | `buffers` | host-visible buffers |

mod buffers;
mod damage;
mod frame;
mod images;
mod init;
mod pipeline;
mod shaders;
mod snapshot;
mod stage3d;
mod surface;

pub(crate) use buffers::*;
pub(crate) use damage::*;
pub(crate) use frame::*;
pub(crate) use images::*;
pub(crate) use pipeline::*;
pub(crate) use shaders::*;


use std::ffi::c_void;

use ash::vk;
use gpu_allocator::vulkan::{
    Allocation, AllocationCreateDesc, AllocationScheme, Allocator,
};
use gpu_allocator::MemoryLocation;

use crate::engine::Vertex;

use super::core::SurfaceLost;
use super::image::ImageStage;
pub use crate::draw::{Batch2D, Frame2D, PlatePush, MAX_PLATE_FEATURES};
pub(crate) use crate::draw::{batch_push_constants, PUSH_CONSTANT_FLOATS};
use super::rt::{PreparedRtScene, RtCamera, RtEnvironment, RtImage, RtImageSource, RtMaterial, RtStage, RtTriangle};
use super::scene::{MeshId, SceneDraw, SceneImage, SceneStage, Vertex3D};
use super::text::{TextSpan, TextStage};
pub(crate) use crate::draw::PLATE_FEATURE_BYTES;
pub(crate) use crate::draw::relief_px_at;

/// The block in bytes. **This is exactly `maxPushConstantsSize`'s
/// Vulkan-guaranteed minimum, so the budget is full** — every one of the 32
/// slots is written. That is why a new SDF mode reinterprets existing fields per
/// mode (5 reads `p_rect` as centre + radius, 6/7 as centre + radius + wedge
/// angle, 8 as centre + half-width with `p_radii.xy` a normal) instead of adding
/// one: there is nothing left to add.
///
/// A block over 128 bytes is not portable by construction — 128 is the floor
/// every conformant implementation must offer, and plenty of drivers offer no
/// more. So growing this means querying `limits.max_push_constants_size` at
/// device init and having a real fallback (a uniform buffer, or splitting the
/// block), not just raising the number. The assertion below is the tripwire: a
/// runtime check would be dead code today, because at exactly 128 it can never
/// fire on a conformant device.
pub(crate) const PUSH_CONSTANT_BYTES: u32 = (PUSH_CONSTANT_FLOATS * 4) as u32;

const _: () = assert!(
    PUSH_CONSTANT_BYTES <= 128,
    "the push-constant block has outgrown the 128-byte Vulkan-guaranteed minimum: \
     query limits.max_push_constants_size at device init and add a fallback path \
     before raising PUSH_CONSTANT_FLOATS"
);

pub(crate) const FRAMES_IN_FLIGHT: usize = 2;

/// shader2d's WindowInfo uniform, in bytes (layout in `draw::window_info_data`).
pub(crate) const WINDOW_INFO_BYTES: vk::DeviceSize = crate::draw::WINDOW_INFO_BYTES as vk::DeviceSize;

/// [`crate::draw::window_info_data`] for a Vulkan extent.
pub(crate) fn window_info_data(extent: vk::Extent2D, clip_corner_radius: f32, relief: (f32, f32)) -> [f32; crate::draw::WINDOW_INFO_BYTES / 4] {
    crate::draw::window_info_data(extent.width, extent.height, clip_corner_radius, relief)
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
    /// LOAD from PRESENT_SRC: a partial frame repaints inside a swapchain
    /// image that still holds an earlier frame.
    render_pass_partial: vk::RenderPass,
    /// Per swapchain image, what it is missing; reset with the swapchain.
    image_ages: Vec<ImageAge>,
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
    /// Whether a frame has needed the blur snapshot; until one has, it is
    /// not allocated (`sync_snapshot_target`).
    snapshot_wanted: bool,
    /// The snapshot's mip levels: 1 where the surface format cannot be
    /// blitted with a linear filter (`blur_mips`), else
    /// [`snapshot_levels`] of the surface. See `snapshot_mip_chain`.
    snapshot_levels: u32,
    /// Whether the surface format can be a blit's source and destination
    /// and filter linearly — what building the snapshot's mip chain takes.
    blur_mips: bool,
    /// Ask for the surface's minimum image count rather than one more
    /// (`set_minimal_swapchain`).
    minimal_swapchain: bool,
    backdrop_sampler: vk::Sampler,
    window_info: AllocatedBuffer,
    /// The bevel-profile generation `window_info` was last written with —
    /// `draw_frame_2d` rewrites the UBO when the layout global moves on.
    profile_gen: u64,
    /// The pinned relief heights (carve drop, roll rise) in physical px as
    /// last uploaded in WindowInfo — compared each frame, since editors set
    /// them straight into the style registry with no generation counter.
    relief_uploaded: (f32, f32),
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
    /// See [`VkRenderer::set_rt_background`].
    rt_background: Option<[f32; 3]>,
    /// See [`VkRenderer::set_rt_environment`].
    rt_environment: RtEnvironment,

    desired_extent: vk::Extent2D,
    corner_radius_px: f32,
    swapchain_dirty: bool,
    present_mode: vk::PresentModeKHR,
    present_debug_count: u64,
    /// Set when a surface call reports the surface lost (see [`SurfaceLost`]):
    /// the display connection is dead, so every later draw is skipped until
    /// a new surface is attached, rather than re-failing (and re-logging)
    /// each frame while the caller's event loop finds out for itself.
    surface_lost: bool,

    // Declared last: everything above must be destroyed before the device/
    // instance the core tears down in its own Drop.
    core: super::core::VkCore,
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
            self.core.device.destroy_render_pass(self.render_pass_partial, None);
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

    /// The blur snapshot halves down to a single texel or to the cap, so a
    /// tiny surface is not asked for levels it cannot have, and a large one
    /// does not build levels no stride reads.
    #[test]
    fn the_blur_snapshot_has_as_many_levels_as_the_widest_stride_reads() {
        use super::{snapshot_levels, SNAPSHOT_LEVELS_MAX};
        let e = |width, height| ash::vk::Extent2D { width, height };
        assert_eq!(snapshot_levels(e(1, 1)), 1);
        assert_eq!(snapshot_levels(e(2, 1)), 2);
        assert_eq!(snapshot_levels(e(40, 7)), 6);
        assert_eq!(snapshot_levels(e(2560, 1600)), SNAPSHOT_LEVELS_MAX);
        // The coarsest level's texel covers a 64 px stride.
        assert_eq!(1 << (SNAPSHOT_LEVELS_MAX - 1), 64);
    }

    /// The WebGPU variants validate with no capabilities at all — WebGPU has
    /// no push constants — and the 2D one carries its block as the uniform
    /// the web renderer binds. naga does not see everything a browser's
    /// compiler rejects (its derivative-uniformity analysis does not follow
    /// calls), so the browser probe is the last word; this catches a
    /// substitution that silently stopped applying.
    #[test]
    fn the_webgpu_shaders_validate_without_push_constants() {
        let web2d = crate::draw::shaders::shader2d_for_webgpu();
        assert!(!web2d.contains("var<push_constant>"));
        assert!(web2d.contains("@group(1) @binding(0) var<uniform> rrect_clip: RRectClip;"));
        for (name, src) in [("shader2d (web)", web2d.as_str()), ("glyph", crate::draw::shaders::GLYPH)] {
            let module = naga::front::wgsl::parse_str(src).unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(src)));
            naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty())
                .validate(&module)
                .unwrap_or_else(|e| panic!("{name} does not validate for WebGPU: {e:?}"));
        }
        // And the block's size is what the renderers lay out.
        let module = naga::front::wgsl::parse_str(&web2d).unwrap();
        let block = module.types.iter().find(|(_, t)| t.name.as_deref() == Some("RRectClip")).expect("RRectClip").1;
        let naga::TypeInner::Struct { span, .. } = block.inner else { panic!("RRectClip is a struct") };
        assert_eq!(span as usize, crate::draw::PUSH_CONSTANT_FLOATS * 4);
        assert!(span as usize <= crate::draw::shaders::WEBGPU_BLOCK_STRIDE);
    }

    #[test]
    fn scene3d_compiles() {
        assert!(!super::scene3d_spirv().is_empty());
    }

    #[test]
    fn scene3d_image_compiles() {
        assert!(!super::scene3d_image_spirv().is_empty());
    }

    /// `WINDOW_INFO_BYTES` sizes the uniform buffer AND its descriptor range,
    /// and `write_window_info` addresses it by float index — all three have to
    /// agree with shader2d's `WindowInfo` struct, and nothing but a comment
    /// said so. A field appended to the WGSL without growing the const writes
    /// the new value past the end of the buffer, which is a validation error
    /// on a good day and a garbage uniform on a bad one.
    ///
    /// Reads the struct out of the shader source rather than duplicating its
    /// shape here, so it measures the thing it is guarding.
    /// build.rs compiles the fixed shaders with its own copy of
    /// `compile_wgsl`'s options. If the two drift, the embedded SPIR-V is no
    /// longer what this crate means by the shader; compare word for word.
    #[test]
    fn precompiled_spirv_matches_runtime_compile() {
        let cases: [(&str, &str, fn() -> &'static [u32]); 5] = [
            ("shader2d", crate::draw::shaders::SHADER2D, super::shader2d_spirv),
            ("glyph", crate::draw::shaders::GLYPH, super::glyph_spirv),
            ("scene3d", crate::draw::shaders::SCENE3D, super::scene3d_spirv),
            ("scene3d_image", crate::draw::shaders::SCENE3D_IMAGE, super::scene3d_image_spirv),
            ("scene3d_lit", crate::draw::shaders::SCENE3D_LIT, super::scene3d_lit_spirv),
        ];
        for (name, source, precompiled) in cases {
            assert!(
                super::compile_wgsl(source) == precompiled(),
                "{name}: build.rs output differs from compile_wgsl"
            );
        }
    }

    #[test]
    fn window_info_layout_matches_the_uniform_size() {
        let src = crate::draw::shaders::SHADER2D;
        let body = src
            .split_once("struct WindowInfo {")
            .expect("WindowInfo moved; this test scans for it")
            .1
            .split_once("\n}")
            .expect("unterminated WindowInfo")
            .0;

        let mut floats = 0usize;
        for line in body.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            let ty = line.split_once(':').expect("field: type").1.trim().trim_end_matches(',');
            floats += match ty {
                "f32" => 1,
                "vec2<f32>" | "vec2f" => 2,
                "vec4<f32>" | "vec4f" => 4,
                // std140-ish: an array of vec4 is its element count x 4.
                t if t.starts_with("array<vec4f,") => {
                    let n: usize = t
                        .trim_start_matches("array<vec4f,")
                        .trim_end_matches('>')
                        .trim()
                        .parse()
                        .expect("array length");
                    n * 4
                }
                other => panic!("WindowInfo field type {other} is not in this test's size table"),
            };
        }

        assert_eq!(
            floats * 4,
            super::WINDOW_INFO_BYTES as usize,
            "WindowInfo is {floats} floats ({} bytes); WINDOW_INFO_BYTES says {}",
            floats * 4,
            super::WINDOW_INFO_BYTES,
        );
    }
}
