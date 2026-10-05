//! The DOM's input vocabulary in cce-ui's terms: a `KeyboardEvent.key` as a
//! [`Key`] and the text it types, a `WheelEvent` as a [`ScrollFrame`]. The
//! browser shell (`web::shell`) feeds these to the [`Driver`](super::driver::Driver),
//! as the Wayland shell maps xkb keysyms and `wl_pointer` axis frames. Plain
//! functions of strings and numbers, so they build and are tested natively.

use super::driver::{ScrollFrame, ScrollSource};
use crate::widget::{Key, NamedKey};

/// A `KeyboardEvent.key` as cce-ui's key and the text it types — the
/// browser's half of the Wayland shell's keysym map, text included: what
/// xkb's `utf8` would carry for the same key, since that is what widgets
/// have always been handed (Tab types "\t" and Enter "\r"; with Ctrl, or
/// Command on a Mac, a letter types its control code and is matched by the
/// letter). `None` for a key the toolkit has no use for (an IME's `Process`,
/// a dead key).
pub fn map_key(key: &str, accel: bool) -> Option<(Key, Option<String>)> {
    let named = |n| Some((Key::Named(n), None));
    let typing = |n, text: &str| Some((Key::Named(n), Some(text.to_string())));
    match key {
        "Escape" => typing(NamedKey::Escape, "\u{1b}"),
        "Enter" => typing(NamedKey::Enter, "\r"),
        "Backspace" => typing(NamedKey::Backspace, "\u{8}"),
        "Tab" => typing(NamedKey::Tab, "\t"),
        "Delete" => typing(NamedKey::Delete, "\u{7f}"),
        " " => typing(NamedKey::Space, if accel { "\0" } else { " " }),
        "ArrowDown" => named(NamedKey::ArrowDown),
        "ArrowUp" => named(NamedKey::ArrowUp),
        "ArrowLeft" => named(NamedKey::ArrowLeft),
        "ArrowRight" => named(NamedKey::ArrowRight),
        "PageUp" => named(NamedKey::PageUp),
        "PageDown" => named(NamedKey::PageDown),
        "Home" => named(NamedKey::Home),
        "End" => named(NamedKey::End),
        "Meta" | "OS" => named(NamedKey::Super),
        "Alt" => named(NamedKey::Alt),
        "Control" => named(NamedKey::Control),
        "Shift" => named(NamedKey::Shift),
        "F1" => named(NamedKey::F1),
        "F2" => named(NamedKey::F2),
        "F3" => named(NamedKey::F3),
        "F4" => named(NamedKey::F4),
        "F5" => named(NamedKey::F5),
        "F6" => named(NamedKey::F6),
        "F7" => named(NamedKey::F7),
        "F8" => named(NamedKey::F8),
        "F9" => named(NamedKey::F9),
        "F10" => named(NamedKey::F10),
        "F11" => named(NamedKey::F11),
        "F12" => named(NamedKey::F12),
        // A key that types: one character (a named key's name is longer).
        k if k.chars().count() == 1 => {
            let c = k.chars().next().unwrap();
            let text = match c {
                // xkb's control transformation: @, A–Z, [ \ ] ^ _ (either
                // case of the letters) to 0x00–0x1f.
                '@'..='_' | 'a'..='z' if accel => char::from(c.to_ascii_uppercase() as u8 & 0x1f).to_string(),
                _ => k.to_string(),
            };
            Some((Key::Character(k.into()), Some(text)))
        }
        _ => None,
    }
}

/// A clipboard shortcut, as the page's clipboard events are raised by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipKey {
    Copy,
    Cut,
    Paste,
}

/// Whether a `KeyboardEvent.key` with Ctrl (or ⌘) held, and Alt not, is a
/// clipboard shortcut: the browser shell lets its default through, since
/// that default is the `copy`, `cut` or `paste` event — and a page gets the
/// clipboard's text only inside a `paste` event.
pub fn clipboard_key(key: &str, accel: bool, alt: bool) -> Option<ClipKey> {
    if !accel || alt {
        return None;
    }
    match key {
        "c" | "C" => Some(ClipKey::Copy),
        "x" | "X" => Some(ClipKey::Cut),
        "v" | "V" => Some(ClipKey::Paste),
        _ => None,
    }
}

