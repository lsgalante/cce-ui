//! The wheel (a multi-line box scrolls) and `impl Input`.

use super::*;

impl Input for TextBox {
    fn focus_role(&self) -> crate::widget::FocusRole {
        crate::widget::FocusRole::Well
    }

    /// A multi-line box that is editing types Tab, as an editor does; a one-line field lets
    /// the Tab walk take it (Tab leaves a field).
    fn keeps_tab(&self) -> bool {
        self.multiline && self.editing
    }
    /// Advances the wheel glide / trackpad coast behind the scroll offsets.
    /// Cheap when idle (the common case); `wants_tick` is unconditional
    /// because it is sampled once at registration.
    fn tick(&mut self, dt: f32, _rect: Rect) -> bool {
        self.scroll_motion.reconcile(self.scroll_x, self.scroll_y);
        if !self.scroll_motion.is_animating() {
            return false;
        }
        let (mx, my) = self.scroll_max;
        let moved = self.scroll_motion.tick(dt, Bounds::max(mx), Bounds::max(my));
        self.scroll_x = self.scroll_motion.x.pos();
        self.scroll_y = self.scroll_motion.y.pos();
        moved || self.scroll_motion.is_animating()
    }

    fn wants_tick(&self) -> bool {
        true
    }

    fn tracks_base_focus(&self) -> bool {
        false
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        // A press moves the caret: a composition in progress is left where
        // it was, cancelled.
        if let Event::MouseButton { state: ElementState::Pressed, .. } = event {
            self.abandon_composition();
        }
        match event {
            Event::MouseButton { button: MouseButton::Right, state: ElementState::Pressed, x: px, y: py, .. } => {
                if self.disabled { return false; }
                // Adapter hit-gates presses; legacy focused an un-editing box before opening
                // the menu (work-before-menu, so `opens_context_menu` can't express it).
                if !self.editing {
                    self.begin_editing();
                    ectx.request_focus();
                }
                ectx.open_context_menu(*px, *py);
                true
            }
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, x: px, y: py, .. } => {
                if self.disabled { return false; }
                // A click aims into the field: it places the caret where it landed,
                // whether or not it is the click that focuses. Only KEYBOARD focus
                // (`Event::FocusIn` -> `begin_editing`) arms select-all. That split is
                // the convention everywhere — tabbing selects a field, clicking points
                // into it — and it is what a prefilled box needs: select-all on the
                // focusing click meant the first keystroke wiped the whole value, which
                // is wrong for the ~26 prefilled single-line boxes across the fleet
                // (login username, reply subject, a unit's ExecStart, a config value).
                // This used to be carved out for multiline only; both arms now agree.
                let focusing = !self.editing;
                if focusing {
                    self.begin_editing();
                    ectx.request_focus();
                }
                let idx = self.position_to_idx(*px, *py);
                self.cursor_idx = idx;
                self.select_anchor = Some(idx);
                self.all_selected = false;
                if focusing {
                    self.sync_editor_state();
                }
                true
            }
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Released, x: px, y: py, .. } => {
                if self.disabled { return false; }
                // Legacy gated releases on the hit test; the adapter delivers them ungated, so
                // re-check containment (the plain block rect — row spans approximated).
                let (bx, by, bw, bh) = (self.rect.x, self.rect.y, self.rect.width, self.rect.height);
                if !(*px >= bx && *px <= bx + bw && *py >= by && *py <= by + bh) {
                    return false;
                }
                if self.dragging {
                    self.dragging = false;
                }
                if self.select_anchor == Some(self.cursor_idx) {
                    self.select_anchor = None;
                }
                true
            }
            Event::PointerMove { x: px, y: py, .. } => {
                // The drag-selection half of the legacy `on_cursor_moved`; hover bookkeeping
                // is the adapter's (Enter/Leave below).
                if self.disabled {
                    return false;
                }
                if self.dragging && self.editing {
                    return self.extend_selection_to(*px, *py);
                }
                false
            }
            Event::MouseEnter => {
                self.hovered = !self.disabled;
                true
            }
            Event::MouseLeave => {
                self.hovered = false;
                true
            }
            Event::MouseWheel { delta, .. } => self.handle_wheel(delta),
            Event::KeyInput(key_event) => self.handle_key(key_event),
            Event::FocusIn => {
                // The legacy `focus()`: enter editing and claim the global slot (unless
                // disabled — legacy early-returned before `set_focused`). Skipped when
                // already editing: a press-then-set_focused sequence must not re-arm
                // select-all over the caret the press just placed.
                if !self.disabled && !self.editing {
                    self.begin_editing();
                    ectx.request_focus();
                }
                false
            }
            Event::FocusOut => {
                self.commit_editing();
                false
            }
            _ => false,
        }
    }

    fn take_change(&mut self) -> bool {
        self.take_change()
    }

    fn value_string(&self) -> Option<String> {
        Some(self.text.clone())
    }

    fn set_value_string(&mut self, val: &str) -> bool {
        self.set_value(val)
    }

    fn a11y_text(&self) -> Option<crate::a11y::A11yText> {
        // What the box holds, never an input method's provisional run; a password as bullets.
        let held = if self.editing { self.committed_buffer() } else { self.text.clone() };
        let len = held.chars().count();
        let text = if self.is_password { "\u{2022}".repeat(len) } else { held };
        // The caret and anchor are indices into what is shown; a composition sits at the
        // caret, so past its start they come back to it.
        let held_idx = |i: usize| {
            let i = match self.composing {
                Some((start, n)) if i > start => i.saturating_sub(n).max(start),
                _ => i,
            };
            i.min(len)
        };
        let selection = self.editing.then(|| {
            let focus = held_idx(self.cursor_idx);
            (self.select_anchor.map_or(focus, held_idx), focus)
        });
        Some(crate::a11y::A11yText {
            text,
            selection,
            multiline: self.multiline,
            password: self.is_password,
            editable: !self.disabled,
            placeholder: self.placeholder.clone(),
            kind: None,
        })
    }

    fn a11y_set_text(&mut self, text: &str) -> bool {
        if self.disabled {
            return false;
        }
        self.abandon_composition();
        let held = if self.editing { self.committed_buffer() } else { self.text.clone() };
        if held == text {
            return false;
        }
        if self.editing {
            let before = self.snapshot();
            self.history.record(before);
        } else {
            self.history.clear();
        }
        self.text = text.to_string();
        self.edit_buffer = text.to_string();
        self.just_changed = true;
        self.cursor_idx = text.chars().count();
        self.select_anchor = None;
        self.all_selected = false;
        self.sync_editor_state();
        self.clamp_scroll();
        if self.editing {
            self.scroll_to_cursor();
        }
        true
    }

    fn context_action(&mut self, action: crate::widget::ContextAction) -> bool {
        use crate::widget::ContextAction as CA;
        match action {
            CA::Cut => {
                let res = self.cut_selection();
                if res {
                    self.just_changed = true;
                }
                res
            }
            CA::Copy => {
                self.copy_selection();
                true
            }
            CA::Paste => {
                let res = self.paste_from_clipboard();
                if res {
                    self.just_changed = true;
                }
                res
            }
            CA::SelectAll => {
                self.select_all();
                true
            }
            CA::ClearText => {
                self.set_value("");
                true
            }
            CA::Undo => self.undo_edit(),
            CA::Redo => self.redo_edit(),
            _ => false,
        }
    }

    fn draggable(&self, _rect: Rect) -> bool {
        !self.disabled
    }

    fn is_dragging(&self) -> bool {
        self.dragging
    }

    fn drag_begin(&mut self, _px: f32, _py: f32, _rect: Rect) {
        if self.disabled || !self.editing { return; }
        self.dragging = true;
    }

    fn drag_update(&mut self, px: f32, py: f32, _rect: Rect) -> bool {
        if self.disabled || !self.editing { return false; }
        self.extend_selection_to(px, py)
    }

    fn drag_end(&mut self) {
        self.dragging = false;
    }
}

