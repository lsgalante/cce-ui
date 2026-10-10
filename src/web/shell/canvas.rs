//! `WebShell`: the shell's state over a canvas — the app, the driver, the renderer, the fonts —
//! measuring the canvas, the keyboard sink's place at the caret, the cursor, and the `Shell` impl the
//! shared `Pacer` drives.

use super::*;

/// The page's side of the run loop, as the Wayland shell's `EngineState` is
/// the compositor's.
pub(super) struct WebShell<A: Application> {
    pub(super) app: A,
    pub(super) driver: Driver,
    pub(super) redraw: bool,
    pub(super) exit: bool,
    pub(super) rx: Receiver<A::Message>,
    pub(super) renderer: WebRenderer,
    pub(super) fs: cosmic_text::FontSystem,
    pub(super) swash: cosmic_text::SwashCache,
    /// The frame's display-list text, shaped by [`build_frame`].
    pub(super) items: Vec<DlText>,
    pub(super) damage_owed: bool,
    pub(super) canvas: HtmlCanvasElement,
    pub(super) sizing: Sizing,
    /// The canvas's CSS size, and `devicePixelRatio`, as last measured.
    pub(super) logical: (f32, f32),
    pub(super) scale: f64,
    pub(super) just_configured: bool,
    pub(super) cursor: CursorIcon,
    /// Command is the shortcut key here: ⌘Z is undo, as every Mac app has it.
    pub(super) mac: bool,
    /// The hidden textarea that holds the keyboard (see the module doc), and
    /// where it was last put (page px: left, top, height).
    pub(super) sink: HtmlTextAreaElement,
    pub(super) sink_at: Option<(f64, f64, f64)>,
}

impl<A: Application> WebShell<A> {
    /// Read the canvas's box and the pixel ratio, and size the drawing
    /// buffer to them. Whether either changed.
    pub(super) fn measure(&mut self) -> bool {
        let dpr = device_pixel_ratio();
        let w = self.canvas.client_width().max(0) as f32;
        let h = self.canvas.client_height().max(0) as f32;
        if (w, h) == self.logical && dpr == self.scale {
            return false;
        }
        self.logical = (w, h);
        if dpr != self.scale {
            self.scale = dpr;
            crate::scale::set_scale_factor(dpr as f32);
        }
        let (pw, ph) = self.physical();
        if self.renderer.size() != (pw, ph) {
            self.renderer.resize(pw, ph);
        }
        self.redraw = true;
        true
    }

    pub(super) fn physical(&self) -> (u32, u32) {
        let s = self.scale as f32;
        ((self.logical.0 * s).round().max(1.0) as u32, (self.logical.1 * s).round().max(1.0) as u32)
    }

    pub(super) fn size(&self) -> LogicalSize {
        LogicalSize::new(self.logical.0, self.logical.1)
    }

    /// Hand the messages posted since the last turn to `update`, as the
    /// Wayland loop's channel source does between dispatches.
    pub(super) fn drain_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            let mut rebuild = false;
            self.app.update(msg, &mut rebuild, &mut self.exit);
            self.redraw |= rebuild;
        }
    }

    pub(super) fn mods_from(&self, ctrl: bool, shift: bool, alt: bool, meta: bool) -> Modifiers {
        if self.mac {
            Modifiers { ctrl: ctrl || meta, shift, alt, logo: false }
        } else {
            Modifiers { ctrl, shift, alt, logo: meta }
        }
    }

    pub(super) fn sync_mods(&mut self, mods: Modifiers) {
        if mods != self.driver.mods {
            self.driver.set_modifiers(&mut self.app, mods);
        }
    }

    pub(super) fn pointer_pos(&mut self, e: &PointerEvent) -> LogicalPosition {
        let (x, y) = (e.offset_x() as f32, e.offset_y() as f32);
        self.driver.cursor_pos = (x, y);
        LogicalPosition::new(x, y)
    }

    /// Put the keyboard sink at the editing widget's caret (the canvas's
    /// corner when nothing is editing), so the input method's candidates
    /// open there; and cancel a composition a widget dropped. Inside the
    /// turn: the shell is borrowed, so the blur and focus this raises (and
    /// the composition's end) reach no handler.
    pub(super) fn place_sink(&mut self) {
        let r = self.canvas.get_bounding_client_rect();
        let (left, top, h) = match crate::ime::caret() {
            Some([x, y, _, h]) => (r.left() + x as f64, r.top() + y as f64, (h as f64).max(8.0)),
            None => (r.left(), r.top(), 16.0),
        };
        if self.sink_at != Some((left, top, h)) {
            self.sink_at = Some((left, top, h));
            let st = self.sink.style();
            let _ = st.set_property("left", &format!("{left}px"));
            let _ = st.set_property("top", &format!("{top}px"));
            let _ = st.set_property("height", &format!("{h}px"));
            let _ = st.set_property("font-size", &format!("{}px", (h * 0.8).round()));
            let _ = st.set_property("line-height", &format!("{h}px"));
        }
        if crate::ime::take_reset() {
            let focused = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.active_element())
                .is_some_and(|a| a == *self.sink.unchecked_ref::<web_sys::Element>());
            self.sink.set_value("");
            if focused {
                let _ = self.sink.blur();
                let _ = self.sink.focus();
            }
        }
    }

    pub(super) fn update_cursor(&mut self) {
        let (x, y) = self.driver.cursor_pos;
        let icon = self.driver.cursor_icon_at(&self.app, x, y, self.size());
        if icon != self.cursor {
            self.cursor = icon;
            let _ = self.canvas.style().set_property("cursor", icon.name());
        }
    }
}

