//! The float ramp's geometry: the strip reserved for its controls, the plot and its key pegs
//! (their size, paint order and hit test), where a dragged key lands (the soft wall at its
//! neighbours, resettling the order), the key pad written back to the selected key, and
//! arranging the controls.

use super::*;

impl Ramp {
    /// The one spacing value the whole control strip uses — matching the
    /// visible gap between the graph opening and the window's top edge (the
    /// widget's 10px graph inset plus the host plate's padding).
    const STRIP_GAP: f32 = 18.0;

    /// The key pad's square well side.
    const PAD_SIDE: f32 = 64.0;

    /// The graph opening's height a Ramp declares (`intrinsic_size`): the drag-in
    /// canvases' 120 (a Slider2D's pad, the Trackpad). Given more, the graph takes it.
    pub(super) const GRAPH_H: f32 = 120.0;

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

    /// A key peg's radius on THIS plot — the disc plate's outer edge, and the flat selection
    /// ring's ([`peg_drawn_r`](Self::peg_drawn_r) is what each peg is drawn to). Never past
    /// the plot inset, so a key at the plot's edge reaches the opening's wall and no further;
    /// on a short plot (an inline ramp a control high) under half the plot's height, so the
    /// pegs stay a few px across instead of swallowing the curve.
    pub(super) fn key_peg_r(&self) -> f32 {
        let plot = self.plot_rect();
        Self::PLOT_INSET.min((plot.height * 0.45).max(4.0) + 2.5)
    }

    /// Key `idx`'s centre on screen.
    pub(super) fn key_centre(&self, idx: usize) -> (f32, f32) {
        let plot = self.plot_rect();
        let key = &self.keys[idx];
        (plot.x + key.pos * plot.width, plot.y + plot.height * (1.0 - key.value))
    }

    /// The pegs in paint order, bottom first: by index, the selected one last (on top).
    pub(super) fn peg_order(&self) -> impl Iterator<Item = usize> + '_ {
        let sel = self.selected_key_idx.filter(|&i| i < self.keys.len());
        (0..self.keys.len()).filter(move |&i| Some(i) != sel).chain(sel)
    }

    /// The key whose peg is under `(px, py)`, each peg within its drawn radius
    /// ([`peg_drawn_r`](Self::peg_drawn_r)), topmost first — the reverse of
    /// [`peg_order`](Self::peg_order), so where pegs overlap the press takes the one drawn on
    /// top. The press and the hover-scroll latch both ask here.
    pub(super) fn key_at(&self, px: f32, py: f32) -> Option<usize> {
        let order: Vec<usize> = self.peg_order().collect();
        order.into_iter().rev().find(|&idx| {
            let (cx, cy) = self.key_centre(idx);
            let (dx, dy) = (px - cx, py - cy);
            let r = self.peg_drawn_r(idx);
            dx * dx + dy * dy <= r * r
        })
    }

    /// Inner margin between the graph opening's walls and the plotted 0..1
    /// domain, so the 0 and 1 gridlines (and their axis numbers) sit visibly
    /// inside the opening instead of on the walls.
    pub(super) const PLOT_INSET: f32 = 22.0;

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
    /// the crossing completes and tracking is free again. Always in 0..1.
    ///
    /// `origin` is where the key was when the drag began: a key grabbed inside a
    /// neighbor's zone has its zone start where it stood, not behind it — from the
    /// zone's far edge, the first nudge toward the neighbor moved it AWAY (a key at
    /// 0.50 by a neighbor at 0.52, nudged to 0.505, jumped back to 0.484).
    pub(super) fn resisted_pos(&self, idx: usize, t_raw: f32, origin: f32) -> f32 {
        let cur = self.keys[idx].pos;
        let t = if t_raw > cur {
            match self.keys.get(idx + 1) {
                Some(next) => Self::soft_wall(t_raw, next.pos, 1.0, origin),
                None => t_raw,
            }
        } else if idx > 0 {
            Self::soft_wall(t_raw, self.keys[idx - 1].pos, -1.0, origin)
        } else {
            t_raw
        };
        t.clamp(0.0, 1.0)
    }

    /// Restore sort order after `keys[i]` changed position, by adjacent
    /// swaps, and return the key's new index. Exact identity tracking — a
    /// re-sort that finds the key again by its position misidentifies the
    /// selection when the dragged key sits within ε of the key it is passing
    /// (leftward crossings flipped the selection onto the passed key).
    /// `ColorRamp::resettle_key` is the same, for its keys.
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
    ///
    /// The zone is `RESIST_ZONE` wide, or narrower when `origin` (where the key
    /// started) is already inside it on the approach side: then it starts there,
    /// so the key never maps behind its own start.
    pub(super) fn soft_wall(t_raw: f32, wall: f32, s: f32, origin: f32) -> f32 {
        let mut entry = wall - s * Self::RESIST_ZONE;
        if s * (origin - entry) > 0.0 && s * (wall - origin) > 0.0 {
            entry = origin;
        }
        let zone = s * (wall - entry);
        let depth = s * (t_raw - entry);
        let full = zone + Self::RESIST_BREAK;
        if depth <= 0.0 || depth >= full {
            return t_raw; // outside the zone, or broken through
        }
        if zone <= 1e-4 {
            return wall; // started against the wall: held there until the break
        }
        let k = full / zone;
        let g = 1.0 - (1.0 - depth / full).powf(k);
        entry + s * zone * g
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
