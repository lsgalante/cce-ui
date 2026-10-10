//! Drawing the region's frame and scrollbars: as prims on a `RenderTarget` (the frame, the idle and
//! fore copies of the bars), or as flat quads for a host on the tuple pipeline.

use super::*;

impl ScrollRegion {
    /// The legacy frame, single-drawn: 1px rounded border (focus/hover tinted, from
    /// `List::solid_border`), inset rounded bg, then the scrollbar track and thumb ON TOP.
    ///
    /// A [`Self::sink_behind`] region draws no FORE copy here: the host draws
    /// its rows after this call, and a fore bar drawn before them would sit
    /// under them. Call [`Self::push_scrollbar_fore`] once the rows are down.
    /// A framed sink region draws its behind copy here, under the bg fill; a
    /// frameless one leaves it to the host, under the host's plate.
    pub fn push_prims(&self, pc: &mut dyn crate::layout::RenderTarget) {
        if self.draw_frame {
            let radius = crate::layout::list_corner_radius();
            let border_color = if self.focused {
                [0.30, 0.50, 0.32, 1.0]
            } else if self.hovered {
                [0.25, 0.25, 0.35, 1.0]
            } else {
                [0.18, 0.18, 0.24, 1.0]
            };
            let all = (true, true, true, true);
            pc.rect_with_radius_corners(border_color, self.x, self.y, self.w, self.h, radius, all);
            // A sink-behind bar's idle copy draws here, UNDER the translucent
            // bg fill (list_bg_color's alpha is 0.3): it shows through dimly,
            // sunk into the list plate — the designer parameter-pane look,
            // self-contained for framed regions. Always, raised or not: the
            // fore copy fades in OVER it, so dropping it at the latch would
            // blink the bar out under a fore copy still at a low alpha.
            if self.sink_behind {
                self.push_scrollbar_prims(pc);
            }
            pc.rect_with_radius_corners(
                crate::color::list_bg_color(),
                self.x + 1.0,
                self.y + 1.0,
                self.w - 2.0,
                self.h - 2.0,
                (radius - 1.0).max(0.0),
                all,
            );
        }
        // A plain region's bar rides on top, always. A sink-behind region's
        // fore copy is the host's to draw after its rows
        // (`push_scrollbar_fore`); a FRAMELESS one draws no idle copy here
        // either — its rows sit directly on the host's plate, so the host
        // owns the under-plate emission via `push_scrollbar_prims`.
        if !self.sink_behind {
            self.push_scrollbar_prims(pc);
        }
    }

    /// A sink-behind region's FORE copy, at the activity's fade: draw it
    /// after the rows. It fades rather than flips and keeps drawing all the
    /// way out — gating it on `scrollbar_raised` would cut the fade off at
    /// the latch. Nothing for a plain region, whose bar `push_prims` drew.
    pub fn push_scrollbar_fore(&self, pc: &mut dyn crate::layout::RenderTarget) {
        if self.sink_behind {
            self.push_scrollbar_prims_alpha(pc, self.activity.fade());
        }
    }

    /// The pill scrollbars alone (track + thumb, both axes), drawn wherever the
    /// host calls it — a frameless sink-behind host draws its idle copy with
    /// this, every frame, BEFORE its plate.
    pub fn push_scrollbar_prims(&self, pc: &mut dyn crate::layout::RenderTarget) {
        self.push_scrollbar_prims_alpha(pc, 1.0);
    }

    /// [`Self::push_scrollbar_prims`] with the track and thumb scaled to
    /// `alpha` — what a host draws the FORE copy with while it fades in and
    /// out. The copy that idles behind the plate is drawn at full alpha; the
    /// plate over it is what dims and frosts it.
    pub fn push_scrollbar_prims_alpha(&self, pc: &mut dyn crate::layout::RenderTarget, alpha: f32) {
        let a = alpha.clamp(0.0, 1.0);
        if a <= 0.001 {
            return;
        }
        let dim = |mut c: [f32; 4]| {
            c[3] *= a;
            c
        };
        // Track and thumb are pills — half-width radius (the designer look).
        // Both tracks before either thumb, so where centred bars cross the
        // horizontal track does not cover the vertical thumb.
        let all = (true, true, true, true);
        let track = dim(crate::color::scrollbar_track_color());
        let thumb = dim(crate::color::scrollbar_thumb_color());
        let v = (self.content_h > self.viewport_h).then(|| self.scrollbar_geom());
        let h = self.h_scroll_active().then(|| self.h_scrollbar_geom());
        if let Some((sb_x, track_y, sb_w, track_h, _, _)) = v {
            pc.rect_with_radius_corners(track, sb_x, track_y, sb_w, track_h, sb_w.min(track_h) * 0.5, all);
        }
        if let Some((track_x, sb_y, track_w, sb_h, _, _)) = h {
            pc.rect_with_radius_corners(track, track_x, sb_y, track_w, sb_h, sb_h.min(track_w) * 0.5, all);
        }
        if let Some((sb_x, _, sb_w, _, thumb_y, thumb_h)) = v {
            pc.rect_with_radius_corners(thumb, sb_x, thumb_y, sb_w, thumb_h, sb_w.min(thumb_h) * 0.5, all);
        }
        if let Some((_, sb_y, _, sb_h, thumb_x, thumb_w)) = h {
            pc.rect_with_radius_corners(thumb, thumb_x, sb_y, thumb_w, sb_h, sb_h.min(thumb_w) * 0.5, all);
        }
    }

    /// Flat background fill for hosts on the tuple pipeline. Flat squares on a
    /// hard flip, the vertical bar only: a sink-behind host wants the prim
    /// path (pills, both axes, the fade), and every one in the DE is on it. The scrollbar is
    /// split into [`Self::push_scrollbar_quads`] so the host can emit it AFTER
    /// the rows — drawn together, the rows paint over the thumb and it peeks
    /// through the inter-row gaps as dotted segments.
    ///
    /// For a sink-behind region, a sunk bar is emitted here FIRST, under the
    /// translucent bg fill (alpha 0.3), so it shows through dimly — and
    /// [`Self::push_scrollbar_quads`] goes quiet. The host's existing
    /// bg → rows → scrollbar order needs no change to adopt the treatment.
    pub fn push_quads(&self, quads: &mut Vec<(f32, f32, f32, f32, [f32; 4])>) {
        if self.sink_behind && !self.activity.raised() {
            self.push_scrollbar_quads_always(quads);
        }
        quads.push((self.x, self.y, self.w, self.h, crate::color::list_bg_color()));
    }

    /// Scrollbar track + thumb when the content overflows; emit after the rows.
    /// For a sink-behind region this is the RAISED layer only — while sunk the
    /// bar was already emitted under the bg by [`Self::push_quads`].
    pub fn push_scrollbar_quads(&self, quads: &mut Vec<(f32, f32, f32, f32, [f32; 4])>) {
        if self.scrollbar_raised() {
            self.push_scrollbar_quads_always(quads);
        }
    }

    pub(super) fn push_scrollbar_quads_always(&self, quads: &mut Vec<(f32, f32, f32, f32, [f32; 4])>) {
        if self.content_h > self.viewport_h {
            let (sb_x, track_y, sb_w, track_h, thumb_y, thumb_h) = self.scrollbar_geom();
            quads.push((sb_x, track_y, sb_w, track_h, crate::color::scrollbar_track_color()));
            quads.push((sb_x, thumb_y, sb_w, thumb_h, crate::color::scrollbar_thumb_color()));
        }
    }
}
