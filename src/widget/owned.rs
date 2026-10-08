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
//! `Owned<W>` is itself a [`WidgetHost`] (every trait method forwards to the boxed widget), so it goes
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

use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;

use super::core::Liveness;
use super::{Event, LayoutConstraints, Point, Size, Widget, WidgetHost, WidgetId};
use crate::context::UiContext;

/// A widget in a heap allocation of its own, which the registry can point at however the
/// `Owned` is moved. See the module docs.
///
/// **The allocation is held as a raw pointer, not a `Box`** (since 2026-10-08). A `Box` is a
/// unique pointer to the language: every reborrow of the widget through it — each
/// `self.button.take_click()` an app makes — is a write through that unique pointer, which
/// under Rust's aliasing model invalidates every raw pointer taken from an earlier borrow.
/// The registry's pointer was one, so the next dispatch through it was undefined behaviour,
/// every frame in every app (a compiler does not exploit it today; Miri reports it). Now the
/// app's `Deref` and the registry both derive their references from the same raw root, and
/// neither invalidates the other; what remains is the rule every `&mut` already obeys — one
/// reached through the registry must not overlap one taken through the `Owned`, which a
/// `UiContext` call that is handed the widget honours by re-deriving its pointer from what
/// it is handed (`UiContext::set_focused`). `an_app_and_the_registry_take_turns_soundly` is
/// the check, run under Miri in CI.
pub struct Owned<W: WidgetHost + 'static> {
    // Dropped in `Drop` before the allocation is freed: the registry stops resolving the
    // widget before the widget's own `Drop` runs.
    live: ManuallyDrop<Liveness>,
    widget: NonNull<W>,
    _owns: std::marker::PhantomData<W>,
}

// What a `Box<W>` is: the `Owned` owns its `W` outright.
unsafe impl<W: WidgetHost + Send + 'static> Send for Owned<W> {}
unsafe impl<W: WidgetHost + Sync + 'static> Sync for Owned<W> {}

impl<W: WidgetHost + 'static> Owned<W> {
    pub fn new(widget: W) -> Self {
        let widget = Box::into_non_null(Box::new(widget));
        Owned { live: ManuallyDrop::new(Liveness::new()), widget, _owns: std::marker::PhantomData }
    }

    /// The widget, by value; the allocation and its token go with the `Owned`.
    pub fn into_inner(self) -> W {
        let mut this = ManuallyDrop::new(self);
        // SAFETY: `this` is never used or dropped again; the token is dropped (and the
        // registry stops resolving the widget) before the allocation is taken back.
        unsafe {
            ManuallyDrop::drop(&mut this.live);
            *Box::from_raw(this.widget.as_ptr())
        }
    }
}

impl<W: WidgetHost + 'static> Drop for Owned<W> {
    fn drop(&mut self) {
        // SAFETY: dropped exactly once, here; the allocation was `Box`-made in `new` and is
        // freed only here, after the token that lets the registry resolve it is gone.
        unsafe {
            ManuallyDrop::drop(&mut self.live);
            drop(Box::from_raw(self.widget.as_ptr()));
        }
    }
}

impl<W: WidgetHost + 'static> Deref for Owned<W> {
    type Target = W;
    fn deref(&self) -> &W {
        // SAFETY: the allocation is live while `self` is, and holds a valid `W`.
        unsafe { self.widget.as_ref() }
    }
}

impl<W: WidgetHost + 'static> DerefMut for Owned<W> {
    fn deref_mut(&mut self) -> &mut W {
        // SAFETY: as in `deref`; `&mut self` is the app's exclusive access.
        unsafe { self.widget.as_mut() }
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
        Owned::new((**self).clone())
    }
}

impl<W: WidgetHost + Default + 'static> Default for Owned<W> {
    fn default() -> Self {
        Owned::new(W::default())
    }
}

impl<W: WidgetHost + std::fmt::Debug + 'static> std::fmt::Debug for Owned<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Owned").field(&**self).finish()
    }
}

