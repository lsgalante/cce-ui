use crate::widget::*;

fn serialize_single_widget(w: &dyn WidgetHost, json: &mut String) {
    let (x, y, width, height) = w.rect();
    let label = w.label().or_else(|| w.base().label.clone()).unwrap_or_default();
    let focused = w.base().focused;
    let hovered = w.base().hovered;
    // Concrete value lookup (6bd value shrink — `value` left `WidgetHost`): the
    // `Input::value` implementors a serialized roster can hold are these five widgets;
    // everything else always reported the default 0.
    let a = w.as_any();
    let value = a
        .downcast_ref::<Checkbox>()
        .map(|x| Input::value(x))
        .or_else(|| a.downcast_ref::<Dropdown>().map(|x| Input::value(x)))
        .or_else(|| a.downcast_ref::<Slider>().map(|x| Input::value(x)))
        .or_else(|| a.downcast_ref::<RangeSlider>().map(|x| Input::value(x)))
        .or_else(|| a.downcast_ref::<Spinbox>().map(|x| Input::value(x)))
        .unwrap_or(0);
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
    // roster can hold are Adapted<MenuBar> and Adapted<Paginator> — WidgetHost's discovery
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

/// Serialize the visible widgets' menu state. Takes dyn refs (not boxes): the designer's
/// roster is concretely typed since the Phase 6bb retype and lends a per-slot dyn view.
pub fn serialize_widgets(widgets: &[&dyn WidgetHost]) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The serialized `value` field must keep matching `Input::value` for every
    /// value-bearing widget (the concrete lookup replaced the deleted
    /// `WidgetHost::value` — a new `Input::value` implementor must be added to the
    /// downcast chain in `serialize_single_widget`).
    #[test]
    fn serialized_value_matches_input_value() {
        let mut cb = Checkbox::new();
        assert!(cb.set_value_string("true"));
        let dd = Dropdown::new(vec!["a".into(), "b".into(), "c".into()], 2);
        let mut sl = Slider::new();
        assert!(sl.set_value_string("0.7"));
        let sb = Spinbox::new(7, 0, 10, 1);
        let btn = Button::new(0.0, 0.0, 10.0, 10.0); // no Input::value — always 0

        for (w, expect) in [
            (&cb as &dyn WidgetHost, 1),
            (&dd as &dyn WidgetHost, 2),
            (&sl as &dyn WidgetHost, 70),
            (&sb as &dyn WidgetHost, 7),
            (&btn as &dyn WidgetHost, 0),
        ] {
            let json = serialize_widgets(&[w]);
            assert!(
                json.contains(&format!("\"value\":{expect}")),
                "{} serialized without value {expect}: {json}",
                w.type_name()
            );
        }
    }
}
