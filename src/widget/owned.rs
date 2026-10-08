//! `Owned<W>` — a widget at an address that does not move, for the registry to point at.
//!
//! `UiContext` keeps raw pointers to the widgets an app registers, and the app owns those
//! widgets as struct fields and `Vec` elements. Two things can leave such a pointer naming
//! something that is no longer the widget:
//!
//! - the widget is DROPPED (a rebuilt list whose old rows were not unregistered) — caught by
//!   the widget's own liveness token (`widget::core::Liveness`) since cce-ui@1d538c6;
//! - the widget is MOVED (the `Vec` it sits in reallocates, the struct holding it is returned
//!   by value) — its token moves with it, so the registry cannot tell, and the next sweep reads
//!   freed or reused memory.
//!
//! `Owned` closes the second. It keeps the widget in a heap allocation of its own and carries a
//! liveness token for that ALLOCATION. Moving an `Owned` moves the box pointer, not the widget;
//! the allocation is freed only when the `Owned` drops, which is exactly when its token dies. The
//! widget can be reached and changed through `Deref`/`DerefMut` — even swapped for another `W`
//! with `mem::swap` — but the allocation always holds a valid `W` while the token lives. So a
//! pointer the registry resolves through an `Owned`'s token always names a live `W`.
//!
//! `Owned<W>` is itself a [`WidgetHost`] (every method forwards to the boxed widget), so it goes
//! wherever a widget went: `register_host(&mut self.button)`, `set_focused`, `render_widget`,
//! `link_parent_child`. Each of those registers through `WidgetTree::register`, which asks
//! [`WidgetHost::stable_target`] and, for an `Owned`, stores the BOXED widget and the
//! allocation's token rather than the `Owned` itself.
//!
//! ```ignore
//! struct App { search: Owned<Adapted<TextBox>>, rows: Vec<Owned<Adapted<Button>>>, ui: UiContext }
//! let search = Owned::new(TextBox::new(""));          // or `TextBox::new("").into()`
//! app.ui.register_host(&mut app.search);              // the Vec may reallocate freely now
//! app.search.set_text("x");                           // Deref: the widget's own methods
//! ```

use std::ops::{Deref, DerefMut};

use super::core::Liveness;
use super::{ContextAction, CornerRadii, Event, FocusRole, LayoutConstraints, Point, Size, Widget, WidgetHost, WidgetId};
use crate::context::UiContext;

/// A widget in a heap allocation of its own, which the registry can point at however the
/// `Owned` is moved. See the module docs.
pub struct Owned<W: WidgetHost + 'static> {
    // Declared first so it is dropped first: the registry stops resolving the widget before
    // the widget's own `Drop` runs and the allocation is freed.
    live: Liveness,
    widget: Box<W>,
}

impl<W: WidgetHost + 'static> Owned<W> {
    pub fn new(widget: W) -> Self {
        Owned { live: Liveness::new(), widget: Box::new(widget) }
    }

    /// The widget, by value; the allocation and its token go with the `Owned`.
    pub fn into_inner(self) -> W {
        let Owned { live, widget } = self;
        drop(live);
        *widget
    }
}

impl<W: WidgetHost + 'static> Deref for Owned<W> {
    type Target = W;
    fn deref(&self) -> &W {
        &self.widget
    }
}

impl<W: WidgetHost + 'static> DerefMut for Owned<W> {
    fn deref_mut(&mut self) -> &mut W {
        &mut self.widget
    }
}

impl<W: WidgetHost + 'static> From<W> for Owned<W> {
    fn from(widget: W) -> Self {
        Owned::new(widget)
    }
}

/// A clone is a different widget in a different allocation, with a token of its own.
impl<W: WidgetHost + Clone + 'static> Clone for Owned<W> {
    fn clone(&self) -> Self {
        Owned::new((*self.widget).clone())
    }
}

impl<W: WidgetHost + Default + 'static> Default for Owned<W> {
    fn default() -> Self {
        Owned::new(W::default())
    }
}

impl<W: WidgetHost + std::fmt::Debug + 'static> std::fmt::Debug for Owned<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Owned").field(&*self.widget).finish()
    }
}

