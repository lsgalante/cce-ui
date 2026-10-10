//! The paint vocabulary: what a frame is made of, and how it is emitted. A paint walk emits
//! [`Prim`]s into one ordered [`DisplayList`] through a [`PaintCtx`], which carries a **clip
//! stack** (each pushed clip is intersected with the current one, so an item records the exact
//! scissor it is drawn under — a rect, a circle and a rounded rect clip compose) and a
//! **translate stack** (local coordinates compose to absolute). The backend tessellates the one
//! list in order (`backend::tessellate`). Pure data and bookkeeping, tested without a GPU.
//!
//! What to emit is the surface vocabulary (`CLAUDE.md`, "Surfaces", and `docs/surfaces.md`): a
//! control face goes through [`ControlPlate`] / [`PaintCtx::control_plate`], a root or pane plate
//! through [`PlateSpec`], a well and its run through [`Field`]; never a hand-rolled carve.
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the display list, [`PaintCtx`]'s stacks, `push`, `append_items`, `replay`, `finish` |
//! | `prim` | [`Prim`], the vocabulary every widget paints in, and its parts (`Cap`, `Radii`, the droplet's finish) |
//! | `surfaces` | [`PlateSpec`], [`PlateStance`], [`Field`], [`ControlPlate`] |
//! | `relief` | `PaintCtx`'s relief emitters: plates, bevels, carves, fields, wells, grooves, lattices, droplets |
//! | `flat` | `PaintCtx`'s flat emitters: quads, images and icons, rounded rects, glow, vectors, circles, arcs, borders |
//! | `text` | the text types and `PaintCtx`'s text emitters |
//! | `render_target` | `PaintCtx` as a flat host's `RenderTarget` |

mod flat;
mod prim;
mod relief;
mod render_target;
mod target;
pub use target::*;
mod surfaces;
mod text;
#[cfg(test)]
mod tests;

pub use prim::*;
pub use surfaces::*;
pub use text::*;

use crate::scene::layout::Rect;
use crate::scene::material::{Finish, Material, PlateRole};


/// A primitive plus the scissor rect it must be clipped to (`None` = unclipped), an
/// optional circular clip `[cx, cy, r]` in logical pixels (`None` = unclipped) — the
/// per-vertex circle clip the tessellators already support, for round panes (the designer's
/// circular network pane) — and an optional rounded-rect clip `[cx, cy, bx, by, r]`
/// (center, SDF half-extents = half-size minus radius, corner radius; logical px) so a
/// plate's children cut off at its rounded corners. The clips compose: the scissor is GPU
/// state, the circle rides the vertices, the rounded rect is per-draw-batch state.
#[derive(Clone, Debug, PartialEq)]
pub struct PaintItem {
    pub prim: Prim,
    pub clip: Option<Rect>,
    pub clip_circle: Option<[f32; 3]>,
    pub clip_rrect: Option<[f32; 5]>,
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
    /// Active circular clips; primitives record the innermost (`last`). Circles don't
    /// intersect analytically like rects, so nesting keeps the innermost only.
    clip_circle_stack: Vec<[f32; 3]>,
    /// Active rounded-rect clips `[cx, cy, bx, by, r]`; innermost wins, like circles.
    clip_rrect_stack: Vec<[f32; 5]>,
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
        PaintCtx {
            list: DisplayList::new(),
            clip_stack: Vec::new(),
            clip_circle_stack: Vec::new(),
            clip_rrect_stack: Vec::new(),
            offset_stack: Vec::new(),
            offset: (0.0, 0.0),
        }
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

    /// Push a circular clip `[cx, cy, r]` (current local space, translated to absolute).
    /// Primitives emitted while it is active record it and tessellate with the per-vertex
    /// circle clip. Pair with [`pop_clip_circle`](PaintCtx::pop_clip_circle), or prefer
    /// [`clip_circle`](PaintCtx::clip_circle).
    pub fn push_clip_circle(&mut self, c: [f32; 3]) {
        self.clip_circle_stack.push([c[0] + self.offset.0, c[1] + self.offset.1, c[2]]);
    }

    pub fn pop_clip_circle(&mut self) {
        self.clip_circle_stack.pop();
    }

    /// Run `f` with `[cx, cy, r]` pushed as a circular clip, popping it afterward.
    pub fn clip_circle<R>(&mut self, c: [f32; 3], f: impl FnOnce(&mut Self) -> R) -> R {
        self.push_clip_circle(c);
        let out = f(self);
        self.pop_clip_circle();
        out
    }

