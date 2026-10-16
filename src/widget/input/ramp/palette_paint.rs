//! What the palette draws: its strip — a canvas well with the swatches and the add slot in
//! it — and while a swatch is selected its colour selector and delete button.

use super::*;
use crate::scene::paint::{ControlPlate, PlateStance};

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
        // control radius, its rim lit while it has the keyboard. The swatch tiles stand in it,
        // the add slot's glyph after them.
        let strip = self.strip_rect();
        let radius = crate::layout::control_corner_radius();
        pc.well_floor(strip, radius, &crate::scene::Material::pane(), false);
        pc.clip_rounded(strip, radius, |pc| {
            pc.icon("plus", self.add_rect(), ADD_TINT);
            for idx in self.swatch_order() {
                self.paint_swatch(idx, pc);
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

impl ColorPalette {
    /// Swatch `idx`'s tile: a control plate whose face is the swatch's colour, opaque — the
    /// face IS the data, so selection never tints it or its rim (a lit rim showed only on the
    /// tile's lit top edge, too faint to read). The selected tile wears the ramps' selection
    /// ring outside it instead, in the focus colour, as a colour ramp's selected peg does.
    /// Raised under relief; flat, the face alone in a hairline of the wells' frame colour, so
    /// a swatch the colour of the plate still shows.
    fn paint_swatch(&self, idx: usize, pc: &mut PaintCtx) {
        let c = self.swatches[idx];
        let face = crate::scene::Material::opaque([c[0], c[1], c[2], 1.0]);
        let (rect, radius) = (self.slot_rect(idx), Self::swatch_radius());
        if crate::layout::control_relief() {
            pc.control_plate(&ControlPlate::control(rect, radius, PlateStance::Raised, Some(face)));
        } else {
            // The hairline as a colour ramp's flat peg draws its own: a shape of the frame
            // colour one hairline larger, under the face, so the two never part at a corner.
            let h = ColorRamp::PEG_HAIRLINE;
            let under = Rect { x: rect.x - h, y: rect.y - h, width: rect.width + 2.0 * h, height: rect.height + 2.0 * h };
            pc.rounded_rect(under, radius + h, (true, true, true, true), crate::color::well_frame_color(false, false));
            pc.control_plate(&ControlPlate::control(rect, radius, PlateStance::Flat, Some(face)));
        }
        if Some(idx) == self.selected_idx {
            let (ring, out) = (ControlPlate::focus_tint(), PEG_RING_GAP + PEG_RING_W);
            let r = radius + out;
            let outer = Rect { x: rect.x - out, y: rect.y - out, width: rect.width + 2.0 * out, height: rect.height + 2.0 * out };
            pc.border(outer, (r, r, r, r), [0.0; 4], [ring[0], ring[1], ring[2], 1.0], PEG_RING_W);
        }
    }
}
