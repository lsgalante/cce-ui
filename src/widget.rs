use winit::event::{ElementState, KeyEvent, MouseButton};
use winit::keyboard::{Key, NamedKey};

use crate::colors;

pub struct TextLabel {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
    pub color: [u8; 3],
}

pub trait Widget {
    fn rect(&self) -> (f32, f32, f32, f32);
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32);

    fn hit_test(&self, px: f32, py: f32) -> bool {
        let (x, y, w, h) = self.rect();
        px >= x && px <= x + w && py >= y && py <= y + h
    }

    fn cursor_moved(&mut self, _px: f32, _py: f32) -> bool { false }
    fn mouse_input(&mut self, _button: MouseButton, _state: ElementState, _px: f32, _py: f32) -> bool { false }

    fn set_hovered(&mut self, _hovered: bool) {}
    fn hovered(&self) -> bool { false }
    fn hover_highlight(&self) -> Option<[f32; 4]> {
        if self.hovered() { Some([1.0, 1.0, 1.0, 0.06]) } else { None }
    }

    fn color(&self) -> [f32; 4];

    fn is_dragging(&self) -> bool { false }
    fn drag_update(&mut self, _px: f32, _py: f32) -> bool { false }
    fn drag_begin(&mut self, _px: f32, _py: f32) {}
    fn drag_end(&mut self) {}
    fn take_click(&mut self) -> bool { false }
    fn draggable(&self) -> bool { false }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> { Vec::new() }
    fn text_labels(&self) -> Vec<TextLabel> { Vec::new() }
    fn value(&self) -> i32 { 0 }
    fn top_room(&self) -> f32 { 0.0 }

    fn set_drag_bounds(&mut self, _bx: f32, _by: f32, _bw: f32, _bh: f32) {}

    fn focus(&mut self) {}
    fn unfocus(&mut self) {}
    fn keyboard_input(&mut self, _event: &KeyEvent) -> bool { false }

    fn menu_click(&mut self) -> Option<(usize, usize)> { None }
    fn set_item_checked(&mut self, _menu_idx: usize, _item_idx: usize, _checked: bool) {}
    fn set_grid_snap(&mut self, _gx: f32, _gy: f32) {}
    fn set_node_name(&mut self, _name: &str) {}
    fn set_visible(&mut self, _visible: bool) {}
    fn visible(&self) -> bool { true }
    fn set_path(&mut self, _segments: &[String]) {}
    fn path_click(&mut self) -> Option<usize> { None }

    fn node_params(&self) -> Vec<(String, String)> { vec![] }
    fn set_display_params(&mut self, _params: &[(String, String)]) {}

    fn set_config_toggle(&mut self, _id: usize, _val: bool) {}
    fn take_config_toggle(&mut self) -> Option<(usize, bool)> { None }
    fn set_show_network_grid(&mut self, _show: bool) {}
    fn set_config_spin(&mut self, _id: usize, _val: f32) {}
    fn take_config_spin(&mut self) -> Option<(usize, f32)> { None }
    fn set_grid_sizes(&mut self, _gx: f32, _gy: f32) {}
    fn set_grid_origin(&mut self, _ox: f32, _oy: f32) {}
    fn set_palette_state(&mut self, _visible: bool, _query: &str, _items: &[String], _selected: usize) {}

    fn set_geom_visible(&mut self, _visible: bool) {}
    fn geom_visible(&self) -> bool { true }
    fn take_geom_toggle(&mut self) -> bool { false }
}

pub struct Header {
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
}

impl Header {
    pub fn new() -> Self { Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, hovered: false } }
}

impl Widget for Header {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { colors::HEADER_BG }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }
}

pub struct ContentBg {
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
    show_network_grid: bool,
    grid_size_x: f32,
    grid_size_y: f32,
    grid_origin_x: f32,
    grid_origin_y: f32,
}

impl ContentBg {
    pub fn new() -> Self {
        Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, hovered: false, show_network_grid: false, grid_size_x: 75.0, grid_size_y: 75.0, grid_origin_x: 0.0, grid_origin_y: 0.0 }
    }
}

impl Widget for ContentBg {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { colors::CONTENT_BG }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }
    fn hit_test(&self, _px: f32, _py: f32) -> bool { false }

    fn set_show_network_grid(&mut self, show: bool) { self.show_network_grid = show; }
    fn set_grid_sizes(&mut self, gx: f32, gy: f32) { self.grid_size_x = gx; self.grid_size_y = gy; }
    fn set_grid_origin(&mut self, ox: f32, oy: f32) { self.grid_origin_x = ox; self.grid_origin_y = oy; }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.show_network_grid || self.grid_size_x <= 0.0 || self.grid_size_y <= 0.0 {
            return vec![];
        }
        let mut quads = Vec::new();
        let grid_color = [0.25, 0.25, 0.32, 1.0];

        let start_y = self.y;
        let diff_y = start_y - self.grid_origin_y;
        let k_y = (diff_y / self.grid_size_y).floor();
        let mut y = self.grid_origin_y + k_y * self.grid_size_y;
        while y < self.y + self.h {
            if y >= self.y {
                quads.push((self.x, y, self.w, 1.0, grid_color));
            }
            y += self.grid_size_y;
        }

        let start_x = self.x;
        let diff_x = start_x - self.grid_origin_x;
        let k_x = (diff_x / self.grid_size_x).floor();
        let mut x = self.grid_origin_x + k_x * self.grid_size_x;
        while x < self.x + self.w {
            if x >= self.x {
                quads.push((x, self.y, 1.0, self.h, grid_color));
            }
            x += self.grid_size_x;
        }
        quads
    }
}

pub struct ViewportBg {
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
}

impl ViewportBg {
    pub fn new() -> Self { Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, hovered: false } }
}

impl Widget for ViewportBg {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { colors::VIEWPORT_BG }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }
    fn hit_test(&self, _px: f32, _py: f32) -> bool { false }
}

pub struct ParametersBg {
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
    display_params: Vec<(String, String)>,
}

impl ParametersBg {
    pub fn new() -> Self { Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, hovered: false, display_params: Vec::new() }
    }
}

impl Widget for ParametersBg {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { colors::PARAM_BG }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }
    fn hit_test(&self, _px: f32, _py: f32) -> bool { false }

    fn set_display_params(&mut self, params: &[(String, String)]) {
        self.display_params = params.to_vec();
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        self.display_params.iter().enumerate().map(|(i, (name, value))| {
            TextLabel {
                text: format!("{}: {}", name, value),
                x: self.x + 8.0,
                y: self.y + 30.0 + (i as f32) * 20.0,
                font_size: 12.0,
                color: [0xaa, 0xaa, 0xbb],
            }
        }).collect()
    }
}

