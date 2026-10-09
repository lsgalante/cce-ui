//! Pointer input, mapped into the driver: buttons, axis frames, CSD grabs and cursors, the pinch
//! gesture.

use super::*;

impl<A: Application> PointerHandler for EngineState<A> {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[smithay_client_toolkit::seat::pointer::PointerEvent],
    ) {
        use smithay_client_toolkit::seat::pointer::PointerEventKind;
        let mut scroll = ScrollFrame::default();
        let mut has_scroll = false;
        let (mut last_lx, mut last_ly) = (0.0f32, 0.0f32);

        // Forced mode: pointer positions arrive in the compositor's scale-1
        // logical space (= physical); divide into the app's logical space.
        let forced = crate::scale::forced_scale().unwrap_or(1.0);
        for event in events {
            let (x, y) = event.position;
            // Overflow-margin mode needs no translation: the rim is
            // right/bottom-only, so frame coords == surface coords.
            let lx = x as f32 / forced;
            let ly = y as f32 / forced;
            // An event on the context menu's popup surface is the app's too,
            // at the popup's offset from the window: menu dispatch works in
            // window coordinates, which now reach outside the window.
            let popup_offset = self.menu_popup_offset(&event.surface);
            let on_popup = popup_offset.is_some();
            let (lx, ly) = match popup_offset {
                Some((ox, oy)) => (lx + ox, ly + oy),
                None => (lx, ly),
            };
            let pos = LogicalPosition::new(lx, ly);

            self.driver.cursor_pos = (lx, ly);
            match &event.kind {
                PointerEventKind::Enter { .. } => {
                    let (driver, t) = self.turn();
                    driver.pointer_enter(t, pos);

                    let cursor_icon = self.cursor_icon_at(lx, ly);
                    self.current_cursor_icon = Some(cursor_icon);
                    if let Some(ref themed_pointer) = self.pointer {
                        let _ = themed_pointer.set_cursor(_conn, cursor_icon);
                    }
                }
                PointerEventKind::Leave { .. } => {
                    self.current_cursor_icon = None;
                    let (driver, t) = self.turn();
                    driver.pointer_leave(t);
                }
                PointerEventKind::Motion { .. } => {
                    let (driver, t) = self.turn();
                    driver.pointer_motion(t, pos);

                    let cursor_icon = self.cursor_icon_at(lx, ly);
                    if self.current_cursor_icon != Some(cursor_icon) {
                        self.current_cursor_icon = Some(cursor_icon);
                        if let Some(ref themed_pointer) = self.pointer {
                            let _ = themed_pointer.set_cursor(_conn, cursor_icon);
                        }
                    }
                }
                PointerEventKind::Press { button, serial, .. } => {
                    let Some(btn) = evdev_button(*button) else { continue };
                    self.last_press_serial = Some(*serial);
                    let seat = self.seats.first().cloned().or_else(|| self.seat_state.seats().next());
                    let site = PressSite {
                        size: self.logical_size(),
                        on_popup,
                        can_grab: self.window.is_some() && seat.is_some(),
                        own_edges: false,
                    };
                    let (driver, t) = self.turn();
                    let press = driver.pointer_press(t, btn, pos, site);
                    if let (Some(window), Some(seat)) = (&self.window, &seat) {
                        match press {
                            Press::Dispatched => {}
                            Press::Resize(edge) => window.resize(seat, *serial, xdg_resize_edge(edge)),
                            Press::Move => window.move_(seat, *serial),
                        }
                    }
                }
                PointerEventKind::Release { button, .. } => {
                    let Some(btn) = evdev_button(*button) else { continue };
                    let (driver, t) = self.turn();
                    driver.pointer_release(t, btn, pos);
                }
                PointerEventKind::Axis { horizontal, vertical, source, .. } => {
                    scroll.h += horizontal.absolute;
                    scroll.v += vertical.absolute;
                    scroll.discrete_h += horizontal.discrete;
                    scroll.discrete_v += vertical.discrete;
                    // The source and the finger-lift stop ride in the same
                    // frame as the deltas (or alone, for the lift): they
                    // decide the smooth-scroll phase.
                    if let Some(source) = source {
                        scroll.source = Some(scroll_source(*source));
                    }
                    scroll.stop |= horizontal.stop || vertical.stop;
                    last_lx = lx;
                    last_ly = ly;
                    has_scroll = true;
                }
            }
        }

        if has_scroll {
            let (driver, t) = self.turn();
            driver.scroll(t, scroll, LogicalPosition::new(last_lx, last_ly));
        }

        // App-driven window move/resize (non-standard CSD; see WindowAction):
        // executed with the serial of the most recent pointer press.
        if let Some(action) = self.inner.as_mut().unwrap().take_window_action() {
            if let (Some(ref window), Some(serial)) = (&self.window, self.last_press_serial) {
                let seat_owned = self.seats.first().cloned().or_else(|| self.seat_state.seats().next());
                if let Some(ref seat) = seat_owned {
                    match action {
                        WindowAction::Move => window.move_(seat, serial),
                        WindowAction::Resize(edge) => window.resize(seat, serial, edge),
                    }
                }
            }
        }
    }
}

