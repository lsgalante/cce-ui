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
//! | `mod.rs` | [`Prim`], the display list, [`PaintCtx`]'s stacks, `push`, `append_items`, `replay`, `finish` |
//! | `surfaces` | [`PlateSpec`], [`PlateStance`], [`Field`], [`ControlPlate`] |
//! | `relief` | `PaintCtx`'s relief emitters: plates, bevels, carves, fields, wells, grooves, lattices, droplets |
//! | `flat` | `PaintCtx`'s flat emitters: quads, images and icons, rounded rects, glow, vectors, circles, arcs, borders |
//! | `text` | the text types and `PaintCtx`'s text emitters |
//! | `render_target` | `PaintCtx` as a flat host's `RenderTarget` |

mod flat;
mod relief;
mod render_target;
mod surfaces;
mod text;
#[cfg(test)]
mod tests;

pub use surfaces::*;
pub use text::*;

use crate::scene::layout::Rect;
use crate::scene::material::{Finish, Material, PlateRole};


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

// Call the family **relief primitives** — see `Prim`. (This note sat above `DropletSpec`,
// which moved to `cce_core::droplet`.)
// Call the family **relief primitives**, not "bevel primitives": `Bevel` is one
// specific member — a filled rounded rect plus a lit roll on its lip — and a
// groove, a fillet or a sphere is not a bevel in any sense. "Relief" is also
// what the rest of the stack already says: `layout::control_relief` gates the
// whole family, and the config node is `relief`. The name **bevel** is reserved
// for two things: the `Bevel` prim, and the shared *edge treatment* every
// relief primitive is shaded with (`bevel_width`, `bevel_depth`,
// `bevel_shader`, `bevel_profile` — the lit roll, not the shape).
pub use cce_core::droplet::DropletSpec;

/// What a droplet's material is shaded with — an extension, since [`DropletSpec`] lives in
/// `cce_core`, which knows no [`Finish`]. `use cce_ui::scene::paint::DropletFinish;` at a
/// call site that writes `spec.finish()`.
pub trait DropletFinish {
    /// The drop's finish: its own gleam, shine and rim in the specular,
    /// shininess and curvature slots of a [`Finish`] (a drop is wetter than
    /// the DE's plates), the shading strength the DE's. The material a
    /// droplet is emitted with carries this — `Material::from_fill(c)
    /// .with_finish(spec.finish())` — and the tessellator reads it from
    /// there like any plate's, instead of packing the slots by hand.
    fn finish(&self) -> Finish;
}

