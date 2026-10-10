//! The pointer: the cursor icon for a point (the CSD resize edges), and the pointer entering,
//! moving, leaving (releasing whatever it held), pressing (the CSD move and resize, the
//! outside-press popover close) and releasing.

use super::*;

impl Driver {
    /// The cursor for the pointer at (lx, ly): the app's
    /// [`Application::cursor_icon`] override, else the standard-CSD edge
    /// cursors (status bars and non-standard-CSD apps fall back to Default).
    pub fn cursor_icon_at<A: Application>(&self, app: &A, lx: f32, ly: f32, size: LogicalSize) -> CursorIcon {
        // Over the context menu the pointer is the menu's,
        // whatever of the app lies at that place under it (a splitter, a
        // resize border) — and in their popups that place may be outside
        // the window altogether.
        if crate::widget::context_menu::is_visible() && crate::widget::context_menu::hit_test(lx, ly) {
            return CursorIcon::Default;
        }
        if let Some(icon) = app.cursor_icon(lx, ly) {
            return icon;
        }
        if !app.standard_csd()
            || !app.csd_resize_borders()
        {
            return CursorIcon::Default;
        }
        match csd_edge(lx, ly, size) {
            Some(ResizeEdge::TopLeft) => CursorIcon::NwResize,
            Some(ResizeEdge::TopRight) => CursorIcon::NeResize,
            Some(ResizeEdge::Top) => CursorIcon::NResize,
            Some(ResizeEdge::BottomLeft) => CursorIcon::SwResize,
            Some(ResizeEdge::BottomRight) => CursorIcon::SeResize,
            Some(ResizeEdge::Bottom) => CursorIcon::SResize,
            Some(ResizeEdge::Left) => CursorIcon::WResize,
            Some(ResizeEdge::Right) => CursorIcon::EResize,
            None => CursorIcon::Default,
        }
    }

    pub fn pointer_enter<A: Application>(&mut self, t: Turn<'_, A>, pos: LogicalPosition) {
        self.note_input();
        self.pointer_motion(t, pos);
    }

    /// The pointer moved to `pos`.
    pub fn pointer_motion<A: Application>(&mut self, t: Turn<'_, A>, pos: LogicalPosition) {
        self.note_input();
        let mut rebuild = false;
        t.app.handle_pointer_move(pos, &mut rebuild);
        if rebuild {
            *t.redraw = true;
        }
    }

    /// The pointer left the window. Focus can move mid-gesture (a fullscreen
    /// switch, a relayout sliding the window away): the real release then
    /// lands on another surface, and an armed drag would live forever. End
    /// held gestures with synthetic releases at the last known position
    /// first; then clear hover with an off-screen move — safe now that no
    /// drag is held.
    pub fn pointer_leave<A: Application>(&mut self, mut t: Turn<'_, A>) {
        self.note_input();
        if self.buttons_down != 0 {
            let (px, py) = self.cursor_pos;
            for btn in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
                if self.buttons_down & button_bit(btn) == 0 {
                    continue;
                }
                let mut rebuild = false;
                let msg = t.app.handle_mouse_input(
                    btn,
                    ElementState::Released,
                    LogicalPosition::new(px, py),
                    &mut rebuild,
                );
                t.deliver(msg, rebuild);
            }
            self.buttons_down = 0;
        }
        let mut rebuild = false;
        t.app.handle_pointer_move(LogicalPosition::new(-10000.0, -10000.0), &mut rebuild);
        if rebuild {
            *t.redraw = true;
        }
    }

    /// A button went down at `pos`. Returns a grab for the shell to start
    /// when the press is the window's (a CSD border, the titlebar band, a
    /// movable root plate); otherwise the press has been dispatched.
    pub fn pointer_press<A: Application>(
        &mut self,
        mut t: Turn<'_, A>,
        btn: MouseButton,
        pos: LogicalPosition,
        site: PressSite,
    ) -> Press {
        self.note_input();
        self.buttons_down |= button_bit(btn);
        let (lx, ly) = (pos.x, pos.y);

        // Client-side decorations: drag and resize. Never on the menu popup:
        // its presses are the menu's, and its coordinates, translated into the
        // window's, would otherwise read as a resize border or a movable plate.
        if site.can_grab
            && btn == MouseButton::Left
            && !site.on_popup
            && t.app.standard_csd()
        {
            // Resize borders off: the compositor's own band outside the
            // window handles it; the move checks still run, so drag-to-move
            // still works.
            if t.app.csd_resize_borders() && !site.own_edges {
                if let Some(edge) = csd_edge(lx, ly, site.size) {
                    return Press::Resize(edge);
                }
            }
            // The titlebar band: y in [8, 32), clear of the top-right buttons.
            let is_widget = t.app.ui_context().is_some_and(|ctx| ctx.is_widget_at(lx, ly));
            if (!is_widget
                && t.app.csd_titlebar_move()
                && (CSD_BORDER..32.0).contains(&ly)
                && lx < site.size.width - 70.0)
                || t.app.is_movable_root_plate_at(lx, ly)
            {
                return Press::Move;
            }
        }

        // Outside-press close for open popovers, BEFORE the app's dispatch:
        // apps commonly region-gate their routing, so an open menu's owner may
        // never hear about a press elsewhere.
        if btn == MouseButton::Left {
            close_popovers_missed_by(t.app, lx, ly);
        }
        // A field still open after the press is announced again, so the
        // frame must be built (`ime::note_press`).
        if crate::ime::note_press() {
            *t.redraw = true;
        }

        let mut rebuild = false;
        let msg = t.app.handle_mouse_input(btn, ElementState::Pressed, pos, &mut rebuild);
        t.deliver(msg, rebuild);
        Press::Dispatched
    }

    /// A button came up at `pos`.
    pub fn pointer_release<A: Application>(&mut self, mut t: Turn<'_, A>, btn: MouseButton, pos: LogicalPosition) {
        self.note_input();
        self.buttons_down &= !button_bit(btn);
        let mut rebuild = false;
        let msg = t.app.handle_mouse_input(btn, ElementState::Released, pos, &mut rebuild);
        t.deliver(msg, rebuild);
    }
}
