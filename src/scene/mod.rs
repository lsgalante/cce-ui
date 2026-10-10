//! The cce-ui core (`docs/rfc-core-rebuild.md`): the generational node [`Arena`] (the widget
//! tree on it is `widget::tree`), the box model (`layout`), the paint vocabulary and display list (`paint`;
//! the walk that fills it from widgets is `widget::painter`), animation (`anim`), materials, the relief's shading
//! (`relief_shade`) and the relief as a height field (`heightfield`). Every frame is built
//! through it.

pub mod anim;
pub mod arena;
pub mod heightfield;
pub mod layout;
pub mod material;
pub mod paint;
pub mod relief_shade;

pub use arena::{Arena, Node, NodeId};
pub use material::{Finish, Frost, FrostDef, Material, MaterialDef, PlateRole, PlateRung};
