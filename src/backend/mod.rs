pub mod app;
pub mod driver;
pub mod frame;
pub mod shell;
pub mod tessellate;
pub mod text;
// The Wayland shell: native only.
#[cfg(not(target_arch = "wasm32"))]
pub mod dnd;
#[cfg(not(target_arch = "wasm32"))]
pub mod menu_popup;
#[cfg(not(target_arch = "wasm32"))]
pub mod window_runner;

pub use app::{Application, AppSender, LogicalPosition, LogicalSize, WindowSettings};
pub use driver::PressedKey;
pub use tessellate::{LineCap, Vertex};
pub use text::get_text_buffer;
#[cfg(not(target_arch = "wasm32"))]
pub use window_runner::{run, EngineState};
