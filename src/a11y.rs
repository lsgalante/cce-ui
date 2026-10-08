//! The accessibility tree: what a window's widgets are, for screen readers and other
//! assistive technology (`docs/rfc-accessibility-locale.md`, phase 1).
//!
//! The tree is AccessKit's (§ 5 of the RFC): [`tree_update`] turns a [`UiContext`] into an
//! [`accesskit::TreeUpdate`] that a platform adapter publishes — AT-SPI on Wayland, NSAccessibility
//! on macOS — and nothing here knows which. A node per registered, visible widget, under one
//! window node:
//!
//! - **role** — the widget's explicit [`WidgetHost::a11y_role`], else [`role_for`]'s guess from
//!   its type and its [`FocusRole`];
//! - **name** — its label;
//! - **value** — [`WidgetHost::a11y_value`] (a widget's `value_string`): a check box or switch
//!   as toggled, a slider, spin button or progress bar as a number, anything else as text;
//! - **bounds** — its rect in LOGICAL px, the window node carrying the HiDPI scale as its
//!   transform, so no widget's bounds change with the scale;
//! - **actions** — focus for every keyboard stop, click for every [`FocusRole::Plate`];
//! - **focus** — the context's focused widget, else the window.
//!
//! Every call answers the whole tree. AccessKit's adapters compare it with the last one and
//! raise events only for nodes that changed; sending changed nodes alone is an optimisation
//! for later, with `backend::frame`'s damage diff as its model.

use accesskit::{Action, Affine, Node, NodeId, Rect, Role, Toggled, TreeId, TreeInfo, TreeUpdate};

use crate::context::UiContext;
use crate::widget::{FocusRole, WidgetHost, WidgetId};

/// The window's node, the root every widget hangs from.
pub const WINDOW: NodeId = NodeId(0);

/// A widget's node: its id, moved up one so no widget can be the window.
pub fn node_id(id: WidgetId) -> NodeId {
    NodeId(id.0 as u64 + 1)
}

/// What a widget is when it does not say ([`WidgetHost::a11y_role`]): by its type, else by
/// what it is to the keyboard — a thing you press is a button, anything else a container.
pub fn role_for(type_name: &str, focus: FocusRole, explicit: Option<Role>) -> Role {
    if let Some(role) = explicit {
        return role;
    }
    match type_name {
        "Button" => Role::Button,
        "Checkbox" => Role::CheckBox,
        "Toggle" => Role::Switch,
        "Slider" | "RangeSlider" | "Slider2D" => Role::Slider,
        "Spinbox" => Role::SpinButton,
        "TextBox" | "KeybindRecorder" => Role::TextInput,
        "Dropdown" | "FontSelector" => Role::ComboBox,
        "ColorSelector" => Role::ColorWell,
        "ButtonStrip" | "Paginator" => Role::TabList,
        "Breadcrumb" => Role::Navigation,
        "TreeList" => Role::Tree,
        "Spreadsheet" => Role::Table,
        "MenuBar" => Role::MenuBar,
        "Label" | "StyledLabel" | "TextLabel" => Role::Label,
        "ProgressBar" | "UsageBar" => Role::ProgressIndicator,
        "ImageView" => Role::Image,
        "Splitter" => Role::Splitter,
        "Graph" | "Trackpad" => Role::Canvas,
        "Group" | "ParametersBg" | "Panel" => Role::Group,
        _ => match focus {
            FocusRole::Plate => Role::Button,
            FocusRole::Well | FocusRole::None => Role::GenericContainer,
        },
    }
}

/// One widget's node, with `children` already resolved.
pub fn widget_node(w: &dyn WidgetHost, children: Vec<NodeId>) -> Node {
    let focus = w.focus_role();
    let role = role_for(w.type_name(), focus, w.a11y_role());
    let mut node = Node::new(role);
    if let Some(label) = w.label().filter(|l| !l.is_empty()) {
        node.set_label(label);
    }
    if let Some(value) = w.a11y_value() {
        match role {
            Role::CheckBox | Role::Switch => {
                if let Some(on) = truth(&value) {
                    node.set_toggled(Toggled::from(on));
                }
            }
            Role::Slider | Role::SpinButton | Role::ProgressIndicator => match value.trim().parse::<f64>() {
                Ok(n) => node.set_numeric_value(n),
                Err(_) => node.set_value(value),
            },
            _ => node.set_value(value),
        }
    }
    let (x, y, width, height) = w.rect();
    node.set_bounds(Rect::new(x as f64, y as f64, (x + width) as f64, (y + height) as f64));
    if focus != FocusRole::None {
        node.add_action(Action::Focus);
    }
    if focus == FocusRole::Plate {
        node.add_action(Action::Click);
    }
    if !children.is_empty() {
        node.set_children(children);
    }
    node
}

/// A check box's or switch's value string as a state: what `value_string` writes for one.
fn truth(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "on" | "yes" => Some(true),
        "false" | "0" | "off" | "no" => Some(false),
        _ => None,
    }
}

