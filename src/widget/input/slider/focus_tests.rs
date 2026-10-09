use super::*;
use crate::widget::{Event, KeyEvent, UiContext, WidgetHost};

fn press(key: NamedKey) -> Event {
    Event::KeyInput(KeyEvent { logical_key: Key::Named(key), state: ElementState::Pressed, text: None, repeat: false, ctrl: false, shift: false, alt: false })
}

/// A focused range: Right steps the low end, Down switches to the high end,
/// Left steps it, End sends it to 1, and the low end can never pass the high.
#[test]
fn range_arrows_step_the_focused_end_and_up_down_switch() {
    let mut ctx = UiContext::new();
    let mut r = RangeSlider::new().with_values(0.2, 0.8);
    WidgetHost::set_rect(&mut r, 0.0, 0.0, 200.0, 16.0);
    assert!(!r.handle_event(&press(NamedKey::ArrowRight), &mut ctx), "unfocused: not this range's key");
    r.handle_event(&Event::FocusIn, &mut ctx);
    assert!(r.handle_event(&press(NamedKey::ArrowRight), &mut ctx));
    let (lo, hi) = r.inner().values();
    assert!((lo - 0.22).abs() < 1e-5 && (hi - 0.8).abs() < 1e-5, "the low end moved");
    assert!(r.handle_event(&press(NamedKey::ArrowDown), &mut ctx));
    assert!(r.handle_event(&press(NamedKey::ArrowLeft), &mut ctx));
    let (lo, hi) = r.inner().values();
    assert!((lo - 0.22).abs() < 1e-5 && (hi - 0.78).abs() < 1e-5, "then the high end");
    assert!(r.handle_event(&press(NamedKey::End), &mut ctx));
    assert_eq!(r.inner().values().1, 1.0);
    assert!(r.handle_event(&press(NamedKey::ArrowUp), &mut ctx));
    assert!(r.handle_event(&press(NamedKey::End), &mut ctx));
    assert_eq!(r.inner().values(), (1.0, 1.0), "the low end stops at the high end");
}

/// A focused band steps by a wheel notch on the arrows, jumps on Home / End,
/// and opens its readout on Enter; unfocused it ignores the keys.
/// The arrows step a wide range by the wheel's own notch.
#[test]
fn arrows_step_a_wide_range_by_the_values_magnitude() {
    let mut ctx = UiContext::new();
    let mut s = Slider::new().with_range(-1000.0, 1000.0).with_readout(true);
    WidgetHost::set_rect(&mut s, 0.0, 0.0, 200.0, 16.0);
    s.inner_mut().set_scaled_value(0.0);
    s.handle_event(&Event::FocusIn, &mut ctx);
    assert!(s.handle_event(&press(NamedKey::ArrowRight), &mut ctx));
    assert!((s.inner().get_scaled_value() - 0.4).abs() < 1e-2, "{}", s.inner().get_scaled_value());
}

#[test]
fn arrows_step_the_band_and_enter_opens_the_readout() {
    let mut ctx = UiContext::new();
    let mut s = Slider::new().with_readout(true);
    WidgetHost::set_rect(&mut s, 0.0, 0.0, 200.0, 16.0);
    let v0 = s.inner().value;
    assert!(!s.handle_event(&press(NamedKey::ArrowRight), &mut ctx));
    assert_eq!(s.inner().value, v0, "unfocused: untouched");
    s.handle_event(&Event::FocusIn, &mut ctx);
    assert!(s.handle_event(&press(NamedKey::ArrowRight), &mut ctx));
    assert!((s.inner().value - (v0 + 0.02)).abs() < 1e-5);
    assert!(s.handle_event(&press(NamedKey::End), &mut ctx));
    assert_eq!(s.inner().value, 1.0);
    assert!(s.handle_event(&press(NamedKey::Home), &mut ctx));
    assert_eq!(s.inner().value, 0.0);
    assert!(s.handle_event(&press(NamedKey::Enter), &mut ctx));
    assert!(s.inner().editing, "Enter opens the readout for typing");
}
