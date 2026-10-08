//! The macOS shell: an [`Application`] run in an AppKit window, the way the
//! Wayland shell runs one on a surface and the browser shell on a canvas. It
//! is the third shell over the shared pieces — [`Driver`] routes what input
//! means, [`Pacer::turn`] decides what a turn does, [`build_frame`] builds
//! the frame — and the renderer is the Vulkan one, on Metal through
//! MoltenVK (a `CAMetalLayer` is a [`SurfaceTarget::Metal`]). What is left
//! here is AppKit's side:
//!
//! - **The window.** An `NSWindow` whose content is the app's root plate:
//!   transparent, its titlebar transparent over full-size content, so the
//!   plate fills the window and the traffic lights stand on its corner. The
//!   window keeps AppKit's own resize edges, so the driver's CSD resize band
//!   is the app's (`PressSite::own_edges`); a press it reads as the
//!   window's (the titlebar band, a movable root plate) drags the window
//!   (`performWindowDragWithEvent:`).
//! - **Events in.** A layer-hosting `NSView` (flipped, so y runs down as
//!   everywhere else in the toolkit) takes the mouse, scroll, magnify and
//!   key events and hands them, mapped by `backend::appkit`, to the driver.
//!   Command is the shortcut key (⌘Z is undo). AppKit's own key repeats
//!   and scroll momentum are dropped: the driver repeats a held key and the
//!   toolkit coasts a flick, as on Wayland.
//! - **Turns.** Main-queue dispatches (`dispatch2`): any event, and any
//!   `AppSender::send` from any thread (`set_wake`), asks for a turn at most
//!   one ACTIVE frame after the last; between, the loop sleeps for whatever
//!   the pacer said. A turn superseded by an earlier one is dropped by its
//!   generation.
//! - **Frames out.** [`build_frame`] at the view's size in points and the
//!   window's backing scale, drawn by the [`VkRenderer`]; `stage_renderer`
//!   (and so the portable `stage_3d`) runs before each draw, `renderer_init`
//!   once.
//!
//! The menu is a minimal one (Quit, ⌘Q), whose quit — like the close
//! button — asks the app to exit the way a compositor's close does.
//!
//! - **Input methods.** The view is an `NSTextInputClient`: while a widget
//!   is editing text, a key press goes through `interpretKeyEvents:`, whose
//!   marked text is the composition and whose inserted text the commit
//!   (`crate::ime`); the candidate window is put under the caret the
//!   widget reported. The clipboard is the general `NSPasteboard`
//!   (`widget::clipboard`).
//!
//! Not there yet: drag and drop, the context menu in a popup window (it is
//! drawn in the window and kept inside it, as on a layer surface), and blur
//! behind the window (`NSVisualEffectView`).
//!
//! **This module has never been run.** It is written against objc2's
//! AppKit bindings and type-checked for `aarch64-apple-darwin`
//! (`scripts/check-mac`) on Linux, where nothing can be linked or run: it
//! needs a Mac, with MoltenVK installed (the Vulkan SDK, or Homebrew's
//! `molten-vk` and `vulkan-loader`).

use std::cell::{Cell, RefCell};
use std::sync::mpsc::Receiver;
use std::time::Duration;

use cursor_icon::CursorIcon;
use dispatch2::{DispatchQueue, DispatchTime};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, ProtocolObject, Sel};
use objc2::{define_class, msg_send, sel, AnyThread, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSApplicationTerminateReply,
    NSBackingStoreType, NSColor, NSCursor, NSEvent, NSEventModifierFlags, NSEventType, NSMenu, NSMenuItem,
    NSTextInputClient, NSTrackingArea, NSTrackingAreaOptions, NSView, NSWindow, NSWindowDelegate, NSWindowStyleMask,
    NSWindowTitleVisibility,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSArray, NSAttributedString, NSAttributedStringKey, NSNotFound, NSNotification,
    NSObjectProtocol, NSPoint, NSRange, NSRangePointer, NSRect, NSSize, NSString, NSUInteger,
};
use objc2_quartz_core::CAMetalLayer;
use web_time::Instant;

use crate::backend::app::{set_wake, AppSender, Application, LogicalPosition, LogicalSize};
use crate::backend::appkit;
use crate::backend::driver::{Driver, Press, PressSite, Turn};
use crate::backend::frame::build_frame;
use crate::backend::shell::{Pacer, Shell, Step, ACTIVE_DISPATCH};
use crate::vk::{SurfaceTarget, VkRenderer};
use crate::widget::{context_menu, ElementState, Key};
use crate::backend::text::DlText;

