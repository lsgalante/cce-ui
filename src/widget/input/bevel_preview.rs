//! `BevelPreview` — the `(bevel)` config type's inline preview: a miniature
//! lit cross-section of a relief profile (plateau → wall → floor), shaped by
//! a "shoulder,base,bias" knob triple. Read-only: it exists to SHOW the
//! current material and take a click, which hosts (cce-data-editor) answer
//! by opening the full `cce-relief` editor. The curve family and knob
//! semantics are cce-relief's — see [`bevel_ease`].

use crate::scene::layout::Rect;
use crate::scene::paint::{Cap, PaintCtx};
use crate::widget::{Adapted, ElementState, Event, EventCtx, Input, Layout, MouseButton, Paint};

/// The relief profile curve family shared by `cce-relief` and this preview:
/// the two-exponent rational ease `h(w) = w^a / (w^a + (1-w)^b)` over a bias
/// pre-warp `w = v^g`. Monotone and endpoint-exact; slider midpoints
/// (0.5, 0.5, 0.5) give a=b=2, g=1 — the analytic smoothstep. Exponents run
/// 0.5 (sharp crease) → 2 → 8 (wide round-over).
pub fn bevel_ease(shoulder: f32, base: f32, bias: f32, v: f32) -> f32 {
    let a = 2.0 * 4f32.powf(2.0 * shoulder - 1.0);
    let be = 2.0 * 4f32.powf(2.0 * base - 1.0);
    let g = 4f32.powf(2.0 * bias - 1.0);
    let w = v.clamp(0.0, 1.0).powf(g);
    let num = w.powf(a);
    let den = num + (1.0 - w).powf(be);
    if den <= f32::EPSILON {
        return if w > 0.5 { 1.0 } else { 0.0 };
    }
    (num / den).clamp(0.0, 1.0)
}

/// Parse a "shoulder,base,bias" knob triple (the `(bevel)` value format, and
/// what cce-relief persists as `profile_knobs` / `edge_knobs`).
pub fn parse_bevel_knobs(s: &str) -> Option<(f32, f32, f32)> {
    let mut it = s.split(',').map(|p| p.trim().parse::<f32>());
    match (it.next(), it.next(), it.next()) {
        (Some(Ok(a)), Some(Ok(b)), Some(Ok(c))) => {
            Some((a.clamp(0.0, 1.0), b.clamp(0.0, 1.0), c.clamp(0.0, 1.0)))
        }
        _ => None,
    }
}

pub struct BevelPreview {
    knobs: (f32, f32, f32),
    just_clicked: bool,
    hovered: bool,
}

impl BevelPreview {
    pub fn new() -> Adapted<BevelPreview> {
        Adapted::new(BevelPreview {
            knobs: (0.5, 0.5, 0.5),
            just_clicked: false,
            hovered: false,
        })
    }

    /// Set the previewed profile from a knob-triple value string; anything
    /// unparsable falls back to the analytic midpoints.
    pub fn set_knobs_str(&mut self, s: &str) {
        self.knobs = parse_bevel_knobs(s).unwrap_or((0.5, 0.5, 0.5));
    }

    pub fn knobs(&self) -> (f32, f32, f32) {
        self.knobs
    }
}

impl Layout for BevelPreview {}