/// Every method forwards to the boxed widget, so an `Owned` behaves exactly as the widget does;
/// the one addition is [`stable_target`](WidgetHost::stable_target).
impl<W: WidgetHost + 'static> WidgetHost for Owned<W> {
    fn stable_target(&mut self) -> Option<(*mut (dyn WidgetHost + 'static), std::sync::Weak<()>)> {
        let ptr: *mut (dyn WidgetHost + 'static) = &mut *self.widget as *mut W;
        Some((ptr, self.live.watch()))
    }

    fn base(&self) -> &Widget { self.widget.base() }
    fn base_mut(&mut self) -> &mut Widget { self.widget.base_mut() }
    fn preferred_height(&self) -> Option<f32> { self.widget.preferred_height() }
    fn label_strip(&self) -> f32 { self.widget.label_strip() }
    fn detached_label_rect(&self) -> Option<crate::scene::layout::Rect> { self.widget.detached_label_rect() }
    fn mark_dirty(&mut self, ctx: &mut UiContext) { self.widget.mark_dirty(ctx) }
    fn as_any(&self) -> &dyn std::any::Any { self.widget.as_any() }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self.widget.as_any_mut() }
    fn handle_event(&mut self, event: &Event, ctx: &mut UiContext) -> bool { self.widget.handle_event(event, ctx) }
    fn measure(&self, constraints: LayoutConstraints, ctx: &UiContext) -> Size { self.widget.measure(constraints, ctx) }
    fn layout(&mut self, origin: Point, constraints: LayoutConstraints, ctx: &mut UiContext) { self.widget.layout(origin, constraints, ctx) }
    fn rect(&self) -> (f32, f32, f32, f32) { self.widget.rect() }
    fn label(&self) -> Option<String> { self.widget.label() }
    fn context_action(&mut self, action: ContextAction) -> bool { self.widget.context_action(action) }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { self.widget.set_rect(x, y, w, h) }
    fn set_row_rect(&mut self, x: f32, w: f32) { self.widget.set_row_rect(x, w) }
    fn hit_test(&self, px: f32, py: f32, ctx: &UiContext) -> bool { self.widget.hit_test(px, py, ctx) }
    fn highlight_quad(&self, ctx: &UiContext) -> Option<(f32, f32, f32, f32, [f32; 4])> { self.widget.highlight_quad(ctx) }
    fn color(&self) -> [f32; 4] { self.widget.color() }
    fn solid_border(&self) -> Option<([f32; 4], f32)> { self.widget.solid_border() }
    fn plate_bevel(&self) -> Option<f32> { self.widget.plate_bevel() }
    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> { self.widget.extra_quads() }
    fn extra_arcs(&self) -> Vec<(f32, f32, f32, f32, f32, f32, [f32; 4])> { self.widget.extra_arcs() }
    fn extra_circles(&self) -> Vec<(f32, f32, f32, [f32; 4])> { self.widget.extra_circles() }
    fn all_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, [f32; 4])> { self.widget.all_quads(ctx) }
    fn paint_self(&self, ui: &UiContext, ctx: &mut crate::scene::paint::PaintCtx) { self.widget.paint_self(ui, ctx) }
    fn clips_children(&self) -> bool { self.widget.clips_children() }
    fn renders_own_subtree(&self) -> bool { self.widget.renders_own_subtree() }
    fn all_rounded_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, f32, [f32; 4], (bool, bool, bool, bool))> {
        self.widget.all_rounded_quads(ctx)
    }
    fn widget_font(&self) -> Option<String> { self.widget.widget_font() }
    fn type_name(&self) -> &'static str { self.widget.type_name() }
    fn popover_rect(&self) -> Option<(f32, f32, f32, f32)> { self.widget.popover_rect() }
    fn render_popover(&self, pc: &mut dyn crate::layout::RenderTarget) { self.widget.render_popover(pc) }
    fn focus(&mut self) { self.widget.focus() }
    fn unfocus(&mut self) { self.widget.unfocus() }
    fn focused(&self, ctx: &UiContext) -> bool { self.widget.focused(ctx) }
    fn prepare_text(&mut self, fs: &mut cosmic_text::FontSystem) { self.widget.prepare_text(fs) }
    fn set_visible(&mut self, visible: bool) { self.widget.set_visible(visible) }
    fn visible(&self) -> bool { self.widget.visible() }
    fn tick(&mut self, dt: f32, ctx: &mut UiContext) -> bool { self.widget.tick(dt, ctx) }
    fn wants_tick(&self) -> bool { self.widget.wants_tick() }
    fn is_child_visible(&self, child_id: WidgetId) -> bool { self.widget.is_child_visible(child_id) }
    fn set_modifiers(&mut self, ctrl: bool, shift: bool, alt: bool) { self.widget.set_modifiers(ctrl, shift, alt) }
    fn z_index(&self) -> i32 { self.widget.z_index() }
    fn is_scrollable(&self) -> bool { self.widget.is_scrollable() }
    fn blocks_root_plate_drag(&self) -> bool { self.widget.blocks_root_plate_drag() }
    fn corner_style(&self) -> (f32, (bool, bool, bool, bool)) { self.widget.corner_style() }
    fn focus_role(&self) -> FocusRole { self.widget.focus_role() }
    fn corner_radii(&self) -> CornerRadii { self.widget.corner_radii() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::tree::WidgetTree;

    #[derive(Clone)]
    struct Tag {
        base: Widget,
        n: u32,
    }
    impl WidgetHost for Tag {
        crate::impl_widget_base!(Tag);
        fn color(&self) -> [f32; 4] {
            [0.0; 4]
        }
    }
    fn tag(n: u32) -> Owned<Tag> {
        Owned::new(Tag { base: Widget::new(), n })
    }
    /// What the registry hands back for `id`, read as a `Tag`.
    fn read(tree: &WidgetTree, id: WidgetId) -> Option<u32> {
        let p = tree.get_ptr(id)?;
        // SAFETY: the registry resolved it, which is what is under test.
        Some(unsafe { (*p).as_any().downcast_ref::<Tag>().unwrap().n })
    }

    #[test]
    fn an_owned_widget_survives_its_vec_reallocating() {
        // The hole `Owned` closes: rows registered, then the Vec grows and moves them.
        let mut rows: Vec<Owned<Tag>> = Vec::with_capacity(1);
        rows.push(tag(7));
        let id = rows[0].base().id();
        let mut tree = WidgetTree::new();
        unsafe { tree.register(id, &mut rows[0] as &mut (dyn WidgetHost + 'static)) };
        let before = tree.get_ptr(id).unwrap();
        for n in 0..64 {
            rows.push(tag(n)); // reallocates, moving every `Owned` — but not what they own
        }
        assert_eq!(tree.get_ptr(id).map(|p| p as *const () as usize), Some(before as *const () as usize));
        assert_eq!(read(&tree, id), Some(7));
        let moved = rows.remove(0); // moved out of the Vec entirely
        assert_eq!(read(&tree, id), Some(7));
        drop(moved);
        assert_eq!(tree.get_ptr(id), None, "dropping the Owned ends it");
    }

    #[test]
    fn swapping_the_widget_out_of_its_box_never_leaves_a_dangling_entry() {
        // The widget's OWN token would say "alive" after it was swapped out and its box freed;
        // the allocation's token is what the registry watches.
        let mut owned = tag(1);
        let id = owned.base().id();
        let mut tree = WidgetTree::new();
        unsafe { tree.register(id, &mut owned as &mut (dyn WidgetHost + 'static)) };
        let mut outside = Tag { base: Widget::new(), n: 2 };
        std::mem::swap(&mut *owned, &mut outside);
        assert_eq!(read(&tree, id), Some(2), "the box holds a valid Tag, the swapped-in one");
        drop(owned);
        assert_eq!(outside.n, 1, "the original lives on outside the box");
        assert_eq!(tree.get_ptr(id), None, "but the box it was registered in is gone");
    }

    #[test]
    fn an_owned_widget_is_the_widget() {
        let mut owned = tag(3);
        owned.set_rect(1.0, 2.0, 3.0, 4.0);
        assert_eq!(WidgetHost::rect(&*owned), (1.0, 2.0, 3.0, 4.0));
        assert_eq!(WidgetHost::rect(&owned), (1.0, 2.0, 3.0, 4.0));
        assert_eq!(owned.base().id(), owned.widget.base().id());
        assert!(owned.as_any().downcast_ref::<Tag>().is_some(), "as_any reaches the widget");
        let copy = owned.clone();
        let (a, _) = owned.stable_target().unwrap();
        let mut copy = copy;
        let (b, _) = copy.stable_target().unwrap();
        assert_ne!(a as *const () as usize, b as *const () as usize, "a clone has its own box");
    }
}
