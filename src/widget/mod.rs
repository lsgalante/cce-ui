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
    Control,
    Shift,
    Alt,
    Super,
    F5,
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
pub use json_layout::{JsonLayoutWidget, JsonLayoutConfig, JsonWidgetConfig, JsonPageConfig, JsonWidget, Justification};

use crate::colors;
use std::sync::atomic::AtomicUsize;
use std::collections::HashMap;

pub const DROPDOWN_ITEM_H: f32 = 22.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WidgetId(pub usize);

pub static NEXT_WIDGET_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug, Clone)]
pub struct LayoutTree {
    pub parents: HashMap<WidgetId, WidgetId>,
    pub children: HashMap<WidgetId, Vec<WidgetId>>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct WidgetPtr(pub *mut (dyn Element + 'static));

impl WidgetPtr {
    pub fn is_null(&self) -> bool {
        self.0.is_null()
    }
    pub fn as_ptr(&self) -> *mut (dyn Element + 'static) {
        self.0
    }
}

impl std::ops::Deref for WidgetPtr {
    type Target = dyn Element + 'static;
    fn deref(&self) -> &Self::Target {
        assert!(!self.0.is_null(), "Attempted to dereference a null WidgetPtr!");
        unsafe { &*self.0 }
    }
}

impl std::ops::DerefMut for WidgetPtr {
    fn deref_mut(&mut self) -> &mut Self::Target {
        assert!(!self.0.is_null(), "Attempted to dereference a null WidgetPtr!");
        unsafe { &mut *self.0 }
    }
}

pub use crate::context::UiContext;

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    PointerMove { x: f32, y: f32, local_x: f32, local_y: f32 },
    MouseButton { button: MouseButton, state: ElementState, x: f32, y: f32, local_x: f32, local_y: f32 },
    MouseWheel { delta: MouseScrollDelta, x: f32, y: f32, local_x: f32, local_y: f32 },
    KeyInput(KeyEvent),
    Tick(f32),

    MouseEnter,
    MouseLeave,
    DragStart { start_x: f32, start_y: f32 },
    DragUpdate { dx: f32, dy: f32, x: f32, y: f32, local_x: f32, local_y: f32 },
    DragEnd,
    FocusIn,
    FocusOut,
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

    /// Opt-in layout style for the scene layout engine (Phase 2b). `None` (the default) means this
    /// widget does not participate in engine-driven layout yet and keeps its legacy `set_rect`
    /// path; return `Some(..)` to have the engine size/position it and its children. See
    /// `scene::bridge`.
    fn layout_style(&self) -> Option<crate::scene::layout::Style> { None }

    /// Intrinsic content size of a leaf widget (e.g. measured text/icon) for the engine's measure
    /// pass. Ignored for widgets that have children.
    fn intrinsic_size(&self) -> Option<crate::scene::layout::Size> { None }

    /// Per-child layout styles, for containers whose child sizing lives on the parent rather than
    /// the children (e.g. `SplitBox` proportions). Returned in `children()` order; entry `i`
    /// overrides child `i`'s own `layout_style`. `None` (default) means children use their own.
    fn layout_children(&self) -> Option<Vec<crate::scene::layout::Style>> { None }

    fn check_out_of_bounds(&self, _event: &Event, _ctx: &UiContext) -> bool {
        false
    }

    fn transform_event_for_child(&self, _child: *mut (dyn Element + 'static), event: Event, _ctx: &UiContext) -> Event {
        event
    }

    fn mark_dirty(&mut self, ctx: &mut UiContext) {
        let mut parent_id = None;
        if let Some(b) = self.base_mut() {
            if b.dirty {
                return;
            }
            b.dirty = true;
            parent_id = b.id.get();
        }
        if let Some(id) = parent_id {
            if let Some(parent_ptr) = ctx.tree.parent_ptr(id) {
                unsafe {
                    (*parent_ptr).mark_dirty(ctx);
                }
            }
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        struct DummyAny;
        static DUMMY: DummyAny = DummyAny;
        &DUMMY
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        struct DummyAny;
        thread_local! {
            static DUMMY_MUT: std::cell::UnsafeCell<DummyAny> = std::cell::UnsafeCell::new(DummyAny);
        }
        DUMMY_MUT.with(|d| unsafe { &mut *d.get() })
    }
    fn as_ptr(&self) -> *mut (dyn Element + 'static) {
        struct DummyElement;
        impl Element for DummyElement {
            fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }
        }
        std::ptr::null_mut::<DummyElement>() as *mut (dyn Element + 'static)
    }
    fn as_ptr_mut(&mut self) -> *mut (dyn Element + 'static) {
        struct DummyElement;
        impl Element for DummyElement {
            fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }
        }
        std::ptr::null_mut::<DummyElement>() as *mut (dyn Element + 'static)
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut UiContext) -> bool {
        match event {
            Event::PointerMove { x, y, .. } => {
                self.cursor_moved(*x, *y, ctx)
            }
            Event::MouseButton { button, state, x, y, .. } => {
                self.mouse_input(*button, *state, *x, *y, ctx)
            }
            Event::MouseWheel { delta, x, y, .. } => {
                self.mouse_wheel(delta, *x, *y, ctx)
            }
            Event::KeyInput(key_event) => {
                self.keyboard_input(key_event, ctx)
            }
            Event::Tick(dt) => {
                self.tick(*dt, ctx)
            }
            _ => false,
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

    fn label(&self) -> Option<String> {
        self.base().and_then(|b| b.label.clone())
    }

    fn get_value_string(&self) -> Option<String> { None }
    fn set_value_string(&mut self, _val: &str) -> bool { false }
    fn take_change(&mut self) -> bool { false }

    fn cut_selection(&mut self) -> bool {
        if let Some(val) = self.get_value_string() {
            clipboard::copy_to_clipboard(&val);
            self.set_value_string("")
        } else {
            false
        }
    }
    fn copy_selection(&self) {
        if let Some(val) = self.get_value_string() {
            clipboard::copy_to_clipboard(&val);
        }
    }
    fn paste_from_clipboard(&mut self) -> bool {
        if let Some(text) = clipboard::read_from_clipboard() {
            self.set_value_string(&text)
        } else {
            false
        }
    }
    fn select_all(&mut self) {}
    fn clear_text(&mut self) {}
    fn copy_key(&self) {}
    fn copy_value(&self) {}
    fn copy_path(&self) {}
    fn delete_key(&mut self) {}
    fn expand_node(&mut self) {}
    fn collapse_node(&mut self) {}
    fn expand_all_nodes(&mut self) {}
    fn collapse_all_nodes(&mut self) {}

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
        if w <= 0.0 || h <= 0.0 {
            return false;
        }
        let (mut hx, mut hw) = if let Some(b) = self.base() {
            if b.row_w > 0.0 {
                (b.row_x, b.row_w)
            } else {
                (x, w)
            }
        } else {
            (x, w)
        };
        let label_x = self.label_x_offset();
        hx += label_x;
        hw -= label_x;
        px >= hx && px <= hx + hw && py >= y && py <= y + h
    }

    fn cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        ctx.set_cursor_pos(px, py);
        if ctx.is_coordinate_covered(self as *const Self as *const () as usize, px, py) {
            let was = self.hovered();
            if was {
                self.set_hovered(false);
                self.handle_event(&Event::MouseLeave, ctx);
            }
            return was;
        }
        self.on_cursor_moved(px, py, ctx)
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if self.base().is_some() {
            let was = self.hovered();
            let is_hit = self.hit_test(px, py, ctx);
            self.set_hovered(is_hit);
            if was != is_hit {
                if is_hit {
                    self.handle_event(&Event::MouseEnter, ctx);
                } else {
                    self.handle_event(&Event::MouseLeave, ctx);
                }
                was != is_hit
            } else {
                false
            }
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

    fn highlight_color(&self, ctx: &UiContext) -> Option<[f32; 4]> {
        let is_focused = ctx.is_focused_addr(self as *const Self as *const () as usize);
        if is_focused {
            Some(colors::highlight_primary_color())
        } else if self.hovered() {
            Some(colors::HIGHLIGHT_SECONDARY)
        } else {
            None
        }
    }

    fn highlight_quad(&self, ctx: &UiContext) -> Option<(f32, f32, f32, f32, [f32; 4])> {
        let hc = self.highlight_color(ctx)?;
        let label_x = self.label_x_offset();
        if let Some(b) = self.base() {
            let hx = if b.row_w > 0.0 { b.row_x } else { b.x } + label_x;
            let hw = if b.row_w > 0.0 { b.row_w } else { b.w } - label_x;
            Some((hx, b.y, hw, b.h, hc))
        } else {
            let (x, y, w, h) = self.rect();
            Some((x + label_x, y, w - label_x, h, hc))
        }
    }

    fn color(&self) -> [f32; 4];
    fn solid_border(&self) -> Option<([f32; 4], f32)> { None }
    fn plate_bevel(&self) -> Option<f32> { None }

    fn is_dragging(&self) -> bool { false }
    fn drag_update(&mut self, _px: f32, _py: f32) -> bool { false }
    fn drag_begin(&mut self, _px: f32, _py: f32) {}
    fn drag_end(&mut self) {}
    fn take_click(&mut self) -> bool { false }
    fn draggable(&self) -> bool { false }

    fn label_x_offset(&self) -> f32 {
        let name = self.type_name();
        if name == "Label" || name == "Button" || name == "Checkbox" || name == "Toggle" || name == "Ramp" {
            return 0.0;
        }
        if crate::layout::control_label_layout() == "side" && self.base().map_or(false, |b| b.label.is_some()) {
            90.0
        } else {
            0.0
        }
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> { Vec::new() }
    fn extra_arcs(&self) -> Vec<(f32, f32, f32, f32, f32, f32, [f32; 4])> { Vec::new() }
    fn extra_circles(&self) -> Vec<(f32, f32, f32, [f32; 4])> { Vec::new() }
    
    fn all_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = self.extra_quads();
        if let Some(hq) = self.highlight_quad(ctx) {
            if hq.4 != colors::HIGHLIGHT_SECONDARY {
                quads.push(hq);
            }
        }
        quads
    }

    /// Emit this widget's OWN primitives (non-recursive) into the single paint pass (Phase 3).
    /// The default composes the pieces the legacy recursive `all_*` emit for one node: rounded
    /// background, plain/decoration quads, circles, and own text. Widgets with richer painting
    /// (borders, bevels, arcs, vectors, SVGs) can override. Recursion into children and clipping
    /// are handled by the paint walk (`scene::painter`), not here.
    fn paint_self(&self, ui: &UiContext, ctx: &mut crate::scene::paint::PaintCtx) {
        use crate::scene::layout::Rect;
        let (x, y, w, h) = self.rect();
        let rect = Rect { x, y, width: w, height: h };
        let color = self.color();

        if self.children(ui).is_empty() {
            // Leaf: emit its own rounded quads directly. For an ordinary widget this is just the
            // rounded background; for widgets that override `all_rounded_quads` with custom
            // geometry (e.g. Graph's nodes and edges) it captures that too. No recursion happens
            // because there are no children.
            for (qx, qy, qw, qh, r, c, corners) in self.all_rounded_quads(ui) {
                ctx.rounded_rect(Rect { x: qx, y: qy, width: qw, height: qh }, r, corners, c);
            }
        } else {
            // Container: reconstruct its own plate (bevel / border / rounded background) — mirrors
            // `push_widget_vertices`. Its children are drawn by the paint walk, so we must NOT call
            // `all_rounded_quads` here (that would recurse and double-draw them).
            let cr = self.corner_radii();
            let radii = (cr.top_left, cr.top_right, cr.bottom_right, cr.bottom_left);
            if let Some(depth) = self.plate_bevel() {
                ctx.bevel(rect, radii, color, depth);
            } else if let Some((border_color, thickness)) = self.solid_border() {
                ctx.border(rect, radii, color, border_color, thickness);
            } else if color[3].abs() > 0.001 {
                let (r1, r2, r3, r4) = self.rounded_corners();
                if r1 || r2 || r3 || r4 {
                    ctx.rounded_rect(rect, self.corner_radius(), (r1, r2, r3, r4), color);
                }
            }
        }

        for (qx, qy, qw, qh, c) in self.all_quads(ui) {
            ctx.quad(Rect { x: qx, y: qy, width: qw, height: qh }, c);
        }
        for (cx, cy, r, t, start, end, c) in self.extra_arcs() {
            ctx.arc(cx, cy, r, t, start, end, c);
        }
        for (cx, cy, r, c) in self.extra_circles() {
            ctx.circle(cx, cy, r, c);
        }
        // Text: NONE by default. Every live legacy widget with own text carries a
        // paint_self override (most via `scene::painter::paint_legacy_leaf` + its own
        // labels); containers' text_labels aggregates are covered by the walk's descent
        // (emitting them here would double-draw every descendant's text — the Phase 6d
        // trap). Migrated widgets go through `Adapted::paint_self`, never this default.
    }

    /// Whether the paint walk should clip this widget's children to its rect (scroll/backplate
    /// containers). Default: no clipping.
    fn clips_children(&self) -> bool { false }

    /// Whether this widget paints its ENTIRE subtree itself through its (recursive)
    /// `all_rounded_quads` / `all_quads` — a legacy "subtree painter" such as `TreeList`, whose
    /// row backgrounds and separators live in an `all_rounded_quads` override that also recurses
    /// into its children. When true, the paint walk emits those directly and does NOT recurse
    /// (the widget already did). Transitional: such widgets will eventually get a proper
    /// non-recursive `paint_self`. Default: false.
    fn renders_own_subtree(&self) -> bool { false }

    fn all_rounded_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, f32, [f32; 4], (bool, bool, bool, bool))> {
        if !self.visible() {
            return Vec::new();
        }
        let mut quads = Vec::new();
        let (r1, r2, r3, r4) = self.rounded_corners();
        if r1 || r2 || r3 || r4 {
            let (x, y, w, h) = self.rect();
            let radius = self.corner_radius();
            let c = self.color();
            if c[3].abs() > 0.001 {
                quads.push((x, y, w, h, radius, c, (r1, r2, r3, r4)));
            }
        }
        for &child_ptr in &self.children(ctx) {
            let widget = unsafe { &*child_ptr };
            quads.extend(widget.all_rounded_quads(ctx));
        }
        quads
    }


    // The per-widget text getters (text_labels / text_labels_with_bounds /
    // text_labels_with_font_and_bounds / get_text_items) are GONE: every widget emits
    // its own text as display-list prims via paint_self (Adapted::paint_self for
    // migrated widgets; paint_legacy_leaf-based overrides for the legacy leaves). The
    // deleted default's base-label synthesis lives on in Adapted's base-label fallback,
    // and its scroll-ancestor clamp in scene::painter::scroll_ancestor_text_bounds.

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
    fn set_selected(&mut self, _selected: bool) {}
    fn keyboard_input(&mut self, _event: &KeyEvent, _ctx: &mut UiContext) -> bool { false }

    fn set_visible(&mut self, _visible: bool) {}
    fn visible(&self) -> bool { true }
    fn tick(&mut self, _dt: f32, _ctx: &mut UiContext) -> bool { false }
    fn wants_tick(&self) -> bool { false }
    fn is_child_visible(&self, _child_id: WidgetId) -> bool { true }
    fn set_modifiers(&mut self, _ctrl: bool, _shift: bool, _alt: bool) {}

    fn as_page_selector(&self) -> Option<&dyn PageSelector> { None }
    fn as_page_selector_mut(&mut self) -> Option<&mut dyn PageSelector> { None }
    fn as_menu_controller(&self) -> Option<&dyn MenuController> { None }
    fn as_menu_controller_mut(&mut self) -> Option<&mut dyn MenuController> { None }
    fn as_graph_controller(&self) -> Option<&dyn GraphController> { None }
    fn as_graph_controller_mut(&mut self) -> Option<&mut dyn GraphController> { None }
    fn as_spreadsheet_controller_mut(&mut self) -> Option<&mut dyn SpreadsheetController> { None }
    fn as_path_controller(&self) -> Option<&dyn PathController> { None }
    fn as_path_controller_mut(&mut self) -> Option<&mut dyn PathController> { None }
    fn as_param_controller(&self) -> Option<&dyn ParamController> { None }
    fn as_param_controller_mut(&mut self) -> Option<&mut dyn ParamController> { None }
    fn as_geom_controller_mut(&mut self) -> Option<&mut dyn GeomController> { None }

    fn parent(&self, ctx: &UiContext) -> Option<*mut (dyn Element + 'static)> {
        let base = self.base()?;
        ctx.tree.parent_ptr(base.id())
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
                    // Symmetric link (Phase 1b): unlike the legacy `parents.insert` this also
                    // records the child under the parent, keeping `children()` consistent.
                    ctx.tree.set_parent(id, Some(p_id));
                }
            } else {
                ctx.tree.set_parent(id, None);
            }
        }
    }

    fn children(&self, ctx: &UiContext) -> Vec<*mut (dyn Element + 'static)> {
        match self.base() {
            Some(base) => ctx.tree.children_ptrs(base.id()),
            None => vec![],
        }
    }

    fn add_child(&mut self, child: *mut (dyn Element + 'static), ctx: &mut UiContext) {
        if let (Some(p_base), Some(c_base)) = (self.base(), unsafe { (*child).base() }) {
            let p_id = p_base.id();
            let c_id = c_base.id();
            let self_ptr = self.as_ptr();
            ctx.register_widget(p_id, self_ptr);
            ctx.register_widget(c_id, child);
            ctx.tree.link(p_id, c_id);
        }
    }

    fn clear_children(&mut self, ctx: &mut UiContext) {
        if let Some(base) = self.base() {
            let id = base.id();
            ctx.clear_children_ids(id);
        }
    }

    fn z_index(&self) -> i32 { 0 }
    fn is_scrollable(&self) -> bool { false }
    fn blocks_backplate_drag(&self) -> bool { true }
    fn rounded_corners(&self) -> (bool, bool, bool, bool) { (false, false, false, false) }

    fn corner_radius(&self) -> f32 { 12.0 }

    fn corner_radii(&self) -> CornerRadii {
        let r = self.corner_radius();
        let (tl, tr, br, bl) = self.rounded_corners();
        CornerRadii::new(
            if tl { r } else { 0.0 },
            if tr { r } else { 0.0 },
            if br { r } else { 0.0 },
            if bl { r } else { 0.0 },
        )
    }
    fn layout_ignore(&self) -> bool { false }
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
        
        let (_, font_size) = crate::layout::control_label_font_detached_parsed();
        let color = colors::control_label_color_detached_for_state(b.hovered, b.focused);
        if crate::layout::control_label_layout() == "side" {
            let y_pos = crate::layout::align_text_y(b.y, b.h, font_size, 0.0);
            Some(TextLabel {
                text: label.clone(),
                x: b.x + 4.0,
                y: y_pos,
                font_size,
                color,
            })
        } else {
            let x_offset = if name == "Slider" || name == "RangeSlider" {
                0.0
            } else {
                4.0
            };
            Some(TextLabel {
                text: label.clone(),
                x: b.x + x_offset,
                y: b.y,
                font_size,
                color,
            })
        }
    }
}

pub mod core;
pub mod input;
pub mod container;
pub mod display;
pub mod editor;
pub mod layout_helper;
pub mod model;
 
// Re-exports
pub use self::editor::TextEditorState;
pub use self::layout_helper::{ColumnLayout, RowLayout};
pub use self::model::{Adapted, EventCtx, Input, Layout, Paint};
pub use self::core::{Widget, focus, hover_animation, clipboard, context_menu, clear_widget_references};
pub use self::core::focus::link_parent_child;
pub use self::input::{
    Button, TextBox, Spinbox, Dropdown, Checkbox, Toggle, Slider, RangeSlider,
    ColorSelector, Finger, Trackpad, Canvas, get_font_db, ActiveThumb, FontSelector,
    ButtonStrip, KeybindRecorder, Ramp, RampKey, ColorRamp, ColorRampKey
};
pub use self::container::{
    Container, ContainerLayout, OverlayLayout, ManualLayout, VerticalLayout, GridLayout, AdaptiveGridLayout,
    ColumnsLayout, MosaicLayout, ReverseMosaicLayout,
    Header, ContentBg, ParametersBg,
    ScrollBox, MenuBar, Spreadsheet, Breadcrumb,
    Switcher, Paginator, ScrollBar, TreeList, TreeElement
};
pub use self::display::{
    TextLabel, Label, StyledLabel, LabelPrim, TextItem, Svg, UsageBar,
    LayoutPreview, FontPreview, InfoBox, StatusDot, InteractiveListItem,
    GraphNode, Graph, Float3, ProgressBar, StatusBar, Splitter, Node, Separator,
    DotStatus, PreviewLayoutMode, Sidebar, Panel, PreviewState, ImagePreviewData, serialize_widgets,
    Viewport3D
};

pub trait PageSelector {
    fn selected_page(&self) -> usize;
    fn set_selected_page(&mut self, page: usize);
    fn sidebar_w(&self) -> f32;
}

pub trait MenuController {
    fn menu_click(&mut self) -> Option<(usize, usize)>;
    fn trigger_menu_click(&mut self, menu_idx: usize, item_idx: usize);
    fn set_item_checked(&mut self, menu_idx: usize, item_idx: usize, checked: bool);
    fn set_menu_items(&mut self, menu_idx: usize, items: &[String]);
    fn is_menu_bar(&self) -> bool;
    fn is_menu_open(&self) -> bool;
    fn menu_items(&self) -> Vec<String>;
    fn menu_item_checked(&self) -> Vec<Option<bool>>;
    fn is_vertical(&self) -> bool;
    fn menu_names(&self) -> Vec<String>;
    fn menu_items_list(&self) -> Vec<Vec<String>>;
    fn menu_checked_list(&self) -> Vec<Vec<Option<bool>>>;
    fn take_context_change(&mut self) -> Option<usize>;
    fn set_context_selected(&mut self, selected: usize);
    fn set_center_items(&mut self, center: bool);
    fn get_menu_items_at(&self, px: f32, py: f32) -> Option<(usize, String, Vec<String>, f32, f32, f32, f32)>;
}

pub trait GraphController {
    fn set_nodes(&mut self, nodes: &[GraphNode]);
    fn get_nodes(&self) -> Vec<GraphNode>;
    fn selected_node(&self) -> Option<usize>;
    fn set_selected_node(&mut self, idx: Option<usize>);
    fn double_clicked_node(&self) -> Option<usize>;
    fn clear_double_clicked_node(&mut self);
    fn set_grid_snap_enabled(&mut self, enabled: bool);
    fn take_node_geom_toggle(&mut self) -> Option<(usize, bool)>;
    fn set_grid_snap(&mut self, gx: f32, gy: f32);
    fn set_grid_sizes(&mut self, gx: f32, gy: f32);
    fn set_skipped_sizes(&mut self, row_h: f32, col_w: f32);
    fn set_grid_origin(&mut self, ox: f32, oy: f32);
    fn grid_origin(&self) -> (f32, f32);
    fn set_show_network_grid(&mut self, show: bool);
    fn take_pending_connection(&mut self) -> Option<(String, String)>;
    fn cancel_connecting(&mut self);
    fn is_node_rect(&self, qx: f32, qy: f32, qw: f32, qh: f32) -> bool;
}

pub trait SpreadsheetController {
    fn set_spreadsheet_data(&mut self, headers: Vec<String>, rows: Vec<Vec<String>>);
}

pub trait PathController {
    fn set_path(&mut self, segments: &[String]);
    fn path_click(&mut self) -> Option<usize>;
}

pub trait ParamController {
    fn node_params(&self) -> Vec<(String, String, String)>;
    fn set_display_params(&mut self, params: &[(String, String, String)]);
}

pub trait GeomController {
    fn set_geom_visible(&mut self, visible: bool);
    fn geom_visible(&self) -> bool;
    fn take_geom_toggle(&mut self) -> bool;
}

pub fn label_offset(w: &dyn Element) -> f32 {
    let name = w.type_name();
    if name == "Label" || name == "Button" || name == "Checkbox" || name == "Toggle" {
        return 0.0;
    }
    w.base().map_or(0.0, |b| b.label_offset())
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CornerRadii {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl CornerRadii {
    pub fn new(tl: f32, tr: f32, br: f32, bl: f32) -> Self {
        Self { top_left: tl, top_right: tr, bottom_right: br, bottom_left: bl }
    }

    pub fn uniform(radius: f32) -> Self {
        Self::new(radius, radius, radius, radius)
    }
}

pub fn match_key_shortcut(event: &KeyEvent, shortcut_str: &str) -> bool {
    let shortcut_lower = shortcut_str.to_lowercase();
    let parts: Vec<&str> = shortcut_lower.split('+').collect();
    
    let mut req_ctrl = false;
    let mut req_shift = false;
    let mut req_key = "";
    
    for part in parts {
        match part {
            "ctrl" | "control" => req_ctrl = true,
            "shift" => req_shift = true,
            "super" | "win" | "logo" | "alt" | "meta" => {}
            k => req_key = k,
        }
    }
    
    if event.ctrl != req_ctrl { return false; }
    if event.shift != req_shift { return false; }
    
    if let Key::Character(ref ch) = event.logical_key {
        let ch_lower = ch.to_lowercase();
        if req_key.len() == 1 {
            return ch_lower == req_key;
        } else {
            let mapped_key = match req_key {
                "slash" => "/",
                "enter" => "enter",
                "escape" => "escape",
                "space" => " ",
                k => k,
            };
            return ch_lower == mapped_key;
        }
    } else if let Key::Named(nk) = event.logical_key {
        let nk_str = format!("{:?}", nk).to_lowercase();
        return nk_str == req_key;
    }
    false
}

