//! Narrow, single-concern widget traits + an adapter into the legacy `WidgetHost` tree — Phase 5 of
//! the core rebuild (see `docs/rfc-core-rebuild.md` §3.5 and §5).
//!
//! Phase 5 replaces the ~123-method [`WidgetHost`] god-trait with small traits, one per concern. A
//! *non-breaking supertrait carve-out* of `WidgetHost` is not possible in Rust, for two reasons found
//! by experiment:
//!
//! 1. The structural methods the layout/paint passes need (`rect`, `children`, `set_rect`, …) are
//!    overridden in dozens of widgets across cce-ui **and** the app crates. Moving them off
//!    `WidgetHost` breaks every override; merely *declaring* them on a supertrait breaks every call
//!    site too, because a supertrait method is always in scope on the subtrait — `elem.children()`
//!    on a `&dyn WidgetHost` becomes ambiguous.
//! 2. Trait-object coercion does not offer a way around it: a blanket "view" impl
//!    `impl<T: WidgetHost> Paint for T` does **not** let `&dyn WidgetHost` coerce to `&dyn Paint`
//!    (that coercion only exists for real supertraits).
//!
//! So we take the RFC's recommended **adapter** path. The traits here — [`Layout`] and [`Paint`] —
//! are *independent* of `WidgetHost` (no super/sub relationship). A widget written against them is
//! placed into the existing `*mut dyn WidgetHost` tree by wrapping it in [`Adapted`], whose `WidgetHost`
//! impl forwards each legacy method to the matching narrow-trait method and supplies the
//! [`Widget`] base that `WidgetHost`'s rect/id/dirty machinery reads. Existing `impl WidgetHost` widgets
//! are untouched; new or migrated widgets implement only the concern traits they need; both kinds
//! coexist in one tree. When the last widget is migrated, `WidgetHost` and this adapter are deleted.
//!
//! This commit lands the two concerns the scene passes already consume: [`Layout`] drives
//! [`crate::scene::bridge`] and [`Paint`] drives [`crate::scene::painter`]. The input/event
//! concern follows in its own commit.

use crate::scene::layout::{Rect, Size};
use crate::scene::paint::{PaintCtx, Prim};
use crate::widget::{
    WidgetHost, Event, TextLabel, UiContext, Widget, WidgetId,
};

/// Layout inputs for the scene layout engine — the RFC's `Widget` concern, named `Layout` here to
/// avoid the existing [`Widget`] base struct.
pub trait Layout {
    /// Intrinsic content size of a leaf (e.g. measured text), consumed by the adapter's
    /// `measure` (gated on [`Layout::intrinsic_measure_width`]).
    fn intrinsic_size(&self) -> Option<Size> {
        None
    }

    /// Whether this widget draws its control label *inline* (inside its own rect, like
    /// `Checkbox`/`Toggle`/`Button`) rather than detached above it (like `ProgressBar`/`Slider`).
    /// Inline-label widgets get no `set_rect` height inflation and no content-rect inset —
    /// mirroring the legacy `label_offset` free function's type-name special cases.
    fn inline_label(&self) -> bool {
        false
    }

    /// Whether `set_rect` grows the widget past the assigned rect to make room for a detached
    /// label above (`ProgressBar`'s legacy convention). Sliders keep the assigned rect and let
    /// the label eat into it instead. Irrelevant for inline-label widgets. Default: grow.
    fn inflates_label_rect(&self) -> bool {
        true
    }

    /// Horizontal inset of the detached base label. Legacy split: `Control::control_label`
    /// added +4px (Spinbox et al., explicitly zeroed for Slider/RangeSlider); the default
    /// `text_labels` used none (ProgressBar). Default: none.
    fn detached_label_inset(&self) -> f32 {
        0.0
    }

    /// Whether `WidgetHost::measure` should prefer [`intrinsic_size`](Layout::intrinsic_size)'s
    /// width over the current rect width (Dropdown's `auto_width` measure override — hosts size
    /// it from `measure`, e.g. cce-system-interface' page dropdown). Default: keep the legacy
    /// `WidgetHost::measure` width (the current rect's).
    fn intrinsic_measure_width(&self) -> bool {
        false
    }

    /// Whether the adapter's hit test substitutes the base row rect (`row_x`/`row_w`, pushed in
    /// by row-layout hosts via `set_row_rect`) plus the side-label inset — the legacy
    /// `WidgetHost::hit_test` default geometry. Migrated controls so far dropped it (accepted
    /// drift); TextBox restores it (cce-files' save-name box relies on row hits). Default: off,
    /// keeping the other migrated widgets exactly as they shipped.
    fn hit_row_rect(&self) -> bool {
        false
    }

    /// Adjust a row-rect assignment before it lands on the base (`WidgetHost::set_row_rect` —
    /// TextBox clamps the row width to its `width`/`max_width`). Default: identity.
    fn adjust_row_rect(&self, x: f32, w: f32) -> (f32, f32) {
        (x, w)
    }

    /// The final base rect landed from a `set_rect`, visible or not — unlike
    /// [`arrange_children`](Layout::arrange_children), which the adapter gates on visibility.
    /// TextBox caches it (its cursor/scroll math reads the laid-out rect between events) and
    /// re-clamps its scroll, the legacy `set_rect` side effect. Default: ignore.
    fn rect_assigned(&mut self, _rect: Rect) {}

    // --- Container concern (transitional). Legacy containers own `Vec<*mut dyn WidgetHost>`
    // children (child-arranging `set_rect` has no ctx to reach the tree) and every one
    // hand-copies the same subtree plumbing: geometry/text aggregation, tick/popover/text-item
    // recursion, hit-through-children. A migrated container keeps the pointer Vec in its model
    // (exposed through these hooks) and the ADAPTER does the shared plumbing once, filtered by
    // `child_visible`. What stays per-widget: child arrangement (`arrange_children` /
    // `layout_children_ctx`) and any event proxying (in `on_event`, via `EventCtx::ui`).
    // Dies with `WidgetHost`: the arena owns the tree and the scene walk owns recursion.

    /// Whether this widget is a container serving
    /// [`container_children`](Layout::container_children). Cheap gate, checked per getter.
    fn has_container_children(&self) -> bool {
        false
    }

    /// The container's child pointers, in stacking order.
    fn container_children(&self) -> Vec<*mut (dyn WidgetHost + 'static)> {
        Vec::new()
    }

    /// Adjust a rect assignment before it lands on the base (Switcher clamps to its parent).
    /// Default: identity.
    fn adjust_rect(&self, requested: Rect) -> Rect {
        requested
    }

    /// Position children after a `set_rect` (no ctx available — use the owned pointers).
    /// Called only while the widget is visible, matching the legacy overrides. `host` is the
    /// adapter's `*mut dyn WidgetHost` — widgets that embed a legacy child (MenuBar's
    /// ButtonStrip) parent it back to the host so legacy parent-chain styling walks work.
    fn arrange_children(&mut self, _rect: Rect, _host: *mut (dyn WidgetHost + 'static)) {}

    /// Per-child visibility policy for the adapter's subtree plumbing (Switcher exposes only
    /// the active child). Default: every child.
    fn child_visible(&self, _child: *mut (dyn WidgetHost + 'static)) -> bool {
        true
    }

    /// Legacy `WidgetHost::z_index` (host render ordering; MenuBar's dropdowns layer at 100+).
    fn z_order(&self) -> i32 {
        0
    }

    /// Republish value-embedded legacy children into the ctx registry (Paginator's ButtonStrip
    /// + Pages). Legacy value-owning containers re-registered their children on EVERY `tick` and
    /// `layout` because the children's addresses move with the owning struct (host struct moves,
    /// `Vec` reallocation) — and the registration is load-bearing: the spatial grid is rebuilt
    /// from registered widgets, and it is the registered ButtonStrip (whose
    /// `blocks_backplate_drag` is true) that makes the sidebar block backplate drags. The
    /// adapter calls this from `WidgetHost::tick` and `WidgetHost::layout`, mirroring the legacy
    /// cadence. `host_id` is the adapter's id, for `link_ids`. Default: nothing embedded.
    fn register_embedded_children(&mut self, _host_id: WidgetId, _ctx: &mut UiContext) {}
}

/// The paint concern — a widget's fill color, its own (non-recursive) geometry emission, and
/// whether it clips its children. Mirrors `WidgetHost::color` / `paint_self` / `clips_children`, but
/// [`paint`](Paint::paint) receives the laid-out `rect` as a parameter (the RFC shape) rather than
/// reading a stored rect, so a narrow widget carries no base of its own.
pub trait Paint {
    /// This widget's fill color (RGBA).
    fn color(&self) -> [f32; 4];

    /// Emit this node's OWN primitives (non-recursive) into `ctx`, given its final `rect`. The
    /// default paints a plain background from [`color`](Paint::color) — the common leaf case.
    /// Recursion into children and clipping are the paint walk's job ([`crate::scene::painter`]),
    /// not this method's.
    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let color = self.color();
        if color[3].abs() > 0.001 {
            ctx.quad(rect, color);
        }
    }

    /// Whether the paint walk clips this widget's children to its `rect` (scroll/backplate
    /// containers). Default: no.
    fn clips_children(&self) -> bool {
        false
    }

    /// Corner rounding `(radius, per-corner flags)` of the widget's background, given its
    /// laid-out rect (MenuBar's corners depend on where it sits against its parent's edges).
    /// **Transitional:** this exists only for legacy render paths that draw widget backgrounds
    /// themselves from style properties (`widget_vertices` / `push_widget_vertices` readers of
    /// `WidgetHost::corner_radius` + `rounded_corners`) — the widget's real geometry is whatever
    /// [`paint`](Paint::paint) emits. Dies with those paths. Default: sharp corners.
    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        None
    }

    /// This widget's OWN popover (dropdown) rect, if one is open — hosts float it above
    /// z-ordered siblings (`register_popover` + `render_popovers`). Containers combine this
    /// with their children's popovers in the adapter. Default: none.
    fn popover(&self, _rect: Rect) -> Option<(f32, f32, f32, f32)> {
        None
    }

    /// Draw this widget's own popover (legacy `WidgetHost::render_popover`).
    fn draw_popover(&self, _rect: Rect, _pc: &mut dyn crate::layout::RenderTarget) {}

    /// Solid border `(color, thickness)` of the widget's background quad. **Transitional**, like
    /// [`corner_style`](Paint::corner_style): `render_widget` gives a widget's background quad a
    /// border+inset treatment when this is `Some` — `Toggle`'s square mode depends on it.
    fn solid_border(&self) -> Option<([f32; 4], f32)> {
        None
    }

    /// Font for this widget's text on legacy text paths (`render_widget` reads
    /// `WidgetHost::widget_font`). **Transitional.**
    fn widget_font(&self) -> Option<String> {
        None
    }

    /// Font for the widget's OWN prim-derived text on the scene paint walk (Phase 6). Defaults
    /// to [`widget_font`](Paint::widget_font) — one font for everything the widget draws, which
    /// is the legacy tuple-pipeline convention. A widget whose content text deliberately
    /// differs from its control font (TextBox with a customized `font_family`/`font_size`)
    /// overrides this; the detached base label always renders in `widget_font`. Only the paint
    /// walk consults it — the legacy `text_labels_with_font_and_bounds` getters keep serving
    /// `widget_font` so unmigrated apps stay byte-identical.
    fn text_font(&self) -> Option<String> {
        self.widget_font()
    }

    /// Receive the control label set on the wrapper via [`Adapted::with_label`] (and legacy
    /// `Control::set_label` paths). Widgets that paint their label themselves (inline-label
    /// widgets) store it here; the default discards it, leaving label drawing to the adapter's
    /// base-label machinery.
    fn sync_label(&mut self, _label: &str) {}

    // --- Legacy dual-geometry escape hatch (transitional; Graph is the only user). Legacy
    // hosts read DIFFERENT getters: the designer's raw render path draws `extra_quads` as
    // PLAIN quads, while `render_widget` and the scene walk consume the rounded view
    // (`all_rounded_quads` / `paint_self`). Legacy Graph served both by overriding all three
    // getters. A migrated widget emits the rounded view from `paint`; when it also serves a
    // plain view, the adapter returns it verbatim from `extra_quads` and empties `all_quads`
    // (mirroring legacy Graph's highlight-only override) so render_widget-style hosts that
    // read BOTH getters never draw the geometry twice. Dies with `WidgetHost`.

    /// Whether this widget serves [`legacy_plain_quads`](Paint::legacy_plain_quads).
    fn serves_legacy_plain_quads(&self) -> bool {
        false
    }

    /// The plain-quad view of this widget's geometry for legacy `extra_quads` readers.
    fn legacy_plain_quads(&self, _rect: Rect) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        Vec::new()
    }

