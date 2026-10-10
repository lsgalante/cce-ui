use std::sync::{OnceLock, RwLock};

#[cfg(not(test))]
static SCALE_FACTOR: RwLock<f32> = RwLock::new(1.0);

// Under `cfg(test)` the process-wide scale is per thread: no test enters a window but
// `window_state`'s, so every shaping test reads this value, and one test setting it would
// rescale another's text mid-shape (`prepare_text` once read 1.0 for its key and 1.5 for
// its buffer, and kept offsets in physical px).
#[cfg(test)]
thread_local! {
    static SCALE_FACTOR: std::cell::Cell<f32> = const { std::cell::Cell::new(1.0) };
}
static FORCED_SCALE: OnceLock<Option<f32>> = OnceLock::new();

/// `CCE_FORCE_SCALE`: HiDPI override for foreign compositors that report a
/// scale-1 output (the display-manager greeter under cage). In forced mode the
/// compositor's logical coordinate space is treated as physical: configure
/// sizes and pointer positions are divided by this factor, layout/rendering
/// scale up by it, and the surface keeps `buffer_scale` 1 so the buffer still
/// matches the size the compositor configured. Unset (the normal case, under
/// cce-fx) this is `None` and nothing changes.
pub fn forced_scale() -> Option<f32> {
    *FORCED_SCALE.get_or_init(|| {
        std::env::var("CCE_FORCE_SCALE")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|s| *s > 0.0 && (*s - 1.0).abs() > 0.001)
    })
}
static APP_ID: RwLock<String> = RwLock::new(String::new());
static FULLSCREEN: RwLock<bool> = RwLock::new(false);
static MAXIMIZED: RwLock<bool> = RwLock::new(false);

// A window's properties are its own (`crate::window_state::Props`): read from the window
// whose code is running, else from these process-wide values, which every setter also
// writes — the last any window set, what a worker thread reads (`docs/rfc-global-state.md`,
// phase 4).

/// The HiDPI scale of the window being drawn (off a window's thread, the last one set).
pub fn scale_factor() -> f32 {
    crate::window_state::entered(|w| w.props.borrow().scale).unwrap_or_else(process_scale_factor)
}

#[cfg(not(test))]
pub(crate) fn process_scale_factor() -> f32 {
    *SCALE_FACTOR.read().unwrap()
}

#[cfg(test)]
pub(crate) fn process_scale_factor() -> f32 {
    SCALE_FACTOR.with(|s| s.get())
}

pub fn set_scale_factor(scale: f32) {
    crate::window_state::entered(|w| w.props.borrow_mut().scale = scale);
    #[cfg(not(test))]
    if let Ok(mut lock) = SCALE_FACTOR.write() {
        *lock = scale;
    }
    #[cfg(test)]
    SCALE_FACTOR.with(|s| s.set(scale));
}

pub fn app_id() -> String {
    crate::window_state::entered(|w| w.props.borrow().app_id.clone()).unwrap_or_else(process_app_id)
}

pub(crate) fn process_app_id() -> String {
    APP_ID.read().unwrap().clone()
}

pub fn set_app_id(id: String) {
    crate::window_state::entered(|w| w.props.borrow_mut().app_id = id.clone());
    if let Ok(mut lock) = APP_ID.write() {
        *lock = id;
    }
}

pub fn is_fullscreen() -> bool {
    crate::window_state::entered(|w| w.props.borrow().fullscreen).unwrap_or_else(|| *FULLSCREEN.read().unwrap())
}

pub fn set_fullscreen(fs: bool) {
    crate::window_state::entered(|w| w.props.borrow_mut().fullscreen = fs);
    if let Ok(mut lock) = FULLSCREEN.write() {
        *lock = fs;
    }
}

pub fn is_maximized() -> bool {
    crate::window_state::entered(|w| w.props.borrow().maximized).unwrap_or_else(|| *MAXIMIZED.read().unwrap())
}

pub fn set_maximized(m: bool) {
    crate::window_state::entered(|w| w.props.borrow_mut().maximized = m);
    if let Ok(mut lock) = MAXIMIZED.write() {
        *lock = m;
    }
}