/// Run `A` in a window until it exits. Must be called on the main thread
/// (a process's `main`), as AppKit requires.
pub fn run<A: Application>() {
    let mtm = MainThreadMarker::new().expect("cce-ui's macOS shell runs on the main thread");
    // The window's interaction state (`crate::window_state`), current while the run loop
    // below runs this window's code — which is until `run` returns.
    let window_state = crate::window_state::WindowState::new();
    let _window = crate::window_state::enter(&window_state);
    let ns_app = NSApplication::sharedApplication(mtm);
    ns_app.setActivationPolicy(NSApplicationActivationPolicy::Regular);

    let (tx, rx) = std::sync::mpsc::channel();
    // The scale is known before the app is built, as on the other shells: a
    // widget may read it as it is made.
    let main_scale = objc2_app_kit::NSScreen::mainScreen(mtm).map_or(2.0, |s| s.backingScaleFactor());
    crate::scale::set_scale_factor(main_scale as f32);
    let mut app = A::create(AppSender::from(tx));
    let settings = app.settings();
    crate::scale::set_app_id(settings.app_id.clone());

    let delegate = Delegate::new(mtm);
    ns_app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    ns_app.setMainMenu(Some(&main_menu(mtm, &settings.title)));

    let style = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::Miniaturizable
        | NSWindowStyleMask::Resizable
        | NSWindowStyleMask::FullSizeContentView;
    let rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(settings.width as f64, settings.height as f64));
    // SAFETY: released-when-closed is turned off at once, as a window made
    // outside a window controller must be.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            rect,
            style,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(&NSString::from_str(&settings.title));
    window.setTitlebarAppearsTransparent(true);
    window.setTitleVisibility(NSWindowTitleVisibility::Hidden);
    window.setOpaque(false);
    window.setBackgroundColor(Some(&NSColor::clearColor()));
    window.setAcceptsMouseMovedEvents(true);
    if let Some((w, h)) = settings.min_size {
        window.setContentMinSize(NSSize::new(w as f64, h as f64));
    }
    window.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));

    let view = CceView::new(mtm, rect);
    let layer = CAMetalLayer::new();
    layer.setOpaque(false);
    let scale = window.backingScaleFactor();
    layer.setContentsScale(scale);
    // A layer-HOSTING view: the layer is set before the view wants one.
    view.setLayer(Some(&layer));
    view.setWantsLayer(true);
    window.setContentView(Some(&view));
    let _ = window.makeFirstResponder(Some(&view));
    window.center();

    let fs = if app.load_system_fonts() {
        crate::create_font_system_with_system_fonts()
    } else {
        crate::create_font_system()
    };
    let (pw, ph) = physical((settings.width as f32, settings.height as f32), scale);
    let layer_ptr: *const std::ffi::c_void = Retained::as_ptr(&layer).cast();
    // SAFETY: the layer is held by the shell, which drops the renderer first.
    let mut renderer = match unsafe { VkRenderer::try_new_for(SurfaceTarget::Metal { layer: layer_ptr }, pw, ph, 0.0) } {
        Ok(r) => r,
        Err(e) => {
            log::error!("[mac] no renderer for the window: {e}");
            return;
        }
    };
    app.renderer_init(&mut renderer);

    let shell = MacShell {
        renderer,
        app,
        driver: Driver::new(),
        redraw: true,
        exit: false,
        rx,
        fs,
        swash: cosmic_text::SwashCache::new(),
        items: Vec::new(),
        damage_owed: true,
        logical: (0.0, 0.0),
        scale,
        just_configured: true,
        cursor: CursorIcon::Default,
        pinch: 1.0,
        pacer: Pacer::new(settings.title),
        generation: 0,
        due: None,
        last_turn: Instant::now(),
        stopped: false,
        ime_caret: None,
        mtm,
        layer,
        view: view.clone(),
        window: window.clone(),
    };
    let shell = RefCell::new(shell);
    shell.borrow_mut().measure();
    SINK.with(|s| *s.borrow_mut() = Some(Box::new(move |ev| shell.borrow_mut().event(ev))));
    // A send from any thread hops to the main queue and asks for a turn.
    set_wake(Some(std::sync::Arc::new(|| DispatchQueue::main().exec_async(|| {
        send(Ev::Wake);
    }))));

    window.makeKeyAndOrderFront(None);
    #[allow(deprecated)]
    ns_app.activateIgnoringOtherApps(true);
    send(Ev::Wake);
    ns_app.run();

    // `stop:` returned the run loop: the app has exited. The shell — the
    // renderer before the layer it presents to — goes with the sink.
    set_wake(None);
    SINK.with(|s| s.borrow_mut().take());
    drop(view);
    drop(window);
}

