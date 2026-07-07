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
//!     within it — distributing flex `grow`, honoring `gap`/`padding`, and applying main/cross
//!     alignment. Written into `LayoutBox::rect`.
//!
//! Because it operates on `Style` + `Size` and writes plain rects, it is fully unit-testable
//! without a GPU or a Wayland surface (see the tests below).
//!
//! Scope of this first cut: a flexbox-style **row/column** model with padding, gap, grow,
//! main-axis alignment (start/center/end/space-between), cross-axis alignment
//! (start/center/end/stretch), fixed/auto sizing, and min/max clamps. Deferred to later
//! increments: shrink weights, wrapping, grid, percentage lengths, and the glyphon text-measure
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

/// The main-axis direction a container lays its children along.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Row,
    Column,
}

/// A length along one axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    /// Size to content (children/intrinsic), plus padding.
    Auto,
    /// Fixed logical pixels, overriding content size.
    Fixed(f32),
}

/// Distribution of free space along the main axis (when no child grows).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainAlign {
    Start,
    Center,
    End,
    SpaceBetween,
}

/// Placement of each child across the cross axis.
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
    pub axis: Axis,
    pub padding: Edges,
    pub gap: f32,
    pub main_align: MainAlign,
    pub cross_align: CrossAlign,
    pub width: Length,
    pub height: Length,
    /// Flex grow weight: share of leftover main-axis space this node claims.
    pub grow: f32,
    pub min_width: f32,
    pub min_height: f32,
    pub max_width: f32,
    pub max_height: f32,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            axis: Axis::Column,
            padding: Edges::ZERO,
            gap: 0.0,
            main_align: MainAlign::Start,
            cross_align: CrossAlign::Start,
            width: Length::Auto,
            height: Length::Auto,
            grow: 0.0,
            min_width: 0.0,
            min_height: 0.0,
            max_width: f32::INFINITY,
            max_height: f32::INFINITY,
        }
    }
}

