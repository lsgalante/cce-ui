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
    /// A beveled plate: a rounded fill at full size plus a light/shadow overlay lip
    /// (mirrors `push_widget_vertices`' bevel branch). `tint` multiplies the lit
    /// roll's specular color — neutral white normally; a host sets it to a
    /// highlight color to mark the plate (the focused-pane treatment) without a
    /// separate border ring. Shader-plates path only; the legacy banded
    /// tessellation ignores it.
    Bevel { rect: Rect, radii: Radii, color: [f32; 4], depth: f32, tint: [f32; 3] },
    /// A recess carved into whatever is already painted underneath — the inverse of
    /// `Bevel`. Emits ONLY the shaded edges, never a fill, so the surface below shows
    /// through the middle: a relief cut into the backplate rather than a plate laid on
    /// top of it. The light vector is negated relative to `Bevel`, so the edges facing
    /// `light_source_position` fall into shadow and the far edges catch the light —
    /// which is what reads as "lower" instead of "raised".
    ///
    /// The shading is a translucent light/shadow overlay, so the carve needs no knowledge
    /// of what it carves: fills, gradients, and translucency below all show through
    /// modulated rather than repainted.
    /// `edges` is (top, right, bottom, left): which walls of the carve actually exist.
    /// A region flush with the plate's own edge is a step, not a trough — see
    /// `push_bevel_edge_vertices_banded`.
    /// `tint` colors the wall's lit rim — the same focused-pane treatment as
    /// [`Prim::Bevel`]'s tint, for carved wells instead of raised plates. A tinted
    /// recess never groups into a host plate's CSG features (a feature carries no
    /// color), so it always renders as the free-carve overlay. Shader-plates path
    /// only; the legacy banded tessellation ignores it.
    Recess { rect: Rect, radii: Radii, depth: f32, edges: (bool, bool, bool, bool), tint: Option<[f32; 3]> },
    /// The inverse of [`Prim::Recess`]: a plateau RAISED out of the surface below.
    /// Like `Recess` it emits only the shaded edges, never a fill — the face is the
    /// untouched surface underneath — so a region outlined by raised rolled bumps
    /// keeps the backplate's own color and translucency. Same wall semantics as
    /// `Recess` (`edges` = top/right/bottom/left); the lighting is the raised sign,
    /// so the edges facing `light_source_position` catch the light. `tint` colors
    /// the lit rim like [`Prim::Recess`]'s — the focused-pane treatment for a
    /// rim-only pane (a fill-less surface can't carry [`Prim::Bevel`]'s tint).
    /// Like a tinted recess it never groups into a host plate's CSG features.
    Boss { rect: Rect, radii: Radii, depth: f32, edges: (bool, bool, bool, bool), tint: Option<[f32; 3]> },
    /// A raised RIM riding the rect's boundary: a bump profile straddling the
    /// outline (span ±depth/2), rising from the surrounding surface to a crest on
    /// the boundary and falling back to the same level inside — an elevated border
    /// around a channel, both faces at the underlying surface's own level. One
    /// primitive, ONE lighting evaluation per pixel: building the same shape from
    /// a Boss plus an inset Recess stacks two shading passes (double specular /
    /// shoulder terms at the crest) and reads far hotter than a plate edge.
    Ridge { rect: Rect, radii: Radii, depth: f32, edges: (bool, bool, bool, bool) },
    /// The window's glass slab: a rounded fill plus a rolled, lit edge around its whole
    /// perimeter, drawn at full size. Distinct from `Bevel`, which insets its fill by
    /// `depth` — a plate must fill the window exactly, or the compositor's rounded window
    /// corners would show a gap. `depth` is the width of the roll-off in px, not a color
    /// offset (the shading amplitude is the DE-wide `bevel_depth`).
    Plate { rect: Rect, radii: Radii, color: [f32; 4], depth: f32 },
    Arc { cx: f32, cy: f32, radius: f32, thickness: f32, start: f32, end: f32, color: [f32; 4] },
    /// A ring band with radial color interpolation — inner rim → crest
    /// (centerline) → outer rim — for rounded rim bevels (the Ramp's key
    /// rings). `radius` is the stroke's outer edge, like `Arc`.
    ArcShaded { cx: f32, cy: f32, radius: f32, thickness: f32, start: f32, end: f32, inner: [f32; 4], crest: [f32; 4], outer: [f32; 4] },
    Vector { x1: f32, y1: f32, x2: f32, y2: f32, thickness: f32, color: [f32; 4], cap: Cap },
    Circle { cx: f32, cy: f32, radius: f32, color: [f32; 4] },
    /// A `Circle` lit as a ball: the disc is shaded per pixel as a hemisphere
    /// under the DE's plate light (same ambient/diffuse/specular model), so it
    /// reads as a sphere sitting on the surface — the slider thumb's look. The
    /// color is the sphere's face color exactly at the lit center, like a
    /// plate's face keeps the app's color. Falls back to a flat circle on the
    /// legacy (`bevel_shader 0`) path.
    Sphere { cx: f32, cy: f32, radius: f32, color: [f32; 4] },
    /// A concave inside-corner fillet for composed carves: a quarter-arc wall
    /// whose centre `(cx, cy)` sits out in the corner's pocket, shaded with the
    /// same step profile as a `Recess`/`Boss` wall (`raised` flips the sign).
    /// `start` is the wedge's start angle (quarter span, hard-cut at the
    /// tangent lines — the neighboring straight walls continue the profile
    /// exactly there). Box radii can only round convex corners; this is the
    /// missing concave piece. SDF path only (no legacy fallback).
    ConcaveFillet { cx: f32, cy: f32, radius: f32, depth: f32, start: f32, raised: bool },
    /// Text in sRGB u8 (the `TextLabel` convention). `font` is a font string for
    /// `get_text_buffer` (family, or "family:size"); `bounds` is a logical `[l, t, r, b]` clip
    /// for the glyph pass (Phase 6: the backend renders these through the glyph pass when the app
    /// opts in via `Application::display_list_text`; the paint walk's clip additionally
    /// applies through the item's `clip`). `attrs` carries the optional shaping attributes
    /// beyond family+size (the font picker's italic/weight preview variants). `layout`, when
    /// `Some`, requests box layout — word-wrap at a width and horizontal/vertical alignment
    /// within a box (the placed-text-box case, e.g. cce-layout-interface's canvas elements);
    /// `None` is the ordinary single-run label. `alpha` fades the glyphs (1.0 = opaque) —
    /// the color stays sRGB u8, so translucent text doesn't need a color-type change.
    Text { text: String, x: f32, y: f32, font_size: f32, color: [u8; 3], alpha: f32, font: Option<String>, bounds: Option<[f32; 4]>, attrs: TextAttrs, layout: Option<TextLayout> },
    /// A user image (id from `cce_ui::vk::upload_rgba`) drawn as a quad, in
    /// display-list order like any other primitive. The paint walk's clip
    /// applies through the item's `clip` as usual.
    Image { image: u32, rect: Rect, alpha: f32 },
}

