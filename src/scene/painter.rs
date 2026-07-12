//! The paint walk — Phase 3 of the core rebuild.
//!
//! One traversal of the widget tree that emits every widget's own primitives into a single
//! [`DisplayList`], in draw order, through a [`PaintCtx`]. Recursion and clipping live *here*
//! (not smeared across each container's `all_*` methods): a widget contributes its own geometry
//! via [`Element::paint_self`], then the walk descends into its children — pushing the widget's
//! rect as a clip first when [`Element::clips_children`] is set, so the clip stack composes
//! automatically instead of every container re-deriving intersections by hand.
//!
//! This replaces, once wired into the backend, the three uncoordinated render paths (top-level
//! `view*`, recursive `all_rounded_quads`, immediate-mode `render_widget`). This module is the
//! walk itself, unit-tested here; routing the backend's `render()` through its `DisplayList`
//! (with GPU `set_scissor_rect` per clip) is the runtime-gated follow-up.

use crate::scene::layout::Rect;
use crate::scene::paint::{DisplayList, PaintCtx, Prim};
use crate::widget::{Element, TextLabel, UiContext};

type ElemPtr = *mut (dyn Element + 'static);

/// Walk the widget subtree rooted at `root` and produce its ordered, clipped [`DisplayList`].
///
/// # Safety
/// `root` and every widget reachable through `Element::children` must be live — the same
/// invariant the rest of the toolkit relies on for its `*mut dyn Element` tree.
pub fn paint_tree(ui: &UiContext, root: ElemPtr) -> DisplayList {
    let mut pc = PaintCtx::new();
    paint_root_into(ui, root, &mut pc);
    pc.finish()
}

/// Walk one root subtree into an existing [`PaintCtx`], for apps that compose several top-level
/// widgets (and their own chrome) into a single display list rather than one `root_window` tree.
///
/// # Safety
/// Same as [`paint_tree`]: `root` and its reachable subtree must be live widgets.
pub fn paint_root_into(ui: &UiContext, root: ElemPtr, pc: &mut PaintCtx) {
    paint_node(ui, root, pc);
}

/// Append ONLY the text of the widget subtree at `root` to `pc` — the paint walk's `Prim::Text`
/// items (per-widget content font, and the walk's container clip composed into each prim's
/// bounds). For hosts that build their frame as a [`PaintCtx`] and already emit a widget's
/// geometry another way, but want its text without re-deriving it through the legacy
/// `text_labels*` getters (the four hand-aggregate clients). The walk only reads through the
/// widget, so a shared `&dyn Element` is enough.
pub fn append_widget_text(ui: &UiContext, root: &dyn Element, pc: &mut PaintCtx) {
    // SAFETY: the walk only reads through `root` (paint_self/children/visible are all `&self`),
    // and widgets are concrete `'static` types — the invariant the toolkit's whole
    // `*mut dyn Element` tree already relies on. Erase the borrowed trait-object lifetime bound
    // to the `'static` `ElemPtr` the walk takes.
    let ptr: ElemPtr = unsafe { std::mem::transmute::<*const dyn Element, ElemPtr>(root as *const dyn Element) };
    let mut scratch = PaintCtx::new();
    paint_node(ui, ptr, &mut scratch);
    for item in scratch.finish().items {
        if let Prim::Text { text, x, y, font_size, color, font, bounds, .. } = item.prim {
            let clip = item.clip.map(|c| [c.x, c.y, c.x + c.width, c.y + c.height]);
            let merged = match (clip, bounds) {
                (Some(a), Some(b)) => Some([a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])]),
                (Some(a), None) => Some(a),
                (None, b) => b,
            };
            pc.text_with(text, x, y, font_size, color, font, merged);
        }
    }
}

/// The `Element` default `paint_self`'s LEAF branch as a reusable body: leaf geometry
/// (rounded quads, plain quads, arcs, circles) followed by the widget's fonted labels.
/// Legacy leaf widgets' `paint_self` overrides call this with their own labels — the
/// labels are PASSED IN rather than fetched through the per-widget text getters, so this
/// helper (and every override built on it) survives the getters' deletion. `pub` so
/// app-local legacy widgets (display-manager's status/session widgets, cloud's fuzzel)
/// can use it too.
pub fn paint_legacy_leaf(
    w: &dyn Element,
    ui: &UiContext,
    pc: &mut PaintCtx,
    labels: Vec<(TextLabel, Option<String>, Option<[f32; 4]>)>,
) {
    for (x, y, qw, qh, r, c, corners) in w.all_rounded_quads(ui) {
        pc.rounded_rect(Rect { x, y, width: qw, height: qh }, r, corners, c);
    }
    for (x, y, qw, qh, c) in w.all_quads(ui) {
        pc.quad(Rect { x, y, width: qw, height: qh }, c);
    }
    for (cx, cy, r, t, s, e, c) in w.extra_arcs() {
        pc.arc(cx, cy, r, t, s, e, c);
    }
    for (cx, cy, r, c) in w.extra_circles() {
        pc.circle(cx, cy, r, c);
    }
    for (tl, font, bounds) in labels {
        pc.text_with(tl.text, tl.x, tl.y, tl.font_size, tl.color, font, bounds);
    }
}

