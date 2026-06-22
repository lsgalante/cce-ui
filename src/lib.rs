pub mod color;
pub mod widget;
pub mod config;
pub mod layout;
pub mod wayland;
pub mod protocol;
pub mod engine;
pub mod scale;
pub mod backend;
pub mod context;
pub mod process;

pub mod colors {
    pub use crate::color::*;
}

pub const SHADER: &str = include_str!("shader.wgsl");

