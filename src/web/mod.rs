//! The browser's side of cce-ui: a WebGPU renderer that draws the same
//! [`Frame2D`](crate::draw::Frame2D)s the Vulkan renderer does, from the same
//! shaders (`draw::shaders`), atlas (`draw::glyphs`) and image queue
//! (`draw::images`). The browser shell that drives it — the page's events in,
//! `Pacer::turn` on animation frames — is the next step of the port (W3).
//!
//! wasm32 only, and it needs web-sys's WebGPU bindings switched on
//! (`--cfg=web_sys_unstable_apis`, set for this crate in `.cargo/config.toml`).

mod renderer;

pub use renderer::{Capture, WebRenderer};
