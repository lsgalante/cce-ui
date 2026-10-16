//! What the palette draws: its strip — a canvas well with the swatches and the add slot in
//! it — and while a swatch is selected its colour selector and delete button.

use super::*;
use super::color_paint::paint_peg;

/// The add slot's `plus` glyph: the grey a dropdown's arrow is drawn in (raw sRGB, as a text
/// colour is).
const ADD_TINT: [f32; 4] = [0x83 as f32 / 255.0, 0x83 as f32 / 255.0, 0x8a as f32 / 255.0, 1.0];

impl Paint for ColorPalette {
    /// No container box: the palette draws its strip and fields itself, on the host's plate,
    /// as the ramps do.
    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    // The fields are the palette's own values, never in the registry, and painted here.
    fn paints_own_subtree(&self) -> bool {
        true
    }

    /// The selector's hex is shaped here: it is the palette's value, never in the registry,
    /// so the host's text pass cannot reach it (its caret reads the shaped offsets).
    fn prepare_text(&mut self, fs: &mut cosmic_text::FontSystem, _rect: Rect) {
        if self.selected_idx.is_some() {
            self.selector.prepare_text(fs);
        }
    }

    fn paint(&self, _rect: Rect, pc: &mut PaintCtx) {
        // The strip is the colour ramp's kind of track: a canvas well a control high, at the
        // control radius, its rim lit while it has the keyboard. The swatches stand in it,
        // the add slot's glyph after them.
        let strip = self.strip_rect();
        let radius = crate::layout::control_corner_radius();
        pc.well_floor(strip, radius, &crate::scene::Material::pane(), false);
        pc.clip_rounded(strip, radius, |pc| {
            pc.icon("plus", self.add_rect(), ADD_TINT);
            for idx in self.swatch_order() {
                let selected = Some(idx) == self.selected_idx;
                paint_peg(pc, self.slot_centre(idx), Self::SWATCH_R, self.swatches[idx], selected);
            }
        });
        pc.well_rim_lit(strip, radius, crate::layout::control_relief(), self.walk().at == Some(Self::STRIP));

        if self.selected_idx.is_some() {
            let dummy = UiContext::new();
            self.selector.paint_self(&dummy, pc);
            if self.deletable() {
                self.del_button.paint_self(&dummy, pc);
            }
        }
    }
}
