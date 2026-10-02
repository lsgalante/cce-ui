//! Single-instance apps over the CCE socket convention: claim or forward.
//!
//! An app that should run once per session (a link opened from elsewhere
//! becomes a tab, `cce-notes open X` shows X in the running window) listens
//! on `/tmp/<prefix>-<WAYLAND_DISPLAY>.sock` — keyed by display, so shadow
//! sessions stay apart for free — and every later launch hands it one line
//! and exits before any Wayland or engine work.
//!
//! ```ignore
//! fn main() {
//!     if cce_ui::ipc::instance::forward_or_claim("cce-foo", &launch_line()) {
//!         return; // the running instance took it
//!     }
//!     cce_ui::engine::run::<Foo>();
//!     cce_ui::ipc::instance::cleanup();
//! }
//!
//! // in Application::new, once the loop's sender exists:
//! cce_ui::ipc::instance::serve(move |line| {
//!     let msg = parse(line)?;            // None: hang up without a reply
//!     sender.send(msg).ok()?;
//!     Some("ok".into())
//! });
//! ```
//!
//! The order in [`forward_or_claim`] is what closes the startup race: try to
//! connect, and only bind after a connect has failed. A refused connection
//! means the socket file outlived a crashed instance and is removed before
//! binding; losing the bind to a simultaneous launch falls back to one more
//! connect. If that also fails the launch proceeds un-listened rather than
//! not at all.
//!
//! The claimed listener has to survive from `main()` (before the engine
//! starts) to `Application::new` (where the app's loop sender first exists),
//! and `engine::run` takes no arguments, so it parks in this module until
//! [`serve`] adopts it. One claim per process.
//!
//! The wire protocol is the app's: one request line in, one reply line out.
//! Anything a forwarded argument needs to mean the same thing in the
//! instance — a relative path made absolute against the *sender's* cwd —
//! is the caller's to do before building the line.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::Mutex;
use std::time::Duration;

/// The listener claimed by [`forward_or_claim`], waiting for [`serve`].
static CLAIMED: Mutex<Option<UnixListener>> = Mutex::new(None);
/// The socket path this process bound (and must unlink on exit), if any.
static OWNED_PATH: Mutex<Option<String>> = Mutex::new(None);

/// How long a launch waits for the running instance to answer. Bounded so an
/// instance whose listener is stuck cannot hang every later launch forever;
/// unanswered, the launch is not forwarded.
const FORWARD_TIMEOUT: Duration = Duration::from_secs(5);

/// How long, and how much, the listener reads from one client before giving
/// up on it and serving the next (see [`super::read_request_line`]).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);
const REQUEST_LIMIT: usize = 64 * 1024;

/// Send `line` to the instance listening on `prefix`'s socket and return its
/// reply line, trimmed.
///
/// `None` when nothing is listening, the write fails, or no reply arrives
/// within the timeout. `Some("")` when the instance read the line and hung
/// up without answering — it still received it. The client half alone, for
/// an app that talks to another app's instance.
pub fn forward(prefix: &str, line: &str) -> Option<String> {
    forward_to(&super::socket_path(prefix), line)
}

fn forward_to(path: &str, line: &str) -> Option<String> {
    let mut stream = UnixStream::connect(path).ok()?;
    let mut msg = line.trim_end_matches('\n').to_string();
    msg.push('\n');
    stream.write_all(msg.as_bytes()).ok()?;
    // Wait for the answer: returning (and exiting) on the write alone races
    // the instance actually reading the line.
    stream.set_read_timeout(Some(FORWARD_TIMEOUT)).ok()?;
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply).ok()?;
    Some(reply.trim().to_string())
}

