//! Focus by widget id, modal scopes, and the Tab walk: the focus stops in reading order, a
//! group's members as one run, and stepping between stops and between runs.

use super::*;

impl UiContext {
    pub fn set_focused_id(&mut self, id: WidgetId) {
        if self.focused_widget == Some(id) {
            return;
        }
        if let Some(old_id) = self.focused_widget {
            self.lend(old_id, |w, ctx| {
                w.unfocus();
                w.handle_event(&Event::FocusOut, ctx);
            });
        }
        self.focused_widget = Some(id);
        // A widget focusing itself mid-event is out on loan: it is not told (`claim_focus`).
        self.lend(id, |w, ctx| {
            w.handle_event(&Event::FocusIn, ctx);
        });
    }

    pub fn is_focused(&self, w: &dyn WidgetHost) -> bool {
        self.is_focused_id(w.base().id())
    }

    pub fn is_focused_id(&self, id: WidgetId) -> bool {
        self.focused_widget == Some(id)
    }

    /// Offer a context action to the focused widget — the runner's first stop
    /// for the `undo` / `redo` chords. Returns whether the widget applied it;
    /// a widget that did is marked dirty.
    pub fn focused_context_action(&mut self, action: crate::widget::ContextAction) -> bool {
        let Some(id) = self.focused_widget else { return false };
        self.lend(id, |w, ctx| {
            let applied = w.context_action(action);
            if applied {
                w.mark_dirty(ctx);
            }
            applied
        })
        .unwrap_or(false)
    }

    pub fn clear_focus(&mut self) {
        if let Some(id) = self.focused_widget.take() {
            self.lend(id, |w, ctx| {
                w.unfocus();
                w.handle_event(&Event::FocusOut, ctx);
            });
        }
    }

    /// Open a modal owned by `owner` (a `Dialog`) around `members`: the Tab walk is trapped
    /// among them (and their embedded children), every widget outside reads as covered
    /// ([`UiContext::is_coordinate_covered`], which every hit test and hover asks), and focus
    /// moves to the first stop inside, remembering where it was. Modals nest; the innermost
    /// rules. Opening one already open replaces its members and keeps its focus memory.
    pub fn open_modal(&mut self, owner: WidgetId, members: Vec<WidgetId>) {
        self.invalidate_coverage_cache();
        if let Some(scope) = self.modals.iter_mut().find(|m| m.owner == owner) {
            scope.members = members;
            return;
        }
        let restore = self.focused_widget;
        self.modals.push(ModalScope { owner, members, restore });
        match self.focus_stops().first() {
            Some(&first) => self.set_focused_id(first),
            None => self.clear_focus(),
        }
    }

    /// Close the modal `owner` opened, giving focus back to what had it before (if that is
    /// still registered and reachable), else to nothing.
    pub fn close_modal(&mut self, owner: WidgetId) {
        let Some(i) = self.modals.iter().position(|m| m.owner == owner) else { return };
        let scope = self.modals.remove(i);
        self.invalidate_coverage_cache();
        let inside = |ctx: &Self, id: WidgetId| ctx.tree.is_registered(id) && ctx.in_modal_scope(id);
        match scope.restore.filter(|&id| inside(self, id)) {
            Some(id) => self.set_focused_id(id),
            None => self.clear_focus(),
        }
    }

    /// The owner of the innermost open modal.
    pub fn modal_owner(&self) -> Option<WidgetId> {
        self.modals.last().map(|m| m.owner)
    }

    /// Whether `id` may take input: true with no modal open; with one, true for the modal's
    /// owner, its members and anything under them in the tree.
    pub fn in_modal_scope(&self, id: WidgetId) -> bool {
        let Some(scope) = self.modals.last() else { return true };
        let mut at = Some(id);
        // Bounded: a malformed parent chain must not hang the walk.
        for _ in 0..64 {
            let Some(cur) = at else { return false };
            if cur == scope.owner || scope.members.contains(&cur) {
                return true;
            }
            at = self.tree.parent_id(cur);
        }
        false
    }

    /// Make `id` the window's focus from INSIDE that widget's own event handling: recorded,
    /// and the holder before told (`unfocus`), but no FocusIn delivered back to `id` —
    /// it is running, holding `&mut self`, and a FocusIn through the registry would be a
    /// second `&mut` to it while the first is live. What a widget that focuses itself does
    /// (`EventCtx::request_focus`; a tree list on a click into its rows), setting whatever
    /// its FocusIn would have set itself.
    pub fn claim_focus(&mut self, id: WidgetId) {
        if let Some(old) = self.focused_widget.filter(|old| *old != id) {
            if let Some(ptr) = self.tree.get_ptr(old) {
                // SAFETY: a registry-resolved live widget other than the claimant.
                unsafe { (*ptr).unfocus() };
            }
        }
        self.focused_widget = Some(id);
    }

