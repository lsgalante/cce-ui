//! Input: hover, the row being typed into, committing a row, the pointer moving, and `impl Input`.

use super::*;

impl ParametersBg {
    /// The row open for typing, if any: a code row, or a hosted text box,
    /// spinbox, slider readout, colour or vector field that is editing. (A
    /// choice row holds `focused_param` while its list is open; that is not
    /// typing.)
    pub(super) fn typing_row(&self) -> Option<usize> {
        let i = self.focused_param?;
        if self.code_editor.is_some() {
            return Some(i);
        }
        fn at<T>(v: &[Option<T>], i: usize) -> Option<&T> {
            v.get(i).and_then(Option::as_ref)
        }
        let typing = at(&self.texts, i).is_some_and(|t| t.editing)
            || at(&self.spinboxes, i).is_some_and(|t| t.editing)
            || at(&self.sliders, i).is_some_and(|t| t.editing)
            || at(&self.colors, i).is_some_and(|t| t.editing)
            || at(&self.float3s, i).is_some_and(|t| t.editing_idx().is_some());
        typing.then_some(i)
    }

    /// Say a field is open for typing (`crate::text_input`): at its row, or
    /// the pane when the row is not laid out. The hosted fields are drawn
    /// from this pane's aggregates, never through their own paint, so their
    /// own claims never run — the pane makes it for them.
    pub(super) fn claim_typing(&self, ctx: &PaintCtx) {
        if let Some(i) = self.typing_row() {
            let (x, y, w, h) = self
                .get_param_rects()
                .get(i)
                .copied()
                .filter(|r| r.3 > 0.0)
                .unwrap_or((self.rect.x, self.rect.y, self.rect.width, self.rect.height));
            let (ox, oy) = ctx.offset();
            crate::text_input::claim(x + ox, y + oy, w, h);
        }
    }

    /// Hand a pointer position to every hosted control, so each re-reads
    /// its own hover (and a ramp row its key drag). Called for every pointer
    /// move, and again from the last known position (`mouse_pos`) whenever a
    /// SCROLL moves the rows under a still pointer — the wheel arm and the
    /// tick's coast — because a control's hover is recomputed only when it
    /// is told where the pointer is, and until 2026-09-28 nothing told it on
    /// a scroll: a control that scrolled under the pointer stayed dark and
    /// one that scrolled away stayed lit until the next motion. Sliders are
    /// in the roster since the same day, having had no hover before.
    pub(super) fn hover_controls(&mut self, px: f32, py: f32, ui: &mut UiContext) -> bool {
        let mut changed = false;
        for s in self.sliders.iter_mut().flatten() {
            if s.on_cursor_moved(px, py, ui) {
                changed = true;
            }
        }
        for sb in self.spinboxes.iter_mut().flatten() {
            if sb.on_cursor_moved(px, py, ui) {
                changed = true;
            }
        }
        for f in self.float3s.iter_mut().flatten() {
            if f.on_cursor_moved(px, py, ui) {
                changed = true;
            }
        }
        for b in self.buttons.iter_mut().flatten() {
            if b.on_cursor_moved(px, py, ui) {
                changed = true;
            }
        }
        for d in self.choices.iter_mut().flatten() {
            if d.on_cursor_moved(px, py, ui) {
                changed = true;
            }
        }
        for tb in self.texts.iter_mut().flatten() {
            if tb.on_cursor_moved(px, py, ui) {
                changed = true;
            }
        }
        for cb in self.toggles.iter_mut().flatten() {
            if cb.on_cursor_moved(px, py, ui) {
                changed = true;
            }
        }
        for c in self.colors.iter_mut().flatten() {
            if c.on_cursor_moved(px, py, ui) {
                changed = true;
            }
        }
        // Ramp rows: a move can drag a key — re-serialize the curve
        // into the row value so hosts polling `node_params` see it.
        for i in 0..self.ramps.len() {
            if let Some(rp) = &mut self.ramps[i] {
                if rp.on_cursor_moved(px, py, ui) {
                    self.display_params[i].1 = rp.inner().spec_string();
                    changed = true;
                }
            }
        }
        let row = self.hoverable_row_at(px, py);
        if row != self.hover_row {
            self.hover_row = row;
            changed = true;
        }
        changed
    }

