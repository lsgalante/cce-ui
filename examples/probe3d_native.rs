//! The 3D probe (`probe3d/scene.rs`) on the Vulkan renderer, in the Wayland
//! runner: screenshot it and compare it with `probe3d_web`'s frame.
//! `PROBE3D_TRACE=1` runs the traced probe (`CCE_VK_RT=compute` keeps
//! Vulkan on the compute tier the browser has).

#[path = "probe3d/scene.rs"]
mod scene;

fn main() {
    if std::env::var("PROBE3D_TRACE").is_ok_and(|v| v == "1") {
        cce_ui::engine::run::<scene::Traced>();
    } else {
        cce_ui::engine::run::<scene::Raster>();
    }
}
