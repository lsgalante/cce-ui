use std::process::Command;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_detached() {
        let cmd = Command::new("true");
        assert!(spawn_detached(cmd).is_ok());
    }
}
