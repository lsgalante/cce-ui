//! `ScrollbarActivity`: the raise/sink hysteresis of a scrollbar that idles behind its host's
//! plate, and how long a raise holds and a fade takes.

/// How long (seconds) a raise/sink scrollbar stays raised after the last wheel
/// scroll or drag release.
pub const SCROLL_ACTIVE_HOLD: f32 = 0.7;

/// How long (seconds) the raise and the sink take to cross-fade. Coming to the
/// fore is a fade, not a flip: the bar reads as rising through the host's
/// frosted plate rather than being swapped for a copy of itself.
pub const SCROLL_FADE_SECS: f32 = 0.18;

/// The raise/sink hysteresis for scrollbars that idle BEHIND their host's
/// translucent plate — the designer parameter-pane treatment, shared so every
/// app's bar behaves the same way. The bar has two depths: *raised* it draws in
/// front of the content and takes input; *sunk* it draws under the host's plate
/// (dimly visible through a translucent one) and is non-interactive, because
/// the plate occludes it.
///
/// The rules: a scroll (wheel, keyboard) or an active thumb drag raises the
/// bar, and a drag release refreshes the hold. Hover only *sustains* a bar
/// that is already raised — it can never raise a sunk one, since the pointer
/// is really over the plate, not the bar. Once nothing holds it up for
/// [`SCROLL_ACTIVE_HOLD`] seconds it sinks, and only scrolling brings it back.
///
/// The owner drives it: [`Self::bump`] on scrolls and drag releases,
/// [`Self::set_hover`] from pointer moves, [`Self::tick`] once per frame
/// (which decays the hold and recomputes — a `true` return is the repaint
/// signal for the raise/sink flip).
#[derive(Debug, Clone, Default)]
pub struct ScrollbarActivity {
    /// Seconds left in the "recently scrolled" window that keeps the bar raised.
    activity: f32,
    hover: bool,
    raised: bool,
    /// How far the FORE copy has faded in, 0..=1. Chases `raised` over
    /// [`SCROLL_FADE_SECS`]; only [`Self::tick`] advances it, so a host that
    /// drives the latch through `recompute` alone keeps the old hard flip.
    fade: f32,
}

impl ScrollbarActivity {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the bar is currently raised in front of the plate. While false
    /// it sits behind the plate and must not take input. This is the LATCH —
    /// it flips at once, so input never waits on the fade.
    pub fn raised(&self) -> bool {
        self.raised
    }

    /// Opacity of the fore copy, 0..=1: 0 while fully sunk (only the copy
    /// behind the plate shows), 1 once risen. Paint with this; gate input on
    /// [`Self::raised`].
    pub fn fade(&self) -> f32 {
        self.fade
    }

    /// Refresh the hold window: call on a wheel/keyboard scroll and on a drag
    /// release.
    pub fn bump(&mut self) {
        self.activity = SCROLL_ACTIVE_HOLD;
    }

    /// Track whether the pointer sits over the bar (raw geometry — the caller
    /// does not gate this on raised; the hysteresis is what limits hover to
    /// sustaining).
    pub fn set_hover(&mut self, over: bool) {
        self.hover = over;
    }

    /// Whether the post-scroll hold window is still running — owners whose tick
    /// chain only runs while frames are being drawn use this to keep frames
    /// coming until the sink actually renders.
    pub fn holding(&self) -> bool {
        self.activity > 0.0
    }

    /// Recompute the latched raised state, returning whether it changed.
    pub fn recompute(&mut self, visible: bool, dragging: bool) -> bool {
        let raised = visible && (dragging || self.activity > 0.0 || (self.raised && self.hover));
        let changed = raised != self.raised;
        self.raised = raised;
        changed
    }

    /// Per-frame decay + recompute. Returns whether the raised state flipped —
    /// the owner's repaint signal.
    pub fn tick(&mut self, dt: f32, visible: bool, dragging: bool) -> bool {
        if self.activity > 0.0 {
            self.activity = (self.activity - dt).max(0.0);
        }
        let flipped = self.recompute(visible, dragging);
        // Chase the latch. The step is over the WHOLE range, so a fade
        // reversed halfway takes proportionally less time rather than
        // restarting — a flick-scroll-flick does not stutter.
        let target = if self.raised { 1.0 } else { 0.0 };
        let step = if SCROLL_FADE_SECS > 0.0 && crate::motion::enabled() { dt / SCROLL_FADE_SECS } else { 1.0 };
        let moved = if (self.fade - target).abs() <= step {
            let done = self.fade != target;
            self.fade = target;
            done
        } else {
            self.fade += step * (target - self.fade).signum();
            true
        };
        flipped || moved
    }
}
