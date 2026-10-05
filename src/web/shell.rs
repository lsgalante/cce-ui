//! The browser shell: an [`Application`] run on a `<canvas>`, the way the
//! Wayland shell runs one on a surface. It is the third piece of the run
//! loop over the shared two — [`Driver`] routes what the page's events mean,
//! [`Pacer::turn`] decides what a turn does — and what is left here is the
//! page's side of both:
//!
//! - **Events in.** Pointer, wheel, key and focus events on the canvas,
//!   mapped into driver calls in cce-ui's terms (`map_key`, `wheel_frame`).
//!   A page has no grabs, so a press on what would be a CSD border is the
//!   app's ([`PressSite::can_grab`] false).
//! - **Turns.** One turn per animation frame while anything moves (the
//!   pacer's ACTIVE cadence), a timer while idle; any event, and any message
//!   on the app's [`AppSender`](crate::engine::AppSender), wakes the loop
//!   for the next frame. Presenting happens inside an animation-frame
//!   callback, which is the browser's own frame pacing — there is no
//!   outstanding frame to wait on ([`Shell::frame_pending`] is false).
//! - **Frames out.** [`build_frame`] at the canvas's CSS size and the page's
//!   `devicePixelRatio`, drawn by the [`WebRenderer`].
//!
//! **The keyboard is a hidden `<textarea>`'s**, not the canvas's: a page can
//! compose input-method text (Japanese, Chinese, Korean, a dead key, an
//! emoji panel) only into an editable element. It takes the focus a press
//! on the canvas would have given the canvas (and the canvas, focused some
//! other way, hands it over), its key events are the app's as the canvas's
//! were, and its composition is the input method's: a key the input method
//! takes (`Process`, keyCode 229, or one sent mid-composition) is left to
//! it, `input` events while composing are the composition
//! (`Driver::preedit`), `compositionend` its commit (`Driver::commit_text`),
//! and text that arrives with no composition (an emoji panel, dictation) is
//! committed as it comes. After each frame it is moved to the editing
//! widget's caret (`ime::caret`), where the input method puts its
//! candidates. A widget that drops a composition (`ime::take_reset`) has it
//! cancelled by taking the focus off the textarea and back, inside the turn,
//! where the events that raises are not the app's.
//!
//! **The clipboard** comes through the page's clipboard events, since a page
//! may read the clipboard only inside a `paste` event: a ⌘/Ctrl+V is held
//! back from the app until its `paste` event has handed over the text (or,
//! if none comes, until the task after), so the widget that pastes on it
//! reads that text (`widget::clipboard`). A ⌘/Ctrl+C or X reaches the app at
//! once, and the `copy` / `cut` event it raises carries whatever the app
//! copied. The three keys' defaults are the only ones the canvas lets the
//! page have.
//!
//! The page owns the canvas's place in it; [`Sizing`] says who owns its
//! size. There is no context-menu popup surface here: the menu is drawn in
//! the canvas and kept inside it (`context_menu::constrain_to`), as on a
//! layer surface.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use cursor_icon::CursorIcon;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{
    AddEventListenerOptions, ClipboardEvent, CompositionEvent, FocusEvent, HtmlCanvasElement, HtmlTextAreaElement, InputEvent,
    KeyboardEvent, PointerEvent, WheelEvent,
};

use super::renderer::{Capture, WebRenderer};
use crate::backend::app::{set_wake, AppSender, Application, LogicalPosition, LogicalSize};
use crate::backend::dom::{clipboard_key, map_key, utf16_range_to_bytes, wheel_frame, ClipKey};
use crate::backend::driver::{Driver, Modifiers, PressSite, ScrollFrame, ScrollSource, Turn};
use crate::backend::frame::build_frame;
use crate::backend::shell::{Pacer, Shell, Step, ACTIVE_DISPATCH};
use crate::widget::{clipboard, context_menu, ElementState, Key, MouseButton};
use crate::backend::text::DlText;

