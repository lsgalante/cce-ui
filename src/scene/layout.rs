//! Hand-rolled two-phase layout engine — Phase 2 of the core rebuild.
//!
//! The legacy toolkit computes layout *inline during paint*, smeared across `LayoutStrategy`, a
//! child-driven `allocate` bump-cursor, and hand-written `set_rect` calls with absolute
//! coordinates — with `SectionContext` literally rendering twice to measure. There is no layout
//! pass that is independent of paint, which is why animated/relayout-able UI is hard and why a
//! widget can end up sized by two different owners (the breadcrumb bug).
//!
//! This module replaces that with a real, self-contained solver that runs over the [`Arena`] and
//! is independent of paint:
//!
//!   * **measure** (bottom-up): each node reports an intrinsic [`Size`] from its children (or, for
//!     a leaf, its content size). Written into `LayoutBox::measured`.
//!   * **arrange** (top-down): each node is given a final [`Rect`] and positions its children
//!     within it. Written into `LayoutBox::rect`.
//!
//! Because it operates on `Style` + `Size` and writes plain rects, it is fully unit-testable
//! without a GPU or a Wayland surface (see the tests below).
//!
//! Layout modes ([`LayoutMode`]): **Flex** (row/column with grow/shrink, gap, padding, main/cross
//! alignment incl. stretch), **Stack** (Z-overlay with per-axis alignment), and **Grid** (fixed
//! column count with uniform column width and per-row heights). Deferred to later increments:
//! wrapping, percentage lengths, width-dependent adaptive grids, and the cosmic-text text-measure
//! hook for real leaf widgets (that lands with Phase 2b integration).

use crate::scene::arena::{Arena, NodeId};

/// A width/height pair in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Size = Size { width: 0.0, height: 0.0 };
    pub fn new(width: f32, height: f32) -> Self {
        Size { width, height }
    }
}

/// A positioned box in logical pixels (absolute coordinates after arrange).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const ZERO: Rect = Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 };
}

/// How an image maps into a bounding box — see [`fit_rect`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FitMode {
    /// Aspect-preserving: the image fills the box on its long axis and
    /// letterboxes on the other, never scaling up past `max_upscale`
    /// (1.0 = never enlarge; f32::INFINITY = always fill).
    Contain { max_upscale: f32 },
    /// The full box, aspect ignored.
    Stretch,
}

/// The rect an `img_w` × `img_h` image occupies inside `bounds` under `mode`,
/// centered on both axes. Zero-sized images yield a zero rect at the box
/// center rather than a division blow-up.
pub fn fit_rect(img_w: u32, img_h: u32, bounds: Rect, mode: FitMode) -> Rect {
    match mode {
        FitMode::Stretch => bounds,
        FitMode::Contain { max_upscale } => {
            if img_w == 0 || img_h == 0 {
                return Rect {
                    x: bounds.x + bounds.width * 0.5,
                    y: bounds.y + bounds.height * 0.5,
                    width: 0.0,
                    height: 0.0,
                };
            }
            let (iw, ih) = (img_w as f32, img_h as f32);
            let scale = (bounds.width / iw).min(bounds.height / ih).min(max_upscale).max(0.0);
            let (w, h) = (iw * scale, ih * scale);
            Rect {
                x: bounds.x + (bounds.width - w) * 0.5,
                y: bounds.y + (bounds.height - h) * 0.5,
                width: w,
                height: h,
            }
        }
    }
}

/// Per-side spacing (padding).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edges {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl Edges {
    pub const ZERO: Edges = Edges { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 };
    pub fn all(v: f32) -> Self {
        Edges { left: v, right: v, top: v, bottom: v }
    }
}

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

// --- axis helpers: project/unproject a Size onto the (main, cross) frame of an axis ---

#[inline]
fn main_of(axis: Axis, s: Size) -> f32 {
    match axis {
        Axis::Row => s.width,
        Axis::Column => s.height,
    }
}