    /// The parameter row under `(px, py)`: a visible, non-header row whose
    /// rect holds the point, and only while the point is inside the pane's
    /// own viewport (a row scrolled out of it is not under anything).
    pub(super) fn hoverable_row_at(&self, px: f32, py: f32) -> Option<usize> {
        if !self.visible
            || px < self.rect.x
            || px > self.rect.x + self.rect.width
            || py < self.rect.y
            || py > self.rect.y + self.rect.height
        {
            return None;
        }
        let hidden = self.hidden_rows();
        self.get_param_rects().into_iter().enumerate().find_map(|(i, (x, y, w, h))| {
            let hoverable = !hidden[i] && h > 0.0 && self.display_params[i].2 != "section" && self.display_params[i].2 != SEPARATOR;
            (hoverable && px >= x && px <= x + w && py >= y && py <= y + h).then_some(i)
        })
    }

    /// The rows moved under a still pointer: re-hover from where it was.
    pub(super) fn rehover_after_scroll(&mut self, ui: &mut UiContext) -> bool {
        match self.mouse_pos {
            Some((px, py)) => self.hover_controls(px, py, ui),
            None => false,
        }
    }

    /// The state half of the legacy `unfocus`: commit the focused row's in-flight value back
    /// into `display_params`, drop the code editor, and unfocus the raw children.
    pub(super) fn commit_and_unfocus(&mut self) {
        if let Some(idx) = self.focused_param {
            if idx < self.display_params.len() {
                let p = &mut self.display_params[idx];
                if p.2.starts_with("spinbox") {
                    if let Some(sb) = &mut self.spinboxes[idx] {
                        sb.unfocus();
                        p.1 = sb.value.to_string();
                    }
                } else if p.2.starts_with("slider") {
                    if let Some(s) = &mut self.sliders[idx] {
                        s.unfocus();
                        let new_val = s.get_scaled_value();
                        p.1 = format!("{:.*}", slider_decimals(&p.2), new_val);
                    }
                } else if is_vec_row(&p.2) {
                    if let Some(f) = &mut self.float3s[idx] {
                        f.unfocus();
                        p.1 = f.value_string();
                    }
                } else if is_text_row(&p.2) {
                    if let Some(tb) = &mut self.texts[idx] {
                        tb.unfocus();
                        p.1 = tb.text.clone();
                    }
                    if let Some(d) = &mut self.choices[idx] {
                        d.unfocus();
                    }
                } else if p.2.starts_with("choice") {
                    if let Some(d) = &mut self.choices[idx] {
                        d.unfocus();
                        if let Some(val) = d.get_value_string() {
                            p.1 = val;
                        }
                    }
                } else if p.2.starts_with("color") || p.2 == "rgb" || p.2 == "rgba" {
                    if let Some(c) = &mut self.colors[idx] {
                        c.unfocus();
                        if let Some(val) = c.get_value_string() {
                            p.1 = val;
                        }
                    }
                } else if p.2 == "code" {
                    if let Some(ref editor) = self.code_editor {
                        p.1 = editor.buffer.clone();
                    }
                    self.code_editor = None;
                    self.code_history.clear();
                }
            }
        }
        self.focused_param = None;
    }

    /// The pointer moved: the scrollbar's hover and thumb drag, and the rows' hover.
    fn on_pointer_move(&mut self, px: &f32, py: &f32, ectx: &mut EventCtx) -> bool {
        let (px, py) = (*px, *py);
        self.mouse_pos = Some((px, py));
        // Track scrollbar hover, then re-latch: hover only sustains an already-raised
        // bar (a sunk one is behind the plate, so the pointer never reaches it), so
        // the only visible change here is the raised state — redraw on that transition.
        let hover = self.hit_test_scrollbar(px, py);
        self.activity.set_hover(hover);
        let raised_changed = self.recompute_scrollbar_raised();
        let Some(ui) = ectx.ui.as_deref_mut() else {
            return raised_changed;
        };
        let mut changed = raised_changed;

        if self.scrollbar_dragging && self.drag_thumb_to(py) {
            changed = true;
        }

        if self.hover_controls(px, py, ui) {
            changed = true;
        }

        changed
    }
}

impl Input for ParametersBg {
    /// While a code row is being edited, the clipboard, selection and
    /// history actions are the editor's — the runner routes the undo / redo
    /// chords here before the app's own history gets them.
    fn context_action(&mut self, action: crate::widget::ContextAction) -> bool {
        if self.code_editing() {
            return self.code_action(action);
        }
        false
    }

