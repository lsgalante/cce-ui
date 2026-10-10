//! A breadcrumb's input: a click on a segment, the right press that records it and opens the
//! context menu, the keyboard walking the segments, and context actions.

use super::*;

impl Input for Breadcrumb {
    /// The widget claims only its segment run, not its laid-out strip: hosts
    /// float the breadcrumb over live content (the designer's graph runs
    /// underneath), and presses on the strip's empty remainder must fall
    /// through to what's beneath. Right-clicks sharpen with it — the
    /// copy-path menu opens over the run, the content's own menu elsewhere.
    fn hit(&self, rect: Rect, px: f32, py: f32) -> bool {
        self.seg_at(rect, px, py).is_some()
    }

    fn focus_role(&self) -> crate::widget::FocusRole {
        crate::widget::FocusRole::Plate
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::FocusIn => {
                self.focused = true;
                // The cursor starts on the current directory (the last segment).
                self.focus_seg = self.path.len().checked_sub(1);
                true
            }
            Event::FocusOut => {
                self.focused = false;
                self.focus_seg = None;
                true
            }
            Event::KeyInput(key_event) => {
                // Left / Right walk the VISIBLE segments (the ellipsis is not a
                // stop); Enter / Space navigate to the cursor's segment, the click.
                if !self.focused || key_event.state != crate::widget::ElementState::Pressed {
                    return false;
                }
                let logicals: Vec<usize> =
                    self.visible_segs(ectx.rect).into_iter().filter_map(|s| s.logical).collect();
                match key_event.logical_key {
                    crate::widget::Key::Named(crate::widget::NamedKey::ArrowLeft)
                    | crate::widget::Key::Named(crate::widget::NamedKey::ArrowRight) => {
                        if logicals.is_empty() {
                            return false;
                        }
                        let right = key_event.logical_key == crate::widget::Key::Named(crate::widget::NamedKey::ArrowRight);
                        let pos = self.focus_seg.and_then(|f| logicals.iter().position(|l| *l == f));
                        let next = match (pos, right) {
                            (Some(p), true) => (p + 1).min(logicals.len() - 1),
                            (Some(p), false) => p.saturating_sub(1),
                            (None, true) => 0,
                            (None, false) => logicals.len() - 1,
                        };
                        self.focus_seg = Some(logicals[next]);
                        true
                    }
                    crate::widget::Key::Named(crate::widget::NamedKey::Enter)
                    | crate::widget::Key::Named(crate::widget::NamedKey::Space) => {
                        match self.focus_seg {
                            Some(i) if i < self.path.len() => {
                                self.clicked_seg = Some(i);
                                true
                            }
                            _ => false,
                        }
                    }
                    _ => false,
                }
            }
            Event::PointerMove { x: px, y: py, .. } => {
                let r = ectx.rect;
                let was = self.hovered;
                self.hovered =
                    *px >= r.x && *px <= r.x + r.width && *py >= r.y && *py <= r.y + r.height;
                let old = self.hovered_seg;
                self.hovered_seg = if self.hovered { self.seg_at(r, *px, *py) } else { None };
                was != self.hovered || old != self.hovered_seg
            }
            Event::MouseLeave => {
                let changed = self.hovered || self.hovered_seg.is_some();
                self.hovered = false;
                self.hovered_seg = None;
                changed
            }
            Event::MouseButton {
                button: MouseButton::Right,
                state: ElementState::Pressed,
                x: px,
                y: py,
                ..
            } => {
                // Record the segment first: the shared menu's header reads it (via the
                // `as_any` downcast in `UiContext::handle_right_click`) to title itself with
                // that segment's path, and "Copy Path" copies it.
                self.right_clicked_seg = self.seg_at(ectx.rect, *px, *py);
                ectx.open_context_menu(*px, *py);
                true
            }
            Event::MouseButton {
                button: MouseButton::Left,
                state: ElementState::Pressed,
                x: px,
                y: py,
                ..
            } => {
                if let Some(i) = self.seg_at(ectx.rect, *px, *py) {
                    if i < self.path.len() {
                        self.clicked_seg = Some(i);
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }


    fn context_action(&mut self, action: crate::widget::ContextAction) -> bool {
        if action != crate::widget::ContextAction::CopyPath {
            return false;
        }
        let idx = self.right_clicked_seg.unwrap_or(self.path.len());
        let path_str = self.path_to_seg(idx);
        crate::widget::clipboard::copy_to_clipboard(&path_str);
        true
    }
}
