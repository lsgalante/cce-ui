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

    /// The notches a VALUE control takes, "up is more": a wheel notch up
    /// is positive, and a finger's travel is positive when the fingers
    /// went UP — which under natural scrolling is the negative of the
    /// pixel delta, since that delta is what a list scrolls by and a
    /// natural list moves its content the way the fingers went. Until
    /// 2026-09-30 every value control read `notches_y` and each had picked
    /// a sign: the slider was right for a natural trackpad and backwards
    /// for a wheel, the spinbox and the menu and palette sliders the other
    /// way round.
    pub fn value_notches_y(&self) -> f32 {
        self.value_notches_of(crate::input::natural_scroll())
    }

    /// [`Self::value_notches_y`] for a given natural-scroll setting.
    pub fn value_notches_of(&self, natural: bool) -> f32 {
        match self {
            MouseScrollDelta::LineDelta(_x, y) => *y,
            MouseScrollDelta::PixelDelta(pos) => {
                let n = (pos.y as f32) / 60.0;
                if natural { -n } else { n }
            }
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
    // The function keys. F5 came first (the login greeter's restart); the
    // rest arrived together on 2026-09-25 for the greeter's F1 power off and
    // F2 reboot.
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
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

pub use crate::scene::paint::{ControlPlate, PlateStance};
pub use crate::widget::model::FocusRole;
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

    /// Where the registry should point for this widget, and the token that says the address
    /// still holds it — `Some` only for a widget whose address cannot move under the registry
    /// ([`Owned`]). `None` (every other widget) registers the widget's own address, watched by
    /// its base's liveness token, which catches a drop but not a move.
    fn stable_target(&mut self) -> Option<(*mut (dyn WidgetHost + 'static), std::sync::Weak<()>)> {
        None
    }

    /// The height of the detached-label strip above this widget's content: zero for
    /// unlabeled widgets and for those whose base label IS their content
    /// ([`Layout::inline_label`]). A widget's rect is always its content plus this
    /// strip — `set_rect` takes that block, `layout` lands the content at the origin
    /// and hangs the strip above it. A strategy reserves that
    /// row above every child's content (`container_layout::label_lead`) and puts
    /// `layout::CONTROL_GAP` between the blocks.
    fn label_strip(&self) -> f32 { self.base().label_offset() }

    /// Where the detached label is drawn: the strip above the content, as wide as the
    /// label's text. `None` for an unlabeled widget and for an inline label. The label
    /// may be wider than the widget's rect (a StatusDot's, a Checkbox's) — the rect is
    /// the content's width, and the text runs past it — so anything wrapping a widget
    /// as a block (a `Group`'s hull) unions this with the rect.
    fn detached_label_rect(&self) -> Option<crate::scene::layout::Rect> { None }


    // Required (the flip): the old defaults manufactured DummyAny stand-ins nothing
    // could legitimately use. `impl_widget_base!` provides both. `as_ptr`/`as_ptr_mut`
    // are GONE from the trait (the plumbing retype): a pointer to a widget you already
    // hold is a plain cast (`w as *mut (dyn WidgetHost + 'static)`); concrete
    // registration sites ride the inherent `Adapted<W>` methods (the registration
    // bridge — derived from a live borrow, never stored beyond the registry).
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;

    /// The widget's own model, as its narrow traits: what [`WidgetHostExt`] reads its
    /// one-line answers off (`Adapted` hands out its inner widget; a test shim with no
    /// model gets [`NoModel`]'s defaults).
    fn layout_model(&self) -> &dyn Layout {
        &NoModel
    }
    fn paint_model(&self) -> &dyn Paint {
        &NoModel
    }
    fn input_model(&self) -> &dyn Input {
        &NoModel
    }
    fn input_model_mut(&mut self) -> &mut dyn Input {
        // A zero-sized value: leaking it allocates nothing.
        Box::leak(Box::new(NoModel))
    }

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

    /// Land the CONTENT box at `origin`, the label strip hanging above it — the one
    /// placement contract (`Adapted` repeats it over its measured content size).
    fn layout(&mut self, origin: Point, constraints: LayoutConstraints, ctx: &mut UiContext) {
        let size = self.measure(constraints, ctx);
        let strip = self.label_strip();
        self.set_rect(origin.x, origin.y - strip, size.width, size.height + strip);
    }

    fn rect(&self) -> (f32, f32, f32, f32) {
        let b = self.base();
        (b.x, b.y, b.w, b.h)
    }


    // The value/polling block (`get_value_string`/`set_value_string`/`take_change`/
    // `take_click`/`value`/`set_text`/`set_selected`) is GONE from the trait (6bd value
    // shrink): apps drain widget state through the concrete inherent `Adapted<W>` methods
    // (which forward to the narrow `Input` hooks). The last dyn readers went concrete-slot
    // (TI's roster drain, cloud's JsonControl, designer's pane-focus sync).


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
        let (hx, hw) = if b.row_w > 0.0 { (b.row_x, b.row_w) } else { (x, w) };
        px >= hx && px <= hx + hw && py >= y && py <= y + h
    }

    // The direct-dispatch entry points (`cursor_moved`, `on_cursor_moved`, `mouse_input`,
    // `mouse_wheel`, `keyboard_input`, `drag_begin`/`drag_update`/`drag_end`) are GONE from
    // the trait (6bd collapse): every event delivery goes through `handle_event` — the entry
    // points live on as inherent `Adapted<W>` methods for concrete in-crate forwards.

    // `hovered`/`set_hovered` are GONE from the trait (6bd batch 2): the state is the base
    // `Widget::hovered` flag, read/written directly by the defaults above; Button/Checkbox
    // keep inherent accessors for immediate-mode hosts.



    // `draggable`/`is_dragging` are GONE from the trait (the ControlPanel endgame
    // removed their last stored-child-pointer consumer): the drag queries are concrete
    // inherent `Adapted<W>` reads; index-driven rosters (TI, designer) route them
    // through per-slot matches like the other value drains.

    

    /// Emit this widget's OWN primitives (non-recursive) into the single paint pass (Phase 3).
    /// Every production host overrides this (`Adapted`, and `Owned` forwarding to it); the
    /// default serves a host with no widget of its own (the test shims): its plate — a solid
    /// border, or a rounded fill in its colour where its corner style rounds — then what its
    /// model paints, text aside. Recursion into children and clipping are the paint walk's
    /// (`scene::painter`), not this.
    fn paint_self(&self, ui: &UiContext, ctx: &mut crate::scene::paint::PaintCtx) {
        use crate::scene::layout::Rect;
        use crate::scene::paint::{PaintCtx, Prim};
        let (x, y, w, h) = self.rect();
        let rect = Rect { x, y, width: w, height: h };
        let color = self.color();
        if let Some((border_color, thickness)) = self.solid_border() {
            let cr = self.corner_radii();
            ctx.border(rect, (cr.top_left, cr.top_right, cr.bottom_right, cr.bottom_left), color, border_color, thickness);
        } else if let Some((radius, corners)) = self.paint_model().corner_style(self.content_rect()) {
            if corners != (false, false, false, false) && color[3].abs() > 0.001 {
                ctx.rounded_rect(rect, radius, corners, color);
            }
        }
        if !self.visible() {
            return;
        }
        // Text: none. A host with text of its own overrides this; emitting the model's text
        // here would double what the walk's descent draws for a container (the Phase 6d trap).
        let mut tmp = PaintCtx::new();
        self.paint_model().paint_ui(ui, self.content_rect(), &mut tmp);
        for item in tmp.finish().items {
            // `replay` emits every prim but Text, which it hands back; dropping it is the point.
            let _text: Option<Prim> = ctx.replay(item.prim);
        }
    }





    // The per-widget text getters (text_labels / text_labels_with_bounds /
    // text_labels_with_font_and_bounds / get_text_items) are GONE: every widget emits
    // its own text as display-list prims via paint_self (Adapted::paint_self). The
    // deleted default's base-label synthesis lives on in Adapted's base-label fallback,
    // and its scroll-ancestor clamp in scene::painter::scroll_ancestor_text_bounds.

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
    fn is_child_visible(&self, _child_id: WidgetId) -> bool { true }

    /// Put the widget's embedded children (`widget::Embedded`) into `ctx`: what
    /// `UiContext::insert` calls once the widget is in. The adapter forwards to
    /// `Layout::register_embedded_children`, which also runs on every layout and tick.
    fn attach_embedded(&mut self, _ctx: &mut UiContext) {}

    /// Take the widget's embedded children back out of `ctx`, so it leaves whole: what
    /// `UiContext::remove` calls before the widget goes. The adapter forwards to
    /// `Layout::release_embedded_children`.
    fn release_embedded(&mut self, _ctx: &mut UiContext) {}

    // `set_parent`/`add_child` are GONE from the trait (6bd batch 4): linking is a tree
    // operation — concrete callers ride the inherent `Adapted` methods, dyn callers go
    // through `focus::link_parent_child` or `ctx.tree` directly. `parent`/`children` are
    // GONE too (the plumbing retype): tree structure is read off `ctx.tree`
    // (`parent_id`/`parent_ptr`/`child_ids`/`children_ptrs`) — the trait no longer
    // proxies it, and no trait method returns a raw pointer. Paginator's field-derived
    // child (the one `Layout::container_children` implementor) reaches the walks through
    // the tree link its per-tick `register_embedded_children` maintains.









    /// The parts a screen reader sees as nodes of their own (`Input::a11y_items`).
    fn a11y_items(&self) -> Vec<crate::a11y::A11yItem> {
        Vec::new()
    }


}

/// The host surface that does not need a slot of its own in `WidgetHost`: what a widget's
/// narrow traits answer ([`Input`], [`Paint`], [`Layout`], reached through the host's
/// [`WidgetHost::input_model`] / [`paint_model`](WidgetHost::paint_model) /
/// [`layout_model`](WidgetHost::layout_model)) and what is derived from the host's own state.
/// Implemented for every host, `dyn WidgetHost` included, so `w.focus_role()` reads as it
/// always did — with this trait in scope (`use cce_ui::widget::WidgetHostExt`). Until
/// 2026-10-08 each of these was a `WidgetHost` method that `Adapted` overrode with a one-line
/// forward and `Owned` forwarded again.
pub trait WidgetHostExt: WidgetHost {
    /// This widget's part in keyboard navigation — `Input::focus_role` through
    /// the adapter; `FocusRole::None` for anything that is not a plate or a well.
    fn focus_role(&self) -> FocusRole {
        self.input_model().focus_role()
    }

    /// Whether, focused, it takes Tab itself instead of the Tab walk (`Input::keeps_tab`).
    fn keeps_tab(&self) -> bool {
        self.input_model().keeps_tab()
    }

    fn blocks_root_plate_drag(&self) -> bool {
        self.input_model().blocks_root_plate_drag()
    }

    fn wants_tick(&self) -> bool {
        self.input_model().wants_tick()
    }

    fn is_scrollable(&self) -> bool {
        self.input_model().scrollable()
    }

    /// An explicit accessibility role, overriding the guess `crate::a11y::role_for` makes
    /// from the widget's type and focus role. Default `None`.
    fn a11y_role(&self) -> Option<accesskit::Role> {
        self.input_model().a11y_role()
    }

    /// The widget's value for assistive technology: a field's text, a slider's number, a
    /// check box's "true" / "false". Default `None`.
    fn a11y_value(&self) -> Option<String> {
        self.input_model().value_string()
    }

    /// The `(min, max, step)` an assistive tool may set the value in (`Input::a11y_range`).
    fn a11y_range(&self) -> Option<(f64, f64, f64)> {
        self.input_model().a11y_range()
    }

    /// Set the value an assistive tool asked for (`Input::a11y_set_value`).
    fn a11y_set_value(&mut self, value: f64) -> bool {
        self.input_model_mut().a11y_set_value(value)
    }

    /// An assistive tool clicked one of them (`Input::a11y_select_item`).
    fn a11y_select_item(&mut self, idx: usize) -> bool {
        self.input_model_mut().a11y_select_item(idx)
    }

    fn set_modifiers(&mut self, ctrl: bool, shift: bool, alt: bool) {
        self.input_model_mut().set_modifiers(ctrl, shift, alt)
    }

    /// Dispatch a context-menu action on this widget. Returns whether it was applied.
    /// Default inert; the adapter forwards to `Input::context_action` (whose default gives
    /// every widget whole-value Cut/Copy/Paste through the value-string pair).
    fn context_action(&mut self, action: ContextAction) -> bool {
        self.input_model_mut().context_action(action)
    }

    fn color(&self) -> [f32; 4] {
        self.paint_model().color()
    }

    fn solid_border(&self) -> Option<([f32; 4], f32)> {
        self.paint_model().solid_border()
    }

    fn widget_font(&self) -> Option<String> {
        self.paint_model().widget_font()
    }

    /// Whether the paint walk should clip this widget's children to its rect (scroll/root plate
    /// containers). Default: no clipping.
    fn clips_children(&self) -> bool {
        self.paint_model().clips_children()
    }

    /// Whether this widget paints its ENTIRE subtree itself through its (recursive)
    /// `all_rounded_quads` / `all_quads` — a legacy "subtree painter" such as `TreeList`, whose
    /// row backgrounds and separators live in an `all_rounded_quads` override that also recurses
    /// into its children. When true, the paint walk emits those directly and does NOT recurse
    /// (the widget already did). Transitional: such widgets will eventually get a proper
    /// non-recursive `paint_self`. Default: false.
    fn renders_own_subtree(&self) -> bool {
        self.paint_model().paints_own_subtree()
    }

    fn z_index(&self) -> i32 {
        self.layout_model().z_order()
    }

    /// The widget's natural CONTENT height — the control below its detached label, if
    /// any. What a layout strategy allots; [`WidgetHost::layout`] places that content
    /// box at the origin it is given and hangs the label ([`WidgetHost::label_strip`])
    /// above it. `None` when the widget has no natural height.
    fn preferred_height(&self) -> Option<f32> {
        self.layout_model().intrinsic_size().map(|s| s.height)
    }

    fn label(&self) -> Option<String> {
        self.base().label.clone()
    }

    fn corner_radii(&self) -> CornerRadii {
        let (r, (tl, tr, br, bl)) =
            self.paint_model().corner_style(self.content_rect()).unwrap_or((0.0, (false, false, false, false)));
        CornerRadii::new(
            if tl { r } else { 0.0 },
            if tr { r } else { 0.0 },
            if br { r } else { 0.0 },
            if bl { r } else { 0.0 },
        )
    }

    fn mark_dirty(&mut self, ctx: &mut UiContext){
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

    // ── What the widget paints, read from its model ──────────────────────────────
    // The legacy tuple views (`extra_quads`, `all_quads`, `all_rounded_quads`,
    // `extra_arcs`, `extra_circles`, `highlight_quad`, `corner_style`) are gone since
    // 2026-10-08: every host paints a widget through `paint_self` / the paint walk, and a
    // composite that draws a child's chrome in its own order reads `painted_prims`.

    /// The rect the widget's model paints into: the host's rect below its detached label.
    fn content_rect(&self) -> crate::scene::layout::Rect {
        let b = self.base();
        let top = if self.layout_model().inline_label() { 0.0 } else { b.label_offset() };
        // Deliberately NOT clamped at zero: hosts under-size labeled sliders (label taller
        // than the assigned rect), and the negative-height quads still rasterize.
        crate::scene::layout::Rect { x: b.x, y: b.y + top, width: b.w, height: b.h - top }
    }

    /// Everything the widget's model paints into [`content_rect`](Self::content_rect), as prims.
    fn painted_prims(&self) -> Vec<crate::scene::paint::Prim> {
        let mut pc = crate::scene::paint::PaintCtx::new();
        self.paint_model().paint(self.content_rect(), &mut pc);
        pc.finish().items.into_iter().map(|item| item.prim).collect()
    }

    /// The container's children that pass its [`Layout::child_visible`] policy; empty for a
    /// non-container.
    fn visible_children(&self) -> Vec<*mut (dyn WidgetHost + 'static)> {
        let model = self.layout_model();
        if !model.has_container_children() {
            return Vec::new();
        }
        model.container_children().into_iter().filter(|c| model.child_visible(*c)).collect()
    }
}

impl<T: WidgetHost + ?Sized> WidgetHostExt for T {}

/// A shown widget's prims as its model paints them, nothing while it is hidden — for a
/// composite that draws a child's chrome in its own order rather than walking it (the params
/// pane's rows, the ramp's key editor, the menubar's strip).
pub(crate) fn shown_prims<W: WidgetHost + ?Sized>(w: &W) -> Vec<crate::scene::paint::Prim> {
    if w.visible() { w.painted_prims() } else { Vec::new() }
}

/// The plain quads among [`shown_prims`], as `(x, y, w, h, colour)`.
pub(crate) fn shown_quads<W: WidgetHost + ?Sized>(w: &W) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
    shown_prims(w)
        .into_iter()
        .filter_map(|prim| match prim {
            crate::scene::paint::Prim::Quad { rect, color } => Some((rect.x, rect.y, rect.width, rect.height, color)),
            _ => None,
        })
        .collect()
}

/// The rounded quads among [`shown_prims`], as `(x, y, w, h, radius, colour, corners)`.
pub(crate) fn shown_rounded_quads<W: WidgetHost + ?Sized>(
    w: &W,
) -> Vec<(f32, f32, f32, f32, f32, [f32; 4], (bool, bool, bool, bool))> {
    shown_prims(w)
        .into_iter()
        .filter_map(|prim| match prim {
            crate::scene::paint::Prim::RoundedRect { rect, radius, corners, color } => {
                Some((rect.x, rect.y, rect.width, rect.height, radius, color, corners))
            }
            _ => None,
        })
        .collect()
}

/// The narrow traits' defaults, for a host that has no widget model of its own (the test
/// shims that implement `WidgetHost` directly): transparent, no focus role, no value.
pub struct NoModel;
impl Layout for NoModel {}
impl Paint for NoModel {
    fn color(&self) -> [f32; 4] {
        [0.0; 4]
    }
}
impl Input for NoModel {}


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
pub mod shaping;
#[cfg(feature = "markdown")]
pub mod markdown;

/// An image a host has ready for a Markdown embed (`![[pic.png]]`): its id
/// from `vk::upload_rgba` and its size in px. `MarkdownView` and
/// `DocEditor` ask the host for one by the embed's link text — sizing at
/// layout, the id again at paint, so a re-upload after a reconnect needs
/// no relayout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmbedImage {
    pub id: u32,
    pub width: u32,
    pub height: u32,
}

