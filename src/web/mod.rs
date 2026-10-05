//! The browser's side of cce-ui: a WebGPU renderer that draws the same
//! [`Frame2D`](crate::draw::Frame2D)s the Vulkan renderer does, from the same
//! shaders (`draw::shaders`), atlas (`draw::glyphs`) and image queue
//! (`draw::images`), and the shell that runs an `Application` on a canvas
//! with it ([`run`]) — the page's events in through the shared `Driver`,
//! `Pacer::turn` on animation frames — and a [`ComputeDevice`] that runs
//! `crate::compute` jobs on WebGPU.
//!
//! wasm32 only, and it needs web-sys's WebGPU bindings switched on
//! (`--cfg=web_sys_unstable_apis`, set for this crate in `.cargo/config.toml`).

mod compute;
mod renderer;
mod scene;
mod shell;

pub use compute::ComputeDevice;
pub use renderer::{Capture, PendingCapture, WebRenderer};
pub use shell::{capture, run, Fonts, Sizing};

use wasm_bindgen::JsValue;

/// Ask the browser for a WebGPU device, with `limits` (WebGPU limit names)
/// raised to what the adapter offers — a device is created at the spec's
/// defaults otherwise, which are below what most adapters can do. What the
/// device rejects later, and why it was lost, goes to the console: a WebGPU
/// validation error is otherwise silent.
pub(crate) async fn request_device(
    limits: &[&str],
) -> Result<(web_sys::Gpu, web_sys::GpuAdapter, web_sys::GpuDevice), JsValue> {
    let window = web_sys::window().ok_or("no window")?;
    let gpu = window.navigator().gpu();
    let adapter = gpu
        .request_adapter()
        .await?
        .into_option()
        .ok_or("this browser offers no WebGPU adapter")?;
    let desc = web_sys::GpuDeviceDescriptor::new();
    if !limits.is_empty() {
        let offered = adapter.limits();
        let required = js_sys::Object::new();
        for name in limits {
            let v = js_sys::Reflect::get(&offered, &JsValue::from_str(name))?;
            if !v.is_undefined() {
                js_sys::Reflect::set(&required, &JsValue::from_str(name), &v)?;
            }
        }
        js_sys::Reflect::set(&desc, &JsValue::from_str("requiredLimits"), &required)?;
    }
    let device: web_sys::GpuDevice = adapter.request_device_with_descriptor(&desc).await?;
    js_sys::Function::new_with_args(
        "d",
        "d.onuncapturederror = (e) => console.error('cce-ui WebGPU:', e.error.message); \
         d.lost.then((i) => console.error('cce-ui WebGPU device lost:', i.reason, i.message));",
    )
    .call1(&JsValue::NULL, &device)?;
    Ok((gpu, adapter, device))
}
