use super::*;
use crate::context::UiContext;

/// A labelled tree's fields sit in its content rect, below the label strip — where
/// `paint` draws the well — not at the top of the block the adapter is given.
#[test]
fn a_labelled_trees_fields_are_in_its_content_rect() {
    let mut tree_list = TreeList::new().with_label("TreeList");
    let strip = tree_list.label_strip();
    assert!(strip > 0.0, "a detached label has a strip");
    tree_list.set_rect(10.0, 52.0, 380.0, 200.0 + strip);
    let content_y = 52.0 + strip;
    let (_, sy, _, sh) = tree_list.search_box.here().unwrap().rect();
    assert_eq!(sy, content_y + 6.0, "the search box is inside the well, one margin down");
    let (_, by, _, _) = tree_list.add_key_btn.here().unwrap().rect();
    assert_eq!(by, sy, "the add-key button shares the search row");
    let header_y = content_y + sh + 12.0;
    assert_eq!(tree_list.scroll_box.base.y, header_y + 26.0, "the rows start under the header");
    let labels = tree_list.own_labels();
    let key = labels.iter().find(|(l, _)| l.text == "Key").expect("a Key header");
    assert_eq!(key.0.y, header_y + 6.0, "the header text is in the header band");
    assert_eq!(tree_list.scroll_box.base.y + tree_list.scroll_box.base.h, 52.0 + strip + 200.0, "the rows end at the block's bottom");

    // In a context the fields are its entries, placed in the same places by the
    // embedded hook.
    let held = (tree_list.search_box.here().unwrap().rect(), tree_list.add_key_btn.here().unwrap().rect());
    let mut ctx = UiContext::new();
    let h = ctx.insert(tree_list);
    ctx.lend_h(h, |t, ctx| t.attach_embedded(ctx));
    let t = &ctx[h];
    assert!(t.search_box.is_attached() && t.add_key_btn.is_attached());
    assert_eq!((t.search_box.get(&ctx).rect(), t.add_key_btn.get(&ctx).rect()), held);
}

/// The add-key popover's box is in the window only while the popover is open: closed,
/// it is no Tab stop and no field a screen reader finds; the add-key button opens it,
/// and focuses it.
#[test]
fn the_add_key_box_is_there_only_while_its_popover_is() {
    let mut ctx = UiContext::new();
    let tl = ctx.insert(TreeList::new());
    WidgetHost::set_rect(&mut ctx[tl], 0.0, 0.0, 400.0, 300.0);
    ctx.tick(0.016);
    let box_id = ctx[tl].add_key_popover_box.id();
    let in_tree = |ctx: &UiContext| crate::a11y::tree_update(ctx, "", 1.0).nodes.iter().any(|(n, _)| *n == crate::a11y::node_id(box_id));
    assert!(!ctx.get_widget(box_id).unwrap().visible(), "closed, the box is hidden");
    assert!(!in_tree(&ctx), "and not in the accessibility tree");
    for _ in 0..4 {
        ctx.focus_step(false);
        assert!(!ctx.is_focused_id(box_id), "nor a Tab stop");
    }

    let b = ctx[tl].field_rects().add_key_btn;
    let (x, y) = (b.x + b.width / 2.0, b.y + b.height / 2.0);
    let at = |state| Event::MouseButton { button: MouseButton::Left, state, x, y, local_x: x, local_y: y };
    ctx.propagate_event(&at(ElementState::Pressed), tl.id());
    ctx.propagate_event(&at(ElementState::Released), tl.id());
    assert!(ctx[tl].add_key_popover_open, "the button opens the popover");
    assert!(ctx.get_widget(box_id).unwrap().visible() && ctx.is_focused_id(box_id), "its box shown and focused");
    assert!(in_tree(&ctx));
}

#[test]
fn test_treelist_blocks_window_drag() {
    // root plate container is DELETED: dissolved windows ask `drag_allowed_at` instead — same
    // walk, minus the registered-movable-root plate container requirement.
    let mut ctx = UiContext::new();
    let tree_list = ctx.insert(TreeList::new());
    ctx[tree_list].set_rect(10.0, 52.0, 380.0, 500.0);

    ctx.tick(0.016);
    ctx.clear_dirty();

    assert!(!ctx.drag_allowed_at(100.0, 200.0), "clicking the TreeList must block the window drag");
    assert!(ctx.drag_allowed_at(600.0, 300.0), "empty surface stays draggable");
}