    /// Clip rect `[x1, y1, x2, y2]` for this widget's text on the legacy bounded-text paths
    /// (`text_labels_with_bounds` / `text_labels_with_font_and_bounds`). `None` (default) keeps
    /// the legacy behavior: unbounded, except inside a scroll ancestor. Graph clips its node
    /// names to its own rect.
    fn text_bounds(&self, _rect: Rect) -> Option<[f32; 4]> {
        None
    }

    /// Per-frame text shaping against the app's `FontSystem` (legacy `WidgetHost::prepare_text`
    /// overrides). TextBox measures its glyph advances here — load-bearing for cursor↔pixel
    /// mapping, not just a render cache. Receives the laid-out content rect. Default: nothing
    /// to shape.
    fn prepare_text(&mut self, _fs: &mut cosmic_text::FontSystem, _rect: Rect) {}

    /// Whether [`paint`](Paint::paint) emits the widget's ENTIRE subtree, so the paint walk
    /// must not also descend into its (ctx-linked) children — the legacy
    /// `WidgetHost::renders_own_subtree` contract. TreeList: its field widgets stay ctx-linked
    /// for event propagation, but their pixels come from `paint`'s own child pass (which
    /// gates the add-key popover box on the popover actually being open).
    fn paints_own_subtree(&self) -> bool {
        false
    }


    /// Whether the adapter re-enables the legacy shared focus/hover highlight overlay
    /// (`WidgetHost::highlight_quad`'s default) for this widget. The adapter suppresses it for
    /// migrated widgets — matching the `None` overrides most legacy controls carried — but
    /// legacy TextBox kept the default: the focused editor gets the primary-highlight tint
    /// over its background (data-editor's teal editing wash). Default: suppressed.
    fn legacy_focus_highlight(&self) -> bool {
        false
    }

    /// Legacy container `extra_quads` aggregation: when `true`, the adapter's `extra_quads`
    /// serves the visible children's `extra_quads` — and ONLY those, like the legacy container
    /// overrides (Paginator returned its strip's + selected page's chrome; its own background
    /// quad lived in `all_quads` alone). Hosts that render a container through the plain
    /// `extra_quads` getter (cce-email's and cce-layout-interface's sidebar draw) read exactly
    /// this view. The widget's own [`paint`](Paint::paint) prims still reach `all_quads` and
    /// the scene walk. Default: off (a leaf's `extra_quads` is its own prims).
    fn aggregates_child_extra_quads(&self) -> bool {
        false
    }

    /// Forward the legacy `WidgetHost::highlight_quad` to somewhere else entirely — Paginator
    /// served its ButtonStrip's highlight (the hovered-tab tint cce-layout-interface draws by
    /// calling `highlight_quad` directly). Outer `Some` replaces the adapter's highlight logic
    /// with the inner value; `None` (default) keeps the standard behavior
    /// ([`legacy_focus_highlight`](Paint::legacy_focus_highlight)). A forwarded highlight is
    /// served ONLY through the direct `highlight_quad` getter — the adapter keeps it out of
    /// `all_quads`/`paint_self`, where the child's own aggregation already carries it (legacy
    /// containers likewise excluded it from their `all_quads` overrides).
    fn forwarded_highlight(&self, _ctx: &UiContext) -> Option<Option<(f32, f32, f32, f32, [f32; 4])>> {
        None
    }

    /// Whether this widget serves
    /// [`legacy_labels_with_font_and_bounds`](Paint::legacy_labels_with_font_and_bounds) —
    /// the text sibling of the dual-geometry escape hatch. The adapter's standard text bridge
    /// gives every own label ONE font ([`widget_font`](Paint::widget_font)) and ONE clip rect
    /// ([`text_bounds`](Paint::text_bounds)); a widget whose legacy
    /// `text_labels_with_font_and_bounds` override assigns them PER LABEL (ParametersBg clips
    /// each label to its viewport but its code editor's to the code box, in monospace) serves
    /// that view verbatim instead. Transitional — dies when `Prim::Text` carries font+bounds.
    fn serves_legacy_labels(&self) -> bool {
        false
    }

    /// The per-label font+bounds text view for legacy `text_labels_with_font_and_bounds`
    /// readers. Served as a FULL replacement: the adapter adds no child aggregation on top, so
    /// a container's implementation must include its children (as the legacy overrides did —
    /// hence the ctx, which the child recursion needs).
    fn legacy_labels_with_font_and_bounds(&self, _rect: Rect, _ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        Vec::new()
    }

}

/// What an event handler may reach beyond its own state — the RFC §3.5 `EventCtx`, grown as
/// migrated widgets need capabilities: the laid-out content rect, the widget's id (scroll-gesture
/// gating keys on it), focus acquisition, and — transitionally — the raw [`UiContext`] for the
/// legacy shared state some widgets consult (`scroll_gesture_new`, …). `ui` is `None` when the
/// event was synthesized outside a routed path (the `FocusIn`/`FocusOut` from direct
/// `focus()`/`unfocus()` calls).
pub struct EventCtx<'a> {
    /// The widget's content rect (detached-label region excluded).
    pub rect: Rect,
    /// This widget's tree id.
    pub id: WidgetId,
    /// The routing context, when routed. **Transitional** — narrow widgets should only touch the
    /// legacy shared fields (scroll gesture state) until those get typed helpers here.
    pub ui: Option<&'a mut UiContext>,
    self_ptr: Option<*mut (dyn WidgetHost + 'static)>,
}

impl EventCtx<'_> {
    /// Make this widget the global focus target (legacy `focus::set_focused(self)`).
    pub fn request_focus(&mut self) {
        if let Some(ptr) = self.self_ptr {
            unsafe { crate::widget::focus::set_focused(&mut *ptr, self.ui.as_deref_mut()) };
        }
    }

    /// Drop this widget's claim on the global focus if it holds it (legacy
    /// `focus::clear_if_matches(self)` — MenuBar releases focus when its dropdowns close).
    pub fn release_focus(&mut self) {
        if let Some(ptr) = self.self_ptr {
            unsafe { crate::widget::focus::clear_if_matches(&mut *ptr) };
        }
    }

    /// Open the shared context menu on this widget (legacy `ctx.handle_right_click(self, …)`),
    /// for widgets that must do work *before* the menu opens — Breadcrumb records which segment
    /// was right-clicked first, so the menu header can show that segment's path.
    /// [`Input::opens_context_menu`] can't express that: the adapter's gate runs instead of
    /// `on_event`, not after it. No-op outside a routed path (no ctx or no self pointer).
    pub fn open_context_menu(&mut self, px: f32, py: f32) {
        if let (Some(ptr), Some(ui)) = (self.self_ptr, self.ui.as_deref_mut()) {
            ui.handle_right_click(ptr, px, py);
        }
    }

    /// The adapter's pointer, for legacy sites that must hand it onward — TreeList makes
    /// itself the focus target (`set_focused_ptr`) and the context-menu target
    /// (`show_context_menu`) with the pointer hosts registered. Transitional; dies with
    /// `WidgetHost`. None outside a routed path.
    pub(crate) fn host_ptr(&self) -> Option<*mut (dyn WidgetHost + 'static)> {
        self.self_ptr
    }
}

/// The input concern — hit-testing and event handling against the laid-out rect. Mirrors the
/// legacy `WidgetHost::hit_test` / `handle_event` pair, but with the RFC's centralizations: the
/// default hit is plain rect containment (no per-widget address hacks), and pointer-positioned
/// events are hit-gated by the adapter *before* they reach [`on_event`](Input::on_event), so a
/// narrow widget never re-implements the "am I actually under the cursor?" boilerplate that every
/// legacy `mouse_input` override carries.
pub trait Input {
    /// Whether the point `(x, y)` hits this widget, given its laid-out `rect`. Override for
    /// non-rectangular hit shapes. Default: containment (edges inclusive, matching the legacy
    /// `hit_test`).
    fn hit(&self, rect: Rect, x: f32, y: f32) -> bool {
        x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height
    }

