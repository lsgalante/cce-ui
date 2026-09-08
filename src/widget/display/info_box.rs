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

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let (x, y) = (rect.x, rect.y);
        let theme = colors::active_theme();
        // A card: the themed surface in a hairline frame, rounded at the plate
        // radius like every other surface set on the plate.
        let r = crate::layout::plate_corner_radius();
        ctx.border(rect, (r, r, r, r), theme.surface_bg, theme.surface_border, 1.0);

        ctx.text(self.title.clone(), x + 16.0, y + 12.0, 12.0, [89, 165, 229]);
        let mut current_y = y + 32.0;
        for (idx, line) in self.lines.iter().enumerate() {
            let color = if idx == self.lines.len() - 1 { [140, 140, 153] } else { [204, 204, 217] };
            ctx.text(line.clone(), x + 16.0, current_y, 11.0, color);
            current_y += 16.0;
        }
    }
}

impl Input for InfoBox {}
