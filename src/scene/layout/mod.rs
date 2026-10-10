//! The box model: a hand-rolled two-phase solver that runs over an [`Arena`] of [`LayoutBox`]es,
//! independent of paint (deliberately not taffy):
//!
//!   * **measure** (bottom-up): each node reports an intrinsic [`Size`] from its children (or, for
//!     a leaf, the content size its host gave it). Written into `LayoutBox::measured`.
//!   * **arrange** (top-down): each node is given a final [`Rect`] and positions its children
//!     within it. Written into `LayoutBox::rect`.
//!
//! Because it operates on `Style` + `Size` and writes plain rects, it is fully unit-testable
//! without a GPU or a Wayland surface.
//!
//! Layout modes ([`LayoutMode`]): **Flex** (row/column with grow/shrink, gap, padding, main/cross
//! alignment incl. stretch), **Stack** (Z-overlay with per-axis alignment), and **Grid** (fixed
//! column count with uniform column width and per-row heights). Not handled: wrapping,
//! percentage lengths, width-dependent adaptive grids, and measuring text — a leaf's size is
//! whatever its host measured ([`LayoutBox::leaf`]).
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the geometry values: `Size`, `Rect`, `Edges`, and fitting an image into a rect |
//! | `style` | what a node asks for: axes, modes, lengths, alignment, `Style` and the spacing-ladder presets, `LayoutBox` |
//! | `solve` | `compute_layout`, `measure` and `arrange`, per mode, and the axis helpers they share |

mod solve;
mod style;
#[cfg(test)]
mod tests;

pub use solve::*;
pub use style::*;

use crate::scene::arena::{Arena, NodeId};

/// A width/height pair in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Size = Size { width: 0.0, height: 0.0 };
    pub fn new(width: f32, height: f32) -> Self {
        Size { width, height }
    }
}

/// A positioned box in logical pixels (absolute coordinates after arrange).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const ZERO: Rect = Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 };

    /// Whether the point `(x, y)` lies in the rect: the left and top edges
    /// are inside, the right and bottom edges are not.
    ///
    /// Half-open on purpose, so rects that share an edge — a row of tabs, a
    /// list's rows — partition the space between them: a point on the
    /// shared edge belongs to exactly one, whichever order they are tested
    /// in. A rect with no width or height contains nothing.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

/// How an image maps into a bounding box — see [`fit_rect`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FitMode {
    /// Aspect-preserving: the image fills the box on its long axis and
    /// letterboxes on the other, never scaling up past `max_upscale`
    /// (1.0 = never enlarge; f32::INFINITY = always fill).
    Contain { max_upscale: f32 },
    /// The full box, aspect ignored.
    Stretch,
}

/// The rect an `img_w` × `img_h` image occupies inside `bounds` under `mode`,
/// centered on both axes. Zero-sized images yield a zero rect at the box
/// center rather than a division blow-up.
pub fn fit_rect(img_w: u32, img_h: u32, bounds: Rect, mode: FitMode) -> Rect {
    match mode {
        FitMode::Stretch => bounds,
        FitMode::Contain { max_upscale } => {
            if img_w == 0 || img_h == 0 {
                return Rect {
                    x: bounds.x + bounds.width * 0.5,
                    y: bounds.y + bounds.height * 0.5,
                    width: 0.0,
                    height: 0.0,
                };
            }
            let (iw, ih) = (img_w as f32, img_h as f32);
            let scale = (bounds.width / iw).min(bounds.height / ih).min(max_upscale).max(0.0);
            let (w, h) = (iw * scale, ih * scale);
            Rect {
                x: bounds.x + (bounds.width - w) * 0.5,
                y: bounds.y + (bounds.height - h) * 0.5,
                width: w,
                height: h,
            }
        }
    }
}

/// Per-side spacing (padding).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edges {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl Edges {
    pub const ZERO: Edges = Edges { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 };
    pub fn all(v: f32) -> Self {
        Edges { left: v, right: v, top: v, bottom: v }
    }
}