/// The scroll-ancestor text clamp the deleted default fonted getter applied. Always `None`
/// since Phase 6av: ScrollBox (the last scroll ancestor type) was demoted to a plain
/// embedded struct — it never appeared as a tree parent, so the walk never matched.
pub fn scroll_ancestor_text_bounds(_w: &dyn Element, _ui: &UiContext) -> Option<[f32; 4]> {
    None
}

/// The deleted `Element::text_labels` default's base-label synthesis: the control label
/// stored on the widget base, positioned by the configured control-label layout. For
/// legacy widgets whose only text was that label (List's columns=None frame).
pub fn base_control_label(w: &dyn Element) -> Vec<TextLabel> {
    if let Some(b) = w.base() {
        if let Some(ref label) = b.label {
            let (_, font_size) = crate::layout::control_label_font_detached_parsed();
            let color = crate::colors::control_label_color_detached_for_state(b.hovered, b.focused);
            if crate::layout::control_label_layout() == "side" {
                let label_x = w.label_x_offset();
                if label_x > 0.0 {
                    let y_pos = crate::layout::align_text_y(b.y, b.h, font_size, 0.0);
                    return vec![TextLabel { text: label.clone(), x: b.x + 4.0, y: y_pos, font_size, color }];
                }
            }
            return vec![TextLabel { text: label.clone(), x: b.x, y: b.y, font_size, color }];
        }
    }
    Vec::new()
}

/// Map a legacy leaf's own plain labels to the (label, font, bounds) triples the deleted
/// default fonted getter produced: the widget's control font on every label plus the
/// scroll-ancestor clamp.
pub fn fonted_leaf_labels(
    w: &dyn Element,
    ui: &UiContext,
    labels: Vec<TextLabel>,
) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
    let font = w.widget_font();
    let bounds = scroll_ancestor_text_bounds(w, ui);
    labels.into_iter().map(|l| (l, font.clone(), bounds)).collect()
}

