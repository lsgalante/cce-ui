//! Text: alignment and layout attributes, and `PaintCtx`'s text emitters.

use super::*;

/// Horizontal alignment of laid-out (boxed) text — the toolkit-plain mirror of
/// `cosmic_text::Align`, mapped at shape time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AlignH {
    #[default]
    Left,
    Center,
    Right,
}

/// Vertical alignment of laid-out text within its box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AlignV {
    #[default]
    Top,
    Middle,
    Bottom,
}

/// Box layout for a [`Prim::Text`]: word-wrap width (`Some` ⇒ multiline wrap; `None` ⇒ single
/// run) and horizontal/vertical alignment within a box of `box_height`. All lengths are logical.
/// The backend shapes it with `get_text_buffer_laid_out`, cached under the box as well as the
/// text, and applies the vertical offset from the shaped height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextLayout {
    pub wrap_width: Option<f32>,
    pub box_height: f32,
    pub align_h: AlignH,
    pub align_v: AlignV,
}

/// Optional shaping attributes for a [`Prim::Text`] — the subset a widget can request beyond
/// family + size. `weight` is the OpenType weight (400 regular, 700 bold); `None` leaves the
/// family default. `stretch` is the OpenType width class (`usWidthClass`, 1 ultra-condensed
/// … 5 normal … 9 ultra-expanded); `None` is normal width — what picks a family's Narrow or
/// Condensed cut over its normal-width sibling at the same weight. Kept toolkit-plain (no
/// cosmic-text types) like the rest of the scene layer; the backend maps them onto
/// `cosmic_text::Style`/`Weight`/`Stretch` at shape time.
///
/// Build one with `..Default::default()` after the fields you set, so a field added here
/// does not break every literal in the sibling crates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextAttrs {
    pub italic: bool,
    pub weight: Option<u16>,
    pub stretch: Option<u16>,
}

impl PaintCtx {

    pub fn text(&mut self, text: impl Into<String>, x: f32, y: f32, font_size: f32, color: [u8; 3]) {
        self.text_with(text, x, y, font_size, color, None, None);
    }

    /// Text with a per-label font and clip rect (`[l, t, r, b]`, local space) — what the
    /// legacy `text_labels_with_font_and_bounds` tuples carry, expressible in the display
    /// list since Phase 6.
    pub fn text_with(
        &mut self,
        text: impl Into<String>,
        x: f32,
        y: f32,
        font_size: f32,
        color: [u8; 3],
        font: Option<String>,
        bounds: Option<[f32; 4]>,
    ) {
        self.text_attrs(text, x, y, font_size, color, font, bounds, TextAttrs::default());
    }

    /// [`text_with`](PaintCtx::text_with) plus shaping attributes (italic / weight) — what the
    /// font picker's style-variant previews need beyond family + size.
    #[allow(clippy::too_many_arguments)]
    pub fn text_attrs(
        &mut self,
        text: impl Into<String>,
        x: f32,
        y: f32,
        font_size: f32,
        color: [u8; 3],
        font: Option<String>,
        bounds: Option<[f32; 4]>,
        attrs: TextAttrs,
    ) {
        let (ox, oy) = self.offset;
        let bounds = bounds.map(|[l, t, r, b]| [l + ox, t + oy, r + ox, b + oy]);
        self.push(Prim::Text { text: text.into(), x: x + ox, y: y + oy, font_size, color, alpha: 1.0, font, bounds, attrs, layout: None });
    }

    /// [`text_with`](PaintCtx::text_with) plus a glyph alpha (1.0 = opaque) — translucent
    /// labels (a pane fading out) without changing the sRGB u8 color convention.
    #[allow(clippy::too_many_arguments)]
    pub fn text_faded(
        &mut self,
        text: impl Into<String>,
        x: f32,
        y: f32,
        font_size: f32,
        color: [u8; 3],
        alpha: f32,
        font: Option<String>,
        bounds: Option<[f32; 4]>,
    ) {
        let (ox, oy) = self.offset;
        let bounds = bounds.map(|[l, t, r, b]| [l + ox, t + oy, r + ox, b + oy]);
        self.push(Prim::Text {
            text: text.into(),
            x: x + ox,
            y: y + oy,
            font_size,
            color,
            alpha,
            font,
            bounds,
            attrs: TextAttrs::default(),
            layout: None,
        });
    }

    /// Boxed text: word-wrap + horizontal/vertical alignment within a box (a placed text box).
    /// Unlike [`text_with`](PaintCtx::text_with), the backend shapes this with the box layout
    /// applied (cached per box). `x, y` are the box's top-left; the backend applies the vertical offset.
    #[allow(clippy::too_many_arguments)]
    pub fn text_boxed(
        &mut self,
        text: impl Into<String>,
        x: f32,
        y: f32,
        font_size: f32,
        color: [u8; 3],
        font: Option<String>,
        bounds: Option<[f32; 4]>,
        attrs: TextAttrs,
        layout: TextLayout,
    ) {
        let (ox, oy) = self.offset;
        let bounds = bounds.map(|[l, t, r, b]| [l + ox, t + oy, r + ox, b + oy]);
        self.push(Prim::Text {
            text: text.into(),
            x: x + ox,
            y: y + oy,
            font_size,
            color,
            alpha: 1.0,
            font,
            bounds,
            attrs,
            layout: Some(layout),
        });
    }
}
