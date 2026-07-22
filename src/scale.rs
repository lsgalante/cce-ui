use std::sync::{OnceLock, RwLock};

static SCALE_FACTOR: RwLock<f32> = RwLock::new(1.0);
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

pub fn scale_factor() -> f32 {
    *SCALE_FACTOR.read().unwrap()
}

pub fn set_scale_factor(scale: f32) {
    if let Ok(mut lock) = SCALE_FACTOR.write() {
        *lock = scale;
    }
}

pub fn app_id() -> String {
    APP_ID.read().unwrap().clone()
}

pub fn set_app_id(id: String) {
    if let Ok(mut lock) = APP_ID.write() {
        *lock = id;
    }
}

pub fn is_fullscreen() -> bool {
    *FULLSCREEN.read().unwrap()
}

pub fn set_fullscreen(fs: bool) {
    if let Ok(mut lock) = FULLSCREEN.write() {
        *lock = fs;
    }
}

pub fn is_maximized() -> bool {
    *MAXIMIZED.read().unwrap()
}

pub fn set_maximized(m: bool) {
    if let Ok(mut lock) = MAXIMIZED.write() {
        *lock = m;
    }
}
