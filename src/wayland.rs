use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle, WindowHandle,
};

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
