use super::*;
use crate::widget::LayoutConstraints;

/// The trigger draws its text a cluster at a time, and each lands where the
/// word shaped whole puts it — "Create" drew as "Cr eat e" while the offsets
/// were measured a letter at a time.
#[test]
fn the_trigger_text_is_spaced_as_the_word_shapes() {
    let dd = Dropdown::new(vec!["Create".to_string()], 0);
    let mut pc = PaintCtx::new();
    dd.paint_text(Rect { x: 0.0, y: 0.0, width: 300.0, height: 24.0 }, &mut pc);
    let drawn: Vec<(String, f32)> = pc
        .finish()
        .items
        .into_iter()
        .filter_map(|item| match item.prim {
            crate::scene::paint::Prim::Text { text, x, .. } => Some((text, x)),
            _ => None,
        })
        .collect();
    assert_eq!(drawn.iter().map(|(t, _)| t.as_str()).collect::<String>(), "Create");
    let (_, size) = crate::layout::control_label_font_detached_parsed();
    let whole = shaped_clusters("Create", size).expect("the test process can shape");
    let x0 = drawn[0].1;
    for ((text, x), &(_, want)) in drawn.iter().zip(whole.iter()) {
        assert!((x - x0 - want).abs() < 0.01, "{text:?} drawn at {} where the word puts it at {want}", x - x0);
    }
}

#[test]
fn test_dropdown_widget_interaction() {
    let mut dummy = crate::context::UiContext::new();
    let options = vec!["Option A".to_string(), "Option B".to_string(), "Option C".to_string()];
    let mut dd = Dropdown::new(options, 0);
    dd.set_rect(10.0, 10.0, 100.0, 24.0);

    // 1. Initial State
    assert!(!dd.open);
    assert_eq!(dd.selected, 0);

    // 2. Click trigger area opens dropdown
    let input_changed = dd.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 20.0, &mut dummy);
    assert!(input_changed);
    assert!(dd.open);

    // 3. Hovering options inside popover
    // Popover starts at y = 10 + 24 = 34. Options are of height 24 each.
    // Hover option B at y = 34 + 24 + 12 = 70.0
    let move_changed = dd.on_cursor_moved(50.0, 70.0, &mut dummy);
    assert!(move_changed);
    assert_eq!(dd.hovered_item, Some(1));

    assert!(dd.is_expanded(), "open and taking input");
    // 4. Click option B selects it and starts the animated close
    let select_changed = dd.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 70.0, &mut dummy);
    assert!(select_changed);
    assert!(dd.closing, "selection starts the animated contraction");
    assert!(dd.open && !dd.is_expanded(), "drawn while it shrinks, but no longer taking input");
    dd.land_anim_for_test();
    assert!(!dd.open);
    assert_eq!(dd.selected, 1);
    assert!(dd.take_change());
}

#[test]
fn test_dropdown_context_menu_with_config() {
    let mut dummy = crate::context::UiContext::new();
    let options = vec!["Option A".to_string(), "Option B".to_string()];
    let mut dd = Dropdown::new(options, 0).with_config("path/to/config.json", "some_key");
    dd.set_rect(10.0, 10.0, 100.0, 24.0);

    assert!(!crate::widget::context_menu::is_visible());

    // Right click dropdown
    let handled = dd.mouse_input(MouseButton::Right, ElementState::Pressed, 50.0, 20.0, &mut dummy);
    assert!(handled);

    assert!(crate::widget::context_menu::is_visible());
    let menu_options = crate::widget::context_menu::options();
    assert!(menu_options.len() >= 3);
    assert_eq!(menu_options[1], "File: path/to/config.json");
    assert_eq!(menu_options[2], "Key: some_key");

    crate::widget::context_menu::hide();
    assert!(!crate::widget::context_menu::is_visible());
}

