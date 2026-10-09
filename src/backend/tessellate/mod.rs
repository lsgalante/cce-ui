//! The tessellator: a `DisplayList` turned into `Vertex` batches with their push constants,
//! plus the vertex helpers apps call directly. Platform-neutral — it produces the data the
//! renderer draws and knows nothing of the window system, so every shell shares it.
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | `Vertex` and the batch types; [`tessellate_display_list`]: a frame's `Tess` (its constants, what it has emitted, the carve-grouping state), each item's prelude and dispatch by prim family, and the batch tail (clip stamping, merging, opening and closing carve hosts) |
//! | `flat` | fills (a frosted one promoted to a zero-depth plate batch), borders, glow, arcs, vectors, circles |
//! | `plates` | bevel, frame, plate, sphere and field: SDF-lit cover quads and their push blocks, or the banded path |
//! | `carves` | recess, boss, ridge, trough (grouped into a host plate as a CSG feature, or an overlay), fillet, groove, lattice, grout, carve union |
//! | `droplet` | the droplet and its scrim, and the silhouette they share |
//! | `shapes` | the vertex builders for flat shapes: quads, lines and capped vectors, rounded rects and the superellipse corner, glow, circles, arcs |
//! | `bevel` | the relief edge as vertices on the legacy banded path: profiles, banded shading, plate faces, solid borders |
//! | `debug` | `CCE_PLATE_DEBUG` (each frame's grouping verdicts) and the debug build's near-roll fallback warning |
//!
//! Every prim family keeps its SDF arm and its banded arm side by side: the banded path
//! (`style.surface.relief shader=false`) exists for A/B comparison, and a prim with no arm there
//! VANISHES rather than degrading. `tests/plate_golden.rs` is the byte-for-byte gate for a change
//! that must not move a pixel.

mod bevel;
mod carves;
mod debug;
mod droplet;
mod flat;
mod plates;
mod shapes;

pub use bevel::*;
pub use shapes::*;
use debug::*;

use crate::draw::Batch2D;
use crate::scene::material::PlateRole;
use crate::scene::paint::{Cap, PaintItem, Prim};

/// The tessellated display list's batches, scissors and rounded clips scaled
/// to physical px.
pub(crate) fn dl_batches_2d(dl_batches: &[DlBatch], scale_f32: f32) -> Vec<Batch2D> {
    dl_batches
        .iter()
        .map(|batch| Batch2D {
            scissor: batch.scissor.map(|clip| {
                (
                    (clip.x * scale_f32).max(0.0) as u32,
                    (clip.y * scale_f32).max(0.0) as u32,
                    (clip.width * scale_f32) as u32,
                    (clip.height * scale_f32) as u32,
                )
            }),
            clip_rrect: batch
                .clip_rrect
                .map(|c| [c[0] * scale_f32, c[1] * scale_f32, c[2] * scale_f32, c[3] * scale_f32, c[4] * scale_f32]),
            start: batch.start,
            end: batch.end,
            plate: batch.plate,
            blur_behind: batch.blur_behind,
        })
        .collect()
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
    pub clip_circle: [f32; 3], // [cx, cy, r]
}

/// A contiguous run of vertices sharing one scissor rect (Phase 3 single paint path) and one
/// rounded-rect clip. `scissor` is a logical-pixel clip (`None` = unclipped); `clip_rrect` is
/// the paint walk's `[cx, cy, bx, by, r]` rounded clip in logical px (`None` = unclipped),
/// applied as per-draw push-constant state; `start..end` indexes the flat vertex buffer.
pub struct DlBatch {
    pub scissor: Option<crate::scene::layout::Rect>,
    pub clip_rrect: Option<[f32; 5]>,
    pub start: u32,
    pub end: u32,
    /// When set, this batch is one SDF-lit plate cover quad (see
    /// [`crate::draw::PlatePush`]; already in physical px). Never merged.
    pub plate: Option<crate::draw::PlatePush>,
    /// A blur-behind plate (negative-alpha color): before drawing this batch
    /// the renderer snapshots the swapchain-so-far into its snapshot image, so
    /// the blur samples everything painted beneath the plate — not just the 3D
    /// scene backdrop. Never merged.
    pub blur_behind: bool,
}

