//! What happens when a session ends: the app exits, the connection is lost (reconnect with
//! backoff, giving up after enough quick failures), or the compositor is gone (exit, or for an
//! app that outlives it, wait for a successor's socket).

/// Why a session's event loop stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SessionEnd {
    /// The app asked to exit.
    AppExit,
    /// The compositor connection died while the compositor itself may well be
    /// alive — a broken transport. The `Application` is intact and can be
    /// re-attached to a fresh connection.
    ConnectionLost,
    /// Nothing answered at the display socket: the compositor this app
    /// belonged to is gone. A deliberate exit unlinks the socket and a crash
    /// leaves it refusing; either way there is no session left to rejoin.
    NoCompositor,
}

/// What [`run`] does once a session has ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AfterSession {
    /// Leave the process-lifetime loop: run `on_exit` and quit.
    Exit,
    /// Sleep this long, then open a fresh session on the same `Application`.
    Reconnect(std::time::Duration),
    /// The compositor is gone and the app outlives it
    /// ([`Application::outlives_compositor`]): wait for a successor's socket,
    /// then open a fresh session on the same `Application`.
    AwaitCompositor,
}

/// The display socket this process connects to: `$WAYLAND_DISPLAY` (absolute,
/// or a name under `$XDG_RUNTIME_DIR`), `wayland-0` when unset — the lookup
/// `Connection::connect_to_env` makes.
pub(super) fn wayland_socket_path() -> Option<std::path::PathBuf> {
    let name = std::env::var_os("WAYLAND_DISPLAY").unwrap_or_else(|| "wayland-0".into());
    let name = std::path::PathBuf::from(name);
    if name.is_absolute() {
        return Some(name);
    }
    Some(std::path::PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?).join(name))
}

/// Sleep until the display socket exists again — the successor compositor
/// has bound it. Polled at 250 ms: a quarter-second after the next login is
/// soon enough, and a daemon waiting through a logged-out hour costs four
/// `stat`s a second. A stale socket a crash left behind satisfies the poll
/// and fails the connect, which comes back here after the same pause.
pub(super) fn await_compositor_socket() {
    loop {
        std::thread::sleep(std::time::Duration::from_millis(250));
        match wayland_socket_path() {
            Some(path) if path.exists() => return,
            Some(_) => {}
            // No runtime dir to look in: keep trying the connect itself.
            None => return,
        }
    }
}

/// How many consecutive failed reconnects before giving up. Reset once a
/// session has survived [`RECONNECT_RESET`], so a long-lived window that loses
/// its connection twice in a day still gets a full budget the second time.
pub(super) const RECONNECT_ATTEMPTS: u32 = 8;

pub(super) const RECONNECT_RESET: std::time::Duration = std::time::Duration::from_secs(10);

/// Decide whether a finished session is followed by another.
///
/// `lived` is how long the session that just ended lasted, `has_app` whether
/// an `Application` exists to carry over, and `attempt` the running count of
/// consecutive reconnects (reset here once a session outlives
/// [`RECONNECT_RESET`]).
///
/// Only a lost connection is retried, and only while the compositor is still
/// there to reconnect to. A reconnect is a repair of THIS session's transport
/// — the fd-exhaustion break `raise_fd_limit` documents — not a way to outlive
/// the compositor. When the connect itself fails the compositor has exited,
/// and it has already saved this window for restore: the next compositor
/// respawns the app from `state.json` on its own. A client that kept
/// retrying instead (the backoff below spans ~25s) reattached to that
/// successor beside the respawned copy, and every restore after a forced
/// exit or a crash came up with two of each cce-ui window. So the process
/// exits, as a Wayland client whose display went away always has.
///
/// Unless the app OUTLIVES the compositor (`outlives`,
/// [`Application::outlives_compositor`]) — a daemon the compositor does not
/// restore. Then there is no copy to collide with and every reason to stay:
/// it waits for the successor and rejoins it.
pub(super) fn after_session(
    end: SessionEnd,
    has_app: bool,
    lived: std::time::Duration,
    attempt: &mut u32,
    outlives: bool,
) -> AfterSession {
    match end {
        SessionEnd::NoCompositor if has_app && outlives => {
            *attempt = 0;
            AfterSession::AwaitCompositor
        }
        SessionEnd::AppExit | SessionEnd::NoCompositor => AfterSession::Exit,
        SessionEnd::ConnectionLost => {
            // Nothing to preserve if we never got as far as building the
            // app — that is a failure to start, not a lost window.
            if !has_app {
                return AfterSession::Exit;
            }
            if lived > RECONNECT_RESET {
                *attempt = 0;
            }
            *attempt += 1;
            if *attempt > RECONNECT_ATTEMPTS {
                return AfterSession::Exit;
            }
            AfterSession::Reconnect(std::time::Duration::from_millis(
                100 * (1 << (*attempt).min(6)),
            ))
        }
    }
}

