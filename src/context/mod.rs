//! `UiContext`: the window's widget registry and everything routed through it. It owns the
//! widgets (`insert` → `Handle<W>`; `lend` takes one out of reach for a call into it), the tree
//! and the spatial grid, and routes events, focus, popovers, modals and the context menu by id.
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the struct, the registry (insert, get, remove, lend), delivery, dirty state, ticks, the hierarchy, `ctx[h]` |
//! | `spatial` | `SpatialGrid`: widget ids bucketed by cell, for hit tests |
//! | `events` | dispatching an event into a tree, the scroll-gesture bookkeeping |
//! | `focus` | focus by id, modals, the Tab walk (`focus_step`, `focus_clusters`) |
//! | `popovers` | popovers, coverage, the cursor, and the hit tests that ask them (`drag_allowed_at`, `is_widget_at`) |
//! | `menu` | showing, routing and drawing the context menu |

mod events;
mod focus;
mod menu;
mod popovers;
mod spatial;
#[cfg(test)]
mod focus_step_tests;
#[cfg(test)]
mod tests;

pub use spatial::SpatialGrid;

use std::collections::HashMap;
use crate::widget::{Handle, WidgetHost, WidgetId, Key, NamedKey, MouseButton, ElementState, Event, WidgetHostExt};

/// One open modal: who opened it, what is inside it, and where focus was before.
#[derive(Debug, Clone)]
struct ModalScope {
    owner: WidgetId,
    members: Vec<WidgetId>,
    restore: Option<WidgetId>,
}

pub struct UiContext {
    /// The widget tree + registry, consolidated into one generational store (Phase 1b of the
    /// core rebuild). Replaces the former `layout_tree` + `widget_registry` maps; see
    /// `scene/tree.rs`.
    pub tree: crate::scene::WidgetTree,
    /// The focused widget's id (Phase 6bc: stored ids, not pointers — a stale id resolves to
    /// `None` through the generational tree instead of dereferencing freed memory).
    pub focused_widget: Option<WidgetId>,
    /// Open-popover registrations, id-keyed like focus (Phase 6bc slice 2).
    pub active_popovers: Vec<WidgetId>,
    /// Open modals, innermost last ([`UiContext::open_modal`]): while one is open the Tab
    /// walk visits only its members, and every widget outside it reads as covered, so no
    /// press or hover reaches what lies behind.
    modals: Vec<ModalScope>,
    /// Memo for `is_coordinate_covered` at a single cursor position: the ids of
    /// every widget whose popover rect contains it. That query scans the entire
    /// registry, and `hit_test` calls it — so dispatching one PointerMove to N
    /// roots cost N×N `popover_rect()` calls (the 1359-row Packages list: 1.85M
    /// per motion event, ~14ms, which starved the whole frame loop). Every one
    /// of those queries shares the same point and differs only in which id it
    /// excludes, so the scan runs once per position and each caller then asks
    /// whether some *other* id covers it. Invalidated whenever the registry
    /// changes or a frame's registration is reset.
    /// `RefCell` because `hit_test` receives `&UiContext` — the memo is an
    /// implementation detail of a read-only query, not shared state.
    covered_cache: std::cell::RefCell<(Option<(f32, f32)>, Vec<WidgetId>)>,
    pub cursor_pos: (f32, f32),
    pub active_grab: Option<WidgetId>,
    pub drag_start_pos: Option<(f32, f32)>,
    pub drag_target: Option<WidgetId>,
    pub is_dragging: bool,
    pub any_dirty: bool,
    pub tick_receivers: Vec<WidgetId>,
    /// How many times `tick` has run. The runner reads it around the app's
    /// own `Application::tick` to see whether the app already advanced the
    /// roster this frame — receivers integrate `dt` (scroll glides, slider
    /// inertia), so a second tick per frame would run them at double speed.
    tick_count: u64,
    pub spatial_grid: SpatialGrid,
    pub last_scroll_time: Option<web_time::Instant>,
    pub scroll_initiate_widget_id: Option<WidgetId>,
    pub scroll_gesture_new: bool,
    pub ctrl_pressed: bool,
    pub shift_pressed: bool,
    pub alt_pressed: bool,
    pub logo_pressed: bool,
}

impl UiContext {
    pub fn new() -> Self {
        Self {
            tree: crate::scene::WidgetTree::new(),
            focused_widget: None,
            active_popovers: Vec::new(),
            modals: Vec::new(),
            covered_cache: std::cell::RefCell::new((None, Vec::new())),
            cursor_pos: (0.0, 0.0),
            active_grab: None,
            drag_start_pos: None,
            drag_target: None,
            is_dragging: false,
            any_dirty: false,
            tick_receivers: Vec::new(),
            tick_count: 0,
            spatial_grid: SpatialGrid::new(100.0),
            last_scroll_time: None,
            scroll_initiate_widget_id: None,
            scroll_gesture_new: false,
            ctrl_pressed: false,
            shift_pressed: false,
            alt_pressed: false,
            logo_pressed: false,
        }
    }

