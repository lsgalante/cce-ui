//! The MCP registry: one file per running server, so a client reaches the
//! instance it wants — the one that owns the focused window — instead of
//! whichever process holds a fixed port.
//!
//! Each server writes `<dir>/<app_id>-<pid>.json` ([`Entry`], mode 0600) once
//! it is listening, in [`dir`] (`$XDG_RUNTIME_DIR/cce/mcp`, mode 0700). The
//! file is removed when the process exits normally; one left behind by a
//! killed process is recognised by its pid and start time and swept by the
//! next reader or writer. The start time is what makes a reused pid harmless:
//! a new process under the old pid has a different one.
//!
//! The entry carries the server's bearer token, so the directory's and the
//! file's modes are the access control: only this user's processes can
//! read a token and call the app's tools.
//!
//! **One directory, every session.** A shadow compositor shares the live
//! session's `$XDG_RUNTIME_DIR`, so its apps file themselves beside the
//! live ones. Each entry names its Wayland display, and [`find`] returns
//! only entries on the caller's: a popup in a shadow never reaches the
//! user's live apps, nor the other way round. An entry with no display
//! (written before the field existed) matches any caller.

use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// One running MCP server, as its registry file spells it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The app's id, as its windows report it (`cce-designer`).
    pub app_id: String,
    pub pid: u32,
    /// Field 22 of `/proc/<pid>/stat` (clock ticks after boot): with `pid`,
    /// names the process even once the pid is reused.
    pub start_time: u64,
    /// `http://127.0.0.1:<port>/mcp`.
    pub url: String,
    /// Sent as `Authorization: Bearer <token>`.
    pub token: String,
    /// App ids whose servers a client should load beside this one (the
    /// vault apps name each other).
    #[serde(default)]
    pub companions: Vec<String>,
    /// The app's `$WAYLAND_DISPLAY` (`wayland-0`); `None` from an older
    /// build, which matches any caller.
    #[serde(default)]
    pub display: Option<String>,
}

/// This process's `$WAYLAND_DISPLAY`, the display entries are filed under
/// and looked up by.
pub fn current_display() -> Option<String> {
    std::env::var("WAYLAND_DISPLAY").ok().filter(|d| !d.is_empty())
}

/// `$XDG_RUNTIME_DIR/cce/mcp`.
pub fn dir() -> PathBuf {
    cce_core::config::runtime_dir().join("cce").join("mcp")
}

/// The registry file's name for an entry.
fn file_name(app_id: &str, pid: u32) -> String {
    // An app id is a reverse-DNS-ish word; keep anything else out of the path.
    let safe: String = app_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' })
        .collect();
    format!("{safe}-{pid}.json")
}

/// The files this process wrote, removed at exit.
static OWN_FILES: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

extern "C" fn remove_own_files() {
    if let Ok(files) = OWN_FILES.lock() {
        for path in files.iter() {
            let _ = fs::remove_file(path);
        }
    }
}

/// Write `entry` into `dir` (created 0700 if missing), replacing any file of
/// the same name, and sweep the stale files beside it. The write is atomic:
/// a reader sees the old file or the whole new one. Returns the path.
pub fn write(dir: &Path, entry: &Entry) -> io::Result<PathBuf> {
    fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    let path = dir.join(file_name(&entry.app_id, entry.pid));
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
        f.write_all(serde_json::to_string_pretty(entry)?.as_bytes())?;
        f.sync_all()?;
    }
    fs::rename(&tmp, &path)?;
    sweep(dir);
    Ok(path)
}

/// [`write`], and remove the file when this process exits normally (a
/// return from `main` or `std::process::exit`; a killed process leaves it
/// for the next reader to sweep).
pub fn register(dir: &Path, entry: &Entry) -> io::Result<PathBuf> {
    let path = write(dir, entry)?;
    if let Ok(mut files) = OWN_FILES.lock() {
        if files.is_empty() {
            // SAFETY: registers a plain extern "C" fn with no captured state.
            unsafe { libc::atexit(remove_own_files) };
        }
        if !files.contains(&path) {
            files.push(path.clone());
        }
    }
    Ok(path)
}

/// Every live server in `dir`, newest first. Files of exited processes are
/// deleted on the way; unreadable files are skipped and left alone.
pub fn read_all(dir: &Path) -> Vec<Entry> {
    let mut entries = Vec::new();
    let Ok(listing) = fs::read_dir(dir) else { return entries };
    for item in listing.flatten() {
        let path = item.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(entry) = fs::read(&path).ok().and_then(|b| serde_json::from_slice::<Entry>(&b).ok()) else {
            continue;
        };
        if alive(entry.pid, entry.start_time) {
            entries.push(entry);
        } else {
            let _ = fs::remove_file(&path);
        }
    }
    entries.sort_by(|a, b| b.start_time.cmp(&a.start_time).then(b.pid.cmp(&a.pid)));
    entries
}

