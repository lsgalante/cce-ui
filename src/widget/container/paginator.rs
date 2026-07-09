//! Narrow-trait `Paginator` (Phase 5r) — a vertical sidebar tab strip plus a stack of pages, of
//! which only the selected one shows. Both halves are EMBEDDED legacy widgets owned by value:
//! the tabs live in a [`ButtonStrip`], the content in a `Vec<Page>` (Page is an embedded-base
//! container that dissolves in Phase 6 — it does not go through `Adapted` itself). The adapter's
//! container concern does the subtree plumbing, filtered to the strip + selected page by
//! [`Layout::child_visible`]; the model keeps the legacy specifics: strip/pages arrangement in
//! [`Layout::arrange_children`], event proxying in [`Input::on_event`] (via `EventCtx::ui`), and
//! the [`PageSelector`] / [`MenuController`] capabilities.
//!
//! Two legacy behaviors ride hooks new with this migration:
//! - [`Layout::register_embedded_children`]: legacy `tick`/`layout` re-registered the strip and
//!   pages into the ctx registry every frame — load-bearing for the spatial grid (the registered
//!   strip is what makes the sidebar block backplate drags; see
//!   `backplate::tests::test_paginator_blocks_backplate_drag`).
//! - [`Paint::aggregates_child_extra_quads`] + [`Paint::forwarded_highlight`]: legacy
//!   `extra_quads` served the children's chrome only (cce-email and cce-layout-interface render
//!   the tab column through that getter — the paginator's own background quad lives in
//!   `all_quads` alone), and legacy `highlight_quad` forwarded to the strip's (the hovered-tab
//!   tint cce-layout-interface draws directly).
//!
//! In practice every app uses only the tab-strip half (`selected_page`/`set_selected_page`/
//! `sidebar_w`/`menu_click`) and manages page content itself; the `pages` container surface
//! (`add_widget_to_page` & co.) is ported bug-for-bug but has no callers workspace-wide.

use crate::colors;
use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::input::ButtonStrip;
use crate::widget::{
    Adapted, Element, Event, EventCtx, Input, Layout, MenuController, PageSelector, Paint,
    UiContext, WidgetId,
};
use super::page::Page;

pub struct Paginator {
    pub sidebar_menu: ButtonStrip,
    pub pages: Vec<Page>,
    pub selected_page: usize,
    pub page_hidden: bool,
    pub sidebar_w: f32,
    pub page_labels: Vec<String>,
    pub on_page_changed_cb: Option<Box<dyn Fn(usize) + Send + Sync>>,
    pub just_clicked: Option<usize>,
}

