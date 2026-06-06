use std::sync::RwLock;

static SCALE_FACTOR: RwLock<f32> = RwLock::new(1.0);

pub fn scale_factor() -> f32 {
    *SCALE_FACTOR.read().unwrap()
}

pub fn set_scale_factor(scale: f32) {
    if let Ok(mut lock) = SCALE_FACTOR.write() {
        *lock = scale;
    }
}
