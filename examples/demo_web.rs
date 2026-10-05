//! The reference app, `src/main.rs`, in a browser: the same `DemoApp` the
//! Wayland shell runs, run on a canvas by `cce_ui::web::run`. Built for
//! wasm32 as a cdylib and bound with wasm-bindgen; a page calls
//! `start(canvas, fonts, families, fill)` — `fonts` each a font file's bytes,
//! `families` the generic serif / sans-serif / monospace families as
//! "serif,sans,mono" (empty for the defaults; any part may be empty), `fill`
//! true to let the page size the canvas (`Sizing::Page`) rather than the app
//! (`Sizing::App`) — and `capture()` for the next frame's pixels.
//! `scripts/web-probe/demo` serves it and drives it.
//!
//! On a native target this example is empty.

#[cfg(target_arch = "wasm32")]
#[allow(dead_code)]
#[path = "../src/main.rs"]
mod demo;

#[cfg(target_arch = "wasm32")]
mod web {
    use cce_ui::web::{Fonts, Sizing};
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    pub async fn start(canvas: web_sys::HtmlCanvasElement, fonts: js_sys::Array, families: String, fill: bool) -> Result<(), JsValue> {
        std::panic::set_hook(Box::new(|info| web_sys::console::error_1(&info.to_string().into())));
        let mut fonts = Fonts::new(fonts.iter().map(|f| js_sys::Uint8Array::new(&f).to_vec()).collect());
        let mut names = families.split(',').map(|f| Some(f.trim()).filter(|f| !f.is_empty()).map(String::from));
        fonts.serif = names.next().flatten();
        fonts.sans_serif = names.next().flatten();
        fonts.monospace = names.next().flatten();
        let sizing = if fill { Sizing::Page } else { Sizing::App };
        cce_ui::web::run::<super::demo::DemoApp>(canvas, fonts, sizing).await
    }

    /// The next frame, read back from the GPU as RGBA8 (`cce_ui::web::capture`).
    #[wasm_bindgen]
    pub async fn capture() -> Result<js_sys::Uint8Array, JsValue> {
        let frame = cce_ui::web::capture().await?;
        Ok(js_sys::Uint8Array::from(&frame.rgba[..]))
    }
}
