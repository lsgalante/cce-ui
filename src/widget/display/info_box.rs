//! `InfoBox` — pure display: a card with a title and lines of text. Under relief the card is a
//! raised pane plate (the DE plate fill under a rolled edge); flat, a themed card with a border.
//! The text is inset by the pane rung's padding on every side, in lines of the toolkit's text
//! leading with a half line between the title and the body, and the box declares the height
//! that holds it (`natural_height`).

use crate::color;
use crate::scene::layout::{Rect, Size};
use crate::scene::paint::PaintCtx;
use crate::widget::{Adapted, Input, Layout, Paint};

#[derive(Debug, Clone)]
pub struct InfoBox {
    pub title: String,
    pub lines: Vec<String>,
}

impl InfoBox {
    pub fn new(title: &str, lines: Vec<String>) -> Adapted<InfoBox> {
        Adapted::new(InfoBox { title: title.to_string(), lines })
    }

    /// The inset round the text, the same on all four sides: an InfoBox is a pane, and this
    /// is the pane rung's (`plate_padding`).
    fn pad() -> f32 {
        crate::layout::plate_padding().max(8.0)
    }

    /// One line of the box's text, title or body: the toolkit's text leading (a TextBox's
    /// line, a Label's height), so lines of prose do not touch.
    fn line_h(font_size: f32) -> f32 {
        font_size * 1.333
    }

    /// The tops of the title's line and of each body line, from the box's top `y`: the body
    /// starts half a line under the title.
    fn line_tops(&self, y: f32, font_size: f32) -> (f32, impl Iterator<Item = f32>) {
        let lh = Self::line_h(font_size);
        let title = y + Self::pad();
        let body = title + 1.5 * lh;
        (title, (0..self.lines.len()).map(move |i| body + i as f32 * lh))
    }

    /// The height that holds the title and every line in the padding: what the box declares
    /// (`intrinsic_size`), at the label font's size.
    pub fn natural_height(&self) -> f32 {
        let (_, font_size) = crate::layout::control_label_font_parsed();
        let lh = Self::line_h(font_size);
        let (title, body) = self.line_tops(0.0, font_size);
        let last = body.last().unwrap_or(title);
        last + lh + Self::pad()
    }
}

impl Layout for InfoBox {
    fn inline_label(&self) -> bool {
        true // draws its own title text
    }

    /// [`natural_height`](InfoBox::natural_height), any width (the lines do not wrap).
    fn intrinsic_size(&self) -> Option<Size> {
        Some(Size::new(0.0, self.natural_height()))
    }
}

impl Paint for InfoBox {
    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::control_label_font())
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let (x, y) = (rect.x, rect.y);
        let r = crate::layout::panel_corner_radius();
        let radii = (r, r, r, r);
        if crate::layout::control_relief() {
            // A pane plate, raised: the DE plate fill under a rolled edge —
            // the same surface a popover or a menu stands on. Its roll is the
            // control wall, capped by its height like every plate's.
            let fill = color::plate_color().unwrap_or_else(|| color::active_theme().surface_bg);
            let depth = crate::layout::bevel_width().min(rect.height * 0.2);
            ctx.plate(rect, radii, &crate::scene::material::Material::from_fill(fill), depth);
        } else {
            // Flat: the themed surface in a hairline frame.
            let theme = color::active_theme();
            ctx.border(rect, radii, theme.surface_bg, theme.surface_border, 1.0);
        }

        // The title in the DE highlight accent, the lines in the label colour;
        // sizes from the label font so the box reads like the controls around it.
        let (_, font_size) = crate::layout::control_label_font_parsed();
        let hc = color::to_srgb(crate::color::highlight_primary_color());
        let title_color = [(hc[0] * 255.0).round() as u8, (hc[1] * 255.0).round() as u8, (hc[2] * 255.0).round() as u8];
        let line_color = color::control_label_color_u8();
        let pad = Self::pad();
        let lh = Self::line_h(font_size);
        // Clipped to the box. The title and lines are caller-supplied text; a box sized
        // smaller than it declares cuts them at its edge.
        let clip = Some([x + pad, y, x + rect.width - pad, y + rect.height]);
        // Each in the middle of its line.
        let text_y = |top: f32| crate::layout::center_text_y(top, lh, font_size);
        let (title, body) = self.line_tops(y, font_size);
        ctx.text_with(self.title.clone(), x + pad, text_y(title), font_size, title_color, None, clip);
        for (line, top) in self.lines.iter().zip(body) {
            ctx.text_with(line.clone(), x + pad, text_y(top), font_size, line_color, None, clip);
        }
    }
}

