//! Narrow, single-concern widget traits + an adapter into the legacy `Element` tree — Phase 5 of
//! the core rebuild (see `docs/rfc-core-rebuild.md` §3.5 and §5).
//!
//! Phase 5 replaces the ~123-method [`Element`] god-trait with small traits, one per concern. A
//! *non-breaking supertrait carve-out* of `Element` is not possible in Rust, for two reasons found
//! by experiment:
//!
//! 1. The structural methods the layout/paint passes need (`rect`, `children`, `set_rect`, …) are
//!    overridden in dozens of widgets across cce-ui **and** the app crates. Moving them off
//!    `Element` breaks every override; merely *declaring* them on a supertrait breaks every call
//!    site too, because a supertrait method is always in scope on the subtrait — `elem.children()`
//!    on a `&dyn Element` becomes ambiguous.
//! 2. Trait-object coercion does not offer a way around it: a blanket "view" impl
//!    `impl<T: Element> Paint for T` does **not** let `&dyn Element` coerce to `&dyn Paint`
//!    (that coercion only exists for real supertraits).
//!
//! So we take the RFC's recommended **adapter** path. The traits here — [`Layout`] and [`Paint`] —
//! are *independent* of `Element` (no super/sub relationship). A widget written against them is
//! placed into the existing `*mut dyn Element` tree by wrapping it in [`Adapted`], whose `Element`
//! impl forwards each legacy method to the matching narrow-trait method and supplies the
//! [`Widget`] base that `Element`'s rect/id/dirty machinery reads. Existing `impl Element` widgets
//! are untouched; new or migrated widgets implement only the concern traits they need; both kinds
//! coexist in one tree. When the last widget is migrated, `Element` and this adapter are deleted.
//!
//! This commit lands the two concerns the scene passes already consume: [`Layout`] drives
//! [`crate::scene::bridge`] and [`Paint`] drives [`crate::scene::painter`]. The input/event
//! concern follows in its own commit.

use crate::scene::layout::{Rect, Size, Style};
use crate::scene::paint::{PaintCtx, Prim};
use crate::widget::{Element, Event, TextLabel, UiContext, Widget, WidgetId};

/// Layout inputs for the scene layout engine — the RFC's `Widget` concern, named `Layout` here to
/// avoid the existing [`Widget`] base struct. Mirrors the opt-in `Element::layout_style` /
/// `intrinsic_size` / `layout_children` hooks consumed by [`crate::scene::bridge`].
pub trait Layout {
    /// Opt-in layout style for the engine. `None` (default) ⇒ this widget does not drive
    /// engine-computed layout. See [`Element::layout_style`].
    fn layout_style(&self) -> Option<Style> {
        None
    }

    /// Intrinsic content size of a leaf (e.g. measured text) for the measure pass. See
    /// [`Element::intrinsic_size`].
    fn intrinsic_size(&self) -> Option<Size> {
        None
    }

    /// Per-child styles for containers that size their children from the parent (e.g. `SplitBox`
    /// proportions), in `children()` order. See [`Element::layout_children`].
    fn layout_children(&self) -> Option<Vec<Style>> {
        None
    }

    /// Whether this widget draws its control label *inline* (inside its own rect, like
    /// `Checkbox`/`Toggle`/`Button`) rather than detached above it (like `ProgressBar`/`Slider`).
    /// Inline-label widgets get no `set_rect` height inflation and no content-rect inset —
    /// mirroring the legacy `label_offset` free function's type-name special cases.
    fn inline_label(&self) -> bool {
        false
    }

    /// Whether legacy container layout passes should skip this widget (the app positions it
    /// itself — legacy `Element::layout_ignore`, read by `Plate` and the page layout). Default:
    /// participate.
    fn layout_ignore(&self) -> bool {
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
}

/// The paint concern — a widget's fill color, its own (non-recursive) geometry emission, and
/// whether it clips its children. Mirrors `Element::color` / `paint_self` / `clips_children`, but
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

    /// Corner rounding `(radius, per-corner flags)` of the widget's background. **Transitional:**
    /// this exists only for legacy render paths that draw widget backgrounds themselves from
    /// style properties (`widget_vertices` / `push_widget_vertices` readers of
    /// `Element::corner_radius` + `rounded_corners`) — the widget's real geometry is whatever
    /// [`paint`](Paint::paint) emits. Dies with those paths. Default: sharp corners.
    fn corner_style(&self) -> Option<(f32, (bool, bool, bool, bool))> {
        None
    }