    /// Push a rounded-rect clip: `rect` (current local space) with corner radius `radius`,
    /// so children of a rounded plate cut off at its corners. Pushes the rect as a scissor
    /// too — the scissor handles the straight edges (and keeps batching), the SDF trims the
    /// corners. A radius of zero degenerates to the plain rect clip. Pair with
    /// [`pop_clip_rounded`](PaintCtx::pop_clip_rounded), or prefer
    /// [`clip_rounded`](PaintCtx::clip_rounded).
    pub fn push_clip_rounded(&mut self, rect: Rect, radius: f32) {
        self.push_clip(rect);
        let r = radius.max(0.0);
        if r > 0.0 {
            let abs = self.apply_offset(rect);
            self.clip_rrect_stack.push([
                abs.x + abs.width / 2.0,
                abs.y + abs.height / 2.0,
                (abs.width / 2.0 - r).max(0.0),
                (abs.height / 2.0 - r).max(0.0),
                r,
            ]);
        } else {
            // Keep push/pop balanced regardless of radius.
            self.clip_rrect_stack.push([0.0; 5]);
        }
    }

    pub fn pop_clip_rounded(&mut self) {
        self.clip_rrect_stack.pop();
        self.pop_clip();
    }

    /// Run `f` with `rect` (radius `radius`) pushed as a rounded clip, popping it afterward.
    pub fn clip_rounded<R>(&mut self, rect: Rect, radius: f32, f: impl FnOnce(&mut Self) -> R) -> R {
        self.push_clip_rounded(rect, radius);
        let out = f(self);
        self.pop_clip_rounded();
        out
    }

    /// Run `f` with an additional translation applied to all emitted coordinates.
    /// Imperative translate pair for spans too large to wrap in
    /// [`translate`](Self::translate)'s closure (an app bracketing its whole
    /// frame in the overflow-margin shift). Must balance before `finish`.
    pub fn push_translate(&mut self, dx: f32, dy: f32) {
        self.offset_stack.push(self.offset);
        self.offset.0 += dx;
        self.offset.1 += dy;
    }

    /// See [`push_translate`](Self::push_translate).
    pub fn pop_translate(&mut self) {
        self.offset = self.offset_stack.pop().expect("translate stack underflow");
    }

    pub fn translate<R>(&mut self, dx: f32, dy: f32, f: impl FnOnce(&mut Self) -> R) -> R {
        self.offset_stack.push(self.offset);
        self.offset = (self.offset.0 + dx, self.offset.1 + dy);
        let out = f(self);
        self.offset = self.offset_stack.pop().expect("translate stack underflow");
        out
    }

    /// The translation applied to what is emitted now: a widget's own rect
    /// plus this is where it lands in the window.
    pub fn offset(&self) -> (f32, f32) {
        self.offset
    }

    fn apply_offset(&self, r: Rect) -> Rect {
        Rect { x: r.x + self.offset.0, y: r.y + self.offset.1, width: r.width, height: r.height }
    }

    fn push(&mut self, prim: Prim) {
        let clip = self.current_clip();
        let clip_circle = self.clip_circle_stack.last().copied();
        // r == 0 entries are balance placeholders (a zero-radius rounded clip is just its
        // scissor rect) — record no rounded clip so batches keep merging.
        let clip_rrect = self.clip_rrect_stack.last().copied().filter(|c| c[4] > 0.0);
        self.list.items.push(PaintItem { prim, clip, clip_circle, clip_rrect });
    }

    /// Append items another context painted — a nested paint walk spliced into this list,
    /// as a host does that keeps a walk's text for a pass of its own. Each item keeps its own
    /// clips and is clipped by this context's current scissor too; its own circular or
    /// rounded clip wins over the current one (the innermost, as when pushing). Items are
    /// absolute already, so this context's offset does not apply.
    pub fn append_items(&mut self, items: impl IntoIterator<Item = PaintItem>) {
        let cur = self.current_clip();
        let cur_circle = self.clip_circle_stack.last().copied();
        let cur_rrect = self.clip_rrect_stack.last().copied().filter(|c| c[4] > 0.0);
        for mut item in items {
            item.clip = match (cur, item.clip) {
                (Some(a), Some(b)) => Some(intersect(a, b)),
                (a, b) => a.or(b),
            };
            item.clip_circle = item.clip_circle.or(cur_circle);
            item.clip_rrect = item.clip_rrect.or(cur_rrect);
            self.list.items.push(item);
        }
    }

