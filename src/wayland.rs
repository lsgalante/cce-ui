use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle, WindowHandle,
};
use smithay_client_toolkit::output::OutputState;

#[derive(Debug, Clone, Copy)]
pub struct WaylandSurfaceHandle {
    pub display_ptr: *mut std::ffi::c_void,
    pub surface_ptr: *mut std::ffi::c_void,
}

unsafe impl Send for WaylandSurfaceHandle {}
unsafe impl Sync for WaylandSurfaceHandle {}

impl HasDisplayHandle for WaylandSurfaceHandle {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let raw = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
            std::ptr::NonNull::new(self.display_ptr).ok_or(HandleError::Unavailable)?,
        ));
        unsafe { Ok(DisplayHandle::borrow_raw(raw)) }
    }
}

impl HasWindowHandle for WaylandSurfaceHandle {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let raw = RawWindowHandle::Wayland(WaylandWindowHandle::new(
            std::ptr::NonNull::new(self.surface_ptr).ok_or(HandleError::Unavailable)?,
        ));
        unsafe { Ok(WindowHandle::borrow_raw(raw)) }
    }
}

/// Helper function to detect the initial display scale factor from Wayland output state.
/// Iterates over all active outputs and returns the maximum scale factor found (defaulting to 1.0).
pub fn detect_scale_factor(output_state: &OutputState) -> f64 {
    let mut max_scale = 1.0;
    for output in output_state.outputs() {
        if let Some(info) = output_state.info(&output) {
            let scale = info.scale_factor as f64;
            if scale > max_scale {
                max_scale = scale;
            }
        }
    }
    max_scale
}

/// Helper to convert a logical pointer position (from Wayland/SCTK events)
/// to physical pixel coordinates based on the display scale factor.
pub fn scale_pointer_pos(pos: (f64, f64), scale: f64) -> (f32, f32) {
    ((pos.0 * scale) as f32, (pos.1 * scale) as f32)
}

#[macro_export]
macro_rules! delegate_wl_callback {
    ($name:ty) => {
        impl wayland_client::Dispatch<wayland_client::protocol::wl_callback::WlCallback, ()> for $name {
            fn event(
                state: &mut Self,
                _proxy: &wayland_client::protocol::wl_callback::WlCallback,
                event: wayland_client::protocol::wl_callback::Event,
                _data: &(),
                _conn: &wayland_client::Connection,
                _qh: &wayland_client::QueueHandle<Self>,
            ) {
                if let wayland_client::protocol::wl_callback::Event::Done { .. } = event {
                    state.frame_callback_pending = false;
                }
            }
        }
    };
}