#[inline]
fn cross_of(axis: Axis, s: Size) -> f32 {
    match axis {
        Axis::Row => s.height,
        Axis::Column => s.width,
    }
}

#[inline]
fn make_size(axis: Axis, main: f32, cross: f32) -> Size {
    match axis {
        Axis::Row => Size::new(main, cross),
        Axis::Column => Size::new(cross, main),
    }
}

/// Offset that places an item of extent `item` within `container` per a start/center/end rule.
#[inline]
fn align_offset(start: bool, center: bool, container: f32, item: f32) -> f32 {
    if center {
        (container - item) / 2.0
    } else if start {
        0.0
    } else {
        container - item // end
    }
}

/// Build a child rect from axis-relative main/cross offsets and extents (offsets are relative to
/// the parent's content origin `content_x`/`content_y`).
#[inline]
fn child_rect(
    axis: Axis,
    content_x: f32,
    content_y: f32,
    main_pos: f32,
    cross_pos: f32,
    main_size: f32,
    cross_size: f32,
) -> Rect {
    match axis {
        Axis::Row => Rect {
            x: content_x + main_pos,
            y: content_y + cross_pos,
            width: main_size,
            height: cross_size,
        },
        Axis::Column => Rect {
            x: content_x + cross_pos,
            y: content_y + main_pos,
            width: cross_size,
            height: main_size,
        },
    }
}

#[inline]
fn content_box(rect: Rect, p: Edges) -> (f32, f32, f32, f32) {
    (
        rect.x + p.left,
        rect.y + p.top,
        (rect.width - p.left - p.right).max(0.0),
        (rect.height - p.top - p.bottom).max(0.0),
    )
}

/// Resolve the node's own size from its content box: apply explicit width/height, add padding for
/// `Auto`, then clamp to min/max.
fn finalize_size(style: &Style, content: Size) -> Size {
    let padded = Size::new(
        content.width + style.padding.left + style.padding.right,
        content.height + style.padding.top + style.padding.bottom,
    );
    let mut size = Size {
        width: match style.width {
            Length::Fixed(v) => v,
            Length::Auto => padded.width,
        },
        height: match style.height {
            Length::Fixed(v) => v,
            Length::Auto => padded.height,
        },
    };
    size.width = size.width.clamp(style.min_width, style.max_width);
    size.height = size.height.clamp(style.min_height, style.max_height);
    size
}

/// Run both passes over the subtree rooted at `root`, laying it out into `available` space at the
/// origin. Writes `measured` and `rect` into every node.
pub fn compute_layout(arena: &mut Arena<LayoutBox>, root: NodeId, available: Size) {
    measure(arena, root);
    let root_rect = Rect { x: 0.0, y: 0.0, width: available.width, height: available.height };
    arrange(arena, root, root_rect);
}

/// Bottom-up intrinsic sizing. Returns and records the node's `measured` size.
pub fn measure(arena: &mut Arena<LayoutBox>, id: NodeId) -> Size {
    let (style, intrinsic) = {
        let b = arena.value(id).expect("measure: stale node");
        (b.style, b.intrinsic)
    };
    let children = arena.children(id).to_vec();

    let content = if children.is_empty() {
        intrinsic.unwrap_or(Size::ZERO)
    } else {
        match style.mode {
            LayoutMode::Flex => measure_flex(arena, &style, &children),
            LayoutMode::Stack => measure_stack(arena, &children),
            LayoutMode::Grid(spec) => measure_grid(arena, spec, &children),
        }
    };

    let size = finalize_size(&style, content);
    arena.value_mut(id).expect("measure: stale node").measured = size;
    size
}

fn measure_flex(arena: &mut Arena<LayoutBox>, style: &Style, children: &[NodeId]) -> Size {
    let mut main = 0.0f32;
    let mut cross = 0.0f32;
    for (i, &child) in children.iter().enumerate() {
        let cs = measure(arena, child);
        if i > 0 {
            main += style.gap;
        }
        main += main_of(style.axis, cs);
        cross = cross.max(cross_of(style.axis, cs));
    }
    make_size(style.axis, main, cross)
}