    /// Re-emit an already-built [`Prim`] through this context, so it re-records the
    /// current clip and translate state. This is the **single** place that has to
    /// learn a new `Prim` variant: a nested paint walk builds a scratch list and
    /// replays it into the real one, and that forwarding match used to exist
    /// verbatim in two crates ([`crate::widget::model`] and cce-cloud's
    /// `json_layout`) — adding `Prim::Groove` compiled against one and broke the
    /// other, caught only by a full workspace build.
    ///
    /// [`Prim::Text`] is NOT emitted: it is returned untouched, because the two
    /// callers disagree about it (a subtree painter authors its own text and wants
    /// it forwarded; everyone else drops it in favour of the widget's own label
    /// bridge). Every other variant is emitted and `None` comes back.
    #[must_use = "a returned Text prim was not emitted — drop or forward it explicitly"]
    pub fn replay(&mut self, prim: Prim) -> Option<Prim> {
        match prim {
            Prim::Text { .. } => return Some(prim),
            Prim::Quad { rect, color } => self.quad(rect, color),
            Prim::RoundedRect { rect, radius, corners, color } => {
                self.rounded_rect(rect, radius, corners, color)
            }
            Prim::Border { rect, radii, fill, border, thickness } => {
                self.border(rect, radii, fill, border, thickness)
            }
            Prim::Bevel { rect, radii, material, depth, tint } => {
                self.bevel_tinted(rect, radii, &material, depth, tint)
            }
            Prim::Frame { rect, hole, hole_radii, material, depth } => {
                self.frame(rect, hole, hole_radii, &material, depth)
            }
            Prim::Recess { rect, radii, depth, edges, tint } => match tint {
                Some(t) => self.recess_tinted(rect, radii, depth, t),
                None => self.recess_edges(rect, radii, depth, edges),
            },
            Prim::Boss { rect, radii, depth, edges, tint } => match tint {
                Some(t) => self.boss_edges_tinted(rect, radii, depth, edges, t),
                None => self.boss_edges(rect, radii, depth, edges),
            },
            Prim::Ridge { rect, radii, depth, edges } => self.ridge_edges(rect, radii, depth, edges),
            Prim::Trough { rect, radii, depth, edges, tint } => match tint {
                Some(t) => self.trough_tinted(rect, radii, depth, t),
                None => self.trough_edges(rect, radii, depth, edges),
            },
            Prim::Field { rect, radii, depth, split, end, tint } => {
                self.field(&Field::spanning(rect, radii, depth, split, end).with_tint(tint))
            }
            Prim::Plate { rect, radii, material, depth, shape } => {
                self.plate_shaped(rect, radii, &material, depth, shape)
            }
            Prim::Arc { cx, cy, radius, thickness, start, end, color } => {
                self.arc(cx, cy, radius, thickness, start, end, color)
            }
            Prim::ArcShaded { cx, cy, radius, thickness, start, end, inner, crest, outer } => {
                self.arc_shaded(cx, cy, radius, thickness, start, end, inner, crest, outer)
            }
            Prim::Vector { x1, y1, x2, y2, thickness, color, cap } => {
                self.vector(x1, y1, x2, y2, thickness, color, cap)
            }
            Prim::Circle { cx, cy, radius, color } => self.circle(cx, cy, radius, color),
            Prim::Sphere { cx, cy, radius, material } => self.sphere(cx, cy, radius, &material),
            Prim::Glow { rect, radius, reach, color } => self.glow(rect, radius, reach, color),
            Prim::Droplet { rect, material, spec } => self.droplet(rect, &material, spec),
            Prim::DropletScrim { rect, material, spec, feather } => {
                self.droplet_scrim(rect, &material, spec, feather)
            }
            Prim::ConcaveFillet { cx, cy, radius, depth, start, raised } => {
                self.concave_fillet(cx, cy, radius, depth, start, raised)
            }
            Prim::Groove { a, b, width, depth, host, strength } => self.groove_strength(a, b, width, depth, host, strength),
            Prim::Lattice { rect, period, origin, cell, radius, depth } => {
                self.lattice(rect, period, origin, cell, radius, depth)
            }
            Prim::Grout { rect, period, origin, cell, radius, color } => {
                self.grout(rect, period, origin, cell, radius, color)
            }
            Prim::Fill { rect, radii, material } => self.fill_material(rect, radii, &material),
            Prim::CarveUnion { boxes, depth, raised } => self.carve_union(boxes, depth, raised),
            Prim::Image { image, rect, alpha } => self.image(image, rect, alpha),
        }
        None
    }

    /// Consume the context and return the accumulated display list.
    pub fn finish(self) -> DisplayList {
        debug_assert!(self.clip_stack.is_empty(), "unbalanced push_clip/pop_clip");
        debug_assert!(self.offset_stack.is_empty(), "unbalanced translate");
        self.list
    }
}
