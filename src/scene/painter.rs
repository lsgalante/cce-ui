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
use crate::scene::paint::{DisplayList, PaintCtx};
use crate::widget::{Element, UiContext};

type ElemPtr = *mut (dyn Element + 'static);

/// Walk the widget subtree rooted at `root` and produce its ordered, clipped [`DisplayList`].
///
/// # Safety
/// `root` and every widget reachable through `Element::children` must be live — the same
/// invariant the rest of the toolkit relies on for its `*mut dyn Element` tree.
pub fn paint_tree(ui: &UiContext, root: ElemPtr) -> DisplayList {
    let mut pc = PaintCtx::new();
    paint_node(ui, root, &mut pc);
    pc.finish()
}

fn paint_node(ui: &UiContext, ptr: ElemPtr, pc: &mut PaintCtx) {
    unsafe {
        if !(*ptr).visible() {
            return;
        }

        // Legacy subtree painters (e.g. TreeList) render their own geometry AND their children via
        // a recursive all_rounded_quads/all_quads; emit those directly and stop descending.
        if (*ptr).renders_own_subtree() {
            for (x, y, w, h, r, c, corners) in (*ptr).all_rounded_quads(ui) {
                pc.rounded_rect(Rect { x, y, width: w, height: h }, r, corners, c);
            }
            for (x, y, w, h, c) in (*ptr).all_quads(ui) {
                pc.quad(Rect { x, y, width: w, height: h }, c);
            }
            for (cx, cy, r, t, s, e, c) in (*ptr).extra_arcs() {
                pc.arc(cx, cy, r, t, s, e, c);
            }
            for (cx, cy, r, c) in (*ptr).extra_circles() {
                pc.circle(cx, cy, r, c);
            }
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
