//! Narrow-trait `Paginator` (Phase 5r; pages folded away in Phase 6au) — a vertical sidebar tab
//! strip. The tabs live in an EMBEDDED [`ButtonStrip`] owned by value; every app manages its own
//! page content keyed on `selected_page()`, so the former `Vec<Page>` stack (empty `Page`
//! containers toggled visible/hidden) is gone — its only observable output, the page-area
//! background quad, is painted directly here.
//!
//! The strip is an [`Embedded`] child: held by value until the paginator is inserted, the
//! context's after that ([`Layout::register_embedded_children`] attaches it and links it
//! under the paginator, so the walks reach it and the spatial grid holds it — the registered
//! strip is what makes the sidebar block root plate drags). The paginator keeps the strip's
//! place and selection and pushes them down there, on every layout and tick.

use crate::colors;
use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::input::ButtonStrip;
use crate::widget::{Adapted, Embedded, WidgetHost, Event, EventCtx, Input, Layout, MenuController, PageSelector, Paint, UiContext, WidgetId};

pub struct Paginator {
    /// The tab strip, the context's once the paginator is (see the module doc).
    pub sidebar_menu: Embedded<Adapted<ButtonStrip>>,
    /// The paginator's content rect, where the strip is placed from.
    rect: Rect,
    /// Whether a page wears a glyph (`with_icons`): the strip's width allows for it.
    has_icons: bool,
    /// A page chosen through `set_selected_page` that the strip (the context's) has not
    /// been shown yet.
    push_page: bool,
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
            sidebar_menu: Embedded::new(Adapted::new(ButtonStrip::new(0.0, 0.0, 0.0, 0.0))),
            rect: Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 },
            has_icons: false,
            push_page: false,
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
            sidebar_menu: Embedded::new(sidebar_menu),
            rect: Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 },
            has_icons: false,
            push_page: false,
            selected_page: 0,
            sidebar_w,
            page_labels: pages,
            on_page_changed_cb: None,
            just_clicked: None,
        })
    }
}

impl Adapted<Paginator> {
    /// A cce-icons glyph per page, worn at the top of its sidebar tab — see
    /// [`ButtonStrip::icons`].
    pub fn with_icons(mut self, icons: Vec<Option<String>>) -> Self {
        let p = self.inner_mut();
        p.has_icons = icons.iter().any(|i| i.is_some());
        let strip = p.sidebar_menu.here_mut().expect("with_icons builds a paginator not yet inserted");
        strip.inner_mut().set_icons(icons);
        p.sidebar_w = p.sidebar_w();
        let w = p.sidebar_w;
        let strip = p.sidebar_menu.here_mut().expect("with_icons builds a paginator not yet inserted");
        let (x, y, _, h) = strip.rect();
        strip.set_rect(x, y, w, h);
        self
    }

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
            // The strip takes it now when it is held here, else at the next layout or tick.
            match self.sidebar_menu.here_mut() {
                Some(strip) => strip.inner_mut().set_selected(Some(page)),
                None => self.push_page = true,
            }
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

        let _ = font_fam;
        // A page with a glyph (`with_icons`) needs the glyph's width; one
        // without, a line of its rotated label.
        let content_w = if self.has_icons { ButtonStrip::ICON_SIDE.max(font_size) } else { font_size };
        let max_w = content_w + 2.0 * padding;
        max_w.max(1.0)
    }
}