impl EmbedImage {
    /// The size it draws at in a column `max_w` wide: the requested width
    /// (else its own), the requested height (else the aspect's), scaled
    /// down as a whole to fit the column.
    pub fn fit(&self, want_w: Option<u32>, want_h: Option<u32>, max_w: f32) -> (f32, f32) {
        let (iw, ih) = (self.width.max(1) as f32, self.height.max(1) as f32);
        let w = want_w.map_or(iw, |w| w as f32);
        let h = want_h.map_or(w * ih / iw, |h| h as f32);
        if w > max_w {
            (max_w, h * max_w / w)
        } else {
            (w, h)
        }
    }
}
#[cfg(feature = "doc_editor")]
pub mod doc_editor;
pub mod line_edit;
pub mod model;
pub mod owned;
pub mod handle;
pub mod embedded;
pub mod scroll_region;
pub mod scroll_motion;
pub mod side_swipe;
 
// Re-exports
pub use self::editor::TextEditorState;
pub use self::line_edit::{EditOutcome, LineEdit};
pub use self::scroll_region::{ScrollRegion, ScrollbarActivity};
pub use self::side_swipe::{SideSwipe, SwipeDir};
pub use self::scroll_motion::{Bounds, ScrollAxis, ScrollMotion, ScrollPhase, ScrollSettings, LINE_PX};
pub use self::model::{Adapted, EventCtx, Input, Layout, Paint};
pub use self::owned::Owned;
pub use self::handle::Handle;
pub use self::embedded::Embedded;
pub use self::core::{Widget, focus, hover_animation, clipboard, context_menu, clear_widget_references};
pub use self::core::focus::link_parent_child;
pub use self::input::{
    Button, TextBox, Spinbox, Dropdown, Checkbox, Toggle, RadioGroup, Slider, RangeSlider,
    ColorSelector, Finger, Trackpad, get_font_db, ActiveThumb, FontSelector,
    BevelPreview, bevel_ease, parse_bevel_knobs, RampPreview,
    ButtonStrip, KeybindRecorder, Ramp, RampKey, ColorRamp, ColorRampKey,
    format_ramp_spec, parse_ramp_spec
};
pub use self::container::{
    Dialog, Group, GroupFrame,
    ContainerLayout, OverlayLayout, VerticalLayout, GridLayout, AdaptiveGridLayout,
    ColumnsLayout, MosaicLayout, ReverseMosaicLayout,
    ContentBg, ParametersBg,
    ScrollBox, MenuBar, SheetColumn, Spreadsheet, Breadcrumb,
    Paginator, TreeList, TreeElement
};
pub use self::display::{
    TextLabel, Label, StyledLabel, LabelPrim, TextItem, UsageBar,
    InfoBox, StatusDot, InteractiveListItem,
    GraphNode, Graph, TaggedQuad, node_wires, Float3, ProgressBar, StatusBar, Splitter, Separator,
    DotStatus, ImageView,
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
    /// The grid's ONE size per axis: the pitch, from the centre of one grid
    /// line to the centre of the next. Nodes are centred on the lattice
    /// intersections. Leaves the node size alone.
    fn set_grid_pitch(&mut self, px: f32, py: f32);
    /// The node body's size, independent of the pitch — hosts scale it with
    /// their zoom as they scale the pitch.
    fn set_node_size(&mut self, w: f32, h: f32);
    /// The older cell-and-gap description of the same lattice — a cell plus
    /// its gap is a pitch, and the node body is the cell. Kept for hosts
    /// that still speak it (cce-files, cce-graph); new code sets the pitch.
    fn set_grid_sizes(&mut self, gx: f32, gy: f32);
    /// The gap half of the cell-and-gap description; see [`set_grid_sizes`].
    fn set_skipped_sizes(&mut self, row_h: f32, col_w: f32);
    /// The lattice intersection node (0, 0) is centred on, window-absolute.
    fn set_grid_origin(&mut self, ox: f32, oy: f32);
    fn grid_origin(&self) -> (f32, f32);
    fn set_show_network_grid(&mut self, show: bool);
    fn take_pending_connection(&mut self) -> Option<(String, String)>;
    /// [`Self::take_pending_connection`] with the INPUT PORT the connection
    /// was dropped on: (input node id, output node name, port). A host whose
    /// nodes read several wires (the k-th `node` parameter into port k)
    /// writes the one the port is. Taking either takes the connection.
    fn take_pending_connection_to_port(&mut self) -> Option<(String, String, usize)> {
        self.take_pending_connection().map(|(id, name)| (id, name, 0))
    }
    /// A node dropped onto a wire, to be spliced in between its ends:
    /// (dragged node id, the wire's upstream node NAME — what Input params
    /// store, the wire's downstream node id). The host rewires both Input
    /// params: dragged.Input = upstream name, downstream.Input = dragged's
    /// name. Default None for hosts whose graphs have no wires to splice.
    fn take_pending_splice(&mut self) -> Option<(String, String, String)> {
        None
    }
    /// A node dropped onto another node, which it swapped places with:
    /// (dragged node id, the other node's id). The widget has traded their
    /// cells; the host trades the rest. Only while the host opted in
    /// (`Graph::set_swap_on_drop`); default None.
    fn take_pending_swap(&mut self) -> Option<(String, String)> {
        None
    }
    /// The wire into an Input that runs through the body a node would have
    /// at lattice cell (col, row), as (upstream id, downstream id): where a
    /// node ADDED there splices in, by the hit test a drop uses. Default
    /// None for hosts whose graphs have no wires to splice.
    fn input_wire_through_cell(&self, _col: f32, _row: f32) -> Option<(String, String)> {
        None
    }
    fn cancel_connecting(&mut self);
    fn is_node_rect(&self, qx: f32, qy: f32, qw: f32, qh: f32) -> bool;
    /// The topmost node whose body contains (px, py), window-absolute coords.
    fn node_at(&self, px: f32, py: f32) -> Option<usize>;
    /// The corner radius of anything node-shaped on the grid at the current
    /// zoom — the cursor, the drop-target highlight (0 = square).
    fn cell_corner_radius(&self) -> f32;
    /// The flat-geometry emission with grid cells tagged by their surviving
    /// rounded corners — for hosts that draw the graph's quads themselves
    /// (the designer) and want cells as superellipse tiles.
    fn geometry_quads_tagged(&self, rect: crate::scene::layout::Rect) -> Vec<TaggedQuad>;
    /// The grid lines and origin axes, flat, over what is beneath — for
    /// hosts that draw the graph's quads themselves, called at the point in
    /// their walk where the grid goes (under the wires and nodes).
    fn paint_grid(&self, rect: crate::scene::layout::Rect, pc: &mut crate::scene::paint::PaintCtx);
    /// The wires and the connection being dragged out, stroked in the wire
    /// style in effect — for hosts that draw the graph's quads themselves,
    /// called after [`paint_grid`](Self::paint_grid) and under the nodes.
    /// The quads carry no wires.
    fn paint_wires(&self, rect: crate::scene::layout::Rect, pc: &mut crate::scene::paint::PaintCtx);
    /// The pixel rect the in-flight node drag will deposit its body on
    /// (`commit_drag`'s resolution), for hosts' drop-target highlight.
    /// None outside a node drag.
    fn drop_target_cell_rect(&self) -> Option<(f32, f32, f32, f32)>;
}