pub struct Canvas {
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
}

impl Canvas {
    pub fn new() -> Self { Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, hovered: false } }
}

impl Widget for Canvas {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }
    fn hit_test(&self, _px: f32, _py: f32) -> bool { false }
}

const DROPDOWN_ITEM_H: f32 = 22.0;
const CHECKBOX_WIDTH: f32 = 16.0;

pub struct MenuBar {
    x: f32, y: f32, w: f32, h: f32,
    hovering: bool,
    pub title: String,
    pub menu_items: Vec<String>,
    pub vertical_items: Vec<String>,
    pub menu_dropdowns: Vec<Vec<String>>,
    menu_dropdown_checked: Vec<Vec<Option<bool>>>,
    hovered_menu: Option<usize>,
    open_menu: Option<usize>,
    hovered_dropdown: Option<usize>,
    clicked_dropdown: Option<(usize, usize)>,
    was_open: Option<usize>,
    pub vertical: bool,
}

impl MenuBar {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            x, y, w, h, hovering: false,
            title: String::new(),
            menu_items: Vec::new(),
            vertical_items: Vec::new(),
            menu_dropdowns: Vec::new(),
            menu_dropdown_checked: Vec::new(),
            hovered_menu: None,
            open_menu: None,
            hovered_dropdown: None,
            clicked_dropdown: None,
            was_open: None,
            vertical: false,
        }
    }

    pub fn with_title(mut self, title: &str) -> Self {
        self.title = title.to_string();
        self
    }

    pub fn with_item(mut self, label: &str, items: &[&str]) -> Self {
        self.menu_items.push(label.to_string());
        self.vertical_items.push(label.to_string());
        self.menu_dropdowns.push(items.iter().map(|s| s.to_string()).collect());
        self.menu_dropdown_checked.push(vec![None; items.len()]);
        self
    }

    pub fn with_item_vh(mut self, horizontal_label: &str, vertical_label: &str, items: &[&str]) -> Self {
        self.menu_items.push(horizontal_label.to_string());
        self.vertical_items.push(vertical_label.to_string());
        self.menu_dropdowns.push(items.iter().map(|s| s.to_string()).collect());
        self.menu_dropdown_checked.push(vec![None; items.len()]);
        self
    }

    pub fn with_vertical(mut self, vertical: bool) -> Self {
        self.vertical = vertical;
        self
    }

    fn item_y_vertical(&self, idx: usize) -> f32 {
        let mut y = 8.0;
        if !self.title.is_empty() {
            y += 24.0;
        }
        y + idx as f32 * 24.0
    }

    fn item_h_vertical(&self) -> f32 {
        24.0
    }

    fn dropdown_rect(&self, idx: usize) -> (f32, f32, f32, f32) {
        if self.vertical {
            let iy = self.item_y_vertical(idx);
            let dx = self.x + self.w;
            let dy = self.y + iy;
            let mut dw = 80.0f32;
            if let Some(items) = self.menu_dropdowns.get(idx) {
                for (i, item) in items.iter().enumerate() {
                    let mut tw = item.len() as f32 * 7.5 + 16.0;
                    if self.menu_dropdown_checked.get(idx)
                        .and_then(|m| m.get(i))
                        .is_some()
                    {
                        tw += CHECKBOX_WIDTH;
                    }
                    dw = dw.max(tw);
                }
            }
            let dh = self.menu_dropdowns.get(idx).map_or(0.0, |items| items.len() as f32 * DROPDOWN_ITEM_H);
            (dx, dy, dw, dh)
        } else {
            let ix = menu_item_x(&self.title, &self.menu_items, idx);
            let iw = menu_item_w(&self.menu_items, idx);
            let dx = self.x + ix;
            let dy = self.y + self.h;
            let mut dw = iw;
            if let Some(items) = self.menu_dropdowns.get(idx) {
                for (i, item) in items.iter().enumerate() {
                    let mut tw = item.len() as f32 * 7.5 + 16.0;
                    if self.menu_dropdown_checked.get(idx)
                        .and_then(|m| m.get(i))
                        .is_some()
                    {
                        tw += CHECKBOX_WIDTH;
                    }
                    dw = dw.max(tw);
                }
            }
            let dh = self.menu_dropdowns.get(idx).map_or(0.0, |items| items.len() as f32 * DROPDOWN_ITEM_H);
            (dx, dy, dw, dh)
        }
    }
}
impl Widget for MenuBar {
    fn rect(&self) -> (f32, f32, f32, f32) {
        if self.vertical {
            let total_h = if self.menu_items.is_empty() {
                self.h
            } else {
                let last_idx = self.menu_items.len() - 1;
                self.item_y_vertical(last_idx) + self.item_h_vertical()
            };
            (self.x, self.y, self.w, total_h)
        } else {
            (self.x, self.y, self.w, self.h)
        }
    }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { colors::PANEL_MENU_BG }
    fn set_hovered(&mut self, v: bool) { self.hovering = v; }
    fn hovered(&self) -> bool { self.hovering }

    fn hit_test(&self, px: f32, py: f32) -> bool {
        let (rx, ry, rw, rh) = self.rect();
        if px >= rx && px <= rx + rw && py >= ry && py <= ry + rh {
            return true;
        }
        if let Some(idx) = self.open_menu {
            let (dx, dy, dw, dh) = self.dropdown_rect(idx);
            if dh > 0.0 && px >= dx && px < dx + dw && py >= dy && py < dy + dh {
                return true;
            }
        }
        false
    }

