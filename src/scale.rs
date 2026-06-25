use std::sync::RwLock;

static SCALE_FACTOR: RwLock<f32> = RwLock::new(1.0);
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
