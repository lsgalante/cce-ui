//! The rebuilt cce-ui core spine (see `docs/rfc-core-rebuild.md`).
//!
//! This module is being grown additively alongside the existing widget system; nothing here is
//! wired into the live render path yet. Phase 1 lands the ownership foundation — a generational
//! node [`Arena`]. Later phases add the layout pass, the paint/display-list, and animation on
//! top of the same node identity.

pub mod anim;
pub mod arena;
pub mod heightfield;
pub mod layout;
pub mod material;
pub mod paint;
pub mod painter;
pub mod relief_shade;
pub mod tree;

pub use arena::{Arena, Node, NodeId};
pub use material::{Finish, Frost, FrostDef, Material, MaterialDef, PlateRole, PlateRung};
pub use tree::WidgetTree;