/// The whole tree of `ctx`'s registered, visible widgets under a window node named `title`,
/// at HiDPI `scale` (see the module docs).
pub fn tree_update(ctx: &UiContext, title: &str, scale: f64) -> TreeUpdate {
    // The widgets, read once. Registered pointers name live widgets (the registry resolves
    // only those: `widget::Owned`, `widget::core::Liveness`), and nothing mutates them while
    // this borrows the context.
    let widgets: Vec<(WidgetId, &dyn WidgetHost)> = ctx
        .tree
        .iter_registered()
        .map(|(id, ptr)| (id, unsafe { &*ptr } as &dyn WidgetHost))
        .filter(|(_, w)| w.visible())
        .collect();
    let shown: std::collections::HashSet<WidgetId> = widgets.iter().map(|(id, _)| *id).collect();

    let mut nodes = Vec::with_capacity(widgets.len() + 1);
    let mut top: Vec<(WidgetId, f32, f32)> = Vec::new();
    for (id, w) in &widgets {
        let children: Vec<NodeId> =
            ctx.tree.child_ids(*id).into_iter().filter(|c| shown.contains(c)).map(node_id).collect();
        nodes.push((node_id(*id), widget_node(*w, children)));
        let parented = ctx.tree.parent_id(*id).is_some_and(|p| shown.contains(&p));
        if !parented {
            let (x, y, _, _) = w.rect();
            top.push((*id, y, x));
        }
    }
    // Reading order, as the keyboard walk takes it: rows top to bottom, then left to right.
    top.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.2.total_cmp(&b.2)).then(a.0 .0.cmp(&b.0 .0)));

    let mut window = Node::new(Role::Window);
    if !title.is_empty() {
        window.set_label(title);
    }
    window.set_transform(Affine::scale(scale));
    window.set_children(top.iter().map(|(id, _, _)| node_id(*id)).collect::<Vec<_>>());
    nodes.push((WINDOW, window));

    let focus = ctx.focused_widget.filter(|id| shown.contains(id)).map(node_id).unwrap_or(WINDOW);
    let mut tree = TreeInfo::new(WINDOW);
    tree.toolkit_name = Some("cce-ui".into());
    tree.toolkit_version = Some(env!("CARGO_PKG_VERSION").into());
    TreeUpdate { nodes, tree: Some(tree), tree_id: TreeId::ROOT, focus }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::{Button, Checkbox, Owned, Slider, TextBox};

    fn node<'a>(update: &'a TreeUpdate, id: NodeId) -> &'a Node {
        &update.nodes.iter().find(|(n, _)| *n == id).expect("node in the update").1
    }

    #[test]
    fn a_window_of_widgets_is_a_tree_of_what_they_are() {
        let mut ctx = UiContext::new();
        let mut save = Owned::new(Button::new(10.0, 40.0, 80.0, 24.0).with_label("Save"));
        let mut name = Owned::new(TextBox::new("Ada".to_string()).with_label("Name"));
        name.set_rect(10.0, 10.0, 200.0, 24.0);
        let mut wrap = Owned::new(Checkbox::new().with_label("Wrap lines"));
        wrap.set_rect(10.0, 70.0, 200.0, 24.0);
        wrap.set_value_string("true");
        let mut zoom = Owned::new(Slider::new().with_label("Zoom"));
        zoom.set_rect(10.0, 100.0, 200.0, 24.0);
        ctx.register_host(&mut save);
        ctx.register_host(&mut name);
        ctx.register_host(&mut wrap);
        ctx.register_host(&mut zoom);
        ctx.set_focused(&mut *name);

        let update = tree_update(&ctx, "Editor", 2.0);
        let window = node(&update, WINDOW);
        assert_eq!(window.role(), Role::Window);
        assert_eq!(window.label(), Some("Editor"));
        assert_eq!(window.transform(), Some(&Affine::scale(2.0)), "the scale is the window's");
        let order: Vec<NodeId> = [&*name as &dyn WidgetHost, &*save, &*wrap, &*zoom]
            .iter()
            .map(|w| node_id(w.base().id()))
            .collect();
        assert_eq!(window.children(), &order[..], "reading order: top to bottom");
        assert_eq!(update.tree.as_ref().map(|t| t.root), Some(WINDOW));
        assert_eq!(update.focus, node_id(name.base().id()), "focus follows the context");

        let b = node(&update, node_id(save.base().id()));
        assert_eq!((b.role(), b.label()), (Role::Button, Some("Save")));
        assert!(b.supports_action(Action::Click) && b.supports_action(Action::Focus));
        assert_eq!(b.bounds(), Some(Rect::new(10.0, 40.0, 90.0, 64.0)), "logical px");

        let t = node(&update, node_id(name.base().id()));
        assert_eq!((t.role(), t.label(), t.value()), (Role::TextInput, Some("Name"), Some("Ada")));
        assert!(!t.supports_action(Action::Click), "a well is not pressed");

        let c = node(&update, node_id(wrap.base().id()));
        assert_eq!((c.role(), c.toggled()), (Role::CheckBox, Some(Toggled::True)));

        let s = node(&update, node_id(zoom.base().id()));
        assert_eq!(s.role(), Role::Slider);
        assert!(s.numeric_value().is_some(), "a slider's value is a number");
    }

    #[test]
    fn hidden_widgets_are_not_in_the_tree_and_focus_falls_back_to_the_window() {
        let mut ctx = UiContext::new();
        let mut hidden = Owned::new(Button::new(0.0, 0.0, 10.0, 10.0).with_label("Ghost"));
        hidden.set_visible(false);
        ctx.register_host(&mut hidden);
        ctx.set_focused(&mut *hidden);
        let update = tree_update(&ctx, "", 1.0);
        assert_eq!(update.nodes.len(), 1, "only the window");
        assert_eq!(update.focus, WINDOW);
        assert_eq!(node(&update, WINDOW).label(), None, "no title, no name");
    }

    #[test]
    fn a_widget_that_says_what_it_is_is_believed() {
        assert_eq!(role_for("Button", FocusRole::Plate, Some(Role::Tab)), Role::Tab);
        assert_eq!(role_for("SomethingNew", FocusRole::Plate, None), Role::Button);
        assert_eq!(role_for("SomethingNew", FocusRole::None, None), Role::GenericContainer);
    }
}
