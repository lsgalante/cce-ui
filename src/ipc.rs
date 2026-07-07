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