    /// React to `event`. Return `true` to consume it (the router marks the widget dirty and
    /// stops propagation). `MouseButton` *presses* and `MouseWheel` arrive only when
    /// [`hit`](Input::hit) passed; *releases* arrive ungated (press-tracking widgets commit or
    /// cancel from anywhere); `MouseEnter` / `MouseLeave` are synthesized by the hover machinery.
    /// Default: ignore everything.
    fn on_event(&mut self, _event: &Event, _ectx: &mut EventCtx) -> bool {
        false
    }

    /// Whether pressing on this widget blocks dragging the movable backplate under it. Passive
    /// display widgets (separators, status dots) return `false` so drags pass through them.
    /// Default: `true`, matching the legacy `WidgetHost` default.
    fn blocks_backplate_drag(&self) -> bool {
        true
    }

    /// Whether a right-click on this widget opens the shared config context menu (the adapter
    /// then routes it to `UiContext::handle_right_click`, which `on_event` can't reach — it has
    /// no ctx by design). Default: no.
    fn opens_context_menu(&self) -> bool {
        false
    }

    /// Container hit policy: hit whenever any [`Layout::child_visible`] child hits (Layer,
    /// Switcher). The container's own rect is not consulted. Default: own-rect hit.
    fn hits_through_children(&self) -> bool {
        false
    }

    /// Whether the adapter hit-gates `MouseButton` presses before `on_event` (the leaf
    /// centralization). Event-proxying containers return `false`: legacy container
    /// `mouse_input` overrides saw every press — Switcher unfocuses its active child when a
    /// press lands outside it, which a gated `on_event` would never learn about.
    fn gates_presses(&self) -> bool {
        true
    }

    // --- The legacy polling/value-binding surface (`take_click`, `take_change`,
    // `get_value_string`/`set_value_string`, `value`) apps read widget state through. Kept on
    // `Input` to avoid a fourth trait bound; replaced by typed messages when RFC §3.5's EventCtx
    // lands. All default to the inert legacy defaults.

    /// Consume the "was clicked since last asked" flag.
    fn take_click(&mut self) -> bool {
        false
    }

    /// Consume the "value changed since last asked" flag.
    fn take_change(&mut self) -> bool {
        false
    }

    /// The widget's value serialized for the config system.
    fn value_string(&self) -> Option<String> {
        None
    }

    /// Set the widget's value from a config string. Returns whether it parsed and changed.
    fn set_value_string(&mut self, _val: &str) -> bool {
        false
    }

    /// The widget's value as an integer (legacy `WidgetHost::value`).
    fn value(&self) -> i32 {
        0
    }

    // --- Clipboard/selection surface (the context menu's Cut/Copy/Paste/Select-All actions
    // call these on their target WidgetHost). The defaults replicate the `WidgetHost` defaults
    // byte-for-byte (whole-value copy through the value-string pair), so widgets migrated
    // before these hooks existed keep their exact behavior; TextBox overrides with real
    // selection-aware implementations.

    fn context_action(&mut self, action: crate::widget::ContextAction) -> bool {
        match action {
            crate::widget::ContextAction::Cut => {
                if let Some(val) = self.value_string() {
                    crate::widget::clipboard::copy_to_clipboard(&val);
                    self.set_value_string("")
                } else {
                    false
                }
            }
            crate::widget::ContextAction::Copy => {
                if let Some(val) = self.value_string() {
                    crate::widget::clipboard::copy_to_clipboard(&val);
                    true
                } else {
                    false
                }
            }
            crate::widget::ContextAction::Paste => {
                if let Some(text) = crate::widget::clipboard::read_from_clipboard() {
                    self.set_value_string(&text)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Whether direct `focus()`/`unfocus()` calls flip the base `focused` flag. Legacy widgets
    /// differ: most set it in their `focus` overrides, but TextBox never did — its detached
    /// label must not color as focused. Default: flip it (what every widget migrated so far
    /// has shipped with).
    fn tracks_base_focus(&self) -> bool {
        true
    }

    /// Selection state pushed in by list/row hosts (legacy `WidgetHost::set_selected`).
    fn set_selected(&mut self, _selected: bool) {}

    // --- Drag surface: legacy hosts (designer, control_panel, parameters_bg, graph, audio…)
    // drive drags by calling these directly on the widget, not through events.

    /// Whether a press on this widget starts a host-driven drag. Receives the laid-out rect:
    /// scroll widgets (Spreadsheet) are draggable only while their content overflows it.
    fn draggable(&self, _rect: Rect) -> bool {
        false
    }
    fn is_dragging(&self) -> bool {
        false
    }
    fn drag_begin(&mut self, _px: f32, _py: f32, _rect: Rect) {}
    /// Returns whether the drag changed the widget's value (drives redraw).
    fn drag_update(&mut self, _px: f32, _py: f32, _rect: Rect) -> bool {
        false
    }
    /// For self-moving widgets (Panel, Splitter): the new origin this drag step wants, or `None`
    /// if unmoved. The adapter applies it to the base rect (the model cannot reach it).
    fn drag_reposition(&mut self, _px: f32, _py: f32, _rect: Rect) -> Option<(f32, f32)> {
        None
    }
    fn drag_end(&mut self) {}
    /// Movement bounds pushed in by hosts (reached via the inherent `Adapted::set_drag_bounds`).
    fn set_drag_bounds(&mut self, _bx: f32, _by: f32, _bw: f32, _bh: f32) {}

    // --- Tick surface: hosts broadcast `WidgetHost::tick(dt)` every frame (the designer's render
    // loop) to advance time-based widget state — inertial scroll velocity, here. Transitional:
    // §3.6 `Animated<T>` + arena-driven frame requests replace hand-ticked state.

    /// Advance time-based state by `dt` seconds against the laid-out rect. Return whether
    /// anything observable changed (drives redraw).
    fn tick(&mut self, _dt: f32, _rect: Rect) -> bool {
        false
    }

    /// Context-carrying tick for legacy stateful containers whose per-frame work needs the
    /// routing context — TreeList commits its inline rename editor, re-targets focus, and
    /// drains its search box on tick. Runs right after [`tick`](Input::tick) with a routed
    /// [`EventCtx`] (ui + the adapter's id/pointer). Transitional, like the capability hooks.
    fn tick_ctx(&mut self, _dt: f32, _ectx: &mut EventCtx) -> bool {
        false
    }

    /// Whether this widget wants `tick` calls from tick-gating hosts (legacy
    /// `WidgetHost::wants_tick`; the designer ticks unconditionally and ignores this).
    fn wants_tick(&self) -> bool {
        false
    }

    /// Whether this widget consumes scroll gestures (legacy `WidgetHost::is_scrollable`, read by
    /// the router's scroll-gesture gating).
    fn scrollable(&self) -> bool {
        false
    }

    // --- Controller capabilities (transitional, like the polling surface above). The legacy
    // tree reaches a widget's typed API through the `WidgetHost::as_*_controller` downcast pairs;
    // `WidgetHost` is implemented exactly once (for `Adapted<W>`), so a migrated controller widget
    // re-exposes its controller impl through these hooks instead — `Some(self)` when `W`
    // implements the trait. Dies with `WidgetHost`: the end state reaches a controller through the
    // concrete `Adapted<W>` (or a `&dyn XController` held directly), per RFC §3.5.


    /// Keyboard modifier state pushed in by hosts before dispatch (legacy
    /// `WidgetHost::set_modifiers`).
    fn set_modifiers(&mut self, _ctrl: bool, _shift: bool, _alt: bool) {}

    /// The widget's visibility flag changed through `WidgetHost::set_visible` (the adapter owns
    /// the flag) — legacy hideable widgets used the setter for side effects (MenuBar closes
    /// its dropdowns and invalidates layout).
    fn visibility_changed(&mut self, _visible: bool) {}

    /// The widget's `WidgetHost::focused` answer, given the base flag — MenuBar reports focused
    /// while any of its dropdowns is open, beyond the flag itself. Default: the flag.
    fn is_focused(&self, base_focused: bool) -> bool {
        base_focused
    }

}

/// Wraps a narrow-trait widget `W` so it lives in the legacy `*mut dyn WidgetHost` tree. Carries the
/// [`Widget`] base that `WidgetHost`'s rect / id / dirty machinery needs, and forwards the concern
/// methods to `W`. See the module docs for why this bridge exists rather than a supertrait split.
///
/// The bounds live on the struct (not just the `WidgetHost` impl) so `Drop` can clear the global
/// focus / context-menu references through `&dyn WidgetHost` — the same guard legacy widgets with
/// `Drop` impls (e.g. the old `Checkbox`) carried.
#[derive(Debug, Clone)]
pub struct Adapted<W: Layout + Paint + Input + 'static> {
    base: Widget,
    /// The [`Widget`] base carries no visibility, and the legacy `WidgetHost` defaults are a no-op
    /// `set_visible` + always-true `visible()` — every hideable legacy widget stores its own
    /// flag. The adapter owns it once for all migrated widgets: hosts toggle panes through
    /// `WidgetHost::set_visible` (the designer), and the hit-test/render bridges gate on it.
    visible: bool,
    inner: W,
}

impl<W: Layout + Paint + Input + 'static> Drop for Adapted<W> {
    fn drop(&mut self) {
        crate::widget::clear_widget_references(self);
    }
}

/// Plain-data widgets constructed via `Default` (PreviewState in cce-files) keep their
/// construction sites when the wrapper lands.
impl<W: Layout + Paint + Input + Default + 'static> Default for Adapted<W> {
    fn default() -> Self {
        Adapted::new(W::default())
    }
}

impl<W: Layout + Paint + Input + 'static> Adapted<W> {
    /// Wrap `inner` with a fresh [`Widget`] base.
    pub fn new(inner: W) -> Self {
        Adapted { base: Widget::new(), visible: true, inner }
    }

    /// The wrapped widget.
    pub fn inner(&self) -> &W {
        &self.inner
    }

    /// The wrapped widget, mutably.
    pub fn inner_mut(&mut self) -> &mut W {
        &mut self.inner
    }

    /// This widget's tree id (assigned lazily), for registering it in a [`UiContext`].
    pub fn id(&self) -> WidgetId {
        self.base.id()
    }

