//! Helpers for the CCE Unix-socket IPC convention: `/tmp/<prefix>-<WAYLAND_DISPLAY>.sock`.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

pub mod instance;

/// Path of a CCE IPC socket for `prefix`, keyed by `$WAYLAND_DISPLAY`.
///
/// `socket_path("cce")` → `/tmp/cce-<display>.sock` (the compositor control socket);
/// `socket_path("cce-status-interface")` → the status socket. Falls back to
/// `/tmp/<prefix>.sock` when `$WAYLAND_DISPLAY` is unset.
pub fn socket_path(prefix: &str) -> String {
    match std::env::var("WAYLAND_DISPLAY") {
        Ok(d) if !d.is_empty() => format!("/tmp/{}-{}.sock", prefix, d),
        _ => format!("/tmp/{}.sock", prefix),
    }
}

/// Connect to the `prefix` socket, send `command` (newline-terminated), and
/// return the reply text. Errors if the socket can't be reached.
pub fn send_command(prefix: &str, command: &str) -> std::io::Result<String> {
    let mut stream = UnixStream::connect(socket_path(prefix))?;
    stream.write_all(command.as_bytes())?;
    if !command.ends_with('\n') {
        stream.write_all(b"\n")?;
    }
    let mut reply = String::new();
    stream.read_to_string(&mut reply)?;
    Ok(reply)
}

/// Ask the compositor to dissolve this client's surfaces out, and return how
/// long it says that will take.
///
/// The fade is the compositor's, not the app's: it ramps the opacity of the
/// scene subtree, which carries the backdrop blur, the drop shadow and the
/// bevel down with the window. A client fading its own pixels instead leaves
/// its surface fully present, so the blur behind it hangs at full strength
/// over a dissolving window — and any part of its drawing that is not plain
/// vertex alpha (shader-lit plate rims, specular) does not fade at all.
///
/// The contract is that the caller keeps its surfaces mapped and its process
/// alive for the returned duration and only then exits. `window_runner` does
/// that for every [`Application`](crate::backend::window_runner::Application);
/// an app driving its own event loop calls this itself. Zero — no compositor,
/// nothing of ours on screen, or fading configured off — means exit now.
pub fn request_close_fade() -> std::time::Duration {
    // This runs on the exit path of every cce-ui app, so it does its own
    // socket call rather than `send_command`: that one reads to EOF with no
    // deadline, and a compositor wedged mid-frame would hang the quit
    // forever. A second is far longer than an IPC round trip and short
    // enough that a user who hit Close still sees the window go.
    const REPLY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);
    let Ok(mut stream) = UnixStream::connect(socket_path("cce")) else {
        return std::time::Duration::ZERO;
    };
    let _ = stream.set_write_timeout(Some(REPLY_TIMEOUT));
    let _ = stream.set_read_timeout(Some(REPLY_TIMEOUT));
    if stream.write_all(b"fade-out\n").is_err() {
        return std::time::Duration::ZERO;
    }
    // The duration is the compositor's to decide (`surface { fade out_ms }`),
    // so it is read back rather than assumed: the two sides would otherwise
    // drift apart the moment the config changed, and the visible failure —
    // the window vanishing partway through its own dissolve — reads as a
    // rendering bug rather than a disagreement about a number.
    let mut reply = String::new();
    if stream.read_to_string(&mut reply).is_err() {
        return std::time::Duration::ZERO;
    }
    let ms: u64 = reply.trim().parse().unwrap_or(0);
    // The compositor clamps this already; clamped again here because a
    // client must never be made to hang on a number from the other side.
    std::time::Duration::from_millis(ms.min(2000))
}

/// Ask the compositor to bring a window to the user: un-minimize it, focus
/// it, raise it, and pan the camera to it — `ccectl focus-window <query>`.
///
/// `query` is what the compositor resolves: a numeric window id, else an
/// app_id (an exact match beats a substring one). An app bringing *itself*
/// forward passes its own app_id — which is what a single-instance app does
/// when a later launch forwards to it, or the work lands in a window parked
/// off-camera and the launch looks like it did nothing. (xdg-activation is
/// not the route to this: the compositor deliberately answers it with an
/// attention notification, not focus.)
///
/// Blocks for one round trip, bounded at a second for the same reason as
/// [`request_close_fade`] — `send_command` reads with no deadline, and a
/// compositor wedged mid-frame must not hang the caller. `Err` when there is
/// no compositor to ask, it does not answer in time, it answers `error: …`
/// (no such window, no seat), or `query` is empty or spans lines.
pub fn focus_window(query: &str) -> std::io::Result<()> {
    focus_window_at(&socket_path("cce"), query)
}

