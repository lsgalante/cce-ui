//! `WidgetTree` — the arena-backed replacement for `UiContext`'s two tree stores.
//!
//! Today `UiContext` keeps the widget tree in two parallel `HashMap`s that must be maintained in
//! lockstep by hand:
//!   * `widget_registry: HashMap<WidgetId, *mut dyn WidgetHost>` — id → live pointer, and
//!   * `layout_tree: { parents: HashMap<WidgetId, WidgetId>, children: HashMap<WidgetId, Vec<WidgetId>> }`.
//!
//! This type folds both into a single generational [`Arena`], keyed through a `WidgetId → NodeId`
//! index so the *public* `WidgetId`-based API (`register_widget`, `link_ids`, `clear_hierarchy`,
//! …) can be preserved unchanged for the app crates. Consolidating the stores removes the
//! hand-sync burden, and the generational [`NodeId`] means a removed widget's handle reads back as
//! `None` instead of dereferencing freed memory.
//!
//! ## One deliberate semantic change vs. the legacy maps
//!
//! The legacy maps are sometimes left **asymmetric**: `WidgetHost::set_parent(Some(p))` writes
//! `parents[child] = p` but does *not* add `child` to `children[p]`; `plate`/`parameters_bg`
//! detach by doing `parents.remove(child)` while leaving `child` in `children[p]`. The arena keeps
//! parent and child links **symmetric** by construction, so here `set_parent`/`detach` update both
//! ends. This is the single behavior difference to watch when swapping `WidgetTree` into
//! `UiContext` — it makes the tree self-consistent, but it must be verified against the running
//! apps (paint recursion and event propagation both read `children`). See
//! `docs/rfc-core-rebuild.md` Phase 1b.
//!
//! Nothing here is wired into `UiContext` yet; this is the tested drop-in the swap will use.

use std::collections::HashMap;

use crate::scene::arena::{Arena, NodeId};
use crate::widget::{WidgetHost, WidgetId};

