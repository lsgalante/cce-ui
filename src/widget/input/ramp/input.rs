//! The float ramp's input: dragging keys, the hover-scroll and its coast, the keyboard on the
//! graph and the controls, and what a screen reader sees and does.

use super::*;

impl Fields for Ramp {
    const TOTAL: usize = FIELD_TOTAL;

    /// The graph, the two dropdowns, then the key pad while a key is selected and the delete
    /// button while it is shown ([`deletable`](Fields::deletable)). With the controls
    /// collapsed, the graph alone.
    fn field_count(&self) -> usize {
        if self.controls_collapsed {
            1
        } else if self.deletable() {
            FIELD_TOTAL
        } else if self.selected_key_idx.is_some() {
            DELETE
        } else {
            PAD
        }
    }

    fn field(&mut self, i: usize) -> &mut dyn WidgetHost {
        match i {
            PRESET => &mut self.preset_dropdown,
            LINE => &mut self.line_type_dropdown,
            PAD => &mut self.key_pad,
            _ => &mut self.del_button,
        }
    }

    fn field_id(&self, i: usize) -> WidgetId {
        match i {
            PRESET => self.preset_dropdown.base().id(),
            LINE => self.line_type_dropdown.base().id(),
            PAD => self.key_pad.base().id(),
            _ => self.del_button.base().id(),
        }
    }

    fn walk(&self) -> &FieldWalk {
        &self.walk
    }

    fn walk_mut(&mut self) -> &mut FieldWalk {
        &mut self.walk
    }

    /// The graph's keys pick and edit the keys ([`own_key`](Fields::own_key)); a key pad's
    /// move moves the selected key; the delete button's press deletes.
    fn field_key(&mut self, i: usize, event: &KeyEvent, own: WidgetId, ui: &mut UiContext) -> bool {
        match i {
            GRAPH => self.own_key(&event.logical_key),
            PRESET => self.preset_dropdown.keyboard_input(event, ui),
            LINE => self.line_type_dropdown.keyboard_input(event, ui),
            PAD => {
                let before = (self.key_pad.inner().value_x(), self.key_pad.inner().value_y());
                let used = self.key_pad.keyboard_input(event, ui);
                if (self.key_pad.inner().value_x(), self.key_pad.inner().value_y()) != before {
                    self.apply_pad_to_selected();
                }
                used
            }
            _ => self.delete_button_key(event, own, ui),
        }
    }

    fn key_count(&self) -> usize {
        self.keys.len()
    }

    fn selected_key(&self) -> Option<usize> {
        self.selected_key_idx
    }

    fn key_pos(&self, i: usize) -> f32 {
        self.keys[i].pos
    }

    fn pick_key(&mut self, idx: usize) {
        self.select_key(idx);
        self.arrange_fields();
    }

    /// A key on the curve at `pos`.
    fn add_key_at(&mut self, pos: f32) -> usize {
        let value = self.get_interpolated_value(pos).clamp(0.0, 1.0);
        self.keys.push(RampKey { pos, value });
        let idx = self.resettle_key(self.keys.len() - 1);
        self.sync_preset();
        self.just_changed = true;
        idx
    }

    fn remove_selected_key(&mut self) {
        self.delete_selected();
    }

    fn delete_button(&mut self) -> &mut Adapted<Button> {
        &mut self.del_button
    }
}

impl Ramp {
    /// The control field item `idx` of [`a11y_items`](Input::a11y_items) stands for, past
    /// the keys: Preset, Line, the key pad's Position and Value, Delete.
    fn item_field(&self, idx: usize) -> Option<usize> {
        let field = match idx.checked_sub(self.keys.len())? {
            0 => PRESET,
            1 => LINE,
            2 | 3 => PAD,
            4 => DELETE,
            _ => return None,
        };
        (field < self.field_count()).then_some(field)
    }

    /// Stop a hover-scroll coast: let go of the latched key and its velocity. A press does,
    /// and so does a key or a reader's action the ramp takes — the coast moves the key it
    /// latched by index and selects it, so left running it moved a neighbour into a deleted
    /// key's place, or took back a selection the keyboard had moved.
    fn stop_coast(&mut self) {
        self.scroll_vel = (0.0, 0.0);
        self.scroll_key_idx = None;
        self.last_key_scroll = None;
    }
}

impl Input for Ramp {
    fn wants_tick(&self) -> bool {
        true
    }

    /// One keyboard stop, the graph first: the ramp walks its own fields (`keeps_tab`).
    fn focus_role(&self) -> crate::widget::FocusRole {
        crate::widget::FocusRole::Well
    }