impl Layout for Paginator {
    /// Keep the rect the strip is placed from (the strip is the context's, and `set_rect`
    /// has none: it is placed in `register_embedded_children`, or here while held by value).
    fn arrange_children(&mut self, rect: Rect, _host: *mut (dyn WidgetHost + 'static)) {
        self.rect = rect;
        let sidebar_w = self.sidebar_w();
        if let Some(strip) = self.sidebar_menu.here_mut() {
            strip.set_rect(rect.x, rect.y, sidebar_w, rect.height);
        }
    }

    /// The strip in the context, linked under the paginator, on the left at its measured
    /// width. A page chosen through the paginator is shown on it; a tab chosen on the strip
    /// itself (the router reaches the linked strip before the paginator, and its click can
    /// end there) becomes the paginator's page.
    fn register_embedded_children(&mut self, host_id: WidgetId, ctx: &mut UiContext) {
        self.sidebar_menu.attach(ctx);
        let (rect, sidebar_w) = (self.rect, self.sidebar_w());
        let strip = self.sidebar_menu.get_mut(ctx);
        strip.set_rect(rect.x, rect.y, sidebar_w, rect.height);
        if std::mem::take(&mut self.push_page) {
            strip.inner_mut().set_selected(Some(self.selected_page));
        } else if let Some(page) = strip.inner().selected {
            self.selected_page = page;
        }
        ctx.link_ids(host_id, self.sidebar_menu.id());
    }

    fn release_embedded_children(&mut self, ctx: &mut UiContext) {
        self.sidebar_menu.detach(ctx);
    }
}

impl Paint for Paginator {
    fn color(&self) -> [f32; 4] {
        colors::sidebar_bg_color()
    }

    /// Own geometry: the sidebar background plus the page-area background — the latter is the
    /// one visual the former empty `Page` stack contributed (its bg quad over the content
    /// area), painted directly since the pages folded away (Phase 6au). Both round at the
    /// plate radius; the page area keeps only its outer corners so the seam with the sidebar
    /// stays straight.
    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let r = crate::layout::plate_corner_radius();
        let c = self.color();
        if c[3] > 0.0 {
            ctx.rounded_rect(rect, r, (true, true, true, true), c);
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
                ctx.rounded_rect(page_rect, r, (false, true, true, false), pc);
            }
        }
    }
}