/// One arena node's payload: the widget's stable id plus its live pointer. The pointer is `None`
/// for a node that has been *linked* into the tree (as a parent/child) but not yet *registered*
/// with a real widget — mirroring the legacy maps, where a `layout_tree` link can precede the
/// `widget_registry` entry. (`*mut dyn WidgetHost` is a fat pointer, so `Option` is the natural
/// "absent" representation — there is no thin null to use as a sentinel.)
#[derive(Clone, Copy)]
struct Entry {
    id: WidgetId,
    ptr: Option<*mut (dyn WidgetHost + 'static)>,
}

/// Resolve an entry's pointer to a usable, non-null pointer (skipping link-only and null-data
/// pointers exactly as the legacy `filter_map` over the registry did).
#[inline]
fn live_ptr(entry: &Entry) -> Option<*mut (dyn WidgetHost + 'static)> {
    match entry.ptr {
        Some(p) if !p.is_null() => Some(p),
        _ => None,
    }
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

    /// Number of nodes known to the tree (registered or link-only).
    pub fn len(&self) -> usize {
        self.arena.len()
    }

    pub fn is_empty(&self) -> bool {
        self.arena.is_empty()
    }

    /// Get (or lazily create) the arena node for `id`. A freshly created node has a `null`
    /// pointer until [`register`](WidgetTree::register) supplies one. Re-creates the node if a
    /// stale `by_id` entry points at a removed slot.
    fn ensure_node(&mut self, id: WidgetId) -> NodeId {
        if let Some(&node) = self.by_id.get(&id) {
            if self.arena.contains(node) {
                return node;
            }
        }
        let node = self.arena.insert(Entry { id, ptr: None });
        self.by_id.insert(id, node);
        node
    }

    /// Register (or overwrite) the live pointer for `id`. Mirrors `register_widget`'s
    /// insert-overwrite semantics. Registering a `null` pointer is allowed (the node exists but
    /// resolves to `None`), matching the legacy behavior where a link can precede registration.
    pub fn register(&mut self, id: WidgetId, ptr: *mut (dyn WidgetHost + 'static)) {
        let node = self.ensure_node(id);
        // `ensure_node` guarantees the node exists.
        self.arena.value_mut(node).unwrap().ptr = Some(ptr);
    }

    /// Make `child` a child of `parent` (deduped, reparenting from any previous parent). Mirrors
    /// `link_ids`, but keeps both ends of the edge consistent. No-op (rather than panic) if the
    /// link would form a cycle, which the legacy maps never guarded against but also never hit.
    pub fn link(&mut self, parent: WidgetId, child: WidgetId) {
        let parent_node = self.ensure_node(parent);
        let child_node = self.ensure_node(child);
        if parent_node == child_node || self.arena.is_ancestor(child_node, parent_node) {
            return;
        }
        self.arena.append_child(parent_node, child_node);
    }

    /// Set or clear `child`'s parent. `Some(p)` links symmetrically (as [`link`](WidgetTree::link));
    /// `None` detaches `child` from its current parent. Replaces the legacy asymmetric
    /// `WidgetHost::set_parent`.
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

    /// Remove `child` from `parent` if it is currently a child of it. Mirrors `unlink_child`.
    pub fn unlink(&mut self, parent: WidgetId, child: WidgetId) {
        if let (Some(&child_node), Some(&parent_node)) =
            (self.by_id.get(&child), self.by_id.get(&parent))
        {
            if self.arena.parent(child_node) == Some(parent_node) {
                self.arena.detach(child_node);
            }
        }
    }

    /// Detach all of `parent`'s children, leaving them as (still-registered) roots. Mirrors
    /// `clear_children_ids`: non-recursive, and it does *not* unregister the child pointers.
    pub fn clear_children(&mut self, parent: WidgetId) {
        if let Some(&parent_node) = self.by_id.get(&parent) {
            let children: Vec<NodeId> = self.arena.children(parent_node).to_vec();
            for child in children {
                self.arena.detach(child);
            }
        }
    }

    /// Drop the entire tree. Mirrors `clear_hierarchy`'s reset of both maps.
    pub fn clear_all(&mut self) {
        self.arena.clear();
        self.by_id.clear();
    }

    /// Remove `id` and its whole subtree, freeing arena slots and dropping their `by_id` entries.
    /// Not used by the legacy-compatible swap (the old maps never removed individual nodes), but
    /// available for the migrated code that will actually reclaim removed widgets.
    pub fn remove(&mut self, id: WidgetId) {
        let Some(&node) = self.by_id.get(&id) else { return };
        let removed_ids: Vec<WidgetId> =
            self.arena.subtree(node).filter_map(|n| self.arena.value(n).map(|e| e.id)).collect();
        self.arena.remove_subtree(node);
        for removed in removed_ids {
            self.by_id.remove(&removed);
        }
    }

    /// Whether `id` currently resolves to a live, non-null widget pointer.
    pub fn is_registered(&self, id: WidgetId) -> bool {
        self.get_ptr(id).is_some()
    }

    /// The live pointer for `id`, or `None` if unknown, link-only (null), or stale.
    pub fn get_ptr(&self, id: WidgetId) -> Option<*mut (dyn WidgetHost + 'static)> {
        let node = *self.by_id.get(&id)?;
        live_ptr(self.arena.value(node)?)
    }

    /// `id`'s parent id, if any.
    pub fn parent_id(&self, id: WidgetId) -> Option<WidgetId> {
        let node = *self.by_id.get(&id)?;
        let parent = self.arena.parent(node)?;
        Some(self.arena.value(parent)?.id)
    }

    /// `id`'s parent pointer, if the parent is registered (non-null).
    pub fn parent_ptr(&self, id: WidgetId) -> Option<*mut (dyn WidgetHost + 'static)> {
        self.parent_id(id).and_then(|p| self.get_ptr(p))
    }

    /// `id`'s child ids in order (including link-only children not yet registered).
    pub fn child_ids(&self, id: WidgetId) -> Vec<WidgetId> {
        let Some(&node) = self.by_id.get(&id) else { return Vec::new() };
        self.arena.children(node).iter().filter_map(|&c| self.arena.value(c).map(|e| e.id)).collect()
    }

    /// `id`'s child pointers in order, skipping any child that is link-only (null pointer) —
    /// exactly matching the legacy `WidgetHost::children` `filter_map` over the registry.
    pub fn children_ptrs(&self, id: WidgetId) -> Vec<*mut (dyn WidgetHost + 'static)> {
        let Some(&node) = self.by_id.get(&id) else { return Vec::new() };
        self.arena
            .children(node)
            .iter()
            .filter_map(|&c| live_ptr(self.arena.value(c)?))
            .collect()
    }

    /// Iterate every registered `(id, ptr)` with a non-null pointer, for the passes that sweep the
    /// whole registry (`clear_dirty`, `rebuild_spatial_grid`, coverage tests).
    pub fn iter_registered(&self) -> impl Iterator<Item = (WidgetId, *mut (dyn WidgetHost + 'static))> + '_ {
        self.by_id.values().filter_map(move |&node| {
            let entry = self.arena.value(node)?;
            live_ptr(entry).map(|p| (entry.id, p))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A minimal real `WidgetHost` so tests exercise genuine `*mut dyn WidgetHost` payloads. The boxes
    // are kept alive in a local `Vec` for the duration of each test; we hand the tree raw
    // pointers into them, mirroring how widgets (owned by the app) are referenced by the tree.
    struct Marker {
        base: crate::widget::Widget,
        #[allow(dead_code)]
        tag: u32,
    }
    impl WidgetHost for Marker {
        crate::impl_widget_base!(Marker);
        fn color(&self) -> [f32; 4] {
            [0.0, 0.0, 0.0, 0.0]
        }
    }

    /// Owns marker widgets and hands out stable raw pointers + ids for them.
    struct Widgets {
        boxes: Vec<Box<Marker>>,
    }
    impl Widgets {
        fn new() -> Self {
            Widgets { boxes: Vec::new() }
        }
        /// Create a widget, returning `(WidgetId, *mut dyn WidgetHost)`.
        fn make(&mut self, tag: u32) -> (WidgetId, *mut (dyn WidgetHost + 'static)) {
            let mut b = Box::new(Marker { base: crate::widget::Widget::new(), tag: tag });
            let ptr: *mut (dyn WidgetHost + 'static) = &mut *b;
            self.boxes.push(b);
            (WidgetId(tag as usize), ptr)
        }
    }

    #[test]
    fn register_and_resolve() {
        let mut w = Widgets::new();
        let mut tree = WidgetTree::new();
        let (id, ptr) = w.make(1);
        assert_eq!(tree.get_ptr(id), None, "unknown id resolves to None");
        tree.register(id, ptr);
        assert_eq!(tree.get_ptr(id), Some(ptr));
        assert!(tree.is_registered(id));
    }

    #[test]
    fn register_overwrites_pointer() {
        let mut w = Widgets::new();
        let mut tree = WidgetTree::new();
        let id = WidgetId(1);
        let (_, p1) = w.make(1);
        let (_, p2) = w.make(2);
        tree.register(id, p1);
        tree.register(id, p2); // same id, new pointer
        assert_eq!(tree.get_ptr(id), Some(p2));
        assert_eq!(tree.len(), 1, "overwrite must not create a second node");
    }

    #[test]
    fn link_is_symmetric_and_deduped() {
        let mut w = Widgets::new();
        let mut tree = WidgetTree::new();
        let (p, pp) = w.make(1);
        let (c, cp) = w.make(2);
        tree.register(p, pp);
        tree.register(c, cp);

        tree.link(p, c);
        tree.link(p, c); // duplicate link is a no-op
        assert_eq!(tree.parent_id(c), Some(p));
        assert_eq!(tree.child_ids(p), vec![c]);
        assert_eq!(tree.children_ptrs(p), vec![cp]);
    }

    #[test]
    fn reparenting_removes_from_old_parent() {
        let mut w = Widgets::new();
        let mut tree = WidgetTree::new();
        let (a, ap) = w.make(1);
        let (b, bp) = w.make(2);
        let (c, cp) = w.make(3);
        tree.register(a, ap);
        tree.register(b, bp);
        tree.register(c, cp);

        tree.link(a, c);
        assert_eq!(tree.child_ids(a), vec![c]);
        tree.link(b, c);
        assert!(tree.child_ids(a).is_empty(), "old parent drops the child");
        assert_eq!(tree.child_ids(b), vec![c]);
        assert_eq!(tree.parent_id(c), Some(b));
    }

    #[test]
    fn link_before_register_uses_null_placeholder() {
        // Mirrors the legacy case where a `layout_tree` link precedes the `widget_registry` entry:
        // the child appears in `child_ids` but is skipped by `children_ptrs` until registered.
        let mut w = Widgets::new();
        let mut tree = WidgetTree::new();
        let (p, pp) = w.make(1);
        tree.register(p, pp);
        let child = WidgetId(2);

        tree.link(p, child); // child not registered yet
        assert_eq!(tree.child_ids(p), vec![child]);
        assert!(tree.children_ptrs(p).is_empty(), "link-only child has no pointer yet");

        let (_, cp) = w.make(2);
        tree.register(child, cp);
        assert_eq!(tree.children_ptrs(p), vec![cp], "now resolvable");
    }

    #[test]
    fn set_parent_none_detaches_symmetrically() {
        // The deliberate divergence from legacy: detaching clears BOTH ends, so the parent's
        // children no longer list the child.
        let mut w = Widgets::new();
        let mut tree = WidgetTree::new();
        let (p, pp) = w.make(1);
        let (c, cp) = w.make(2);
        tree.register(p, pp);
        tree.register(c, cp);
        tree.link(p, c);

        tree.set_parent(c, None);
        assert_eq!(tree.parent_id(c), None);
        assert!(tree.child_ids(p).is_empty(), "symmetric detach clears parent's child list too");
        assert!(tree.is_registered(c), "detach keeps the widget registered");
    }

    #[test]
    fn clear_children_detaches_but_keeps_registration() {
        let mut w = Widgets::new();
        let mut tree = WidgetTree::new();
        let (p, pp) = w.make(1);
        let (c1, c1p) = w.make(2);
        let (c2, c2p) = w.make(3);
        tree.register(p, pp);
        tree.register(c1, c1p);
        tree.register(c2, c2p);
        tree.link(p, c1);
        tree.link(p, c2);

        tree.clear_children(p);
        assert!(tree.child_ids(p).is_empty());
        assert_eq!(tree.parent_id(c1), None);
        assert!(tree.is_registered(c1) && tree.is_registered(c2), "children stay registered");
    }

    #[test]
    fn clear_all_empties_everything() {
        let mut w = Widgets::new();
        let mut tree = WidgetTree::new();
        let (p, pp) = w.make(1);
        let (c, cp) = w.make(2);
        tree.register(p, pp);
        tree.register(c, cp);
        tree.link(p, c);

        tree.clear_all();
        assert!(tree.is_empty());
        assert_eq!(tree.get_ptr(p), None);
        assert_eq!(tree.parent_id(c), None);
    }

    #[test]
    fn remove_makes_stale_ids_resolve_to_none() {
        // The safety win over the legacy registry, which never removed entries (leaving dangling
        // pointers): after removal, the id resolves to None instead of a freed pointer.
        let mut w = Widgets::new();
        let mut tree = WidgetTree::new();
        let (p, pp) = w.make(1);
        let (c, cp) = w.make(2);
        tree.register(p, pp);
        tree.register(c, cp);
        tree.link(p, c);

        tree.remove(p); // removes p and its subtree (c)
        assert_eq!(tree.get_ptr(p), None);
        assert_eq!(tree.get_ptr(c), None, "descendant removed too");
        assert!(tree.is_empty());
    }

    #[test]
    fn iter_registered_yields_only_non_null() {
        let mut w = Widgets::new();
        let mut tree = WidgetTree::new();
        let (p, pp) = w.make(1);
        tree.register(p, pp);
        tree.link(p, WidgetId(99)); // link-only, null pointer

        let seen: Vec<WidgetId> = tree.iter_registered().map(|(id, _)| id).collect();
        assert_eq!(seen, vec![p], "link-only (null) node is not yielded");
    }
}
