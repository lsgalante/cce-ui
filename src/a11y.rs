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
//! An open context menu (`widget::context_menu`, one per thread, whatever app shows it) is a
//! [`Role::Menu`] under the window, its rows items: a `✓` row a checkable item, a `●` / `○`
//! row a radio item (toggled as marked), a slider row a slider with its number, a header a
//! label; `-` separators are left out. While it is open the keyboard is in it, so the focus
//! is its highlighted row, else the menu.
//!
//! An app that draws without a [`UiContext`] (the status bar, the terminal, the map…) says
//! what it shows through [`Application::accessibility`](crate::backend::app::Application::accessibility),
//! pushing AccessKit nodes into [`AppNodes`]; [`app_tree`] puts both halves and the menu
//! under one window.
//!
//! Every call answers the whole tree. AccessKit's adapters compare it with the last one and
//! raise events only for nodes that changed; sending changed nodes alone is an optimisation
//! for later, with `backend::frame`'s damage diff as its model.

use accesskit::{Action, Affine, Node, NodeId, Rect, Role, Toggled, TreeId, TreeInfo, TreeUpdate};

use crate::context::UiContext;
use crate::widget::{FocusRole, NamedKey, WidgetHost, WidgetId};

/// The window's node, the root every widget hangs from.
pub const WINDOW: NodeId = NodeId(0);

/// The widget a node is, when it is a widget's ([`node_id`]'s inverse).
pub fn widget_of(node: NodeId) -> Option<WidgetId> {
    (node.0 > 0 && node.0 < ITEM_BASE).then(|| WidgetId((node.0 - 1) as usize))
}

/// A part of a widget a screen reader sees as a node of its own, under the widget's node:
/// a radio group's radio buttons (`Input::a11y_items`). A click on it is
/// `Input::a11y_select_item`.
#[derive(Debug, Clone, PartialEq)]
pub struct A11yItem {
    pub role: Role,
    pub label: String,
    /// Where it is, in window px.
    pub rect: crate::scene::layout::Rect,
    /// Its checked state, for a radio button or a check item.
    pub toggled: Option<bool>,
    /// Whether the keyboard is on it: the widget's focus, given to this item.
    pub focused: bool,
}

/// Where widgets' items begin: `ITEM_BASE + (widget id << 16) + index`, below the apps' own
/// nodes and above every widget's.
const ITEM_BASE: u64 = 1 << 60;

/// The node of item `idx` of widget `id` ([`A11yItem`]).
pub fn item_id(id: WidgetId, idx: usize) -> NodeId {
    NodeId(ITEM_BASE + ((id.0 as u64) << 16) + (idx as u64 & 0xffff))
}

/// The widget and item a node is, when it is an item's ([`item_id`]'s inverse).
pub fn item_of(node: NodeId) -> Option<(WidgetId, usize)> {
    (node.0 >= ITEM_BASE && node.0 < APP_BASE)
        .then(|| (WidgetId(((node.0 - ITEM_BASE) >> 16) as usize), ((node.0 - ITEM_BASE) & 0xffff) as usize))
}

/// The context-menu row a node is, when it is one ([`menu_row_id`]'s inverse).
pub fn menu_row_of(node: NodeId) -> Option<usize> {
    (node.0 > MENU.0).then(|| (node.0 - MENU.0 - 1) as usize)
}

/// The open context menu's node, and the first of its rows' (`MENU + 1 + row`): far above any
/// widget's id.
pub const MENU: NodeId = NodeId(1 << 62);

/// The node of the open context menu's row `row`.
pub fn menu_row_id(row: usize) -> NodeId {
    NodeId(MENU.0 + 1 + row as u64)
}

/// Where an app's own nodes ([`AppNodes::id`]) begin: above every widget's, below the menu's.
const APP_BASE: u64 = 1 << 61;

/// The nodes an app declares itself, for what it draws without a [`UiContext`] — see
/// [`Application::accessibility`](crate::backend::app::Application::accessibility). Ids come
/// from [`AppNodes::id`], an app's own numbering; children are set on a node with
/// `Node::set_children`, and only the nodes pushed with [`push_top`](Self::push_top) hang
/// from the window.
#[derive(Default)]
pub struct AppNodes {
    nodes: Vec<(NodeId, Node)>,
    top: Vec<NodeId>,
    focus: Option<NodeId>,
}

