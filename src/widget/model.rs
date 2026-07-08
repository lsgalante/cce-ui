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
use crate::widget::{Element, Event, UiContext, Widget, WidgetId};

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

    /// React to `event`, given the laid-out `rect`. Return `true` to consume it (the router marks
    /// the widget dirty and stops propagation). `MouseButton` / `MouseWheel` events arrive only
    /// when [`hit`](Input::hit) passed; `MouseEnter` / `MouseLeave` are synthesized by the hover
    /// machinery. Default: ignore everything.
    fn on_event(&mut self, _event: &Event, _rect: Rect) -> bool {
        false
    }
}

/// Wraps a narrow-trait widget `W` so it lives in the legacy `*mut dyn Element` tree. Carries the
/// [`Widget`] base that `Element`'s rect / id / dirty machinery needs, and forwards the concern
/// methods to `W`. See the module docs for why this bridge exists rather than a supertrait split.
pub struct Adapted<W> {
    base: Widget,
    inner: W,
}

impl<W> Adapted<W> {
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

    /// Attach a detached control label (drawn above the widget by the shared `text_labels`
    /// machinery). Mirrors the `with_label` builders legacy control widgets carry, so
    /// construction sites keep their shape when a widget migrates.
    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self
    }

    /// The rect the wrapped widget paints into: the widget's rect minus the detached-label
    /// region at the top (zero inset when there is no label — `Widget::label_offset`).
    fn content_rect(&self) -> Rect {
        let top = self.base.label_offset();
        Rect {
            x: self.base.x,
            y: self.base.y + top,
            width: self.base.w,
            height: (self.base.h - top).max(0.0),
        }
    }
}

impl<W: Layout + Paint + Input + 'static> Element for Adapted<W> {
    fn base(&self) -> Option<&Widget> {
        Some(&self.base)
    }
    fn base_mut(&mut self) -> Option<&mut Widget> {
        Some(&mut self.base)
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
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

    // --- Legacy structural conventions the adapter owns on the widget's behalf ---

    /// The detached-label convention shared by legacy control widgets: the widget grows past the
    /// rect its parent assigns to make room for the label above (`ProgressBar`/`Slider`-style
    /// `set_rect` overrides). Zero-cost when no label is set.
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.base.x = x;
        self.base.y = y;
        self.base.w = w;
        self.base.h = h + self.base.label_offset();
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
    fn paint_self(&self, _ui: &UiContext, ctx: &mut PaintCtx) {
        Paint::paint(&self.inner, self.content_rect(), ctx);
        // The detached label, exactly as the legacy default `paint_self` emits it.
        for tl in self.text_labels() {
            ctx.text(tl.text, tl.x, tl.y, tl.font_size, tl.color);
        }
    }

    /// Reverse bridge for legacy render loops that read geometry via `all_rounded_quads` (e.g.
    /// cce-test-interface's view path): the wrapped widget's [`Paint::paint`] output, converted
    /// back to the legacy tuples. Covers the widget's OWN geometry only — adapted widgets are
    /// leaves for now; the recursive child walk belongs to `scene::painter`.
    fn all_rounded_quads(&self, _ctx: &UiContext) -> Vec<(f32, f32, f32, f32, f32, [f32; 4], (bool, bool, bool, bool))> {
        if !self.visible() {
            return Vec::new();
        }
        let mut pc = PaintCtx::new();
        Paint::paint(&self.inner, self.content_rect(), &mut pc);
        pc.finish()
            .items
            .into_iter()
            .filter_map(|item| match item.prim {
                Prim::RoundedRect { rect, radius, corners, color } => {
                    Some((rect.x, rect.y, rect.width, rect.height, radius, color, corners))
                }
                // Radius 0 routes through the backend's plain-quad branch, byte-for-byte.
                Prim::Quad { rect, color } => {
                    Some((rect.x, rect.y, rect.width, rect.height, 0.0, color, (false, false, false, false)))
                }
                _ => None,
            })
            .collect()
    }

    // --- Input concern -> `Input` ---
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
        let (x, y, w, h) = self.rect();
        let rect = Rect { x, y, width: w, height: h };
        match event {
            // Hit-gate pointer-positioned events once, here, so narrow widgets never carry the
            // per-widget "check hit_test first" boilerplate legacy `mouse_input` overrides do.
            Event::MouseButton { x: px, y: py, .. } | Event::MouseWheel { x: px, y: py, .. } => {
                self.hit_test(*px, *py, ctx) && Input::on_event(&mut self.inner, event, rect)
            }
            // Offer the raw move to the widget; if unconsumed, run the legacy hover bookkeeping
            // (base.hovered + MouseEnter/MouseLeave synthesis, which re-enters this method and
            // reaches `on_event` through the arm below).
            Event::PointerMove { x: px, y: py, .. } => {
                if Input::on_event(&mut self.inner, event, rect) {
                    return true;
                }
                let (px, py) = (*px, *py);
                self.cursor_moved(px, py, ctx)
            }
            // Everything else (KeyInput, Tick, Enter/Leave, Drag*, Focus*) forwards directly —
            // the legacy default dispatch would route these to leaf handlers Adapted never
            // overrides, so there is no behavior to fall back to.
            _ => Input::on_event(&mut self.inner, event, rect),
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
        fn on_event(&mut self, event: &Event, _rect: Rect) -> bool {
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
