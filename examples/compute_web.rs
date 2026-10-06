//! The compute probe on WebGPU (`web::ComputeDevice`), in a browser: the
//! shared jobs (`compute_probe/jobs.rs`), one line each, returned to the
//! page. A cdylib for wasm-bindgen; natively it is empty.

#[cfg(target_arch = "wasm32")]
#[path = "compute_probe/jobs.rs"]
mod jobs;

#[cfg(target_arch = "wasm32")]
mod web {
    use super::jobs;
    use wasm_bindgen::prelude::*;

    /// The device's name and every job's line, newline-separated.
    #[wasm_bindgen]
    pub async fn run_jobs() -> Result<String, JsValue> {
        std::panic::set_hook(Box::new(|info| web_sys::console::error_1(&info.to_string().into())));
        let mut dev = cce_ui::web::ComputeDevice::new().await?;
        web_sys::console::log_1(&dev.device_name().into());
        Ok(crate::compute_jobs!(dev, .await).join("\n"))
    }
}