    fn cursor_moved(&mut self, px: f32, py: f32) -> bool {
        self.was_open = None;
        let was = self.hovering;
        self.hovering = self.hit_test(px, py);
        let old_menu = self.hovered_menu;
        let old_dd = self.hovered_dropdown;
        self.hovered_menu = None;
        self.hovered_dropdown = None;

        if self.hovering {
            if self.vertical {
                if px >= self.x && px < self.x + self.w {
                    let ly = py - self.y;
                    for i in 0..self.menu_items.len() {
                        let iy = self.item_y_vertical(i);
                        let ih = self.item_h_vertical();
                        if ly >= iy && ly < iy + ih {
                            self.hovered_menu = Some(i);
                            break;
                        }
                    }
                }
            } else {
                if py >= self.y && py < self.y + self.h {
                    let lx = px - self.x;
                    for i in 0..self.menu_items.len() {
                        let ix = menu_item_x(&self.title, &self.menu_items, i);
                        if lx >= ix && lx < ix + menu_item_w(&self.menu_items, i) {
                            self.hovered_menu = Some(i);
                            break;
                        }
                    }
                }
            }
            if let Some(idx) = self.open_menu {
                let (dx, dy, dw, dh) = self.dropdown_rect(idx);
                if px >= dx && px < dx + dw && py >= dy && py < dy + dh {
                    let di = ((py - dy) / DROPDOWN_ITEM_H) as usize;
                    if let Some(items) = self.menu_dropdowns.get(idx) {
                        if di < items.len() {
                            self.hovered_dropdown = Some(di);
                        }
                    }
                }
            }
        }

        was != self.hovering || old_menu != self.hovered_menu || old_dd != self.hovered_dropdown
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button != MouseButton::Left || state != ElementState::Pressed { return false; }
        if !self.hit_test(px, py) { return false; }

        if self.vertical {
            if px >= self.x && px < self.x + self.w {
                let ly = py - self.y;
                for i in 0..self.menu_items.len() {
                    let iy = self.item_y_vertical(i);
                    let ih = self.item_h_vertical();
                    if ly >= iy && ly < iy + ih {
                        if self.was_open == Some(i) {
                            self.was_open = None;
                        } else {
                            self.open_menu = Some(i);
                            self.was_open = None;
                        }
                        return true;
                    }
                }
                self.was_open = None;
                self.open_menu = None;
                return true;
            }
        } else {
            if py >= self.y && py < self.y + self.h {
                let lx = px - self.x;
                for i in 0..self.menu_items.len() {
                    let ix = menu_item_x(&self.title, &self.menu_items, i);
                    if lx >= ix && lx < ix + menu_item_w(&self.menu_items, i) {
                        if self.was_open == Some(i) {
                            self.was_open = None;
                        } else {
                            self.open_menu = Some(i);
                            self.was_open = None;
                        }
                        return true;
                    }
                }
                self.was_open = None;
                self.open_menu = None;
                return true;
            }
        }

        if let Some(idx) = self.open_menu {
            let (dx, dy, dw, dh) = self.dropdown_rect(idx);
            if px >= dx && px < dx + dw && py >= dy && py < dy + dh {
                let di = ((py - dy) / DROPDOWN_ITEM_H) as usize;
                if let Some(items) = self.menu_dropdowns.get(idx) {
                    if di < items.len() {
                        self.clicked_dropdown = Some((idx, di));
                        self.open_menu = None;
                        return true;
                    }
                }
            }
        }

        false
    }

    fn unfocus(&mut self) {
        self.was_open = self.open_menu.take();
    }

    fn menu_click(&mut self) -> Option<(usize, usize)> {
        self.clicked_dropdown.take()
    }

    fn set_item_checked(&mut self, menu_idx: usize, item_idx: usize, checked: bool) {
        if let Some(menu) = self.menu_dropdown_checked.get_mut(menu_idx) {
            if item_idx < menu.len() {
                menu[item_idx] = Some(checked);
            }
        }
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        if let Some(idx) = self.hovered_menu {
            if self.vertical {
                let iy = self.item_y_vertical(idx);
                let ih = self.item_h_vertical();
                quads.push((self.x, self.y + iy, self.w, ih, colors::PANEL_MENU_HOVER));
            } else {
                let ix = menu_item_x(&self.title, &self.menu_items, idx);
                quads.push((self.x + ix, self.y, menu_item_w(&self.menu_items, idx), self.h, colors::PANEL_MENU_HOVER));
            }
        }
        if let Some(idx) = self.open_menu {
            let (dx, dy, dw, dh) = self.dropdown_rect(idx);
            if dh > 0.0 {
                quads.push((dx, dy, dw, dh, colors::PANEL_MENU_BG));
                if let Some(di) = self.hovered_dropdown {
                    quads.push((dx, dy + di as f32 * DROPDOWN_ITEM_H, dw, DROPDOWN_ITEM_H, colors::PANEL_MENU_HOVER));
                }
            }
        }
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        if !self.title.is_empty() {
            labels.push(TextLabel {
                text: self.title.clone(),
                x: self.x + 8.0,
                y: self.y + 7.0,
                font_size: 12.0,
                color: [0xaa, 0xaa, 0xbb],
            });
        }
        if self.vertical {
            for (i, item) in self.vertical_items.iter().enumerate() {
                let iy = self.item_y_vertical(i);
                labels.push(TextLabel {
                    text: item.clone(),
                    x: self.x + 8.0,
                    y: self.y + iy + 5.0,
                    font_size: 12.0,
                    color: [0xcc, 0xcc, 0xd4],
                });
            }
        } else {
            for (i, item) in self.menu_items.iter().enumerate() {
                labels.push(TextLabel {
                    text: item.clone(),
                    x: self.x + menu_item_x(&self.title, &self.menu_items, i),
                    y: self.y + 7.0,
                    font_size: 12.0,
                    color: [0xcc, 0xcc, 0xd4],
                });
            }
        }
        if let Some(idx) = self.open_menu {
            if let Some(items) = self.menu_dropdowns.get(idx) {
                let (dx, dy, _, _) = self.dropdown_rect(idx);
                for (i, item) in items.iter().enumerate() {
                    let checked = self.menu_dropdown_checked.get(idx)
                        .and_then(|m| m.get(i))
                        .and_then(|&v| v);
                    let prefix = match checked {
                        Some(true) => "\u{2713} ",
                        Some(false) => "  ",
                        None => "",
                    };
                    labels.push(TextLabel {
                        text: format!("{}{}", prefix, item),
                        x: dx + 8.0,
                        y: dy + i as f32 * DROPDOWN_ITEM_H + 5.0,
                        font_size: 12.0,
                        color: [0xcc, 0xcc, 0xd4],
                    });
                }
            }
        }
        labels
    }
}

pub enum ButtonKind {
    Primary,
    Reset,
}

pub struct Button {
    x: f32, y: f32, w: f32, h: f32,
    hovering: bool,
    pressed: bool,
    just_clicked: bool,
    kind: ButtonKind,
}

impl Button {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h, hovering: false, pressed: false, just_clicked: false, kind: ButtonKind::Primary }
    }

    pub fn new_reset(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h, hovering: false, pressed: false, just_clicked: false, kind: ButtonKind::Reset }
    }
}

impl Widget for Button {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn set_hovered(&mut self, v: bool) { self.hovering = v; }
    fn hovered(&self) -> bool { self.hovering }
    fn color(&self) -> [f32; 4] {
        match self.kind {
            ButtonKind::Primary => {
                if self.pressed { colors::BUTTON_PRESS }
                else if self.hovering { colors::BUTTON_HOVER }
                else { colors::BUTTON_IDLE }
            }
            ButtonKind::Reset => {
                if self.pressed { colors::RESET_BTN_PRESS }
                else if self.hovering { colors::RESET_BTN_HOVER }
                else { colors::RESET_BTN_IDLE }
            }
        }
    }

