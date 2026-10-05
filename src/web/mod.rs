//! The browser's side of cce-ui: a WebGPU renderer that draws the same
//! [`Frame2D`](crate::draw::Frame2D)s the Vulkan renderer does, from the same
//! shaders (`draw::shaders`), atlas (`draw::glyphs`) and image queue
//! (`draw::images`), and the shell that runs an `Application` on a canvas
//! with it ([`run`]) — the page's events in through the shared `Driver`,
//! `Pacer::turn` on animation frames.
//!
//! wasm32 only, and it needs web-sys's WebGPU bindings switched on
//! (`--cfg=web_sys_unstable_apis`, set for this crate in `.cargo/config.toml`).

mod renderer;
mod shell;

pub use renderer::{Capture, PendingCapture, WebRenderer};
pub use shell::{capture, run, Fonts, Sizing};
