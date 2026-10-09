//! The spacing ladder — root plate, pane plate, controls, list rows — and the label margins
//! (`CLAUDE.md`, "The standard app").

use super::*;

pub fn control_label_margin() -> f32 {
    registry_float("control_label_margin").unwrap_or(6.0)
}

pub(crate) fn control_label_strip() -> f32 {
    let (_, font_size) = control_label_font_detached_parsed();
    font_size + control_label_margin()
}

pub fn label_margin() -> f32 {
    control_label_margin()
}

pub fn set_control_label_margin(margin: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("control_label_margin", margin);
    }
}

pub fn set_label_margin(margin: f32) {
    set_control_label_margin(margin);
}

pub fn nested_section_label_alignment() -> u8 {
    registry_float("nested_section_label_alignment").map(|v| v as u8).unwrap_or(0)
}

pub fn set_nested_section_label_alignment(align: u8) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("nested_section_label_alignment", align as f32);
    }
}

pub fn nested_section_label_offset() -> f32 {
    registry_float("nested_section_label_offset").unwrap_or(0.0)
}

pub fn set_nested_section_label_offset(offset: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("nested_section_label_offset", offset);
    }
}

/// The pane rung's padding: from a pane plate's rim to its content, in
/// logical px (`style.surface.plate.padding`). The second rung of the
/// spacing ladder — [`root_plate_inset`] / [`root_plate_gap`] on the root
/// plate, this and [`plate_gap`] inside a pane plate, [`control_gap`]
/// between controls. Registry-backed (live-reloadable); the legacy flat
/// `plate_padding = N` line still loads as a fallback.
pub fn plate_padding() -> f32 {
    registry_float("plate_padding").unwrap_or(20.0)
}

pub fn set_plate_padding(padding: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("plate_padding", padding);
    }
}

/// Legacy: the page-level margin (`style.surface.page.margin`). Unset, it
/// IS the pane rung's [`plate_padding`] — a page is a pane — so an app
/// still reading it lands on the ladder. Set, it is honoured as before.
pub fn page_margin() -> f32 {
    registry_float("page_margin").unwrap_or_else(plate_padding)
}

pub fn set_page_margin(margin: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("page_margin", margin);
    }
}

pub fn grid_min_col_width() -> f32 {
    registry_float("grid_min_col_width").unwrap_or(260.0)
}

pub fn set_grid_min_col_width(width: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("grid_min_col_width", width);
    }
}

pub fn grid_gap() -> f32 {
    registry_float("grid_gap").unwrap_or(8.0)
}

pub fn set_grid_gap(gap: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("grid_gap", gap);
    }
}

/// Legacy: the inter-column gap (`style.layout.column.gap`). Unset, it is
/// the root plate's [`root_plate_gap`] — columns are siblings on the plate.
pub fn column_gap() -> f32 {
    registry_float("column_gap").unwrap_or_else(root_plate_gap)
}

pub fn set_column_gap(gap: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("column_gap", gap);
    }
}

/// Legacy: a control panel's padding (`style.control.control_panel.padding`).
/// Unset, it is the pane rung's [`plate_padding`] — a control panel is a pane.
pub fn control_panel_padding() -> f32 {
    registry_float("control_panel_padding").unwrap_or_else(plate_padding)
}

pub fn set_control_panel_padding(padding: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("control_panel_padding", padding);
    }
}

/// Legacy: a control panel's gap (`style.control.control_panel.gap`).
/// Unset, it is the pane rung's [`plate_gap`].
pub fn control_panel_gap() -> f32 {
    registry_float("control_panel_gap").unwrap_or_else(plate_gap)
}

pub fn set_control_panel_gap(gap: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("control_panel_gap", gap);
    }
}

/// The section carves' depth multiplier (`style.container.section.depth`,
/// default 1.0): scales the params pane's section-well wall — width and step
/// together — relative to the DE-wide relief material, so sections can read
/// deeper or shallower than the controls around them. Values past 1.0 let the
/// roll widen across the channel groove between the well wall and its packed
/// controls; tune to taste.
pub fn section_depth() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("section_depth").unwrap_or(1.0)
}

/// The parameter rows' backdrop compression
/// (`style.surface.param.backdrop_compression`, 0..1): when set, every
/// parameter row in a params pane is floored with the pane material frosted
/// at THIS compression before its controls paint, so each parameter sits on
/// a tablet that pulls the view toward the tint while the pane around it
/// stays at its own — the row-scale twin of the designer's node
/// compression. `None` (unset) draws no floor — the rows are bare, as they
/// always were.
pub fn param_compression() -> Option<f32> {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("param_compression").map(|v| v.clamp(0.0, 1.0))
}

