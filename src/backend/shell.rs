//! The run loop's shared half: what one turn of the loop does once a shell
//! has dispatched whatever its window system delivered. A [`Shell`] is a
//! window system's side of the contract — the Wayland runner's `EngineState`
//! is one — and a [`Pacer`] drives it one [`turn`](Pacer::turn) at a time:
//! the app's tick at a sane `dt`, its requested size and title, key repeat,
//! the decision to present (fresh, or a warm-down re-render) and the cadence
//! the loop should sleep at until the next turn.
//!
//! Before this the turn was written into the Wayland runner's loop, between
//! a calloop dispatch and a protocol-error check. A shell with a different
//! loop — the browser's animation frames, an AppKit run loop — calls `turn`
//! from wherever its loop turns and sleeps (or schedules) for what it says.

use std::time::Duration;
use web_time::Instant;

use super::app::Application;
use super::driver::{Driver, Turn};

/// Loop cadence while something is in motion: one turn per frame.
pub const ACTIVE_DISPATCH: Duration = Duration::from_millis(16);

/// Default cap on the runner's idle sleep — see `Application::idle_poll_interval`.
pub const IDLE_DISPATCH: Duration = Duration::from_millis(1000);

/// How long the cadence stays at frame rate after the last genuine redraw.
///
/// Sparse, isolated commits get their frame callbacks serviced multiple
/// compositor frames late (measured 22-128ms on cce-fx, growing per sparse
/// commit), while a continuously committing surface is serviced in one frame
/// (~16ms). A short warm-down keeps interactive sequences (hover, typing,
/// scrolling) in the healthy continuous regime; idle still idles.
///
/// A warm-down step need not draw: the Wayland shell commits a frame callback
/// with no buffer (`EngineState::keepalive_commit`). Until 2026-10-05 it
/// re-rendered the whole frame and presented it with full damage — about 12
/// identical frames after every hover or keystroke, each re-blurred by the
/// compositor.
pub const WARM_DOWN: Duration = Duration::from_millis(200);

/// A redraw this soon after the previous one, or after input, belongs to a
/// sequence and gets the [`WARM_DOWN`]; one further from both is an
/// isolated update and does not.
///
/// The warm-down is for interaction and for streams (a terminal printing, a
/// sync narrating progress): a run of commits close together, where a late
/// frame callback would delay the next one. An app's own isolated update —
/// the status bar's stats once a second, a clock once a minute — has no next
/// frame to delay, and each one used to buy twelve more wakes at frame rate:
/// the stats module woke ~13 times a second for one redraw (2026-10-06).
pub const SEQUENCE_GAP: Duration = Duration::from_millis(500);

