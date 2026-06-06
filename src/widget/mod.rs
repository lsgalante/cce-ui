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

pub mod json_layout;
pub use json_layout::{JsonLayoutWidget, JsonLayoutConfig, JsonWidgetConfig, JsonPageConfig, JsonWidget};

use crate::colors;

pub const DROPDOWN_ITEM_H: f32 = 22.0;

pub trait Element {
    fn base(&self) -> Option<&Widget> { None }
    fn base_mut(&mut self) -> Option<&mut Widget> { None }

    fn rect(&self) -> (f32, f32, f32, f32) {
        if let Some(b) = self.base() {
            (b.x, b.y, b.w, b.h)
        } else {
            (0.0, 0.0, 0.0, 0.0)
        }
    }

    fn label(&self) -> Option<String> { None }
    
    fn set_curved_circle(&mut self, _circle: Option<(f32, f32, f32)>) {}
    fn set_uniform_background(&mut self, _uniform: bool) {}
    fn set_network_opacity(&mut self, _opacity: f32) {}
    fn set_cell_color(&mut self, _color: [f32; 3]) {}
    fn set_gap_color(&mut self, _color: [f32; 3]) {}

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if let Some(b) = self.base_mut() {
            b.x = x;
            b.y = y;
            b.w = w;
            b.h = h;
        }
    }

    fn set_row_rect(&mut self, x: f32, w: f32) {
        if let Some(b) = self.base_mut() {
            b.row_x = x;
            b.row_w = w;
        }
    }

    fn hit_test(&self, px: f32, py: f32) -> bool {
        if crate::widget::popovers::is_coordinate_covered(self as *const Self as *const () as usize, px, py) {
            return false;
        }
        let (x, y, w, h) = self.rect();
        let (hx, hw) = if let Some(b) = self.base() {
            if b.row_w > 0.0 {
                (b.row_x, b.row_w)
            } else {
                (x, w)
            }
        } else {
            (x, w)
        };
        px >= hx && px <= hx + hw && py >= y && py <= y + h
    }

    fn cursor_moved(&mut self, px: f32, py: f32) -> bool {
        hover_animation::set_cursor_pos(px, py);
        if crate::widget::popovers::is_coordinate_covered(self as *const Self as *const () as usize, px, py) {
            let was = self.hovered();
            self.set_hovered(false);
            return was;
        }
        self.on_cursor_moved(px, py)
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32) -> bool {
        if self.base().is_some() {
            let was = self.hovered();
            let is_hit = self.hit_test(px, py);
            self.set_hovered(is_hit);
            was != is_hit
        } else {
            false
        }
    }

    fn mouse_input(&mut self, _button: MouseButton, _state: ElementState, _px: f32, _py: f32) -> bool { false }
    fn mouse_wheel(&mut self, _delta: &MouseScrollDelta, _px: f32, _py: f32) -> bool { false }

    fn set_hovered(&mut self, hovered: bool) {
        if let Some(b) = self.base_mut() {
            b.hovered = hovered;
        }
    }

    fn hovered(&self) -> bool {
        if let Some(b) = self.base() {
            b.hovered
        } else {
            false
        }
    }

    fn is_active(&self) -> bool { false }

    fn highlight_color(&self) -> Option<[f32; 4]> {
        let is_focused = self.base().map_or(false, |b| b.focused);
        if is_focused || self.is_active() {
            Some(colors::highlight_primary_color())
        } else if self.hovered() {
            Some(colors::HIGHLIGHT_SECONDARY)
        } else {
            None
        }
    }

    fn highlight_quad(&self) -> Option<(f32, f32, f32, f32, [f32; 4])> {
        let hc = self.highlight_color()?;
        if let Some(b) = self.base() {
            let hx = if b.row_w > 0.0 { b.row_x } else { b.x };
            let hw = if b.row_w > 0.0 { b.row_w } else { b.w };
            Some((hx, b.y, hw, b.h, hc))
        } else {
            let (x, y, w, h) = self.rect();
            Some((x, y, w, h, hc))
        }
    }

    fn color(&self) -> [f32; 4];

    fn is_dragging(&self) -> bool { false }
    fn drag_update(&mut self, _px: f32, _py: f32) -> bool { false }
    fn drag_begin(&mut self, _px: f32, _py: f32) {}
    fn drag_end(&mut self) {}
    fn take_click(&mut self) -> bool { false }
    fn draggable(&self) -> bool { false }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> { Vec::new() }
    fn extra_arcs(&self) -> Vec<(f32, f32, f32, f32, f32, f32, [f32; 4])> { Vec::new() }
    fn all_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = self.extra_quads();
        if let Some(hq) = self.highlight_quad() {
            if hq.4 == colors::HIGHLIGHT_SECONDARY {
                hover_animation::register_hovered(hq.0, hq.1, hq.2, hq.3, hq.4);
            } else {
                quads.push(hq);
            }
        }
        quads
    }
    fn text_labels(&self) -> Vec<TextLabel> {
        if let Some(b) = self.base() {
            if let Some(ref label) = b.label {
                return vec![TextLabel {
                    text: label.clone(),
                    x: b.x,
                    y: b.y,
                    font_size: 12.0,
                    color: [0x83, 0x83, 0x8a],
                }];
            }
        }
        Vec::new()
    }
    fn widget_font(&self) -> Option<String> { None }
    fn value(&self) -> i32 { 0 }
    fn type_name(&self) -> &'static str {
        let full_name = std::any::type_name::<Self>();
        full_name.split("::").last().unwrap_or("Widget")
    }
    fn popover_rect(&self) -> Option<(f32, f32, f32, f32)> { None }
    fn render_popover(&self, _pc: &mut dyn crate::layout::RenderTarget) {}
    fn set_text(&mut self, text: &str) {
        if let Some(b) = self.base_mut() {
            b.label = Some(text.to_string());
        }
    }

    fn set_drag_bounds(&mut self, _bx: f32, _by: f32, _bw: f32, _bh: f32) {}

    fn focus(&mut self) {
        if let Some(b) = self.base_mut() {
            b.focused = true;
        }
    }
    fn unfocus(&mut self) {
        if let Some(b) = self.base_mut() {
            b.focused = false;
        }
    }
    fn focused(&self) -> bool {
        if let Some(b) = self.base() {
            b.focused
        } else {
            false
        }
    }
    fn prepare_text(&mut self, _fs: &mut glyphon::FontSystem) {}
    fn get_text_items(&self) -> Vec<(&glyphon::Buffer, f32, f32, glyphon::Color)> { Vec::new() }
    fn set_selected(&mut self, _selected: bool) {}
    fn keyboard_input(&mut self, _event: &KeyEvent) -> bool { false }

    fn menu_click(&mut self) -> Option<(usize, usize)> { None }
    fn get_menu_items_at(&self, _px: f32, _py: f32) -> Option<(usize, String, Vec<String>, f32, f32, f32, f32)> { None }
    fn trigger_menu_click(&mut self, _menu_idx: usize, _item_idx: usize) {}
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
    fn grid_origin(&self) -> (f32, f32) { (0.0, 0.0) }
    fn set_skipped_sizes(&mut self, _row_h: f32, _col_w: f32) {}
    fn set_palette_state(&mut self, _visible: bool, _query: &str, _items: &[String], _selected: usize) {}

    fn set_geom_visible(&mut self, _visible: bool) {}
    fn geom_visible(&self) -> bool { true }
    fn take_geom_toggle(&mut self) -> bool { false }
    fn set_spreadsheet_data(&mut self, _headers: Vec<String>, _rows: Vec<Vec<String>>) {}
    fn tick(&mut self, _dt: f32) -> bool { false }

    fn set_nodes(&mut self, _nodes: &[GraphNode]) {}
    fn get_nodes(&self) -> Vec<GraphNode> { vec![] }
    fn selected_node(&self) -> Option<usize> { None }
    fn set_selected_node(&mut self, _idx: Option<usize>) {}
    fn double_clicked_node(&self) -> Option<usize> { None }
    fn clear_double_clicked_node(&mut self) {}
    fn set_grid_snap_enabled(&mut self, _enabled: bool) {}
    fn take_node_geom_toggle(&mut self) -> Option<(usize, bool)> { None }

    fn parent(&self) -> Option<*mut (dyn Element + 'static)> { None }
    fn set_parent(&mut self, _parent: Option<*mut (dyn Element + 'static)>) {}
    fn children(&self) -> Vec<*mut (dyn Element + 'static)> { vec![] }
    fn add_child(&mut self, _child: *mut (dyn Element + 'static)) {}
    fn clear_children(&mut self) {}
    fn z_index(&self) -> i32 { 0 }
    fn set_center_items(&mut self, _center: bool) {}
    fn menu_items(&self) -> Vec<String> { vec![] }
    fn menu_item_checked(&self) -> Vec<Option<bool>> { vec![] }
    fn is_vertical(&self) -> bool { false }

    fn is_plate(&self) -> bool { false }
    fn rounded_corners(&self) -> (bool, bool, bool, bool) { (false, false, false, false) }
    fn layout_ignore(&self) -> bool { false }
    fn text_labels_with_bounds(&self) -> Vec<(TextLabel, Option<[f32; 4]>)> {
        self.text_labels().into_iter().map(|l| (l, None)).collect()
    }
    fn text_labels_with_font_and_bounds(&self) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        self.text_labels().into_iter().map(|l| (l, None, None)).collect()
    }

    fn set_modifiers(&mut self, _ctrl: bool, _shift: bool, _alt: bool) {}
    fn menu_names(&self) -> Vec<String> { vec![] }
    fn selected_page(&self) -> usize { 0 }
    fn set_selected_page(&mut self, _page: usize) {}
    fn is_page_hidden(&self) -> bool { false }
    fn set_page_hidden(&mut self, _hidden: bool) {}
    fn set_pages(&mut self, _pages: Vec<String>) {}
    fn sidebar_w(&self) -> f32 { 0.0 }
    fn set_sidebar_mode(&mut self, _enabled: bool) {}
    fn set_sidebar_label(&mut self, _label: Option<String>) {}
    fn add_widget_to_page(&mut self, _page_idx: usize, _widget: *mut (dyn Element + 'static)) {}
    fn clear_page_widgets(&mut self, _page_idx: usize) {}
    fn menu_items_list(&self) -> Vec<Vec<String>> { vec![] }
    fn menu_checked_list(&self) -> Vec<Vec<Option<bool>>> { vec![] }
    fn color_u8(&self) -> Option<[u8; 4]> { None }
    fn update_bounds(&mut self, _count: usize, _viewport_y: f32, _viewport_h: f32) {}
    fn get_item_draw_y(&self, _idx: usize, _offset: f32) -> Option<f32> { None }
}



pub mod core;
pub mod input;
pub mod container;
pub mod display;

// Re-exports
pub use self::core::{Widget, focus, hover_animation, popovers, clipboard, context_menu};
pub use self::input::{
    Button, TextBox, Spinbox, Dropdown, Checkbox, Toggle, Slider, RangeSlider,
    ColorSelector, Finger, Trackpad, Canvas, get_font_db, ActiveThumb, FontSelector
};
pub use self::container::{
    Container, Header, ContentBg, ViewportBg, ParametersBg, ScrollingList,
    ScrollBox, Menu, MenuBar, Spreadsheet, Breadcrumb, Plate,
    Paginator
};
pub use self::display::{
    TextLabel, Label, SectionHeader, StyledLabel, TextItem, Svg, UsageBar,
    LayoutPreview, FontPreview, InfoBox, StatusDot, InteractiveListItem,
    GraphNode, Graph, Float3, ProgressBar, StatusBar, Splitter, Node, Separator,
    DotStatus, PreviewLayoutMode, Sidebar, Panel, serialize_widgets
};

pub fn label_offset(w: &dyn Element) -> f32 {
    w.base().map_or(0.0, |b| b.label_offset())
}
