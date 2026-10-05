//! The 3D probe (`probe3d/scene.rs`) on the WebGPU renderer, in a browser:
//! the browser shell runs it (`web::run`), and `capture()` reads a frame
//! back. A cdylib for wasm-bindgen, served by `scripts/web-probe/probe3d`;
//! natively it is empty.

#[cfg(target_arch = "wasm32")]
#[path = "probe3d/scene.rs"]
mod scene;

#[cfg(target_arch = "wasm32")]
mod web {
    use cce_ui::web::{Fonts, Sizing};
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    pub async fn start(canvas: web_sys::HtmlCanvasElement, fonts: js_sys::Array, _families: String, _fill: bool) -> Result<(), JsValue> {
        std::panic::set_hook(Box::new(|info| web_sys::console::error_1(&info.to_string().into())));
        let fonts = Fonts::new(fonts.iter().map(|f| js_sys::Uint8Array::new(&f).to_vec()).collect());
        cce_ui::web::run::<super::scene::Probe3d>(canvas, fonts, Sizing::Page).await
    }

    #[wasm_bindgen]
    pub async fn capture() -> Result<js_sys::Uint8Array, JsValue> {
        let frame = cce_ui::web::capture().await?;
        Ok(js_sys::Uint8Array::from(&frame.rgba[..]))
    }
}
