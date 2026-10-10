//! What a node asks of the solver: its axis and mode, lengths, main and cross alignment, padding,
//! gap, grow and shrink — `Style`, with the spacing ladder's presets — and the `LayoutBox` the
//! solver writes its answer into.

use super::*;

/// The main-axis direction a flex container lays its children along.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Row,
    Column,
}

/// How a node arranges its children.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LayoutMode {
    /// Row/column flex (uses `Style::axis`).
    Flex,
    /// All children overlaid in the same box (Z-stack), aligned per axis.
    Stack,
    /// Fixed-column grid, filling left-to-right then top-to-bottom.
    Grid(GridSpec),
}

/// A fixed-column grid: uniform column width (max child width), per-row heights.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridSpec {
    pub columns: usize,
    pub col_gap: f32,
    pub row_gap: f32,
}

/// A length along one axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    /// Size to content (children/intrinsic), plus padding.
    Auto,
    /// Fixed logical pixels, overriding content size.
    Fixed(f32),
}

/// Distribution of free space along the main axis (when no child grows/shrinks). For [`Stack`]
/// this selects horizontal placement.
///
/// [`Stack`]: LayoutMode::Stack
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainAlign {
    Start,
    Center,
    End,
    SpaceBetween,
}

/// Placement of each child across the cross axis. For [`Stack`] this selects vertical placement.
///
/// [`Stack`]: LayoutMode::Stack
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossAlign {
    Start,
    Center,
    End,
    /// Fill the container's cross-axis content extent.
    Stretch,
}

/// Layout inputs for a node.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    pub mode: LayoutMode,
    pub axis: Axis,
    pub padding: Edges,
    pub gap: f32,
    pub main_align: MainAlign,
    pub cross_align: CrossAlign,
    pub width: Length,
    pub height: Length,
    /// Flex grow weight: share of leftover main-axis space this node claims.
    pub grow: f32,
    /// Flex shrink weight: share of a main-axis overflow this node gives up.
    pub shrink: f32,
    pub min_width: f32,
    pub min_height: f32,
    pub max_width: f32,
    pub max_height: f32,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            mode: LayoutMode::Flex,
            axis: Axis::Column,
            padding: Edges::ZERO,
            gap: 0.0,
            main_align: MainAlign::Start,
            cross_align: CrossAlign::Start,
            width: Length::Auto,
            height: Length::Auto,
            grow: 0.0,
            shrink: 0.0,
            min_width: 0.0,
            min_height: 0.0,
            max_width: f32::INFINITY,
            max_height: f32::INFINITY,
        }
    }
}

impl Style {
    pub fn row() -> Self {
        Style { mode: LayoutMode::Flex, axis: Axis::Row, ..Default::default() }
    }
    pub fn column() -> Self {
        Style { mode: LayoutMode::Flex, axis: Axis::Column, ..Default::default() }
    }
    pub fn stack() -> Self {
        Style { mode: LayoutMode::Stack, ..Default::default() }
    }
    pub fn grid(columns: usize, col_gap: f32, row_gap: f32) -> Self {
        Style { mode: LayoutMode::Grid(GridSpec { columns: columns.max(1), col_gap, row_gap }), ..Default::default() }
    }
    // ── The spacing ladder as presets ─────────────────────────────────
    // An app on the standard root plate never names a padding or gap: it
    // picks the rung. Root presets inset by `root_plate_inset` (the plate's
    // roll plus one padding) and space siblings by `root_plate_gap`; pane
    // presets by `plate_padding` / `plate_gap`; the controls presets space
    // a form's controls by `control_gap` with no inset of their own, since
    // they sit inside a pane or root preset that already has one.

    /// A column of siblings standing on the root plate, stretched across it.
    pub fn root_column() -> Self {
        Self::column()
            .padding(crate::layout::root_plate_inset())
            .gap(crate::layout::root_plate_gap())
            .cross_align(CrossAlign::Stretch)
    }
    /// A row of siblings standing on the root plate.
    pub fn root_row() -> Self {
        Self::row()
            .padding(crate::layout::root_plate_inset())
            .gap(crate::layout::root_plate_gap())
            .cross_align(CrossAlign::Stretch)
    }
    /// A column inside a pane plate, inset from its rim.
    pub fn pane_column() -> Self {
        Self::column()
            .padding(crate::layout::plate_padding())
            .gap(crate::layout::plate_gap())
            .cross_align(CrossAlign::Stretch)
    }
    /// A row inside a pane plate.
    pub fn pane_row() -> Self {
        Self::row()
            .padding(crate::layout::plate_padding())
            .gap(crate::layout::plate_gap())
            .cross_align(CrossAlign::Stretch)
    }
    /// A column of controls: the control gap between them, no inset.
    pub fn controls_column() -> Self {
        Self::column().gap(crate::layout::control_gap())
    }
    /// A row of controls: the control gap between them, no inset.
    pub fn controls_row() -> Self {
        Self::row().gap(crate::layout::control_gap())
    }

    pub fn gap(mut self, v: f32) -> Self {
        self.gap = v;
        self
    }
    pub fn padding(mut self, v: f32) -> Self {
        self.padding = Edges::all(v);
        self
    }
    pub fn grow(mut self, v: f32) -> Self {
        self.grow = v;
        self
    }
    pub fn shrink(mut self, v: f32) -> Self {
        self.shrink = v;
        self
    }
    pub fn main_align(mut self, a: MainAlign) -> Self {
        self.main_align = a;
        self
    }
    pub fn cross_align(mut self, a: CrossAlign) -> Self {
        self.cross_align = a;
        self
    }
    pub fn width(mut self, w: Length) -> Self {
        self.width = w;
        self
    }
    pub fn height(mut self, h: Length) -> Self {
        self.height = h;
        self
    }
}

/// A node's layout state: inputs (`style`, optional `intrinsic` content size for leaves) and the
/// two computed outputs (`measured`, then `rect`).
#[derive(Debug, Clone, Copy)]
pub struct LayoutBox {
    pub style: Style,
    /// Content size for a leaf (e.g. measured text). Ignored when the node has children.
    pub intrinsic: Option<Size>,
    pub measured: Size,
    pub rect: Rect,
}

impl LayoutBox {
    /// A container node laid out from its children.
    pub fn container(style: Style) -> Self {
        LayoutBox { style, intrinsic: None, measured: Size::ZERO, rect: Rect::ZERO }
    }

    /// A leaf node with a fixed content size.
    pub fn leaf(style: Style, content: Size) -> Self {
        LayoutBox { style, intrinsic: Some(content), measured: Size::ZERO, rect: Rect::ZERO }
    }
}