/// An image draw from the display list: `at` is the vertex index it sorts
/// before (its position in the tessellated stream); `clip` is the item's
/// paint-walk clip. Logical coordinates throughout.
pub struct DlImage {
    pub image: u32,
    pub rect: crate::scene::layout::Rect,
    pub alpha: f32,
    pub at: u32,
    pub clip: Option<crate::scene::layout::Rect>,
}

/// Tessellate a `scene::paint::DisplayList`'s geometry into a flat vertex buffer plus per-clip draw
/// batches, reusing the same tessellators as the legacy path so vertices are identical. `Text`
/// prims are skipped here — text is still rendered via the app's `text_areas()` path. `sw`/`sh` are
/// logical surface dimensions (as everywhere else); `scale` is the HiDPI factor, needed because an
/// item's circular clip rides the vertices in PHYSICAL pixels. Consecutive prims sharing a clip are
/// merged into one batch (the circle clip is per-vertex, so it never splits batches).
pub fn tessellate_display_list(
    dl: &crate::scene::paint::DisplayList,
    sw: f32,
    sh: f32,
    scale: f32,
) -> (Vec<Vertex>, Vec<DlBatch>, Vec<DlImage>, Vec<[f32; 12]>) {
    let mut t = Tess::new(sw, sh, scale);
    for item in &dl.items {
        t.item(item);
    }
    t.dbg.report();
    (t.verts, t.batches, t.images, t.features)
}

/// One frame's tessellation in progress: its constants, what it has emitted, and the carve
/// grouping state.
struct Tess {
    /// Logical surface size, and the HiDPI factor (a circular clip and every push block ride
    /// in PHYSICAL px).
    sw: f32,
    sh: f32,
    scale: f32,
    /// SDF-lit plate path (shader2d's plate branch) vs the legacy banded vertex shading.
    shader_plates: bool,
    /// The light, from `scene::relief_shade`, which is also what cce-relief predicts pixels
    /// with — one definition, so the editor cannot draw a different material than the
    /// renderer applies.
    light: [f32; 3],
    /// [shading strength (1.0 at the default bevel_depth), specular strength, shininess,
    /// curvature/AO strength] — the DE's finish, for the CARVES, which shade whatever is
    /// beneath them and so take the host's. A prim that carries a Material (Plate, Bevel,
    /// Sphere, Droplet) pushes its own `material.finish` instead. Curvature is kept near the
    /// raised path's crest amplitude: the recess shoulder's brightening lands on the same
    /// pixels as its specular line, and the two stack — at 0.5 the step read several times
    /// hotter than a plate roll.
    finish: [f32; 4],
    verts: Vec<Vertex>,
    batches: Vec<DlBatch>,
    images: Vec<DlImage>,
    /// Carves CSG'd into plates (see Frame2D::plate_features), plus the plate they group
    /// into: the most recent Plate/Bevel batch, provided only Text and Image prims (which
    /// draw through separate paths anyway) intervene.
    features: Vec<[f32; 12]>,
    /// Open carve-host plates, in emission order (innermost candidates last). A STACK, not a
    /// single slot: a sibling plate emitted between a root plate and its later carves (a
    /// hovered button's opaque fill among transparent ones) must not sever those carves from
    /// the root plate they are carved into — that severing rendered every button after the
    /// hovered one through the visually-different overlay fallback. Ordinary geometry still
    /// closes every open plate (the draw-order rule in [`Tess::batch`]).
    plate_stack: Vec<(usize, crate::scene::layout::Rect)>,
    /// Which plate last appended a carve feature: a plate's features are addressed as one
    /// contiguous [offset, count] run (PlatePush::host), so a plate may only receive MORE
    /// features while no other plate has appended any since.
    last_feature_plate: Option<usize>,
    dbg: PlateDbg,
}