/// What the view, the delegate and the loop's dispatches hand the shell, in
/// plain values: the shell is generic over the app and the AppKit classes
/// cannot be, so they reach it through [`SINK`].
enum Ev {
    Moved { x: f32, y: f32 },
    Entered { x: f32, y: f32 },
    Exited,
    /// Answers true when the press is a window drag the view must start.
    Pressed { number: i64, flags: u64, x: f32, y: f32 },
    Released { number: i64, flags: u64, x: f32, y: f32 },
    Scrolled { dx: f64, dy: f64, precise: bool, inverted: bool, phase: u64, momentum: u64, flags: u64, x: f32, y: f32 },
    Magnified { phase: u64, magnification: f64 },
    Keyed { code: u16, chars: String, unmodified: String, flags: u64, down: bool },
    /// The input method's composition changed (marked text).
    Preedit(Option<crate::ime::Preedit>),
    /// The input method committed text.
    Commit(String),
    FlagsChanged { code: u16, flags: u64 },
    Focused(bool),
    /// The window's size or backing scale changed.
    Resized,
    /// The close button, or Quit: the app is asked to exit.
    CloseRequested,
    /// A turn scheduled under this generation is due.
    Turn(u64),
    /// Turn soon.
    Wake,
}

thread_local! {
    static SINK: RefCell<Option<Box<dyn FnMut(Ev) -> bool>>> = const { RefCell::new(None) };
}

/// Hand `ev` to the shell. An event raised from inside a dispatch (AppKit
/// calling back while the app runs) finds the shell busy and is dropped,
/// as the browser shell drops one; so is everything after the exit.
fn send(ev: Ev) -> bool {
    SINK.with(|s| match s.try_borrow_mut() {
        Ok(mut sink) => sink.as_mut().is_some_and(|f| f(ev)),
        Err(_) => false,
    })
}

/// The window's side of the run loop, as the Wayland shell's `EngineState`
/// is the compositor's. Fields drop in order: the renderer before the layer
/// it presents to.
struct MacShell<A: Application> {
    renderer: VkRenderer,
    app: A,
    driver: Driver,
    redraw: bool,
    exit: bool,
    rx: Receiver<A::Message>,
    fs: cosmic_text::FontSystem,
    swash: cosmic_text::SwashCache,
    items: Vec<DlText>,
    damage_owed: bool,
    /// The view's size in points, and the window's backing scale.
    logical: (f32, f32),
    scale: f64,
    just_configured: bool,
    cursor: CursorIcon,
    /// The pinch's cumulative scale (AppKit reports each step's change).
    pinch: f32,
    pacer: Pacer,
    /// The generation of the turn that is due; older dispatches are stale.
    generation: u64,
    due: Option<Instant>,
    last_turn: Instant,
    stopped: bool,
    /// The caret the input method was last told of.
    ime_caret: Option<[f32; 4]>,
    mtm: MainThreadMarker,
    layer: Retained<CAMetalLayer>,
    view: Retained<CceView>,
    window: Retained<NSWindow>,
}