impl Input for InfoBox {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::paint::Prim;

    /// The box declares the height that holds its text with the same padding above the title
    /// as below the last line, a line more for each body line; the lines are a full leading
    /// apart, never touching, and the body starts half a line further below the title.
    #[test]
    fn an_info_box_holds_its_lines_evenly_padded() {
        let (_, fs) = crate::layout::control_label_font_parsed();
        let (pad, lh) = (InfoBox::pad(), InfoBox::line_h(fs));
        let b = InfoBox::new("Title", vec!["one".into(), "two".into()]);
        let h = b.natural_height();
        assert!((h - (2.0 * pad + 3.5 * lh)).abs() < 0.01, "{h}");
        assert_eq!(Layout::intrinsic_size(&*b).map(|s| s.height), Some(h));
        let one = InfoBox::new("Title", vec!["one".into()]);
        assert!((h - one.natural_height() - lh).abs() < 0.01, "a line more per body line");

        let rect = Rect { x: 10.0, y: 20.0, width: 300.0, height: h };
        let mut pc = PaintCtx::new();
        Paint::paint(&*b, rect, &mut pc);
        let ys: Vec<f32> = pc.finish().items.iter().filter_map(|i| match &i.prim { Prim::Text { y, .. } => Some(*y), _ => None }).collect();
        assert_eq!(ys.len(), 3);
        let above = ys[0] - rect.y;
        let below = rect.y + rect.height - (ys[2] + crate::layout::line_height(fs));
        assert!((above - below).abs() < 0.01, "padded {above} above, {below} below");
        assert!((ys[2] - ys[1] - lh).abs() < 0.01 && (ys[1] - ys[0] - 1.5 * lh).abs() < 0.01, "{ys:?}");
    }
}


#[cfg(test)]
mod bounded_text_audit {
    use crate::scene::layout::Rect;
    use crate::scene::paint::{PaintCtx, Prim};
    use crate::widget::Paint;

    /// Every string a widget paints must carry a clip, so that a value longer
    /// than the box it was given is cut at the box instead of drawn across
    /// whatever sits beside it. This is the invariant the whole audit was
    /// about; it is asserted here over a sample of widgets rather than in each
    /// of their files so that a NEW widget drawing unbounded text trips it.
    ///
    /// The one deliberate exception is `Node`, which draws its name in the
    /// gutter beside itself and cannot know how much gutter it has — see the
    /// comment there. It is excluded on purpose, not forgotten.
    fn unbounded_strings<F: FnOnce(&mut PaintCtx)>(paint: F) -> Vec<String> {
        let mut pc = PaintCtx::new();
        paint(&mut pc);
        pc.finish()
            .items
            .iter()
            .filter_map(|i| match &i.prim {
                Prim::Text { text, bounds: None, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    /// A box narrower than any of its content — the shape that used to spill.
    const TIGHT: Rect = Rect { x: 40.0, y: 10.0, width: 50.0, height: 28.0 };

    #[test]
    fn info_box_text_is_bounded() {
        let b = super::InfoBox::new(
            "A title far wider than fifty pixels",
            vec!["and a line wider still, by some margin".to_string()],
        );
        let loose = unbounded_strings(|pc| Paint::paint(&*b, TIGHT, pc));
        assert!(loose.is_empty(), "InfoBox drew unbounded text: {loose:?}");
    }

    #[test]
    fn label_text_is_bounded() {
        let l = crate::widget::Label::new("a label considerably wider than its box");
        let loose = unbounded_strings(|pc| Paint::paint(&*l, TIGHT, pc));
        assert!(loose.is_empty(), "Label drew unbounded text: {loose:?}");
    }

    #[test]
    fn slider_readout_is_bounded() {
        let s = crate::widget::Slider::new();
        let loose = unbounded_strings(|pc| Paint::paint(&*s, TIGHT, pc));
        assert!(loose.is_empty(), "Slider drew unbounded text: {loose:?}");
    }

    #[test]
    fn checkbox_label_is_bounded() {
        let mut c = crate::widget::Checkbox::new();
        c.set_text("a checkbox label much wider than fifty pixels");
        let loose = unbounded_strings(|pc| Paint::paint(&*c, TIGHT, pc));
        assert!(loose.is_empty(), "Checkbox drew unbounded text: {loose:?}");
    }
}