    /// Solid border `(color, thickness)` of the widget's background quad. **Transitional**, like
    /// [`corner_style`](Paint::corner_style): `render_widget` gives a widget's background quad a
    /// border+inset treatment when this is `Some` — `Toggle`'s square mode depends on it.
    fn solid_border(&self) -> Option<([f32; 4], f32)> {
        None
    }

    /// Font for this widget's text on legacy text paths (`render_widget` reads
    /// `Element::widget_font`). **Transitional.**
    fn widget_font(&self) -> Option<String> {
        None
    }

    /// Receive the control label set on the wrapper via [`Adapted::with_label`] (and legacy
    /// `Control::set_label` paths). Widgets that paint their label themselves (inline-label
    /// widgets) store it here; the default discards it, leaving label drawing to the adapter's
    /// base-label machinery.
    fn sync_label(&mut self, _label: &str) {}
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
    self_ptr: Option<*mut (dyn Element + 'static)>,
}

impl EventCtx<'_> {
    /// Make this widget the global focus target (legacy `focus::set_focused(self)`).
    pub fn request_focus(&mut self) {
        if let Some(ptr) = self.self_ptr {
            unsafe { crate::widget::focus::set_focused(&mut *ptr) };
        }
    }
}

/// The input concern — hit-testing and event handling against the laid-out rect. Mirrors the
/// legacy `Element::hit_test` / `handle_event` pair, but with the RFC's centralizations: the
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
    /// Default: `true`, matching the legacy `Element` default.
    fn blocks_backplate_drag(&self) -> bool {
        true
    }

    /// Whether a right-click on this widget opens the shared config context menu (the adapter
    /// then routes it to `UiContext::handle_right_click`, which `on_event` can't reach — it has
    /// no ctx by design). Default: no.
    fn opens_context_menu(&self) -> bool {
        false
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

    /// The widget's value as an integer (legacy `Element::value`).
    fn value(&self) -> i32 {
        0
    }

    /// Selection state pushed in by list/row hosts (legacy `Element::set_selected`).
    fn set_selected(&mut self, _selected: bool) {}

    // --- Drag surface: legacy hosts (designer, control_panel, parameters_bg, graph, audio…)
    // drive drags by calling these directly on the widget, not through events.

    fn draggable(&self) -> bool {
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
    /// Movement bounds pushed in by hosts (legacy `Element::set_drag_bounds`).
    fn set_drag_bounds(&mut self, _bx: f32, _by: f32, _bw: f32, _bh: f32) {}
}

/// Wraps a narrow-trait widget `W` so it lives in the legacy `*mut dyn Element` tree. Carries the
/// [`Widget`] base that `Element`'s rect / id / dirty machinery needs, and forwards the concern
/// methods to `W`. See the module docs for why this bridge exists rather than a supertrait split.
///
/// The bounds live on the struct (not just the `Element` impl) so `Drop` can clear the global
/// focus / context-menu references through `&dyn Element` — the same guard legacy widgets with
/// `Drop` impls (e.g. the old `Checkbox`) carried.
#[derive(Debug, Clone)]
pub struct Adapted<W: Layout + Paint + Input + 'static> {
    base: Widget,
    inner: W,
}

impl<W: Layout + Paint + Input + 'static> Drop for Adapted<W> {
    fn drop(&mut self) {
        crate::widget::clear_widget_references(self);
    }
}