impl<A: Application> MacShell<A> {
    fn event(&mut self, ev: Ev) -> bool {
        if self.stopped {
            return false;
        }
        let mut drag = false;
        match ev {
            Ev::Turn(g) => {
                if g == self.generation {
                    self.take_turn();
                }
                return false;
            }
            Ev::Wake => {}
            Ev::Moved { x, y } => {
                let pos = self.at(x, y);
                let (driver, t) = self.turn();
                driver.pointer_motion(t, pos);
                self.update_cursor();
            }
            Ev::Entered { x, y } => {
                let pos = self.at(x, y);
                let (driver, t) = self.turn();
                driver.pointer_enter(t, pos);
                self.update_cursor();
            }
            Ev::Exited => {
                let (driver, t) = self.turn();
                driver.pointer_leave(t);
            }
            Ev::Pressed { number, flags, x, y } => {
                let Some(btn) = appkit::button(number, flags) else { return false };
                self.sync_mods(flags);
                let pos = self.at(x, y);
                // AppKit resizes from its own window edges; a move is a drag.
                let site = PressSite { size: self.size(), on_popup: false, can_grab: true, own_edges: true };
                let (driver, t) = self.turn();
                drag = driver.pointer_press(t, btn, pos, site) == Press::Move;
            }
            Ev::Released { number, flags, x, y } => {
                let Some(btn) = appkit::button(number, flags) else { return false };
                let pos = self.at(x, y);
                let (driver, t) = self.turn();
                driver.pointer_release(t, btn, pos);
                self.update_cursor();
            }
            Ev::Scrolled { dx, dy, precise, inverted, phase, momentum, flags, x, y } => {
                // The system's natural-scrolling setting is the one in force
                // here, not input.kdl's: value controls read it through this.
                if precise {
                    crate::input::force_natural_scroll(Some(inverted));
                }
                let Some(frame) = appkit::scroll_frame(dx, dy, precise, inverted, phase, momentum) else { return false };
                self.sync_mods(flags);
                let pos = self.at(x, y);
                let (driver, t) = self.turn();
                driver.scroll(t, frame, pos);
            }
            Ev::Magnified { phase, magnification } => {
                if phase & appkit::phase::BEGAN != 0 {
                    self.pinch = 1.0;
                    self.driver.pinch_begin();
                }
                if magnification != 0.0 {
                    self.pinch *= 1.0 + magnification as f32;
                    let pinch = self.pinch;
                    let (driver, t) = self.turn();
                    driver.pinch_update(t, pinch);
                }
                if phase & (appkit::phase::ENDED | appkit::phase::CANCELLED) != 0 {
                    self.driver.pinch_end();
                }
            }
            Ev::Keyed { code, chars, unmodified, flags, down } => {
                let accel = flags & (appkit::flags::COMMAND | appkit::flags::CONTROL) != 0;
                let Some((key, text)) = appkit::map_key(code, &chars, &unmodified, accel) else { return false };
                self.sync_mods(flags);
                let state = if down { ElementState::Pressed } else { ElementState::Released };
                let (driver, t) = self.turn();
                driver.key(t, key, text, state);
            }
            Ev::Preedit(preedit) => {
                let (driver, t) = self.turn();
                driver.preedit(t, preedit);
            }
            Ev::Commit(text) => {
                let (driver, t) = self.turn();
                driver.preedit(t, None);
                let (driver, t) = self.turn();
                driver.commit_text(t, text);
            }
            Ev::FlagsChanged { code, flags } => {
                // A modifier key went down or up: the new state, and the key
                // itself as the other shells report it.
                self.sync_mods(flags);
                let bit = match code {
                    0x37 | 0x36 => appkit::flags::COMMAND,
                    0x38 | 0x3C => appkit::flags::SHIFT,
                    0x3A | 0x3D => appkit::flags::OPTION,
                    0x3B | 0x3E => appkit::flags::CONTROL,
                    _ => 0,
                };
                if let (true, Some((key @ Key::Named(_), _))) = (bit != 0, appkit::map_key(code, "", "", false)) {
                    let state = if flags & bit != 0 { ElementState::Pressed } else { ElementState::Released };
                    let (driver, t) = self.turn();
                    driver.key(t, key, None, state);
                }
            }
            Ev::Focused(focused) => {
                let (driver, t) = self.turn();
                driver.keyboard_focus(t, focused);
            }
            Ev::Resized => {
                if self.measure() {
                    self.just_configured = true;
                }
            }
            Ev::CloseRequested => self.exit = true,
        }
        self.wake();
        drag
    }

    /// Ask for a turn one ACTIVE frame after the last at the latest, unless
    /// one is due sooner already.
    fn wake(&mut self) {
        let at = (self.last_turn + ACTIVE_DISPATCH).max(Instant::now());
        if self.due.is_some_and(|d| d <= at) {
            return;
        }
        self.schedule(at.saturating_duration_since(Instant::now()));
    }

    fn schedule(&mut self, after: Duration) {
        self.generation += 1;
        self.due = Some(Instant::now() + after);
        let g = self.generation;
        let when = DispatchTime::try_from(after).unwrap_or(DispatchTime::NOW);
        let _ = DispatchQueue::main().after(when, move || {
            send(Ev::Turn(g));
        });
    }

    fn take_turn(&mut self) {
        self.due = None;
        self.last_turn = Instant::now();
        while let Ok(msg) = self.rx.try_recv() {
            let mut rebuild = false;
            self.app.update(msg, &mut rebuild, &mut self.exit);
            self.redraw |= rebuild;
        }
        let mut pacer = std::mem::replace(&mut self.pacer, Pacer::new(String::new()));
        let step = pacer.turn(self);
        self.pacer = pacer;
        match step {
            Step::Exit => self.finish(),
            Step::Sleep(d) => {
                if self.due.is_none() {
                    self.schedule(d);
                }
            }
        }
    }

