//! No style getter may read the registry while it already holds a read of it.
//!
//! `STYLE_REGISTRY` is a `std::sync::RwLock`, and std's lock queues new readers
//! behind a WAITING writer. A getter written as
//!
//! ```ignore
//! get_style_registry().read().unwrap().get_float("toggle_corner_radius")
//!     .unwrap_or_else(control_corner_radius)
//! ```
//!
//! keeps its guard alive to the end of the statement, so the fallback's own
//! read is a second read on the same thread. A writer that arrives between
//! the two waits for the first, the second waits for the writer, and every
//! reader in the process then queues behind both. That hung
//! `cargo test -p cce-designer` (2026-09-25): ~40 test threads parked in
//! `RwLock::read_contended` on `STYLE_REGISTRY`, the writer being the
//! designer's `reload_config` (one write per config line, on every
//! `State::new`) and the reader any unset corner radius.
//!
//! An integration test on purpose: here cce-ui is built WITHOUT `cfg(test)`,
//! as it is inside every app's suite, so the per-thread style overlay the unit
//! tests get is out of the way and the getters hit the shared lock.
//!
//! The stress runs in a CHILD process — this binary re-executed — because a
//! deadlock on the process-wide registry cannot be recovered from in-process:
//! the parent kills the child at a deadline and fails, rather than hanging.

use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cce_ui::layout;

const CHILD_ENV: &str = "CCE_UI_REENTRANCY_CHILD";

/// Every getter whose unset slot falls back to another style getter — the
/// shape that nests. With an empty config each of them takes the fallback.
const FALLBACK_GETTERS: &[fn() -> f32] = &[
    layout::toggle_corner_radius,
    layout::slider_corner_radius,
    layout::color_selector_preview_corner_radius,
    layout::color_selector_corner_radius,
    layout::button_corner_radius,
    layout::spinbox_corner_radius,
    layout::textbox_corner_radius,
    layout::list_corner_radius,
    layout::tree_corner_radius,
    layout::font_selector_corner_radius,
    layout::menu_corner_radius,
    layout::dropdown_corner_radius,
];

/// The child: one thread takes and drops the write lock as fast as it can,
/// the others call every fallback getter. Against a nested read this parks
/// within milliseconds; without one it runs out its time and exits.
fn hammer() {
    // Initialise (reload_config runs once) before the race starts, so the
    // writer is the only writer.
    for g in FALLBACK_GETTERS {
        g();
    }
    let stop = Arc::new(AtomicBool::new(false));
    let writer = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                drop(layout::get_style_registry().write().unwrap());
            }
        })
    };
    let readers: Vec<_> = (0..4)
        .map(|_| {
            std::thread::spawn(|| {
                let until = Instant::now() + Duration::from_secs(2);
                while Instant::now() < until {
                    for g in FALLBACK_GETTERS {
                        std::hint::black_box(g());
                    }
                }
            })
        })
        .collect();
    for r in readers {
        r.join().unwrap();
    }
    stop.store(true, Ordering::Relaxed);
    writer.join().unwrap();
}

#[test]
fn fallback_getters_never_nest_a_registry_read() {
    if std::env::var_os(CHILD_ENV).is_some() {
        hammer();
        return;
    }

    // An empty config, so no slot is set and every getter falls back. The
    // directory is never created; an absent config.kdl reads as no config.
    let config_home = std::env::temp_dir().join(format!("cce-ui-reentrancy-{}", std::process::id()));
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "fallback_getters_never_nest_a_registry_read", "--test-threads=1", "-q"])
        .env(CHILD_ENV, "1")
        .env("XDG_CONFIG_HOME", &config_home)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("re-execute the test binary");

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "the stress child failed: {status}");
            return;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "style getters deadlocked on STYLE_REGISTRY: a getter read the registry while \
                 holding a read of it, and a queued writer parked both"
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
