use std::io::Write;
use std::process::Command;
use std::sync::Mutex;

static TRACKED_PROCESSES: Mutex<Vec<std::process::Child>> = Mutex::new(Vec::new());

/// Spawns a process detached and automatically reaps it when it exits.
///
/// If an active Tokio runtime is available, it will use a background Tokio task
/// to wait on the process. Otherwise, it will fallback to a background OS thread.
pub fn spawn_detached(mut cmd: Command) -> std::io::Result<()> {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        let mut tokio_cmd = tokio::process::Command::from(cmd);
        let mut child = tokio_cmd.spawn()?;
        handle.spawn(async move {
            let _ = child.wait().await;
        });
    } else {
        let mut child = cmd.spawn()?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
    Ok(())
}

/// Spawns a process and registers it to be automatically killed when the application exits.
pub fn spawn_tracked(mut cmd: Command) -> std::io::Result<()> {
    let child = cmd.spawn()?;
    if let Ok(mut lock) = TRACKED_PROCESSES.lock() {
        lock.push(child);
    }
    Ok(())
}

/// Kills all spawned and tracked child processes. Called automatically on application exit.
pub fn cleanup_spawned_processes() {
    if let Ok(mut lock) = TRACKED_PROCESSES.lock() {
        for mut child in lock.drain(..) {
            let _ = child.kill();
        }
    }
}

/// Resolve the `cce-cloud` binary, preferring `~/.local/bin`.
pub fn get_cce_cloud_cmd() -> String {
    if let Ok(home) = std::env::var("HOME") {
        let path = format!("{}/.local/bin/cce-cloud", home);
        if std::path::Path::new(&path).exists() {
            return path;
        }
    }
    "cce-cloud".to_string()
}

fn send_sigterm(pid: u32) {
    let ret = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
    if ret != 0 {
        log::warn!(
            "[cloud-popup] SIGTERM to pid {} failed: {}",
            pid,
            std::io::Error::last_os_error()
        );
    }
}

/// What a click on a popup trigger should do next — the result of
/// [`CloudPopupTracker::click`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudPopupClick {
    /// The click closed (or canceled) this source's own popup; don't spawn a
    /// new one.
    ToggledOff,
    /// Spawn this source's popup now; any other source's popup has been
    /// killed and the tracker is pending on this source.
    Open,
}

/// Tracks an application's single active `cce-cloud` popup.
///
/// The pattern: a trigger (button, tray icon, …) identified by a `source`
/// string spawns a `cce-cloud` process on a worker thread ([`CloudPopup`]);
/// the thread reports the pid back through the app's event loop
/// ([`on_spawned`](Self::on_spawned)) and reports exit when the popup closes
/// ([`on_closed`](Self::on_closed)). Clicking the trigger again while its
/// popup is open (or still spawning) toggles it off. Only one popup exists at
/// a time: opening one source's popup kills any other's.
#[derive(Debug, Default)]
pub struct CloudPopupTracker {
    active_pid: Option<u32>,
    active_source: Option<String>,
}

impl CloudPopupTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// The tracked popup's pid, if that process is still alive and still a
    /// `cce-cloud` (guards against pid reuse).
    fn running_pid(&self) -> Option<u32> {
        let pid = self.active_pid?;
        let comm = std::fs::read_to_string(format!("/proc/{}/comm", pid)).ok()?;
        (comm.trim() == "cce-cloud").then_some(pid)
    }

    /// Whether the tracked popup is currently running.
    pub fn is_open(&self) -> bool {
        self.running_pid().is_some()
    }

    /// Handle a click on `source`'s trigger. Kills whatever popup is open;
    /// returns [`CloudPopupClick::ToggledOff`] when the click closed or
    /// canceled `source`'s own popup, [`CloudPopupClick::Open`] when the
    /// caller should now spawn `source`'s popup.
    pub fn click(&mut self, source: &str) -> CloudPopupClick {
        if let Some(pid) = self.running_pid() {
            log::debug!("[cloud-popup] killing open popup pid {} on click for '{}'", pid, source);
            send_sigterm(pid);
            self.active_pid = None;
            if self.active_source.as_deref() == Some(source) {
                self.active_source = None;
                return CloudPopupClick::ToggledOff;
            }
        } else if self.active_source.as_deref() == Some(source) {
            // Same source clicked again while its popup was still spawning:
            // cancel it (on_spawned will kill the late-arriving pid).
            self.active_source = None;
            return CloudPopupClick::ToggledOff;
        }
        self.active_source = Some(source.to_string());
        CloudPopupClick::Open
    }

    /// A spawner thread announced its popup's pid. Adopts the pid if `source`
    /// is still the active one; kills the process if the popup was canceled
    /// or superseded while spawning.
    pub fn on_spawned(&mut self, pid: u32, source: &str) {
        if self.active_source.as_deref() == Some(source) {
            self.active_pid = Some(pid);
        } else {
            log::debug!("[cloud-popup] pid {} for source '{}' is obsolete/canceled, killing", pid, source);
            send_sigterm(pid);
        }
    }

    /// A spawner thread reported its popup closed (`pid` 0 when it never
    /// spawned). Returns `true` — with the tracker cleared — when it was the
    /// active popup, so the caller can run its close action (e.g. restore
    /// focus). `false` for stale reports from superseded popups.
    pub fn on_closed(&mut self, pid: u32, source: &str) -> bool {
        if self.active_pid == Some(pid)
            || (pid == 0 && self.active_source.as_deref() == Some(source))
        {
            self.active_pid = None;
            self.active_source = None;
            true
        } else {
            false
        }
    }
}

