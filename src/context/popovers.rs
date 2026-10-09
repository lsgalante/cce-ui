//! Popovers (registering, closing on a press that misses them, which one is under a point),
//! coverage, the cursor position, and the hit tests that ask them.

use super::*;

impl UiContext {
    pub fn clear_popovers(&mut self) {
        self.active_popovers.clear();
        self.invalidate_coverage_cache();
    }

    /// Close any open popover whose owner the press MISSED — the engine calls
    /// this on every Left press before the app's dispatch, so an outside click
    /// always reaches an open menu even in apps that region-gate their event
    /// routing (a canvas click never reaching a sidebar dropdown's root).
    /// Scans the whole registry (like `is_coordinate_covered`'s fallback) —
    /// popover registration is optional and spotty across apps. A press ON the
    /// owner (trigger or popover) is left entirely to the app's own dispatch:
    /// its `take_change` plumbing is gated on that delivery. Owners receive the
    /// real press event, so their ordinary outside-press handling runs; a
    /// second delivery through the app's own dispatch is idempotent (a closing
    /// dropdown ignores further presses).
    pub fn close_popovers_missed_by_press(&mut self, x: f32, y: f32) {
        self.close_popovers_missed_by_press_with(x, y, |_| (0.0, 0.0));
    }

    /// The widgets with an open popover, in registry order.
    pub fn popover_owners(&self) -> Vec<WidgetId> {
        self.tree
            .iter_registered()
            .filter_map(|(id, ptr)| unsafe {
                ptr.as_ref().and_then(|w| {
                    (w.visible() && w.popover_rect().is_some()).then_some(id)
                })
            })
            .collect()
    }

    /// [`close_popovers_missed_by_press`](Self::close_popovers_missed_by_press)
    /// for a press in SURFACE coordinates: `offset` is where each owner is
    /// drawn relative to where it was laid out
    /// (`Application::popover_offset`), and the press is carried back into
    /// the owner's own coordinates before it is tested and delivered. On a
    /// scrolled page the unshifted test called a press on a menu row a miss,
    /// and closed the menu under the click.
    pub fn close_popovers_missed_by_press_with(&mut self, x: f32, y: f32, offset: impl Fn(WidgetId) -> (f32, f32)) {
        for id in self.popover_owners() {
            let Some(w) = self.get_widget(id) else { continue };
            let (dx, dy) = offset(id);
            let (x, y) = (x - dx, y - dy);
            if !w.hit_test(x, y, self) {
                let ev = Event::MouseButton {
                    button: crate::widget::MouseButton::Left,
                    state: crate::widget::ElementState::Pressed,
                    x,
                    y,
                    local_x: x,
                    local_y: y,
                };
                self.lend(id, |w, ctx| {
                    w.handle_event(&ev, ctx);
                });
            }
        }
    }

    /// The registered widget whose OPEN popover contains `(x, y)`, if any — the
    /// press-priority companion to
    /// [`close_popovers_missed_by_press`](Self::close_popovers_missed_by_press).
    /// A popover paints OVER whatever sits beneath it, but positional dispatch
    /// knows nothing about z-order: an app iterating its roots can hand the
    /// press to a closed sibling whose trigger band lies under the open menu
    /// (the Default Apps page's Terminal dropdown covering the Images row).
    /// Apps route a `MouseButton` to this owner before their positional
    /// dispatch. Scans the registry like the missed-press walk — popover
    /// registration is optional and spotty, so `active_popovers` alone cannot
    /// be trusted to know about every open menu.
    pub fn popover_owner_at(&self, x: f32, y: f32) -> Option<WidgetId> {
        self.tree.iter_registered().find_map(|(id, ptr)| unsafe {
            ptr.as_ref().and_then(|w| {
                if !w.visible() {
                    return None;
                }
                let (rx, ry, rw, rh) = w.popover_rect()?;
                (x >= rx && x <= rx + rw && y >= ry && y <= ry + rh).then_some(id)
            })
        })
    }

    /// Register the open popover of the widget `id` names (the occlusion walks resolve it
    /// through the tree).
    pub fn register_popover_id(&mut self, id: WidgetId) {
        if !self.active_popovers.contains(&id) {
            self.active_popovers.push(id);
        }
        self.invalidate_coverage_cache();
    }

