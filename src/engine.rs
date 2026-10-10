//! The client-facing names, re-exported in one place: what an app's
//! `use cce_ui::engine::{...}` reaches. The portable contract first; the
//! native shell's entry point and Wayland-typed items after it.

pub use crate::backend::app::{
    accessibility_tree, Application, AppSender, LogicalPosition, LogicalSize, RenderContext, Stage3D, WindowAction, WindowSettings,
};
pub use crate::draw::lit::{LitDraw, LitLight, LitMaterial, LitMeshId, LitStage3D, LitVertex};
pub use crate::draw::rt::{PreparedRtScene, RtCamera, RtEnvironment, RtImage, RtMaterial, RtTriangle};
pub use crate::draw::scene::{MeshId, SceneDraw, SceneImage, Vertex3D};
pub use crate::backend::driver::PressedKey;
pub use crate::backend::tessellate::{
    Vertex, LineCap,
    quad_vertices, quad_vertices_with_clip, line_vertices,
    vector_vertices, rounded_rect_vertices_corners, push_rounded_rect_vertices_corners,
    push_plate_bevel_vertices,
    circle_vertices, push_arc_background_vertices, push_plate_solid_border_vertices,
};
pub use crate::text::{get_text_buffer_laid_out, shaped_cluster_offsets, shaping_for};
pub use cursor_icon::CursorIcon;

#[cfg(not(any(target_arch = "wasm32", target_os = "macos")))]
pub use crate::backend::app::{LayerAnchor, LayerKeyboardInteractivity, LayerKind, LayerSettings};
#[cfg(not(any(target_arch = "wasm32", target_os = "macos")))]
pub use crate::backend::window_runner::{run, xdg_toplevel, EngineState};
#[cfg(target_os = "macos")]
pub use crate::mac::run;
