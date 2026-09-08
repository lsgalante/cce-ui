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

impl MouseScrollDelta {
    /// Vertical scroll in wheel-notch equivalents for VALUE widgets (sliders,
    /// float3 rows). The pixel divisor is calibrated against a measured
    /// trackpad stream, not a notch convention: a real two-finger swipe
    /// delivers 10–20 axis units per event at 6–8ms intervals (~2000
    /// units/sec sustained). At 60 units per notch-equivalent (0.02 of the
    /// range each), that sustains ~0.6 range/sec — a full sweep is a couple
    /// of committed swipes, while slow fine-tuning events (2–5 units) move
    /// well under one readout tick. 15 (the DE's hardware-notch unit) slams
    /// bound-to-bound in ~150ms; 120 (the wheel standard) needs ~6000px of
    /// finger travel per sweep.
    pub fn notches_y(&self) -> f32 {
        match self {
            MouseScrollDelta::LineDelta(_x, y) => *y,
            MouseScrollDelta::PixelDelta(pos) => (pos.y as f32) / 60.0,
        }
    }
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
    pub alt: bool,
}

/// Text justification for widget labels/content (shared by Button, cce-files' row
/// list, and settings; formerly defined by the dissolved json_layout host).
#[derive(serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Justification {
    Left,
    Center,
    Right,
}

/// A context-menu action dispatched on the menu's target widget (6bd phase 1: one enum
/// replaces the 13 per-action `WidgetHost` methods). `ClearText` is the search-box "Cear" item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContextAction {
    Cut,
    Copy,
    Paste,
    SelectAll,
    /// Step the widget's own edit history (a text box's typing). Routed by
    /// the runner to the focused widget on the `undo` / `redo` chords before
    /// the app's `Application::undo` / `redo` get their turn; also reachable
    /// as "Undo" / "Redo" context-menu rows.
    Undo,
    Redo,
    ClearText,
    CopyKey,
    CopyValue,
    CopyPath,
    DeleteKey,
    ExpandNode,
    CollapseNode,
    ExpandAll,
    CollapseAll,
    /// Ramp: hide/show the bottom control strip, the graph claiming the space.
    ToggleRampControls,
}

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

/// The single host surface every widget presents to the machinery (context routing, the
/// paint walk, the render loop, app dyn broadcasts). **Formerly `Element`**, the ~125-method
/// god-trait — renamed at the 6bd flip once census-driven shrink batches brought it down to
/// the measured blueprint. `Adapted<W>` is the one production implementor; concrete behavior
/// lives on the narrow `Layout`/`Paint`/`Input` traits it wraps. The direct-dispatch and
/// value blocks shrink further as apps move to routed events / concrete slots.
pub trait WidgetHost {
    /// The widget's shared base state — GUARANTEED (the flip): the `Option` escape hatch
    /// and its `WidgetId(0)` sentinel class are gone. `Adapted` (the one production
    /// implementor) always owns a base; test shims carry one via `impl_widget_base!`.
    fn base(&self) -> &Widget;
    fn base_mut(&mut self) -> &mut Widget;
    /// The widget's natural CONTENT height — the control below its detached label, if
    /// any. What a layout strategy allots; [`WidgetHost::layout`] places that content
    /// box at the origin it is given and hangs the label ([`WidgetHost::label_strip`])
    /// above it. `None` when the widget has no natural height.
    fn preferred_height(&self) -> Option<f32> { None }

    /// The height of the detached-label strip above this widget's content: zero for
    /// unlabeled and inline-label widgets. A widget's occupied rect is its content plus
    /// this strip, whichever legacy convention its `set_rect` follows; `layout` lands
    /// the content at the origin and the strip above it, in the gap a strategy leaves
    /// between rows (`layout::CONTROL_GAP` holds one).
    fn label_strip(&self) -> f32 { 0.0 }

    fn mark_dirty(&mut self, ctx: &mut UiContext) {
        let b = self.base_mut();
        if b.dirty {
            return;
        }
        b.dirty = true;
        if let Some(id) = b.id.get() {
            if let Some(parent_ptr) = ctx.tree.parent_ptr(id) {
                unsafe {
                    (*parent_ptr).mark_dirty(ctx);
                }
            }
        }
    }