    fn cursor_moved(&mut self, px: f32, py: f32) -> bool {
        let was = self.hovering;
        self.hovering = self.hit_test(px, py);
        was != self.hovering
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button != MouseButton::Left { return false; }
        match state {
            ElementState::Pressed => {
                if self.hit_test(px, py) {
                    self.pressed = true;
                    return true;
                }
            }
            ElementState::Released => {
                if self.pressed && self.hit_test(px, py) {
                    self.just_clicked = true;
                }
                let was = self.pressed;
                self.pressed = false;
                return was;
            }
        }
        false
    }

    fn take_click(&mut self) -> bool {
        if self.just_clicked { self.just_clicked = false; true } else { false }
    }
}

pub enum PageButton {
    Active,
    Inactive,
}

pub struct Sidebar {
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
}

impl Sidebar {
    pub fn new(w: f32) -> Self { Self { x: 0.0, y: 0.0, w, h: 0.0, hovered: false } }
}

impl Widget for Sidebar {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { colors::SIDEBAR_BG }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }
}

pub struct Panel {
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
    dragging: bool,
    drag_ox: f32, drag_oy: f32,
    drag_start_x: f32, drag_start_y: f32,
    bounds: Option<(f32, f32, f32, f32)>,
}

impl Panel {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h, hovered: false, dragging: false, drag_ox: 0.0, drag_oy: 0.0, drag_start_x: 0.0, drag_start_y: 0.0, bounds: None }
    }

    pub fn set_bounds(&mut self, bx: f32, by: f32, bw: f32, bh: f32) {
        self.bounds = Some((bx, by, bw, bh));
    }
}

impl Widget for Panel {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { if self.dragging { colors::PANEL_DRAG } else { colors::PANEL_IDLE } }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }

    fn set_drag_bounds(&mut self, bx: f32, by: f32, bw: f32, bh: f32) {
        self.bounds = Some((bx, by, bw, bh));
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button != MouseButton::Left { return false; }
        match state {
            ElementState::Pressed => {
                if self.hit_test(px, py) {
                    self.drag_begin(px, py);
                    return true;
                }
            }
            ElementState::Released => {
                if self.dragging { self.drag_end(); return true; }
            }
        }
        false
    }

    fn is_dragging(&self) -> bool { self.dragging }
    fn draggable(&self) -> bool { true }

    fn drag_update(&mut self, px: f32, py: f32) -> bool {
        let nx = px - self.drag_ox;
        let ny = py - self.drag_oy;
        let (nx, ny) = if let Some((bx, by, bw, bh)) = self.bounds {
            (nx.clamp(bx, bx + bw - self.w), ny.clamp(by, by + bh - self.h))
        } else {
            (nx, ny)
        };
        if (nx - self.x).abs() > 0.01 || (ny - self.y).abs() > 0.01 {
            self.x = nx;
            self.y = ny;
            return true;
        }
        false
    }

    fn drag_begin(&mut self, px: f32, py: f32) {
        self.dragging = true;
        self.drag_ox = px - self.x;
        self.drag_oy = py - self.y;
        self.drag_start_x = self.x;
        self.drag_start_y = self.y;
    }

    fn drag_end(&mut self) { self.dragging = false; }
}

pub struct Node {
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
    selected: bool,
    dragging: bool,
    drag_ox: f32, drag_oy: f32,
    bounds: Option<(f32, f32, f32, f32)>,
    grid_snap_x: f32,
    grid_snap_y: f32,
    grid_origin_x: f32,
    grid_origin_y: f32,
    name: String,
    pub parameters: Vec<(String, String)>,
    geom_visible: bool,
    geom_toggled: bool,
    toggle_hovered: bool,
}

impl Node {
    pub fn new(x: f32, y: f32, w: f32, h: f32, name: &str) -> Self {
        Self {
            x, y, w, h,
            hovered: false, selected: false, dragging: false, drag_ox: 0.0, drag_oy: 0.0,
            bounds: None, grid_snap_x: 0.0, grid_snap_y: 0.0,
            grid_origin_x: 0.0, grid_origin_y: 0.0,
            name: name.to_string(), parameters: Vec::new(),
            geom_visible: true, geom_toggled: false, toggle_hovered: false,
        }
    }

    pub fn with_params(mut self, params: &[(&str, &str)]) -> Self {
        self.parameters = params.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        self
    }

    pub fn with_grid_snap(mut self, gx: f32, gy: f32) -> Self {
        self.grid_snap_x = gx;
        self.grid_snap_y = gy;
        self
    }

    fn toggle_rect(&self) -> (f32, f32, f32, f32) {
        (self.x + self.w - 30.0, self.y + (self.h - 18.0) / 2.0, 18.0, 18.0)
    }
}

