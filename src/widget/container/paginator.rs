//! Narrow-trait `Paginator` (Phase 5r; pages folded away in Phase 6au) — a vertical sidebar tab
//! strip. The tabs live in an EMBEDDED [`ButtonStrip`] owned by value; every app manages its own
//! page content keyed on `selected_page()`, so the former `Vec<Page>` stack (empty `Page`
//! containers toggled visible/hidden) is gone — its only observable output, the page-area
//! background quad, is painted directly here.
//!
//! Two legacy behaviors ride hooks from the 5r migration:
//! - [`Layout::register_embedded_children`]: legacy `tick`/`layout` re-registered the strip into
//!   the ctx registry every frame — load-bearing for the spatial grid (the registered strip is
//!   what makes the sidebar block backplate drags).
//! - [`Paint::aggregates_child_extra_quads`] + [`Paint::forwarded_highlight`]: legacy
//!   `extra_quads` served the strip's chrome only (cce-mail and cce-layout-interface render
//!   the tab column through that getter — the paginator's own background quads live in
//!   `all_quads` alone), and legacy `highlight_quad` forwarded to the strip's (the hovered-tab
//!   tint cce-layout-interface draws directly).

use crate::colors;
use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::input::ButtonStrip;
use crate::widget::{
    Adapted, WidgetHost, Event, EventCtx, Input, Layout, MenuController, PageSelector, Paint,
    UiContext, WidgetId,
};

pub struct Paginator {
    pub sidebar_menu: Adapted<ButtonStrip>,
    pub selected_page: usize,
    pub sidebar_w: f32,
    pub page_labels: Vec<String>,
    pub on_page_changed_cb: Option<Box<dyn Fn(usize) + Send + Sync>>,
    pub just_clicked: Option<usize>,
}

impl Paginator {
    pub fn new(pages: Vec<String>) -> Adapted<Paginator> {
        let num_pages = pages.len();

        let temp_paginator = Paginator {
            sidebar_menu: Adapted::new(ButtonStrip::new(0.0, 0.0, 0.0, 0.0)),
            selected_page: 0,
            sidebar_w: 0.0,
            page_labels: pages.clone(),
            on_page_changed_cb: None,
            just_clicked: None,
        };
        let sidebar_w = temp_paginator.sidebar_w();

        let mut sidebar_menu = Adapted::new(
            ButtonStrip::new(0.0, 0.0, sidebar_w, 0.0)
                .with_vertical(true)
                .with_buttons(pages.clone()),
        );
        if num_pages > 0 {
            sidebar_menu.inner_mut().set_selected(Some(0));
        }

        Adapted::new(Paginator {
            sidebar_menu,
            selected_page: 0,
            sidebar_w,
            page_labels: pages,
            on_page_changed_cb: None,
            just_clicked: None,
        })
    }
}

impl Adapted<Paginator> {
    pub fn with_sidebar_mode(self, _enabled: bool) -> Self {
        self
    }

    pub fn with_tabs_rotated(self, _rotated: bool) -> Self {
        self
    }

    pub fn with_tabs_at_top(self, _top: bool) -> Self {
        self
    }

    pub fn with_tab_y_offset(self, _offset: f32) -> Self {
        self
    }

    pub fn with_title(self, _title: &str) -> Self {
        self
    }

    pub fn with_vertical(self, _vertical: bool) -> Self {
        self
    }

    pub fn on_page_changed<F: Fn(usize) + Send + Sync + 'static>(mut self, cb: F) -> Self {
        self.on_page_changed_cb = Some(Box::new(cb));
        self
    }
}

impl PageSelector for Paginator {
    fn selected_page(&self) -> usize {
        self.selected_page
    }

    fn set_selected_page(&mut self, page: usize) {
        if page < self.page_labels.len() {
            self.selected_page = page;
            self.sidebar_menu.inner_mut().set_selected(Some(page));
            if let Some(ref cb) = self.on_page_changed_cb {
                cb(page);
            }
        }
    }