/// One `cce-cloud` popup invocation: where it opens and how it's parented.
///
/// [`run_json`](Self::run_json) / [`run_dmenu`](Self::run_dmenu) block until
/// the popup closes — call them from a worker thread, report the pid from
/// `on_spawn` back to the event loop for [`CloudPopupTracker::on_spawned`],
/// and report [`CloudPopupTracker::on_closed`] when they return.
#[derive(Debug, Clone, Default)]
pub struct CloudPopup {
    x: i32,
    y: i32,
    parent_app_id: Option<String>,
    align_right: bool,
}

impl CloudPopup {
    pub fn at(x: i32, y: i32) -> Self {
        Self { x, y, ..Self::default() }
    }

    /// Parent the popup to a surface by app id (compositor-side placement).
    pub fn parent_app_id(mut self, app_id: impl Into<String>) -> Self {
        self.parent_app_id = Some(app_id.into());
        self
    }

    /// Grow the popup leftward from `x` instead of rightward.
    pub fn align_right(mut self) -> Self {
        self.align_right = true;
        self
    }

    /// `--json` mode: pipe a JSON page description, block until the popup
    /// closes, and return its trimmed stdout (the selection JSON). `None`
    /// when the popup was dismissed/killed without a selection.
    pub fn run_json(
        &self,
        layout_json: &str,
        on_spawn: impl FnOnce(u32),
    ) -> std::io::Result<Option<String>> {
        self.run(&["--json".to_string()], layout_json, on_spawn)
    }

    /// `--dmenu` mode: pipe newline-separated items, block until the popup
    /// closes, and return the selected line. `None` when nothing was picked.
    pub fn run_dmenu(
        &self,
        prompt: &str,
        items: &str,
        on_spawn: impl FnOnce(u32),
    ) -> std::io::Result<Option<String>> {
        self.run(&["--dmenu".to_string(), "-p".to_string(), prompt.to_string()], items, on_spawn)
    }

    fn run(
        &self,
        mode_args: &[String],
        stdin_payload: &str,
        on_spawn: impl FnOnce(u32),
    ) -> std::io::Result<Option<String>> {
        let mut args = mode_args.to_vec();
        args.extend(["-x".to_string(), self.x.to_string(), "-y".to_string(), self.y.to_string()]);
        if let Some(ref parent) = self.parent_app_id {
            args.extend(["--parent-app-id".to_string(), parent.clone()]);
        }
        if self.align_right {
            args.push("--align-right".to_string());
        }

        let mut child = Command::new(get_cce_cloud_cmd())
            .args(&args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        on_spawn(child.id());

        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(stdin_payload.as_bytes());
            // dropped here: cce-cloud sees EOF
        }

        let output = child.wait_with_output()?;
        let err_str = String::from_utf8_lossy(&output.stderr);
        if !err_str.is_empty() {
            log::debug!("[cce-cloud stderr] {}", err_str);
        }
        if !output.status.success() {
            return Ok(None);
        }
        let out = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok((!out.is_empty()).then_some(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_detached() {
        let cmd = Command::new("true");
        assert!(spawn_detached(cmd).is_ok());
    }

    #[test]
    fn test_spawn_tracked() {
        let cmd = Command::new("true");
        assert!(spawn_tracked(cmd).is_ok());
        cleanup_spawned_processes();
    }

    // --- CloudPopupTracker state machine ---
    // (pids here are never live cce-cloud processes, so running_pid() is
    // always None — these tests cover the pending/source transitions; the
    // kill-the-open-popup paths need a live popup and are covered by the
    // manual smoke test.)

    #[test]
    fn click_open_then_second_click_cancels_pending() {
        let mut t = CloudPopupTracker::new();
        assert_eq!(t.click("layout"), CloudPopupClick::Open);
        assert_eq!(t.click("layout"), CloudPopupClick::ToggledOff);
        // canceled: a late pid announcement must not be adopted
        t.on_spawned(4_000_000, "layout");
        assert!(!t.is_open());
        assert!(!t.on_closed(4_000_000, "layout"));
    }

    #[test]
    fn different_source_supersedes_pending() {
        let mut t = CloudPopupTracker::new();
        assert_eq!(t.click("tray:a"), CloudPopupClick::Open);
        assert_eq!(t.click("tray:b"), CloudPopupClick::Open);
        // a's late spawn is obsolete; b's is adopted
        t.on_spawned(4_000_001, "tray:a");
        t.on_spawned(4_000_002, "tray:b");
        assert!(t.on_closed(4_000_002, "tray:b"));
    }

    #[test]
    fn closed_with_pid_zero_matches_pending_source_only() {
        let mut t = CloudPopupTracker::new();
        assert_eq!(t.click("window"), CloudPopupClick::Open);
        assert!(!t.on_closed(0, "layout"));
        assert!(t.on_closed(0, "window"));
        assert_eq!(t.click("window"), CloudPopupClick::Open);
    }
}
