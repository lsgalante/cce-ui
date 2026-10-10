//! `Dropdown`: a trigger showing the chosen option, and a popover list that grows out of it
//! (`Paint::popover` / `draw_popover`), opening and closing with an animation. It is a
//! detached-label control: the adapter draws the label in the strip above the trigger, at
//! `Layout::detached_label_inset`, and the trigger's plate is the control alone — a flush field
//! that is all run (`docs/surfaces.md`), or a flat plate (`with_flat`). Its arrow stands in the
//! trigger's arrow slot ([`arrow_slot`]), the one column every dropdown's ▼ shares; a list may
//! mark options as a context menu does (`mark_column`; `docs/widgets.md`, "Dropdown").
//!
//! - A press hits the widget's rect or its open popover (`Input::hit`); `is_expanded` says
//!   whether presses and keys are the dropdown's (not while the closing animation runs).
//! - `parent_snapshot` is data a host may assign (the Ramp's field does): the popover is
//!   clamped to that parent's rect and fades into its colour. No pointer is held.
//! - `Layout::intrinsic_measure_width` gives the `auto_width` measure (a host sizing its
//!   dropdown from `WidgetHost::measure`).
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the struct and its constants (the arrow slot, marks), construction and setters, `impl Layout` |
//! | `text` | measuring the trigger's text: monospace cells, shaped clusters, the displayed text |
//! | `popover` | the list's open and close animation, its geometry and rows |
//! | `paint` | the trigger's plate, arrow and text, the list, `impl Paint` |
//! | `input` | keys and `impl Input` |

mod input;
mod paint;
mod popover;
mod text;
#[cfg(test)]
mod tests;

use text::*;

use crate::colors;
use crate::scene::layout::{Rect, Size};
use crate::scene::paint::PaintCtx;
use crate::widget::model::{Adapted, EventCtx, Input, Layout, Paint};
use crate::widget::{
    WidgetHost, ElementState, Event, Key, MouseButton, NamedKey,
};


/// Read-data stand-in for the legacy direct-write `parent` pointer (6bd — no stored widget
/// pointers): the popover clamp and bg fade-blend read the host's rect/kind/color ctx-less at
/// paint time. Callers that want the Ramp clamp assign it directly, same activation model as
/// the old field.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParentSnapshot {
    pub rect: (f32, f32, f32, f32),
    pub is_ramp: bool,
    pub color: [f32; 4],
}

/// The side of a trigger's arrow, the `chevron-down` glyph: the size the
/// "▼" it replaced drew at in the 10px face it was set in.
pub(crate) const ARROW_SIDE: f32 = 8.0;

/// The side of a list row's mark glyph, and the gap after it (see
/// [`Dropdown::mark_column`]).
const MARK_SIDE: f32 = 11.0;

const MARK_GAP: f32 = 6.0;

/// The arrow's colour: the grey the "▼" was drawn in (raw sRGB, as a text
/// colour is).
const ARROW_COLOR: [f32; 4] = [0x83 as f32 / 255.0, 0x83 as f32 / 255.0, 0x8a as f32 / 255.0, 1.0];

/// Width of the ARROW SLOT, a trigger's right end in which its arrow is
/// centred: `ARROW_SLOT` and a relief wall at this band height. A textpick
/// picker (`ParametersBg`) is exactly this wide with its arrow in its middle,
/// so its arrow lines up with every other trigger's in a column of rows.
/// The wall is the lip a flush run's face rises out of at a field's seam.
pub fn arrow_slot(band_h: f32) -> f32 {
    ARROW_SLOT + crate::layout::bevel_width().min(band_h * 0.2)
}

/// The arrow slot less its wall — a picker's face.
pub const ARROW_SLOT: f32 = 24.0;