/// A `WheelEvent`'s deltas as a scroll frame in the units the driver takes
/// (a `wl_pointer` axis frame's: a finger in px, a wheel notch as a discrete
/// step plus ten units, positive down and right — the DOM's signs too).
///
/// A page cannot ask which device scrolled. A line or page delta is a wheel
/// (Firefox's notches); a pixel delta is a wheel when it comes in whole
/// notch-sized steps on one axis (Chromium's 100 px a notch, or 120), and a
/// finger otherwise — a trackpad's deltas are small and fractional.
pub fn wheel_frame(mode: u32, dx: f64, dy: f64) -> ScrollFrame {
    // Whole notches, at least one each way a delta goes.
    let notches = |d: f64, per: f64| -> i32 {
        match (d / per).round() as i32 {
            _ if d == 0.0 => 0,
            0 => d.signum() as i32,
            n => n.clamp(-100, 100),
        }
    };
    let wheel = |per: f64| {
        let (nh, nv) = (notches(dx, per), notches(dy, per));
        ScrollFrame {
            h: nh as f64 * 10.0,
            v: nv as f64 * 10.0,
            discrete_h: nh,
            discrete_v: nv,
            source: Some(ScrollSource::Wheel),
            stop: false,
        }
    };
    match mode {
        // DOM_DELTA_LINE: Firefox gives three lines a notch.
        1 => wheel(3.0),
        // DOM_DELTA_PAGE: a notch is a page.
        2 => wheel(1.0),
        _ => {
            let notch_sized = |d: f64| d == 0.0 || (d.fract() == 0.0 && d.abs() >= 50.0);
            if (dx == 0.0 || dy == 0.0) && notch_sized(dx) && notch_sized(dy) {
                wheel(100.0)
            } else {
                ScrollFrame { h: dx, v: dy, source: Some(ScrollSource::Finger), ..Default::default() }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_keys_type_what_xkb_types() {
        assert_eq!(map_key("Tab", false), Some((Key::Named(NamedKey::Tab), Some("\t".into()))));
        assert_eq!(map_key("Enter", false), Some((Key::Named(NamedKey::Enter), Some("\r".into()))));
        assert_eq!(map_key("Backspace", false), Some((Key::Named(NamedKey::Backspace), Some("\u{8}".into()))));
        assert_eq!(map_key(" ", false), Some((Key::Named(NamedKey::Space), Some(" ".into()))));
        assert_eq!(map_key("ArrowLeft", false), Some((Key::Named(NamedKey::ArrowLeft), None)));
        assert_eq!(map_key("Dead", false), None);
        assert_eq!(map_key("Process", false), None);
    }

    #[test]
    fn a_shortcut_is_matched_by_its_letter_and_types_its_control_code() {
        assert_eq!(map_key("a", false), Some((Key::Character("a".into()), Some("a".into()))));
        assert_eq!(map_key("z", true), Some((Key::Character("z".into()), Some("\u{1a}".into()))));
        // Ctrl+Shift+Z: the shifted letter, the same control code.
        assert_eq!(map_key("Z", true), Some((Key::Character("Z".into()), Some("\u{1a}".into()))));
        assert_eq!(map_key("1", true), Some((Key::Character("1".into()), Some("1".into()))));
        assert_eq!(map_key("é", false), Some((Key::Character("é".into()), Some("é".into()))));
    }

    #[test]
    fn only_an_accelerated_c_x_or_v_is_a_clipboard_shortcut() {
        assert_eq!(clipboard_key("c", true, false), Some(ClipKey::Copy));
        assert_eq!(clipboard_key("X", true, false), Some(ClipKey::Cut));
        assert_eq!(clipboard_key("v", true, false), Some(ClipKey::Paste));
        // Ctrl+Shift+V, a paste too.
        assert_eq!(clipboard_key("V", true, false), Some(ClipKey::Paste));
        assert_eq!(clipboard_key("v", false, false), None);
        assert_eq!(clipboard_key("v", true, true), None);
        assert_eq!(clipboard_key("z", true, false), None);
    }

    #[test]
    fn notches_are_discrete_and_a_trackpad_is_a_finger() {
        // Chromium's mouse wheel: 100 px a notch, three notches up.
        let f = wheel_frame(0, 0.0, -300.0);
        assert_eq!((f.discrete_v, f.discrete_h, f.source), (-3, 0, Some(ScrollSource::Wheel)));
        assert_eq!(f.v, -30.0);
        // Firefox's: three lines a notch.
        let f = wheel_frame(1, 0.0, 3.0);
        assert_eq!((f.discrete_v, f.source), (1, Some(ScrollSource::Wheel)));
        // A page delta is a notch.
        assert_eq!(wheel_frame(2, 0.0, -1.0).discrete_v, -1);
        // A trackpad: small, fractional, often on both axes.
        let f = wheel_frame(0, 1.5, -4.25);
        assert_eq!((f.discrete_v, f.h, f.v, f.source), (0, 1.5, -4.25, Some(ScrollSource::Finger)));
        // Whole but notch-sized on both axes at once: a finger too.
        assert_eq!(wheel_frame(0, 100.0, 100.0).source, Some(ScrollSource::Finger));
    }
}
