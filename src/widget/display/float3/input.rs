//! A `Float3`'s input: the wheel routed to the row (or the ball) the gesture began on, the nearest
//! band to a point, and `impl Input` (drags, the tick, events) delegating to the rows and the ball.

use super::*;

impl Float3 {
    /// Wheel over the rows, the parameter pane's slider-row contract: under the band style the
    /// capture zone is each row's shape halo ([`Slider::scroll_hit`]) or its gesture latch,
    /// otherwise the row's rect; a row in zone takes the wheel ungated (the halo already gated
    /// spatially, and the adapter's rect gate would clip its fringe). Returns whether a row took
    /// it, whether or not the value string ticked over.
    ///
    /// ONE row takes it: the latched one, else the NEAREST of the rows whose
    /// halo holds the pointer. A halo reaches past its band by more than the
    /// gap between rows, so two rows' halos hold any point between them —
    /// and until 2026-09-28 the first in X/Y/Z order won, so a scroll over
    /// the Y band turned X.
    /// The BALL comes before the rows: a gesture it holds, or one nobody
    /// holds that falls on it, rolls it ([`Self::ball_scroll`]).
    pub fn wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32, ui: &mut UiContext) -> bool {
        let ball_latched = !ui.scroll_gesture_new && ui.scroll_initiate_widget_id == Some(self.ball_id);
        let band_latched = self.wheel_latched(ui) && !ball_latched;
        if self.ball && (ball_latched || (!band_latched && self.ball_hit(px, py))) {
            ui.scroll_initiate_widget_id = Some(self.ball_id);
            self.ball_scroll(delta);
            return true;
        }
        let Some(i) = self.wheel_row(px, py, ui) else {
            return false;
        };
        let s = &mut self.sliders[i];
        let was_scroll = s.scroll_enabled;
        s.set_scroll(true);
        let taken = s.mouse_wheel_ungated(delta, px, py, ui);
        s.set_scroll(was_scroll);
        taken
    }

    /// The row a wheel at `(px, py)` belongs to, if any — see [`Self::wheel`] —
    /// with its band centre's distance from the pointer for a host choosing
    /// between this group and its neighbours.
    pub fn wheel_row(&self, px: f32, py: f32, ui: &UiContext) -> Option<usize> {
        let latched = self
            .sliders
            .iter()
            .position(|s| !ui.scroll_gesture_new && ui.scroll_initiate_widget_id == Some(s.base().id()));
        latched.or_else(|| self.nearest_band(px, py).map(|(i, _)| i))
    }

    /// The nearest row whose halo holds the pointer, with the distance from
    /// the pointer to that band's centre line.
    pub fn nearest_band(&self, px: f32, py: f32) -> Option<(usize, f32)> {
        self.get_row_rects()
            .into_iter()
            .enumerate()
            .filter(|(i, r)| self.sliders[*i].inner().scroll_hit(Rect { x: r.0, y: r.1, width: r.2, height: r.3 }, px, py))
            .map(|(i, r)| (i, (py - (r.1 + r.3 * 0.5)).abs()))
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }
}

impl Input for Float3 {
    fn draggable(&self, _rect: Rect) -> bool {
        self.dragging_idx.is_some() || self.ball_drag.is_some()
    }

    fn is_dragging(&self) -> bool {
        self.dragging_idx.is_some() || self.ball_drag.is_some()
    }

    /// A host-driven drag begins on the row under the pointer — unless a press already
    /// started one (the pane's press path), in which case that row keeps it. The ball
    /// comes first: it stands beside the rows, inside their span of y.
    fn drag_begin(&mut self, px: f32, py: f32, _rect: Rect) {
        if self.dragging_idx.is_some() || self.ball_drag.is_some() {
            return;
        }
        if self.ball_hit(px, py) {
            self.ball_begin(px, py);
            return;
        }
        let rows = self.get_row_rects();
        for (i, r) in rows.into_iter().enumerate() {
            if py >= r.1 && py <= r.1 + r.3 {
                self.sliders[i].drag_begin(px, py);
                self.dragging_idx = Some(i);
                return;
            }
        }
    }

