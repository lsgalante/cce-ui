//! The renderer probe's scene in the native Wayland runner — see
//! `examples/probe/scene.rs`. Screenshot it in a shadow (or headless) session
//! and compare with `probe_web`'s capture of the same scene.

#[path = "probe/scene.rs"]
mod scene;

fn main() {
    cce_ui::engine::run::<scene::ProbeApp>();
}
