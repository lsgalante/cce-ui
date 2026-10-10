//! The page's event loop: a turn per animation frame while active and on a timer while idle, and
//! the pointer, wheel, key, focus, clipboard and input-method events installed on the canvas and
//! the keyboard sink, with the browser's key repeats dropped and a finger's lift synthesized.

use super::*;

/// When the next turn is due.
#[derive(Default)]
pub(super) struct Sched {
    /// An animation frame is requested: the next turn is in it.
    pub(super) frame: Option<i32>,
    /// An idle timer is set; it requests the frame when it fires.
    pub(super) timer: Option<i32>,
    /// The app exited: nothing turns again.
    pub(super) stopped: bool,
}

/// The shell, the pacer and the schedule, shared by the page's callbacks.
/// The schedule is its own cell, borrowed only briefly and never across a
/// call into the app, so a message the app sends mid-turn can wake the loop.
pub(super) struct Loop<A: Application> {
    pub(super) shell: RefCell<WebShell<A>>,
    pub(super) pacer: RefCell<Pacer>,
    pub(super) sched: RefCell<Sched>,
    pub(super) frame_cb: RefCell<Option<Closure<dyn FnMut(f64)>>>,
    pub(super) timer_cb: RefCell<Option<Closure<dyn FnMut()>>>,
    pub(super) finger_end_cb: RefCell<Option<Closure<dyn FnMut()>>>,
    /// The lift timer the last finger frame set; the next frame cancels it.
    pub(super) finger_timer: Cell<Option<i32>>,
    /// A ⌘/Ctrl+V held back until its `paste` event, and the timer that
    /// lets it through if no event comes.
    pub(super) held_paste: RefCell<Option<HeldKey>>,
    pub(super) paste_cb: RefCell<Option<Closure<dyn FnMut()>>>,
}

/// A key press, kept to dispatch later: the key, its text and the
/// modifiers it came with (ctrl, shift, alt, meta).
pub(super) struct HeldKey {
    pub(super) key: Key,
    pub(super) text: Option<String>,
    pub(super) mods: (bool, bool, bool, bool),
}

/// A browser reports no lift for a two-finger scroll: this long without a
/// finger frame is one, and ends the gesture (`ScrollPhase::FingerEnd`) so a
/// flick coasts and a side swipe readies its next turn.
pub(super) const FINGER_LIFT: Duration = Duration::from_millis(120);

impl<A: Application> Loop<A> {
    /// Turn the loop at the next animation frame.
    pub(super) fn wake(&self) {
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
    pub(super) fn on_frame(&self) {
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
    pub(super) fn event(&self, f: impl FnOnce(&mut WebShell<A>)) {
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
    pub(super) fn install(lp: &Rc<Self>, canvas: &HtmlCanvasElement, sink: &HtmlTextAreaElement) -> Result<(), JsValue> {
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

    pub(super) fn key(&self, e: &KeyboardEvent, state: ElementState) {
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

    pub(super) fn dispatch_key(&self, k: HeldKey, state: ElementState) {
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
    pub(super) fn release_paste(&self) {
        let held = self.held_paste.borrow_mut().take();
        if let Some(k) = held {
            self.dispatch_key(k, ElementState::Pressed);
        }
    }

    pub(super) fn arm_finger_lift(&self) {
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
    pub(super) fn finger_lift(&self) {
        self.finger_timer.set(None);
        let frame = ScrollFrame { source: Some(ScrollSource::Finger), stop: true, ..Default::default() };
        self.event(|s| {
            let (x, y) = s.driver.cursor_pos;
            let (driver, t) = s.turn();
            driver.scroll(t, frame, LogicalPosition::new(x, y));
        });
    }
}
