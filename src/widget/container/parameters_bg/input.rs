//! Input: hover, the row being typed into, committing a row, and `impl Input`.

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

        if self.scrollbar_dragging {
            let sb_track_h = self.rect.height - 8.0;
            let sb_track_y = self.rect.y + 4.0;
            let visible_ratio = self.rect.height / self.content_h;
            let thumb_h = if sb_track_h <= 20.0 {
                sb_track_h
            } else {
                (sb_track_h * visible_ratio).clamp(20.0, sb_track_h)
            };
            let max_scroll = (self.content_h - self.rect.height).max(0.0);

            let target_thumb_y = py - self.drag_offset_y;
            let new_scroll_ratio = if sb_track_h - thumb_h > 0.0 {
                ((target_thumb_y - sb_track_y) / (sb_track_h - thumb_h)).clamp(0.0, 1.0)
            } else {
                0.0
            };

            let old_scroll = self.scroll_y;
            self.scroll_y = new_scroll_ratio * max_scroll;
            if (self.scroll_y - old_scroll).abs() > 0.01 {
                self.update_slider_rects();
                changed = true;
            }
        }

        if self.hover_controls(px, py, ui) {
            changed = true;
        }

        changed
    }

    /// A press or release: the scrollbar, a section's title box, open popovers, then the rows' controls.
    fn on_mouse_button(&mut self, button: &MouseButton, state: &ElementState, px: &f32, py: &f32, ectx: &mut EventCtx) -> bool {
        if !self.visible {
            return false;
        }
        let (button, state, px, py) = (*button, *state, *px, *py);
        let Some(ui) = ectx.ui.as_deref_mut() else {
            return false;
        };

        if button == MouseButton::Left {
            if state == ElementState::Pressed {
                // Only a raised bar can be grabbed — a sunk one is behind the plate,
                // so the press falls through to the pane content underneath it.
                if self.activity.raised() && self.hit_test_scrollbar(px, py) {
                    // Legacy called `self.focus()` here — base flag only, which
                    // nothing reads (see module docs).
                    self.scrollbar_dragging = true;

                    let sb_track_h = self.rect.height - 8.0;
                    let sb_track_y = self.rect.y + 4.0;
                    let visible_ratio = self.rect.height / self.content_h;
                    let thumb_h = if sb_track_h <= 20.0 {
                        sb_track_h
                    } else {
                        (sb_track_h * visible_ratio).clamp(20.0, sb_track_h)
                    };
                    let max_scroll = (self.content_h - self.rect.height).max(0.0);
                    let scroll_ratio = if max_scroll > 0.0 { self.scroll_y / max_scroll } else { 0.0 };
                    let thumb_y = sb_track_y + scroll_ratio * (sb_track_h - thumb_h);

                    let click_offset = py - thumb_y;
                    if click_offset >= 0.0 && click_offset <= thumb_h {
                        self.drag_offset_y = click_offset;
                    } else {
                        // Clicked outside the thumb: jump thumb center to py
                        self.drag_offset_y = thumb_h / 2.0;
                        let target_thumb_y = py - self.drag_offset_y;
                        let new_scroll_ratio = if sb_track_h - thumb_h > 0.0 {
                            ((target_thumb_y - sb_track_y) / (sb_track_h - thumb_h)).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        self.scroll_y = new_scroll_ratio * max_scroll;
                        self.update_slider_rects();
                    }
                    return true;
                }
            } else if state == ElementState::Released
                && self.scrollbar_dragging {
                    self.scrollbar_dragging = false;
                    self.activity.bump();
                    return true;
                }
        }

        // A press on a section's title box collapses/expands it. Checked before the
        // rows so a header can never be shadowed by a control under it, and only on
        // the press — the matching release lands on whatever the relayout moved
        // under the pointer, which must not toggle it straight back.
        if button == MouseButton::Left && state == ElementState::Pressed {
            let rects = self.get_param_rects();
            let hit = self.display_params.iter().enumerate().position(|(i, p)| {
                if p.2 != "section" {
                    return false;
                }
                let (bx, by, bw, bh) = self.section_title_box(i, rects[i]);
                px >= bx && px <= bx + bw && py >= by && py <= by + bh
            });
            if let Some(i) = hit {
                let title = self.display_params[i].0.clone();
                let collapsed = self.collapsed.contains(&title);
                // Collapsing out from under a focused row would strand the editor.
                self.commit_and_unfocus();
                self.set_section_collapsed(&title, !collapsed);
                return true;
            }
        }

        let hidden = self.hidden_rows();

        // 1. Check open dropdown popovers first (since they are drawn on top)
        for (i, d_opt) in self.choices.iter_mut().enumerate() {
            if hidden[i] {
                continue;
            }
            if let Some(d) = d_opt {
                if std::env::var("CCE_PARAM_DEBUG").is_ok() {
                    eprintln!("[pdbg] press ({px:.0},{py:.0}) choice[{i}] open-priority: popover_rect={:?}", d.popover_rect());
                }
                if let Some((ox, oy, ow, oh)) = d.popover_rect() {
                    // Textpick pickers are press-driven end to end
                    // (selection fires on the option PRESS): a release
                    // over the open surface is swallowed, never
                    // dispatched — mid-animation it can read as an
                    // outside press and close the menu it just opened.
                    if state != ElementState::Pressed
                        && self.display_params[i].2.starts_with("textpick")
                    {
                        if px >= ox && px <= ox + ow && py >= oy && py <= oy + oh {
                            return true;
                        }
                        continue;
                    }
                    let consumed = d.mouse_input(button, state, px, py, ui);
                    if std::env::var("CCE_PARAM_DEBUG").is_ok() {
                        eprintln!("[pdbg]   -> open dropdown consumed={consumed}");
                    }
                    if consumed {
                        if d.take_change() {
                            if let Some(val) = d.get_value_string() {
                                // A textpick row's pick fills its
                                // TextBox — the box IS the value.
                                if self.display_params[i].2.starts_with("textpick") {
                                    if let Some(tb) = &mut self.texts[i] {
                                        tb.text = val.clone();
                                        tb.edit_buffer = val.clone();
                                    }
                                }
                                self.display_params[i].1 = val;
                            }
                        }
                        return true;
                    }
                }
            }
        }
        // The ramp rows' field dropdowns can pop over neighboring rows too.
        for (i, rp_opt) in self.ramps.iter_mut().enumerate() {
            if hidden[i] {
                continue;
            }
            if let Some(rp) = rp_opt {
                let ramp = rp.inner();
                if (ramp.preset_dropdown.popover_rect().is_some()
                    || ramp.line_type_dropdown.popover_rect().is_some())
                    && rp.mouse_input(button, state, px, py, ui) {
                        self.display_params[i].1 = rp.inner().spec_string();
                        return true;
                    }
            }
        }

        // 2. Propagate to our widgets
        for (i, p) in self.display_params.iter_mut().enumerate() {
            if hidden[i] {
                continue;
            }
            if p.2.starts_with("choice") {
                if let Some(d) = &mut self.choices[i] {
                    if d.mouse_input(button, state, px, py, ui) {
                        // Claim the param focus while the dropdown is
                        // open — the KeyInput arm above is gated on
                        // `focused_param`, and without this the choice
                        // row was the ONE row type that never set it,
                        // so Escape/arrows/Enter could not reach an
                        // open params dropdown (found via cce-designer).
                        if d.open {
                            self.focused_param = Some(i);
                        } else if self.focused_param == Some(i) {
                            self.focused_param = None;
                        }
                        if d.take_change() {
                            if let Some(val) = d.get_value_string() {
                                p.1 = val;
                            }
                        }
                        return true;
                    }
                }
            } else if p.2 == "button" {
                if let Some(b) = &mut self.buttons[i] {
                    if b.mouse_input(button, state, px, py, ui) {
                        if std::env::var("CCE_PARAM_DEBUG").is_ok() {
                            eprintln!("[pdbg] press ({px:.0},{py:.0}) BUTTON[{i}] '{}' consumed", p.0);
                        }
                        if b.take_click() {
                            p.1 = "clicked".to_string();
                        }
                        return true;
                    }
                }
            } else if is_text_row(&p.2) {
                if let Some(d) = &mut self.choices[i] {
                    // The picker acts on PRESSES only; the release
                    // over the button is swallowed. Releases used to
                    // reach the dropdown, and one arriving before the
                    // open animation's first frame (a fast or
                    // injected click) read as an outside press and
                    // closed the menu it had just opened.
                    let (bx, by, bw, bh) = d.rect();
                    let on_button =
                        px >= bx && px <= bx + bw && py >= by && py <= by + bh;
                    if state == ElementState::Pressed {
                        if d.mouse_input(button, state, px, py, ui) {
                            if d.take_change() {
                                if let Some(val) = d.get_value_string() {
                                    if let Some(tb) = &mut self.texts[i] {
                                        tb.text = val.clone();
                                        tb.edit_buffer = val.clone();
                                    }
                                    p.1 = val;
                                }
                            }
                            return true;
                        }
                    } else if on_button {
                        return true;
                    }
                }
                if let Some(tb) = &mut self.texts[i] {
                    if tb.mouse_input(button, state, px, py, ui) {
                        if tb.editing {
                            self.focused_param = Some(i);
                        } else {
                            if self.focused_param == Some(i) {
                                self.focused_param = None;
                            }
                        }
                        if tb.take_change() {
                            if let Some(val) = tb.get_value_string() {
                                p.1 = val;
                            }
                        }
                        return true;
                    }
                }
            } else if p.2.starts_with("spinbox") {
                if let Some(sb) = &mut self.spinboxes[i] {
                    if sb.mouse_input(button, state, px, py, ui) {
                        p.1 = sb.value.to_string();
                        if sb.editing {
                            self.focused_param = Some(i);
                        } else {
                            if self.focused_param == Some(i) {
                                self.focused_param = None;
                            }
                        }
                        return true;
                    }
                }
            } else if p.2 == "toggle" || p.2 == "checkbox" {
                if let Some(cb) = &mut self.toggles[i] {
                    if cb.mouse_input(button, state, px, py, ui) {
                        if cb.take_change() {
                            if let Some(val) = cb.get_value_string() {
                                p.1 = val;
                            }
                        }
                        return true;
                    }
                }
            } else if p.2.starts_with("color") || p.2 == "rgb" || p.2 == "rgba" {
                if let Some(c) = &mut self.colors[i] {
                    if c.mouse_input(button, state, px, py, ui) {
                        if let Some(val) = c.get_value_string() {
                            p.1 = val;
                        }
                        if c.editing {
                            self.focused_param = Some(i);
                        } else {
                            if self.focused_param == Some(i) {
                                self.focused_param = None;
                            }
                        }
                        return true;
                    }
                }
            } else if p.2 == "ramp" {
                if let Some(rp) = &mut self.ramps[i] {
                    if rp.mouse_input(button, state, px, py, ui) {
                        p.1 = rp.inner().spec_string();
                        return true;
                    }
                }
            }
        }

        if button == MouseButton::Left && state == ElementState::Pressed {
            let rects = self.get_param_rects();
            let mut clicked_any_focusable = false;
            for (i, p) in self.display_params.iter_mut().enumerate() {
                if hidden[i] {
                    continue;
                }
                if p.2 == "code" {
                    let r = rects[i];
                    if px >= r.0 && px <= r.0 + r.2 && py >= r.1 + Self::CODE_BOX_TOP && py <= r.1 + r.3 {
                        // Clicking inside the box already being edited moves the
                        // caret and keeps the buffer (and its history); a click
                        // into a different row's box starts a fresh editor.
                        let mut editor = match (self.focused_param == Some(i), self.code_editor.take()) {
                            (true, Some(e)) => e,
                            (_, other) => {
                                drop(other);
                                self.code_history.clear();
                                TextEditorState::new(p.1.clone())
                            }
                        };
                        self.focused_param = Some(i);
                        let click_x = px - self.code_text_x(r);
                        let click_y = py - (r.1 + Self::CODE_TOP);
                        let line = (click_y / Self::CODE_LINE_H).floor().max(0.0) as usize;
                        let col = (click_x / self.code_col_w() + 0.5).floor().max(0.0) as usize;
                        let at = map_2d_to_1d(&editor.buffer, line, col);
                        if ui.shift_pressed {
                            if editor.select_anchor.is_none() {
                                editor.select_anchor = Some(editor.cursor_idx);
                            }
                        } else {
                            editor.clear_selection();
                        }
                        editor.cursor_idx = at;
                        self.code_editor = Some(editor);
                        clicked_any_focusable = true;
                        break;
                    }
                } else if p.2.starts_with("slider") {
                    let r = rects[i];
                    if py >= r.1 && py <= r.1 + r.3 {
                        if let Some(s) = &mut self.sliders[i] {
                            if s.mouse_input(button, state, px, py, ui) {
                                if s.editing {
                                    self.focused_param = Some(i);
                                    clicked_any_focusable = true;
                                }
                                break;
                            }
                        }
                    }
                } else if is_vec_row(&p.2) {
                    let r = rects[i];
                    if py >= r.1 && py <= r.1 + r.3 {
                        if let Some(f) = &mut self.float3s[i] {
                            if f.mouse_input(button, state, px, py, ui) {
                                if f.editing_idx().is_some() {
                                    self.focused_param = Some(i);
                                    clicked_any_focusable = true;
                                }
                                break;
                            }
                        }
                    }
                }
            }
            if !clicked_any_focusable {
                self.commit_and_unfocus();
            }
            return true;
        }
        false
    }

    /// A key, to the focused row: the code editor's chords on a code row, else the row's control.
    fn on_key(&mut self, event: &crate::widget::KeyEvent, ectx: &mut EventCtx) -> bool {
        if !self.visible {
            return false;
        }
        let Some(ui) = ectx.ui.as_deref_mut() else {
            return false;
        };
        if let Some(idx) = self.focused_param {
            if event.state == ElementState::Pressed {
                let p = &mut self.display_params[idx];
                if p.2 == "code" {
                    if let Some(mut editor) = self.code_editor.take() {
                        // Edits go to the BUFFER, not the value: a script
                        // re-evaluates the node on every value change, and
                        // a half-typed line would fail on every keystroke.
                        // ctrl+enter applies, and so does leaving the row
                        // (Escape, a click elsewhere, `unfocus`).
                        let before = editor.clone();
                        let mut handled = true;
                        let mut edited = false;
                        let mut apply = false;
                        let mut should_unfocus = false;
                        let shift = event.shift;
                        let move_vertical = |editor: &mut TextEditorState, delta: i32| {
                            if shift && editor.select_anchor.is_none() {
                                editor.select_anchor = Some(editor.cursor_idx);
                            } else if !shift {
                                editor.clear_selection();
                            }
                            let (line, col) = get_cursor_line_col(&editor.buffer, editor.cursor_idx);
                            let total = editor.buffer.split('\n').count() as i32;
                            let target = (line as i32 + delta).clamp(0, total - 1) as usize;
                            editor.cursor_idx = map_2d_to_1d(&editor.buffer, target, col);
                        };
                        match &event.logical_key {
                            Key::Named(NamedKey::Backspace) => {
                                edited = editor.delete_backwards();
                            }
                            Key::Named(NamedKey::Delete) => {
                                edited = editor.delete_forwards();
                            }
                            Key::Named(NamedKey::Enter) if event.ctrl => {
                                apply = true;
                            }
                            Key::Named(NamedKey::Enter) => {
                                // Auto-indent: the line's own leading whitespace,
                                // one level deeper after an opening brace.
                                let line_start = get_line_start(&editor.buffer, editor.cursor_idx);
                                let line: String = editor.buffer.chars().skip(line_start).take(editor.cursor_idx - line_start).collect();
                                let mut indent: String = line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
                                if line.trim_end().ends_with('{') || line.trim_end().ends_with('(') || line.trim_end().ends_with('[') {
                                    indent.push_str(Self::CODE_INDENT);
                                }
                                editor.insert_text(&format!("\n{indent}"));
                                edited = true;
                            }
                            Key::Named(NamedKey::Tab) if event.shift => {
                                // Dedent the current line by one level, or what it has.
                                let line_start = get_line_start(&editor.buffer, editor.cursor_idx);
                                let leading = editor.buffer.chars().skip(line_start).take_while(|c| *c == ' ').count().min(Self::CODE_INDENT.len());
                                if leading > 0 {
                                    let chars: Vec<char> = editor.buffer.chars().collect();
                                    editor.buffer = chars[..line_start].iter().chain(&chars[line_start + leading..]).collect();
                                    editor.cursor_idx = editor.cursor_idx.saturating_sub(leading).max(line_start);
                                    editor.clear_selection();
                                    edited = true;
                                }
                            }
                            Key::Named(NamedKey::Tab) => {
                                editor.insert_text(Self::CODE_INDENT);
                                edited = true;
                            }
                            Key::Named(NamedKey::Escape) => {
                                apply = true;
                                should_unfocus = true;
                            }
                            Key::Named(NamedKey::ArrowLeft) => {
                                editor.move_cursor_left(shift);
                            }
                            Key::Named(NamedKey::ArrowRight) => {
                                editor.move_cursor_right(shift);
                            }
                            Key::Named(NamedKey::ArrowUp) => move_vertical(&mut editor, -1),
                            Key::Named(NamedKey::ArrowDown) => move_vertical(&mut editor, 1),
                            Key::Named(NamedKey::Home) => {
                                if shift && editor.select_anchor.is_none() {
                                    editor.select_anchor = Some(editor.cursor_idx);
                                } else if !shift {
                                    editor.clear_selection();
                                }
                                editor.cursor_idx = get_line_start(&editor.buffer, editor.cursor_idx);
                            }
                            Key::Named(NamedKey::End) => {
                                if shift && editor.select_anchor.is_none() {
                                    editor.select_anchor = Some(editor.cursor_idx);
                                } else if !shift {
                                    editor.clear_selection();
                                }
                                editor.cursor_idx = get_line_end(&editor.buffer, editor.cursor_idx);
                            }
                            Key::Character(s) => {
                                if event.ctrl {
                                    match s.to_lowercase().as_str() {
                                        "f" => {
                                            editor.move_cursor_right(false);
                                        }
                                        "b" => {
                                            editor.move_cursor_left(false);
                                        }
                                        "p" => move_vertical(&mut editor, -1),
                                        "n" => move_vertical(&mut editor, 1),
                                        "a" if event.shift => {
                                            editor.select_all();
                                        }
                                        "a" => {
                                            editor.clear_selection();
                                            editor.cursor_idx = get_line_start(&editor.buffer, editor.cursor_idx);
                                        }
                                        "e" => {
                                            editor.clear_selection();
                                            editor.cursor_idx = get_line_end(&editor.buffer, editor.cursor_idx);
                                        }
                                        "d" => {
                                            edited = editor.delete_forwards();
                                        }
                                        "h" => {
                                            edited = editor.delete_backwards();
                                        }
                                        "k" => {
                                            let current_idx = editor.cursor_idx;
                                            let end_idx = get_line_end(&editor.buffer, current_idx);
                                            let chars: Vec<char> = editor.buffer.chars().collect();
                                            if current_idx < chars.len() {
                                                let delete_end = if chars[current_idx] == '\n' { current_idx + 1 } else { end_idx };
                                                editor.buffer = chars[..current_idx].iter().chain(&chars[delete_end..]).collect();
                                                editor.clear_selection();
                                                edited = true;
                                            }
                                        }
                                        // The clipboard and history chords are the
                                        // context actions, so the chord, the runner's
                                        // routing and the menu row do one thing.
                                        "c" | "x" | "v" | "z" => {
                                            let action = match (s.to_lowercase().as_str(), event.shift) {
                                                ("c", _) => crate::widget::ContextAction::Copy,
                                                ("x", _) => crate::widget::ContextAction::Cut,
                                                ("v", _) => crate::widget::ContextAction::Paste,
                                                ("z", true) => crate::widget::ContextAction::Redo,
                                                _ => crate::widget::ContextAction::Undo,
                                            };
                                            apply_code_action(&mut editor, &mut self.code_history, action);
                                        }
                                        _ => {
                                            handled = false;
                                        }
                                    }
                                } else {
                                    editor.insert_text(s);
                                    edited = true;
                                }
                            }
                            _ => {
                                handled = false;
                            }
                        }
                        if edited {
                            // Typing runs coalesce into one undo step; anything
                            // structural (a newline, a paste, a deletion of a
                            // selection) starts a new one.
                            let plain_char = matches!(&event.logical_key, Key::Character(c) if !event.ctrl && c.chars().count() == 1);
                            if plain_char {
                                self.code_history.record_grouped(before, 1);
                            } else {
                                self.code_history.record(before);
                            }
                        }
                        if apply {
                            p.1 = editor.buffer.clone();
                        }
                        if should_unfocus {
                            self.focused_param = None;
                            self.code_editor = None;
                            self.code_history.clear();
                        } else {
                            self.code_editor = Some(editor);
                        }
                        if handled {
                            return true;
                        }
                    }
                } else if is_text_row(&p.2) {
                    if let Some(d) = &mut self.choices[idx] {
                        if d.open && d.keyboard_input(event, ui) {
                            if d.take_change() {
                                if let Some(val) = d.get_value_string() {
                                    if let Some(tb) = &mut self.texts[idx] {
                                        tb.text = val.clone();
                                        tb.edit_buffer = val.clone();
                                    }
                                    p.1 = val;
                                }
                            }
                            return true;
                        }
                    }
                    if let Some(tb) = &mut self.texts[idx] {
                        if tb.keyboard_input(event, ui) {
                            if !tb.editing {
                                p.1 = tb.text.clone();
                                self.focused_param = None;
                            } else {
                                p.1 = tb.edit_buffer.clone();
                            }
                            return true;
                        }
                    }
                } else if p.2.starts_with("choice") {
                    if let Some(d) = &mut self.choices[idx] {
                        if d.keyboard_input(event, ui) {
                            if !d.open {
                                if let Some(val) = d.get_value_string() {
                                    p.1 = val;
                                }
                                self.focused_param = None;
                            }
                            return true;
                        }
                    }
                } else if p.2.starts_with("spinbox") {
                    if let Some(sb) = &mut self.spinboxes[idx] {
                        if sb.keyboard_input(event, ui) {
                            if !sb.editing {
                                p.1 = sb.value.to_string();
                                self.focused_param = None;
                            } else {
                                p.1 = sb.edit_buffer.clone();
                            }
                            return true;
                        }
                    }
                } else if p.2.starts_with("slider") {
                    if let Some(s) = &mut self.sliders[idx] {
                        if s.keyboard_input(event, ui) {
                            let new_val = s.get_scaled_value();
                            p.1 = format!("{:.*}", slider_decimals(&p.2), new_val);
                            if !s.editing {
                                self.focused_param = None;
                            }
                            return true;
                        }
                    }
                } else if p.2.starts_with("color") || p.2 == "rgb" || p.2 == "rgba" {
                    if let Some(c) = &mut self.colors[idx] {
                        if c.keyboard_input(event, ui) {
                            if let Some(val) = c.get_value_string() {
                                p.1 = val;
                            }
                            if !c.editing {
                                self.focused_param = None;
                            }
                            return true;
                        }
                    }
                } else if is_vec_row(&p.2) {
                    if let Some(f) = &mut self.float3s[idx] {
                        if f.keyboard_input(event, ui) {
                            p.1 = f.value_string();
                            if f.editing_idx().is_none() {
                                self.focused_param = None;
                            }
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// A wheel notch or finger scroll: to the value control the gesture began on, else the pane.
    fn on_wheel(&mut self, delta: &MouseScrollDelta, px: &f32, py: &f32, ectx: &mut EventCtx) -> bool {
        let self_id = ectx.id;
        if !self.visible {
            return false;
        }
        let (px, py) = (*px, *py);
        let Some(ui) = ectx.ui.as_deref_mut() else {
            return false;
        };
        let mut changed = false;
        // A value row that took the wheel (slider/float3/spinbox),
        // whether or not its 2-decimal string ticked over. Gating the
        // pane's viewport-scroll fallback on the STRING (`changed`)
        // let every sub-tick trackpad event scroll the pane instead —
        // a slider-vs-pane tug-of-war that shifted the rows under the
        // pointer mid-adjust.
        let mut wheel_taken = false;
        // Gesture ownership. A gesture the PANE acquired — its first
        // event fell on no control and scrolled the rows — stays the
        // pane's until the gesture ends (`scroll_gesture_new`), however
        // the rows travel under the pointer meanwhile. Without this a
        // list scroll ran until a slider's halo or a spinbox row slid
        // under the pointer, which then took every remaining event of
        // the same gesture and adjusted a value the user never aimed
        // at (the Alt+D settings tab, 2026-09-20). A gesture that
        // began ON a control is that control's, as before, and one
        // nobody claimed (dead space, a control at its limit) is still
        // open to spatial acquisition — the band slider's feel.
        let pane_owns = !ui.scroll_gesture_new && ui.scroll_initiate_widget_id == Some(self_id);
        // A trackpad gesture is the PANE's, from anywhere — a
        // two-finger swipe is a scroll everywhere on the desktop, and
        // a pane that is mostly controls (the Alt+D Settings list)
        // was otherwise scrollable only from a label. A wheel notch
        // still adjusts the control under the pointer. Unconditional
        // since 2026-09-21: the first cut kept hover-adjust for finger
        // gestures in a pane whose content fits, which made the main
        // params pane feel different from the Settings list depending
        // on how many parameters the node had. Finger-end frames (no
        // delta) take the same path so the pane's coast starts.
        let finger = matches!(delta, MouseScrollDelta::PixelDelta(_))
            && matches!(
                crate::widget::scroll_motion::current_scroll_phase(),
                crate::widget::ScrollPhase::Finger | crate::widget::ScrollPhase::FingerEnd
            );
        let rects = self.get_param_rects();
        // The exception to the finger rule: a VALUE control takes the
        // gesture that BEGINS on it, trackpad included, and keeps it
        // until the gesture ends — a spinbox by its row, a slider
        // and each row of a float3 by the band's own halo
        // (`Slider::scroll_hit`), the same zones a wheel notch
        // acquires by. A two-finger scroll is aimed like a wheel:
        // over a band it is the value being turned, anywhere else
        // (the label column, the gaps, a toggle, a section header)
        // the pane being scrolled. Spinboxes first (2026-09-28,
        // earlier the same day), sliders and float3 rows after: the
        // slider's wheel arm was written for trackpad streams all
        // along, and the pane's rule kept every one of them from
        // reaching it. A gesture the pane or another control
        // acquired never lands on a control sliding under the
        // pointer — that is the leak the latch exists to stop — so
        // the LATCHED control is asked first, and only a gesture
        // nobody owns is open to the control under the pointer.
        let owner = {
            let latched_to = |id| !ui.scroll_gesture_new && ui.scroll_initiate_widget_id == Some(id);
            let in_row = |r: (f32, f32, f32, f32)| {
                py >= r.1 && py <= r.1 + r.3 && px >= self.rect.x && px <= self.rect.x + self.rect.width
            };
            let n = self.display_params.len();
            let latched = (0..n).find(|&i| {
                self.spinboxes[i].as_ref().is_some_and(|sb| latched_to(sb.base().id()))
                    || self.sliders[i].as_ref().is_some_and(|sl| latched_to(sl.base().id()))
                    || self.float3s[i].as_ref().is_some_and(|f| f.wheel_latched(ui))
            });
            // Under the pointer: the NEAREST control whose zone
            // holds it, by the distance to the band's (or the
            // spinbox row's) centre line. A band's halo reaches
            // past the band by more than the gap between rows, so
            // two neighbours' zones hold any point between them,
            // and the first in row order is the wrong answer for
            // the lower half of that gap.
            latched.or_else(|| {
                if pane_owns {
                    return None;
                }
                (0..n)
                    .filter_map(|i| {
                        if rects[i].3 <= 0.0 {
                            return None;
                        }
                        if self.spinboxes[i].is_some() {
                            let r = rects[i];
                            return in_row(r).then(|| (i, (py - (r.1 + r.3 * 0.5)).abs()));
                        }
                        if let Some(sl) = self.sliders[i].as_ref() {
                            let (sx, sy, sw, sh) = sl.rect();
                            let ty = sl.label_strip();
                            let band = Rect { x: sx, y: sy + ty, width: sw, height: sh - ty };
                            return sl
                                .inner()
                                .scroll_hit(band, px, py)
                                .then(|| (i, (py - (band.y + band.height * 0.5)).abs()));
                        }
                        // A float3's bands, and its trackball when it has one.
                        self.float3s[i].as_ref().and_then(|f| f.wheel_zone(px, py)).map(|d| (i, d))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(i, _)| i)
            })
        };
        let pane_takes = pane_owns || (finger && owner.is_none());
        for (i, p) in self.display_params.iter_mut().enumerate() {
            if pane_takes {
                break;
            }
            // A gesture a control owns is nobody else's: the pointer
            // may have drifted onto another band's halo since it began.
            if owner.is_some() && owner != Some(i) {
                continue;
            }
            if p.2.starts_with("slider") {
                // The capture zone is the slider's own shape halo
                // (`Slider::scroll_hit` — the band plus the traveling
                // swell, inset), so scrolls off the shape fall through
                // to the pane's viewport scroll below.
                // `owner` made the zone test: this slider is latched
                // (mid-gesture the slider that acquired the scroll
                // keeps it — its halo travels away from the pointer
                // as the value moves), or its band is the nearest
                // whose halo holds the pointer.
                let in_zone = owner == Some(i);
                if in_zone {
                    if let Some(s) = &mut self.sliders[i] {
                        let was_scroll = s.scroll_enabled;
                        s.set_scroll(true);
                        // Ungated: the in_zone halo above already gated
                        // spatially, and the adapter's rect gate would
                        // clip the halo's fringe outside the row rect.
                        if s.mouse_wheel_ungated(delta, px, py, ui) {
                            wheel_taken = true;
                            let new_val = s.get_scaled_value();
                            let old_val = &p.1;
                            let new_val_str = format!("{:.*}", slider_decimals(&p.2), new_val);
                            if *old_val != new_val_str {
                                p.1 = new_val_str;
                                changed = true;
                            }
                        }
                        s.set_scroll(was_scroll);
                    }
                }
            } else if is_vec_row(&p.2) {
                if owner == Some(i) {
                    // The group's rows apply the slider-row contract
                    // themselves (band halo / gesture latch, else the
                    // row strip; ungated forward).
                    if let Some(f) = &mut self.float3s[i] {
                        if f.wheel(delta, px, py, ui) {
                            wheel_taken = true;
                            let new_val_str = f.value_string();
                            if p.1 != new_val_str {
                                p.1 = new_val_str;
                                changed = true;
                            }
                        }
                    }
                }
            } else if p.2.starts_with("spinbox")
                && owner == Some(i) {
                    if let Some(sb) = &mut self.spinboxes[i] {
                        // The widget's own wheel arm: one step per
                        // notch, fractions carried between events.
                        // Ungated — `owner` already tested the
                        // row, and a latched gesture may have
                        // drifted off it.
                        wheel_taken = true;
                        ui.scroll_initiate_widget_id = Some(sb.base().id());
                        sb.mouse_wheel_ungated(delta, px, py, ui);
                        let new_val_str = sb.value.to_string();
                        if p.1 != new_val_str {
                            p.1 = new_val_str;
                            changed = true;
                        }
                    }
                }
        }

        // The legacy tail's `self.hit_test(px, py, ctx)`: occlusion via the adapter's
        // address, then rect-or-popover containment.
        let mut swallowed = changed || wheel_taken;
        if !ui.is_coordinate_covered(self_id, px, py) {
            let in_rect = px >= self.rect.x
                && px <= self.rect.x + self.rect.width
                && py >= self.rect.y
                && py <= self.rect.y + self.rect.height;
            let in_popover = self.own_popover_rect().is_some_and(|(rx, ry, rw, rh)| {
                px >= rx && px <= rx + rw && py >= ry && py <= ry + rh
            });
            if in_rect || in_popover {
                if !changed && !wheel_taken {
                    if crate::scroll_debug() {
                        eprintln!("[scroll] params: PANE-SCROLL fallback at ({px:.0},{py:.0})");
                    }
                    // The pane takes the gesture (see `pane_owns`).
                    ui.scroll_initiate_widget_id = Some(self_id);
                    let max_scroll = (self.content_h - self.rect.height).max(0.0);
                    self.scroll_motion.reconcile(0.0, self.scroll_y);
                    let moved = self.scroll_motion.apply(
                        delta,
                        (crate::widget::LINE_PX, crate::widget::LINE_PX),
                        crate::widget::Bounds::max(0.0),
                        crate::widget::Bounds::max(max_scroll),
                    );
                    self.scroll_y = self.scroll_motion.y.pos();
                    if moved {
                        self.update_slider_rects();
                        self.activity.bump();
                        self.recompute_scrollbar_raised();
                        self.rehover_after_scroll(ui);
                    }
                }
                // An opaque pane swallows EVERY wheel over it, whether
                // anything moved or not: returning false would hand the
                // event to whatever lies BEHIND the plate — the designer
                // routes unhandled wheels to the 3D viewport, whose rect
                // is the whole window in the floating layout, so a
                // near-miss on a slider would orbit the camera through
                // the pane.
                swallowed = true;
            }
        }

        swallowed
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
            let sb_track_h = self.rect.height - 8.0;
            let sb_track_y = self.rect.y + 4.0;
            let visible_ratio = self.rect.height / self.content_h;
            let thumb_h = if sb_track_h <= 20.0 {
                sb_track_h
            } else {
                (sb_track_h * visible_ratio).clamp(20.0, sb_track_h)
            };
            let max_scroll = (self.content_h - self.rect.height).max(0.0);

            let target_thumb_y = py - self.drag_offset_y;
            let new_scroll_ratio = if sb_track_h - thumb_h > 0.0 {
                ((target_thumb_y - sb_track_y) / (sb_track_h - thumb_h)).clamp(0.0, 1.0)
            } else {
                0.0
            };

            let old_scroll = self.scroll_y;
            self.scroll_y = new_scroll_ratio * max_scroll;
            if (self.scroll_y - old_scroll).abs() > 0.01 {
                self.update_slider_rects();
                return true;
            }
            return false;
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