    /// Focus `id` as a direct `w.focus()` did — the widget is told (`focus`), the holder
    /// before it is told it lost focus (`unfocus`) — and record it as the window's focus,
    /// which a direct call never did: the Tab walk and the accessibility tree read the
    /// record. For an app that drives a widget's focus itself (`docs/rfc-global-state.md`,
    /// phase 2); a focus change the context should announce with FocusIn / FocusOut is
    /// [`set_focused_id`](Self::set_focused_id).
    pub fn focus_id(&mut self, id: WidgetId) {
        if let Some(old) = self.focused_widget.filter(|old| *old != id) {
            if let Some(w) = self.get_widget_mut(old) {
                w.unfocus();
            }
        }
        self.focused_widget = Some(id);
        if let Some(w) = self.get_widget_mut(id) {
            w.focus();
        }
    }

    /// Unfocus `id` as a direct `w.unfocus()` did, and drop the window's record of focus if
    /// it was `id` — which a direct call never did, leaving the Tab walk and the
    /// accessibility tree on a widget that had let go.
    pub fn unfocus_id(&mut self, id: WidgetId) {
        if self.focused_widget == Some(id) {
            self.focused_widget = None;
        }
        if let Some(w) = self.get_widget_mut(id) {
            w.unfocus();
        }
    }

    pub fn has_focus(&self) -> bool {
        self.focused_widget.is_some()
    }

    /// The keyboard stops in reading order (row, then x): the registered,
    /// visible, on-screen widgets with a `focus_role`. Rows are bucketed by
    /// vertical overlap, so a short control centred beside a taller one is on
    /// its row. `CCE_FOCUS_DEBUG=1` prints them.
    pub(super) fn focus_stops(&self) -> Vec<WidgetId> {
        // (y, bottom, x, id) per stop.
        let mut found: Vec<(f32, f32, f32, WidgetId)> = Vec::new();
        for (id, ptr) in self.tree.iter_registered() {
            if ptr.is_null() {
                continue;
            }
            let w = unsafe { &*ptr };
            if w.focus_role() == crate::widget::FocusRole::None || !w.visible() || !self.in_modal_scope(id) {
                continue;
            }
            let (x, y, width, height) = w.rect();
            if width <= 0.0 || height <= 0.0 {
                continue;
            }
            // Parked off-screen (the hidden-editor idiom: a 1x1 rect at
            // (-1000, -1000)) — nothing to see, so not a stop.
            if x + width <= 0.0 || y + height <= 0.0 {
                continue;
            }
            found.push((y, y + height, x, id));
        }
        if found.is_empty() {
            if std::env::var_os("CCE_FOCUS_DEBUG").is_some() {
                eprintln!("[focus] no stops: no registered, visible widget with a focus role and a rect");
            }
            return Vec::new();
        }
        // Reading order: rows first, x within a row. A stop joins the current
        // row when its top lies above the row's first stop's bottom — a 12px
        // checkbox centred a few px below the 26px button beside it is on the
        // button's row, not a row of its own.
        found.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut rows: Vec<Vec<(f32, f32, f32, WidgetId)>> = Vec::new();
        for s in found {
            match rows.last_mut() {
                Some(row) if s.0 < row[0].1 => row.push(s),
                _ => rows.push(vec![s]),
            }
        }
        let mut stops: Vec<WidgetId> = Vec::new();
        for mut row in rows {
            row.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
            stops.extend(row.into_iter().map(|s| s.3));
        }
        stops
    }