/// Who decides the canvas's size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sizing {
    /// The app does, as it sizes a window: the canvas is given the app's
    /// `WindowSettings` size at start and its `desired_size` after (CSS px).
    App,
    /// The page does: the canvas is wherever its CSS box puts it, the app's
    /// size requests are not honoured (a tiling compositor's answer), and a
    /// change of the box is a resize.
    Page,
}

/// The fonts an app is run with: a page has no font directory and no
/// fontconfig, so it says both what there is and what the generic families
/// are. A generic left `None` is the first family the set has of a list of
/// well-known ones (DejaVu, Noto, FreeFont, …).
#[derive(Debug, Clone, Default)]
pub struct Fonts {
    /// Font files' bytes (TrueType, OpenType, collections), in the order a
    /// directory scan would find them: where text falls back to the first
    /// face holding a glyph, order decides it.
    pub files: Vec<Vec<u8>>,
    pub serif: Option<String>,
    pub sans_serif: Option<String>,
    pub monospace: Option<String>,
}

impl Fonts {
    pub fn new(files: Vec<Vec<u8>>) -> Self {
        Self { files, ..Default::default() }
    }
}

/// Run `A` on `canvas`, its text set in `fonts`. Returns once the app is
/// up; the loop runs on the page's callbacks from then on, until the app
/// asks to exit.
///
/// One app per page: the toolkit's context menu is one per thread, and so is
/// [`set_wake`].
pub async fn run<A: Application>(canvas: HtmlCanvasElement, fonts: Fonts, sizing: Sizing) -> Result<(), JsValue> {
    // Every font database the toolkit builds from here on — this one, its
    // own for widget geometry, and the measuring one — loads the page's.
    crate::page_fonts::provide(fonts.files, fonts.serif, fonts.sans_serif, fonts.monospace);
    let fs = crate::create_font_system();

    let mut renderer = WebRenderer::new(canvas.clone()).await?;
    // The scale is known before the app is built, as the Wayland shell sets
    // it before `create`: a widget may read it as it is made.
    let dpr = device_pixel_ratio();
    crate::scale::set_scale_factor(dpr as f32);
    let (tx, rx) = std::sync::mpsc::channel();
    let mut app = A::create(AppSender::from(tx));
    // The app's persistent GPU resources, as the Wayland shell's
    // `renderer_init` makes them: once, before the first frame.
    app.init_3d(&mut renderer);
    let settings = app.settings();
    crate::scale::set_app_id(settings.app_id.clone());
    if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
        doc.set_title(&settings.title);
    }

    let style = canvas.style();
    if sizing == Sizing::App {
        set_css_size(&canvas, settings.width, settings.height);
    }
    // The canvas can be focused (it hands the focus to the keyboard sink),
    // draws no focus outline of its own, and keeps touches for the app
    // rather than panning the page.
    canvas.set_tab_index(0);
    let _ = style.set_property("outline", "none");
    let _ = style.set_property("touch-action", "none");
    let sink = keyboard_sink()?;

    let shell = WebShell {
        app,
        driver: Driver::new(),
        redraw: true,
        exit: false,
        rx,
        renderer,
        fs,
        swash: cosmic_text::SwashCache::new(),
        items: Vec::new(),
        damage_owed: true,
        canvas: canvas.clone(),
        sizing,
        logical: (0.0, 0.0),
        scale: dpr,
        just_configured: false,
        cursor: CursorIcon::Default,
        mac: is_mac(),
        sink: sink.clone(),
        sink_at: None,
    };
    let lp = Rc::new(Loop {
        shell: RefCell::new(shell),
        pacer: RefCell::new(Pacer::new(settings.title)),
        sched: RefCell::new(Sched::default()),
        frame_cb: RefCell::new(None),
        timer_cb: RefCell::new(None),
        finger_end_cb: RefCell::new(None),
        finger_timer: Cell::new(None),
        held_paste: RefCell::new(None),
        paste_cb: RefCell::new(None),
    });
    lp.shell.borrow_mut().measure();
    lp.shell.borrow_mut().just_configured = true;
    Loop::install(&lp, &canvas, &sink)?;
    lp.wake();
    Ok(())
}