/// The server for `app_id` on `display` (the caller's, as a rule:
/// [`current_display`]): the one in process `pid` when given and
/// registered, else the newest instance of the app. An entry on another
/// display is never returned; one with no display, or a caller with none,
/// matches.
pub fn find(dir: &Path, app_id: &str, pid: Option<u32>, display: Option<&str>) -> Option<Entry> {
    let on_display = |e: &Entry| match (e.display.as_deref(), display) {
        (Some(theirs), Some(ours)) => theirs == ours,
        _ => true,
    };
    let mut of_app: Vec<Entry> = read_all(dir).into_iter().filter(|e| e.app_id == app_id && on_display(e)).collect();
    if let Some(pid) = pid {
        if let Some(i) = of_app.iter().position(|e| e.pid == pid) {
            return Some(of_app.swap_remove(i));
        }
    }
    of_app.into_iter().next()
}

/// Delete the files in `dir` whose process has exited.
fn sweep(dir: &Path) {
    let _ = read_all(dir);
}

/// Whether process `pid` is running and is the one that started at `start_time`.
fn alive(pid: u32, start_time: u64) -> bool {
    process_start_time(pid) == Some(start_time)
}

/// Field 22 of `/proc/<pid>/stat`, or `None` when there is no such process.
pub fn process_start_time(pid: u32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    parse_start_time(&stat)
}

/// The start time out of a `/proc/<pid>/stat` line. The command name (field
/// 2) is in parentheses and may itself hold spaces and `)`, so the fields
/// are counted from the LAST `)`: the state (field 3) is the first after it.
fn parse_start_time(stat: &str) -> Option<u64> {
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().nth(22 - 3)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cce-ui-mcp-registry-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn entry(app_id: &str, pid: u32, start_time: u64) -> Entry {
        Entry {
            app_id: app_id.into(),
            pid,
            start_time,
            url: "http://127.0.0.1:1/mcp".into(),
            token: "t".into(),
            companions: vec![],
            display: Some("wayland-0".into()),
        }
    }

    fn me() -> Entry {
        let pid = std::process::id();
        entry("cce-test", pid, process_start_time(pid).unwrap())
    }

    #[test]
    fn start_time_is_counted_from_the_last_paren() {
        // A command name holding ") " must not shift the fields.
        let stat = "4242 (a) b c) S 1 4242 4242 0 -1 4194560 100 0 0 0 1 2 0 0 20 0 1 0 987654 1000 10";
        assert_eq!(parse_start_time(stat), Some(987654));
        assert_eq!(parse_start_time("garbage"), None);
        assert!(process_start_time(std::process::id()).is_some());
    }

    #[test]
    fn a_written_entry_is_found_by_app_and_by_pid() {
        let d = scratch("find");
        let mine = me();
        let path = write(&d, &mine).unwrap();
        assert!(path.ends_with(format!("cce-test-{}.json", mine.pid)));
        assert_eq!(find(&d, "cce-test", None, Some("wayland-0")), Some(mine.clone()));
        assert_eq!(find(&d, "cce-test", Some(mine.pid), Some("wayland-0")), Some(mine.clone()));
        // A pid with no entry falls back to the newest instance of the app.
        assert_eq!(find(&d, "cce-test", Some(1), Some("wayland-0")), Some(mine.clone()));
        assert_eq!(find(&d, "cce-other", None, Some("wayland-0")), None);
        // Another session's display: a shadow never reaches the live apps.
        assert_eq!(find(&d, "cce-test", Some(mine.pid), Some("wayland-1")), None);
        // A caller that knows no display, or an entry from an older build
        // that names none, still matches.
        assert_eq!(find(&d, "cce-test", None, None), Some(mine.clone()));
        let legacy = Entry { display: None, ..mine };
        write(&d, &legacy).unwrap();
        assert_eq!(find(&d, "cce-test", None, Some("wayland-1")), Some(legacy));
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn modes_keep_the_token_private() {
        use std::os::unix::fs::PermissionsExt;
        let d = scratch("modes");
        let path = write(&d, &me()).unwrap();
        assert_eq!(fs::metadata(&d).unwrap().permissions().mode() & 0o777, 0o700);
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn stale_entries_are_swept_and_never_returned() {
        let d = scratch("stale");
        let mine = me();
        // This pid with the wrong start time: a reused pid.
        let reused = entry("cce-test", mine.pid, mine.start_time + 1);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("cce-test-reused.json"), serde_json::to_string(&reused).unwrap()).unwrap();
        // A pid that cannot exist (above the kernel's pid_max ceiling).
        let gone = entry("cce-test", 4_194_305, 1);
        fs::write(d.join("cce-test-gone.json"), serde_json::to_string(&gone).unwrap()).unwrap();
        // A file that is not an entry stays: it is not ours to judge.
        fs::write(d.join("notes.json"), "{").unwrap();

        write(&d, &mine).unwrap();
        assert_eq!(read_all(&d), vec![mine]);
        assert!(!d.join("cce-test-reused.json").exists());
        assert!(!d.join("cce-test-gone.json").exists());
        assert!(d.join("notes.json").exists());
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn an_odd_app_id_stays_inside_the_directory() {
        assert_eq!(file_name("../../etc/passwd", 7), ".._.._etc_passwd-7.json");
    }
}