impl Widget for Node {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] {
        if self.dragging { colors::NODE_DRAG }
        else if self.selected { colors::NODE_SELECTED }
        else { colors::NODE_IDLE }
    }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }

    fn focus(&mut self) { self.selected = true; }
    fn unfocus(&mut self) { self.selected = false; }

    fn node_params(&self) -> Vec<(String, String)> { self.parameters.clone() }

    fn text_labels(&self) -> Vec<TextLabel> {
        vec![TextLabel {
            text: self.name.clone(),
            x: self.x + 8.0,
            y: self.y + (self.h - 12.0) / 2.0,
            font_size: 14.0,
            color: [0xcc, 0xcc, 0xd4],
        }]
    }

    fn set_drag_bounds(&mut self, bx: f32, by: f32, bw: f32, bh: f32) {
        self.bounds = Some((bx, by, bw, bh));
    }

    fn cursor_moved(&mut self, px: f32, py: f32) -> bool {
        let was_hovered = self.hovered;
        self.hovered = self.hit_test(px, py);

        let was_toggle_hovered = self.toggle_hovered;
        let (tx, ty, tw, th) = self.toggle_rect();
        self.toggle_hovered = px >= tx && px < tx + tw && py >= ty && py < ty + th;

        was_hovered != self.hovered || was_toggle_hovered != self.toggle_hovered
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button != MouseButton::Left { return false; }
        match state {
            ElementState::Pressed => {
                if self.hit_test(px, py) {
                    let (tx, ty, tw, th) = self.toggle_rect();
                    if px >= tx && px < tx + tw && py >= ty && py < ty + th {
                        self.geom_visible = !self.geom_visible;
                        self.geom_toggled = true;
                        return true;
                    }
                    self.drag_begin(px, py);
                    return true;
                }
            }
            ElementState::Released => {
                if self.dragging { self.drag_end(); return true; }
            }
        }
        false
    }

    fn is_dragging(&self) -> bool { self.dragging }
    fn draggable(&self) -> bool { !self.toggle_hovered }

    fn set_grid_snap(&mut self, gx: f32, gy: f32) { self.grid_snap_x = gx; self.grid_snap_y = gy; }
    fn set_grid_origin(&mut self, ox: f32, oy: f32) { self.grid_origin_x = ox; self.grid_origin_y = oy; }
    fn set_node_name(&mut self, name: &str) { self.name = name.to_string(); }

    fn drag_update(&mut self, px: f32, py: f32) -> bool {
        let nx = px - self.drag_ox;
        let ny = py - self.drag_oy;
        let (nx, ny) = if let Some((bx, by, bw, bh)) = self.bounds {
            (nx.clamp(bx, bx + bw - self.w), ny.clamp(by, by + bh - self.h))
        } else {
            (nx, ny)
        };
        let nx = if self.grid_snap_x > 0.0 {
            let relative = nx - self.grid_origin_x;
            let snapped = (relative / self.grid_snap_x).round() * self.grid_snap_x;
            snapped + self.grid_origin_x
        } else { nx };
        let ny = if self.grid_snap_y > 0.0 {
            let relative = ny - self.grid_origin_y;
            let snapped = (relative / self.grid_snap_y).round() * self.grid_snap_y;
            snapped + self.grid_origin_y
        } else { ny };
        if (nx - self.x).abs() > 0.01 || (ny - self.y).abs() > 0.01 {
            self.x = nx;
            self.y = ny;
            return true;
        }
        false
    }

    fn drag_begin(&mut self, px: f32, py: f32) {
        self.dragging = true;
        self.drag_ox = px - self.x;
        self.drag_oy = py - self.y;
    }

    fn drag_end(&mut self) { self.dragging = false; }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let (tx, ty, tw, th) = self.toggle_rect();
        let bg_color = if self.toggle_hovered {
            colors::TOGGLE_HOVER
        } else {
            colors::TOGGLE_OFF
        };

        let mut quads = vec![
            (tx, ty, tw, th, bg_color)
        ];

        if self.geom_visible {
            let inset = 3.0;
            quads.push((tx + inset, ty + inset, tw - inset * 2.0, th - inset * 2.0, colors::TOGGLE_ON));
        }

        quads
    }

    fn set_geom_visible(&mut self, visible: bool) { self.geom_visible = visible; }
    fn geom_visible(&self) -> bool { self.geom_visible }
    fn take_geom_toggle(&mut self) -> bool { std::mem::take(&mut self.geom_toggled) }
}

fn menu_item_x(title: &str, menu_items: &[String], idx: usize) -> f32 {
    let mut x = 8.0;
    if !title.is_empty() {
        x += title.len() as f32 * 7.5 + 24.0;
    }
    for i in 0..idx {
        x += menu_items[i].len() as f32 * 7.5 + 16.0;
    }
    x
}

fn menu_item_w(menu_items: &[String], idx: usize) -> f32 {
    menu_items[idx].len() as f32 * 7.5 + 16.0
}

pub struct Checkbox {
    x: f32, y: f32, w: f32, h: f32,
    hovering: bool,
    checked: bool,
    just_clicked: bool,
}

impl Checkbox {
    pub fn new() -> Self {
        Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, hovering: false, checked: false, just_clicked: false }
    }
}

impl Widget for Checkbox {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { colors::CHECKBOX_BG }
    fn set_hovered(&mut self, v: bool) { self.hovering = v; }
    fn hovered(&self) -> bool { self.hovering }

    fn cursor_moved(&mut self, px: f32, py: f32) -> bool {
        let was = self.hovering;
        self.hovering = self.hit_test(px, py);
        was != self.hovering
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button != MouseButton::Left { return false; }
        match state {
            ElementState::Released => {
                if self.hit_test(px, py) {
                    self.checked = !self.checked;
                    self.just_clicked = true;
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    fn take_click(&mut self) -> bool {
        if self.just_clicked { self.just_clicked = false; true } else { false }
    }
}

#[derive(Debug, Clone)]
pub struct Toggle {
    x: f32, y: f32, w: f32, h: f32,
    hovering: bool,
    toggled: bool,
    just_toggled: bool,
    label: Option<String>,
}

impl Toggle {
    pub fn new() -> Self {
        Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, hovering: false, toggled: false, just_toggled: false, label: None }
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.label = Some(label.to_string());
        self
    }

    pub fn set_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    pub fn set_toggled(&mut self, v: bool) {
        self.toggled = v;
    }
}

impl Widget for Toggle {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { if self.toggled { colors::TOGGLE_ON } else { colors::TOGGLE_OFF } }
    fn set_hovered(&mut self, v: bool) { self.hovering = v; }
    fn hovered(&self) -> bool { self.hovering }

    fn top_room(&self) -> f32 { 0.0 }

    fn cursor_moved(&mut self, px: f32, py: f32) -> bool {
        let was = self.hovering;
        self.hovering = self.hit_test(px, py);
        was != self.hovering
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button != MouseButton::Left { return false; }
        match state {
            ElementState::Released => {
                if self.hit_test(px, py) {
                    self.toggled = !self.toggled;
                    self.just_toggled = true;
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        let bg = if self.toggled { colors::TOGGLE_ON } else { colors::TOGGLE_OFF };
        quads.push((self.x, self.y, self.w, self.h, bg));
        if self.hovering && self.label.is_some() {
            quads.push((self.x - 87.0, self.y, 87.0 + self.w, self.h, [1.0, 1.0, 1.0, 0.06]));
        }
        quads
    }

    fn take_click(&mut self) -> bool {
        if self.just_toggled { self.just_toggled = false; true } else { false }
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        if let Some(ref label) = self.label {
            vec![TextLabel {
                text: label.clone(),
                x: self.x - 85.0,
                y: self.y + (self.h - 12.0) / 2.0,
                font_size: 12.0,
                color: [0x83, 0x83, 0x8a],
            }]
        } else {
            Vec::new()
        }
    }
}

pub struct Slider {
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
    dragging: bool,
    value: f32,
    drag_offset: f32,
}

impl Slider {
    pub fn new() -> Self {
        Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, hovered: false, dragging: false, value: 0.5, drag_offset: 0.0 }
    }
}

impl Widget for Slider {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { colors::SLIDER_TRACK }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }

    fn draggable(&self) -> bool { true }
    fn is_dragging(&self) -> bool { self.dragging }

    fn drag_update(&mut self, px: f32, _py: f32) -> bool {
        let (sx, _, sw, _) = self.rect();
        let thumb_size = self.h * 0.9;
        let range = sw - thumb_size;
        let raw = (px - self.drag_offset - sx) / range;
        let new_val = raw.clamp(0.0, 1.0);
        if (new_val - self.value).abs() > 0.001 {
            self.value = new_val;
            return true;
        }
        false
    }

    fn drag_begin(&mut self, px: f32, _py: f32) {
        self.dragging = true;
        let thumb_size = self.h * 0.9;
        let thumb_x = self.x + self.value * (self.w - thumb_size);
        self.drag_offset = px - thumb_x;
    }

    fn drag_end(&mut self) { self.dragging = false; }
}

pub struct ProgressBar {
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
    value: f32,
}

impl ProgressBar {
    pub fn new(value: f32) -> Self {
        Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, hovered: false, value }
    }
}

impl Widget for ProgressBar {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { colors::PROGRESS_BG }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }
}

pub struct StatusBar {
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
}

impl StatusBar {
    pub fn new() -> Self { Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, hovered: false } }
}

impl Widget for StatusBar {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { colors::STATUS_BG }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }
}