    pub fn get_widget(&self, id: WidgetId) -> Option<&(dyn WidgetHost + 'static)> {
        self.tree.get_ptr(id).map(|ptr| unsafe { &*ptr })
    }

    pub fn get_widget_mut(&mut self, id: WidgetId) -> Option<&mut (dyn WidgetHost + 'static)> {
        self.tree.get_ptr(id).map(|ptr| unsafe { &mut *ptr })
    }

    /// Every widget the context holds (and has not lent out), with its id, in no particular
    /// order — for a sweep over the whole window (a focus heir, a hit search).
    pub fn widgets(&self) -> impl Iterator<Item = (WidgetId, &(dyn WidgetHost + 'static))> + '_ {
        // SAFETY: as `get_widget`: the context's own widgets, borrowed through `&self`.
        self.tree.iter_registered().map(|(id, ptr)| (id, unsafe { &*ptr }))
    }

    // ── Widgets the context owns, by handle (docs/rfc-owning-registry.md) ──────────────

    /// Take ownership of `widget`, register it under its own id, and hand back its handle.
    /// The widget lives in the context from here on; reach it with [`get`](Self::get) /
    /// [`get_mut`](Self::get_mut) (or `ctx[h]`), give it back with [`remove`](Self::remove).
    pub fn insert<W: WidgetHost + 'static>(&mut self, widget: W) -> Handle<W> {
        // A widget that animates is ticked by the context (`tick`): a tree list applies its
        // search there.
        let wants_tick = crate::widget::WidgetHostExt::wants_tick(&widget);
        let id = self.tree.insert_owned(widget);
        self.invalidate_coverage_cache();
        if wants_tick {
            self.register_tick_receiver(id);
        }
        // Its embedded children (`widget::Embedded`) follow it in.
        self.lend(id, |w, ctx| w.attach_embedded(ctx));
        Handle::from_id(id)
    }

    /// The widget `h` names: `None` once it is removed, or while the context has it out on
    /// loan (a widget reaching for itself while it handles an event).
    pub fn get<W: WidgetHost + 'static>(&self, h: Handle<W>) -> Option<&W> {
        // SAFETY: the tree's own allocation, typed by `owned_root`, not lent; borrowed from
        // `&self`, so no `&mut` to it can be live.
        self.tree.owned_root::<W>(h.id()).map(|p| unsafe { &*p.as_ptr() })
    }

