//! The client-facing names, re-exported in one place: what an app's
//! `use cce_ui::engine::{...}` reaches. The portable contract first; the
//! native shell's entry point and Wayland-typed items after it.

pub use crate::backend::app::{
    Application, AppSender, LogicalPosition, LogicalSize, RenderContext, Stage3D, WindowAction, WindowSettings,
};
pub use crate::draw::rt::{RtCamera, RtEnvironment, RtImage, RtMaterial, RtTriangle};
pub use crate::draw::scene::{MeshId, SceneDraw, SceneImage, Vertex3D};
pub use crate::backend::driver::PressedKey;
pub use crate::backend::tessellate::{
    Vertex, LineCap,
    quad_vertices, quad_vertices_with_clip, quad_vertices_clipped, line_vertices,
    vector_vertices, rounded_rect_vertices_corners, push_rounded_rect_vertices_corners,
    rounded_rect_vertices, push_rounded_rect_vertices, plate_bevel_vertices,
    push_plate_bevel_vertices, widget_vertices, push_widget_vertices,
    extra_quad_vertices, push_extra_quad_vertices, extra_quad_vertices_clipped,
    push_extra_quad_vertices_clipped, circle_vertices, circle_border_vertices,
    arc_background_vertices, push_arc_background_vertices, push_plate_solid_border_vertices,
};
pub use crate::backend::text::{get_text_buffer_laid_out, shaped_cluster_offsets, shaping_for};
pub use cursor_icon::CursorIcon;

#[cfg(not(any(target_arch = "wasm32", target_os = "macos")))]
pub use crate::backend::app::{LayerAnchor, LayerKeyboardInteractivity, LayerKind, LayerSettings};
#[cfg(not(any(target_arch = "wasm32", target_os = "macos")))]
pub use crate::backend::window_runner::{run, xdg_toplevel, EngineState};
#[cfg(target_os = "macos")]
pub use crate::mac::run;
