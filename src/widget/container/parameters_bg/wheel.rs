//! The wheel: which control owns a gesture (the one it began on, latched until it ends), turning
//! that control, or scrolling the pane.

use super::*;

impl ParametersBg {
    /// A wheel notch or finger scroll: to the value control that owns the gesture, else the
    /// pane's own scroll. An opaque pane swallows every wheel over it.
    pub(super) fn on_wheel(&mut self, delta: &MouseScrollDelta, px: &f32, py: &f32, ectx: &mut EventCtx) -> bool {
        let self_id = ectx.id;
        if !self.visible {
            return false;
        }
        let (px, py) = (*px, *py);
        let Some(ui) = ectx.ui.as_deref_mut() else {
            return false;
        };
        let taken = match self.wheel_owner(px, py, ui, self_id) {
            Some(i) => self.wheel_control(i, delta, px, py, ui),
            None => false,
        };
        self.wheel_pane(delta, px, py, ui, self_id, taken)
    }

    /// The row whose value control owns this wheel event, if any. A gesture belongs to what it
    /// begins on: a value control — a spinbox by its row, a slider and each row of a float3 by
    /// the band's own halo (`Slider::scroll_hit`), a float3's trackball — takes a gesture that
    /// begins on it, trackpad included, and keeps it until the gesture ends. So the LATCHED
    /// control is asked first, and only a gesture nobody owns is open to the control under the
    /// pointer. A gesture the PANE acquired (its first event fell on no control and scrolled the
    /// rows) stays the pane's until it ends, however the rows travel under the pointer; without
    /// that, a list scroll ran until a slider's halo slid under the pointer, which then took the
    /// rest of the gesture and adjusted a value the user never aimed at.
    pub(super) fn wheel_owner(&self, px: f32, py: f32, ui: &UiContext, self_id: crate::widget::WidgetId) -> Option<usize> {
        let pane_owns = !ui.scroll_gesture_new && ui.scroll_initiate_widget_id == Some(self_id);
        let rects = self.get_param_rects();
        let latched_to = |id| !ui.scroll_gesture_new && ui.scroll_initiate_widget_id == Some(id);
        let in_row = |r: (f32, f32, f32, f32)| {
            py >= r.1 && py <= r.1 + r.3 && px >= self.rect.x && px <= self.rect.x + self.rect.width
        };
        let n = self.display_params.len();
        let latched = (0..n).find(|&i| {
            self.spinboxes[i].as_ref().is_some_and(|sb| latched_to(sb.base().id()))
                || self.sliders[i].as_ref().is_some_and(|sl| latched_to(sl.base().id()))
                || self.float3s[i].as_ref().is_some_and(|f| f.wheel_latched(ui))
        });
        // Under the pointer: the NEAREST control whose zone
        // holds it, by the distance to the band's (or the
        // spinbox row's) centre line. A band's halo reaches
        // past the band by more than the gap between rows, so
        // two neighbours' zones hold any point between them,
        // and the first in row order is the wrong answer for
        // the lower half of that gap.
        latched.or_else(|| {
            if pane_owns {
                return None;
            }
            (0..n)
                .filter_map(|i| {
                    if rects[i].3 <= 0.0 {
                        return None;
                    }
                    if self.spinboxes[i].is_some() {
                        let r = rects[i];
                        return in_row(r).then(|| (i, (py - (r.1 + r.3 * 0.5)).abs()));
                    }
                    if let Some(sl) = self.sliders[i].as_ref() {
                        let (sx, sy, sw, sh) = sl.rect();
                        let ty = sl.label_strip();
                        let band = Rect { x: sx, y: sy + ty, width: sw, height: sh - ty };
                        return sl
                            .inner()
                            .scroll_hit(band, px, py)
                            .then(|| (i, (py - (band.y + band.height * 0.5)).abs()));
                    }
                    // A float3's bands, and its trackball when it has one.
                    self.float3s[i].as_ref().and_then(|f| f.wheel_zone(px, py)).map(|d| (i, d))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
        })
    }

    /// Row `i`'s value control takes the wheel event, its value written back into the row;
    /// true if it took it (whether or not the row's string ticked over: gating the pane's
    /// fallback on the STRING let every sub-tick trackpad event scroll the pane instead, a
    /// tug-of-war that shifted the rows under the pointer mid-adjust). Ungated forwards
    /// throughout: the owner test already placed the pointer, and a latched gesture may have
    /// drifted off the control's rect.
    pub(super) fn wheel_control(&mut self, i: usize, delta: &MouseScrollDelta, px: f32, py: f32, ui: &mut UiContext) -> bool {
        let p = &mut self.display_params[i];
        if p.2.starts_with("slider") {
            let Some(s) = &mut self.sliders[i] else { return false };
            let was_scroll = s.scroll_enabled;
            s.set_scroll(true);
            let taken = s.mouse_wheel_ungated(delta, px, py, ui);
            if taken {
                let new_val_str = format!("{:.*}", slider_decimals(&p.2), s.get_scaled_value());
                if p.1 != new_val_str {
                    p.1 = new_val_str;
                }
            }
            s.set_scroll(was_scroll);
            taken
        } else if is_vec_row(&p.2) {
            // The group's rows apply the slider-row contract themselves (band halo / gesture
            // latch, else the row strip).
            let Some(f) = &mut self.float3s[i] else { return false };
            if !f.wheel(delta, px, py, ui) {
                return false;
            }
            let new_val_str = f.value_string();
            if p.1 != new_val_str {
                p.1 = new_val_str;
            }
            true
        } else if p.2.starts_with("spinbox") {
            // The widget's own wheel arm: one step per notch, fractions carried between events.
            let Some(sb) = &mut self.spinboxes[i] else { return false };
            ui.scroll_initiate_widget_id = Some(sb.base().id());
            sb.mouse_wheel_ungated(delta, px, py, ui);
            let new_val_str = sb.value.to_string();
            if p.1 != new_val_str {
                p.1 = new_val_str;
            }
            true
        } else {
            false
        }
    }

    /// The pane's own scroll, for a wheel event no control `taken`: over the pane (or its
    /// open popover) and not covered by anything above it, the pane takes the gesture and
    /// scrolls its rows.
    pub(super) fn wheel_pane(&mut self, delta: &MouseScrollDelta, px: f32, py: f32, ui: &mut UiContext, self_id: crate::widget::WidgetId, taken: bool) -> bool {
        // The legacy tail's `self.hit_test(px, py, ctx)`: occlusion via the adapter's
        // address, then rect-or-popover containment.
        let mut swallowed = taken;
        if !ui.is_coordinate_covered(self_id, px, py) {
            let in_rect = px >= self.rect.x
                && px <= self.rect.x + self.rect.width
                && py >= self.rect.y
                && py <= self.rect.y + self.rect.height;
            let in_popover = self.own_popover_rect().is_some_and(|(rx, ry, rw, rh)| {
                px >= rx && px <= rx + rw && py >= ry && py <= ry + rh
            });
            if in_rect || in_popover {
                if !taken {
                    if crate::scroll_debug() {
                        eprintln!("[scroll] params: PANE-SCROLL fallback at ({px:.0},{py:.0})");
                    }
                    // The pane takes the gesture (see `pane_owns`).
                    ui.scroll_initiate_widget_id = Some(self_id);
                    let max_scroll = (self.content_h - self.rect.height).max(0.0);
                    self.scroll_motion.reconcile(0.0, self.scroll_y);
                    let moved = self.scroll_motion.apply(
                        delta,
                        (crate::widget::LINE_PX, crate::widget::LINE_PX),
                        crate::widget::Bounds::max(0.0),
                        crate::widget::Bounds::max(max_scroll),
                    );
                    self.scroll_y = self.scroll_motion.y.pos();
                    if moved {
                        self.update_slider_rects();
                        self.activity.bump();
                        self.recompute_scrollbar_raised();
                        self.rehover_after_scroll(ui);
                    }
                }
                // An opaque pane swallows EVERY wheel over it, whether
                // anything moved or not: returning false would hand the
                // event to whatever lies BEHIND the plate — the designer
                // routes unhandled wheels to the 3D viewport, whose rect
                // is the whole window in the floating layout, so a
                // near-miss on a slider would orbit the camera through
                // the pane.
                swallowed = true;
            }
        }

        swallowed
    }
}