    fn sidebar_w(&self) -> f32 {
        if self.page_labels.is_empty() {
            return self.sidebar_w;
        }
        let font_info = crate::layout::menubar_font_parsed();
        let font_fam = font_info.0;
        let font_size = font_info.1;
        let padding = crate::layout::button_padding();

        let mut max_w = 0.0;
        for label in &self.page_labels {
            let trimmed = label.trim();
            let space_idx = trimmed.find(' ');
            let has_icon = space_idx.map(|idx| trimmed.split_at(idx).0.trim().chars().count() == 1).unwrap_or(false);
            let content_w = if has_icon {
                let space_idx = space_idx.unwrap();
                let (icon, _) = trimmed.split_at(space_idx);
                let icon = icon.trim();
                let icon_font_size = 14.0;
                let est_icon_w = crate::widget::display::measure_text_width(icon, &font_fam, icon_font_size);
                est_icon_w.max(font_size)
            } else {
                font_size
            };
            let w = content_w + 2.0 * padding;
            if w > max_w {
                max_w = w;
            }
        }
        max_w.max(1.0)
    }
}

impl Layout for Paginator {
    fn has_container_children(&self) -> bool {
        true
    }

    fn container_children(&self) -> Vec<*mut (dyn WidgetHost + 'static)> {
        vec![&self.sidebar_menu as &dyn WidgetHost as *const (dyn WidgetHost + 'static) as *mut (dyn WidgetHost + 'static)]
    }

    /// The legacy `set_rect` body: strip on the left at its measured width.
    fn arrange_children(&mut self, rect: Rect, _host: *mut (dyn WidgetHost + 'static)) {
        let (x, y, h) = (rect.x, rect.y, rect.height);
        let sidebar_w = self.sidebar_w();
        self.sidebar_menu.set_rect(x, y, sidebar_w, h);
    }

    fn register_embedded_children(&mut self, host_id: WidgetId, ctx: &mut UiContext) {
        let menu_ptr = self.sidebar_menu.as_ptr_mut();
        ctx.register_widget(self.sidebar_menu.id(), menu_ptr);
        ctx.link_ids(host_id, self.sidebar_menu.id());
    }
}

impl Paint for Paginator {
    fn color(&self) -> [f32; 4] {
        colors::sidebar_bg_color()
    }

    /// Own geometry: the sidebar background plus the page-area background — the latter is the
    /// one visual the former empty `Page` stack contributed (its bg quad over the content
    /// area), painted directly since the pages folded away (Phase 6au).
    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let c = self.color();
        if c[3] > 0.0 {
            ctx.quad(rect, c);
        }
        if !self.page_labels.is_empty() {
            let mut pc = colors::page_color();
            pc[3] *= crate::layout::page_opacity();
            if pc[3] > 0.0 {
                let sidebar_w = self.sidebar_w();
                let page_rect = Rect {
                    x: rect.x + sidebar_w,
                    y: rect.y,
                    width: (rect.width - sidebar_w).max(0.0),
                    height: rect.height,
                };
                ctx.quad(page_rect, pc);
            }
        }
    }

    /// Legacy `extra_quads` served the strip's + selected page's chrome only — cce-mail and
    /// cce-layout-interface draw the tab column through this getter, over their own backgrounds.
    fn aggregates_child_extra_quads(&self) -> bool {
        true
    }

    /// Legacy `highlight_quad` forwarded to the strip's (the hovered-tab tint
    /// cce-layout-interface draws directly).
    fn forwarded_highlight(&self, ctx: &UiContext) -> Option<Option<(f32, f32, f32, f32, [f32; 4])>> {
        Some(self.sidebar_menu.highlight_quad(ctx))
    }
}

impl Input for Paginator {
    fn blocks_backplate_drag(&self) -> bool {
        false
    }

    /// Legacy `mouse_input` saw every press — the strip and pages ran their own hit checks.
    fn gates_presses(&self) -> bool {
        false
    }

    fn wants_tick(&self) -> bool {
        true
    }

    /// Event proxying, the legacy forwarding body: the strip, draining its click into the page
    /// selection. (The former empty pages also received every event, but had nothing to do with
    /// them — no children, never scrollable.)
    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        let Some(ui) = ectx.ui.as_deref_mut() else {
            return false;
        };
        match event {
            Event::PointerMove { x: px, y: py, .. } => self.sidebar_menu.cursor_moved(*px, *py, ui),
            Event::MouseButton { button, state, x: px, y: py, .. } => {
                let mut changed = false;
                if self.sidebar_menu.mouse_input(*button, *state, *px, *py, ui) {
                    changed = true;
                    if let Some(idx) = self.sidebar_menu.inner_mut().take_click() {
                        self.set_selected_page(idx);
                        self.just_clicked = Some(idx);
                    }
                }
                changed
            }
            Event::MouseWheel { delta, x: px, y: py, .. } => self.sidebar_menu.mouse_wheel(delta, *px, *py, ui),
            Event::KeyInput(key_event) => self.sidebar_menu.keyboard_input(key_event, ui),
            _ => false,
        }
    }

}