    fn drag_update(&mut self, px: f32, py: f32, _rect: Rect) -> bool {
        if self.ball_drag.is_some() {
            return self.ball_roll(px, py);
        }
        match self.dragging_idx {
            Some(i) => self.sliders[i].drag_update(px, py),
            None => false,
        }
    }

    fn drag_end(&mut self) {
        self.ball_drag = None;
        if let Some(i) = self.dragging_idx.take() {
            self.sliders[i].drag_end();
        }
    }

    /// The rows' wheel-glide inertia.
    fn tick(&mut self, dt: f32, _rect: Rect) -> bool {
        let mut dummy = UiContext::new();
        let mut changed = false;
        for s in self.sliders.iter_mut() {
            changed |= WidgetHost::tick(s, dt, &mut dummy);
        }
        changed
    }

    fn take_change(&mut self) -> bool {
        let mut any = false;
        for s in self.sliders.iter_mut() {
            any |= s.take_change();
        }
        any
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button, state, x: px, y: py, .. } => {
                if *button != MouseButton::Left {
                    return false;
                }
                let mut dummy = UiContext::new();
                match state {
                    ElementState::Pressed => {
                        // A press on the ball takes hold of it; the drag
                        // that follows turns the vector (`drag_update`).
                        if self.ball_hit(*px, *py) {
                            self.ball_begin(*px, *py);
                            return true;
                        }
                        let rows = self.get_row_rects();
                        for (i, r) in rows.into_iter().enumerate() {
                            if *py < r.1 || *py > r.1 + r.3 {
                                continue;
                            }
                            // The child's own readout click claims focus through the
                            // dummy ctx (a no-op beyond the thread-local slot); the
                            // GROUP is the host's focus target, as before.
                            if !self.sliders[i].mouse_input(*button, *state, *px, *py, &mut dummy) {
                                continue;
                            }
                            if self.sliders[i].is_dragging() {
                                self.dragging_idx = Some(i);
                            }
                            if self.sliders[i].editing {
                                for (j, s) in self.sliders.iter_mut().enumerate() {
                                    if j != i && s.editing {
                                        s.unfocus();
                                    }
                                }
                                ectx.request_focus();
                            }
                            return true;
                        }
                        false
                    }
                    ElementState::Released => {
                        let mut any = false;
                        for s in self.sliders.iter_mut() {
                            any |= s.mouse_input(*button, *state, *px, *py, &mut dummy);
                        }
                        if self.dragging_idx.take().is_some() {
                            any = true;
                        }
                        if self.ball_drag.take().is_some() {
                            any = true;
                        }
                        any
                    }
                }
            }
            Event::MouseWheel { delta, x: px, y: py, .. } => {
                let mut dummy = UiContext::new();
                let ui = ectx.ui.as_deref_mut();
                match ui {
                    Some(ui) => self.wheel(delta, *px, *py, ui),
                    None => self.wheel(delta, *px, *py, &mut dummy),
                }
            }
            Event::PointerMove { x: px, y: py, .. } => {
                let mut dummy = UiContext::new();
                let mut changed = false;
                for s in self.sliders.iter_mut() {
                    changed |= s.on_cursor_moved(*px, *py, &mut dummy);
                }
                changed
            }
            Event::KeyInput(key_event) => {
                let mut dummy = UiContext::new();
                for s in self.sliders.iter_mut() {
                    if s.editing {
                        return s.keyboard_input(key_event, &mut dummy);
                    }
                }
                false
            }
            // Focus loss commits every open readout edit (each row's own FocusOut).
            Event::FocusOut => {
                for s in self.sliders.iter_mut() {
                    s.unfocus();
                }
                true
            }
            _ => false,
        }
    }
}
