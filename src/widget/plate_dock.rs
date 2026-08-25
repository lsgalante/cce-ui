//! The plate-corner dock affordance (RFC Phase 7c), generalized from
//! cce-designer's plate corner: a small circular control on a pane plate's
//! top-right that opens a menu (Collapse/Expand, Detach/Reattach, plus
//! host-specific rows), collapses the plate to a title stub, and arms
//! click-vs-drag for dock repositioning.
//!
//! This module owns the app-agnostic PROTOCOL: control geometry and hit
//! testing, the per-plate state vocabulary, the standard menu assembly, and
//! the press-becomes-drag threshold. The HOST keeps everything that is
//! policy: which plates carry controls, where docks are and what dropping
//! means, how a detached pane becomes a window (process model, sync channel),
//! and how collapse reshapes its layout. The role flip a detach implies on
//! the plate itself is [`crate::scene::paint::PlateSpec::detached`].

/// Radius of the corner control's hit circle (and its drawn dot).
pub const CORNER_R: f32 = 8.0;

/// Centre inset from the plate's top-right corner, on both axes. Clears the
/// plate's own corner arc at the radii the DE ships.
pub const CORNER_INSET: f32 = 14.0;

/// A plate needs at least this much room before it earns a corner control —
/// below it the trigger would cover the pane it belongs to.
pub const MIN_PLATE_SPAN: f32 = 3.0 * CORNER_INSET;

/// Height of a collapsed plate: its title stub. Deep enough for the title
/// text and the corner control that restores it, and no deeper.
pub const STUB_H: f32 = 26.0;

/// Pointer travel (Chebyshev) past which an armed corner press stops being a
/// click and becomes a dock drag.
pub const DRAG_THRESHOLD: f32 = 4.0;

/// One plate's dock state. `collapsed` and `detached` are exclusive in
/// practice (a detached pane's stub is not collapsible — the menu offers only
/// Reattach), but the type does not enforce it; the host's dispatch does.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlateDockState {
    /// Shrunk to its title stub in the host layout.
    pub collapsed: bool,
    /// Moved out into its own window; the host keeps a stub for reattaching.
    pub detached: bool,
}

impl PlateDockState {
    /// Whether the plate currently shows as a title stub (either way).
    pub fn stubbed(&self) -> bool {
        self.collapsed || self.detached
    }
}

/// What the standard corner menu can do to its plate. Hosts append their own
/// rows after these (the designer's spreadsheet span modes, for example) —
/// the row protocol is (label, action) pairs in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlateDockAction {
    Collapse,
    Expand,
    Detach,
    Reattach,
}

/// Centre of the corner control for a plate occupying `rect`
/// (`(x, y, w, h)`), or `None` when the plate is too small to carry one. A
/// stub is BUILT to carry the control and is shorter than the minimum span a
/// full pane must clear, so it centres vertically instead — that control is
/// the only way to bring the pane back.
pub fn corner_center(rect: (f32, f32, f32, f32), stubbed: bool) -> Option<(f32, f32)> {
    let (x, y, w, h) = rect;
    if w < MIN_PLATE_SPAN {
        return None;
    }
    if stubbed {
        return Some((x + w - CORNER_INSET, y + h / 2.0));
    }
    if h < MIN_PLATE_SPAN {
        return None;
    }
    Some((x + w - CORNER_INSET, y + CORNER_INSET))
}

/// Whether `(px, py)` hits a control centred at `center`.
pub fn corner_hit(center: (f32, f32), px: f32, py: f32) -> bool {
    let (dx, dy) = (px - center.0, py - center.1);
    dx * dx + dy * dy <= CORNER_R * CORNER_R
}

/// Whether an armed corner press at `press` has travelled far enough at
/// `cursor` to become a dock drag rather than a click.
pub fn press_becomes_drag(press: (f32, f32), cursor: (f32, f32)) -> bool {
    (cursor.0 - press.0).abs().max((cursor.1 - press.1).abs()) > DRAG_THRESHOLD
}

/// Draw the corner control at `center`: the plate-border-colored dot,
/// enlarged when `emphasized` (hovered, or its menu is open). Earned by the
/// second consumer (RFC 7c-2) — both hosts drew the identical dot.
pub fn draw_corner_dot(
    pc: &mut crate::scene::paint::PaintCtx,
    center: (f32, f32),
    emphasized: bool,
) {
    let r = if emphasized { CORNER_R * 1.15 } else { CORNER_R };
    let fill = crate::color::plate_border_color().unwrap_or([0.55, 0.58, 0.66, 0.85]);
    pc.circle(center.0, center.1, r, fill);
}

/// The standard corner-menu rows for a plate in `state`. A detached pane's
/// stub offers ONLY Reattach (collapsing it would mean nothing); otherwise
/// Collapse/Expand per state, then Detach when the host allows it. The host
/// appends its own rows after these and owns dispatch.
pub fn standard_menu(state: PlateDockState, can_detach: bool) -> Vec<(String, PlateDockAction)> {
    if state.detached {
        return vec![("Reattach".to_string(), PlateDockAction::Reattach)];
    }
    let mut rows = Vec::new();
    if state.collapsed {
        rows.push(("Expand".to_string(), PlateDockAction::Expand));
    } else {
        rows.push(("Collapse".to_string(), PlateDockAction::Collapse));
    }
    if can_detach {
        rows.push(("Detach".to_string(), PlateDockAction::Detach));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corner_geometry_and_arming() {
        // A full-size plate: control inset from the top-right.
        let c = corner_center((100.0, 50.0, 300.0, 200.0), false).unwrap();
        assert_eq!(c, (100.0 + 300.0 - CORNER_INSET, 50.0 + CORNER_INSET));
        assert!(corner_hit(c, c.0 + CORNER_R - 0.1, c.1));
        assert!(!corner_hit(c, c.0 + CORNER_R + 0.1, c.1));

        // Too narrow, or full-height too short: no control.
        assert!(corner_center((0.0, 0.0, MIN_PLATE_SPAN - 1.0, 200.0), false).is_none());
        assert!(corner_center((0.0, 0.0, 300.0, MIN_PLATE_SPAN - 1.0), false).is_none());

        // A stub is exempt from the height guard and centres vertically —
        // its control is the only way back.
        let s = corner_center((0.0, 0.0, 300.0, STUB_H), true).unwrap();
        assert_eq!(s, (300.0 - CORNER_INSET, STUB_H / 2.0));

        // Click-vs-drag threshold.
        assert!(!press_becomes_drag((10.0, 10.0), (13.0, 13.0)));
        assert!(press_becomes_drag((10.0, 10.0), (10.0, 15.0)));
    }

    #[test]
    fn standard_menu_variants() {
        let plain = PlateDockState::default();
        let rows = standard_menu(plain, true);
        assert_eq!(
            rows.iter().map(|(l, _)| l.as_str()).collect::<Vec<_>>(),
            ["Collapse", "Detach"]
        );
        assert_eq!(standard_menu(plain, false).len(), 1);

        let collapsed = PlateDockState { collapsed: true, detached: false };
        assert_eq!(standard_menu(collapsed, true)[0].1, PlateDockAction::Expand);
        assert!(collapsed.stubbed());

        let detached = PlateDockState { collapsed: false, detached: true };
        let rows = standard_menu(detached, true);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, PlateDockAction::Reattach);
        assert!(detached.stubbed());
    }
}
