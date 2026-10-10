//! The application and window delegate: Quit and the close button ask the app to exit; resizes,
//! backing-scale changes and focus are handed to the shell.

use super::*;

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and Delegate no Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "CceUiDelegate"]
    pub(super) struct Delegate;

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
    pub(super) fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: NSObject's `init`.
        unsafe { msg_send![super(this), init] }
    }
}
