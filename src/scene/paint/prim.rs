//! [`Prim`], the vocabulary every widget paints in — quads, rounded rects, vectors, arcs, circles,
//! text, images and the relief primitives — with its parts: line caps, per-corner radii, and the
//! finish a droplet is shaded with.

use super::*;

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
