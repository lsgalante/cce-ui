//! Bridge between the live widget tree and the scene layout engine — Phase 2b of the core
//! rebuild.
//!
//! [`scene::layout`](crate::scene::layout) is a pure solver over `Arena<LayoutBox>`; the running
//! UI is a tree of `Box<dyn Element>` widgets linked through [`UiContext`]. This module connects
//! them: given a widget subtree, it builds a throwaway `LayoutBox` arena mirroring that subtree,
//! runs measure/arrange, and writes the resulting rects back into the widgets.
//!
//! Migration is incremental. A widget opts in by returning `Some` from
//! [`Element::layout_style`]; leaf widgets report size via [`Element::intrinsic_size`]. Widgets
//! that return `None` keep their legacy `set_rect` path untouched — so a single container can be
//! moved onto the engine and verified in a running app without disturbing the rest.
//!
//! Structure comes from each widget's own [`Element::children`], so it respects widgets that
//! store children locally as well as the default `UiContext`-backed tree.

use crate::scene::arena::{Arena, NodeId};
use crate::scene::layout::{self, LayoutBox, Rect};
use crate::widget::{Element, UiContext};

type ElemPtr = *mut (dyn Element + 'static);

/// Lay out the widget subtree rooted at `root` into `area` using the scene layout engine, writing
/// the computed rect into every widget via [`Element::set_rect`].
///
/// Widgets are written in pre-order (parent before children), so a container's own rect is set
/// before the engine-computed rects of its children overwrite anything the container's `set_rect`
/// might have done.
///
/// # Safety
/// `root` must be a valid, live widget pointer, and the subtree reachable through
/// `Element::children` must consist of live widgets — the same invariant the rest of the toolkit
/// relies on for its `*mut dyn Element` tree.
pub fn layout_subtree(ctx: &UiContext, root: ElemPtr, area: Rect) {
    let mut arena: Arena<LayoutBox> = Arena::new();
    let mut order: Vec<(NodeId, ElemPtr)> = Vec::new();
    let root_node = build(&mut arena, ctx, root, &mut order);

    layout::measure(&mut arena, root_node);
    layout::arrange(&mut arena, root_node, area);

    for &(node, ptr) in &order {
        let r = arena.value(node).expect("layout node missing").rect;
        unsafe {
            (*ptr).set_rect(r.x, r.y, r.width, r.height);
        }
    }
}

/// Recursively mirror the widget subtree into `arena`, recording (node, widget) pairs in pre-order.
fn build(arena: &mut Arena<LayoutBox>, ctx: &UiContext, ptr: ElemPtr, order: &mut Vec<(NodeId, ElemPtr)>) -> NodeId {
    let (style, intrinsic, children) = unsafe {
        let style = (*ptr).layout_style().unwrap_or_default();
        let intrinsic = (*ptr).intrinsic_size();
        let children = (*ptr).children(ctx);
        (style, intrinsic, children)
    };

    // A node with children is a container; a childless node is a leaf sized by its intrinsic size.
    let node = if children.is_empty() {
        match intrinsic {
            Some(size) => arena.insert(LayoutBox::leaf(style, size)),
            None => arena.insert(LayoutBox::container(style)),
        }
    } else {
        arena.insert(LayoutBox::container(style))
    };
    order.push((node, ptr));

    for child in children {
        let child_node = build(arena, ctx, child, order);
        arena.append_child(node, child_node);
    }
    node
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::layout::{CrossAlign, Size, Style};
    use crate::widget::{Widget, WidgetId};

    /// A minimal real widget: carries a `Widget` base (so it has an id and a settable rect) and an
    /// opt-in layout style / intrinsic size.
    struct W {
        base: Widget,
        style: Option<Style>,
        intrinsic: Option<Size>,
    }
    impl W {
        fn container(style: Style) -> Box<W> {
            Box::new(W { base: Widget::new(), style: Some(style), intrinsic: None })
        }
        fn leaf(w: f32, h: f32) -> Box<W> {
            Box::new(W { base: Widget::new(), style: None, intrinsic: Some(Size::new(w, h)) })
        }
    }
    impl Element for W {
        crate::impl_widget_base!(W);
        fn color(&self) -> [f32; 4] {
            [0.0, 0.0, 0.0, 0.0]
        }
        fn layout_style(&self) -> Option<Style> {
            self.style
        }
        fn intrinsic_size(&self) -> Option<Size> {
            self.intrinsic
        }
    }

    /// Register a widget in `ctx` and return its (id, ptr).
    fn reg(ctx: &mut UiContext, w: &mut W) -> (WidgetId, ElemPtr) {
        let id = w.base.id();
        let ptr: ElemPtr = w as *mut W;
        ctx.register_widget(id, ptr);
        (id, ptr)
    }

    fn rect_of(ptr: ElemPtr) -> Rect {
        let (x, y, w, h) = unsafe { (*ptr).rect() };
        Rect { x, y, width: w, height: h }
    }

    #[test]
    fn drives_a_real_column_of_leaves() {
        let mut ctx = UiContext::new();
        // Keep the boxes alive for the duration; hand the tree raw pointers into them.
        let mut root = W::container(Style::column().gap(4.0));
        let mut a = W::leaf(10.0, 10.0);
        let mut b = W::leaf(10.0, 20.0);

        let (root_id, root_ptr) = reg(&mut ctx, &mut root);
        let (a_id, a_ptr) = reg(&mut ctx, &mut a);
        let (b_id, b_ptr) = reg(&mut ctx, &mut b);
        ctx.link_ids(root_id, a_id);
        ctx.link_ids(root_id, b_id);

        layout_subtree(&ctx, root_ptr, Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 });

        assert_eq!(rect_of(root_ptr), Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 });
        assert_eq!(rect_of(a_ptr), Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 });
        assert_eq!(rect_of(b_ptr), Rect { x: 0.0, y: 14.0, width: 10.0, height: 20.0 });
    }

    #[test]
    fn lays_out_at_a_nonzero_origin_with_padding() {
        let mut ctx = UiContext::new();
        let mut root = W::container(Style::row().padding(5.0));
        let mut a = W::leaf(10.0, 10.0);

        let (root_id, root_ptr) = reg(&mut ctx, &mut root);
        let (a_id, a_ptr) = reg(&mut ctx, &mut a);
        ctx.link_ids(root_id, a_id);

        // Subtree lives at (20, 30), not the origin.
        layout_subtree(&ctx, root_ptr, Rect { x: 20.0, y: 30.0, width: 100.0, height: 50.0 });

        assert_eq!(rect_of(root_ptr), Rect { x: 20.0, y: 30.0, width: 100.0, height: 50.0 });
        // child inset by padding from the container's origin.
        assert_eq!(rect_of(a_ptr), Rect { x: 25.0, y: 35.0, width: 10.0, height: 10.0 });
    }

    #[test]
    fn drives_a_nested_subtree() {
        let mut ctx = UiContext::new();
        let mut root = W::container(Style::row());
        let mut inner = W::container(Style::column().gap(2.0).cross_align(CrossAlign::Start));
        let mut c1 = W::leaf(10.0, 10.0);
        let mut c2 = W::leaf(10.0, 10.0);
        let mut sibling = W::leaf(5.0, 5.0);

        let (root_id, root_ptr) = reg(&mut ctx, &mut root);
        let (inner_id, inner_ptr) = reg(&mut ctx, &mut inner);
        let (c1_id, c1_ptr) = reg(&mut ctx, &mut c1);
        let (c2_id, c2_ptr) = reg(&mut ctx, &mut c2);
        let (sib_id, sib_ptr) = reg(&mut ctx, &mut sibling);
        ctx.link_ids(root_id, inner_id);
        ctx.link_ids(root_id, sib_id);
        ctx.link_ids(inner_id, c1_id);
        ctx.link_ids(inner_id, c2_id);

        layout_subtree(&ctx, root_ptr, Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 });

        assert_eq!(rect_of(inner_ptr), Rect { x: 0.0, y: 0.0, width: 10.0, height: 22.0 });
        assert_eq!(rect_of(c1_ptr), Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 });
        assert_eq!(rect_of(c2_ptr), Rect { x: 0.0, y: 12.0, width: 10.0, height: 10.0 });
        assert_eq!(rect_of(sib_ptr).x, 10.0, "sibling follows inner's measured width");
    }
}
