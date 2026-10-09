//! Text shaping for the runner: the shaped-buffer cache, family resolution,
//! the display list's text prims gathered for the glyph pass, and the
//! popover-occlusion clamp. Platform-neutral — nothing here knows the window
//! system; moved out of `window_runner` so another shell can share it.
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | vertical text (the status bar on a screen edge) |
//! | `cache` | the shaped-buffer cache: its key, lookup, insertion and least-recently-used eviction |
//! | `fonts` | face aliases, rescans and the font-op log every database replays, family resolution and face snapping |
//! | `shape` | shaping a buffer (single run or laid out in a box), through the cache |
//! | `runs` | shaped runs and clusters (caret and hit maths), bidi levels and visual order |
//! | `display` | the display list's text prims gathered for the glyph pass, and the popover-occlusion clamp |

mod cache;
use cache::*;
mod display;
pub use display::*;
mod fonts;
pub use fonts::*;
mod runs;
pub use runs::*;
mod shape;
pub use shape::*;
#[cfg(test)]
mod text_cache_tests;
#[cfg(test)]
mod visual_order_tests;

use cosmic_text::{FontSystem, Buffer, Attrs, Metrics};
use std::rc::Rc;
use crate::draw::TextSpan;

/// Vertical text, for a process whose window is a vertical bar (the status bar on a
/// screen edge): while it is on, a [`TextLabel`](crate::widget::display::TextLabel) stacks
/// its characters one per line, centred in a column `bar_thickness` px wide, and text is
/// shaped at a looser 1.05 line height. Process-wide because it is a property of the app,
/// not of one window or one label, and shaping may run on any thread. It was two bare
/// `pub static`s at the crate root (`IS_VERTICAL`, `BAR_THICKNESS`) until 2026-10-07.
pub fn set_vertical_text(bar_thickness: Option<u32>) {
    use std::sync::atomic::Ordering::Relaxed;
    // The window's own (`crate::window_state::Props`), and the process's for a worker.
    crate::window_state::entered(|w| w.props.borrow_mut().vertical_text = bar_thickness);
    if let Some(t) = bar_thickness {
        VERTICAL_BAR_THICKNESS.store(t, Relaxed);
    }
    VERTICAL_TEXT.store(bar_thickness.is_some(), Relaxed);
}

/// The vertical bar's thickness while vertical text is on (see [`set_vertical_text`]).
pub fn vertical_text() -> Option<u32> {
    crate::window_state::entered(|w| w.props.borrow().vertical_text).unwrap_or_else(process_vertical_text)
}

/// The process-wide vertical text: the last any window set (what a worker thread reads).
pub(crate) fn process_vertical_text() -> Option<u32> {
    use std::sync::atomic::Ordering::Relaxed;
    VERTICAL_TEXT.load(Relaxed).then(|| VERTICAL_BAR_THICKNESS.load(Relaxed))
}

static VERTICAL_TEXT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

static VERTICAL_BAR_THICKNESS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(24);
