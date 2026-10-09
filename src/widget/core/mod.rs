use crate::widget::WidgetHost;

pub mod hover_animation;

/// The system clipboard, as text: what every widget's copy, cut and paste go
/// through. One synchronous pair, with a backend per platform:
///
/// - **Wayland** (Linux and the other non-Apple unixes): `wl-copy` /
///   `wl-paste`, `xclip` where those are missing.
/// - **macOS**: the general `NSPasteboard`, plain-text type.
/// - **A page**: a browser hands a page the clipboard only inside a `paste`
///   event, so a read is the text of the last one the browser shell saw (it
///   holds a ⌘/Ctrl+V back until the event has come; `web::shell`), or what
///   the page itself copied last. A copy writes through the async Clipboard
///   API where the page is a secure context, and the shell also answers the
///   `copy` / `cut` event a ⌘/Ctrl+C or X raises with it, which needs none.
///   Before this a copy in a page panicked (`std::thread::spawn`).
pub mod clipboard;

pub mod context_menu;

#[derive(Debug, Clone)]
pub struct Widget {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub label: Option<String>,
    /// What a screen reader calls the widget when it draws no label of its own
    /// (an `aria-label`): the accessibility tree prefers it over [`label`](Self::label),
    /// and nothing draws it. For a control whose label stands beside it in the host's
    /// own text — a value editor in a table row, an icon button.
    pub accessible_name: Option<String>,
    pub hovered: bool,
    pub row_x: f32,
    pub row_w: f32,
    pub focused: bool,
    pub id: std::cell::Cell<Option<crate::widget::WidgetId>>,
    pub dirty: bool,
    pub config_file: Option<String>,
    pub config_key: Option<String>,
}

impl Widget {
    pub fn new() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            label: None,
            accessible_name: None,
            hovered: false,
            row_x: 0.0,
            row_w: 0.0,
            focused: false,
            id: std::cell::Cell::new(None),
            dirty: true,
            config_file: None,
            config_key: None,
        }
    }

    pub fn new_rect(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            x,
            y,
            w,
            h,
            label: None,
            accessible_name: None,
            hovered: false,
            row_x: 0.0,
            row_w: 0.0,
            focused: false,
            id: std::cell::Cell::new(None),
            dirty: true,
            config_file: None,
            config_key: None,
        }
    }

    pub fn id(&self) -> crate::widget::WidgetId {
        let current = self.id.get();
        if let Some(id) = current {
            id
        } else {
            let next = crate::widget::NEXT_WIDGET_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let id = crate::widget::WidgetId(next);
            self.id.set(Some(id));
            id
        }
    }

    /// The detached-label strip this widget carries above its content: the one
    /// control-label formula (`layout::control_label_strip`) when a label is set,
    /// zero otherwise.
    pub fn label_offset(&self) -> f32 {
        if self.label.is_some() { crate::layout::control_label_strip() } else { 0.0 }
    }
}


pub fn clear_widget_references(w: &dyn WidgetHost) {
    // Focus needs no clearing: the context keeps an id, which a dropped widget's
    // generation no longer resolves.
    context_menu::clear_if_matches(w);
}

#[macro_export]
macro_rules! impl_widget_base {
    ($name:ident) => {
        fn base(&self) -> &$crate::widget::Widget { &self.base }
        fn base_mut(&mut self) -> &mut $crate::widget::Widget { &mut self.base }
        fn as_any(&self) -> &dyn std::any::Any { self }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    };
}

#[cfg(test)]
mod context_menu_slider_tests;

#[cfg(test)]
mod context_menu_padding_tests;

#[cfg(test)]
mod context_menu_page_tests;

#[cfg(test)]
mod context_menu_action_tests;
