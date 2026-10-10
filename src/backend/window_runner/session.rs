//! `run`: one session per connection around the same app — a new surface, swapchain and renderer
//! each time — reconnecting while the compositor is there and waiting out a restart for an app
//! that outlives it.

use super::*;

/// Raise this process's file-descriptor soft limit toward its hard limit.
///
/// A cce-ui client's fd usage is not bounded by anything the app controls.
/// Every dmabuf-feedback event the compositor sends carries a format-table
/// fd, and those arrive per surface whenever scanout candidacy changes —
/// entering the overview re-sends one for every window at once. Long-lived
/// windows sit at 700+ open fds in normal use, against a soft limit of 1024.
///
/// Crossing that limit does not fail politely. `recvmsg` drops the SCM_RIGHTS
/// payload when it cannot allocate descriptors, while still delivering the
/// message body — so libwayland hits a message whose fd never arrived,
/// reports "file descriptor expected", and the connection dies. That is
/// precisely the transport break [`run`] reconnects from below, at the cost
/// of a rebuilt window.
///
/// The compositor raises itself to 65536 for the same reason and then
/// deliberately restores the inherited limit for the programs it spawns
/// (cce-compositor `process.rs::cleanup_child`) — right for an arbitrary
/// child, far too low for a dmabuf-heavy Wayland client. So each client
/// raises its own, to the same ceiling.
pub(super) fn raise_fd_limit() {
    unsafe {
        let mut lim: libc::rlimit = std::mem::zeroed();
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) != 0 {
            return;
        }
        let want = std::cmp::min(65536, lim.rlim_max);
        if lim.rlim_cur >= want {
            return;
        }
        let raised = libc::rlimit { rlim_cur: want, rlim_max: lim.rlim_max };
        if libc::setrlimit(libc::RLIMIT_NOFILE, &raised) == 0 {
            log::info!("[window_runner] fd limit raised {} -> {}", lim.rlim_cur, want);
        } else {
            log::warn!("[window_runner] could not raise fd limit from {}", lim.rlim_cur);
        }
    }
}