/// One display item in flight: the item, its circle clip as the vertex attribute carries it,
/// where its vertices start, and what its prim asks of the batch tail.
struct Item<'a> {
    item: &'a PaintItem,
    /// The item's circle clip, logical [cx, cy, r] → physical px ([0; 3] = none).
    circle: [f32; 3],
    /// The item's first vertex (a frosted Border closes its fill's batch and moves it on).
    start: u32,
    /// Blur-behind: a prim whose FILL alpha is negative asks the renderer to snapshot the
    /// frame-so-far before it draws ([`wants_blur_behind`]).
    blur_behind: bool,
    /// The plate push of a lit prim: its batch carries it and is never merged.
    plate: Option<crate::draw::PlatePush>,
    /// A frosted flat fill promoted to a zero-depth plate batch: it carries a recipe like any
    /// plate, but it is ordinary geometry to the carve grouping — it opens no host and closes
    /// the open ones.
    promoted: bool,
    /// A filled plate that carves may group into: its batch opens a carve host.
    made_plate: Option<crate::scene::layout::Rect>,
}

impl Tess {
    fn new(sw: f32, sh: f32, scale: f32) -> Self {
        let dbg = PlateDbg { on: plate_debug(), ..PlateDbg::default() };
        let shader_plates = crate::layout::bevel_shader();
        let light = crate::scene::relief_shade::light_vector();
        let finish = crate::scene::material::Finish::from_style().to_array();
        Tess {
            sw,
            sh,
            scale,
            shader_plates,
            light,
            finish,
            verts: Vec::new(),
            batches: Vec::new(),
            images: Vec::new(),
            features: Vec::new(),
            plate_stack: Vec::new(),
            last_feature_plate: None,
            dbg,
        }
    }

    /// Fixed 16-segment fans read as polygons once a circle/arc is pane-sized; scale the fan
    /// with the PHYSICAL radius (capped — beyond 128 the chord error is subpixel even on HiDPI).
    fn segs(&self, radius: f32) -> usize {
        ((radius * self.scale) as usize).clamp(16, 128)
    }

    /// One display item: its prim's vertices, then the batch tail.
    fn item(&mut self, item: &PaintItem) {
        let mut it = Item {
            item,
            circle: item
                .clip_circle
                .map(|c| [c[0] * self.scale, c[1] * self.scale, c[2] * self.scale])
                .unwrap_or([0.0f32, 0.0, 0.0]),
            start: self.verts.len() as u32,
            blur_behind: wants_blur_behind(&item.prim),
            plate: None,
            promoted: false,
            made_plate: None,
        };
        if self.prim(&mut it) {
            self.batch(it);
        }
    }

    /// The item's prim, by family. False when it leaves nothing to batch: text (drawn through
    /// the glyph pass), an image (queued for the image pass at this point in the stream), or
    /// a prim with nothing to show.
    fn prim(&mut self, it: &mut Item) -> bool {
        match &it.item.prim {
            Prim::Text { .. } => false,
            Prim::Image { image, rect, alpha } => {
                self.images.push(DlImage {
                    image: *image,
                    rect: *rect,
                    alpha: *alpha,
                    at: self.verts.len() as u32,
                    clip: it.item.clip,
                });
                false
            }
            Prim::Quad { .. }
            | Prim::RoundedRect { .. }
            | Prim::Fill { .. }
            | Prim::Border { .. }
            | Prim::Glow { .. }
            | Prim::Arc { .. }
            | Prim::ArcShaded { .. }
            | Prim::Vector { .. }
            | Prim::Circle { .. } => self.flat(it),
            Prim::Bevel { .. } | Prim::Frame { .. } | Prim::Plate { .. } | Prim::Field { .. } | Prim::Sphere { .. } => {
                self.plates(it)
            }
            Prim::Recess { .. }
            | Prim::Boss { .. }
            | Prim::Ridge { .. }
            | Prim::Trough { .. }
            | Prim::ConcaveFillet { .. }
            | Prim::Groove { .. }
            | Prim::Lattice { .. }
            | Prim::Grout { .. }
            | Prim::CarveUnion { .. } => self.carves(it),
            Prim::Droplet { .. } | Prim::DropletScrim { .. } => self.droplet(it),
        }
    }