fn focus_window_at(path: &str, query: &str) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};
    const REPLY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);
    let query = query.trim();
    // A newline would end this command and start another on the control
    // socket; refuse rather than pass an injection along.
    if query.is_empty() || query.contains(['\n', '\r']) {
        return Err(Error::new(ErrorKind::InvalidInput, format!("not a window query: {query:?}")));
    }
    let mut stream = UnixStream::connect(path)?;
    stream.set_write_timeout(Some(REPLY_TIMEOUT))?;
    stream.set_read_timeout(Some(REPLY_TIMEOUT))?;
    stream.write_all(format!("focus-window {query}\n").as_bytes())?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply)?;
    match reply.trim() {
        "ok" => Ok(()),
        other => Err(Error::new(ErrorKind::Other, format!("focus-window {query}: {other}"))),
    }
}

/// One request line from a socket client, bounded in size and in TOTAL time.
///
/// For a listener thread that serves one client after another: a plain
/// `BufReader::read_line` with no timeout lets a client that connects and
/// says nothing (or trickles a byte at a time) hold the thread for good, and
/// every later client waits behind it. A per-read timeout alone is not
/// enough, since a trickle resets it with every byte; the deadline here is
/// for the whole line.
///
/// Returns the line with its `\n` (or what arrived before EOF). `None` on
/// EOF before any byte, the deadline, more than `limit` bytes without a
/// newline, a read error, or invalid UTF-8. The stream's read timeout is
/// cleared again on success.
///
/// Nothing past the newline is consumed: each chunk is PEEKED first and only
/// the bytes up to the newline are read, so whatever the client sent after
/// the line is still on the socket for the caller to read. Until 2026-10-06
/// the whole chunk was read and the tail truncated away — cce-cloud's
/// switcher client sends its request line and then the window list down the
/// same connection, and when both had arrived by the time the daemon read,
/// the list went with the tail and the switcher opened empty.
pub fn read_request_line(conn: &UnixStream, limit: usize, deadline: std::time::Duration) -> Option<String> {
    let until = std::time::Instant::now() + deadline;
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    let mut reader = conn;
    loop {
        let left = until.checked_duration_since(std::time::Instant::now()).filter(|d| !d.is_zero())?;
        conn.set_read_timeout(Some(left)).ok()?;
        let n = match peek(conn, &mut chunk) {
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return None,
        };
        if n == 0 {
            if buf.is_empty() {
                return None;
            }
            break;
        }
        // Consume through the newline if the peek holds one, else all of it.
        let take = chunk[..n].iter().position(|&b| b == b'\n').map_or(n, |end| end + 1);
        // Already queued, so this returns at once with exactly `take` bytes.
        reader.read_exact(&mut chunk[..take]).ok()?;
        buf.extend_from_slice(&chunk[..take]);
        if buf.last() == Some(&b'\n') {
            break;
        }
        if buf.len() > limit {
            return None;
        }
    }
    let _ = conn.set_read_timeout(None);
    String::from_utf8(buf).ok()
}