thread_local! {
    /// Callers of [`capture`] waiting on the next frame: each a promise's resolve.
    static CAPTURES: RefCell<Vec<js_sys::Function>> = const { RefCell::new(Vec::new()) };
}

/// The next frame the shell draws, read back from the GPU. A fresh frame is
/// asked for, as an input event would ask, and this resolves once it is
/// drawn. For screenshot tests: a headless browser compositing in software
/// leaves a WebGPU canvas out of its page screenshots (and `toDataURL`).
pub async fn capture() -> Result<Capture, JsValue> {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| CAPTURES.with(|c| c.borrow_mut().push(resolve)));
    crate::backend::app::wake();
    let got = wasm_bindgen_futures::JsFuture::from(promise).await?;
    let got: js_sys::Array = got.dyn_into().map_err(|_| JsValue::from_str("cce-ui: the frame was not captured"))?;
    Ok(Capture {
        width: got.get(0).as_f64().unwrap_or(0.0) as u32,
        height: got.get(1).as_f64().unwrap_or(0.0) as u32,
        rgba: js_sys::Uint8Array::new(&got.get(2)).to_vec(),
    })
}

/// The page's side of the run loop, as the Wayland shell's `EngineState` is
/// the compositor's.
struct WebShell<A: Application> {
    app: A,
    driver: Driver,
    redraw: bool,
    exit: bool,
    rx: Receiver<A::Message>,
    renderer: WebRenderer,
    fs: cosmic_text::FontSystem,
    swash: cosmic_text::SwashCache,
    /// The frame's display-list text, shaped by [`build_frame`].
    items: Vec<DlText>,
    damage_owed: bool,
    canvas: HtmlCanvasElement,
    sizing: Sizing,
    /// The canvas's CSS size, and `devicePixelRatio`, as last measured.
    logical: (f32, f32),
    scale: f64,
    just_configured: bool,
    cursor: CursorIcon,
    /// Command is the shortcut key here: ⌘Z is undo, as every Mac app has it.
    mac: bool,
    /// The hidden textarea that holds the keyboard (see the module doc), and
    /// where it was last put (page px: left, top, height).
    sink: HtmlTextAreaElement,
    sink_at: Option<(f64, f64, f64)>,
}

