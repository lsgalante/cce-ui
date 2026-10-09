//! Event propagation: an event dispatched into a widget tree by its root's id, the scroll-gesture
//! bookkeeping every routed wheel passes through (a pause starts a new gesture), and finding the
//! scrollable under the pointer.

use super::*;

impl UiContext {
    /// The scroll-gesture bookkeeping every wheel dispatch must pass
    /// through: a gap over 250ms since the last wheel starts a NEW gesture
    /// (`scroll_gesture_new`, and the initiator is cleared), a shorter gap
    /// continues the current one. `propagate_event` calls this for the
    /// wheels it routes; a host that hands a wheel straight to a widget's
    /// `handle_event` (the designer's modal dialog, whose panes it dispatches
    /// itself) calls it first — or the flags stay whatever the last routed
    /// wheel left, and a pane inside the dialog reads a fresh gesture as the
    /// tail of one the MAIN pane owned and lets the control under the pointer
    /// take it (2026-09-20: the Settings list would not scroll after the
    /// params pane had).
    pub fn note_scroll_event(&mut self) {
        let now = web_time::Instant::now();
        let elapsed_ms = match self.last_scroll_time {
            None => 999999,
            Some(last) => now.duration_since(last).as_millis(),
        };
        if elapsed_ms >= 5 {
            let is_new_gesture = elapsed_ms > 250;
            if is_new_gesture {
                self.scroll_initiate_widget_id = None;
                self.scroll_gesture_new = true;
            } else {
                self.scroll_gesture_new = false;
            }
            if crate::scroll_debug() {
                eprintln!(
                    "[scroll] router: gap={elapsed_ms}ms new_gesture={is_new_gesture} initiator={:?}",
                    self.scroll_initiate_widget_id
                );
            }
            self.last_scroll_time = Some(now);
        }
    }

    /// Dispatch an event into the tree rooted at `root` — a `WidgetId` resolved through the
    /// registry (the plumbing retype: the router's last raw-pointer API boundary is gone; apps
    /// name roots by id and the registry is the one place a pointer lives). The root must be
    /// registered — apps already register every widget for focus/coverage — and an
    /// unresolvable root is a loud no-op, never a deref.
    pub fn propagate_event(&mut self, event: &Event, root: WidgetId) -> bool {
        if self.tree.get_ptr(root).is_none() {
            eprintln!("propagate_event: unregistered/stale root {root:?} — event dropped");
            return false;
        }
        if let Event::MouseWheel { .. } = event {
            self.note_scroll_event();
        }
        if let Event::KeyInput(ref key_event) = event {
            let is_scroll_key = matches!(
                &key_event.logical_key,
                Key::Named(NamedKey::PageUp)
                    | Key::Named(NamedKey::PageDown)
                    | Key::Named(NamedKey::Home)
                    | Key::Named(NamedKey::End)
                    | Key::Named(NamedKey::ArrowUp)
                    | Key::Named(NamedKey::ArrowDown)
            );
            if is_scroll_key {
                if let Some(focused) = self.focused_widget {
                    if self.deliver(focused, event) {
                        return true;
                    }
                }
                let (cx, cy) = self.cursor_pos;
                if let Some(scrollable) = self.find_hovered_scrollable(root, cx, cy) {
                    if self.deliver(scrollable, event) {
                        return true;
                    }
                }
            }
        }
        self.propagate_event_impl(event, root)
    }

