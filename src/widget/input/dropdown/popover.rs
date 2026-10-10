//! The popover list: its open and close animation, its width, insets and padding, and its rows
//! as drawn.

use super::*;

impl Dropdown {

    /// LIVE animation progress in [0, 1], wall-clock from the last transition.
    /// 1 = fully open, 0 = fully contracted. Geometry never reads this
    /// directly — it reads the per-frame `anim_snap` (see the field docs).
    pub(super) fn anim_progress_now(&self) -> f32 {
        let Some(start) = self.anim_start else {
            return if self.open && !self.closing { 1.0 } else { 0.0 };
        };
        // Animations off: a transition in flight has already landed.
        if !crate::motion::enabled() {
            return if self.closing { 0.0 } else { 1.0 };
        }
        let el = start.elapsed().as_secs_f32() / Self::ANIM_S;
        if self.closing {
            (self.anim_from - el).clamp(0.0, 1.0)
        } else {
            (self.anim_from + el).clamp(0.0, 1.0)
        }
    }

    pub(super) fn begin_open(&mut self) {
        self.anim_from = self.anim_progress_now();
        self.anim_start = Some(web_time::Instant::now());
        self.open = true;
        self.closing = false;
        self.anim_snap = self.anim_progress_now();
    }

    pub(super) fn begin_close(&mut self) {
        if !self.open || self.closing {
            return;
        }
        self.anim_from = self.anim_progress_now();
        self.anim_start = Some(web_time::Instant::now());
        self.closing = true;
        self.anim_snap = self.anim_progress_now();
    }

    /// Fold finished animations back into settled state and refresh the
    /// per-frame progress snapshot (draw paths are `&self`, so this runs from
    /// the mutation entry points: `tick` and `on_event`). Also heals an
    /// externally forced `open = false` (a direct field write skips the
    /// animation; reset so the next open still animates).
    pub(super) fn settle_anim(&mut self) {
        if self.closing {
            if self.anim_progress_now() <= 0.0 {
                self.closing = false;
                self.open = false;
                self.anim_start = None;
                self.hovered_item = None;
            }
        } else if self.open {
            if self.anim_progress_now() >= 1.0 {
                self.anim_start = None;
            }
        } else {
            self.anim_start = None;
        }
        self.anim_snap = self.anim_progress_now();
    }

    /// Land an in-flight open or close instantly (tests can't wait out the
    /// wall clock).
    #[cfg(test)]
    pub(super) fn land_anim_for_test(&mut self) {
        self.anim_start = Some(web_time::Instant::now() - std::time::Duration::from_secs(1));
        self.settle_anim();
    }

    pub(super) fn popover_width(&self, content: Rect) -> f32 {
        content.width.max(self.content_width())
    }

    /// The relief wall the open plate eats out of its own OUTER edges. The
    /// menu's surface is carved exactly like the closed trigger's
    /// (`carve_inside` at `depth`, then a trough straddling the carved edge by
    /// ±depth/2), so the outermost `depth` of the box is valley rather than
    /// face. A row laid flush against that edge therefore has its bottom
    /// padding — and any descender sitting in it — painted over by the wall,
    /// which is what cut the last option of every menu in half. The flat path
    /// draws a 1px outline instead and costs a row only its outermost pixel.
    pub(super) fn plate_inset(&self, trigger_h: f32) -> f32 {
        if self.raised() {
            crate::layout::bevel_width().min(trigger_h * 0.2)
        } else {
            1.0
        }
    }

    /// What [`Self::popover_geom`] reserves above and below the row strip:
    /// [`Self::plate_inset`] on whichever menu edges are OUTER edges of the
    /// open surface. A downward menu meets the trigger band along its top and
    /// owns only its bottom edge; an upward one is the other way round; a menu
    /// that replaces the trigger owns both.
    pub(super) fn popover_pads(&self, content: Rect) -> (f32, f32) {
        let inset = self.plate_inset(content.height);
        let anchor = self.popover_anchor.unwrap_or(content);
        let base_y = anchor.y - self.label_top();
        let open_upward = self.open_upward.unwrap_or(base_y > 400.0);
        if self.menu_replaces_trigger {
            (inset, inset)
        } else if open_upward {
            (inset, 0.0)
        } else {
            (0.0, inset)
        }
    }

    /// Where the first option row sits: the menu box's top edge, past the
    /// relief wall when that edge is an outer one. Rows run from here at
    /// [`Self::ROW_H`] — the ONE origin the paint and the hit test share.
    pub(super) fn rows_top(&self, content: Rect) -> f32 {
        let (_, ry, _, _) = self.popover_geom(content);
        ry + self.popover_pads(content).0
    }

    /// Which option lies at `py`, or None for the reserved wall at the menu's
    /// outer edge (and for anything past the last row).
    pub(super) fn row_at(&self, content: Rect, py: f32) -> Option<usize> {
        let rel = py - self.rows_top(content);
        if rel < 0.0 {
            return None;
        }
        let idx = (rel / Self::ROW_H) as usize;
        (idx < self.options.len()).then_some(idx)
    }