    /// Tab stays in the ramp while it has a field to go that way: past its last field (or,
    /// Shift+Tab, before its first) the walk leaves it.
    fn keeps_tab(&self, reverse: bool) -> bool {
        self.keeps_field_tab(reverse)
    }

    /// The ramp's parts as a screen reader's nodes, in reading order: a radio button per key
    /// (on while selected, and the keyboard's node while the graph has it), named with its
    /// reading; the two dropdowns as combo boxes with their choice; while a key is selected
    /// the key pad's two axes as sliders, Position and Value, which a reader may set; and
    /// the delete button while it is shown. Each control's item stands for its field, so it
    /// is the keyboard's node while that field has it. None of the controls with them
    /// collapsed.
    fn a11y_items(&self, _rect: Rect) -> Vec<crate::a11y::A11yItem> {
        use crate::a11y::A11yItem;
        use accesskit::Role;
        let n = self.keys.len();
        let r = self.key_peg_r();
        let mut items: Vec<A11yItem> = (0..n)
            .map(|i| {
                let (cx, cy) = self.key_centre(i);
                let key = &self.keys[i];
                let (place, count) = ((i + 1).to_string(), n.to_string());
                let (pos, value) = (format!("{:.2}", key.pos), format!("{:.2}", key.value));
                let args = [("n", place.as_str()), ("count", &count), ("position", &pos), ("value", &value)];
                let label = crate::l10n::tr_args("ramp-key", &args);
                let selected = self.selected_key_idx == Some(i);
                A11yItem {
                    toggled: Some(selected),
                    focused: selected,
                    ..A11yItem::new(Role::RadioButton, label, Rect { x: cx - r, y: cy - r, width: 2.0 * r, height: 2.0 * r })
                }
            })
            .collect();
        if self.controls_collapsed {
            return items;
        }
        let rect = |w: &dyn WidgetHost| {
            let (x, y, width, height) = w.rect();
            Rect { x, y, width, height }
        };
        for i in [PRESET, LINE] {
            let dd = if i == PRESET { &self.preset_dropdown } else { &self.line_type_dropdown };
            items.push(A11yItem {
                value: dd.a11y_value(),
                field: Some(self.field_id(i)),
                ..A11yItem::new(Role::ComboBox, dd.label().unwrap_or_default(), rect(dd))
            });
        }
        if self.selected_key_idx.is_some() {
            let mut axes = self.key_pad.inner().axis_items(
                rect(&self.key_pad),
                [crate::l10n::tr("ramp-position"), crate::l10n::tr("ramp-value")],
            );
            // The pad's keyboard is read as its first axis.
            axes[0].field = Some(self.field_id(PAD));
            items.extend(axes);
        }
        if self.deletable() {
            items.push(A11yItem {
                field: Some(self.field_id(DELETE)),
                ..A11yItem::new(Role::Button, crate::l10n::tr("ramp-delete-key"), rect(&self.del_button))
            });
        }
        items
    }

    /// A reader's Click on item `idx`: on a key, select it and put the keyboard on the
    /// graph; on a control, give it the keyboard and do what Space does there (a dropdown
    /// opens, Delete deletes). The keyboard moves at the next tick, which has the context.
    fn a11y_select_item(&mut self, idx: usize) -> bool {
        if idx < self.keys.len() {
            self.stop_coast();
            let changed = self.selected_key_idx != Some(idx);
            if changed {
                self.select_key(idx);
                self.arrange_fields();
            }
            return self.reader_to_own() || changed;
        }
        let Some(field) = self.item_field(idx) else { return false };
        self.stop_coast();
        self.reader_to_field(field, field != PAD);
        true
    }

    /// A reader's number for the key pad's Position or Value item: the selected key moves.
    fn a11y_set_item_value(&mut self, idx: usize, value: f64) -> bool {
        let Some(axis) = idx.checked_sub(self.keys.len() + 2).filter(|&a| a < 2) else { return false };
        if self.item_field(idx) != Some(PAD) || !self.key_pad.inner_mut().set_axis(axis, value) {
            return false;
        }
        self.stop_coast();
        self.apply_pad_to_selected();
        true
    }