    // Required (the flip): the old defaults manufactured DummyAny stand-ins nothing
    // could legitimately use. `impl_widget_base!` provides both. `as_ptr`/`as_ptr_mut`
    // are GONE from the trait (the plumbing retype): a pointer to a widget you already
    // hold is a plain cast (`w as *mut (dyn WidgetHost + 'static)`); concrete
    // registration sites ride the inherent `Adapted<W>` methods (the registration
    // bridge — derived from a live borrow, never stored beyond the registry).
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;

    fn handle_event(&mut self, event: &Event, ctx: &mut UiContext) -> bool {
        // The default serves test shims only (Adapted overrides this): base hover
        // bookkeeping on moves, tick forwarding, everything else inert — the old
        // per-method dispatch died with the direct-dispatch entry points (6bd collapse).
        match event {
            Event::PointerMove { x, y, .. } => {
                let (px, py) = (*x, *y);
                ctx.set_cursor_pos(px, py);
                let was = self.base().hovered;
                let is_hit = self.hit_test(px, py, ctx);
                self.base_mut().hovered = is_hit;
                was != is_hit
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
        let b = self.base();
        (b.x, b.y, b.w, b.h)
    }

    fn label(&self) -> Option<String> {
        self.base().label.clone()
    }

    // The value/polling block (`get_value_string`/`set_value_string`/`take_change`/
    // `take_click`/`value`/`set_text`/`set_selected`) is GONE from the trait (6bd value
    // shrink): apps drain widget state through the concrete inherent `Adapted<W>` methods
    // (which forward to the narrow `Input` hooks). The last dyn readers went concrete-slot
    // (TI's roster drain, cloud's JsonControl, designer's pane-focus sync).

    /// Dispatch a context-menu action on this widget. Returns whether it was applied.
    /// Default inert; the adapter forwards to `Input::context_action` (whose default gives
    /// every widget whole-value Cut/Copy/Paste through the value-string pair).
    fn context_action(&mut self, _action: ContextAction) -> bool {
        false
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let b = self.base_mut();
        b.x = x;
        b.y = y;
        b.w = w;
        b.h = h;
    }

    fn set_row_rect(&mut self, x: f32, w: f32) {
        let b = self.base_mut();
        b.row_x = x;
        b.row_w = w;
    }

    fn hit_test(&self, px: f32, py: f32, ctx: &UiContext) -> bool {
        if ctx.is_coordinate_covered(self.base().id(), px, py) {
            return false;
        }
        let (x, y, w, h) = self.rect();
        if w <= 0.0 || h <= 0.0 {
            return false;
        }
        let b = self.base();
        let (mut hx, mut hw) = if b.row_w > 0.0 { (b.row_x, b.row_w) } else { (x, w) };
        let label_x = self.label_x_offset();
        hx += label_x;
        hw -= label_x;
        px >= hx && px <= hx + hw && py >= y && py <= y + h
    }

    // The direct-dispatch entry points (`cursor_moved`, `on_cursor_moved`, `mouse_input`,
    // `mouse_wheel`, `keyboard_input`, `drag_begin`/`drag_update`/`drag_end`) are GONE from
    // the trait (6bd collapse): every event delivery goes through `handle_event` — the entry
    // points live on as inherent `Adapted<W>` methods for concrete in-crate forwards.

    // `hovered`/`set_hovered` are GONE from the trait (6bd batch 2): the state is the base
    // `Widget::hovered` flag, read/written directly by the defaults above; Button/Checkbox
    // keep inherent accessors for immediate-mode hosts.

    fn highlight_quad(&self, ctx: &UiContext) -> Option<(f32, f32, f32, f32, [f32; 4])> {
        // Focus/hover highlight color, folded from the zero-override `highlight_color` (6bd).
        let is_focused = ctx.is_focused_id(self.base().id());
        let hc = if is_focused {
            colors::highlight_primary_color()
        } else if self.base().hovered {
            colors::HIGHLIGHT_SECONDARY
        } else {
            return None;
        };
        let label_x = self.label_x_offset();
        let b = self.base();
        let hx = if b.row_w > 0.0 { b.row_x } else { b.x } + label_x;
        let hw = if b.row_w > 0.0 { b.row_w } else { b.w } - label_x;
        Some((hx, b.y, hw, b.h, hc))
    }

    fn color(&self) -> [f32; 4];
    fn solid_border(&self) -> Option<([f32; 4], f32)> { None }
    fn plate_bevel(&self) -> Option<f32> { None }

    // `draggable`/`is_dragging` are GONE from the trait (the ControlPanel endgame
    // removed their last stored-child-pointer consumer): the drag queries are concrete
    // inherent `Adapted<W>` reads; index-driven rosters (TI, designer) route them
    // through per-slot matches like the other value drains.

    fn label_x_offset(&self) -> f32 {
        let name = self.type_name();
        if name == "Label" || name == "Button" || name == "Checkbox" || name == "Toggle" || name == "Ramp" {
            return 0.0;
        }
        if crate::layout::control_label_layout() == "side" && self.base().label.is_some() {
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
    /// (borders, relief primitives, arcs, vectors, SVGs) can override. Recursion into children and clipping
    /// are handled by the paint walk (`scene::painter`), not here.
    fn paint_self(&self, ui: &UiContext, ctx: &mut crate::scene::paint::PaintCtx) {
        use crate::scene::layout::Rect;
        let (x, y, w, h) = self.rect();
        let rect = Rect { x, y, width: w, height: h };
        let color = self.color();

        if ui.tree.children_ptrs(self.base().id()).is_empty() {
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
                let (radius, (r1, r2, r3, r4)) = self.corner_style();
                if r1 || r2 || r3 || r4 {
                    ctx.rounded_rect(rect, radius, (r1, r2, r3, r4), color);
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

    /// Whether the paint walk should clip this widget's children to its rect (scroll/root plate
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
        let (radius, (r1, r2, r3, r4)) = self.corner_style();
        if r1 || r2 || r3 || r4 {
            let (x, y, w, h) = self.rect();
            let c = self.color();
            if c[3].abs() > 0.001 {
                quads.push((x, y, w, h, radius, c, (r1, r2, r3, r4)));
            }
        }
        for &child_ptr in &ctx.tree.children_ptrs(self.base().id()) {
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
    fn type_name(&self) -> &'static str {
        let full_name = std::any::type_name::<Self>();
        full_name.split("::").last().unwrap_or("Widget")
    }
    fn popover_rect(&self) -> Option<(f32, f32, f32, f32)> { None }
    fn render_popover(&self, _pc: &mut dyn crate::layout::RenderTarget) {}

    fn focus(&mut self) {
        self.base_mut().focused = true;
    }
    fn unfocus(&mut self) {
        self.base_mut().focused = false;
    }
    fn focused(&self, ctx: &UiContext) -> bool {
        ctx.is_focused_id(self.base().id())
    }
    fn prepare_text(&mut self, _fs: &mut cosmic_text::FontSystem) {}

    fn set_visible(&mut self, _visible: bool) {}
    fn visible(&self) -> bool { true }
    fn tick(&mut self, _dt: f32, _ctx: &mut UiContext) -> bool { false }
    fn wants_tick(&self) -> bool { false }
    fn is_child_visible(&self, _child_id: WidgetId) -> bool { true }
    fn set_modifiers(&mut self, _ctrl: bool, _shift: bool, _alt: bool) {}

    // `set_parent`/`add_child` are GONE from the trait (6bd batch 4): linking is a tree
    // operation — concrete callers ride the inherent `Adapted` methods, dyn callers go
    // through `focus::link_parent_child` or `ctx.tree` directly. `parent`/`children` are
    // GONE too (the plumbing retype): tree structure is read off `ctx.tree`
    // (`parent_id`/`parent_ptr`/`child_ids`/`children_ptrs`) — the trait no longer
    // proxies it, and no trait method returns a raw pointer. Paginator's field-derived
    // child (the one `Layout::container_children` implementor) reaches the walks through
    // the tree link its per-tick `register_embedded_children` maintains.

    fn z_index(&self) -> i32 { 0 }
    fn is_scrollable(&self) -> bool { false }
    fn blocks_root_plate_drag(&self) -> bool { true }

    /// Uniform corner radius + per-corner on-flags, in one read (6bd batch 2 — replaced the
    /// separate `corner_radius`/`rounded_corners` getters). The radius is meaningful even with
    /// every corner off: Menu/StatusBar report their parent's radius to children this way, so
    /// the flags-off channel can't be folded into `corner_radii`.
    fn corner_style(&self) -> (f32, (bool, bool, bool, bool)) {
        (12.0, (false, false, false, false))
    }

    fn corner_radii(&self) -> CornerRadii {
        let (r, (tl, tr, br, bl)) = self.corner_style();
        CornerRadii::new(
            if tl { r } else { 0.0 },
            if tr { r } else { 0.0 },
            if br { r } else { 0.0 },
            if bl { r } else { 0.0 },
        )
    }
}

// The `Control` subtrait (set_label + control_label) is DELETED (6bd value shrink):
// zero dyn consumers and zero `control_label()` callers remained; `set_label` lives on as
// the inherent `Adapted<W>` method every call site already resolved to (it shadowed the
// trait), and detached-label paint moved to the adapter in the Phase 5 leaf sweeps.

pub mod core;
pub mod input;
pub mod plate_dock;
pub mod container;
pub mod display;
pub mod editor;
pub mod layout_helper;
pub mod model;
pub mod scroll_region;
pub mod scroll_motion;
 
// Re-exports
pub use self::editor::TextEditorState;
pub use self::layout_helper::{ColumnLayout, RowLayout};
pub use self::scroll_region::{ScrollRegion, ScrollbarActivity};
pub use self::scroll_motion::{Bounds, ScrollAxis, ScrollMotion, ScrollPhase, ScrollSettings, LINE_PX};
pub use self::model::{Adapted, EventCtx, Input, Layout, Paint};
pub use self::core::{Widget, focus, hover_animation, clipboard, context_menu, clear_widget_references};
pub use self::core::focus::link_parent_child;
pub use self::input::{
    Button, TextBox, Spinbox, Dropdown, Checkbox, Toggle, Slider, RangeSlider,
    ColorSelector, Finger, Trackpad, get_font_db, ActiveThumb, FontSelector,
    BevelPreview, bevel_ease, parse_bevel_knobs, RampPreview,
    ButtonStrip, KeybindRecorder, Ramp, RampKey, ColorRamp, ColorRampKey,
    format_ramp_spec, parse_ramp_spec
};
pub use self::container::{
    ContainerLayout, OverlayLayout, ManualLayout, VerticalLayout, GridLayout, AdaptiveGridLayout,
    ColumnsLayout, MosaicLayout, ReverseMosaicLayout,
    ContentBg, ParametersBg,
    ScrollBox, MenuBar, Spreadsheet, Breadcrumb,
    Paginator, TreeList, TreeElement
};
pub use self::display::{
    TextLabel, Label, StyledLabel, LabelPrim, TextItem, UsageBar,
    InfoBox, StatusDot, InteractiveListItem,
    GraphNode, Graph, TaggedQuad, Float3, ProgressBar, StatusBar, Splitter, Node, Separator,
    DotStatus, Panel, ImageView, serialize_widgets,
    truncate_head, truncate_tail,
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
    /// A node dropped onto a wire, to be spliced in between its ends:
    /// (dragged node id, the wire's upstream node NAME — what Input params
    /// store, the wire's downstream node id). The host rewires both Input
    /// params: dragged.Input = upstream name, downstream.Input = dragged's
    /// name. Default None for hosts whose graphs have no wires to splice.
    fn take_pending_splice(&mut self) -> Option<(String, String, String)> {
        None
    }
    fn cancel_connecting(&mut self);
    fn is_node_rect(&self, qx: f32, qy: f32, qw: f32, qh: f32) -> bool;
    /// The topmost node whose body contains (px, py), window-absolute coords.
    fn node_at(&self, px: f32, py: f32) -> Option<usize>;
    /// The grid cells' superellipse corner radius at the current zoom (0 = square).
    fn cell_corner_radius(&self) -> f32;
    /// The flat-geometry emission with grid cells tagged by their surviving
    /// rounded corners — for hosts that draw the graph's quads themselves
    /// (the designer) and want cells as superellipse tiles.
    fn geometry_quads_tagged(&self, rect: crate::scene::layout::Rect) -> Vec<TaggedQuad>;
    /// The pixel rect of the cell an in-flight node drag will deposit on
    /// (`commit_drag`'s resolution), for hosts' drop-target highlight.
    /// None outside a node drag.
    fn drop_target_cell_rect(&self) -> Option<(f32, f32, f32, f32)>;
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

pub fn label_offset(w: &dyn WidgetHost) -> f32 {
    let name = w.type_name();
    if name == "Label" || name == "Button" || name == "Checkbox" || name == "Toggle" {
        return 0.0;
    }
    w.base().label_offset()
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
    let mut req_alt = false;
    let mut req_key = "";

    for part in parts {
        match part {
            "ctrl" | "control" => req_ctrl = true,
            "shift" => req_shift = true,
            "alt" | "meta" => req_alt = true,
            // Super chords belong to the compositor; a client never sees them.
            "super" | "win" | "logo" => {}
            k => req_key = k,
        }
    }

    if event.ctrl != req_ctrl { return false; }
    if event.shift != req_shift { return false; }
    if event.alt != req_alt { return false; }
    
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