/// A closed dropdown takes Enter only when it is the focused widget.
///
/// Hosts that broadcast a key event to every widget root (cce-system-interface's
/// `dispatch_page_event`) depend on each widget declining what is not addressed to
/// it — a key event carries no position to filter on. Without the focus gate the
/// first dropdown in the host's dispatch order opened on *any* Enter that reached
/// it, swallowing the Return meant for whatever actually held focus: settings'
/// Browser page opened Page Color Scheme instead of committing the Homepage field,
/// and its Power page opened the last of nine menus instead of the focused one.
#[test]
fn closed_dropdown_takes_enter_only_when_focused() {
    let mut dummy = crate::context::UiContext::new();
    let enter = crate::widget::KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Named(NamedKey::Enter),
        text: None,
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };

    let opts = vec!["Dark".to_string(), "Light".to_string()];
    let mut dd = Dropdown::new(opts.clone(), 0);
    dd.set_rect(10.0, 10.0, 100.0, 24.0);

    // Nothing focused: the Return belongs to someone else, so it is declined and
    // the menu stays shut.
    dummy.focused_widget = None;
    assert!(!dd.keyboard_input(&enter, &mut dummy));
    assert!(!dd.open);

    // Another widget focused: same — this is the settings-page case, where the
    // focused TextBox sits later in the reversed dispatch order.
    let other = Dropdown::new(opts, 0);
    dummy.focused_widget = Some(other.id());
    assert!(!dd.keyboard_input(&enter, &mut dummy));
    assert!(!dd.open);

    // Focused: Enter opens it, and arms the hover on the selection as before.
    dummy.focused_widget = Some(dd.id());
    assert!(dd.keyboard_input(&enter, &mut dummy));
    assert!(dd.open);
    assert_eq!(dd.hovered_item, Some(0));

    // Once open the gate is out of the way, so Escape still closes it even if the
    // focus moved on (`FocusOut` closes it too, but the key path must not depend
    // on that having run).
    dummy.focused_widget = None;
    let escape = crate::widget::KeyEvent {
        logical_key: Key::Named(NamedKey::Escape),
        ..enter.clone()
    };
    assert!(dd.keyboard_input(&escape, &mut dummy));
    assert!(dd.closing || !dd.open);
}

#[test]
fn test_dropdown_separators() {
    let mut dummy = crate::context::UiContext::new();
    let options = vec![
        "Option A".to_string(),
        "-".to_string(),
        "Option B".to_string(),
    ];
    let mut dd = Dropdown::new(options, 0);
    dd.set_rect(10.0, 10.0, 100.0, 24.0);

    // Open dropdown
    dd.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 20.0, &mut dummy);
    assert!(dd.open);

    // Hover over separator at index 1 at y = 34 + 24 + 12 = 70.0
    dd.on_cursor_moved(50.0, 70.0, &mut dummy);
    assert_eq!(dd.hovered_item, None); // Separator should not be hovered

    // Click separator at index 1
    let clicked = dd.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 70.0, &mut dummy);
    assert!(clicked);
    assert!(dd.open); // Dropdown should remain open
    assert_eq!(dd.selected, 0); // Selection should not change

    // Hover over Option B at index 2 at y = 34 + 48 + 12 = 94.0
    dd.on_cursor_moved(50.0, 94.0, &mut dummy);
    assert_eq!(dd.hovered_item, Some(2));

    // Keyboard arrow up from index 2 should skip separator (index 1) and go to index 0
    let key_up = crate::widget::KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Named(NamedKey::ArrowUp),
        text: None,
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    dd.keyboard_input(&key_up, &mut dummy);
    assert_eq!(dd.hovered_item, Some(0));
}

