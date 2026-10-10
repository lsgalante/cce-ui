//! The solver: `compute_layout` (measure then arrange from a root), `measure` (bottom-up) and
//! `arrange` (top-down), each per mode, and the axis helpers that project a size onto an axis's
//! main and cross extents.

use super::*;

// --- axis helpers: project/unproject a Size onto the (main, cross) frame of an axis ---

#[inline]
pub(super) fn main_of(axis: Axis, s: Size) -> f32 {
    match axis {
        Axis::Row => s.width,
        Axis::Column => s.height,
    }
}

#[inline]
pub(super) fn cross_of(axis: Axis, s: Size) -> f32 {
    match axis {
        Axis::Row => s.height,
        Axis::Column => s.width,
    }
}

#[inline]
pub(super) fn make_size(axis: Axis, main: f32, cross: f32) -> Size {
    match axis {
        Axis::Row => Size::new(main, cross),
        Axis::Column => Size::new(cross, main),
    }
}

/// Offset that places an item of extent `item` within `container` per a start/center/end rule.
#[inline]
pub(super) fn align_offset(start: bool, center: bool, container: f32, item: f32) -> f32 {
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
pub(super) fn child_rect(
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
pub(super) fn content_box(rect: Rect, p: Edges) -> (f32, f32, f32, f32) {
    (
        rect.x + p.left,
        rect.y + p.top,
        (rect.width - p.left - p.right).max(0.0),
        (rect.height - p.top - p.bottom).max(0.0),
    )
}

/// Resolve the node's own size from its content box: apply explicit width/height, add padding for
/// `Auto`, then clamp to min/max.
pub(super) fn finalize_size(style: &Style, content: Size) -> Size {
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

pub(super) fn measure_flex(arena: &mut Arena<LayoutBox>, style: &Style, children: &[NodeId]) -> Size {
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

pub(super) fn measure_stack(arena: &mut Arena<LayoutBox>, children: &[NodeId]) -> Size {
    let mut w = 0.0f32;
    let mut h = 0.0f32;
    for &child in children {
        let cs = measure(arena, child);
        w = w.max(cs.width);
        h = h.max(cs.height);
    }
    Size::new(w, h)
}

pub(super) fn measure_grid(arena: &mut Arena<LayoutBox>, spec: GridSpec, children: &[NodeId]) -> Size {
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

pub(super) fn arrange_flex(
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

pub(super) fn arrange_stack(
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

pub(super) fn arrange_grid(
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