    /// The app exited: let it take its leave, then return the run loop.
    fn finish(&mut self) {
        self.stopped = true;
        self.app.on_exit();
        let ns_app = NSApplication::sharedApplication(self.mtm);
        ns_app.stop(None);
        // `stop:` takes effect after the next event; this is it.
        if let Some(ev) = NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
            NSEventType::ApplicationDefined,
            NSPoint::new(0.0, 0.0),
            NSEventModifierFlags::empty(),
            0.0,
            0,
            None,
            0,
            0,
            0,
        ) {
            ns_app.postEvent_atStart(&ev, true);
        }
    }

    /// Read the view's size and the backing scale, and size the drawable to
    /// them. Whether either changed.
    fn measure(&mut self) -> bool {
        let bounds = self.view.bounds();
        let (w, h) = (bounds.size.width as f32, bounds.size.height as f32);
        let scale = self.window.backingScaleFactor();
        if (w, h) == self.logical && scale == self.scale {
            return false;
        }
        self.logical = (w, h);
        if scale != self.scale {
            self.scale = scale;
            crate::scale::set_scale_factor(scale as f32);
        }
        // MoltenVK reports the layer's bounds times its contents scale as
        // the surface's extent.
        self.layer.setContentsScale(scale);
        let (pw, ph) = physical(self.logical, self.scale);
        self.renderer.resize(pw, ph);
        self.redraw = true;
        true
    }

    /// Keep the input method in step with the frame just drawn: a
    /// composition a widget dropped, or one with no widget editing any more,
    /// is discarded (the marked text cleared first, so the `unmarkText` this
    /// may call commits nothing); a caret that moved has the candidate
    /// window follow it.
    fn sync_input_method(&mut self) {
        let caret = crate::ime::caret();
        let drop = crate::ime::take_reset() || (caret.is_none() && !self.view.ivars().marked.borrow().is_empty());
        let context = self.view.inputContext();
        if drop {
            self.view.ivars().marked.borrow_mut().clear();
            crate::ime::set_preedit(None);
            if let Some(c) = &context {
                c.discardMarkedText();
            }
        }
        if caret != self.ime_caret {
            self.ime_caret = caret;
            if let Some(c) = &context {
                c.invalidateCharacterCoordinates();
            }
        }
    }

    fn size(&self) -> LogicalSize {
        LogicalSize::new(self.logical.0, self.logical.1)
    }

    fn at(&mut self, x: f32, y: f32) -> LogicalPosition {
        self.driver.cursor_pos = (x, y);
        LogicalPosition::new(x, y)
    }

    fn sync_mods(&mut self, flags: u64) {
        let mods = appkit::modifiers(flags);
        if mods != self.driver.mods {
            self.driver.set_modifiers(&mut self.app, mods);
        }
    }

    fn update_cursor(&mut self) {
        let (x, y) = self.driver.cursor_pos;
        let icon = self.driver.cursor_icon_at(&self.app, x, y, self.size());
        if icon != self.cursor {
            self.cursor = icon;
            ns_cursor(icon).set();
        }
    }
}

impl<A: Application> Shell for MacShell<A> {
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
        if (w as f32, h as f32) != self.logical {
            self.window.setContentSize(NSSize::new(w as f64, h as f64));
            self.measure();
        }
    }

    fn sync(&mut self) {
        // No popup window to host the menu yet: it is drawn in the window
        // and kept inside it, frames asked for while a page turn animates.
        if context_menu::is_visible() {
            if context_menu::is_turning() {
                self.redraw = true;
            }
            context_menu::set_hosted(false);
            context_menu::constrain_to(0.0, 0.0, self.logical.0, self.logical.1);
        }
    }

    fn set_title(&mut self, title: &str) {
        self.window.setTitle(&NSString::from_str(title));
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
        let (pw, ph) = physical(self.logical, self.scale);
        let e = self.renderer.pending_extent();
        if (e.width, e.height) != (pw, ph) {
            self.renderer.resize(pw, ph);
        }
        if self.app.stage_renderer(&mut self.renderer, size, self.scale) {
            self.redraw = true;
        }
        if self.renderer.draw_frame_2d(frame.frame2d()) {
            self.damage_owed = false;
        } else {
            self.redraw = true;
        }
        self.sync_input_method();
    }
}

fn physical(logical: (f32, f32), scale: f64) -> (u32, u32) {
    let s = scale as f32;
    ((logical.0 * s).round().max(1.0) as u32, (logical.1 * s).round().max(1.0) as u32)
}