    fn scrollable(&self) -> bool {
        true
    }

    /// Legacy `mouse_input` saw every press (and consumes every left press — the designer
    /// relies on the panel swallowing clicks anywhere while it's the dispatch target).
    fn gates_presses(&self) -> bool {
        false
    }

    /// Legacy hit reach: the panel rect, or an open popover (a dropdown row's list extends
    /// below the panel).
    fn hit(&self, rect: Rect, px: f32, py: f32) -> bool {
        if px >= rect.x && px <= rect.x + rect.width && py >= rect.y && py <= rect.y + rect.height {
            return true;
        }
        if let Some((pop_x, pop_y, pop_w, pop_h)) = self.own_popover_rect() {
            if px >= pop_x && px <= pop_x + pop_w && py >= pop_y && py <= pop_y + pop_h {
                return true;
            }
        }
        false
    }


    // --- The host-driven drag surface (the designer routes pointer drags here directly). ---

    fn draggable(&self, _rect: Rect) -> bool {
        self.scrollbar_dragging
            || self.dragging_param.is_some()
            || self.display_params.iter().any(|p| p.2.starts_with("slider") || is_vec_row(&p.2))
    }

    fn is_dragging(&self) -> bool {
        self.scrollbar_dragging || self.dragging_param.is_some()
    }

    fn drag_begin(&mut self, px: f32, py: f32, _rect: Rect) {
        if self.scrollbar_dragging {
            return;
        }
        let rects = self.get_param_rects();
        for (i, p) in self.display_params.iter().enumerate() {
            if p.2.starts_with("slider") {
                let r = rects[i];
                if let Some(s) = &mut self.sliders[i] {
                    let top = s.label_strip();
                    if py >= r.1 + top && py <= r.1 + r.3 {
                        s.drag_begin(px, py);
                        self.dragging_param = Some(i);
                        break;
                    }
                }
            } else if is_vec_row(&p.2) {
                let r = rects[i];
                if py >= r.1 && py <= r.1 + r.3 {
                    if let Some(f) = &mut self.float3s[i] {
                        let mut dummy = crate::context::UiContext::new();
                        if f.mouse_input(MouseButton::Left, ElementState::Pressed, px, py, &mut dummy) {
                            self.dragging_param = Some(i);
                            break;
                        }
                    }
                }
            }
        }
    }

    fn drag_update(&mut self, px: f32, py: f32, _rect: Rect) -> bool {
        if self.scrollbar_dragging {
            return self.drag_thumb_to(py);
        }

        if let Some(i) = self.dragging_param {
            if let Some(s) = &mut self.sliders[i] {
                if s.drag_update(px, py) {
                    let new_val = s.get_scaled_value();
                    let old_val = &self.display_params[i].1;
                    let new_val_str = format!("{:.*}", slider_decimals(&self.display_params[i].2), new_val);
                    if *old_val != new_val_str {
                        self.display_params[i].1 = new_val_str;
                        return true;
                    }
                }
            } else if let Some(f) = &mut self.float3s[i] {
                if f.drag_update(px, py) {
                    let new_val_str = f.value_string();
                    if self.display_params[i].1 != new_val_str {
                        self.display_params[i].1 = new_val_str;
                        return true;
                    }
                }
            }
        }
        false
    }

    fn drag_end(&mut self) {
        if self.scrollbar_dragging {
            self.scrollbar_dragging = false;
            self.activity.bump();
            return;
        }
        if let Some(i) = self.dragging_param.take() {
            if let Some(s) = &mut self.sliders[i] {
                s.drag_end();
            } else if let Some(f) = &mut self.float3s[i] {
                f.drag_end();
            }
        }
    }