    /// The ONE continuous surface drawn while open: the trigger band unioned
    /// with the revealed menu area — the status-interface treatment, where the
    /// module box literally grows into its menu instead of spawning a detached
    /// popover plate.
    pub(super) fn unified_geom_drawn(&self, content: Rect) -> (f32, f32, f32, f32) {
        let (ax, ay, aw, ah) = self.popover_geom_drawn(content);
        if self.menu_replaces_trigger {
            // No band: the revealed menu IS the whole open surface.
            return (ax, ay, aw, ah);
        }
        let (tx, ty) = (content.x, content.y);
        let (tw, th) = (content.width, content.height);
        let x0 = tx.min(ax);
        let y0 = ty.min(ay);
        let x1 = (tx + tw).max(ax + aw);
        let y1 = (ty + th).max(ay + ah);
        (x0, y0, x1 - x0, y1 - y0)
    }

    /// The menu box revealed this frame: the full geometry with the height
    /// revealed — and the width grown out of the trigger — by the eased
    /// progress (cubic-out, the status-interface module-menu curve). Rows keep
    /// their final positions and slide into view under the traveling edge; an
    /// upward popover anchors its bottom edge to the trigger instead.
    ///
    /// The width grows out of the TRIGGER'S OWN SPAN, both edges travelling:
    /// a trigger nested in a wider field (a [`Self::popover_anchor`] — the
    /// textpick picker at the right end of its text box) opens as a plate the
    /// button's width under the button and widens out to the field. Growing
    /// from the anchor's left edge put a button-wide sliver under the text
    /// instead, and the union with the button spanned the whole field from
    /// the first frame. A trigger at its menu's left edge — every unanchored
    /// one — grows rightward exactly as it always did.
    pub(super) fn popover_geom_drawn(&self, content: Rect) -> (f32, f32, f32, f32) {
        let (rx, ry, rw, rh) = self.popover_geom(content);
        let a = self.anim_snap;
        if a >= 1.0 {
            return (rx, ry, rw, rh);
        }
        let t = 1.0 - (1.0 - a) * (1.0 - a) * (1.0 - a);
        let base_y = content.y - self.label_top();
        let open_upward = self.open_upward.unwrap_or(base_y > 400.0);
        let (r0, r1) = (rx, rx + rw);
        let s0 = content.x.clamp(r0, r1);
        let s1 = (content.x + content.width).clamp(s0, r1);
        let x0 = s0 + (r0 - s0) * t;
        let x1 = s1 + (r1 - s1) * t;
        let ah = rh * t;
        let ay = if open_upward { ry + rh - ah } else { ry };
        (x0, ay, x1 - x0, ah)
    }

    /// Popover geometry against the laid-out content rect — the legacy `get_popover_geom`,
    /// with the base-rect reads rewritten in content-rect terms (`base.y + base.h` ⇒
    /// `content.y + content.height`, `base.y + label_offset` ⇒ `content.y`).
    ///
    /// The box is the row strip PLUS `Self::popover_pads` — it is a plate,
    /// and its outer edges are relief wall, not face. Rows start at
    /// `Self::rows_top`, never at `ry`.
    pub fn popover_geom(&self, content: Rect) -> (f32, f32, f32, f32) {
        let (pad_top, pad_bottom) = self.popover_pads(content);
        let content = self.popover_anchor.unwrap_or(content);
        let rw = self.popover_width(content);
        let rh = self.options.len() as f32 * Self::ROW_H + pad_top + pad_bottom;

        let base_y = content.y - self.label_top();
        let open_upward = self.open_upward.unwrap_or(base_y > 400.0);
        let mut rx = content.x;
        let mut ry = if self.menu_replaces_trigger {
            // The menu takes the trigger's slot: flush with its bottom edge
            // (upward) or its top edge (downward) rather than stacked past it.
            if open_upward { content.y + content.height - rh } else { content.y }
        } else if open_upward {
            content.y - rh
        } else {
            content.y + content.height
        };

        let is_ramp = self.parent_snapshot.is_some_and(|s| s.is_ramp);

        if is_ramp {
            if let Some(snap) = self.parent_snapshot {
                let (px, py, pw_parent, ph_parent) = snap.rect;
                if pw_parent > 0.0 && ph_parent > 0.0 {
                    let dy_down = content.y + content.height;
                    let dy_up = content.y - rh;

                    if self.open_upward.is_none() {
                        if dy_down + rh > py + ph_parent && dy_up >= py {
                            ry = dy_up;
                        } else if dy_up < py && dy_down + rh <= py + ph_parent {
                            ry = dy_down;
                        }
                    }

                    // Clamp X to parent borders
                    if rx < px {
                        rx = px;
                    }
                    if rx + rw > px + pw_parent {
                        rx = px + pw_parent - rw;
                    }

                    // Clamp Y to parent borders
                    if ry < py {
                        ry = py;
                    }
                    if ry + rh > py + ph_parent {
                        ry = py + ph_parent - rh;
                    }
                }
            }
        }

        (rx, ry, rw, rh)
    }
}