/// The toolkit's cursor as AppKit's. AppKit has no public diagonal resize
/// cursors before macOS 15, so those are the arrow; the two straight ones
/// are deprecated there, for the directional cursors macOS 15 added, and
/// still what every earlier release has.
#[allow(deprecated)]
fn ns_cursor(icon: CursorIcon) -> Retained<NSCursor> {
    match icon {
        CursorIcon::Pointer => NSCursor::pointingHandCursor(),
        CursorIcon::Text | CursorIcon::VerticalText => NSCursor::IBeamCursor(),
        CursorIcon::Crosshair | CursorIcon::Cell => NSCursor::crosshairCursor(),
        CursorIcon::Grab => NSCursor::openHandCursor(),
        CursorIcon::Grabbing | CursorIcon::Move | CursorIcon::AllScroll => NSCursor::closedHandCursor(),
        CursorIcon::NotAllowed | CursorIcon::NoDrop => NSCursor::operationNotAllowedCursor(),
        CursorIcon::EResize | CursorIcon::WResize | CursorIcon::EwResize | CursorIcon::ColResize => {
            NSCursor::resizeLeftRightCursor()
        }
        CursorIcon::NResize | CursorIcon::SResize | CursorIcon::NsResize | CursorIcon::RowResize => {
            NSCursor::resizeUpDownCursor()
        }
        CursorIcon::ContextMenu => NSCursor::contextualMenuCursor(),
        CursorIcon::Copy => NSCursor::dragCopyCursor(),
        CursorIcon::Alias => NSCursor::dragLinkCursor(),
        _ => NSCursor::arrowCursor(),
    }
}

/// An application menu with Quit (⌘Q) in it: every Mac app has one, and
/// without it ⌘Q does nothing.
fn main_menu(mtm: MainThreadMarker, title: &str) -> Retained<NSMenu> {
    let bar = NSMenu::new(mtm);
    let app_item = NSMenuItem::new(mtm);
    let app_menu = NSMenu::new(mtm);
    // SAFETY: `terminate:` is NSApplication's, which answers it through the
    // delegate's `applicationShouldTerminate:` below.
    let quit = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(&format!("Quit {title}")),
            Some(sel!(terminate:)),
            ns_string!("q"),
        )
    };
    app_menu.addItem(&quit);
    app_item.setSubmenu(Some(&app_menu));
    bar.addItem(&app_item);
    bar
}

/// Where an event's pointer is, in the view's (flipped) coordinates.
fn location(view: &NSView, event: &NSEvent) -> (f32, f32) {
    let p = view.convertPoint_fromView(event.locationInWindow(), None);
    (p.x as f32, p.y as f32)
}

/// The view's input-method state, for the `NSTextInputClient` methods.
#[derive(Default)]
struct ViewIme {
    /// The marked text (the composition) as the input method last set it.
    marked: RefCell<String>,
    /// Inside `interpretKeyEvents:` for a key press, whose characters are
    /// these; and whether the input method took the press.
    in_key: Cell<bool>,
    consumed: Cell<bool>,
    key_chars: RefCell<String>,
}

/// The text of what an input method hands over: an `NSString`, or an
/// `NSAttributedString` around one.
fn ns_text(obj: &AnyObject) -> String {
    if let Some(a) = obj.downcast_ref::<NSAttributedString>() {
        a.string().to_string()
    } else if let Some(s) = obj.downcast_ref::<NSString>() {
        s.to_string()
    } else {
        String::new()
    }
}

