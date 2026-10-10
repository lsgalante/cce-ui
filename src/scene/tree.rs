//! `WidgetTree` — `UiContext`'s widget tree (`UiContext::tree`): the widgets themselves and
//! their parent/child links, in one generational [`Arena`] keyed through a
//! `WidgetId → NodeId` index, so the context's `WidgetId`-based API (`link_ids`,
//! `clear_hierarchy`, …) and the apps' handles name nodes by id. Links are **symmetric** by
//! construction: `set_parent` / `unlink` update both ends.
//!
//! ## The tree owns its widgets
//!
//! Every widget in the tree was moved into it (`UiContext::insert`) and lives in an allocation
//! the tree keeps, held as a raw ROOT (never a `Box` across accesses: a `Box` is a unique
//! pointer to the language, and every reborrow through it would invalidate the references the
//! context hands out). Every access — the app's through a handle, the context's dispatch —
//! derives from that root, and a widget out on loan (`UiContext::lend`) resolves to nothing,
//! so no two `&mut`s to one widget ever coexist (`docs/rfc-owning-registry.md`).
use std::any::TypeId;
use std::collections::HashMap;
use std::ptr::NonNull;

use crate::scene::arena::{Arena, NodeId};
use crate::widget::{WidgetHost, WidgetId};

/// One arena node's payload: the widget's stable id and, once it is inserted, the widget. A
/// node can be LINKED (as a parent or child) before its widget is inserted, as an app may
/// link ids before it builds the widgets; such a node resolves to nothing.
struct Entry {
    id: WidgetId,
    slot: Option<Slot>,
    /// Out on loan (`UiContext::lend`): while set, nothing in the tree resolves the widget,
    /// so a re-entrant reach for it is a miss rather than a second `&mut`.
    lent: bool,
}

/// A widget the tree owns: its allocation, held as the raw root every access derives from,
/// and its type for typed access. Dropping the slot drops the widget.
struct Slot {
    root: NonNull<dyn WidgetHost>,
    type_id: TypeId,
}

impl Drop for Slot {
    fn drop(&mut self) {
        // SAFETY: `root` was made by `Box::into_raw` in `insert_owned` and is freed only here,
        // once (`take_owned` forgets the slot it frees itself).
        unsafe { drop(Box::from_raw(self.root.as_ptr())) };
    }
}

/// The entry's widget, unless there is none yet or it is out on loan.
#[inline]
fn live_ptr(entry: &Entry) -> Option<*mut (dyn WidgetHost + 'static)> {
    if entry.lent {
        return None;
    }
    entry.slot.as_ref().map(|s| s.root.as_ptr())
}

/// The consolidated, generational widget tree. See the module docs.
pub struct WidgetTree {
    arena: Arena<Entry>,
    by_id: HashMap<WidgetId, NodeId>,
}

impl Default for WidgetTree {
    fn default() -> Self {
        Self::new()
    }
}

impl WidgetTree {
    pub fn new() -> Self {
        WidgetTree { arena: Arena::new(), by_id: HashMap::new() }
    }

    /// Number of nodes known to the tree (inserted or link-only).
    pub fn len(&self) -> usize {
        self.arena.len()
    }

    pub fn is_empty(&self) -> bool {
        self.arena.is_empty()
    }

    /// Get (or lazily create) the arena node for `id`. A freshly created node has no widget
    /// until [`insert_owned`](WidgetTree::insert_owned) supplies one. Re-creates the node if a
    /// stale `by_id` entry points at a removed slot.
    fn ensure_node(&mut self, id: WidgetId) -> NodeId {
        if let Some(&node) = self.by_id.get(&id) {
            if self.arena.contains(node) {
                return node;
            }
        }
        let node = self.arena.insert(Entry { id, slot: None, lent: false });
        self.by_id.insert(id, node);
        node
    }

    /// Make `child` a child of `parent` (deduped, reparenting from any previous parent). Keeps
    /// both ends of the edge consistent. No-op (rather than panic) if the link would form a
    /// cycle.
    pub fn link(&mut self, parent: WidgetId, child: WidgetId) {
        let parent_node = self.ensure_node(parent);
        let child_node = self.ensure_node(child);
        if parent_node == child_node || self.arena.is_ancestor(child_node, parent_node) {
            return;
        }
        self.arena.append_child(parent_node, child_node);
    }