    /// The stops in WALK order, clustered: a registered `Group`'s members are
    /// one contiguous run, placed where the group's first member falls in
    /// reading order and ordered among themselves by reading order; every
    /// other stop is a run of one. A member of two groups belongs to the
    /// group that comes first. Tab walks the runs end to end
    /// (`focus_step`); the group chords jump between them (`focus_step_group`).
    pub fn focus_clusters(&self) -> Vec<Vec<WidgetId>> {
        let stops = self.focus_stops();
        if stops.is_empty() {
            return Vec::new();
        }
        // Each group's member positions among the stops, groups ordered by
        // their first member.
        let mut groups: Vec<Vec<usize>> = Vec::new();
        for (_, ptr) in self.tree.iter_registered() {
            if ptr.is_null() {
                continue;
            }
            let w = unsafe { &*ptr };
            let Some(g) = w.as_any().downcast_ref::<crate::widget::Group>() else { continue };
            let mut pos: Vec<usize> = g.members().iter().filter_map(|m| stops.iter().position(|s| s == m)).collect();
            pos.sort_unstable();
            pos.dedup();
            if !pos.is_empty() {
                groups.push(pos);
            }
        }
        groups.sort_by_key(|p| p[0]);
        let mut claimed = vec![false; stops.len()];
        let mut clusters: Vec<(usize, Vec<WidgetId>)> = Vec::new();
        for pos in groups {
            let free: Vec<usize> = pos.into_iter().filter(|&i| !claimed[i]).collect();
            if free.is_empty() {
                continue;
            }
            for &i in &free {
                claimed[i] = true;
            }
            clusters.push((free[0], free.iter().map(|&i| stops[i]).collect()));
        }
        for (i, id) in stops.iter().enumerate() {
            if !claimed[i] {
                clusters.push((i, vec![*id]));
            }
        }
        clusters.sort_by_key(|c| c.0);
        let clusters: Vec<Vec<WidgetId>> = clusters.into_iter().map(|c| c.1).collect();
        // CCE_FOCUS_DEBUG=1: the runs in walk order, with what each stop is.
        if std::env::var_os("CCE_FOCUS_DEBUG").is_some() {
            for (ci, run) in clusters.iter().enumerate() {
                for (i, id) in run.iter().enumerate() {
                    if let Some(ptr) = self.tree.get_ptr(*id) {
                        let w = unsafe { &*ptr };
                        let (x, y, width, height) = w.rect();
                        eprintln!(
                            "[focus] run {ci} stop {i}: {} {:?} at ({x:.0},{y:.0} {width:.0}x{height:.0}){}",
                            w.type_name(),
                            w.focus_role(),
                            if self.focused_widget == Some(*id) { " <- focused" } else { "" }
                        );
                    }
                }
            }
        }
        clusters
    }

    /// Keyboard navigation in plate terms (see "Plates, wells and seams" in
    /// `CLAUDE.md`): move focus to the next (`reverse` = previous) plate or
    /// well in walk order — reading order, a group's members walked together
    /// (`focus_clusters`). The traversal wraps, and with nothing focused the
    /// first (or last) stop takes it. Focusing goes through `set_focused_id`,
    /// so the new stop gets its `FocusIn` — a well opens for typing, a plate
    /// arms Enter / Space. Returns whether focus moved. The runner calls this
    /// for Tab when the app opts in (`Application::plate_navigation`).
    pub fn focus_step(&mut self, reverse: bool) -> bool {
        let stops: Vec<WidgetId> = self.focus_clusters().into_iter().flatten().collect();
        if stops.is_empty() {
            return false;
        }
        let n = stops.len();
        let current = self.focused_widget.and_then(|f| stops.iter().position(|s| *s == f));
        let next = match (current, reverse) {
            (Some(i), false) => (i + 1) % n,
            (Some(i), true) => (i + n - 1) % n,
            (None, false) => 0,
            (None, true) => n - 1,
        };
        let id = stops[next];
        if self.focused_widget == Some(id) {
            return false;
        }
        self.set_focused_id(id);
        true
    }

    /// Jump to the next (`reverse` = previous) run of `focus_clusters` — the
    /// next group, or the next ungrouped stop — landing on its first stop;
    /// wraps. The runner calls this for the `focus_next_group` /
    /// `focus_prev_group` chords (input.kdl, cce-ui domain; defaults
    /// `ctrl+tab` / `ctrl+shift+tab`) when the app opts in.
    pub fn focus_step_group(&mut self, reverse: bool) -> bool {
        let clusters = self.focus_clusters();
        if clusters.is_empty() {
            return false;
        }
        let n = clusters.len();
        let current = self.focused_widget.and_then(|f| clusters.iter().position(|c| c.contains(&f)));
        let next = match (current, reverse) {
            (Some(i), false) => (i + 1) % n,
            (Some(i), true) => (i + n - 1) % n,
            (None, false) => 0,
            (None, true) => n - 1,
        };
        let id = clusters[next][0];
        if self.focused_widget == Some(id) {
            return false;
        }
        self.set_focused_id(id);
        true
    }
}