/// Upper bound on an idle sleep. The loop is woken early by any event the
/// shell's window system delivers and by messages on the app's sender, so
/// this only caps how long an app-side poll that bypasses both (see
/// `Application::idle_poll_interval`) can wait. `CCE_UI_IDLE_MS` overrides
/// it — `16` restores the old always-ticking loop for a bisect.
pub fn idle_dispatch() -> Duration {
    static IDLE: std::sync::OnceLock<Duration> = std::sync::OnceLock::new();
    *IDLE.get_or_init(|| {
        std::env::var("CCE_UI_IDLE_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or(IDLE_DISPATCH)
    })
}

/// A window system's side of the run loop.
pub trait Shell {
    type App: Application;

    /// The input driver and the app's turn, borrowed apart.
    fn turn(&mut self) -> (&mut Driver, Turn<'_, Self::App>);

    fn app(&self) -> &Self::App;

    /// The runner's dirty flag: a frame is wanted.
    fn redraw(&mut self) -> &mut bool;

    /// The app asked to exit (its `update` set the flag).
    fn exit_requested(&self) -> bool;

    /// The window system sized the window since the last turn: this turn the
    /// app's own `desired_size` is not asked (the configure wins). Clears it.
    fn take_just_configured(&mut self) -> bool;

    /// The app wants its window `w` x `h` (frame px). The shell resizes — or
    /// does not, if it already is — and raises `redraw` when it did.
    fn request_size(&mut self, w: u32, h: u32);

    /// Once a turn, after the app's tick and size: whatever the window system
    /// keeps in step with the app (the Wayland shell's overflow rim, popover
    /// region and menu popup).
    fn sync(&mut self) {}

    /// The app's title changed.
    fn set_title(&mut self, title: &str);

    /// A presented frame has not yet been released by the window system's
    /// pacing (Wayland: the frame callback is outstanding), so presenting now
    /// must wait. Asked once a turn, with `redraw` already known, so a shell
    /// can give up on a release that is never coming.
    fn frame_pending(&mut self) -> bool;

    /// The window has been configured: there is a surface to present on.
    fn configured(&self) -> bool;

    /// Build and present a frame. `fresh` is a frame something asked for;
    /// otherwise it is a warm-down step (see [`WARM_DOWN`]): nothing changed,
    /// so a shell that can keep its pacing without drawing should.
    fn present(&mut self, fresh: bool);
}

/// What the loop should do after a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// The app asked to exit; the session ends (after whatever leave-taking
    /// the shell does — the Wayland shell waits out the compositor's fade).
    Exit,
    /// Turn again after at most this long, or sooner if an event arrives.
    Sleep(Duration),
}

/// The loop's pacing state, one per session.
pub struct Pacer {
    last_tick: Instant,
    /// The loop slept idle before this turn: its interval is not animation time.
    slept_idle: bool,
    /// Turns stay at frame rate until this instant (see [`WARM_DOWN`]).
    warm_until: Option<Instant>,
    /// When the last genuine redraw was presented (see [`SEQUENCE_GAP`]).
    last_redraw: Option<Instant>,
    last_title: String,
}

impl Pacer {
    /// Before the first turn the loop runs at [`ACTIVE_DISPATCH`].
    pub fn new(title: String) -> Self {
        Self { last_tick: Instant::now(), slept_idle: false, warm_until: None, last_redraw: None, last_title: title }
    }

    /// One turn of the loop. The cadence is ACTIVE while anything is in
    /// motion (a redraw pending or just done, an animation, a held key, the
    /// post-activity warm-down); otherwise the app's own poll interval or
    /// [`idle_dispatch`]. Before 2026-09-11 it was a flat 16 ms whatever the
    /// state: every cce-ui client woke 60 times a second forever — ~1200
    /// wakeups/s across a session's twenty clients — and each wake ran tick,
    /// desired_size, title and margin checks for nothing.
    pub fn turn<S: Shell>(&mut self, shell: &mut S) -> Step {
        if shell.exit_requested() {
            return Step::Exit;
        }

        let now = Instant::now();
        let mut dt = now.duration_since(self.last_tick).as_secs_f32();
        self.last_tick = now;
        if dt > 0.1 {
            dt = 0.1;
        }
        // Waking from an idle sleep: the interval is not animation time. An
        // animation an event just started must take its first step at frame
        // size, not leap 100 ms in one tick.
        if self.slept_idle {
            dt = dt.min(1.0 / 60.0);
        }

        {
            let (driver, t) = shell.turn();
            driver.tick(t, dt);
        }

        if !shell.take_just_configured() {
            if let Some((w, h)) = shell.app().desired_size() {
                shell.request_size(w, h);
            }
        }

        shell.sync();

        {
            let (driver, t) = shell.turn();
            driver.repeat_keys(t);
        }
        let title = shell.app().settings().title;
        if title != self.last_title {
            shell.set_title(&title);
            self.last_title = title;
        }

        let pending = shell.frame_pending();

        if *shell.redraw() {
            // Genuine dirt that is part of a sequence — input just arrived, or
            // the last redraw was moments ago — extends the warm window;
            // warm-down renders below do NOT, so idle decays in one window.
            // An isolated update gets none (see `SEQUENCE_GAP`).
            let now = Instant::now();
            let recent = |t: Option<Instant>| t.is_some_and(|t| now.duration_since(t) < SEQUENCE_GAP);
            let input = shell.turn().0.last_input;
            if recent(input) || recent(self.last_redraw) {
                self.warm_until = Some(now + WARM_DOWN);
            }
            self.last_redraw = Some(now);
        }
        let mut rendered = false;
        if *shell.redraw() && !pending {
            *shell.redraw() = false;
            if shell.configured() {
                shell.present(true);
                rendered = true;
            }
        } else if !*shell.redraw() && !pending && self.warm_until.is_some_and(|t| Instant::now() < t) {
            // Warm-down re-render, paced by the shell's frame release.
            if shell.configured() {
                shell.present(false);
                rendered = true;
            }
        }

        // Anything still moving keeps the frame cadence; a pending frame on
        // its own does not (its release arrives as an event) unless a redraw
        // is queued behind it, which is what the shell's `frame_pending`
        // times. `redraw` still set here means the frame was withheld (a
        // frame pending, or no configure yet) and must be retried soon.
        let warm = self.warm_until.is_some_and(|t| Instant::now() < t);
        let key_held = shell.turn().0.pressed_key.is_some();
        let busy = *shell.redraw() || rendered || warm || key_held;
        self.slept_idle = !busy;
        Step::Sleep(if busy {
            ACTIVE_DISPATCH
        } else {
            let app_poll = shell.app().idle_poll_interval();
            app_poll.map_or(idle_dispatch(), |d| d.min(idle_dispatch()))
        })
    }
}

#[cfg(test)]
mod tests {
    //! The loop's policy, turned by hand against a shell that records what
    //! it was asked to do.
    use super::*;
    use crate::backend::app::{AppSender, LogicalPosition, WindowSettings};
    use crate::widget::{ElementState, Key, KeyEvent, MouseButton, MouseScrollDelta};

    #[derive(Default)]
    struct App {
        title: String,
        desired: Option<(u32, u32)>,
        poll: Option<Duration>,
        /// Each tick's dt.
        ticks: Vec<f32>,
        /// Ask for a frame from the next tick.
        animate: bool,
    }

    impl Application for App {
        type Message = ();
        fn create(_: AppSender<()>) -> Self {
            unreachable!("built directly")
        }
        fn settings(&self) -> WindowSettings {
            WindowSettings { title: self.title.clone(), app_id: "mock".into(), width: 10, height: 10, fullscreen: false, min_size: None }
        }
        fn update(&mut self, _: (), _: &mut bool, _: &mut bool) {}
        fn tick(&mut self, dt: f32, needs_rebuild: &mut bool) {
            self.ticks.push(dt);
            *needs_rebuild |= self.animate;
        }
        fn handle_pointer_move(&mut self, _: LogicalPosition, _: &mut bool) {}
        fn handle_mouse_input(&mut self, _: MouseButton, _: ElementState, _: LogicalPosition, _: &mut bool) -> Option<()> {
            None
        }
        fn handle_mouse_wheel(&mut self, _: &MouseScrollDelta, _: LogicalPosition, _: &mut bool) {}
        fn handle_key_input(&mut self, _: &KeyEvent, _: &mut bool) -> Option<()> {
            None
        }
        fn desired_size(&self) -> Option<(u32, u32)> {
            self.desired
        }
        fn idle_poll_interval(&self) -> Option<Duration> {
            self.poll
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    enum Did {
        Size(u32, u32),
        Title(String),
        Present(bool),
    }

    struct Mock {
        app: App,
        driver: Driver,
        redraw: bool,
        exit: bool,
        just_configured: bool,
        pending: bool,
        configured: bool,
        did: Vec<Did>,
    }

    impl Mock {
        fn new() -> Self {
            Self {
                app: App::default(),
                driver: Driver::new(),
                redraw: false,
                exit: false,
                just_configured: false,
                pending: false,
                configured: true,
                did: Vec::new(),
            }
        }
    }

    impl Shell for Mock {
        type App = App;
        fn turn(&mut self) -> (&mut Driver, Turn<'_, App>) {
            (&mut self.driver, Turn { app: &mut self.app, redraw: &mut self.redraw, exit: &mut self.exit })
        }
        fn app(&self) -> &App {
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
            self.did.push(Did::Size(w, h));
        }
        fn set_title(&mut self, title: &str) {
            self.did.push(Did::Title(title.into()));
        }
        fn frame_pending(&mut self) -> bool {
            self.pending
        }
        fn configured(&self) -> bool {
            self.configured
        }
        fn present(&mut self, fresh: bool) {
            self.did.push(Did::Present(fresh));
        }
    }

    fn idle() -> Step {
        Step::Sleep(idle_dispatch())
    }

    #[test]
    fn a_quiet_window_sleeps_idle_or_at_the_apps_poll() {
        let (mut p, mut s) = (Pacer::new(String::new()), Mock::new());
        assert_eq!(p.turn(&mut s), idle());
        assert!(s.did.is_empty(), "nothing to do: {:?}", s.did);

        s.app.poll = Some(Duration::from_millis(250));
        assert_eq!(p.turn(&mut s), Step::Sleep(Duration::from_millis(250).min(idle_dispatch())));
    }

    #[test]
    fn a_redraw_presents_then_warms_down_at_frame_rate() {
        let (mut p, mut s) = (Pacer::new(String::new()), Mock::new());
        // Interactive: an input event just arrived.
        s.driver.last_input = Some(Instant::now());
        s.redraw = true;
        assert_eq!(p.turn(&mut s), Step::Sleep(ACTIVE_DISPATCH));
        assert_eq!(s.did, vec![Did::Present(true)]);
        assert!(!s.redraw);

        // Inside the warm window: re-render, and keep the frame cadence.
        assert_eq!(p.turn(&mut s), Step::Sleep(ACTIVE_DISPATCH));
        assert_eq!(s.did.last(), Some(&Did::Present(false)));
    }

    #[test]
    fn an_isolated_update_presents_and_goes_idle_without_a_warm_down() {
        let (mut p, mut s) = (Pacer::new(String::new()), Mock::new());
        // No input, no recent redraw: the app's own once-a-second update.
        s.redraw = true;
        assert_eq!(p.turn(&mut s), Step::Sleep(ACTIVE_DISPATCH));
        assert_eq!(s.did, vec![Did::Present(true)]);
        // No warm-down re-render: straight to the idle sleep.
        assert_eq!(p.turn(&mut s), idle());
        assert_eq!(s.did, vec![Did::Present(true)], "an isolated update warmed down");
    }

    #[test]
    fn a_redraw_soon_after_another_warms_down_without_input() {
        let (mut p, mut s) = (Pacer::new(String::new()), Mock::new());
        s.redraw = true;
        p.turn(&mut s);
        p.turn(&mut s);
        // A stream: the next update lands well inside SEQUENCE_GAP.
        s.redraw = true;
        assert_eq!(p.turn(&mut s), Step::Sleep(ACTIVE_DISPATCH));
        assert_eq!(p.turn(&mut s), Step::Sleep(ACTIVE_DISPATCH));
        assert_eq!(s.did.last(), Some(&Did::Present(false)), "the stream lost its warm-down");
    }

    #[test]
    fn a_pending_frame_withholds_the_present_and_keeps_the_redraw() {
        let (mut p, mut s) = (Pacer::new(String::new()), Mock::new());
        s.redraw = true;
        s.pending = true;
        assert_eq!(p.turn(&mut s), Step::Sleep(ACTIVE_DISPATCH));
        assert!(s.did.is_empty());
        assert!(s.redraw, "the frame is withheld, not dropped");

        s.pending = false;
        p.turn(&mut s);
        assert_eq!(s.did, vec![Did::Present(true)]);
    }

    #[test]
    fn before_the_first_configure_a_redraw_is_spent_without_a_present() {
        // What the loop always did: the configure itself raises a redraw.
        let (mut p, mut s) = (Pacer::new(String::new()), Mock::new());
        s.configured = false;
        s.redraw = true;
        p.turn(&mut s);
        assert!(s.did.is_empty());
        assert!(!s.redraw);
    }

    #[test]
    fn the_apps_size_and_title_reach_the_shell_and_a_configure_wins_its_turn() {
        let (mut p, mut s) = (Pacer::new("a".into()), Mock::new());
        s.app.desired = Some((300, 200));
        s.app.title = "b".into();
        s.just_configured = true;
        p.turn(&mut s);
        assert_eq!(s.did, vec![Did::Title("b".into())], "no size on a configure's turn, the title once");

        s.did.clear();
        p.turn(&mut s);
        assert_eq!(s.did, vec![Did::Size(300, 200)], "the size on the next; the title is unchanged");
    }

    #[test]
    fn a_held_key_keeps_the_frame_cadence() {
        let (mut p, mut s) = (Pacer::new(String::new()), Mock::new());
        let (driver, t) = s.turn();
        driver.key(t, Key::Character("a".into()), Some("a".into()), ElementState::Pressed);
        // The key's own dispatch asked for nothing; the hold alone keeps it busy.
        s.redraw = false;
        assert_eq!(p.turn(&mut s), Step::Sleep(ACTIVE_DISPATCH));
    }

    #[test]
    fn an_exit_ends_the_turn_before_anything_ticks() {
        let (mut p, mut s) = (Pacer::new(String::new()), Mock::new());
        s.exit = true;
        assert_eq!(p.turn(&mut s), Step::Exit);
        assert!(s.app.ticks.is_empty());
    }

    #[test]
    fn the_first_tick_after_an_idle_sleep_is_one_frame_long() {
        let (mut p, mut s) = (Pacer::new(String::new()), Mock::new());
        assert_eq!(p.turn(&mut s), idle());
        std::thread::sleep(Duration::from_millis(40));
        // Woken from idle by something that animates: its first step is a frame.
        s.app.animate = true;
        p.turn(&mut s);
        let woke = *s.app.ticks.last().unwrap();
        assert!(woke <= 1.0 / 60.0 + 1e-6, "dt after an idle sleep: {woke}");

        // Busy now, so the next interval IS animation time, up to 100 ms.
        std::thread::sleep(Duration::from_millis(40));
        p.turn(&mut s);
        let busy = *s.app.ticks.last().unwrap();
        assert!((0.035..=0.1).contains(&busy), "dt while busy: {busy}");
    }
}
