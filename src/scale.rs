//! A window's own properties ([`Props`]): its HiDPI scale, the display metric, its app id,
//! whether it is fullscreen or maximized, and vertical text — and which of them the code
//! running now reads (`docs/rfc-global-state.md`, phase 4).
//!
//! The current properties are those of the window entered on this thread
//! (`window_state::enter` makes them current), else this THREAD's own. There is no
//! process-wide copy: until 2026-10-12 every setter also wrote process statics, so a read
//! off any window got whichever window wrote last, and tests pinned the scale per thread to
//! keep from seeing each other's. Code that runs no window (a custom shell such as
//! cce-cloud's, a tool, a test) sets and reads its own thread's; no worker thread reads
//! these. This module names no window type: `window_state` hands it the properties to make
//! current.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::OnceLock;

static FORCED_SCALE: OnceLock<Option<f32>> = OnceLock::new();

/// `CCE_FORCE_SCALE`: HiDPI override for foreign compositors that report a
/// scale-1 output (the display-manager greeter under cage). In forced mode the
/// compositor's logical coordinate space is treated as physical: configure
/// sizes and pointer positions are divided by this factor, layout/rendering
/// scale up by it, and the surface keeps `buffer_scale` 1 so the buffer still
/// matches the size the compositor configured. Unset (the normal case, under
/// cce-fx) this is `None` and nothing changes.
pub fn forced_scale() -> Option<f32> {
    *FORCED_SCALE.get_or_init(|| {
        std::env::var("CCE_FORCE_SCALE")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|s| *s > 0.0 && (*s - 1.0).abs() > 0.001)
    })
}
/// A window's own properties (see the module docs).
#[derive(Debug, Clone)]
pub struct Props {
    /// The HiDPI scale it is drawn at ([`scale_factor`]).
    pub scale: f32,
    /// The display it is on, in logical px per mm (`units::metric`).
    pub metric: Option<crate::units::Metric>,
    pub app_id: String,
    pub fullscreen: bool,
    pub maximized: bool,
    /// Set while it stands on a screen edge as a vertical bar of this thickness
    /// (`text::vertical_text`).
    pub vertical_text: Option<u32>,
}

impl Default for Props {
    fn default() -> Props {
        Props { scale: 1.0, metric: None, app_id: String::new(), fullscreen: false, maximized: false, vertical_text: None }
    }
}

thread_local! {
    /// The entered window's properties, if a window is entered on this thread.
    static CURRENT: RefCell<Option<Rc<RefCell<Props>>>> = const { RefCell::new(None) };
    /// This thread's own, current while no window is entered.
    static THREAD: Rc<RefCell<Props>> = Rc::new(RefCell::new(Props::default()));
}

/// Run `f` on the current properties: the entered window's, else this thread's own.
pub fn with_props<R>(f: impl FnOnce(&RefCell<Props>) -> R) -> R {
    let props = CURRENT.with(|c| c.borrow().clone()).unwrap_or_else(|| THREAD.with(Rc::clone));
    f(&props)
}

/// Make `props` the current properties (`None`: this thread's own again), returning the
/// ones current before: what `window_state::enter` and its guard do.
pub(crate) fn make_current(props: Option<Rc<RefCell<Props>>>) -> Option<Rc<RefCell<Props>>> {
    CURRENT.with(|c| c.replace(props))
}

/// The HiDPI scale of the window being drawn (see the module docs).
pub fn scale_factor() -> f32 {
    with_props(|p| p.borrow().scale)
}

pub fn set_scale_factor(scale: f32) {
    with_props(|p| p.borrow_mut().scale = scale);
}

pub fn app_id() -> String {
    with_props(|p| p.borrow().app_id.clone())
}

pub fn set_app_id(id: String) {
    with_props(|p| p.borrow_mut().app_id = id);
}

pub fn is_fullscreen() -> bool {
    with_props(|p| p.borrow().fullscreen)
}

pub fn set_fullscreen(fs: bool) {
    with_props(|p| p.borrow_mut().fullscreen = fs);
}

pub fn is_maximized() -> bool {
    with_props(|p| p.borrow().maximized)
}

pub fn set_maximized(m: bool) {
    with_props(|p| p.borrow_mut().maximized = m);
}