impl DropletFinish for DropletSpec {
    fn finish(&self) -> Finish {
        Finish { spec: self.gleam, shininess: self.shine, curvature: self.rim, ..Finish::from_style() }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Prim {
    Quad { rect: Rect, color: [f32; 4] },
    RoundedRect { rect: Rect, radius: f32, corners: (bool, bool, bool, bool), color: [f32; 4] },
    /// A rounded fill plus a solid border stroke — a widget's own "plate"
    /// (`append_widget_plate`'s non-bevel branch).
    Border { rect: Rect, radii: Radii, fill: [f32; 4], border: [f32; 4], thickness: f32 },
    /// A beveled plate: a rounded fill at full size plus a light/shadow overlay lip
    /// (`append_widget_plate`'s bevel branch). `tint` colours the
    /// roll's light and shadow — neutral white normally; a host sets it to a
    /// highlight color to mark the plate (the focused-pane treatment) without a
    /// separate border ring: the light goes to the tint, the shadow to a dark
    /// tint, so the relief still reads. Shader-plates path only; the legacy
    /// banded tessellation ignores it.
    Bevel { rect: Rect, radii: Radii, material: Material, depth: f32, tint: [f32; 3] },
    /// A [`Prim::Bevel`] turned inside out: the plate's face is everything in
    /// `rect` OUTSIDE `hole`, and its rolled edge runs round the hole's
    /// outline, falling INTO the hole. So a corner of the hole is an inside
    /// corner of the plate — a cove, rounded at the hole's radius in the DE's
    /// corner family — which a box can only round convex. A band of a
    /// window's edge with an opening above it (the designer's playbar shelf):
    /// its top edge and both coves are ONE outline, one profile evaluation,
    /// with no join to stack two shadings at. `rect` bounds the face (its
    /// own edges are not rolled — lay them past the window or under
    /// something), and it is a carve host like a Bevel: carves inside `rect`
    /// group into it. Shader-plates path; the legacy path fills `rect`.
    Frame { rect: Rect, hole: Rect, hole_radii: Radii, material: Material, depth: f32 },
    /// A recess carved into whatever is already painted underneath — the inverse of
    /// `Bevel`. Emits ONLY the shaded edges, never a fill, so the surface below shows
    /// through the middle: a relief cut into the root plate rather than a plate laid on
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
    /// `tint` colours the wall's light and shadow — the same focused-pane
    /// treatment as [`Prim::Bevel`]'s tint, for carved wells instead of raised
    /// plates. A tinted
    /// recess never groups into a host plate's CSG features (a feature carries no
    /// color), so it always renders as the free-carve overlay. Shader-plates path
    /// only; the legacy banded tessellation ignores it.
    Recess { rect: Rect, radii: Radii, depth: f32, edges: (bool, bool, bool, bool), tint: Option<[f32; 3]> },
    /// The inverse of [`Prim::Recess`]: a plateau RAISED out of the surface below.
    /// Like `Recess` it emits only the shaded edges, never a fill — the face is the
    /// untouched surface underneath — so a region outlined by raised rolled bumps
    /// keeps the root plate's own color and translucency. Same wall semantics as
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
    /// The sunken twin of [`Prim::Ridge`]: a VALLEY riding the rect's boundary —
    /// a bump profile straddling the outline (span ±depth/2), falling from the
    /// surrounding surface to a trough on the boundary and rising back to the
    /// same level inside, so both faces sit at the underlying surface's own
    /// level. This is the seam a flush inset control leaves ([`PaintCtx::inset_plate`]).
    ///
    /// Same reason to exist as `Ridge`, measured: building this from a `Recess`
    /// on an outset rect plus a `Boss` on the rect (what `inset_plate` used to
    /// emit) stacks two independent shading passes. At depth 4.8 that read as a
    /// band 15px wide instead of 8 with THREE lobes — bright, dark, brighter —
    /// because the recess ring's own lit rim lands ~depth outside the control
    /// instead of merging into one wall, and the highlight peaked 22% hotter
    /// than a single evaluation of the same depth. It looked like two concentric
    /// rings, which is what it was.
    ///
    /// `edges` and the host-box fade behave exactly as [`Prim::Recess`]'s.
    /// SDF path only; the legacy banded tessellation approximates it with the
    /// old two-step stack (like `Ridge`, which approximates itself there).
    ///
    /// `tint` lights the rim like [`Prim::Recess`]'s — the focus treatment of a
    /// flush control plate (`PaintCtx::control_plate`).
    Trough { rect: Rect, radii: Radii, depth: f32, edges: (bool, bool, bool, bool), tint: Option<[f32; 3]> },
    /// A sunken well holding a FLUSH run: one field, one outer contour.
    /// Between `split` and `end` (xs in the same space as `rect`) the
    /// interior is back at the surface's level, as a flush control plate's
    /// face, inside a valley on the outline; on either side of that it is a
    /// [`Prim::Recess`] — the interior one step down. A run that reaches an
    /// end of the field (`split` left of it, or `end` right of it — by
    /// [`FIELD_RUN_ONLY`]) has no well on that side. The forms in use: a
    /// parameter pane's text row with its completion picker and a spinbox's
    /// value with its -/+ run (the run at the right end), a flush control
    /// plate (all run), a toggle (a run half the field wide, at the left end
    /// off and the right end on, gliding between).
    ///
    /// Why one prim and not the two it replaces, side by side: each of those
    /// shades its OWN box, so at the seam the outline breaks — each box
    /// turns its own square corner there, and the strong line of the edge
    /// jumps from the recess's outer rim to the trough's inner lip. Here the
    /// outline is evaluated once (the whole field, its own radii), its wall
    /// blends from the step to the valley across a wall's width about the
    /// seam — the two agree on the outer half, falling from the surface to
    /// half the step, and part on the inner half — and the seam is the
    /// well's floor rising to the run's face: a step wall along `split`,
    /// fading to nothing where it meets the outer wall, the one place the
    /// two sides stand at the same height.
    ///
    /// SDF path only; the legacy banded tessellation and flat hosts draw the
    /// two-box form it replaced. `tint` lights it like a recess's.
    Field { rect: Rect, radii: Radii, depth: f32, split: f32, end: f32, tint: Option<[f32; 3]> },
    /// The window's glass slab: a rounded fill plus a rolled, lit edge around its whole
    /// perimeter, drawn at full size. Distinct from `Bevel`, which insets its fill by
    /// `depth` — a plate must fill the window exactly, or the compositor's rounded window
    /// corners would show a gap. `depth` is the width of the roll-off in px, not a color
    /// offset (the shading amplitude is the DE-wide `bevel_depth`).
    ///
    /// `shape` overrides the DE-wide corner exponent (`layout::corner_shape`)
    /// for this one plate — `Some(2.0)` is circular arcs, so a plate whose
    /// radii reach its half-extent is a true circle regardless of the
    /// squircle the rest of the DE wears. `None` follows the DE.
    Plate { rect: Rect, radii: Radii, material: Material, depth: f32, shape: Option<f32> },
    Arc { cx: f32, cy: f32, radius: f32, thickness: f32, start: f32, end: f32, color: [f32; 4] },
    /// A ring band with radial color interpolation — inner rim → crest
    /// (centerline) → outer rim — for rounded rim bevels (the Ramp's key
    /// rings). `radius` is the stroke's outer edge, like `Arc`.
    ArcShaded { cx: f32, cy: f32, radius: f32, thickness: f32, start: f32, end: f32, inner: [f32; 4], crest: [f32; 4], outer: [f32; 4] },
    Vector { x1: f32, y1: f32, x2: f32, y2: f32, thickness: f32, color: [f32; 4], cap: Cap },
    Circle { cx: f32, cy: f32, radius: f32, color: [f32; 4] },
    /// A feathered aura around (and over) a rounded rect: the interior fills
    /// at the color's full alpha, and outside the boundary the alpha falls
    /// off smoothly to zero across `reach` px. Tessellated as concentric
    /// per-vertex-alpha rings the GPU interpolates, so the gradient is
    /// per-pixel smooth — no stacked-layer banding. Highlights and soft
    /// focus auras (the designer's drop-target glow) are the intended use;
    /// no relief shading, no light involvement.
    Glow { rect: Rect, radius: f32, reach: f32, color: [f32; 4] },
    /// A `Circle` lit as a ball: the disc is shaded per pixel as a hemisphere
    /// under the DE's plate light (same ambient/diffuse/specular model), so it
    /// reads as a sphere sitting on the surface — the slider thumb's look. The
    /// color is the sphere's face color exactly at the lit center, like a
    /// plate's face keeps the app's color. Falls back to a flat circle on the
    /// legacy (`bevel_shader 0`) path.
    Sphere { cx: f32, cy: f32, radius: f32, material: Material },
    /// A hanging water droplet clinging to the TOP edge of `rect`, lit per pixel
    /// by shader mode 10: the silhouette is a smooth union of a film "sheet"
    /// attached to the top edge (square top corners — the attach line) and a
    /// belly capsule resting on the rect's bottom, blended metaball-style so a
    /// waist forms where the sides pull up. Shaded as a glass dome under the
    /// DE's plate light — same ambient/diffuse and decoupled specular as the
    /// plates, plus a fresnel rim crest and a thin-edge clarity falloff (tint
    /// opacity drops toward the silhouette, so the frosted backdrop shows
    /// through clearer at the rim, which is what reads as water rather than
    /// plastic). Shape knobs in [`DropletSpec`]. On the legacy (`bevel_shader
    /// 0`) path it degrades to the flat hanging capsule — square top, round
    /// bottom — rather than vanishing.
    Droplet { rect: Rect, material: Material, spec: DropletSpec },
    /// The same silhouette as [`Prim::Droplet`] under the same [`DropletSpec`],
    /// filled FLAT and feathered inward: opaque through the interior, fading
    /// to nothing over `feather` px as it approaches the drop's edge. A
    /// vignette shaped exactly like the drop, for grounding text drawn on top
    /// of one — not a second lit body, so it carries no dome, rim, gleam or
    /// contact shadow.
    ///
    /// It shares the droplet's shader path rather than approximating the
    /// outline with a rounded rect, so the two can never disagree about where
    /// the drop's edge is. On the legacy (`bevel_shader 0`) path it degrades
    /// to the same flat rounded-rect outline `Prim::Droplet` falls back to.
    DropletScrim { rect: Rect, material: Material, spec: DropletSpec, feather: f32 },
    /// A concave inside-corner fillet for composed carves: a quarter-arc wall
    /// whose centre `(cx, cy)` sits out in the corner's pocket, shaded with the
    /// same step profile as a `Recess`/`Boss` wall (`raised` flips the sign).
    /// `start` is the wedge's start angle (quarter span, hard-cut at the
    /// tangent lines — the neighboring straight walls continue the profile
    /// exactly there). Box radii can only round convex corners; this is the
    /// missing concave piece. SDF path only (no legacy fallback).
    ConcaveFillet { cx: f32, cy: f32, radius: f32, depth: f32, start: f32, raised: bool },
    /// An engraved line: a groove of half-width `width / 2` running along the
    /// segment `a`–`b`, cut into whatever is painted beneath. Like [`Prim::Recess`]
    /// it emits only shading, never a fill — but its shape is a SLAB (a band about
    /// an arbitrary line) rather than a box, which is what lets it run at an angle.
    /// A box SDF can only carve axis-aligned walls; this is the diagonal case.
    ///
    /// Both walls come from ONE profile evaluation on `|distance to the line|`, so
    /// the groove carries a single specular/shoulder term — the same reason
    /// [`Prim::Ridge`] exists instead of stacking a boss on a recess.
    /// `width` 0 makes the two walls meet in a V.
    ///
    /// `depth` is the transition width in px (the wall's run), matching
    /// [`Prim::Recess`]. `host` is the surface the groove is engraved into: the
    /// shading fades out across that box's perimeter roll, so a seam cut across a
    /// plate dies into the plate's own rolled edge instead of ending on a hard line.
    /// SDF path only — the legacy banded tessellation draws nothing (like `Ridge`).
    ///
    /// `strength` scales the groove's shading, specular and AO — 1.0 is the
    /// DE's finish, 0.0 no groove at all — which is how a carve with no colour
    /// of its own fades ([`Prim::faded`]).
    Groove { a: (f32, f32), b: (f32, f32), width: f32, depth: f32, host: Rect, strength: f32 },
    /// A periodic field of identical rounded-box wells — every cell of a grid
    /// carved into whatever is painted beneath, as ONE surface. The wells
    /// repeat every `period` (x, y) with one cell centred at `origin`, each
    /// `cell` (w, h) big with `radius` corners; the wall runs from the cell
    /// edge OUTWARD over `depth` px (floor at the edge, plateau one run out),
    /// so a rail between two cells carries one wall from each side and the
    /// rail face is whatever the runs leave. Shading lands only inside `rect`.
    /// The wall's outer edge is MITRED, not offset: it is the cell grown by
    /// the run at the same `radius`, so a crossing keeps the cell's corner
    /// rounding instead of sweeping at `radius + depth`, and the four walls
    /// meet on the diagonals.
    ///
    /// This exists because a lattice drawn as one [`Prim::Recess`] per cell is
    /// N independent overlays: where four rounded rings meet at a crossing
    /// their shadings stack in colour space and read as overlapping effects,
    /// not a junction. Here the pixel is folded into the period and the
    /// distance is to the NEAREST cell — the union of every well — evaluated
    /// once, so the rail centre lines and the diagonals at each crossing are
    /// true mitres, and the cost is one draw regardless of how many cells the
    /// surface holds (a free carve per cell also runs into the per-frame
    /// feature budget long before a zoomed-out grid does). SDF path only.
    Lattice { rect: Rect, period: (f32, f32), origin: (f32, f32), cell: (f32, f32), radius: f32, depth: f32 },
    /// `color`, flat, everywhere inside `rect` that is OUTSIDE a periodic
    /// field of rounded cells — the same field [`Prim::Lattice`] carves
    /// (`period`, one cell centred at `origin`, each `cell` big with `radius`
    /// corners), painted as grout rather than shaded. One draw for the whole
    /// grid, with the cells' superellipse corners exact: what a graph's grid
    /// lines are when the cells are the surface beneath showing through.
    /// SDF path only — the legacy banded tessellation draws nothing.
    Grout { rect: Rect, period: (f32, f32), origin: (f32, f32), cell: (f32, f32), radius: f32, color: [f32; 4] },
    /// A flat fill of a MATERIAL: `rect` at `radii`, no roll, no rim — the
    /// material's tint, frosted at the material's own recipe when it is
    /// frosted. What a frosted `RoundedRect` promotes to, except that the
    /// recipe is the material's rather than the DE default's, so a fill can
    /// compress harder (or softer) than the pane it sits on. Opens no carve
    /// host: carves emitted after it overlay it, as they overlay any flat
    /// geometry. An opaque material draws as a plain rounded fill.
    Fill { rect: Rect, radii: Radii, material: Material },
    /// Several rounded boxes carved (`raised` false) or raised (`raised`
    /// true) as ONE shape: the union of the boxes is the well, and its wall
    /// follows the union's outline — straddling it by ±`depth`/2 like every
    /// carve boundary — through a single profile evaluation per pixel. An L,
    /// a T, a plus, a slot with a round end: any outline boxes can compose.
    ///
    /// The alternative, one [`Prim::Recess`] per box, is N overlays that
    /// each shade their own full outline: where two boxes overlap, each
    /// draws a wall straight through the other's interior, and where their
    /// walls cross the shadings stack in colour space — the junction reads
    /// as two effects laid over each other, not one shape. Here the pixel's
    /// distance is to the NEAREST box (the union SDF), so a box's wall
    /// vanishes wherever it runs inside another, and an inside corner is a
    /// sharp mitre (round it with [`Prim::ConcaveFillet`] if it must be
    /// concave-rounded — the union has no radius there by construction).
    /// Outer corners are mitred like [`Prim::Lattice`]'s: the wall band runs
    /// between each box shrunk and grown by half the run at the box's own
    /// radius, so a corner keeps its radius instead of sweeping wider.
    ///
    /// The boxes ride the frame's plate-feature buffer (the same slots CSG
    /// carves use, 64 per frame), so a union costs one draw plus one slot
    /// per box. When the budget cannot hold all of a union's boxes the
    /// tessellator keeps as many as fit — a degraded shape rather than none —
    /// and says so under `CCE_PLATE_DEBUG`. SDF path only.
    CarveUnion { boxes: Vec<(Rect, Radii)>, depth: f32, raised: bool },
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

impl Prim {
    /// This prim at `alpha` of its strength (0..1): a colour's alpha scaled,
    /// a text's or an image's alpha, a groove's shading. What a host fading a
    /// part of its drawing out or in replays it through (the context menu's
    /// page turn). The relief prims with no colour or strength of their own —
    /// the walls, plates and materials — come back as they are: they are
    /// drawn whole or not at all.
    pub fn faded(self, alpha: f32) -> Prim {
        let a = alpha.clamp(0.0, 1.0);
        let f = |c: [f32; 4]| [c[0], c[1], c[2], c[3] * a];
        match self {
            Prim::Quad { rect, color } => Prim::Quad { rect, color: f(color) },
            Prim::RoundedRect { rect, radius, corners, color } => Prim::RoundedRect { rect, radius, corners, color: f(color) },
            Prim::Border { rect, radii, fill, border, thickness } => Prim::Border { rect, radii, fill: f(fill), border: f(border), thickness },
            Prim::Arc { cx, cy, radius, thickness, start, end, color } => Prim::Arc { cx, cy, radius, thickness, start, end, color: f(color) },
            Prim::ArcShaded { cx, cy, radius, thickness, start, end, inner, crest, outer } => {
                Prim::ArcShaded { cx, cy, radius, thickness, start, end, inner: f(inner), crest: f(crest), outer: f(outer) }
            }
            Prim::Vector { x1, y1, x2, y2, thickness, color, cap } => Prim::Vector { x1, y1, x2, y2, thickness, color: f(color), cap },
            Prim::Circle { cx, cy, radius, color } => Prim::Circle { cx, cy, radius, color: f(color) },
            Prim::Glow { rect, radius, reach, color } => Prim::Glow { rect, radius, reach, color: f(color) },
            Prim::Grout { rect, period, origin, cell, radius, color } => Prim::Grout { rect, period, origin, cell, radius, color: f(color) },
            Prim::Groove { a: p, b, width, depth, host, strength } => Prim::Groove { a: p, b, width, depth, host, strength: strength * a },
            Prim::Text { text, x, y, font_size, color, alpha, font, bounds, attrs, layout } => {
                Prim::Text { text, x, y, font_size, color, alpha: alpha * a, font, bounds, attrs, layout }
            }
            Prim::Image { image, rect, alpha } => Prim::Image { image, rect, alpha: alpha * a },
            other => other,
        }
    }
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
