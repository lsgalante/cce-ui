//! Generational node arena — the ownership spine of the rebuilt cce-ui core.
//!
//! This is Phase 1 of the core rebuild (see `docs/rfc-core-rebuild.md`). It replaces the old
//! model where the widget tree was smeared across three parallel stores kept in sync by hand
//! (`Backplate.children: Vec<*mut dyn WidgetHost>`, `UiContext.layout_tree`, and
//! `UiContext.widget_registry`) and traversed through raw `*mut dyn WidgetHost` pointers that
//! `Drop` did not fully clear.
//!
//! Here there is exactly **one** store. Every node lives in the [`Arena`], addressed by a
//! [`NodeId`] that carries a generation. When a node is removed its slot's generation is bumped,
//! so any [`NodeId`] still pointing at the old occupant reads back as [`None`] instead of
//! dereferencing freed memory. Use-after-free becomes a missed lookup, not undefined behavior —
//! the single property that dissolves the dangling-pointer bug class.
//!
//! The arena is a **forest**: a freshly [`insert`](Arena::insert)ed node is a detached root
//! (`parent == None`); [`append_child`](Arena::append_child) links nodes into trees. Children are
//! stored as `Vec<NodeId>` (indices, not pointers), so traversal never aliases a `&mut`, which
//! keeps the whole thing safe and borrow-checker-friendly without `unsafe`.
//!
//! [`Node<T>`] is generic over its payload for now. In later phases the payload grows into the
//! rich per-node record from the RFC (widget + style + computed layout + animation + dirty
//! flags); nothing about the identity/ownership model below changes when it does.

use std::num::NonZeroU32;

/// A stable handle to a node in an [`Arena`].
///
/// Carries both a slot index and a generation. The generation makes the handle *safe across
/// removal*: once the node it referred to is removed (and its slot possibly reused for a
/// different node), every lookup with this id returns [`None`]. `NodeId` is `Copy` and cheap to
/// pass around; `Option<NodeId>` is the same size as `NodeId` thanks to the `NonZero` generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId {
    index: u32,
    generation: NonZeroU32,
}

impl NodeId {
    /// Opaque index into the arena's backing storage. Exposed only for debugging/telemetry —
    /// do not use it to bypass generational checks.
    #[inline]
    pub fn slot_index(self) -> u32 {
        self.index
    }
}

/// A node in the arena: a payload plus its tree links. Links are only mutated through the
/// [`Arena`] so the parent/child relationship stays symmetric.
#[derive(Debug, Clone)]
pub struct Node<T> {
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    value: T,
}

impl<T> Node<T> {
    /// This node's parent, or `None` if it is a detached root.
    #[inline]
    pub fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    /// This node's direct children, in order.
    #[inline]
    pub fn children(&self) -> &[NodeId] {
        &self.children
    }