define_class!(
    // SAFETY: NSView has no subclassing requirements, and CceView no Drop.
    #[unsafe(super(NSView, objc2_app_kit::NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "CceUiView"]
    #[ivars = ViewIme]
    struct CceView;

    unsafe impl NSObjectProtocol for CceView {}

    // The input method's side of the view: AppKit calls these from inside
    // `interpretKeyEvents:` (see `key`), and the candidate window asks where
    // the caret is.
    unsafe impl NSTextInputClient for CceView {
        #[unsafe(method(insertText:replacementRange:))]
        unsafe fn insert_text(&self, string: &AnyObject, _replacement: NSRange) {
            let text = ns_text(string);
            let ime = self.ivars();
            let was_marked = !std::mem::take(&mut *ime.marked.borrow_mut()).is_empty();
            // A plain key typing its own character is left to the key path,
            // which keeps its named keys and the driver's repeat.
            if ime.in_key.get() && !was_marked && text == *ime.key_chars.borrow() {
                return;
            }
            ime.consumed.set(true);
            send(Ev::Commit(text));
        }

        #[unsafe(method(doCommandBySelector:))]
        unsafe fn do_command(&self, _selector: Sel) {
            // A key the input method does not take (Return, Backspace, an
            // arrow): left to the key path.
        }

        #[unsafe(method(setMarkedText:selectedRange:replacementRange:))]
        unsafe fn set_marked_text(&self, string: &AnyObject, selected: NSRange, _replacement: NSRange) {
            let text = ns_text(string);
            let ime = self.ivars();
            ime.consumed.set(true);
            *ime.marked.borrow_mut() = text.clone();
            let cursor = crate::backend::dom::utf16_range_to_bytes(
                &text,
                Some(selected.location as u32),
                Some((selected.location + selected.length) as u32),
            );
            send(Ev::Preedit((!text.is_empty()).then(|| crate::ime::Preedit { text, cursor })));
        }

        #[unsafe(method(unmarkText))]
        fn unmark_text(&self) {
            // Accept the composition as it stands.
            let text = std::mem::take(&mut *self.ivars().marked.borrow_mut());
            if !text.is_empty() {
                send(Ev::Commit(text));
            }
        }

        #[unsafe(method(selectedRange))]
        fn selected_range(&self) -> NSRange {
            NSRange::new(NSNotFound as usize, 0)
        }

        #[unsafe(method(markedRange))]
        fn marked_range(&self) -> NSRange {
            let marked = self.ivars().marked.borrow();
            if marked.is_empty() {
                NSRange::new(NSNotFound as usize, 0)
            } else {
                NSRange::new(0, marked.encode_utf16().count())
            }
        }

        #[unsafe(method(hasMarkedText))]
        fn has_marked_text(&self) -> bool {
            !self.ivars().marked.borrow().is_empty()
        }

        #[unsafe(method_id(attributedSubstringForProposedRange:actualRange:))]
        unsafe fn attributed_substring(&self, _range: NSRange, _actual: NSRangePointer) -> Option<Retained<NSAttributedString>> {
            None
        }

        #[unsafe(method_id(validAttributesForMarkedText))]
        fn valid_attributes(&self) -> Retained<NSArray<NSAttributedStringKey>> {
            NSArray::new()
        }

        /// Where the candidate window goes: under the editing widget's caret,
        /// in screen coordinates.
        #[unsafe(method(firstRectForCharacterRange:actualRange:))]
        unsafe fn first_rect(&self, _range: NSRange, _actual: NSRangePointer) -> NSRect {
            let [x, y, w, h] = crate::ime::caret().unwrap_or([0.0, 0.0, 1.0, 16.0]);
            let caret = NSRect::new(NSPoint::new(x as f64, y as f64), NSSize::new(w.max(1.0) as f64, h as f64));
            let in_window = self.convertRect_toView(caret, None);
            self.window().map_or(in_window, |w| w.convertRectToScreen(in_window))
        }

        #[unsafe(method(characterIndexForPoint:))]
        fn character_index(&self, _point: NSPoint) -> NSUInteger {
            NSNotFound as NSUInteger
        }
    }

    impl CceView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            let (x, y) = location(self, event);
            send(Ev::Moved { x, y });
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            let (x, y) = location(self, event);
            send(Ev::Moved { x, y });
        }

        #[unsafe(method(rightMouseDragged:))]
        fn right_mouse_dragged(&self, event: &NSEvent) {
            let (x, y) = location(self, event);
            send(Ev::Moved { x, y });
        }

        #[unsafe(method(otherMouseDragged:))]
        fn other_mouse_dragged(&self, event: &NSEvent) {
            let (x, y) = location(self, event);
            send(Ev::Moved { x, y });
        }

        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, event: &NSEvent) {
            let (x, y) = location(self, event);
            send(Ev::Entered { x, y });
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            send(Ev::Exited);
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            self.press(event);
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &NSEvent) {
            self.press(event);
        }

        #[unsafe(method(otherMouseDown:))]
        fn other_mouse_down(&self, event: &NSEvent) {
            self.press(event);
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            self.release(event);
        }

        #[unsafe(method(rightMouseUp:))]
        fn right_mouse_up(&self, event: &NSEvent) {
            self.release(event);
        }

        #[unsafe(method(otherMouseUp:))]
        fn other_mouse_up(&self, event: &NSEvent) {
            self.release(event);
        }

        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {
            let (x, y) = location(self, event);
            send(Ev::Scrolled {
                dx: event.scrollingDeltaX(),
                dy: event.scrollingDeltaY(),
                precise: event.hasPreciseScrollingDeltas(),
                inverted: event.isDirectionInvertedFromDevice(),
                phase: event.phase().0 as u64,
                momentum: event.momentumPhase().0 as u64,
                flags: event.modifierFlags().0 as u64,
                x,
                y,
            });
        }

        #[unsafe(method(magnifyWithEvent:))]
        fn magnify(&self, event: &NSEvent) {
            send(Ev::Magnified { phase: event.phase().0 as u64, magnification: event.magnification() });
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            self.key(event, true);
        }

        #[unsafe(method(keyUp:))]
        fn key_up(&self, event: &NSEvent) {
            self.key(event, false);
        }

        #[unsafe(method(flagsChanged:))]
        fn flags_changed(&self, event: &NSEvent) {
            send(Ev::FlagsChanged { code: event.keyCode(), flags: event.modifierFlags().0 as u64 });
        }
    }
);

