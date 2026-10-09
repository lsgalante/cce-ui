use std::collections::HashMap;
use crate::widget::{Handle, WidgetHost, WidgetId, Key, NamedKey, MouseButton, ElementState, Event, WidgetHostExt};

pub struct SpatialGrid {
    pub cell_size: f32,
    pub cells: HashMap<(i32, i32), Vec<WidgetId>>,
}

impl SpatialGrid {
    pub fn new(cell_size: f32) -> Self {
        Self {
            cell_size,
            cells: HashMap::new(),
        }
    }

    pub fn clear(&mut self) {
        self.cells.clear();
    }

    pub fn insert(&mut self, id: WidgetId, rect: (f32, f32, f32, f32)) {
        let (x, y, w, h) = rect;
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let start_x = (x / self.cell_size).floor() as i32;
        let end_x = ((x + w) / self.cell_size).floor() as i32;
        let start_y = (y / self.cell_size).floor() as i32;
        let end_y = ((y + h) / self.cell_size).floor() as i32;

        let start_x = start_x.max(-1000);
        let end_x = end_x.min(1000);
        let start_y = start_y.max(-1000);
        let end_y = end_y.min(1000);

        for cx in start_x..=end_x {
            for cy in start_y..=end_y {
                self.cells.entry((cx, cy)).or_default().push(id);
            }
        }
    }