/// An evdev button code as one of cce-ui's buttons; the rest are not routed.
pub(super) fn evdev_button(code: u32) -> Option<MouseButton> {
    match code {
        272 => Some(MouseButton::Left),
        273 => Some(MouseButton::Right),
        274 => Some(MouseButton::Middle),
        _ => None,
    }
}

pub(super) fn xdg_resize_edge(edge: ResizeEdge) -> xdg_toplevel::ResizeEdge {
    match edge {
        ResizeEdge::Top => xdg_toplevel::ResizeEdge::Top,
        ResizeEdge::Bottom => xdg_toplevel::ResizeEdge::Bottom,
        ResizeEdge::Left => xdg_toplevel::ResizeEdge::Left,
        ResizeEdge::Right => xdg_toplevel::ResizeEdge::Right,
        ResizeEdge::TopLeft => xdg_toplevel::ResizeEdge::TopLeft,
        ResizeEdge::TopRight => xdg_toplevel::ResizeEdge::TopRight,
        ResizeEdge::BottomLeft => xdg_toplevel::ResizeEdge::BottomLeft,
        ResizeEdge::BottomRight => xdg_toplevel::ResizeEdge::BottomRight,
    }
}

/// A `wl_pointer` axis source as the driver's. Anything newer than the four
/// known sources scrolls as a wheel, as it did when the runner matched on
/// the protocol enum itself.
pub(super) fn scroll_source(source: wl_pointer::AxisSource) -> ScrollSource {
    match source {
        wl_pointer::AxisSource::Finger => ScrollSource::Finger,
        wl_pointer::AxisSource::Continuous => ScrollSource::Continuous,
        wl_pointer::AxisSource::WheelTilt => ScrollSource::WheelTilt,
        _ => ScrollSource::Wheel,
    }
}

impl<A: Application> wayland_client::Dispatch<ZwpPointerGesturesV1, ()> for EngineState<A> {
    fn event(
        _state: &mut Self,
        _proxy: &ZwpPointerGesturesV1,
        _event: zwp_pointer_gestures::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}
}

impl<A: Application> wayland_client::Dispatch<ZwpPointerGesturePinchV1, ()> for EngineState<A> {
    fn event(
        state: &mut Self,
        _proxy: &ZwpPointerGesturePinchV1,
        event: zwp_pointer_gesture_pinch_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            zwp_pointer_gesture_pinch_v1::Event::Begin { .. } => state.driver.pinch_begin(),
            zwp_pointer_gesture_pinch_v1::Event::Update { scale, .. } => {
                let (driver, t) = state.turn();
                driver.pinch_update(t, scale as f32);
            }
            zwp_pointer_gesture_pinch_v1::Event::End { .. } => state.driver.pinch_end(),
            _ => {}
        }
    }
}