pub struct Splitter {
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
    dragging: bool,
    drag_ox: f32,
}

impl Splitter {
    pub fn new(w: f32) -> Self {
        Self { x: 0.0, y: 0.0, w, h: 0.0, hovered: false, dragging: false, drag_ox: 0.0 }
    }
}

impl Widget for Splitter {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] {
        if self.dragging { colors::SPLITTER_DRAG }
        else if self.hovered { colors::SPLITTER_HOVER }
        else { colors::SPLITTER_IDLE }
    }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }

    fn cursor_moved(&mut self, px: f32, py: f32) -> bool {
        let was = self.hovered;
        self.hovered = self.hit_test(px, py);
        was != self.hovered
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button != MouseButton::Left { return false; }
        match state {
            ElementState::Pressed => {
                if self.hit_test(px, py) {
                    self.drag_begin(px, py);
                    return true;
                }
            }
            ElementState::Released => {
                if self.dragging { self.drag_end(); return true; }
            }
        }
        false
    }

    fn is_dragging(&self) -> bool { self.dragging }
    fn draggable(&self) -> bool { true }

    fn drag_update(&mut self, px: f32, _py: f32) -> bool {
        let new_x = px - self.drag_ox;
        if (new_x - self.x).abs() > 0.5 {
            self.x = new_x;
            return true;
        }
        false
    }

    fn drag_begin(&mut self, px: f32, _py: f32) {
        self.dragging = true;
        self.drag_ox = px - self.x;
    }

    fn drag_end(&mut self) { self.dragging = false; }
}

#[derive(Debug, Clone)]
pub struct Spinbox {
    x: f32, y: f32, w: f32, h: f32,
    pub value: i32,
    min: i32, max: i32, step: i32,
    editing: bool,
    edit_buffer: String,
    hovered: bool,
    hover_dec: bool,
    hover_inc: bool,
    label: Option<String>,
    unit: Option<String>,
}

impl Spinbox {
    pub fn new(value: i32, min: i32, max: i32, step: i32) -> Self {
        Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, value, min, max, step, editing: false, edit_buffer: String::new(), hovered: false, hover_dec: false, hover_inc: false, label: None, unit: None }
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.label = Some(label.to_string());
        self
    }

    pub fn set_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    pub fn with_unit(mut self, unit: &str) -> Self {
        self.unit = Some(unit.to_string());
        self
    }

    pub fn set_unit(&mut self, unit: &str) {
        self.unit = Some(unit.to_string());
    }
}

impl Widget for Spinbox {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { colors::SPINBOX_BG }
    fn value(&self) -> i32 { self.value }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }

    fn top_room(&self) -> f32 { if self.label.is_some() { 16.0 } else { 0.0 } }

    fn cursor_moved(&mut self, px: f32, _py: f32) -> bool {
        let was = self.hovered;
        self.hovered = self.hit_test(px, _py);
        if !self.hovered {
            let changed = self.hover_dec || self.hover_inc;
            self.hover_dec = false;
            self.hover_inc = false;
            return changed || was != self.hovered;
        }
        let split = self.x + self.w * 0.55;
        let hd = px >= split && px < split + self.w * 0.225;
        let hi = px >= split + self.w * 0.225;
        let changed = hd != self.hover_dec || hi != self.hover_inc;
        self.hover_dec = hd;
        self.hover_inc = hi;
        changed || was != self.hovered
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button != MouseButton::Left { return false; }
        if !self.hit_test(px, py) { return false; }
        match state {
            ElementState::Pressed => {
                let split = self.x + self.w * 0.55;
                if px >= split && px < split + self.w * 0.225 {
                    self.value = (self.value - self.step).max(self.min);
                    true
                } else if px >= split + self.w * 0.225 {
                    self.value = (self.value + self.step).min(self.max);
                    true
                } else if px < split {
                    self.editing = true;
                    self.edit_buffer = self.value.to_string();
                    true
                } else {
                    false
                }
            }
            ElementState::Released => {
                false
            }
        }
    }

    fn focus(&mut self) {
        self.editing = true;
        self.edit_buffer = self.value.to_string();
    }

    fn unfocus(&mut self) {
        if self.editing {
            self.editing = false;
            if let Ok(val) = self.edit_buffer.parse::<i32>() {
                self.value = val.clamp(self.min, self.max);
            }
        }
    }

    fn keyboard_input(&mut self, event: &KeyEvent) -> bool {
        if !self.editing { return false; }
        if event.state != ElementState::Pressed { return false; }
        match &event.logical_key {
            Key::Named(NamedKey::Backspace) => {
                self.edit_buffer.pop();
                true
            }
            Key::Named(NamedKey::Enter) => {
                if let Ok(val) = self.edit_buffer.parse::<i32>() {
                    self.value = val.clamp(self.min, self.max);
                }
                self.editing = false;
                true
            }
            Key::Named(NamedKey::Escape) => {
                self.editing = false;
                true
            }
            _ => {
                if let Some(text) = &event.text {
                    if !event.repeat {
                        for ch in text.chars() {
                            match ch {
                                '-' if self.edit_buffer.is_empty() => self.edit_buffer.push('-'),
                                '0'..='9' => self.edit_buffer.push(ch),
                                _ => {}
                            }
                        }
                    }
                }
                true
            }
        }
    }

    fn hover_highlight(&self) -> Option<[f32; 4]> {
        None // rendered in extra_quads to cover label area
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        if self.hovered {
            let (hy, hh) = if self.label.is_some() {
                (self.y - 20.0, self.h + 20.0)
            } else {
                (self.y, self.h)
            };
            quads.push((self.x, hy, self.w, hh, [1.0, 1.0, 1.0, 0.06]));
        }
        let split = self.x + self.w * 0.55;
        let btn_w = self.w * 0.225;
        let inc_col = if self.hover_inc { colors::SPINBOX_BUTTON_HOVER } else { colors::SPINBOX_BUTTON };
        let dec_col = if self.hover_dec { colors::SPINBOX_BUTTON_HOVER } else { colors::SPINBOX_BUTTON };
        quads.push((self.x, self.y, self.w * 0.55, self.h, colors::SPINBOX_DISPLAY));
        quads.push((split, self.y, btn_w, self.h, dec_col));
        quads.push((split + btn_w, self.y, btn_w, self.h, inc_col));
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        if let Some(ref label) = self.label {
            labels.push(TextLabel {
                text: label.clone(),
                x: self.x + 4.0,
                y: self.y - 16.0,
                font_size: 12.0,
                color: [0x83, 0x83, 0x8a],
            });
        }
        let value_text = if self.editing { self.edit_buffer.clone() } else { self.value.to_string() };
        labels.push(TextLabel {
            text: value_text,
            x: self.x + 4.0,
            y: self.y + 2.0,
            font_size: 14.0,
            color: [0xcc, 0xcc, 0xd4],
        });
        if let Some(ref unit) = self.unit {
            labels.push(TextLabel {
                text: unit.clone(),
                x: self.x + 4.0 + 36.0,
                y: self.y + 3.0,
                font_size: 11.0,
                color: [0x73, 0x73, 0x7a],
            });
        }
        labels.push(TextLabel {
            text: "-".to_string(),
            x: self.x + self.w * 0.6625 - 4.0,
            y: self.y + 2.0,
            font_size: 12.0,
            color: [0xcc, 0xcc, 0xd4],
        });
        labels.push(TextLabel {
            text: "+".to_string(),
            x: self.x + self.w * 0.8875 - 4.0,
            y: self.y + 2.0,
            font_size: 12.0,
            color: [0xcc, 0xcc, 0xd4],
        });
        labels
    }
}

