//! The input vocabulary every shell hands the toolkit in: element states, buttons, positions, wheel
//! deltas (with how a value control reads them), keys and key events, the routed `Event`, and
//! matching a key event against a shortcut string.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementState {
    Pressed,
    Released,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u16),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MouseScrollDelta {
    LineDelta(f32, f32),
    PixelDelta(Position),
}

impl MouseScrollDelta {
    /// Vertical scroll in wheel-notch equivalents for VALUE widgets (sliders,
    /// float3 rows). The pixel divisor is calibrated against a measured
    /// trackpad stream, not a notch convention: a real two-finger swipe
    /// delivers 10–20 axis units per event at 6–8ms intervals (~2000
    /// units/sec sustained). At 60 units per notch-equivalent (0.02 of the
    /// range each), that sustains ~0.6 range/sec — a full sweep is a couple
    /// of committed swipes, while slow fine-tuning events (2–5 units) move
    /// well under one readout tick. 15 (the DE's hardware-notch unit) slams
    /// bound-to-bound in ~150ms; 120 (the wheel standard) needs ~6000px of
    /// finger travel per sweep.
    pub fn notches_y(&self) -> f32 {
        match self {
            MouseScrollDelta::LineDelta(_x, y) => *y,
            MouseScrollDelta::PixelDelta(pos) => (pos.y as f32) / 60.0,
        }
    }

    /// The notches a VALUE control takes, "up is more": a wheel notch up
    /// is positive, and a finger's travel is positive when the fingers
    /// went UP — which under natural scrolling is the negative of the
    /// pixel delta, since that delta is what a list scrolls by and a
    /// natural list moves its content the way the fingers went. Until
    /// 2026-09-30 every value control read `notches_y` and each had picked
    /// a sign: the slider was right for a natural trackpad and backwards
    /// for a wheel, the spinbox and the menu and palette sliders the other
    /// way round.
    pub fn value_notches_y(&self) -> f32 {
        self.value_notches_of(crate::input::natural_scroll())
    }

    /// [`Self::value_notches_y`] for a given natural-scroll setting.
    pub fn value_notches_of(&self, natural: bool) -> f32 {
        match self {
            MouseScrollDelta::LineDelta(_x, y) => *y,
            MouseScrollDelta::PixelDelta(pos) => {
                let n = (pos.y as f32) / 60.0;
                if natural { -n } else { n }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    Named(NamedKey),
    Character(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NamedKey {
    Backspace,
    Tab,
    Enter,
    Escape,
    Space,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    End,
    Home,
    PageDown,
    PageUp,
    Delete,
    Control,
    Shift,
    Alt,
    Super,
    // The function keys. F5 came first (the login greeter's restart); the
    // rest arrived together on 2026-09-25 for the greeter's F1 power off and
    // F2 reboot.
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    pub state: ElementState,
    pub logical_key: Key,
    pub text: Option<String>,
    pub repeat: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    PointerMove { x: f32, y: f32, local_x: f32, local_y: f32 },
    MouseButton { button: MouseButton, state: ElementState, x: f32, y: f32, local_x: f32, local_y: f32 },
    MouseWheel { delta: MouseScrollDelta, x: f32, y: f32, local_x: f32, local_y: f32 },
    KeyInput(KeyEvent),
    Tick(f32),

    MouseEnter,
    MouseLeave,
    DragStart { start_x: f32, start_y: f32 },
    DragUpdate { dx: f32, dy: f32, x: f32, y: f32, local_x: f32, local_y: f32 },
    DragEnd,
    FocusIn,
    FocusOut,
}

pub fn match_key_shortcut(event: &KeyEvent, shortcut_str: &str) -> bool {
    let shortcut_lower = shortcut_str.to_lowercase();
    let parts: Vec<&str> = shortcut_lower.split('+').collect();
    
    let mut req_ctrl = false;
    let mut req_shift = false;
    let mut req_alt = false;
    let mut req_key = "";

    for part in parts {
        match part {
            "ctrl" | "control" => req_ctrl = true,
            "shift" => req_shift = true,
            "alt" | "meta" => req_alt = true,
            // Super chords belong to the compositor; a client never sees them.
            "super" | "win" | "logo" => {}
            k => req_key = k,
        }
    }

    if event.ctrl != req_ctrl { return false; }
    if event.shift != req_shift { return false; }
    if event.alt != req_alt { return false; }
    
    if let Key::Character(ref ch) = event.logical_key {
        let ch_lower = ch.to_lowercase();
        if req_key.len() == 1 {
            return ch_lower == req_key;
        } else {
            let mapped_key = match req_key {
                "slash" => "/",
                "enter" => "enter",
                "escape" => "escape",
                "space" => " ",
                k => k,
            };
            return ch_lower == mapped_key;
        }
    } else if let Key::Named(nk) = event.logical_key {
        let nk_str = format!("{:?}", nk).to_lowercase();
        return nk_str == req_key;
    }
    false
}

#[cfg(test)]
mod value_notch_tests {
    use super::{MouseScrollDelta, Position};

    /// A value control reads "up is more": a wheel notch up is positive
    /// either way; a finger's pixel delta is taken as it comes with natural
    /// scrolling off, and negated with it on, since that delta is what a
    /// list scrolls by and a natural list follows the fingers.
    #[test]
    fn a_value_control_reads_up_as_more_on_a_wheel_and_a_natural_finger() {
        let wheel_up = MouseScrollDelta::LineDelta(0.0, 1.0);
        let finger = MouseScrollDelta::PixelDelta(Position { x: 0.0, y: -60.0 });
        assert_eq!(wheel_up.value_notches_of(false), 1.0);
        assert_eq!(wheel_up.value_notches_of(true), 1.0);
        assert_eq!(finger.value_notches_of(false), -1.0, "natural off: the delta as it comes");
        assert_eq!(finger.value_notches_of(true), 1.0, "natural on: the fingers went up, so more");
        // Under `cfg(test)` the toolkit's own suite reads natural as off,
        // unless a thread forces it.
        assert_eq!(finger.value_notches_y(), -1.0);
        crate::input::force_natural_scroll(Some(true));
        assert_eq!(finger.value_notches_y(), 1.0);
        crate::input::force_natural_scroll(None);
        assert_eq!(finger.value_notches_y(), -1.0);
    }
}
