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
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | `Sizing`, `Fonts`, `run`, `capture`, and the small DOM helpers |
//! | `canvas` | `WebShell`: the shell's state over the canvas, its sizing, the keyboard sink's place, and the `Shell` impl |
//! | `events` | the loop: scheduling turns on animation frames and timers, and the page's events installed on the canvas and the sink |

mod canvas;
mod events;

use canvas::WebShell;
use events::{Loop, Sched};

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
use crate::text::DlText;


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
    // A page has no environment: its locale is the browser's, and it must be
    // decided before the first font system is built from it.
    if let Some(lang) = web_sys::window().and_then(|w| w.navigator().language()) {
        crate::locale::set_locale(&lang);
    }
    // The page's window's interaction state (`crate::window_state`). The loop runs on the
    // page's callbacks after this returns, so the state is entered for the page's life.
    std::mem::forget(crate::window_state::enter(&crate::window_state::WindowState::new()));
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
