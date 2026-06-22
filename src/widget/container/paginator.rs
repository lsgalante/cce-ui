use crate::colors;
use crate::widget::*;
use crate::widget::input::ButtonStrip;
use crate::widget::display::TextLabel;
use super::page::Page;

pub struct Paginator {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    pub sidebar_menu: ButtonStrip,
    pub pages: Vec<Page>,
    pub selected_page: usize,
    pub page_hidden: bool,
    pub sidebar_w: f32,
    pub page_labels: Vec<String>,
    parent: Option<*mut (dyn Element + 'static)>,
    pub on_page_changed_cb: Option<Box<dyn Fn(usize) + Send + Sync>>,
    pub just_clicked: Option<usize>,
}

impl Paginator {
    pub fn new(pages: Vec<String>) -> Self {
        let num_pages = pages.len();

        let temp_paginator = Self {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            sidebar_menu: ButtonStrip::new(0.0, 0.0, 0.0, 0.0),
            pages: Vec::new(),
            selected_page: 0,
            page_hidden: false,
            sidebar_w: 0.0,
            page_labels: pages.clone(),
            parent: None,
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

        Self {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            sidebar_menu,
            pages: pages_containers,
            selected_page: 0,
            page_hidden: false,
            sidebar_w,
            page_labels: pages,
            parent: None,
            on_page_changed_cb: None,
            just_clicked: None,
        }
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

impl Element for Paginator {
    fn rect(&self) -> (f32, f32, f32, f32) {
        (self.x, self.y, self.w, self.h)
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.x = x;
        self.y = y;
        self.w = w;
        self.h = h;

        let sidebar_w = self.sidebar_w();
        self.sidebar_menu.set_rect(x, y, sidebar_w, h);

        let page_x = x + sidebar_w;
        let page_w = (w - sidebar_w).max(0.0);
        for (i, plate) in self.pages.iter_mut().enumerate() {
            plate.set_rect(page_x, y, page_w, h);
            plate.visible = (i == self.selected_page) && !self.page_hidden;
        }
    }

    fn color(&self) -> [f32; 4] {
        colors::sidebar_bg_color()
    }

    fn as_ptr(&self) -> *mut (dyn Element + 'static) {
        self as *const Self as *mut Self as *mut (dyn Element + 'static)
    }

    fn as_ptr_mut(&mut self) -> *mut (dyn Element + 'static) {
        self as *mut Self as *mut (dyn Element + 'static)
    }

    fn parent(&self, _ctx: &UiContext) -> Option<*mut (dyn Element + 'static)> {
        self.parent
    }

    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>, _ctx: &mut UiContext) {
        self.parent = parent;
    }

    fn children(&self, _ctx: &UiContext) -> Vec<*mut (dyn Element + 'static)> {
        let mut childs = Vec::new();
        childs.push(&self.sidebar_menu as &dyn Element as *const (dyn Element + 'static) as *mut (dyn Element + 'static));
        for plate in &self.pages {
            childs.push(plate as &dyn Element as *const (dyn Element + 'static) as *mut (dyn Element + 'static));
        }
        childs
    }

    fn cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        let mut changed = false;
        if self.sidebar_menu.cursor_moved(px, py, ctx) {
            changed = true;
        }
        for (i, plate) in self.pages.iter_mut().enumerate() {
            if i == self.selected_page && !self.page_hidden {
                if plate.cursor_moved(px, py, ctx) {
                    changed = true;
                }
            }
        }
        changed
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        let mut changed = false;
        if self.sidebar_menu.mouse_input(button, state, px, py, ctx) {
            changed = true;
            if let Some(idx) = self.sidebar_menu.take_click() {
                self.set_selected_page(idx);
                self.just_clicked = Some(idx);
            }
        }
        for (i, plate) in self.pages.iter_mut().enumerate() {
            if i == self.selected_page && !self.page_hidden {
                if plate.mouse_input(button, state, px, py, ctx) {
                    changed = true;
                }
            }
        }
        changed
    }

    fn tick(&mut self, dt: f32, ctx: &mut UiContext) -> bool {
        let mut changed = false;
        if self.sidebar_menu.tick(dt, ctx) {
            changed = true;
        }
        for (i, plate) in self.pages.iter_mut().enumerate() {
            if i == self.selected_page && !self.page_hidden {
                if plate.tick(dt, ctx) {
                    changed = true;
                }
            }
        }
        changed
    }

    fn all_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        let c = self.color();
        if c[3] > 0.0 {
            quads.push((self.x, self.y, self.w, self.h, c));
        }
        quads.extend(self.sidebar_menu.all_quads(ctx));
        for (i, plate) in self.pages.iter().enumerate() {
            if i == self.selected_page && !self.page_hidden {
                quads.extend(plate.all_quads(ctx));
            }
        }
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        labels.extend(self.sidebar_menu.text_labels());
        for (i, plate) in self.pages.iter().enumerate() {
            if i == self.selected_page && !self.page_hidden {
                labels.extend(plate.text_labels());
            }
        }
        labels
    }

    fn text_labels_with_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<[f32; 4]>)> {
        let mut labels = Vec::new();
        labels.extend(self.sidebar_menu.text_labels_with_bounds(ctx));
        for (i, plate) in self.pages.iter().enumerate() {
            if i == self.selected_page && !self.page_hidden {
                labels.extend(plate.text_labels_with_bounds(ctx));
            }
        }
        labels
    }

    fn text_labels_with_font_and_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        let mut labels = Vec::new();
        labels.extend(self.sidebar_menu.text_labels_with_font_and_bounds(ctx));
        for (i, plate) in self.pages.iter().enumerate() {
            if i == self.selected_page && !self.page_hidden {
                labels.extend(plate.text_labels_with_font_and_bounds(ctx));
            }
        }
        labels
    }

