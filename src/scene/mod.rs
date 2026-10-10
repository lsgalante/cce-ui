//! The cce-ui core (`docs/rfc-core-rebuild.md`): the generational node [`Arena`] and the widget
//! tree on it (`tree`), the box model (`layout`), the paint vocabulary and display list (`paint`)
//! with the walk that fills it (`painter`), animation (`anim`), materials, the relief's shading
//! (`relief_shade`) and the relief as a height field (`heightfield`). Every frame is built
//! through it.

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