    /// The batch tail: stamp the item's circle clip over everything it emitted, merge it into
    /// the previous batch when it can, and open or close carve hosts.
    fn batch(&mut self, it: Item) {
        let end = self.verts.len() as u32;
        if end == it.start {
            return;
        }
        // Some tessellators (quad_vertices, vector_vertices) don't thread the circle clip —
        // stamp the whole emitted range so every prim kind honors it uniformly.
        if it.item.clip_circle.is_some() {
            for v in self.verts[it.start as usize..].iter_mut() {
                v.clip_circle = it.circle;
            }
        }
        // Merge into the previous batch if it shares this clip pair and is contiguous.
        // Plate batches carry per-draw push constants, and blur-behind batches
        // trigger the renderer's snapshot copy, so neither ever merges.
        if it.plate.is_none() && !it.blur_behind {
            // Ordinary geometry painted after a plate ends its carve-grouping
            // window: a recess emitted later must overlay this geometry (the
            // fallback path), not shade beneath it inside the plate's draw.
            if self.dbg.on && !self.plate_stack.is_empty() {
                *self.dbg.closed_by.entry(prim_kind(&it.item.prim)).or_insert(0) += self.plate_stack.len();
            }
            self.plate_stack.clear();
            if let Some(last) = self.batches.last_mut() {
                if last.plate.is_none()
                    && last.scissor == it.item.clip
                    && last.clip_rrect == it.item.clip_rrect
                    && last.end == it.start
                {
                    last.end = end;
                    return;
                }
            }
        }
        if it.promoted {
            self.plate_stack.clear();
        }
        self.batches.push(DlBatch {
            scissor: it.item.clip,
            clip_rrect: it.item.clip_rrect,
            start: it.start,
            end,
            plate: it.plate,
            blur_behind: it.blur_behind,
        });
        if let Some(prect) = it.made_plate {
            self.plate_stack.push((self.batches.len() - 1, prect));
            if self.dbg.on {
                self.dbg.opened += 1;
            }
        }
    }
}

/// Blur-behind marker: a prim whose FILL alpha is negative asks the
/// renderer to snapshot the frame-so-far before it draws. Every
/// fill-bearing prim counts — the shader's a<0 branch runs for all of
/// them, and a variant missing here still frosts, but against the
/// stale scene backdrop instead of the frame: a flat tint with no
/// content and no blur, which is how the Dropdown popover (Border)
/// and the menubar panels (Quad) shipped visibly unfrosted while the
/// context menu (Plate) worked.
fn wants_blur_behind(prim: &Prim) -> bool {
    matches!(
        prim,
        Prim::Quad { color, .. } | Prim::RoundedRect { color, .. } if color[3] < 0.0
    ) || matches!(
        prim,
        Prim::Bevel { material, .. }
        | Prim::Frame { material, .. }
        | Prim::Plate { material, .. }
        | Prim::Droplet { material, .. }
            if material.fill(PlateRole::Nested)[3] < 0.0
    ) || matches!(
        prim,
        Prim::Border { fill, .. } if fill[3] < 0.0
    ) || matches!(
        prim,
        Prim::Fill { material, .. } if material.fill(PlateRole::Nested)[3] < 0.0
    )
}

/// The push-constant block for a raised SDF-lit plate over `rect` (logical px in,
/// physical px out). Corner radii clamp to the half-extent cap the SDF needs.
///
/// `shape` is a per-plate corner exponent (`Prim::Plate`'s override); `None`
/// follows the DE-wide `layout::corner_shape`. The span factor follows the
/// exponent actually used, so a circular override (2.0) spans nothing and a
/// half-extent radius lands on a true circle.
#[allow(clippy::too_many_arguments)]
/// The push block of a frosted flat fill promoted to a zero-depth plate: a
/// mode-1 plate with no roll (`t` = 0.001, so the face is exactly the fill),
/// corners at the nominal radii in the configured `corner_shape`, and the
/// fill's own frost recipe in `host.zw` (`Material::from_fill` decodes the
/// sentinel).
///
/// The shape must be `corner_shape`, not a fixed circle: a frosted `Border`
/// draws its stroke as `push_plate_solid_border_vertices` geometry in that
/// shape, and the unfrosted fill fan uses it too. A circular face under a
/// squircle stroke left the stroke cutting inside the face's corners.
fn flat_frost_push(
    rect: &crate::scene::layout::Rect,
    radii: (f32, f32, f32, f32),
    fill: [f32; 4],
    scale: f32,
    light: [f32; 3],
    material: [f32; 4],
) -> crate::draw::PlatePush {
    let mut p = plate_push_raised(rect, radii, 0.0, scale, light, material, false, None);
    let [fz, fw] = crate::scene::material::Material::from_fill(fill).frost.pack(scale);
    p.host[2] = fz;
    p.host[3] = fw;
    p
}