    /// Attach a control label. Mirrors the `with_label` builders legacy control widgets carry,
    /// so construction sites keep their shape when a widget migrates. The label is stored on the
    /// base (legacy machinery: label offsets, context-menu titles) *and* pushed into the widget
    /// via [`Paint::sync_label`] for widgets that paint it themselves.
    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self.inner.sync_label(label);
        self
    }

    /// Bind this widget to a config file/key (right-click context-menu editing). Mirrors the
    /// legacy `with_config` builders.
    pub fn with_config(mut self, file: &str, key: &str) -> Self {
        self.base.config_file = Some(file.to_string());
        self.base.config_key = Some(key.to_string());
        self
    }

    /// Update the control label, keeping the base copy (legacy machinery) and the widget's own
    /// copy ([`Paint::sync_label`]) in step. Inherent so it shadows `Control::set_label` — which
    /// writes only the base and would leave a self-painting label stale — at every call site,
    /// regardless of which traits are in scope.
    pub fn set_label(&mut self, label: &str) {
        self.base.label = Some(label.to_string());
        self.inner.sync_label(label);
    }

    // --- The value/polling drains (off `WidgetHost` in the 6bd value shrink): apps read
    // widget state through these concrete methods; each forwards to the narrow `Input`
    // hook. The last dyn readers went concrete-slot instead (TI roster, cloud JsonControl,
    // designer pane-focus sync).

    /// Drain the one-shot click flag (Button-class widgets).
    pub fn take_click(&mut self) -> bool {
        Input::take_click(&mut self.inner)
    }

    /// Drain the one-shot value-changed flag.
    pub fn take_change(&mut self) -> bool {
        Input::take_change(&mut self.inner)
    }

    /// The widget's value serialized to a string (config writes, context-menu Copy).
    pub fn get_value_string(&self) -> Option<String> {
        Input::value_string(&self.inner)
    }

    /// Parse and apply a value string; returns whether the value changed.
    pub fn set_value_string(&mut self, val: &str) -> bool {
        Input::set_value_string(&mut self.inner, val)
    }

    /// The widget's value as an integer.
    pub fn value(&self) -> i32 {
        Input::value(&self.inner)
    }

    /// Selection state pushed in by list/row hosts.
    pub fn set_selected(&mut self, selected: bool) {
        Input::set_selected(&mut self.inner, selected)
    }

    /// Text-content mutation: keep the base copy and the widget's own copy
    /// ([`Paint::sync_label`]) in step, like `set_label`.
    pub fn set_text(&mut self, text: &str) {
        self.base.label = Some(text.to_string());
        Paint::sync_label(&mut self.inner, text);
    }

    /// The rect the wrapped widget paints into: the widget's rect minus the detached-label
    /// region at the top (zero inset when there is no label, or when the widget draws its label
    /// inline — `Widget::label_offset` / [`Layout::inline_label`]).
    fn content_rect(&self) -> Rect {
        let top = if Layout::inline_label(&self.inner) { 0.0 } else { self.base.label_offset() };
        Rect {
            x: self.base.x,
            y: self.base.y + top,
            width: self.base.w,
            // Deliberately NOT clamped at zero: legacy geometry computed `h - label_offset`
            // raw, and hosts under-size labeled sliders (label taller than the assigned rect);
            // the resulting negative-height quads still rasterize (flipped), which is what
            // keeps those tracks visible. Clamping made them vanish — found the hard way.
            height: self.base.h - top,
        }
    }
}

impl<W: Layout + Paint + Input + 'static> Adapted<W> {
    /// This widget as a type-erased host pointer (off the `WidgetHost` trait — the
    /// plumbing retype). Registration-bridge material: derived from a live borrow at the
    /// call, stored only in the `WidgetTree` registry.
    pub fn as_ptr(&self) -> *mut (dyn WidgetHost + 'static) {
        self as *const Self as *mut Self as *mut (dyn WidgetHost + 'static)
    }

    pub fn as_ptr_mut(&mut self) -> *mut (dyn WidgetHost + 'static) {
        self as *mut Self as *mut (dyn WidgetHost + 'static)
    }

    /// Whether a press here may start a drag (off `WidgetHost` — the ControlPanel
    /// endgame; forwards to the narrow `Input` hook with the laid-out content rect).
    pub fn draggable(&self) -> bool {
        Input::draggable(&self.inner, self.content_rect())
    }

    /// Whether the widget's own drag is live (off `WidgetHost` with `draggable`).
    pub fn is_dragging(&self) -> bool {
        Input::is_dragging(&self.inner)
    }

    /// Movement bounds pushed in by hosts (off the `WidgetHost` trait since 6bd — the one
    /// production caller is concrete: designer's network panel).
    pub fn set_drag_bounds(&mut self, bx: f32, by: f32, bw: f32, bh: f32) {
        Input::set_drag_bounds(&mut self.inner, bx, by, bw, bh)
    }

    /// Unlink all tree children (off `WidgetHost` in 6bd batch 2 — every caller is a concrete
    /// `Adapted` field).
    pub fn clear_children(&mut self, ctx: &mut UiContext) {
        ctx.clear_children_ids(self.base.id());
    }

    // --- The direct-dispatch entry points, inherent since the 6bd collapse. In-crate
    // composites forward to their CONCRETE embedded children through these; dyn callers
    // and the router go through `handle_event`, which these forward to (the two paths
    // are identical by construction — including Drag*, which handle_event maps onto the
    // Input drag hooks).

    pub fn mouse_input(&mut self, button: crate::widget::MouseButton, state: crate::widget::ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        self.handle_event(
            &Event::MouseButton { button, state, x: px, y: py, local_x: px, local_y: py },
            ctx,
        )
    }
    pub fn mouse_wheel(&mut self, delta: &crate::widget::MouseScrollDelta, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        self.handle_event(
            &Event::MouseWheel { delta: *delta, x: px, y: py, local_x: px, local_y: py },
            ctx,
        )
    }

    /// [`Self::mouse_wheel`] WITHOUT the adapter's rect hit-gate: straight to the
    /// widget's `Input::on_event`. For hosts that already zone-gated the wheel
    /// themselves against a capture region LARGER than the widget rect — the
    /// band slider's shape-conforming halo extends past the row rect, and the
    /// rect gate would clip exactly the fringe the halo exists to catch
    /// (`ParametersBg`'s slider forwarding). The widget's own on_event still
    /// applies its fine-grained zone test.
    pub fn mouse_wheel_ungated(
        &mut self,
        delta: &crate::widget::MouseScrollDelta,
        px: f32,
        py: f32,
        ctx: &mut UiContext,
    ) -> bool {
        let rect = self.content_rect();
        let id = self.base.id();
        let self_ptr = self.as_ptr_mut();
        let mut ectx = EventCtx { rect, id, ui: Some(ctx), self_ptr: Some(self_ptr) };
        Input::on_event(
            &mut self.inner,
            &Event::MouseWheel { delta: *delta, x: px, y: py, local_x: px, local_y: py },
            &mut ectx,
        )
    }
    pub fn keyboard_input(&mut self, event: &crate::widget::KeyEvent, ctx: &mut UiContext) -> bool {
        self.handle_event(&Event::KeyInput(event.clone()), ctx)
    }
    pub fn drag_begin(&mut self, px: f32, py: f32) {
        let rect = self.content_rect();
        Input::drag_begin(&mut self.inner, px, py, rect)
    }
    pub fn drag_update(&mut self, px: f32, py: f32) -> bool {
        let rect = self.content_rect();
        if let Some((nx, ny)) = Input::drag_reposition(&mut self.inner, px, py, rect) {
            self.base.x = nx;
            self.base.y = ny;
            return true;
        }
        Input::drag_update(&mut self.inner, px, py, rect)
    }
    pub fn drag_end(&mut self) {
        Input::drag_end(&mut self.inner)
    }

    /// The deleted trait default: coverage-gated hover dispatch (an open popover covering
    /// the point clears the hover instead of recomputing it).
    pub fn cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        ctx.set_cursor_pos(px, py);
        if ctx.is_coordinate_covered(self.base.id(), px, py) {
            let was = self.base.hovered;
            if was {
                self.base.hovered = false;
                self.handle_event(&Event::MouseLeave, ctx);
            }
            return was;
        }
        self.on_cursor_moved(px, py, ctx)
    }

    /// Ungated pointer moves (no popover-coverage check): offer the raw move to the widget,
    /// then fall back to the base hover bookkeeping, mirroring `handle_event`'s `PointerMove`
    /// arm. Not routed *through* `handle_event`, because the routed path reaches this method
    /// too (via `cursor_moved`) and would recurse; on that path `on_event` sees the same
    /// unconsumed move twice, which is fine — a hover recompute is idempotent (anything
    /// that changed on the first call consumed it there).
    pub fn on_cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        let rect = self.content_rect();
        let id = self.base.id();
        let self_ptr = self.as_ptr_mut();
        let mut ectx = EventCtx { rect, id, ui: Some(ctx), self_ptr: Some(self_ptr) };
        let event = Event::PointerMove { x: px, y: py, local_x: px, local_y: py };
        if Input::on_event(&mut self.inner, &event, &mut ectx) {
            return true;
        }
        let was = self.base.hovered;
        let is_hit = self.hit_test(px, py, ctx);
        self.base.hovered = is_hit;
        if was != is_hit {
            let transition = if is_hit { Event::MouseEnter } else { Event::MouseLeave };
            self.handle_event(&transition, ctx);
            true
        } else {
            false
        }
    }

    /// Register + link a child under this widget (off `WidgetHost` in 6bd batch 4; dyn callers
    /// went to `focus::link_parent_child`/tree ops).
    pub fn add_child(&mut self, child: *mut (dyn WidgetHost + 'static), ctx: &mut UiContext) {
        // The old WidgetHost default's tree link…
        let c_id = unsafe { (*child).base().id() };
        let p_id = self.base.id();
        let self_ptr = self.as_ptr();
        ctx.register_widget(p_id, self_ptr);
        ctx.register_widget(c_id, child);
        ctx.tree.link(p_id, c_id);
        // …plus, for containers, the legacy container extra: parent the child back (Layer,
        // Switcher) — the symmetric tree link the child's own set_parent used to make.
        if Layout::has_container_children(&self.inner) {
            let self_ptr = self.as_ptr_mut();
            ctx.register_widget(self.base.id(), self_ptr);
            ctx.register_widget(c_id, child);
            ctx.tree.set_parent(c_id, Some(self.base.id()));
        }
    }

    /// Register + (un)link this widget under a parent (off `WidgetHost` in 6bd batch 4).
    pub fn set_parent(&mut self, parent: Option<*mut (dyn WidgetHost + 'static)>, ctx: &mut UiContext) {
        // Replica of the old WidgetHost default: symmetric tree link.
        let id = self.base.id();
        if let Some(p_ptr) = parent {
            let p_id = unsafe { (*p_ptr).base().id() };
            ctx.register_widget(p_id, p_ptr);
            let self_ptr = self.as_ptr();
            ctx.register_widget(id, self_ptr);
            ctx.tree.set_parent(id, Some(p_id));
        } else {
            ctx.tree.set_parent(id, None);
        }
    }

    /// The model's intrinsic content size (off the `WidgetHost` trait since 6bd — the concrete
    /// callers are fonts'/graph's hand-laid button/dropdown sizing).
    pub fn intrinsic_size(&self) -> Option<Size> {
        Layout::intrinsic_size(&self.inner)
    }

    /// Run the wrapped widget's [`Paint::paint`] against its content rect and return the emitted
    /// prims — the shared source for the reverse bridges (`extra_quads`, `all_rounded_quads`,
    /// `extra_circles`, `extra_arcs`, prim-derived `text_labels`) that legacy render loops read.
    fn painted_prims(&self) -> Vec<Prim> {
        let mut pc = PaintCtx::new();
        Paint::paint(&self.inner, self.content_rect(), &mut pc);
        pc.finish().items.into_iter().map(|item| item.prim).collect()
    }

    /// This widget's OWN plain-quad prims from [`Paint::paint`] — the shared source for the
    /// `extra_quads`/`all_quads` reverse bridges (kept separate from `extra_quads` itself,
    /// which may serve the child aggregation instead —
    /// [`Paint::aggregates_child_extra_quads`]).
    fn own_plain_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        self.painted_prims()
            .into_iter()
            .filter_map(|prim| match prim {
                Prim::Quad { rect, color } => Some((rect.x, rect.y, rect.width, rect.height, color)),
                _ => None,
            })
            .collect()
    }

    /// The container's children that pass the [`Layout::child_visible`] policy — the set the
    /// adapter's subtree plumbing (aggregation, recursion, hit-through) operates on. Empty for
    /// non-containers.
    fn visible_children(&self) -> Vec<*mut (dyn WidgetHost + 'static)> {
        if !Layout::has_container_children(&self.inner) {
            return Vec::new();
        }
        Layout::container_children(&self.inner)
            .into_iter()
            .filter(|c| Layout::child_visible(&self.inner, *c))
            .collect()
    }

    /// This widget's OWN text (prim-derived + detached base label), before any child
    /// aggregation — the shared source for the three text getters.
    pub(crate) fn own_text_labels(&self) -> Vec<TextLabel> {
        if !self.visible() {
            return Vec::new();
        }
        let mut out: Vec<TextLabel> = self
            .painted_prims()
            .into_iter()
            .filter_map(|prim| match prim {
                Prim::Text { text, x, y, font_size, color, .. } => {
                    Some(TextLabel { text, x, y, font_size, color })
                }
                _ => None,
            })
            .collect();
        if !Layout::inline_label(&self.inner) {
            out.extend(self.base_label_fallback());
        }
        out
    }

    /// The paint-walk view of `own_labels_with_font_and_bounds`: prim-derived text carries the
    /// widget's content font ([`Paint::text_font`]); the detached base label keeps
    /// `widget_font` either way (via `own_labels_with_prim_font`).
    fn own_labels_for_walk(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        self.own_labels_with_prim_font(ctx, Paint::text_font(&self.inner))
    }

    fn own_labels_with_prim_font(&self, _ctx: &UiContext, prim_font: Option<String>) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        let base_font = Paint::widget_font(&self.inner);
        let mut fonted: Vec<(TextLabel, Option<String>)> = Vec::new();
        if self.visible() {
            fonted.extend(
                self.painted_prims()
                    .into_iter()
                    .filter_map(|prim| match prim {
                        Prim::Text { text, x, y, font_size, color, .. } => {
                            Some((TextLabel { text, x, y, font_size, color }, prim_font.clone()))
                        }
                        _ => None,
                    }),
            );
            if !Layout::inline_label(&self.inner) {
                fonted.extend(self.base_label_fallback().into_iter().map(|l| (l, base_font.clone())));
            }
        }

        if let Some(bounds) = Paint::text_bounds(&self.inner, self.content_rect()) {
            return fonted
                .into_iter()
                .map(|(l, font)| (l, font, Some(bounds)))
                .collect();
        }

        // The legacy scroll-ancestor clamp ended here: always a no-op since Phase 6av —
        // ScrollBox (the last scroll ancestor type) never appeared as a tree parent.
        fonted
            .into_iter()
            .map(|(l, font)| (l, font, None::<[f32; 4]>))
            .collect::<Vec<_>>()
    }

    /// The base-label text of a *detached*-label widget — a replica of the legacy default
    /// `WidgetHost::text_labels` body (which an overriding impl can no longer call).
    fn base_label_fallback(&self) -> Vec<TextLabel> {
        let b = &self.base;
        if let Some(ref label) = b.label {
            let (_, font_size) = crate::layout::control_label_font_detached_parsed();
            let color = crate::colors::control_label_color_detached_for_state(b.hovered, b.focused);
            if crate::layout::control_label_layout() == "side" {
                let label_x = WidgetHost::label_x_offset(self);
                if label_x > 0.0 {
                    let y_pos = crate::layout::align_text_y(b.y, b.h, font_size, 0.0);
                    return vec![TextLabel { text: label.clone(), x: b.x + 4.0, y: y_pos, font_size, color }];
                }
            }
            let inset = Layout::detached_label_inset(&self.inner);
            return vec![TextLabel { text: label.clone(), x: b.x + inset, y: b.y, font_size, color }];
        }
        Vec::new()
    }
}

