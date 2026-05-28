#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementState {
    Pressed,
    Released,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u16),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MouseScrollDelta {
    LineDelta(f32, f32),
    PixelDelta(Position),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    Named(NamedKey),
    Character(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NamedKey {
    Backspace,
    Tab,
    Enter,
    Escape,
    Space,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    End,
    Home,
    PageDown,
    PageUp,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    pub state: ElementState,
    pub logical_key: Key,
    pub text: Option<String>,
    pub repeat: bool,
    pub ctrl: bool,
    pub shift: bool,
}

use crate::colors;

pub mod focus {
    use super::Widget;
    use std::cell::Cell;

    thread_local! {
        static FOCUSED_WIDGET: Cell<Option<*mut (dyn Widget + 'static)>> = Cell::new(None);
    }

    pub fn set_focused(w: &mut dyn Widget) {
        FOCUSED_WIDGET.with(|cell| {
            let new_ptr = unsafe {
                std::mem::transmute::<*mut dyn Widget, *mut (dyn Widget + 'static)>(w as *mut dyn Widget)
            };
            if let Some(old_ptr) = cell.get() {
                let old_data = old_ptr as *mut () as usize;
                let new_data = new_ptr as *mut () as usize;
                if old_data != new_data {
                    unsafe {
                        (*old_ptr).unfocus();
                    }
                    cell.set(Some(new_ptr));
                }
            } else {
                cell.set(Some(new_ptr));
            }
        });
    }

    pub fn is_focused(w: &dyn Widget) -> bool {
        FOCUSED_WIDGET.with(|cell| {
            if let Some(ptr) = cell.get() {
                let current_data = ptr as *const () as usize;
                let query_data = w as *const dyn Widget as *const () as usize;
                current_data == query_data
            } else {
                false
            }
        })
    }

    pub fn clear_focus() {
        FOCUSED_WIDGET.with(|cell| {
            if let Some(ptr) = cell.take() {
                unsafe {
                    (*ptr).unfocus();
                }
            }
        });
    }

    pub fn clear_if_matches(w: &dyn Widget) {
        FOCUSED_WIDGET.with(|cell| {
            if let Some(ptr) = cell.get() {
                let current_data = ptr as *const () as usize;
                let query_data = w as *const dyn Widget as *const () as usize;
                if current_data == query_data {
                    cell.set(None);
                }
            }
        });
    }

    pub fn has_focus() -> bool {
        FOCUSED_WIDGET.with(|cell| cell.get().is_some())
    }

    pub fn link_parent_child(parent: &mut dyn Widget, child: &mut dyn Widget) {
        let parent_ptr = unsafe {
            std::mem::transmute::<*mut dyn Widget, *mut (dyn Widget + 'static)>(parent as *mut dyn Widget)
        };
        let child_ptr = unsafe {
            std::mem::transmute::<*mut dyn Widget, *mut (dyn Widget + 'static)>(child as *mut dyn Widget)
        };
        parent.add_child(child_ptr);
        child.set_parent(Some(parent_ptr));
    }

    pub fn navigate_focus(key: &super::Key, ctrl: bool) -> bool {
        FOCUSED_WIDGET.with(|cell| {
            let ptr = match cell.get() {
                Some(p) => p,
                None => return false,
            };

            unsafe {
                match (key, ctrl) {
                    (super::Key::Character(c), true) if c == "u" || c == "U" => {
                        if let Some(parent_ptr) = (*ptr).parent() {
                            let parent_ref = &mut *parent_ptr;
                            set_focused(parent_ref);
                            parent_ref.focus();
                            return true;
                        }
                    }
                    (super::Key::Character(c), true) if c == "i" || c == "I" => {
                        let mut children = (*ptr).children();
                        if !children.is_empty() {
                            let child_ref = &mut *children[0];
                            set_focused(child_ref);
                            child_ref.focus();
                            return true;
                        }
                    }
                    (super::Key::Character(c), true) if c == "j" || c == "J" => {
                        if let Some(parent_ptr) = (*ptr).parent() {
                            let mut siblings = (*parent_ptr).children();
                            let current_idx = siblings.iter().position(|&x| {
                                let a = x as *mut () as usize;
                                let b = ptr as *mut () as usize;
                                a == b
                            });
                            if let Some(idx) = current_idx {
                                let next_idx = (idx + 1) % siblings.len();
                                let sibling_ref = &mut *siblings[next_idx];
                                set_focused(sibling_ref);
                                sibling_ref.focus();
                                return true;
                            }
                        }
                    }
                    (super::Key::Character(c), true) if c == "k" || c == "K" => {
                        if let Some(parent_ptr) = (*ptr).parent() {
                            let mut siblings = (*parent_ptr).children();
                            let current_idx = siblings.iter().position(|&x| {
                                let a = x as *mut () as usize;
                                let b = ptr as *mut () as usize;
                                a == b
                            });
                            if let Some(idx) = current_idx {
                                let prev_idx = if idx == 0 { siblings.len() - 1 } else { idx - 1 };
                                let sibling_ref = &mut *siblings[prev_idx];
                                set_focused(sibling_ref);
                                sibling_ref.focus();
                                return true;
                            }
                        }
                    }
                    _ => {}
                }
            }
            false
        })
    }
}

pub mod clipboard {
    pub fn copy_to_clipboard(text: &str) {
        let text = text.to_string();
        std::thread::spawn(move || {
            if let Ok(mut child) = std::process::Command::new("wl-copy")
                .stdin(std::process::Stdio::piped())
                .spawn()
            {
                if let Some(mut stdin) = child.stdin.take() {
                    use std::io::Write;
                    let _ = stdin.write_all(text.as_bytes());
                }
                let _ = child.wait();
            } else if let Ok(mut child) = std::process::Command::new("xclip")
                .arg("-selection")
                .arg("clipboard")
                .stdin(std::process::Stdio::piped())
                .spawn()
            {
                if let Some(mut stdin) = child.stdin.take() {
                    use std::io::Write;
                    let _ = stdin.write_all(text.as_bytes());
                }
                let _ = child.wait();
            }
        });
    }

    pub fn read_from_clipboard() -> Option<String> {
        if let Ok(output) = std::process::Command::new("wl-paste")
            .arg("-n")
            .output()
        {
            if output.status.success() {
                if let Ok(text) = String::from_utf8(output.stdout) {
                    return Some(text);
                }
            }
        }
        if let Ok(output) = std::process::Command::new("xclip")
            .arg("-selection")
            .arg("clipboard")
            .arg("-o")
            .output()
        {
            if output.status.success() {
                if let Ok(text) = String::from_utf8(output.stdout) {
                    return Some(text);
                }
            }
        }
        None
    }
}

pub mod context_menu {
    use super::{Widget, TextBox, MouseButton, ElementState, TextLabel};
    use std::cell::RefCell;

    #[derive(Debug, Clone)]
    pub struct ContextMenuState {
        pub x: f32,
        pub y: f32,
        pub w: f32,
        pub h: f32,
        pub visible: bool,
        pub options: Vec<String>,
        pub hovered_item: Option<usize>,
        pub target: Option<*mut TextBox>,
    }

    impl ContextMenuState {
        pub fn new() -> Self {
            Self {
                x: 0.0,
                y: 0.0,
                w: 120.0,
                h: 0.0,
                visible: false,
                options: Vec::new(),
                hovered_item: None,
                target: None,
            }
        }

        pub fn show(&mut self, x: f32, y: f32, options: Vec<String>, target: *mut TextBox) {
            self.x = x;
            self.y = y;
            self.options = options;
            self.h = self.options.len() as f32 * 24.0;
            self.visible = true;
            self.hovered_item = None;
            self.target = Some(target);
        }

        pub fn hide(&mut self) {
            self.visible = false;
            self.target = None;
        }

        pub fn hit_test(&self, px: f32, py: f32) -> bool {
            if !self.visible { return false; }
            px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
        }

        pub fn cursor_moved(&mut self, px: f32, py: f32) -> bool {
            if !self.visible { return false; }
            let was_hovered = self.hovered_item;
            self.hovered_item = None;
            if px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h {
                let idx = ((py - self.y) / 24.0) as usize;
                if idx < self.options.len() {
                    self.hovered_item = Some(idx);
                }
            }
            self.hovered_item != was_hovered
        }

        pub fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
            if !self.visible { return false; }
            if button != MouseButton::Left || state != ElementState::Pressed {
                if state == ElementState::Pressed {
                    self.hide();
                    return true;
                }
                return false;
            }

            if px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h {
                let idx = ((py - self.y) / 24.0) as usize;
                if idx < self.options.len() {
                    let opt = self.options[idx].clone();
                    if let Some(target_ptr) = self.target {
                        unsafe {
                            let target = &mut *target_ptr;
                            match opt.as_str() {
                                "Cut" => {
                                    if target.cut_selection() {
                                        target.just_changed = true;
                                    }
                                }
                                "Copy" => {
                                    target.copy_selection();
                                }
                                "Paste" => {
                                    if target.paste_from_clipboard() {
                                        target.just_changed = true;
                                    }
                                }
                                "Select All" => {
                                    target.select_all();
                                }
                                _ => {}
                            }
                        }
                    }
                }
                self.hide();
                return true;
            } else {
                self.hide();
                return true;
            }
        }

        pub fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
            let mut quads = Vec::new();
            if !self.visible { return quads; }

            // border
            quads.push((self.x, self.y, self.w, self.h, [0.22, 0.22, 0.28, 1.0]));
            // bg
            quads.push((self.x + 1.0, self.y + 1.0, self.w - 2.0, self.h - 2.0, [0.06, 0.06, 0.09, 1.0]));

            if let Some(h_idx) = self.hovered_item {
                let iy = self.y + h_idx as f32 * 24.0;
                quads.push((self.x + 2.0, iy + 2.0, self.w - 4.0, 20.0, [0.20, 0.40, 0.65, 0.6]));
            }

            quads
        }

        pub fn text_labels(&self) -> Vec<TextLabel> {
            let mut labels = Vec::new();
            if !self.visible { return labels; }

            for (idx, opt) in self.options.iter().enumerate() {
                let iy = self.y + idx as f32 * 24.0 + (24.0 - 12.0) / 2.0;
                let text_color = if self.hovered_item == Some(idx) {
                    [0xff, 0xff, 0xff]
                } else {
                    [0xcc, 0xcc, 0xd4]
                };

                labels.push(TextLabel {
                    text: opt.clone(),
                    x: self.x + 8.0,
                    y: iy,
                    font_size: 12.0,
                    color: text_color,
                });
            }
            labels
        }
    }

    thread_local! {
        pub static CONTEXT_MENU: RefCell<ContextMenuState> = RefCell::new(ContextMenuState::new());
    }

    pub fn is_visible() -> bool {
        CONTEXT_MENU.with(|m| m.borrow().visible)
    }

    pub fn show(x: f32, y: f32, options: Vec<String>, target: *mut TextBox) {
        CONTEXT_MENU.with(|m| m.borrow_mut().show(x, y, options, target));
    }

    pub fn hide() {
        CONTEXT_MENU.with(|m| m.borrow_mut().hide());
    }

    pub fn x() -> f32 { CONTEXT_MENU.with(|m| m.borrow().x) }
    pub fn y() -> f32 { CONTEXT_MENU.with(|m| m.borrow().y) }
    pub fn w() -> f32 { CONTEXT_MENU.with(|m| m.borrow().w) }
    pub fn h() -> f32 { CONTEXT_MENU.with(|m| m.borrow().h) }
    pub fn hovered_item() -> Option<usize> { CONTEXT_MENU.with(|m| m.borrow().hovered_item) }
    pub fn options() -> Vec<String> { CONTEXT_MENU.with(|m| m.borrow().options.clone()) }

    pub fn hit_test(px: f32, py: f32) -> bool {
        CONTEXT_MENU.with(|m| m.borrow().hit_test(px, py))
    }

    pub fn cursor_moved(px: f32, py: f32) -> bool {
        CONTEXT_MENU.with(|m| m.borrow_mut().cursor_moved(px, py))
    }

    pub fn mouse_input(button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        CONTEXT_MENU.with(|m| m.borrow_mut().mouse_input(button, state, px, py))
    }

    pub fn extra_quads() -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        CONTEXT_MENU.with(|m| m.borrow().extra_quads())
    }

    pub fn text_labels() -> Vec<TextLabel> {
        CONTEXT_MENU.with(|m| m.borrow().text_labels())
    }
}

#[derive(Debug, Clone)]
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
    fn set_row_rect(&mut self, _x: f32, _w: f32) {}

    fn hit_test(&self, px: f32, py: f32) -> bool {
        let (x, y, w, h) = self.rect();
        px >= x && px <= x + w && py >= y && py <= y + h
    }

    fn cursor_moved(&mut self, _px: f32, _py: f32) -> bool { false }
    fn mouse_input(&mut self, _button: MouseButton, _state: ElementState, _px: f32, _py: f32) -> bool { false }
    fn mouse_wheel(&mut self, _delta: &MouseScrollDelta, _px: f32, _py: f32) -> bool { false }

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
    fn widget_font(&self) -> Option<String> { None }
    fn value(&self) -> i32 { 0 }
    fn top_room(&self) -> f32 { 0.0 }

    fn set_drag_bounds(&mut self, _bx: f32, _by: f32, _bw: f32, _bh: f32) {}

    fn focus(&mut self) {}
    fn unfocus(&mut self) {}
    fn keyboard_input(&mut self, _event: &KeyEvent) -> bool { false }

    fn menu_click(&mut self) -> Option<(usize, usize)> { None }
    fn set_item_checked(&mut self, _menu_idx: usize, _item_idx: usize, _checked: bool) {}
    fn set_menu_items(&mut self, _menu_idx: usize, _items: &[String]) {}
    fn is_menu_bar(&self) -> bool { false }
    fn is_menu_open(&self) -> bool { false }
    fn set_grid_snap(&mut self, _gx: f32, _gy: f32) {}
    fn set_node_name(&mut self, _name: &str) {}
    fn set_visible(&mut self, _visible: bool) {}
    fn visible(&self) -> bool { true }
    fn set_path(&mut self, _segments: &[String]) {}
    fn path_click(&mut self) -> Option<usize> { None }

    fn node_params(&self) -> Vec<(String, String, String)> { vec![] }
    fn set_display_params(&mut self, _params: &[(String, String, String)]) {}

    fn set_config_toggle(&mut self, _id: usize, _val: bool) {}
    fn take_config_toggle(&mut self) -> Option<(usize, bool)> { None }
    fn set_show_network_grid(&mut self, _show: bool) {}
    fn set_config_spin(&mut self, _id: usize, _val: f32) {}
    fn take_config_spin(&mut self) -> Option<(usize, f32)> { None }
    fn set_grid_sizes(&mut self, _gx: f32, _gy: f32) {}
    fn set_grid_origin(&mut self, _ox: f32, _oy: f32) {}
    fn set_skipped_sizes(&mut self, _row_h: f32, _col_w: f32) {}
    fn set_palette_state(&mut self, _visible: bool, _query: &str, _items: &[String], _selected: usize) {}

    fn set_geom_visible(&mut self, _visible: bool) {}
    fn geom_visible(&self) -> bool { true }
    fn take_geom_toggle(&mut self) -> bool { false }
    fn set_spreadsheet_data(&mut self, _headers: Vec<String>, _rows: Vec<Vec<String>>) {}
    fn tick(&mut self, _dt: f32) -> bool { false }

    fn parent(&self) -> Option<*mut (dyn Widget + 'static)> { None }
    fn set_parent(&mut self, _parent: Option<*mut (dyn Widget + 'static)>) {}
    fn children(&self) -> Vec<*mut (dyn Widget + 'static)> { vec![] }
    fn add_child(&mut self, _child: *mut (dyn Widget + 'static)) {}
    fn clear_children(&mut self) {}
}

#[derive(Clone)]
pub struct Container {
    pub parent: Option<*mut (dyn Widget + 'static)>,
    pub children: Vec<*mut (dyn Widget + 'static)>,
}

impl Container {
    pub fn new() -> Self {
        Self { parent: None, children: Vec::new() }
    }
}

impl Widget for Container {
    fn rect(&self) -> (f32, f32, f32, f32) { (0.0, 0.0, 0.0, 0.0) }
    fn set_rect(&mut self, _x: f32, _y: f32, _w: f32, _h: f32) {}
    fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }

    fn focus(&mut self) {
        focus::set_focused(self);
    }
    fn unfocus(&mut self) {}

    fn parent(&self) -> Option<*mut (dyn Widget + 'static)> { self.parent }
    fn set_parent(&mut self, parent: Option<*mut (dyn Widget + 'static)>) { self.parent = parent; }
    fn children(&self) -> Vec<*mut (dyn Widget + 'static)> { self.children.clone() }
    fn add_child(&mut self, child: *mut (dyn Widget + 'static)) { self.children.push(child); }
    fn clear_children(&mut self) { self.children.clear(); }
}

impl Drop for Container {
    fn drop(&mut self) {
        focus::clear_if_matches(self);
    }
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
    skipped_row_h: f32,
    skipped_col_w: f32,
}

impl ContentBg {
    pub fn new() -> Self {
        Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, hovered: false, show_network_grid: false, grid_size_x: 150.0, grid_size_y: 75.0, grid_origin_x: 0.0, grid_origin_y: 0.0, skipped_row_h: 37.5, skipped_col_w: 37.5 }
    }
}

impl Widget for ContentBg {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] {
        if self.show_network_grid {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            colors::CONTENT_BG
        }
    }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }
    fn hit_test(&self, _px: f32, _py: f32) -> bool { false }

    fn set_show_network_grid(&mut self, show: bool) { self.show_network_grid = show; }
    fn set_grid_sizes(&mut self, gx: f32, gy: f32) { self.grid_size_x = gx; self.grid_size_y = gy; }
    fn set_grid_origin(&mut self, ox: f32, oy: f32) { self.grid_origin_x = ox; self.grid_origin_y = oy; }
    fn set_skipped_sizes(&mut self, row_h: f32, col_w: f32) { self.skipped_row_h = row_h; self.skipped_col_w = col_w; }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.show_network_grid || self.grid_size_x <= 0.0 || self.grid_size_y <= 0.0 {
            return vec![];
        }
        let mut quads = Vec::new();
        let grid_color = [0.0, 0.0, 0.0, 0.0];
        let max_alpha = colors::CONTENT_BG[3]; // Peak opacity in the middle of gradient cells matches non-gradient cells
        let steps = 20; // Silky-smooth gradient transition

        let step_y = self.grid_size_y + self.skipped_row_h;
        let step_x = self.grid_size_x + self.skipped_col_w;

        if step_y >= 4.0 && step_x >= 4.0 {
            let ry_start = ((self.y - self.grid_origin_y) / step_y).floor() as i32 - 1;
            let ry_end = ((self.y + self.h - self.grid_origin_y) / step_y).ceil() as i32 + 1;
            let ry_start = ry_start.max(-100_000);
            let ry_end = ry_end.min(100_000);

            let cx_start = ((self.x - self.grid_origin_x) / step_x).floor() as i32 - 1;
            let cx_end = ((self.x + self.w - self.grid_origin_x) / step_x).ceil() as i32 + 1;
            let cx_start = cx_start.max(-100_000);
            let cx_end = cx_end.min(100_000);

            // Draw individual cell backgrounds to avoid stacking with gradients
            for ry in ry_start..=ry_end {
                let y1 = self.grid_origin_y + (ry as f32) * step_y;
                let draw_start_y = y1.max(self.y);
                let draw_end_y = (y1 + self.grid_size_y).min(self.y + self.h);
                if draw_start_y < draw_end_y {
                    for cx in cx_start..=cx_end {
                        let x1 = self.grid_origin_x + (cx as f32) * step_x;
                        let draw_start_x = x1.max(self.x);
                        let draw_end_x = (x1 + self.grid_size_x).min(self.x + self.w);
                        if draw_start_x < draw_end_x {
                            quads.push((draw_start_x, draw_start_y, draw_end_x - draw_start_x, draw_end_y - draw_start_y, colors::CONTENT_BG));
                        }
                    }
                }
            }
        }

        // Draw interstitial row gradients (horizontal bands fading to 0 alpha at left and right sides)
        if self.skipped_row_h > 0.0 {
            let step_y = self.grid_size_y + self.skipped_row_h;
            let step_x = self.grid_size_x + self.skipped_col_w;
            if step_y >= 4.0 && step_x >= 4.0 {
                let k_start = ((self.y - self.grid_origin_y) / step_y).floor() as i32 - 1;
                let k_end = ((self.y + self.h - self.grid_origin_y) / step_y).ceil() as i32 + 1;
                let k_start = k_start.max(-100_000);
                let k_end = k_end.min(100_000);

                let cx_start = ((self.x - self.grid_origin_x) / step_x).floor() as i32 - 1;
                let cx_end = ((self.x + self.w - self.grid_origin_x) / step_x).ceil() as i32 + 1;
                let cx_start = cx_start.max(-100_000);
                let cx_end = cx_end.min(100_000);

                for k in k_start..=k_end {
                    let y1 = self.grid_origin_y + (k as f32) * step_y;
                    let y2 = y1 + self.grid_size_y;
                    if y1 >= self.y + self.h {
                        continue;
                    }
                    let draw_start_y = y2.max(self.y);
                    let draw_end_y = (y2 + self.skipped_row_h).min(self.y + self.h);
                    if draw_start_y >= draw_end_y {
                        continue;
                    }

                    for cx in cx_start..=cx_end {
                        let x1 = self.grid_origin_x + (cx as f32) * step_x;
                        let x_mid = x1 + self.grid_size_x / 2.0;
                        let w_total = self.grid_size_x;
                        let sub_w = w_total / steps as f32;

                        for i in 0..steps {
                            let sx_start = x1 + i as f32 * sub_w;
                            let sx_end = sx_start + sub_w;
                            let draw_start_x = sx_start.max(self.x);
                            let draw_end_x = sx_end.min(self.x + self.w);
                            if draw_start_x < draw_end_x {
                                let sx_mid = (sx_start + sx_end) / 2.0;
                                let dist = (sx_mid - x_mid).abs();
                                let d = (dist / (w_total / 2.0)).min(1.0);
                                
                                // Fade the cell background color from max_alpha in the middle to transparent at the edges
                                let alpha = max_alpha * (1.0 - d);
                                if alpha > 0.001 {
                                    quads.push((draw_start_x, draw_start_y, draw_end_x - draw_start_x, draw_end_y - draw_start_y, [colors::CONTENT_BG[0], colors::CONTENT_BG[1], colors::CONTENT_BG[2], alpha]));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Draw interstitial column gradients (vertical bands fading to 0 alpha at top and bottom)
        if self.skipped_col_w > 0.0 {
            let step_y = self.grid_size_y + self.skipped_row_h;
            let step_x = self.grid_size_x + self.skipped_col_w;
            if step_y >= 4.0 && step_x >= 4.0 {
                let k_start = ((self.x - self.grid_origin_x) / step_x).floor() as i32 - 1;
                let k_end = ((self.x + self.w - self.grid_origin_x) / step_x).ceil() as i32 + 1;
                let k_start = k_start.max(-100_000);
                let k_end = k_end.min(100_000);

                let ry_start = ((self.y - self.grid_origin_y) / step_y).floor() as i32 - 1;
                let ry_end = ((self.y + self.h - self.grid_origin_y) / step_y).ceil() as i32 + 1;
                let ry_start = ry_start.max(-100_000);
                let ry_end = ry_end.min(100_000);

                for k in k_start..=k_end {
                    let x1 = self.grid_origin_x + (k as f32) * step_x;
                    let x2 = x1 + self.grid_size_x;
                    if x1 >= self.x + self.w {
                        continue;
                    }
                    let draw_start_x = x2.max(self.x);
                    let draw_end_x = (x2 + self.skipped_col_w).min(self.x + self.w);
                    if draw_start_x >= draw_end_x {
                        continue;
                    }

                    for ry in ry_start..=ry_end {
                        let y1 = self.grid_origin_y + (ry as f32) * step_y;
                        let y_mid = y1 + self.grid_size_y / 2.0;
                        let h_total = self.grid_size_y;
                        let sub_h = h_total / steps as f32;

                        for i in 0..steps {
                            let sy_start = y1 + i as f32 * sub_h;
                            let sy_end = sy_start + sub_h;
                            let draw_start_y = sy_start.max(self.y);
                            let draw_end_y = sy_end.min(self.y + self.h);
                            if draw_start_y < draw_end_y {
                                let sy_mid = (sy_start + sy_end) / 2.0;
                                let dist = (sy_mid - y_mid).abs();
                                let d = (dist / (h_total / 2.0)).min(1.0);
                                
                                // Fade the cell background color from max_alpha in the middle to transparent at the edges
                                let alpha = max_alpha * (1.0 - d);
                                if alpha > 0.001 {
                                    quads.push((draw_start_x, draw_start_y, draw_end_x - draw_start_x, draw_end_y - draw_start_y, [colors::CONTENT_BG[0], colors::CONTENT_BG[1], colors::CONTENT_BG[2], alpha]));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Draw the grid borders
        let step_y = self.grid_size_y + self.skipped_row_h;
        if step_y >= 4.0 {
            let k_start = ((self.y - self.grid_origin_y) / step_y).floor() as i32 - 1;
            let k_end = ((self.y + self.h - self.grid_origin_y) / step_y).ceil() as i32 + 1;
            let k_start = k_start.max(-100_000);
            let k_end = k_end.min(100_000);
            for k in k_start..=k_end {
                let y1 = self.grid_origin_y + (k as f32) * step_y;
                let y2 = y1 + self.grid_size_y;
                if y1 >= self.y + self.h {
                    continue;
                }
                if y1 >= self.y {
                    quads.push((self.x, y1, self.w, 1.0, grid_color));
                }
                if y2 >= self.y && y2 < self.y + self.h {
                    quads.push((self.x, y2, self.w, 1.0, grid_color));
                }
            }
        }

        let step_x = self.grid_size_x + self.skipped_col_w;
        if step_x >= 4.0 {
            let k_start = ((self.x - self.grid_origin_x) / step_x).floor() as i32 - 1;
            let k_end = ((self.x + self.w - self.grid_origin_x) / step_x).ceil() as i32 + 1;
            let k_start = k_start.max(-100_000);
            let k_end = k_end.min(100_000);
            for k in k_start..=k_end {
                let x1 = self.grid_origin_x + (k as f32) * step_x;
                let x2 = x1 + self.grid_size_x;
                if x1 >= self.x + self.w {
                    continue;
                }
                if x1 >= self.x {
                    quads.push((x1, self.y, 1.0, self.h, grid_color));
                }
                if x2 >= self.x && x2 < self.x + self.w {
                    quads.push((x2, self.y, 1.0, self.h, grid_color));
                }
            }
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
    display_params: Vec<(String, String, String)>,
    dragging_param: Option<usize>,
    drag_offset: f32,
    pub focused_param: Option<usize>,
}

impl ParametersBg {
    pub fn new() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            hovered: false,
            display_params: Vec::new(),
            dragging_param: None,
            drag_offset: 0.0,
            focused_param: None,
        }
    }

    pub fn get_param_rects(&self) -> Vec<(f32, f32, f32, f32)> {
        let mut rects = Vec::new();
        let mut cur_y = self.y + 30.0;
        for p in &self.display_params {
            let h = if p.2 == "code" { 200.0 } else { 20.0 };
            rects.push((self.x + 8.0, cur_y, self.w - 16.0, h));
            cur_y += h + 8.0;
        }
        rects
    }
}

fn parse_slider_range(ptype: &str) -> (f32, f32) {
    if ptype.starts_with("slider:") {
        let parts: Vec<&str> = ptype.split(':').collect();
        if parts.len() >= 3 {
            if let (Ok(min), Ok(max)) = (parts[1].parse::<f32>(), parts[2].parse::<f32>()) {
                return (min, max);
            }
        }
    }
    (0.0, 2.0)
}

impl Widget for ParametersBg {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { colors::PARAM_BG }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }
    fn hit_test(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }

    fn set_display_params(&mut self, params: &[(String, String, String)]) {
        self.display_params = params.to_vec();
    }

    fn node_params(&self) -> Vec<(String, String, String)> {
        self.display_params.clone()
    }

    fn draggable(&self) -> bool {
        self.display_params.iter().any(|p| p.2.starts_with("slider"))
    }

    fn is_dragging(&self) -> bool {
        self.dragging_param.is_some()
    }

    fn drag_begin(&mut self, px: f32, py: f32) {
        let rects = self.get_param_rects();
        for (i, p) in self.display_params.iter().enumerate() {
            if p.2.starts_with("slider") {
                let r = rects[i];
                let row_y = r.1;
                if py >= row_y - 2.0 && py <= row_y + 18.0 {
                    let val = p.1.parse::<f32>().unwrap_or(0.0);
                    let (min, max) = parse_slider_range(&p.2);
                    let denom = max - min;
                    let t = if denom != 0.0 {
                        ((val - min) / denom).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let track_x = self.x + 100.0;
                    let track_w = (self.w - 100.0 - 20.0).max(10.0);
                    let thumb_size = 10.0;
                    let thumb_x = track_x + t * (track_w - thumb_size);

                    let click_offset = px - thumb_x;
                    if click_offset >= 0.0 && click_offset <= thumb_size {
                        self.drag_offset = click_offset;
                    } else {
                        self.drag_offset = thumb_size / 2.0;
                    }
                    self.dragging_param = Some(i);
                    break;
                }
            }
        }
    }

    fn drag_update(&mut self, px: f32, _py: f32) -> bool {
        if let Some(i) = self.dragging_param {
            let track_x = self.x + 100.0;
            let track_w = (self.w - 100.0 - 20.0).max(10.0);
            let thumb_size = 10.0;
            let range = track_w - thumb_size;
            if range > 0.0 {
                let raw = (px - self.drag_offset - track_x) / range;
                let t = raw.clamp(0.0, 1.0);
                let (min, max) = parse_slider_range(&self.display_params[i].2);
                let new_val = min + t * (max - min);
                let old_val = &self.display_params[i].1;
                let new_val_str = format!("{:.2}", new_val);
                if *old_val != new_val_str {
                    self.display_params[i].1 = new_val_str;
                    return true;
                }
            }
        }
        false
    }

    fn drag_end(&mut self) {
        self.dragging_param = None;
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button == MouseButton::Left && state == ElementState::Pressed {
            let rects = self.get_param_rects();
            let mut clicked_any_code = false;
            for (i, p) in self.display_params.iter().enumerate() {
                if p.2 == "code" {
                    let r = rects[i];
                    if px >= r.0 && px <= r.0 + r.2 && py >= r.1 + 18.0 && py <= r.1 + r.3 {
                        self.focused_param = Some(i);
                        clicked_any_code = true;
                        break;
                    }
                }
            }
            if !clicked_any_code {
                self.focused_param = None;
            }
            return true;
        }
        false
    }

    fn keyboard_input(&mut self, event: &KeyEvent) -> bool {
        if let Some(idx) = self.focused_param {
            if event.state == ElementState::Pressed {
                let p = &mut self.display_params[idx];
                match &event.logical_key {
                    Key::Named(NamedKey::Backspace) => {
                        if !p.1.is_empty() {
                            p.1.pop();
                            return true;
                        }
                    }
                    Key::Named(NamedKey::Enter) => {
                        p.1.push('\n');
                        return true;
                    }
                    Key::Named(NamedKey::Escape) => {
                        self.focused_param = None;
                        return true;
                    }
                    Key::Character(s) => {
                        p.1.push_str(s);
                        return true;
                    }
                    _ => {}
                }
            }
        }
        false
    }

    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32) -> bool {
        let mut changed = false;
        let rects = self.get_param_rects();
        for (i, p) in self.display_params.iter_mut().enumerate() {
            if p.2.starts_with("slider") {
                let r = rects[i];
                let row_y = r.1;
                if py >= row_y - 2.0 && py <= row_y + 18.0 && px >= self.x && px <= self.x + self.w {
                    let val = p.1.parse::<f32>().unwrap_or(0.0);
                    let (min, max) = parse_slider_range(&p.2);
                    let scroll_amount = match delta {
                        MouseScrollDelta::LineDelta(_x, y) => *y,
                        MouseScrollDelta::PixelDelta(pos) => (pos.y as f32) / 120.0,
                    };
                    let step = (max - min) * 0.02;
                    let new_val = (val + scroll_amount * step).clamp(min, max);
                    let old_val = &p.1;
                    let new_val_str = format!("{:.2}", new_val);
                    if *old_val != new_val_str {
                        p.1 = new_val_str;
                        changed = true;
                    }
                }
            }
        }
        changed
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        let rects = self.get_param_rects();
        for (i, p) in self.display_params.iter().enumerate() {
            let r = rects[i];
            if p.2.starts_with("slider") {
                let val = p.1.parse::<f32>().unwrap_or(0.0);
                let (min, max) = parse_slider_range(&p.2);
                let denom = max - min;
                let t = if denom != 0.0 {
                    ((val - min) / denom).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let track_x = self.x + 100.0;
                let track_w = (self.w - 100.0 - 20.0).max(10.0);
                let track_y = r.1 + 6.0;
                let track_h = 4.0;

                let thumb_size = 10.0;
                let thumb_x = track_x + t * (track_w - thumb_size);
                let thumb_y = r.1 + 3.0;

                quads.push((track_x, track_y, track_w, track_h, colors::SLIDER_TRACK));

                let thumb_color = if self.dragging_param == Some(i) {
                    colors::SLIDER_THUMB_DRAG
                } else {
                    colors::SLIDER_THUMB
                };
                quads.push((thumb_x, thumb_y, thumb_size, thumb_size, thumb_color));
            } else if p.2 == "code" {
                quads.push((r.0, r.1 + 18.0, r.2, r.3 - 18.0, [0.08, 0.08, 0.10, 1.0]));
                let border_color = if self.focused_param == Some(i) {
                    [0.25, 0.45, 0.85, 1.0]
                } else {
                    [0.20, 0.20, 0.25, 1.0]
                };
                let (bx, by, bw, bh) = (r.0, r.1 + 18.0, r.2, r.3 - 18.0);
                quads.push((bx, by, bw, 1.0, border_color));
                quads.push((bx, by + bh - 1.0, bw, 1.0, border_color));
                quads.push((bx, by, 1.0, bh, border_color));
                quads.push((bx + bw - 1.0, by, 1.0, bh, border_color));
            }
        }
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let rects = self.get_param_rects();
        self.display_params.iter().enumerate().map(|(i, (name, value, ptype))| {
            let r = rects[i];
            if ptype.starts_with("slider") {
                let val = value.parse::<f32>().unwrap_or(0.0);
                TextLabel {
                    text: format!("{}: {:.2}", name, val),
                    x: self.x + 8.0,
                    y: r.1,
                    font_size: 12.0,
                    color: [0xaa, 0xaa, 0xbb],
                }
            } else if ptype == "code" {
                TextLabel {
                    text: format!("{}:\n{}", name, value),
                    x: self.x + 12.0,
                    y: r.1,
                    font_size: 12.0,
                    color: [0xaa, 0xaa, 0xbb],
                }
            } else {
                TextLabel {
                    text: format!("{}: {}", name, value),
                    x: self.x + 8.0,
                    y: r.1,
                    font_size: 12.0,
                    color: [0xaa, 0xaa, 0xbb],
                }
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
    pub visible: bool,
    pub focused: bool,
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
            visible: true,
            focused: false,
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
        if !self.visible {
            return (0.0, 0.0, 0.0, 0.0);
        }
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
    fn color(&self) -> [f32; 4] {
        if !self.visible {
            [0.0, 0.0, 0.0, 0.0]
        } else if self.focused {
            colors::PANEL_MENU_FOCUSED
        } else {
            colors::PANEL_MENU_BG
        }
    }
    fn set_hovered(&mut self, v: bool) { self.hovering = v; }
    fn hovered(&self) -> bool { self.hovering }

    fn hit_test(&self, px: f32, py: f32) -> bool {
        if !self.visible {
            return false;
        }
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
        if !self.visible {
            return false;
        }
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
        if !self.visible {
            return false;
        }
        if button != MouseButton::Left || state != ElementState::Pressed { return false; }
        if !self.hit_test(px, py) { return false; }

        if self.vertical {
            if px >= self.x && px < self.x + self.w {
                let ly = py - self.y;
                for i in 0..self.menu_items.len() {
                    let iy = self.item_y_vertical(i);
                    let ih = self.item_h_vertical();
                    if ly >= iy && ly < iy + ih {
                        if self.was_open == Some(i) || self.open_menu == Some(i) {
                            self.open_menu = None;
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
                        if self.was_open == Some(i) || self.open_menu == Some(i) {
                            self.open_menu = None;
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

    fn focus(&mut self) {
        self.focused = true;
        focus::set_focused(self);
    }

    fn unfocus(&mut self) {
        self.focused = false;
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

    fn set_menu_items(&mut self, menu_idx: usize, items: &[String]) {
        if menu_idx < self.menu_dropdowns.len() {
            self.menu_dropdowns[menu_idx] = items.to_vec();
            self.menu_dropdown_checked[menu_idx] = vec![Some(false); items.len()];
        }
    }

    fn is_menu_bar(&self) -> bool { self.visible }
    fn is_menu_open(&self) -> bool { self.visible && self.open_menu.is_some() }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.visible {
            return Vec::new();
        }
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
        if !self.visible {
            return Vec::new();
        }
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
                    x: self.x + menu_item_x(&self.title, &self.menu_items, i) + 8.0,
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

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }
    fn visible(&self) -> bool {
        self.visible
    }
}

impl Drop for MenuBar {
    fn drop(&mut self) {
        focus::clear_if_matches(self);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Reset,
    ListRow,
    CopyIcon,
}

#[derive(Debug, Clone)]
pub struct Button {
    x: f32, y: f32, w: f32, h: f32,
    hovering: bool,
    pressed: bool,
    just_clicked: bool,
    kind: ButtonKind,
    pub label: Option<String>,
    pub selected: bool,
}

impl Button {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h, hovering: false, pressed: false, just_clicked: false, kind: ButtonKind::Primary, label: None, selected: false }
    }

    pub fn new_reset(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h, hovering: false, pressed: false, just_clicked: false, kind: ButtonKind::Reset, label: None, selected: false }
    }

    pub fn new_list_row(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h, hovering: false, pressed: false, just_clicked: false, kind: ButtonKind::ListRow, label: None, selected: false }
    }

    pub fn new_copy_icon(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h, hovering: false, pressed: false, just_clicked: false, kind: ButtonKind::CopyIcon, label: None, selected: false }
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.label = Some(label.to_string());
        self
    }

    pub fn with_selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
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
            ButtonKind::ListRow => {
                if self.selected {
                    if self.pressed { [0.30, 0.52, 0.78, 0.6] }
                    else if self.hovering { [0.30, 0.52, 0.78, 0.5] }
                    else { [0.20, 0.40, 0.65, 0.4] }
                } else {
                    if self.pressed { [0.20, 0.20, 0.25, 0.25] }
                    else if self.hovering { [0.20, 0.20, 0.25, 0.15] }
                    else { [0.0, 0.0, 0.0, 0.0] }
                }
            }
            ButtonKind::CopyIcon => {
                if self.selected {
                    if self.pressed { [0.30, 0.52, 0.78, 0.5] }
                    else if self.hovering { [0.30, 0.52, 0.78, 0.5] }
                    else { [0.20, 0.40, 0.65, 0.2] }
                } else {
                    if self.pressed { [0.20, 0.20, 0.25, 0.25] }
                    else if self.hovering { [0.20, 0.20, 0.25, 0.25] }
                    else { [0.0, 0.0, 0.0, 0.0] }
                }
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

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        if let Some(ref label) = self.label {
            let font_size = 12.0;
            let est_w = if label == "📋" {
                12.0
            } else {
                label.len() as f32 * 6.5
            };
            let color = match self.kind {
                ButtonKind::ListRow | ButtonKind::CopyIcon => {
                    if self.selected { [230, 230, 242] }
                    else { [178, 178, 191] }
                }
                _ => [0xcc, 0xcc, 0xd4]
            };
            labels.push(TextLabel {
                text: label.clone(),
                x: self.x + (self.w - est_w) / 2.0,
                y: self.y + (self.h - font_size) / 2.0 - 1.0,
                font_size,
                color,
            });
        }
        labels
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
    pub parameters: Vec<(String, String, String)>,
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
        self.parameters = params.iter().map(|(k, v)| (k.to_string(), v.to_string(), "string".to_string())).collect();
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

    fn focus(&mut self) {
        self.selected = true;
        focus::set_focused(self);
    }
    fn unfocus(&mut self) { self.selected = false; }

    fn node_params(&self) -> Vec<(String, String, String)> { self.parameters.clone() }
    fn set_display_params(&mut self, params: &[(String, String, String)]) {
        self.parameters = params.to_vec();
    }

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

impl Drop for Node {
    fn drop(&mut self) {
        focus::clear_if_matches(self);
    }
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
    row_x: f32,
    row_w: f32,
}

impl Toggle {
    pub fn new() -> Self {
        Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, hovering: false, toggled: false, just_toggled: false, label: None, row_x: 0.0, row_w: 0.0 }
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

    pub fn toggled(&self) -> bool {
        self.toggled
    }
}

impl Widget for Toggle {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn set_row_rect(&mut self, x: f32, w: f32) { self.row_x = x; self.row_w = w; }
    fn color(&self) -> [f32; 4] { if self.toggled { colors::TOGGLE_ON } else { colors::TOGGLE_OFF } }
    fn set_hovered(&mut self, v: bool) { self.hovering = v; }
    fn hovered(&self) -> bool { self.hovering }

    fn hit_test(&self, px: f32, py: f32) -> bool {
        let (x, y, w, h) = self.rect();
        let hx = if self.row_w > 0.0 { self.row_x } else { x };
        let hw = if self.row_w > 0.0 { self.row_w } else { w };
        let (hy, hh) = if self.label.is_some() {
            (y - 18.0, h + 18.0)
        } else {
            (y, h)
        };
        px >= hx && px <= hx + hw && py >= hy && py <= hy + hh
    }

    fn top_room(&self) -> f32 { if self.label.is_some() { 18.0 } else { 0.0 } }

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
        if self.hovering {
            let hx = if self.row_w > 0.0 { self.row_x } else { self.x };
            let hw = if self.row_w > 0.0 { self.row_w } else { self.w };
            let (hy, hh) = if self.label.is_some() {
                (self.y - 18.0, self.h + 18.0)
            } else {
                (self.y, self.h)
            };
            quads.push((hx, hy, hw, hh, [1.0, 1.0, 1.0, 0.06]));
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
                x: self.x,
                y: self.y - 18.0,
                font_size: 12.0,
                color: [0x83, 0x83, 0x8a],
            }]
        } else {
            Vec::new()
        }
    }
}

#[derive(Debug, Clone)]
pub struct Label {
    x: f32, y: f32, w: f32, h: f32,
    text: String,
    font_size: f32,
    color: [u8; 3],
    hovered: bool,
    row_x: f32,
    row_w: f32,
}

impl Label {
    pub fn new(text: &str) -> Self {
        Self {
            x: 0.0, y: 0.0, w: 0.0, h: 0.0,
            text: text.to_string(),
            font_size: 12.0,
            color: [0x83, 0x83, 0x8a],
            hovered: false,
            row_x: 0.0,
            row_w: 0.0,
        }
    }

    pub fn with_font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    pub fn with_color(mut self, color: [u8; 3]) -> Self {
        self.color = color;
        self
    }

    pub fn set_text(&mut self, text: &str) {
        self.text = text.to_string();
    }

    pub fn set_color(&mut self, color: [u8; 3]) {
        self.color = color;
    }
}

impl Widget for Label {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn set_row_rect(&mut self, x: f32, w: f32) { self.row_x = x; self.row_w = w; }
    fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }

    fn hit_test(&self, px: f32, py: f32) -> bool {
        let (x, y, w, h) = self.rect();
        let hx = if self.row_w > 0.0 { self.row_x } else { x };
        let hw = if self.row_w > 0.0 { self.row_w } else { w };
        px >= hx && px <= hx + hw && py >= y && py <= y + h
    }

    fn cursor_moved(&mut self, px: f32, py: f32) -> bool {
        let was = self.hovered;
        self.hovered = self.hit_test(px, py);
        was != self.hovered
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        if self.hovered {
            let hx = if self.row_w > 0.0 { self.row_x } else { self.x };
            let hw = if self.row_w > 0.0 { self.row_w } else { self.w };
            quads.push((hx, self.y, hw, self.h, [1.0, 1.0, 1.0, 0.06]));
        }
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        vec![TextLabel {
            text: self.text.clone(),
            x: self.x,
            y: self.y + (self.h - self.font_size) / 2.0,
            font_size: self.font_size,
            color: self.color,
        }]
    }
}

#[derive(Debug, Clone)]
pub struct Svg {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub quads: Vec<(f32, f32, f32, f32, [f32; 4])>,
}

impl Svg {
    pub fn new(svg_data: &[u8], x: f32, y: f32, w: f32, h: f32) -> Option<Self> {
        let opt = resvg::usvg::Options::default();
        let fontdb = resvg::usvg::fontdb::Database::new();
        let tree = resvg::usvg::Tree::from_data(svg_data, &opt, &fontdb).ok()?;
        
        let target_w = w as u32;
        let target_h = h as u32;
        if target_w == 0 || target_h == 0 {
            return None;
        }
        let mut pixmap = resvg::tiny_skia::Pixmap::new(target_w, target_h)?;
        
        let orig_w = tree.size().width();
        let orig_h = tree.size().height();
        let sx = target_w as f32 / orig_w;
        let sy = target_h as f32 / orig_h;
        let transform = resvg::tiny_skia::Transform::from_scale(sx, sy);
        
        resvg::render(&tree, transform, &mut pixmap.as_mut());
        
        let mut quads = Vec::new();
        let pixels = pixmap.data();
        for row in 0..target_h {
            for col in 0..target_w {
                let idx = ((row * target_w + col) * 4) as usize;
                if idx + 3 < pixels.len() {
                    let a = pixels[idx + 3] as f32 / 255.0;
                    if a > 0.0 {
                        let r = pixels[idx] as f32 / 255.0;
                        let g = pixels[idx + 1] as f32 / 255.0;
                        let b = pixels[idx + 2] as f32 / 255.0;
                        quads.push((
                            x + col as f32,
                            y + row as f32,
                            1.0,
                            1.0,
                            [r, g, b, a],
                        ));
                    }
                }
            }
        }
        
        Some(Self { x, y, w, h, quads })
    }

    pub fn from_file<P: AsRef<std::path::Path>>(path: P, x: f32, y: f32, w: f32, h: f32) -> Option<Self> {
        let data = std::fs::read(path).ok()?;
        Self::new(&data, x, y, w, h)
    }

    pub fn from_str(svg_str: &str, x: f32, y: f32, w: f32, h: f32) -> Option<Self> {
        Self::new(svg_str.as_bytes(), x, y, w, h)
    }
}

impl Widget for Svg {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let dx = x - self.x;
        let dy = y - self.y;
        for quad in &mut self.quads {
            quad.0 += dx;
            quad.1 += dy;
        }
        self.x = x;
        self.y = y;
        self.w = w;
        self.h = h;
    }
    
    fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }
    
    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        self.quads.clone()
    }
}

pub struct Slider {
    x: f32, y: f32, w: f32, h: f32,
    hovered: bool,
    dragging: bool,
    value: f32,
    drag_offset: f32,
    scroll_enabled: bool,
}

impl Slider {
    pub fn new() -> Self {
        Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, hovered: false, dragging: false, value: 0.5, drag_offset: 0.0, scroll_enabled: false }
    }

    pub fn with_scroll(mut self, enabled: bool) -> Self {
        self.scroll_enabled = enabled;
        self
    }

    pub fn set_scroll(&mut self, enabled: bool) {
        self.scroll_enabled = enabled;
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

    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32) -> bool {
        if !self.scroll_enabled {
            return false;
        }
        let (sx, sy, sw, sh) = self.rect();
        if px >= sx && px <= sx + sw && py >= sy && py <= sy + sh {
            let scroll_amount = match delta {
                MouseScrollDelta::LineDelta(_x, y) => *y,
                MouseScrollDelta::PixelDelta(pos) => (pos.y as f32) / 120.0,
            };
            let step = 0.02;
            let new_val = (self.value - scroll_amount * step).clamp(0.0, 1.0);
            if (new_val - self.value).abs() > 0.0001 {
                self.value = new_val;
                return true;
            }
        }
        false
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let thumb_size = self.h * 0.9;
        let thumb_x = self.x + self.value * (self.w - thumb_size);
        let thumb_y = self.y + (self.h - thumb_size) / 2.0;
        let thumb_color = if self.dragging {
            colors::SLIDER_THUMB_DRAG
        } else {
            colors::SLIDER_THUMB
        };
        vec![(thumb_x, thumb_y, thumb_size, thumb_size, thumb_color)]
    }
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
    row_x: f32,
    row_w: f32,
    pub decimals: u32,
    pub parent: Option<*mut (dyn Widget + 'static)>,
    pub children: Vec<*mut (dyn Widget + 'static)>,
}

impl Spinbox {
    pub fn new(value: i32, min: i32, max: i32, step: i32) -> Self {
        Self { x: 0.0, y: 0.0, w: 0.0, h: 0.0, value, min, max, step, editing: false, edit_buffer: String::new(), hovered: false, hover_dec: false, hover_inc: false, label: None, unit: None, row_x: 0.0, row_w: 0.0, decimals: 0, parent: None, children: Vec::new() }
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

    pub fn with_decimals(mut self, decimals: u32) -> Self {
        self.decimals = decimals;
        self
    }
}

impl Widget for Spinbox {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn set_row_rect(&mut self, x: f32, w: f32) { self.row_x = x; self.row_w = w; }
    fn color(&self) -> [f32; 4] { colors::SPINBOX_BG }
    fn value(&self) -> i32 { self.value }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }
    fn widget_font(&self) -> Option<String> { Some("monospace".to_string()) }

    fn hit_test(&self, px: f32, py: f32) -> bool {
        let (x, y, w, h) = self.rect();
        let hx = if self.row_w > 0.0 { self.row_x } else { x };
        let hw = if self.row_w > 0.0 { self.row_w } else { w };
        let (hy, hh) = if self.label.is_some() {
            (y - 20.0, h + 20.0)
        } else {
            (y, h)
        };
        px >= hx && px <= hx + hw && py >= hy && py <= hy + hh
    }

    fn top_room(&self) -> f32 { if self.label.is_some() { 16.0 } else { 0.0 } }

    fn cursor_moved(&mut self, px: f32, py: f32) -> bool {
        let was = self.hovered;
        self.hovered = self.hit_test(px, py);
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
        if self.decimals > 0 {
            let divisor = 10.0f32.powi(self.decimals as i32);
            self.edit_buffer = format!("{:.width$}", self.value as f32 / divisor, width = self.decimals as usize);
        } else {
            self.edit_buffer = self.value.to_string();
        }
        focus::set_focused(self);
    }

    fn unfocus(&mut self) {
        if self.editing {
            self.editing = false;
            if self.decimals > 0 {
                if let Ok(val_f) = self.edit_buffer.parse::<f32>() {
                    let divisor = 10.0f32.powi(self.decimals as i32);
                    self.value = (val_f * divisor).round() as i32;
                    self.value = self.value.clamp(self.min, self.max);
                }
            } else {
                if let Ok(val) = self.edit_buffer.parse::<i32>() {
                    self.value = val.clamp(self.min, self.max);
                }
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
                if self.decimals > 0 {
                    if let Ok(val_f) = self.edit_buffer.parse::<f32>() {
                        let divisor = 10.0f32.powi(self.decimals as i32);
                        self.value = (val_f * divisor).round() as i32;
                        self.value = self.value.clamp(self.min, self.max);
                    }
                } else {
                    if let Ok(val) = self.edit_buffer.parse::<i32>() {
                        self.value = val.clamp(self.min, self.max);
                    }
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
                    for ch in text.chars() {
                        match ch {
                            '-' if self.edit_buffer.is_empty() => self.edit_buffer.push('-'),
                            '.' if self.decimals > 0 && !self.edit_buffer.contains('.') => self.edit_buffer.push('.'),
                            '0'..='9' => self.edit_buffer.push(ch),
                            _ => {}
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
            let hx = if self.row_w > 0.0 { self.row_x } else { self.x };
            let hw = if self.row_w > 0.0 { self.row_w } else { self.w };
            quads.push((hx, hy, hw, hh, [1.0, 1.0, 1.0, 0.06]));
        }
        let split = self.x + self.w * 0.55;
        let btn_w = self.w * 0.225;
        let inc_col = if self.hover_inc { colors::SPINBOX_BUTTON_HOVER } else { colors::SPINBOX_BUTTON };
        let dec_col = if self.hover_dec { colors::SPINBOX_BUTTON_HOVER } else { colors::SPINBOX_BUTTON };
        
        let display_bg = if self.editing {
            [0.06, 0.10, 0.18, 1.0] // Focused dark-blue input field look
        } else {
            colors::SPINBOX_DISPLAY
        };
        
        quads.push((self.x, self.y, self.w * 0.55, self.h, display_bg));
        quads.push((split, self.y, btn_w, self.h, dec_col));
        quads.push((split + btn_w, self.y, btn_w, self.h, inc_col));
        
        if self.editing {
            let border_color = [0.20, 0.50, 0.85, 1.0]; // Bright focused blue border
            // Top border
            quads.push((self.x, self.y, self.w * 0.55, 1.0, border_color));
            // Bottom border
            quads.push((self.x, self.y + self.h - 1.0, self.w * 0.55, 1.0, border_color));
            // Left border
            quads.push((self.x, self.y, 1.0, self.h, border_color));
            // Right border
            quads.push((self.x + self.w * 0.55 - 1.0, self.y, 1.0, self.h, border_color));

            // Caret cursor
            let char_width = 8.4;
            let cursor_x = self.x + 4.0 + (self.edit_buffer.len() as f32 * char_width);
            let max_cursor_x = split - 4.0;
            let final_cursor_x = cursor_x.min(max_cursor_x);
            let cursor_y = self.y + (self.h - 14.0) / 2.0;
            quads.push((final_cursor_x, cursor_y, 1.5, 14.0, [0.80, 0.80, 0.85, 1.0]));
        }
        
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
        let value_text = if self.editing {
            self.edit_buffer.clone()
        } else if self.decimals > 0 {
            let divisor = 10.0f32.powi(self.decimals as i32);
            format!("{:.width$}", self.value as f32 / divisor, width = self.decimals as usize)
        } else {
            self.value.to_string()
        };
        labels.push(TextLabel {
            text: value_text,
            x: self.x + 4.0,
            y: self.y + (self.h - 14.0) / 2.0 - 2.0,
            font_size: 14.0,
            color: [0xcc, 0xcc, 0xd4],
        });
        if let Some(ref unit) = self.unit {
            labels.push(TextLabel {
                text: unit.clone(),
                x: self.x + 4.0 + 36.0,
                y: self.y + (self.h - 11.0) / 2.0 - 2.0,
                font_size: 11.0,
                color: [0x73, 0x73, 0x7a],
            });
        }
        labels.push(TextLabel {
            text: "-".to_string(),
            x: self.x + self.w * 0.6625 - 4.0,
            y: self.y + (self.h - 12.0) / 2.0 - 2.0,
            font_size: 12.0,
            color: [0xcc, 0xcc, 0xd4],
        });
        labels.push(TextLabel {
            text: "+".to_string(),
            x: self.x + self.w * 0.8875 - 4.0,
            y: self.y + (self.h - 12.0) / 2.0 - 2.0,
            font_size: 12.0,
            color: [0xcc, 0xcc, 0xd4],
        });
        labels
    }

    fn parent(&self) -> Option<*mut (dyn Widget + 'static)> { self.parent }
    fn set_parent(&mut self, parent: Option<*mut (dyn Widget + 'static)>) { self.parent = parent; }
    fn children(&self) -> Vec<*mut (dyn Widget + 'static)> { self.children.clone() }
    fn add_child(&mut self, child: *mut (dyn Widget + 'static)) { self.children.push(child); }
    fn clear_children(&mut self) { self.children.clear(); }
}

impl Drop for Spinbox {
    fn drop(&mut self) {
        focus::clear_if_matches(self);
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
    row_x: f32,
    row_w: f32,
    pub command: String,
    pub parent: Option<*mut (dyn Widget + 'static)>,
    pub children: Vec<*mut (dyn Widget + 'static)>,
}

impl ColorSelector {
    pub fn new(color: [u8; 3]) -> Self {
        Self {
            x: 0.0, y: 0.0, w: 0.0, h: 0.0,
            color,
            just_clicked: false,
            hovered: false,
            editing: false,
            edit_buffer: String::new(),
            label: None,
            row_x: 0.0,
            row_w: 0.0,
            command: "clear-color-interface".to_string(),
            parent: None,
            children: Vec::new(),
        }
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.label = Some(label.to_string());
        self
    }

    pub fn with_command(mut self, command: &str) -> Self {
        self.command = command.to_string();
        self
    }
}

impl Widget for ColorSelector {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn set_row_rect(&mut self, x: f32, w: f32) { self.row_x = x; self.row_w = w; }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }

    fn hit_test(&self, px: f32, py: f32) -> bool {
        let (x, y, w, h) = self.rect();
        let hx = if self.row_w > 0.0 { self.row_x } else { x };
        let hw = if self.row_w > 0.0 { self.row_w } else { w };
        let (hy, hh) = if self.label.is_some() {
            (y - 20.0, h + 20.0)
        } else {
            (y, h)
        };
        px >= hx && px <= hx + hw && py >= hy && py <= hy + hh
    }

    fn top_room(&self) -> f32 { if self.label.is_some() { 16.0 } else { 0.0 } }

    fn color(&self) -> [f32; 4] {
        colors::to_linear([
            self.color[0] as f32 / 255.0,
            self.color[1] as f32 / 255.0,
            self.color[2] as f32 / 255.0,
            1.0,
        ])
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
            let hex = format!("#{:02x}{:02x}{:02x}", self.color[0], self.color[1], self.color[2]);
            if let Ok(output) = std::process::Command::new(&self.command)
                .arg(&hex)
                .output()
            {
                let stdout_str = String::from_utf8_lossy(&output.stdout);
                let mut found_color = None;
                for line in stdout_str.lines().rev() {
                    if let Some(c) = parse_hex(line.trim()) {
                        found_color = Some(c);
                        break;
                    }
                }
                if let Some(new_color) = found_color {
                    self.color = new_color;
                }
            }
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
        focus::set_focused(self);
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
            let hx = if self.row_w > 0.0 { self.row_x } else { self.x };
            let hw = if self.row_w > 0.0 { self.row_w } else { self.w };
            quads.push((hx, hy, hw, hh, [1.0, 1.0, 1.0, 0.06]));
        }
        let pick_x = self.x + self.w * 0.65;
        let pick_w = self.w * 0.35;
        let linear_c = colors::to_linear([
            self.color[0] as f32 / 255.0,
            self.color[1] as f32 / 255.0,
            self.color[2] as f32 / 255.0,
            1.0,
        ]);
        quads.push((pick_x, self.y, pick_w, self.h, linear_c));
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

    fn parent(&self) -> Option<*mut (dyn Widget + 'static)> { self.parent }
    fn set_parent(&mut self, parent: Option<*mut (dyn Widget + 'static)>) { self.parent = parent; }
    fn children(&self) -> Vec<*mut (dyn Widget + 'static)> { self.children.clone() }
    fn add_child(&mut self, child: *mut (dyn Widget + 'static)) { self.children.push(child); }
    fn clear_children(&mut self) { self.children.clear(); }
}

impl Drop for ColorSelector {
    fn drop(&mut self) {
        focus::clear_if_matches(self);
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

pub struct Spreadsheet {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    hovered: bool,
    visible: bool,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    scroll_y: f32,
    scroll_velocity: f32,
    dragging_scrollbar: bool,
    drag_offset_y: f32,
    scrollbar_hovered: bool,
    scrollbar_thumb_hovered: bool,
}

impl Spreadsheet {
    pub fn new() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            hovered: false,
            visible: false,
            headers: Vec::new(),
            rows: Vec::new(),
            scroll_y: 0.0,
            scroll_velocity: 0.0,
            dragging_scrollbar: false,
            drag_offset_y: 0.0,
            scrollbar_hovered: false,
            scrollbar_thumb_hovered: false,
        }
    }
}

impl Widget for Spreadsheet {
    fn rect(&self) -> (f32, f32, f32, f32) {
        if !self.visible {
            (0.0, 0.0, 0.0, 0.0)
        } else {
            (self.x, self.y, self.w, self.h)
        }
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.x = x;
        self.y = y;
        self.w = w;
        self.h = h;
    }

    fn color(&self) -> [f32; 4] {
        if !self.visible {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            colors::PARAM_BG
        }
    }

    fn set_hovered(&mut self, v: bool) {
        self.hovered = v;
    }

    fn hovered(&self) -> bool {
        self.hovered
    }

    fn hit_test(&self, px: f32, py: f32) -> bool {
        if !self.visible {
            return false;
        }
        let (rx, ry, rw, rh) = self.rect();
        px >= rx && px <= rx + rw && py >= ry && py <= ry + rh
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn visible(&self) -> bool {
        self.visible
    }

    fn set_spreadsheet_data(&mut self, headers: Vec<String>, rows: Vec<Vec<String>>) {
        self.headers = headers;
        self.rows = rows;
        
        // Clamp scroll_y to new bounds
        let content_h = self.rows.len() as f32 * 24.0;
        let visible_h = (self.h - 24.0).max(0.0);
        let max_scroll_y = (content_h - visible_h).max(0.0);
        self.scroll_y = self.scroll_y.clamp(0.0, max_scroll_y);
    }

    fn cursor_moved(&mut self, px: f32, py: f32) -> bool {
        let was_hovered = self.hovered;
        self.hovered = self.hit_test(px, py);

        let was_sb_hovered = self.scrollbar_hovered;
        let was_thumb_hovered = self.scrollbar_thumb_hovered;

        let content_h = self.rows.len() as f32 * 24.0;
        let visible_h = (self.h - 24.0).max(0.0);
        if visible_h > 0.0 && content_h > visible_h {
            let scrollbar_w = 6.0;
            let scrollbar_padding = 2.0;
            let scrollbar_x = self.x + self.w - scrollbar_w - scrollbar_padding;
            let track_y = self.y + 24.0;

            self.scrollbar_hovered = px >= scrollbar_x - 2.0 && px <= self.x + self.w
                && py >= track_y && py <= self.y + self.h;

            let thumb_h = ((visible_h / content_h) * visible_h).clamp(15.0_f32.min(visible_h), visible_h);
            let max_scroll_y = content_h - visible_h;
            let scroll_ratio = self.scroll_y / max_scroll_y;
            let track_scroll_range = visible_h - thumb_h;
            let thumb_y = track_y + scroll_ratio * track_scroll_range;

            self.scrollbar_thumb_hovered = px >= scrollbar_x - 2.0 && px <= self.x + self.w
                && py >= thumb_y && py <= thumb_y + thumb_h;
        } else {
            self.scrollbar_hovered = false;
            self.scrollbar_thumb_hovered = false;
        }

        was_hovered != self.hovered
            || was_sb_hovered != self.scrollbar_hovered
            || was_thumb_hovered != self.scrollbar_thumb_hovered
    }

    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32) -> bool {
        if !self.visible {
            return false;
        }
        if self.hit_test(px, py) {
            let content_h = self.rows.len() as f32 * 24.0;
            let visible_h = (self.h - 24.0).max(0.0);
            if visible_h > 0.0 && content_h > visible_h {
                let scroll_amount = match delta {
                    MouseScrollDelta::LineDelta(_x, y) => *y * 24.0,
                    MouseScrollDelta::PixelDelta(pos) => pos.y as f32,
                };
                self.scroll_velocity += scroll_amount * 12.0;
                return true;
            }
        }
        false
    }

    fn draggable(&self) -> bool {
        if !self.visible {
            return false;
        }
        let content_h = self.rows.len() as f32 * 24.0;
        let visible_h = (self.h - 24.0).max(0.0);
        visible_h > 0.0 && content_h > visible_h
    }

    fn is_dragging(&self) -> bool {
        self.dragging_scrollbar
    }

    fn drag_begin(&mut self, px: f32, py: f32) {
        self.scroll_velocity = 0.0;
        let content_h = self.rows.len() as f32 * 24.0;
        let visible_h = (self.h - 24.0).max(0.0);
        if visible_h > 0.0 && content_h > visible_h {
            let scrollbar_w = 6.0;
            let scrollbar_padding = 2.0;
            let scrollbar_x = self.x + self.w - scrollbar_w - scrollbar_padding;
            let track_y = self.y + 24.0;

            if px >= scrollbar_x - 4.0 && px <= self.x + self.w
                && py >= track_y && py <= self.y + self.h
            {
                self.dragging_scrollbar = true;

                let thumb_h = ((visible_h / content_h) * visible_h).clamp(15.0_f32.min(visible_h), visible_h);
                let max_scroll_y = content_h - visible_h;
                let scroll_ratio = self.scroll_y / max_scroll_y;
                let track_scroll_range = visible_h - thumb_h;
                let thumb_y = track_y + scroll_ratio * track_scroll_range;

                if py >= thumb_y && py <= thumb_y + thumb_h {
                    self.drag_offset_y = py - thumb_y;
                } else {
                    self.drag_offset_y = thumb_h / 2.0;
                    let new_thumb_y = py - self.drag_offset_y;
                    let scroll_ratio = if track_scroll_range > 0.0 {
                        ((new_thumb_y - track_y) / track_scroll_range).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    self.scroll_y = scroll_ratio * max_scroll_y;
                }
            }
        }
    }

    fn drag_update(&mut self, _px: f32, py: f32) -> bool {
        if self.dragging_scrollbar {
            self.scroll_velocity = 0.0;
            let content_h = self.rows.len() as f32 * 24.0;
            let visible_h = (self.h - 24.0).max(0.0);
            if visible_h > 0.0 && content_h > visible_h {
                let thumb_h = ((visible_h / content_h) * visible_h).clamp(15.0_f32.min(visible_h), visible_h);
                let max_scroll_y = content_h - visible_h;
                let track_y = self.y + 24.0;
                let track_scroll_range = visible_h - thumb_h;

                let new_thumb_y = py - self.drag_offset_y;
                let scroll_ratio = if track_scroll_range > 0.0 {
                    ((new_thumb_y - track_y) / track_scroll_range).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let old_scroll_y = self.scroll_y;
                self.scroll_y = scroll_ratio * max_scroll_y;

                return (self.scroll_y - old_scroll_y).abs() > 0.01;
            }
        }
        false
    }

    fn drag_end(&mut self) {
        self.dragging_scrollbar = false;
        self.scroll_velocity = 0.0;
    }

    fn tick(&mut self, dt: f32) -> bool {
        if self.scroll_velocity.abs() > 0.01 {
            let content_h = self.rows.len() as f32 * 24.0;
            let visible_h = (self.h - 24.0).max(0.0);
            let max_scroll_y = (content_h - visible_h).max(0.0);
            let old_scroll_y = self.scroll_y;

            self.scroll_y = (self.scroll_y + self.scroll_velocity * dt).clamp(0.0, max_scroll_y);

            // Decelerate with friction (exponential decay)
            let friction = 8.0;
            self.scroll_velocity *= (-friction * dt).exp();

            if self.scroll_y == 0.0 || self.scroll_y == max_scroll_y {
                self.scroll_velocity = 0.0;
            }

            if self.scroll_velocity.abs() < 5.0 {
                self.scroll_velocity = 0.0;
            }

            (self.scroll_y - old_scroll_y).abs() > 0.01
        } else {
            false
        }
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.visible {
            return Vec::new();
        }
        let mut quads = Vec::new();
        
        // Header bg
        quads.push((self.x, self.y, self.w, 24.0, [0.12, 0.12, 0.16, 0.4]));

        // Zebra rows
        let row_h = 24.0;
        let body_top = self.y + 24.0;
        let body_bottom = self.y + self.h;
        for i in 0..self.rows.len() {
            let ry = self.y + 24.0 + i as f32 * row_h - self.scroll_y;
            if ry + row_h <= body_top || ry >= body_bottom {
                continue;
            }
            let draw_y = ry.max(body_top);
            let draw_h = (ry + row_h).min(body_bottom) - draw_y;
            if draw_h > 0.0 {
                let row_color = if i % 2 == 0 {
                    [0.10, 0.10, 0.13, 0.15]
                } else {
                    [0.08, 0.08, 0.11, 0.05]
                };
                quads.push((self.x, draw_y, self.w, draw_h, row_color));

                // Horizontal row separator
                let sep_y = ry + row_h;
                if sep_y >= body_top && sep_y < body_bottom {
                    quads.push((self.x, sep_y, self.w, 1.0, [0.20, 0.20, 0.25, 0.15]));
                }
            }
        }

        // Header separator
        quads.push((self.x, self.y + 24.0, self.w, 1.0, [0.20, 0.20, 0.25, 0.25]));

        // Vertical separators
        let divider_h = self.h;
        if divider_h > 0.0 && !self.headers.is_empty() {
            let n_cols = self.headers.len();
            for i in 1..n_cols {
                let r = i as f32 / n_cols as f32;
                quads.push((self.x + self.w * r, self.y, 1.0, divider_h, [0.20, 0.20, 0.25, 0.15]));
            }
        }

        // Scrollbar track & thumb
        let content_h = self.rows.len() as f32 * row_h;
        let visible_h = (self.h - 24.0).max(0.0);
        if visible_h > 0.0 && content_h > visible_h {
            let scrollbar_w = 6.0;
            let scrollbar_padding = 2.0;
            let scrollbar_x = self.x + self.w - scrollbar_w - scrollbar_padding;
            let track_y = self.y + 24.0;
            let track_h = visible_h;

            // Track BG
            quads.push((scrollbar_x, track_y, scrollbar_w, track_h, [0.05, 0.05, 0.08, 0.15]));

            // Thumb
            let thumb_h = ((visible_h / content_h) * visible_h).clamp(15.0_f32.min(visible_h), visible_h);
            let max_scroll_y = content_h - visible_h;
            let scroll_ratio = self.scroll_y / max_scroll_y;
            let track_scroll_range = visible_h - thumb_h;
            let thumb_y = track_y + scroll_ratio * track_scroll_range;

            let thumb_color = if self.dragging_scrollbar {
                [0.40, 0.40, 0.48, 1.0]
            } else if self.scrollbar_thumb_hovered {
                [0.32, 0.32, 0.38, 1.0]
            } else if self.scrollbar_hovered {
                [0.24, 0.24, 0.30, 0.9]
            } else {
                [0.18, 0.18, 0.24, 0.7]
            };

            quads.push((scrollbar_x, thumb_y, scrollbar_w, thumb_h, thumb_color));
        }

        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        if !self.visible {
            return Vec::new();
        }
        let mut labels = Vec::new();
        if self.headers.is_empty() {
            return labels;
        }

        let n_cols = self.headers.len();
        for (i, header) in self.headers.iter().enumerate() {
            let cx = self.x + self.w * (i as f32 / n_cols as f32) + 8.0;
            labels.push(TextLabel {
                text: header.clone(),
                x: cx,
                y: self.y + 6.0,
                font_size: 12.0,
                color: [0xdd, 0xdd, 0xee],
            });
        }

        let row_h = 24.0;
        let body_top = self.y + 24.0;
        let body_bottom = self.y + self.h;
        for (i, row) in self.rows.iter().enumerate() {
            let ry = self.y + 24.0 + i as f32 * row_h - self.scroll_y;
            // Only show text if the row is fully inside the spreadsheet body
            if ry < body_top || ry + row_h > body_bottom {
                continue;
            }

            for (col_idx, val) in row.iter().enumerate().take(n_cols) {
                let cx = self.x + self.w * (col_idx as f32 / n_cols as f32) + 8.0;
                labels.push(TextLabel {
                    text: val.clone(),
                    x: cx,
                    y: ry + 6.0,
                    font_size: 12.0,
                    color: [0xbb, 0xbb, 0xcc],
                });
            }
        }
        labels
    }
}

#[derive(Debug, Clone)]
pub struct ScrollBox {
    x: f32, y: f32, w: f32, h: f32,
    pub scroll_y: f32,
    pub content_h: f32,
    pub viewport_y: f32,
    pub viewport_h: f32,
    hovered: bool,
    pub parent: Option<*mut (dyn Widget + 'static)>,
    pub children: Vec<*mut (dyn Widget + 'static)>,
}

impl ScrollBox {
    pub fn new() -> Self {
        Self {
            x: 0.0, y: 0.0, w: 0.0, h: 0.0,
            scroll_y: 0.0,
            content_h: 0.0,
            viewport_y: 0.0,
            viewport_h: 0.0,
            hovered: false,
            parent: None,
            children: Vec::new(),
        }
    }

    pub fn update_bounds(&mut self, content_h: f32, viewport_y: f32, viewport_h: f32) {
        self.content_h = content_h;
        self.viewport_y = viewport_y;
        self.viewport_h = viewport_h;
        let max_scroll = (content_h - viewport_h).max(0.0);
        self.scroll_y = self.scroll_y.clamp(0.0, max_scroll);
    }

    pub fn get_item_draw_y(&self, virtual_y: f32, item_h: f32) -> Option<f32> {
        let draw_y = self.viewport_y + virtual_y - self.scroll_y;
        if draw_y >= self.viewport_y - 1.0 && draw_y + item_h <= self.viewport_y + self.viewport_h + 1.0 {
            Some(draw_y)
        } else {
            None
        }
    }
}

impl Widget for ScrollBox {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn color(&self) -> [f32; 4] { [0.08, 0.08, 0.12, 0.3] }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }
    fn hover_highlight(&self) -> Option<[f32; 4]> { None }

    fn focus(&mut self) {
        focus::set_focused(self);
    }
    fn unfocus(&mut self) {}

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button == MouseButton::Left && state == ElementState::Pressed {
            if self.hit_test(px, py) {
                self.focus();
                return true;
            }
        }
        false
    }

    fn cursor_moved(&mut self, px: f32, py: f32) -> bool {
        let was = self.hovered;
        self.hovered = self.hit_test(px, py);
        was != self.hovered
    }

    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32) -> bool {
        if self.hit_test(px, py) {
            let scroll_speed = 24.0;
            let dy = match delta {
                MouseScrollDelta::LineDelta(_, y) => -y * scroll_speed,
                MouseScrollDelta::PixelDelta(pos) => -pos.y as f32,
            };
            let old_scroll = self.scroll_y;
            let max_scroll = (self.content_h - self.viewport_h).max(0.0);
            self.scroll_y = (self.scroll_y + dy).clamp(0.0, max_scroll);
            (self.scroll_y - old_scroll).abs() > 0.01
        } else {
            false
        }
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        
        // Background
        quads.push((self.x, self.y, self.w, self.h, [0.08, 0.08, 0.12, 0.3]));

        // Border lines
        let box_border_color = if focus::is_focused(self) {
            [0.30, 0.50, 0.32, 1.0] // Focused green
        } else if self.hovered {
            [0.25, 0.25, 0.35, 1.0] // Hovered
        } else {
            [0.18, 0.18, 0.24, 1.0] // Default
        };
        quads.push((self.x, self.y, self.w, 1.0, box_border_color)); // Top
        quads.push((self.x, self.y + self.h - 1.0, self.w, 1.0, box_border_color)); // Bottom
        quads.push((self.x, self.y, 1.0, self.h, box_border_color)); // Left
        quads.push((self.x + self.w - 1.0, self.y, 1.0, self.h, box_border_color)); // Right

        // Scrollbar
        if self.content_h > self.viewport_h {
            let sb_x = self.x + self.w - 8.0;
            let sb_w = 4.0;
            let sb_track_h = self.viewport_h - 8.0;
            let sb_track_y = self.viewport_y + 4.0;

            // Track
            quads.push((sb_x, sb_track_y, sb_w, sb_track_h, [0.15, 0.15, 0.20, 0.3]));

            // Thumb
            let visible_ratio = self.viewport_h / self.content_h;
            let thumb_h = (sb_track_h * visible_ratio).clamp(20.0, sb_track_h);
            let max_scroll = (self.content_h - self.viewport_h).max(0.0);
            let scroll_ratio = if max_scroll > 0.0 { self.scroll_y / max_scroll } else { 0.0 };
            let thumb_y = sb_track_y + scroll_ratio * (sb_track_h - thumb_h);

            quads.push((sb_x, thumb_y, sb_w, thumb_h, [0.60, 0.60, 0.65, 0.4]));
        }

        quads
    }

    fn keyboard_input(&mut self, event: &KeyEvent) -> bool {
        if !focus::is_focused(self) {
            return false;
        }
        if event.state != ElementState::Pressed {
            return false;
        }
        if event.ctrl {
            match &event.logical_key {
                Key::Character(c) if c == "n" || c == "N" => {
                    let old_scroll = self.scroll_y;
                    let max_scroll = (self.content_h - self.viewport_h).max(0.0);
                    self.scroll_y = (self.scroll_y + 24.0).clamp(0.0, max_scroll);
                    (self.scroll_y - old_scroll).abs() > 0.01
                }
                Key::Character(c) if c == "p" || c == "P" => {
                    let old_scroll = self.scroll_y;
                    let max_scroll = (self.content_h - self.viewport_h).max(0.0);
                    self.scroll_y = (self.scroll_y - 24.0).clamp(0.0, max_scroll);
                    (self.scroll_y - old_scroll).abs() > 0.01
                }
                _ => false,
            }
        } else {
            match &event.logical_key {
                Key::Named(NamedKey::ArrowDown) => {
                    let old_scroll = self.scroll_y;
                    let max_scroll = (self.content_h - self.viewport_h).max(0.0);
                    self.scroll_y = (self.scroll_y + 24.0).clamp(0.0, max_scroll);
                    (self.scroll_y - old_scroll).abs() > 0.01
                }
                Key::Named(NamedKey::ArrowUp) => {
                    let old_scroll = self.scroll_y;
                    let max_scroll = (self.content_h - self.viewport_h).max(0.0);
                    self.scroll_y = (self.scroll_y - 24.0).clamp(0.0, max_scroll);
                    (self.scroll_y - old_scroll).abs() > 0.01
                }
                _ => false,
            }
        }
    }

    fn parent(&self) -> Option<*mut (dyn Widget + 'static)> { self.parent }
    fn set_parent(&mut self, parent: Option<*mut (dyn Widget + 'static)>) { self.parent = parent; }
    fn children(&self) -> Vec<*mut (dyn Widget + 'static)> { self.children.clone() }
    fn add_child(&mut self, child: *mut (dyn Widget + 'static)) { self.children.push(child); }
    fn clear_children(&mut self) { self.children.clear(); }
}

impl Drop for ScrollBox {
    fn drop(&mut self) {
        focus::clear_if_matches(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;


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

    #[test]
    fn test_spreadsheet_dynamic() {
        let mut spreadsheet = Spreadsheet::new();
        spreadsheet.set_rect(10.0, 10.0, 100.0, 200.0);
        spreadsheet.set_visible(true);

        // Initially empty
        assert!(spreadsheet.headers.is_empty());
        assert!(spreadsheet.rows.is_empty());
        assert!(spreadsheet.text_labels().is_empty());

        // Set dynamic headers and rows
        let headers = vec!["ColA".to_string(), "ColB".to_string()];
        let rows = vec![
            vec!["Val1".to_string(), "Val2".to_string()],
            vec!["Val3".to_string(), "Val4".to_string()],
        ];
        spreadsheet.set_spreadsheet_data(headers, rows);

        assert_eq!(spreadsheet.headers.len(), 2);
        assert_eq!(spreadsheet.rows.len(), 2);

        // Verify labels generated
        let labels = spreadsheet.text_labels();
        // 2 headers + 4 cell values = 6 labels total
        assert_eq!(labels.len(), 6);
        assert_eq!(labels[0].text, "ColA");
        assert_eq!(labels[1].text, "ColB");
        assert_eq!(labels[2].text, "Val1");
        assert_eq!(labels[3].text, "Val2");
        assert_eq!(labels[4].text, "Val3");
        assert_eq!(labels[5].text, "Val4");

        // Verify positions are correct (col 0 starts at x = 10.0 + 8.0 = 18.0)
        assert_eq!(labels[0].x, 18.0);
        // Col 1 starts at x = 10.0 + 100.0 * 0.5 + 8.0 = 68.0
        assert_eq!(labels[1].x, 68.0);
        assert_eq!(labels[2].x, 18.0);
        assert_eq!(labels[3].x, 68.0);
    }

    #[test]
    fn test_spreadsheet_scrolling() {
        let mut spreadsheet = Spreadsheet::new();
        // Visible height is 100px. Header is 24px, so body is 76px.
        spreadsheet.set_rect(0.0, 0.0, 100.0, 100.0);
        spreadsheet.set_visible(true);

        let headers = vec!["ColA".to_string()];
        // Each row is 24px. With 10 rows, content_h = 240px.
        let mut rows = Vec::new();
        for i in 0..10 {
            rows.push(vec![format!("Row{}", i)]);
        }
        spreadsheet.set_spreadsheet_data(headers, rows);

        // Content height is 240px, visible height is 100px (body is 76px).
        // Since content height > visible body height, it should be draggable.
        assert!(spreadsheet.draggable());

        // Max scroll height = 240.0 - 76.0 = 164.0
        
        // Initial scroll position should be 0.0
        assert_eq!(spreadsheet.scroll_y, 0.0);

        // Scroll down via mouse wheel (positive delta scrolls content down, scroll_y increases via tick)
        let delta = MouseScrollDelta::LineDelta(0.0, 2.0);
        // Mouse over spreadsheet (50, 50)
        let changed = spreadsheet.mouse_wheel(&delta, 50.0, 50.0);
        assert!(changed);
        assert_eq!(spreadsheet.scroll_y, 0.0);
        assert!(spreadsheet.scroll_velocity > 0.0);

        // Tick to apply velocity
        let mut ticked_change = false;
        for _ in 0..100 {
            if spreadsheet.tick(0.016) {
                ticked_change = true;
            }
        }
        assert!(ticked_change);
        assert!(spreadsheet.scroll_y > 0.0);
        assert_eq!(spreadsheet.scroll_velocity, 0.0);

        // Scroll back to top
        let delta_up = MouseScrollDelta::LineDelta(0.0, -10.0);
        spreadsheet.mouse_wheel(&delta_up, 50.0, 50.0);
        assert!(spreadsheet.scroll_velocity < 0.0);

        // Tick back to top
        for _ in 0..100 {
            spreadsheet.tick(0.016);
        }
        assert_eq!(spreadsheet.scroll_y, 0.0);
        assert_eq!(spreadsheet.scroll_velocity, 0.0);

        // Drag test
        // Scrollbar width is 6px. Padding is 2px. Width is 100px.
        // Scrollbar track x is from 92px to 98px.
        // Let's drag. Click at (94, 50).
        spreadsheet.drag_begin(94.0, 50.0);
        assert!(spreadsheet.is_dragging());

        // Update drag to y = 80
        let changed_drag = spreadsheet.drag_update(94.0, 80.0);
        assert!(changed_drag);
        assert!(spreadsheet.scroll_y > 0.0);

        // End drag
        spreadsheet.drag_end();
        assert!(!spreadsheet.is_dragging());
    }

    #[test]
    fn test_spreadsheet_zero_height_no_panic() {
        let mut spreadsheet = Spreadsheet::new();
        // Visible height is set to 0.0
        spreadsheet.set_rect(0.0, 0.0, 100.0, 0.0);
        spreadsheet.set_visible(true);

        let headers = vec!["ColA".to_string()];
        let mut rows = Vec::new();
        for i in 0..10 {
            rows.push(vec![format!("Row{}", i)]);
        }
        // This should not panic
        spreadsheet.set_spreadsheet_data(headers, rows);

        // This should not panic
        spreadsheet.cursor_moved(50.0, 50.0);
        
        let delta = MouseScrollDelta::LineDelta(0.0, -2.0);
        // This should not panic
        spreadsheet.mouse_wheel(&delta, 50.0, 50.0);

        // This should not panic
        assert!(!spreadsheet.draggable());

        // This should not panic
        spreadsheet.drag_begin(94.0, 50.0);
        spreadsheet.drag_update(94.0, 80.0);
        spreadsheet.drag_end();

        // This should not panic and return empty quads for scrollbar
        let _quads = spreadsheet.extra_quads();
        // The header quad and divider (if any) are drawn, but scrollbar is not
        // Let's verify that the scrollbar was not drawn
        // (the last quad would be the scrollbar thumb with thumb_color if drawn,
        // but here scrollbar track & thumb shouldn't be added)
        assert_eq!(spreadsheet.scroll_y, 0.0);
        
        // Let's check text labels (should be empty because self.h is 0)
        let labels = spreadsheet.text_labels();
        // Headers labels are still generated since they don't depend on scroll/height,
        // but rows shouldn't be
        assert_eq!(labels.len(), 1); // Only header ColA
    }

    #[test]
    fn test_scroll_box_bounds_scrolling() {
        let mut sb = ScrollBox::new();
        sb.set_rect(10.0, 20.0, 100.0, 100.0);
        
        // 1. Initially scroll is 0
        assert_eq!(sb.scroll_y, 0.0);

        // 2. Update bounds: content_h = 150 (greater than viewport_h = 100)
        sb.update_bounds(150.0, 20.0, 100.0);
        assert_eq!(sb.scroll_y, 0.0);
        assert_eq!(sb.content_h, 150.0);
        assert_eq!(sb.viewport_h, 100.0);

        // 3. Scroll inside bounds
        let delta = MouseScrollDelta::LineDelta(0.0, -2.0); // scroll down by 2 lines (48px)
        let changed = sb.mouse_wheel(&delta, 50.0, 50.0);
        assert!(changed);
        assert_eq!(sb.scroll_y, 48.0);

        // 4. Clamps at max scroll: 150 - 100 = 50
        let delta_large = MouseScrollDelta::LineDelta(0.0, -10.0);
        sb.mouse_wheel(&delta_large, 50.0, 50.0);
        assert_eq!(sb.scroll_y, 50.0);

        // 5. Test item draw coordinates
        // Virtual item at virtual_y = 10, item_h = 24
        // Screen draw y = viewport_y + virtual_y - scroll_y = 20 + 10 - 50 = -20
        // -20 < viewport_y + 2.0 (22.0), so it should return None (not visible)
        assert!(sb.get_item_draw_y(10.0, 24.0).is_none());

        // Virtual item at virtual_y = 60, item_h = 24
        // Screen draw y = 20 + 60 - 50 = 30
        // 30 >= 22.0 and 30 + 24 <= 118.0, so it should return Some(30.0)
        assert_eq!(sb.get_item_draw_y(60.0, 24.0), Some(30.0));
    }

    #[test]
    fn test_dropdown_widget_interaction() {
        let options = vec!["Option A".to_string(), "Option B".to_string(), "Option C".to_string()];
        let mut dd = Dropdown::new(options, 0);
        dd.set_rect(10.0, 10.0, 100.0, 24.0);

        // 1. Initial State
        assert!(!dd.open);
        assert_eq!(dd.selected, 0);

        // 2. Click trigger area opens dropdown
        let input_changed = dd.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 20.0);
        assert!(input_changed);
        assert!(dd.open);

        // 3. Hovering options inside popover
        // Popover starts at y = 10 + 24 = 34. Options are of height 24 each.
        // Hover option B at y = 34 + 24 + 12 = 70.0
        let move_changed = dd.cursor_moved(50.0, 70.0);
        assert!(move_changed);
        assert_eq!(dd.hovered_item, Some(1));

        // 4. Click option B selects it and closes dropdown
        let select_changed = dd.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 70.0);
        assert!(select_changed);
        assert!(!dd.open);
        assert_eq!(dd.selected, 1);
        assert!(dd.take_change());
    }

    #[test]
    fn test_textbox_selection_highlight() {
        let mut tb = TextBox::new("Initial Text".to_string());
        tb.set_rect(10.0, 10.0, 200.0, 30.0);

        // 1. Initial state
        assert!(!tb.editing);
        assert!(!tb.all_selected);

        // 2. Click focuses and triggers highlighting
        let clicked = tb.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 20.0);
        assert!(clicked);
        assert!(tb.editing);
        assert!(tb.all_selected);
        assert_eq!(tb.edit_buffer, "Initial Text");

        // 3. Typing a key replaces all text
        let key_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Character("A".to_string()),
            text: Some("A".to_string()),
            repeat: false,
            ctrl: false,
            shift: false,
        };
        let handled = tb.keyboard_input(&key_ev);
        assert!(handled);
        assert!(!tb.all_selected);
        assert_eq!(tb.edit_buffer, "A");

        // 4. Pressing Enter commits change
        let enter_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::Enter),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
        };
        let handled_enter = tb.keyboard_input(&enter_ev);
        assert!(handled_enter);
        assert!(!tb.editing);
        assert_eq!(tb.text, "A");
        assert!(tb.take_change());
    }

    #[test]
    fn test_textbox_drag_and_modifier_selection() {
        let mut tb = TextBox::new("Hello World".to_string());
        tb.set_rect(10.0, 10.0, 200.0, 30.0);

        // 1. Initial click focuses and selects all
        let pressed = tb.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 20.0);
        assert!(pressed);
        let released = tb.mouse_input(MouseButton::Left, ElementState::Released, 50.0, 20.0);
        assert!(released);
        assert!(tb.editing);
        assert!(tb.all_selected);
        assert_eq!(tb.cursor_idx, 11);
        assert_eq!(tb.select_anchor, Some(0));

        // 2. Click inside placed caret at index 5 (x = 10 + 8 + 5 * 7.2 = 54)
        let pressed_inside = tb.mouse_input(MouseButton::Left, ElementState::Pressed, 54.0, 20.0);
        assert!(pressed_inside);
        assert_eq!(tb.cursor_idx, 5);
        assert_eq!(tb.select_anchor, Some(5));
        assert!(!tb.all_selected);

        // 3. Drag to index 11 (x = 10 + 8 + 11 * 7.2 = 97.2)
        tb.drag_begin(54.0, 20.0);
        let updated = tb.drag_update(97.2, 20.0);
        assert!(updated);
        assert_eq!(tb.cursor_idx, 11);
        assert_eq!(tb.select_anchor, Some(5));
        tb.drag_end();

        // 4. Keyboard ArrowLeft with Shift shrinks selection from 11 to 10
        let left_shift_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::ArrowLeft),
            text: None,
            repeat: false,
            ctrl: false,
            shift: true,
        };
        let handled = tb.keyboard_input(&left_shift_ev);
        assert!(handled);
        assert_eq!(tb.cursor_idx, 10);
        assert_eq!(tb.select_anchor, Some(5));

        // 5. Keyboard ArrowLeft without Shift collapses selection to start (index 5)
        let left_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::ArrowLeft),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
        };
        let handled = tb.keyboard_input(&left_ev);
        assert!(handled);
        assert_eq!(tb.cursor_idx, 5);
        assert_eq!(tb.select_anchor, None);

        // 6. Keyboard Shift+Up highlights to beginning (cursor 0, anchor 5)
        let up_shift_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::ArrowUp),
            text: None,
            repeat: false,
            ctrl: false,
            shift: true,
        };
        let handled = tb.keyboard_input(&up_shift_ev);
        assert!(handled);
        assert_eq!(tb.cursor_idx, 0);
        assert_eq!(tb.select_anchor, Some(5));

        // 7. Typing a key replaces selected range "Hello" with "Rust"
        let rust_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Character("Rust".to_string()),
            text: Some("Rust".to_string()),
            repeat: false,
            ctrl: false,
            shift: false,
        };
        let handled = tb.keyboard_input(&rust_ev);
        assert!(handled);
        assert_eq!(tb.edit_buffer, "Rust World");
        assert_eq!(tb.cursor_idx, 4);
        assert_eq!(tb.select_anchor, None);
    }
}

// Generic text item layout wrapper
#[derive(Debug)]
pub struct TextItem {
    pub buffer: glyphon::Buffer,
    pub x: f32,
    pub y: f32,
    pub color: glyphon::Color,
}

// Styled label builder with optional strikethrough
#[derive(Debug)]
pub struct StyledLabel {
    pub buffer: glyphon::Buffer,
    pub w: f32,
    pub color: [f32; 4],
    pub g_color: glyphon::Color,
    pub strikethrough: bool,
    pub strikethrough_color: Option<[f32; 4]>,
}

impl StyledLabel {
    pub fn new(fs: &mut glyphon::FontSystem, text: &str, size: f32, color: [f32; 4]) -> Self {
        Self::new_with_family(fs, text, size, color, "sans-serif")
    }

    pub fn new_with_family(fs: &mut glyphon::FontSystem, text: &str, size: f32, color: [f32; 4], family: &str) -> Self {
        let metrics = glyphon::Metrics::new(size, size * 1.4);
        let mut buffer = glyphon::Buffer::new(fs, metrics);
        let attrs = glyphon::Attrs::new().family(glyphon::Family::Name(family));
        buffer.set_text(fs, text, attrs, glyphon::Shaping::Advanced);
        buffer.shape_until_scroll(fs, true);
        let w = buffer.layout_runs().next().map(|r| r.line_w).unwrap_or(0.0);
        let g_color = glyphon::Color::rgb(
            (color[0] * 255.0) as u8,
            (color[1] * 255.0) as u8,
            (color[2] * 255.0) as u8,
        );
        Self {
            buffer,
            w,
            color,
            g_color,
            strikethrough: false,
            strikethrough_color: None,
        }
    }

    pub fn with_strikethrough(mut self, enabled: bool) -> Self {
        self.strikethrough = enabled;
        self
    }

    pub fn with_strikethrough_color(mut self, color: [f32; 4]) -> Self {
        self.strikethrough_color = Some(color);
        self
    }

    pub fn draw(self, text_items: &mut Vec<TextItem>, x: f32, y: f32) -> f32 {
        let w = self.w;
        text_items.push(TextItem {
            buffer: self.buffer,
            x,
            y,
            color: self.g_color,
        });
        w
    }

    pub fn strikethrough_rect(&self, x: f32, y: f32, scale: f32) -> Option<(f32, f32, f32, f32, [f32; 4])> {
        if self.strikethrough {
            let col = self.strikethrough_color.unwrap_or(self.color);
            let font_size = self.buffer.metrics().font_size;
            let line_y = self.buffer.layout_runs().next().map(|r| r.line_y).unwrap_or(font_size * 1.05);
            let offset_y = line_y - 0.28 * font_size;
            Some((
                x,
                y + offset_y * scale,
                self.w,
                1.0 * scale,
                col,
            ))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScrollingList {
    pub scroll_box: ScrollBox,
    pub item_height: f32,
    pub item_gap: f32,
}

impl ScrollingList {
    pub fn new(item_height: f32, item_gap: f32) -> Self {
        Self {
            scroll_box: ScrollBox::new(),
            item_height,
            item_gap,
        }
    }

    pub fn update_bounds(&mut self, count: usize, viewport_y: f32, viewport_h: f32) {
        let item_height_full = self.item_height + self.item_gap;
        let content_h = count as f32 * item_height_full;
        self.scroll_box.update_bounds(content_h, viewport_y, viewport_h);
    }

    pub fn get_item_draw_y(&self, idx: usize, offset: f32) -> Option<f32> {
        let item_height_full = self.item_height + self.item_gap;
        let virtual_y = idx as f32 * item_height_full + offset;
        self.scroll_box.get_item_draw_y(virtual_y, self.item_height)
    }

    pub fn scroll_y(&self) -> f32 {
        self.scroll_box.scroll_y
    }

    pub fn set_scroll_y(&mut self, val: f32) {
        self.scroll_box.scroll_y = val;
    }
}

impl Default for ScrollingList {
    fn default() -> Self {
        Self::new(24.0, 4.0)
    }
}

impl Widget for ScrollingList {
    fn rect(&self) -> (f32, f32, f32, f32) {
        self.scroll_box.rect()
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.scroll_box.set_rect(x, y, w, h);
    }

    fn color(&self) -> [f32; 4] {
        self.scroll_box.color()
    }

    fn set_hovered(&mut self, v: bool) {
        self.scroll_box.set_hovered(v);
    }

    fn hovered(&self) -> bool {
        self.scroll_box.hovered()
    }

    fn hover_highlight(&self) -> Option<[f32; 4]> {
        self.scroll_box.hover_highlight()
    }

    fn cursor_moved(&mut self, px: f32, py: f32) -> bool {
        self.scroll_box.cursor_moved(px, py)
    }

    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32) -> bool {
        self.scroll_box.mouse_wheel(delta, px, py)
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        self.scroll_box.mouse_input(button, state, px, py)
    }

    fn focus(&mut self) {
        self.scroll_box.focus();
    }

    fn unfocus(&mut self) {
        self.scroll_box.unfocus();
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        self.scroll_box.extra_quads()
    }

    fn keyboard_input(&mut self, event: &KeyEvent) -> bool {
        self.scroll_box.keyboard_input(event)
    }

    fn parent(&self) -> Option<*mut (dyn Widget + 'static)> { self.scroll_box.parent() }
    fn set_parent(&mut self, parent: Option<*mut (dyn Widget + 'static)>) { self.scroll_box.set_parent(parent); }
    fn children(&self) -> Vec<*mut (dyn Widget + 'static)> { self.scroll_box.children() }
    fn add_child(&mut self, child: *mut (dyn Widget + 'static)) { self.scroll_box.add_child(child); }
    fn clear_children(&mut self) { self.scroll_box.clear_children(); }
}

unsafe impl Send for Container {}
unsafe impl Sync for Container {}
unsafe impl Send for ScrollBox {}
unsafe impl Sync for ScrollBox {}
unsafe impl Send for Spinbox {}
unsafe impl Sync for Spinbox {}
unsafe impl Send for ColorSelector {}
unsafe impl Sync for ColorSelector {}
unsafe impl Send for ScrollingList {}
unsafe impl Sync for ScrollingList {}

// ── Dropdown Widget ──

#[derive(Debug, Clone)]
pub struct Dropdown {
    x: f32, y: f32, w: f32, h: f32,
    pub options: Vec<String>,
    pub selected: usize,
    pub open: bool,
    hovered: bool,
    hovered_item: Option<usize>,
    just_changed: bool,
    label: Option<String>,
    row_x: f32,
    row_w: f32,
    pub parent: Option<*mut (dyn Widget + 'static)>,
    pub children: Vec<*mut (dyn Widget + 'static)>,
}

impl Dropdown {
    pub fn new(options: Vec<String>, selected: usize) -> Self {
        Self {
            x: 0.0, y: 0.0, w: 0.0, h: 0.0,
            options,
            selected,
            open: false,
            hovered: false,
            hovered_item: None,
            just_changed: false,
            label: None,
            row_x: 0.0,
            row_w: 0.0,
            parent: None,
            children: Vec::new(),
        }
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.label = Some(label.to_string());
        self
    }

    pub fn set_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    pub fn take_change(&mut self) -> bool {
        let changed = self.just_changed;
        self.just_changed = false;
        changed
    }

    pub fn render_popover(&self, pc: &mut dyn crate::layout::RenderTarget) {
        if !self.open { return; }
        
        let dy = self.y + self.h;
        let dh = self.options.len() as f32 * 24.0;
        
        pc.rect([0.22, 0.22, 0.28, 1.0], self.x, dy, self.w, dh); // border
        pc.rect([0.06, 0.06, 0.09, 1.0], self.x + 1.0, dy + 1.0, self.w - 2.0, dh - 2.0); // bg
        
        if let Some(h_idx) = self.hovered_item {
            let iy = dy + h_idx as f32 * 24.0;
            pc.rect([0.20, 0.40, 0.65, 0.6], self.x + 2.0, iy + 2.0, self.w - 4.0, 20.0);
        }
        
        for (idx, opt) in self.options.iter().enumerate() {
            let iy = dy + idx as f32 * 24.0 + (24.0 - 12.0) / 2.0;
            let text_color = if self.hovered_item == Some(idx) {
                [0xff, 0xff, 0xff]
            } else if self.selected == idx {
                [0x3a, 0x9a, 0xff]
            } else {
                [0xcc, 0xcc, 0xd4]
            };
            
            pc.text(
                opt,
                self.x + 8.0,
                iy,
                12.0,
                [
                    text_color[0] as f32 / 255.0,
                    text_color[1] as f32 / 255.0,
                    text_color[2] as f32 / 255.0,
                    1.0,
                ],
            );
        }
    }
}

impl Default for Dropdown {
    fn default() -> Self {
        Self::new(Vec::new(), 0)
    }
}

impl Widget for Dropdown {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.x = x; self.y = y; self.w = w; self.h = h; }
    fn set_row_rect(&mut self, x: f32, w: f32) { self.row_x = x; self.row_w = w; }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }

    fn color(&self) -> [f32; 4] {
        [0.08, 0.08, 0.12, 1.0]
    }

    fn hit_test(&self, px: f32, py: f32) -> bool {
        let (x, y, w, h) = self.rect();
        let hx = if self.row_w > 0.0 { self.row_x } else { x };
        let hw = if self.row_w > 0.0 { self.row_w } else { w };
        let (hy, hh) = if self.label.is_some() {
            (y - 18.0, h + 18.0)
        } else {
            (y, h)
        };
        if self.open {
            let dy = y + h;
            let dh = self.options.len() as f32 * 24.0;
            let hit_trigger = px >= hx && px <= hx + hw && py >= hy && py <= hy + hh;
            let hit_popover = px >= x && px <= x + w && py >= dy && py <= dy + dh;
            hit_trigger || hit_popover
        } else {
            px >= hx && px <= hx + hw && py >= hy && py <= hy + hh
        }
    }

    fn top_room(&self) -> f32 { if self.label.is_some() { 18.0 } else { 0.0 } }

    fn cursor_moved(&mut self, px: f32, py: f32) -> bool {
        let was_hovered = self.hovered;
        let was_hovered_item = self.hovered_item;
        
        self.hovered = self.hit_test(px, py);
        self.hovered_item = None;

        if self.open {
            let (x, y, w, h) = self.rect();
            let dy = y + h;
            let dh = self.options.len() as f32 * 24.0;
            if px >= x && px <= x + w && py >= dy && py <= dy + dh {
                let idx = ((py - dy) / 24.0) as usize;
                if idx < self.options.len() {
                    self.hovered_item = Some(idx);
                }
            }
        }

        self.hovered != was_hovered || self.hovered_item != was_hovered_item
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if button != MouseButton::Left || state != ElementState::Pressed { return false; }

        let (x, y, w, h) = self.rect();
        let (hy, hh) = if self.label.is_some() {
            (y - 18.0, h + 18.0)
        } else {
            (y, h)
        };
        let dy = y + h;
        let dh = self.options.len() as f32 * 24.0;

        let inside_trigger = px >= x && px <= x + w && py >= hy && py <= hy + hh;
        let inside_popover = self.open && px >= x && px <= x + w && py >= dy && py <= dy + dh;

        if inside_popover {
            let idx = ((py - dy) / 24.0) as usize;
            if idx < self.options.len() {
                if self.selected != idx {
                    self.selected = idx;
                    self.just_changed = true;
                }
            }
            self.open = false;
            return true;
        }

        if inside_trigger {
            self.open = !self.open;
            if self.open {
                self.focus();
            } else {
                self.unfocus();
            }
            return true;
        }

        if self.open {
            self.open = false;
            return true;
        }

        false
    }

    fn focus(&mut self) {
        focus::set_focused(self);
    }

    fn unfocus(&mut self) {
        self.open = false;
    }

    fn keyboard_input(&mut self, event: &KeyEvent) -> bool {
        if event.state != ElementState::Pressed { return false; }
        if !self.open {
            if let Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) = event.logical_key {
                self.open = true;
                self.hovered_item = Some(self.selected);
                return true;
            }
            return false;
        }
        
        match event.logical_key {
            Key::Named(NamedKey::ArrowDown) => {
                let current = self.hovered_item.unwrap_or(self.selected);
                if current + 1 < self.options.len() {
                    self.hovered_item = Some(current + 1);
                } else {
                    self.hovered_item = Some(0);
                }
                true
            }
            Key::Named(NamedKey::ArrowUp) => {
                let current = self.hovered_item.unwrap_or(self.selected);
                if current > 0 {
                    self.hovered_item = Some(current - 1);
                } else {
                    self.hovered_item = Some(self.options.len() - 1);
                }
                true
            }
            Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                if let Some(idx) = self.hovered_item {
                    if self.selected != idx {
                        self.selected = idx;
                        self.just_changed = true;
                    }
                }
                self.open = false;
                true
            }
            Key::Named(NamedKey::Escape) => {
                self.open = false;
                true
            }
            _ => false
        }
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        
        if self.hovered && !self.open {
            let (hy, hh) = if self.label.is_some() {
                (self.y - 18.0, self.h + 18.0)
            } else {
                (self.y, self.h)
            };
            let hx = if self.row_w > 0.0 { self.row_x } else { self.x };
            let hw = if self.row_w > 0.0 { self.row_w } else { self.w };
            quads.push((hx, hy, hw, hh, [1.0, 1.0, 1.0, 0.04]));
        }

        let bg_color = [0.08, 0.08, 0.12, 1.0];
        let border_color = if self.open {
            [0.30, 0.50, 0.32, 1.0]
        } else if self.hovered {
            [0.25, 0.25, 0.35, 1.0]
        } else {
            [0.18, 0.18, 0.24, 1.0]
        };

        quads.push((self.x, self.y, self.w, self.h, border_color));
        quads.push((self.x + 1.0, self.y + 1.0, self.w - 2.0, self.h - 2.0, bg_color));

        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        
        if let Some(ref label) = self.label {
            labels.push(TextLabel {
                text: label.clone(),
                x: self.x + 4.0,
                y: self.y - 14.0,
                font_size: 11.0,
                color: [0x83, 0x83, 0x8a],
            });
        }

        let selected_text = self.options.get(self.selected).cloned().unwrap_or_default();
        labels.push(TextLabel {
            text: selected_text,
            x: self.x + 8.0,
            y: self.y + (self.h - 12.0) / 2.0,
            font_size: 12.0,
            color: [0xdd, 0xdd, 0xe2],
        });

        labels.push(TextLabel {
            text: "▼".to_string(),
            x: self.x + self.w - 18.0,
            y: self.y + (self.h - 10.0) / 2.0,
            font_size: 10.0,
            color: [0x83, 0x83, 0x8a],
        });

        labels
    }

    fn parent(&self) -> Option<*mut (dyn Widget + 'static)> { self.parent }
    fn set_parent(&mut self, parent: Option<*mut (dyn Widget + 'static)>) { self.parent = parent; }
    fn children(&self) -> Vec<*mut (dyn Widget + 'static)> { self.children.clone() }
    fn add_child(&mut self, child: *mut (dyn Widget + 'static)) { self.children.push(child); }
    fn clear_children(&mut self) { self.children.clear(); }
}

unsafe impl Send for Dropdown {}
unsafe impl Sync for Dropdown {}

// ── TextBox Widget ──

#[derive(Debug, Clone)]
pub struct TextBox {
    x: f32, y: f32, w: f32, h: f32,
    pub text: String,
    pub editing: bool,
    pub edit_buffer: String,
    hovered: bool,
    just_changed: bool,
    label: Option<String>,
    row_x: f32,
    row_w: f32,
    pub disabled: bool,
    pub all_selected: bool,
    pub cursor_idx: usize,
    pub select_anchor: Option<usize>,
    pub dragging: bool,
    pub just_focused: bool,
    pub drag_start_idx: Option<usize>,
    pub parent: Option<*mut (dyn Widget + 'static)>,
    pub children: Vec<*mut (dyn Widget + 'static)>,
    pub max_width: Option<f32>,
    pub width: Option<f32>,
}

impl TextBox {
    pub fn new(text: String) -> Self {
        Self {
            x: 0.0, y: 0.0, w: 0.0, h: 0.0,
            text,
            editing: false,
            edit_buffer: String::new(),
            hovered: false,
            just_changed: false,
            label: None,
            row_x: 0.0,
            row_w: 0.0,
            disabled: false,
            all_selected: false,
            cursor_idx: 0,
            select_anchor: None,
            dragging: false,
            just_focused: false,
            drag_start_idx: None,
            parent: None,
            children: Vec::new(),
            max_width: Some(300.0),
            width: None,
        }
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.label = Some(label.to_string());
        self
    }

    pub fn set_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    pub fn take_change(&mut self) -> bool {
        let changed = self.just_changed;
        self.just_changed = false;
        changed
    }

    pub fn with_max_width(mut self, max_w: Option<f32>) -> Self {
        self.max_width = max_w;
        self
    }

    pub fn set_max_width(&mut self, max_w: Option<f32>) {
        self.max_width = max_w;
    }

    pub fn with_width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }

    pub fn set_width(&mut self, w: f32) {
        self.width = Some(w);
    }

    pub fn copy_selection(&self) {
        let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
        let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);
        if start != end {
            let chars: Vec<char> = self.edit_buffer.chars().collect();
            let selected_text: String = chars[start..end].iter().collect();
            clipboard::copy_to_clipboard(&selected_text);
        }
    }

    pub fn cut_selection(&mut self) -> bool {
        let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
        let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);
        if start != end {
            let chars: Vec<char> = self.edit_buffer.chars().collect();
            let selected_text: String = chars[start..end].iter().collect();
            clipboard::copy_to_clipboard(&selected_text);

            let mut new_buf = String::new();
            for i in 0..start {
                new_buf.push(chars[i]);
            }
            for i in end..chars.len() {
                new_buf.push(chars[i]);
            }
            self.edit_buffer = new_buf;
            self.cursor_idx = start;
            self.select_anchor = None;
            self.all_selected = false;
            true
        } else {
            false
        }
    }

    pub fn paste_from_clipboard(&mut self) -> bool {
        if let Some(text) = clipboard::read_from_clipboard() {
            let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
            let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);
            let chars: Vec<char> = self.edit_buffer.chars().collect();
            let mut new_buf = String::new();
            for i in 0..start {
                new_buf.push(chars[i]);
            }
            let mut inserted_count = 0;
            for ch in text.chars() {
                if !ch.is_control() && ch != '\n' && ch != '\r' {
                    new_buf.push(ch);
                    inserted_count += 1;
                }
            }
            for i in end..chars.len() {
                new_buf.push(chars[i]);
            }
            self.edit_buffer = new_buf;
            self.cursor_idx = start + inserted_count;
            self.select_anchor = None;
            self.all_selected = false;
            true
        } else {
            false
        }
    }

    pub fn select_all(&mut self) {
        let len = self.edit_buffer.chars().count();
        self.select_anchor = Some(0);
        self.cursor_idx = len;
        self.all_selected = len > 0;
    }
}

impl Default for TextBox {
    fn default() -> Self {
        Self::new(String::new())
    }
}

impl Widget for TextBox {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.x = x;
        self.y = y;
        self.w = if let Some(explicit_w) = self.width {
            explicit_w
        } else if let Some(max_w) = self.max_width {
            w.min(max_w)
        } else {
            w
        };
        self.h = h;
    }
    fn set_row_rect(&mut self, x: f32, w: f32) {
        self.row_x = x;
        self.row_w = if let Some(explicit_w) = self.width {
            explicit_w
        } else if let Some(max_w) = self.max_width {
            w.min(max_w)
        } else {
            w
        };
    }
    fn set_hovered(&mut self, v: bool) { self.hovered = v; }
    fn hovered(&self) -> bool { self.hovered }

    fn color(&self) -> [f32; 4] {
        [0.10, 0.10, 0.16, 1.0]
    }

    fn hit_test(&self, px: f32, py: f32) -> bool {
        let (x, y, w, h) = self.rect();
        let hx = if self.row_w > 0.0 { self.row_x } else { x };
        let hw = if self.row_w > 0.0 { self.row_w } else { w };
        let (hy, hh) = if self.label.is_some() {
            (y - 18.0, h + 18.0)
        } else {
            (y, h)
        };
        px >= hx && px <= hx + hw && py >= hy && py <= hy + hh
    }

    fn top_room(&self) -> f32 { if self.label.is_some() { 18.0 } else { 0.0 } }

    fn cursor_moved(&mut self, px: f32, py: f32) -> bool {
        if self.disabled {
            let was = self.hovered;
            self.hovered = false;
            return was;
        }
        let mut changed = false;
        if self.dragging && self.editing {
            let char_width = 7.2;
            let drag_idx = (((px - (self.x + 8.0)) / char_width).round() as isize)
                .max(0)
                .min(self.edit_buffer.chars().count() as isize) as usize;
            if self.cursor_idx != drag_idx {
                self.cursor_idx = drag_idx;
                self.just_focused = false;
                let len = self.edit_buffer.chars().count();
                let start = self.select_anchor.unwrap_or(0).min(self.cursor_idx);
                let end = self.select_anchor.unwrap_or(0).max(self.cursor_idx);
                self.all_selected = start == 0 && end == len && len > 0;
                changed = true;
            }
        }
        let was = self.hovered;
        self.hovered = self.hit_test(px, py);
        if was != self.hovered {
            changed = true;
        }
        changed
    }

    fn draggable(&self) -> bool { !self.disabled }
    fn is_dragging(&self) -> bool { self.dragging }
    fn widget_font(&self) -> Option<String> { Some("monospace".to_string()) }

    fn drag_begin(&mut self, _px: f32, _py: f32) {
        if self.disabled || !self.editing { return; }
        self.dragging = true;
    }

    fn drag_update(&mut self, px: f32, _py: f32) -> bool {
        if self.disabled || !self.editing { return false; }
        let char_width = 7.2;
        let drag_idx = (((px - (self.x + 8.0)) / char_width).round() as isize)
            .max(0)
            .min(self.edit_buffer.chars().count() as isize) as usize;
        if self.cursor_idx != drag_idx {
            self.cursor_idx = drag_idx;
            self.just_focused = false;
            let len = self.edit_buffer.chars().count();
            let start = self.select_anchor.unwrap_or(0).min(self.cursor_idx);
            let end = self.select_anchor.unwrap_or(0).max(self.cursor_idx);
            self.all_selected = start == 0 && end == len && len > 0;
            return true;
        }
        false
    }

    fn drag_end(&mut self) {
        self.dragging = false;
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        if self.disabled { return false; }
        
        let (x, y, w, h) = self.rect();
        let (hy, hh) = if self.label.is_some() {
            (y - 18.0, h + 18.0)
        } else {
            (y, h)
        };
        let is_inside = px >= x && px <= x + w && py >= hy && py <= hy + hh;

        if button == MouseButton::Right {
            if state == ElementState::Pressed {
                if !is_inside {
                    return false;
                }
                
                // Focus the TextBox if it wasn't editing yet
                if !self.editing {
                    self.editing = true;
                    self.edit_buffer = self.text.clone();
                    let len = self.edit_buffer.chars().count();
                    self.cursor_idx = len;
                    self.select_anchor = Some(0);
                    self.all_selected = len > 0;
                    self.just_focused = true;
                    focus::set_focused(self);
                }

                // Place cursor under click if clicked inside the content bounds
                if px >= x && px <= x + w && py >= y && py <= y + h {
                    let char_width = 7.2;
                    let click_idx = (((px - (self.x + 8.0)) / char_width).round() as isize)
                        .max(0)
                        .min(self.edit_buffer.chars().count() as isize) as usize;
                    self.cursor_idx = click_idx;
                    self.select_anchor = Some(click_idx);
                    self.all_selected = false;
                }
                
                // Spawn context menu
                let options = vec![
                    "Cut".to_string(),
                    "Copy".to_string(),
                    "Paste".to_string(),
                    "Select All".to_string(),
                ];
                context_menu::show(px, py, options, self as *mut TextBox);
                return true;
            }
            return false;
        }

        if button != MouseButton::Left { return false; }
        
        let (x, y, w, h) = self.rect();
        let (hy, hh) = if self.label.is_some() {
            (y - 18.0, h + 18.0)
        } else {
            (y, h)
        };
        let is_inside = px >= x && px <= x + w && py >= hy && py <= hy + hh;

        if state == ElementState::Pressed {
            if !is_inside {
                if self.editing {
                    self.unfocus();
                    return true;
                }
                return false;
            }
            
            let char_width = 7.2;
            let click_idx = (((px - (self.x + 8.0)) / char_width).round() as isize)
                .max(0)
                .min(self.edit_buffer.chars().count() as isize) as usize;
            
            if self.editing {
                self.cursor_idx = click_idx;
                self.select_anchor = Some(click_idx);
                self.all_selected = false;
                self.just_focused = false;
                self.drag_start_idx = Some(click_idx);
            } else {
                self.editing = true;
                self.edit_buffer = self.text.clone();
                let len = self.edit_buffer.chars().count();
                self.cursor_idx = len;
                self.select_anchor = Some(0);
                self.all_selected = len > 0;
                self.just_focused = true;
                self.drag_start_idx = Some(click_idx);
                focus::set_focused(self);
            }
            true
        } else if state == ElementState::Released {
            if is_inside && self.editing && self.just_focused {
                let len = self.edit_buffer.chars().count();
                self.cursor_idx = len;
                self.select_anchor = Some(0);
                self.all_selected = len > 0;
            }
            self.just_focused = false;
            self.drag_start_idx = None;
            true
        } else {
            false
        }
    }

    fn focus(&mut self) {
        if self.disabled { return; }
        if !self.editing {
            self.editing = true;
            self.edit_buffer = self.text.clone();
            let len = self.edit_buffer.chars().count();
            self.cursor_idx = len;
            self.select_anchor = Some(0);
            self.all_selected = len > 0;
            self.just_focused = true;
            focus::set_focused(self);
        }
    }

    fn unfocus(&mut self) {
        if self.editing {
            self.editing = false;
            self.all_selected = false;
            self.select_anchor = None;
            self.dragging = false;
            self.just_focused = false;
            self.drag_start_idx = None;
            if self.text != self.edit_buffer {
                self.text = self.edit_buffer.clone();
                self.just_changed = true;
            }
        }
    }

    fn keyboard_input(&mut self, event: &KeyEvent) -> bool {
        if self.disabled { return false; }
        if !self.editing { return false; }
        if event.state != ElementState::Pressed { return false; }
        let char_count = self.edit_buffer.chars().count();
        match &event.logical_key {
            Key::Named(NamedKey::Backspace) => {
                let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
                let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);
                if start != end {
                    let chars: Vec<char> = self.edit_buffer.chars().collect();
                    let mut new_buf = String::new();
                    for i in 0..start {
                        new_buf.push(chars[i]);
                    }
                    for i in end..chars.len() {
                        new_buf.push(chars[i]);
                    }
                    self.edit_buffer = new_buf;
                    self.cursor_idx = start;
                    self.select_anchor = None;
                    self.all_selected = false;
                } else if self.cursor_idx > 0 {
                    let chars: Vec<char> = self.edit_buffer.chars().collect();
                    let mut new_buf = String::new();
                    for i in 0..(self.cursor_idx - 1) {
                        new_buf.push(chars[i]);
                    }
                    for i in self.cursor_idx..chars.len() {
                        new_buf.push(chars[i]);
                    }
                    self.edit_buffer = new_buf;
                    self.cursor_idx -= 1;
                    self.select_anchor = None;
                    self.all_selected = false;
                }
                true
            }
            Key::Named(NamedKey::Delete) => {
                let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
                let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);
                if start != end {
                    let chars: Vec<char> = self.edit_buffer.chars().collect();
                    let mut new_buf = String::new();
                    for i in 0..start {
                        new_buf.push(chars[i]);
                    }
                    for i in end..chars.len() {
                        new_buf.push(chars[i]);
                    }
                    self.edit_buffer = new_buf;
                    self.cursor_idx = start;
                    self.select_anchor = None;
                    self.all_selected = false;
                } else if self.cursor_idx < char_count {
                    let chars: Vec<char> = self.edit_buffer.chars().collect();
                    let mut new_buf = String::new();
                    for i in 0..self.cursor_idx {
                        new_buf.push(chars[i]);
                    }
                    for i in (self.cursor_idx + 1)..chars.len() {
                        new_buf.push(chars[i]);
                    }
                    self.edit_buffer = new_buf;
                    self.select_anchor = None;
                    self.all_selected = false;
                }
                true
            }
            Key::Named(NamedKey::ArrowLeft) => {
                if event.shift {
                    if self.select_anchor.is_none() {
                        self.select_anchor = Some(self.cursor_idx);
                    }
                    if self.cursor_idx > 0 {
                        self.cursor_idx -= 1;
                    }
                } else {
                    let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
                    let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);
                    if start != end {
                        self.cursor_idx = start;
                    } else if self.cursor_idx > 0 {
                        self.cursor_idx -= 1;
                    }
                    self.select_anchor = None;
                }
                let len = self.edit_buffer.chars().count();
                let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
                let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);
                self.all_selected = start == 0 && end == len && len > 0;
                true
            }
            Key::Named(NamedKey::ArrowRight) => {
                if event.shift {
                    if self.select_anchor.is_none() {
                        self.select_anchor = Some(self.cursor_idx);
                    }
                    if self.cursor_idx < char_count {
                        self.cursor_idx += 1;
                    }
                } else {
                    let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
                    let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);
                    if start != end {
                        self.cursor_idx = end;
                    } else if self.cursor_idx < char_count {
                        self.cursor_idx += 1;
                    }
                    self.select_anchor = None;
                }
                let len = self.edit_buffer.chars().count();
                let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
                let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);
                self.all_selected = start == 0 && end == len && len > 0;
                true
            }
            Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::Home) => {
                if event.shift {
                    if self.select_anchor.is_none() {
                        self.select_anchor = Some(self.cursor_idx);
                    }
                    self.cursor_idx = 0;
                } else {
                    self.cursor_idx = 0;
                    self.select_anchor = None;
                }
                let len = self.edit_buffer.chars().count();
                let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
                let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);
                self.all_selected = start == 0 && end == len && len > 0;
                true
            }
            Key::Named(NamedKey::ArrowDown) | Key::Named(NamedKey::End) => {
                if event.shift {
                    if self.select_anchor.is_none() {
                        self.select_anchor = Some(self.cursor_idx);
                    }
                    self.cursor_idx = char_count;
                } else {
                    self.cursor_idx = char_count;
                    self.select_anchor = None;
                }
                let len = self.edit_buffer.chars().count();
                let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
                let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);
                self.all_selected = start == 0 && end == len && len > 0;
                true
            }
            Key::Named(NamedKey::Enter) => {
                self.text = self.edit_buffer.clone();
                self.editing = false;
                self.all_selected = false;
                self.select_anchor = None;
                self.just_changed = true;
                true
            }
            Key::Named(NamedKey::Escape) => {
                self.editing = false;
                self.all_selected = false;
                self.select_anchor = None;
                true
            }
            Key::Character(ref s) if event.ctrl => {
                match s.as_str() {
                    "c" | "C" => {
                        self.copy_selection();
                        true
                    }
                    "x" | "X" => {
                        if self.cut_selection() {
                            self.just_changed = true;
                        }
                        true
                    }
                    "v" | "V" => {
                        if self.paste_from_clipboard() {
                            self.just_changed = true;
                        }
                        true
                    }
                    "a" | "A" => {
                        self.select_all();
                        true
                    }
                    _ => false,
                }
            }
            _ => {
                if event.ctrl {
                    return false;
                }
                if let Some(text) = &event.text {
                    let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
                    let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);
                    let chars: Vec<char> = self.edit_buffer.chars().collect();
                    let mut new_buf = String::new();
                    for i in 0..start {
                        new_buf.push(chars[i]);
                    }
                    let mut inserted_count = 0;
                    for ch in text.chars() {
                        if ch.is_alphanumeric() || ch == ' ' || ch == '-' || ch == '_' || ch == '*' {
                            new_buf.push(ch);
                            inserted_count += 1;
                        }
                    }
                    for i in end..chars.len() {
                        new_buf.push(chars[i]);
                    }
                    self.edit_buffer = new_buf;
                    self.cursor_idx = start + inserted_count;
                    self.select_anchor = None;
                    self.all_selected = false;
                } else {
                    self.select_anchor = None;
                    self.all_selected = false;
                }
                true
            }
        }
    }

    fn hover_highlight(&self) -> Option<[f32; 4]> {
        None
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        if self.disabled {
            quads.push((self.x, self.y, self.w, self.h, [0.12, 0.12, 0.16, 1.0]));
            quads.push((self.x + 1.0, self.y + 1.0, self.w - 2.0, self.h - 2.0, [0.06, 0.06, 0.08, 1.0]));
            return quads;
        }
        if self.hovered {
            let (hy, hh) = if self.label.is_some() {
                (self.y - 18.0, self.h + 18.0)
            } else {
                (self.y, self.h)
            };
            let hx = if self.row_w > 0.0 { self.row_x } else { self.x };
            let hw = if self.row_w > 0.0 { self.row_w } else { self.w };
            quads.push((hx, hy, hw, hh, [1.0, 1.0, 1.0, 0.06]));
        }
        let bg_color = if self.editing {
            [0.06, 0.10, 0.18, 1.0]
        } else {
            [0.08, 0.08, 0.12, 1.0]
        };
        let border_color = if self.editing {
            [0.20, 0.50, 0.85, 1.0]
        } else if self.hovered {
            [0.25, 0.25, 0.35, 1.0]
        } else {
            [0.18, 0.18, 0.24, 1.0]
        };
        quads.push((self.x, self.y, self.w, self.h, border_color));
        quads.push((self.x + 1.0, self.y + 1.0, self.w - 2.0, self.h - 2.0, bg_color));

        if self.editing {
            let char_width = 7.2;
            let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
            let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);
            
            if start != end {
                let highlight_x = self.x + 8.0 + (start as f32 * char_width);
                let max_x = self.x + self.w - 6.0;
                let highlight_w = ((end - start) as f32 * char_width).min(max_x - highlight_x).max(0.0);
                quads.push((
                    highlight_x,
                    self.y + (self.h - 16.0) / 2.0,
                    highlight_w,
                    16.0,
                    [0.20, 0.40, 0.75, 0.4],
                ));
            }

            let cursor_x = self.x + 8.0 + (self.cursor_idx as f32 * char_width);
            let max_cursor_x = self.x + self.w - 6.0;
            let final_cursor_x = cursor_x.min(max_cursor_x);
            let cursor_y = self.y + (self.h - 14.0) / 2.0;
            quads.push((final_cursor_x, cursor_y, 1.5, 14.0, [0.80, 0.80, 0.85, 1.0]));
        }

        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        if let Some(ref label) = self.label {
            labels.push(TextLabel {
                text: label.clone(),
                x: self.x + 4.0,
                y: self.y - 14.0,
                font_size: 11.0,
                color: [0x83, 0x83, 0x8a],
            });
        }
        let val_text = if self.editing {
            self.edit_buffer.clone()
        } else {
            self.text.clone()
        };
        labels.push(TextLabel {
            text: val_text,
            x: self.x + 8.0,
            y: self.y + (self.h - 12.0) / 2.0,
            font_size: 12.0,
            color: if self.disabled {
                [0x53, 0x53, 0x5a]
            } else if self.all_selected {
                [0xff, 0xff, 0xff]
            } else if self.editing {
                [0xee, 0xee, 0xf5]
            } else {
                [0xcc, 0xcc, 0xd4]
            },
        });
        labels
    }

    fn parent(&self) -> Option<*mut (dyn Widget + 'static)> { self.parent }
    fn set_parent(&mut self, parent: Option<*mut (dyn Widget + 'static)>) { self.parent = parent; }
    fn children(&self) -> Vec<*mut (dyn Widget + 'static)> { self.children.clone() }
    fn add_child(&mut self, child: *mut (dyn Widget + 'static)) { self.children.push(child); }
    fn clear_children(&mut self) { self.children.clear(); }
}

impl Drop for TextBox {
    fn drop(&mut self) {
        focus::clear_if_matches(self);
    }
}

unsafe impl Send for TextBox {}
unsafe impl Sync for TextBox {}


