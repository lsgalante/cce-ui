//! The float ramp's geometry: the strip reserved for its controls, the plot and its key rings,
//! the rolled rim, where a dragged key lands (the soft wall at its neighbours, resettling the
//! order), the key pad written back to the selected key, and arranging the controls.

use super::*;

impl Ramp {
    /// The one spacing value the whole control strip uses — matching the
    /// visible gap between the graph opening and the window's top edge (the
    /// widget's 10px graph inset plus the host plate's padding).
    const STRIP_GAP: f32 = 18.0;

    /// The key pad's square well side.
    const PAD_SIDE: f32 = 64.0;

    /// Vertical reserve under the curve area — the strip stack at the
    /// uniform STRIP_GAP rhythm (labeled dropdown row, labeled pad row),
    /// closed by a bottom margin sized so the VISIBLE bottom gap (widget
    /// margin + host plate padding, ~8) lands on STRIP_GAP as well.
    pub(super) fn strip_reserve() -> f32 {
        let strip = Self::label_strip();
        10.0 + Self::STRIP_GAP + strip + 22.0
            + Self::STRIP_GAP + strip + Self::PAD_SIDE
            + 10.0
    }

    /// Key peg ring stroke centerline radius (the 2px stroke spans ±1px).
    /// Paint and the grab hit-test share it: a press anywhere inside a ring
    /// lands on that key.
    const KEY_RING_R: f32 = 26.0;

    /// The key ring radius on THIS plot: the editor's full ring, shrunk so a
    /// peg never outgrows the plot it sits in (an inline ramp a control high
    /// draws pegs a few px across, not 26px discs swallowing the curve).
    pub(super) fn key_ring_r(&self) -> f32 {
        let plot = self.plot_rect();
        Self::KEY_RING_R.min((plot.height * 0.45).max(4.0))
    }

    /// Inner margin between the graph opening's walls and the plotted 0..1
    /// domain, so the 0 and 1 gridlines (and their axis numbers) sit visibly
    /// inside the opening instead of on the walls.
    const PLOT_INSET: f32 = 22.0;

    /// The plot rect: where the ramp's 0..1 × 0..1 domain maps on screen —
    /// the graph opening inset by [`PLOT_INSET`](Self::PLOT_INSET). Every
    /// t/value ↔ pixel mapping (paint and input alike) goes through this.
    pub(super) fn plot_rect(&self) -> Rect {
        let gh = self.graph_h();
        Rect {
            x: self.base.x + 10.0 + Self::PLOT_INSET,
            y: self.base.y + 10.0 + Self::PLOT_INSET,
            width: (self.base.w - 20.0 - 2.0 * Self::PLOT_INSET).max(1.0),
            height: (gh - 2.0 * Self::PLOT_INSET).max(1.0),
        }
    }

    /// Neighbor resistance (drag), in track units: the soft wall starts
    /// RESIST_ZONE before a neighbor's position, and pushing the cursor
    /// RESIST_BREAK past the neighbor breaks through.
    const RESIST_ZONE: f32 = 0.10;
    const RESIST_BREAK: f32 = 0.16;

    /// Where a drag whose cursor sits at `t_raw` actually puts key `idx`:
    /// 1:1 tracking until the cursor enters a neighbor's resistance zone,
    /// then the key compresses toward the neighbor with growing resistance
    /// (slope 1 at the zone edge, flattening at the wall), and once the
    /// cursor overshoots the neighbor by RESIST_BREAK the key pops through —
    /// the crossing completes and tracking is free again.
    pub(super) fn resisted_pos(&self, idx: usize, t_raw: f32) -> f32 {
        let cur = self.keys[idx].pos;
        if t_raw > cur {
            if let Some(next) = self.keys.get(idx + 1) {
                return Self::soft_wall(t_raw, next.pos, 1.0);
            }
        } else if idx > 0 {
            return Self::soft_wall(t_raw, self.keys[idx - 1].pos, -1.0);
        }
        t_raw
    }