    /// Set or clear `child`'s parent. `Some(p)` links symmetrically (as [`link`](WidgetTree::link));
    /// `None` detaches `child` from its current parent.
    pub fn set_parent(&mut self, child: WidgetId, parent: Option<WidgetId>) {
        match parent {
            Some(p) => self.link(p, child),
            None => {
                if let Some(&node) = self.by_id.get(&child) {
                    self.arena.detach(node);
                }
            }
        }
    }

    /// Remove `child` from `parent` if it is currently a child of it.
    pub fn unlink(&mut self, parent: WidgetId, child: WidgetId) {
        if let (Some(&child_node), Some(&parent_node)) =
            (self.by_id.get(&child), self.by_id.get(&parent))
        {
            if self.arena.parent(child_node) == Some(parent_node) {
                self.arena.detach(child_node);
            }
        }
    }

    /// Detach all of `parent`'s children, leaving them as roots. Non-recursive.
    pub fn clear_children(&mut self, parent: WidgetId) {
        if let Some(&parent_node) = self.by_id.get(&parent) {
            let children: Vec<NodeId> = self.arena.children(parent_node).to_vec();
            for child in children {
                self.arena.detach(child);
            }
        }
    }

    /// Drop every link, and every node that is only linked: the widgets stay, unlinked, as
    /// roots — an app that rebuilds its links every frame does not hand its widgets back by
    /// doing so.
    pub fn clear_all(&mut self) {
        let nodes: Vec<NodeId> = self.by_id.values().copied().collect();
        for &node in &nodes {
            if self.arena.contains(node) {
                self.arena.detach(node);
            }
        }
        for node in nodes {
            let keep = self.arena.value(node).is_some_and(|e| e.slot.is_some());
            if !keep && self.arena.contains(node) {
                let id = self.arena.value(node).map(|e| e.id);
                self.arena.remove_subtree(node);
                if let Some(id) = id {
                    self.by_id.remove(&id);
                }
            }
        }
        self.by_id.retain(|_, n| self.arena.contains(*n));
    }

    /// Take ownership of `widget`: it moves into an allocation the tree keeps, under its own
    /// id (keeping any links made to that id already), and is dropped when it is taken back
    /// or the tree is. Returns the id.
    pub fn insert_owned<W: WidgetHost + 'static>(&mut self, widget: W) -> WidgetId {
        let id = widget.base().id();
        let raw: *mut (dyn WidgetHost + 'static) = Box::into_raw(Box::new(widget));
        // SAFETY: `Box::into_raw` never returns null.
        let root = unsafe { NonNull::new_unchecked(raw) };
        let node = self.ensure_node(id);
        let entry = self.arena.value_mut(node).unwrap();
        // Two widgets with one id are a clone of a widget whose id was already drawn (the id
        // cell is copied): the second would replace — and drop — the first.
        debug_assert!(entry.slot.is_none(), "insert: {id:?} is already in the context (a clone of a widget that is?)");
        entry.lent = false;
        entry.slot = Some(Slot { root, type_id: TypeId::of::<W>() });
        id
    }