#[test]
fn test_exact_app_layout_blocks_drag() {
    // The data-editor shape: a parentless tree registered directly (dissolved root).
    let mut ctx = UiContext::new();
    let tree_list = ctx.insert(TreeList::new());

    ctx.rebuild_spatial_grid();

    let list_top = 52.0;
    let list_bottom = 600.0 - 180.0;
    ctx[tree_list].set_rect(10.0, list_top, 380.0, list_bottom - list_top);
    ctx.rebuild_spatial_grid();

    assert!(!ctx.drag_allowed_at(100.0, 200.0), "clicking the TreeList under the app layout must block the drag");
}

#[test]
fn test_treelist_separators() {
    let mut tree_list = TreeList::new();
    tree_list.set_rect(10.0, 52.0, 380.0, 500.0);
    tree_list.set_flat_keys(vec![
        ("style.data.tree.corner_radius".to_string(), serde_json::Value::Number(serde_json::Number::from(8)))
    ]);
    
    println!("Flat keys size: {}", tree_list.flat_keys.len());
    println!("Items count: {}", tree_list.items.len());
    for (i, item) in tree_list.items.iter().enumerate() {
        println!("Item {}: {:?}", i, item);
    }
    println!("scroll_box base.x: {}", tree_list.scroll_box.base.x);
    println!("scroll_box viewport_y: {}", tree_list.scroll_box.viewport_y);
    println!("scroll_box viewport_h: {}", tree_list.scroll_box.viewport_h);
    println!("scroll_box scroll_y: {}", tree_list.scroll_box.scroll_y);
    println!("item_height: {}", tree_list.item_height);
    
    let quads = crate::widget::shown_rounded_quads(&tree_list);
    println!("Rounded quads count: {}", quads.len());
    for (i, q) in quads.iter().enumerate() {
        println!("Quad {}: {:?}", i, q);
    }
    assert!(quads.len() > 1, "Should have more than 1 quad!");
}

#[test]
fn test_keybind_label() {
    let mut tree_list = TreeList::new();
    tree_list.set_rect(10.0, 52.0, 380.0, 500.0);
    tree_list.annotations = vec![Some("menu:flat,adaptive".to_string())];
    tree_list.set_flat_keys(vec![
        ("input.accel_profile".to_string(), serde_json::Value::String("flat".to_string()))
    ]);
    
    let labels = tree_list.own_labels();
    for label in &labels {
        println!("TEST LABEL: {:?}", label);
    }
    
    let has_menu_label = labels.iter().any(|(l, _)| l.text == "(menu)");
    assert!(has_menu_label, "Should have (menu) label!");
}

#[test]
fn test_treelist_headers() {
    let tree_list = TreeList::new();
    let labels = tree_list.own_labels();
    assert!(labels.iter().any(|(l, _)| l.text == "Key"), "Should have Key header!");
    assert!(labels.iter().any(|(l, _)| l.text == "Type"), "Should have Type header!");
    assert!(labels.iter().any(|(l, _)| l.text == "Value"), "Should have Value header!");
}

