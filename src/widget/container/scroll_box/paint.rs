//! Painting a scroll box's bar: the carved relief scrollbar (shared with the text box's), the
//! centred pills a sink-behind bar draws behind and in front of the plate, and the flat quads.

use super::*;

/// Shared relief-scrollbar painter: the track a carved groove
/// ([`crate::scene::paint::PaintCtx::recess`]), the thumb a raised rounded
/// plate riding in it ([`crate::scene::paint::PaintCtx::bevel`], configured
/// thumb color) — the DE highlight/shadow bevel treatment. `viewport` is the
/// scrolling area's box; the bar hugs its right edge. No-op while the content
/// fits.
pub fn paint_relief_scrollbar(
    pc: &mut crate::scene::paint::PaintCtx,
    viewport: crate::scene::layout::Rect,
    content_h: f32,
    scroll_y: f32,
) {
    if content_h <= viewport.height {
        return;
    }
    let sb_w = crate::layout::scrollbar_width();
    let sb_x = viewport.x + viewport.width - sb_w - 4.0;
    let sb_track_h = viewport.height - 8.0;
    let sb_track_y = viewport.y + 4.0;

    let visible_ratio = viewport.height / content_h;
    let thumb_h = if sb_track_h <= 20.0 {
        sb_track_h
    } else {
        (sb_track_h * visible_ratio).clamp(20.0, sb_track_h)
    };
    let max_scroll = (content_h - viewport.height).max(0.0);
    let scroll_ratio = if max_scroll > 0.0 { scroll_y / max_scroll } else { 0.0 };
    let thumb_y = sb_track_y + scroll_ratio * (sb_track_h - thumb_h);

    // Pill radii; the roll width scales to the bar (the DE bevel width would
    // swallow a 14px-wide thumb whole).
    let r = sb_w * 0.5;
    let depth = crate::layout::bevel_width().min(sb_w * 0.35);
    let radii = (r, r, r, r);
    use crate::scene::layout::Rect;
    let (track, track_radii) =
        crate::layout::carve_inside(Rect { x: sb_x, y: sb_track_y, width: sb_w, height: sb_track_h }, radii, depth);
    pc.recess(track, track_radii, depth);
    pc.bevel(
        Rect { x: sb_x, y: thumb_y, width: sb_w, height: thumb_h },
        radii,
        &crate::scene::material::Material::from_fill(crate::color::scrollbar_thumb_color()),
        depth,
    );
}

impl ScrollBox {
    /// The bar as flat pills in the track and thumb colours, scaled to
    /// `alpha` — what a sink-behind host draws twice: the idle copy at 1
    /// BEFORE its plate, and the fore copy at [`Self::scrollbar_fade`]
    /// after its content. (The relief bar is not used for a sinking one:
    /// shader-lit relief does not fade with a vertex alpha.)
    pub fn paint_scrollbar_pills(&self, pc: &mut crate::scene::paint::PaintCtx, alpha: f32) {
        let a = alpha.clamp(0.0, 1.0);
        let Some((sb_x, track_y, sb_w, track_h, thumb_y, thumb_h)) = self.bar_geom() else { return };
        if a <= 0.001 {
            return;
        }
        let dim = |mut c: [f32; 4]| {
            c[3] *= a;
            c
        };
        use crate::scene::layout::Rect;
        let all = (true, true, true, true);
        pc.rounded_rect(Rect { x: sb_x, y: track_y, width: sb_w, height: track_h }, sb_w.min(track_h) * 0.5, all, dim(crate::color::scrollbar_track_color()));
        pc.rounded_rect(Rect { x: sb_x, y: thumb_y, width: sb_w, height: thumb_h }, sb_w.min(thumb_h) * 0.5, all, dim(crate::color::scrollbar_thumb_color()));
    }

    pub fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();

        // Background
        if self.show_background {
            quads.push((self.base.x, self.base.y, self.base.w, self.base.h, crate::color::list_bg_color()));
        }



        // Scrollbar
        if let Some((sb_x, track_y, sb_w, track_h, thumb_y, thumb_h)) = self.bar_geom() {
            quads.push((sb_x, track_y, sb_w, track_h, crate::color::scrollbar_track_color()));
            quads.push((sb_x, thumb_y, sb_w, thumb_h, crate::color::scrollbar_thumb_color()));
        }

        quads
    }

    /// The scrollbar in the DE's relief language — see
    /// [`paint_relief_scrollbar`]. The flat-quad view stays available through
    /// [`extra_quads`](Self::extra_quads) for legacy paths.
    pub fn paint_scrollbar_relief(&self, pc: &mut crate::scene::paint::PaintCtx) {
        paint_relief_scrollbar(
            pc,
            crate::scene::layout::Rect {
                x: self.base.x,
                y: self.viewport_y,
                width: self.base.w,
                height: self.viewport_h,
            },
            self.content_h,
            self.scroll_y,
        );
    }
}
