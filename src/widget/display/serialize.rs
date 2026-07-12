use crate::widget::*;

fn serialize_single_widget(w: &dyn Element, json: &mut String) {
    let (x, y, width, height) = w.rect();
    let label = w.label().or_else(|| w.base().and_then(|b| b.label.clone())).unwrap_or_default();
    let focused = w.base().map_or(false, |b| b.focused);
    let hovered = w.hovered();
    let value = w.value();
    let type_name = w.type_name();

    // Escape JSON label
    let escaped_label = label.replace('\\', "\\\\").replace('"', "\\\"");

    json.push_str(&format!(
        "{{\"type\":\"{}\",\"label\":\"{}\",\"rect\":[{},{},{},{}],\"focused\":{},\"hovered\":{},\"value\":{}",
        type_name, escaped_label, x, y, width, height, focused, hovered, value
    ));

    // Handle children
    let dummy = crate::context::UiContext::new();
    let children = w.children(&dummy);
    let mut menu_items = Vec::new();
    let mut is_menu_open = false;
    let mut is_vertical = false;
    let mut checked_states = Vec::new();
    // Concrete capability lookup (Phase 6aw): the MenuController implementors a serialized
    // roster can hold are Adapted<MenuBar> and Adapted<Paginator> — Element's discovery
    // hooks are gone.
    let mc: Option<&dyn MenuController> = w
        .as_any()
        .downcast_ref::<MenuBar>()
        .map(|m| m as &dyn MenuController)
        .or_else(|| w.as_any().downcast_ref::<Paginator>().map(|p| p as &dyn MenuController));
    if let Some(mc) = mc {
        menu_items = mc.menu_items();
        is_menu_open = mc.is_menu_open();
        is_vertical = mc.is_vertical();
        checked_states = mc.menu_item_checked();
    }

    if type_name == "Menu" && is_menu_open && !menu_items.is_empty() {
        json.push_str(",\"children\":[");
        let mut max_len = 0;
        for item in &menu_items {
            max_len = max_len.max(item.len());
        }
        let dw = (max_len as f32 * 7.5 + 40.0).max(120.0);
        let dx = if is_vertical { x + width } else { x };
        let dy = if is_vertical { y } else { y + height };

        for (i, item) in menu_items.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            let item_y = dy + i as f32 * DROPDOWN_ITEM_H;
            let checked = checked_states.get(i).copied().flatten().unwrap_or(false);
            let item_escaped = item.replace('\\', "\\\\").replace('"', "\\\"");
            json.push_str(&format!(
                "{{\"type\":\"MenuItem\",\"label\":\"{}\",\"rect\":[{},{},{},{}],\"focused\":false,\"hovered\":false,\"value\":{}}}",
                item_escaped, dx, item_y, dw, DROPDOWN_ITEM_H, if checked { 1 } else { 0 }
            ));
        }
        json.push_str("]}");
    } else if !children.is_empty() {
        json.push_str(",\"children\":[");
        let mut first = true;
        for child_ptr in &children {
            unsafe {
                if !(**child_ptr).visible() {
                    continue;
                }
                if !first {
                    json.push(',');
                }
                first = false;
                serialize_single_widget(&**child_ptr, json);
            }
        }
        json.push_str("]}");
    } else {
        json.push('}');
    }
}

pub fn serialize_widgets(widgets: &[Box<dyn Element>]) -> String {
    let mut json = String::new();
    json.push('[');
    let mut first = true;
    for w in widgets {
        if !w.visible() {
            continue;
        }
        if !first {
            json.push(',');
        }
        first = false;
        serialize_single_widget(&**w, &mut json);
    }
    json.push(']');
    json
}
