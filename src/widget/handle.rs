//! `Handle<W>` — how an app names a widget the [`UiContext`] owns.
//!
//! A handle is a `WidgetId` and the widget's type: `Copy`, cheap, and good for the widget's
//! life. It is not a reference; the widget is reached through the context
//! (`ctx.get_mut(h)`, `ctx[h]`), which is what lets the compiler keep an app's access and the
//! context's own from overlapping. A removed widget's handle resolves to `None`. See
//! `docs/rfc-owning-registry.md`.
//!
//! [`UiContext`]: crate::context::UiContext

use std::marker::PhantomData;

use super::WidgetId;

pub struct Handle<W: ?Sized> {
    id: WidgetId,
    _w: PhantomData<fn() -> W>,
}

impl<W: ?Sized> Handle<W> {
    /// The handle for the widget registered under `id` — for the context, which made it.
    pub(crate) fn from_id(id: WidgetId) -> Self {
        Handle { id, _w: PhantomData }
    }

    /// A handle that names no widget: it resolves to nothing (`get`, `lend_h`), and
    /// indexing the context with it panics. It stands in a handle field of a value built
    /// where there is no context — a page state a worker thread fetches, whose widgets the
    /// app's own copy keeps — and that field is never read. Ids start at 1, so 0 is no
    /// widget's.
    pub const fn none() -> Self {
        Handle { id: WidgetId(0), _w: PhantomData }
    }

    /// The widget's id: what focus, links, popovers and dispatch roots are keyed by.
    pub fn id(&self) -> WidgetId {
        self.id
    }
}

/// [`Handle::none`], so a state that holds handles can derive `Default` for the copy a
/// worker builds.
impl<W: ?Sized> Default for Handle<W> {
    fn default() -> Self {
        Self::none()
    }
}

impl<W: ?Sized> Clone for Handle<W> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<W: ?Sized> Copy for Handle<W> {}

impl<W: ?Sized> PartialEq for Handle<W> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl<W: ?Sized> Eq for Handle<W> {}