    /// The dispatch body, by id: each widget it calls is lent for the call.
    pub(super) fn propagate_event_impl(&mut self, event: &Event, root: WidgetId) -> bool {
        if let Event::Tick(_) = event {
            return false;
        }
        // Track drag gestures based on mouse events
        match event {
            Event::MouseButton { button, state, x, y, .. } if *button == MouseButton::Left => {
                if *state == ElementState::Pressed {
                    // Apps re-dispatch the SAME press to several roots (a plain loop
                    // over their top-level widgets); only the first call for a given
                    // press may reset the drag bookkeeping — a later call would wipe
                    // the target an earlier root just armed, killing the drag before
                    // its first move. drag_start_pos is cleared on release, so an
                    // equal position here means "same press, next root".
                    if self.drag_start_pos != Some((*x, *y)) {
                        // A fresh press while a grab is still armed means the
                        // release never arrived (lost to a focus change or eaten
                        // compositor-side). End the stale drag and drop the grab —
                        // otherwise active_grab redirects every event to the old
                        // target forever and the whole UI stops responding.
                        if self.active_grab.is_some() {
                            if self.is_dragging {
                                if let Some(target) = self.drag_target {
                                    self.deliver_dirty(target, &Event::DragEnd);
                                }
                            }
                            self.active_grab = None;
                        }
                        self.drag_start_pos = Some((*x, *y));
                        self.is_dragging = false;
                        self.drag_target = None;
                    }
                } else if *state == ElementState::Released {
                    if self.is_dragging {
                        if let Some(target) = self.drag_target {
                            self.deliver_dirty(target, &Event::DragEnd);
                        }
                        self.active_grab = None;
                    }
                    self.drag_start_pos = None;
                    self.drag_target = None;
                    self.is_dragging = false;
                }
            }
            Event::PointerMove { x, y, .. } => {
                if let Some((sx, sy)) = self.drag_start_pos {
                    if let Some(target_id) = self.drag_target {
                        let (dx, dy) = (*x - sx, *y - sy);
                        if self.is_dragging {
                            if let Some((cx, cy, _, _)) = self.rect_of(target_id) {
                                let drag_evt = Event::DragUpdate { dx, dy, x: *x, y: *y, local_x: *x - cx, local_y: *y - cy };
                                self.deliver_dirty(target_id, &drag_evt);
                            }
                        } else if (dx * dx + dy * dy).sqrt() > 3.0 {
                            self.is_dragging = true;
                            self.active_grab = Some(target_id);
                            self.deliver_dirty(target_id, &Event::DragStart { start_x: sx, start_y: sy });
                        }
                    }
                }
            }
            _ => {}
        }

        // Normal grab redirection for mouse events if active
        if let Some(grabbed_id) = self.active_grab {
            if let Event::PointerMove { .. }
            | Event::MouseButton { .. }
            | Event::MouseWheel { .. }
            | Event::DragStart { .. }
            | Event::DragUpdate { .. }
            | Event::DragEnd = event
            {
                if self.tree.get_ptr(grabbed_id).is_some() {
                    return self.deliver(grabbed_id, event);
                }
            }
        }

        // For KeyInput, send directly to focused widget if it exists
        if let Event::KeyInput(_) = event {
            if let Some(focused) = self.focused_widget {
                if self.deliver(focused, event) {
                    return true;
                }
            }
        }

        let mut handled = false;
        let mut children: Vec<WidgetId> =
            self.tree.child_ids(root).into_iter().filter(|&c| self.tree.get_ptr(c).is_some()).collect();
        children.sort_by_key(|&c| self.get_widget(c).map_or(0, |w| w.z_index()));

        // Determine if we should record a drag target candidate
        let check_drag_target = matches!(
            event,
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, .. }
        );

        let localized = |event: &Event, rect: Option<(f32, f32, f32, f32)>| {
            let (cx, cy, _, _) = rect.unwrap_or_default();
            let mut local_adjusted = event.clone();
            match &mut local_adjusted {
                Event::PointerMove { local_x, local_y, .. }
                | Event::MouseButton { local_x, local_y, .. }
                | Event::MouseWheel { local_x, local_y, .. }
                | Event::DragUpdate { local_x, local_y, .. } => {
                    *local_x -= cx;
                    *local_y -= cy;
                }
                _ => {}
            }
            local_adjusted
        };

        match event {
            Event::PointerMove { .. } | Event::Tick(_) => {
                for child in children.into_iter().rev() {
                    let local_adjusted = localized(event, self.rect_of(child));
                    if self.propagate_event_impl(&local_adjusted, child) {
                        handled = true;
                    }
                }
                if self.deliver(root, event) {
                    handled = true;
                }
            }
            _ => {
                for child in children.into_iter().rev() {
                    let local_adjusted = localized(event, self.rect_of(child));
                    if self.propagate_event_impl(&local_adjusted, child) {
                        if check_drag_target {
                            self.drag_target = Some(child);
                        }
                        return true;
                    }
                }
                if self.deliver(root, event) {
                    if check_drag_target {
                        self.drag_target = Some(root);
                    }
                    return true;
                }
            }
        }
        handled
    }

    pub(super) fn find_hovered_scrollable(&self, root: WidgetId, cx: f32, cy: f32) -> Option<WidgetId> {
        let w = self.get_widget(root)?;
        if !w.visible() || !w.hit_test(cx, cy, self) {
            return None;
        }
        for child in self.tree.child_ids(root).into_iter().rev() {
            if let Some(scrollable) = self.find_hovered_scrollable(child, cx, cy) {
                return Some(scrollable);
            }
        }
        w.is_scrollable().then_some(root)
    }
}
