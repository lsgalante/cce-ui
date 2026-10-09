//! The context menu through the context: showing it, a right click on a widget, and routing
//! pointer input to it and its quads and labels to the host.

use super::*;

impl UiContext {
    pub fn is_context_menu_visible(&self) -> bool {
        crate::widget::context_menu::is_visible()
    }

    /// Open the shared context menu on the widget `target`, which its actions go to.
    pub fn show_context_menu(&mut self, x: f32, y: f32, options: Vec<String>, header_count: usize, target: WidgetId) {
        crate::widget::context_menu::show(x, y, options, header_count, target);
    }

    /// [`show_context_menu`](Self::show_context_menu) with each row's action beside its
    /// label (`None` for a header, or a row the host handles itself), so the label is only
    /// what is shown and a translated menu still does what it did.
    pub fn show_context_menu_rows(&mut self, x: f32, y: f32, rows: Vec<(String, Option<crate::widget::ContextAction>)>, header_count: usize, target: WidgetId) {
        let (options, actions): (Vec<String>, Vec<Option<crate::widget::ContextAction>>) = rows.into_iter().unzip();
        self.show_context_menu(x, y, options, header_count, target);
        crate::widget::context_menu::set_row_actions(actions);
    }

    /// Open the shared config context menu for a right-click on `target` — the widget
    /// itself, which the context has out on loan while it handles the press, so it is handed
    /// over rather than looked up.
    pub fn handle_right_click(&mut self, target: &dyn WidgetHost, px: f32, py: f32) {
        let name = target.type_name();
        let label = if name == "Breadcrumb" {
            target.as_any().downcast_ref::<crate::widget::container::Breadcrumb>().map(|bc| {
                let idx = bc.right_clicked_seg.unwrap_or(bc.path.len());
                bc.path_to_seg(idx)
            })
        } else {
            target.label()
        };
        let header = if let Some(lbl) = label {
            format!("[{}]: {}", name, lbl)
        } else {
            format!("[{}]", name)
        };

        let mut config_info = None;
        {
            let b = target.base();
            if let (Some(ref file), Some(ref key)) = (&b.config_file, &b.config_key) {
                config_info = Some((file.clone(), key.clone()));
            }
        }

        use crate::widget::ContextAction as CA;
        use crate::l10n::{tr, tr_args};
        // Each row's label in the user's language; what it does is its action, not its words.
        let row = |label: String, action: CA| (label, Some(action));
        let mut rows: Vec<(String, Option<CA>)> = vec![(header, None)];
        let mut header_count = 1;

        if let Some((file, key)) = config_info {
            rows.push((tr_args("menu-config-file", &[("file", &file)]), None));
            rows.push((tr_args("menu-config-key", &[("key", &key)]), None));
            header_count = 3;
        }

        if name == "TextBox" {
            let is_password = target
                .as_any()
                .downcast_ref::<crate::widget::input::TextBox>()
                .is_some_and(|tb| tb.is_password);
            if !is_password {
                rows.extend([row(tr("menu-cut"), CA::Cut), row(tr("menu-copy"), CA::Copy)]);
            }
            rows.extend([row(tr("menu-paste"), CA::Paste), row(tr("menu-select-all"), CA::SelectAll)]);
            // A search box says so (`with_search`); an English "Search..." placeholder is the
            // older sign, still read for the apps that set one themselves.
            let is_search = target
                .as_any()
                .downcast_ref::<crate::widget::input::TextBox>()
                .is_some_and(|tb| tb.is_search || tb.placeholder.as_deref() == Some("Search..."));
            if is_search {
                rows.push(row(tr("menu-clear"), CA::ClearText));
            }
        } else if name == "Breadcrumb" {
            rows.push(row(tr("menu-copy-path"), CA::CopyPath));
        } else if name == "Ramp" {
            let collapsed = target
                .as_any()
                .downcast_ref::<crate::widget::input::Ramp>()
                .is_some_and(|r| r.controls_collapsed);
            let label = tr("menu-collapse-controls");
            let label = if collapsed { format!("{}{label}", crate::widget::context_menu::MARK_CHECK) } else { label };
            rows.push((label, Some(CA::ToggleRampControls)));
            rows.extend([row(tr("menu-copy"), CA::Copy), row(tr("menu-paste"), CA::Paste)]);
        } else {
            rows.extend([row(tr("menu-copy"), CA::Copy), row(tr("menu-paste"), CA::Paste)]);
        }

        let scroll_y = crate::widget::hover_animation::get_scroll_offset();
        let adjusted_py = py - scroll_y;
        self.show_context_menu_rows(px, adjusted_py, rows, header_count, target.base().id());
    }

    pub fn hide_context_menu(&mut self) {
        crate::widget::context_menu::hide();
    }

    pub fn hit_test_context_menu(&self, px: f32, py: f32) -> bool {
        crate::widget::context_menu::hit_test(px, py)
    }

    pub fn cursor_moved_context_menu(&mut self, px: f32, py: f32) -> bool {
        crate::widget::context_menu::cursor_moved(px, py)
    }

    pub fn mouse_input_context_menu(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        crate::widget::context_menu::mouse_input(button, state, px, py, Some(self))
    }

    pub fn context_menu_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        crate::widget::context_menu::extra_quads()
    }

    pub fn context_menu_labels(&self) -> Vec<crate::widget::display::TextLabel> {
        crate::widget::context_menu::text_labels()
    }
}