fn plate_push_raised(
    rect: &crate::scene::layout::Rect,
    radii: (f32, f32, f32, f32),
    width: f32,
    scale: f32,
    light: [f32; 3],
    material: [f32; 4],
    scale_corners: bool,
    shape: Option<f32>,
) -> crate::draw::PlatePush {
    // Floored: a rect already shrunk past its padding (a window dragged
    // below what its layout can hold) has a NEGATIVE extent here, and
    // `clamp(0.0, cap)` with a negative cap is a panic, not a zero radius.
    let cap = (rect.width.min(rect.height) * 0.5).max(0.0);
    let shape = shape.map_or_else(crate::layout::corner_shape, |n| n.clamp(2.0, 16.0));
    // For PLATES (`scale_corners`), widen the corner span by the
    // curvature-match factor (see `layout::corner_span_factor`): the diagonal
    // curvature radius equals the configured radius, the corner reads as the
    // same size as a circular one, and every roll inset ≤ r stays crease-free
    // (past the diagonal curvature radius the offset curve the specular band
    // follows creases into a visible square corner). Widget-scale overlay
    // reliefs (recess/boss/ridge fallbacks) pass false: their radii must MATCH
    // the nominal-radius squircles of the widget silhouettes around them, and
    // at their few-px roll widths the offset crease is subpixel.
    let rscale = if scale_corners { crate::layout::corner_span_factor_for(shape) } else { 1.0 };
    crate::draw::PlatePush {
        rect: [
            (rect.x + rect.width * 0.5) * scale,
            (rect.y + rect.height * 0.5) * scale,
            rect.width * 0.5 * scale,
            rect.height * 0.5 * scale,
        ],
        radii: [
            (radii.0 * rscale).clamp(0.0, cap) * scale,
            (radii.1 * rscale).clamp(0.0, cap) * scale,
            (radii.2 * rscale).clamp(0.0, cap) * scale,
            (radii.3 * rscale).clamp(0.0, cap) * scale,
        ],
        light: [light[0], light[1], light[2], width * scale],
        material,
        // Mode-1 semantics: [feature offset, feature count] — no carves yet;
        // the tessellator fills these in as recesses group into this plate.
        host: [0.0, 0.0, 0.0, 0.0],
        specular_tint: [1.0, 1.0, 1.0, 0.0],
        mode: 1.0,
        shape,
    }
}

#[cfg(test)]
mod frame_tests {
    use crate::scene::layout::Rect;
    use crate::scene::paint::PaintCtx;

    /// A frame is a plate turned inside out: its batch carries the HOLE as
    /// the SDF box, in mode 17, its cover quad is the face's bound, and it
    /// hosts the carves inside that bound as a Bevel does.
    #[test]
    fn a_frame_is_an_inside_out_plate_that_hosts_its_carves() {
        if !crate::layout::bevel_shader() {
            eprintln!("skipping: the shader plates are off in this configuration");
            return;
        }
        let (w, h, scale) = (400.0f32, 200.0f32, 1.0f32);
        let face = Rect { x: -10.0, y: 100.0, width: 420.0, height: 110.0 };
        let hole = Rect { x: 0.0, y: -100.0, width: 400.0, height: 224.0 };
        let mut pc = PaintCtx::new();
        let material = crate::scene::Material::opaque([0.3, 0.3, 0.35, 1.0]);
        pc.frame(face, hole, (0.0, 0.0, 20.0, 20.0), &material, 8.0);
        pc.recess(Rect { x: 40.0, y: 150.0, width: 30.0, height: 20.0 }, (4.0, 4.0, 4.0, 4.0), 3.0);
        let dl = pc.finish();
        let (_, batches, _, features) = super::tessellate_display_list(&dl, w, h, scale);
        let plate = batches.iter().find_map(|b| b.plate.filter(|p| p.mode == 17.0)).expect("a mode-17 batch");
        assert_eq!(plate.rect, [200.0, 12.0, 200.0, 112.0], "the SDF box is the hole");
        assert_eq!(plate.radii[2], 20.0, "the hole's bottom corners are the coves");
        assert_eq!(features.len(), 1, "the carve inside the face groups into the frame");
    }
}
