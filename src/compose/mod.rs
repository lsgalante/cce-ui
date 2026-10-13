//! Composing a page out of widgets: placing a widget in a block ([`place`]), settings-page
//! sections (`section`: `PageFlow`, `SectionContext`) and [`Form`] (`form`: declared rows, cells
//! and `lay_row`), all painting onto any `P: AsMut<PaintCtx>` — a frame's own
//! [`PaintCtx`](crate::scene::paint::PaintCtx), or a host's page buffer around one. `render`
//! still holds the legacy walk that replays a widget onto a flat
//! [`RenderTarget`](crate::scene::paint::RenderTarget). Above the style getters in `layout`, which
//! it reads, and beside `widget`, whose widgets it places and paints.

mod form;
mod render;
mod section;

pub use form::{lay_row, line_height as form_line_height, text_width as form_text_width, Cell, Form, Group as FormGroup};
pub use render::*;
pub use section::*;

/// A text colour given as `[r, g, b, a]` in 0..1 (what a page passes), as the paint vocabulary
/// takes it: sRGB bytes.
fn rgb(c: [f32; 4]) -> [u8; 3] {
    let byte = |v: f32| (v * 255.0).clamp(0.0, 255.0) as u8;
    [byte(c[0]), byte(c[1]), byte(c[2])]
}