impl Input for Paginator {
    fn blocks_root_plate_drag(&self) -> bool {
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
            Event::PointerMove { x: px, y: py, .. } => {
                self.sidebar_menu.lend(ui, |w, ui| w.cursor_moved(*px, *py, ui)).unwrap_or(false)
            }
            Event::MouseButton { button, state, x: px, y: py, .. } => {
                let mut changed = false;
                let clicked = self.sidebar_menu.lend(ui, |w, ui| {
                    w.mouse_input(*button, *state, *px, *py, ui).then(|| w.inner_mut().take_click())
                });
                if let Some(Some(click)) = clicked {
                    changed = true;
                    if let Some(idx) = click {
                        self.set_selected_page(idx);
                        self.just_clicked = Some(idx);
                    }
                }
                changed
            }
            Event::MouseWheel { delta, x: px, y: py, .. } => {
                self.sidebar_menu.lend(ui, |w, ui| w.mouse_wheel(delta, *px, *py, ui)).unwrap_or(false)
            }
            Event::KeyInput(key_event) => self.sidebar_menu.lend(ui, |w, ui| w.keyboard_input(key_event, ui)).unwrap_or(false),
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
    use crate::widget::{ElementState, Handle, MouseButton};

    /// A paginator in `ctx`, placed: inserting it puts its strip in too, and the embedded
    /// hook places the strip.
    fn paginator(ctx: &mut UiContext) -> Handle<Adapted<Paginator>> {
        let h = ctx.insert(Paginator::new(vec!["One".to_string(), "Two".to_string()]));
        ctx.lend_h(h, |p, ctx| {
            WidgetHost::set_rect(p, 0.0, 0.0, 400.0, 300.0);
            p.attach_embedded(ctx);
        });
        h
    }

    #[test]
    fn sidebar_click_switches_page_and_drains_menu_click() {
        let mut ctx = UiContext::new();
        let h = paginator(&mut ctx);

        // Click the second tab (the strip commits selection on release): the selection moves
        // and menu_click reports (1, 0) once.
        let (bx, by, bw, bh) = ctx[h].sidebar_menu.get(&ctx).item_rect(1);
        assert!(bw > 0.0, "strip placed by the embedded hook");
        let (cx, cy) = (bx + bw / 2.0, by + bh / 2.0);
        ctx.lend_h(h, |p, ctx| {
            p.mouse_input(MouseButton::Left, ElementState::Pressed, cx, cy, ctx);
            p.mouse_input(MouseButton::Left, ElementState::Released, cx, cy, ctx);
        });
        let p = &mut ctx[h];
        assert_eq!(p.selected_page, 1);
        assert_eq!(MenuController::menu_click(&mut **p), Some((1, 0)));
        assert_eq!(MenuController::menu_click(&mut **p), None, "click drained");

        // The PageSelector capability is reached through the concrete adapter (the
        // cce-test-interface downcast shape).
        assert_eq!(PageSelector::selected_page(&**p), 1);
        assert!(PageSelector::sidebar_w(&**p) > 0.0);
    }

    #[test]
    fn the_sidebar_bg_is_rounded_and_the_strip_is_the_contexts() {
        let mut ctx = UiContext::new();
        let h = paginator(&mut ctx);

        // The sidebar background is a rounded rect of the paginator's own paint, never a
        // plain quad.
        let bg = colors::sidebar_bg_color();
        if bg[3] > 0.0 {
            let r = crate::layout::plate_corner_radius();
            let bg_quad = (0.0, 0.0, 400.0, 300.0, r, bg, (true, true, true, true));
            assert!(!crate::widget::shown_quads(&ctx[h]).iter().any(|q| q.4 == bg && q.2 == 400.0), "no plain bg");
            assert!(crate::widget::shown_rounded_quads(&ctx[h]).contains(&bg_quad), "a rounded bg");
        }

        // The strip is an entry of the context's own, linked under the paginator (the spatial
        // grid feeds off it — the registered strip is what blocks root plate drags over the
        // sidebar, and the walks reach it through the link).
        let strip_id = ctx[h].sidebar_menu.id();
        assert!(ctx.tree.is_owned(strip_id), "the strip is the context's");
        assert!(ctx.tree.child_ids(h.id()).contains(&strip_id), "linked under the paginator");
    }

    /// A page chosen through the paginator reaches its strip at the next layout or tick, and
    /// a paginator taken out of the context comes back with its strip in it.
    #[test]
    fn the_strip_follows_the_page_and_leaves_with_the_paginator() {
        let mut ctx = UiContext::new();
        let h = paginator(&mut ctx);
        PageSelector::set_selected_page(&mut *ctx[h], 1);
        ctx.lend_h(h, |p, ctx| WidgetHost::tick(p, 0.016, ctx));
        assert_eq!(ctx[h].sidebar_menu.get(&ctx).inner().selected, Some(1));

        // A tab clicked on the strip itself (the router reaches the linked strip before the
        // paginator) stays chosen, and becomes the paginator's page.
        let strip = ctx[h].sidebar_menu.handle().expect("the strip is the context's");
        let (bx, by, bw, bh) = ctx[strip].item_rect(0);
        let (cx, cy) = (bx + bw / 2.0, by + bh / 2.0);
        ctx.lend_h(strip, |s, ctx| {
            s.mouse_input(MouseButton::Left, ElementState::Pressed, cx, cy, ctx);
            s.mouse_input(MouseButton::Left, ElementState::Released, cx, cy, ctx);
        });
        ctx.lend_h(h, |p, ctx| WidgetHost::tick(p, 0.016, ctx));
        assert_eq!(ctx[strip].inner().selected, Some(0), "the tick left the strip's choice alone");
        assert_eq!(ctx[h].selected_page, 0);

        let strip_id = ctx[h].sidebar_menu.id();
        let p = ctx.remove(h).expect("given back");
        assert!(!ctx.tree.is_registered(strip_id), "the strip left with it");
        assert_eq!(p.sidebar_menu.here().map(|s| s.inner().selected), Some(Some(0)));
    }
}
