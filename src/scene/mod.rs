//! The rebuilt cce-ui core spine (see `docs/rfc-core-rebuild.md`).
//!
//! This module is being grown additively alongside the existing widget system; nothing here is
//! wired into the live render path yet. Phase 1 lands the ownership foundation — a generational
//! node [`Arena`]. Later phases add the layout pass, the paint/display-list, and animation on
//! top of the same node identity.

pub mod anim;
pub mod arena;
pub mod layout;
pub mod paint;
pub mod painter;
pub mod tree;

pub use arena::{Arena, Node, NodeId};
pub use tree::WidgetTree;