impl CceView {
    fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ViewIme::default());
        // SAFETY: NSView's designated initializer.
        let view: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        // Enter, exit and motion wherever the view is, window key or not.
        let options = NSTrackingAreaOptions::MouseEnteredAndExited
            | NSTrackingAreaOptions::MouseMoved
            | NSTrackingAreaOptions::ActiveAlways
            | NSTrackingAreaOptions::InVisibleRect;
        // SAFETY: the view owns the area and outlives it; no user info.
        let area = unsafe {
            NSTrackingArea::initWithRect_options_owner_userInfo(
                NSTrackingArea::alloc(),
                NSRect::ZERO,
                options,
                Some(&view),
                None,
            )
        };
        view.addTrackingArea(&area);
        view
    }

    fn press(&self, event: &NSEvent) {
        let (x, y) = location(self, event);
        let ev = Ev::Pressed { number: event.buttonNumber() as i64, flags: event.modifierFlags().0 as u64, x, y };
        if send(ev) {
            if let Some(window) = self.window() {
                window.performWindowDragWithEvent(event);
            }
        }
    }

    fn release(&self, event: &NSEvent) {
        let (x, y) = location(self, event);
        send(Ev::Released { number: event.buttonNumber() as i64, flags: event.modifierFlags().0 as u64, x, y });
    }

    fn key(&self, event: &NSEvent, down: bool) {
        let text = |s: Option<Retained<NSString>>| s.map(|s| s.to_string()).unwrap_or_default();
        let flags = event.modifierFlags().0 as u64;
        // While a widget is editing text, a key press goes to the input
        // method first (`interpretKeyEvents:`, which calls back into the
        // `NSTextInputClient` methods above); what it takes is composition
        // or a commit, what it leaves takes the key path below. A ⌘
        // shortcut never goes, and nothing does while nothing is editing,
        // so an input method left on does not eat an app's single-key
        // commands.
        let ime = self.ivars();
        if down && flags & appkit::flags::COMMAND == 0 && crate::ime::caret().is_some() {
            ime.in_key.set(true);
            ime.consumed.set(false);
            *ime.key_chars.borrow_mut() = text(event.characters());
            self.interpretKeyEvents(&NSArray::from_slice(&[event]));
            ime.in_key.set(false);
            if ime.consumed.get() {
                return;
            }
        }
        // The driver repeats a held key itself, as it does on Wayland.
        if event.isARepeat() {
            return;
        }
        let keyed = |down| Ev::Keyed {
            code: event.keyCode(),
            chars: text(event.characters()),
            unmodified: text(event.charactersIgnoringModifiers()),
            flags,
            down,
        };
        send(keyed(down));
        // AppKit sends no keyUp for a key pressed with Command held, so a ⌘
        // shortcut is released as it is pressed: otherwise the driver would
        // take the key as held, and repeat ⌘Z until the window lost focus.
        if down && flags & appkit::flags::COMMAND != 0 {
            send(keyed(false));
        }
    }
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and Delegate no Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "CceUiDelegate"]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl NSApplicationDelegate for Delegate {
        /// Quit asks the app to exit, as a close does; the shell stops the
        /// run loop once it has.
        #[unsafe(method(applicationShouldTerminate:))]
        fn should_terminate(&self, _sender: &NSApplication) -> NSApplicationTerminateReply {
            send(Ev::CloseRequested);
            NSApplicationTerminateReply::TerminateCancel
        }
    }

    unsafe impl NSWindowDelegate for Delegate {
        #[unsafe(method(windowShouldClose:))]
        fn window_should_close(&self, _sender: &NSWindow) -> bool {
            send(Ev::CloseRequested);
            false
        }

        #[unsafe(method(windowDidResize:))]
        fn window_did_resize(&self, _notification: &NSNotification) {
            send(Ev::Resized);
        }

        #[unsafe(method(windowDidChangeBackingProperties:))]
        fn window_did_change_backing(&self, _notification: &NSNotification) {
            send(Ev::Resized);
        }

        #[unsafe(method(windowDidBecomeKey:))]
        fn window_did_become_key(&self, _notification: &NSNotification) {
            send(Ev::Focused(true));
        }

        #[unsafe(method(windowDidResignKey:))]
        fn window_did_resign_key(&self, _notification: &NSNotification) {
            send(Ev::Focused(false));
        }
    }
);

impl Delegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: NSObject's `init`.
        unsafe { msg_send![super(this), init] }
    }
}