/// Auto-deref to the wrapped widget, so call sites keep using a migrated widget's own state and
/// methods directly (`dot.status`, `dot.set_status(..)`) without knowing about the wrapper.
/// (By-value builders can't flow through `Deref` — those get mirrored per-widget, like
/// `with_label` here or `UsageBar::with_colors`.)
impl<W: Layout + Paint + Input + 'static> std::ops::Deref for Adapted<W> {
    type Target = W;
    fn deref(&self) -> &W {
        &self.inner
    }
}

impl<W: Layout + Paint + Input + 'static> std::ops::DerefMut for Adapted<W> {
    fn deref_mut(&mut self) -> &mut W {
        &mut self.inner
    }
}

impl<W: Layout + Paint + Input + 'static> WidgetHost for Adapted<W> {
    fn base(&self) -> &Widget {
        &self.base
    }
    fn base_mut(&mut self) -> &mut Widget {
        &mut self.base
    }
    // `as_any` exposes the *inner* widget: legacy code downcasts by concrete widget type
    // (`json_layout`'s `downcast_mut::<Checkbox>()`), and the adapter must be transparent to it.
    fn as_any(&self) -> &dyn std::any::Any {
        &self.inner
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        &mut self.inner
    }
    fn set_visible(&mut self, visible: bool) {
        if self.visible != visible {
            self.visible = visible;
            Input::visibility_changed(&mut self.inner, visible);
        }
    }
    fn visible(&self) -> bool {
        self.visible
    }

    // --- Container concern: tree lifecycle, child layout, and subtree recursion. The tree
    // itself stays in `ctx.tree` (the WidgetHost defaults' store); a container model additionally
    // keeps its own pointer Vec via the `Layout` hooks, because `set_rect`-time arrangement
    // has no ctx to reach the tree.

    fn is_child_visible(&self, child_id: WidgetId) -> bool {
        if !Layout::has_container_children(&self.inner) {
            return true;
        }
        for child in Layout::container_children(&self.inner) {
            if unsafe { (*child).base().id() } == child_id {
                return Layout::child_visible(&self.inner, child);
            }
        }
        false
    }

    fn z_index(&self) -> i32 {
        Layout::z_order(&self.inner)
    }

    fn set_modifiers(&mut self, ctrl: bool, shift: bool, alt: bool) {
        Input::set_modifiers(&mut self.inner, ctrl, shift, alt)
    }

    fn focused(&self, _ctx: &UiContext) -> bool {
        Input::is_focused(&self.inner, self.base.focused)
    }


    fn layout(&mut self, origin: crate::widget::Point, constraints: crate::widget::LayoutConstraints, ctx: &mut UiContext) {
        // The WidgetHost default (measure + set_rect), plus recursive child layout for visible
        // containers — the ctx-carrying half of the arrangement the model can't do in
        // `arrange_children`.
        let size = self.measure(constraints, ctx);
        self.set_rect(origin.x, origin.y, size.width, size.height);
        let host_id = self.base.id();
        Layout::register_embedded_children(&mut self.inner, host_id, ctx);
    }

    fn prepare_text(&mut self, fs: &mut cosmic_text::FontSystem) {
        if self.visible() {
            let rect = self.content_rect();
            Paint::prepare_text(&mut self.inner, fs, rect);
            for child in self.visible_children() {
                unsafe { (*child).prepare_text(fs) };
            }
        }
    }

    fn popover_rect(&self) -> Option<(f32, f32, f32, f32)> {
        if !self.visible() {
            return None;
        }
        Paint::popover(&self.inner, self.content_rect())
            .or_else(|| self.visible_children().into_iter().find_map(|c| unsafe { &*c }.popover_rect()))
    }

    fn render_popover(&self, pc: &mut dyn crate::layout::RenderTarget) {
        if !self.visible() {
            return;
        }
        Paint::draw_popover(&self.inner, self.content_rect(), pc);
        for child in self.visible_children() {
            unsafe { &*child }.render_popover(pc);
        }
    }

    // --- Legacy structural conventions the adapter owns on the widget's behalf ---

    /// The detached-label convention shared by legacy control widgets: the widget grows past the
    /// rect its parent assigns to make room for the label above (`ProgressBar`/`Slider`-style
    /// `set_rect` overrides). Inline-label widgets ([`Layout::inline_label`]) draw the label
    /// inside their rect and get no inflation. Zero-cost when no label is set.
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let r = Layout::adjust_rect(&self.inner, Rect { x, y, width: w, height: h });
        let inflation = if Layout::inline_label(&self.inner) || !Layout::inflates_label_rect(&self.inner) {
            0.0
        } else {
            self.base.label_offset()
        };
        self.base.x = r.x;
        self.base.y = r.y;
        self.base.w = r.width;
        self.base.h = r.height + inflation;
        // Ungated rect notification (TextBox re-clamps scroll on every assignment, hidden or
        // not — the legacy `set_rect` side effect).
        let landed = Rect { x: self.base.x, y: self.base.y, width: self.base.w, height: self.base.h };
        Layout::rect_assigned(&mut self.inner, landed);
        // Containers position their children from the assigned rect (legacy `set_rect`
        // overrides); hidden containers skip it, like the legacy impls.
        if self.visible {
            let content = self.content_rect();
            let host = self.as_ptr_mut();
            Layout::arrange_children(&mut self.inner, content, host);
        }
    }

    fn preferred_height(&self) -> Option<f32> {
        Layout::intrinsic_size(&self.inner).map(|s| s.height)
    }

    /// The `WidgetHost::measure` default, except the width consults the intrinsic size when the
    /// widget opts in ([`Layout::intrinsic_measure_width`] — Dropdown's `auto_width`).
    fn measure(&self, constraints: crate::widget::LayoutConstraints, _ctx: &UiContext) -> crate::widget::Size {
        let (_, _, w, h) = self.rect();
        let pref_w = if Layout::intrinsic_measure_width(&self.inner) {
            Layout::intrinsic_size(&self.inner).map_or(w, |s| s.width)
        } else {
            w
        };
        let pref_h = self.preferred_height().unwrap_or(h);
        crate::widget::Size {
            width: pref_w.clamp(constraints.min_width, constraints.max_width),
            height: pref_h.clamp(constraints.min_height, constraints.max_height),
        }
    }

    /// Narrow widgets own every pixel they draw through [`Paint::paint`]; the legacy shared
    /// hover-highlight overlay is suppressed (matching what most control widgets' `None`
    /// overrides do today) — unless the widget opts back in
    /// ([`Paint::legacy_focus_highlight`], TextBox), in which case this replicates the
    /// `WidgetHost` default byte-for-byte: primary tint when ctx-focused (or active), secondary
    /// when hovered, over the row-substituted, side-label-inset span.
    fn highlight_quad(&self, ctx: &UiContext) -> Option<(f32, f32, f32, f32, [f32; 4])> {
        // A forwarding widget (Paginator → its ButtonStrip) serves the forwarded value here —
        // and only here; `all_quads`/`paint_self` gate on `legacy_focus_highlight` instead, so
        // the forwarded quad is never double-drawn.
        if let Some(forwarded) = Paint::forwarded_highlight(&self.inner, ctx) {
            return forwarded;
        }
        if !Paint::legacy_focus_highlight(&self.inner) {
            return None;
        }
        let is_focused = ctx.is_focused_id(self.base.id());
        let hc = if is_focused {
            crate::colors::highlight_primary_color()
        } else if self.base.hovered {
            crate::colors::HIGHLIGHT_SECONDARY
        } else {
            return None;
        };
        let label_x = WidgetHost::label_x_offset(self);
        let hx = if self.base.row_w > 0.0 { self.base.row_x } else { self.base.x } + label_x;
        let hw = if self.base.row_w > 0.0 { self.base.row_w } else { self.base.w } - label_x;
        Some((hx, self.base.y, hw, self.base.h, hc))
    }

    /// Report the *inner* type's name, not `Adapted<W>`: runtime type-name matching (e.g.
    /// `layout.rs`' span-full widget list) must keep seeing the widget it knows.
    fn type_name(&self) -> &'static str {
        std::any::type_name::<W>().split("::").last().unwrap_or("Widget")
    }

    // --- Paint concern -> `Paint` ---
    fn color(&self) -> [f32; 4] {
        Paint::color(&self.inner)
    }
    fn clips_children(&self) -> bool {
        Paint::clips_children(&self.inner)
    }
    fn corner_style(&self) -> (f32, (bool, bool, bool, bool)) {
        // 12.0 / all-off mirrors the `WidgetHost` default for widgets without a corner style.
        Paint::corner_style(&self.inner, self.content_rect())
            .unwrap_or((12.0, (false, false, false, false)))
    }
    fn solid_border(&self) -> Option<([f32; 4], f32)> {
        Paint::solid_border(&self.inner)
    }
    fn widget_font(&self) -> Option<String> {
        Paint::widget_font(&self.inner)
    }
    /// Scene-path emission. Geometry comes from [`Paint::paint`]; its plain `Text` prims are
    /// REPLACED by the same font+bounds view the standard text bridges serve
    /// (`own_labels_with_font_and_bounds`, or the per-label hatch), so a display list built by
    /// the paint walk carries per-widget fonts and clip rects (Phase 6 — text ordering
    /// relative to geometry is immaterial: glyphs always render in the later text pass).
    fn renders_own_subtree(&self) -> bool {
        Paint::paints_own_subtree(&self.inner)
    }

    fn paint_self(&self, ui: &UiContext, ctx: &mut PaintCtx) {
        let mut tmp = PaintCtx::new();
        Paint::paint(&self.inner, self.content_rect(), &mut tmp);
        // Subtree painters (paints_own_subtree) author their COMPLETE text in paint() —
        // per-child fonts and clip bounds included — so their Text prims pass through
        // verbatim and the single-font own-labels re-derivation below is skipped
        // (re-deriving would flatten a composite's mixed child fonts to widget_font).
        let subtree = Paint::paints_own_subtree(&self.inner);
        for item in tmp.finish().items {
            // Re-emitting through ctx re-records clip state, so restore the
            // circular clip the widget authored the prim under (Ramp's
            // foam-cell fills) — it would otherwise be dropped here.
            let clip_circle = item.clip_circle;
            if let Some(c) = clip_circle {
                ctx.push_clip_circle(c);
            }
            // `replay` emits every prim but Text and hands Text back — the two
            // callers disagree about it. A subtree painter authored its own text
            // (per-child fonts and clips) so that passes through verbatim;
            // otherwise it is dropped in favour of the own-labels bridge below.
            if let Some(Prim::Text { text, x, y, font_size, color, font, bounds, .. }) =
                ctx.replay(item.prim)
            {
                if subtree {
                    ctx.text_with(text, x, y, font_size, color, font, bounds);
                }
            }
            if clip_circle.is_some() {
                ctx.pop_clip_circle();
            }
        }
        // The legacy default `paint_self` drained `all_quads`, which carries the focus
        // highlight — replicate for opt-in widgets, over the background (same draw order).
        // Forwarded highlights stay out: the paint walk reaches the owning child itself.
        if Paint::legacy_focus_highlight(&self.inner) {
            if let Some((hx, hy, hw, hh, hc)) = WidgetHost::highlight_quad(self, ui) {
                if hc != crate::colors::HIGHLIGHT_SECONDARY {
                    ctx.quad(Rect { x: hx, y: hy, width: hw, height: hh }, hc);
                }
            }
        }
        // Own text with per-label font+bounds: the hatch view verbatim for hatched widgets
        // (caveat: its contract includes raw container children — those few widgets keep the
        // hatch until their hosts adopt the walk), else the standard own-labels bridge (prim
        // text + the detached base label, one font, text_bounds or the scroll-ancestor clip).
        if subtree {
            return;
        }
        let labels = if Paint::serves_legacy_labels(&self.inner) {
            Paint::legacy_labels_with_font_and_bounds(&self.inner, self.content_rect(), ui)
        } else {
            self.own_labels_for_walk(ui)
        };
        for (tl, font, bounds) in labels {
            ctx.text_with(tl.text, tl.x, tl.y, tl.font_size, tl.color, font, bounds);
        }
    }

    // The legacy per-widget text getters are deleted from `WidgetHost`: this adapter's text
    // reaches the frame through `paint_self` above (prim-derived own labels + the
    // detached base label), and composites that need a concrete Adapted child's labels
    // call `own_labels_with_font_and_bounds` directly (pub(crate)).

    // --- Reverse bridges: [`Paint::paint`] output converted back to the legacy geometry
    // getters external render loops read (cce-test-interface's `all_*` calls, `render_widget`'s
    // `all_quads` loop, the demo's `extra_*` loops). Each prim kind maps to the getter legacy
    // widgets used for it — plain quads to `extra_quads` (→ `all_quads`), rounded to
    // `all_rounded_quads` — so apps that read BOTH getters draw each prim exactly once. Covers
    // the widget's OWN geometry only: adapted widgets are leaves for now; recursion belongs to
    // `scene::painter`.

    fn all_rounded_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, f32, [f32; 4], (bool, bool, bool, bool))> {
        if !self.visible() {
            return Vec::new();
        }
        let mut out: Vec<_> = self
            .painted_prims()
            .into_iter()
            .filter_map(|prim| match prim {
                Prim::RoundedRect { rect, radius, corners, color } => {
                    Some((rect.x, rect.y, rect.width, rect.height, radius, color, corners))
                }
                _ => None,
            })
            .collect();
        // Containers recurse, matching the `WidgetHost` default this override replaces.
        for child in self.visible_children() {
            out.extend(unsafe { &*child }.all_rounded_quads(ctx));
        }
        out
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.visible() {
            return Vec::new();
        }
        if Paint::serves_legacy_plain_quads(&self.inner) {
            return Paint::legacy_plain_quads(&self.inner, self.content_rect());
        }
        // Legacy container aggregation (Paginator): the plain view is the visible children's
        // chrome, and only that — the widget's own background stays in `all_quads`.
        if Paint::aggregates_child_extra_quads(&self.inner) {
            let mut out = Vec::new();
            for child in self.visible_children() {
                out.extend(unsafe { &*child }.extra_quads());
            }
            return out;
        }
        self.own_plain_quads()
    }

    /// When the widget serves a legacy plain-quad view, its geometry reaches
    /// `render_widget`-style hosts (which read BOTH quad getters) through `all_rounded_quads`
    /// only — `all_quads` must stay empty or they draw it twice. Mirrors legacy Graph's
    /// highlight-only `all_quads` override. Otherwise: the `WidgetHost` default minus the shared
    /// highlight (suppressed for all adapted widgets via `highlight_quad -> None`).
    fn all_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if Paint::serves_legacy_plain_quads(&self.inner) {
            return Vec::new();
        }
        // Own prims directly (NOT `extra_quads`, which may serve the child aggregation — those
        // children arrive once, through the recursion below).
        let mut quads = if self.visible() { self.own_plain_quads() } else { Vec::new() };
        // The `WidgetHost` default's highlight inclusion (secondary/hover tint excluded), live
        // only for widgets that opt into the legacy overlay. A forwarded highlight
        // ([`Paint::forwarded_highlight`]) is deliberately excluded: its owner's aggregation
        // already carries it, matching the legacy container `all_quads` overrides.
        if Paint::legacy_focus_highlight(&self.inner) {
            if let Some(hq) = WidgetHost::highlight_quad(self, ctx) {
                if hq.4 != crate::colors::HIGHLIGHT_SECONDARY {
                    quads.push(hq);
                }
            }
        }
        if self.visible() {
            // Container aggregation, replicating the shared legacy loop (Layer, Switcher):
            // children contribute their plain quads, except a rounded-cornered child's
            // background quad — that one arrives through `all_rounded_quads` instead.
            for child in self.visible_children() {
                let widget = unsafe { &*child };
                let (wx, wy, ww, wh) = widget.rect();
                let has_rounded = widget.corner_style().1 != (false, false, false, false);
                for (qx, qy, qw, qh, qc) in widget.all_quads(ctx) {
                    if has_rounded
                        && (qx - wx).abs() < 0.1
                        && (qy - wy).abs() < 0.1
                        && (qw - ww).abs() < 0.1
                        && (qh - wh).abs() < 0.1
                    {
                        continue;
                    }
                    quads.push((qx, qy, qw, qh, qc));
                }
            }
        }
        quads
    }

    fn extra_circles(&self) -> Vec<(f32, f32, f32, [f32; 4])> {
        if !self.visible() {
            return Vec::new();
        }
        self.painted_prims()
            .into_iter()
            .filter_map(|prim| match prim {
                Prim::Circle { cx, cy, radius, color } => Some((cx, cy, radius, color)),
                _ => None,
            })
            .collect()
    }

    fn extra_arcs(&self) -> Vec<(f32, f32, f32, f32, f32, f32, [f32; 4])> {
        if !self.visible() {
            return Vec::new();
        }
        self.painted_prims()
            .into_iter()
            .filter_map(|prim| match prim {
                Prim::Arc { cx, cy, radius, thickness, start, end, color } => {
                    Some((cx, cy, radius, thickness, start, end, color))
                }
                _ => None,
            })
            .collect()
    }

    // --- Input concern -> `Input` ---
    fn blocks_backplate_drag(&self) -> bool {
        Input::blocks_backplate_drag(&self.inner)
    }
    fn context_action(&mut self, action: crate::widget::ContextAction) -> bool {
        Input::context_action(&mut self.inner, action)
    }
    /// Row-rect assignment (row-layout hosts): apply the widget's clamp
    /// ([`Layout::adjust_row_rect`] — TextBox's `width`/`max_width`), then the base write the
    /// `WidgetHost` default does.
    fn set_row_rect(&mut self, x: f32, w: f32) {
        let (rx, rw) = Layout::adjust_row_rect(&self.inner, x, w);
        self.base.row_x = rx;
        self.base.row_w = rw;
    }
    fn tick(&mut self, dt: f32, ctx: &mut UiContext) -> bool {
        // Legacy value-owning containers healed their children's registry entries every tick
        // (addresses move with the owning struct); same cadence here.
        let host_id = self.base.id();
        Layout::register_embedded_children(&mut self.inner, host_id, ctx);
        let rect = self.content_rect();
        let mut changed = Input::tick(&mut self.inner, dt, rect);
        {
            let self_ptr = self.as_ptr_mut();
            let mut ectx = EventCtx { rect, id: host_id, ui: Some(&mut *ctx), self_ptr: Some(self_ptr) };
            changed |= Input::tick_ctx(&mut self.inner, dt, &mut ectx);
        }
        if self.visible {
            for child in self.visible_children() {
                changed |= unsafe { &mut *child }.tick(dt, ctx);
            }
        }
        changed
    }
    fn wants_tick(&self) -> bool {
        Input::wants_tick(&self.inner)
    }
    fn is_scrollable(&self) -> bool {
        Input::scrollable(&self.inner)
    }
    /// Focus set/cleared directly (hosts call `w.focus()`/`w.unfocus()`): keep the base flag
    /// (unless the widget opts out — [`Input::tracks_base_focus`], TextBox's legacy `focus`
    /// never set it) and tell the widget via the same `FocusIn`/`FocusOut` events the router
    /// would send.
    fn focus(&mut self) {
        if Input::tracks_base_focus(&self.inner) {
            self.base.focused = true;
        }
        let self_ptr = self.as_ptr_mut();
        let mut ectx = EventCtx { rect: self.content_rect(), id: self.base.id(), ui: None, self_ptr: Some(self_ptr) };
        Input::on_event(&mut self.inner, &Event::FocusIn, &mut ectx);
    }
    fn unfocus(&mut self) {
        if Input::tracks_base_focus(&self.inner) {
            self.base.focused = false;
        }
        let self_ptr = self.as_ptr_mut();
        let mut ectx = EventCtx { rect: self.content_rect(), id: self.base.id(), ui: None, self_ptr: Some(self_ptr) };
        Input::on_event(&mut self.inner, &Event::FocusOut, &mut ectx);
    }

    fn hit_test(&self, px: f32, py: f32, ctx: &UiContext) -> bool {
        // Hidden widgets are not hittable. Legacy widgets with a visibility toggle (Spreadsheet)
        // carry this gate themselves — and need it: hosts broadcast wheel/press dispatch to
        // every widget (the designer) and rely on hidden ones rejecting the hit.
        if !self.visible() {
            return false;
        }
        // Containers with a hit-through policy delegate entirely to their visible children
        // (each child runs its own coverage check) — the legacy Layer/Switcher pattern, which
        // never consulted the container's own rect or coverage.
        if Input::hits_through_children(&self.inner) {
            return self
                .visible_children()
                .into_iter()
                .any(|c| unsafe { &*c }.hit_test(px, py, ctx));
        }
        // Preserve the legacy occlusion check (a covering layer swallows the hit), then delegate
        // the geometric test to the narrow trait instead of the row/label-offset machinery.
        if ctx.is_coordinate_covered(self.base.id(), px, py) {
            return false;
        }
        let (x, y, w, h) = self.rect();
        // Row-hit opt-in ([`Layout::hit_row_rect`]): replicate the legacy `hit_test` default's
        // geometry — substitute the host-pushed row span and inset by the side label — before
        // the narrow test. The width<=0 reject also comes from that default.
        if Layout::hit_row_rect(&self.inner) {
            if w <= 0.0 || h <= 0.0 {
                return false;
            }
            let (mut hx, mut hw) = if self.base.row_w > 0.0 { (self.base.row_x, self.base.row_w) } else { (x, w) };
            let label_x = WidgetHost::label_x_offset(self);
            hx += label_x;
            hw -= label_x;
            return Input::hit(&self.inner, Rect { x: hx, y, width: hw, height: h }, px, py);
        }
        Input::hit(&self.inner, Rect { x, y, width: w, height: h }, px, py)
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut UiContext) -> bool {
        let rect = self.content_rect();
        let id = self.base.id();
        let self_ptr = self.as_ptr_mut();
        macro_rules! ectx {
            () => {
                EventCtx { rect, id, ui: Some(ctx), self_ptr: Some(self_ptr) }
            };
        }
        match event {
            // A hit right-press on a context-menu widget routes to the shared config menu —
            // `on_event` can't (that policy needs the target's WidgetHost pointer), so the adapter
            // owns it.
            Event::MouseButton {
                button: crate::widget::MouseButton::Right,
                state: crate::widget::ElementState::Pressed,
                x: px,
                y: py,
                ..
            } if Input::opens_context_menu(&self.inner) => {
                if self.hit_test(*px, *py, ctx) {
                    ctx.handle_right_click(self_ptr, *px, *py);
                    return true;
                }
                false
            }
            // Hit-gate PRESSES and wheel once, here, so narrow widgets never carry the
            // per-widget "check hit_test first" boilerplate legacy `mouse_input` overrides do.
            // RELEASES are deliberately NOT gated: a press-tracking widget (Button) must see the
            // release wherever the cursor ended up, to commit or cancel — exactly what legacy
            // `mouse_input` overrides did by receiving every release. Event-proxying containers
            // opt out of the press gate (`Input::gates_presses`): legacy container overrides
            // saw every press (Switcher unfocuses its child on an outside press).
            Event::MouseButton { state: crate::widget::ElementState::Pressed, x: px, y: py, .. }
                if !Input::gates_presses(&self.inner) =>
            {
                let _ = (px, py);
                Input::on_event(&mut self.inner, event, &mut ectx!())
            }
            Event::MouseButton { state: crate::widget::ElementState::Pressed, x: px, y: py, .. }
            | Event::MouseWheel { x: px, y: py, .. } => {
                self.hit_test(*px, *py, ctx) && Input::on_event(&mut self.inner, event, &mut ectx!())
            }
            Event::MouseButton { state: crate::widget::ElementState::Released, .. } => {
                Input::on_event(&mut self.inner, event, &mut ectx!())
            }
            // Offer the raw move to the widget; if unconsumed, run the legacy hover bookkeeping
            // (base.hovered + MouseEnter/MouseLeave synthesis, which re-enters this method and
            // reaches `on_event` through the arm below).
            Event::PointerMove { x: px, y: py, .. } => {
                if Input::on_event(&mut self.inner, event, &mut ectx!()) {
                    return true;
                }
                let (px, py) = (*px, *py);
                self.cursor_moved(px, py, ctx)
            }
            // The router's drag lifecycle (recorded drag target → DragStart/DragUpdate/
            // DragEnd) maps to the Input drag hooks, exactly like the direct
            // `WidgetHost::drag_*` entry points below — `on_event` is offered first, but no
            // widget consumes Drag* there today; without these arms the events fell into the
            // on_event default and every ROUTED drag was silently dead (the reason each app
            // historically kept its own held-drag index and called drag_update directly).
            Event::DragStart { start_x, start_y } => {
                if Input::on_event(&mut self.inner, event, &mut ectx!()) {
                    return true;
                }
                Input::drag_begin(&mut self.inner, *start_x, *start_y, rect);
                true
            }
            Event::DragUpdate { x, y, .. } => {
                if Input::on_event(&mut self.inner, event, &mut ectx!()) {
                    return true;
                }
                if let Some((nx, ny)) = Input::drag_reposition(&mut self.inner, *x, *y, rect) {
                    self.base.x = nx;
                    self.base.y = ny;
                    return true;
                }
                Input::drag_update(&mut self.inner, *x, *y, rect)
            }
            Event::DragEnd => {
                if Input::on_event(&mut self.inner, event, &mut ectx!()) {
                    return true;
                }
                Input::drag_end(&mut self.inner);
                true
            }
            // A widget hidden while still holding focus (the designer keys into
            // `focused_widget`; hiding a pane doesn't unfocus it) must not consume keys —
            // formerly the `keyboard_input` entry point's gate, now on the one funnel
            // (which also closes the routed path's missing-gate hole).
            Event::KeyInput(_) if !self.visible() => false,
            // Everything else (KeyInput, Tick, Enter/Leave, Focus*) forwards directly —
            // the legacy default dispatch would route these to leaf handlers Adapted never
            // overrides, so there is no behavior to fall back to.
            _ => Input::on_event(&mut self.inner, event, &mut ectx!()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::PathController;
    use crate::scene::layout::{Rect, Size};
    use crate::scene::paint::Prim;
    use crate::scene::painter::paint_tree;
    use crate::widget::UiContext;

    /// A leaf that only knows the two narrow concerns — no `WidgetHost` in sight: it reports an
    /// intrinsic size ([`Layout`]) and a color ([`Paint`]).
    struct Dot {
        color: [f32; 4],
        size: Size,
    }
    impl Layout for Dot {
        fn intrinsic_size(&self) -> Option<Size> {
            Some(self.size)
        }
    }
    impl Paint for Dot {
        fn color(&self) -> [f32; 4] {
            self.color
        }
    }
    impl Input for Dot {}

    /// A narrow container: it drives a column layout ([`Layout`]) and paints nothing.
    struct Col;
    impl Layout for Col {}
    impl Paint for Col {
        fn color(&self) -> [f32; 4] {
            [0.0, 0.0, 0.0, 0.0]
        }
    }
    impl Input for Col {}

    fn rect_of(ptr: *mut (dyn WidgetHost + 'static)) -> Rect {
        let (x, y, w, h) = unsafe { (*ptr).rect() };
        Rect { x, y, width: w, height: h }
    }

    #[test]
    fn narrow_widget_lays_out_and_paints_through_the_adapter() {
        // A pure narrow-trait widget tree (Col + two Dots), wrapped in `Adapted`, is laid out by
        // the existing bridge and painted by the existing painter — proving a widget that never
        // touches `WidgetHost` participates in both live passes.
        let mut ctx = UiContext::new();
        let mut root = Box::new(Adapted::new(Col));
        let mut a = Box::new(Adapted::new(Dot { color: [1.0, 0.0, 0.0, 1.0], size: Size::new(10.0, 10.0) }));
        let mut b = Box::new(Adapted::new(Dot { color: [0.0, 1.0, 0.0, 1.0], size: Size::new(10.0, 20.0) }));

        let (root_id, root_ptr) = (root.id(), root.as_ptr_mut());
        let (a_id, a_ptr) = (a.id(), a.as_ptr_mut());
        let (b_id, b_ptr) = (b.id(), b.as_ptr_mut());
        ctx.register_widget(root_id, root_ptr);
        ctx.register_widget(a_id, a_ptr);
        ctx.register_widget(b_id, b_ptr);
        ctx.link_ids(root_id, a_id);
        ctx.link_ids(root_id, b_id);

        // Layout by hand (the Phase-2b bridge is gone; apps drive the solver directly) —
        // the same column-of-two placement the bridge used to compute.
        unsafe {
            (*root_ptr).set_rect(0.0, 0.0, 100.0, 100.0);
            (*a_ptr).set_rect(0.0, 0.0, 10.0, 10.0);
            (*b_ptr).set_rect(0.0, 14.0, 10.0, 20.0);
        }
        assert_eq!(rect_of(a_ptr), Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 });
        assert_eq!(rect_of(b_ptr), Rect { x: 0.0, y: 14.0, width: 10.0, height: 20.0 });

        // Paint: each Dot's `Paint::paint` default emits one quad at its laid-out rect, in colour.
        let list = paint_tree(&ctx, unsafe { &*root_ptr });
        let quads: Vec<_> = list
            .items
            .iter()
            .filter_map(|it| match it.prim {
                Prim::Quad { rect, color } => Some((rect, color)),
                _ => None,
            })
            .collect();
        assert!(
            quads.iter().any(|(r, c)| *r == Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 } && c[0] == 1.0),
            "red Dot painted at its laid-out rect: {quads:?}",
        );
        assert!(
            quads.iter().any(|(r, c)| *r == Rect { x: 0.0, y: 14.0, width: 10.0, height: 20.0 } && c[1] == 1.0),
            "green Dot painted at its laid-out rect: {quads:?}",
        );
    }

    /// A narrow interactive widget: counts left-clicks and records hover transitions — all
    /// through [`Input::on_event`], never touching `WidgetHost`.
    struct Clicker {
        clicks: u32,
        entered: u32,
        left: u32,
    }
    impl Layout for Clicker {}
    impl Paint for Clicker {
        fn color(&self) -> [f32; 4] {
            [0.5, 0.5, 0.5, 1.0]
        }
    }
    impl Input for Clicker {
        fn on_event(&mut self, event: &Event, _ectx: &mut EventCtx) -> bool {
            use crate::widget::{ElementState, MouseButton};
            match event {
                Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, .. } => {
                    self.clicks += 1;
                    true
                }
                Event::MouseEnter => {
                    self.entered += 1;
                    false
                }
                Event::MouseLeave => {
                    self.left += 1;
                    false
                }
                _ => false,
            }
        }
    }

    #[test]
    fn narrow_widget_receives_routed_events_through_the_adapter() {
        use crate::widget::{ElementState, MouseButton};
        let mut ctx = UiContext::new();
        let mut w = Box::new(Adapted::new(Clicker { clicks: 0, entered: 0, left: 0 }));
        let (id, ptr) = (w.id(), w.as_ptr_mut());
        ctx.register_widget(id, ptr);
        unsafe { (*ptr).set_rect(10.0, 10.0, 40.0, 20.0) };

        let click_at = |x: f32, y: f32| Event::MouseButton {
            button: MouseButton::Left,
            state: ElementState::Pressed,
            x,
            y,
            local_x: x,
            local_y: y,
        };

        // A click inside the rect is hit-gated in, consumed, and counted.
        assert!(ctx.propagate_event(&click_at(20.0, 15.0), id), "in-rect click is consumed");
        // A click outside never reaches on_event (the adapter's hit gate rejects it).
        assert!(!ctx.propagate_event(&click_at(200.0, 200.0), id), "out-of-rect click passes through");
        assert_eq!(w.inner().clicks, 1, "only the in-rect click was counted");

        // Hover: moving inside synthesizes MouseEnter (via the legacy bookkeeping the adapter
        // preserves) and sets the base hover flag; moving away synthesizes MouseLeave.
        ctx.propagate_event(&Event::PointerMove { x: 20.0, y: 15.0, local_x: 20.0, local_y: 15.0 }, id);
        assert_eq!(w.inner().entered, 1, "MouseEnter reached on_event");
        assert!(unsafe { (*ptr).base().hovered }, "base hover flag set through the adapter");
        ctx.propagate_event(&Event::PointerMove { x: 200.0, y: 200.0, local_x: 200.0, local_y: 200.0 }, id);
        assert_eq!(w.inner().left, 1, "MouseLeave reached on_event");
        assert!(!unsafe { (*ptr).base().hovered }, "base hover flag cleared");
    }

    /// A narrow widget that is also a controller: the controller trait is reached through the
    /// concrete `Adapted<W>` by deref (Phase 6aw -- the `WidgetHost::as_*_controller` discovery
    /// hooks are deleted).
    struct Crumbs {
        segs: Vec<String>,
        clicked: Option<usize>,
    }
    impl Layout for Crumbs {}
    impl Paint for Crumbs {
        fn color(&self) -> [f32; 4] {
            [0.0; 4]
        }
    }
    impl Input for Crumbs {
    }
    impl PathController for Crumbs {
        fn set_path(&mut self, segments: &[String]) {
            self.segs = segments.to_vec();
        }
        fn path_click(&mut self) -> Option<usize> {
            self.clicked.take()
        }
    }

    #[test]
    fn controller_capability_reached_through_the_concrete_adapter() {
        let mut w = Box::new(Adapted::new(Crumbs { segs: Vec::new(), clicked: Some(2) }));

        // The controller trait is reached by deref through the concrete Adapted<W>...
        PathController::set_path(&mut **w, &["home".to_string(), "user".to_string()]);
        assert_eq!(PathController::path_click(&mut **w), Some(2));

        // ...and lands on the same state the concrete widget sees.
        assert_eq!(w.inner().segs, vec!["home".to_string(), "user".to_string()]);
        assert_eq!(w.inner().clicked, None, "path_click drained through the deref");
    }
    /// Phase 6: the paint walk's text prims carry the widget's font and clip rect (what the
    /// display-list text path renders), not the bare `Paint::paint` text.
    #[test]
    fn paint_walk_text_carries_font_and_bounds() {
        struct Tag;
        impl Layout for Tag {}
        impl Paint for Tag {
            fn color(&self) -> [f32; 4] {
                [0.0; 4]
            }
            fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
                ctx.text("hi", rect.x + 2.0, rect.y + 2.0, 12.0, [1, 2, 3]);
            }
            fn widget_font(&self) -> Option<String> {
                Some("Mono:12".into())
            }
            fn text_bounds(&self, rect: Rect) -> Option<[f32; 4]> {
                Some([rect.x, rect.y, rect.x + rect.width, rect.y + rect.height])
            }
        }
        impl Input for Tag {}

        let mut ctx = UiContext::new();
        let mut w = Box::new(Adapted::new(Tag));
        let (id, ptr) = (w.id(), w.as_ptr_mut());
        ctx.register_widget(id, ptr);
        unsafe { (*ptr).set_rect(10.0, 20.0, 100.0, 30.0) };

        let list = paint_tree(&ctx, unsafe { &*ptr });
        let texts: Vec<_> = list
            .items
            .iter()
            .filter_map(|it| match &it.prim {
                Prim::Text { text, font, bounds, .. } => Some((text.clone(), font.clone(), *bounds)),
                _ => None,
            })
            .collect();
        assert_eq!(texts.len(), 1, "one text prim, no plain duplicate");
        assert_eq!(texts[0].0, "hi");
        assert_eq!(texts[0].1.as_deref(), Some("Mono:12"), "widget_font attached");
        assert_eq!(texts[0].2, Some([10.0, 20.0, 110.0, 50.0]), "text_bounds attached");
    }

}