#[derive(Debug, Clone)]
pub struct Dropdown {
    pub options: Vec<String>,
    pub selected: usize,
    pub open: bool,
    pub(crate) hovered_item: Option<usize>,
    just_changed: bool,
    /// Host read-data, written ONLY by direct assignment (the Ramp-clamp unit test; no
    /// production writer). Read by the Ramp popover clamp and the fade-blend parent color,
    /// like the legacy `parent` pointer it replaces.
    pub parent_snapshot: Option<ParentSnapshot>,
    /// Optional popover anchor override: the menu hangs off THIS rect instead
    /// of the trigger's — for triggers embedded in a larger control (the
    /// parameter pane's textpick picker button nested in its TextBox), where
    /// the menu should span the whole field, not the button sliver.
    pub popover_anchor: Option<Rect>,
    /// The arrow stands in the MIDDLE of the trigger instead of in the arrow
    /// slot at its right end — for a trigger that carries nothing else
    /// (the textpick picker), whose padding about the arrow should be even.
    pub center_arrow: bool,
    pub font_family: String,
    pub custom_display_text: Option<String>,
    pub open_upward: Option<bool>,
    pub auto_width: bool,
    /// Synced copy of the control label ([`Paint::sync_label`]) — drives the side/detached
    /// offsets the base label geometry imposes on the widget's own geometry.
    label: Option<String>,
    /// Own hover flag, maintained from `MouseEnter`/`MouseLeave` (the adapter's hover
    /// bookkeeping hit-tests through [`Input::hit`], which includes the open popover — matching
    /// the legacy `on_cursor_moved` + popover-aware `hit_test` pair).
    hovered: bool,
    /// App-owned concentric frame (Phase 6s): `(rect, radius, corners)` of the rounded plate
    /// the dropdown sits in. When set, the corner adjustment uses it INSTEAD of walking for a
    /// root plate container ancestor — the hook that keeps the adjustment after an app dissolves its
    /// root plate container (the walk finds nothing once the widget is parentless).
    corner_frame: Option<((f32, f32, f32, f32), f32, (bool, bool, bool, bool))>,
    /// The raised trigger plate's corner radii, top-left clockwise, in place
    /// of the configured radius on all four — a trigger that is one end of
    /// a wider field (a parameter pane's completion picker, square where it
    /// meets the text box, the field's own radius on the outside).
    radii: Option<(f32, f32, f32, f32)>,
    /// Raised style: the closed control's background is an SDF-lit `Bevel`
    /// plate (fill + rolled lit edge) instead of a flat fill + border stroke.
    raised: Option<bool>,
    /// Flat stance ([`crate::widget::PlateStance::Flat`]): the trigger's face
    /// alone, no groove, silhouette equal to its rect. Overrides `raised`.
    /// The concentric corner adjustment below does not apply — a flat fill
    /// carries one radius, and the adjustment exists to nest relief outlines.
    flat: bool,
    /// Per-widget override for the trigger plate's FACE, bypassing
    /// [`crate::scene::Material::control_face`] on the configured fill. The default
    /// forces the face opaque; an app that wants its controls made of the
    /// same frosted material as its panes passes the blur-behind sentinel (a
    /// negative alpha) here, which that helper would strip.
    face: Option<crate::scene::Material>,
    /// Keyboard focus (FocusIn / FocusOut): lights the trigger plate's rim.
    focused: bool,
    /// The open menu REPLACES the trigger instead of growing out of it: no
    /// trigger band (display text + arrow) in the open surface, the rows alone,
    /// with the menu's edge anchored where the trigger's was (its bottom for
    /// an upward menu, its top for a downward one) so the rows occupy the
    /// trigger's slot. The trigger itself stops painting once the revealed
    /// menu covers it. The settings app's page switcher: the current page
    /// already reads blue in the list, so the band repeated it.
    menu_replaces_trigger: bool,
    /// Expand/contract animation (the status-interface module-menu feel).
    /// WALL-CLOCK, not dt-stepped: progress runs from `anim_from` at
    /// `anim_start` toward 1 (or 0 while `closing`) over [`Self::ANIM_S`], read
    /// at draw time — so an app that never ticks its UiContext can never strand
    /// the popover mid-size; ticks only drive redraws and settle a landed
    /// close. `closing` keeps `open` (and the shrinking popover) alive while
    /// everything interactive gates on `!closing`.
    anim_from: f32,
    anim_start: Option<web_time::Instant>,
    closing: bool,
    /// Per-frame SNAPSHOT of the wall-clock progress, refreshed in `tick`
    /// (before each render) and on every routed event. All geometry readers —
    /// popover_rect at registration, draw_popover, the engine's occlusion
    /// clamp — use this one value, so the animated rect is stable within a
    /// frame: the clamp's exact-match overlay-text exemption compares text
    /// bounds against popover_rect evaluated later in the same pass, and a
    /// live clock read there would never match.
    anim_snap: f32,
}

impl Dropdown {
    /// Expansion/contraction duration — the status-interface module-menu pace.
    const ANIM_S: f32 = 0.14;

    /// One option row's height — a fixed pitch, not the trigger's height.
    /// `popover_geom` sizes the box by it, `draw_popover` lays the labels out
    /// on it and `row_at` reads it back.
    const ROW_H: f32 = 24.0;

    /// The style in force: the per-widget override (`with_raised`) when set, else
    /// the DE's `control_relief`, read live so a runtime switch
    /// (`layout::set_control_relief`) restyles every control at once.
    fn raised(&self) -> bool {
        self.raised.unwrap_or_else(crate::layout::control_relief)
    }

    pub fn new(options: Vec<String>, selected: usize) -> Adapted<Dropdown> {
        Adapted::new(Dropdown {
            options,
            selected,
            open: false,
            hovered_item: None,
            just_changed: false,
            parent_snapshot: None,
            popover_anchor: None,
            center_arrow: false,
            font_family: "sans-serif".to_string(),
            custom_display_text: None,
            open_upward: None,
            auto_width: false,
            label: None,
            hovered: false,
            corner_frame: None,
            radii: None,
            raised: None,
            flat: false,
            face: None,
            focused: false,
            menu_replaces_trigger: false,
            anim_from: 0.0,
            anim_start: None,
            closing: false,
            anim_snap: 0.0,
        })
    }

    /// Whether the menu is open and taking input: open and not shrinking
    /// closed. `open` alone stays true through the closing animation, while
    /// the plate is still drawn but presses and keys are no longer its — a
    /// host that routes input to the dropdown ahead of what is under it asks
    /// this.
    pub fn is_expanded(&self) -> bool {
        self.open && !self.closing
    }