#[derive(Debug, Clone)]
pub struct ColorSelector {
    x: f32, y: f32, w: f32, h: f32,
    pub color: [u8; 3],
    just_clicked: bool,
    hovered: bool,
    editing: bool,
    edit_buffer: String,
    label: Option<String>,
}

impl ColorSelector {
    pub fn new(color: [u8; 3]) -> Self {
        Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, color, just_clicked: false, hovered: false, editing: false, edit_buffer: String::new(), label: None }
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.label = Some(label.to_string());
        self
    }
}

impl Widget for ColorSelector {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }

    fn top_room(&self) -> f32 { if self.label.is_some() { 16.0 } else { 0.0 } }

    fn color(&self) -> [f32; 4] {
        [self.color[0] as f32 / 255.0, self.color[1] as f32 / 255.0, self.color[2] as f32 / 255.0, 1.0]
    }

    fn cursor_moved(&mut self, px: f32, py: f32) -> bool {
        let was = self.hovered;
        self.hovered = self.hit_test(px, py);
        was != self.hovered
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button != MouseButton::Left { return false; }
        if state != ElementState::Pressed { return false; }
        if !self.hit_test(px, py) { return false; }
        if px >= self.x + self.w * 0.65 {
            self.just_clicked = true;
            return true;
        }
        self.focus();
        true
    }

    fn take_click(&mut self) -> bool {
        if self.just_clicked { self.just_clicked = false; true } else { false }
    }

    fn focus(&mut self) {
        self.editing = true;
        self.edit_buffer = format!("#{:02x}{:02x}{:02x}", self.color[0], self.color[1], self.color[2]);
    }

    fn unfocus(&mut self) {
        if self.editing {
            self.editing = false;
            if let Some(c) = parse_hex(&self.edit_buffer) {
                self.color = c;
            }
        }
    }

    fn keyboard_input(&mut self, event: &KeyEvent) -> bool {
        if !self.editing { return false; }
        if event.state != ElementState::Pressed { return false; }
        match &event.logical_key {
            Key::Named(NamedKey::Backspace) => {
                self.edit_buffer.pop();
                true
            }
            Key::Named(NamedKey::Enter) => {
                if let Some(c) = parse_hex(&self.edit_buffer) {
                    self.color = c;
                }
                self.editing = false;
                true
            }
            Key::Named(NamedKey::Escape) => {
                self.editing = false;
                true
            }
            _ => {
                if let Some(text) = &event.text {
                    if !event.repeat {
                        for ch in text.chars() {
                            match ch {
                                '#' if self.edit_buffer.is_empty() => self.edit_buffer.push('#'),
                                '0'..='9' | 'a'..='f' | 'A'..='F' => {
                                    if self.edit_buffer.len() < 7 { self.edit_buffer.push(ch.to_ascii_lowercase()); }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                true
            }
        }
    }

    fn hover_highlight(&self) -> Option<[f32; 4]> {
        None // rendered in extra_quads to cover label area
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        if self.hovered {
            let (hy, hh) = if self.label.is_some() {
                (self.y - 20.0, self.h + 20.0)
            } else {
                (self.y, self.h)
            };
            quads.push((self.x, hy, self.w, hh, [1.0, 1.0, 1.0, 0.06]));
        }
        let pick_x = self.x + self.w * 0.65;
        let pick_w = self.w * 0.35;
        let (r, g, b, _) = (self.color[0] as f32 / 255.0, self.color[1] as f32 / 255.0, self.color[2] as f32 / 255.0, 1.0);
        quads.push((pick_x, self.y, pick_w, self.h, [r, g, b, 1.0]));
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        if let Some(ref label) = self.label {
            labels.push(TextLabel {
                text: label.clone(),
                x: self.x + 4.0,
                y: self.y - 16.0,
                font_size: 12.0,
                color: [0x83, 0x83, 0x8a],
            });
        }
        let hex = if self.editing { self.edit_buffer.clone() } else { format!("#{:02x}{:02x}{:02x}", self.color[0], self.color[1], self.color[2]) };
        labels.push(TextLabel {
            text: hex,
            x: self.x + 4.0,
            y: self.y + 3.0,
            font_size: 12.0,
            color: [0xcc, 0xcc, 0xd4],
        });
        labels
    }
}

const BREADCRUMB_PADDING: f32 = 8.0;
const SEGMENT_GAP: f32 = 4.0;

pub struct Breadcrumb {
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
    path: Vec<String>,
    hovered_seg: Option<usize>,
    clicked_seg: Option<usize>,
}

impl Breadcrumb {
    pub fn new() -> Self {
        Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, hovered: false,
               path: Vec::new(), hovered_seg: None, clicked_seg: None }
    }

    fn seg_at(&self, px: f32) -> Option<usize> {
        let mut cx = self.x + BREADCRUMB_PADDING;
        for (i, seg) in self.path.iter().enumerate() {
            let w = seg.len() as f32 * 7.5;
            if px >= cx && px < cx + w {
                return Some(i);
            }
            cx += w + SEGMENT_GAP;
        }
        None
    }
}

impl Widget for Breadcrumb {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { [0.10, 0.10, 0.14, 1.0] }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }

    fn cursor_moved(&mut self, px: f32, py: f32) -> bool {
        let was = self.hovered;
        self.hovered = self.hit_test(px, py);
        let old = self.hovered_seg;
        self.hovered_seg = if self.hovered { self.seg_at(px) } else { None };
        was != self.hovered || old != self.hovered_seg
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, _py: f32) -> bool {
        if button != MouseButton::Left || state != ElementState::Pressed { return false; }
        if let Some(i) = self.seg_at(px) {
            if i < self.path.len() - 1 {
                self.clicked_seg = Some(i);
                return true;
            }
        }
        false
    }

    fn set_path(&mut self, segments: &[String]) {
        let mut s = Vec::with_capacity(segments.len().max(1));
        if segments.is_empty() || (segments.len() == 1 && segments[0].is_empty()) {
            s.push("/".to_string());
        } else {
            s.push("/".to_string());
            for name in segments {
                s.push(format!(" \u{203A} {}", name));
            }
        }
        self.path = s;
    }

    fn path_click(&mut self) -> Option<usize> { self.clicked_seg.take() }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        if let Some(i) = self.hovered_seg {
            let mut cx = self.x + BREADCRUMB_PADDING;
            for j in 0..i {
                let w = self.path[j].len() as f32 * 7.5;
                cx += w + SEGMENT_GAP;
            }
            let w = self.path[i].len() as f32 * 7.5;
            quads.push((cx, self.y, w, self.h, [1.0, 1.0, 1.0, 0.06]));
        }
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        let mut cx = self.x + BREADCRUMB_PADDING;
        for (i, seg) in self.path.iter().enumerate() {
            labels.push(TextLabel {
                text: seg.clone(),
                x: cx,
                y: self.y + 6.0,
                font_size: 12.0,
                color: if i == self.path.len() - 1 { [0xcc, 0xcc, 0xd4] } else { [0x88, 0x88, 0x99] },
            });
            cx += seg.len() as f32 * 7.5 + SEGMENT_GAP;
        }
        labels
    }
}

fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 { return None; }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some([r, g, b])
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::event::{ElementState, MouseButton};

    #[test]
    fn test_node_toggle_geometry_visibility() {
        let mut node = Node::new(100.0, 100.0, 200.0, 50.0, "Test Node");

        // 1. Initial state
        assert!(node.geom_visible());
        assert!(!node.take_geom_toggle());
        assert!(node.draggable());

        // Get the toggle rect
        let (tx, ty, tw, th) = node.toggle_rect();
        
        // 2. Hover toggle area
        // Move cursor inside toggle area
        let changed = node.cursor_moved(tx + tw / 2.0, ty + th / 2.0);
        assert!(changed);
        assert!(node.toggle_hovered);
        assert!(!node.draggable(), "Node should not be draggable when hovering over the toggle widget");

        // Move cursor outside toggle area but inside node
        let changed2 = node.cursor_moved(tx - 10.0, ty + th / 2.0);
        assert!(changed2);
        assert!(!node.toggle_hovered);
        assert!(node.draggable());

        // 3. Click toggle area
        // Move cursor back inside toggle area
        node.cursor_moved(tx + tw / 2.0, ty + th / 2.0);
        // Press Left button
        let input_changed = node.mouse_input(MouseButton::Left, ElementState::Pressed, tx + tw / 2.0, ty + th / 2.0);
        assert!(input_changed);
        assert!(!node.geom_visible(), "Geometry visibility should be toggled off");
        assert!(node.take_geom_toggle(), "take_geom_toggle should return true after toggle click");
        assert!(!node.take_geom_toggle(), "take_geom_toggle should clear state after being called once");

        // Click again to toggle back on
        let input_changed2 = node.mouse_input(MouseButton::Left, ElementState::Pressed, tx + tw / 2.0, ty + th / 2.0);
        assert!(input_changed2);
        assert!(node.geom_visible(), "Geometry visibility should be toggled back on");
        assert!(node.take_geom_toggle());
    }

    #[test]
    fn test_menubar_vertical_horizontal_labels() {
        // Create MenuBar with custom horizontal and vertical labels
        let mut menubar = MenuBar::new(0.0, 0.0, 120.0, 30.0)
            .with_title("App")
            .with_item_vh("FileH", "FileV", &["Open", "Save"])
            .with_item("Edit", &["Undo"]);

        // 1. Horizontal mode (default)
        assert!(!menubar.vertical);
        let labels_h = menubar.text_labels();
        // Title should be "App", item 0 should be "FileH", item 1 should be "Edit"
        assert_eq!(labels_h[0].text, "App");
        assert_eq!(labels_h[1].text, "FileH");
        assert_eq!(labels_h[2].text, "Edit");

        // Hover test in horizontal layout
        let ix = menu_item_x("App", &["FileH".to_string(), "Edit".to_string()], 0);
        let iw = menu_item_w(&["FileH".to_string(), "Edit".to_string()], 0);
        
        // Move cursor inside "FileH" bounds
        menubar.cursor_moved(ix + iw / 2.0, 15.0);
        assert_eq!(menubar.hovered_menu, Some(0));

        // 2. Vertical mode
        let mut menubar_v = menubar.with_vertical(true);
        assert!(menubar_v.vertical);
        let labels_v = menubar_v.text_labels();
        // Title should be "App", item 0 should be "FileV", item 1 should be "Edit"
        assert_eq!(labels_v[0].text, "App");
        assert_eq!(labels_v[1].text, "FileV");
        assert_eq!(labels_v[2].text, "Edit");

        // Hover test in vertical layout
        let iy = menubar_v.item_y_vertical(0);
        let ih = menubar_v.item_h_vertical();

        // Move cursor inside "FileV" bounds
        menubar_v.cursor_moved(20.0, iy + ih / 2.0);
        assert_eq!(menubar_v.hovered_menu, Some(0));
    }
}
