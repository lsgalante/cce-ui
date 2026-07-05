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
}