/// Every method forwards to the boxed widget, so an `Owned` behaves exactly as the widget does;
/// the one addition is [`stable_target`](WidgetHost::stable_target).
impl<W: WidgetHost + 'static> WidgetHost for Owned<W> {
    fn layout_model(&self) -> &dyn crate::widget::Layout { (**self).layout_model() }
    fn paint_model(&self) -> &dyn crate::widget::Paint { (**self).paint_model() }
    fn input_model(&self) -> &dyn crate::widget::Input { (**self).input_model() }
    fn input_model_mut(&mut self) -> &mut dyn crate::widget::Input { (**self).input_model_mut() }
    fn stable_target(&mut self) -> Option<(*mut (dyn WidgetHost + 'static), std::sync::Weak<()>)> {
        // The raw root itself, not a pointer taken from a reborrow (see the type's docs).
        let ptr: *mut (dyn WidgetHost + 'static) = self.widget.as_ptr();
        Some((ptr, self.live.watch()))
    }

    fn base(&self) -> &Widget { (**self).base() }
    fn base_mut(&mut self) -> &mut Widget { (**self).base_mut() }
    fn label_strip(&self) -> f32 { (**self).label_strip() }
    fn detached_label_rect(&self) -> Option<crate::scene::layout::Rect> { (**self).detached_label_rect() }
    fn as_any(&self) -> &dyn std::any::Any { (**self).as_any() }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { (**self).as_any_mut() }
    fn handle_event(&mut self, event: &Event, ctx: &mut UiContext) -> bool { (**self).handle_event(event, ctx) }
    fn measure(&self, constraints: LayoutConstraints, ctx: &UiContext) -> Size { (**self).measure(constraints, ctx) }
    fn layout(&mut self, origin: Point, constraints: LayoutConstraints, ctx: &mut UiContext) { (**self).layout(origin, constraints, ctx) }
    fn rect(&self) -> (f32, f32, f32, f32) { (**self).rect() }
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) { (**self).set_rect(x, y, w, h) }
    fn set_row_rect(&mut self, x: f32, w: f32) { (**self).set_row_rect(x, w) }
    fn hit_test(&self, px: f32, py: f32, ctx: &UiContext) -> bool { (**self).hit_test(px, py, ctx) }
    fn highlight_quad(&self, ctx: &UiContext) -> Option<(f32, f32, f32, f32, [f32; 4])> { (**self).highlight_quad(ctx) }
    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> { (**self).extra_quads() }
    fn extra_arcs(&self) -> Vec<(f32, f32, f32, f32, f32, f32, [f32; 4])> { (**self).extra_arcs() }
    fn extra_circles(&self) -> Vec<(f32, f32, f32, [f32; 4])> { (**self).extra_circles() }
    fn all_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, [f32; 4])> { (**self).all_quads(ctx) }
    fn paint_self(&self, ui: &UiContext, ctx: &mut crate::scene::paint::PaintCtx) { (**self).paint_self(ui, ctx) }
    fn all_rounded_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, f32, [f32; 4], (bool, bool, bool, bool))> {
        (**self).all_rounded_quads(ctx)
    }
    fn type_name(&self) -> &'static str { (**self).type_name() }
    fn popover_rect(&self) -> Option<(f32, f32, f32, f32)> { (**self).popover_rect() }
    fn render_popover(&self, pc: &mut dyn crate::layout::RenderTarget) { (**self).render_popover(pc) }
    fn focus(&mut self) { (**self).focus() }
    fn unfocus(&mut self) { (**self).unfocus() }
    fn focused(&self, ctx: &UiContext) -> bool { (**self).focused(ctx) }
    fn prepare_text(&mut self, fs: &mut cosmic_text::FontSystem) { (**self).prepare_text(fs) }
    fn set_visible(&mut self, visible: bool) { (**self).set_visible(visible) }
    fn visible(&self) -> bool { (**self).visible() }
    fn tick(&mut self, dt: f32, ctx: &mut UiContext) -> bool { (**self).tick(dt, ctx) }
    fn is_child_visible(&self, child_id: WidgetId) -> bool { (**self).is_child_visible(child_id) }
    fn corner_style(&self) -> (f32, (bool, bool, bool, bool)) { (**self).corner_style() }
    fn a11y_items(&self) -> Vec<crate::a11y::A11yItem> { (**self).a11y_items() }
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

    /// The app and the registry take turns on one widget, as every frame of every app does:
    /// the app changes it through the `Owned`, the registry reads and writes it through its
    /// pointer, and back. With the widget in a `Box`, each app access invalidated the
    /// registry's pointer under Rust's aliasing rules; run under Miri (`miri` in CI), this is
    /// the check that neither way in invalidates the other.
    #[test]
    fn an_app_and_the_registry_take_turns_soundly() {
        let mut w = tag(0);
        let id = w.base().id();
        let mut tree = WidgetTree::new();
        unsafe { tree.register(id, &mut w as &mut (dyn WidgetHost + 'static)) };
        for i in 1..=4u32 {
            w.n += 1; // the app, through `DerefMut`
            let p = tree.get_ptr(id).unwrap();
            // SAFETY: the registry resolved it; no app reference is live across this.
            unsafe { (*p).as_any_mut().downcast_mut::<Tag>().unwrap().n += 10 };
            assert_eq!(w.n, i * 11, "the app sees what the registry wrote");
            assert_eq!(read(&tree, id), Some(w.n), "and the registry what the app wrote");
        }
        let moved = w; // moving the `Owned` moves no widget
        assert_eq!(read(&tree, id), Some(44));
        drop(moved);
        assert_eq!(read(&tree, id), None, "dropped: the registry no longer resolves it");
    }

    /// A registration through a pointer to the boxed widget itself — what a widget mid-event
    /// hands over to open its context menu or take focus — keeps the box's root: that
    /// pointer is a reborrow the next app access invalidates. The app and the registry then
    /// still take turns soundly (under Miri, the check).
    #[test]
    fn an_inner_registration_keeps_the_root() {
        let mut w = tag(1);
        let id = w.base().id();
        let mut tree = WidgetTree::new();
        unsafe { tree.register(id, &mut w as &mut (dyn WidgetHost + 'static)) };
        let root = tree.get_ptr(id).unwrap();
        let inner: &mut Tag = &mut w;
        unsafe { tree.register(id, inner as &mut (dyn WidgetHost + 'static)) };
        assert!(std::ptr::addr_eq(tree.get_ptr(id).unwrap(), root), "the root is kept");
        for i in 2..=4u32 {
            w.n = i; // the app
            assert_eq!(read(&tree, id), Some(i), "the registry, through the kept root");
        }
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
        assert_eq!(owned.base().id(), unsafe { owned.widget.as_ref() }.base().id());
        assert!(owned.as_any().downcast_ref::<Tag>().is_some(), "as_any reaches the widget");
        let copy = owned.clone();
        let (a, _) = owned.stable_target().unwrap();
        let mut copy = copy;
        let (b, _) = copy.stable_target().unwrap();
        assert_ne!(a as *const () as usize, b as *const () as usize, "a clone has its own box");
    }
}