impl<W: ?Sized> std::hash::Hash for Handle<W> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<W: ?Sized> std::fmt::Debug for Handle<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Handle<{}>({:?})", std::any::type_name::<W>(), self.id)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use crate::context::UiContext;
    use crate::widget::{ElementState, Event, MouseButton, Slider, Widget, WidgetHost};

    /// A widget that records, each time it handles an event, whether the context would hand
    /// it out again from inside that call; and counts its drops.
    struct Reacher {
        base: Widget,
        saw_itself: Option<bool>,
        drops: Rc<Cell<u32>>,
    }
    impl Reacher {
        fn new(drops: &Rc<Cell<u32>>) -> Self {
            Reacher { base: Widget::new(), saw_itself: None, drops: drops.clone() }
        }
    }
    impl Drop for Reacher {
        fn drop(&mut self) {
            self.drops.set(self.drops.get() + 1);
        }
    }
    impl WidgetHost for Reacher {
        crate::impl_widget_base!(Reacher);
        fn handle_event(&mut self, _event: &Event, ctx: &mut UiContext) -> bool {
            let id = self.base.id();
            self.saw_itself = Some(ctx.get_widget_mut(id).is_some() || ctx.lend(id, |_, _| ()).is_some());
            true
        }
    }

    /// A widget the context owns is reached by its handle, changed through it, given back by
    /// value, and its handle then resolves to nothing; dropping the context drops the rest.
    #[test]
    fn an_owned_widget_is_reached_by_handle_and_given_back() {
        let drops = Rc::new(Cell::new(0));
        let mut ctx = UiContext::new();
        let a = ctx.insert(Reacher::new(&drops));
        let b = ctx.insert(Reacher::new(&drops));
        assert!(ctx.tree.is_registered(a.id()), "registered under its own id");
        ctx[a].saw_itself = Some(false);
        assert_eq!(ctx.get(a).and_then(|w| w.saw_itself), Some(false));
        let back = ctx.remove(a).expect("given back");
        assert_eq!(back.saw_itself, Some(false));
        assert!(ctx.get(a).is_none() && !ctx.tree.is_registered(a.id()), "a removed handle names nothing");
        drop(back);
        assert_eq!(drops.get(), 1);
        drop(ctx);
        assert_eq!(drops.get(), 2, "the context drops what it owns");
        let _ = b;
    }

    /// Routed dispatch reaches an owned widget exactly as a registered one: a press arms the
    /// drag, a move past the slop drags the slider.
    #[test]
    fn routed_dispatch_reaches_an_owned_widget() {
        let mut ctx = UiContext::new();
        let h = ctx.insert(Slider::new());
        WidgetHost::set_rect(&mut ctx[h], 0.0, 0.0, 200.0, 30.0);
        let press = Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, x: 100.0, y: 15.0, local_x: 100.0, local_y: 15.0 };
        assert!(ctx.propagate_event(&press, h.id()));
        assert!(ctx[h].is_dragging());
        let release = Event::MouseButton { button: MouseButton::Left, state: ElementState::Released, x: 150.0, y: 15.0, local_x: 150.0, local_y: 15.0 };
        ctx.propagate_event(&release, h.id());
        assert!(!ctx[h].is_dragging());
    }

    /// While the context is inside a widget it has that widget out on loan: the widget
    /// reaching for itself through the context finds nothing — a miss, not a second `&mut`.
    /// Afterwards it is back.
    #[test]
    fn a_lent_widget_is_not_reachable_until_it_is_back() {
        let drops = Rc::new(Cell::new(0));
        let mut ctx = UiContext::new();
        let h = ctx.insert(Reacher::new(&drops));
        let ev = Event::FocusIn;
        assert!(ctx.propagate_event(&ev, h.id()));
        assert_eq!(ctx[h].saw_itself, Some(false), "out on loan while handling");
        assert!(ctx.get(h).is_some() && ctx.lend(h.id(), |_, _| ()).is_some(), "back afterwards");
    }

    /// A handle that names no widget resolves to nothing, and never to a widget the
    /// context owns.
    #[test]
    fn a_none_handle_names_nothing() {
        let mut ctx = UiContext::new();
        let h = ctx.insert(Slider::new());
        let none: super::Handle<crate::widget::Adapted<Slider>> = Default::default();
        assert_ne!(none, h);
        assert!(ctx.get(none).is_none() && ctx.lend_h(none, |_, _| ()).is_none());
        assert!(ctx.get(h).is_some());
    }

    /// A widget that animates is ticked by the context once it is inserted, as one
    /// registered by pointer was, and leaves the tick list when it is removed. Without it an
    /// inserted tree list never applied its search (it does so in its tick).
    #[test]
    fn an_inserted_widget_that_ticks_is_ticked() {
        let mut ctx = UiContext::new();
        let h = ctx.insert(crate::widget::TextBox::new(String::new()));
        assert!(ctx.tick_receivers.contains(&h.id()), "a text box ticks");
        let still = ctx.insert(Slider::new());
        assert!(!ctx.tick_receivers.contains(&still.id()), "a slider does not");
        ctx.remove(h);
        assert!(!ctx.tick_receivers.contains(&h.id()), "removed, it leaves the tick list");
    }

    /// An app that rebuilds its links every frame (`clear_hierarchy`) does not hand back the
    /// widgets the context owns by doing so.
    #[test]
    fn clearing_the_hierarchy_keeps_owned_widgets() {
        let drops = Rc::new(Cell::new(0));
        let mut ctx = UiContext::new();
        let parent = ctx.insert(Reacher::new(&drops));
        let child = ctx.insert(Reacher::new(&drops));
        ctx.link_ids(parent.id(), child.id());
        ctx.clear_hierarchy();
        assert!(ctx.get(parent).is_some() && ctx.get(child).is_some());
        assert!(ctx.tree.child_ids(parent.id()).is_empty(), "the links go");
        assert_eq!(drops.get(), 0);
    }

    /// The app's access through its handle and the context's dispatch take turns on one
    /// widget, under Miri's aliasing models too (CI runs this module under Miri).
    #[test]
    fn an_app_and_the_context_take_turns_through_a_handle() {
        let drops = Rc::new(Cell::new(0));
        let mut ctx = UiContext::new();
        let h = ctx.insert(Reacher::new(&drops));
        for _ in 0..3 {
            ctx[h].saw_itself = None;
            ctx.propagate_event(&Event::FocusIn, h.id());
            let w = ctx.get_mut(h).unwrap();
            assert_eq!(w.saw_itself, Some(false));
            w.saw_itself = None;
            ctx.set_focused_id(h.id());
            ctx.clear_focus();
        }
    }
}