#[test]
fn test_dropdown_ramp_parent_constraints() {
    let mut ramp = crate::widget::Ramp::new();
    // Set the rect of parent Ramp
    ramp.set_rect(20.0, 20.0, 410.0, 260.0);

    let options = vec![
        "Option 1".to_string(),
        "Option 2".to_string(),
        "Option 3".to_string(),
        "Option 4".to_string(),
        "Option 5".to_string(),
        "Option 6".to_string(),
    ];
    let mut dd = Dropdown::new(options, 0).with_label("Preset");
    dd.set_rect(30.0, 125.0, 110.0, 20.0);

    // Link the dropdown to the Ramp's read-data (the legacy direct-write path)
    dd.parent_snapshot = Some(ParentSnapshot {
        rect: crate::widget::WidgetHost::rect(&ramp),
        is_ramp: true,
        color: crate::widget::WidgetHostExt::color(&ramp),
    });

    // Compute geometry
    let (rx, ry, rw, rh) = dd.get_popover_geom();

    // Validate coordinates stay inside the parent Ramp bounds: x in [20, 430], y in [20, 280]
    assert!(rx >= 20.0, "rx {} should be >= 20.0", rx);
    assert!(rx + rw <= 430.0, "rx + rw {} should be <= 430.0", rx + rw);
    assert!(ry >= 20.0, "ry {} should be >= 20.0", ry);
    assert!(ry + rh <= 280.0, "ry + rh {} should be <= 280.0", ry + rh);
}

#[test]
fn test_dropdown_label_fade_out() {
    let options = vec!["This is a very long option name that will exceed the dropdown width".to_string()];
    let mut dd = Dropdown::new(options, 0);
    dd.set_rect(10.0, 10.0, 100.0, 24.0); // very narrow dropdown

    let labels = dd.own_text_labels();
    // Labels are individual characters of selected_text (the arrow is a glyph, not text).
    assert!(labels.len() > 2);

    // The last character label should be faded (i.e. not the default color)
    let last_char_idx = labels.len() - 1;
    let first_char = &labels[0];
    let last_char = &labels[last_char_idx];

    let tc = colors::dropdown_text_color();
    let expected_color = [
        (colors::linear_to_srgb(tc[0]) * 255.0).round() as u8,
        (colors::linear_to_srgb(tc[1]) * 255.0).round() as u8,
        (colors::linear_to_srgb(tc[2]) * 255.0).round() as u8,
    ];
    assert_eq!(first_char.color, expected_color);
    assert_ne!(last_char.color, expected_color); // color has shifted towards background
}

/// A list whose options carry the context menu's marks draws them as
/// glyphs in a column at the left, the labels after it and lined up, and
/// the trigger shows its value without the mark — the convention a
/// "View" menu-button marks its switches by. Skipped without the icon set.
#[test]
fn a_marked_option_draws_its_glyph() {
    if !std::path::Path::new(&crate::icons_dir()).join("check.svg").is_file() {
        eprintln!("skipped: no icon set");
        return;
    }
    #[derive(Default)]
    struct Rec {
        icons: Vec<(String, f32)>,
        texts: Vec<(String, f32)>,
    }
    impl crate::scene::paint::RenderTarget for Rec {
        fn rect(&mut self, _: [f32; 4], _: f32, _: f32, _: f32, _: f32) {}
        fn text(&mut self, t: &str, x: f32, _: f32, _: f32, _: [f32; 4]) {
            self.texts.push((t.to_string(), x));
        }
        fn text_with_font_and_bounds(&mut self, t: &str, x: f32, _: f32, _: f32, _: [f32; 4], _: &str, _: Option<[f32; 4]>) {
            self.texts.push((t.to_string(), x));
        }
        fn icon(&mut self, name: &str, r: Rect, _: [f32; 4]) {
            self.icons.push((name.to_string(), r.x));
        }
    }
    let opts = vec!["✓ Show Grid".to_string(), "Control Panel".to_string(), "○ Opacity 50%".to_string()];
    let mut dd = Dropdown::new(opts, 0).with_custom_display_text("View");
    let rect = Rect { x: 10.0, y: 10.0, width: 200.0, height: 24.0 };
    {
        let d = dd.inner_mut();
        d.open = true;
        d.anim_snap = 1.0;
    }
    let mut rec = Rec::default();
    crate::widget::Paint::draw_popover(dd.inner(), rect, &mut rec);
    // The trigger band redrawn over the list keeps its arrow; the rest
    // are the rows' marks.
    rec.icons.retain(|(n, _)| n != "chevron-down");
    let names: Vec<&str> = rec.icons.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["check", "circle-outline"]);
    let x_of = |t: &str| rec.texts.iter().find(|(s, _)| s == t).map(|(_, x)| *x).unwrap_or_else(|| panic!("{t:?} not drawn: {:?}", rec.texts));
    assert_eq!(x_of("Show Grid"), x_of("Control Panel"), "an unmarked row keeps the mark column");
    assert!(x_of("Show Grid") > rec.icons[0].1, "the label stands after its mark");
    assert!(rec.texts.iter().all(|(t, _)| !t.contains(['✓', '○', '●'])), "no mark drawn as text: {:?}", rec.texts);
    let plain = Dropdown::new(vec!["✓ On".to_string(), "Off".to_string()], 0);
    assert_eq!(plain.inner().display_text(), "On", "the trigger shows the value without its mark");
}

