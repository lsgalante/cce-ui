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
use crate::scene::paint::PaintCtx;
use crate::widget::{Element, UiContext, Widget, WidgetId};

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
}

impl<W: Layout + Paint + 'static> Element for Adapted<W> {
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

    // --- Paint concern -> `Paint` ---
    fn color(&self) -> [f32; 4] {
        Paint::color(&self.inner)
    }
    fn clips_children(&self) -> bool {
        Paint::clips_children(&self.inner)
    }
    fn paint_self(&self, _ui: &UiContext, ctx: &mut PaintCtx) {
        let (x, y, w, h) = self.rect();
        Paint::paint(&self.inner, Rect { x, y, width: w, height: h }, ctx);
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
}