pub trait SpreadsheetController {
    fn set_spreadsheet_data(&mut self, headers: Vec<String>, rows: Vec<Vec<String>>);
    /// The table as columns of values ([`SheetColumn`]), a column a header:
    /// the cells are written as they are painted, so a refill costs a copy
    /// of the values. Rows of text, by default.
    fn set_spreadsheet_columns(&mut self, headers: Vec<String>, columns: Vec<SheetColumn>) {
        let n = columns.iter().map(SheetColumn::len).max().unwrap_or(0);
        let rows = (0..n).map(|r| columns.iter().map(|c| c.cell(r)).collect()).collect();
        self.set_spreadsheet_data(headers, rows);
    }
    /// The selected rows, as indices into the rows last set, ascending.
    fn selected_rows(&self) -> Vec<usize> {
        Vec::new()
    }
    /// Replace the selection; a row the table does not have is left out.
    fn set_selected_rows(&mut self, _rows: &[usize]) {}
    /// Whether the selection changed since this was last asked.
    fn take_selection_change(&mut self) -> bool {
        false
    }
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


#[cfg(test)]
mod value_notch_tests {
    use super::{MouseScrollDelta, Position};

    /// A value control reads "up is more": a wheel notch up is positive
    /// either way; a finger's pixel delta is taken as it comes with natural
    /// scrolling off, and negated with it on, since that delta is what a
    /// list scrolls by and a natural list follows the fingers.
    #[test]
    fn a_value_control_reads_up_as_more_on_a_wheel_and_a_natural_finger() {
        let wheel_up = MouseScrollDelta::LineDelta(0.0, 1.0);
        let finger = MouseScrollDelta::PixelDelta(Position { x: 0.0, y: -60.0 });
        assert_eq!(wheel_up.value_notches_of(false), 1.0);
        assert_eq!(wheel_up.value_notches_of(true), 1.0);
        assert_eq!(finger.value_notches_of(false), -1.0, "natural off: the delta as it comes");
        assert_eq!(finger.value_notches_of(true), 1.0, "natural on: the fingers went up, so more");
        // Under `cfg(test)` the toolkit's own suite reads natural as off,
        // unless a thread forces it.
        assert_eq!(finger.value_notches_y(), -1.0);
        crate::input::force_natural_scroll(Some(true));
        assert_eq!(finger.value_notches_y(), 1.0);
        crate::input::force_natural_scroll(None);
        assert_eq!(finger.value_notches_y(), -1.0);
    }
}