impl Paginator {
    pub fn new(pages: Vec<String>) -> Adapted<Paginator> {
        let num_pages = pages.len();

        let temp_paginator = Paginator {
            sidebar_menu: ButtonStrip::new(0.0, 0.0, 0.0, 0.0),
            pages: Vec::new(),
            selected_page: 0,
            page_hidden: false,
            sidebar_w: 0.0,
            page_labels: pages.clone(),
            on_page_changed_cb: None,
            just_clicked: None,
        };
        let sidebar_w = temp_paginator.sidebar_w();

        let mut sidebar_menu = ButtonStrip::new(0.0, 0.0, sidebar_w, 0.0)
            .with_vertical(true)
            .with_buttons(pages.clone());
        if num_pages > 0 {
            sidebar_menu.set_selected(Some(0));
        }

        let mut pages_containers = Vec::new();
        for _ in 0..num_pages {
            let mut page = Page::new(0.0, 0.0, 0.0, 0.0);
            page.visible = false;
            pages_containers.push(page);
        }
        if num_pages > 0 {
            pages_containers[0].visible = true;
        }

        Adapted::new(Paginator {
            sidebar_menu,
            pages: pages_containers,
            selected_page: 0,
            page_hidden: false,
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
        if page < self.pages.len() {
            self.selected_page = page;
            self.sidebar_menu.set_selected(Some(page));
            for (i, page_item) in self.pages.iter_mut().enumerate() {
                page_item.visible = i == page;
            }
            if let Some(ref cb) = self.on_page_changed_cb {
                cb(page);
            }
        }
    }

    fn is_page_hidden(&self) -> bool {
        self.page_hidden
    }

    fn set_page_hidden(&mut self, hidden: bool) {
        self.page_hidden = hidden;
    }

    fn set_pages(&mut self, pages: Vec<String>) {
        self.page_labels = pages.clone();
        self.sidebar_menu.buttons = pages.clone();
        self.sidebar_menu.generate_rotated_labels();

        self.pages.clear();
        for _ in 0..pages.len() {
            let mut page = Page::new(0.0, 0.0, 0.0, 0.0);
            page.visible = false;
            self.pages.push(page);
        }
        if !self.pages.is_empty() {
            let idx = self.selected_page.min(self.pages.len() - 1);
            self.pages[idx].visible = true;
        }
    }

    fn set_pages_with_items(&mut self, pages: Vec<String>, _items: Vec<Vec<String>>) {
        self.set_pages(pages);
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

    fn set_sidebar_mode(&mut self, _enabled: bool) {}

    fn set_sidebar_label(&mut self, _label: Option<String>) {}

    fn add_widget_to_page(&mut self, page_idx: usize, widget: *mut (dyn Element + 'static), ctx: &mut UiContext) {
        if page_idx < self.pages.len() {
            self.pages[page_idx].add_child(widget, ctx);
        }
    }

    fn clear_page_widgets(&mut self, page_idx: usize, ctx: &mut UiContext) {
        if page_idx < self.pages.len() {
            self.pages[page_idx].clear_children(ctx);
        }
    }
}

impl Layout for Paginator {
    fn has_container_children(&self) -> bool {
        true
    }

    fn container_children(&self) -> Vec<*mut (dyn Element + 'static)> {
        let mut childs: Vec<*mut (dyn Element + 'static)> = Vec::new();
        childs.push(&self.sidebar_menu as &dyn Element as *const (dyn Element + 'static) as *mut (dyn Element + 'static));
        for plate in &self.pages {
            childs.push(plate as &dyn Element as *const (dyn Element + 'static) as *mut (dyn Element + 'static));
        }
        childs
    }

    /// The strip always shows; a page only while selected and not hidden (legacy
    /// `is_child_visible`).
    fn child_visible(&self, child: *mut (dyn Element + 'static)) -> bool {
        let strip_ptr = &self.sidebar_menu as &dyn Element as *const (dyn Element + 'static);
        if std::ptr::addr_eq(child, strip_ptr) {
            return true;
        }
        if let Some(plate) = self.pages.get(self.selected_page) {
            if !self.page_hidden {
                let plate_ptr = plate as &dyn Element as *const (dyn Element + 'static);
                if std::ptr::addr_eq(child, plate_ptr) {
                    return true;
                }
            }
        }
        false
    }

    /// The legacy `set_rect` body: strip on the left at its measured width, every page filling
    /// the remainder, page visibility synced to the selection.
    fn arrange_children(&mut self, rect: Rect, _host: *mut (dyn Element + 'static)) {
        let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);
        let sidebar_w = self.sidebar_w();
        self.sidebar_menu.set_rect(x, y, sidebar_w, h);

        let page_x = x + sidebar_w;
        let page_w = (w - sidebar_w).max(0.0);
        let selected = self.selected_page;
        let hidden = self.page_hidden;
        for (i, plate) in self.pages.iter_mut().enumerate() {
            plate.set_rect(page_x, y, page_w, h);
            plate.visible = (i == selected) && !hidden;
        }
    }

    fn register_embedded_children(&mut self, host_id: WidgetId, ctx: &mut UiContext) {
        let menu_ptr = &mut self.sidebar_menu as *mut ButtonStrip;
        ctx.register_widget(self.sidebar_menu.base.id(), menu_ptr);
        ctx.link_ids(host_id, self.sidebar_menu.base.id());

        for plate in &mut self.pages {
            let plate_ptr = plate as *mut Page;
            ctx.register_widget(plate.base.base.id(), plate_ptr);
            ctx.link_ids(host_id, plate.base.base.id());
        }
    }
}

impl Paint for Paginator {
    fn color(&self) -> [f32; 4] {
        colors::sidebar_bg_color()
    }

    /// Own geometry is just the sidebar background (the legacy `all_quads` head; the strip's
    /// and selected page's pixels arrive through the adapter's child aggregation / the paint
    /// walk's recursion).
    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let c = self.color();
        if c[3] > 0.0 {
            ctx.quad(rect, c);
        }
    }

    /// Legacy `extra_quads` served the strip's + selected page's chrome only — cce-email and
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

    /// Event proxying, the legacy forwarding bodies: strip first (draining its click into the
    /// page selection), then the selected page while not hidden.
    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        let Some(ui) = ectx.ui.as_deref_mut() else {
            return false;
        };
        let selected = self.selected_page;
        let hidden = self.page_hidden;
        match event {
            Event::PointerMove { x: px, y: py, .. } => {
                let mut changed = self.sidebar_menu.cursor_moved(*px, *py, ui);
                for (i, plate) in self.pages.iter_mut().enumerate() {
                    if i == selected && !hidden {
                        if plate.cursor_moved(*px, *py, ui) {
                            changed = true;
                        }
                    }
                }
                changed
            }
            Event::MouseButton { button, state, x: px, y: py, .. } => {
                let mut changed = false;
                if self.sidebar_menu.mouse_input(*button, *state, *px, *py, ui) {
                    changed = true;
                    if let Some(idx) = self.sidebar_menu.take_click() {
                        self.set_selected_page(idx);
                        self.just_clicked = Some(idx);
                    }
                }
                for (i, plate) in self.pages.iter_mut().enumerate() {
                    if i == selected && !hidden {
                        if plate.mouse_input(*button, *state, *px, *py, ui) {
                            changed = true;
                        }
                    }
                }
                changed
            }
            Event::MouseWheel { delta, x: px, y: py, .. } => {
                let mut changed = self.sidebar_menu.mouse_wheel(delta, *px, *py, ui);
                for (i, plate) in self.pages.iter_mut().enumerate() {
                    if i == selected && !hidden {
                        if plate.mouse_wheel(delta, *px, *py, ui) {
                            changed = true;
                        }
                    }
                }
                changed
            }
            Event::KeyInput(key_event) => {
                let mut changed = self.sidebar_menu.keyboard_input(key_event, ui);
                for (i, plate) in self.pages.iter_mut().enumerate() {
                    if i == selected && !hidden {
                        if plate.keyboard_input(key_event, ui) {
                            changed = true;
                        }
                    }
                }
                changed
            }
            _ => false,
        }
    }

    fn menu_controller(&self) -> Option<&dyn MenuController> {
        Some(self)
    }
    fn menu_controller_mut(&mut self) -> Option<&mut dyn MenuController> {
        Some(self)
    }
    fn page_selector(&self) -> Option<&dyn PageSelector> {
        Some(self)
    }
    fn page_selector_mut(&mut self) -> Option<&mut dyn PageSelector> {
        Some(self)
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
        Element::set_rect(&mut p, 0.0, 0.0, 400.0, 300.0);
        p
    }

    #[test]
    fn sidebar_click_switches_page_and_drains_menu_click() {
        let mut ctx = UiContext::new();
        let mut p = paginator();
        let (id, ptr) = (p.id(), p.as_ptr_mut());
        ctx.register_widget(id, ptr);

        // Click the second tab (the strip commits selection on release): the selection moves,
        // the pages' visibility flips, and menu_click reports (1, 0) once.
        let (bx, by, bw, bh) = p.sidebar_menu.item_rect(1);
        assert!(bw > 0.0, "strip laid out by arrange_children");
        p.mouse_input(MouseButton::Left, ElementState::Pressed, bx + bw / 2.0, by + bh / 2.0, &mut ctx);
        p.mouse_input(MouseButton::Left, ElementState::Released, bx + bw / 2.0, by + bh / 2.0, &mut ctx);
        assert_eq!(p.selected_page, 1);
        assert!(p.pages[1].visible && !p.pages[0].visible);
        {
            let elem: &mut dyn Element = &mut p;
            assert_eq!(elem.as_menu_controller_mut().unwrap().menu_click(), Some((1, 0)));
            assert_eq!(elem.as_menu_controller_mut().unwrap().menu_click(), None, "click drained");
        }

        // The PageSelector capability rides the wrapper (cce-test-interface's downcast).
        let elem: &dyn Element = &p;
        let ps = elem.as_page_selector().unwrap();
        assert_eq!(ps.selected_page(), 1);
        assert!(ps.sidebar_w() > 0.0);
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
        let extra = Element::extra_quads(&p);
        let strip_extra = p.sidebar_menu.extra_quads();
        assert_eq!(extra.len(), strip_extra.len(), "children-only plain view (pages emit none)");
        let bg = colors::sidebar_bg_color();
        if bg[3] > 0.0 {
            let bg_quad = (0.0, 0.0, 400.0, 300.0, bg);
            assert!(!extra.contains(&bg_quad), "no own bg in extra_quads");
            assert!(Element::all_quads(&p, &ctx).contains(&bg_quad), "own bg in all_quads");
        }

        // The embedded strip + pages land in the registry on tick (the spatial grid feeds off
        // it — the registered strip is what blocks backplate drags over the sidebar).
        Element::tick(&mut p, 0.016, &mut ctx);
        let strip_id = p.sidebar_menu.base.id();
        assert!(
            ctx.tree.iter_registered().any(|(w_id, _)| w_id == strip_id),
            "strip registered by the tick-path healing"
        );
    }
}
