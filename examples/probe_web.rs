//! The renderer probe's scene through the WebGPU renderer, in a browser —
//! see `examples/probe/scene.rs`. Built for wasm32 as a cdylib and bound with
//! wasm-bindgen; a page calls `probe(canvas, fonts)` and gets the frame back
//! as RGBA8 bytes, the canvas's own pixels read back from the GPU.
//!
//! On a native target this example is empty.

#[cfg(target_arch = "wasm32")]
#[path = "probe/scene.rs"]
mod scene;

#[cfg(target_arch = "wasm32")]
mod web {
    use super::scene::{ProbeApp, H, W};
    use cce_ui::backend::frame::build_frame;
    use cce_ui::engine::{AppSender, Application, LogicalSize};
    use cce_ui::web::WebRenderer;
    use wasm_bindgen::prelude::*;

    /// Render the probe scene into `canvas` with the fonts in `fonts` (each a
    /// font file's bytes) and return the frame as RGBA8.
    #[wasm_bindgen]
    pub async fn probe(canvas: web_sys::HtmlCanvasElement, fonts: js_sys::Array) -> Result<js_sys::Uint8Array, JsValue> {
        std::panic::set_hook(Box::new(|info| web_sys::console::error_1(&info.to_string().into())));

        let mut db = cce_ui::cosmic_text::fontdb::Database::new();
        for font in fonts.iter() {
            db.load_font_data(js_sys::Uint8Array::new(&font).to_vec());
        }
        let mut fs = cce_ui::cosmic_text::FontSystem::new_with_locale_and_db("en-US".into(), db);
        let mut swash = cce_ui::cosmic_text::SwashCache::new();

        let mut renderer = WebRenderer::new(canvas).await?;
        renderer.resize(W, H);
        cce_ui::scale::set_scale_factor(1.0);
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut app = ProbeApp::create(AppSender::from(tx));

        let mut owed = true;
        let mut items = Vec::new();
        let size = LogicalSize::new(W as f32, H as f32);
        let frame = build_frame(&mut app, &mut fs, size, 1.0, &mut owed, &mut items);
        if frame.dl_text {
            let spans = frame.text_spans(&items);
            renderer.prepare_text(&mut fs, &mut swash, &spans);
        }
        renderer.capture_next_frame();
        renderer.draw_frame_2d(frame.frame2d())?;
        let capture = renderer.take_capture().await?.ok_or("no frame was captured")?;
        Ok(js_sys::Uint8Array::from(&capture.rgba[..]))
    }
}