/// `recv(MSG_PEEK)`: what is queued on the socket, left there. Blocks (up to
/// the read timeout) like `read` when nothing is. `UnixStream::peek` is
/// still unstable.
fn peek(conn: &UnixStream, buf: &mut [u8]) -> std::io::Result<usize> {
    use std::os::fd::AsRawFd;
    // SAFETY: `buf` is a live, writable slice of `buf.len()` bytes.
    let n = unsafe { libc::recv(conn.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len(), libc::MSG_PEEK) };
    if n < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(n as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::read_request_line;
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use std::time::{Duration, Instant};

    #[test]
    fn a_request_line_is_bounded_in_time_and_size() {
        let (mut client, server) = UnixStream::pair().unwrap();
        client.write_all(b"open note\nmore").unwrap();
        assert_eq!(read_request_line(&server, 1024, Duration::from_secs(1)).as_deref(), Some("open note\n"));

        // Only the line is consumed: what the client sent after it in the
        // same burst (cce-cloud's switcher list) is still there to read.
        let (mut client, mut server) = UnixStream::pair().unwrap();
        client.write_all(b"{\"args\":[]}\nWindow A (a)\nWindow B (b)\n").unwrap();
        drop(client);
        assert_eq!(read_request_line(&server, 1024, Duration::from_secs(1)).as_deref(), Some("{\"args\":[]}\n"));
        let mut rest = String::new();
        server.read_to_string(&mut rest).unwrap();
        assert_eq!(rest, "Window A (a)\nWindow B (b)\n");

        // A line longer than one peek, then a tail: read across chunks,
        // and the tail is still left.
        let (mut client, mut server) = UnixStream::pair().unwrap();
        let long = format!("{}\ntail", "y".repeat(10_000));
        let writer = std::thread::spawn(move || client.write_all(long.as_bytes()).unwrap());
        let line = read_request_line(&server, 64 * 1024, Duration::from_secs(1)).unwrap();
        writer.join().unwrap();
        assert_eq!(line.len(), 10_001);
        let mut rest = String::new();
        server.read_to_string(&mut rest).unwrap();
        assert_eq!(rest, "tail");

        // Silent: given up at the deadline, not held forever.
        let (_quiet, server) = UnixStream::pair().unwrap();
        let t = Instant::now();
        assert_eq!(read_request_line(&server, 1024, Duration::from_millis(100)), None);
        assert!(t.elapsed() < Duration::from_millis(500));

        // Trickling a byte at a time: the TOTAL deadline still ends it.
        let (mut client, server) = UnixStream::pair().unwrap();
        let trickle = std::thread::spawn(move || {
            for _ in 0..40 {
                if client.write_all(b"x").is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        });
        let t = Instant::now();
        assert_eq!(read_request_line(&server, 1024, Duration::from_millis(150)), None);
        assert!(t.elapsed() < Duration::from_millis(500), "took {:?}", t.elapsed());
        drop(server);
        trickle.join().unwrap();

        // Over the size cap.
        let (mut client, server) = UnixStream::pair().unwrap();
        client.write_all(&[b'z'; 2000]).unwrap();
        assert_eq!(read_request_line(&server, 1024, Duration::from_millis(200)), None);
    }

    /// Against a stand-in compositor on a private path (never the session's
    /// control socket): the command it sends, `ok` as success, `error: …`
    /// as an error, a silent compositor bounded, a bad query never sent.
    #[test]
    fn focus_window_sends_one_command_and_reads_the_verdict() {
        use super::focus_window_at;
        use std::io::Read;
        use std::os::unix::net::UnixListener;

        let path = format!("/tmp/cce-ui-focus-test-{}.sock", std::process::id());
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).unwrap();
        let replies: [&[u8]; 3] = [b"ok\n", b"error: window not found\n", b""];
        let server = std::thread::spawn(move || {
            let mut got = Vec::new();
            for reply in replies {
                let (mut conn, _) = listener.accept().unwrap();
                let line = read_request_line(&conn, 1024, Duration::from_secs(1)).unwrap();
                got.push(line);
                if reply.is_empty() {
                    // Say nothing and keep the connection open: a wedged compositor.
                    let mut rest = Vec::new();
                    let _ = conn.read_to_end(&mut rest);
                } else {
                    conn.write_all(reply).unwrap();
                }
            }
            got
        });

        assert!(focus_window_at(&path, "cce-browser").is_ok());
        let err = focus_window_at(&path, "  nothing-here ").unwrap_err();
        assert!(err.to_string().contains("window not found"), "{err}");
        let t = Instant::now();
        assert!(focus_window_at(&path, "12").is_err(), "a silent compositor is an error");
        assert!(t.elapsed() < Duration::from_secs(3), "and a bounded one: {:?}", t.elapsed());
        for bad in ["", "   ", "a\nquit", "a\rb"] {
            assert_eq!(
                focus_window_at(&path, bad).unwrap_err().kind(),
                std::io::ErrorKind::InvalidInput,
                "{bad:?}"
            );
        }
        let got = server.join().unwrap();
        assert_eq!(got, ["focus-window cce-browser\n", "focus-window nothing-here\n", "focus-window 12\n"]);
        let _ = std::fs::remove_file(&path);
    }
}
