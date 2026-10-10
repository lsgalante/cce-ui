//! `CceView`: the flipped, layer-hosting view the window shows. It maps mouse, scroll, magnify and
//! key events to `Ev`s, and is the `NSTextInputClient` an input method composes into.

use super::*;

/// Where an event's pointer is, in the view's (flipped) coordinates.
fn location(view: &NSView, event: &NSEvent) -> (f32, f32) {
    let p = view.convertPoint_fromView(event.locationInWindow(), None);
    (p.x as f32, p.y as f32)
}

/// The view's input-method state, for the `NSTextInputClient` methods.
#[derive(Default)]
pub(super) struct ViewIme {
    /// The marked text (the composition) as the input method last set it.
    pub(super) marked: RefCell<String>,
    /// Inside `interpretKeyEvents:` for a key press, whose characters are
    /// these; and whether the input method took the press.
    pub(super) in_key: Cell<bool>,
    pub(super) consumed: Cell<bool>,
    pub(super) key_chars: RefCell<String>,
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
    pub(super) struct CceView;

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
    pub(super) fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
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