/// Hand `line` to a running instance, or claim the instance socket.
///
/// `true` when a running instance took the launch: the caller should exit
/// without starting. `false` when this process is now the instance — the
/// listener parked for [`serve`] — or when single-instance handling failed
/// entirely and the launch should proceed standalone.
pub fn forward_or_claim(prefix: &str, line: &str) -> bool {
    let path = super::socket_path(prefix);
    if forward_to(&path, line).is_some() {
        return true;
    }
    // Nothing answered. A socket file that still exists is a leftover from a
    // crashed instance; binding needs it gone.
    if std::path::Path::new(&path).exists() {
        let _ = std::fs::remove_file(&path);
    }
    match UnixListener::bind(&path) {
        Ok(listener) => {
            *CLAIMED.lock().unwrap_or_else(|e| e.into_inner()) = Some(listener);
            *OWNED_PATH.lock().unwrap_or_else(|e| e.into_inner()) = Some(path);
            false
        }
        // Lost the bind race to a simultaneous launch: it is the instance.
        Err(_) => forward_to(&path, line).is_some(),
    }
}

/// Serve the listener [`forward_or_claim`] claimed, on a thread of its own.
///
/// `handle` gets each request line, trimmed, and returns the reply line to
/// write back (a trailing newline is added), or `None` to hang up without
/// one. It runs on the listener thread, so it should hand work to the app's
/// loop (a calloop channel) rather than do it; answering straight from
/// shared state is fine for a query the loop need not see.
///
/// Each client is read under a total deadline and size cap, so one that
/// connects and says nothing cannot wedge the listener for every launch
/// after it. `false` when this process claimed nothing (it runs standalone,
/// or `serve` already took the listener).
pub fn serve<F>(mut handle: F) -> bool
where
    F: FnMut(&str) -> Option<String> + Send + 'static,
{
    let Some(listener) = CLAIMED.lock().unwrap_or_else(|e| e.into_inner()).take() else {
        return false;
    };
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(conn) = conn else { continue };
            let Some(line) = super::read_request_line(&conn, REQUEST_LIMIT, REQUEST_TIMEOUT) else {
                continue;
            };
            if let Some(mut reply) = handle(line.trim()) {
                reply.push('\n');
                let _ = (&conn).write_all(reply.as_bytes());
            }
        }
    });
    true
}

/// Unlink the socket if this process bound it. Call it after the engine loop
/// returns; a crash skips it, which is what the stale-socket removal in
/// [`forward_or_claim`] exists for.
pub fn cleanup() {
    if let Some(path) = OWNED_PATH.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole round trip in one process: the first call claims, `serve`
    /// answers, a second launch is forwarded and sees the reply, and
    /// `cleanup` removes the socket. One test, because the claim is
    /// process-wide.
    #[test]
    fn claim_serve_forward_cleanup() {
        let prefix = format!("cce-ui-instance-test-{}", std::process::id());
        let path = crate::ipc::socket_path(&prefix);
        // A stale file from a "crashed" instance is replaced, not fatal.
        std::fs::write(&path, b"").unwrap();

        assert!(!forward_or_claim(&prefix, "first"), "nothing was running: this launch is the instance");
        let (tx, rx) = std::sync::mpsc::channel();
        assert!(serve(move |line| {
            tx.send(line.to_string()).ok()?;
            match line {
                "silent" => None,
                l => Some(format!("ok {l}")),
            }
        }));
        assert!(!serve(|_| None), "the listener is served once");

        assert_eq!(forward(&prefix, "open x\n").as_deref(), Some("ok open x"));
        assert!(forward_or_claim(&prefix, "second"), "a running instance takes the launch");
        // Read and hung up on: still received.
        assert_eq!(forward(&prefix, "silent").as_deref(), Some(""));
        let got: Vec<String> = rx.try_iter().collect();
        assert_eq!(got, ["open x", "second", "silent"]);

        // A client that connects and says nothing does not wedge the listener.
        let _quiet = UnixStream::connect(&path).unwrap();
        let t = std::time::Instant::now();
        assert_eq!(forward(&prefix, "after").as_deref(), Some("ok after"));
        assert!(t.elapsed() < FORWARD_TIMEOUT, "took {:?}", t.elapsed());

        cleanup();
        assert!(!std::path::Path::new(&path).exists());
        assert_eq!(forward(&prefix, "gone"), None);
    }
}
