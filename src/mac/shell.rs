//! `MacShell`: the shell's state, what each event does to it, scheduling and taking a turn, the
//! frame and its present, and the `Shell` impl the shared `Pacer` drives.

use super::*;

/// The window's side of the run loop, as the Wayland shell's `EngineState`
/// is the compositor's. Fields drop in order: the renderer before the layer
/// it presents to.
pub(super) struct MacShell<A: Application> {
    pub(super) renderer: VkRenderer,
    pub(super) app: A,
    pub(super) driver: Driver,
    pub(super) redraw: bool,
    pub(super) exit: bool,
    pub(super) rx: Receiver<A::Message>,
    pub(super) fs: cosmic_text::FontSystem,
    pub(super) swash: cosmic_text::SwashCache,
    pub(super) items: Vec<DlText>,
    pub(super) damage_owed: bool,
    /// The view's size in points, and the window's backing scale.
    pub(super) logical: (f32, f32),
    pub(super) scale: f64,
    pub(super) just_configured: bool,
    pub(super) cursor: CursorIcon,
    /// The pinch's cumulative scale (AppKit reports each step's change).
    pub(super) pinch: f32,
    pub(super) pacer: Pacer,
    /// The generation of the turn that is due; older dispatches are stale.
    pub(super) generation: u64,
    pub(super) due: Option<Instant>,
    pub(super) last_turn: Instant,
    pub(super) stopped: bool,
    /// The caret the input method was last told of.
    pub(super) ime_caret: Option<[f32; 4]>,
    pub(super) mtm: MainThreadMarker,
    pub(super) layer: Retained<CAMetalLayer>,
    pub(super) view: Retained<CceView>,
    pub(super) window: Retained<NSWindow>,
}

impl<A: Application> MacShell<A> {
    pub(super) fn event(&mut self, ev: Ev) -> bool {
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
    pub(super) fn wake(&mut self) {
        let at = (self.last_turn + ACTIVE_DISPATCH).max(Instant::now());
        if self.due.is_some_and(|d| d <= at) {
            return;
        }
        self.schedule(at.saturating_duration_since(Instant::now()));
    }

    pub(super) fn schedule(&mut self, after: Duration) {
        self.generation += 1;
        self.due = Some(Instant::now() + after);
        let g = self.generation;
        let when = DispatchTime::try_from(after).unwrap_or(DispatchTime::NOW);
        let _ = DispatchQueue::main().after(when, move || {
            send(Ev::Turn(g));
        });
    }

    pub(super) fn take_turn(&mut self) {
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
    pub(super) fn finish(&mut self) {
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
    pub(super) fn measure(&mut self) -> bool {
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
    pub(super) fn sync_input_method(&mut self) {
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

    pub(super) fn size(&self) -> LogicalSize {
        LogicalSize::new(self.logical.0, self.logical.1)
    }

    pub(super) fn at(&mut self, x: f32, y: f32) -> LogicalPosition {
        self.driver.cursor_pos = (x, y);
        LogicalPosition::new(x, y)
    }

    pub(super) fn sync_mods(&mut self, flags: u64) {
        let mods = appkit::modifiers(flags);
        if mods != self.driver.mods {
            self.driver.set_modifiers(&mut self.app, mods);
        }
    }

    pub(super) fn update_cursor(&mut self) {
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