#[test]
fn test_treelist_search_filtering() {
    let mut tree_list = TreeList::new();
    tree_list.set_rect(10.0, 52.0, 380.0, 500.0);
    tree_list.set_flat_keys(vec![
        ("style.data.tree.corner_radius".to_string(), serde_json::Value::Number(serde_json::Number::from(8))),
        ("style.data.tree.border_color".to_string(), serde_json::Value::String("#ff0000".to_string())),
        ("input.accel_profile".to_string(), serde_json::Value::String("flat".to_string())),
    ]);
    
    // Match none
    tree_list.query = "nonexistent".to_string();
    tree_list.rebuild_tree();
    assert!(tree_list.items.is_empty(), "Tree should be empty for nonexistent search query!");

    // Match partially on key path
    tree_list.query = "corner".to_string();
    tree_list.rebuild_tree();
    assert!(!tree_list.items.is_empty(), "Tree should have items matching 'corner'!");
    let has_corner = tree_list.items.iter().any(|item| match item {
        TreeElement::Leaf { name, .. } => name == "corner_radius",
        _ => false,
    });
    assert!(has_corner, "Tree should contain 'corner_radius' item!");
    let has_accel = tree_list.items.iter().any(|item| match item {
        TreeElement::Leaf { name, .. } => name == "accel_profile",
        _ => false,
    });
    assert!(!has_accel, "Tree should not contain 'accel_profile' item!");

    // Match on value
    tree_list.query = "flat".to_string();
    tree_list.rebuild_tree();
    let has_accel = tree_list.items.iter().any(|item| match item {
        TreeElement::Leaf { name, .. } => name == "accel_profile",
        _ => false,
    });
    assert!(has_accel, "Tree should contain 'accel_profile' when matching on value 'flat'!");

    // Collapse matching section when filtered
    tree_list.query = "corner".to_string();
    tree_list.collapsed_sections.insert("style.data.tree".to_string());
    tree_list.rebuild_tree();
    let has_corner = tree_list.items.iter().any(|item| match item {
        TreeElement::Leaf { name, .. } => name == "corner_radius",
        _ => false,
    });
    assert!(!has_corner, "Tree should NOT contain 'corner_radius' item when its parent section 'style.data.tree' is collapsed!");
    
    let has_collapsed_section = tree_list.items.iter().any(|item| match item {
        TreeElement::Section { path, collapsed, .. } => path == "style.data.tree" && *collapsed,
        _ => false,
    });
    assert!(has_collapsed_section, "Tree should contain 'style.data.tree' collapsed section!");
}

#[test]
fn test_treelist_double_click_rename() {
    let mut ctx = UiContext::new();
    let h = ctx.insert(TreeList::new());
    ctx.lend_h(h, |t, ctx| {
        t.set_rect(0.0, 0.0, 380.0, 500.0);
        t.attach_embedded(ctx);
    });
    ctx[h].set_flat_keys(vec![
        ("style.control.dropdown.color".to_string(), serde_json::Value::String("#ff00ff".to_string()))
    ]);
    let double_click = |ctx: &mut UiContext, py: f32| {
        ctx.lend_h(h, |t, ctx| {
            t.mouse_input(MouseButton::Left, ElementState::Pressed, 10.0, py, ctx);
            std::thread::sleep(std::time::Duration::from_millis(10));
            t.mouse_input(MouseButton::Left, ElementState::Pressed, 10.0, py, ctx);
        });
    };
    // The editor is the context's while a rename is up, and given back when it commits.
    let commit = |ctx: &mut UiContext, name: &str| {
        let eb = ctx[h].edit_box.handle().expect("the editor is in the context while it is up");
        let b = &mut ctx[eb];
        b.text = name.to_string();
        b.edit_buffer = name.to_string();
        b.editing = false;
        ctx.lend_h(h, |t, ctx| t.tick(0.016, ctx));
        assert!(ctx.get(eb).is_none(), "the committed editor left the context");
    };

    // 1. Test renaming a section (row 0)
    let list_top = ctx[h].scroll_box.viewport_y;
    double_click(&mut ctx, list_top + 10.0);
    assert!(ctx[h].editing_key_idx.is_some());
    assert_eq!(ctx[h].edit_box.get(&ctx).text, "style"); // Pre-populated with relative name!
    commit(&mut ctx, "theme");
    let req = ctx[h].take_rename_request();
    assert_eq!(req, Some(("style".to_string(), "theme".to_string())));

    // 2. Test renaming a leaf (row 3)
    ctx[h].rebuild_tree();
    let py3 = list_top + 3.0 * ctx[h].item_height + 10.0; // Click row 3 (Leaf "color")
    double_click(&mut ctx, py3);
    assert!(ctx[h].editing_key_idx.is_some());
    assert_eq!(ctx[h].edit_box.get(&ctx).text, "color"); // Pre-populated with relative name "color"!
    commit(&mut ctx, "bg_color");
    let req = ctx[h].take_rename_request();
    assert_eq!(req, Some(("style.control.dropdown.color".to_string(), "style.control.dropdown.bg_color".to_string())));
}