#[cfg(test)]
mod reconnect_tests {
    use super::{after_session, AfterSession, SessionEnd, RECONNECT_ATTEMPTS, RECONNECT_RESET};
    use std::time::Duration;

    const LONG: Duration = Duration::from_secs(60);
    const SHORT: Duration = Duration::from_millis(50);

    #[test]
    fn app_exit_ends_the_process() {
        let mut attempt = 0;
        assert_eq!(after_session(SessionEnd::AppExit, true, LONG, &mut attempt, false), AfterSession::Exit);
        assert_eq!(attempt, 0);
    }

    #[test]
    fn lost_transport_reconnects_with_backoff() {
        let mut attempt = 0;
        assert_eq!(
            after_session(SessionEnd::ConnectionLost, true, LONG, &mut attempt, false),
            AfterSession::Reconnect(Duration::from_millis(200))
        );
        assert_eq!(attempt, 1);
        assert_eq!(
            after_session(SessionEnd::ConnectionLost, true, SHORT, &mut attempt, false),
            AfterSession::Reconnect(Duration::from_millis(400))
        );
        assert_eq!(attempt, 2);
    }

    /// The compositor exited (its socket is unlinked, or refusing after a
    /// crash). It saved this window for restore, so the successor respawns
    /// the app itself; a client that waited for it reattached beside the
    /// respawned copy, and the restore came up with two of every window.
    #[test]
    fn compositor_gone_exits_instead_of_waiting_for_a_successor() {
        let mut attempt = 0;
        assert_eq!(
            after_session(SessionEnd::NoCompositor, true, LONG, &mut attempt, false),
            AfterSession::Exit
        );
        // Even mid-budget: a reconnect that finds nobody listening is the
        // compositor leaving, not another transport break.
        let mut attempt = 3;
        assert_eq!(
            after_session(SessionEnd::NoCompositor, true, SHORT, &mut attempt, false),
            AfterSession::Exit
        );
    }

    /// A daemon the compositor does not restore (the status bar, the
    /// notifier) waits for the successor instead — with no copy to collide
    /// with, exiting only took its D-Bus names down with it. It starts a fresh
    /// budget, and a transport break still reconnects as before.
    #[test]
    fn an_app_that_outlives_the_compositor_waits_for_the_next() {
        let mut attempt = 3;
        assert_eq!(
            after_session(SessionEnd::NoCompositor, true, SHORT, &mut attempt, true),
            AfterSession::AwaitCompositor
        );
        assert_eq!(attempt, 0);
        assert_eq!(
            after_session(SessionEnd::ConnectionLost, true, LONG, &mut attempt, true),
            AfterSession::Reconnect(Duration::from_millis(200))
        );
        // Asked to exit, or never started: it still goes.
        assert_eq!(after_session(SessionEnd::AppExit, true, LONG, &mut attempt, true), AfterSession::Exit);
        assert_eq!(after_session(SessionEnd::NoCompositor, false, SHORT, &mut attempt, true), AfterSession::Exit);
    }

    #[test]
    fn nothing_to_carry_over_gives_up() {
        let mut attempt = 0;
        assert_eq!(
            after_session(SessionEnd::ConnectionLost, false, SHORT, &mut attempt, false),
            AfterSession::Exit
        );
        assert_eq!(
            after_session(SessionEnd::NoCompositor, false, SHORT, &mut attempt, false),
            AfterSession::Exit
        );
    }

    #[test]
    fn budget_is_bounded_and_resets_after_a_long_session() {
        let mut attempt = 0;
        for _ in 0..RECONNECT_ATTEMPTS {
            assert!(matches!(
                after_session(SessionEnd::ConnectionLost, true, SHORT, &mut attempt, false),
                AfterSession::Reconnect(_)
            ));
        }
        assert_eq!(
            after_session(SessionEnd::ConnectionLost, true, SHORT, &mut attempt, false),
            AfterSession::Exit
        );
        // A session that outlived the reset window earns a fresh budget.
        assert_eq!(
            after_session(SessionEnd::ConnectionLost, true, RECONNECT_RESET + SHORT, &mut attempt, false),
            AfterSession::Reconnect(Duration::from_millis(200))
        );
        assert_eq!(attempt, 1);
    }

    #[test]
    fn backoff_caps_at_six_point_four_seconds() {
        let mut attempt = 6;
        assert_eq!(
            after_session(SessionEnd::ConnectionLost, true, SHORT, &mut attempt, false),
            AfterSession::Reconnect(Duration::from_millis(6400))
        );
        assert_eq!(
            after_session(SessionEnd::ConnectionLost, true, SHORT, &mut attempt, false),
            AfterSession::Reconnect(Duration::from_millis(6400))
        );
    }
}