impl TextBox {

    /// Port of the legacy `mouse_wheel` body (scroll the multiline/no-wrap viewports).
    pub(super) fn handle_wheel(&mut self, delta: &MouseScrollDelta) -> bool {
        let pad = self.pad();
        if self.disabled { return false; }
        let char_width = self.char_width();
        let line_height = self.line_height();

        let max_w = self.wrap_width(self.rect.width);

        let (lines, _) = if self.multiline {
            self.wrap_text(max_w)
        } else {
            let buffer = if self.editing { &self.edit_buffer } else { &self.text };
            (vec![buffer.clone()], vec![(0, 0); buffer.chars().count() + 1])
        };

        let mut dy_px = 0.0;
        let mut max_scroll_y = 0.0;
        if self.multiline {
            let content_h = lines.len() as f32 * line_height;
            max_scroll_y = (content_h - (self.rect.height - 2.0 * pad)).max(0.0);
            dy_px = match *delta {
                MouseScrollDelta::LineDelta(_, dy) => -dy * line_height * 2.0,
                MouseScrollDelta::PixelDelta(pos) => -pos.y as f32,
            };
        }

        let mut dx_px = 0.0;
        let mut max_scroll_x = 0.0;
        if !self.line_wrap_enabled() {
            let content_w = self.content_width(&lines);
            max_scroll_x = (content_w - (self.rect.width - 2.0 * pad)).max(0.0);
            let natural = crate::layout::touchpad_natural_scroll();
            let scroll_amt_x = match *delta {
                MouseScrollDelta::LineDelta(dx, dy) => {
                    if !self.multiline {
                        let scroll_val = if dy != 0.0 { -dy } else { if natural { -dx } else { dx } };
                        scroll_val * char_width * 3.0
                    } else {
                        let scroll_val = if natural { -dx } else { dx };
                        scroll_val * char_width * 3.0
                    }
                }
                MouseScrollDelta::PixelDelta(pos) => {
                    if !self.multiline {
                        
                        if pos.y != 0.0 { -pos.y as f32 } else { if natural { -pos.x as f32 } else { pos.x as f32 } }
                    } else {
                        if natural { -pos.x as f32 } else { pos.x as f32 }
                    }
                }
            };
            dx_px = scroll_amt_x;
        }

        // Both axes through the shared motion: notches glide, finger tracks
        // 1:1, a flick coasts. The pub offsets are the drawn values.
        self.scroll_max = (max_scroll_x, max_scroll_y);
        self.scroll_motion.reconcile(self.scroll_x, self.scroll_y);
        let discrete = matches!(delta, MouseScrollDelta::LineDelta(..));
        let changed = self.scroll_motion.apply_px(
            dx_px,
            dy_px,
            discrete,
            Bounds::max(max_scroll_x),
            Bounds::max(max_scroll_y),
        );
        self.scroll_x = self.scroll_motion.x.pos();
        self.scroll_y = self.scroll_motion.y.pos();
        changed
    }
}