/// Run an [`Application`] to completion, surviving loss of the compositor
/// connection.
///
/// A Wayland connection cannot be repaired once its transport state breaks — a
/// single dropped file descriptor on a dmabuf-feedback event is enough, and
/// libwayland then fails every dispatch with `EINVAL`. Exiting the process on
/// that error (the old behavior) threw away everything the window held: a
/// terminal's shell and scrollback, an editor's unsaved buffer.
///
/// So a connection is one *session*. Objects that belong to the connection —
/// the Wayland globals, the surface, the swapchain, the renderer — are rebuilt
/// per session. The things that carry user state outlive it: the `Application`
/// itself, the calloop loop, and the message channel. Keeping the **same
/// channel** matters as much as keeping the app: worker threads hold clones of
/// its `Sender` (cce-terminal's pty reader is the canonical case), and a fresh
/// channel would orphan them into a live-but-deaf process.
///
/// What is repaired is the transport, never the compositor: a reconnect only
/// goes through while the compositor that owned the lost session is still
/// listening. If the connect itself fails the compositor has exited, and the
/// process exits with it — see `after_session` for why staying alive there
/// duplicated every window on the next session restore.
///
/// Caveat: GPU resources belong to the renderer, so a rebuild re-runs
/// [`Application::renderer_init`]. Images uploaded outside it (e.g. in
/// [`Application::create`](crate::backend::app::Application::create)) are not replayed into the new renderer — upload from
/// `renderer_init` if they must survive a reconnect.
pub fn run<A: Application>() {
    raise_fd_limit();
    // This window's interaction state (menu, hover highlight, swipe, composition), current
    // for the whole run: every session, every callback (`crate::window_state`). It outlives
    // a reconnect, as the app does, so a menu open across one stays open.
    let window_state = crate::window_state::WindowState::new();
    let _window = crate::window_state::enter(&window_state);

    // Outlives every session: worker threads hold this Sender, and the app's
    // own event sources are registered on this loop once.
    let (sender, channel) = calloop::channel::channel::<A::Message>();
    // Drop payloads come back from the per-drop reader threads (see
    // `backend::dnd`); registered once, like the app channel, because the
    // loop outlives a reconnect while the EngineState does not.
    let (drop_tx, drop_rx) =
        calloop::channel::channel::<crate::backend::dnd::DroppedData>();
    // The accessibility adapter's callbacks (`backend::a11y_unix`), from its own thread;
    // registered once, as the loop outlives a reconnect.
    let (a11y_tx, a11y_rx) = calloop::channel::channel::<crate::backend::a11y_unix::Event>();
    let mut event_loop = match EventLoop::try_new() {
        Ok(l) => l,
        Err(e) => {
            log::error!("[window_runner] cannot create event loop: {e}");
            return;
        }
    };
    event_loop
        .handle()
        .insert_source(channel, |event, _metadata, app_state: &mut EngineState<A>| {
            if let calloop::channel::Event::Msg(msg) = event {
                let mut rebuild = false;
                app_state.inner.as_mut().unwrap().update(msg, &mut rebuild, &mut app_state.exit);
                if rebuild {
                    app_state.redraw = true;
                }
            }
        })
        .unwrap();
    event_loop
        .handle()
        .insert_source(drop_rx, |event, _metadata, app_state: &mut EngineState<A>| {
            if let calloop::channel::Event::Msg(drop) = event {
                // The transfer is complete, so the source can be released now
                // — doing it any earlier costs the payload.
                if let Some(offer) = app_state.pending_drop_offer.take() {
                    offer.finish();
                    offer.destroy();
                }
                let mut rebuild = false;
                if let Some(app) = app_state.inner.as_mut() {
                    app.handle_drop(&drop.mime, &drop.bytes, drop.pos, &mut rebuild);
                }
                if rebuild {
                    app_state.redraw = true;
                }
            }
        })
        .unwrap();

    event_loop
        .handle()
        .insert_source(a11y_rx, |event, _metadata, app_state: &mut EngineState<A>| {
            use crate::backend::a11y_unix::{act, Acted, Event};
            let calloop::channel::Event::Msg(event) = event else { return };
            if app_state.inner.is_none() {
                return;
            }
            if let Event::Action(request) = &event {
                match act(app_state.inner.as_mut().unwrap(), request) {
                    Acted::Nothing => return,
                    Acted::Changed => app_state.redraw = true,
                    Acted::Key(key) => {
                        app_state.redraw = true;
                        let (driver, t) = app_state.turn();
                        driver.press_named_key(t, key);
                    }
                }
            }
            let (Some(publisher), Some(app)) = (app_state.a11y.as_mut(), app_state.inner.as_mut()) else { return };
            match event {
                // Published from here, not at the next frame: an idle window renders none,
                // and AccessKit wants the tree by the next refresh.
                Event::Activated => {
                    if crate::backend::a11y_unix::debug() {
                        eprintln!("[a11y] a screen reader connected");
                    }
                    publisher.publish(app, crate::scale::scale_factor() as f64)
                }
                Event::Deactivated => {}
                // Carried out above; the tree it left is published now, as on arrival.
                Event::Action(_) => publisher.publish(app, crate::scale::scale_factor() as f64),
            }
        })
        .unwrap();

    let mut app: Option<A> = None;
    let mut sources_registered = false;
    let mut attempt: u32 = 0;

    loop {
        let started = std::time::Instant::now();
        let (returned_app, end) =
            run_session(&mut event_loop, sender.clone(), drop_tx.clone(), a11y_tx.clone(), app.take(), !sources_registered);
        app = returned_app;
        sources_registered = true;

        let outlives = app.as_ref().is_some_and(|a| a.outlives_compositor());
        match after_session(end, app.is_some(), started.elapsed(), &mut attempt, outlives) {
            AfterSession::Exit => {
                match end {
                    SessionEnd::AppExit => {}
                    SessionEnd::NoCompositor if app.is_some() => log::warn!(
                        "[window_runner] compositor is gone; exiting (its successor restores the session itself)"
                    ),
                    SessionEnd::NoCompositor => {
                        log::error!("[window_runner] no compositor connection; giving up")
                    }
                    SessionEnd::ConnectionLost if app.is_some() => log::error!(
                        "[window_runner] connection lost; giving up after {} attempts",
                        attempt - 1
                    ),
                    SessionEnd::ConnectionLost => {
                        log::error!("[window_runner] no compositor connection; giving up")
                    }
                }
                break;
            }
            AfterSession::Reconnect(backoff) => {
                log::warn!(
                    "[window_runner] compositor connection lost; reconnecting in {backoff:?} (attempt {attempt})"
                );
                std::thread::sleep(backoff);
            }
            AfterSession::AwaitCompositor => {
                log::warn!("[window_runner] compositor is gone; waiting for the next one");
                await_compositor_socket();
                log::info!("[window_runner] a compositor is back; rejoining");
            }
        }
    }

    if let Some(mut app) = app {
        app.on_exit();
    }
}

