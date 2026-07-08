//! Display list + paint context — Phase 3 of the core rebuild (single paint path).
//!
//! Today the toolkit paints through **three** uncoordinated routes — the app's top-level
//! `view*` methods, each container's recursive `all_quads`/`all_rounded_quads` (with clipping
//! hand-copied into every container), and the immediate-mode `render_widget`/`SectionContext`.
//! Nothing arbitrates z-order (hence the `overlay_quads` escape hatch) and every clip is CPU
//! rect-intersection math duplicated per container (there is no GPU scissor).
//!
//! This module is the foundation for collapsing those into **one** ordered pass: a paint walk
//! emits primitives into a single [`DisplayList`] through a [`PaintCtx`] that carries a **clip
//! stack** (each pushed clip is intersected with the current one, so a primitive records the exact
//! scissor rect it should be drawn under) and a **translate stack** (local coordinates compose to
//! absolute — the seam Phase 4 animation slides/scales through). The backend then tessellates the
//! one ordered list, using the recorded clip as a GPU `set_scissor_rect`.
//!
//! This first cut is pure data + bookkeeping, fully unit-tested without a GPU. Wiring the widget
//! tree's paint into it, and routing the backend through the result, are the runtime-gated
//! follow-ups.

use crate::scene::layout::Rect;

/// End-cap style for a [`Prim::Vector`], mirroring the toolkit's line caps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cap {
    Flat,
    Round,
    Arrow,
}

/// A single paint primitive in logical pixels (absolute coordinates once emitted). These mirror
/// the toolkit's existing tessellators so a `DisplayList` maps directly onto them at draw time.
/// Per-corner radii `(top_left, top_right, bottom_right, bottom_left)`, matching the toolkit's
/// `CornerRadii` order.
pub type Radii = (f32, f32, f32, f32);

#[derive(Clone, Debug, PartialEq)]
pub enum Prim {
    Quad { rect: Rect, color: [f32; 4] },
    RoundedRect { rect: Rect, radius: f32, corners: (bool, bool, bool, bool), color: [f32; 4] },
    /// A rounded fill plus a solid border stroke — a widget's own "plate" (mirrors
    /// `push_widget_vertices`' non-bevel branch: rounded bg + `push_plate_solid_border_vertices`).
    Border { rect: Rect, radii: Radii, fill: [f32; 4], border: [f32; 4], thickness: f32 },
    /// A beveled plate: an inset rounded fill plus lightened/darkened edges (mirrors
    /// `push_widget_vertices`' bevel branch).
    Bevel { rect: Rect, radii: Radii, color: [f32; 4], depth: f32 },
    Arc { cx: f32, cy: f32, radius: f32, thickness: f32, start: f32, end: f32, color: [f32; 4] },
    Vector { x1: f32, y1: f32, x2: f32, y2: f32, thickness: f32, color: [f32; 4], cap: Cap },
    Circle { cx: f32, cy: f32, radius: f32, color: [f32; 4] },
    Text { text: String, x: f32, y: f32, font_size: f32, color: [u8; 3] },
}

/// A primitive plus the scissor rect it must be clipped to (`None` = unclipped).
#[derive(Clone, Debug, PartialEq)]
pub struct PaintItem {
    pub prim: Prim,
    pub clip: Option<Rect>,
}

/// An ordered list of clipped primitives — the single source of truth for a frame's geometry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplayList {
    pub items: Vec<PaintItem>,
}

impl DisplayList {
    pub fn new() -> Self {
        DisplayList { items: Vec::new() }
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Intersection of two rects, clamped so width/height never go negative (an empty clip is a
/// zero-size rect — nothing draws under it).
fn intersect(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.width).min(b.x + b.width);
    let y1 = (a.y + a.height).min(b.y + b.height);
    Rect { x: x0, y: y0, width: (x1 - x0).max(0.0), height: (y1 - y0).max(0.0) }
}

/// Accumulates a [`DisplayList`] while a paint walk pushes/pops clips and translations.
///
/// Coordinates passed to the emit methods (and to [`push_clip`](PaintCtx::push_clip)) are in the
/// **current** local space; the active translation is applied so everything recorded is absolute.
pub struct PaintCtx {
    list: DisplayList,
    /// Each entry is the effective (already-intersected, absolute) clip at that depth.
    clip_stack: Vec<Rect>,
    /// Saved offsets for nesting; `offset` is the current cumulative translation.
    offset_stack: Vec<(f32, f32)>,
    offset: (f32, f32),
}

impl Default for PaintCtx {
    fn default() -> Self {
        Self::new()
    }
}

impl PaintCtx {
    pub fn new() -> Self {
        PaintCtx { list: DisplayList::new(), clip_stack: Vec::new(), offset_stack: Vec::new(), offset: (0.0, 0.0) }
    }

    /// The scissor rect primitives are currently recorded under.
    pub fn current_clip(&self) -> Option<Rect> {
        self.clip_stack.last().copied()
    }

    /// Push a clip (in current local space); it is translated to absolute and intersected with the
    /// enclosing clip. Pair with [`pop_clip`](PaintCtx::pop_clip), or prefer [`clip`](PaintCtx::clip).
    pub fn push_clip(&mut self, rect: Rect) {
        let r = self.apply_offset(rect);
        let effective = match self.clip_stack.last() {
            Some(cur) => intersect(*cur, r),
            None => r,
        };
        self.clip_stack.push(effective);
    }

    pub fn pop_clip(&mut self) {
        self.clip_stack.pop();
    }

    /// Run `f` with `rect` pushed as a clip, popping it afterward.
    pub fn clip<R>(&mut self, rect: Rect, f: impl FnOnce(&mut Self) -> R) -> R {
        self.push_clip(rect);
        let out = f(self);
        self.pop_clip();
        out
    }

    /// Run `f` with an additional translation applied to all emitted coordinates.
    pub fn translate<R>(&mut self, dx: f32, dy: f32, f: impl FnOnce(&mut Self) -> R) -> R {
        self.offset_stack.push(self.offset);
        self.offset = (self.offset.0 + dx, self.offset.1 + dy);
        let out = f(self);
        self.offset = self.offset_stack.pop().expect("translate stack underflow");
        out
    }

    fn apply_offset(&self, r: Rect) -> Rect {
        Rect { x: r.x + self.offset.0, y: r.y + self.offset.1, width: r.width, height: r.height }
    }

    fn push(&mut self, prim: Prim) {
        let clip = self.current_clip();
        self.list.items.push(PaintItem { prim, clip });
    }

    pub fn quad(&mut self, rect: Rect, color: [f32; 4]) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Quad { rect, color });
    }

