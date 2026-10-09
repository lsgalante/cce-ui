//! `Embedded<W>` — a child widget a composite holds (a tree list's search box, a paginator's
//! strip), the context's like every other widget once the composite is.
//!
//! A composite is built before any context exists (`TreeList::new()`), so its children start
//! out held by value. When the composite is inserted into a [`UiContext`], the context calls
//! its [`Layout::register_embedded_children`](crate::widget::Layout::register_embedded_children)
//! hook, which [`attach`](Embedded::attach)es each child: it moves into the context as an
//! entry of its own and the cell keeps its [`Handle`]. From then on the composite reaches it
//! through the context it is handed (an event's, a tick's, `paint_ui`'s), while the composite
//! itself is out on loan and the child is not — so neither can be reached twice. When the
//! composite is removed, [`Layout::release_embedded_children`](crate::widget::Layout::release_embedded_children)
//! [`detach`](Embedded::detach)es them, and the composite comes back whole, its children in it.
//! See `docs/rfc-owning-registry.md`, phase 4.

use super::{Handle, WidgetHost, WidgetId};
use crate::context::UiContext;

enum Slot<W> {
    /// Held here: the composite is not in a context (yet, or any more).
    Here(Box<W>),
    /// In the context, by handle.
    There(Handle<W>),
}

pub struct Embedded<W: WidgetHost + 'static> {
    id: WidgetId,
    slot: Slot<W>,
}

impl<W: WidgetHost + 'static> Embedded<W> {
    /// A child held by value until its composite is inserted.
    pub fn new(widget: W) -> Self {
        Embedded { id: widget.base().id(), slot: Slot::Here(Box::new(widget)) }
    }

    /// The child's id: the same before and after it moves into the context.
    pub fn id(&self) -> WidgetId {
        self.id
    }

    /// Its handle, once it is in a context.
    pub fn handle(&self) -> Option<Handle<W>> {
        match self.slot {
            Slot::There(h) => Some(h),
            Slot::Here(_) => None,
        }
    }

    pub fn is_attached(&self) -> bool {
        matches!(self.slot, Slot::There(_))
    }

    /// Move the child into `ctx` (nothing if it is there already).
    pub fn attach(&mut self, ctx: &mut UiContext) {
        if let Slot::Here(_) = self.slot {
            let Slot::Here(w) = std::mem::replace(&mut self.slot, Slot::There(Handle::none())) else { unreachable!() };
            self.slot = Slot::There(ctx.insert(*w));
        }
    }

    /// Take the child back out of `ctx` (nothing if it is held here). A child the context no
    /// longer has, or has out on loan, stays named by its handle.
    pub fn detach(&mut self, ctx: &mut UiContext) {
        if let Slot::There(h) = self.slot {
            if let Some(w) = ctx.remove(h) {
                self.slot = Slot::Here(Box::new(w));
            }
        }
    }

    /// The child, wherever it is.
    ///
    /// # Panics
    /// If it is in a context that no longer has it, or has it out on loan.
    pub fn get<'a>(&'a self, ctx: &'a UiContext) -> &'a W {
        match &self.slot {
            Slot::Here(w) => w,
            Slot::There(h) => &ctx[*h],
        }
    }

    /// [`get`](Self::get), mutably.
    pub fn get_mut<'a>(&'a mut self, ctx: &'a mut UiContext) -> &'a mut W {
        match &mut self.slot {
            Slot::Here(w) => w,
            Slot::There(h) => &mut ctx[*h],
        }
    }

    /// The child while it is held here (a composite outside any context, a test).
    pub fn here(&self) -> Option<&W> {
        match &self.slot {
            Slot::Here(w) => Some(w),
            Slot::There(_) => None,
        }
    }

    /// [`here`](Self::here), mutably.
    pub fn here_mut(&mut self) -> Option<&mut W> {
        match &mut self.slot {
            Slot::Here(w) => Some(w),
            Slot::There(_) => None,
        }
    }

    /// The child and the context together, for a call that needs both (forwarding an
    /// event, focusing): it is attached first, and lent for the call. `None` if the context
    /// has it out on loan already.
    pub fn lend<R>(&mut self, ctx: &mut UiContext, f: impl FnOnce(&mut W, &mut UiContext) -> R) -> Option<R> {
        self.attach(ctx);
        let Slot::There(h) = self.slot else { unreachable!() };
        ctx.lend_h(h, f)
    }
}

impl<W: WidgetHost + 'static> std::fmt::Debug for Embedded<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.slot {
            Slot::Here(_) => write!(f, "Embedded::Here({:?})", self.id),
            Slot::There(h) => write!(f, "Embedded::There({h:?})"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::{Adapted, Slider};

    /// A child held by value moves into the context on attach, keeps its id, is reached
    /// through the context there, and comes back on detach.
    #[test]
    fn a_child_moves_in_and_back_out() {
        let mut ctx = UiContext::new();
        let mut child: Embedded<Adapted<Slider>> = Embedded::new(Slider::new());
        let id = child.id();
        child.here_mut().unwrap().set_value(0.25);
        child.attach(&mut ctx);
        assert!(child.is_attached() && ctx.tree.is_registered(id), "in the context under its own id");
        assert!(child.here().is_none());
        assert_eq!(child.get(&ctx).value(), 25);
        child.get_mut(&mut ctx).set_value(0.5);
        child.attach(&mut ctx);
        assert_eq!(child.handle().map(|h| h.id()), Some(id), "a second attach changes nothing");
        child.detach(&mut ctx);
        assert!(!ctx.tree.is_registered(id), "out of the context again");
        assert_eq!(child.here().map(|w| w.value()), Some(50));
    }

    /// A child lent for a call is out of the context's reach until the call returns.
    #[test]
    fn a_lent_child_is_not_reached_twice() {
        let mut ctx = UiContext::new();
        let mut child: Embedded<Adapted<Slider>> = Embedded::new(Slider::new());
        let id = child.id();
        let inside = child.lend(&mut ctx, |_, ctx| ctx.lend(id, |_, _| ()).is_none());
        assert_eq!(inside, Some(true), "out on loan while lent");
        assert!(ctx.lend(id, |_, _| ()).is_some(), "back afterwards");
    }
}
