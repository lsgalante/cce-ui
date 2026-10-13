//! Input: hover and the keyboard's highlight, presses (a row's action, a page turn, the back
//! band), running a row wherever it is drawn and the host's pick, the wheel, and the slider
//! rows.

use super::*;

impl ContextMenuState {

    /// Make row `idx` a slider. Widens the plate to hold the label, the
    /// readout at its widest (both ends of the range) and the band.
    pub fn set_row_slider(&mut self, idx: usize, slider: MenuSlider) {
        if idx >= self.options.len() {
            return;
        }
        self.sliders[idx] = Some(slider);
        let (family, size) = label_font();
        let measure = |t: &str| crate::widget::display::measure_text_width(t, &family, size);
        let readout_w = [slider.min, slider.max]
            .iter()
            .map(|&v| measure(&MenuSlider { value: v, ..slider }.readout()))
            .fold(0.0f32, f32::max);
        let need = PAD + measure(&self.options[idx]) + SLIDER_GAP + readout_w + SLIDER_GAP + SLIDER_W + PAD;
        self.w = self.w.max(need);
    }

    /// Whether a press on row `idx` could run it: not a header, not a
    /// separator, not disabled.
    pub(super) fn actionable(&self, idx: usize) -> bool {
        idx >= self.header_count && !self.row_disabled(idx) && self.options.get(idx).is_some_and(|o| o != "-")
    }

    /// Highlight row `idx` as the pointer would, scrolled into view —
    /// the keyboard's hover. `None`, or a row that cannot run, clears it.
    pub fn set_hovered_item(&mut self, idx: Option<usize>) {
        self.hovered_item = idx.filter(|&i| self.actionable(i));
        if let Some(i) = self.hovered_item {
            self.scroll_into_view(i);
        }
    }

    /// Move the highlight to the next row that can run, `dir` > 0 down
    /// and < 0 up, skipping headers, separators and disabled rows; from no highlight,
    /// the first (or last) such row. Stops at either end rather than
    /// wrapping. Returns the highlighted row.
    pub fn step_hovered(&mut self, dir: i32) -> Option<usize> {
        let n = self.options.len() as i32;
        let step = if dir < 0 { -1 } else { 1 };
        let mut i = match self.hovered_item {
            Some(h) => h as i32,
            None if step > 0 => -1,
            None => n,
        };
        loop {
            i += step;
            if i < 0 || i >= n {
                return self.hovered_item;
            }
            if self.actionable(i as usize) {
                self.set_hovered_item(Some(i as usize));
                return self.hovered_item;
            }
        }
    }

    /// The slider on row `idx`, if it is one.
    pub fn slider(&self, idx: usize) -> Option<MenuSlider> {
        self.sliders.get(idx).copied().flatten()
    }

    /// The band row `idx`'s slider draws over and a press grabs.
    pub fn slider_band(&self, idx: usize) -> crate::scene::layout::Rect {
        crate::scene::layout::Rect {
            x: self.x + self.w - PAD - SLIDER_W,
            y: self.row_y(idx) + 3.0,
            width: SLIDER_W,
            height: ROW_H - 6.0,
        }
    }

    pub(super) fn set_slider_value(&mut self, idx: usize, v: f32) -> bool {
        let Some(Some(s)) = self.sliders.get_mut(idx) else { return false };
        let v = s.clamp_snap(v);
        if (v - s.value).abs() < f32::EPSILON {
            return false;
        }
        s.value = v;
        self.slider_change = Some((idx, v));
        true
    }