/// Horizontal alignment of laid-out (boxed) text — the toolkit-plain mirror of
/// `cosmic_text::Align`, mapped at shape time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AlignH {
    #[default]
    Left,
    Center,
    Right,
}

/// Vertical alignment of laid-out text within its box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AlignV {
    #[default]
    Top,
    Middle,
    Bottom,
}

/// Box layout for a [`Prim::Text`]: word-wrap width (`Some` ⇒ multiline wrap; `None` ⇒ single
/// run) and horizontal/vertical alignment within a box of `box_height`. All lengths are logical.
/// The backend shapes an uncached buffer (`get_text_buffer_laid_out`) so the wrap/align do not
/// pollute the shared single-run cache, and applies the vertical offset from the shaped height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextLayout {
    pub wrap_width: Option<f32>,
    pub box_height: f32,
    pub align_h: AlignH,
    pub align_v: AlignV,
}

/// Optional shaping attributes for a [`Prim::Text`] — the subset a widget can request beyond
/// family + size. `weight` is the OpenType weight (400 regular, 700 bold); `None` leaves the
/// family default. Kept toolkit-plain (no cosmic-text types) like the rest of the scene layer;
/// the backend maps them onto `cosmic_text::Style`/`Weight` at shape time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextAttrs {
    pub italic: bool,
    pub weight: Option<u16>,
}

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

    pub fn quad(&mut self, rect: Rect, color: [f32; 4]) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Quad { rect, color });
    }

    /// A user image (id from `cce_ui::vk::upload_rgba`) drawn at `rect`.
    pub fn image(&mut self, image: u32, rect: Rect, alpha: f32) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Image { image, rect, alpha });
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

    /// A sphere-lit circle — see `Prim::Sphere`.
    pub fn sphere(&mut self, cx: f32, cy: f32, radius: f32, color: [f32; 4]) {
        let (ox, oy) = self.offset;
        self.push(Prim::Sphere { cx: cx + ox, cy: cy + oy, radius, color });
    }

    /// A concave inside-corner fillet — see `Prim::ConcaveFillet`. `start` is
    /// the quarter wedge's start angle; the arc's centre sits in the corner's
    /// pocket and the wall descends (or rises, `raised`) away from it.
    pub fn concave_fillet(&mut self, cx: f32, cy: f32, radius: f32, depth: f32, start: f32, raised: bool) {
        let (ox, oy) = self.offset;
        self.push(Prim::ConcaveFillet { cx: cx + ox, cy: cy + oy, radius, depth, start, raised });
    }

    pub fn border(&mut self, rect: Rect, radii: Radii, fill: [f32; 4], border: [f32; 4], thickness: f32) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Border { rect, radii, fill, border, thickness });
    }

    pub fn bevel(&mut self, rect: Rect, radii: Radii, color: [f32; 4], depth: f32) {
        self.bevel_tinted(rect, radii, color, depth, [1.0, 1.0, 1.0]);
    }

    /// `bevel` with a specular tint — see `Prim::Bevel::tint`.
    pub fn bevel_tinted(&mut self, rect: Rect, radii: Radii, color: [f32; 4], depth: f32, tint: [f32; 3]) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Bevel { rect, radii, color, depth, tint });
    }

    /// Carve a recess into the already-painted surface below. Unlike `bevel`, this fills
    /// nothing — the shading is an overlay, so it composes over whatever was painted.
    pub fn recess(&mut self, rect: Rect, radii: Radii, depth: f32) {
        self.recess_edges(rect, radii, depth, (true, true, true, true));
    }

    /// [`PaintCtx::recess`] with the lit rim tinted — see `Prim::Recess::tint`
    /// (the focused-well treatment).
    pub fn recess_tinted(&mut self, rect: Rect, radii: Radii, depth: f32, tint: [f32; 3]) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Recess { rect, radii, depth, edges: (true, true, true, true), tint: Some(tint) });
    }

    /// Raise a plateau out of the already-painted surface below — the inverse of
    /// [`PaintCtx::recess`]. Only the edges are shaded; the face stays the surface
    /// beneath, so the raised region inherits the backplate's color. `depth` is the
    /// roll width in px (pass [`crate::layout::bevel_width`] unless the widget
    /// needs a tighter lip).
    pub fn boss(&mut self, rect: Rect, radii: Radii, depth: f32) {
        self.boss_edges(rect, radii, depth, (true, true, true, true));
    }

    /// [`PaintCtx::boss`] with only some of the walls — see `Prim::Boss`.
    pub fn boss_edges(
        &mut self,
        rect: Rect,
        radii: Radii,
        depth: f32,
        edges: (bool, bool, bool, bool),
    ) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Boss { rect, radii, depth, edges, tint: None });
    }

    /// [`PaintCtx::boss_edges`] with a specular tint on the lit rim — see
    /// `Prim::Boss::tint`.
    pub fn boss_edges_tinted(
        &mut self,
        rect: Rect,
        radii: Radii,
        depth: f32,
        edges: (bool, bool, bool, bool),
        tint: [f32; 3],
    ) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Boss { rect, radii, depth, edges, tint: Some(tint) });
    }

    /// A flush inset control: `rect`'s plate sits SUNKEN into the surface with
    /// its face level with it — a groove ring carved around the control (the
    /// outward wall steps down) and the control's own beveled lip rising back
    /// up inside. Two opposite-facing bevels; the face never leaves the
    /// surface plane. An opaque `color` fills the face (Bevel); transparent
    /// degrades to edges-only (Boss), the surface below showing through as
    /// the face. `depth` is the roll width of both walls; the ring is
    /// expanded by depth/2, so the descending wall meets the rising lip in a
    /// tight V-groove with no flat floor between them.
    pub fn inset_plate(&mut self, rect: Rect, radii: Radii, color: [f32; 4], depth: f32) {
        let g = depth * 0.5;
        let outer = Rect {
            x: rect.x - g,
            y: rect.y - g,
            width: rect.width + 2.0 * g,
            height: rect.height + 2.0 * g,
        };
        let (r1, r2, r3, r4) = radii;
        self.recess(outer, (r1 + g, r2 + g, r3 + g, r4 + g), depth);
        if color[3] > 0.001 {
            self.bevel(rect, radii, color, depth);
        } else {
            self.boss(rect, radii, depth);
        }
    }

    /// Raise a rim along `rect`'s boundary — see `Prim::Ridge`. `depth` is the
    /// full width of the bump (it straddles the outline by ±depth/2).
    pub fn ridge(&mut self, rect: Rect, radii: Radii, depth: f32) {
        self.ridge_edges(rect, radii, depth, (true, true, true, true));
    }

    /// [`PaintCtx::ridge`] with only some of the walls — see `Prim::Ridge`.
    pub fn ridge_edges(
        &mut self,
        rect: Rect,
        radii: Radii,
        depth: f32,
        edges: (bool, bool, bool, bool),
    ) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Ridge { rect, radii, depth, edges });
    }

    /// [`PaintCtx::recess`] with only some of the walls — see `Prim::Recess`.
    pub fn recess_edges(
        &mut self, rect: Rect, radii: Radii, depth: f32,
        edges: (bool, bool, bool, bool),
    ) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Recess { rect, radii, depth, edges, tint: None });
    }

    /// The window's glass slab: rounded fill at full size plus a rolled, lit perimeter.
    /// `depth` is the roll-off width in px — pass [`crate::layout::bevel_width`] unless the
    /// window wants a shallower edge than the DE default.
    pub fn plate(&mut self, rect: Rect, radii: Radii, color: [f32; 4], depth: f32) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Plate { rect, radii, color, depth });
    }

    pub fn arc(&mut self, cx: f32, cy: f32, radius: f32, thickness: f32, start: f32, end: f32, color: [f32; 4]) {
        let (ox, oy) = self.offset;
        self.push(Prim::Arc { cx: cx + ox, cy: cy + oy, radius, thickness, start, end, color });
    }

    /// A radially-shaded ring band — see [`Prim::ArcShaded`].
    #[allow(clippy::too_many_arguments)]
    pub fn arc_shaded(
        &mut self,
        cx: f32,
        cy: f32,
        radius: f32,
        thickness: f32,
        start: f32,
        end: f32,
        inner: [f32; 4],
        crest: [f32; 4],
        outer: [f32; 4],
    ) {
        let (ox, oy) = self.offset;
        self.push(Prim::ArcShaded {
            cx: cx + ox,
            cy: cy + oy,
            radius,
            thickness,
            start,
            end,
            inner,
            crest,
            outer,
        });
    }

    pub fn text(&mut self, text: impl Into<String>, x: f32, y: f32, font_size: f32, color: [u8; 3]) {
        self.text_with(text, x, y, font_size, color, None, None);
    }

    /// Text with a per-label font and clip rect (`[l, t, r, b]`, local space) — what the
    /// legacy `text_labels_with_font_and_bounds` tuples carry, expressible in the display
    /// list since Phase 6.
    pub fn text_with(
        &mut self,
        text: impl Into<String>,
        x: f32,
        y: f32,
        font_size: f32,
        color: [u8; 3],
        font: Option<String>,
        bounds: Option<[f32; 4]>,
    ) {
        self.text_attrs(text, x, y, font_size, color, font, bounds, TextAttrs::default());
    }

    /// [`text_with`](PaintCtx::text_with) plus shaping attributes (italic / weight) — what the
    /// font picker's style-variant previews need beyond family + size.
    #[allow(clippy::too_many_arguments)]
    pub fn text_attrs(
        &mut self,
        text: impl Into<String>,
        x: f32,
        y: f32,
        font_size: f32,
        color: [u8; 3],
        font: Option<String>,
        bounds: Option<[f32; 4]>,
        attrs: TextAttrs,
    ) {
        let (ox, oy) = self.offset;
        let bounds = bounds.map(|[l, t, r, b]| [l + ox, t + oy, r + ox, b + oy]);
        self.push(Prim::Text { text: text.into(), x: x + ox, y: y + oy, font_size, color, alpha: 1.0, font, bounds, attrs, layout: None });
    }

    /// [`text_with`](PaintCtx::text_with) plus a glyph alpha (1.0 = opaque) — translucent
    /// labels (a pane fading out) without changing the sRGB u8 color convention.
    #[allow(clippy::too_many_arguments)]
    pub fn text_faded(
        &mut self,
        text: impl Into<String>,
        x: f32,
        y: f32,
        font_size: f32,
        color: [u8; 3],
        alpha: f32,
        font: Option<String>,
        bounds: Option<[f32; 4]>,
    ) {
        let (ox, oy) = self.offset;
        let bounds = bounds.map(|[l, t, r, b]| [l + ox, t + oy, r + ox, b + oy]);
        self.push(Prim::Text {
            text: text.into(),
            x: x + ox,
            y: y + oy,
            font_size,
            color,
            alpha,
            font,
            bounds,
            attrs: TextAttrs::default(),
            layout: None,
        });
    }

    /// Boxed text: word-wrap + horizontal/vertical alignment within a box (a placed text box).
    /// Unlike [`text_with`](PaintCtx::text_with), the backend shapes this uncached with the box
    /// layout applied. `x, y` are the box's top-left; the backend applies the vertical offset.
    #[allow(clippy::too_many_arguments)]
    pub fn text_boxed(
        &mut self,
        text: impl Into<String>,
        x: f32,
        y: f32,
        font_size: f32,
        color: [u8; 3],
        font: Option<String>,
        bounds: Option<[f32; 4]>,
        attrs: TextAttrs,
        layout: TextLayout,
    ) {
        let (ox, oy) = self.offset;
        let bounds = bounds.map(|[l, t, r, b]| [l + ox, t + oy, r + ox, b + oy]);
        self.push(Prim::Text {
            text: text.into(),
            x: x + ox,
            y: y + oy,
            font_size,
            color,
            alpha: 1.0,
            font,
            bounds,
            attrs,
            layout: Some(layout),
        });
    }

    /// Consume the context and return the accumulated display list.
    pub fn finish(self) -> DisplayList {
        debug_assert!(self.clip_stack.is_empty(), "unbalanced push_clip/pop_clip");
        debug_assert!(self.offset_stack.is_empty(), "unbalanced translate");
        self.list
    }
}

