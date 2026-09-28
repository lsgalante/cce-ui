//! The DE-wide animations switch.
//!
//! One flag, followed by every cce-ui widget that eases and by the
//! compositor: when it is off, anything that would glide, fade, slide or
//! coast lands on its target in the same frame instead. Disabled means
//! "snap", never "freeze" — a dropdown still opens, a scroll still moves.
//!
//! The switch is a file, [`STATE_PATH`], holding `on` or `off`. The System
//! Interface's Power page sets it per power mode and `cce-power-apply`
//! writes it as root whenever the mode changes (plug, unplug, boot), which
//! is why it lives under /run rather than in `~/.config/cce`: the writer has
//! no session and no `$HOME`. No file means on.
//!
//! [`enabled`] is cheap enough for per-frame use: it re-reads the file at
//! most every [`RECHECK`], so a running client follows a mode change within
//! half a second and nothing needs restarting or reloading. `CCE_ANIMATIONS`
//! (`0`/`off` or `1`/`on`) overrides the file for one process, for testing.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Where the switch lives. Shared with `cce-power-apply`, the writer.
pub const STATE_PATH: &str = "/run/cce/animations";

/// How stale [`enabled`] may be. A mode change is a plug or an unplug, so
/// half a second is instant to a person, and the stat stays off the frame.
pub const RECHECK: Duration = Duration::from_millis(500);

/// `on` / `off` (surrounding whitespace ignored); anything else says nothing.
pub fn parse(text: &str) -> Option<bool> {
    match text.trim() {
        "on" | "1" | "true" => Some(true),
        "off" | "0" | "false" => Some(false),
        _ => None,
    }
}

/// What the state file says right now, uncached. `None` when there is no
/// file or it holds nothing recognizable — both of which mean "animate".
pub fn read_state() -> Option<bool> {
    parse(&std::fs::read_to_string(STATE_PATH).ok()?)
}

static CACHE: Mutex<Option<(Instant, bool)>> = Mutex::new(None);

/// Under `cfg(test)` the switch is the SUITE's, not the machine's: on,
/// unless a test forces it with [`force_for_test`]. Until 2026-09-28
/// `enabled` read `/run/cce/animations` in the test binary too, so three
/// glide and fade tests passed or failed with the laptop's power mode —
/// off on battery, on when plugged in — and read as a broken glide rather
/// than a borrowed switch. Thread-local rather than the shared cache,
/// because libtest runs tests in parallel and a process-wide override set
/// by one test would race every other test's read; a test that forces it
/// does so for its own thread only, and the value resets with the thread.
#[cfg(test)]
thread_local! {
    static FORCED: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

/// Set what [`enabled`] answers on this thread, for a test that exercises
/// the snap-instead-of-ease path. Tests never set `CCE_ANIMATIONS`, since
/// an environment variable is process-wide.
#[cfg(test)]
pub fn force_for_test(value: bool) {
    FORCED.with(|f| f.set(value));
}

/// Whether to animate. Every easing in the toolkit asks this before it
/// steps, and snaps to its target when the answer is no.
pub fn enabled() -> bool {
    #[cfg(test)]
    {
        return FORCED.with(|f| f.get());
    }
    #[cfg(not(test))]
    enabled_on_this_machine()
}

/// [`enabled`] as the shipped binary answers it: the `CCE_ANIMATIONS`
/// override for this process, else the state file, re-read at most every
/// [`RECHECK`].
#[cfg(not(test))]
fn enabled_on_this_machine() -> bool {
    static ENV: std::sync::OnceLock<Option<bool>> = std::sync::OnceLock::new();
    if let Some(forced) = *ENV.get_or_init(|| std::env::var("CCE_ANIMATIONS").ok().and_then(|v| parse(&v))) {
        return forced;
    }
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let now = Instant::now();
    match *cache {
        Some((at, value)) if now.duration_since(at) < RECHECK => value,
        _ => {
            let value = read_state().unwrap_or(true);
            *cache = Some((now, value));
            value
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_reads_the_helpers_spelling_and_nothing_else() {
        assert_eq!(parse("off\n"), Some(false));
        assert_eq!(parse("on"), Some(true));
        assert_eq!(parse(" 0 "), Some(false));
        assert_eq!(parse(""), None);
        assert_eq!(parse("disabled"), None);
    }

    /// The suite's switch is on whatever the machine's file says, a test
    /// can force it off for its own thread only, and another thread still
    /// sees it on.
    #[test]
    fn the_suite_animates_whatever_the_machine_says() {
        assert!(enabled(), "on by default, not read off {STATE_PATH}");
        force_for_test(false);
        assert!(!enabled(), "a test can force the snap path");
        let elsewhere = std::thread::spawn(enabled).join().unwrap();
        assert!(elsewhere, "the force is this thread's alone");
        force_for_test(true);
        assert!(enabled());
    }
}
