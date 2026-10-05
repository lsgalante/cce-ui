//! AppKit's input vocabulary in cce-ui's terms: an `NSEvent`'s key code and
//! characters as a [`Key`] and the text it types, its scroll deltas and
//! phases as a [`ScrollFrame`], its modifier flags as the driver's
//! [`Modifiers`]. The macOS shell (`mac::shell`) feeds these to the
//! [`Driver`](super::driver::Driver), as the browser shell feeds `dom`'s.
//! Plain functions of numbers and strings, so they build and are tested on
//! every target — the shell itself can only be built on a Mac.

use super::dom;
use super::driver::{Modifiers, ScrollFrame, ScrollSource};
use crate::widget::{Key, MouseButton, NamedKey};

/// `NSEventModifierFlags` bits the shell reads.
pub mod flags {
    pub const SHIFT: u64 = 1 << 17;
    pub const CONTROL: u64 = 1 << 18;
    pub const OPTION: u64 = 1 << 19;
    pub const COMMAND: u64 = 1 << 20;
}

/// `NSEventPhase` bits: a trackpad gesture's place in its life.
pub mod phase {
    pub const BEGAN: u64 = 1;
    pub const STATIONARY: u64 = 2;
    pub const CHANGED: u64 = 4;
    pub const ENDED: u64 = 8;
    pub const CANCELLED: u64 = 16;
    pub const MAY_BEGIN: u64 = 32;
}

/// The modifier flags as the driver's. Command is the shortcut key, as the
/// browser shell has it on a Mac (⌘Z is undo, matched as the `ctrl+z`
/// chord): it and Control both read as `ctrl`, and there is no `logo`.
pub fn modifiers(f: u64) -> Modifiers {
    Modifiers {
        ctrl: f & (flags::CONTROL | flags::COMMAND) != 0,
        shift: f & flags::SHIFT != 0,
        alt: f & flags::OPTION != 0,
        logo: false,
    }
}

/// An `NSEvent.buttonNumber` as the driver's button, given the flags it came
/// with: a Control-click is a right click, as every Mac app reads it (one
/// button is still a common mouse). The extra buttons are not the toolkit's.
pub fn button(number: i64, f: u64) -> Option<MouseButton> {
    match number {
        0 if f & flags::CONTROL != 0 && f & flags::COMMAND == 0 => Some(MouseButton::Right),
        0 => Some(MouseButton::Left),
        1 => Some(MouseButton::Right),
        2 => Some(MouseButton::Middle),
        _ => None,
    }
}

/// A key as cce-ui's key and the text it types — what xkb's `utf8` would
/// carry on Wayland, as `dom::map_key` gives it in a page. The named keys
/// are read from the hardware `key_code` (`kVK_*`), since AppKit spells
/// them in private-use characters (and Backspace as DEL); everything else
/// from the characters: `characters` as typed (Option and dead keys
/// applied: Option-e e is "é"), or with `accel` — Command or Control held —
/// `unmodified` (`charactersIgnoringModifiers`), so ⌘Z is the letter z
/// typing its control code, as Ctrl+Z is on Linux. `None` for a key the
/// toolkit has no use for: a dead key (no characters yet), a function key
/// past F12.
pub fn map_key(key_code: u16, characters: &str, unmodified: &str, accel: bool) -> Option<(Key, Option<String>)> {
    let named = |n| Some((Key::Named(n), None));
    let typing = |n, text: &str| Some((Key::Named(n), Some(text.to_string())));
    match key_code {
        0x24 | 0x4C => typing(NamedKey::Enter, "\r"),
        0x30 => typing(NamedKey::Tab, "\t"),
        0x31 => typing(NamedKey::Space, if accel { "\0" } else { " " }),
        0x33 => typing(NamedKey::Backspace, "\u{8}"),
        0x35 => typing(NamedKey::Escape, "\u{1b}"),
        0x75 => typing(NamedKey::Delete, "\u{7f}"),
        0x7B => named(NamedKey::ArrowLeft),
        0x7C => named(NamedKey::ArrowRight),
        0x7D => named(NamedKey::ArrowDown),
        0x7E => named(NamedKey::ArrowUp),
        0x73 => named(NamedKey::Home),
        0x77 => named(NamedKey::End),
        0x74 => named(NamedKey::PageUp),
        0x79 => named(NamedKey::PageDown),
        0x37 | 0x36 => named(NamedKey::Super),
        0x38 | 0x3C => named(NamedKey::Shift),
        0x3A | 0x3D => named(NamedKey::Alt),
        0x3B | 0x3E => named(NamedKey::Control),
        0x7A => named(NamedKey::F1),
        0x78 => named(NamedKey::F2),
        0x63 => named(NamedKey::F3),
        0x76 => named(NamedKey::F4),
        0x60 => named(NamedKey::F5),
        0x61 => named(NamedKey::F6),
        0x62 => named(NamedKey::F7),
        0x64 => named(NamedKey::F8),
        0x65 => named(NamedKey::F9),
        0x6D => named(NamedKey::F10),
        0x67 => named(NamedKey::F11),
        0x6F => named(NamedKey::F12),
        _ => {
            let chars = if accel { unmodified } else { characters };
            // AppKit's function-key characters (NSUpArrowFunctionKey …) are
            // private use: a key named above, or one the toolkit has no name for.
            if chars.chars().any(|c| ('\u{F700}'..='\u{F8FF}').contains(&c)) {
                return None;
            }
            dom::map_key(chars, accel)
        }
    }
}

