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
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | `run`, the events the AppKit side hands the shell (`Ev`, `send`), the menu |
//! | `shell` | `MacShell`: the event handling, turns, the frame and the `Shell` impl |
//! | `view` | `CceView`, the layer-hosting view: mouse, keys, scroll, magnify, the input-method client |
//! | `delegate` | the application and window delegate: quit, close, resize, focus |

mod delegate;
mod shell;
mod view;

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

use delegate::Delegate;
use shell::MacShell;
use view::CceView;

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

fn physical(logical: (f32, f32), scale: f64) -> (u32, u32) {
    let s = scale as f32;
    ((logical.0 * s).round().max(1.0) as u32, (logical.1 * s).round().max(1.0) as u32)
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
