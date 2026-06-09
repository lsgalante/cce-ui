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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::collections::HashMap;

pub const DROPDOWN_ITEM_H: f32 = 22.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WidgetId(pub usize);

pub static NEXT_WIDGET_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug, Clone)]
pub struct LayoutTree {
    pub parents: HashMap<WidgetId, WidgetId>,
    pub children: HashMap<WidgetId, Vec<WidgetId>>,
}

pub use crate::context::UiContext;

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    PointerMove { x: f32, y: f32 },
    MouseButton { button: MouseButton, state: ElementState, x: f32, y: f32 },
    MouseWheel { delta: MouseScrollDelta, x: f32, y: f32 },
    KeyInput(KeyEvent),
    Tick(f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutConstraints {
    pub min_width: f32,
    pub max_width: f32,
    pub min_height: f32,
    pub max_height: f32,
}

impl LayoutConstraints {
    pub fn new(min_w: f32, max_w: f32, min_h: f32, max_h: f32) -> Self {
        Self { min_width: min_w, max_width: max_w, min_height: min_h, max_height: max_h }
    }
    
    pub fn loose(max_w: f32, max_h: f32) -> Self {
        Self { min_width: 0.0, max_width: max_w, min_height: 0.0, max_height: max_h }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

pub trait Element {
    fn base(&self) -> Option<&Widget> { None }
    fn base_mut(&mut self) -> Option<&mut Widget> { None }
    fn preferred_height(&self) -> Option<f32> { None }

    fn as_any(&self) -> &dyn std::any::Any {
        panic!("as_any not implemented");
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        panic!("as_any_mut not implemented");
    }
    fn as_ptr(&self) -> *mut (dyn Element + 'static) {
        panic!("as_ptr not implemented");
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut UiContext) -> bool {
        match event {
            Event::PointerMove { x, y } => {
                self.cursor_moved(*x, *y, ctx)
            }
            Event::MouseButton { button, state, x, y } => {
                self.mouse_input(*button, *state, *x, *y, ctx)
            }
            Event::MouseWheel { delta, x, y } => {
                self.mouse_wheel(delta, *x, *y, ctx)
            }
            Event::KeyInput(key_event) => {
                self.keyboard_input(key_event, ctx)
            }
            Event::Tick(dt) => {
                self.tick(*dt, ctx)
            }
        }
    }

    fn measure(&self, constraints: LayoutConstraints, _ctx: &UiContext) -> Size {
        let (_, _, w, h) = self.rect();
        let pref_h = self.preferred_height().unwrap_or(h);
        
        let width = w.clamp(constraints.min_width, constraints.max_width);
        let height = pref_h.clamp(constraints.min_height, constraints.max_height);
        
        Size { width, height }
    }

    fn layout(&mut self, origin: Point, constraints: LayoutConstraints, ctx: &mut UiContext) {
        let size = self.measure(constraints, ctx);
        self.set_rect(origin.x, origin.y, size.width, size.height);
    }

    fn rect(&self) -> (f32, f32, f32, f32) {
        if let Some(b) = self.base() {
            (b.x, b.y, b.w, b.h)
        } else {
            (0.0, 0.0, 0.0, 0.0)
        }
    }

    fn label(&self) -> Option<String> { None }

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

    fn hit_test(&self, px: f32, py: f32, ctx: &UiContext) -> bool {
        if ctx.is_coordinate_covered(self as *const Self as *const () as usize, px, py) {
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

    fn cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        ctx.set_cursor_pos(px, py);
        if ctx.is_coordinate_covered(self as *const Self as *const () as usize, px, py) {
            let was = self.hovered();
            self.set_hovered(false);
            return was;
        }
        self.on_cursor_moved(px, py, ctx)
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if self.base().is_some() {
            let was = self.hovered();
            let is_hit = self.hit_test(px, py, ctx);
            self.set_hovered(is_hit);
            was != is_hit
        } else {
            false
        }
    }

    fn mouse_input(&mut self, _button: MouseButton, _state: ElementState, _px: f32, _py: f32, _ctx: &mut UiContext) -> bool { false }
    fn mouse_wheel(&mut self, _delta: &MouseScrollDelta, _px: f32, _py: f32, _ctx: &mut UiContext) -> bool { false }

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

    fn highlight_color(&self, ctx: &UiContext) -> Option<[f32; 4]> {
        let is_focused = ctx.is_focused_addr(self as *const Self as *const () as usize);
        if is_focused || self.is_active() {
            Some(colors::highlight_primary_color())
        } else if self.hovered() {
            Some(colors::HIGHLIGHT_SECONDARY)
        } else {
            None
        }
    }

    fn highlight_quad(&self, ctx: &UiContext) -> Option<(f32, f32, f32, f32, [f32; 4])> {
        let hc = self.highlight_color(ctx)?;
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
    
    fn all_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = self.extra_quads();
        if let Some(hq) = self.highlight_quad(ctx) {
            if hq.4 != colors::HIGHLIGHT_SECONDARY {
                quads.push(hq);
            }
        }
        quads
    }

    fn paint(&mut self, ctx: &mut UiContext) {
        if let Some(hq) = self.highlight_quad(ctx) {
            if hq.4 == colors::HIGHLIGHT_SECONDARY {
                ctx.register_hovered(hq.0, hq.1, hq.2, hq.3, hq.4);
            }
        }
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
    
    fn text_labels_with_bounds(&self, _ctx: &UiContext) -> Vec<(TextLabel, Option<[f32; 4]>)> {
        self.text_labels().into_iter().map(|l| (l, None)).collect()
    }
    
    fn text_labels_with_font_and_bounds(&self, _ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        let font = self.widget_font();
        self.text_labels().into_iter().map(|l| (l, font.clone(), None)).collect()
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
    fn focused(&self, ctx: &UiContext) -> bool {
        ctx.is_focused_addr(self as *const Self as *const () as usize)
    }
    fn prepare_text(&mut self, _fs: &mut glyphon::FontSystem) {}
    fn get_text_items(&self) -> Vec<(&glyphon::Buffer, f32, f32, glyphon::Color)> { Vec::new() }
    fn set_selected(&mut self, _selected: bool) {}
    fn keyboard_input(&mut self, _event: &KeyEvent, _ctx: &mut UiContext) -> bool { false }

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
    fn tick(&mut self, _dt: f32, _ctx: &mut UiContext) -> bool { false }

    fn set_nodes(&mut self, _nodes: &[GraphNode]) {}
    fn get_nodes(&self) -> Vec<GraphNode> { vec![] }
    fn selected_node(&self) -> Option<usize> { None }
    fn set_selected_node(&mut self, _idx: Option<usize>) {}
    fn double_clicked_node(&self) -> Option<usize> { None }
    fn clear_double_clicked_node(&mut self) {}
    fn set_grid_snap_enabled(&mut self, _enabled: bool) {}
    fn take_node_geom_toggle(&mut self) -> Option<(usize, bool)> { None }

    fn parent(&self, ctx: &UiContext) -> Option<*mut (dyn Element + 'static)> {
        let base = self.base()?;
        let id = base.id();
        let parent_id = ctx.layout_tree.parents.get(&id).copied()?;
        ctx.widget_registry.get(&parent_id).copied()
    }

    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>, ctx: &mut UiContext) {
        if let Some(base) = self.base() {
            let id = base.id();
            if let Some(p_ptr) = parent {
                if let Some(p_base) = unsafe { (*p_ptr).base() } {
                    let p_id = p_base.id();
                    ctx.register_widget(p_id, p_ptr);
                    let self_ptr = self.as_ptr();
                    ctx.register_widget(id, self_ptr);
                    ctx.layout_tree.parents.insert(id, p_id);
                }
            } else {
                ctx.layout_tree.parents.remove(&id);
            }
        }
    }

    fn children(&self, ctx: &UiContext) -> Vec<*mut (dyn Element + 'static)> {
        if let Some(base) = self.base() {
            let id = base.id();
            let child_ids = ctx.layout_tree.children.get(&id).cloned().unwrap_or_default();
            child_ids.iter().filter_map(|cid| ctx.widget_registry.get(cid).copied()).collect()
        } else {
            vec![]
        }
    }

    fn add_child(&mut self, child: *mut (dyn Element + 'static), ctx: &mut UiContext) {
        if let (Some(p_base), Some(c_base)) = (self.base(), unsafe { (*child).base() }) {
            let p_id = p_base.id();
            let c_id = c_base.id();
            let self_ptr = self.as_ptr();
            ctx.register_widget(p_id, self_ptr);
            ctx.register_widget(c_id, child);
            ctx.layout_tree.parents.insert(c_id, p_id);
            let children = ctx.layout_tree.children.entry(p_id).or_default();
            if !children.contains(&c_id) {
                children.push(c_id);
            }
        }
    }

    fn clear_children(&mut self, ctx: &mut UiContext) {
        if let Some(base) = self.base() {
            let id = base.id();
            ctx.clear_children_ids(id);
        }
    }

    fn z_index(&self) -> i32 { 0 }
    fn set_center_items(&mut self, _center: bool) {}
    fn menu_items(&self) -> Vec<String> { vec![] }
    fn menu_item_checked(&self) -> Vec<Option<bool>> { vec![] }
    fn is_vertical(&self) -> bool { false }

    fn is_plate(&self) -> bool { false }
    fn rounded_corners(&self) -> (bool, bool, bool, bool) { (false, false, false, false) }
    fn layout_ignore(&self) -> bool { false }

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
    fn add_widget_to_page(&mut self, _page_idx: usize, _widget: *mut (dyn Element + 'static), _ctx: &mut UiContext) {}
    fn clear_page_widgets(&mut self, _page_idx: usize, _ctx: &mut UiContext) {}
    fn menu_items_list(&self) -> Vec<Vec<String>> { vec![] }
    fn menu_checked_list(&self) -> Vec<Vec<Option<bool>>> { vec![] }
    fn color_u8(&self) -> Option<[u8; 4]> { None }
    fn update_bounds(&mut self, _count: usize, _viewport_y: f32, _viewport_h: f32) {}
    fn get_item_draw_y(&self, _idx: usize, _offset: f32) -> Option<f32> { None }
}

pub trait Control: Element {
    fn set_label(&mut self, label: &str) {
        if let Some(b) = self.base_mut() {
            b.label = Some(label.to_string());
        }
    }

    fn control_label(&self) -> Option<TextLabel> {
        let b = self.base()?;
        let label = b.label.as_ref()?;
        let name = self.type_name();
        
        let x_offset = if name == "Slider" || name == "RangeSlider" {
            0.0
        } else {
            4.0
        };
        
        Some(TextLabel {
            text: label.clone(),
            x: b.x + x_offset,
            y: b.y,
            font_size: 12.0,
            color: [0x83, 0x83, 0x8a],
        })
    }
}

pub mod core;
pub mod input;
pub mod container;
pub mod display;
pub mod editor;

// Re-exports
pub use self::editor::TextEditorState;
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
    let name = w.type_name();
    if name == "Label" || name == "Button" || name == "Checkbox" || name == "Toggle" {
        return 0.0;
    }
    w.base().map_or(0.0, |b| b.label_offset())
}