    /// Restore sort order after `keys[i]` changed position, by adjacent
    /// swaps, and return the key's new index. Exact identity tracking —
    /// `sort_keys`' float-pos re-match misidentifies the selection when the
    /// dragged key sits within ε of the key it is passing (leftward
    /// crossings flipped the selection onto the passed key).
    pub(super) fn resettle_key(&mut self, mut i: usize) -> usize {
        while i + 1 < self.keys.len() && self.keys[i].pos > self.keys[i + 1].pos {
            self.keys.swap(i, i + 1);
            i += 1;
        }
        while i > 0 && self.keys[i].pos < self.keys[i - 1].pos {
            self.keys.swap(i, i - 1);
            i -= 1;
        }
        i
    }

    /// A key's rolled edge: the disc's own surface curving away at the
    /// perimeter — NOT a separate border. Each sub-arc blends radially from
    /// the surface color at the band's inner edge (continuing the flat top
    /// seamlessly), through a half-rolled tint, to the silhouette — which
    /// leans toward the light on the lit side and falls into shadow opposite,
    /// and runs denser than the top the way a glass edge reads. `r` is the
    /// outer-edge radius; `base`/`top_alpha` are the disc's surface color.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn rolled_rim_arc(
        pc: &mut PaintCtx,
        cx: f32,
        cy: f32,
        r: f32,
        thickness: f32,
        start: f32,
        end: f32,
        az: f32,
        base: [f32; 3],
        top_alpha: f32,
    ) {
        let sweep = end - start;
        let steps = ((sweep.abs() / 0.18).ceil() as usize).max(1);
        let tint = |sv: f32, k: f32| -> [f32; 3] {
            [
                (base[0] + k * sv).clamp(0.0, 1.0),
                (base[1] + k * sv).clamp(0.0, 1.0),
                (base[2] + k * sv).clamp(0.0, 1.0),
            ]
        };
        for i in 0..steps {
            let a0 = start + sweep * i as f32 / steps as f32;
            let a1 = start + sweep * (i + 1) as f32 / steps as f32;
            let sv = ((a0 + a1) / 2.0 + az).cos();
            let mid = tint(sv, 0.20);
            let edge = tint(sv, 0.38);
            let mid_a = (top_alpha + 0.78) / 2.0;
            pc.arc_shaded(
                cx,
                cy,
                r,
                thickness,
                a0,
                a1,
                [base[0], base[1], base[2], top_alpha],
                [mid[0], mid[1], mid[2], mid_a],
                [edge[0], edge[1], edge[2], 0.78],
            );
        }
    }

    /// Apply the key pad's two axes to the selected key: x is the key's
    /// track position (order restored by adjacent swaps), y its value.
    pub(super) fn apply_pad_to_selected(&mut self) {
        let Some(idx) = self.selected_key_idx else { return };
        self.keys[idx].pos = self.key_pad.inner().value_x();
        self.keys[idx].value = self.key_pad.inner().value_y();
        let settled = self.resettle_key(idx);
        self.selected_key_idx = Some(settled);
        self.sync_preset();
        self.just_changed = true;
    }

    /// One soft wall at `wall`, approached along direction `s` (±1). Maps the
    /// cursor's depth into the zone onto the zone's width with an ease that
    /// reaches the wall exactly at breakthrough depth — continuous at the
    /// zone edge, asymptotically stiff at the wall, then a `RESIST_BREAK`
    /// pop as the mapping hands back to 1:1 tracking.
    pub(super) fn soft_wall(t_raw: f32, wall: f32, s: f32) -> f32 {
        let entry = wall - s * Self::RESIST_ZONE;
        let depth = s * (t_raw - entry);
        let full = Self::RESIST_ZONE + Self::RESIST_BREAK;
        if depth <= 0.0 || depth >= full {
            return t_raw; // outside the zone, or broken through
        }
        let k = full / Self::RESIST_ZONE;
        let g = 1.0 - (1.0 - depth / full).powf(k);
        entry + s * Self::RESIST_ZONE * g
    }

    /// The curve area's height: the widget minus the control strip — or,
    /// with the controls collapsed (context-menu toggle), minus just the
    /// top/bottom insets, the graph claiming the strip's space.
    pub(super) fn graph_h(&self) -> f32 {
        if self.controls_collapsed {
            (self.base.h - 20.0).max(30.0)
        } else {
            (self.base.h - Self::strip_reserve()).max(30.0)
        }
    }

    /// The detached-label strip height the labeled dropdowns carry
    /// (`Widget::label_offset`'s formula).
    pub fn label_strip() -> f32 {
        crate::layout::control_label_strip()
    }

    /// Lay out the control strip under the curve area. One rhythm: the label
    /// tabs sit STRIP_GAP under the graph and every other gap shares the
    /// same rhythm, all columns one shared height on one shared baseline. The labeled dropdowns
    /// get rects that INCLUDE their label strip (the adapter carves it off the
    /// content); the unlabeled columns get the content band only. The preset
    /// column takes the wider share — its options are the strip's longest
    /// strings and used to clip.
    pub(super) fn arrange_fields(&mut self) {
        let (x, y, w, h) = (self.base.x, self.base.y, self.base.w, self.base.h);
        if self.controls_collapsed {
            self.preset_dropdown.set_rect(-1000.0, -1000.0, 0.0, 0.0);
            self.line_type_dropdown.set_rect(-1000.0, -1000.0, 0.0, 0.0);
            self.key_pad.set_rect(-1000.0, -1000.0, 0.0, 0.0);
            self.del_button.set_rect(-1000.0, -1000.0, 0.0, 0.0);
            let _ = (x, y, w, h);
            return;
        }
        let gh = self.graph_h();
        let graph_bottom = y + 10.0 + gh;
        let ctrl_h = 22.0;
        let strip = Self::label_strip();
        let gap = Self::STRIP_GAP;
        // One rhythm: every gap in the strip — graph to label tab, row to
        // row, columns, pad to button — is STRIP_GAP.
        let ctrl_y = graph_bottom + gap + strip;
        let (dd_y, dd_h) = (ctrl_y - strip, ctrl_h + strip);
        let track_x = x + 10.0;
        let track_w = w - 20.0;

        if self.selected_key_idx.is_some() {
            // Selected: the dropdowns keep their full-width row, and a second
            // row below carries the square key pad (pos × value) with the
            // delete button beside it, centered on the pad's well.
            let pad_side = Self::PAD_SIDE;
            let del_w: f32 = if self.del_button.inner().has_icon() { ctrl_h } else { 64.0 };
            let pre_w = ((track_w - gap) * 0.58).max(40.0);
            let line_w = (track_w - gap - pre_w).max(40.0);
            self.preset_dropdown.set_rect(track_x, dd_y, pre_w, dd_h);
            self.line_type_dropdown.set_rect(track_x + pre_w + gap, dd_y, line_w, dd_h);
            let row2_y = ctrl_y + ctrl_h + gap;
            self.key_pad.set_rect(track_x, row2_y, pad_side, pad_side + strip);
            self.del_button.set_rect(
                track_x + pad_side + gap,
                row2_y + strip + (pad_side - ctrl_h) / 2.0,
                del_w,
                ctrl_h,
            );
        } else {
            // Two columns, preset the wider share.
            let pre_w = ((track_w - gap) * 0.58).max(40.0);
            let line_w = (track_w - gap - pre_w).max(40.0);
            self.preset_dropdown.set_rect(track_x, dd_y, pre_w, dd_h);
            self.line_type_dropdown.set_rect(track_x + pre_w + gap, dd_y, line_w, dd_h);
            self.key_pad.set_rect(-1000.0, -1000.0, 0.0, 0.0);
            self.del_button.set_rect(-1000.0, -1000.0, 0.0, 0.0);
        }
    }
}