impl<W: Layout + Paint + Input + 'static> Adapted<W> {
    /// Wrap `inner` with a fresh [`Widget`] base.
    pub fn new(inner: W) -> Self {
        Adapted { base: Widget::new(), inner }
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
    /// Run the wrapped widget's [`Paint::paint`] against its content rect and return the emitted
    /// prims — the shared source for the reverse bridges (`extra_quads`, `all_rounded_quads`,
    /// `extra_circles`, `extra_arcs`, prim-derived `text_labels`) that legacy render loops read.
    fn painted_prims(&self) -> Vec<Prim> {
        let mut pc = PaintCtx::new();
        Paint::paint(&self.inner, self.content_rect(), &mut pc);
        pc.finish().items.into_iter().map(|item| item.prim).collect()
    }

    /// The base-label text of a *detached*-label widget — a replica of the legacy default
    /// `Element::text_labels` body (which an overriding impl can no longer call).
    fn base_label_fallback(&self) -> Vec<TextLabel> {
        let b = &self.base;
        if let Some(ref label) = b.label {
            let (_, font_size) = crate::layout::control_label_font_detached_parsed();
            let color = crate::colors::control_label_color_detached_for_state(b.hovered, b.focused);
            if crate::layout::control_label_layout() == "side" {
                let label_x = Element::label_x_offset(self);
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

impl<W: Layout + Paint + Input + 'static> Element for Adapted<W> {
    fn base(&self) -> Option<&Widget> {
        Some(&self.base)
    }
    fn base_mut(&mut self) -> Option<&mut Widget> {
        Some(&mut self.base)
    }
    // `as_any` exposes the *inner* widget: legacy code downcasts by concrete widget type
    // (`json_layout`'s `downcast_mut::<Checkbox>()`), and the adapter must be transparent to it.
    fn as_any(&self) -> &dyn std::any::Any {
        &self.inner
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        &mut self.inner
    }
    fn as_ptr(&self) -> *mut (dyn Element + 'static) {
        self as *const Self as *mut Self as *mut (dyn Element + 'static)
    }
    fn as_ptr_mut(&mut self) -> *mut (dyn Element + 'static) {
        self as *mut Self as *mut (dyn Element + 'static)
    }

    // --- Layout concern -> `Layout` ---
    fn layout_style(&self) -> Option<Style> {
        Layout::layout_style(&self.inner)
    }
    fn intrinsic_size(&self) -> Option<Size> {
        Layout::intrinsic_size(&self.inner)
    }
    fn layout_children(&self) -> Option<Vec<Style>> {
        Layout::layout_children(&self.inner)
    }
    fn layout_ignore(&self) -> bool {
        Layout::layout_ignore(&self.inner)
    }

    // --- Legacy structural conventions the adapter owns on the widget's behalf ---

    /// The detached-label convention shared by legacy control widgets: the widget grows past the
    /// rect its parent assigns to make room for the label above (`ProgressBar`/`Slider`-style
    /// `set_rect` overrides). Inline-label widgets ([`Layout::inline_label`]) draw the label
    /// inside their rect and get no inflation. Zero-cost when no label is set.
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let inflation = if Layout::inline_label(&self.inner) || !Layout::inflates_label_rect(&self.inner) {
            0.0
        } else {
            self.base.label_offset()
        };
        self.base.x = x;
        self.base.y = y;
        self.base.w = w;
        self.base.h = h + inflation;
    }

    fn preferred_height(&self) -> Option<f32> {
        Layout::intrinsic_size(&self.inner).map(|s| s.height)
    }

    /// Narrow widgets own every pixel they draw through [`Paint::paint`]; the legacy shared
    /// hover-highlight overlay is suppressed (matching what most control widgets' `None`
    /// overrides do today).
    fn highlight_quad(&self, _ctx: &UiContext) -> Option<(f32, f32, f32, f32, [f32; 4])> {
        None
    }

    /// Report the *inner* type's name, not `Adapted<W>`: runtime type-name matching (e.g.
    /// `layout.rs`' span-full widget list) must keep seeing the widget it knows.
    fn type_name(&self) -> &'static str {
        std::any::type_name::<W>().split("::").last().unwrap_or("Widget")
    }

    /// Text-content mutation (legacy `Element::set_text` wrote only `base.label`): keep the base
    /// copy and the widget's own copy ([`Paint::sync_label`]) in step, like `set_label`.
    fn set_text(&mut self, text: &str) {
        self.base.label = Some(text.to_string());
        Paint::sync_label(&mut self.inner, text);
    }

    // --- Paint concern -> `Paint` ---
    fn color(&self) -> [f32; 4] {
        Paint::color(&self.inner)
    }
    fn clips_children(&self) -> bool {
        Paint::clips_children(&self.inner)
    }
    fn corner_radius(&self) -> f32 {
        // 12.0 mirrors the `Element` default for widgets without a corner style.
        Paint::corner_style(&self.inner).map_or(12.0, |(r, _)| r)
    }
    fn rounded_corners(&self) -> (bool, bool, bool, bool) {
        Paint::corner_style(&self.inner).map_or((false, false, false, false), |(_, c)| c)
    }
    fn solid_border(&self) -> Option<([f32; 4], f32)> {
        Paint::solid_border(&self.inner)
    }
    fn widget_font(&self) -> Option<String> {
        Paint::widget_font(&self.inner)
    }
    fn paint_self(&self, _ui: &UiContext, ctx: &mut PaintCtx) {
        Paint::paint(&self.inner, self.content_rect(), ctx);
        // Inline-label widgets emit their own text in `paint`; detached labels come from the
        // base, exactly as the legacy default `paint_self` emits them.
        if !Layout::inline_label(&self.inner) {
            for tl in self.base_label_fallback() {
                ctx.text(tl.text, tl.x, tl.y, tl.font_size, tl.color);
            }
        }
    }

    /// Text derived from the [`Paint::paint`] `Text` prims (one source of truth for what the
    /// widget draws — inline labels, readouts), plus the base-label text for detached-label
    /// widgets (drawn by the adapter, since the label lives on the base).
    fn text_labels(&self) -> Vec<TextLabel> {
        let mut out: Vec<TextLabel> = self
            .painted_prims()
            .into_iter()
            .filter_map(|prim| match prim {
                Prim::Text { text, x, y, font_size, color } => {
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

    // --- Reverse bridges: [`Paint::paint`] output converted back to the legacy geometry
    // getters external render loops read (cce-test-interface's `all_*` calls, `render_widget`'s
    // `all_quads` loop, the demo's `extra_*` loops). Each prim kind maps to the getter legacy
    // widgets used for it — plain quads to `extra_quads` (→ `all_quads`), rounded to
    // `all_rounded_quads` — so apps that read BOTH getters draw each prim exactly once. Covers
    // the widget's OWN geometry only: adapted widgets are leaves for now; recursion belongs to
    // `scene::painter`.

    fn all_rounded_quads(&self, _ctx: &UiContext) -> Vec<(f32, f32, f32, f32, f32, [f32; 4], (bool, bool, bool, bool))> {
        if !self.visible() {
            return Vec::new();
        }
        self.painted_prims()
            .into_iter()
            .filter_map(|prim| match prim {
                Prim::RoundedRect { rect, radius, corners, color } => {
                    Some((rect.x, rect.y, rect.width, rect.height, radius, color, corners))
                }
                _ => None,
            })
            .collect()
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.visible() {
            return Vec::new();
        }
        self.painted_prims()
            .into_iter()
            .filter_map(|prim| match prim {
                Prim::Quad { rect, color } => Some((rect.x, rect.y, rect.width, rect.height, color)),
                _ => None,
            })
            .collect()
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
    fn take_click(&mut self) -> bool {
        Input::take_click(&mut self.inner)
    }
    fn take_change(&mut self) -> bool {
        Input::take_change(&mut self.inner)
    }
    fn get_value_string(&self) -> Option<String> {
        Input::value_string(&self.inner)
    }
    fn set_value_string(&mut self, val: &str) -> bool {
        Input::set_value_string(&mut self.inner, val)
    }
    fn value(&self) -> i32 {
        Input::value(&self.inner)
    }
    fn set_selected(&mut self, selected: bool) {
        Input::set_selected(&mut self.inner, selected)
    }
    fn draggable(&self) -> bool {
        Input::draggable(&self.inner)
    }
    fn is_dragging(&self) -> bool {
        Input::is_dragging(&self.inner)
    }
    fn drag_begin(&mut self, px: f32, py: f32) {
        let rect = self.content_rect();
        Input::drag_begin(&mut self.inner, px, py, rect)
    }
    fn drag_update(&mut self, px: f32, py: f32) -> bool {
        let rect = self.content_rect();
        if let Some((nx, ny)) = Input::drag_reposition(&mut self.inner, px, py, rect) {
            self.base.x = nx;
            self.base.y = ny;
            return true;
        }
        Input::drag_update(&mut self.inner, px, py, rect)
    }
    fn drag_end(&mut self) {
        Input::drag_end(&mut self.inner)
    }
    fn set_drag_bounds(&mut self, bx: f32, by: f32, bw: f32, bh: f32) {
        Input::set_drag_bounds(&mut self.inner, bx, by, bw, bh)
    }

    // --- Legacy direct-dispatch entry points. Hosts (treelist's add-key button, parameters_bg's
    // checkboxes, app pages) call these ON the widget instead of routing an Event through
    // `propagate_event`; without these overrides they'd hit the inert Element defaults and the
    // widget would go deaf on those paths. Route them into `handle_event` so the hit-gating /
    // context-menu / on_event pipeline applies identically on both paths.

    fn mouse_input(&mut self, button: crate::widget::MouseButton, state: crate::widget::ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        self.handle_event(
            &Event::MouseButton { button, state, x: px, y: py, local_x: px, local_y: py },
            ctx,
        )
    }
    fn mouse_wheel(&mut self, delta: &crate::widget::MouseScrollDelta, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        self.handle_event(
            &Event::MouseWheel { delta: *delta, x: px, y: py, local_x: px, local_y: py },
            ctx,
        )
    }
    fn keyboard_input(&mut self, event: &crate::widget::KeyEvent, ctx: &mut UiContext) -> bool {
        self.handle_event(&Event::KeyInput(event.clone()), ctx)
    }

    /// Focus set/cleared directly (hosts call `w.focus()`/`w.unfocus()`): keep the base flag and
    /// tell the widget via the same `FocusIn`/`FocusOut` events the router would send.
    fn focus(&mut self) {
        self.base.focused = true;
        let mut ectx = EventCtx { rect: self.content_rect(), id: self.base.id(), ui: None, self_ptr: None };
        Input::on_event(&mut self.inner, &Event::FocusIn, &mut ectx);
    }
    fn unfocus(&mut self) {
        self.base.focused = false;
        let mut ectx = EventCtx { rect: self.content_rect(), id: self.base.id(), ui: None, self_ptr: None };
        Input::on_event(&mut self.inner, &Event::FocusOut, &mut ectx);
    }

    fn hit_test(&self, px: f32, py: f32, ctx: &UiContext) -> bool {
        // Preserve the legacy occlusion check (a covering layer swallows the hit), then delegate
        // the geometric test to the narrow trait instead of the row/label-offset machinery.
        if ctx.is_coordinate_covered(self as *const Self as *const () as usize, px, py) {
            return false;
        }
        let (x, y, w, h) = self.rect();
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
            // `on_event` can't (that policy needs the target's Element pointer), so the adapter
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
            // `mouse_input` overrides did by receiving every release.
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
            // Everything else (KeyInput, Tick, Enter/Leave, Drag*, Focus*) forwards directly —
            // the legacy default dispatch would route these to leaf handlers Adapted never
            // overrides, so there is no behavior to fall back to.
            _ => Input::on_event(&mut self.inner, event, &mut ectx!()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::bridge::layout_subtree;
    use crate::scene::layout::{CrossAlign, Size, Style};
    use crate::scene::paint::Prim;
    use crate::scene::painter::paint_tree;
    use crate::widget::UiContext;

    /// A leaf that only knows the two narrow concerns — no `Element` in sight: it reports an
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
    impl Layout for Col {
        fn layout_style(&self) -> Option<Style> {
            Some(Style::column().gap(4.0).cross_align(CrossAlign::Start))
        }
    }
    impl Paint for Col {
        fn color(&self) -> [f32; 4] {
            [0.0, 0.0, 0.0, 0.0]
        }
    }
    impl Input for Col {}

    fn rect_of(ptr: *mut (dyn Element + 'static)) -> Rect {
        let (x, y, w, h) = unsafe { (*ptr).rect() };
        Rect { x, y, width: w, height: h }
    }

    #[test]
    fn narrow_widget_lays_out_and_paints_through_the_adapter() {
        // A pure narrow-trait widget tree (Col + two Dots), wrapped in `Adapted`, is laid out by
        // the existing bridge and painted by the existing painter — proving a widget that never
        // touches `Element` participates in both live passes.
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

        // Layout: column of a 10x10 then a 10x20, gap 4, in a 100x100 area.
        layout_subtree(&ctx, root_ptr, Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 });
        assert_eq!(rect_of(a_ptr), Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 });
        assert_eq!(rect_of(b_ptr), Rect { x: 0.0, y: 14.0, width: 10.0, height: 20.0 });

        // Paint: each Dot's `Paint::paint` default emits one quad at its laid-out rect, in colour.
        let list = paint_tree(&ctx, root_ptr);
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
    /// through [`Input::on_event`], never touching `Element`.
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
        assert!(ctx.propagate_event(&click_at(20.0, 15.0), ptr), "in-rect click is consumed");
        // A click outside never reaches on_event (the adapter's hit gate rejects it).
        assert!(!ctx.propagate_event(&click_at(200.0, 200.0), ptr), "out-of-rect click passes through");
        assert_eq!(w.inner().clicks, 1, "only the in-rect click was counted");

        // Hover: moving inside synthesizes MouseEnter (via the legacy bookkeeping the adapter
        // preserves) and sets the base hover flag; moving away synthesizes MouseLeave.
        ctx.propagate_event(&Event::PointerMove { x: 20.0, y: 15.0, local_x: 20.0, local_y: 15.0 }, ptr);
        assert_eq!(w.inner().entered, 1, "MouseEnter reached on_event");
        assert!(unsafe { (*ptr).hovered() }, "base hover flag set through the adapter");
        ctx.propagate_event(&Event::PointerMove { x: 200.0, y: 200.0, local_x: 200.0, local_y: 200.0 }, ptr);
        assert_eq!(w.inner().left, 1, "MouseLeave reached on_event");
        assert!(!unsafe { (*ptr).hovered() }, "base hover flag cleared");
    }
}
