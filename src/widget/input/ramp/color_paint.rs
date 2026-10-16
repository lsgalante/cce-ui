//! What the colour ramp draws: its track — a canvas well with the gradient in it — its key
//! pegs, and while a key is selected its sliders and delete button.

use super::*;
use super::color::mix;

impl Paint for ColorRamp {
    /// No container box: the editor draws its track, pegs and fields itself, on the host's
    /// plate, as the float Ramp does.
    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    // The fields are the editor's own values, never in the registry, and painted here
    // (while a key is selected): the walk must not also descend.
    fn paints_own_subtree(&self) -> bool {
        true
    }

    fn paint(&self, _rect: Rect, pc: &mut PaintCtx) {
        // The track is a canvas well cut through the host's plate, the one the float Ramp's
        // graph and every draw-in canvas share: its floor, the gradient, then its rim over the
        // gradient's edges. At the CONTROL radius, not the panel one: the track is a control's
        // height (`ramp_height`, 32), and the panel's 24 would round it into a pill; at the
        // text box's radius it reads as the field it is. The gradient is clipped to the well's
        // rounded shape, so its square slices never poke past the corners.
        let track = self.track_rect();
        let radius = crate::layout::control_corner_radius();
        pc.well_floor(track, radius, &crate::scene::Material::pane(), false);
        pc.clip_rounded(track, radius, |pc| self.paint_gradient(track, pc));
        // Lit while the track has the keyboard.
        pc.well_rim_lit(track, radius, crate::layout::control_relief(), self.walk().at == Some(Self::TRACK));

        self.paint_pegs(pc);

        // The fields paint themselves (`paint_self`), once.
        if self.selected_key_idx.is_some() {
            let dummy = UiContext::new();
            self.r_slider.paint_self(&dummy, pc);
            self.g_slider.paint_self(&dummy, pc);
            self.b_slider.paint_self(&dummy, pc);
            if self.deletable() {
                self.del_button.paint_self(&dummy, pc);
            }
        }
    }
}

impl ColorRamp {
    /// The gradient across `track`: each run between two keys in slices at most a logical
    /// pixel wide, each the run's colour at its middle (as
    /// [`get_interpolated_color`](Self::get_interpolated_color) has it), so no step shows and
    /// every key's colour meets the track exactly at its position; before the first key and
    /// after the last, that key's colour, solid. There is no gradient primitive to hand it
    /// to; a track's slices are about one per pixel of its width.
    fn paint_gradient(&self, track: Rect, pc: &mut PaintCtx) {
        let (Some(first), Some(last)) = (self.keys.first(), self.keys.last()) else { return };
        let x_at = |t: f32| track.x + t.clamp(0.0, 1.0) * track.width;
        let mut run = |x0: f32, x1: f32, c: [f32; 3]| {
            if x1 > x0 {
                pc.quad(Rect { x: x0, y: track.y, width: x1 - x0, height: track.height }, [c[0], c[1], c[2], 1.0]);
            }
        };
        run(track.x, x_at(first.pos), first.color);
        for pair in self.keys.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            let (x0, x1) = (x_at(a.pos), x_at(b.pos));
            let n = (x1 - x0).ceil() as usize;
            for i in 0..n {
                let (s0, s1) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
                let c = mix(a.color, b.color, (s0 + s1) / 2.0);
                run(x0 + s0 * (x1 - x0), x0 + s1 * (x1 - x0), c);
            }
        }
        run(x_at(last.pos), track.x + track.width, last.color);
    }

    /// The key pegs ([`paint_peg`]), in paint order — the selected one last, on top.
    fn paint_pegs(&self, pc: &mut PaintCtx) {
        for idx in self.peg_order() {
            paint_peg(pc, self.peg_centre(idx), Self::PEG_R, self.keys[idx].color, Some(idx) == self.selected_key_idx);
        }
    }
}

/// One colour peg of face radius `r` centred at `(cx, cy)`: a colour ramp's key. Its face is `color`, opaque: the face IS the data, so selection never tints it; a
/// `selected` peg wears the focus ring outside the face instead. Under relief the face is a
/// disc plate ([`PaintCtx::plate_shaped`] at `Some(2.0)`, a true circle) with a control
/// plate's roll (the relief width, at most 0.4 of the radius); flat, a flat disc in a hairline
/// of the wells' frame colour, so a colour the plate's still shows.
pub(super) fn paint_peg(pc: &mut PaintCtx, (cx, cy): (f32, f32), r: f32, color: [f32; 3], selected: bool) {
    let face = [color[0], color[1], color[2], 1.0];
    if crate::layout::control_relief() {
        let depth = crate::layout::bevel_width().min(0.4 * r);
        let disc = Rect { x: cx - r, y: cy - r, width: 2.0 * r, height: 2.0 * r };
        pc.plate_shaped(disc, (r, r, r, r), &crate::scene::Material::opaque(face), depth, Some(2.0));
    } else {
        pc.circle(cx, cy, r + ColorRamp::PEG_HAIRLINE, crate::color::well_frame_color(false, false));
        pc.circle(cx, cy, r, face);
    }
    if selected {
        let ring = crate::scene::paint::ControlPlate::focus_tint();
        pc.arc(cx, cy, r + PEG_RING_GAP + PEG_RING_W, PEG_RING_W, 0.0, std::f32::consts::TAU, [ring[0], ring[1], ring[2], 1.0]);
    }
}
