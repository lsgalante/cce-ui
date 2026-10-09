//! The layout concern: a widget's intrinsic size, where its label goes, adjustments to the rect it
//! is assigned, its children's arrangement and z order, and the children it embeds.

use super::*;

/// Layout inputs for the scene layout engine — the RFC's `Widget` concern, named `Layout` here to
/// avoid the existing [`Widget`] base struct.
pub trait Layout {
    /// Intrinsic content size of a leaf (e.g. measured text), consumed by the adapter's
    /// `measure` (gated on [`Layout::intrinsic_measure_width`]).
    fn intrinsic_size(&self) -> Option<Size> {
        None
    }

    /// Whether this widget's base label IS its content — the text a `Button` face, a
    /// `Checkbox` row or a `Label` draws itself — rather than a control label, which
    /// the adapter draws detached above the content (the one convention for every
    /// labeled control: `layout::control_label_strip` tall, at
    /// [`detached_label_inset`](Layout::detached_label_inset)). Inline-label widgets
    /// carry no label strip and get no content-rect inset.
    fn inline_label(&self) -> bool {
        false
    }

    /// Horizontal inset of the detached base label: [`crate::layout::DETACHED_LABEL_INSET`]
    /// for every control, so a column of labels is one line and the carve-out tabs
    /// (which hug the label at this inset) sit under their labels.
    fn detached_label_inset(&self) -> f32 {
        crate::layout::DETACHED_LABEL_INSET
    }

    /// Whether `WidgetHost::measure` should prefer [`intrinsic_size`](Layout::intrinsic_size)'s
    /// width over the current rect width (Dropdown's `auto_width` measure override — hosts size
    /// it from `measure`, e.g. cce-system-interface' page dropdown). Default: keep the legacy
    /// `WidgetHost::measure` width (the current rect's).
    fn intrinsic_measure_width(&self) -> bool {
        false
    }

    /// Whether the adapter's hit test substitutes the base row rect (`row_x`/`row_w`, pushed in
    /// by row-layout hosts via `set_row_rect`) — the legacy
    /// `WidgetHost::hit_test` default geometry. Migrated controls so far dropped it (accepted
    /// drift); TextBox restores it (cce-files' save-name box relies on row hits). Default: off,
    /// keeping the other migrated widgets exactly as they shipped.
    fn hit_row_rect(&self) -> bool {
        false
    }

    /// Adjust a row-rect assignment before it lands on the base (`WidgetHost::set_row_rect` —
    /// TextBox clamps the row width to its `width`/`max_width`). Default: identity.
    fn adjust_row_rect(&self, x: f32, w: f32) -> (f32, f32) {
        (x, w)
    }

    /// The final base rect landed from a `set_rect`, visible or not — unlike
    /// [`arrange_children`](Layout::arrange_children), which the adapter gates on visibility.
    /// TextBox caches it (its cursor/scroll math reads the laid-out rect between events) and
    /// re-clamps its scroll, the legacy `set_rect` side effect. Default: ignore.
    fn rect_assigned(&mut self, _rect: Rect) {}

    /// Adjust a rect assignment before it lands on the base (Switcher clamps to its parent).
    /// Default: identity.
    fn adjust_rect(&self, requested: Rect) -> Rect {
        requested
    }

    /// Position children after a `set_rect`. Called only while the widget is visible. There
    /// is no context here: a child the context holds (`widget::Embedded`) is placed in
    /// [`register_embedded_children`](Layout::register_embedded_children) from the rect
    /// kept here.
    fn arrange_children(&mut self, _rect: Rect) {}

    /// Legacy `WidgetHost::z_index` (host render ordering; MenuBar's dropdowns layer at 100+).
    fn z_order(&self) -> i32 {
        0
    }

    /// Republish value-embedded legacy children into the ctx registry (Paginator's ButtonStrip
    /// and Pages). Legacy value-owning containers re-registered their children on EVERY `tick` and
    /// `layout` because the children's addresses move with the owning struct (host struct moves,
    /// `Vec` reallocation) — and the registration is load-bearing: the spatial grid is rebuilt
    /// from registered widgets, and it is the registered ButtonStrip (whose
    /// `blocks_root_plate_drag` is true) that makes the sidebar block root plate drags. The
    /// adapter calls this from `WidgetHost::tick` and `WidgetHost::layout`, mirroring the legacy
    /// cadence. `host_id` is the adapter's id, for `link_ids`. Default: nothing embedded.
    fn register_embedded_children(&mut self, _host_id: WidgetId, _ctx: &mut UiContext) {}

    /// Give back the children `register_embedded_children` put into `ctx`
    /// (`Embedded::detach`), so a widget removed from its context leaves with them. Called by
    /// `UiContext::remove`. Default: nothing embedded.
    fn release_embedded_children(&mut self, _ctx: &mut UiContext) {}
}
