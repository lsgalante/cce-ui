//! The scroll settings: read once from `input.kdl` (the app's domain, then `cce-ui`), with the
//! animations switch stopping a wheel's glide (and never a flick's coast), and a test's per-thread
//! override.

/// The process-wide smooth-scroll tunables, resolved once from `input.kdl`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollSettings {
    /// Wheel notches glide toward their target (false = the legacy jump).
    pub smooth: bool,
    /// Wheel glide rate, 1/s. 12 reaches 95% of a notch in ~250ms.
    pub ease_rate: f32,
    /// Trackpad flicks coast after the lift.
    pub kinetic: bool,
    /// Coast decay, 1/s. 6 halves the speed every ~115ms.
    pub friction: f32,
}

impl Default for ScrollSettings {
    fn default() -> Self {
        Self { smooth: true, ease_rate: 12.0, kinetic: true, friction: 6.0 }
    }
}

pub(super) static SETTINGS: std::sync::OnceLock<ScrollSettings> = std::sync::OnceLock::new();

/// This app's effective smooth-scroll settings (`<app>` → `cce-ui` → defaults).
/// With animations off ([`crate::motion`]) a wheel notch jumps, whatever
/// input.kdl says; that is checked per call, so it follows the switch while
/// the app runs.
///
/// **A flick coasts either way.** Until 2026-09-29 the switch turned the
/// coast off with the glide, so on a power mode that has animations off a
/// trackpad scroll stopped dead at the lift in every pane. The glide of a
/// notch is an animation the toolkit adds, and the switch is for those;
/// the coast is the rest of a gesture the hand made, and `kinetic_scroll`
/// in input.kdl is the setting for it.
///
/// A thread's override ([`force_scroll_settings`], for a test) wins over
/// all of it, animations switch included.
pub fn scroll_settings() -> ScrollSettings {
    if let Some(forced) = SETTINGS_OVERRIDE.with(|f| f.get()) {
        return forced;
    }
    with_animations(configured_scroll_settings(), crate::motion::enabled())
}

thread_local! {
    static SETTINGS_OVERRIDE: std::cell::Cell<Option<ScrollSettings>> = const { std::cell::Cell::new(None) };
}

/// Force what [`scroll_settings`] answers on this thread, for a test whose
/// result hangs on a coast or a glide — which input.kdl and the power
/// mode's animations switch would otherwise decide on the machine that runs
/// it. `None` lifts it. Thread-local, as `input::force_natural_scroll` is,
/// because a suite runs its tests in parallel.
pub fn force_scroll_settings(settings: Option<ScrollSettings>) {
    SETTINGS_OVERRIDE.with(|f| f.set(settings));
}

/// `configured` as the animations switch leaves it: the glide follows the
/// switch, the coast does not.
pub(super) fn with_animations(configured: ScrollSettings, animations: bool) -> ScrollSettings {
    ScrollSettings { smooth: configured.smooth && animations, ..configured }
}

pub(super) fn configured_scroll_settings() -> ScrollSettings {
    *SETTINGS.get_or_init(|| {
        let input = crate::input::cached();
        let app = crate::config::get_app_name().unwrap_or_default();
        let d = ScrollSettings::default();
        let flag = |key: &str, default: bool| {
            input
                .resolve_setting(&app, "", key)
                .and_then(crate::input::SettingValue::as_bool)
                .unwrap_or(default)
        };
        let rate = |key: &str, default: f32| {
            input
                .resolve_setting(&app, "", key)
                .and_then(crate::input::SettingValue::as_f64)
                .map(|v| v as f32)
                .filter(|v| v.is_finite() && *v > 0.0)
                .unwrap_or(default)
        };
        ScrollSettings {
            smooth: flag("smooth_scroll", d.smooth),
            ease_rate: rate("scroll_ease", d.ease_rate),
            kinetic: flag("kinetic_scroll", d.kinetic),
            friction: rate("scroll_friction", d.friction),
        }
    })
}
