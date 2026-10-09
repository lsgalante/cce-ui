//! `PaintCtx`'s flat emitters: quads, images and cce-icons glyphs, rounded rects, glow, vectors,
//! circles, arcs and borders.

use super::*;

impl PaintCtx {

    pub fn quad(&mut self, rect: Rect, color: [f32; 4]) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Quad { rect, color });
    }

    /// A user image (id from `cce_ui::vk::upload_rgba`) drawn at `rect`.
    pub fn image(&mut self, image: u32, rect: Rect, alpha: f32) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Image { image, rect, alpha });
    }

    /// A bundled cce-icons glyph (`cce-icons/svg/<name>.svg`) drawn at
    /// `rect`, tinted `color` — given as a text colour is, raw sRGB, so a
    /// glyph and the label beside it match — with the colour's alpha as the
    /// image's. Rasterized at twice the rect's longer side so it stays crisp
    /// on a 2x output, and cached (see [`crate::upload_icon_tinted`]).
    /// `false`, and nothing drawn, when the glyph is missing.
    ///
    /// The ONE way the toolkit draws a symbol: a chevron, a mark, a + or a
    /// − is this, never a character in whatever face the font falls back to.
    /// A `weather-*` glyph carries its own colours; draw it with
    /// [`icon_untinted`](Self::icon_untinted).
    pub fn icon(&mut self, name: &str, rect: Rect, color: [f32; 4]) -> bool {
        let px = (rect.width.max(rect.height) * 2.0).ceil().max(1.0) as u32;
        match crate::upload_icon_tinted(name, px, crate::icon_tint(color)) {
            Some((id, _, _)) => {
                self.image(id, rect, color[3]);
                true
            }
            None => false,
        }
    }

    /// [`icon`](Self::icon) for a glyph that carries its own colours (the
    /// `weather-*` family): drawn as it is, at `alpha`.
    pub fn icon_untinted(&mut self, name: &str, rect: Rect, alpha: f32) -> bool {
        let px = (rect.width.max(rect.height) * 2.0).ceil().max(1.0) as u32;
        match crate::upload_icon(name, px) {
            Some((id, _, _)) => {
                self.image(id, rect, alpha);
                true
            }
            None => false,
        }
    }

    pub fn rounded_rect(&mut self, rect: Rect, radius: f32, corners: (bool, bool, bool, bool), color: [f32; 4]) {
        let rect = self.apply_offset(rect);
        self.push(Prim::RoundedRect { rect, radius, corners, color });
    }

    /// Feathered aura over a rounded rect (see [`Prim::Glow`]): interior at
    /// the color's alpha, smooth per-pixel falloff to zero across `reach` px
    /// outside the boundary.
    pub fn glow(&mut self, rect: Rect, radius: f32, reach: f32, color: [f32; 4]) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Glow { rect, radius, reach, color });
    }

    pub fn vector(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, thickness: f32, color: [f32; 4], cap: Cap) {
        let (ox, oy) = self.offset;
        self.push(Prim::Vector { x1: x1 + ox, y1: y1 + oy, x2: x2 + ox, y2: y2 + oy, thickness, color, cap });
    }

    pub fn circle(&mut self, cx: f32, cy: f32, radius: f32, color: [f32; 4]) {
        let (ox, oy) = self.offset;
        self.push(Prim::Circle { cx: cx + ox, cy: cy + oy, radius, color });
    }

    pub fn border(&mut self, rect: Rect, radii: Radii, fill: [f32; 4], border: [f32; 4], thickness: f32) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Border { rect, radii, fill, border, thickness });
    }

    pub fn arc(&mut self, cx: f32, cy: f32, radius: f32, thickness: f32, start: f32, end: f32, color: [f32; 4]) {
        let (ox, oy) = self.offset;
        self.push(Prim::Arc { cx: cx + ox, cy: cy + oy, radius, thickness, start, end, color });
    }

    /// A radially-shaded ring band — see [`Prim::ArcShaded`].
    #[allow(clippy::too_many_arguments)]
    pub fn arc_shaded(
        &mut self,
        cx: f32,
        cy: f32,
        radius: f32,
        thickness: f32,
        start: f32,
        end: f32,
        inner: [f32; 4],
        crest: [f32; 4],
        outer: [f32; 4],
    ) {
        let (ox, oy) = self.offset;
        self.push(Prim::ArcShaded {
            cx: cx + ox,
            cy: cy + oy,
            radius,
            thickness,
            start,
            end,
            inner,
            crest,
            outer,
        });
    }
}
