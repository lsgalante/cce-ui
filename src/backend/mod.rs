pub mod app;
pub mod appkit;
pub mod dom;
pub mod driver;
pub mod frame;
pub mod shell;
pub mod tessellate;
/// Text shaping and the shaped-buffer cache moved to the top-level `text`
/// module (2026-10-10): widgets need it, and it is not part of a shell.
/// Re-exported here for apps that still name the old path.
pub use crate::text;
pub mod touch;
// The Wayland shell: native, but for macOS.
#[cfg(not(any(target_arch = "wasm32", target_os = "macos")))]
pub mod a11y_unix;
#[cfg(not(any(target_arch = "wasm32", target_os = "macos")))]
pub mod dnd;
#[cfg(not(any(target_arch = "wasm32", target_os = "macos")))]
pub mod menu_popup;
#[cfg(not(any(target_arch = "wasm32", target_os = "macos")))]
pub mod text_input;
#[cfg(not(any(target_arch = "wasm32", target_os = "macos")))]
pub mod window_runner;


pub use app::{Application, AppSender, LogicalPosition, LogicalSize, WindowSettings};
pub use driver::PressedKey;
pub use tessellate::{LineCap, Vertex};
pub use text::get_text_buffer;
#[cfg(not(any(target_arch = "wasm32", target_os = "macos")))]
pub use window_runner::{run, EngineState};
#[cfg(target_os = "macos")]
pub use crate::mac::run;
