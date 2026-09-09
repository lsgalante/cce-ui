//! `RampPreview` — the `(ramp)` config type's inline preview: the spec's
//! value curve drawn as a lit polyline in a dark well. Read-only, the
//! sibling of [`super::bevel_preview::BevelPreview`]: it exists to SHOW the
//! curve and take a click, which hosts (cce-data-editor) answer by opening
//! the full `cce-ramp` editor on the key. The spec format is the DE-wide
//! ramp string (`"smooth;0.000:0.150,0.400:1.000,…"` — see
//! [`super::ramp::parse_ramp_spec`]).

use crate::scene::layout::Rect;
use crate::scene::paint::{Cap, PaintCtx};
use crate::widget::{Adapted, ElementState, Event, EventCtx, Input, Layout, MouseButton, Paint};

pub struct RampPreview {
    /// Parsed spec: sorted `(pos, value)` keys + smooth/linear blending.
    keys: Vec<(f32, f32)>,
    smooth: bool,
    just_clicked: bool,
    hovered: bool,
}

impl RampPreview {
    pub fn new() -> Adapted<RampPreview> {
        Adapted::new(RampPreview {
            keys: vec![(0.0, 0.0), (1.0, 1.0)],
            smooth: false,
            just_clicked: false,
            hovered: false,
        })
    }

    /// Set the previewed curve from a ramp spec string; anything unparsable
    /// falls back to the linear identity.
    pub fn set_spec_str(&mut self, s: &str) {
        match crate::widget::parse_ramp_spec(s) {
            Some((keys, smooth)) => {
                self.keys = keys;
                self.smooth = smooth;
            }
            None => {
                self.keys = vec![(0.0, 0.0), (1.0, 1.0)];
                self.smooth = false;
            }
        }
    }

    /// The curve's value at `t` — endpoint-clamped, per-segment linear or
    /// smoothstep, mirroring `Ramp::get_interpolated_value` and the policy
    /// crate's `ramp_value`.
    fn value_at(&self, t: f32) -> f32 {
        let keys = &self.keys;
        if keys.is_empty() {
            return 0.0;
        }
        if t <= keys[0].0 {
            return keys[0].1;
        }
        if t >= keys[keys.len() - 1].0 {
            return keys[keys.len() - 1].1;
        }
        for pair in keys.windows(2) {
            let (p1, v1) = pair[0];
            let (p2, v2) = pair[1];
            if t >= p1 && t <= p2 {
                let range = p2 - p1;
                if range.abs() < 1e-4 {
                    return v1;
                }
                let mut w = (t - p1) / range;
                if self.smooth {
                    w = w * w * (3.0 - 2.0 * w);
                }
                return v1 * (1.0 - w) + v2 * w;
            }
        }
        keys[keys.len() - 1].1
    }
}

impl Layout for RampPreview {}

impl Paint for RampPreview {
    fn color(&self) -> [f32; 4] {
        // The well bg is emitted in `paint` (hover-dependent); no base fill.
        [0.0, 0.0, 0.0, 0.0]
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        // The opening: the shared canvas well (`PaintCtx::well_floor`), its floor
        // lifted on hover (the click cue); the rim is drawn last, over the content.
        let radius = crate::layout::textbox_corner_radius();
        ctx.well_floor(rect, radius, self.hovered);

        let m = 4.0f32;
        let x_l = rect.x + m;
        let x_r = rect.x + rect.width - m;
        let y_hi = rect.y + m;
        let y_lo = rect.y + rect.height - m;
        if x_r <= x_l || y_lo <= y_hi {
            ctx.well_rim(rect, radius, crate::layout::control_relief());
            return;
        }
        let y_of = |v: f32| y_lo - v.clamp(0.0, 1.0) * (y_lo - y_hi);

        // The curve, lit per segment under the DE light azimuth — the same
        // treatment as BevelPreview's surface stroke, so the two types read
        // as siblings in a key list.
        let az = crate::layout::light_source_position();
        let (lx, ly) = (az.cos(), -az.sin());
        let base_c = [0.60f32, 0.65, 0.74];
        let n_seg = 24usize;
        let mut prev = (x_l, y_of(self.value_at(0.0)));
        for i in 1..=n_seg {
            let t = i as f32 / n_seg as f32;
            let x = x_l + (x_r - x_l) * t;
            let y = y_of(self.value_at(t));
            let (dx, dy) = (x - prev.0, y - prev.1);
            let len = (dx * dx + dy * dy).sqrt().max(1e-3);
            let (nx, ny) = (dy / len, -dx / len);
            let lit = (nx * lx + ny * ly) * 0.35;
            let col = [
                (base_c[0] + lit).clamp(0.0, 1.0),
                (base_c[1] + lit).clamp(0.0, 1.0),
                (base_c[2] + lit).clamp(0.0, 1.0),
                1.0,
            ];
            ctx.vector(prev.0, prev.1, x, y, 1.5, col, Cap::Round);
            prev = (x, y);
        }

        ctx.well_rim(rect, radius, crate::layout::control_relief());
    }
}

impl Input for RampPreview {
    fn on_event(&mut self, event: &Event, _ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, .. } => {
                self.just_clicked = true;
                true
            }
            Event::MouseEnter => {
                self.hovered = true;
                false
            }
            Event::MouseLeave => {
                self.hovered = false;
                false
            }
            _ => false,
        }
    }

    fn take_click(&mut self) -> bool {
        std::mem::take(&mut self.just_clicked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_parse_and_interpolate() {
        let mut p = RampPreview {
            keys: vec![],
            smooth: false,
            just_clicked: false,
            hovered: false,
        };
        p.set_spec_str("linear;0.0:0.0,0.5:1.0,1.0:0.0");
        assert!((p.value_at(0.25) - 0.5).abs() < 1e-4);
        assert!((p.value_at(0.5) - 1.0).abs() < 1e-4);
        // Unparsable falls back to the identity, not to empty.
        p.set_spec_str("garbage");
        assert!((p.value_at(0.5) - 0.5).abs() < 1e-4);
    }
}
