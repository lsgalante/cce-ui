use crate::colors;
use crate::widget::*;
use crate::widget::input::ButtonStrip;
use crate::widget::display::TextLabel;
use super::plate::Plate;

pub struct Paginator {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    pub sidebar_menu: ButtonStrip,
    pub plates: Vec<Plate>,
    pub selected_page: usize,
    pub page_hidden: bool,
    pub sidebar_w: f32,
    pub pages: Vec<String>,
    parent: Option<*mut (dyn Element + 'static)>,
    pub on_page_changed_cb: Option<Box<dyn Fn(usize) + Send + Sync>>,
}

impl Paginator {
    pub fn new(sidebar_w: f32, pages: Vec<String>) -> Self {
        let num_pages = pages.len();

        let mut sidebar_menu = ButtonStrip::new(0.0, 0.0, sidebar_w, 0.0)
            .with_vertical(true)
            .with_buttons(pages.clone());
        if num_pages > 0 {
            sidebar_menu.set_selected(Some(0));
        }

        let mut plates = Vec::new();
        for _ in 0..num_pages {
            let mut plate = Plate::new(0.0, 0.0, 0.0, 0.0).with_draggable(false);
            plate.visible = false;
            plates.push(plate);
        }
        if num_pages > 0 {
            plates[0].visible = true;
        }

        Self {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            sidebar_menu,
            plates,
            selected_page: 0,
            page_hidden: false,
            sidebar_w,
            pages,
            parent: None,
            on_page_changed_cb: None,
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
        if page < self.plates.len() {
            self.selected_page = page;
            self.sidebar_menu.set_selected(Some(page));
            for (i, plate) in self.plates.iter_mut().enumerate() {
                plate.visible = i == page;
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
        self.pages = pages.clone();
        self.sidebar_menu.buttons = pages.clone();
        self.sidebar_menu.generate_rotated_labels();

        self.plates.clear();
        for _ in 0..pages.len() {
            let mut plate = Plate::new(0.0, 0.0, 0.0, 0.0).with_draggable(false);
            plate.visible = false;
            self.plates.push(plate);
        }
        if !self.plates.is_empty() {
            let idx = self.selected_page.min(self.plates.len() - 1);
            self.plates[idx].visible = true;
        }
    }

    fn set_pages_with_items(&mut self, pages: Vec<String>, _items: Vec<Vec<String>>) {
        self.set_pages(pages);
    }

    fn sidebar_w(&self) -> f32 {
        self.sidebar_w
    }

    fn set_sidebar_mode(&mut self, _enabled: bool) {}

    fn set_sidebar_label(&mut self, _label: Option<String>) {}

    fn add_widget_to_page(&mut self, page_idx: usize, widget: *mut (dyn Element + 'static), ctx: &mut UiContext) {
        if page_idx < self.plates.len() {
            self.plates[page_idx].add_child(widget, ctx);
            unsafe {
                (*widget).set_parent(Some(&mut self.plates[page_idx] as *mut _), ctx);
            }
        }
    }

    fn clear_page_widgets(&mut self, page_idx: usize, ctx: &mut UiContext) {
        if page_idx < self.plates.len() {
            self.plates[page_idx].clear_children(ctx);
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

        self.sidebar_menu.set_rect(x, y, self.sidebar_w, h);

        let page_x = x + self.sidebar_w;
        let page_w = (w - self.sidebar_w).max(0.0);
        for (i, plate) in self.plates.iter_mut().enumerate() {
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
        for plate in &self.plates {
            childs.push(plate as &dyn Element as *const (dyn Element + 'static) as *mut (dyn Element + 'static));
        }
        childs
    }

    fn cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        let mut changed = false;
        if self.sidebar_menu.cursor_moved(px, py, ctx) {
            changed = true;
        }
        for (i, plate) in self.plates.iter_mut().enumerate() {
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
            }
        }
        for (i, plate) in self.plates.iter_mut().enumerate() {
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
        for (i, plate) in self.plates.iter_mut().enumerate() {
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
        for (i, plate) in self.plates.iter().enumerate() {
            if i == self.selected_page && !self.page_hidden {
                quads.extend(plate.all_quads(ctx));
            }
        }
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        labels.extend(self.sidebar_menu.text_labels());
        for (i, plate) in self.plates.iter().enumerate() {
            if i == self.selected_page && !self.page_hidden {
                labels.extend(plate.text_labels());
            }
        }
        labels
    }

    fn text_labels_with_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<[f32; 4]>)> {
        let mut labels = Vec::new();
        labels.extend(self.sidebar_menu.text_labels_with_bounds(ctx));
        for (i, plate) in self.plates.iter().enumerate() {
            if i == self.selected_page && !self.page_hidden {
                labels.extend(plate.text_labels_with_bounds(ctx));
            }
        }
        labels
    }

    fn text_labels_with_font_and_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        let mut labels = Vec::new();
        labels.extend(self.sidebar_menu.text_labels_with_font_and_bounds(ctx));
        for (i, plate) in self.plates.iter().enumerate() {
            if i == self.selected_page && !self.page_hidden {
                labels.extend(plate.text_labels_with_font_and_bounds(ctx));
            }
        }
        labels
    }

    fn get_text_items(&self) -> Vec<(&glyphon::Buffer, f32, f32, glyphon::Color)> {
        let mut items = Vec::new();
        items.extend(self.sidebar_menu.get_text_items());
        for (i, plate) in self.plates.iter().enumerate() {
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

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        quads.extend(self.sidebar_menu.extra_quads());
        for (i, plate) in self.plates.iter().enumerate() {
            if i == self.selected_page && !self.page_hidden {
                quads.extend(plate.extra_quads());
            }
        }
        quads
    }
}

impl Drop for Paginator {
    fn drop(&mut self) {
        clear_widget_references(self);
    }
}