/// `PaintCtx` as a popover render target: display-list hosts pass their frame
/// ctx straight into `render_popover`, so popovers draw REAL prims — relief
/// plates, rounded rects, bounded text — instead of the flattened
/// `PopoverCollector` view (which stays for legacy tuple hosts).
impl crate::layout::RenderTarget for PaintCtx {
    fn rect(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32) {
        self.quad(Rect { x, y, width: w, height: h }, color);
    }
    fn rect_with_radius(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32, radius: f32) {
        self.rounded_rect(Rect { x, y, width: w, height: h }, radius, (true, true, true, true), color);
    }
    fn rect_with_radius_corners(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32, radius: f32, corners: (bool, bool, bool, bool)) {
        self.rounded_rect(Rect { x, y, width: w, height: h }, radius, corners, color);
    }
    fn text(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4]) {
        let c = [
            (color[0] * 255.0).clamp(0.0, 255.0) as u8,
            (color[1] * 255.0).clamp(0.0, 255.0) as u8,
            (color[2] * 255.0).clamp(0.0, 255.0) as u8,
        ];
        PaintCtx::text(self, content, x, y, size, c);
    }
    fn text_with_font(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], font: &str) {
        crate::layout::RenderTarget::text_with_font_and_bounds(self, content, x, y, size, color, font, None);
    }
    fn text_with_bounds(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], bounds: Option<[f32; 4]>) {
        let c = [
            (color[0] * 255.0).clamp(0.0, 255.0) as u8,
            (color[1] * 255.0).clamp(0.0, 255.0) as u8,
            (color[2] * 255.0).clamp(0.0, 255.0) as u8,
        ];
        self.text_with(content, x, y, size, c, None, bounds);
    }
    fn text_with_font_and_bounds(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], font: &str, bounds: Option<[f32; 4]>) {
        let c = [
            (color[0] * 255.0).clamp(0.0, 255.0) as u8,
            (color[1] * 255.0).clamp(0.0, 255.0) as u8,
            (color[2] * 255.0).clamp(0.0, 255.0) as u8,
        ];
        self.text_with(content, x, y, size, c, Some(font.to_string()), bounds);
    }
    fn push_clip_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.push_clip(Rect { x, y, width: w, height: h });
    }
    fn pop_clip_rect(&mut self) {
        self.pop_clip();
    }
    fn inset_plate(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32, radius: f32, depth: f32) {
        PaintCtx::inset_plate(self, Rect { x, y, width: w, height: h }, (radius, radius, radius, radius), color, depth);
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
    #[test]
    fn text_with_translates_position_and_bounds() {
        let mut ctx = PaintCtx::new();
        ctx.translate(10.0, 20.0, |ctx| {
            ctx.text_with("hi", 1.0, 2.0, 12.0, [1, 2, 3], Some("Mono".into()), Some([0.0, 0.0, 50.0, 30.0]));
            ctx.text("plain", 3.0, 4.0, 10.0, [9, 9, 9]);
        });
        let list = ctx.finish();
        match &list.items[0].prim {
            Prim::Text { x, y, font, bounds, .. } => {
                assert_eq!((*x, *y), (11.0, 22.0), "position translated");
                assert_eq!(font.as_deref(), Some("Mono"));
                assert_eq!(*bounds, Some([10.0, 20.0, 60.0, 50.0]), "bounds translated");
            }
            other => panic!("expected Text, got {other:?}"),
        }
        match &list.items[1].prim {
            Prim::Text { font, bounds, .. } => {
                assert_eq!(*font, None, "plain text carries no font");
                assert_eq!(*bounds, None);
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

}
