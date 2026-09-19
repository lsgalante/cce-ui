//! Narrow-trait info box (Phase 5i leaf sweep). Pure display: themed card + border + title/lines.

use crate::colors;
use crate::scene::layout::Rect;
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
}

impl Layout for InfoBox {
    fn inline_label(&self) -> bool {
        true // draws its own title text
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
        let r = crate::layout::plate_corner_radius();
        let radii = (r, r, r, r);
        if crate::layout::control_relief() {
            // A pane plate, raised: the DE plate fill under a rolled edge —
            // the same surface a popover or a menu stands on. Its roll is the
            // control wall, capped by its height like every plate's.
            let fill = colors::plate_color().unwrap_or_else(|| colors::active_theme().surface_bg);
            let depth = crate::layout::bevel_width().min(rect.height * 0.2);
            ctx.plate(rect, radii, fill, depth);
        } else {
            // Flat: the themed surface in a hairline frame.
            let theme = colors::active_theme();
            ctx.border(rect, radii, theme.surface_bg, theme.surface_border, 1.0);
        }

        // The title in the DE highlight accent, the lines in the label colour;
        // sizes from the label font so the box reads like the controls around it.
        let (_, font_size) = crate::layout::control_label_font_parsed();
        let hc = colors::to_srgb(crate::color::highlight_primary_color());
        let title_color = [(hc[0] * 255.0).round() as u8, (hc[1] * 255.0).round() as u8, (hc[2] * 255.0).round() as u8];
        let line_color = colors::control_label_color_u8();
        let pad = crate::layout::plate_padding().max(8.0);
        let line_h = crate::layout::line_height(font_size);
        // Clipped to the box. The title and lines are caller-supplied text in
        // a box the caller also sizes, so nothing here guarantees they fit.
        let clip = Some([x + pad, y, x + rect.width - pad, y + rect.height]);
        ctx.text_with(self.title.clone(), x + pad, y + pad, font_size, title_color, None, clip);
        let mut current_y = y + pad + line_h * 1.4;
        for line in &self.lines {
            ctx.text_with(line.clone(), x + pad, current_y, font_size, line_color, None, clip);
            current_y += line_h;
        }
    }
}

impl Input for InfoBox {}


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