/// A trigger that carries nothing but its arrow (the textpick picker)
/// stands it in its middle: its padding about the arrow is even.
#[test]
fn a_centred_arrow_stands_in_the_middle_of_its_trigger() {
    let arrow_x = |center: bool| {
        let mut dd = Dropdown::new(vec!["a".to_string()], 0).with_custom_display_text("");
        dd.inner_mut().center_arrow = center;
        dd.set_rect(10.0, 10.0, 30.0, 24.0);
        dd.inner().arrow_rect(Rect { x: 10.0, y: 10.0, width: 30.0, height: 24.0 }).x
    };
    let adv = super::ARROW_SIDE;
    let x = arrow_x(true);
    assert!((x + 0.5 * adv - 25.0).abs() < 1e-3, "centred on the trigger's middle: {x} + {adv}/2");
    let slot = super::arrow_slot(24.0);
    assert!((arrow_x(false) + 0.5 * adv - (40.0 - 0.5 * slot)).abs() < 1e-3, "an ordinary trigger centres it in the slot at its right end");
}

#[test]
fn test_dropdown_auto_width() {
    let dummy = crate::context::UiContext::new();
    let options = vec!["Short".to_string(), "A much longer option name".to_string()];
    let mut dd = Dropdown::new(options, 0).with_auto_width(true);
    dd.set_rect(10.0, 10.0, 50.0, 24.0);

    let size = dd.measure(LayoutConstraints::new(0.0, 500.0, 24.0, 24.0), &dummy);
    assert!(size.width > 50.0, "Measured auto-width {} should be greater than original width 50.0", size.width);

    let dd_no_auto = Dropdown::new(vec!["Short".to_string(), "A much longer option name".to_string()], 0);
    let size_no_auto = dd_no_auto.measure(LayoutConstraints::new(0.0, 500.0, 24.0, 24.0), &dummy);
    assert_eq!(size_no_auto.width, 0.0);
}

#[test]
fn intrinsic_size_fits_widest_option() {
    let wide = Dropdown::new(
        vec!["Short".to_string(), "A much longer option name".to_string()],
        0,
    );
    let size = Layout::intrinsic_size(wide.inner()).expect("dropdown reports intrinsic size");
    assert!(size.width >= wide.content_width(), "width fits the widest option");
    assert_eq!(size.height, crate::layout::dropdown_height());

    let narrow = Dropdown::new(vec!["Hi".to_string()], 0);
    assert!(
        size.width > Layout::intrinsic_size(narrow.inner()).unwrap().width,
        "more/longer options measure wider",
    );
}

#[test]
fn menu_button_trigger_fits_display_text_not_widest_option() {
    // A menu-button dropdown (fixed custom display text) sizes its trigger to that text, so a
    // long menu entry (e.g. a recent-file path) no longer stretches the "File" button.
    let menu = Dropdown::new(
        vec![
            "New".to_string(),
            "/home/user/some/very/long/recent/project/path".to_string(),
        ],
        0,
    )
    .with_custom_display_text("File");

    let trigger = Layout::intrinsic_size(menu.inner()).expect("dropdown reports intrinsic size");
    assert!(
        trigger.width < menu.inner().content_width(),
        "menu-button trigger ({}) fits its display text, not the widest option ({})",
        trigger.width,
        menu.inner().content_width(),
    );

    // The open popover still expands to the widest option.
    let content = Rect { x: 0.0, y: 0.0, width: trigger.width, height: trigger.height };
    assert!(
        menu.inner().popover_geom(content).2 >= menu.inner().content_width(),
        "popover still fits the widest option",
    );

    // The trigger leaves the paint pass's full budget (8px left pad + 28px right/arrow
    // reservation) for the label, so the display text renders without tripping the right-edge
    // fade. This is the paint condition `start_x + total_advance > right_limit` restated:
    // `content.x + 8 + advance > content.x + width - 28`, i.e. it must hold that
    // `width >= advance + 36`. Measured via `text_advance` — the same function the paint pass
    // lays out with — so the guarantee holds in a monospace UI font too.
    let (font_family, font_size) = crate::layout::control_label_font_detached_parsed();
    let advance = text_advance("File", &font_family, font_size);
    assert!(
        trigger.width >= advance + 36.0,
        "trigger width ({}) leaves room for the laid-out label (advance {} + 36px budget), \
         so it doesn't fade",
        trigger.width,
        advance,
    );
}

