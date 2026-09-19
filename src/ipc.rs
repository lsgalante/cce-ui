//! Helpers for the CCE Unix-socket IPC convention: `/tmp/<prefix>-<WAYLAND_DISPLAY>.sock`.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

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