    pub fn rounded_rect(&mut self, rect: Rect, radius: f32, corners: (bool, bool, bool, bool), color: [f32; 4]) {
        let rect = self.apply_offset(rect);
        self.push(Prim::RoundedRect { rect, radius, corners, color });
    }

    pub fn vector(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, thickness: f32, color: [f32; 4], cap: Cap) {
        let (ox, oy) = self.offset;
        self.push(Prim::Vector { x1: x1 + ox, y1: y1 + oy, x2: x2 + ox, y2: y2 + oy, thickness, color, cap });
    }

    pub fn circle(&mut self, cx: f32, cy: f32, radius: f32, color: [f32; 4]) {
        let (ox, oy) = self.offset;
        self.push(Prim::Circle { cx: cx + ox, cy: cy + oy, radius, color });
    }

    pub fn border(&mut self, rect: Rect, radii: Radii, fill: [f32; 4], border: [f32; 4], thickness: f32) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Border { rect, radii, fill, border, thickness });
    }

    pub fn bevel(&mut self, rect: Rect, radii: Radii, color: [f32; 4], depth: f32) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Bevel { rect, radii, color, depth });
    }

    pub fn arc(&mut self, cx: f32, cy: f32, radius: f32, thickness: f32, start: f32, end: f32, color: [f32; 4]) {
        let (ox, oy) = self.offset;
        self.push(Prim::Arc { cx: cx + ox, cy: cy + oy, radius, thickness, start, end, color });
    }

    pub fn text(&mut self, text: impl Into<String>, x: f32, y: f32, font_size: f32, color: [u8; 3]) {
        let (ox, oy) = self.offset;
        self.push(Prim::Text { text: text.into(), x: x + ox, y: y + oy, font_size, color });
    }

    /// Consume the context and return the accumulated display list.
    pub fn finish(self) -> DisplayList {
        debug_assert!(self.clip_stack.is_empty(), "unbalanced push_clip/pop_clip");
        debug_assert!(self.offset_stack.is_empty(), "unbalanced translate");
        self.list
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, width: w, height: h }
    }

    #[test]
    fn emits_in_order_unclipped() {
        let mut ctx = PaintCtx::new();
        ctx.quad(r(0.0, 0.0, 10.0, 10.0), [1.0, 0.0, 0.0, 1.0]);
        ctx.quad(r(5.0, 5.0, 10.0, 10.0), [0.0, 1.0, 0.0, 1.0]);
        let list = ctx.finish();
        assert_eq!(list.len(), 2);
        assert_eq!(list.items[0].clip, None);
        assert!(matches!(list.items[0].prim, Prim::Quad { color, .. } if color[0] == 1.0));
        assert!(matches!(list.items[1].prim, Prim::Quad { color, .. } if color[1] == 1.0));
    }

    #[test]
    fn clip_is_recorded_and_popped() {
        let mut ctx = PaintCtx::new();
        ctx.clip(r(0.0, 0.0, 50.0, 50.0), |ctx| {
            ctx.quad(r(10.0, 10.0, 5.0, 5.0), [0.0; 4]);
        });
        ctx.quad(r(60.0, 60.0, 5.0, 5.0), [0.0; 4]); // outside any clip now
        let list = ctx.finish();
        assert_eq!(list.items[0].clip, Some(r(0.0, 0.0, 50.0, 50.0)));
        assert_eq!(list.items[1].clip, None, "clip popped after the closure");
    }

    #[test]
    fn nested_clips_intersect() {
        let mut ctx = PaintCtx::new();
        ctx.clip(r(0.0, 0.0, 100.0, 100.0), |ctx| {
            ctx.clip(r(50.0, 50.0, 100.0, 100.0), |ctx| {
                ctx.quad(r(0.0, 0.0, 1.0, 1.0), [0.0; 4]);
            });
        });
        // Intersection of (0,0,100,100) and (50,50,100,100) = (50,50,50,50).
        assert_eq!(ctx.finish().items[0].clip, Some(r(50.0, 50.0, 50.0, 50.0)));
    }

    #[test]
    fn non_overlapping_clips_produce_empty_scissor() {
        let mut ctx = PaintCtx::new();
        ctx.clip(r(0.0, 0.0, 10.0, 10.0), |ctx| {
            ctx.clip(r(100.0, 100.0, 10.0, 10.0), |ctx| {
                ctx.quad(r(0.0, 0.0, 1.0, 1.0), [0.0; 4]);
            });
        });
        let clip = ctx.finish().items[0].clip.unwrap();
        assert_eq!((clip.width, clip.height), (0.0, 0.0), "empty intersection");
    }

    #[test]
    fn translate_applies_to_coordinates_and_restores() {
        let mut ctx = PaintCtx::new();
        ctx.translate(100.0, 200.0, |ctx| {
            ctx.quad(r(0.0, 0.0, 5.0, 5.0), [0.0; 4]);
        });
        ctx.quad(r(0.0, 0.0, 5.0, 5.0), [0.0; 4]); // back at origin
        let list = ctx.finish();
        assert!(matches!(list.items[0].prim, Prim::Quad { rect, .. } if rect.x == 100.0 && rect.y == 200.0));
        assert!(matches!(list.items[1].prim, Prim::Quad { rect, .. } if rect.x == 0.0 && rect.y == 0.0));
    }

    #[test]
    fn nested_translate_is_cumulative() {
        let mut ctx = PaintCtx::new();
        ctx.translate(10.0, 10.0, |ctx| {
            ctx.translate(5.0, 5.0, |ctx| {
                ctx.circle(0.0, 0.0, 3.0, [0.0; 4]);
            });
        });
        assert!(matches!(ctx.finish().items[0].prim, Prim::Circle { cx, cy, .. } if cx == 15.0 && cy == 15.0));
    }

    #[test]
    fn clip_pushed_under_translation_is_absolute() {
        let mut ctx = PaintCtx::new();
        ctx.translate(20.0, 20.0, |ctx| {
            ctx.clip(r(0.0, 0.0, 30.0, 30.0), |ctx| {
                ctx.quad(r(0.0, 0.0, 5.0, 5.0), [0.0; 4]);
            });
        });
        let item = &ctx.finish().items[0];
        // Clip translated to absolute (20,20,30,30); prim likewise at (20,20).
        assert_eq!(item.clip, Some(r(20.0, 20.0, 30.0, 30.0)));
        assert!(matches!(item.prim, Prim::Quad { rect, .. } if rect.x == 20.0 && rect.y == 20.0));
    }

    #[test]
    fn all_primitive_kinds_emit() {
        let mut ctx = PaintCtx::new();
        ctx.quad(r(0.0, 0.0, 1.0, 1.0), [0.0; 4]);
        ctx.rounded_rect(r(0.0, 0.0, 1.0, 1.0), 2.0, (true, false, true, false), [0.0; 4]);
        ctx.border(r(0.0, 0.0, 10.0, 10.0), (2.0, 2.0, 2.0, 2.0), [0.1; 4], [0.9; 4], 1.5);
        ctx.bevel(r(0.0, 0.0, 10.0, 10.0), (2.0, 2.0, 2.0, 2.0), [0.3; 4], 2.0);
        ctx.arc(5.0, 5.0, 4.0, 1.0, 0.0, 3.14, [0.0; 4]);
        ctx.vector(0.0, 0.0, 10.0, 0.0, 1.0, [0.0; 4], Cap::Arrow);
        ctx.circle(5.0, 5.0, 3.0, [0.0; 4]);
        ctx.text("hi", 1.0, 2.0, 12.0, [255, 255, 255]);
        assert_eq!(ctx.finish().len(), 8);
    }

    #[test]
    fn border_and_bevel_are_offset() {
        let mut ctx = PaintCtx::new();
        ctx.translate(10.0, 20.0, |ctx| {
            ctx.border(r(0.0, 0.0, 5.0, 5.0), (1.0, 1.0, 1.0, 1.0), [0.0; 4], [1.0; 4], 1.0);
            ctx.bevel(r(0.0, 0.0, 5.0, 5.0), (1.0, 1.0, 1.0, 1.0), [0.0; 4], 1.0);
        });
        let list = ctx.finish();
        assert!(matches!(list.items[0].prim, Prim::Border { rect, .. } if rect.x == 10.0 && rect.y == 20.0));
        assert!(matches!(list.items[1].prim, Prim::Bevel { rect, .. } if rect.x == 10.0 && rect.y == 20.0));
    }
}