/// Migration additions: popover routing through the adapter (`WidgetHost::popover_rect` /
/// `render_popover`), outside-press close, and Escape via routed key events.
#[test]
fn popover_reaches_hosts_through_the_adapter() {
    let mut dummy = crate::context::UiContext::new();
    let options = vec!["A".to_string(), "B".to_string()];
    let mut dd = Dropdown::new(options, 0);
    dd.set_rect(10.0, 10.0, 100.0, 24.0);

    assert!(WidgetHost::popover_rect(&dd).is_none(), "closed dropdown registers no popover");

    dd.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 20.0, &mut dummy);
    assert!(dd.open);
    // popover_rect reports the ANIMATED box — land the expansion first.
    dd.land_anim_for_test();
    let (rx, ry, rw, rh) = WidgetHost::popover_rect(&dd).expect("open dropdown registers its popover");
    // The box is the band, the rows, and the relief wall the plate needs
    // along its one outer edge (the bottom, for a downward menu) — without
    // that reserve the trough painted over the last row's descenders.
    let wall = dd.plate_inset(24.0);
    assert!(wall > 0.0, "the default relief styling carves the open plate");
    assert_eq!((rx, ry), (10.0, 10.0), "the unified surface starts at the trigger band");
    assert!(
        rw >= 100.0 && rh == 24.0 + 2.0 * 24.0 + wall,
        "trigger band (24) + menu (2 * 24) + the bottom wall as one box",
    );
    assert_eq!(
        dd.rows_top(Rect { x: 10.0, y: 10.0, width: 100.0, height: 24.0 }),
        34.0,
        "a downward menu's rows still start flush under the band",
    );
    let trigger = Rect { x: 10.0, y: 10.0, width: 100.0, height: 24.0 };
    assert_eq!(dd.row_at(trigger, 34.0 + 47.9), Some(1), "the last row is whole");
    assert_eq!(dd.row_at(trigger, 34.0 + 48.1), None, "the wall below it picks nothing");

    // An outside press closes it (ungated presses — `gates_presses` is false).
    let closed = dd.mouse_input(MouseButton::Left, ElementState::Pressed, 500.0, 500.0, &mut dummy);
    assert!(closed);
    assert!(dd.closing, "outside press starts the animated contraction");
    dd.land_anim_for_test();
    assert!(!dd.open);
    assert!(!dd.take_change(), "outside close does not report a change");
}