impl Paint for BevelPreview {
    fn color(&self) -> [f32; 4] {
        // The well bg is emitted in `paint` (hover-dependent); no base fill.
        [0.0, 0.0, 0.0, 0.0]
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        // The opening: the shared canvas well (`PaintCtx::well_floor`), its floor
        // lifted on hover (the click cue); the rim is drawn last, over the content.
        let radius = crate::layout::textbox_corner_radius();
        ctx.well_floor(rect, radius, &crate::scene::Material::pane(), self.hovered);

        // Mini cutaway: plateau band, the wall over a square-ish domain, then
        // the floor — cce-relief's `draw_section` reduced to swatch scale.
        let m = 3.0f32;
        let x_l = rect.x + m;
        let x_r = rect.x + rect.width - m;
        let y_top = rect.y + m + 2.0;
        let drop = (rect.height - 2.0 * m - 6.0).max(4.0);
        let y_bot = y_top + drop;
        let avail = x_r - x_l;
        let wall_w = drop.min(avail * 0.5);
        let plateau_w = (avail - wall_w) * 0.45;
        let (x0, x1) = (x_l + plateau_w, x_l + plateau_w + wall_w);

        let (s, b, c) = self.knobs;
        let surface_y = |x: f32| -> f32 {
            if x <= x0 {
                y_top
            } else if x <= x1 {
                y_top + bevel_ease(s, b, c, (x - x0) / wall_w) * drop
            } else {
                y_bot
            }
        };

        // The slab under the surface, in the plate material's tone.
        let mut slab = crate::color::page_low_color();
        slab = [slab[0] * 1.25 + 0.03, slab[1] * 1.25 + 0.03, slab[2] * 1.25 + 0.03, 1.0];
        let slab_bot = rect.y + rect.height - m;
        let step = 2.0f32;
        let mut x = x_l;
        while x < x_r {
            let sy = surface_y((x + step / 2.0).min(x_r));
            let w = step.min(x_r - x);
            ctx.quad(Rect { x, y: sy, width: w, height: (slab_bot - sy).max(0.0) }, slab);
            x += step;
        }

        // The surface stroke, lit per segment under the DE light azimuth —
        // the same shading the real walls answer to.
        let az = crate::layout::light_source_position();
        let (lx, ly) = (az.cos(), -az.sin());
        let base_c = [0.60f32, 0.65, 0.74];
        let n_seg = 24usize;
        let mut prev = (x_l, surface_y(x_l));
        for i in 1..=n_seg {
            let x = x_l + (x_r - x_l) * i as f32 / n_seg as f32;
            let y = surface_y(x);
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

impl Input for BevelPreview {
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
    use crate::widget::{UiContext, WidgetHost};

    #[test]
    fn ease_is_monotone_and_endpoint_exact() {
        for &(s, b, c) in &[(0.5, 0.5, 0.5), (0.0, 1.0, 0.3), (0.9, 0.1, 0.8)] {
            assert!(bevel_ease(s, b, c, 0.0).abs() < 1e-4);
            assert!((bevel_ease(s, b, c, 1.0) - 1.0).abs() < 1e-4);
            let mut last = -1.0f32;
            for i in 0..=32 {
                let h = bevel_ease(s, b, c, i as f32 / 32.0);
                assert!(h >= last - 1e-4, "monotone at ({s},{b},{c})");
                last = h;
            }
        }
        // Midpoints are the analytic smoothstep.
        let mid = bevel_ease(0.5, 0.5, 0.5, 0.5);
        assert!((mid - 0.5).abs() < 1e-4);
    }

    #[test]
    fn knob_parse_clamps_and_rejects() {
        assert_eq!(parse_bevel_knobs("0.2, 0.7, 1.5"), Some((0.2, 0.7, 1.0)));
        assert_eq!(parse_bevel_knobs("0.2,0.7"), None);
        assert_eq!(parse_bevel_knobs("junk"), None);
    }

    #[test]
    fn click_reports_once() {
        let mut ctx = UiContext::new();
        let mut p = BevelPreview::new();
        let (id, ptr) = (p.id(), p.as_ptr_mut());
        ctx.register_widget(id, ptr);
        WidgetHost::set_rect(&mut p, 0.0, 0.0, 125.0, 26.0);
        let ev = Event::MouseButton {
            button: MouseButton::Left,
            state: ElementState::Pressed,
            x: 10.0,
            y: 10.0,
            local_x: 10.0,
            local_y: 10.0,
        };
        assert!(ctx.propagate_event(&ev, id));
        assert!(p.take_click());
        assert!(!p.take_click());
    }
}