    /// The tree's widget `id` as a `W`: its root, when it is there, it is a `W`, and it is
    /// not out on loan.
    pub fn owned_root<W: WidgetHost + 'static>(&self, id: WidgetId) -> Option<NonNull<W>> {
        let entry = self.arena.value(*self.by_id.get(&id)?)?;
        let slot = entry.slot.as_ref()?;
        if entry.lent || slot.type_id != TypeId::of::<W>() {
            return None;
        }
        Some(slot.root.cast::<W>())
    }

    /// Give the tree's widget `id` back by value. Its node goes; its children stay, as roots.
    pub fn take_owned<W: WidgetHost + 'static>(&mut self, id: WidgetId) -> Option<W> {
        let node = *self.by_id.get(&id)?;
        let entry = self.arena.value_mut(node)?;
        if entry.lent || entry.slot.as_ref()?.type_id != TypeId::of::<W>() {
            return None;
        }
        let slot = std::mem::ManuallyDrop::new(entry.slot.take()?);
        // SAFETY: the slot's root was made by `Box::into_raw` of a `W` (its type id says so);
        // the slot is forgotten (ManuallyDrop) so it is freed only here, by the `Box` the
        // widget is moved out of.
        let widget = unsafe { *Box::from_raw(slot.root.cast::<W>().as_ptr()) };
        self.clear_children(id);
        self.arena.remove_subtree(node);
        self.by_id.remove(&id);
        Some(widget)
    }

    /// Mark `id` lent (or back). Returns whether it was free to lend: false for an unknown id,
    /// a link-only one, or one already out.
    pub fn set_lent(&mut self, id: WidgetId, lent: bool) -> bool {
        let Some(&node) = self.by_id.get(&id) else { return false };
        let Some(entry) = self.arena.value_mut(node) else { return false };
        if lent && (entry.lent || entry.slot.is_none()) {
            return false;
        }
        entry.lent = lent;
        true
    }

    /// Whether `id` names a widget the tree holds and has not lent out.
    pub fn is_registered(&self, id: WidgetId) -> bool {
        self.get_ptr(id).is_some()
    }

    /// The widget `id` names, or `None` if unknown, link-only or out on loan. For the
    /// context, which hands out references derived from it under its own borrow rules.
    pub(crate) fn get_ptr(&self, id: WidgetId) -> Option<*mut (dyn WidgetHost + 'static)> {
        let node = *self.by_id.get(&id)?;
        live_ptr(self.arena.value(node)?)
    }

    /// `id`'s parent id, if any.
    pub fn parent_id(&self, id: WidgetId) -> Option<WidgetId> {
        let node = *self.by_id.get(&id)?;
        let parent = self.arena.parent(node)?;
        Some(self.arena.value(parent)?.id)
    }

    /// `id`'s child ids in order (including link-only children not yet inserted).
    pub fn child_ids(&self, id: WidgetId) -> Vec<WidgetId> {
        let Some(&node) = self.by_id.get(&id) else { return Vec::new() };
        self.arena.children(node).iter().filter_map(|&c| self.arena.value(c).map(|e| e.id)).collect()
    }

    /// `id`'s children in order, skipping any that is link-only or out on loan.
    pub(crate) fn children_ptrs(&self, id: WidgetId) -> Vec<*mut (dyn WidgetHost + 'static)> {
        let Some(&node) = self.by_id.get(&id) else { return Vec::new() };
        self.arena
            .children(node)
            .iter()
            .filter_map(|&c| live_ptr(self.arena.value(c)?))
            .collect()
    }

    /// Every widget the tree holds and has not lent out, for the passes that sweep the whole
    /// registry (`clear_dirty`, `rebuild_spatial_grid`, the focus walk).
    pub(crate) fn iter_registered(&self) -> impl Iterator<Item = (WidgetId, *mut (dyn WidgetHost + 'static))> + '_ {
        self.by_id.values().filter_map(move |&node| {
            let entry = self.arena.value(node)?;
            live_ptr(entry).map(|p| (entry.id, p))
        })
    }

    /// The ids of every widget the tree holds and has not lent out, in no particular order.
    pub fn registered_ids(&self) -> Vec<WidgetId> {
        self.iter_registered().map(|(id, _)| id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal widget whose drops are counted.
    struct Marker {
        base: crate::widget::Widget,
        drops: std::rc::Rc<std::cell::Cell<u32>>,
    }
    impl Drop for Marker {
        fn drop(&mut self) {
            self.drops.set(self.drops.get() + 1);
        }
    }
    impl WidgetHost for Marker {
        crate::impl_widget_base!(Marker);
    }

    fn marker(drops: &std::rc::Rc<std::cell::Cell<u32>>) -> Marker {
        Marker { base: crate::widget::Widget::new(), drops: drops.clone() }
    }

    fn tree_of(n: usize) -> (WidgetTree, Vec<WidgetId>, std::rc::Rc<std::cell::Cell<u32>>) {
        let drops = std::rc::Rc::new(std::cell::Cell::new(0));
        let mut tree = WidgetTree::new();
        let ids = (0..n).map(|_| tree.insert_owned(marker(&drops))).collect();
        (tree, ids, drops)
    }

    #[test]
    fn insert_and_resolve() {
        let (tree, ids, _) = tree_of(1);
        assert!(tree.is_registered(ids[0]));
        assert!(tree.owned_root::<Marker>(ids[0]).is_some());
        assert!(tree.owned_root::<crate::widget::Adapted<crate::widget::Slider>>(ids[0]).is_none(), "typed by what was inserted");
        assert!(!tree.is_registered(WidgetId(usize::MAX)), "unknown id resolves to nothing");
    }

    #[test]
    fn link_is_symmetric_and_deduped() {
        let (mut tree, ids, _) = tree_of(2);
        let (p, c) = (ids[0], ids[1]);
        tree.link(p, c);
        tree.link(p, c); // duplicate link is a no-op
        assert_eq!(tree.parent_id(c), Some(p));
        assert_eq!(tree.child_ids(p), vec![c]);
        assert_eq!(tree.children_ptrs(p), vec![tree.get_ptr(c).unwrap()]);
    }

    #[test]
    fn reparenting_removes_from_old_parent() {
        let (mut tree, ids, _) = tree_of(3);
        let (a, b, c) = (ids[0], ids[1], ids[2]);
        tree.link(a, c);
        tree.link(b, c);
        assert!(tree.child_ids(a).is_empty(), "old parent drops the child");
        assert_eq!(tree.child_ids(b), vec![c]);
        assert_eq!(tree.parent_id(c), Some(b));
        tree.link(c, b);
        assert_eq!(tree.parent_id(b), None, "a link that would close a cycle is refused");
    }

    #[test]
    fn a_link_can_come_before_the_widget() {
        let (mut tree, ids, drops) = tree_of(1);
        let p = ids[0];
        let w = marker(&drops);
        let child = w.base.id();
        tree.link(p, child);
        assert_eq!(tree.child_ids(p), vec![child]);
        assert!(tree.children_ptrs(p).is_empty(), "a link-only child resolves to nothing");
        tree.insert_owned(w);
        assert_eq!(tree.child_ids(p), vec![child], "inserting keeps the link");
        assert_eq!(tree.children_ptrs(p).len(), 1);
    }

    #[test]
    fn detaching_and_clearing_keep_the_widgets() {
        let (mut tree, ids, drops) = tree_of(3);
        let (p, c1, c2) = (ids[0], ids[1], ids[2]);
        tree.link(p, c1);
        tree.link(p, c2);
        tree.set_parent(c1, None);
        assert_eq!(tree.child_ids(p), vec![c2], "a symmetric detach");
        tree.clear_children(p);
        assert!(tree.child_ids(p).is_empty() && tree.parent_id(c2).is_none());
        tree.link(p, c1);
        tree.link(p, WidgetId(usize::MAX)); // link-only
        tree.clear_all();
        assert_eq!(tree.len(), 3, "the link-only node goes, the widgets stay");
        assert!(ids.iter().all(|&id| tree.is_registered(id) && tree.parent_id(id).is_none()));
        assert_eq!(drops.get(), 0);
    }

    #[test]
    fn a_widget_taken_back_leaves_its_children() {
        let (mut tree, ids, drops) = tree_of(2);
        let (p, c) = (ids[0], ids[1]);
        tree.link(p, c);
        let back = tree.take_owned::<Marker>(p).expect("given back");
        assert_eq!(back.base.id(), p);
        assert!(!tree.is_registered(p) && tree.take_owned::<Marker>(p).is_none());
        assert!(tree.is_registered(c) && tree.parent_id(c).is_none(), "the child stays, a root");
        assert_eq!(drops.get(), 0);
        drop(back);
        drop(tree);
        assert_eq!(drops.get(), 2, "the tree drops what it holds");
    }

    #[test]
    fn a_lent_widget_resolves_to_nothing() {
        let (mut tree, ids, _) = tree_of(2);
        let (p, c) = (ids[0], ids[1]);
        tree.link(p, c);
        assert!(tree.set_lent(c, true));
        assert!(!tree.set_lent(c, true), "not lent twice");
        assert!(tree.get_ptr(c).is_none() && tree.children_ptrs(p).is_empty());
        assert!(tree.owned_root::<Marker>(c).is_none() && tree.take_owned::<Marker>(c).is_none());
        assert_eq!(tree.registered_ids(), vec![p]);
        tree.set_lent(c, false);
        assert!(tree.is_registered(c));
        assert!(!tree.set_lent(WidgetId(usize::MAX), true), "nothing to lend");
    }
}