    /// The wheel over a slider row steps it; anywhere else it does
    /// nothing and says so. Up is more, as on every slider in the DE.
    pub fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32) -> bool {
        if !self.visible {
            return false;
        }
        // A side swipe turns a page: forward with the pointer on a page
        // row, back from anywhere on a page that has somewhere to go.
        if self.hit_test(px, py) && self.slider_drag.is_none() {
            let turn = match crate::widget::side_swipe::feed(delta) {
                Some(crate::widget::SwipeDir::Forward) => {
                    self.row_at(px, py).filter(|&i| self.leads_to_page(i) && !self.row_disabled(i)).map(PageTurn::Into)
                }
                Some(crate::widget::SwipeDir::Back) => self.back.is_some().then_some(PageTurn::Back),
                None => None,
            };
            if turn.is_some() {
                self.turn = turn;
                return true;
            }
        }
        let Some(idx) = self.row_at(px, py) else { return false };
        let Some(s) = self.slider(idx).filter(|_| !self.row_disabled(idx)) else {
            // Not a slider: a menu cut down to fit scrolls its rows.
            // Up shows the rows above, as every list in the DE does.
            if self.max_scroll() <= 0.0 {
                return false;
            }
            self.scroll_by(-delta.notches_y() * ROW_H);
            return true;
        };
        self.wheel_accum += delta.value_notches_y();
        let whole = self.wheel_accum.trunc();
        if whole == 0.0 {
            return false;
        }
        self.wheel_accum -= whole;
        let step = if s.step > 0.0 { s.step } else { (s.max - s.min) * 0.02 };
        self.set_slider_value(idx, s.value + whole * step)
    }

    /// A left press on a slider row: on the band it takes hold and jumps
    /// the value to the pointer; anywhere on the row it is the slider's
    /// and the menu stays open. `false` for any other row, and for a
    /// disabled slider.
    pub fn slider_press(&mut self, px: f32, py: f32) -> bool {
        if !self.visible {
            return false;
        }
        let Some(idx) = self.row_at(px, py) else { return false };
        if self.slider(idx).is_none() || self.row_disabled(idx) {
            return false;
        }
        let band = self.slider_band(idx);
        if px >= band.x && px <= band.x + band.width {
            self.slider_drag = Some(idx);
            self.slider_drag_to(px);
        }
        true
    }

    /// Move the held slider to the pointer's place along its band —
    /// wherever the pointer is, so a drag that leaves the plate keeps
    /// working. `false` when nothing is held or nothing moved.
    pub fn slider_drag_to(&mut self, px: f32) -> bool {
        let Some(idx) = self.slider_drag else { return false };
        let Some(s) = self.slider(idx) else { return false };
        let band = self.slider_band(idx);
        let t = ((px - band.x) / band.width.max(1.0)).clamp(0.0, 1.0);
        self.set_slider_value(idx, s.min + t * (s.max - s.min))
    }

    /// End a slider drag; `true` if one was held.
    pub fn slider_release(&mut self) -> bool {
        self.slider_drag.take().is_some()
    }

    pub fn take_slider_change(&mut self) -> Option<(usize, f32)> {
        self.slider_change.take()
    }

    pub fn cursor_moved(&mut self, px: f32, py: f32) -> bool {
        if !self.visible { return false; }
        self.last_cursor = Some((px, py));
        if self.slider_drag.is_some() {
            return self.slider_drag_to(px);
        }
        let was = (self.hovered_item, self.back_hovered);
        self.rehover(px, py);
        (self.hovered_item, self.back_hovered) != was
    }

    pub(super) fn rehover(&mut self, px: f32, py: f32) -> bool {
        let was_hovered = (self.hovered_item, self.back_hovered);
        self.back_hovered = self.on_back_band(px, py);
        // A "-" row is a SEPARATOR (the dropdown's convention): engraved,
        // never hovered, never an action — nor is a header or a disabled row.
        self.hovered_item = self.row_at(px, py).filter(|&idx| self.actionable(idx));
        (self.hovered_item, self.back_hovered) != was_hovered
    }

    pub fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: Option<&mut crate::context::UiContext>) -> bool {
        if !self.visible { return false; }
        if button == MouseButton::Left && state == ElementState::Released && self.slider_release() {
            return true;
        }
        if button == MouseButton::Left && state == ElementState::Pressed && self.slider_press(px, py) {
            return true;
        }
        if button != MouseButton::Left || state != ElementState::Pressed {
            if state == ElementState::Pressed {
                self.hide();
                return true;
            }
            return false;
        }

        if let Some(turn) = self.turn_at(px, py) {
            self.turn = Some(turn);
            return true;
        }
        // On a row that runs, the row; anywhere else — a header, a
        // separator, a disabled row, the padding, off the plate — the press
        // only closes the menu.
        if self.row_at(px, py).is_some_and(|idx| self.activate(idx, ctx)) {
            return true;
        }
        self.hide();
        true
    }

    /// Run row `idx` as a press on it does, wherever it is drawn — what
    /// Enter on the highlight ([`activate_hovered`](Self::activate_hovered))
    /// and a screen reader's Click do, a row scrolled out of a shortened
    /// menu included:
    ///
    /// - a row with an action runs it on the menu's target (through `ctx`;
    ///   with no context, or a target that is gone, it runs nothing), as
    ///   does a row whose English label the fallback knows
    ///   (`legacy_action_for_label`);
    /// - a row with no action of its own is the HOST's: it is recorded for
    ///   [`take_picked`](Self::take_picked) (the label fallback still runs
    ///   on a target that takes it);
    /// - either way the menu closes;
    /// - a page row asks to turn into its page (`take_turn`), and stays up.
    ///
    /// `false`, and nothing done, for a row that cannot run — a header, a
    /// separator, a disabled row, a slider (which is worked, not run) —
    /// or a hidden menu.
    pub fn activate(&mut self, idx: usize, ctx: Option<&mut crate::context::UiContext>) -> bool {
        if !self.visible || !self.actionable(idx) || self.slider(idx).is_some() {
            return false;
        }
        if self.leads_to_page(idx) {
            self.turn = Some(PageTurn::Into(idx));
            return true;
        }
        let own = self.actions.get(idx).copied().flatten();
        if own.is_none() {
            self.picked = Some((self.generation, idx));
        }
        if let (Some(target_id), Some(ctx)) = (self.target, ctx) {
            let action = own.or_else(|| legacy_action_for_label(&self.options[idx]));
            if let (Some(action), Some(target)) = (action, ctx.get_widget_mut(target_id)) {
                let _ = target.context_action(action);
            }
        }
        self.hide();
        true
    }

    /// Run the highlighted row — see [`activate`](Self::activate). What a
    /// host routes Enter to; `false` with nothing highlighted.
    pub fn activate_hovered(&mut self, ctx: Option<&mut crate::context::UiContext>) -> bool {
        self.hovered_item.is_some_and(|idx| self.activate(idx, ctx))
    }

    /// The last row of the host's own that was run — a press, Enter or a
    /// screen reader's Click on a row with no action — as `(generation,
    /// row)`, once. A host that keeps its own rows' meaning beside the
    /// labels (a `Vec` of its actions, the [`generation`](Self::generation)
    /// of the `show` it built them for) routes presses through
    /// [`mouse_input`](Self::mouse_input), then takes the pick and runs it
    /// when the generation is its own. A reader's Click arrives between
    /// events, so a host takes the pick in `tick` too.
    pub fn take_picked(&mut self) -> Option<(u64, usize)> {
        self.picked.take()
    }
}