impl AppNodes {
    /// The node id of the app's own node `n` (any number below 2^61).
    pub fn id(n: u64) -> NodeId {
        NodeId(APP_BASE + (n & (APP_BASE - 1)))
    }

    /// A node that hangs from another of the app's nodes (which lists it as a child).
    pub fn push(&mut self, id: NodeId, node: Node) {
        self.nodes.push((id, node));
    }

    /// A node directly under the window, in the order pushed (after the widgets').
    pub fn push_top(&mut self, id: NodeId, node: Node) {
        self.top.push(id);
        self.nodes.push((id, node));
    }

    /// The node the keyboard is on, when it is one of the app's own.
    pub fn set_focus(&mut self, id: NodeId) {
        self.focus = Some(id);
    }
}

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
        "Group" | "ParametersBg" => Role::Group,
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
    let name = w.base().accessible_name.clone().filter(|n| !n.is_empty());
    if let Some(label) = name.or_else(|| w.label().filter(|l| !l.is_empty())) {
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
    if let Some((min, max, step)) = w.a11y_range() {
        node.set_min_numeric_value(min);
        node.set_max_numeric_value(max);
        if step > 0.0 {
            node.set_numeric_value_step(step);
        }
        node.add_action(Action::SetValue);
    }
    for action in [Action::Click, Action::Increment, Action::Decrement] {
        if key_for(w, action).is_some() {
            node.add_action(action);
        }
    }
    if !children.is_empty() {
        node.set_children(children);
    }
    node
}

/// The key a keyboard user presses on the focused widget to do `action`, which is how the
/// Linux adapter carries it out (`backend::a11y_unix::act`): Space clicks a plate (what
/// arms on `FocusIn`); Right / Left step a slider or a range's focused end, Up / Down a
/// spin button. `None`: the widget does not take it. The node advertises exactly the
/// actions this answers, so the tree never offers one the runner cannot perform.
pub fn key_for(w: &dyn WidgetHost, action: Action) -> Option<NamedKey> {
    let role = role_for(w.type_name(), w.focus_role(), w.a11y_role());
    match (action, role) {
        // A radio group is clicked through its radio buttons (`A11yItem`), not as a whole.
        (Action::Click, Role::RadioGroup) => None,
        (Action::Click, _) if w.focus_role() == FocusRole::Plate => Some(NamedKey::Space),
        (Action::Increment, Role::Slider) if w.type_name() != "Slider2D" => Some(NamedKey::ArrowRight),
        (Action::Decrement, Role::Slider) if w.type_name() != "Slider2D" => Some(NamedKey::ArrowLeft),
        (Action::Increment, Role::SpinButton) => Some(NamedKey::ArrowUp),
        (Action::Decrement, Role::SpinButton) => Some(NamedKey::ArrowDown),
        _ => None,
    }
}

/// Whether a widget is on screen: visible, with a size, and not parked far off the window
/// (x or y below -9000). A widget an app is not showing should be HIDDEN
/// (`WidgetHost::set_visible(false)`), which takes it out of the Tab walk too; the sentinel
/// is a backstop for one only parked. cce-data-editor parked its per-type value editors at
/// -1000, above the sentinel, so all of them were in the tree until it hid them (2026-10-08).
fn shown_on_screen(w: &dyn WidgetHost) -> bool {
    let (x, y, width, height) = w.rect();
    w.visible() && width > 0.0 && height > 0.0 && x > -9000.0 && y > -9000.0
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
    window_tree(Some(ctx), AppNodes::default(), title, scale)
}

/// An app's whole tree: its [`UiContext`]'s widgets if it has one, the nodes its
/// [`Application::accessibility`](crate::backend::app::Application::accessibility) declares,
/// and an open context menu, under a window named by its settings' title.
pub fn app_tree<A: crate::backend::app::Application>(app: &mut A, scale: f64) -> TreeUpdate {
    let mut own = AppNodes::default();
    app.accessibility(&mut own);
    let title = app.settings().title;
    window_tree(app.ui_context(), own, &title, scale)
}