    /// Shared access to the payload.
    #[inline]
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Mutable access to the payload. Tree links are intentionally not reachable here — use the
    /// [`Arena`] methods so both ends of every edge stay consistent.
    #[inline]
    pub fn value_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

enum Slot<T> {
    /// A live node. `generation` matches the [`NodeId`] handed out for it.
    Occupied { generation: NonZeroU32, node: Node<T> },
    /// A free slot. `generation` is the generation the *next* occupant will receive, and
    /// `next_free` chains the free list.
    Vacant { generation: NonZeroU32, next_free: Option<u32> },
}

impl<T> Slot<T> {
    #[inline]
    fn generation(&self) -> NonZeroU32 {
        match self {
            Slot::Occupied { generation, .. } | Slot::Vacant { generation, .. } => *generation,
        }
    }
}

#[inline]
fn bump(generation: NonZeroU32) -> NonZeroU32 {
    // Wrap while skipping 0 (which `NonZeroU32` cannot hold). A wrap only aliases a generation
    // after 2^32-1 reuses of the same slot, which no real UI session approaches.
    let next = generation.get().wrapping_add(1);
    NonZeroU32::new(if next == 0 { 1 } else { next }).unwrap()
}

const FIRST_GENERATION: NonZeroU32 = match NonZeroU32::new(1) {
    Some(g) => g,
    None => unreachable!(),
};

/// A generational forest of [`Node<T>`]. See the module docs for the design.
pub struct Arena<T> {
    slots: Vec<Slot<T>>,
    free_head: Option<u32>,
    len: usize,
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Arena<T> {
    /// An empty arena.
    pub fn new() -> Self {
        Arena { slots: Vec::new(), free_head: None, len: 0 }
    }

    /// An empty arena with room for `capacity` nodes before reallocating.
    pub fn with_capacity(capacity: usize) -> Self {
        Arena { slots: Vec::with_capacity(capacity), free_head: None, len: 0 }
    }

    /// Number of live nodes.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether there are no live nodes.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Whether `id` still refers to a live node (generation matches).
    #[inline]
    pub fn contains(&self, id: NodeId) -> bool {
        matches!(self.slots.get(id.index as usize),
            Some(Slot::Occupied { generation, .. }) if *generation == id.generation)
    }

    /// Insert a detached node (a new root) and return its id.
    pub fn insert(&mut self, value: T) -> NodeId {
        self.len += 1;
        match self.free_head {
            Some(index) => {
                let slot = &mut self.slots[index as usize];
                let generation = slot.generation();
                let next_free = match slot {
                    Slot::Vacant { next_free, .. } => *next_free,
                    Slot::Occupied { .. } => unreachable!("free list pointed at an occupied slot"),
                };
                self.free_head = next_free;
                *slot = Slot::Occupied {
                    generation,
                    node: Node { parent: None, children: Vec::new(), value },
                };
                NodeId { index, generation }
            }
            None => {
                let index = self.slots.len() as u32;
                let generation = FIRST_GENERATION;
                self.slots.push(Slot::Occupied {
                    generation,
                    node: Node { parent: None, children: Vec::new(), value },
                });
                NodeId { index, generation }
            }
        }
    }

    /// Shared access to a node, or `None` if `id` is stale/out of range.
    #[inline]
    pub fn get(&self, id: NodeId) -> Option<&Node<T>> {
        match self.slots.get(id.index as usize) {
            Some(Slot::Occupied { generation, node }) if *generation == id.generation => Some(node),
            _ => None,
        }
    }

    /// Mutable access to a node, or `None` if `id` is stale/out of range.
    #[inline]
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node<T>> {
        match self.slots.get_mut(id.index as usize) {
            Some(Slot::Occupied { generation, node }) if *generation == id.generation => Some(node),
            _ => None,
        }
    }

    /// Convenience: shared access to a node's payload.
    #[inline]
    pub fn value(&self, id: NodeId) -> Option<&T> {
        self.get(id).map(Node::value)
    }

    /// Convenience: mutable access to a node's payload.
    #[inline]
    pub fn value_mut(&mut self, id: NodeId) -> Option<&mut T> {
        self.get_mut(id).map(Node::value_mut)
    }

    /// Mutable access to two distinct nodes at once. Returns `None` if the ids are equal or
    /// either is stale. Needed by passes that move data between two nodes (e.g. reparenting or
    /// parent→child layout) without cloning.
    pub fn get_pair_mut(&mut self, a: NodeId, b: NodeId) -> Option<(&mut Node<T>, &mut Node<T>)> {
        if a.index == b.index {
            return None;
        }
        let (lo, hi, swapped) = if a.index < b.index { (a, b, false) } else { (b, a, true) };
        let (left, right) = self.slots.split_at_mut(hi.index as usize);
        let lo_node = match left.get_mut(lo.index as usize) {
            Some(Slot::Occupied { generation, node }) if *generation == lo.generation => node,
            _ => return None,
        };
        let hi_node = match right.get_mut(0) {
            Some(Slot::Occupied { generation, node }) if *generation == hi.generation => node,
            _ => return None,
        };
        Some(if swapped { (hi_node, lo_node) } else { (lo_node, hi_node) })
    }

    /// This node's parent (or `None` for a root or a stale id).
    #[inline]
    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.get(id).and_then(Node::parent)
    }

    /// This node's direct children (empty for a leaf or a stale id).
    #[inline]
    pub fn children(&self, id: NodeId) -> &[NodeId] {
        match self.get(id) {
            Some(node) => node.children(),
            None => &[],
        }
    }