    /// [`get`](Self::get), mutably. The borrow is of the whole context, so an app cannot
    /// hold the widget across another context call — the one rule the pointer registry left
    /// to discipline, now the compiler's.
    pub fn get_mut<W: WidgetHost + 'static>(&mut self, h: Handle<W>) -> Option<&mut W> {
        // SAFETY: as in `get`, and `&mut self` is exclusive: nothing else in the context is
        // touching the widget while this borrow lives.
        self.tree.owned_root::<W>(h.id()).map(|p| unsafe { &mut *p.as_ptr() })
    }

    /// Give the widget `h` names back by value, unregistered: its links, its focus and its
    /// place in the tick list go with it, and its embedded children come back inside it.
    /// `None` if it is gone or out on loan.
    pub fn remove<W: WidgetHost + 'static>(&mut self, h: Handle<W>) -> Option<W> {
        let id = h.id();
        self.tree.owned_root::<W>(id)?;
        // Its embedded children (`widget::Embedded`) come back into it first.
        self.lend(id, |w, ctx| w.release_embedded(ctx));
        let widget = self.tree.take_owned::<W>(id)?;
        if self.focused_widget == Some(id) {
            self.focused_widget = None;
        }
        self.tick_receivers.retain(|&r| r != id);
        self.invalidate_coverage_cache();
        Some(widget)
    }

    /// Lend the widget `id` out for one call: `f` gets it and the context, and it is back in
    /// the registry when `f` returns. While it is out nothing in the context resolves it, so
    /// whatever `f` does with the context — dispatch, focus, a handle lookup — cannot reach
    /// the widget a second time. `None` if `id` is unknown, stale, or already out.
    ///
    /// Every call the context makes into a widget that hands it the context comes through
    /// here; so does a host that places or drives a widget it holds by handle.
    pub fn lend<R>(&mut self, id: WidgetId, f: impl FnOnce(&mut (dyn WidgetHost + 'static), &mut UiContext) -> R) -> Option<R> {
        let ptr = self.tree.get_ptr(id)?;
        if !self.tree.set_lent(id, true) {
            return None;
        }
        // SAFETY: a live widget the registry resolved, now out on loan: until it is put back
        // no resolver in the context hands it out, so this `&mut` is the only one.
        let out = f(unsafe { &mut *ptr }, self);
        self.tree.set_lent(id, false);
        Some(out)
    }

    /// [`lend`](Self::lend) by handle, typed.
    pub fn lend_h<W: WidgetHost + 'static, R>(&mut self, h: Handle<W>, f: impl FnOnce(&mut W, &mut UiContext) -> R) -> Option<R> {
        let root = self.tree.owned_root::<W>(h.id())?;
        if !self.tree.set_lent(h.id(), true) {
            return None;
        }
        // SAFETY: as in `lend`; the root is the tree's own `W`.
        let out = f(unsafe { &mut *root.as_ptr() }, self);
        self.tree.set_lent(h.id(), false);
        Some(out)
    }

    /// Hand `event` to widget `id`, lent for the call; a widget that takes it is marked dirty.
    fn deliver(&mut self, id: WidgetId, event: &Event) -> bool {
        self.lend(id, |w, ctx| {
            let handled = w.handle_event(event, ctx);
            if handled {
                w.mark_dirty(ctx);
            }
            handled
        })
        .unwrap_or(false)
    }

    /// [`deliver`](Self::deliver), marking the widget dirty whatever it answers (the drag
    /// lifecycle's start and end).
    fn deliver_dirty(&mut self, id: WidgetId, event: &Event) {
        self.lend(id, |w, ctx| {
            w.handle_event(event, ctx);
            w.mark_dirty(ctx);
        });
    }

    /// A widget's rect, read without lending it.
    fn rect_of(&self, id: WidgetId) -> Option<(f32, f32, f32, f32)> {
        self.get_widget(id).map(|w| w.rect())
    }

    pub fn is_dirty(&self) -> bool {
        self.any_dirty
    }

    pub fn clear_dirty(&mut self) {
        self.any_dirty = false;
        let ptrs: Vec<*mut (dyn WidgetHost + 'static)> =
            self.tree.iter_registered().map(|(_, ptr)| ptr).collect();
        for ptr in ptrs {
            unsafe {
                (*ptr).base_mut().dirty = false;
            }
        }
        self.rebuild_spatial_grid();
    }

    pub fn rebuild_spatial_grid(&mut self) {
        self.spatial_grid.clear();
        let entries: Vec<(WidgetId, *mut (dyn WidgetHost + 'static))> =
            self.tree.iter_registered().collect();
        for (id, ptr) in entries {
            unsafe {
                let rect = (*ptr).rect();
                self.spatial_grid.insert(id, rect);
            }
        }
    }

    pub fn register_tick_receiver(&mut self, id: WidgetId) {
        if !self.tick_receivers.contains(&id) {
            self.tick_receivers.push(id);
        }
    }

    pub fn unregister_tick_receiver(&mut self, id: WidgetId) {
        self.tick_receivers.retain(|&x| x != id);
    }

    pub fn is_widget_visible(&self, id: WidgetId) -> bool {
        let mut curr = id;
        loop {
            if let Some(w_ptr) = self.tree.get_ptr(curr) {
                unsafe {
                    if !(*w_ptr).visible() {
                        return false;
                    }
                }
            } else {
                return false;
            }
            if let Some(parent_id) = self.tree.parent_id(curr) {
                curr = parent_id;
            } else {
                break;
            }
        }
        true
    }

    /// Number of `tick` calls so far (see the field doc).
    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    pub fn tick(&mut self, dt: f32) -> bool {
        self.tick_count = self.tick_count.wrapping_add(1);
        let mut changed = false;
        let ids = self.tick_receivers.clone();
        for id in ids {
            if self.is_widget_visible(id) {
                let ticked = self.lend(id, |w, ctx| {
                    let moved = w.tick(dt, ctx);
                    if moved {
                        w.mark_dirty(ctx);
                    }
                    moved
                });
                changed |= ticked.unwrap_or(false);
            }
        }
        changed
    }

    pub fn link_ids(&mut self, parent: WidgetId, child: WidgetId) {
        self.tree.link(parent, child);
    }

    pub fn unlink_child(&mut self, parent: WidgetId, child: WidgetId) {
        self.tree.unlink(parent, child);
    }

    pub fn clear_children_ids(&mut self, parent: WidgetId) {
        self.tree.clear_children(parent);
    }

    pub fn clear_hierarchy(&mut self) {
        self.tree.clear_all();
        self.invalidate_coverage_cache();
    }
}

/// `ctx[h]`: the widget `h` names. Panics if it was removed or is out on loan — use
/// [`UiContext::get`] where either can happen.
impl<W: WidgetHost + 'static> std::ops::Index<Handle<W>> for UiContext {
    type Output = W;
    fn index(&self, h: Handle<W>) -> &W {
        self.get(h).unwrap_or_else(|| panic!("{h:?} is not in the context (removed, or out on loan)"))
    }
}

impl<W: WidgetHost + 'static> std::ops::IndexMut<Handle<W>> for UiContext {
    fn index_mut(&mut self, h: Handle<W>) -> &mut W {
        self.get_mut(h).unwrap_or_else(|| panic!("{h:?} is not in the context (removed, or out on loan)"))
    }
}
