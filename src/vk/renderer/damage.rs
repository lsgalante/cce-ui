//! The partial frame: rects in physical px, what a frame damages, and the first frosted
//! plate over nothing that needs no snapshot.

use super::*;

/// How far a swapchain image's pixels are behind the latest frame. A frame
/// with [`Frame2D::damage`] repaints only what the image it acquired is
/// missing — the frame's own damage plus whatever frames that went to the
/// OTHER images changed in the meantime — instead of every pixel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum ImageAge {
    /// Never rendered, or a full-surface frame went by: repaint everything.
    Unknown,
    /// Holds the latest frame.
    Current,
    /// Holds the latest frame except inside this rect.
    Behind(vk::Rect2D),
}

pub(super) fn rect_union(a: vk::Rect2D, b: vk::Rect2D) -> vk::Rect2D {
    if a.extent.width == 0 || a.extent.height == 0 {
        return b;
    }
    if b.extent.width == 0 || b.extent.height == 0 {
        return a;
    }
    let x0 = a.offset.x.min(b.offset.x);
    let y0 = a.offset.y.min(b.offset.y);
    let x1 = (a.offset.x + a.extent.width as i32).max(b.offset.x + b.extent.width as i32);
    let y1 = (a.offset.y + a.extent.height as i32).max(b.offset.y + b.extent.height as i32);
    vk::Rect2D {
        offset: vk::Offset2D { x: x0, y: y0 },
        extent: vk::Extent2D { width: (x1 - x0) as u32, height: (y1 - y0) as u32 },
    }
}

/// `a` cut down to `b`; zero-sized (a legal scissor that draws nothing) when
/// they do not meet.
pub(super) fn rect_intersect(a: vk::Rect2D, b: vk::Rect2D) -> vk::Rect2D {
    let x0 = a.offset.x.max(b.offset.x);
    let y0 = a.offset.y.max(b.offset.y);
    let x1 = (a.offset.x + a.extent.width as i32).min(b.offset.x + b.extent.width as i32);
    let y1 = (a.offset.y + a.extent.height as i32).min(b.offset.y + b.extent.height as i32);
    vk::Rect2D {
        offset: vk::Offset2D { x: x0, y: y0 },
        extent: vk::Extent2D {
            width: (x1 - x0).max(0) as u32,
            height: (y1 - y0).max(0) as u32,
        },
    }
}

/// How far beyond its own rect a frosted plate's blur can sample, physical
/// px: shader2d's 7x7 kernel reaches three taps of the plate's stride either
/// way, plus the rim's refraction offset. Generous on purpose — the panel's
/// stride is 5.5 px at scale 2, so the real reach is ~17 px — because an
/// under-estimate shows as a seam and an over-estimate only repaints more.
pub(super) const BLUR_REACH_PX: i32 = 96;

pub(super) fn rect_expand(r: vk::Rect2D, by: i32) -> vk::Rect2D {
    vk::Rect2D {
        offset: vk::Offset2D { x: r.offset.x - by, y: r.offset.y - by },
        extent: vk::Extent2D { width: r.extent.width + 2 * by as u32, height: r.extent.height + 2 * by as u32 },
    }
}

pub(super) fn rect_overlaps(a: vk::Rect2D, b: vk::Rect2D) -> bool {
    let i = rect_intersect(a, b);
    i.extent.width > 0 && i.extent.height > 0
}

/// The physical-px box a run of vertices covers (positions are NDC, y up).
pub(crate) fn verts_bounds(verts: &[crate::engine::Vertex], extent: vk::Extent2D) -> Option<vk::Rect2D> {
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for v in verts {
        let px = (v.position[0] + 1.0) * 0.5 * extent.width as f32;
        let py = (1.0 - v.position[1]) * 0.5 * extent.height as f32;
        x0 = x0.min(px);
        y0 = y0.min(py);
        x1 = x1.max(px);
        y1 = y1.max(py);
    }
    if x0 > x1 {
        return None;
    }
    let (x0, y0) = ((x0.floor() - 1.0) as i32, (y0.floor() - 1.0) as i32);
    let (x1, y1) = ((x1.ceil() + 1.0) as i32, (y1.ceil() + 1.0) as i32);
    Some(vk::Rect2D {
        offset: vk::Offset2D { x: x0, y: y0 },
        extent: vk::Extent2D { width: (x1 - x0).max(0) as u32, height: (y1 - y0).max(0) as u32 },
    })
}

/// Index of the frosted batch that is the first thing the frame draws, if
/// it may sample the zeroed scene backdrop instead of a snapshot of the
/// frame so far: with nothing drawn yet, a transparent clear and no scene in
/// the backdrop, the two hold the same pixels — and the backdrop needs no
/// copy and, in a partial frame, does not depend on pixels outside the
/// repainted region. That is what lets the root plate, frosted in every
/// themed app and the size of the window, stay out of the region growth
/// below.
pub(crate) fn first_frost_exempt(batches: &[Batch2D], images: &[crate::draw::ImageQuad], clear: [f32; 4], use_backdrop: bool) -> Option<usize> {
    if use_backdrop || clear != [0.0, 0.0, 0.0, 0.0] {
        return None;
    }
    let first = batches.iter().position(|b| b.start < b.end)?;
    let batch = &batches[first];
    let image_before = images.iter().any(|q| q.z_before <= batch.start);
    (batch.blur_behind && !image_before).then_some(first)
}

#[cfg(test)]
mod frost_exempt_tests {
    use super::*;

    fn batch(start: u32, end: u32, blur: bool) -> Batch2D {
        Batch2D { scissor: None, clip_rrect: None, start, end, plate: None, blur_behind: blur }
    }

    /// Only a frosted batch that is the first thing drawn, over a
    /// transparent clear with no scene, may skip its snapshot.
    #[test]
    fn only_the_first_frost_over_nothing_is_exempt() {
        let clear = [0.0; 4];
        let root_first = [batch(0, 6, true), batch(6, 12, true)];
        assert_eq!(first_frost_exempt(&root_first, &[], clear, false), Some(0));
        // An empty leading batch does not count as drawing.
        assert_eq!(first_frost_exempt(&[batch(0, 0, false), batch(0, 6, true)], &[], clear, false), Some(1));
        // Something drawn first, a scene backdrop, or an opaque clear: no.
        assert_eq!(first_frost_exempt(&[batch(0, 6, false), batch(6, 12, true)], &[], clear, false), None);
        assert_eq!(first_frost_exempt(&root_first, &[], clear, true), None);
        assert_eq!(first_frost_exempt(&root_first, &[], [0.0, 0.0, 0.0, 1.0], false), None);
        let image = crate::draw::ImageQuad { image: 1, rect: (0.0, 0.0, 1.0, 1.0), alpha: 1.0, z_before: 0, clip: None };
        assert_eq!(first_frost_exempt(&root_first, &[image], clear, false), None);
    }
}
