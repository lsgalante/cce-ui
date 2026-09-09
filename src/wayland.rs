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
    if let Some(forced) = crate::scale::forced_scale() {
        return forced as f64;
    }
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

/// The display metric (logical px per mm) for the output `detect_scale_factor`
/// chose — the same selection rule, so scale and metric describe one
/// display. Measured from the output's `wl_output` geometry (EDID physical
/// size, or the compositor's configured override in its place — the client
/// cannot tell the two apart, and reports "measured" for both; `ccectl
/// outputs` says which) against its logical size: xdg-output's when the
/// compositor sends one (exact under fractional scale), else the current
/// mode divided by the integer `wl_output` scale. Outputs with no physical
/// size (headless, a virtual output, an EDID-less projector) or an
/// implausible one fall back to the assumed CSS metric, flagged as such.
pub fn detect_metric(output_state: &OutputState, scale: f64) -> crate::units::Metric {
    use crate::units::{Metric, MetricSource};
    let scale_f = scale as f32;
    let mut best: Option<Metric> = None;
    for output in output_state.outputs() {
        let Some(info) = output_state.info(&output) else { continue };
        if (info.scale_factor as f64) < scale && crate::scale::forced_scale().is_none() {
            // Not the display the scale was taken from.
            continue;
        }
        let (mm_w, mm_h) = info.physical_size;
        if mm_w <= 0 || mm_h <= 0 {
            continue;
        }
        let logical = match info.logical_size {
            Some((w, h)) if w > 0 && h > 0 => (w as f32, h as f32),
            _ => {
                let Some(mode) = info.modes.iter().find(|m| m.current) else { continue };
                let s = (info.scale_factor.max(1)) as f32;
                (mode.dimensions.0 as f32 / s, mode.dimensions.1 as f32 / s)
            }
        };
        if let Some(m) = Metric::from_sizes(scale_f, logical, (mm_w as f32, mm_h as f32), MetricSource::Measured) {
            best = Some(m);
            break;
        }
    }
    best.unwrap_or_else(|| Metric::assumed(scale_f))
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

