pub mod color;
pub mod widget;
pub mod layout;
pub mod wayland;
pub mod protocol;

pub mod colors {
    pub use crate::color::*;
}

pub const SHADER: &str = include_str!("shader.wgsl");
