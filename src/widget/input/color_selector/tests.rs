use super::*;

#[test]
fn test_colorselector_keyboard_navigation() {
    let mut dummy = crate::context::UiContext::new();
    let mut cs = ColorSelector::new([255, 0, 0]);
    assert_eq!(cs.color, [255, 0, 0]);
    assert_eq!(cs.alpha, 255);
    assert!(!cs.editing);

    // 1. Focus color selector
    cs.focus();
    assert!(cs.editing);
    assert_eq!(cs.edit_buffer, "#ff0000");
    assert_eq!(cs.cursor_idx, 7);

    // 2. Backspace deletes character before cursor
    let backspace_ev = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Named(NamedKey::Backspace),
        text: None,
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    let handled = cs.keyboard_input(&backspace_ev, &mut dummy);
    assert!(handled);
    assert_eq!(cs.edit_buffer, "#ff000");
    assert_eq!(cs.cursor_idx, 6);

    // 3. ArrowLeft moves cursor index
    let left_ev = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Named(NamedKey::ArrowLeft),
        text: None,
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    let handled = cs.keyboard_input(&left_ev, &mut dummy);
    assert!(handled);
    assert_eq!(cs.cursor_idx, 5);

    // 4. Backspace at cursor index 5
    let handled = cs.keyboard_input(&backspace_ev, &mut dummy);
    assert!(handled);
    assert_eq!(cs.edit_buffer, "#ff00");
    assert_eq!(cs.cursor_idx, 4);

    // 5. ArrowRight moves cursor index
    let right_ev = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Named(NamedKey::ArrowRight),
        text: None,
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    let handled = cs.keyboard_input(&right_ev, &mut dummy);
    assert!(handled);
    assert_eq!(cs.cursor_idx, 5);

    // 6. Delete deletes character at cursor
    let delete_ev = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Named(NamedKey::Delete),
        text: None,
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    // Move cursor to index 3
    let handled = cs.keyboard_input(&left_ev, &mut dummy); // 4
    assert!(handled);
    let handled = cs.keyboard_input(&left_ev, &mut dummy); // 3
    assert!(handled);
    assert_eq!(cs.cursor_idx, 3);
    // buffer is "#ff0", index 3 points to the first '0'. Let's delete it.
    let handled = cs.keyboard_input(&delete_ev, &mut dummy);
    assert!(handled);
    assert_eq!(cs.edit_buffer, "#ff0");
    assert_eq!(cs.cursor_idx, 3);

    // 7. Typing hex character inserts at cursor index
    let type_b_ev = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Character("b".to_string()),
        text: Some("b".to_string()),
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    let handled = cs.keyboard_input(&type_b_ev, &mut dummy);
    assert!(handled);
    assert_eq!(cs.edit_buffer, "#ffb0");
    assert_eq!(cs.cursor_idx, 4);

    // Move to index 1 and insert a 'c'
    for _ in 0..3 {
        let handled = cs.keyboard_input(&left_ev, &mut dummy);
        assert!(handled);
    }
    assert_eq!(cs.cursor_idx, 1);
    let type_c_ev = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Character("c".to_string()),
        text: Some("c".to_string()),
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    let handled = cs.keyboard_input(&type_c_ev, &mut dummy);
    assert!(handled);
    assert_eq!(cs.edit_buffer, "#cffb0");
    assert_eq!(cs.cursor_idx, 2);

    // 8. Enter commits the color
    let type_5_ev = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Character("5".to_string()),
        text: Some("5".to_string()),
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    // Move to index 5
    for _ in 0..4 {
        cs.keyboard_input(&right_ev, &mut dummy);
    }
    assert_eq!(cs.cursor_idx, 6);
    cs.keyboard_input(&type_5_ev, &mut dummy);
    assert_eq!(cs.edit_buffer, "#cffb05");
    assert_eq!(cs.cursor_idx, 7);

    let enter_ev = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Named(NamedKey::Enter),
        text: None,
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    let handled = cs.keyboard_input(&enter_ev, &mut dummy);
    assert!(handled);
    assert!(!cs.editing);
    assert_eq!(cs.color, [0xcf, 0xfb, 0x05]);
    assert_eq!(cs.alpha, 255);
}

#[test]
fn test_colorselector_alpha_support() {
    let mut dummy = crate::context::UiContext::new();
    let mut cs = ColorSelector::new_rgba([255, 0, 0, 128]);
    assert_eq!(cs.color, [255, 0, 0]);
    assert_eq!(cs.alpha, 128);
    assert!(cs.with_alpha);

    cs.focus();
    assert!(cs.editing);
    assert_eq!(cs.edit_buffer, "#ff000080");
    assert_eq!(cs.cursor_idx, 9);

    let type_a_ev = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Character("a".to_string()),
        text: Some("a".to_string()),
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    let backspace_ev = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Named(NamedKey::Backspace),
        text: None,
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    cs.keyboard_input(&backspace_ev, &mut dummy);
    cs.keyboard_input(&backspace_ev, &mut dummy);
    assert_eq!(cs.edit_buffer, "#ff0000");

    let type_b_ev = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Character("b".to_string()),
        text: Some("b".to_string()),
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    cs.keyboard_input(&type_a_ev, &mut dummy);
    cs.keyboard_input(&type_b_ev, &mut dummy);
    assert_eq!(cs.edit_buffer, "#ff0000ab");

    let enter_ev = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Named(NamedKey::Enter),
        text: None,
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    cs.keyboard_input(&enter_ev, &mut dummy);
    assert!(!cs.editing);
    assert_eq!(cs.color, [255, 0, 0]);
    assert_eq!(cs.alpha, 171);
}
/// The alpha preview's checker must not reach the legacy plain-quad
/// view: a host that draws that view beside the paint draws it AFTER the
/// prims, and the white cells then landed over the colour fill — a black
/// swatch read as a black-and-white checker. The paint keeps its order
/// (base, cells, colour on top) and the plain view carries no cells.
#[test]
fn alpha_checker_stays_out_of_the_plain_quad_view() {
    use crate::scene::paint::Prim;
    let mut cs = ColorSelector::new_rgba([0, 0, 0, 255]);
    cs.recessed = Some(true);
    let rect = Rect { x: 0.0, y: 0.0, width: 200.0, height: 40.0 };
    crate::widget::WidgetHost::set_rect(&mut cs, rect.x, rect.y, rect.width, rect.height);
    let mut pc = PaintCtx::new();
    let inner: &ColorSelector = &cs;
    crate::widget::Paint::paint(inner, rect, &mut pc);
    let items = pc.finish().items;
    let last_fill = items
        .iter()
        .rposition(|it| matches!(it.prim, Prim::RoundedRect { color, .. } if color == [0.0, 0.0, 0.0, 1.0]))
        .expect("the colour fill is painted");
    let white_cells: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, it)| matches!(it.prim, Prim::RoundedRect { color, radius, .. } if color == [1.0; 4] && radius == 0.0))
        .map(|(i, _)| i)
        .collect();
    assert!(!white_cells.is_empty(), "the checker is painted");
    assert!(white_cells.iter().all(|&i| i < last_fill), "every cell is under the colour fill");
    assert!(
        !items.iter().any(|it| matches!(it.prim, Prim::Quad { color, .. } if color == [1.0; 4])),
        "no cell is a plain quad, so the legacy plain view cannot carry it"
    );
    let plain = crate::widget::shown_quads(&cs);
    assert!(plain.iter().all(|q| q.4 != [1.0, 1.0, 1.0, 1.0]), "the plain view has no white cells: {plain:?}");
}