    pub fn query(&self, px: f32, py: f32) -> &[WidgetId] {
        let cx = (px / self.cell_size).floor() as i32;
        let cy = (py / self.cell_size).floor() as i32;
        self.cells.get(&(cx, cy)).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

/// `register_host` was handed a widget outside an `Owned` box: say so once per widget type, on
/// stderr (most apps install no logger), so a missed field shows up without failing anything.
fn warn_unowned(type_name: &'static str) {
    thread_local! {
        static WARNED: std::cell::RefCell<std::collections::HashSet<&'static str>> = Default::default();
    }
    if WARNED.with(|w| w.borrow_mut().insert(type_name)) {
        eprintln!(
            "cce-ui: register_host: a {type_name} is registered outside an Owned box; it must \
             not move while registered (hold it as cce_ui::widget::Owned<..>)"
        );
    }
}

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

    // ── Widgets the context owns, by handle (docs/rfc-owning-registry.md) ──────────────

    /// Take ownership of `widget`, register it under its own id, and hand back its handle.
    /// The widget lives in the context from here on; reach it with [`get`](Self::get) /
    /// [`get_mut`](Self::get_mut) (or `ctx[h]`), give it back with [`remove`](Self::remove).
    pub fn insert<W: WidgetHost + 'static>(&mut self, widget: W) -> Handle<W> {
        // A widget that animates is ticked by the context (`tick`), as one registered by
        // pointer is (`register_widget`): a tree list applies its search there.
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

    /// Dispatch an event into the tree rooted at `root` — a `WidgetId` resolved through the
    /// registry (the plumbing retype: the router's last raw-pointer API boundary is gone; apps
    /// name roots by id and the registry is the one place a pointer lives). The root must be
    /// registered — apps already register every widget for focus/coverage — and an
    /// unresolvable root is a loud no-op, never a deref.
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
    fn propagate_event_impl(&mut self, event: &Event, root: WidgetId) -> bool {
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
                if let Some(parent_ptr) = self.tree.get_ptr(parent_id) {
                    unsafe {
                        if !(*parent_ptr).is_child_visible(curr) {
                            return false;
                        }
                    }
                }
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

    // --- Focus management (id-keyed; Phase 6bc) ---
    pub fn set_focused(&mut self, w: &mut dyn WidgetHost) {
        let id = w.base().id();
        // Refresh the registry with the pointer we were just handed, so focus on a
        // not-yet-registered widget keeps working (the legacy code stored this pointer
        // directly; the id must resolve for FocusOut/KeyInput dispatch to reach it).
        let new_ptr = unsafe {
            std::mem::transmute::<*mut dyn WidgetHost, *mut (dyn WidgetHost + 'static)>(w as *mut dyn WidgetHost)
        };
        // SAFETY: derived from the live borrow we were handed.
        unsafe { self.tree.register(id, new_ptr) };
        self.set_focused_id(id);
    }

    /// Transitional pointer form (TreeList focuses its adapter via `EventCtx::host_ptr`).
    ///
    /// # Safety
    ///
    /// `new_ptr` must be null or point to a live widget at the call. It is read to derive the
    /// id and refresh the registry (see [`WidgetTree::register`](crate::scene::tree::WidgetTree::register)).
    pub unsafe fn set_focused_ptr(&mut self, new_ptr: *mut (dyn WidgetHost + 'static)) {
        if new_ptr.is_null() {
            return;
        }
        // SAFETY: the caller's contract.
        let id = unsafe { (*new_ptr).base().id() };
        unsafe { self.tree.register(id, new_ptr) };
        self.set_focused_id(id);
    }

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

    /// Focus `w` as a direct `w.focus()` did — the widget is told (`focus`), the holder before
    /// it is told it lost focus (`unfocus`) — and record it as the window's focus, which a
    /// direct call never did: the Tab walk and the accessibility tree read the record, and
    /// went on pointing at the widget before. For an app that drives a widget's focus
    /// itself (`docs/rfc-global-state.md`, phase 2); a focus change the context should
    /// announce with FocusIn / FocusOut is [`set_focused_id`](Self::set_focused_id).
    pub fn focus_widget(&mut self, w: &mut dyn WidgetHost) {
        let id = w.base().id();
        if let Some(old) = self.focused_widget.filter(|old| *old != id) {
            if let Some(ptr) = self.tree.get_ptr(old) {
                // SAFETY: a registry-resolved live widget, not `w` (a different id).
                unsafe { (*ptr).unfocus() };
            }
        }
        self.focused_widget = Some(id);
        w.focus();
    }

    /// Unfocus `w` as a direct `w.unfocus()` did, and drop the window's record of focus if
    /// it was `w` — which a direct call never did, leaving the Tab walk and the
    /// accessibility tree on a widget that had let go.
    pub fn unfocus_widget(&mut self, w: &mut dyn WidgetHost) {
        if self.focused_widget == Some(w.base().id()) {
            self.focused_widget = None;
        }
        w.unfocus();
    }

    /// [`focus_widget`](Self::focus_widget) by id — for a widget the context owns.
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

    /// [`unfocus_widget`](Self::unfocus_widget) by id — for a widget the context owns.
    pub fn unfocus_id(&mut self, id: WidgetId) {
        if self.focused_widget == Some(id) {
            self.focused_widget = None;
        }
        if let Some(w) = self.get_widget_mut(id) {
            w.unfocus();
        }
    }

    pub fn clear_if_matches(&mut self, w: &dyn WidgetHost) {
        if self.focused_widget == Some(w.base().id()) {
            self.focused_widget = None;
        }
    }

    pub fn has_focus(&self) -> bool {
        self.focused_widget.is_some()
    }

    /// The keyboard stops in reading order (row, then x): the registered,
    /// visible, on-screen widgets with a `focus_role`. Rows are bucketed by
    /// vertical overlap, so a short control centred beside a taller one is on
    /// its row. `CCE_FOCUS_DEBUG=1` prints them.
    fn focus_stops(&self) -> Vec<WidgetId> {
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

    // `navigate_focus` (tree-walk ctrl-nav) is DELETED (the plumbing retype): it had
    // zero callers — its `focus::navigate_focus` twin was the one wired up, and that one
    // walked an empty dummy context (provably inert). Section-level keyboard nav lives
    // app-side (settings' focused_section machinery).

    /// Register a widget the app owns, by reference: `ctx.register_host(&mut self.button)`.
    ///
    /// Hold the widget in an [`Owned`](crate::widget::Owned) box: the registry then points at
    /// the boxed widget, which stays put however the field or `Vec` holding the `Owned` moves,
    /// and stops resolving it when the `Owned` drops. A bare widget is registered at its own
    /// address, which is only good until it moves — so that is logged, once per widget type.
    pub fn register_host(&mut self, w: &mut (dyn WidgetHost + 'static)) {
        if w.stable_target().is_none() {
            warn_unowned(w.type_name());
        }
        let id = w.base().id();
        // SAFETY: derived from the live borrow we were handed.
        unsafe { self.register_widget(id, w as *mut (dyn WidgetHost + 'static)) };
    }

    /// Register a widget that lives INSIDE another registered widget (a tree list's search box,
    /// a paginator's menu): it is as stable as its parent's allocation, so no `Owned` of its own
    /// is wanted and none is warned about.
    pub(crate) fn register_embedded(&mut self, w: &mut (dyn WidgetHost + 'static)) {
        let id = w.base().id();
        // SAFETY: derived from the live borrow we were handed.
        unsafe { self.register_widget(id, w as *mut (dyn WidgetHost + 'static)) };
    }

    /// Register a widget by raw pointer. Prefer [`register_host`](Self::register_host), which
    /// takes a reference; this form is for the toolkit's own pointer-routed paths.
    ///
    /// # Safety
    ///
    /// `ptr` must be null or point to a live widget at the call; it is read here (see
    /// [`WidgetTree::register`](crate::scene::tree::WidgetTree::register)).
    pub unsafe fn register_widget(&mut self, id: WidgetId, ptr: *mut (dyn WidgetHost + 'static)) {
        // SAFETY: the caller's contract.
        unsafe { self.tree.register(id, ptr) };
        // A newcomer may itself have a popover rect, so the coverage memo can no
        // longer be trusted. Pages that re-register a whole list do it before
        // dispatching, so the memo is rebuilt once and then serves every root.
        self.invalidate_coverage_cache();
        unsafe {
            if !ptr.is_null() && (*ptr).wants_tick() {
                self.register_tick_receiver(id);
            }
        }
    }

    /// Drop `id`'s registration. **Apps that rebuild a `Vec` of widgets must call this for the
    /// outgoing ids**, because `WidgetId`s are globally monotonic (`NEXT_WIDGET_ID.fetch_add`)
    /// and are never reused: the replacements register under *new* ids, so re-registering does
    /// not overwrite the old entries. Those keep raw pointers into the freed Vec, and several
    /// paths walk the whole registry and dereference — `close_popovers_missed_by_press` runs on
    /// every left press (`backend/window_runner.rs`), and `is_coordinate_covered` falls back to a
    /// full scan — so a stale entry is a use-after-free, not just a leak.
    ///
    /// Apps that call [`clear_hierarchy`](Self::clear_hierarchy) every rebuild do not need this;
    /// the wipe already drops the outgoing ids.
    pub fn unregister_widget(&mut self, id: WidgetId) {
        self.tree.remove(id);
        self.unregister_tick_receiver(id);
        self.invalidate_coverage_cache();
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

    // --- Popovers ---
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

    /// Register an open popover. Takes `&mut` so the registry can be refreshed with the
    /// pointer we are handed (the occlusion walks resolve the stored id through the tree).
    /// [`register_popover`](Self::register_popover) for a widget already registered — one
    /// the context owns, named by its handle's id.
    pub fn register_popover_id(&mut self, id: WidgetId) {
        if !self.active_popovers.contains(&id) {
            self.active_popovers.push(id);
        }
        self.invalidate_coverage_cache();
    }

    pub fn register_popover(&mut self, w: &mut (dyn WidgetHost + 'static)) {
        let id = w.base().id();
        // SAFETY: derived from the live borrow we were handed.
        unsafe { self.tree.register(id, w as *mut (dyn WidgetHost + 'static)) };
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


    // NOTE: the animated hover-highlight for this context previously lived here as
    // `tick_hover` / `get_hover_quad`, duplicating the live thread-local implementation in
    // widget/core.rs (`hover_animation`). Both were dead (zero callers workspace-wide) and were
    // removed; the single source of truth is `hover_animation`. This will be folded into the
    // Animated<T> primitive in the core rebuild (see cce-ui/docs/rfc-core-rebuild.md, Phase 4).

    // --- Context Menu ---
    pub fn is_context_menu_visible(&self) -> bool {
        crate::widget::context_menu::is_visible()
    }

    /// Open the shared context menu on `target`.
    ///
    /// # Safety
    ///
    /// `target` must be null or point to a live widget at the call.
    pub unsafe fn show_context_menu(&mut self, x: f32, y: f32, options: Vec<String>, header_count: usize, target: *mut (dyn WidgetHost + 'static)) {
        if target.is_null() {
            return;
        }
        // SAFETY: the caller's contract.
        let id = unsafe { (*target).base().id() };
        unsafe { self.tree.register(id, target) };
        crate::widget::context_menu::show(x, y, options, header_count, id);
    }

    /// [`show_context_menu`](Self::show_context_menu) with each row's action beside its
    /// label (`None` for a header, or a row the host handles itself), so the label is only
    /// what is shown and a translated menu still does what it did.
    ///
    /// # Safety
    ///
    /// As for `show_context_menu`: `target` is null or a live widget at the call.
    pub unsafe fn show_context_menu_rows(&mut self, x: f32, y: f32, rows: Vec<(String, Option<crate::widget::ContextAction>)>, header_count: usize, target: *mut (dyn WidgetHost + 'static)) {
        let (options, actions): (Vec<String>, Vec<Option<crate::widget::ContextAction>>) = rows.into_iter().unzip();
        // SAFETY: the caller's contract, passed on.
        unsafe { self.show_context_menu(x, y, options, header_count, target) };
        crate::widget::context_menu::set_row_actions(actions);
    }

    /// Open the shared config context menu for a right-click on `target`.
    ///
    /// # Safety
    ///
    /// `target` must be null or point to a live widget at the call.
    pub unsafe fn handle_right_click(&mut self, target: *mut (dyn WidgetHost + 'static), px: f32, py: f32) {
        if target.is_null() {
            return;
        }
        let name = unsafe { (*target).type_name() };
        let label = if name == "Breadcrumb" {
            unsafe {
                if let Some(bc) = (*target).as_any().downcast_ref::<crate::widget::container::Breadcrumb>() {
                    let idx = bc.right_clicked_seg.unwrap_or(bc.path.len());
                    Some(bc.path_to_seg(idx))
                } else {
                    None
                }
            }
        } else {
            unsafe { (*target).label() }
        };
        let header = if let Some(lbl) = label {
            format!("[{}]: {}", name, lbl)
        } else {
            format!("[{}]", name)
        };

        let mut config_info = None;
        {
            let b = unsafe { (*target).base() };
            if let (Some(ref file), Some(ref key)) = (&b.config_file, &b.config_key) {
                config_info = Some((file.clone(), key.clone()));
            }
        }

        use crate::widget::ContextAction as CA;
        use crate::l10n::{tr, tr_args};
        // Each row's label in the user's language; what it does is its action, not its words.
        let row = |label: String, action: CA| (label, Some(action));
        let mut rows: Vec<(String, Option<CA>)> = vec![(header, None)];
        let mut header_count = 1;

        if let Some((file, key)) = config_info {
            rows.push((tr_args("menu-config-file", &[("file", &file)]), None));
            rows.push((tr_args("menu-config-key", &[("key", &key)]), None));
            header_count = 3;
        }

        if name == "TextBox" {
            let is_password = unsafe {
                (*target)
                    .as_any()
                    .downcast_ref::<crate::widget::input::TextBox>()
                    .is_some_and(|tb| tb.is_password)
            };
            if !is_password {
                rows.extend([row(tr("menu-cut"), CA::Cut), row(tr("menu-copy"), CA::Copy)]);
            }
            rows.extend([row(tr("menu-paste"), CA::Paste), row(tr("menu-select-all"), CA::SelectAll)]);
            // A search box says so (`with_search`); an English "Search..." placeholder is the
            // older sign, still read for the apps that set one themselves.
            let is_search = unsafe {
                if let Some(tb) = (*target).as_any().downcast_ref::<crate::widget::input::TextBox>() {
                    tb.is_search || tb.placeholder.as_deref() == Some("Search...")
                } else {
                    false
                }
            };
            if is_search {
                rows.push(row(tr("menu-clear"), CA::ClearText));
            }
        } else if name == "Breadcrumb" {
            rows.push(row(tr("menu-copy-path"), CA::CopyPath));
        } else if name == "Ramp" {
            let collapsed = unsafe {
                (*target)
                    .as_any()
                    .downcast_ref::<crate::widget::input::Ramp>()
                    .map(|r| r.controls_collapsed)
                    .unwrap_or(false)
            };
            let label = tr("menu-collapse-controls");
            let label = if collapsed { format!("{}{label}", crate::widget::context_menu::MARK_CHECK) } else { label };
            rows.push((label, Some(CA::ToggleRampControls)));
            rows.extend([row(tr("menu-copy"), CA::Copy), row(tr("menu-paste"), CA::Paste)]);
        } else {
            rows.extend([row(tr("menu-copy"), CA::Copy), row(tr("menu-paste"), CA::Paste)]);
        }

        let scroll_y = crate::widget::hover_animation::get_scroll_offset();
        let adjusted_py = py - scroll_y;
        // SAFETY: the caller's contract, passed on.
        unsafe { self.show_context_menu_rows(px, adjusted_py, rows, header_count, target) };
    }

    pub fn hide_context_menu(&mut self) {
        crate::widget::context_menu::hide();
    }

    pub fn hit_test_context_menu(&self, px: f32, py: f32) -> bool {
        crate::widget::context_menu::hit_test(px, py)
    }

    pub fn cursor_moved_context_menu(&mut self, px: f32, py: f32) -> bool {
        crate::widget::context_menu::cursor_moved(px, py)
    }

    pub fn mouse_input_context_menu(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        crate::widget::context_menu::mouse_input(button, state, px, py, Some(self))
    }

    pub fn context_menu_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        crate::widget::context_menu::extra_quads()
    }

    pub fn context_menu_labels(&self) -> Vec<crate::widget::display::TextLabel> {
        crate::widget::context_menu::text_labels()
    }

    /// Whether the point lies inside an OPEN popover's plate. Popovers are drawn on top of
    /// everything and are interactive UI, but they are not spatial-grid widgets — a press
    /// there must never start a window move (the widgets beneath may not block dragging,
    /// e.g. Graph's edge-exclusive canvas hit test).
    fn point_in_active_popover(&self, px: f32, py: f32) -> bool {
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

    fn find_hovered_scrollable(&self, root: WidgetId, cx: f32, cy: f32) -> Option<WidgetId> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::{WidgetHost, Widget};

    /// The router's drag lifecycle drives the Input drag hooks end-to-end: a routed press
    /// records the drag target, the first >3px move synthesizes DragStart, further moves
    /// deliver DragUpdate (the slider value follows), and the release delivers DragEnd.
    /// Regression test for the silent-drop gap: `Input::on_event` defaults ignore Drag*
    /// events, so `Adapted::handle_event` must map them onto the hooks itself.
    #[test]
    fn routed_drag_reaches_input_drag_hooks() {
        use crate::widget::{ElementState, Event, MouseButton, Slider};

        let mut ctx = UiContext::new();
        let mut slider = Slider::new();
        WidgetHost::set_rect(&mut slider, 0.0, 0.0, 200.0, 30.0);
        let ptr = slider.as_ptr_mut();
        let id = slider.base().id();
        // SAFETY: a test widget, live for the whole test.
        unsafe { ctx.register_widget(id, ptr) };

        let press = Event::MouseButton {
            button: MouseButton::Left,
            state: ElementState::Pressed,
            x: 100.0,
            y: 15.0,
            local_x: 100.0,
            local_y: 15.0,
        };
        assert!(ctx.propagate_event(&press, id), "press in the track arms the drag");
        assert!(slider.is_dragging());
        let v0 = slider.value;

        // First move past the 3px threshold starts the drag; the next one updates it.
        let mv = |x: f32| Event::PointerMove { x, y: 15.0, local_x: x, local_y: 15.0 };
        ctx.propagate_event(&mv(110.0), id);
        assert!(ctx.is_dragging, "router crossed the drag threshold");
        ctx.propagate_event(&mv(140.0), id);
        assert!(
            slider.value > v0 + 0.05,
            "DragUpdate reached Input::drag_update (value {} -> {})",
            v0,
            slider.value
        );

        let release = Event::MouseButton {
            button: MouseButton::Left,
            state: ElementState::Released,
            x: 140.0,
            y: 15.0,
            local_x: 140.0,
            local_y: 15.0,
        };
        ctx.propagate_event(&release, id);
        assert!(!slider.is_dragging(), "DragEnd reached Input::drag_end");
        assert!(!ctx.is_dragging);
    }

    /// The multi-root press dispatch (how apps actually loop: one press propagated to
    /// EVERY top-level root, no break): a later root's propagate call must not wipe the
    /// drag target an earlier root just armed. This was live-broken in every plain-loop
    /// app (the demo, colors) while the single-root test above passed — found the first
    /// time a held drag could be driven headlessly (ccectl pointer-press).
    #[test]
    fn multi_root_press_dispatch_keeps_the_drag_target() {
        let mut ctx = UiContext::new();
        let mut slider = crate::widget::Slider::new().with_value(0.5);
        let id = slider.id();
        ctx.register_host(&mut slider);
        slider.set_rect(0.0, 0.0, 200.0, 30.0);
        let mut other = Block { base: Widget::new_rect(300.0, 300.0, 50.0, 50.0) };
        let other_ptr = &mut other as *mut _ as *mut (dyn crate::widget::WidgetHost + 'static);
        let other_id = other.base.id();
        // SAFETY: a test widget, live for the whole test.
        unsafe { ctx.register_widget(other_id, other_ptr) };

        let press = Event::MouseButton {
            button: MouseButton::Left,
            state: ElementState::Pressed,
            x: 100.0,
            y: 15.0,
            local_x: 100.0,
            local_y: 15.0,
        };
        // The app loop: same press to both roots, slider first.
        assert!(ctx.propagate_event(&press, id));
        ctx.propagate_event(&press, other_id);
        assert_eq!(ctx.drag_target, Some(id), "the second root's call must not wipe the armed target");

        let v0 = slider.value;
        let mv = |x: f32| Event::PointerMove { x, y: 15.0, local_x: x, local_y: 15.0 };
        for root in [id, other_id] {
            ctx.propagate_event(&mv(110.0), root);
        }
        for root in [id, other_id] {
            ctx.propagate_event(&mv(140.0), root);
        }
        assert!(ctx.is_dragging, "threshold crossed despite multi-root dispatch");
        assert!(slider.value > v0 + 0.05, "DragUpdate drove the slider ({} -> {})", v0, slider.value);

        let release = Event::MouseButton {
            button: MouseButton::Left,
            state: ElementState::Released,
            x: 140.0,
            y: 15.0,
            local_x: 140.0,
            local_y: 15.0,
        };
        for root in [id, other_id] {
            ctx.propagate_event(&release, root);
        }
        assert!(!slider.is_dragging());
        assert!(!ctx.is_dragging);
    }

    /// A plain drag-blocking widget (the `WidgetHost` default) at a fixed rect.
    struct Block {
        base: Widget,
    }
    impl WidgetHost for Block {
        crate::impl_widget_base!(Block);
    }

    /// `drag_allowed_at` — the window-drag question: allowed on empty surface, denied over a
    /// drag-blocking widget.
    #[test]
    fn drag_allowed_everywhere_except_blocking_widgets() {
        let mut ctx = UiContext::new();
        let mut w = Block { base: Widget::new_rect(10.0, 10.0, 50.0, 50.0) };
        let ptr = &mut w as *mut _ as *mut (dyn crate::widget::WidgetHost + 'static);
        // SAFETY: a test widget, live for the whole test.
        unsafe { ctx.register_widget(w.base.id(), ptr) };
        ctx.rebuild_spatial_grid();

        assert!(ctx.drag_allowed_at(200.0, 200.0), "empty surface is draggable");
        assert!(!ctx.drag_allowed_at(20.0, 20.0), "a drag-blocking widget denies the drag");
    }
}

#[cfg(test)]
mod focus_step_tests {
    use super::*;
    use crate::widget::{Button, TextBox, WidgetHost};

    /// Tab walks plates and wells in reading order (row, then x), wraps, and
    /// Shift+Tab walks back; a focused well opened for typing on the way.
    #[test]
    fn focus_step_walks_plates_and_wells_in_reading_order() {
        let mut ctx = UiContext::new();
        let mut a = Button::new(0.0, 0.0, 80.0, 24.0).with_label("A");
        let mut b = Button::new(0.0, 0.0, 80.0, 24.0).with_label("B");
        let mut t = TextBox::new("well".to_string());
        // Placed out of registration order: b is right of a on the first row (and a
        // few px lower — a shorter control centred on the row, still the same row), t below.
        WidgetHost::set_rect(&mut b, 100.0, 16.0, 80.0, 12.0);
        WidgetHost::set_rect(&mut a, 10.0, 10.0, 80.0, 24.0);
        WidgetHost::set_rect(&mut t, 10.0, 50.0, 200.0, 24.0);
        for w in [&mut b as &mut dyn WidgetHost, &mut a, &mut t] {
            let (id, ptr) = (w.base().id(), w as *mut dyn WidgetHost);
            let ptr = unsafe { std::mem::transmute::<*mut dyn WidgetHost, *mut (dyn WidgetHost + 'static)>(ptr) };
            // SAFETY: a test widget, live for the whole test.
            unsafe { ctx.register_widget(id, ptr) };
        }
        let (ia, ib, it) = (a.id(), b.id(), t.id());

        assert!(ctx.focus_step(false));
        assert!(ctx.is_focused_id(ia), "first stop: the top-left plate");
        assert!(ctx.focus_step(false));
        assert!(ctx.is_focused_id(ib), "then the plate to its right");
        assert!(ctx.focus_step(false));
        assert!(ctx.is_focused_id(it), "then the well on the next row");
        assert!(t.editing, "a well opens for typing when focused");
        assert!(ctx.focus_step(false));
        assert!(ctx.is_focused_id(ia), "wraps to the first stop");
        assert!(ctx.focus_step(true));
        assert!(ctx.is_focused_id(it), "Shift+Tab wraps back to the last");

        // A widget with no role is not a stop.
        let mut sep = crate::widget::Separator::new(0.0, 0.0, 10.0, 1.0, [1.0; 4]);
        WidgetHost::set_rect(&mut sep, 300.0, 10.0, 10.0, 1.0);
        assert_eq!(crate::widget::WidgetHostExt::focus_role(&sep), crate::widget::FocusRole::None);

        // A group's members walk together, where the group's first member falls:
        // grouping a and t (skipping b, which sits between them in reading order)
        // makes the walk a, t, b — and the group chord jumps a -> b -> a.
        let mut g = crate::widget::Group::new(vec![ia, it]);
        let (gid, gptr) = (g.base().id(), &mut g as *mut dyn WidgetHost);
        let gptr = unsafe { std::mem::transmute::<*mut dyn WidgetHost, *mut (dyn WidgetHost + 'static)>(gptr) };
        // SAFETY: a test widget, live for the whole test.
        unsafe { ctx.register_widget(gid, gptr) };
        assert_eq!(ctx.focus_clusters(), vec![vec![ia, it], vec![ib]]);
        ctx.set_focused_id(ia);
        assert!(ctx.focus_step(false));
        assert!(ctx.is_focused_id(it), "the group's second member before the ungrouped stop");
        assert!(ctx.focus_step(false));
        assert!(ctx.is_focused_id(ib));
        assert!(ctx.focus_step_group(false));
        assert!(ctx.is_focused_id(ia), "the group chord wraps to the group's first stop");
        assert!(ctx.focus_step_group(false));
        assert!(ctx.is_focused_id(ib), "then to the next run");
        ctx.unregister_widget(gid);

        // A plate parked off-screen (the hidden-editor idiom) is not a stop either.
        let mut parked = Button::new(0.0, 0.0, 1.0, 1.0).with_label("parked");
        WidgetHost::set_rect(&mut parked, -1000.0, -1000.0, 1.0, 1.0);
        let (pid, pptr) = (parked.base().id(), &mut parked as *mut dyn WidgetHost);
        let pptr = unsafe { std::mem::transmute::<*mut dyn WidgetHost, *mut (dyn WidgetHost + 'static)>(pptr) };
        // SAFETY: a test widget, live for the whole test.
        unsafe { ctx.register_widget(pid, pptr) };
        for _ in 0..4 {
            ctx.focus_step(false);
            assert!(!ctx.is_focused_id(pid), "the parked plate never takes focus");
        }
    }
}