/// One connection's lifetime: connect, build the surface and renderer, pump
/// events until the app exits or the connection dies. Returns the
/// `Application` so the caller can hand it to the next session.
pub(super) fn run_session<'l, A: Application>(
    event_loop: &mut EventLoop<'l, EngineState<A>>,
    sender: calloop::channel::Sender<A::Message>,
    drop_tx: calloop::channel::Sender<crate::backend::dnd::DroppedData>,
    a11y_tx: calloop::channel::Sender<crate::backend::a11y_unix::Event>,
    existing_app: Option<A>,
    register_app_sources: bool,
) -> (Option<A>, SessionEnd) {
    let conn = match Connection::connect_to_env() {
        Ok(c) => c,
        Err(e) => {
            log::error!("[window_runner] cannot connect to compositor: {e}");
            return (existing_app, SessionEnd::NoCompositor);
        }
    };
    let (globals, mut event_queue) = match registry_queue_init(&conn) {
        Ok(v) => v,
        Err(e) => {
            log::error!("[window_runner] registry init failed: {e}");
            return (existing_app, SessionEnd::ConnectionLost);
        }
    };
    let qh = event_queue.handle();

    let compositor_state = CompositorState::bind(&globals, &qh).unwrap();
    let xdg_shell_state = XdgShell::bind(&globals, &qh).unwrap();
    let layer_shell_state = LayerShell::bind(&globals, &qh).ok();
    let shm_state = Shm::bind(&globals, &qh).unwrap();
    let seat_state = SeatState::new(&globals, &qh);
    let output_state = OutputState::new(&globals, &qh);

    let pointer_gestures: Option<ZwpPointerGesturesV1> = globals.bind(&qh, 1..=3, ()).ok();
    let text_input_manager: Option<ZwpTextInputManagerV3> = globals.bind(&qh, 1..=1, ()).ok();

    let mut engine_state = EngineState {
        a11y: None,
        data_device_manager: DataDeviceManagerState::bind(&globals, &qh).ok(),
        data_devices: Vec::new(),
        drag_mime: None,
        drag_pos: LogicalPosition::new(0.0, 0.0),
        drop_tx: Some(drop_tx),
        pending_drop_offer: None,
        applied_input_regions: None,
        registry_state: RegistryState::new(&globals),
        compositor_state,
        xdg_shell_state,
        layer_shell_state,
        shm_state,
        seat_state,
        output_state,
        seats: Vec::new(),
        pointer: None,
        keyboard: None,
        window: None,
        layer_surface: None,
        is_layer_app: false,
        layer_hidden: false,
        surface: None,
        inner: None,
        renderer: None,
        font_system: None,
        swash_cache: cosmic_text::SwashCache::new(),
        scale_factor: 1.0,
        committed_buffer_scale: 1,
        entered_outputs: Vec::new(),
        logical_width: 0.0,
        logical_height: 0.0,
        frame_logical: (0.0, 0.0),
        applied_margin: 0.0,
        overflow_was_active: false,
        sent_popover_region: None,
        menu_popup: None,
        menu_renderer: None,
        menu_icon_ids: std::collections::HashMap::new(),
        display_ptr: 0,
        exit: false,
        redraw: false,
        damage_owed: true,
        frame_record: Default::default(),
        frame_callback_pending: false,
        frame_callback_armed_at: None,
        keepalive_pending: false,
        keepalive_armed_at: None,
        extent_gate_skips: 0,
        first_configure_received: false,
        driver: Driver::new(),
        sender,
        current_cursor_icon: None,
        qh: qh.clone(),
        just_configured: false,
        pointer_gestures,
        text_input_manager,
        text_input: None,
        text_input_state: Default::default(),
        pinch_gesture: None,
        cce_toplevel: None,
        pending_grid_patch: None,
        last_press_serial: None,
        touch: None,
        touch_tracker: Default::default(),
        touch_offset: (0.0, 0.0),
        touch_scroll_at: None,
        dl_text_items: Vec::new(),
    };

    if let Err(e) = event_queue.roundtrip(&mut engine_state) {
        log::error!("[window_runner] initial roundtrip failed: {e}");
        return (existing_app, SessionEnd::ConnectionLost);
    }

    let scale = detect_scale_factor(&engine_state.output_state);
    engine_state.scale_factor = scale;
    crate::scale::set_scale_factor(scale as f32);
    crate::window_state::set_metric(crate::wayland::detect_metric(&engine_state.output_state, scale));

    // A reconnect re-attaches the SAME app: its state is the thing worth
    // saving, and `A::new` would both discard it and hand a fresh Sender to
    // worker threads that are still holding the original.
    let inner = match existing_app {
        Some(app) => app,
        None => A::create(AppSender::from(engine_state.sender.clone())),
    };
    if crate::backend::a11y_unix::wanted(&inner) {
        engine_state.a11y = crate::backend::a11y_unix::Publisher::start(a11y_tx);
    }
    let settings = inner.settings();
    crate::scale::set_app_id(settings.app_id.clone());
    engine_state.logical_width = settings.width as f32;
    engine_state.logical_height = settings.height as f32;
    engine_state.inner = Some(inner);

    let surface = engine_state.compositor_state.create_surface(&qh);
    // A grid app's surface is pinned to scale 1: the patch's `scale` is
    // BUFFER px per virtual unit and already carries the output scale (the
    // patch manager folds it in), so adopting the output scale here would
    // square it — the client renders a doubled buffer and the compositor
    // downsamples it straight back into blur.
    if engine_state.inner.as_ref().unwrap().grid() {
        engine_state.scale_factor = 1.0;
    }
    // Forced-scale mode renders scaled-up into a buffer_scale-1 surface.
    let buffer_scale = if crate::scale::forced_scale().is_some()
        || engine_state.inner.as_ref().unwrap().grid()
    {
        1
    } else {
        scale as i32
    };
    surface.set_buffer_scale(buffer_scale);
    engine_state.committed_buffer_scale = buffer_scale;

    let layer_settings = engine_state.inner.as_ref().unwrap().layer();
    if let Some(ls) = layer_settings {
        engine_state.is_layer_app = true;
        engine_state.attach_layer_role(&surface, &ls, settings.width, settings.height);
    } else {
        let window = engine_state.xdg_shell_state.create_window(surface.clone(), WindowDecorations::None, &qh);
        window.set_title(&settings.title);
        window.set_app_id(&settings.app_id);
        if settings.fullscreen {
            window.set_fullscreen(None);
        }
        if let Some((min_w, min_h)) = settings.min_size {
            window.set_min_size(Some((min_w, min_h)));
        }
        let wants_utility = engine_state.inner.as_ref().unwrap().utility();
        let wants_grid = engine_state.inner.as_ref().unwrap().grid();
        {
            // Bound for EVERY app now, not just utility/grid ones: the
            // toplevel also carries the popover-region hint (manager v7),
            // which any app with a dropdown wants. Role declarations go
            // BEFORE the initial commit so the mode is set by the time the
            // compositor maps the window. Version floors: set_utility
            // appeared at manager 5, the grid role at 6; the range tops at 7
            // so a newer compositor grants the hint and an older one simply
            // yields a lower-versioned toplevel — the hint send is gated on
            // version() >= 7 (send_popover_region), and on a pre-5
            // compositor the bind fails and the app runs plain.
            let version = if wants_grid { 6..=7 } else { 5..=7 };
            match globals.bind::<crate::protocol::cce_window_management_v1::zcce_window_manager_v1::ZcceWindowManagerV1, _, _>(&qh, version, ()) {
                Ok(cce_wm) => {
                    let toplevel = cce_wm.get_cce_toplevel(&surface, &qh, ());
                    if wants_utility {
                        toplevel.set_utility();
                    }
                    if wants_grid {
                        toplevel.set_grid();
                    }
                    engine_state.cce_toplevel = Some(toplevel);
                }
                Err(e) => {
                    log::warn!("[window_runner] cce window-management declaration unavailable: {e}");
                }
            }
        }
        window.commit();
        engine_state.window = Some(window);
    }
    engine_state.surface = Some(surface);

    // Overflow-margin mode: the surface (and so the GPU swapchain) is a rim
    // larger than the window frame on every side; geometry/input-region are
    // published per-resize.
    let rim = 2.0 * engine_state.inner.as_ref().unwrap().overflow_margin() as f32;
    if let Err(lost) = engine_state.init_gpu(&conn, settings.width as f32 + rim, settings.height as f32 + rim) {
        log::error!("[window_runner] cannot create the renderer, ending session: {lost}");
        return (engine_state.inner.take(), SessionEnd::ConnectionLost);
    }
    engine_state
        .inner
        .as_mut()
        .unwrap()
        .renderer_init(engine_state.renderer.as_mut().unwrap());

    let loop_handle = event_loop.handle();
    let wayland_token = match WaylandSource::new(conn.clone(), event_queue).insert(loop_handle.clone())
    {
        Ok(token) => token,
        Err(e) => {
            log::error!("[window_runner] cannot register the wayland source: {e}");
            return (engine_state.inner.take(), SessionEnd::ConnectionLost);
        }
    };

    // The app's own sources live on the persistent loop, so they are registered
    // once for the process — re-registering per session would double-deliver
    // every event on them.
    if register_app_sources {
        engine_state.inner.as_mut().unwrap().register_sources(&loop_handle);
    }

    /// Same switch as the renderer's present tracer, resolved once — this sits
    /// in the per-iteration path, so a `std::env::var` call here would be I/O
    /// on the loop that is under measurement.
    fn loop_debug() -> bool {
        crate::vk::present_debug()
    }

    /// Seconds after session start at which to inject a simulated connection
    /// loss, from `CCE_UI_FAULT_RECONNECT`. Resolved once: this is read from
    /// the per-iteration path.
    fn fault_reconnect_after() -> Option<std::time::Duration> {
        static AFTER: std::sync::OnceLock<Option<std::time::Duration>> =
            std::sync::OnceLock::new();
        *AFTER.get_or_init(|| {
            std::env::var("CCE_UI_FAULT_RECONNECT")
                .ok()
                .and_then(|v| v.parse::<f32>().ok())
                .map(std::time::Duration::from_secs_f32)
        })
    }

    let mut end = SessionEnd::AppExit;
    let session_start = std::time::Instant::now();
    // The loop's pacing — what a turn does and how long to sleep after it —
    // is the shared `Pacer`'s (backend::shell); this loop is the Wayland
    // side: dispatch, the connection's health, the close fade.
    let mut pacer = Pacer::new(settings.title.clone());
    let mut next_timeout = ACTIVE_DISPATCH;
    loop {
        // Frame callbacks arrive with a p50 of 0ms but a ~0.5s tail, while the
        // compositor's own trace shows it firing them within one or two vsyncs
        // of the arm. Tracing each iteration bisects that: if this loop keeps
        // turning at ~16ms all through a long wait, the event was not there to
        // read, and the delay is upstream rather than in dispatching it.
        let iter_start = if loop_debug() {
            Some(std::time::Instant::now())
        } else {
            None
        };
        if let Err(e) = event_loop.dispatch(next_timeout, &mut engine_state) {
            log::error!("[window_runner] event loop error, ending session: {e:?}");
            end = SessionEnd::ConnectionLost;
            break;
        }
        if let Some(start) = iter_start {
            let t = debug_clock_ms();
            eprintln!(
                "[vk] t={} loop dispatch={}us pending_cb={}",
                t,
                start.elapsed().as_micros(),
                engine_state.frame_callback_pending
            );
        }
        // A protocol error kills the connection permanently, but it surfaces
        // through queue flushes whose errors calloop's WaylandSource swallows
        // (it only treats Io errors as fatal) — without this check the loop
        // spins forever on a dead display while wayland-backend re-prints the
        // error on every flush attempt.
        if let Some(perr) = conn.protocol_error() {
            log::error!("[window_runner] wayland protocol error, ending session: {perr}");
            end = SessionEnd::ConnectionLost;
            break;
        }
        // Fault injection for the reconnect path (`CCE_UI_FAULT_RECONNECT=<secs>`):
        // real connection loss is a rare race that cannot be provoked on demand,
        // so this drops the session exactly as a transport error would. One-shot
        // per process, so the app reconnects and then stays up.
        if let Some(after) = fault_reconnect_after() {
            static FIRED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if session_start.elapsed() >= after
                && !FIRED.swap(true, std::sync::atomic::Ordering::Relaxed)
            {
                log::warn!("[window_runner] CCE_UI_FAULT_RECONNECT: dropping the session");
                end = SessionEnd::ConnectionLost;
                break;
            }
        }
        match pacer.turn(&mut engine_state) {
            Step::Sleep(timeout) => next_timeout = timeout,
            Step::Exit => {
                // The close dissolve. It is the COMPOSITOR that fades us — it
                // ramps our scene subtree's opacity, which takes the backdrop
                // blur, drop shadow and bevel down with the window; all this side
                // has to do is not vanish before it finishes. So keep the surface
                // mapped and the loop turning for exactly as long as the
                // compositor asked for, then leave. Dispatching (rather than
                // sleeping) keeps the connection pumped and lets any last
                // animation finish on screen while the window dissolves.
                let fade = crate::ipc::request_close_fade();
                if !fade.is_zero() {
                    let until = std::time::Instant::now() + fade;
                    loop {
                        let left = until.saturating_duration_since(std::time::Instant::now());
                        if left.is_zero() {
                            break;
                        }
                        if event_loop.dispatch(left.min(ACTIVE_DISPATCH), &mut engine_state).is_err() {
                            break;
                        }
                    }
                }
                break;
            }
        }
    }

    // Tear the session down: drop its Wayland source from the persistent loop
    // (leaving it would leak a dead source per reconnect), then hand the app
    // back before `engine_state` drops the renderer and the surface with it.
    // `on_exit` and process cleanup belong to the app's real exit, in `run`.
    loop_handle.remove(wayland_token);
    let app = engine_state.inner.take();
    drop(engine_state);
    (app, end)
}