impl Style {
    pub fn row() -> Self {
        Style { axis: Axis::Row, ..Default::default() }
    }
    pub fn column() -> Self {
        Style { axis: Axis::Column, ..Default::default() }
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

#[inline]
fn pad_main(axis: Axis, p: Edges) -> f32 {
    match axis {
        Axis::Row => p.left + p.right,
        Axis::Column => p.top + p.bottom,
    }
}

#[inline]
fn pad_cross(axis: Axis, p: Edges) -> f32 {
    match axis {
        Axis::Row => p.top + p.bottom,
        Axis::Column => p.left + p.right,
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

    let (content_main, content_cross) = if children.is_empty() {
        let s = intrinsic.unwrap_or(Size::ZERO);
        (main_of(style.axis, s), cross_of(style.axis, s))
    } else {
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
        (main, cross)
    };

    let full = make_size(
        style.axis,
        content_main + pad_main(style.axis, style.padding),
        content_cross + pad_cross(style.axis, style.padding),
    );

    let mut size = Size {
        width: match style.width {
            Length::Fixed(v) => v,
            Length::Auto => full.width,
        },
        height: match style.height {
            Length::Fixed(v) => v,
            Length::Auto => full.height,
        },
    };
    size.width = size.width.clamp(style.min_width, style.max_width);
    size.height = size.height.clamp(style.min_height, style.max_height);

    arena.value_mut(id).expect("measure: stale node").measured = size;
    size
}

/// Top-down placement. Assigns `rect` to `id`, then positions its children within it.
pub fn arrange(arena: &mut Arena<LayoutBox>, id: NodeId, rect: Rect) {
    arena.value_mut(id).expect("arrange: stale node").rect = rect;

    let style = arena.value(id).unwrap().style;
    let children = arena.children(id).to_vec();
    if children.is_empty() {
        return;
    }

    let content_x = rect.x + style.padding.left;
    let content_y = rect.y + style.padding.top;
    let content_w = (rect.width - style.padding.left - style.padding.right).max(0.0);
    let content_h = (rect.height - style.padding.top - style.padding.bottom).max(0.0);
    let content = Size::new(content_w, content_h);
    let content_main = main_of(style.axis, content);
    let content_cross = cross_of(style.axis, content);

    // Snapshot each child's measured main/cross extent and grow weight.
    let mut child_main = Vec::with_capacity(children.len());
    let mut child_cross = Vec::with_capacity(children.len());
    let mut grows = Vec::with_capacity(children.len());
    for &child in &children {
        let b = arena.value(child).unwrap();
        child_main.push(main_of(style.axis, b.measured));
        child_cross.push(cross_of(style.axis, b.measured));
        grows.push(b.style.grow);
    }

    let n = children.len();
    let total_main: f32 = child_main.iter().sum::<f32>() + style.gap * (n as f32 - 1.0);
    let free = content_main - total_main;
    let total_grow: f32 = grows.iter().sum();
    let extra_per_grow = if total_grow > 0.0 && free > 0.0 { free / total_grow } else { 0.0 };

    // Alignment only distributes leftover space when nothing grows (grow already consumes it).
    let (start_offset, spacing_extra) = if total_grow > 0.0 {
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

    let mut placements: Vec<(NodeId, Rect)> = Vec::with_capacity(n);
    let mut main_pos = start_offset;
    for i in 0..n {
        let main_size = child_main[i] + grows[i] * extra_per_grow;
        let cross_size = match style.cross_align {
            CrossAlign::Stretch => content_cross,
            _ => child_cross[i],
        };
        let cross_pos = match style.cross_align {
            CrossAlign::Start | CrossAlign::Stretch => 0.0,
            CrossAlign::Center => (content_cross - cross_size) / 2.0,
            CrossAlign::End => content_cross - cross_size,
        };
        let r = child_rect(style.axis, content_x, content_y, main_pos, cross_pos, main_size, cross_size);
        placements.push((children[i], r));
        main_pos += main_size + style.gap + spacing_extra;
    }

    for (child, r) in placements {
        arrange(arena, child, r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // Two 10-wide leaves, grow 1 and 3. Free = 100 - 20 = 80, split 1:3 => +20 and +60.
        let a = arena.insert(LayoutBox::leaf(Style::default().grow(1.0), Size::new(10.0, 10.0)));
        let b = arena.insert(LayoutBox::leaf(Style::default().grow(3.0), Size::new(10.0, 10.0)));
        arena.append_child(root, a);
        arena.append_child(root, b);

        compute_layout(&mut arena, root, Size::new(100.0, 50.0));
        assert_eq!(rect_of(&arena, a).width, 30.0);
        assert_eq!(rect_of(&arena, b).width, 70.0);
        assert_eq!(rect_of(&arena, b).x, 30.0, "b starts after a's grown width");
    }

    #[test]
    fn main_align_center_and_end() {
        let mut arena = Arena::new();
        let root_c = arena.insert(LayoutBox::container(Style::row().main_align(MainAlign::Center)));
        let a = leaf(&mut arena, 20.0, 10.0);
        arena.append_child(root_c, a);
        compute_layout(&mut arena, root_c, Size::new(100.0, 50.0));
        assert_eq!(rect_of(&arena, a).x, 40.0, "centered: (100-20)/2");

        let mut arena2 = Arena::new();
        let root_e = arena2.insert(LayoutBox::container(Style::row().main_align(MainAlign::End)));
        let b = arena2.insert(LayoutBox::leaf(Style::default(), Size::new(20.0, 10.0)));
        arena2.append_child(root_e, b);
        compute_layout(&mut arena2, root_e, Size::new(100.0, 50.0));
        assert_eq!(rect_of(&arena2, b).x, 80.0, "end: 100-20");
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
        assert_eq!(rect_of(&arena, a).y, 20.0, "centered on cross axis: (50-10)/2");

        let mut arena2 = Arena::new();
        let root2 = arena2.insert(LayoutBox::container(Style::row().cross_align(CrossAlign::Stretch)));
        let b = arena2.insert(LayoutBox::leaf(Style::default(), Size::new(10.0, 10.0)));
        arena2.append_child(root2, b);
        compute_layout(&mut arena2, root2, Size::new(100.0, 50.0));
        assert_eq!(rect_of(&arena2, b).height, 50.0, "stretched to cross content extent");
        assert_eq!(rect_of(&arena2, b).y, 0.0);
    }

    #[test]
    fn auto_container_measures_to_content() {
        // A column with two 10x10 leaves and gap 4 should measure 10 wide x 24 tall.
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
        assert_eq!(m.width, 200.0, "fixed width wins over 10px content");
        assert_eq!(m.height, 10.0, "auto height still follows content");
    }

    #[test]
    fn nested_containers_lay_out_recursively() {
        // root(row) -> [ inner(column, gap 2) -> [c1 10x10, c2 10x10], sibling 5x5 ]
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

        // inner measures 10 wide x 22 tall; sits at origin; sibling to its right.
        assert_eq!(rect_of(&arena, inner), Rect { x: 0.0, y: 0.0, width: 10.0, height: 22.0 });
        assert_eq!(rect_of(&arena, c1), Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 });
        assert_eq!(rect_of(&arena, c2), Rect { x: 0.0, y: 12.0, width: 10.0, height: 10.0 });
        assert_eq!(rect_of(&arena, sibling).x, 10.0, "sibling follows inner's width");
    }
}
