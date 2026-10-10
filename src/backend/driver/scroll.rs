//! Scrolling: wheel and finger frames turned into the app's wheel events with their phase, a touch
//! read as a tap, a scroll or a held button, and the pinch fallback for an app with no pinch of
//! its own.

use super::*;

impl Driver {
    /// One coalesced frame of scrolling at `pos`. Publishes the frame's
    /// phase (`scroll_motion::set_scroll_phase`) before the app sees it.
    pub fn scroll<A: Application>(&mut self, t: Turn<'_, A>, frame: ScrollFrame, pos: LogicalPosition) {
        self.note_input();
        let ScrollFrame { h, v, discrete_h, discrete_v, source, stop } = frame;
        // Per-app scroll factors from input.kdl (`<app>`/`cce-ui` domain
        // `input { }` blocks); the compositor's global device scaling has
        // already been applied at the source.
        let factors = crate::input::scroll_factors();
        let (phase, delta) = scroll_delta(&frame, factors);
        crate::widget::scroll_motion::set_scroll_phase(phase);
        if crate::scroll_debug() {
            static T0: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
            let ms = T0.get_or_init(Instant::now).elapsed().as_millis();
            eprintln!(
                "[scroll {ms}ms] runner: coalesced=({h:.2},{v:.2}) discrete=({discrete_h},{discrete_v}) source={source:?} stop={stop} phase={phase:?} factors=(tp {:.2}, m {:.2}) -> {delta:?} at ({:.0},{:.0})",
                factors.trackpad, factors.mouse, pos.x, pos.y
            );
        }
        let mut rebuild = false;
        self.sync_mods(t.app);
        t.app.handle_mouse_wheel(&delta, pos, &mut rebuild);
        if rebuild {
            *t.redraw = true;
        }
    }

    /// What a touchscreen finger did, as pointer input (the shell's
    /// [`TouchTracker`](crate::backend::touch::TouchTracker) decides which): a hover
    /// or drag moves the pointer, a press and release are the left button's,
    /// and a scroll is a trackpad finger scroll dispatched at `scroll_at`,
    /// where the finger went down — the gesture belongs to what it began on,
    /// however far the content moves.
    ///
    /// Never a CSD move or resize, and never an app's window action: a touch
    /// serial does not satisfy the compositor's pointer-grab check, and an
    /// action left queued would run on the next pointer press with that
    /// press's serial, so one a touch queued is dropped.
    pub fn touch<A: Application>(
        &mut self,
        mut t: Turn<'_, A>,
        actions: Vec<crate::backend::touch::TouchAction>,
        scroll_at: Option<(f32, f32)>,
    ) {
        self.note_input();
        use crate::backend::touch::TouchAction;
        for action in actions {
            let mut rebuild = false;
            let mut msg = None;
            match action {
                TouchAction::Hover(x, y) | TouchAction::Drag(x, y) => {
                    t.app.handle_pointer_move(LogicalPosition::new(x, y), &mut rebuild);
                }
                TouchAction::Leave => {
                    t.app.handle_pointer_move(LogicalPosition::new(-10000.0, -10000.0), &mut rebuild);
                }
                TouchAction::Press(x, y) => {
                    // Outside-press close for open popovers, as the pointer's
                    // press does before the app's own dispatch.
                    close_popovers_missed_by(t.app, x, y);
                    // A tapped field that stays open is announced again,
                    // so the on-screen keyboard can follow the tap.
                    if crate::ime::note_press() {
                        rebuild = true;
                    }
                    let pos = LogicalPosition::new(x, y);
                    msg = t.app.handle_mouse_input(MouseButton::Left, ElementState::Pressed, pos, &mut rebuild);
                }
                TouchAction::Release(x, y) => {
                    let pos = LogicalPosition::new(x, y);
                    msg = t.app.handle_mouse_input(MouseButton::Left, ElementState::Released, pos, &mut rebuild);
                }
                TouchAction::Scroll(dx, dy) => self.touch_scroll(t.app, scroll_at, ScrollPhase::Finger, dx, dy, &mut rebuild),
                TouchAction::ScrollEnd => {
                    self.touch_scroll(t.app, scroll_at, ScrollPhase::FingerEnd, 0.0, 0.0, &mut rebuild)
                }
            }
            t.deliver(msg, rebuild);
        }
        let _ = t.app.take_window_action();
    }

    /// Finger travel as a trackpad pixel scroll. A finger is always
    /// "natural" — the content goes where it is pushed — so the dispatch runs
    /// with natural scrolling on whatever the trackpad's setting, and a value
    /// control (`MouseScrollDelta::value_notches_y`) reads the finger's real
    /// direction. 1:1, without the trackpad's per-app factor: the content
    /// stays under the finger.
    pub(super) fn touch_scroll<A: Application>(
        &self,
        app: &mut A,
        scroll_at: Option<(f32, f32)>,
        phase: ScrollPhase,
        dx: f32,
        dy: f32,
        rebuild: &mut bool,
    ) {
        let Some((x, y)) = scroll_at else { return };
        crate::widget::scroll_motion::set_scroll_phase(phase);
        let delta = MouseScrollDelta::PixelDelta(Position { x: dx as f64, y: dy as f64 });
        if crate::scroll_debug() {
            eprintln!("[scroll] touch: phase={phase:?} -> {delta:?} at ({x:.0},{y:.0})");
        }
        self.sync_mods(app);
        crate::input::with_natural_scroll(true, || {
            app.handle_mouse_wheel(&delta, LogicalPosition::new(x, y), rebuild);
        });
    }

    /// A pinch gesture began.
    pub fn pinch_begin(&mut self) {
        self.last_pinch_scale = 1.0;
    }

    /// A pinch gesture ended.
    pub fn pinch_end(&mut self) {
        self.last_pinch_scale = 1.0;
    }

    /// The pinch's cumulative `scale` moved, with the pointer where it last was.
    pub fn pinch_update<A: Application>(&mut self, t: Turn<'_, A>, scale: f32) {
        self.note_input();
        let factor = scale / self.last_pinch_scale;
        self.last_pinch_scale = scale;

        let (px, py) = self.cursor_pos;
        let mut rebuild = false;

        // First offer the gesture as-is: apps with true pinch surfaces (the
        // designer's 3D viewport) consume it here at 1:1 scale instead of
        // through the wheel synthesis below.
        if t.app.handle_pinch(factor, LogicalPosition::new(px, py), &mut rebuild) {
            if rebuild {
                *t.redraw = true;
            }
            return;
        }

        // Calculate the y_delta for PixelDelta mapping.
        // Since cce-graph interprets factor = 1.0 + y_delta * 0.015, we reverse it:
        let y_delta = (factor - 1.0) / 0.015;
        let delta = MouseScrollDelta::PixelDelta(Position {
            x: 0.0,
            y: y_delta as f64,
        });

        if let Some(ctx) = t.app.ui_context_mut() {
            ctx.ctrl_pressed = true; // Force ctrl_pressed = true for the pinch event
        }
        // A synthesized delta, not a scroll gesture: no glide, no fling.
        crate::widget::scroll_motion::set_scroll_phase(crate::widget::ScrollPhase::Wheel);

        t.app.handle_mouse_wheel(&delta, LogicalPosition::new(px, py), &mut rebuild);

        if let Some(ctx) = t.app.ui_context_mut() {
            ctx.ctrl_pressed = self.mods.ctrl; // Restore original state
        }

        if rebuild {
            *t.redraw = true;
        }
    }
}
