//! The cce protocols: the inspector, and window management (windows, outputs, seat, toplevels).

use super::*;

impl<A: Application> wayland_client::Dispatch<crate::protocol::zcce_inspector_v1::ZcceInspectorV1, ()> for EngineState<A> {
    fn event(
        _state: &mut Self,
        _proxy: &crate::protocol::zcce_inspector_v1::ZcceInspectorV1,
        _event: crate::protocol::zcce_inspector_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}
}

impl<A: Application> wayland_client::Dispatch<crate::protocol::cce_window_management_v1::zcce_window_manager_v1::ZcceWindowManagerV1, ()> for EngineState<A> {
    fn event(
        _state: &mut Self,
        _proxy: &crate::protocol::cce_window_management_v1::zcce_window_manager_v1::ZcceWindowManagerV1,
        _event: crate::protocol::cce_window_management_v1::zcce_window_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}

    wayland_client::event_created_child!(
        EngineState<A>,
        crate::protocol::cce_window_management_v1::zcce_window_manager_v1::ZcceWindowManagerV1,
        [
            6 => (crate::protocol::cce_window_management_v1::zcce_window_v1::ZcceWindowV1, ()),
            7 => (crate::protocol::cce_window_management_v1::zcce_output_v1::ZcceOutputV1, ()),
            8 => (crate::protocol::cce_window_management_v1::zcce_seat_v1::ZcceSeatV1, ()),
        ]
    );
}

impl<A: Application> wayland_client::Dispatch<crate::protocol::cce_window_management_v1::zcce_window_v1::ZcceWindowV1, ()> for EngineState<A> {
    fn event(
        _state: &mut Self,
        _proxy: &crate::protocol::cce_window_management_v1::zcce_window_v1::ZcceWindowV1,
        _event: crate::protocol::cce_window_management_v1::zcce_window_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}
}

impl<A: Application> wayland_client::Dispatch<crate::protocol::cce_window_management_v1::zcce_output_v1::ZcceOutputV1, ()> for EngineState<A> {
    fn event(
        _state: &mut Self,
        _proxy: &crate::protocol::cce_window_management_v1::zcce_output_v1::ZcceOutputV1,
        _event: crate::protocol::cce_window_management_v1::zcce_output_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}
}

impl<A: Application> wayland_client::Dispatch<crate::protocol::cce_window_management_v1::zcce_seat_v1::ZcceSeatV1, ()> for EngineState<A> {
    fn event(
        _state: &mut Self,
        _proxy: &crate::protocol::cce_window_management_v1::zcce_seat_v1::ZcceSeatV1,
        _event: crate::protocol::cce_window_management_v1::zcce_seat_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}
}

impl<A: Application> wayland_client::Dispatch<crate::protocol::cce_window_management_v1::zcce_toplevel_v1::ZcceToplevelV1, ()> for EngineState<A> {
    fn event(
        state: &mut Self,
        _proxy: &crate::protocol::cce_window_management_v1::zcce_toplevel_v1::ZcceToplevelV1,
        event: crate::protocol::cce_window_management_v1::zcce_toplevel_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        use crate::protocol::cce_window_management_v1::zcce_toplevel_v1::Event;
        if let Event::GridPatch { serial, x, y, width, height, scale } = event {
            // A newer patch supersedes an unconsumed older one.
            state.pending_grid_patch = Some((serial, x, y, width, height, scale));
            state.redraw = true;
        }
    }
}
