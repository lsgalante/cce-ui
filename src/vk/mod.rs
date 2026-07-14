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
//! The wgpu↔Vulkan Y-flip is a negative-height viewport (like wgpu-hal), NOT
//! naga's ADJUST_COORDINATE_SPACE — a shader-side flip would reverse winding
//! and break the 3D pipeline's back-face culling.

mod renderer;
mod scene;
mod text;

pub use renderer::{Batch2D, Frame2D, VkRenderer};
pub use scene::{MeshId, SceneDraw, Vertex3D};
pub use text::TextSpan;