    /// Make `child` the last child of `parent`, detaching it from any previous parent first.
    ///
    /// Panics if either id is stale, if `parent == child`, or if the link would create a cycle
    /// (`parent` is `child` or one of its descendants). These are programmer errors — reads of a
    /// stale id are still safe via [`get`](Arena::get); it is *mutating* through one that trips.
    pub fn append_child(&mut self, parent: NodeId, child: NodeId) {
        assert!(self.contains(parent), "append_child: parent is not a live node");
        assert!(self.contains(child), "append_child: child is not a live node");
        assert!(parent != child, "append_child: cannot make a node its own child");
        assert!(
            !self.is_ancestor(child, parent),
            "append_child: would create a cycle (parent is a descendant of child)"
        );

        self.detach(child);
        self.get_mut(child).unwrap().parent = Some(parent);
        self.get_mut(parent).unwrap().children.push(child);
    }

    /// Unlink `id` from its parent, leaving it (and its subtree) in the arena as a detached root.
    /// No-op if `id` is stale or already a root.
    pub fn detach(&mut self, id: NodeId) {
        let Some(parent) = self.parent(id) else { return };
        if let Some(parent_node) = self.get_mut(parent) {
            parent_node.children.retain(|&c| c != id);
        }
        if let Some(node) = self.get_mut(id) {
            node.parent = None;
        }
    }

    /// Remove `id` and its entire subtree from the arena, freeing every slot. Detaches `id` from
    /// its parent first. Every [`NodeId`] into the removed subtree is invalidated (later lookups
    /// return `None`). Returns the number of nodes removed; no-op returning 0 for a stale id.
    pub fn remove_subtree(&mut self, id: NodeId) -> usize {
        if !self.contains(id) {
            return 0;
        }
        self.detach(id);

        // Collect the subtree (pre-order) before mutating, so we don't invalidate mid-walk.
        let mut to_free = Vec::new();
        let mut stack = vec![id];
        while let Some(current) = stack.pop() {
            to_free.push(current);
            // Children are pushed as-is; order within the free set does not matter.
            stack.extend_from_slice(self.children(current));
        }

        for node_id in &to_free {
            let index = node_id.index as usize;
            let old_generation = self.slots[index].generation();
            self.slots[index] = Slot::Vacant {
                generation: bump(old_generation),
                next_free: self.free_head,
            };
            self.free_head = Some(node_id.index);
        }
        self.len -= to_free.len();
        to_free.len()
    }

    /// Whether `maybe_ancestor` is `id` itself or one of its ancestors.
    pub fn is_ancestor(&self, maybe_ancestor: NodeId, id: NodeId) -> bool {
        let mut current = Some(id);
        while let Some(node) = current {
            if node == maybe_ancestor {
                return true;
            }
            current = self.parent(node);
        }
        false
    }

    /// Iterator over `id`'s ancestors, nearest first, excluding `id` itself. Empty for a stale id.
    pub fn ancestors(&self, id: NodeId) -> Ancestors<'_, T> {
        Ancestors { arena: self, next: self.parent(id) }
    }

    /// Iterator over the subtree rooted at `id` in pre-order (`id` first, then each child's
    /// subtree). This is the walk order for layout and paint passes. Empty for a stale id.
    pub fn subtree(&self, id: NodeId) -> Subtree<'_, T> {
        let stack = if self.contains(id) { vec![id] } else { Vec::new() };
        Subtree { arena: self, stack }
    }

    /// Remove every node.
    pub fn clear(&mut self) {
        self.slots.clear();
        self.free_head = None;
        self.len = 0;
    }
}

/// Iterator returned by [`Arena::ancestors`].
pub struct Ancestors<'a, T> {
    arena: &'a Arena<T>,
    next: Option<NodeId>,
}

impl<'a, T> Iterator for Ancestors<'a, T> {
    type Item = NodeId;
    fn next(&mut self) -> Option<NodeId> {
        let current = self.next?;
        self.next = self.arena.parent(current);
        Some(current)
    }
}

/// Iterator returned by [`Arena::subtree`] (pre-order DFS).
pub struct Subtree<'a, T> {
    arena: &'a Arena<T>,
    stack: Vec<NodeId>,
}