impl<A: Application> Shell for WebShell<A> {
    type App = A;

    fn turn(&mut self) -> (&mut Driver, Turn<'_, A>) {
        (&mut self.driver, Turn { app: &mut self.app, redraw: &mut self.redraw, exit: &mut self.exit })
    }

    fn app(&self) -> &A {
        &self.app
    }

    fn redraw(&mut self) -> &mut bool {
        &mut self.redraw
    }

    fn exit_requested(&self) -> bool {
        self.exit
    }

    fn take_just_configured(&mut self) -> bool {
        std::mem::replace(&mut self.just_configured, false)
    }

    fn request_size(&mut self, w: u32, h: u32) {
        if self.sizing == Sizing::App && (w as f32, h as f32) != self.logical {
            set_css_size(&self.canvas, w, h);
            self.measure();
        }
    }

    fn sync(&mut self) {
        // The page laid the canvas out anew, or moved it to another
        // display: a configure, which wins the next turn over the app's size.
        if self.measure() {
            self.just_configured = true;
        }
        if CAPTURES.with(|c| !c.borrow().is_empty()) {
            self.redraw = true;
        }
        // No popup surface to host the menu: it is drawn in the canvas and
        // kept inside it, frames asked for while a page turn animates.
        if context_menu::is_visible() {
            if context_menu::is_turning() {
                self.redraw = true;
            }
            context_menu::set_hosted(false);
            context_menu::constrain_to(0.0, 0.0, self.logical.0, self.logical.1);
        }
    }

    fn set_title(&mut self, title: &str) {
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            doc.set_title(title);
        }
    }

    fn frame_pending(&mut self) -> bool {
        false
    }

    fn configured(&self) -> bool {
        self.logical.0 > 0.0 && self.logical.1 > 0.0
    }

    fn present(&mut self, _fresh: bool) {
        let size = self.size();
        let frame = build_frame(&mut self.app, &mut self.fs, size, self.scale, &mut self.damage_owed, &mut self.items);
        if frame.dl_text {
            let spans = frame.text_spans(&self.items);
            self.renderer.prepare_text(&mut self.fs, &mut self.swash, &spans);
        }
        // The app's 3D scene, staged last, as the Wayland shell's
        // `stage_renderer`; true asks for another frame.
        if self.app.stage_3d(&mut self.renderer, size, self.scale) {
            self.redraw = true;
        }
        let waiting = CAPTURES.with(|c| std::mem::take(&mut *c.borrow_mut()));
        if !waiting.is_empty() {
            self.renderer.capture_next_frame();
        }
        match self.renderer.draw_frame_2d(frame.frame2d()) {
            Ok(()) => self.damage_owed = false,
            Err(e) => {
                web_sys::console::error_2(&"cce-ui: frame not drawn:".into(), &e);
                self.redraw = true;
            }
        }
        self.place_sink();
        if waiting.is_empty() {
            return;
        }
        let pending = self.renderer.take_pending_capture();
        wasm_bindgen_futures::spawn_local(async move {
            let got = match pending {
                Some(p) => p.read().await.ok(),
                None => None,
            };
            let value: JsValue = match got {
                Some(c) => js_sys::Array::of3(&c.width.into(), &c.height.into(), &js_sys::Uint8Array::from(&c.rgba[..])).into(),
                None => JsValue::NULL,
            };
            for resolve in waiting {
                let _ = resolve.call1(&JsValue::NULL, &value);
            }
        });
    }
}