    fn get_text_items(&self) -> Vec<(&glyphon::Buffer, f32, f32, glyphon::Color)> {
        let mut items = Vec::new();
        items.extend(self.sidebar_menu.get_text_items());
        for (i, plate) in self.pages.iter().enumerate() {
            if i == self.selected_page && !self.page_hidden {
                items.extend(plate.get_text_items());
            }
        }
        items
    }

    fn as_page_selector(&self) -> Option<&dyn PageSelector> {
        Some(self)
    }

    fn as_page_selector_mut(&mut self) -> Option<&mut dyn PageSelector> {
        Some(self)
    }

    fn as_menu_controller(&self) -> Option<&dyn MenuController> {
        Some(self)
    }

    fn as_menu_controller_mut(&mut self) -> Option<&mut dyn MenuController> {
        Some(self)
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        quads.extend(self.sidebar_menu.extra_quads());
        for (i, plate) in self.pages.iter().enumerate() {
            if i == self.selected_page && !self.page_hidden {
                quads.extend(plate.extra_quads());
            }
        }
        quads
    }

    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        let mut changed = false;
        if self.sidebar_menu.mouse_wheel(delta, px, py, ctx) {
            changed = true;
        }
        for (i, plate) in self.pages.iter_mut().enumerate() {
            if i == self.selected_page && !self.page_hidden {
                if plate.mouse_wheel(delta, px, py, ctx) {
                    changed = true;
                }
            }
        }
        changed
    }

    fn keyboard_input(&mut self, event: &KeyEvent, ctx: &mut UiContext) -> bool {
        let mut changed = false;
        if self.sidebar_menu.keyboard_input(event, ctx) {
            changed = true;
        }
        for (i, plate) in self.pages.iter_mut().enumerate() {
            if i == self.selected_page && !self.page_hidden {
                if plate.keyboard_input(event, ctx) {
                    changed = true;
                }
            }
        }
        changed
    }

    fn prepare_text(&mut self, fs: &mut glyphon::FontSystem) {
        self.sidebar_menu.prepare_text(fs);
        for (i, plate) in self.pages.iter_mut().enumerate() {
            if i == self.selected_page && !self.page_hidden {
                plate.prepare_text(fs);
            }
        }
    }

    fn highlight_quad(&self, ctx: &UiContext) -> Option<(f32, f32, f32, f32, [f32; 4])> {
        self.sidebar_menu.highlight_quad(ctx)
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

impl Drop for Paginator {
    fn drop(&mut self) {
        clear_widget_references(self);
    }
}