    /// Legacy `tick` forwarded to the raw children (the adapter's recursion now) and the
    /// checkbox rows. The checkbox tick never touches the ctx (a leaf `Adapted` tick is
    /// ctx-free), so the in-file dummy-ctx convention (`drag_begin`, `collect_child_quads`)
    /// applies.
    fn tick(&mut self, dt: f32, _rect: Rect) -> bool {
        if !self.visible {
            return false;
        }
        let mut changed = false;
        let mut dummy = crate::context::UiContext::new();
        // Choice rows tick their Dropdowns' open/close animation. This was the
        // ONLY path that can advance a pane dropdown's anim_snap (the widget's
        // tick-receiver registration points at an id pane internals never put
        // in the host tree), and without it every params-pane dropdown opened
        // at zero drawn extent: logically open, invisible, reporting a sliver
        // popover rect — and the next click toggled it closed again.
        for d in self.choices.iter_mut().flatten() {
            if d.tick(dt, &mut dummy) {
                changed = true;
            }
        }
        for cb in self.toggles.iter_mut().flatten() {
            if cb.tick(dt, &mut dummy) {
                changed = true;
            }
        }
        // Slider rows tick their wheel-glide inertia — fold a coasting value
        // back into the row string so hosts syncing off display_params apply
        // it, exactly like a live wheel event would.
        for i in 0..self.sliders.len() {
            if let Some(s) = &mut self.sliders[i] {
                if s.tick(dt, &mut dummy) {
                    let new_val = s.get_scaled_value();
                    let new_val_str = format!("{:.*}", slider_decimals(&self.display_params[i].2), new_val);
                    if self.display_params[i].1 != new_val_str {
                        self.display_params[i].1 = new_val_str;
                    }
                    changed = true;
                }
            }
        }
        // Float3 rows are three slider rows: same glide, same fold-back.
        for i in 0..self.float3s.len() {
            if let Some(f) = &mut self.float3s[i] {
                if f.tick(dt, &mut dummy) {
                    let new_val_str = f.value_string();
                    if self.display_params[i].1 != new_val_str {
                        self.display_params[i].1 = new_val_str;
                    }
                    changed = true;
                }
            }
        }
        // Ramp rows tick their field widgets (preset application, slider→key
        // sync) and drain their change flag — fold the curve back into the row
        // value when it moved.
        for i in 0..self.ramps.len() {
            if let Some(rp) = &mut self.ramps[i] {
                if rp.tick(dt, &mut dummy) {
                    self.display_params[i].1 = rp.inner().spec_string();
                    changed = true;
                }
            }
        }
        // Color rows tick their picker-stream poll (`cce-color-editor --stream`
        // lines applying live) — fold a changed value back into the row so
        // hosts syncing off display_params see it while the picker is open.
        for i in 0..self.colors.len() {
            if let Some(c) = &mut self.colors[i] {
                if c.tick(dt, &mut dummy) {
                    if let Some(val) = c.get_value_string() {
                        if self.display_params[i].1 != val {
                            self.display_params[i].1 = val;
                            self.tick_value_changed = true;
                        }
                    }
                    changed = true;
                }
            }
        }
        // The pane's own wheel glide / trackpad coast: adopt any host write to
        // `scroll_y`, advance, and re-seat the rows when the offset moved.
        self.scroll_motion.reconcile(0.0, self.scroll_y);
        let pane_max = (self.content_h - self.rect.height).max(0.0);
        if self.scroll_motion.tick(dt, crate::widget::Bounds::max(0.0), crate::widget::Bounds::max(pane_max)) {
            self.scroll_y = self.scroll_motion.y.pos();
            self.update_slider_rects();
            self.rehover_after_scroll(&mut dummy);
            changed = true;
        }
        if self.scroll_motion.is_animating() {
            changed = true;
        }
        // Decay the "recently scrolled" window; keep frames coming until it expires so the
        // scrollbar's sink behind the plate actually renders.
        if self.activity.holding() {
            changed = true;
        }
        // Re-latch the raised state (e.g. sink once the scroll window lapses).
        let visible = self.scrollbar_visible();
        if self.activity.tick(dt, visible, self.scrollbar_dragging) {
            changed = true;
        }
        changed
    }

    fn visibility_changed(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            // Hosts call `unfocus()` directly (the designer's pane switches): commit the
            // focused row and unfocus the children. Needs no ctx, so the direct path's
            // ui-less synthesis works too.
            Event::FocusOut => {
                self.commit_and_unfocus();
                true
            }
            Event::PointerMove { x: px, y: py, .. } => self.on_pointer_move(px, py, ectx),
            Event::MouseButton { button, state, x: px, y: py, .. } => self.on_mouse_button(button, state, px, py, ectx),
            Event::KeyInput(event) => self.on_key(event, ectx),
            Event::MouseWheel { delta, x: px, y: py, .. } => self.on_wheel(delta, px, py, ectx),
            _ => false,
        }
    }
}