impl MenuController for Paginator {
    fn menu_click(&mut self) -> Option<(usize, usize)> {
        self.just_clicked.take().map(|idx| (idx, 0))
    }

    fn trigger_menu_click(&mut self, _menu_idx: usize, _item_idx: usize) {}
    fn set_item_checked(&mut self, _menu_idx: usize, _item_idx: usize, _checked: bool) {}
    fn set_menu_items(&mut self, _menu_idx: usize, _items: &[String]) {}
    fn is_menu_bar(&self) -> bool { false }
    fn is_menu_open(&self) -> bool { false }
    fn menu_items(&self) -> Vec<String> { Vec::new() }
    fn menu_item_checked(&self) -> Vec<Option<bool>> { Vec::new() }
    fn is_vertical(&self) -> bool { true }
    fn menu_names(&self) -> Vec<String> { Vec::new() }
    fn menu_items_list(&self) -> Vec<Vec<String>> { Vec::new() }
    fn menu_checked_list(&self) -> Vec<Vec<Option<bool>>> { Vec::new() }
    fn take_context_change(&mut self) -> Option<usize> { None }
    fn set_context_selected(&mut self, _selected: usize) {}
    fn set_center_items(&mut self, _center: bool) {}
    fn get_menu_items_at(&self, _px: f32, _py: f32) -> Option<(usize, String, Vec<String>, f32, f32, f32, f32)> { None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::UiContext;
    use crate::widget::{ElementState, MouseButton};

    fn paginator() -> Adapted<Paginator> {
        let mut p = Paginator::new(vec!["One".to_string(), "Two".to_string()]);
        WidgetHost::set_rect(&mut p, 0.0, 0.0, 400.0, 300.0);
        p
    }

    #[test]
    fn sidebar_click_switches_page_and_drains_menu_click() {
        let mut ctx = UiContext::new();
        let mut p = paginator();
        let (id, ptr) = (p.id(), p.as_ptr_mut());
        ctx.register_widget(id, ptr);

        // Click the second tab (the strip commits selection on release): the selection moves
        // and menu_click reports (1, 0) once.
        let (bx, by, bw, bh) = p.sidebar_menu.item_rect(1);
        assert!(bw > 0.0, "strip laid out by arrange_children");
        p.mouse_input(MouseButton::Left, ElementState::Pressed, bx + bw / 2.0, by + bh / 2.0, &mut ctx);
        p.mouse_input(MouseButton::Left, ElementState::Released, bx + bw / 2.0, by + bh / 2.0, &mut ctx);
        assert_eq!(p.selected_page, 1);
        assert_eq!(MenuController::menu_click(&mut *p), Some((1, 0)));
        assert_eq!(MenuController::menu_click(&mut *p), None, "click drained");

        // The PageSelector capability is reached through the concrete adapter (the
        // cce-test-interface downcast shape).
        assert_eq!(PageSelector::selected_page(&*p), 1);
        assert!(PageSelector::sidebar_w(&*p) > 0.0);
    }

    #[test]
    fn plain_quads_split_like_legacy_and_registration_heals_on_tick() {
        let mut ctx = UiContext::new();
        let mut p = paginator();
        let (id, ptr) = (p.id(), p.as_ptr_mut());
        ctx.register_widget(id, ptr);

        // Legacy split: `extra_quads` is the children's chrome only; the sidebar background
        // quad lives in `all_quads` alone (email/layout-interface draw their own backgrounds
        // under `extra_quads`).
        let extra = WidgetHost::extra_quads(&p);
        let strip_extra = p.sidebar_menu.extra_quads();
        assert_eq!(extra.len(), strip_extra.len(), "children-only plain view (pages emit none)");
        let bg = colors::sidebar_bg_color();
        if bg[3] > 0.0 {
            let bg_quad = (0.0, 0.0, 400.0, 300.0, bg);
            assert!(!extra.contains(&bg_quad), "no own bg in extra_quads");
            assert!(WidgetHost::all_quads(&p, &ctx).contains(&bg_quad), "own bg in all_quads");
        }

        // The embedded strip + pages land in the registry on tick (the spatial grid feeds off
        // it — the registered strip is what blocks backplate drags over the sidebar).
        WidgetHost::tick(&mut p, 0.016, &mut ctx);
        let strip_id = p.sidebar_menu.id();
        assert!(
            ctx.tree.iter_registered().any(|(w_id, _)| w_id == strip_id),
            "strip registered by the tick-path healing"
        );
    }
}