fn measure_stack(arena: &mut Arena<LayoutBox>, children: &[NodeId]) -> Size {
    let mut w = 0.0f32;
    let mut h = 0.0f32;
    for &child in children {
        let cs = measure(arena, child);
        w = w.max(cs.width);
        h = h.max(cs.height);
    }
    Size::new(w, h)
}

fn measure_grid(arena: &mut Arena<LayoutBox>, spec: GridSpec, children: &[NodeId]) -> Size {
    let cols = spec.columns.max(1);
    let sizes: Vec<Size> = children.iter().map(|&c| measure(arena, c)).collect();
    let cell_w = sizes.iter().fold(0.0f32, |m, s| m.max(s.width));
    let rows = sizes.len().div_ceil(cols);
    let mut row_heights = vec![0.0f32; rows];
    for (i, s) in sizes.iter().enumerate() {
        let r = i / cols;
        row_heights[r] = row_heights[r].max(s.height);
    }
    let content_w = cols as f32 * cell_w + (cols as f32 - 1.0) * spec.col_gap;
    let content_h =
        row_heights.iter().sum::<f32>() + (rows as f32 - 1.0).max(0.0) * spec.row_gap;
    Size::new(content_w, content_h)
}

/// Top-down placement. Assigns `rect` to `id`, then positions its children within it.
pub fn arrange(arena: &mut Arena<LayoutBox>, id: NodeId, rect: Rect) {
    arena.value_mut(id).expect("arrange: stale node").rect = rect;

    let style = arena.value(id).unwrap().style;
    let children = arena.children(id).to_vec();
    if children.is_empty() {
        return;
    }
    let (cx, cy, cw, ch) = content_box(rect, style.padding);

    let placements = match style.mode {
        LayoutMode::Flex => arrange_flex(arena, &style, &children, cx, cy, cw, ch),
        LayoutMode::Stack => arrange_stack(arena, &style, &children, cx, cy, cw, ch),
        LayoutMode::Grid(spec) => arrange_grid(arena, spec, &children, cx, cy),
    };

    for (child, r) in placements {
        arrange(arena, child, r);
    }
}

fn arrange_flex(
    arena: &Arena<LayoutBox>,
    style: &Style,
    children: &[NodeId],
    cx: f32,
    cy: f32,
    cw: f32,
    ch: f32,
) -> Vec<(NodeId, Rect)> {
    let axis = style.axis;
    let content = Size::new(cw, ch);
    let content_main = main_of(axis, content);
    let content_cross = cross_of(axis, content);

    let mut child_main = Vec::with_capacity(children.len());
    let mut child_cross = Vec::with_capacity(children.len());
    let mut grows = Vec::with_capacity(children.len());
    let mut shrinks = Vec::with_capacity(children.len());
    for &child in children {
        let b = arena.value(child).unwrap();
        child_main.push(main_of(axis, b.measured));
        child_cross.push(cross_of(axis, b.measured));
        grows.push(b.style.grow);
        shrinks.push(b.style.shrink);
    }

    let n = children.len();
    let total_main: f32 = child_main.iter().sum::<f32>() + style.gap * (n as f32 - 1.0);
    let free = content_main - total_main;
    let total_grow: f32 = grows.iter().sum();
    let total_shrink: f32 = shrinks.iter().sum();

    // Resolve each child's main extent: grow to fill, or shrink to fit, else keep measured.
    let mut sizes = child_main.clone();
    let distributed = if free > 0.0 && total_grow > 0.0 {
        for i in 0..n {
            sizes[i] += grows[i] / total_grow * free;
        }
        true
    } else if free < 0.0 && total_shrink > 0.0 {
        let deficit = -free;
        for i in 0..n {
            sizes[i] = (child_main[i] - shrinks[i] / total_shrink * deficit).max(0.0);
        }
        true
    } else {
        false
    };

    // Alignment only distributes leftover space when grow/shrink didn't consume it.
    let (start_offset, spacing_extra) = if distributed {
        (0.0, 0.0)
    } else {
        match style.main_align {
            MainAlign::Start => (0.0, 0.0),
            MainAlign::Center => (free.max(0.0) / 2.0, 0.0),
            MainAlign::End => (free.max(0.0), 0.0),
            MainAlign::SpaceBetween => {
                (0.0, if n > 1 { free.max(0.0) / (n as f32 - 1.0) } else { 0.0 })
            }
        }
    };

    let mut out = Vec::with_capacity(n);
    let mut main_pos = start_offset;
    for i in 0..n {
        let main_size = sizes[i];
        let cross_size = match style.cross_align {
            CrossAlign::Stretch => content_cross,
            _ => child_cross[i],
        };
        let cross_pos = match style.cross_align {
            CrossAlign::Start | CrossAlign::Stretch => 0.0,
            CrossAlign::Center => (content_cross - cross_size) / 2.0,
            CrossAlign::End => content_cross - cross_size,
        };
        out.push((children[i], child_rect(axis, cx, cy, main_pos, cross_pos, main_size, cross_size)));
        main_pos += main_size + style.gap + spacing_extra;
    }
    out
}

