//! The toolkit's raw-Vulkan (ash) renderer — the workspace-wide replacement for
//! the wgpu backend, grown in cce-designer (milestones 1–3 + cutover) and moved
//! here for general use.
//!
//! `VkRenderer` owns the whole stack: instance (+ validation layers in debug
//! builds or with `CCE_VK_VALIDATION=1`), `VK_KHR_wayland_surface`, sRGB
//! swapchain (premultiplied alpha, FIFO), gpu-allocator memory, and three
//! pipelines compiled from WGSL through naga at startup:
//!
//! - **2D** (`shader2d.wgsl`): the union of the engine and designer dialects —
//!   quads with circle clipping, the wavy-blob sentinel, window-corner
//!   rounding (radius 0 disables), and blur-behind plates (negative alpha)
//!   sampling the renderer-managed backdrop.
//! - **Text** (`glyph.wgsl` + [`TextSpan`]): cosmic-text shaping (via glyphon's
//!   re-export) + swash rasterization into a self-managed glyph atlas, with
//!   per-span bounds clipping, rotation, and circle clipping.
//! - **3D** (`scene3d.wgsl` + [`SceneDraw`]): handle-based [`Vertex3D`] meshes
//!   with a depth buffer, rendered into the full-size backdrop image
//!   (scissored to a pane) and copied beneath the UI pass — the same image is
//!   the blur-behind source.
//!
//! A fourth pipeline is built lazily on first use:
//!
//! - **RT** (`rt.wgsl` + [`RtTriangle`]/[`RtMaterial`]/[`RtCamera`]): the
//!   tier-1 compute path tracer — CPU-built SAH BVH in storage buffers,
//!   progressive accumulation, blitted into the backdrop's viewport-pane
//!   region as a drop-in alternative to the raster 3D pass. Plain compute:
//!   no `VK_KHR_ray_*` needed.
//!
//! The wgpu↔Vulkan Y-flip is a negative-height viewport (like wgpu-hal), NOT
//! naga's ADJUST_COORDINATE_SPACE — a shader-side flip would reverse winding
//! and break the 3D pipeline's back-face culling.

mod core;
pub mod image;
mod renderer;
mod rt;
mod scene;
mod text;

pub use core::VkCore;
pub use image::{free_image, upload_rgba, ImageQuad};
pub use renderer::{Batch2D, Frame2D, VkRenderer};
pub use rt::{RtCamera, RtMaterial, RtOffscreen, RtTriangle};
pub use scene::{MeshId, SceneDraw, Vertex3D};
pub use text::TextSpan;

/// Pay the process-wide, window-independent renderer costs up front: the
/// shared Vulkan instance (ICD enumeration + driver init), one throwaway
/// device (loads the driver's device-level libraries), and the naga WGSL
/// compiles. For daemon-style processes (cce-cloud) that build a renderer per
/// window: called at daemon startup, it moves the multi-second cold-cache hit
/// off the first window's critical path.
pub fn prewarm() {
    renderer::shader2d_spirv();
    renderer::glyph_spirv();
    renderer::scene3d_spirv();
    drop(VkCore::new_headless());
}
