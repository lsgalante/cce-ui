//! Composing a page out of widgets on a flat host: the walk that replays a widget onto a
//! [`RenderTarget`](crate::scene::paint::RenderTarget) (`render`), settings-page sections
//! (`section`: `PageFlow`, `SectionContext`) and [`Form`] (`form`: declared rows, cells and
//! `lay_row`). Above the style getters in `layout`, which it reads, and beside `widget`,
//! whose widgets it places.

mod form;
mod render;
mod section;

pub use form::{lay_row, line_height as form_line_height, text_width as form_text_width, Cell, Form, Group as FormGroup};
pub use render::*;
pub use section::*;