/// Whether a params pane lays each row's label BESIDE its control
/// (`style.surface.param.label_layout = "inline"`, the default) or lets the
/// control carry it in the strip above itself (`"stacked"`, the layout every
/// row had before 2026-09-21). Inline, the pane owns the labels: it measures
/// a label column off the widest label, hands each control the rest of the
/// row, and the controls are built unlabelled — an unlabelled control takes
/// its whole rect (`WidgetHost::label_strip` is zero), so the row is one
/// control tall. Toggles and buttons carry their label as their own face and
/// are inline either way.
pub fn param_labels_inline() -> bool {
    lazy_init_style_registry();
    get_style_registry()
        .read()
        .unwrap()
        .get_string("param_label_layout")
        .is_none_or(|v| v.trim() != "stacked")
}

pub fn section_padding() -> f32 {
    registry_float("section_padding").unwrap_or(8.0)
}

pub fn set_section_padding(padding: f32) {
    if let Ok(mut r) = get_style_registry().write() {
        r.set_float("section_padding", padding);
    }
}

/// Padding between the window plate's edge and the objects sitting on it, in
/// logical px (`style.surface.plate.root.padding` in config.kdl). DE-wide so
/// every app's content sits the same distance off the plate rim.
pub fn root_plate_padding() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("root_plate_padding").unwrap_or(16.0)
}

/// Gap between sibling objects on the window plate, in logical px
/// (`style.surface.plate.root.gap`) — pane splits, control rows. The companion to [`root_plate_padding`]:
/// rim distance vs object spacing.
pub fn root_plate_gap() -> f32 {
    lazy_init_style_registry();
    get_style_registry().read().unwrap().get_float("root_plate_gap").unwrap_or(12.0)
}

/// Where content starts on the standard root plate, measured from the
/// WINDOW edge: the plate's rolled rim ([`bevel_width`]) plus one
/// [`root_plate_padding`]. The padding is a run of flat plate face, the
/// same run [`root_plate_gap`] leaves between two siblings; but the face
/// only begins where the roll ends, so a bare padding at a window edge
/// leaves most of it on the roll — measured at 4px of visible flat against
/// 12 between panes (cce-mail, 2026-09-19). This is the one number an app
/// on the standard plate insets by at its four edges; between siblings it
/// uses the gap, and everything inside a pane plate uses
/// [`plate_padding`]. An app whose base is NOT the rolled root plate (a
/// transparent surface, a bare fill) has no roll to clear and insets by
/// [`root_plate_padding`] alone.
pub fn root_plate_inset() -> f32 {
    bevel_width() + root_plate_padding()
}

/// Gap between siblings INSIDE a pane plate, in logical px
/// (`style.surface.plate.gap`) — the pane rung's twin of
/// [`root_plate_gap`]. Unset, it is the root gap: one number reads as one
/// rhythm across both rungs unless a config says otherwise.
pub fn plate_gap() -> f32 {
    registry_float("plate_gap").unwrap_or_else(root_plate_gap)
}

/// Gap between controls, in logical px (`style.control.gap`) — the control
/// rung of the ladder: what the layout strategies put between a form's
/// controls (and between a detached label's block and the next), in both
/// axes. Unset, it is [`CONTROL_GAP`], one control height, the value every
/// strategy's `Default` carried as a literal.
pub fn control_gap() -> f32 {
    registry_float("control_gap").unwrap_or(CONTROL_GAP)
}

/// Gap inside a list row, in logical px (`style.control.list_gap`) — the rung below the
/// controls: what a scrolling list's row puts between its cells (a row's buttons, its glyph
/// and its name) and between its content and the list's wall. A row is one control's height,
/// so the control gap (one control height) would part it into islands. Unset, it is
/// [`CONTROL_TEXT_INSET`], the inset text keeps from a control's wall.
pub fn list_gap() -> f32 {
    registry_float("list_gap").unwrap_or(CONTROL_TEXT_INSET)
}

/// Roll-off width for the wall where a bar (menubar / status bar / the demo's
/// header band) steps down into the window plate. Wider than the plate's own
/// perimeter roll on purpose: the carve depth saturates at `bevel_width` in the
/// tessellator, so the extra width flattens the wall's slope — a soft, gradual
/// transition into the bar — instead of cutting a proportionally deeper groove.
pub fn bar_wall_width() -> f32 {
    bevel_width() * 1.75
}
