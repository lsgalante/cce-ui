//! Geometry the layers below `scene` share: [`Rect`], the positioned box every
//! layer passes around. It lives at the bottom so a style getter that answers in
//! rects (`layout::carve_inside`) names nothing above it; `scene::layout`
//! re-exports it, and `cce_ui::scene::layout::Rect` is still the path apps use.

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