    /// Set (or clear) the app-owned concentric frame — see the `corner_frame` field docs.
    /// `Self::radii`: the raised trigger plate's corners, or `None` for
    /// the configured radius.
    pub fn set_radii(&mut self, radii: Option<(f32, f32, f32, f32)>) {
        self.radii = radii;
    }

    pub fn set_corner_frame(&mut self, frame: Option<((f32, f32, f32, f32), f32, (bool, bool, bool, bool))>) {
        self.corner_frame = frame;
    }

    pub fn take_change(&mut self) -> bool {
        let changed = self.just_changed;
        self.just_changed = false;
        changed
    }

    /// Horizontal inset added to a measured label to get a dropdown width the label fits inside
    /// without tripping `paint_text`'s right-edge fade: an 8px left pad plus the 28px right
    /// reservation (`right_limit = w - 28`, room for the 10px gap and the arrow) = 36px, plus a
    /// 2px cushion for the small gap between the ink-`measure_text_width` used here and the M-dummy
    /// advance the paint pass measures with. Shared by `content_width` (widest option) and
    /// `display_width` (collapsed display text) so the two can't drift.
    const LABEL_INSET: f32 = 38.0;

    /// The width the list keeps for MARKS: an option whose text begins with
    /// `context_menu::MARK_CHECK` ("✓ "), `MARK_ON` ("● ") or `MARK_OFF`
    /// ("○ ") is drawn with the check / circle / circle-outline glyph in a
    /// column at its left and its text after it — the context menu's
    /// convention, so a "View" menu-button can mark its switches as a menu
    /// does. When any option is marked every row keeps the column, so the
    /// labels line up; otherwise the list is as it was.
    fn mark_column(&self) -> f32 {
        let marked = self.options.iter().any(|o| crate::widget::context_menu::split_mark(o).0.is_some());
        if marked { MARK_SIDE + MARK_GAP } else { 0.0 }
    }

    /// The detached-label strip height above the content rect (zero unlabeled) —
    /// the adapter's `Widget::label_offset` over the synced label.
    fn label_top(&self) -> f32 {
        crate::widget::input::slider::detached_strip(&self.label)
    }
}

impl Adapted<Dropdown> {
    /// Raised style: see the `raised` field.
    pub fn with_raised(mut self, raised: bool) -> Self {
        self.raised = Some(raised);
        self
    }

    /// Override the trigger plate's face — see the `face` field.
    pub fn with_face(mut self, face: crate::scene::Material) -> Self {
        self.face = Some(face);
        self
    }

    /// Draw the trigger with no relief at all — see
    /// [`crate::widget::PlateStance::Flat`]. Overrides `with_raised`.
    pub fn with_flat(mut self, flat: bool) -> Self {
        self.flat = flat;
        self
    }

    pub fn with_custom_display_text(mut self, text: &str) -> Self {
        self.custom_display_text = Some(text.to_string());
        self
    }

    pub fn with_font_family(mut self, font_family: &str) -> Self {
        self.font_family = font_family.to_string();
        self
    }

    pub fn with_open_upward(mut self, open_upward: bool) -> Self {
        self.open_upward = Some(open_upward);
        self
    }

    pub fn with_auto_width(mut self, auto_width: bool) -> Self {
        self.auto_width = auto_width;
        self
    }

    /// The open menu replaces the trigger: see the `menu_replaces_trigger` field.
    pub fn with_menu_replaces_trigger(mut self, replaces: bool) -> Self {
        self.menu_replaces_trigger = replaces;
        self
    }

    /// Popover geometry from the widget's laid-out rect — the legacy inherent
    /// `get_popover_geom` shape, for callers that hold the wrapper.
    pub fn get_popover_geom(&self) -> (f32, f32, f32, f32) {
        let (x, y, w, h) = WidgetHost::rect(self);
        let top = self.inner().label_top();
        self.inner().popover_geom(Rect { x, y: y + top, width: w, height: h - top })
    }
}

impl Layout for Dropdown {

    fn z_order(&self) -> i32 {
        if self.open {
            100
        } else {
            0
        }
    }

    /// Content size for the scene layout engine (Phase 2b). A normal dropdown is wide enough for
    /// the widest option (via `content_width`, which already includes the arrow/padding inset), so
    /// the control doesn't resize as the selection changes. A menu-button dropdown (fixed
    /// `custom_display_text`, e.g. a "File" menu) instead sizes to its display text — its label is
    /// fixed regardless of options, so fitting the widest entry would just stretch the trigger. The
    /// open popover still expands to the widest option via `popover_width`'s `.max(content_width())`.
    fn intrinsic_size(&self) -> Option<Size> {
        let width = if self.custom_display_text.is_some() {
            self.display_width()
        } else {
            self.content_width()
        };
        Some(Size::new(width, crate::layout::dropdown_height()))
    }

    fn intrinsic_measure_width(&self) -> bool {
        self.auto_width
    }
}

unsafe impl Send for Dropdown {}

unsafe impl Sync for Dropdown {}