/// The tree of `ctx`'s widgets (if any) and `app`'s own nodes under one window. Focus, most
/// specific first: an open menu's, else the app's own, else the context's, else the window.
pub fn window_tree(ctx: Option<&UiContext>, app: AppNodes, title: &str, scale: f64) -> TreeUpdate {
    let empty;
    let ctx = match ctx {
        Some(ctx) => ctx,
        None => {
            empty = UiContext::new();
            &empty
        }
    };
    // The widgets, read once. Registered pointers name live widgets (the registry resolves
    // only those: `widget::Owned`, `widget::core::Liveness`), and nothing mutates them while
    // this borrows the context.
    let widgets: Vec<(WidgetId, &dyn WidgetHost)> = ctx
        .tree
        .iter_registered()
        .map(|(id, ptr)| (id, unsafe { &*ptr } as &dyn WidgetHost))
        .filter(|(_, w)| shown_on_screen(*w))
        .collect();
    let shown: std::collections::HashSet<WidgetId> = widgets.iter().map(|(id, _)| *id).collect();

    let mut nodes = Vec::with_capacity(widgets.len() + 1);
    let mut top: Vec<(WidgetId, f32, f32)> = Vec::new();
    // The keyboard's node: a focused widget's, or its focused item's.
    let mut item_focus: Option<NodeId> = None;
    for (id, w) in &widgets {
        // The widget's own items come first, then its linked children.
        let mut children: Vec<NodeId> = Vec::new();
        for (i, item) in w.a11y_items().into_iter().enumerate() {
            let nid = item_id(*id, i);
            let mut node = Node::new(item.role);
            node.set_label(item.label);
            let r = item.rect;
            node.set_bounds(Rect::new(r.x as f64, r.y as f64, (r.x + r.width) as f64, (r.y + r.height) as f64));
            if let Some(on) = item.toggled {
                node.set_toggled(Toggled::from(on));
            }
            node.add_action(Action::Click);
            if item.focused && ctx.focused_widget == Some(*id) {
                item_focus = Some(nid);
            }
            nodes.push((nid, node));
            children.push(nid);
        }
        children.extend(ctx.tree.child_ids(*id).into_iter().filter(|c| shown.contains(c)).map(node_id));
        let mut node = widget_node(*w, children);
        // The open modal is said to be one: a reader keeps to it.
        if ctx.modal_owner() == Some(*id) {
            node.set_modal();
        }
        nodes.push((node_id(*id), node));
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
    let mut children: Vec<NodeId> = top.iter().map(|(id, _, _)| node_id(*id)).collect();
    children.extend(app.top.iter().copied());
    nodes.extend(app.nodes);
    if crate::widget::context_menu::is_visible() {
        children.push(MENU);
    }
    window.set_children(children);
    nodes.push((WINDOW, window));

    let mut focus = app
        .focus
        .or(item_focus)
        .or_else(|| ctx.focused_widget.filter(|id| shown.contains(id)).map(node_id))
        .unwrap_or(WINDOW);
    if let Some(menu_focus) = push_context_menu(&mut nodes) {
        focus = menu_focus;
    }
    let mut tree = TreeInfo::new(WINDOW);
    tree.toolkit_name = Some("cce-ui".into());
    tree.toolkit_version = Some(env!("CARGO_PKG_VERSION").into());
    TreeUpdate { nodes, tree: Some(tree), tree_id: TreeId::ROOT, focus }
}

/// The open context menu and its rows, pushed onto `nodes`; the node the keyboard is on
/// while it is open, or `None` when no menu is.
fn push_context_menu(nodes: &mut Vec<(NodeId, Node)>) -> Option<NodeId> {
    use crate::widget::context_menu as cm;
    if !cm::is_visible() {
        return None;
    }
    let (mx, my, mw, mh) = (cm::x() as f64, cm::y() as f64, cm::w() as f64, cm::h() as f64);
    let headers = cm::header_count();
    let mut menu = Node::new(Role::Menu);
    menu.set_bounds(Rect::new(mx, my, mx + mw, my + mh));
    let mut rows = Vec::new();
    for (i, label) in cm::options().iter().enumerate() {
        if label.trim() == "-" {
            continue;
        }
        // `split_mark` answers the glyph a mark is drawn as: "check" for `✓`, "circle" /
        // "circle-outline" for a radio row on / off.
        let (mark, text) = cm::split_mark(label);
        let role = if i < headers {
            Role::Label
        } else if cm::slider(i).is_some() {
            Role::Slider
        } else {
            match mark {
                Some("check") => Role::MenuItemCheckBox,
                Some("circle") | Some("circle-outline") => Role::MenuItemRadio,
                _ => Role::MenuItem,
            }
        };
        let mut row = Node::new(role);
        row.set_label(text.trim());
        let y = cm::row_y(i) as f64;
        row.set_bounds(Rect::new(mx, y, mx + mw, y + cm::ROW_H as f64));
        match (role, mark) {
            (Role::MenuItemCheckBox, _) => row.set_toggled(Toggled::True),
            (Role::MenuItemRadio, Some(glyph)) => row.set_toggled(Toggled::from(glyph == "circle")),
            _ => {}
        }
        if let Some(slider) = cm::slider(i) {
            row.set_numeric_value(slider.value as f64);
            row.set_min_numeric_value(slider.min as f64);
            row.set_max_numeric_value(slider.max as f64);
            row.set_numeric_value_step(slider.step as f64);
        }
        if i >= headers {
            row.add_action(Action::Click);
        }
        rows.push(menu_row_id(i));
        nodes.push((menu_row_id(i), row));
    }
    menu.set_children(rows);
    nodes.push((MENU, menu));
    Some(cm::hovered_item().filter(|&i| i >= headers).map(menu_row_id).unwrap_or(MENU))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::{Button, Checkbox, Owned, RangeSlider, Slider, Spinbox, TextBox};

    /// An open dialog is a modal `Dialog` node holding its members; a radio group in it is a
    /// `RadioGroup` of `RadioButton` items, the chosen one checked and, with the group
    /// focused, the keyboard's node.
    #[test]
    fn a_dialog_is_modal_and_a_radio_group_is_its_radio_buttons() {
        use crate::widget::{Dialog, RadioGroup};
        let mut ctx = UiContext::new();
        let mut size = Owned::new(RadioGroup::new(["Small", "Medium", "Large"]).with_selected(1));
        size.set_rect(120.0, 120.0, 200.0, 100.0);
        let mut ok = Owned::new(Button::new(120.0, 240.0, 80.0, 24.0).with_label("OK"));
        let mut dialog = Owned::new(Dialog::new().with_label("Size"));
        ctx.register_host(&mut dialog);
        ctx.register_host(&mut size);
        ctx.register_host(&mut ok);
        dialog.open(&mut ctx, vec![size.base().id(), ok.base().id()]);

        let update = tree_update(&ctx, "App", 1.0);
        let d = node(&update, node_id(dialog.base().id()));
        assert_eq!((d.role(), d.label(), d.is_modal()), (Role::Dialog, Some("Size"), true));
        assert_eq!(d.children(), &[node_id(size.base().id()), node_id(ok.base().id())]);
        assert_eq!(node(&update, WINDOW).children(), &[node_id(dialog.base().id())], "the members hang from it");

        let g = node(&update, node_id(size.base().id()));
        assert_eq!(g.role(), Role::RadioGroup);
        assert!(!g.supports_action(Action::Click), "clicked through its buttons");
        let buttons: Vec<(Option<&str>, Option<Toggled>)> = g
            .children()
            .iter()
            .map(|c| node(&update, *c))
            .inspect(|b| assert!(b.role() == Role::RadioButton && b.supports_action(Action::Click)))
            .map(|b| (b.label(), b.toggled()))
            .collect();
        assert_eq!(
            buttons,
            [(Some("Small"), Some(Toggled::False)), (Some("Medium"), Some(Toggled::True)), (Some("Large"), Some(Toggled::False))]
        );
        assert_eq!(update.focus, item_id(size.base().id(), 1), "the dialog's first stop, on its chosen button");
        assert_eq!(item_of(item_id(size.base().id(), 2)), Some((size.base().id(), 2)));
        assert_eq!(widget_of(item_id(size.base().id(), 2)), None, "an item is not a widget");
    }

    /// A node offers the actions the Linux adapter can carry out, and each by the key a
    /// keyboard user presses: Space on a plate, Right / Left on a slider or a range,
    /// Up / Down on a spin button, nothing on a text box.
    /// A control named without a label of its own (a value editor in a table row) is called
    /// by its accessible name, which wins over a label and adds no label strip.
    #[test]
    fn an_accessible_name_names_a_widget_and_draws_nothing() {
        let mut text = TextBox::new(String::new());
        let strip = text.label_strip();
        text.set_accessible_name(Some("Value of font_size"));
        assert_eq!(widget_node(&text, Vec::new()).label(), Some("Value of font_size"));
        assert_eq!(text.label_strip(), strip, "no label strip for a name nobody draws");
        let mut save = Button::new(0.0, 0.0, 80.0, 24.0).with_label("Save");
        save.set_accessible_name(Some("Save the file"));
        assert_eq!(widget_node(&save, Vec::new()).label(), Some("Save the file"));
        save.set_accessible_name(None);
        assert_eq!(widget_node(&save, Vec::new()).label(), Some("Save"), "back to the label");
    }

    #[test]
    fn a_node_offers_the_actions_its_keys_carry_out() {
        let button = Button::new(0.0, 0.0, 80.0, 24.0).with_label("Save");
        let slider = Slider::new().with_label("Zoom");
        let range = RangeSlider::new();
        let spin = Spinbox::new(0, 0, 10, 1);
        let text = TextBox::new(String::new());
        let check = Checkbox::new();
        let offers = |w: &dyn WidgetHost| {
            let node = widget_node(w, Vec::new());
            [Action::Click, Action::Increment, Action::Decrement]
                .into_iter()
                .filter(|a| node.supports_action(*a))
                .map(|a| (a, key_for(w, a).expect("an offered action has its key")))
                .collect::<Vec<_>>()
        };
        assert_eq!(offers(&button), [(Action::Click, NamedKey::Space)]);
        assert_eq!(offers(&check), [(Action::Click, NamedKey::Space)]);
        assert_eq!(offers(&slider), [(Action::Increment, NamedKey::ArrowRight), (Action::Decrement, NamedKey::ArrowLeft)]);
        assert_eq!(offers(&range), [(Action::Increment, NamedKey::ArrowRight), (Action::Decrement, NamedKey::ArrowLeft)]);
        assert_eq!(offers(&spin), [(Action::Increment, NamedKey::ArrowUp), (Action::Decrement, NamedKey::ArrowDown)]);
        assert_eq!(offers(&text), [], "a well is typed into, not pressed or stepped");

        let spin_node = widget_node(&spin, Vec::new());
        assert!(spin_node.supports_action(Action::SetValue), "a reader sets a spin button");
        assert_eq!((spin_node.min_numeric_value(), spin_node.max_numeric_value()), (Some(0.0), Some(10.0)));
        assert_eq!(spin_node.numeric_value_step(), Some(1.0));
        assert!(!widget_node(&button, Vec::new()).supports_action(Action::SetValue));
        assert_eq!(key_for(&text, Action::Click), None);
    }

    fn node(update: &TreeUpdate, id: NodeId) -> &Node {
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
    fn an_open_context_menu_is_a_menu_and_holds_the_keyboard() {
        use crate::widget::context_menu as cm;
        let ctx = UiContext::new();
        let rows = vec![
            "[TextBox]: Name".to_string(),
            "✓ Wrap lines".to_string(),
            "● Follow editor".to_string(),
            "-".to_string(),
            "Opacity".to_string(),
            "Copy".to_string(),
        ];
        cm::show(40.0, 50.0, rows, 1, WidgetId(7));
        cm::set_row_slider(4, cm::MenuSlider { value: 50.0, min: 0.0, max: 100.0, step: 5.0, decimals: 0, suffix: "%" });
        let update = tree_update(&ctx, "", 1.0);
        assert!(node(&update, WINDOW).children().contains(&MENU), "the menu hangs from the window");
        let menu = node(&update, MENU);
        assert_eq!(menu.role(), Role::Menu);
        assert_eq!(menu.children(), &[menu_row_id(0), menu_row_id(1), menu_row_id(2), menu_row_id(4), menu_row_id(5)][..], "no separator");
        let row = |i| node(&update, menu_row_id(i));
        assert_eq!((row(0).role(), row(0).label()), (Role::Label, Some("[TextBox]: Name")));
        assert_eq!((row(1).role(), row(1).label(), row(1).toggled()), (Role::MenuItemCheckBox, Some("Wrap lines"), Some(Toggled::True)));
        assert_eq!((row(2).role(), row(2).toggled()), (Role::MenuItemRadio, Some(Toggled::True)));
        assert_eq!((row(4).role(), row(4).numeric_value(), row(4).max_numeric_value()), (Role::Slider, Some(50.0), Some(100.0)));
        assert_eq!(row(5).role(), Role::MenuItem);
        assert!(row(5).supports_action(Action::Click) && !row(0).supports_action(Action::Click));
        assert_eq!(update.focus, MENU, "nothing highlighted: the menu has the keyboard");
        cm::set_hovered_item(Some(5));
        assert_eq!(tree_update(&ctx, "", 1.0).focus, menu_row_id(5), "the highlight is the focus");
        cm::hide();
        let closed = tree_update(&ctx, "", 1.0);
        assert!(!node(&closed, WINDOW).children().contains(&MENU) && closed.focus == WINDOW);
    }

    #[test]
    fn an_app_without_widgets_declares_its_own_nodes() {
        // A status-bar module: no UiContext, a clock it draws itself.
        let mut own = AppNodes::default();
        let (bar, clock, button) = (AppNodes::id(1), AppNodes::id(2), AppNodes::id(3));
        let mut group = Node::new(Role::Group);
        group.set_label("Status");
        group.set_children(vec![clock, button]);
        own.push_top(bar, group);
        let mut label = Node::new(Role::Label);
        label.set_value("12:30");
        own.push(clock, label);
        let mut b = Node::new(Role::Button);
        b.set_label("Volume");
        b.add_action(Action::Click);
        own.push(button, b);
        own.set_focus(button);

        let update = window_tree(None, own, "Status bar", 1.0);
        assert_eq!(node(&update, WINDOW).children(), &[bar][..], "only what was pushed to the top");
        assert_eq!(node(&update, bar).children(), &[clock, button][..]);
        assert_eq!(node(&update, clock).value(), Some("12:30"));
        assert_eq!(update.focus, button, "the app's focus");
        assert!(bar != WINDOW && bar != MENU && bar.0 > node_id(WidgetId(usize::MAX >> 4)).0, "its own id range");
    }

    #[test]
    fn parked_and_sizeless_widgets_are_not_on_screen() {
        let mut ctx = UiContext::new();
        let mut parked = Owned::new(Button::new(-10_000.0, 0.0, 80.0, 24.0).with_label("Parked"));
        let mut empty = Owned::new(Button::new(10.0, 10.0, 0.0, 0.0).with_label("Empty"));
        let mut shown = Owned::new(Button::new(10.0, 10.0, 80.0, 24.0).with_label("Shown"));
        ctx.register_host(&mut parked);
        ctx.register_host(&mut empty);
        ctx.register_host(&mut shown);
        let update = tree_update(&ctx, "", 1.0);
        assert_eq!(node(&update, WINDOW).children(), &[node_id(shown.base().id())][..]);
    }

    #[test]
    fn a_node_id_names_back_what_it_was_made_from() {
        for id in [WidgetId(0), WidgetId(1), WidgetId(123_456)] {
            assert_eq!(widget_of(node_id(id)), Some(id));
        }
        for row in [0, 1, 40] {
            assert_eq!(menu_row_of(menu_row_id(row)), Some(row));
            assert_eq!(widget_of(menu_row_id(row)), None, "a menu row is no widget");
        }
        assert_eq!(widget_of(WINDOW), None);
        assert_eq!((widget_of(MENU), menu_row_of(MENU)), (None, None));
        assert_eq!((widget_of(AppNodes::id(5)), menu_row_of(AppNodes::id(5))), (None, None), "an app's own node is neither");
    }

    #[test]
    fn a_widget_that_says_what_it_is_is_believed() {
        assert_eq!(role_for("Button", FocusRole::Plate, Some(Role::Tab)), Role::Tab);
        assert_eq!(role_for("SomethingNew", FocusRole::Plate, None), Role::Button);
        assert_eq!(role_for("SomethingNew", FocusRole::None, None), Role::GenericContainer);
    }
}