    /// The graph context menu's actions. Overriding loses the trait-default
    /// clipboard arms, so Copy/Paste (the spec string) are restated here.
    fn context_action(&mut self, action: ContextAction) -> bool {
        match action {
            ContextAction::ToggleRampControls => {
                self.controls_collapsed = !self.controls_collapsed;
                self.just_changed = true;
                self.arrange_fields();
                true
            }
            ContextAction::Copy => {
                crate::widget::clipboard::copy_to_clipboard(&self.spec_string());
                true
            }
            ContextAction::Paste => {
                if let Some(text) = crate::widget::clipboard::read_from_clipboard() {
                    let changed = self.set_spec(&text);
                    if changed {
                        self.just_changed = true;
                    }
                    changed
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// The curve as a ramp spec string ([`format_ramp_spec`]) — the value hosts
    /// poll and persist for ramp-valued params.
    fn value_string(&self) -> Option<String> {
        Some(self.spec_string())
    }

    fn set_value_string(&mut self, val: &str) -> bool {
        self.set_spec(val)
    }

    /// The open dropdown popover extends the hit area (the 5p Dropdown pattern).
    fn hit(&self, rect: Rect, x: f32, y: f32) -> bool {
        if let Some((px, py, pw, ph)) = self.open_popover() {
            if within(Rect { x: px, y: py, width: pw, height: ph }, x, y) {
                return true;
            }
        }
        within(rect, x, y)
    }

    fn tick_ctx(&mut self, dt: f32, ectx: &mut EventCtx) -> bool {
        let own = ectx.id;
        let Some(ui) = ectx.ui.as_deref_mut() else {
            return false;
        };
        let mut changed = self.just_changed;
        self.just_changed = false;

        if self.follow_focus(own, ui) {
            changed = true;
        }

        // Hover-scroll inertia: once the finger stream stops (>60ms without
        // an event), the latched key coasts on the estimated velocity with
        // exponential decay, still resettling and syncing like live scrolls.
        if let (Some(idx), Some(last)) = (self.scroll_key_idx, self.last_key_scroll) {
            if last.elapsed().as_secs_f32() > 0.06 && idx < self.keys.len() {
                let (vx, vy) = self.scroll_vel;
                // Animations off: the key stops where the scroll left it.
                if (vx.abs() > 0.02 || vy.abs() > 0.02) && crate::motion::enabled() {
                    let pos = (self.keys[idx].pos + vx * dt).clamp(0.0, 1.0);
                    let value = (self.keys[idx].value + vy * dt).clamp(0.0, 1.0);
                    self.scroll_key_idx = Some(self.move_key(idx, pos, value));
                    let f = (-5.0 * dt).exp();
                    self.scroll_vel = (vx * f, vy * f);
                    changed = true;
                } else {
                    self.scroll_vel = (0.0, 0.0);
                    self.last_key_scroll = None;
                }
            }
        }

        // The dropdown's tick reports its open/close animation running, not a pick: read
        // as a pick, opening the trigger re-applied the preset it still pointed at and
        // wiped a hand-edited curve. A pick is its change flag — the keyboard's land here,
        // the mouse's are drained in `on_event`.
        if self.preset_dropdown.tick(dt, ui) {
            changed = true;
        }
        if self.preset_dropdown.take_change() {
            let idx = self.preset_dropdown.selected;
            self.apply_preset(idx);
            changed = true;
        }
        
        if self.line_type_dropdown.tick(dt, ui) {
            changed = true;
        }
        // The key pad and the delete button have no tick of their own: neither animates,
        // and a pad change arrives through the event that made it.
        changed
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button, state, x, y, .. } => {
                let (button, state, px, py) = (*button, *state, *x, *y);
                // Right-press in the graph opening → the shared context menu (Collapse
                // controls, Copy, Paste). Before the ui borrow: open_context_menu needs the
                // whole EventCtx.
                if button == MouseButton::Right {
                    if state == ElementState::Pressed && within(self.graph_rect(), px, py) {
                        ectx.open_context_menu(px, py);
                        return true;
                    }
                    return false;
                }
                let own = ectx.id;
                let Some(ui) = ectx.ui.as_deref_mut() else { return false };
                if button != MouseButton::Left {
                    return false;
                }

                if self.preset_dropdown.mouse_input(button, state, px, py, ui) {
                    if self.preset_dropdown.take_change() {
                        let idx = self.preset_dropdown.selected;
                        self.apply_preset(idx);
                    }
                    self.adopt_pressed_field(own, ui);
                    return true;
                }
                if self.line_type_dropdown.mouse_input(button, state, px, py, ui) {
                    self.adopt_pressed_field(own, ui);
                    return true;
                }

                if state == ElementState::Pressed {
                    // Any press cancels a hover-scroll glide in progress.
                    self.stop_coast();
                    // A press that takes or makes a key gives the graph the keyboard when one
                    // of the ramp's controls has it, so the graph's keys act on that key — as
                    // they do when the press brings the keyboard from outside the ramp (the
                    // host focuses it: `FocusIn`).
                    let to_graph = self.focused_field(own, ui).is_some_and(|f| f != GRAPH);
                    // Grab the key whose peg is under the press, the topmost where pegs overlap —
                    // the one drawn on top is the one taken.
                    if let Some(idx) = self.key_at(px, py) {
                        if to_graph {
                            self.focus_field(GRAPH, own, ui);
                        }
                        self.select_key(idx);
                        self.is_dragging_key = true;
                        // The key keeps where on its peg it was taken: its centre snapping to
                        // the pointer jumped it up to a peg's radius on the first move.
                        let key = &self.keys[idx];
                        let (t, v) = self.plot_unit(px, py);
                        self.drag_grab = Some(DragGrab { origin: key.pos, offset: (key.pos - t, key.value - v) });
                        self.arrange_fields();
                        return true;
                    }

                    // Creation accepts the whole opening (the inset gutters included);
                    // the domain mapping clamps to the plot's 0..1.
                    if within(self.graph_rect(), px, py) {
                        if to_graph {
                            self.focus_field(GRAPH, own, ui);
                        }
                        let (t, v) = self.plot_unit(px, py);
                        let (t, val) = (t.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
                        self.keys.push(RampKey { pos: t, value: val });
                        let new_idx = self.resettle_key(self.keys.len() - 1);
                        self.sync_preset();
                        self.just_changed = true;
                        self.select_key(new_idx);
                        // Arm the drag: a fresh key follows the pointer until release,
                        // so create-and-place is one gesture (the grab-branch behavior).
                        self.is_dragging_key = true;
                        self.drag_grab = Some(DragGrab { origin: t, offset: (0.0, 0.0) });
                        self.arrange_fields();
                        return true;
                    }

                    if self.selected_key_idx.is_some() {
                        if self.key_pad.mouse_input(button, state, px, py, ui) {
                            self.apply_pad_to_selected();
                            return true;
                        }
                        // Pressed only: the button clicks on its release, below.
                        if self.del_button.mouse_input(button, state, px, py, ui) {
                            return true;
                        }
                    }
                    false
                } else {
                    // A release is the ramp's only when something of it used it: a key drag
                    // ending, the key pad or the delete button letting go. Releases reach
                    // widgets without a hit test, and claiming every one while a key was
                    // selected kept them from whatever the press was on — a host's next
                    // row never clicked. Both fields are told regardless of the selection,
                    // so neither is left holding a press.
                    let dragged = std::mem::take(&mut self.is_dragging_key);
                    self.drag_grab = None;
                    let pad = self.key_pad.mouse_input(button, state, px, py, ui);
                    let del = self.del_button.mouse_input(button, state, px, py, ui);
                    if del && self.del_button.take_click() {
                        self.delete_selected();
                    }
                    dragged || pad || del
                }
            }
            Event::PointerMove { x, y, .. } => {
                let (px, py) = (*x, *y);
                let Some(ui) = ectx.ui.as_deref_mut() else { return false };
                if self.preset_dropdown.cursor_moved(px, py, ui) {
                    return true;
                }
                if self.line_type_dropdown.cursor_moved(px, py, ui) {
                    return true;
                }

                let mut changed = false;
                if self.is_dragging_key {
                    if let Some(idx) = self.selected_key_idx {
                        let grab = self.drag_grab.unwrap_or(DragGrab { origin: self.keys[idx].pos, offset: (0.0, 0.0) });
                        let (t, v) = self.plot_unit(px, py);
                        let t_raw = (t + grab.offset.0).clamp(0.0, 1.0);
                        let t = self.resisted_pos(idx, t_raw, grab.origin);
                        let val = (v + grab.offset.1).clamp(0.0, 1.0);
                        self.move_key(idx, t, val);
                        changed = true;
                    }
                }

                if self.selected_key_idx.is_some() {
                    if self.key_pad.cursor_moved(px, py, ui) {
                        self.apply_pad_to_selected();
                        changed = true;
                    }
                    if self.del_button.cursor_moved(px, py, ui) {
                        changed = true;
                    }
                }
                if changed {
                    self.just_changed = true;
                }
                changed
            }
            Event::MouseWheel { delta, x, y, .. } => {
                // The dropdowns first (mirroring the press order, the preset's pick drained),
                // then the hover-scroll, then the key pad with the key sync.
                let (delta, px, py) = (*delta, *x, *y);
                let Some(ui) = ectx.ui.as_deref_mut() else { return false };
                if self.preset_dropdown.mouse_wheel(&delta, px, py, ui) {
                    if self.preset_dropdown.take_change() {
                        let idx = self.preset_dropdown.selected;
                        self.apply_preset(idx);
                    }
                    return true;
                }
                if self.line_type_dropdown.mouse_wheel(&delta, px, py, ui) {
                    return true;
                }
                // Hover-scroll: a gesture STARTING over a key latches it and
                // steers it on both axes — following the fingers like a drag
                // — until the stream pauses (fingers lifted). Mid-gesture the
                // latch holds even if the key slides out from under the
                // cursor. Latching also selects the key, so the pad tracks.
                let plot = self.plot_rect();
                if ui.scroll_gesture_new {
                    self.scroll_key_idx = self.key_at(px, py);
                    self.scroll_vel = (0.0, 0.0);
                }
                if let Some(idx) = self.scroll_key_idx {
                    if idx < self.keys.len() {
                        ui.scroll_initiate_widget_id = Some(ectx.id);
                        // Damped well below 1:1 — hover-scroll is for fine
                        // adjustment; the drag paths cover coarse moves.
                        let (dx, dy) = match &delta {
                            MouseScrollDelta::LineDelta(x, y) => (*x * 0.005, *y * 0.005),
                            MouseScrollDelta::PixelDelta(pos) => (
                                0.2 * pos.x as f32 / plot.width,
                                0.2 * pos.y as f32 / plot.height,
                            ),
                        };
                        // Velocity estimate for the release glide: EMA of
                        // applied delta over inter-event time. A leisurely
                        // wheel produces negligible velocity (big gaps clamp
                        // to 0.1s); fast trackpad streams build real speed.
                        let now = web_time::Instant::now();
                        let dt_ev = self
                            .last_key_scroll
                            .map(|t| now.duration_since(t).as_secs_f32())
                            .unwrap_or(0.016)
                            .clamp(0.004, 0.1);
                        self.last_key_scroll = Some(now);
                        let (ivx, ivy) = (-dx / dt_ev, dy / dt_ev);
                        self.scroll_vel = (
                            self.scroll_vel.0 * 0.65 + ivx * 0.35,
                            self.scroll_vel.1 * 0.65 + ivy * 0.35,
                        );
                        // Direct manipulation: the key moves WITH the scroll
                        // (runner deltas are content-motion negated, so both
                        // axes flip): scroll right → key right, down → down.
                        let pos = (self.keys[idx].pos - dx).clamp(0.0, 1.0);
                        let value = (self.keys[idx].value + dy).clamp(0.0, 1.0);
                        self.scroll_key_idx = Some(self.move_key(idx, pos, value));
                        self.just_changed = true;
                        self.arrange_fields();
                        return true;
                    }
                    self.scroll_key_idx = None;
                }
                if self.selected_key_idx.is_some() && self.key_pad.mouse_wheel(&delta, px, py, ui) {
                    self.apply_pad_to_selected();
                    return true;
                }
                false
            }
            Event::KeyInput(event) => {
                let own = ectx.id;
                let Some(ui) = ectx.ui.as_deref_mut() else { return false };
                let used = self.walk_key(event, own, ui);
                if used {
                    self.stop_coast();
                }
                used
            }
            Event::FocusIn => {
                // Focused itself — by the Tab walk (either way), a click, a reader — the
                // ramp takes the keyboard on its graph.
                self.base.focused = true;
                self.walk_focus_in();
                true
            }
            Event::FocusOut => {
                self.base.focused = false;
                self.walk_focus_out();
                true
            }
            _ => false,
        }
    }

    // The key drag and the key pad's own drag both run through the ramp: the router records
    // the ramp as the drag target once a press is handled here, and the pad's moves sync the
    // selected key as they go.
    fn draggable(&self, _rect: Rect) -> bool {
        self.is_dragging()
    }
    fn is_dragging(&self) -> bool {
        self.is_dragging_key || self.key_pad.is_dragging()
    }
    fn drag_update(&mut self, px: f32, py: f32, _rect: Rect) -> bool {
        if self.key_pad.is_dragging() && self.key_pad.drag_update(px, py) {
            self.apply_pad_to_selected();
            return true;
        }
        false
    }
    fn drag_end(&mut self) {
        self.key_pad.drag_end();
        self.is_dragging_key = false;
        self.drag_grab = None;
    }
}