impl<A: Application> WebShell<A> {
    /// Read the canvas's box and the pixel ratio, and size the drawing
    /// buffer to them. Whether either changed.
    fn measure(&mut self) -> bool {
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

    fn physical(&self) -> (u32, u32) {
        let s = self.scale as f32;
        ((self.logical.0 * s).round().max(1.0) as u32, (self.logical.1 * s).round().max(1.0) as u32)
    }

    fn size(&self) -> LogicalSize {
        LogicalSize::new(self.logical.0, self.logical.1)
    }

    /// Hand the messages posted since the last turn to `update`, as the
    /// Wayland loop's channel source does between dispatches.
    fn drain_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            let mut rebuild = false;
            self.app.update(msg, &mut rebuild, &mut self.exit);
            self.redraw |= rebuild;
        }
    }

    fn mods_from(&self, ctrl: bool, shift: bool, alt: bool, meta: bool) -> Modifiers {
        if self.mac {
            Modifiers { ctrl: ctrl || meta, shift, alt, logo: false }
        } else {
            Modifiers { ctrl, shift, alt, logo: meta }
        }
    }

    fn sync_mods(&mut self, mods: Modifiers) {
        if mods != self.driver.mods {
            self.driver.set_modifiers(&mut self.app, mods);
        }
    }

    fn pointer_pos(&mut self, e: &PointerEvent) -> LogicalPosition {
        let (x, y) = (e.offset_x() as f32, e.offset_y() as f32);
        self.driver.cursor_pos = (x, y);
        LogicalPosition::new(x, y)
    }

    /// Put the keyboard sink at the editing widget's caret (the canvas's
    /// corner when nothing is editing), so the input method's candidates
    /// open there; and cancel a composition a widget dropped. Inside the
    /// turn: the shell is borrowed, so the blur and focus this raises (and
    /// the composition's end) reach no handler.
    fn place_sink(&mut self) {
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

    fn update_cursor(&mut self) {
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

/// When the next turn is due.
#[derive(Default)]
struct Sched {
    /// An animation frame is requested: the next turn is in it.
    frame: Option<i32>,
    /// An idle timer is set; it requests the frame when it fires.
    timer: Option<i32>,
    /// The app exited: nothing turns again.
    stopped: bool,
}

/// The shell, the pacer and the schedule, shared by the page's callbacks.
/// The schedule is its own cell, borrowed only briefly and never across a
/// call into the app, so a message the app sends mid-turn can wake the loop.
struct Loop<A: Application> {
    shell: RefCell<WebShell<A>>,
    pacer: RefCell<Pacer>,
    sched: RefCell<Sched>,
    frame_cb: RefCell<Option<Closure<dyn FnMut(f64)>>>,
    timer_cb: RefCell<Option<Closure<dyn FnMut()>>>,
    finger_end_cb: RefCell<Option<Closure<dyn FnMut()>>>,
    /// The lift timer the last finger frame set; the next frame cancels it.
    finger_timer: Cell<Option<i32>>,
    /// A ⌘/Ctrl+V held back until its `paste` event, and the timer that
    /// lets it through if no event comes.
    held_paste: RefCell<Option<HeldKey>>,
    paste_cb: RefCell<Option<Closure<dyn FnMut()>>>,
}

/// A key press, kept to dispatch later: the key, its text and the
/// modifiers it came with (ctrl, shift, alt, meta).
struct HeldKey {
    key: Key,
    text: Option<String>,
    mods: (bool, bool, bool, bool),
}

/// A browser reports no lift for a two-finger scroll: this long without a
/// finger frame is one, and ends the gesture (`ScrollPhase::FingerEnd`) so a
/// flick coasts and a side swipe readies its next turn.
const FINGER_LIFT: Duration = Duration::from_millis(120);

impl<A: Application> Loop<A> {
    /// Turn the loop at the next animation frame.
    fn wake(&self) {
        let mut s = self.sched.borrow_mut();
        if s.stopped || s.frame.is_some() {
            return;
        }
        let win = window();
        if let Some(t) = s.timer.take() {
            win.clear_timeout_with_handle(t);
        }
        let cb = self.frame_cb.borrow();
        if let Some(cb) = cb.as_ref() {
            s.frame = win.request_animation_frame(cb.as_ref().unchecked_ref()).ok();
        }
    }

    /// One turn, in an animation frame.
    fn on_frame(&self) {
        self.sched.borrow_mut().frame = None;
        let step = {
            let mut shell = self.shell.borrow_mut();
            shell.drain_messages();
            self.pacer.borrow_mut().turn(&mut *shell)
        };
        match step {
            Step::Exit => {
                self.sched.borrow_mut().stopped = true;
                set_wake(None);
                self.shell.borrow_mut().app.on_exit();
            }
            Step::Sleep(d) if d <= ACTIVE_DISPATCH => self.wake(),
            Step::Sleep(d) => {
                let mut s = self.sched.borrow_mut();
                let cb = self.timer_cb.borrow();
                // A frame already requested (the app posted itself a message
                // mid-turn) turns sooner than any timer would.
                if let (None, None, Some(cb)) = (s.frame, s.timer, cb.as_ref()) {
                    s.timer = window()
                        .set_timeout_with_callback_and_timeout_and_arguments_0(cb.as_ref().unchecked_ref(), d.as_millis() as i32)
                        .ok();
                }
            }
        }
    }

    /// Dispatch an event to the shell, then turn soon. Input after the exit
    /// is dropped.
    fn event(&self, f: impl FnOnce(&mut WebShell<A>)) {
        if self.sched.borrow().stopped {
            return;
        }
        // A page event fired from inside a turn (a focus change the app's
        // own DOM call caused) finds the shell borrowed: it is dropped
        // rather than panicking the page.
        let Ok(mut shell) = self.shell.try_borrow_mut() else { return };
        f(&mut shell);
        drop(shell);
        self.wake();
    }

    /// The callbacks: the loop's own two, the wake hook, and the canvas's events.
    fn install(lp: &Rc<Self>, canvas: &HtmlCanvasElement, sink: &HtmlTextAreaElement) -> Result<(), JsValue> {
        let l = lp.clone();
        *lp.frame_cb.borrow_mut() = Some(Closure::new(move |_t: f64| l.on_frame()));
        let l = lp.clone();
        *lp.timer_cb.borrow_mut() = Some(Closure::new(move || {
            l.sched.borrow_mut().timer = None;
            l.wake();
        }));
        let l = lp.clone();
        *lp.finger_end_cb.borrow_mut() = Some(Closure::new(move || l.finger_lift()));
        let l = lp.clone();
        *lp.paste_cb.borrow_mut() = Some(Closure::new(move || l.release_paste()));
        let l = Rc::downgrade(lp);
        set_wake(Some(Box::new(move || {
            if let Some(l) = l.upgrade() {
                l.wake();
            }
        })));

        let target: &web_sys::EventTarget = canvas.as_ref();
        let l = lp.clone();
        listen(target, "pointermove", false, move |e: PointerEvent| {
            l.event(|s| {
                let pos = s.pointer_pos(&e);
                let (driver, t) = s.turn();
                driver.pointer_motion(t, pos);
                s.update_cursor();
            })
        })?;
        let l = lp.clone();
        listen(target, "pointerenter", false, move |e: PointerEvent| {
            l.event(|s| {
                let pos = s.pointer_pos(&e);
                let (driver, t) = s.turn();
                driver.pointer_enter(t, pos);
                s.update_cursor();
            })
        })?;
        let l = lp.clone();
        listen(target, "pointerleave", false, move |_e: PointerEvent| {
            l.event(|s| {
                let (driver, t) = s.turn();
                driver.pointer_leave(t);
            })
        })?;
        let l = lp.clone();
        let c = canvas.clone();
        let k = sink.clone();
        listen(target, "pointerdown", true, move |e: PointerEvent| {
            let Some(btn) = dom_button(e.button()) else { return };
            // Take the keyboard first: the focus event this fires is
            // dispatched now, before the shell is borrowed below. Capture
            // keeps a drag's moves and its release coming to the canvas when
            // the pointer leaves it, as a Wayland implicit grab does.
            let _ = k.focus();
            let _ = c.set_pointer_capture(e.pointer_id());
            e.prevent_default();
            l.event(|s| {
                let mods = s.mods_from(e.ctrl_key(), e.shift_key(), e.alt_key(), e.meta_key());
                s.sync_mods(mods);
                let pos = s.pointer_pos(&e);
                let site = PressSite { size: s.size(), on_popup: false, can_grab: false, own_edges: false };
                let (driver, t) = s.turn();
                driver.pointer_press(t, btn, pos, site);
            })
        })?;
        let l = lp.clone();
        listen(target, "pointerup", false, move |e: PointerEvent| {
            let Some(btn) = dom_button(e.button()) else { return };
            l.event(|s| {
                let pos = s.pointer_pos(&e);
                let (driver, t) = s.turn();
                driver.pointer_release(t, btn, pos);
                s.update_cursor();
            })
        })?;
        // The right button is the app's, not the page's menu.
        listen(target, "contextmenu", true, |e: web_sys::Event| e.prevent_default())?;
        let l = lp.clone();
        listen(target, "wheel", true, move |e: WheelEvent| {
            e.prevent_default();
            let frame = wheel_frame(e.delta_mode(), e.delta_x(), e.delta_y());
            l.event(|s| {
                let mods = s.mods_from(e.ctrl_key(), e.shift_key(), e.alt_key(), e.meta_key());
                s.sync_mods(mods);
                let pos = LogicalPosition::new(e.offset_x() as f32, e.offset_y() as f32);
                s.driver.cursor_pos = (pos.x, pos.y);
                let (driver, t) = s.turn();
                driver.scroll(t, frame, pos);
            });
            if frame.source == Some(ScrollSource::Finger) {
                l.arm_finger_lift();
            }
        })?;
        // The canvas focused some other way (Tab, the page's script) hands
        // the keyboard to the sink.
        let k = sink.clone();
        listen(target, "focus", false, move |_e: FocusEvent| {
            let _ = k.focus();
        })?;
        let keys: &web_sys::EventTarget = sink.as_ref();
        let l = lp.clone();
        listen(keys, "keydown", true, move |e: KeyboardEvent| l.key(&e, ElementState::Pressed))?;
        let l = lp.clone();
        listen(keys, "keyup", true, move |e: KeyboardEvent| l.key(&e, ElementState::Released))?;
        for (name, focused) in [("focus", true), ("blur", false)] {
            let l = lp.clone();
            let c = canvas.clone();
            listen(keys, name, false, move |e: FocusEvent| {
                // Over to the canvas is on its way back.
                let to_canvas = e.related_target().is_some_and(|r| AsRef::<JsValue>::as_ref(&r) == AsRef::<JsValue>::as_ref(&c));
                if !focused && to_canvas {
                    return;
                }
                l.event(|s| {
                    let (driver, t) = s.turn();
                    driver.keyboard_focus(t, focused);
                })
            })?;
        }
        // The input method's half.
        let l = lp.clone();
        let k = sink.clone();
        listen(keys, "input", false, move |e: InputEvent| {
            if e.is_composing() {
                let text = k.value();
                let cursor = utf16_range_to_bytes(&text, k.selection_start().ok().flatten(), k.selection_end().ok().flatten());
                l.event(|s| {
                    let (driver, t) = s.turn();
                    driver.preedit(t, Some(crate::ime::Preedit { text, cursor }));
                });
            } else if e.input_type() == "insertFromPaste" {
                // A paste that reached the sink by the browser's default (a
                // handler of the page's swallowed its `paste` event): the
                // clipboard's text all the same, for the held ⌘/Ctrl+V.
                clipboard::pasted(k.value());
                k.set_value("");
                l.release_paste();
            } else {
                // Text with no composition: an emoji panel, dictation.
                let text = k.value();
                k.set_value("");
                if !text.is_empty() {
                    l.event(|s| {
                        let (driver, t) = s.turn();
                        driver.commit_text(t, text);
                    });
                }
            }
        })?;
        let l = lp.clone();
        let k = sink.clone();
        listen(keys, "compositionend", false, move |e: CompositionEvent| {
            let text = e.data().unwrap_or_default();
            k.set_value("");
            l.event(|s| {
                let (driver, t) = s.turn();
                driver.preedit(t, None);
                let (driver, t) = s.turn();
                driver.commit_text(t, text);
            });
        })?;
        // The clipboard's events, raised by the three keys `key` lets
        // through. They go to the focused element or the body, so they are
        // heard on the document.
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            let doc: &web_sys::EventTarget = doc.as_ref();
            let l = lp.clone();
            listen(doc, "paste", true, move |e: ClipboardEvent| {
                if let Some(text) = e.clipboard_data().and_then(|d| d.get_data("text/plain").ok()) {
                    clipboard::pasted(text);
                }
                e.prevent_default();
                l.release_paste();
            })?;
            for name in ["copy", "cut"] {
                listen(doc, name, true, move |e: ClipboardEvent| {
                    if let (Some(text), Some(data)) = (clipboard::take_copied(), e.clipboard_data()) {
                        if data.set_data("text/plain", &text).is_ok() {
                            e.prevent_default();
                        }
                    }
                })?;
            }
        }
        // The page laying the canvas out anew is a configure; the turn it
        // wakes measures the box (`sync`).
        let l = lp.clone();
        let observer = Closure::<dyn FnMut()>::new(move || l.wake());
        let ro = web_sys::ResizeObserver::new(observer.as_ref().unchecked_ref())?;
        ro.observe(canvas);
        observer.forget();
        std::mem::forget(ro);
        Ok(())
    }

    fn key(&self, e: &KeyboardEvent, state: ElementState) {
        // The input method's key: it composes with it (and a browser that
        // sends the key confirming a composition after its end still marks
        // it 229).
        if e.is_composing() || e.key_code() == 229 {
            return;
        }
        let accel = e.ctrl_key() || e.meta_key();
        let Some((key, text)) = map_key(&e.key(), accel) else { return };
        let clip = clipboard_key(&e.key(), accel, e.alt_key());
        // A clipboard key's default is its clipboard event: the page keeps it.
        if clip.is_none() && !passes_to_page(e) {
            e.prevent_default();
        }
        // The driver repeats a held key itself, at the toolkit's own rate
        // (`KEY_REPEAT_DELAY` / `_INTERVAL`), as it does on Wayland, where
        // the compositor sends one press: the browser's repeats are dropped.
        if e.repeat() {
            return;
        }
        let mods = (e.ctrl_key(), e.shift_key(), e.alt_key(), e.meta_key());
        if state == ElementState::Pressed {
            match clip {
                // Held until the `paste` event has handed over the text: it
                // is raised after this listener returns, in the same task,
                // so a zero timer is the fallback for a browser that raises
                // none (the clipboard then reads what it read before).
                Some(ClipKey::Paste) => {
                    self.release_paste();
                    *self.held_paste.borrow_mut() = Some(HeldKey { key, text, mods });
                    if let Some(cb) = self.paste_cb.borrow().as_ref() {
                        let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(cb.as_ref().unchecked_ref(), 0);
                    }
                    return;
                }
                // A copy left over from a menu click is not this key's.
                Some(ClipKey::Copy | ClipKey::Cut) => {
                    clipboard::take_copied();
                }
                None => {}
            }
        } else {
            // A release never overtakes the press it ends.
            self.release_paste();
        }
        self.dispatch_key(HeldKey { key, text, mods }, state);
    }

    fn dispatch_key(&self, k: HeldKey, state: ElementState) {
        let (ctrl, shift, alt, meta) = k.mods;
        self.event(|s| {
            let mods = s.mods_from(ctrl, shift, alt, meta);
            s.sync_mods(mods);
            let (driver, t) = s.turn();
            driver.key(t, k.key, k.text, state);
        });
    }

    /// Hand a held ⌘/Ctrl+V to the app, if one is held: its `paste` event
    /// has come, or will not.
    fn release_paste(&self) {
        let held = self.held_paste.borrow_mut().take();
        if let Some(k) = held {
            self.dispatch_key(k, ElementState::Pressed);
        }
    }

    fn arm_finger_lift(&self) {
        let win = window();
        if let Some(t) = self.finger_timer.take() {
            win.clear_timeout_with_handle(t);
        }
        if let Some(cb) = self.finger_end_cb.borrow().as_ref() {
            self.finger_timer.set(
                win.set_timeout_with_callback_and_timeout_and_arguments_0(
                    cb.as_ref().unchecked_ref(),
                    FINGER_LIFT.as_millis() as i32,
                )
                .ok(),
            );
        }
    }

    /// No finger frame for [`FINGER_LIFT`]: the gesture ended.
    fn finger_lift(&self) {
        self.finger_timer.set(None);
        let frame = ScrollFrame { source: Some(ScrollSource::Finger), stop: true, ..Default::default() };
        self.event(|s| {
            let (x, y) = s.driver.cursor_pos;
            let (driver, t) = s.turn();
            driver.scroll(t, frame, LogicalPosition::new(x, y));
        });
    }
}

/// The hidden textarea the keyboard goes to (see the module doc): fixed in
/// the page, invisible, never hit, and no help offered on what is typed.
fn keyboard_sink() -> Result<HtmlTextAreaElement, JsValue> {
    let doc = window().document().ok_or_else(|| JsValue::from_str("cce-ui: no document"))?;
    let sink: HtmlTextAreaElement = doc.create_element("textarea")?.dyn_into()?;
    for (k, v) in [("autocomplete", "off"), ("autocorrect", "off"), ("autocapitalize", "off"), ("spellcheck", "false"), ("aria-hidden", "true")] {
        sink.set_attribute(k, v)?;
    }
    let st = sink.style();
    for (k, v) in [
        ("position", "fixed"),
        ("left", "0px"),
        ("top", "0px"),
        ("width", "1px"),
        ("height", "16px"),
        ("padding", "0"),
        ("border", "0"),
        ("margin", "0"),
        ("outline", "none"),
        ("resize", "none"),
        ("overflow", "hidden"),
        ("white-space", "pre"),
        ("opacity", "0"),
        ("pointer-events", "none"),
        ("caret-color", "transparent"),
        ("color", "transparent"),
        ("background", "transparent"),
    ] {
        st.set_property(k, v)?;
    }
    doc.body().ok_or_else(|| JsValue::from_str("cce-ui: no body"))?.append_child(&sink)?;
    Ok(sink)
}

fn window() -> web_sys::Window {
    web_sys::window().expect("cce-ui's browser shell runs in a window")
}

fn device_pixel_ratio() -> f64 {
    web_sys::window().map_or(1.0, |w| w.device_pixel_ratio())
}

fn is_mac() -> bool {
    web_sys::window()
        .and_then(|w| w.navigator().platform().ok())
        .is_some_and(|p| p.starts_with("Mac") || p.starts_with("iP"))
}

fn set_css_size(canvas: &HtmlCanvasElement, w: u32, h: u32) {
    let style = canvas.style();
    let _ = style.set_property("width", &format!("{w}px"));
    let _ = style.set_property("height", &format!("{h}px"));
}

/// Add `f` as the target's `name` listener for the page's lifetime. A
/// `cancelable` one is added `passive: false`, so it may cancel the event's
/// default (the page's scroll, menu, focus move or shortcut).
fn listen<E: JsCast + 'static>(
    target: &web_sys::EventTarget,
    name: &str,
    cancelable: bool,
    mut f: impl FnMut(E) + 'static,
) -> Result<(), JsValue> {
    let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| f(e.unchecked_into()));
    let opts = AddEventListenerOptions::new();
    opts.set_passive(!cancelable);
    target.add_event_listener_with_callback_and_add_event_listener_options(name, cb.as_ref().unchecked_ref(), &opts)?;
    cb.forget();
    Ok(())
}

/// A DOM `button` as the driver's. The back and forward buttons are not
/// buttons the toolkit has.
fn dom_button(b: i16) -> Option<MouseButton> {
    match b {
        0 => Some(MouseButton::Left),
        1 => Some(MouseButton::Middle),
        2 => Some(MouseButton::Right),
        _ => None,
    }
}

/// Keys the page keeps however the app routes keys: reload and the
/// developer tools.
fn passes_to_page(e: &KeyboardEvent) -> bool {
    let k = e.key();
    let accel = e.ctrl_key() || e.meta_key();
    k == "F5" || k == "F12" || (accel && (k == "r" || k == "R")) || (accel && e.shift_key() && (k == "I" || k == "J"))
}