fn arrange_stack(
    arena: &Arena<LayoutBox>,
    style: &Style,
    children: &[NodeId],
    cx: f32,
    cy: f32,
    cw: f32,
    ch: f32,
) -> Vec<(NodeId, Rect)> {
    // Stack has no axis: `main_align` places children horizontally, `cross_align` vertically.
    let (h_start, h_center) =
        (style.main_align == MainAlign::Start || style.main_align == MainAlign::SpaceBetween,
         style.main_align == MainAlign::Center);
    let (v_start, v_center) =
        (style.cross_align == CrossAlign::Start, style.cross_align == CrossAlign::Center);
    let stretch_v = style.cross_align == CrossAlign::Stretch;

    let mut out = Vec::with_capacity(children.len());
    for &child in children {
        let m = arena.value(child).unwrap().measured;
        let w = m.width;
        let h = if stretch_v { ch } else { m.height };
        let x = cx + align_offset(h_start, h_center, cw, w);
        let y = cy + if stretch_v { 0.0 } else { align_offset(v_start, v_center, ch, h) };
        out.push((child, Rect { x, y, width: w, height: h }));
    }
    out
}

fn arrange_grid(
    arena: &Arena<LayoutBox>,
    spec: GridSpec,
    children: &[NodeId],
    cx: f32,
    cy: f32,
) -> Vec<(NodeId, Rect)> {
    let cols = spec.columns.max(1);
    let sizes: Vec<Size> = children.iter().map(|&c| arena.value(c).unwrap().measured).collect();
    let cell_w = sizes.iter().fold(0.0f32, |m, s| m.max(s.width));
    let rows = sizes.len().div_ceil(cols);

    let mut row_heights = vec![0.0f32; rows];
    for (i, s) in sizes.iter().enumerate() {
        row_heights[i / cols] = row_heights[i / cols].max(s.height);
    }
    // y offset of each row's top.
    let mut row_y = vec![0.0f32; rows];
    let mut acc = 0.0;
    for r in 0..rows {
        row_y[r] = acc;
        acc += row_heights[r] + spec.row_gap;
    }

    let mut out = Vec::with_capacity(children.len());
    for (i, &child) in children.iter().enumerate() {
        let col = i % cols;
        let row = i / cols;
        let x = cx + col as f32 * (cell_w + spec.col_gap);
        let y = cy + row_y[row];
        out.push((child, Rect { x, y, width: sizes[i].width, height: sizes[i].height }));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_contain_letterboxes_and_centers() {
        let b = Rect { x: 10.0, y: 20.0, width: 100.0, height: 50.0 };
        // 200x100 source, scale limited by both axes equally -> 100x50 fill
        let r = fit_rect(200, 100, b, FitMode::Contain { max_upscale: 4.0 });
        assert_eq!((r.x, r.y, r.width, r.height), (10.0, 20.0, 100.0, 50.0));
        // tall source letterboxes horizontally: scale = 50/200 -> 25x50
        let r = fit_rect(100, 200, b, FitMode::Contain { max_upscale: 4.0 });
        assert_eq!((r.width, r.height), (25.0, 50.0));
        assert_eq!(r.x, 10.0 + (100.0 - 25.0) * 0.5);
        assert_eq!(r.y, 20.0);
    }

    #[test]
    fn fit_contain_caps_upscale_but_downscales_freely() {
        let b = Rect { x: 0.0, y: 0.0, width: 400.0, height: 400.0 };
        // small source: would need 8x, capped at 4x, centered
        let r = fit_rect(50, 50, b, FitMode::Contain { max_upscale: 4.0 });
        assert_eq!((r.width, r.height), (200.0, 200.0));
        assert_eq!((r.x, r.y), (100.0, 100.0));
        // large source downscales with no floor (the old .max(1.0) bug)
        let r = fit_rect(800, 800, b, FitMode::Contain { max_upscale: 4.0 });
        assert_eq!((r.width, r.height), (400.0, 400.0));
    }

    #[test]
    fn fit_degenerate_inputs() {
        let b = Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 };
        let r = fit_rect(0, 50, b, FitMode::Contain { max_upscale: 4.0 });
        assert_eq!((r.width, r.height), (0.0, 0.0));
        let r = fit_rect(10, 10, b, FitMode::Stretch);
        assert_eq!((r.width, r.height), (100.0, 100.0));
    }

    fn leaf(arena: &mut Arena<LayoutBox>, w: f32, h: f32) -> NodeId {
        arena.insert(LayoutBox::leaf(Style::default(), Size::new(w, h)))
    }

    fn rect_of(arena: &Arena<LayoutBox>, id: NodeId) -> Rect {
        arena.value(id).unwrap().rect
    }

    #[test]
    fn row_places_children_left_to_right_with_gap() {
        let mut arena = Arena::new();
        let root = arena.insert(LayoutBox::container(Style::row().gap(5.0)));
        let a = leaf(&mut arena, 10.0, 10.0);
        let b = leaf(&mut arena, 20.0, 10.0);
        arena.append_child(root, a);
        arena.append_child(root, b);

        compute_layout(&mut arena, root, Size::new(100.0, 50.0));
        assert_eq!(rect_of(&arena, a), Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 });
        assert_eq!(rect_of(&arena, b), Rect { x: 15.0, y: 0.0, width: 20.0, height: 10.0 });
    }

    #[test]
    fn padding_offsets_content() {
        let mut arena = Arena::new();
        let mut s = Style::row();
        s.padding = Edges { left: 5.0, right: 0.0, top: 7.0, bottom: 0.0 };
        let root = arena.insert(LayoutBox::container(s));
        let a = leaf(&mut arena, 10.0, 10.0);
        arena.append_child(root, a);

        compute_layout(&mut arena, root, Size::new(100.0, 50.0));
        assert_eq!(rect_of(&arena, a), Rect { x: 5.0, y: 7.0, width: 10.0, height: 10.0 });
    }

    #[test]
    fn grow_distributes_free_space_by_weight() {
        let mut arena = Arena::new();
        let root = arena.insert(LayoutBox::container(Style::row()));
        let a = arena.insert(LayoutBox::leaf(Style::default().grow(1.0), Size::new(10.0, 10.0)));
        let b = arena.insert(LayoutBox::leaf(Style::default().grow(3.0), Size::new(10.0, 10.0)));
        arena.append_child(root, a);
        arena.append_child(root, b);

        compute_layout(&mut arena, root, Size::new(100.0, 50.0));
        assert_eq!(rect_of(&arena, a).width, 30.0);
        assert_eq!(rect_of(&arena, b).width, 70.0);
        assert_eq!(rect_of(&arena, b).x, 30.0);
    }

    #[test]
    fn shrink_absorbs_overflow_by_weight() {
        // Two 60-wide leaves in 100px, both shrink 1 => 20px deficit split evenly => 50 each.
        let mut arena = Arena::new();
        let root = arena.insert(LayoutBox::container(Style::row()));
        let a = arena.insert(LayoutBox::leaf(Style::default().shrink(1.0), Size::new(60.0, 10.0)));
        let b = arena.insert(LayoutBox::leaf(Style::default().shrink(1.0), Size::new(60.0, 10.0)));
        arena.append_child(root, a);
        arena.append_child(root, b);

        compute_layout(&mut arena, root, Size::new(100.0, 50.0));
        assert_eq!(rect_of(&arena, a).width, 50.0);
        assert_eq!(rect_of(&arena, b).width, 50.0);
        assert_eq!(rect_of(&arena, b).x, 50.0);
    }

    #[test]
    fn main_align_center_and_end() {
        let mut arena = Arena::new();
        let root_c = arena.insert(LayoutBox::container(Style::row().main_align(MainAlign::Center)));
        let a = leaf(&mut arena, 20.0, 10.0);
        arena.append_child(root_c, a);
        compute_layout(&mut arena, root_c, Size::new(100.0, 50.0));
        assert_eq!(rect_of(&arena, a).x, 40.0);

        let mut arena2 = Arena::new();
        let root_e = arena2.insert(LayoutBox::container(Style::row().main_align(MainAlign::End)));
        let b = arena2.insert(LayoutBox::leaf(Style::default(), Size::new(20.0, 10.0)));
        arena2.append_child(root_e, b);
        compute_layout(&mut arena2, root_e, Size::new(100.0, 50.0));
        assert_eq!(rect_of(&arena2, b).x, 80.0);
    }

    #[test]
    fn space_between_pushes_children_to_edges() {
        let mut arena = Arena::new();
        let root = arena.insert(LayoutBox::container(Style::row().main_align(MainAlign::SpaceBetween)));
        let a = leaf(&mut arena, 10.0, 10.0);
        let b = leaf(&mut arena, 10.0, 10.0);
        arena.append_child(root, a);
        arena.append_child(root, b);

        compute_layout(&mut arena, root, Size::new(100.0, 50.0));
        assert_eq!(rect_of(&arena, a).x, 0.0);
        assert_eq!(rect_of(&arena, b).x, 90.0);
    }

    #[test]
    fn cross_align_center_and_stretch() {
        let mut arena = Arena::new();
        let root = arena.insert(LayoutBox::container(Style::row().cross_align(CrossAlign::Center)));
        let a = leaf(&mut arena, 10.0, 10.0);
        arena.append_child(root, a);
        compute_layout(&mut arena, root, Size::new(100.0, 50.0));
        assert_eq!(rect_of(&arena, a).y, 20.0);

        let mut arena2 = Arena::new();
        let root2 = arena2.insert(LayoutBox::container(Style::row().cross_align(CrossAlign::Stretch)));
        let b = arena2.insert(LayoutBox::leaf(Style::default(), Size::new(10.0, 10.0)));
        arena2.append_child(root2, b);
        compute_layout(&mut arena2, root2, Size::new(100.0, 50.0));
        assert_eq!(rect_of(&arena2, b).height, 50.0);
        assert_eq!(rect_of(&arena2, b).y, 0.0);
    }

    #[test]
    fn auto_container_measures_to_content() {
        let mut arena = Arena::new();
        let root = arena.insert(LayoutBox::container(Style::column().gap(4.0)));
        let a = leaf(&mut arena, 10.0, 10.0);
        let b = leaf(&mut arena, 10.0, 10.0);
        arena.append_child(root, a);
        arena.append_child(root, b);

        let m = measure(&mut arena, root);
        assert_eq!(m, Size::new(10.0, 24.0));
    }

    #[test]
    fn fixed_length_overrides_content_and_clamps() {
        let mut arena = Arena::new();
        let s = Style::column().width(Length::Fixed(200.0));
        let root = arena.insert(LayoutBox::container(s));
        let a = leaf(&mut arena, 10.0, 10.0);
        arena.append_child(root, a);

        let m = measure(&mut arena, root);
        assert_eq!(m.width, 200.0);
        assert_eq!(m.height, 10.0);
    }

    #[test]
    fn nested_containers_lay_out_recursively() {
        let mut arena = Arena::new();
        let root = arena.insert(LayoutBox::container(Style::row().gap(0.0)));
        let inner = arena.insert(LayoutBox::container(Style::column().gap(2.0)));
        let c1 = leaf(&mut arena, 10.0, 10.0);
        let c2 = leaf(&mut arena, 10.0, 10.0);
        let sibling = leaf(&mut arena, 5.0, 5.0);
        arena.append_child(root, inner);
        arena.append_child(root, sibling);
        arena.append_child(inner, c1);
        arena.append_child(inner, c2);

        compute_layout(&mut arena, root, Size::new(100.0, 100.0));
        assert_eq!(rect_of(&arena, inner), Rect { x: 0.0, y: 0.0, width: 10.0, height: 22.0 });
        assert_eq!(rect_of(&arena, c1), Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 });
        assert_eq!(rect_of(&arena, c2), Rect { x: 0.0, y: 12.0, width: 10.0, height: 10.0 });
        assert_eq!(rect_of(&arena, sibling).x, 10.0);
    }

    #[test]
    fn stack_overlays_children_and_aligns_per_axis() {
        let mut arena = Arena::new();
        let root = arena.insert(LayoutBox::container(Style::stack()));
        let a = leaf(&mut arena, 10.0, 10.0);
        let b = leaf(&mut arena, 30.0, 20.0);
        arena.append_child(root, a);
        arena.append_child(root, b);
        compute_layout(&mut arena, root, Size::new(100.0, 100.0));
        // Start/Start: both at the content origin, at their own sizes.
        assert_eq!(rect_of(&arena, a), Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 });
        assert_eq!(rect_of(&arena, b), Rect { x: 0.0, y: 0.0, width: 30.0, height: 20.0 });

        // Centered on both axes.
        let mut arena2 = Arena::new();
        let root2 = arena2.insert(LayoutBox::container(
            Style::stack().main_align(MainAlign::Center).cross_align(CrossAlign::Center),
        ));
        let c = arena2.insert(LayoutBox::leaf(Style::default(), Size::new(10.0, 10.0)));
        arena2.append_child(root2, c);
        compute_layout(&mut arena2, root2, Size::new(100.0, 100.0));
        assert_eq!(rect_of(&arena2, c), Rect { x: 45.0, y: 45.0, width: 10.0, height: 10.0 });
    }

    #[test]
    fn grid_flows_children_by_columns() {
        // 3 leaves (10x10) in a 2-col grid, gaps 5/5.
        let mut arena = Arena::new();
        let root = arena.insert(LayoutBox::container(Style::grid(2, 5.0, 5.0)));
        let a = leaf(&mut arena, 10.0, 10.0);
        let b = leaf(&mut arena, 10.0, 10.0);
        let c = leaf(&mut arena, 10.0, 10.0);
        arena.append_child(root, a);
        arena.append_child(root, b);
        arena.append_child(root, c);

        // measured: 2 cols * 10 + 5 = 25 wide; 2 rows * 10 + 5 = 25 tall.
        let m = measure(&mut arena, root);
        assert_eq!(m, Size::new(25.0, 25.0));

        compute_layout(&mut arena, root, Size::new(200.0, 200.0));
        assert_eq!(rect_of(&arena, a), Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 });
        assert_eq!(rect_of(&arena, b), Rect { x: 15.0, y: 0.0, width: 10.0, height: 10.0 });
        assert_eq!(rect_of(&arena, c), Rect { x: 0.0, y: 15.0, width: 10.0, height: 10.0 });
    }
}
