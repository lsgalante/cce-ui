//! The 3D probe (`probe3d/scene.rs`) on the Vulkan renderer, in the Wayland
//! runner: screenshot it and compare it with `probe3d_web`'s frame.

#[path = "probe3d/scene.rs"]
mod scene;

fn main() {
    cce_ui::engine::run::<scene::Probe3d>();
}