/// A scroll event's deltas (`scrollingDeltaX` / `Y`) as a frame in the
/// driver's units — a `wl_pointer` axis frame's: positive down and right, a
/// finger in px, a wheel notch a discrete step plus ten units — or `None`
/// for an event that carries nothing to scroll by.
///
/// - **Which device**: `precise` (`hasPreciseScrollingDeltas`) is a
///   trackpad or a Magic Mouse, its deltas in points: a finger. Otherwise a
///   wheel, its deltas in lines, accelerated: a notch each whole line, and
///   at least one each way a delta goes.
/// - **Which way**: AppKit's delta is what the CONTENT moves by, the
///   opposite of what a list scrolls by. And the system has applied the
///   user's natural-scrolling setting (`inverted`,
///   `isDirectionInvertedFromDevice`) to both devices, where a Linux
///   compositor applies it to the trackpad alone; so a wheel is turned back
///   to its own direction, as a Wayland wheel is, and a value control reads
///   the notch as it does there. A finger keeps the system's direction, and
///   the shell tells `input::natural_scroll` what it is.
/// - **When it ends**: a finger's frame with `phase` ENDED or CANCELLED is
///   the lift. What follows a lift is the system's own momentum
///   (`momentum` nonzero), dropped: the toolkit coasts a flick itself
///   (`ScrollMotion`), and taking both would coast twice. A gesture's
///   touch-down (MAY_BEGIN) and a still finger carry nothing.
pub fn scroll_frame(dx: f64, dy: f64, precise: bool, inverted: bool, phase: u64, momentum: u64) -> Option<ScrollFrame> {
    if momentum != 0 {
        return None;
    }
    if precise {
        let stop = phase & (phase::ENDED | phase::CANCELLED) != 0;
        if dx == 0.0 && dy == 0.0 && !stop {
            return None;
        }
        return Some(ScrollFrame { h: -dx, v: -dy, source: Some(ScrollSource::Finger), stop, ..Default::default() });
    }
    // A wheel, in its own direction: down the list is positive.
    let device = if inverted { 1.0 } else { -1.0 };
    let notches = |d: f64| -> i32 {
        match d.round() as i32 {
            _ if d == 0.0 => 0,
            0 => d.signum() as i32,
            n => n.clamp(-100, 100),
        }
    };
    let (nh, nv) = (notches(dx * device), notches(dy * device));
    if nh == 0 && nv == 0 {
        return None;
    }
    Some(ScrollFrame {
        h: nh as f64 * 10.0,
        v: nv as f64 * 10.0,
        discrete_h: nh,
        discrete_v: nv,
        source: Some(ScrollSource::Wheel),
        stop: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_keys_come_from_the_key_code_and_type_what_xkb_types() {
        // Backspace: AppKit's characters are DEL, xkb's text is BS.
        assert_eq!(map_key(0x33, "\u{7f}", "\u{7f}", false), Some((Key::Named(NamedKey::Backspace), Some("\u{8}".into()))));
        assert_eq!(map_key(0x75, "\u{F728}", "\u{F728}", false), Some((Key::Named(NamedKey::Delete), Some("\u{7f}".into()))));
        assert_eq!(map_key(0x24, "\r", "\r", false), Some((Key::Named(NamedKey::Enter), Some("\r".into()))));
        assert_eq!(map_key(0x4C, "\u{3}", "\u{3}", false), Some((Key::Named(NamedKey::Enter), Some("\r".into()))));
        assert_eq!(map_key(0x30, "\t", "\t", false), Some((Key::Named(NamedKey::Tab), Some("\t".into()))));
        assert_eq!(map_key(0x7B, "\u{F702}", "\u{F702}", false), Some((Key::Named(NamedKey::ArrowLeft), None)));
        assert_eq!(map_key(0x60, "\u{F708}", "\u{F708}", false), Some((Key::Named(NamedKey::F5), None)));
        // F13: a function key with no name here.
        assert_eq!(map_key(0x69, "\u{F710}", "\u{F710}", false), None);
    }

    #[test]
    fn a_command_shortcut_is_its_letter_typing_its_control_code() {
        // ⌘Z: the letter, unmodified, typing ^Z as Ctrl+Z does on Linux.
        assert_eq!(map_key(0x06, "z", "z", true), Some((Key::Character("z".into()), Some("\u{1a}".into()))));
        // ⇧⌘Z.
        assert_eq!(map_key(0x06, "Z", "Z", true), Some((Key::Character("Z".into()), Some("\u{1a}".into()))));
        // Control-A: AppKit's characters are already ^A; the key is still a.
        assert_eq!(map_key(0x00, "\u{1}", "a", true), Some((Key::Character("a".into()), Some("\u{1}".into()))));
        assert_eq!(modifiers(flags::COMMAND | flags::SHIFT), Modifiers { ctrl: true, shift: true, alt: false, logo: false });
    }

    #[test]
    fn option_types_what_it_composes_and_a_dead_key_types_nothing_yet() {
        assert_eq!(map_key(0x00, "å", "a", false), Some((Key::Character("å".into()), Some("å".into()))));
        assert_eq!(map_key(0x0E, "", "e", false), None);
        assert_eq!(map_key(0x0E, "é", "e", false), Some((Key::Character("é".into()), Some("é".into()))));
    }

    #[test]
    fn a_control_click_is_a_right_click() {
        assert_eq!(button(0, 0), Some(MouseButton::Left));
        assert_eq!(button(0, flags::CONTROL), Some(MouseButton::Right));
        assert_eq!(button(0, flags::CONTROL | flags::COMMAND), Some(MouseButton::Left));
        assert_eq!(button(1, 0), Some(MouseButton::Right));
        assert_eq!(button(2, 0), Some(MouseButton::Middle));
        assert_eq!(button(3, 0), None);
    }

    #[test]
    fn a_trackpad_is_a_finger_scrolling_the_list_and_its_momentum_is_dropped() {
        // Fingers moving up under natural scrolling: content up, list down.
        let f = scroll_frame(0.0, -4.5, true, true, phase::CHANGED, 0).unwrap();
        assert_eq!((f.v, f.h, f.source, f.stop), (4.5, 0.0, Some(ScrollSource::Finger), false));
        // The lift.
        let f = scroll_frame(0.0, 0.0, true, true, phase::ENDED, 0).unwrap();
        assert!(f.stop);
        // The touch-down, and the system's coast after the lift.
        assert!(scroll_frame(0.0, 0.0, true, true, phase::MAY_BEGIN, 0).is_none());
        assert!(scroll_frame(0.0, -12.0, true, true, 0, phase::CHANGED).is_none());
    }

    #[test]
    fn a_wheel_notch_is_discrete_and_in_the_wheels_own_direction() {
        // Rolled toward the user (down the list), natural scrolling off:
        // AppKit says the content moves up.
        let f = scroll_frame(0.0, -1.0, false, false, 0, 0).unwrap();
        assert_eq!((f.discrete_v, f.v, f.source), (1, 10.0, Some(ScrollSource::Wheel)));
        // The same roll with natural scrolling on: AppKit flips the sign,
        // the frame does not.
        let f = scroll_frame(0.0, 1.0, false, true, 0, 0).unwrap();
        assert_eq!(f.discrete_v, 1);
        // Accelerated: three lines, three notches; a fraction is one.
        assert_eq!(scroll_frame(0.0, 3.2, false, false, 0, 0).unwrap().discrete_v, -3);
        assert_eq!(scroll_frame(0.0, 0.1, false, false, 0, 0).unwrap().discrete_v, -1);
        assert!(scroll_frame(0.0, 0.0, false, false, 0, 0).is_none());
    }
}