    /// Whether `(px, py)` is covered by an open popover or a popover-carrying widget other
    /// than `query_id` (the querying widget excludes itself). Every widget has a base id
    /// now (the flip) — the old `WidgetId(0)` no-base sentinel is gone.
    /// Is `(px, py)` covered by some widget's popover rect other than `query_id` — or does
    /// `query_id` lie behind an open modal ([`UiContext::open_modal`]), where everything is?
    ///
    /// The covering set depends only on the point, so it is computed once and
    /// memoized; `query_id` is applied afterwards as an exclusion. See the
    /// `covered_at` field for why the previous per-call registry scan mattered.
    pub fn is_coordinate_covered(&self, query_id: WidgetId, px: f32, py: f32) -> bool {
        // Behind an open modal everything is covered, wherever the point is.
        if !self.in_modal_scope(query_id) {
            return true;
        }
        let mut cache = self.covered_cache.borrow_mut();
        if cache.0 != Some((px, py)) {
            cache.1.clear();
            for &pop_id in self.active_popovers.iter() {
                if let Some(ptr) = self.tree.get_ptr(pop_id) {
                    unsafe {
                        if let Some((x, y, width, height)) = (*ptr).popover_rect() {
                            if px >= x && px <= x + width && py >= y && py <= y + height {
                                cache.1.push(pop_id);
                            }
                        }
                    }
                }
            }
            for (id, ptr) in self.tree.iter_registered() {
                unsafe {
                    if let Some(w) = ptr.as_ref() {
                        if w.visible() {
                            if let Some((x, y, width, height)) = w.popover_rect() {
                                if px >= x && px <= x + width && py >= y && py <= y + height {
                                    cache.1.push(id);
                                }
                            }
                        }
                    }
                }
            }
            cache.0 = Some((px, py));
        }
        cache.1.iter().any(|&id| id != query_id)
    }

    /// Drop the `is_coordinate_covered` memo — whenever the registry changes or
    /// a popover's geometry may have moved under a stationary cursor.
    pub fn invalidate_coverage_cache(&self) {
        let mut cache = self.covered_cache.borrow_mut();
        cache.0 = None;
        cache.1.clear();
    }

    // The hover highlight is `widget::hover_animation`'s (one store; the context's
    // write-only copy went with the global-state RFC's phase 1). The cursor is the
    // context's: the scroll box reads it.
    pub fn set_cursor_pos(&mut self, x: f32, y: f32) {
        self.cursor_pos = (x, y);
    }

    /// Whether the point lies inside an OPEN popover's plate. Popovers are drawn on top of
    /// everything and are interactive UI, but they are not spatial-grid widgets — a press
    /// there must never start a window move (the widgets beneath may not block dragging,
    /// e.g. Graph's edge-exclusive canvas hit test).
    pub(super) fn point_in_active_popover(&self, px: f32, py: f32) -> bool {
        // The shared context menu is a popover too — a thread-local one,
        // with no widget id to register — and a press on one of its rows
        // used to start a window move in any app whose background does
        // not block dragging (cce-graph, cce-data-editor: the row never
        // fired, the window slid).
        if crate::widget::context_menu::is_visible() && crate::widget::context_menu::hit_test(px, py) {
            return true;
        }
        for &pop_id in &self.active_popovers {
            if let Some(ptr) = self.tree.get_ptr(pop_id) {
                unsafe {
                    if let Some((x, y, w, h)) = (*ptr).popover_rect() {
                        if px >= x && px <= x + w && py >= y && py <= y + h {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// The window-drag question (Phase 6: every root plate container is dissolved, so the surface
    /// itself is the movable plate): a drag may start anywhere no drag-blocking widget sits
    /// under the cursor.
    pub fn drag_allowed_at(&self, px: f32, py: f32) -> bool {
        if self.point_in_active_popover(px, py) {
            return false;
        }
        let scroll_y = crate::widget::hover_animation::get_scroll_offset();
        let mut candidate_ids = self.spatial_grid.query(px, py).to_vec();
        if scroll_y != 0.0 {
            candidate_ids.extend_from_slice(self.spatial_grid.query(px, py + scroll_y));
            candidate_ids.sort_unstable();
            candidate_ids.dedup();
        }
        for &id in &candidate_ids {
            if let Some(ptr) = self.tree.get_ptr(id) {
                unsafe {
                    if !ptr.is_null() {
                        let w = &*ptr;
                        let is_hit = w.hit_test(px, py, self)
                            || (scroll_y != 0.0 && w.hit_test(px, py + scroll_y, self));
                        if is_hit && w.blocks_root_plate_drag() {
                            return false;
                        }
                    }
                }
            }
        }
        true
    }

    pub fn is_widget_at(&self, px: f32, py: f32) -> bool {
        let scroll_y = crate::widget::hover_animation::get_scroll_offset();
        let mut candidate_ids = self.spatial_grid.query(px, py).to_vec();
        if scroll_y != 0.0 {
            candidate_ids.extend_from_slice(self.spatial_grid.query(px, py + scroll_y));
            candidate_ids.sort_unstable();
            candidate_ids.dedup();
        }
        for &id in &candidate_ids {
            if let Some(ptr) = self.tree.get_ptr(id) {
                unsafe {
                    if !ptr.is_null() {
                        let w = &*ptr;
                        let is_hit = w.hit_test(px, py, self) || (scroll_y != 0.0 && w.hit_test(px, py + scroll_y, self));
                        if is_hit && w.blocks_root_plate_drag() {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}
