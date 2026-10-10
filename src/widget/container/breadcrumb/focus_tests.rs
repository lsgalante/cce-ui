use super::*;
use crate::widget::{ElementState, Event, Key, KeyEvent, NamedKey, UiContext, WidgetHost};

fn press(key: NamedKey) -> Event {
    Event::KeyInput(KeyEvent { logical_key: Key::Named(key), state: ElementState::Pressed, text: None, repeat: false, ctrl: false, shift: false, alt: false })
}

/// Focus lands the cursor on the current directory; Left walks back a
/// visible segment; Enter navigates to the cursor's segment (the click).
#[test]
fn cursor_walks_segments_and_enter_navigates() {
    let mut ctx = UiContext::new();
    let mut b = Breadcrumb::new();
    b.set_path(&["home".to_string(), "lsgalante".to_string(), "projects".to_string()]);
    WidgetHost::set_rect(&mut b, 10.0, 20.0, 400.0, 26.0);
    b.handle_event(&Event::FocusIn, &mut ctx);
    assert_eq!(b.inner().focus_seg, Some(2), "cursor on the current directory");
    assert!(b.handle_event(&press(NamedKey::ArrowLeft), &mut ctx));
    assert_eq!(b.inner().focus_seg, Some(1));
    assert!(b.handle_event(&press(NamedKey::Enter), &mut ctx));
    assert_eq!(b.inner().clicked_seg, Some(1), "Enter is the click on the cursor's segment");
    b.handle_event(&Event::FocusOut, &mut ctx);
    assert_eq!(b.inner().focus_seg, None);
}