fn paint_node(ui: &UiContext, ptr: ElemPtr, pc: &mut PaintCtx) {
    unsafe {
        if !(*ptr).visible() {
            return;
        }

        // Legacy subtree painters (e.g. TreeList) render their own geometry AND their children
        // through their own recursive aggregates, exposed via a paint_self override (see
        // TreeList::paint_self) — emit that and stop; the walk must not also descend.
        if (*ptr).renders_own_subtree() {
            (*ptr).paint_self(ui, pc);
            return;
        }

        (*ptr).paint_self(ui, pc);

        let children = (*ptr).children(ui);
        if children.is_empty() {
            return;
        }
        if (*ptr).clips_children() {
            let (x, y, w, h) = (*ptr).rect();
            pc.clip(Rect { x, y, width: w, height: h }, |pc| {
                for &child in &children {
                    paint_node(ui, child, pc);
                }
            });
        } else {
            for &child in &children {
                paint_node(ui, child, pc);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::paint::Prim;
    use crate::widget::Widget;

    /// A synthetic widget that paints a single quad tagged by `tag` (encoded in the red channel),
    /// so tests can assert emission order and clipping precisely. Children come from the ctx tree.
    struct P {
        base: Widget,
        tag: f32,
        clips: bool,
        vis: bool,
    }
    impl P {
        fn new(tag: f32) -> Box<P> {
            Box::new(P { base: Widget::new(), tag, clips: false, vis: true })
        }
    }
    impl Element for P {
        crate::impl_widget_base!(P);
        fn color(&self) -> [f32; 4] {
            [self.tag, 0.0, 0.0, 1.0]
        }
        fn visible(&self) -> bool {
            self.vis
        }
        fn clips_children(&self) -> bool {
            self.clips
        }
        fn paint_self(&self, _ui: &UiContext, ctx: &mut PaintCtx) {
            let (x, y, w, h) = self.rect();
            ctx.quad(Rect { x, y, width: w, height: h }, [self.tag, 0.0, 0.0, 1.0]);
        }
    }

    fn reg(ctx: &mut UiContext, w: &mut P) -> (crate::widget::WidgetId, ElemPtr) {
        let ptr = w.as_ptr_mut();
        let id = w.base.id();
        ctx.register_widget(id, ptr);
        (id, ptr)
    }

    /// Tags of the emitted quads, in order.
    fn tags(list: &DisplayList) -> Vec<f32> {
        list.items
            .iter()
            .map(|it| match it.prim {
                Prim::Quad { color, .. } => color[0],
                _ => -1.0,
            })
            .collect()
    }

    #[test]
    fn walks_parent_then_children_in_order() {
        let mut ctx = UiContext::new();
        let mut root = P::new(1.0);
        let mut a = P::new(2.0);
        let mut b = P::new(3.0);
        root.base.w = 100.0;
        root.base.h = 100.0;

        let (root_id, root_ptr) = reg(&mut ctx, &mut root);
        let (a_id, _) = reg(&mut ctx, &mut a);
        let (b_id, _) = reg(&mut ctx, &mut b);
        ctx.link_ids(root_id, a_id);
        ctx.link_ids(root_id, b_id);

        let list = paint_tree(&ctx, root_ptr);
        assert_eq!(tags(&list), vec![1.0, 2.0, 3.0], "parent, then children left-to-right");
        assert!(list.items.iter().all(|it| it.clip.is_none()), "no clipping widget => no clips");
    }

    #[test]
    fn clipping_container_clips_its_children() {
        let mut ctx = UiContext::new();
        let mut root = P::new(1.0);
        root.clips = true;
        root.base.x = 0.0;
        root.base.y = 0.0;
        root.base.w = 50.0;
        root.base.h = 50.0;
        let mut child = P::new(2.0);
        child.base.x = 10.0;
        child.base.y = 10.0;
        child.base.w = 100.0;
        child.base.h = 100.0;

        let (root_id, root_ptr) = reg(&mut ctx, &mut root);
        let (child_id, _) = reg(&mut ctx, &mut child);
        ctx.link_ids(root_id, child_id);

        let list = paint_tree(&ctx, root_ptr);
        // Root paints itself unclipped; the child is clipped to the root's rect.
        assert_eq!(list.items[0].clip, None, "root's own quad is not self-clipped");
        assert_eq!(
            list.items[1].clip,
            Some(Rect { x: 0.0, y: 0.0, width: 50.0, height: 50.0 }),
            "child clipped to the clipping container",
        );
    }

    #[test]
    fn invisible_subtree_is_skipped() {
        let mut ctx = UiContext::new();
        let mut root = P::new(1.0);
        let mut mid = P::new(2.0);
        mid.vis = false; // invisible: itself and its child must be skipped
        let mut leaf = P::new(3.0);
        let mut sibling = P::new(4.0);

        let (root_id, root_ptr) = reg(&mut ctx, &mut root);
        let (mid_id, _) = reg(&mut ctx, &mut mid);
        let (leaf_id, _) = reg(&mut ctx, &mut leaf);
        let (sib_id, _) = reg(&mut ctx, &mut sibling);
        ctx.link_ids(root_id, mid_id);
        ctx.link_ids(root_id, sib_id);
        ctx.link_ids(mid_id, leaf_id);

        let list = paint_tree(&ctx, root_ptr);
        assert_eq!(tags(&list), vec![1.0, 4.0], "mid (invisible) and its leaf are skipped");
    }

    #[test]
    fn nested_clipping_containers_intersect() {
        let mut ctx = UiContext::new();
        let mut root = P::new(1.0);
        root.clips = true;
        root.base.w = 100.0;
        root.base.h = 100.0;
        let mut inner = P::new(2.0);
        inner.clips = true;
        inner.base.x = 50.0;
        inner.base.y = 50.0;
        inner.base.w = 100.0;
        inner.base.h = 100.0;
        let mut leaf = P::new(3.0);
        leaf.base.w = 200.0;
        leaf.base.h = 200.0;

        let (root_id, root_ptr) = reg(&mut ctx, &mut root);
        let (inner_id, _) = reg(&mut ctx, &mut inner);
        let (leaf_id, _) = reg(&mut ctx, &mut leaf);
        ctx.link_ids(root_id, inner_id);
        ctx.link_ids(inner_id, leaf_id);

        let list = paint_tree(&ctx, root_ptr);
        // inner's OWN quad is clipped by its parent (root) only — its own rect clips its children,
        // not itself. The leaf, a child of inner, is clipped to inner∩root = (50,50,50,50).
        assert_eq!(list.items[1].clip, Some(Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 }));
        assert_eq!(list.items[2].clip, Some(Rect { x: 50.0, y: 50.0, width: 50.0, height: 50.0 }));
    }

    #[test]
    fn default_paint_self_emits_rounded_background() {
        // A widget using the DEFAULT paint_self (rounded corners + opaque color) emits a rounded
        // rect for its background.
        struct Rounded {
            base: Widget,
        }
        impl Element for Rounded {
            crate::impl_widget_base!(Rounded);
            fn color(&self) -> [f32; 4] {
                [0.2, 0.4, 0.6, 1.0]
            }
            fn rounded_corners(&self) -> (bool, bool, bool, bool) {
                (true, true, true, true)
            }
            fn corner_radius(&self) -> f32 {
                4.0
            }
        }
        let mut ctx = UiContext::new();
        let mut w = Rounded { base: Widget::new() };
        w.base.w = 20.0;
        w.base.h = 10.0;
        let ptr = w.as_ptr_mut();
        ctx.register_widget(w.base.id(), ptr);

        let list = paint_tree(&ctx, ptr);
        assert!(
            list.items.iter().any(|it| matches!(it.prim, Prim::RoundedRect { radius, .. } if radius == 4.0)),
            "default paint_self emits the rounded background",
        );
    }
}