/// `with_menu_replaces_trigger`: the open surface is the rows alone, sat
/// in the trigger's slot — no band, and the trigger stops painting once
/// the menu covers it.
#[test]
fn menu_replaces_trigger_drops_the_band() {
    let mut dummy = crate::context::UiContext::new();
    let options = vec!["A".to_string(), "B".to_string(), "C".to_string()];
    let mut dd = Dropdown::new(options, 1)
        .with_open_upward(true)
        .with_menu_replaces_trigger(true);
    dd.set_rect(10.0, 300.0, 100.0, 24.0);

    dd.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 310.0, &mut dummy);
    assert!(dd.open);
    dd.land_anim_for_test();
    let (rx, ry, rw, rh) = WidgetHost::popover_rect(&dd).expect("open dropdown registers its popover");
    // No band, so BOTH edges are outer ones and both carry the wall.
    let wall = dd.plate_inset(24.0);
    assert!(
        (rh - (3.0 * 24.0 + 2.0 * wall)).abs() < 0.01,
        "three rows and nothing else — no trigger band: {rh}",
    );
    assert_eq!((rx, ry + rh), (10.0, 324.0), "the menu's bottom edge sits on the trigger's bottom edge");
    assert_eq!(
        dd.rows_top(Rect { x: 10.0, y: 300.0, width: 100.0, height: 24.0 }),
        ry + wall,
        "the rows start inside the wall",
    );
    assert!(rw >= 100.0);
    assert_eq!(dd.get_popover_geom(), (rx, ry, rw, rh), "the drawn box is the full menu once landed");

    // The trigger's slot is now the bottom row: a press there picks it,
    // rather than toggling the trigger closed.
    dd.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 312.0, &mut dummy);
    assert_eq!(dd.selected, 2);
    assert!(dd.take_change());
    assert!(dd.closing);
    dd.land_anim_for_test();
    assert!(!dd.open);

    // The default keeps the band.
    let mut plain = Dropdown::new(vec!["A".to_string(), "B".to_string()], 0).with_open_upward(true);
    plain.set_rect(10.0, 300.0, 100.0, 24.0);
    plain.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 310.0, &mut dummy);
    plain.land_anim_for_test();
    let (_, ry, _, rh) = WidgetHost::popover_rect(&plain).unwrap();
    // Upward: the menu's TOP is the outer edge, so the wall goes there and
    // the rows still end flush on the band.
    let wall = plain.plate_inset(24.0);
    assert_eq!(
        (ry, rh),
        (252.0 - wall, 24.0 + 2.0 * 24.0 + wall),
        "band (24) + two rows (48) + the top wall, stacked above the trigger",
    );
    assert_eq!(
        plain.rows_top(Rect { x: 10.0, y: 300.0, width: 100.0, height: 24.0 }),
        252.0,
        "the rows end flush on the band",
    );
}

/// A trigger nested at the right end of a wider field (the textpick
/// picker, through `popover_anchor`) opens as a plate its own width,
/// under itself, and widens out to the field — both edges travelling. An
/// unanchored trigger still grows rightward from its own left edge.
#[test]
fn a_nested_trigger_grows_its_menu_out_of_its_own_span() {
    let trigger = Rect { x: 170.0, y: 10.0, width: 30.0, height: 24.0 };
    let field = Rect { x: 10.0, y: 10.0, width: 190.0, height: 24.0 };
    let mut dd = Dropdown::new(vec!["A".to_string(), "B".to_string()], 0).with_open_upward(false);
    dd.popover_anchor = Some(field);

    dd.anim_snap = 0.0;
    let (x, _, w, h) = dd.popover_geom_drawn(trigger);
    assert_eq!((x, w, h), (170.0, 30.0, 0.0), "it begins as the button");
    let (ux, _, uw, _) = dd.unified_geom_drawn(trigger);
    assert_eq!((ux, uw), (170.0, 30.0), "the open surface is the button alone at first");

    dd.anim_snap = 0.5;
    let (x, _, w, _) = dd.popover_geom_drawn(trigger);
    assert!(x > 10.0 && x < 170.0, "the left edge travels from the button toward the field's: {x}");
    assert_eq!(x + w, 200.0, "the right edge stays where the button's is");

    dd.anim_snap = 1.0;
    let (x, _, w, _) = dd.popover_geom_drawn(trigger);
    assert_eq!((x, w), (10.0, 190.0), "it lands spanning the field");

    let plain = {
        let mut d = Dropdown::new(vec!["A".to_string(), "B".to_string()], 0).with_open_upward(false);
        d.anim_snap = 0.5;
        d
    };
    let wide = Rect { x: 10.0, y: 10.0, width: 100.0, height: 24.0 };
    let (x, _, w, _) = plain.popover_geom_drawn(wide);
    assert_eq!(x, 10.0, "an unanchored menu keeps its left edge");
    assert!(w >= 100.0, "and grows rightward out of the trigger");
}