impl<'a, T> Iterator for Subtree<'a, T> {
    type Item = NodeId;
    fn next(&mut self) -> Option<NodeId> {
        let current = self.stack.pop()?;
        // Push children in reverse so they are visited left-to-right.
        let children = self.arena.children(current);
        for &child in children.iter().rev() {
            self.stack.push(child);
        }
        Some(current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_get() {
        let mut arena: Arena<&str> = Arena::new();
        let a = arena.insert("a");
        assert_eq!(arena.len(), 1);
        assert!(!arena.is_empty());
        assert_eq!(arena.value(a), Some(&"a"));
        assert_eq!(arena.parent(a), None); // detached root
        assert!(arena.children(a).is_empty());
    }

    #[test]
    fn stale_id_reads_as_none_after_removal() {
        // The core safety property: a handle to a removed node never dereferences freed data.
        let mut arena: Arena<i32> = Arena::new();
        let a = arena.insert(10);
        assert!(arena.contains(a));
        arena.remove_subtree(a);
        assert!(!arena.contains(a));
        assert!(arena.get(a).is_none());
        assert_eq!(arena.value(a), None);
        assert_eq!(arena.len(), 0);
    }

    #[test]
    fn slot_reuse_bumps_generation_and_invalidates_old_handle() {
        let mut arena: Arena<i32> = Arena::new();
        let first = arena.insert(1);
        arena.remove_subtree(first);
        let second = arena.insert(2); // reuses the freed slot

        assert_eq!(second.slot_index(), first.slot_index(), "slot should be reused");
        assert_ne!(second, first, "generation must differ so the old handle is distinct");
        assert_eq!(arena.value(second), Some(&2));
        assert!(arena.get(first).is_none(), "stale handle to the reused slot is still None");
    }

    #[test]
    fn append_child_links_both_ends() {
        let mut arena: Arena<&str> = Arena::new();
        let parent = arena.insert("p");
        let child = arena.insert("c");
        arena.append_child(parent, child);

        assert_eq!(arena.parent(child), Some(parent));
        assert_eq!(arena.children(parent), &[child]);
    }

    #[test]
    fn reparenting_removes_from_old_parent() {
        let mut arena: Arena<&str> = Arena::new();
        let a = arena.insert("a");
        let b = arena.insert("b");
        let child = arena.insert("c");

        arena.append_child(a, child);
        assert_eq!(arena.children(a), &[child]);

        arena.append_child(b, child);
        assert!(arena.children(a).is_empty(), "old parent must drop the child");
        assert_eq!(arena.children(b), &[child]);
        assert_eq!(arena.parent(child), Some(b));
    }

    #[test]
    fn detach_keeps_node_but_unlinks_parent() {
        let mut arena: Arena<&str> = Arena::new();
        let parent = arena.insert("p");
        let child = arena.insert("c");
        arena.append_child(parent, child);

        arena.detach(child);
        assert_eq!(arena.parent(child), None);
        assert!(arena.children(parent).is_empty());
        assert!(arena.contains(child), "detach must not free the node");
    }

    #[test]
    fn remove_subtree_frees_all_descendants() {
        let mut arena: Arena<i32> = Arena::new();
        let root = arena.insert(0);
        let a = arena.insert(1);
        let b = arena.insert(2);
        let a1 = arena.insert(11);
        arena.append_child(root, a);
        arena.append_child(root, b);
        arena.append_child(a, a1);

        let freed = arena.remove_subtree(a);
        assert_eq!(freed, 2, "a and a1");
        assert!(!arena.contains(a));
        assert!(!arena.contains(a1));
        assert!(arena.contains(root));
        assert!(arena.contains(b));
        assert_eq!(arena.children(root), &[b], "a must be gone from root's children");
    }

    #[test]
    fn subtree_iterates_preorder_left_to_right() {
        let mut arena: Arena<&str> = Arena::new();
        let root = arena.insert("root");
        let a = arena.insert("a");
        let b = arena.insert("b");
        let a1 = arena.insert("a1");
        let a2 = arena.insert("a2");
        arena.append_child(root, a);
        arena.append_child(root, b);
        arena.append_child(a, a1);
        arena.append_child(a, a2);

        let order: Vec<&str> = arena.subtree(root).map(|id| *arena.value(id).unwrap()).collect();
        assert_eq!(order, vec!["root", "a", "a1", "a2", "b"]);
    }

    #[test]
    fn ancestors_walk_nearest_first() {
        let mut arena: Arena<&str> = Arena::new();
        let root = arena.insert("root");
        let a = arena.insert("a");
        let a1 = arena.insert("a1");
        arena.append_child(root, a);
        arena.append_child(a, a1);

        let anc: Vec<NodeId> = arena.ancestors(a1).collect();
        assert_eq!(anc, vec![a, root]);
        assert!(arena.ancestors(root).next().is_none(), "root has no ancestors");
    }

    #[test]
    fn is_ancestor_reports_self_and_chain() {
        let mut arena: Arena<i32> = Arena::new();
        let root = arena.insert(0);
        let a = arena.insert(1);
        arena.append_child(root, a);

        assert!(arena.is_ancestor(root, a));
        assert!(arena.is_ancestor(a, a), "a node is its own ancestor for cycle-check purposes");
        assert!(!arena.is_ancestor(a, root));
    }

    #[test]
    #[should_panic(expected = "cycle")]
    fn append_child_rejects_cycles() {
        let mut arena: Arena<i32> = Arena::new();
        let root = arena.insert(0);
        let a = arena.insert(1);
        arena.append_child(root, a);
        // Trying to make root a child of a (its descendant) would form a cycle.
        arena.append_child(a, root);
    }

    #[test]
    fn get_pair_mut_yields_distinct_nodes_in_argument_order() {
        let mut arena: Arena<i32> = Arena::new();
        let a = arena.insert(1);
        let b = arena.insert(2);

        let (na, nb) = arena.get_pair_mut(a, b).unwrap();
        *na.value_mut() += 100;
        *nb.value_mut() += 200;
        assert_eq!(arena.value(a), Some(&101));
        assert_eq!(arena.value(b), Some(&202));

        // Order preserved when the higher-index id is passed first.
        let (nb2, na2) = arena.get_pair_mut(b, a).unwrap();
        assert_eq!(*nb2.value(), 202);
        assert_eq!(*na2.value(), 101);

        assert!(arena.get_pair_mut(a, a).is_none(), "same id must be rejected");
    }

    #[test]
    fn clear_empties_the_arena() {
        let mut arena: Arena<i32> = Arena::new();
        let a = arena.insert(1);
        arena.insert(2);
        arena.clear();
        assert!(arena.is_empty());
        assert!(arena.get(a).is_none());
    }

    // Validation against a real trait object: the arena must be able to *own* and tree actual
    // `dyn WidgetHost` widgets (the payload type Phase 3 will use), not just Copy scalars.
    #[test]
    fn holds_and_trees_real_dyn_element_payloads() {
        use crate::widget::WidgetHost;

        // A minimal real `WidgetHost` — `color` is the trait's only required method, everything
        // else is defaulted, so this exercises the actual trait object without dragging in a
        // heavyweight widget constructor.
        struct Marker {
            base: crate::widget::Widget,
            tint: [f32; 4],
            painted: std::cell::Cell<bool>,
        }
        impl WidgetHost for Marker {
            crate::impl_widget_base!(Marker);
            fn color(&self) -> [f32; 4] {
                self.painted.set(true);
                self.tint
            }
        }

        let mut arena: Arena<Box<dyn WidgetHost>> = Arena::new();
        let root = arena.insert(Box::new(Marker { base: crate::widget::Widget::new(), tint: [1.0, 0.0, 0.0, 1.0], painted: false.into() }));
        let child = arena.insert(Box::new(Marker { base: crate::widget::Widget::new(), tint: [0.0, 1.0, 0.0, 1.0], painted: false.into() }));
        arena.append_child(root, child);

        // Walk the subtree the way a paint pass will, calling a real trait method on each node.
        let tints: Vec<[f32; 4]> = arena
            .subtree(root)
            .map(|id| arena.value(id).unwrap().color())
            .collect();
        assert_eq!(tints, vec![[1.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 1.0]]);

        // The trait object is genuinely stored (its base is reachable through the box).
        let _ = arena.value(root).unwrap().base();

        // Removing the root frees the child too, proving ownership lives in the arena.
        arena.remove_subtree(root);
        assert!(arena.is_empty());
    }
}
