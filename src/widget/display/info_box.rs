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
        ctx.text(self.title.clone(), x + pad, y + pad, font_size, title_color);
        let mut current_y = y + pad + line_h * 1.4;
        for line in &self.lines {
            ctx.text(line.clone(), x + pad, current_y, font_size, line_color);
            current_y += line_h;
        }
    }
}

impl Input for InfoBox {}
